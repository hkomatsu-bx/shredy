//! `shredy-gui` エントリ。Slint ビューとコア配線を結線する（設計 §2.2 / M2）。
//!
//! 責務は薄く保つ（プログラミングガイド §14.1）：ロギング初期化 → 設定ロード →
//! ウィンドウ生成 → コールバック結線 → イベントループ起動。各責務は専用モジュールへ
//! 委譲する：[`logging`]・[`config_io`]・[`state`]・[`view`]・[`wiring`]。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use crate::config_io::{config_summary, initial_status, load_config};
use crate::state::{AppState, lock};
use crate::view::{refresh_rename, refresh_ui};
use crate::wiring::{
    wire_actionbar, wire_cancel, wire_context, wire_delete, wire_exif, wire_folder_diff,
    wire_folders, wire_navigation, wire_recheck, wire_rename, wire_results_io, wire_scan,
    wire_selection, wire_settings, wire_utility,
};

mod cache_io;
mod config_io;
mod logging;
mod recipe_io;
mod state;
mod view;
mod wiring;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    // 依存の log 出力を捕捉・抑制（icu の警告ノイズ対策）。Slint より先に install する。
    logging::install();

    let (config, config_path, config_dir, config_warn) = load_config();
    let startup_msg = format!("shredy-gui {}", shredy_core::VERSION);
    // init_logging が起動マーカーを INFO イベントとして記録する（遅延生成・仕様 §5.5）。
    // 削除ログ（shredy_delete.log）は起動時に作らず、削除実行時に生成する（§3.6, wiring）。
    // GUI はコンソールを持たないため stderr ミラーは無効（console=false）。ログは shredy.log のみ。
    let _log_guard = shredy_core::init_logging(
        &config_dir,
        config.log_level.to_filter(),
        &startup_msg,
        false,
    )
    .ok();
    // 設定ロード時の警告はロギング初期化後に記録する（規約: 診断は shredy.log へ）。
    if let Some(w) = config_warn {
        tracing::warn!("{w}");
    }

    let ui = AppWindow::new()?;
    // バージョンはコア（git コミット数由来）を唯一の情報源とする（slint 側のハードコードを排除）。
    ui.set_app_version(format!("v{}", shredy_core::version()).into());
    ui.set_config_summary(config_summary(&config).into());
    ui.set_status_text(initial_status(&config).into());

    let mut app_state = AppState::new(config, config_path, config_dir);
    // 保存済みリネームレシピ（§3.10.1 / C-M4）。破損は空＋警告ログで続行。
    app_state.recipes = recipe_io::load_recipes(&app_state.config_dir);
    let state = Arc::new(Mutex::new(app_state));

    refresh_ui(&ui, &lock(&state));
    // 一括リネームの初期反映（保存済みレシピ一覧など。§3.10.1）。
    refresh_rename(&ui, &lock(&state));

    wire_scan(&ui, &state);
    wire_cancel(&ui, &state);
    wire_navigation(&ui, &state);
    wire_selection(&ui, &state);
    wire_folders(&ui, &state);
    wire_actionbar(&ui, &state);
    wire_delete(&ui, &state);
    wire_context(&ui, &state);
    wire_recheck(&ui, &state);
    wire_results_io(&ui, &state);
    wire_settings(&ui, &state);
    wire_utility(&ui, &state);
    wire_exif(&ui, &state);
    wire_rename(&ui, &state);
    wire_folder_diff(&ui, &state);

    let result = ui.run();
    // 終了ログ（イベントループ終了時。_log_guard はこの後に drop されるため記録される）。
    tracing::info!("アプリケーションを終了します");
    result
}
