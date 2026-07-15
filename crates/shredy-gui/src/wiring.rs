//! Slint コールバックとコアの結線（設計 §2.2）。
//!
//! 責務ごとのサブモジュールへ分割している（凝集度向上）。借用エンジン
//! (`SearchEngine<'a>`) は非 `Send` のため各ワーカースレッド内で構築し、結果
//! （`Send` な `Vec<DuplicateGroup>`）を `upgrade_in_event_loop` で UI へ戻す。
//!
//! - `scan` — 検索開始/中断/結果破棄
//! - `selection` — 選択二概念（§3.4）
//! - `folders` — 対象フォルダ管理（§5）
//! - `actionbar` — 比較方式・表示フィルタ
//! - `delete` — 削除/移動/手動削除・結果 I/O・再チェック（§3.6）
//! - `context` — コンテキストメニュー・起点選択/ロック・プレビュー（§3.4/§3.5）
//!
//! 本モジュールは各サブモジュールの公開結線関数を再エクスポートし、複数サブモジュールが
//! 共有する小ヘルパ（[`with_ui`]・[`apply_row_change`]）を保持する。

use shredy_gui::viewmodel::FilterMode;

use crate::AppWindow;
use crate::state::AppState;
use crate::view::{refresh_ui, update_file_row};

mod actionbar;
mod context;
mod delete;
mod exif;
mod folder_diff;
mod folders;
mod rename;
mod scan;
mod selection;
mod settings;
mod utility;

pub(crate) use actionbar::wire_actionbar;
pub(crate) use context::wire_context;
pub(crate) use delete::{wire_delete, wire_recheck, wire_results_io};
pub(crate) use exif::wire_exif;
pub(crate) use folder_diff::wire_folder_diff;
pub(crate) use folders::wire_folders;
pub(crate) use rename::wire_rename;
pub(crate) use scan::{wire_cancel, wire_navigation, wire_scan};
pub(crate) use selection::wire_selection;
pub(crate) use settings::wire_settings;
pub(crate) use utility::wire_utility;

/// UI スレッド上のコールバック内で `ui_weak` を upgrade し、生きていればクロージャを実行する。
///
/// ウィンドウ破棄後の遅延コールバックでは何もしない。ワーカースレッドから UI へ戻す
/// [`crate::view::dispatch_ui`] とは異なり、本関数は **同一（UI）スレッド上で即時** 実行する。
fn with_ui(ui_weak: &slint::Weak<AppWindow>, f: impl FnOnce(AppWindow)) {
    if let Some(ui) = ui_weak.upgrade() {
        f(ui);
    }
}

/// 単一行のチェック/ロック変更を UI へ反映する。
///
/// 表示フィルタ有効時は行の出入りで平坦 index がずれるため全再構築、Off 時は
/// 当該 1 行のみ差分更新して全 2000+ 行の再構築（`chrono` 整形含む）を避ける（U12 perf）。
fn apply_row_change(ui: &AppWindow, st: &AppState, g: i32, f: i32) {
    if st.filter_mode != FilterMode::Off {
        refresh_ui(ui, st);
    } else {
        update_file_row(ui, st, g, f);
    }
}
