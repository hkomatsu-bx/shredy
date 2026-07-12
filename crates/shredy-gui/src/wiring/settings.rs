//! オプション設定ページ（§3.8）の結線。
//!
//! 設定値は `SettingsView` で UI へ一方向に渡し、変更はキー名指定の 3 コールバック
//! （bool/int/text）で受けて core 設定（`AppConfig`）へ反映・即保存する。キー→フィールドの
//! 対応は純関数（`apply_*_setting`）に切り出してテスト可能にする。

use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use shredy_core::{
    AppConfig, AppTheme, CompareMethod, FileSizeUnit, KeepFileRule, LogLevelConfig,
    OfficeSimilarityLevel,
};

use crate::state::{lock, save_config, AppState};
use crate::view::{refresh_status_flags, refresh_summary, refresh_ui};
use crate::{AppWindow, SettingsView};

use super::with_ui;

/// 設定ページの結線。初期表示の反映も行う。
pub(crate) fn wire_settings(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // 初期表示（設定値＋テーマ）を反映する。
    {
        let st = lock(state);
        ui.set_settings(settings_view(&st.config));
        ui.set_theme_mode(i32::from(st.config.theme.index()));
    }

    // 真偽値トグル。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_set_setting_bool(move |key, val| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                if !apply_bool_setting(&mut st.config, key.as_str(), val) {
                    return;
                }
                save_config(&st);
                // 削除動作・キャッシュ・ログ表示（ステータスバー）とごみ箱チップ（サマリ）を同期。
                refresh_status_flags(&ui, &st);
                refresh_summary(&ui, &st);
            });
        });
    }

    // 選択肢（index）・数値（しきい値）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_set_setting_int(move |key, val| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                if !apply_int_setting(&mut st.config, key.as_str(), val) {
                    return;
                }
                save_config(&st);
                match key.as_str() {
                    // 保持ルールは派生キャッシュ（保持対象・回収量）と結果行の状態列に影響する。
                    "keep_rule" => {
                        st.refresh_result_cache();
                        refresh_ui(&ui, &st);
                    }
                    // テーマは UI へ反映（changed ハンドラが Palette を切り替える）。
                    "theme" => ui.set_theme_mode(val),
                    // 比較方式ラベル/index・ログレベル表示などを同期。
                    _ => {
                        refresh_summary(&ui, &st);
                        refresh_status_flags(&ui, &st);
                    }
                }
            });
        });
    }

    // テキスト（サイズフィルタ・拡張子・除外パターン）。次回スキャンへ反映＝即保存のみ。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_set_setting_text(move |key, val| {
            with_ui(&ui_weak, |ui| {
                let _ = ui;
                let mut st = lock(&state);
                if !apply_text_setting(&mut st.config, key.as_str(), val.as_str()) {
                    return;
                }
                save_config(&st);
            });
        });
    }

    // 既定値に戻す（22 設定項目のみ・確認ダイアログは app.slint 側で確定済み）。
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_reset_settings(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                reset_settings_to_default(&mut st.config);
                save_config(&st);
                // keep_rule も既定へ戻るため派生キャッシュ（保持対象・回収量）を再計算する。
                st.refresh_result_cache();
                // 22 項目を再 push し、テーマと結果行・ステータス・サマリを一括同期する。
                ui.set_settings(settings_view(&st.config));
                ui.set_theme_mode(i32::from(st.config.theme.index()));
                refresh_ui(&ui, &st);
            });
        });
    }
}

/// `AppConfig` から設定ページの表示状態を構築する。
pub(crate) fn settings_view(c: &AppConfig) -> SettingsView {
    SettingsView {
        compare_method: i32::from(c.compare_method.index()),
        image_threshold: i32::from(c.image_similarity_threshold),
        office_include_style: matches!(
            c.office_similarity_level,
            OfficeSimilarityLevel::IncludeStyle
        ),
        exclude_system: c.exclude_system_folders,
        exclude_zero: c.exclude_zero_byte_files,
        exclude_onedrive: c.exclude_onedrive_on_demand,
        size_min: c.file_size_filter_min.to_string().into(),
        size_max: c.file_size_filter_max.to_string().into(),
        size_unit: file_size_unit_index(c.file_size_filter_unit),
        include_ext: c.include_extensions.clone().into(),
        exclude_pat: c.exclude_patterns.clone().into(),
        use_recycle: c.use_recycle_bin,
        confirm_delete: c.confirm_before_delete,
        save_log: c.save_delete_log,
        update_list: c.update_list_after_delete,
        delete_readonly: c.delete_read_only,
        keep_rule: keep_rule_index(c.keep_file_rule),
        save_folder_state: c.save_folder_check_state,
        use_cache: c.use_hash_cache,
        log_level: log_level_index(c.log_level),
        follow_symlinks: c.follow_symlinks,
        theme: i32::from(c.theme.index()),
    }
}

/// 設定ページ（§3.8）で編集可能な 22 項目のみを既定値へ戻す。
///
/// `SettingsView` が公開する設定だけを `AppConfig::default()` から上書きし、画面外の状態
/// ——登録フォルダ（`target_folders`）・破損/拡張子レシピ（`corrupt_scan_groups` /
/// `wrong_extension_groups`）・上級チューニング（部分照合オフセット/サイズ・バッファ上下限・
/// 大容量しきい値・追加 temp パターン）——は保持する。これによりユーザーがリセットしても
/// 走査対象や独自レシピを失わない。
pub(crate) fn reset_settings_to_default(c: &mut AppConfig) {
    let d = AppConfig::default();
    // 比較方式・類似度
    c.compare_method = d.compare_method;
    c.image_similarity_threshold = d.image_similarity_threshold;
    c.office_similarity_level = d.office_similarity_level;
    // 除外条件
    c.exclude_system_folders = d.exclude_system_folders;
    c.exclude_zero_byte_files = d.exclude_zero_byte_files;
    c.exclude_onedrive_on_demand = d.exclude_onedrive_on_demand;
    c.file_size_filter_min = d.file_size_filter_min;
    c.file_size_filter_max = d.file_size_filter_max;
    c.file_size_filter_unit = d.file_size_filter_unit;
    c.include_extensions = d.include_extensions;
    c.exclude_patterns = d.exclude_patterns;
    // 削除動作
    c.use_recycle_bin = d.use_recycle_bin;
    c.confirm_before_delete = d.confirm_before_delete;
    c.save_delete_log = d.save_delete_log;
    c.update_list_after_delete = d.update_list_after_delete;
    c.delete_read_only = d.delete_read_only;
    c.keep_file_rule = d.keep_file_rule;
    // 動作・表示
    c.save_folder_check_state = d.save_folder_check_state;
    c.use_hash_cache = d.use_hash_cache;
    c.log_level = d.log_level;
    c.follow_symlinks = d.follow_symlinks;
    c.theme = d.theme;
}

/// 真偽値設定をキー名で適用する。未知キーは `false`。
pub(crate) fn apply_bool_setting(c: &mut AppConfig, key: &str, val: bool) -> bool {
    match key {
        "office_include_style" => {
            c.office_similarity_level = if val {
                OfficeSimilarityLevel::IncludeStyle
            } else {
                OfficeSimilarityLevel::ContentOnly
            }
        }
        "exclude_system" => c.exclude_system_folders = val,
        "exclude_zero" => c.exclude_zero_byte_files = val,
        "exclude_onedrive" => c.exclude_onedrive_on_demand = val,
        "use_recycle" => c.use_recycle_bin = val,
        "confirm_delete" => c.confirm_before_delete = val,
        "save_log" => c.save_delete_log = val,
        "update_list" => c.update_list_after_delete = val,
        "delete_readonly" => c.delete_read_only = val,
        "save_folder_state" => c.save_folder_check_state = val,
        "use_cache" => c.use_hash_cache = val,
        "follow_symlinks" => c.follow_symlinks = val,
        _ => return false,
    }
    true
}

/// 選択肢・数値設定をキー名で適用する。未知キー・範囲外 index は `false`。
pub(crate) fn apply_int_setting(c: &mut AppConfig, key: &str, val: i32) -> bool {
    match key {
        "compare_method" => {
            let Some(m) = u8::try_from(val).ok().and_then(CompareMethod::from_index) else {
                return false;
            };
            c.compare_method = m;
        }
        "image_threshold" => {
            // clamp(0, 100) で常に u8 範囲内のため try_from は失敗しない（到達不能の防御）。
            c.image_similarity_threshold =
                u8::try_from(val.clamp(0, 100)).expect("clamp(0,100) は u8 範囲内");
        }
        "size_unit" => {
            let Some(u) = file_size_unit_from_index(val) else {
                return false;
            };
            c.file_size_filter_unit = u;
        }
        "keep_rule" => {
            let Some(r) = keep_rule_from_index(val) else {
                return false;
            };
            c.keep_file_rule = r;
        }
        "log_level" => {
            let Some(l) = log_level_from_index(val) else {
                return false;
            };
            c.log_level = l;
        }
        "theme" => {
            let Some(t) = u8::try_from(val).ok().and_then(AppTheme::from_index) else {
                return false;
            };
            c.theme = t;
        }
        _ => return false,
    }
    true
}

/// テキスト設定をキー名で適用する。サイズは u64 としてパースし、不正値は無視（`false`）。
pub(crate) fn apply_text_setting(c: &mut AppConfig, key: &str, val: &str) -> bool {
    match key {
        "size_min" => match parse_size(val) {
            Some(v) => c.file_size_filter_min = v,
            None => return false,
        },
        "size_max" => match parse_size(val) {
            Some(v) => c.file_size_filter_max = v,
            None => return false,
        },
        "include_ext" => c.include_extensions = val.to_string(),
        "exclude_pat" => c.exclude_patterns = val.to_string(),
        _ => return false,
    }
    true
}

/// サイズ文字列を u64 へ。空欄は 0（無制限）。非数値は `None`。
fn parse_size(s: &str) -> Option<u64> {
    let t = s.trim();
    if t.is_empty() {
        Some(0)
    } else {
        t.parse::<u64>().ok()
    }
}

fn file_size_unit_index(u: FileSizeUnit) -> i32 {
    match u {
        FileSizeUnit::Byte => 0,
        FileSizeUnit::Kb => 1,
        FileSizeUnit::Mb => 2,
    }
}

fn file_size_unit_from_index(i: i32) -> Option<FileSizeUnit> {
    match i {
        0 => Some(FileSizeUnit::Byte),
        1 => Some(FileSizeUnit::Kb),
        2 => Some(FileSizeUnit::Mb),
        _ => None,
    }
}

fn keep_rule_index(r: KeepFileRule) -> i32 {
    match r {
        KeepFileRule::ByDateNewest => 0,
        KeepFileRule::ByDateOldest => 1,
        KeepFileRule::ByPathAsc => 2,
        KeepFileRule::ByPathDesc => 3,
        KeepFileRule::ByPathLengthShortest => 4,
        KeepFileRule::ByPathLengthLongest => 5,
    }
}

fn keep_rule_from_index(i: i32) -> Option<KeepFileRule> {
    match i {
        0 => Some(KeepFileRule::ByDateNewest),
        1 => Some(KeepFileRule::ByDateOldest),
        2 => Some(KeepFileRule::ByPathAsc),
        3 => Some(KeepFileRule::ByPathDesc),
        4 => Some(KeepFileRule::ByPathLengthShortest),
        5 => Some(KeepFileRule::ByPathLengthLongest),
        _ => None,
    }
}

fn log_level_index(l: LogLevelConfig) -> i32 {
    match l {
        LogLevelConfig::Trace => 0,
        LogLevelConfig::Debug => 1,
        LogLevelConfig::Information => 2,
        LogLevelConfig::Warning => 3,
        LogLevelConfig::Error => 4,
        LogLevelConfig::Critical => 5,
        LogLevelConfig::None => 6,
    }
}

fn log_level_from_index(i: i32) -> Option<LogLevelConfig> {
    match i {
        0 => Some(LogLevelConfig::Trace),
        1 => Some(LogLevelConfig::Debug),
        2 => Some(LogLevelConfig::Information),
        3 => Some(LogLevelConfig::Warning),
        4 => Some(LogLevelConfig::Error),
        5 => Some(LogLevelConfig::Critical),
        6 => Some(LogLevelConfig::None),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_setting_applies_known_keys() {
        let mut c = AppConfig::default();
        assert!(apply_bool_setting(&mut c, "delete_readonly", true));
        assert!(c.delete_read_only);
        assert!(apply_bool_setting(&mut c, "use_recycle", false));
        assert!(!c.use_recycle_bin);
        assert!(apply_bool_setting(&mut c, "office_include_style", true));
        assert_eq!(
            c.office_similarity_level,
            OfficeSimilarityLevel::IncludeStyle
        );
        assert!(apply_bool_setting(&mut c, "exclude_onedrive", true));
        assert!(c.exclude_onedrive_on_demand);
    }

    #[test]
    fn bool_setting_rejects_unknown_key() {
        let mut c = AppConfig::default();
        assert!(!apply_bool_setting(&mut c, "nope", true));
    }

    #[test]
    fn int_setting_applies_indices_and_clamps_threshold() {
        let mut c = AppConfig::default();
        assert!(apply_int_setting(&mut c, "compare_method", 8));
        assert_eq!(c.compare_method, CompareMethod::ImageSimilarity);
        assert!(apply_int_setting(&mut c, "keep_rule", 5));
        assert_eq!(c.keep_file_rule, KeepFileRule::ByPathLengthLongest);
        assert!(apply_int_setting(&mut c, "theme", 2));
        assert_eq!(c.theme, AppTheme::Dark);
        // しきい値は 0..=100 にクランプ。
        assert!(apply_int_setting(&mut c, "image_threshold", 250));
        assert_eq!(c.image_similarity_threshold, 100);
    }

    #[test]
    fn int_setting_rejects_out_of_range_index() {
        let mut c = AppConfig::default();
        assert!(!apply_int_setting(&mut c, "compare_method", 9));
        assert!(!apply_int_setting(&mut c, "keep_rule", -1));
        assert!(!apply_int_setting(&mut c, "theme", 3));
    }

    #[test]
    fn text_setting_parses_size_and_keeps_strings() {
        let mut c = AppConfig::default();
        assert!(apply_text_setting(&mut c, "size_min", "1024"));
        assert_eq!(c.file_size_filter_min, 1024);
        assert!(apply_text_setting(&mut c, "size_max", "  "));
        assert_eq!(c.file_size_filter_max, 0); // 空欄=無制限
        assert!(!apply_text_setting(&mut c, "size_min", "abc")); // 非数値は不採用
        assert!(apply_text_setting(&mut c, "include_ext", "jpg;png"));
        assert_eq!(c.include_extensions, "jpg;png");
    }

    #[test]
    fn settings_view_round_trips_indices() {
        let c = AppConfig {
            compare_method: CompareMethod::OfficeSimilarity,
            keep_file_rule: KeepFileRule::ByPathAsc,
            log_level: LogLevelConfig::Warning,
            theme: AppTheme::Light,
            ..AppConfig::default()
        };
        let v = settings_view(&c);
        assert_eq!(v.compare_method, 7);
        assert_eq!(v.keep_rule, 2);
        assert_eq!(v.log_level, 3);
        assert_eq!(v.theme, 1);
    }

    #[test]
    fn reset_restores_settings_and_preserves_other_state() {
        use shredy_core::config::{CorruptScanGroups, TargetFolder, WrongExtensionGroups};

        // 画面外の状態（フォルダ・レシピ・上級チューニング）を非既定値で確定する。
        // リセット後もこの値が保持される＝期待値の土台となる。
        let preserved = AppConfig {
            target_folders: vec![TargetFolder {
                path: "C:/keep".into(),
                checked: true,
            }],
            partial_match_offset_bytes: 16,
            partial_match_size_bytes: 4096,
            max_buf_size: 1_000_000,
            min_buf_size: 1024,
            large_file_threshold_bytes: 123,
            additional_temp_patterns: vec!["~$keep.docx".to_string()],
            corrupt_scan_groups: CorruptScanGroups::empty(), // 既定は all()
            wrong_extension_groups: WrongExtensionGroups::ARCHIVE, // 既定に ARCHIVE は無い
            ..AppConfig::default()
        };

        // 同じ保持状態のまま、設定ページの 22 項目だけを非既定値へ書き換える。
        let mut c = AppConfig {
            compare_method: CompareMethod::OfficeSimilarity,
            image_similarity_threshold: 50,
            office_similarity_level: OfficeSimilarityLevel::IncludeStyle,
            exclude_system_folders: false,
            exclude_zero_byte_files: true,
            exclude_onedrive_on_demand: true,
            file_size_filter_min: 10,
            file_size_filter_max: 999,
            file_size_filter_unit: FileSizeUnit::Mb,
            include_extensions: "jpg;png".to_string(),
            exclude_patterns: "*.tmp".to_string(),
            use_recycle_bin: false,
            confirm_before_delete: false,
            save_delete_log: false,
            update_list_after_delete: false,
            delete_read_only: true,
            keep_file_rule: KeepFileRule::ByPathAsc,
            save_folder_check_state: false,
            use_hash_cache: false,
            log_level: LogLevelConfig::Error,
            follow_symlinks: true,
            theme: AppTheme::Dark,
            ..preserved.clone()
        };

        reset_settings_to_default(&mut c);

        // 22 項目は既定へ戻り、保持フィールドは非既定値のまま＝preserved と完全一致。
        assert_eq!(c, preserved);
    }
}
