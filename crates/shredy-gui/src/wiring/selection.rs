//! 選択二概念（§3.4）の結線。
//!
//! 「行選択（ハイライト・単一）」と「削除選択（チェック・複数）」を区別する。
//! ハイライトの真実源は `SelectionState` で、`view::sync_highlight` が Slint へ反映する。

use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use shredy_gui::bridge::selection;

use crate::AppWindow;
use crate::state::{AppState, lock};
use crate::view::{file_path_at, refresh_ui, sync_highlight};

use super::{apply_row_change, with_ui};

/// 選択二概念（§3.4）と全選択/選択解除のコールバック結線。
pub(crate) fn wire_selection(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_toggle_check(move |g, f| toggle_check(&ui_weak, &state, g, f));
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        // §3.4 改: シングルクリック＝フォーカス、フォーカス済み行の再クリック＝削除選択トグル。
        // g は元グループの絶対 index（grp.gi）。ロック行はトグル不可（チェックボックス同様）。
        // ハイライトの真実源は SelectionState で、refresh_ui が Slint へ反映する。
        ui.on_row_clicked(move |g, f| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                let key = (g as usize, f as usize);
                if st.selection.highlighted() == Some(key) {
                    // フォーカス済み行の再クリック＝削除選択トグル（ロック行は不可）。
                    let locked = file_path_at(&st, g, f).is_some_and(|p| st.locks.is_locked(&p));
                    if !locked {
                        st.selection.toggle_check(key);
                        apply_row_change(&ui, &st, g, f);
                    }
                } else {
                    // ハイライトのみ変更: 真実源 SelectionState を更新し sync_highlight が
                    // Slint へ反映（モデル不変＝全再構築なし。二重管理を解消, polish3）。
                    st.selection.highlight(key);
                    sync_highlight(&ui, &st);
                    // プレビューを開いていれば内容を追従させる（§3.5）。
                    super::context::reload_preview_if_open(&ui, &st);
                }
            });
        });
    }
    // 全選択（core `Selector::select_all`＝「1つ残す」適用）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_select_all(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                apply_select_all_to_state(&mut st);
                refresh_ui(&ui, &st);
            });
        });
    }
    // 選択解除（削除選択のみクリア。ハイライトは保持）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_clear_selection(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.selection.clear_checks();
                refresh_ui(&ui, &st);
            });
        });
    }
}

fn toggle_check(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>, g: i32, f: i32) {
    with_ui(ui_weak, |ui| {
        let mut st = lock(state);
        // ロック行は削除選択にできない（行クリック経路と対称。多重防御として、最終的に
        // plan() も除外するが UI 状態の一貫性のためここでも弾く, §3.4）。
        if file_path_at(&st, g, f).is_some_and(|p| st.locks.is_locked(&p)) {
            return;
        }
        st.selection.toggle_check((g as usize, f as usize));
        apply_row_change(&ui, &st, g, f);
    });
}

fn apply_select_all_to_state(st: &mut AppState) {
    let rule = st.config.keep_file_rule;
    let AppState {
        groups,
        selection: state,
        locks,
        ..
    } = st;
    selection::apply_select_all(groups, rule, locks, state);
}
