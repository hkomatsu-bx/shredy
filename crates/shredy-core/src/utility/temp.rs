//! 一時ファイルの分類（仕様 §3.9.2）。
//!
//! ファイル/フォルダ名をカテゴリ別のワイルドカードパターン表と照合し、最初に一致した
//! カテゴリを返す純関数を提供する。判定はファイル列挙（[`super`] の走査）から切り出し、
//! 単体テスト可能にする。

use crate::scan::matches_wildcard;

/// 一時ファイルのカテゴリ（仕様 §3.9.2 の表に対応）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TempCategory {
    /// Windows 一時ファイル（`~$*` / `*.tmp` / `Thumbs.db` / `desktop.ini`）。
    WindowsTemp,
    /// Office 一時ファイル（`~$*.doc` / `~$*.xls` / `~$*.ppt`）。
    OfficeTemp,
    /// macOS メタファイル（`.DS_Store` / `__MACOSX`）。
    MacOsMeta,
    /// バックアップファイル（`*.bak` / `*.old`）。
    Backup,
    /// ビルド成果物（`*.obj` / `*.pdb` / `node_modules`）。
    BuildArtifact,
    /// 設定で追加されたユーザー定義パターン。
    Custom,
}

impl TempCategory {
    /// 表示用ラベル（GUI のカテゴリ列）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            TempCategory::WindowsTemp => "Windows 一時ファイル",
            TempCategory::OfficeTemp => "Office 一時ファイル",
            TempCategory::MacOsMeta => "macOS メタファイル",
            TempCategory::Backup => "バックアップファイル",
            TempCategory::BuildArtifact => "ビルド成果物",
            TempCategory::Custom => "追加パターン",
        }
    }
}

// より限定的なパターン（Office）を Windows の `~$*` より先に評価するため、照合順を
// 固定する。`node_modules` / `__MACOSX` はフォルダ名（仕様表の末尾 `\` はフォルダの意）。
const OFFICE_TEMP: &[&str] = &["~$*.doc", "~$*.xls", "~$*.ppt"];
const MACOS_META: &[&str] = &[".DS_Store", "__MACOSX"];
const BACKUP: &[&str] = &["*.bak", "*.old"];
const BUILD_ARTIFACT: &[&str] = &["*.obj", "*.pdb", "node_modules"];
const WINDOWS_TEMP: &[&str] = &["~$*", "*.tmp", "Thumbs.db", "desktop.ini"];

/// ファイル/フォルダ名を一時ファイルとして分類する（仕様 §3.9.2）。
///
/// 組み込みパターン表（カテゴリ順）→ 設定の追加パターン（`Custom`）の順に照合し、最初に
/// 一致したカテゴリを返す。どれにも一致しなければ `None`。照合は大小無視（[`matches_wildcard`]）。
#[must_use]
pub fn classify_temp(name: &str, additional: &[String]) -> Option<TempCategory> {
    let matches_any = |pats: &[&str]| pats.iter().any(|p| matches_wildcard(p, name));
    if matches_any(OFFICE_TEMP) {
        return Some(TempCategory::OfficeTemp);
    }
    if matches_any(MACOS_META) {
        return Some(TempCategory::MacOsMeta);
    }
    if matches_any(BACKUP) {
        return Some(TempCategory::Backup);
    }
    if matches_any(BUILD_ARTIFACT) {
        return Some(TempCategory::BuildArtifact);
    }
    if matches_any(WINDOWS_TEMP) {
        return Some(TempCategory::WindowsTemp);
    }
    if additional
        .iter()
        .any(|p| !p.is_empty() && matches_wildcard(p, name))
    {
        return Some(TempCategory::Custom);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_windows_temp() {
        assert_eq!(
            classify_temp("foo.tmp", &[]),
            Some(TempCategory::WindowsTemp)
        );
        assert_eq!(
            classify_temp("Thumbs.db", &[]),
            Some(TempCategory::WindowsTemp)
        );
        assert_eq!(
            classify_temp("desktop.ini", &[]),
            Some(TempCategory::WindowsTemp)
        );
        // 大小無視。
        assert_eq!(
            classify_temp("THUMBS.DB", &[]),
            Some(TempCategory::WindowsTemp)
        );
    }

    #[test]
    fn office_lock_more_specific_than_windows() {
        // `~$foo.doc` は Office パターンに先に一致する（`~$*` の Windows より優先）。
        assert_eq!(
            classify_temp("~$report.doc", &[]),
            Some(TempCategory::OfficeTemp)
        );
        // `.docx` は Office パターン非該当だが `~$*` で Windows に該当。
        assert_eq!(
            classify_temp("~$report.docx", &[]),
            Some(TempCategory::WindowsTemp)
        );
    }

    #[test]
    fn classifies_macos_backup_and_build() {
        assert_eq!(
            classify_temp(".DS_Store", &[]),
            Some(TempCategory::MacOsMeta)
        );
        assert_eq!(
            classify_temp("__MACOSX", &[]),
            Some(TempCategory::MacOsMeta)
        );
        assert_eq!(classify_temp("data.bak", &[]), Some(TempCategory::Backup));
        assert_eq!(classify_temp("old.OLD", &[]), Some(TempCategory::Backup));
        assert_eq!(
            classify_temp("module.obj", &[]),
            Some(TempCategory::BuildArtifact)
        );
        assert_eq!(
            classify_temp("node_modules", &[]),
            Some(TempCategory::BuildArtifact)
        );
    }

    #[test]
    fn additional_patterns_classify_as_custom() {
        let add = vec!["*.log".to_string(), "cache".to_string()];
        assert_eq!(classify_temp("app.log", &add), Some(TempCategory::Custom));
        assert_eq!(classify_temp("cache", &add), Some(TempCategory::Custom));
    }

    #[test]
    fn empty_additional_pattern_never_matches() {
        // 空文字パターンは全名に一致してしまうため除外する。
        assert_eq!(classify_temp("anything.txt", &["".to_string()]), None);
    }

    #[test]
    fn non_temp_returns_none() {
        assert_eq!(classify_temp("document.docx", &[]), None);
        assert_eq!(classify_temp("photo.jpg", &[]), None);
    }
}
