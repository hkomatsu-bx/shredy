//! 設定（`shredy.config`）の読み書きと検索条件の構築（仕様 §5, ガイド §13.7）。
//!
//! JSON キー・enum 値は仕様 §5 の PascalCase を `serde(rename_all = "PascalCase")` で維持する。
//! 新規キーは `#[serde(default)]` で後方互換を保ち、不正値はデフォルトへフォールバックする
//! （SECURITY-13: 安全なデシリアライズ）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::KeepFileRule;

mod flags;
pub use flags::{CorruptScanGroups, WrongExtensionGroups};

/// 設定ファイルの既定ファイル名。
pub const CONFIG_FILE_NAME: &str = "shredy.config";

/// 比較方式（設定表現, 仕様 §5）。CLI の `-S` 値順に対応する 9 値。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CompareMethod {
    #[default]
    HashBlake3,
    HashSha256,
    HashXxh128,
    PartialMatch,
    NameSizeDate,
    NameSize,
    NameOnly,
    OfficeSimilarity,
    ImageSimilarity,
}

impl CompareMethod {
    /// 全 9 方式（宣言順）。CLI の `-S` 値・GUI プルダウン index（0〜8）に対応する。
    pub const ALL: [CompareMethod; 9] = [
        CompareMethod::HashBlake3,
        CompareMethod::HashSha256,
        CompareMethod::HashXxh128,
        CompareMethod::PartialMatch,
        CompareMethod::NameSizeDate,
        CompareMethod::NameSize,
        CompareMethod::NameOnly,
        CompareMethod::OfficeSimilarity,
        CompareMethod::ImageSimilarity,
    ];

    /// 0 始まりの index（CLI の `-S` 値・GUI プルダウンと対応）。
    #[must_use]
    pub fn index(self) -> u8 {
        match self {
            CompareMethod::HashBlake3 => 0,
            CompareMethod::HashSha256 => 1,
            CompareMethod::HashXxh128 => 2,
            CompareMethod::PartialMatch => 3,
            CompareMethod::NameSizeDate => 4,
            CompareMethod::NameSize => 5,
            CompareMethod::NameOnly => 6,
            CompareMethod::OfficeSimilarity => 7,
            CompareMethod::ImageSimilarity => 8,
        }
    }

    /// 0 始まりの index から方式を得る（範囲外は `None`）。`-S` 値・プルダウン index の
    /// 解決を CLI/GUI で重複させず、この 1 箇所へ集約する（方式追加時の同期漏れ防止）。
    #[must_use]
    pub fn from_index(i: u8) -> Option<Self> {
        Self::ALL.get(i as usize).copied()
    }
}

/// Office 類似度の比較精度（仕様 §3.8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OfficeSimilarityLevel {
    #[default]
    ContentOnly,
    IncludeStyle,
}

/// 外観テーマ（仕様 §3.8）。`System` は OS の配色設定に追従する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AppTheme {
    #[default]
    System,
    Light,
    Dark,
}

impl AppTheme {
    /// 全 3 値（宣言順）。GUI のテーマ選択 index（0〜2）に対応する。
    pub const ALL: [AppTheme; 3] = [AppTheme::System, AppTheme::Light, AppTheme::Dark];

    /// 0 始まりの index（GUI プルダウンと対応）。
    #[must_use]
    pub fn index(self) -> u8 {
        match self {
            AppTheme::System => 0,
            AppTheme::Light => 1,
            AppTheme::Dark => 2,
        }
    }

    /// 0 始まりの index からテーマを得る（範囲外は `None`）。
    #[must_use]
    pub fn from_index(i: u8) -> Option<Self> {
        Self::ALL.get(i as usize).copied()
    }
}

/// ファイルサイズフィルタの単位（仕様 §5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FileSizeUnit {
    #[default]
    Byte,
    #[serde(rename = "KB")]
    Kb,
    #[serde(rename = "MB")]
    Mb,
}

impl FileSizeUnit {
    /// 単位の倍率（バイト）。
    #[must_use]
    pub fn multiplier(self) -> u64 {
        match self {
            FileSizeUnit::Byte => 1,
            FileSizeUnit::Kb => 1024,
            FileSizeUnit::Mb => 1024 * 1024,
        }
    }
}

/// ログ出力レベル（仕様 §5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LogLevelConfig {
    Trace,
    Debug,
    #[default]
    Information,
    Warning,
    Error,
    Critical,
    None,
}

impl LogLevelConfig {
    /// `tracing` のレベルフィルタへ変換する。
    #[must_use]
    pub fn to_filter(self) -> tracing::level_filters::LevelFilter {
        use tracing::level_filters::LevelFilter;
        match self {
            LogLevelConfig::Trace => LevelFilter::TRACE,
            LogLevelConfig::Debug => LevelFilter::DEBUG,
            LogLevelConfig::Information => LevelFilter::INFO,
            LogLevelConfig::Warning => LevelFilter::WARN,
            LogLevelConfig::Error | LogLevelConfig::Critical => LevelFilter::ERROR,
            LogLevelConfig::None => LevelFilter::OFF,
        }
    }
}

/// 登録フォルダと選択状態（仕様 §5 `TargetFolders`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TargetFolder {
    pub path: String,
    pub checked: bool,
}

/// アプリケーション設定。仕様 §5 のキーを網羅する。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AppConfig {
    pub target_folders: Vec<TargetFolder>,
    pub compare_method: CompareMethod,
    pub image_similarity_threshold: u8,
    pub office_similarity_level: OfficeSimilarityLevel,
    pub exclude_patterns: String,
    pub include_extensions: String,
    pub use_recycle_bin: bool,
    pub confirm_before_delete: bool,
    pub save_delete_log: bool,
    pub update_list_after_delete: bool,
    pub keep_file_rule: KeepFileRule,
    pub save_folder_check_state: bool,
    pub exclude_system_folders: bool,
    pub exclude_zero_byte_files: bool,
    pub file_size_filter_min: u64,
    pub file_size_filter_max: u64,
    pub file_size_filter_unit: FileSizeUnit,
    pub partial_match_offset_bytes: u64,
    pub partial_match_size_bytes: u64,
    pub delete_read_only: bool,
    pub max_buf_size: u64,
    pub min_buf_size: u64,
    pub use_hash_cache: bool,
    pub log_level: LogLevelConfig,
    pub large_file_threshold_bytes: u64,
    pub additional_temp_patterns: Vec<String>,
    pub follow_symlinks: bool,
    /// OneDrive オンデマンド（クラウドのみ・未ダウンロード）ファイルを走査から除外する
    /// （仕様 §7.1）。既定 OFF。実体が無くハッシュ計算で不要なダウンロードを誘発するため。
    #[serde(rename = "ExcludeOneDriveOnDemand")]
    pub exclude_onedrive_on_demand: bool,
    pub corrupt_scan_groups: CorruptScanGroups,
    pub wrong_extension_groups: WrongExtensionGroups,
    pub theme: AppTheme,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            target_folders: Vec::new(),
            compare_method: CompareMethod::default(),
            image_similarity_threshold: 90,
            office_similarity_level: OfficeSimilarityLevel::default(),
            exclude_patterns: String::new(),
            include_extensions: String::new(),
            use_recycle_bin: true,
            confirm_before_delete: true,
            save_delete_log: true,
            update_list_after_delete: true,
            keep_file_rule: KeepFileRule::ByDateNewest,
            save_folder_check_state: true,
            exclude_system_folders: true,
            exclude_zero_byte_files: false,
            file_size_filter_min: 0,
            file_size_filter_max: 0,
            file_size_filter_unit: FileSizeUnit::default(),
            partial_match_offset_bytes: 0,
            partial_match_size_bytes: 0,
            delete_read_only: false,
            max_buf_size: 0,
            min_buf_size: 0,
            use_hash_cache: true,
            log_level: LogLevelConfig::default(),
            large_file_threshold_bytes: 524_288_000,
            additional_temp_patterns: Vec::new(),
            // 既定はリパースポイント（シンボリックリンク/ジャンクション）を辿らない。
            // 辿るとジャンクション配下の実ファイルが走査・削除対象になり実体を巻き込む
            // ため（仕様 §安全優先「リパースポイントはスキップ」）。
            follow_symlinks: false,
            exclude_onedrive_on_demand: false,
            corrupt_scan_groups: CorruptScanGroups::default(),
            wrong_extension_groups: WrongExtensionGroups::default(),
            theme: AppTheme::default(),
        }
    }
}

impl AppConfig {
    /// 設定ファイルを読み込む。不正値・欠損キーはデフォルトへフォールバックする。
    pub fn load(path: &Path) -> Result<AppConfig> {
        let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
        Self::from_json(&text)
    }

    /// JSON 文字列から設定を構築する。
    pub fn from_json(text: &str) -> Result<AppConfig> {
        serde_json::from_str(text).map_err(|e| CoreError::Config(format!("invalid config: {e}")))
    }

    /// 設定を JSON 文字列へ直列化する。
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| CoreError::Config(format!("serialize failed: {e}")))
    }

    /// 設定ファイルへ保存する。
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = self.to_json()?;
        std::fs::write(path, text).map_err(|e| CoreError::io(path, e))
    }

    /// 設定ファイルの既定パスを返す（`--config` 上書き優先、無ければ実行フォルダ）。
    #[must_use]
    pub fn resolve_path(exe_dir: &Path, cli_override: Option<&Path>) -> PathBuf {
        match cli_override {
            Some(p) if p.is_dir() => p.join(CONFIG_FILE_NAME),
            Some(p) => p.to_path_buf(),
            None => exe_dir.join(CONFIG_FILE_NAME),
        }
    }

    /// 値の妥当性を検証する（SECURITY-05: 入力検証）。
    pub fn validate(&self) -> Result<()> {
        if self.image_similarity_threshold > 100 {
            return Err(CoreError::Config(format!(
                "ImageSimilarityThreshold must be 0..=100, got {}",
                self.image_similarity_threshold
            )));
        }
        if self.file_size_filter_max != 0 && self.file_size_filter_max < self.file_size_filter_min {
            return Err(CoreError::Config(
                "FileSizeFilterMax must be >= FileSizeFilterMin (or 0)".to_string(),
            ));
        }
        Ok(())
    }
}

/// 検索条件（走査フィルタ）。`ScanConfigBuilder` で構築する（Item 7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanConfig {
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub include_extensions: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub exclude_system_folders: bool,
    pub exclude_zero_byte: bool,
    pub follow_symlinks: bool,
    /// OneDrive オンデマンドファイルを走査から除外する（仕様 §7.1）。
    pub exclude_onedrive_on_demand: bool,
    pub io_parallelism: Option<usize>,
    /// 部分一致 (S3) の開始位置（バイト）。
    pub partial_match_offset: u64,
    /// 部分一致 (S3) のチェックサイズ（バイト）。0 は候補集合の最小サイズ。
    pub partial_match_size: u64,
    /// Office 類似度 (S7) の比較精度（仕様 §3.11）。
    pub office_similarity_level: OfficeSimilarityLevel,
    /// 画像類似度 (S8) のしきい値（0〜100%, 仕様 §3.12）。
    pub image_similarity_threshold: u8,
}

/// `ScanConfig` のビルダー。
#[derive(Debug, Clone, Default)]
pub struct ScanConfigBuilder {
    min_size: Option<u64>,
    max_size: Option<u64>,
    include_extensions: Vec<String>,
    exclude_patterns: Vec<String>,
    exclude_system_folders: bool,
    exclude_zero_byte: bool,
    follow_symlinks: bool,
    exclude_onedrive_on_demand: bool,
    io_parallelism: Option<usize>,
    partial_match_offset: u64,
    partial_match_size: u64,
    office_similarity_level: OfficeSimilarityLevel,
    image_similarity_threshold: u8,
}

impl ScanConfigBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            // リパースポイントは既定で辿らない（安全優先, AppConfig と一致）。
            follow_symlinks: false,
            image_similarity_threshold: 90,
            ..Self::default()
        }
    }

    /// `AppConfig` から検索条件を組み立てる。
    #[must_use]
    pub fn from_app_config(cfg: &AppConfig) -> Self {
        let mult = cfg.file_size_filter_unit.multiplier();
        // 単位倍率の乗算は飽和させる（release の wrapping で誤ったサイズ閾値になるのを防ぐ）。
        let to_opt = |v: u64| {
            if v == 0 {
                None
            } else {
                Some(v.saturating_mul(mult))
            }
        };
        ScanConfigBuilder {
            min_size: to_opt(cfg.file_size_filter_min),
            max_size: to_opt(cfg.file_size_filter_max),
            include_extensions: split_list(&cfg.include_extensions),
            exclude_patterns: split_list(&cfg.exclude_patterns),
            exclude_system_folders: cfg.exclude_system_folders,
            exclude_zero_byte: cfg.exclude_zero_byte_files,
            follow_symlinks: cfg.follow_symlinks,
            exclude_onedrive_on_demand: cfg.exclude_onedrive_on_demand,
            io_parallelism: None,
            partial_match_offset: cfg.partial_match_offset_bytes,
            partial_match_size: cfg.partial_match_size_bytes,
            office_similarity_level: cfg.office_similarity_level,
            image_similarity_threshold: cfg.image_similarity_threshold,
        }
    }

    #[must_use]
    pub fn io_parallelism(mut self, n: Option<usize>) -> Self {
        self.io_parallelism = n;
        self
    }

    #[must_use]
    pub fn build(self) -> ScanConfig {
        ScanConfig {
            min_size: self.min_size,
            max_size: self.max_size,
            include_extensions: self.include_extensions,
            exclude_patterns: self.exclude_patterns,
            exclude_system_folders: self.exclude_system_folders,
            exclude_zero_byte: self.exclude_zero_byte,
            follow_symlinks: self.follow_symlinks,
            exclude_onedrive_on_demand: self.exclude_onedrive_on_demand,
            io_parallelism: self.io_parallelism,
            partial_match_offset: self.partial_match_offset,
            partial_match_size: self.partial_match_size,
            office_similarity_level: self.office_similarity_level,
            image_similarity_threshold: self.image_similarity_threshold,
        }
    }
}

/// `;` / `,` 区切りの文字列をトリム済みトークン列へ分解する（空要素は除外）。
fn split_list(s: &str) -> Vec<String> {
    s.split([';', ','])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_spec_values() {
        let c = AppConfig::default();
        assert_eq!(c.compare_method, CompareMethod::HashBlake3);
        assert_eq!(c.image_similarity_threshold, 90);
        assert!(c.use_recycle_bin);
        assert!(c.confirm_before_delete);
        assert_eq!(c.keep_file_rule, KeepFileRule::ByDateNewest);
        assert_eq!(c.large_file_threshold_bytes, 524_288_000);
    }

    #[test]
    fn missing_keys_fall_back_to_default() {
        let c = AppConfig::from_json("{}").unwrap();
        assert_eq!(c, AppConfig::default());
    }

    #[test]
    fn default_does_not_follow_reparse_points() {
        // 安全優先: 既定でリパースポイントを辿らない（ジャンクション配下の実体巻き込み防止）。
        assert!(!AppConfig::default().follow_symlinks);
        assert!(!ScanConfigBuilder::new().build().follow_symlinks);
    }

    #[test]
    fn size_filter_multiplier_saturates_on_overflow() {
        // 巨大値 × 単位倍率の乗算が wrapping せず u64::MAX へ飽和すること。
        let c = AppConfig {
            file_size_filter_max: u64::MAX,
            file_size_filter_unit: FileSizeUnit::Mb,
            ..AppConfig::default()
        };
        let cfg = ScanConfigBuilder::from_app_config(&c).build();
        assert_eq!(cfg.max_size, Some(u64::MAX));
    }

    #[test]
    fn pascal_case_keys_are_used() {
        let json = AppConfig::default().to_json().unwrap();
        assert!(json.contains("\"CompareMethod\""));
        assert!(json.contains("\"HashBlake3\""));
        assert!(json.contains("\"UseRecycleBin\""));
    }

    #[test]
    fn compare_method_index_roundtrips_for_all_variants() {
        // index ↔ CompareMethod が全 9 方式で一意・対称（CLI -S 値・GUI プルダウンと整合）。
        for (i, m) in CompareMethod::ALL.iter().enumerate() {
            assert_eq!(m.index() as usize, i);
            assert_eq!(CompareMethod::from_index(i as u8), Some(*m));
        }
        assert_eq!(CompareMethod::from_index(9), None);
    }

    #[test]
    fn validate_rejects_bad_threshold() {
        let c = AppConfig {
            image_similarity_threshold: 150,
            ..AppConfig::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn onedrive_exclusion_defaults_off_and_propagates() {
        // 既定 OFF（仕様 §7.1）。
        assert!(!AppConfig::default().exclude_onedrive_on_demand);
        assert!(!ScanConfigBuilder::new().build().exclude_onedrive_on_demand);
        // AppConfig → ScanConfig へ伝播する。
        let c = AppConfig {
            exclude_onedrive_on_demand: true,
            ..AppConfig::default()
        };
        assert!(
            ScanConfigBuilder::from_app_config(&c)
                .build()
                .exclude_onedrive_on_demand
        );
    }

    #[test]
    fn onedrive_key_uses_camel_cased_onedrive() {
        // PascalCase 自動変換ではなく明示 rename で "OneDrive" の綴りを保つ。
        let json = AppConfig::default().to_json().unwrap();
        assert!(json.contains("\"ExcludeOneDriveOnDemand\""));
    }

    #[test]
    fn scan_config_builder_applies_unit_multiplier() {
        let c = AppConfig {
            file_size_filter_min: 2,
            file_size_filter_unit: FileSizeUnit::Mb,
            ..AppConfig::default()
        };
        let sc = ScanConfigBuilder::from_app_config(&c).build();
        assert_eq!(sc.min_size, Some(2 * 1024 * 1024));
        assert_eq!(sc.max_size, None);
    }

    #[test]
    fn split_list_trims_and_drops_empty() {
        assert_eq!(
            split_list("*.exe; *.dll,, .tmp"),
            vec!["*.exe", "*.dll", ".tmp"]
        );
        assert!(split_list("").is_empty());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // PBT-02: AppConfig の JSON round-trip は恒等。
        #[test]
        fn app_config_json_round_trip(
            threshold in 0u8..=100,
            min in 0u64..1_000_000,
            recycle in any::<bool>(),
        ) {
            let c = AppConfig {
                image_similarity_threshold: threshold,
                file_size_filter_min: min,
                use_recycle_bin: recycle,
                ..AppConfig::default()
            };
            let json = c.to_json().unwrap();
            let back = AppConfig::from_json(&json).unwrap();
            prop_assert_eq!(c, back);
        }
    }
}
