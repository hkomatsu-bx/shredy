//! 対象フォルダ管理の結線（§5）。
//!
//! 追加/削除/チェック切替は結果行に依存しないため `view::refresh_folders` で
//! フォルダ一覧＋サマリのみ更新し、結果行の全再構築を避ける。

use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use crate::config_io::{add_target_folders, batch_add_status};
use crate::state::{lock, save_config, AppState};
use crate::view::refresh_folders;
use crate::AppWindow;

use super::with_ui;

pub(crate) fn wire_folders(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // フォルダ追加（ネイティブダイアログ）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_add_folder(move || {
            // 複数フォルダをまとめて選択できる（rfd の複数選択ダイアログ, §5）。
            let Some(picked) = rfd::FileDialog::new().pick_folders() else {
                return; // キャンセル
            };
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                let summary = add_target_folders(&mut st.config, &picked);
                if summary.added > 0 {
                    save_config(&st);
                    // 結果行は不変。フォルダ一覧＋サマリのみ更新（全行再構築を避ける）。
                    refresh_folders(&ui, &st);
                }
                if summary.invalid > 0 {
                    tracing::warn!(
                        "一括追加で無効なパスを {} 件スキップしました",
                        summary.invalid
                    );
                }
                ui.set_status_text(batch_add_status(summary).into());
            });
        });
    }
    // フォルダ削除。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_remove_folder(move |i| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                let idx = i as usize;
                if idx < st.config.target_folders.len() {
                    st.config.target_folders.remove(idx);
                    save_config(&st);
                    refresh_folders(&ui, &st);
                }
            });
        });
    }
    // チェック切替。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_toggle_folder(move |i| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                let idx = i as usize;
                if let Some(tf) = st.config.target_folders.get_mut(idx) {
                    tf.checked = !tf.checked;
                    save_config(&st);
                    refresh_folders(&ui, &st);
                }
            });
        });
    }
    // 全て選択 / 全て解除（チェック状態の一括変更, U12 M6）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_check_all_folders(move || set_all_folders_checked(&ui_weak, &state, true));
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_uncheck_all_folders(move || set_all_folders_checked(&ui_weak, &state, false));
    }
    // 全て削除（登録フォルダを全消去, U12 M6）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_remove_all_folders(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.config.target_folders.clear();
                save_config(&st);
                refresh_folders(&ui, &st);
            });
        });
    }
}

/// 登録フォルダのチェック状態を一括設定する（全て選択/全て解除の共通処理）。
fn set_all_folders_checked(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    on: bool,
) {
    with_ui(ui_weak, |ui| {
        let mut st = lock(state);
        for tf in &mut st.config.target_folders {
            tf.checked = on;
        }
        save_config(&st);
        refresh_folders(&ui, &st);
    });
}
