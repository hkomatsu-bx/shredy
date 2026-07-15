//! コンテキストメニュー（§3.4 / §3.5a）・起点フォルダ選択/ロック・プレビューの結線。

use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, ModelRc, VecModel};

use shredy_core::{DuplicateGroup, LongPath};

use shredy_gui::bridge::{preview, selection};

use crate::state::{AppState, lock};
use crate::view::{file_path_at, open_in_explorer, open_with_default, refresh_ui, sync_highlight};
use crate::{AppWindow, FormatField};

use super::{apply_row_change, with_ui};

pub(crate) fn wire_context(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // ↑ 親フォルダ（起点を 1 階層上げる, §3.5a）。状態に依らない純変換。
    ui.on_parent_path(|p| selection::parent_dir(p.as_str()).into());
    // ロック切替（ロック時は削除選択からも外す）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_toggle_lock(move |g, f| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                let Some(path) = file_path_at(&st, g, f) else {
                    return;
                };
                if st.locks.is_locked(&path) {
                    st.locks.unlock(&path);
                } else {
                    st.locks.lock(&path);
                    st.selection.set_checked((g as usize, f as usize), false);
                }
                apply_row_change(&ui, &st, g, f);
            });
        });
    }
    // エクスプローラーで開く（ファイルを選択表示）。
    {
        let state = Arc::clone(state);
        ui.on_open_in_explorer(move |g, f| {
            let path = {
                let st = lock(&state);
                file_path_at(&st, g, f)
            };
            if let Some(path) = path {
                open_in_explorer(&path);
            }
        });
    }
    // フォルダ直下選択（invert で直下以外）。
    wire_origin_select(ui, state, SelectKind::Direct);
    // ツリー配下選択（invert で配下以外）。
    wire_origin_select(ui, state, SelectKind::Tree);
    // パターン選択（起点フォルダ TextBox の値をワイルドカードとして使用）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_select_pattern_cb(move |pattern| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                apply_select_pattern_to_state(&mut st, pattern.as_str());
                refresh_ui(&ui, &st);
            });
        });
    }
    // 一括ロック（§3.4, U12 M8a）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_lock_all(move || {
            lock_op(&ui_weak, &state, |g, rule, locks| {
                selection::apply_lock_all(g, rule, locks);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_unlock_all(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                selection::unlock_all(&mut st.locks);
                refresh_ui(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_lock_pattern_cb(move |pattern| {
            lock_op(&ui_weak, &state, move |g, rule, locks| {
                selection::apply_lock_pattern(g, pattern.as_str(), rule, locks);
            });
        });
    }
    // フォルダ直下/ツリー配下ロック（コンテキストメニュー, §3.4）。
    wire_origin_lock(ui, state, SelectKind::Direct);
    wire_origin_lock(ui, state, SelectKind::Tree);
    // プレビュー切替（§3.5）: パネル開閉。開く時はハイライト行を読み込む。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_preview_toggle(move || {
            with_ui(&ui_weak, |ui| {
                let open = !ui.get_preview_open();
                ui.set_preview_open(open);
                if open {
                    let st = lock(&state);
                    load_preview(&ui, &st);
                }
            });
        });
    }
    // ダブルクリック: その行をハイライトするのみ（プレビューは開かない, polish3 指摘）。
    // プレビューはコンテキストメニュー「プレビュー表示切替」から開く。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_row_double_clicked(move |g, f| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.selection.highlight((g as usize, f as usize));
                // ハイライトのみ変更: sync_highlight で差分更新（全再構築を避ける, U12 perf）。
                sync_highlight(&ui, &st);
                // プレビューを開いていれば内容を追従させる（§3.5）。
                reload_preview_if_open(&ui, &st);
            });
        });
    }
    // 画像プレビューの 90° 回転。
    {
        let ui_weak = ui.as_weak();
        ui.on_preview_rotate(move || {
            with_ui(&ui_weak, |ui| {
                let r = ui.get_preview_rotation();
                ui.set_preview_rotation(if r >= 270 { 0 } else { r + 90 });
            });
        });
    }
    // 非対応形式を関連付けアプリで開く。
    {
        let state = Arc::clone(state);
        ui.on_preview_open_with(move || {
            let path = {
                let st = lock(&state);
                st.selection
                    .highlighted()
                    .and_then(|(g, f)| file_path_at(&st, g as i32, f as i32))
            };
            if let Some(path) = path {
                open_with_default(&path);
            }
        });
    }
    // プレビューパネルの「フォルダを表示」（§3.5）。ハイライト行の格納フォルダを開く。
    {
        let state = Arc::clone(state);
        ui.on_preview_show_folder(move || {
            let path = {
                let st = lock(&state);
                st.selection
                    .highlighted()
                    .and_then(|(g, f)| file_path_at(&st, g as i32, f as i32))
            };
            if let Some(path) = path {
                open_in_explorer(&path);
            }
        });
    }
    // 未実装メニュー項目の通知（結果保存/見直す/再チェック/フォルダ比較）。
    {
        let ui_weak = ui.as_weak();
        ui.on_notify_unimplemented(move |label| {
            with_ui(&ui_weak, |ui| {
                ui.set_status_text(format!("「{label}」は後続マイルストーンで実装します").into());
            });
        });
    }
}

/// プレビューのメタ情報表示をクリアする（未選択時）。
fn clear_preview_meta(ui: &AppWindow) {
    ui.set_preview_kind(preview::PreviewKind::Other.as_i32());
    ui.set_preview_name("".into());
    ui.set_preview_ext("".into());
    clear_preview_exif(ui);
    set_preview_format_meta(ui, &[]);
}

/// 画像 EXIF 表示（§3.1#1）をクリアする。
fn clear_preview_exif(ui: &AppWindow) {
    ui.set_preview_exif_datetime("".into());
    ui.set_preview_exif_camera("".into());
    ui.set_preview_exif_lens("".into());
    ui.set_preview_exif_gps("".into());
}

/// 画像 EXIF 表示（§3.1#1）を設定する。
fn set_preview_exif(ui: &AppWindow, ex: &preview::PreviewExif) {
    ui.set_preview_exif_datetime(ex.datetime.as_str().into());
    ui.set_preview_exif_camera(ex.camera.as_str().into());
    ui.set_preview_exif_lens(ex.lens.as_str().into());
    ui.set_preview_exif_gps(ex.gps.as_str().into());
}

/// 形式固有メタデータ表示（§3.1#2）を設定する（空でクリア）。
fn set_preview_format_meta(ui: &AppWindow, rows: &[(String, String)]) {
    let fields: Vec<FormatField> = rows
        .iter()
        .map(|(k, v)| FormatField {
            key: k.as_str().into(),
            value: v.as_str().into(),
        })
        .collect();
    ui.set_preview_format_meta(ModelRc::new(VecModel::from(fields)));
}

/// プレビューが開いているときだけ、ハイライト行の内容を読み直す（§3.5）。
///
/// プレビューを開いたまま別行をハイライトした場合に内容を追従させる。
pub(crate) fn reload_preview_if_open(ui: &AppWindow, st: &AppState) {
    if ui.get_preview_open() {
        load_preview(ui, st);
    }
}

/// ハイライト行のプレビューを UI へ読み込む（§3.5）。画像/テキスト/非対応で出し分ける。
fn load_preview(ui: &AppWindow, st: &AppState) {
    // 前回内容が残らないよう先にクリア（テキストは即時、画像は kind 切替で非表示になる）。
    ui.set_preview_text("".into());
    let Some((g, f)) = st.selection.highlighted() else {
        clear_preview_meta(ui);
        return;
    };
    let Some(path) = file_path_at(st, g as i32, f as i32) else {
        clear_preview_meta(ui);
        return;
    };
    // メタ情報は種別（拡張子）のみ表示（サイズ等は一覧で確認可, §3.5 指摘）。
    let name = path
        .as_path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .as_path()
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .unwrap_or_else(|| "—".to_string());
    ui.set_preview_name(name.into());
    ui.set_preview_ext(ext.into());
    ui.set_preview_rotation(0);

    let kind = match preview::classify(&path) {
        preview::PreviewKind::Image if preview::image_within_limit(&path) => {
            // HEIC/HEIF は core でデコードして RGBA8 から構築、それ以外は OS デコーダ。
            // デコード不能はヘルパ内で tracing 記録のうえ None ＝ Other へフォールバック。
            match preview::load_image_preview(&path) {
                Some(img) => {
                    ui.set_preview_image(img);
                    preview::PreviewKind::Image
                }
                None => preview::PreviewKind::Other,
            }
        }
        preview::PreviewKind::Text => match preview::read_text_preview(&path) {
            Some(t) => {
                ui.set_preview_text(t.into());
                preview::PreviewKind::Text
            }
            None => preview::PreviewKind::Other,
        },
        // 画像だが上限超過、または非対応形式。
        _ => preview::PreviewKind::Other,
    };
    ui.set_preview_kind(kind.as_i32());

    // 画像プレビュー時は EXIF（撮影日時・カメラ・レンズ・GPS）を表示する（§3.1#1）。
    if kind == preview::PreviewKind::Image {
        match preview::read_preview_exif(&path) {
            Some(ex) => set_preview_exif(ui, &ex),
            None => clear_preview_exif(ui),
        }
    } else {
        clear_preview_exif(ui);
    }
    // 形式固有メタデータ（Office/ZIP, §3.1#2）。非対応形式は空になり行は表示されない。
    set_preview_format_meta(ui, &preview::read_preview_format_meta(&path));
}

/// ロック操作の共通処理: 状態内の groups/locks を分割借用して bridge のロック関数を適用し、
/// ロックされた行の削除選択チェックを外して UI を更新する（ロック行は削除対象外, §3.4）。
fn lock_op(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    apply: impl FnOnce(&[DuplicateGroup], shredy_core::KeepFileRule, &mut shredy_core::LockSet),
) {
    with_ui(ui_weak, |ui| {
        let mut st = lock(state);
        apply_lock_to_state(&mut st, apply);
        clear_checks_on_locked(&mut st);
        refresh_ui(&ui, &st);
    });
}

/// ロック済みファイルの削除選択チェックを外す（座標を先に収集してから可変更新）。
///
/// `groups`/`locks` は `AppState` の独立フィールド。先に不変借用で座標 Vec を作ってから
/// `selection` を可変更新することで借用衝突を避ける。
fn clear_checks_on_locked(st: &mut AppState) {
    let locked: Vec<(usize, usize)> = st
        .groups
        .iter()
        .enumerate()
        .flat_map(|(gi, g)| {
            g.files
                .iter()
                .enumerate()
                .filter(|(_, f)| st.locks.is_locked(&f.path))
                .map(move |(fi, _)| (gi, fi))
        })
        .collect();
    for key in locked {
        st.selection.set_checked(key, false);
    }
}

/// 直下/ツリー配下ロックを結線する（起点フォルダ string + invert を受ける）。
fn wire_origin_lock(ui: &AppWindow, state: &Arc<Mutex<AppState>>, kind: SelectKind) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    let handler = move |origin: slint::SharedString, invert: bool| {
        let Ok(origin_lp) = LongPath::new(origin.as_str()) else {
            with_ui(&ui_weak, |ui| {
                ui.set_status_text("起点フォルダのパスが不正です".into());
            });
            return;
        };
        lock_op(&ui_weak, &state, move |g, rule, locks| match kind {
            SelectKind::Direct => selection::apply_lock_direct(g, &origin_lp, invert, rule, locks),
            SelectKind::Tree => selection::apply_lock_tree(g, &origin_lp, invert, rule, locks),
        });
    };
    match kind {
        SelectKind::Direct => ui.on_lock_direct_cb(handler),
        SelectKind::Tree => ui.on_lock_tree_cb(handler),
    }
}

#[derive(Clone, Copy)]
enum SelectKind {
    Direct,
    Tree,
}

/// 直下/ツリー配下選択を結線する（起点フォルダ string + invert を受ける）。
fn wire_origin_select(ui: &AppWindow, state: &Arc<Mutex<AppState>>, kind: SelectKind) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    let handler = move |origin: slint::SharedString, invert: bool| {
        with_ui(&ui_weak, |ui| {
            let Ok(origin_lp) = LongPath::new(origin.as_str()) else {
                ui.set_status_text("起点フォルダのパスが不正です".into());
                return;
            };
            let mut st = lock(&state);
            apply_origin_select_to_state(&mut st, kind, &origin_lp, invert);
            refresh_ui(&ui, &st);
        });
    };
    match kind {
        SelectKind::Direct => ui.on_select_direct_cb(handler),
        SelectKind::Tree => ui.on_select_tree_cb(handler),
    }
}

fn apply_select_pattern_to_state(st: &mut AppState, pattern: &str) {
    let rule = st.config.keep_file_rule;
    let AppState {
        groups,
        selection: state,
        locks,
        ..
    } = st;
    selection::apply_select_pattern(groups, pattern, rule, locks, state);
}

fn apply_origin_select_to_state(
    st: &mut AppState,
    kind: SelectKind,
    origin: &LongPath,
    invert: bool,
) {
    let rule = st.config.keep_file_rule;
    let AppState {
        groups,
        selection: state,
        locks,
        ..
    } = st;
    match kind {
        SelectKind::Direct => {
            selection::apply_select_direct(groups, origin, invert, rule, locks, state);
        }
        SelectKind::Tree => {
            selection::apply_select_tree(groups, origin, invert, rule, locks, state);
        }
    }
}

fn apply_lock_to_state(
    st: &mut AppState,
    apply: impl FnOnce(&[DuplicateGroup], shredy_core::KeepFileRule, &mut shredy_core::LockSet),
) {
    let rule = st.config.keep_file_rule;
    let AppState { groups, locks, .. } = st;
    apply(groups, rule, locks);
}
