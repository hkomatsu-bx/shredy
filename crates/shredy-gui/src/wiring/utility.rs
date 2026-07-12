//! ユーティリティスキャン（§3.9）の結線。
//!
//! 重複検索（`scan`/`delete`）とは独立した系統。スキャン種別は現在のページ
//! （[`crate::view::page_to_kind`]）から決まる。スキャン・削除・隔離移動はワーカースレッドで
//! 実行し UI を固めない。削除/移動は core の [`Deleter::plan_paths`] を通し「1つ残す」は
//! 適用せず（全件可）、ロック/使用中/読み取り専用/リパースの安全フィルタのみ適用する。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use slint::ComponentHandle;

use shredy_core::progress::{is_cancelled, new_cancel};
use shredy_core::{
    find_thumbnail_cache_items, DeleteMode, Deleter, FileOp, LockSet, LongPath, ProgressEvent,
    RealFs, UtilityConfig, UtilityItem, UtilityKind, UtilityScanner,
};

/// 削除/隔離ワーカーへ渡す設定スナップショット（ロック下で取得し即時解放）。
///
/// 同型の `bool` が 3 つ（`save_log`/`delete_ro`/`update_list`）並ぶため、位置依存の
/// タプルではなく名前付きフィールドで取り違えを型レベルで防ぐ（§14）。
struct UtilOpSnapshot {
    paths: Vec<LongPath>,
    locks: LockSet,
    save_log: bool,
    config_dir: PathBuf,
    delete_ro: bool,
    update_list: bool,
}

use shredy_gui::bridge::util_select;

use crate::config_io::resolve_roots;
use crate::state::{lock, AppState};
use crate::view::{dispatch_ui, open_in_explorer, page_to_kind, refresh_util, update_util_row};
use crate::{AppWindow, Page};

use super::with_ui;

/// ユーティリティスキャン関連のコールバックをすべて結線する。
pub(crate) fn wire_utility(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    wire_util_scan(ui, state);
    wire_util_selection(ui, state);
    wire_util_context(ui, state);
    wire_util_delete(ui, state);
    wire_util_quarantine(ui, state);
    wire_util_reset(ui, state);
}

/// コンテキストメニュー（起点フォルダ基準の選択/ロック, §3.4）を結線する。
fn wire_util_context(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_select_direct(move |origin, invert| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                let idxs = util_select::direct_indices(&st.util_items, origin.as_str(), invert);
                st.util_check_indices(&idxs);
                refresh_util(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_select_tree(move |origin, invert| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                let idxs = util_select::tree_indices(&st.util_items, origin.as_str(), invert);
                st.util_check_indices(&idxs);
                refresh_util(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_lock_one(move |idx| {
            let Ok(idx) = usize::try_from(idx) else {
                return;
            };
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.util_lock_indices(&[idx]);
                refresh_util(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_lock_direct(move |origin, invert| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                let idxs = util_select::direct_indices(&st.util_items, origin.as_str(), invert);
                st.util_lock_indices(&idxs);
                refresh_util(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_lock_tree(move |origin, invert| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                let idxs = util_select::tree_indices(&st.util_items, origin.as_str(), invert);
                st.util_lock_indices(&idxs);
                refresh_util(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_unlock_all(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.util_unlock_all();
                refresh_util(&ui, &st);
            });
        });
    }
}

/// 「スキャン開始」: 現在のページ種別でユーティリティ走査を行う。
fn wire_util_scan(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_util_scan(move || {
        with_ui(&ui_weak, |ui| {
            // サムネ削除（§3.10.4・整理）は UtilityKind を持たず専用スキャンを使う。
            if ui.get_active_page() == Page::UtilThumbnail {
                perform_thumbnail_scan(&ui, &state);
                return;
            }
            let Some(kind) = page_to_kind(ui.get_active_page()) else {
                return; // ユーティリティページ以外（通常は到達しない）。
            };
            perform_util_scan(&ui, &state, kind);
        });
    });
}

/// 選択操作（全選択 / 全解除 / 行ハイライト / チェック切替 / 場所を開く）を結線する。
fn wire_util_selection(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_select_all(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.util_select_all();
                refresh_util(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_clear_selection(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.util_clear_checks();
                refresh_util(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_check_toggled(move |idx| {
            let Ok(idx) = usize::try_from(idx) else {
                return;
            };
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.util_toggle_check(idx);
                // 単一行のトグルは差分更新（全行再構築を避ける, M-2）。
                update_util_row(&ui, &st, idx);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_row_clicked(move |idx| {
            let Ok(idx) = usize::try_from(idx) else {
                return;
            };
            with_ui(&ui_weak, |ui| {
                {
                    let mut st = lock(&state);
                    st.util_highlighted = Some(idx);
                }
                ui.set_util_highlighted(idx as i32);
            });
        });
    }
    {
        let state = Arc::clone(state);
        ui.on_util_open_explorer(move |idx| {
            let Ok(idx) = usize::try_from(idx) else {
                return;
            };
            let st = lock(&state);
            if let Some(it) = st.util_items.get(idx) {
                open_in_explorer(&it.path);
            }
        });
    }
}

/// 削除（確認ダイアログ→実行）を結線する。
fn wire_util_delete(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_request_delete(move || {
            with_ui(&ui_weak, |ui| {
                let (count, confirm, recycle) = {
                    let st = lock(&state);
                    (
                        st.util_checked.len(),
                        st.config.confirm_before_delete,
                        st.config.use_recycle_bin,
                    )
                };
                if count == 0 {
                    return;
                }
                if confirm {
                    let body = crate::view::delete_dialog_text(count, recycle);
                    ui.set_util_delete_text(body.into());
                    ui.set_show_util_delete_dialog(true);
                } else {
                    perform_util_delete(&ui, &state);
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_confirm_delete(move || {
            with_ui(&ui_weak, |ui| {
                ui.set_show_util_delete_dialog(false);
                perform_util_delete(&ui, &state);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_util_cancel_delete(move || {
            with_ui(&ui_weak, |ui| ui.set_show_util_delete_dialog(false));
        });
    }
}

/// 隔離移動（破損ファイル §3.9.4 既定アクション）を結線する。
fn wire_util_quarantine(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_request_quarantine(move || {
            with_ui(&ui_weak, |ui| {
                if lock(&state).util_checked.is_empty() {
                    return;
                }
                ui.set_show_util_quarantine_dialog(true);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_util_confirm_quarantine(move || {
            with_ui(&ui_weak, |ui| {
                let dest = ui.get_util_quarantine_dest().to_string();
                ui.set_show_util_quarantine_dialog(false);
                perform_util_quarantine(&ui, &state, dest);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_util_browse_quarantine(move || {
            let Some(picked) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            with_ui(&ui_weak, |ui| {
                ui.set_util_quarantine_dest(picked.to_string_lossy().into_owned().into());
            });
        });
    }
}

/// ページ遷移時にユーティリティ結果をクリアする（前ページの結果が残らないように）。
fn wire_util_reset(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_util_reset(move || {
        with_ui(&ui_weak, |ui| {
            let mut st = lock(&state);
            st.clear_util();
            refresh_util(&ui, &st);
        });
    });
}

/// ユーティリティ走査をワーカースレッドで実行し、結果を UI へ戻す。
fn perform_util_scan(ui: &AppWindow, state: &Arc<Mutex<AppState>>, kind: UtilityKind) {
    let (roots, cfg) = {
        let st = lock(state);
        (
            resolve_roots(&st.config),
            UtilityConfig::from_app_config(&st.config),
        )
    };
    if roots.is_empty() {
        ui.set_status_text("対象フォルダが未設定です（「フォルダ追加」で追加してください）".into());
        return;
    }

    ui.set_is_searching(true);
    ui.set_status_text("スキャン中…".into());
    let cancel = new_cancel();
    {
        let mut st = lock(state);
        st.cancel = Some(Arc::clone(&cancel));
        st.clear_util();
        refresh_util(ui, &st);
    }

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let (tx, rx) = channel::<ProgressEvent>();
        // 進捗イベントは UI へは出さず破棄する（ユーティリティ走査は単フェーズ）。
        let drain = thread::spawn(move || rx.into_iter().count());
        let started = Instant::now();
        let scanner = UtilityScanner::new(cfg);
        let items = scanner.scan(kind, &roots, &tx, &cancel);
        drop(tx);
        // tx drop で rx が閉じ drain は自然終了する。パニック時のみ記録する。
        if drain.join().is_err() {
            tracing::warn!("進捗ドレインスレッドがパニックしました");
        }
        let cancelled = is_cancelled(&cancel);
        let secs = started.elapsed().as_secs_f64();
        let count = items.len();
        dispatch_ui(&ui_weak, move |ui| {
            let mut st = lock(&state);
            st.set_util_items(items);
            st.cancel = None;
            refresh_util(&ui, &st);
            ui.set_is_searching(false);
            let status = if cancelled {
                format!("中断しました（{count} 件検出・{secs:.1} 秒）")
            } else {
                format!("完了 · {count} 件検出 · 処理時間 {secs:.1} 秒")
            };
            ui.set_status_text(status.into());
        });
    });
}

/// サムネイルキャッシュ削除候補をスキャンする（§3.10.4・整理）。
///
/// core の [`find_thumbnail_cache_items`] が対象フォルダ配下の `Thumbs.db` ＋ ユーザー全体の
/// システムキャッシュ（対象フォルダ無関係）を列挙する。検出 6 種と同じ一覧 UI・選択・削除
/// フローを共用する（削除は [`perform_util_delete`]）。対象フォルダ未設定でもシステムキャッシュ
/// は列挙されるため、早期 return はしない。
fn perform_thumbnail_scan(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let roots = {
        let st = lock(state);
        resolve_roots(&st.config)
    };

    ui.set_is_searching(true);
    ui.set_status_text("スキャン中…".into());
    let cancel = new_cancel();
    {
        let mut st = lock(state);
        st.cancel = Some(Arc::clone(&cancel));
        st.clear_util();
        refresh_util(ui, &st);
    }

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let started = Instant::now();
        let items = find_thumbnail_cache_items(&roots, &cancel);
        let cancelled = is_cancelled(&cancel);
        let secs = started.elapsed().as_secs_f64();
        let count = items.len();
        dispatch_ui(&ui_weak, move |ui| {
            let mut st = lock(&state);
            st.set_util_items(items);
            st.cancel = None;
            refresh_util(&ui, &st);
            ui.set_is_searching(false);
            let status = if cancelled {
                format!("中断しました（{count} 件検出・{secs:.1} 秒）")
            } else {
                format!("完了 · {count} 件検出 · 処理時間 {secs:.1} 秒")
            };
            ui.set_status_text(status.into());
        });
    });
}

/// 選択中のユーティリティ項目を削除する（ごみ箱/完全削除・ワーカースレッド・§3.6）。
fn perform_util_delete(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let (snap, mode) = {
        let st = lock(state);
        let mode = if st.config.use_recycle_bin {
            DeleteMode::RecycleBin
        } else {
            DeleteMode::Permanent
        };
        let snap = UtilOpSnapshot {
            paths: st.util_checked_paths(),
            locks: st.util_locks.clone(),
            save_log: st.config.save_delete_log,
            config_dir: st.config_dir.clone(),
            delete_ro: st.config.delete_read_only,
            update_list: st.config.update_list_after_delete,
        };
        (snap, mode)
    };
    let UtilOpSnapshot {
        paths,
        locks,
        save_log,
        config_dir,
        delete_ro,
        update_list,
    } = snap;
    if paths.is_empty() {
        ui.set_status_text("削除対象がありません".into());
        return;
    }

    ui.set_is_searching(true);
    ui.set_status_text("削除中…".into());

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let fs = RealFs;
        let deleter = Deleter::new(&fs);
        // 「1つ残す」は適用しない（plan_paths）。ロック・使用中等の安全フィルタは適用する。
        let plan = deleter.plan_paths(&paths, &locks, delete_ro);
        let report = deleter.execute_logged(
            &plan,
            FileOp::Delete(mode),
            &config_dir,
            save_log,
            shredy_core::now_unix_secs(),
        );
        let deleted: Vec<LongPath> = report.deleted.iter().map(|(p, _)| p.clone()).collect();
        let ok = report.deleted.len();
        let failed = report.failed.len();
        finish_util_op(
            &ui_weak,
            &state,
            deleted,
            update_list,
            format!("{ok} 件削除, {failed} 件失敗"),
        );
    });
}

/// 選択中のユーティリティ項目を隔離フォルダへ移動する（破損 §3.9.4 / ワーカースレッド）。
fn perform_util_quarantine(ui: &AppWindow, state: &Arc<Mutex<AppState>>, dest: String) {
    let dest = dest.trim().to_string();
    if dest.is_empty() {
        ui.set_status_text("隔離先フォルダを指定してください".into());
        return;
    }
    let Ok(dest_lp) = LongPath::new(&dest) else {
        ui.set_status_text("隔離先フォルダのパスが不正です".into());
        return;
    };
    let snap = {
        let st = lock(state);
        UtilOpSnapshot {
            paths: st.util_checked_paths(),
            locks: st.util_locks.clone(),
            save_log: st.config.save_delete_log,
            config_dir: st.config_dir.clone(),
            delete_ro: st.config.delete_read_only,
            update_list: st.config.update_list_after_delete,
        }
    };
    let UtilOpSnapshot {
        paths,
        locks,
        save_log,
        config_dir,
        delete_ro,
        update_list,
    } = snap;
    if paths.is_empty() {
        ui.set_status_text("隔離対象がありません".into());
        return;
    }

    ui.set_is_searching(true);
    ui.set_status_text("隔離中…".into());

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let fs = RealFs;
        let deleter = Deleter::new(&fs);
        let plan = deleter.plan_paths(&paths, &locks, delete_ro);
        let dest_fs = dest_lp.fs_path();
        // 隔離はフォルダ構造を再現しない（roots 空＝隔離先直下へ・同名は自動リネーム）。
        let report = deleter.execute_logged(
            &plan,
            FileOp::Move {
                dest: dest_fs.as_ref(),
                roots: &[],
            },
            &config_dir,
            save_log,
            shredy_core::now_unix_secs(),
        );
        let moved: Vec<LongPath> = report.deleted.iter().map(|(p, _)| p.clone()).collect();
        let ok = report.deleted.len();
        let failed = report.failed.len();
        finish_util_op(
            &ui_weak,
            &state,
            moved,
            update_list,
            format!("{ok} 件を隔離, {failed} 件失敗（{dest}）"),
        );
    });
}

/// 削除/隔離の完了を UI スレッドで反映する（処理済みを一覧から除去・状態テキスト）。
///
/// `update_list=true`（既定）なら処理済みを除いた一覧へ更新する。`false` なら一覧を保持し
/// 削除選択のみ解除する（再スキャンで除去）。
fn finish_util_op(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    processed: Vec<LongPath>,
    update_list: bool,
    status: String,
) {
    let state = Arc::clone(state);
    dispatch_ui(ui_weak, move |ui| {
        let mut st = lock(&state);
        if update_list {
            let remaining = remove_util_deleted(&st.util_items, &processed);
            st.set_util_items(remaining);
        } else {
            st.util_clear_checks();
        }
        refresh_util(&ui, &st);
        ui.set_is_searching(false);
        ui.set_status_text(status.into());
    });
}

/// 処理済み（削除/移動）パスを除いたユーティリティ項目を返す（イミュータブル: 新ベクタ）。
fn remove_util_deleted(items: &[UtilityItem], processed: &[LongPath]) -> Vec<UtilityItem> {
    let gone: HashSet<&Path> = processed.iter().map(|p| p.as_path()).collect();
    items
        .iter()
        .filter(|it| !gone.contains(it.path.as_path()))
        .cloned()
        .collect()
}
