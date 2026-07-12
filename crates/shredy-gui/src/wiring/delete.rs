//! 削除・移動・手動削除（§3.6 / §3.6a）と結果 I/O・再チェックの結線。
//!
//! 削除/移動/保存/読み込み/再チェックの I/O はワーカースレッドで実行し UI を固めない。
//! 削除・移動は core の `Deleter::plan` を通し「1つ残す」(§3.4)・ロック・使用中除外を適用する。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use slint::ComponentHandle;

use shredy_core::{
    DeleteLog, DeleteMode, Deleter, DuplicateGroup, ExecListWriter, FileOp, KeepFileRule, LockSet,
    LongPath, RealFs, Selection,
};

use shredy_gui::bridge::{groups as group_edit, selection};
use shredy_gui::viewmodel::SelectionState;

use crate::state::{lock, AppState};
use crate::view::{delete_dialog_text, dispatch_ui, open_in_explorer, refresh_ui};
use crate::AppWindow;

use super::with_ui;

pub(crate) fn wire_delete(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // 「選択削除」: 確認が必要ならダイアログ、不要なら即実行。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_request_delete(move || {
            with_ui(&ui_weak, |ui| {
                let (count, confirm, recycle) = {
                    let st = lock(&state);
                    (
                        st.selection.checked_count(),
                        st.config.confirm_before_delete,
                        st.config.use_recycle_bin,
                    )
                };
                if count == 0 {
                    return;
                }
                if confirm {
                    ui.set_delete_dialog_text(delete_dialog_text(count, recycle).into());
                    ui.set_show_delete_dialog(true);
                } else {
                    perform_delete(&ui, &state);
                }
            });
        });
    }
    // ダイアログ「実行」。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_confirm_delete(move || {
            with_ui(&ui_weak, |ui| {
                ui.set_show_delete_dialog(false);
                perform_delete(&ui, &state);
            });
        });
    }
    // ダイアログ「キャンセル」。
    {
        let ui_weak = ui.as_weak();
        ui.on_cancel_delete(move || {
            with_ui(&ui_weak, |ui| ui.set_show_delete_dialog(false));
        });
    }
    // 移動先フォルダの参照（ネイティブダイアログ, U12 M8b）。
    {
        let ui_weak = ui.as_weak();
        ui.on_browse_move_dest(move || {
            let Some(picked) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            with_ui(&ui_weak, |ui| {
                ui.set_move_dest(picked.to_string_lossy().into_owned().into());
            });
        });
    }
    // 「移動」実行（選択ファイルを移動先へ, §3.6 / U12 M8b）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_confirm_move(move || {
            with_ui(&ui_weak, |ui| {
                let dest = ui.get_move_dest().to_string();
                ui.set_show_move_dialog(false);
                perform_move(&ui, &state, dest);
            });
        });
    }
    // 手動削除（§3.6a / U12 M8b）: 選択パスを exec-list へ書き出し PowerShell コマンドを提示。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_request_manual(move || open_manual_delete(&ui_weak, &state));
    }
    // 手動削除のモード切替（0=ごみ箱 / 1=完全削除 / 2=移動）でコマンドを再生成。
    {
        let ui_weak = ui.as_weak();
        ui.on_set_manual_mode(move |i| {
            with_ui(&ui_weak, |ui| {
                let list_str = ui.get_manual_list_path().to_string();
                if list_str.is_empty() {
                    return; // ダイアログ未生成時の誤発火を無視。
                }
                let cmd = manual_command(Path::new(&list_str), i, &ui.get_move_dest());
                ui.set_manual_mode(i);
                ui.set_manual_command(cmd.into());
                ui.set_manual_copied(false);
            });
        });
    }
    // exec-list の場所をエクスプローラーで開く（ファイル選択表示）。
    {
        let ui_weak = ui.as_weak();
        ui.on_open_manual_location(move || {
            with_ui(&ui_weak, |ui| {
                if let Ok(lp) = LongPath::new(ui.get_manual_list_path().as_str()) {
                    open_in_explorer(&lp);
                }
            });
        });
    }
    // 生成コマンドをクリップボードへコピー（U12 polish）。
    {
        let ui_weak = ui.as_weak();
        ui.on_copy_manual(move || {
            with_ui(&ui_weak, |ui| {
                let cmd = ui.get_manual_command().to_string();
                match copy_to_clipboard(&cmd) {
                    Ok(()) => ui.set_manual_copied(true),
                    Err(e) => {
                        tracing::warn!("クリップボードへのコピーに失敗しました: {e}");
                        ui.set_status_text("クリップボードへのコピーに失敗しました".into());
                    }
                }
            });
        });
    }
}

/// 文字列をクリップボードへコピーする（`arboard`）。
fn copy_to_clipboard(text: &str) -> Result<(), arboard::Error> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text.to_string())
}

/// 手動削除モード（0/1/2）に応じた PowerShell コマンドを生成する。
///
/// モード 2（移動）は `dest` が `Some` のとき core 側で Move-Item になる（第 2 引数の
/// `DeleteMode` は無視される＝ダミー）。`dest` 未指定時は無効なコマンドを避けて案内文を返す。
fn manual_command(list: &Path, mode: i32, dest: &str) -> String {
    match mode {
        2 if dest.trim().is_empty() => {
            "（移動先フォルダを「選択ファイルを移動」で指定してください）".to_string()
        }
        2 => ExecListWriter::powershell_command(list, DeleteMode::RecycleBin, Some(dest)),
        1 => ExecListWriter::powershell_command(list, DeleteMode::Permanent, None),
        _ => ExecListWriter::powershell_command(list, DeleteMode::RecycleBin, None),
    }
}

/// 選択パスを exec-list へ書き出し、手動削除ダイアログを開く（§3.6a）。
fn open_manual_delete(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    with_ui(ui_weak, |ui| {
        let (paths, dir) = {
            let st = lock(state);
            let paths: Vec<LongPath> = selection::checked_paths(&st.groups, &st.selection)
                .into_iter()
                .filter(|p| !st.locks.is_locked(p))
                .collect();
            (paths, st.config_dir.clone())
        };
        if paths.is_empty() {
            ui.set_status_text("手動削除の対象がありません（ロック/未選択）".into());
            return;
        }
        let stamp = DeleteLog::date_stamp(shredy_core::now_unix_secs());
        match ExecListWriter::write_list(&dir, &paths, &stamp) {
            Ok(list) => {
                let cmd = manual_command(&list, 0, &ui.get_move_dest());
                ui.set_manual_list_path(list.to_string_lossy().into_owned().into());
                ui.set_manual_mode(0);
                ui.set_manual_command(cmd.into());
                ui.set_manual_copied(false);
                ui.set_show_manual_dialog(true);
            }
            Err(e) => {
                tracing::error!("手動削除リストの書き込みに失敗しました: {e}");
                ui.set_status_text("手動削除リストの書き込みに失敗しました".into());
            }
        }
    });
}

/// 移動をワーカースレッドで実行し、結果をグループへ反映する（§3.6, U12 M8b）。
fn perform_move(ui: &AppWindow, state: &Arc<Mutex<AppState>>, dest: String) {
    let dest = dest.trim().to_string();
    if dest.is_empty() {
        ui.set_status_text("移動先フォルダを指定してください".into());
        return;
    }
    // 移動先をロングパス正規化（`\\?\`・最大 4096 字）。不正パスは中断。
    let Ok(dest_lp) = LongPath::new(&dest) else {
        ui.set_status_text("移動先フォルダのパスが不正です".into());
        return;
    };
    // 削除と同じく plan() を通し「1つ残す」(§3.4)・ロック・使用中除外を適用する。
    // 直接 move_to を呼ぶとグループ全件チェック時に全件移動してグループが空になり
    // 「1つ残す」が壊れるため、削除経路と対称化する。
    let (snap, roots) = {
        let st = lock(state);
        let paths: Vec<LongPath> = selection::checked_paths(&st.groups, &st.selection)
            .into_iter()
            .filter(|p| !st.locks.is_locked(p))
            .collect();
        // 構造再現の基準: チェック済みの登録対象フォルダ（§3.10）。不正パスは除外。
        let roots: Vec<LongPath> = st
            .config
            .target_folders
            .iter()
            .filter(|t| t.checked)
            .filter_map(|t| LongPath::new(&t.path).ok())
            .collect();
        let snap = DeleteSnapshot {
            groups: st.groups.clone(),
            sel: Selection::from_paths(paths),
            rule: st.config.keep_file_rule,
            locks: st.locks.clone(),
            save_log: st.config.save_delete_log,
            config_dir: st.config_dir.clone(),
            delete_read_only: st.config.delete_read_only,
            update_list: st.config.update_list_after_delete,
        };
        (snap, roots)
    };
    let DeleteSnapshot {
        groups,
        sel,
        rule,
        locks,
        save_log,
        config_dir,
        delete_read_only,
        update_list,
    } = snap;
    if sel.is_empty() {
        ui.set_status_text("移動対象がありません（ロック/未選択）".into());
        return;
    }

    ui.set_is_searching(true);
    ui.set_status_text("移動中…".into());

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let fs = RealFs;
        let deleter = Deleter::new(&fs);
        let plan = deleter.plan(&groups, &sel, rule, &locks, delete_read_only);
        // 移動先も `\\?\`（fs_path）で渡し、長パスの dest にも対応する。実行＋監査ログ
        // （移動も §3.6 の記録対象）は core の execute_logged に集約。
        let dest_fs = dest_lp.fs_path();
        let report = deleter.execute_logged(
            &plan,
            FileOp::Move {
                dest: dest_fs.as_ref(),
                roots: &roots,
            },
            &config_dir,
            save_log,
            shredy_core::now_unix_secs(),
        );
        let moved: Vec<LongPath> = report.deleted.iter().map(|(p, _)| p.clone()).collect();
        let remaining = group_edit::remove_deleted(&groups, &moved);
        let ok = report.deleted.len();
        let failed = report.failed.len();
        let state = Arc::clone(&state);
        dispatch_ui(&ui_weak, move |ui| {
            let mut st = lock(&state);
            // UpdateListAfterDelete=false なら一覧を保持（移動済み項目を残す）。
            if update_list {
                st.set_groups(remaining);
            }
            st.selection = SelectionState::new();
            refresh_ui(&ui, &st);
            ui.set_is_searching(false);
            ui.set_status_text(format!("{ok} 件移動, {failed} 件失敗（{dest}）").into());
        });
    });
}

/// 結果保存（§3.2）/ 見直す（保存結果の読み込み）の結線。
pub(crate) fn wire_results_io(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // 結果保存: 現在の重複グループを JSON で書き出す。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_save_result(move || {
            let groups = lock(&state).groups.clone(); // Arc::clone（参照カウント増のみ）
            if groups.is_empty() {
                with_ui(&ui_weak, |ui| {
                    ui.set_status_text("保存する結果がありません".into())
                });
                return;
            }
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Shredy 結果 (JSON)", &["json"])
                .set_file_name("shredy_results.json")
                .save_file()
            else {
                return; // キャンセル
            };
            // 直列化＋書き込みはワーカースレッドで行い UI を固めない（大量結果・NAS 保存先対策）。
            with_ui(&ui_weak, |ui| ui.set_status_text("結果を保存中…".into()));
            let ui_weak = ui_weak.clone();
            thread::spawn(move || {
                let result = shredy_gui::bridge::persist::to_json(&groups)
                    .map_err(|e| e.to_string())
                    .and_then(|json| std::fs::write(&path, json).map_err(|e| e.to_string()));
                dispatch_ui(&ui_weak, move |ui| match result {
                    Ok(()) => ui.set_status_text(
                        format!("結果を保存しました（{}）", path.display()).into(),
                    ),
                    Err(e) => {
                        tracing::error!("結果の保存に失敗しました: {e}");
                        ui.set_status_text("結果の保存に失敗しました".into());
                    }
                });
            });
        });
    }
    // 見直す: 保存済み JSON を読み込み結果として表示する（実体は再走査しない）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_review(move || {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Shredy 結果 (JSON)", &["json"])
                .pick_file()
            else {
                return;
            };
            // 読み込み＋パースはワーカースレッドで行い UI を固めない（大量結果・NAS 元対策）。
            with_ui(&ui_weak, |ui| {
                ui.set_status_text("保存結果を読み込み中…".into())
            });
            let ui_weak = ui_weak.clone();
            let state = Arc::clone(&state);
            thread::spawn(move || {
                let loaded = std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|text| {
                        shredy_gui::bridge::persist::from_json(&text).map_err(|e| e.to_string())
                    });
                dispatch_ui(&ui_weak, move |ui| match loaded {
                    Ok(groups) => {
                        let count = groups.len();
                        let mut st = lock(&state);
                        // 別結果セットの読み込み: 選択・ロックは旧パス基準のためリセット。
                        st.set_groups(groups);
                        st.selection = SelectionState::new();
                        st.locks = shredy_core::LockSet::new();
                        refresh_ui(&ui, &st);
                        ui.set_status_text(
                            format!("保存結果を読み込みました（{count} グループ）").into(),
                        );
                    }
                    Err(e) => {
                        tracing::error!("保存結果の読み込みに失敗しました: {e}");
                        ui.set_status_text("保存結果の読み込みに失敗しました".into());
                    }
                });
            });
        });
    }
}

/// 「再チェック（存在確認）」: 結果一覧の各ファイルがディスク上に存在するか再確認し、
/// 消えたファイルを除外する（§3.2）。重複が解消したグループは除く。
pub(crate) fn wire_recheck(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_recheck(move || {
        with_ui(&ui_weak, |ui| {
            let groups = lock(&state).groups.clone();
            if groups.is_empty() {
                return;
            }
            ui.set_is_searching(true);
            ui.set_status_text("存在確認中…".into());
            let ui_weak = ui.as_weak();
            let state = Arc::clone(&state);
            thread::spawn(move || {
                // ディスク上に存在しないファイルを収集（`\\?\` パスは std で判定可能）。
                let missing: Vec<LongPath> = groups
                    .iter()
                    .flat_map(|g| g.files.iter())
                    .filter(|f| !f.path.as_path().exists())
                    .map(|f| f.path.clone())
                    .collect();
                let remaining = group_edit::remove_deleted(&groups, &missing);
                let missing_count = missing.len();
                let state = Arc::clone(&state);
                dispatch_ui(&ui_weak, move |ui| {
                    let mut st = lock(&state);
                    st.set_groups(remaining);
                    st.selection = SelectionState::new();
                    refresh_ui(&ui, &st);
                    ui.set_is_searching(false);
                    ui.set_status_text(
                        format!("再チェック完了（{missing_count} 件が見つからず除外）").into(),
                    );
                });
            });
        });
    });
}

/// 削除・移動ワーカーへ渡す設定スナップショット（ロック下で取得し即時解放）。
///
/// 同型の `bool` が 3 つ（`save_log`/`delete_read_only`/`update_list`）並ぶため、
/// 位置依存のタプルではなく名前付きフィールドで取り違えを型レベルで防ぐ（§14）。
struct DeleteSnapshot {
    groups: Arc<Vec<DuplicateGroup>>,
    sel: Selection,
    rule: KeepFileRule,
    locks: LockSet,
    save_log: bool,
    config_dir: PathBuf,
    delete_read_only: bool,
    update_list: bool,
}

/// 削除をワーカースレッドで実行し、結果をグループへ反映する（§3.6）。
///
/// 削除 I/O は時間がかかり得るため UI スレッドをブロックしない。実行中は
/// `is_searching` で UI を抑止し、完了時に `upgrade_in_event_loop` で反映する。
fn perform_delete(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // ロック下で削除条件をスナップショット（groups はクローン）してから解放。
    let (snap, mode) = {
        let st = lock(state);
        // ロック済みは削除対象から除外（UI でも抑止するが防御的に）。
        let paths: Vec<LongPath> = selection::checked_paths(&st.groups, &st.selection)
            .into_iter()
            .filter(|p| !st.locks.is_locked(p))
            .collect();
        let mode = if st.config.use_recycle_bin {
            DeleteMode::RecycleBin
        } else {
            DeleteMode::Permanent
        };
        let snap = DeleteSnapshot {
            groups: st.groups.clone(),
            sel: Selection::from_paths(paths),
            rule: st.config.keep_file_rule,
            locks: st.locks.clone(),
            save_log: st.config.save_delete_log,
            config_dir: st.config_dir.clone(),
            delete_read_only: st.config.delete_read_only,
            update_list: st.config.update_list_after_delete,
        };
        (snap, mode)
    };
    let DeleteSnapshot {
        groups,
        sel,
        rule,
        locks,
        save_log,
        config_dir,
        delete_read_only,
        update_list,
    } = snap;

    ui.set_is_searching(true);
    ui.set_status_text("削除中…".into());

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let fs = RealFs;
        let deleter = Deleter::new(&fs);
        let plan = deleter.plan(&groups, &sel, rule, &locks, delete_read_only);
        if plan.to_delete.is_empty() {
            finish_delete(&ui_weak, &state, groups, 0, 0, true, update_list);
            return;
        }
        // 実行＋監査ログ（init→実行→append→完了 INFO）は core の execute_logged に集約。
        let report = deleter.execute_logged(
            &plan,
            FileOp::Delete(mode),
            &config_dir,
            save_log,
            shredy_core::now_unix_secs(),
        );
        let deleted: Vec<LongPath> = report.deleted.iter().map(|(p, _)| p.clone()).collect();
        let remaining = Arc::new(group_edit::remove_deleted(&groups, &deleted));
        finish_delete(
            &ui_weak,
            &state,
            remaining,
            report.deleted.len(),
            report.failed.len(),
            false,
            update_list,
        );
    });
}

/// 削除完了を UI スレッドで反映する（グループ更新・選択クリア・状態テキスト）。
///
/// `update_list=true`（設定 `UpdateListAfterDelete`・既定）なら削除済みを除いた `remaining`
/// で一覧を更新する。`false` なら一覧を保持し、削除済み項目を残す（手動の「再チェック」で除去）。
fn finish_delete(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    remaining: Arc<Vec<DuplicateGroup>>,
    ok: usize,
    failed: usize,
    nothing_to_delete: bool,
    update_list: bool,
) {
    let state = Arc::clone(state);
    dispatch_ui(ui_weak, move |ui| {
        let mut st = lock(&state);
        if update_list {
            st.set_groups_arc(remaining);
        }
        st.selection = SelectionState::new();
        refresh_ui(&ui, &st);
        ui.set_is_searching(false);
        let status = if nothing_to_delete {
            "削除対象がありません（ロック/使用中、または「1つ残す」により除外）".to_string()
        } else {
            format!("{ok} 件削除, {failed} 件失敗")
        };
        ui.set_status_text(status.into());
    });
}
