//! アクションバー（比較方式プルダウン・表示フィルタ）の結線（U12 M6）。

use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use shredy_gui::viewmodel::FilterMode;

use crate::AppWindow;
use crate::state::{AppState, lock, save_config};
use crate::view::{refresh_summary, refresh_ui};

use super::with_ui;

/// 比較方式プルダウンと「選択済みのみ」フィルタの結線。
pub(crate) fn wire_actionbar(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // 比較方式の選択（次回スキャンへ保留: config 更新のみ。結果/キャッシュはスキャン時に解決）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_set_compare_method(move |i| {
            let Some(method) = crate::view::compare_method_from_index(i) else {
                return;
            };
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.config.compare_method = method;
                save_config(&st);
                // 結果行・フォルダ一覧は不変。方式ラベル/index を持つサマリのみ更新する。
                refresh_summary(&ui, &st);
            });
        });
    }
    // 表示フィルタ（選択済みのみ/未選択のみ）。同一モード再選択で解除（表示のみ・選択は不変）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_set_filter(move |mode| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.filter_mode = st.filter_mode.toggled(FilterMode::from_i32(mode));
                refresh_ui(&ui, &st);
            });
        });
    }
}
