//! 設定のロード・対象フォルダ管理・表示用サマリ（設計 §5）。

use std::path::{Path, PathBuf};

use shredy_core::config::{CONFIG_FILE_NAME, TargetFolder};
use shredy_core::{AppConfig, LongPath};

/// 設定をロードする（実行フォルダの `shredy.config`、無ければ既定）。
///
/// 設定・パス・dir に加え、既定値へフォールバックした場合の警告メッセージ（`Some`）を返す。
/// 本関数はロギング初期化前に呼ばれるため、警告は呼び出し側が `init_logging` 後に
/// `tracing::warn!` で `shredy.log` へ記録する（規約: 診断は shredy.log へ）。
pub(crate) fn load_config() -> (AppConfig, PathBuf, PathBuf, Option<String>) {
    let dir = current_config_dir().unwrap_or_else(|| PathBuf::from("."));
    let path = dir.join(CONFIG_FILE_NAME);
    if !path.exists() {
        return (AppConfig::default(), path, dir, None);
    }
    let config = match AppConfig::load(&path) {
        Ok(c) => c,
        Err(e) => {
            let warn = format!("shredy.config の読み込みに失敗、既定値を使用します: {e}");
            return (AppConfig::default(), path, dir, Some(warn));
        }
    };
    if let Err(e) = config.validate() {
        let warn = format!("shredy.config が不正、既定値を使用します: {e}");
        return (AppConfig::default(), path, dir, Some(warn));
    }
    (config, path, dir, None)
}

fn current_config_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// 設定の `TargetFolders`（チェック済み）を対象ルートにする。
pub(crate) fn resolve_roots(config: &AppConfig) -> Vec<LongPath> {
    config
        .target_folders
        .iter()
        .filter(|tf| tf.checked)
        .filter_map(|tf| LongPath::new(&tf.path).ok())
        .collect()
}

/// フォルダ追加の結果。
pub(crate) enum AddFolderOutcome {
    Added,
    Duplicate,
    InvalidPath,
}

/// 対象フォルダを追加する（大小無視で重複排除）。
pub(crate) fn add_target_folder(config: &mut AppConfig, picked: &Path) -> AddFolderOutcome {
    let path = picked.to_string_lossy().into_owned();
    let Ok(new_lp) = LongPath::new(path.as_str()) else {
        return AddFolderOutcome::InvalidPath;
    };
    let dup = config.target_folders.iter().any(|tf| {
        LongPath::new(tf.path.as_str())
            .map(|lp| lp.eq_ignore_case(&new_lp))
            .unwrap_or(false)
    });
    if dup {
        return AddFolderOutcome::Duplicate;
    }
    config.target_folders.push(TargetFolder {
        path,
        checked: true,
    });
    AddFolderOutcome::Added
}

/// 複数フォルダ一括追加の結果集計（複数選択ダイアログ用, §5）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BatchAddSummary {
    /// 新規に追加した件数。
    pub added: usize,
    /// 大小無視で既に登録済みだった件数。
    pub duplicate: usize,
    /// パスとして不正で追加できなかった件数。
    pub invalid: usize,
}

/// 複数の対象フォルダを一括追加し、結果を集計する（各件は [`add_target_folder`] に委譲）。
pub(crate) fn add_target_folders(config: &mut AppConfig, picked: &[PathBuf]) -> BatchAddSummary {
    let mut summary = BatchAddSummary::default();
    for path in picked {
        match add_target_folder(config, path) {
            AddFolderOutcome::Added => summary.added += 1,
            AddFolderOutcome::Duplicate => summary.duplicate += 1,
            AddFolderOutcome::InvalidPath => summary.invalid += 1,
        }
    }
    summary
}

/// 一括追加結果を表示用ステータス文言にする（例: `2 件追加 / 1 件は登録済み`）。
pub(crate) fn batch_add_status(summary: BatchAddSummary) -> String {
    let mut parts = Vec::new();
    if summary.added > 0 {
        parts.push(format!("{} 件追加", summary.added));
    }
    if summary.duplicate > 0 {
        parts.push(format!("{} 件は登録済み", summary.duplicate));
    }
    if summary.invalid > 0 {
        parts.push(format!("{} 件は無効なパス", summary.invalid));
    }
    if parts.is_empty() {
        return "フォルダが選択されませんでした".to_string();
    }
    parts.join(" / ")
}

/// 起動時のステータス文言（対象フォルダ未設定なら明示）。
pub(crate) fn initial_status(config: &AppConfig) -> &'static str {
    if resolve_roots(config).is_empty() {
        "待機中（対象フォルダ未設定）"
    } else {
        "待機中"
    }
}

/// 設定の要約（ごみ箱削除・削除前確認の有効状態）を表示用文字列にする。
pub(crate) fn config_summary(config: &AppConfig) -> String {
    let recycle = if config.use_recycle_bin {
        "ごみ箱削除: 有効"
    } else {
        "ごみ箱削除: 無効"
    };
    let confirm = if config.confirm_before_delete {
        "削除前確認: 有効"
    } else {
        "削除前確認: 無効"
    };
    format!("{recycle} / {confirm}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_add_counts_added_and_duplicates() {
        let mut config = AppConfig::default();
        let paths = [
            PathBuf::from("C:\\a"),
            PathBuf::from("C:\\b"),
            PathBuf::from("C:\\a"), // 同一パス（重複）
        ];
        let s = add_target_folders(&mut config, &paths);
        assert_eq!(s.added, 2);
        assert_eq!(s.duplicate, 1);
        assert_eq!(s.invalid, 0);
        assert_eq!(config.target_folders.len(), 2);
    }

    #[test]
    fn batch_add_treats_existing_case_insensitively_as_duplicate() {
        let mut config = AppConfig::default();
        add_target_folder(&mut config, Path::new("C:\\keep"));
        // 大小違いでも登録済み扱い、新規のみ追加。
        let s = add_target_folders(
            &mut config,
            &[PathBuf::from("C:\\KEEP"), PathBuf::from("C:\\new")],
        );
        assert_eq!(s.added, 1);
        assert_eq!(s.duplicate, 1);
        assert_eq!(config.target_folders.len(), 2);
    }

    #[test]
    fn batch_add_status_formats_summary() {
        assert_eq!(
            batch_add_status(BatchAddSummary {
                added: 3,
                duplicate: 0,
                invalid: 0
            }),
            "3 件追加"
        );
        assert_eq!(
            batch_add_status(BatchAddSummary {
                added: 2,
                duplicate: 1,
                invalid: 0
            }),
            "2 件追加 / 1 件は登録済み"
        );
        assert_eq!(
            batch_add_status(BatchAddSummary {
                added: 0,
                duplicate: 3,
                invalid: 0
            }),
            "3 件は登録済み"
        );
        assert_eq!(
            batch_add_status(BatchAddSummary::default()),
            "フォルダが選択されませんでした"
        );
    }
}
