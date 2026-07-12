//! ロングパス・Unicode の集約点（ガイド §13.1 / §13.8）。
//!
//! `\\?\` プレフィックス付与・UTF-16 変換・NFC 正規化・Unicode 大小無視比較を
//! `LongPath` newtype に閉じ込め、アプリ全体で 1 箇所に統一する（Item 6）。

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

use crate::error::{CoreError, Result};

/// 対応パス長の上限（文字数, spec §ロングパス「最大 4096 文字」）。
pub const MAX_PATH_LEN: usize = 4096;

/// 正規化済みパスを保持する newtype。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongPath(PathBuf);

impl LongPath {
    /// パスを受け取り、`.`/`..` を字句的に解決して保持する。
    ///
    /// ファイルシステムへはアクセスしない（純粋な字句正規化）。上限 [`MAX_PATH_LEN`]
    /// 文字を超えるパスは [`CoreError::InvalidPath`] として弾く（spec §ロングパス・境界検証）。
    pub fn new(p: impl Into<PathBuf>) -> Result<Self> {
        let raw = p.into();
        let normalized = normalize_lexical(&raw);
        // 文字数で判定（多バイトの日本語パスを誤って弾かないよう byte 長ではなく文字数）。
        if normalized.to_string_lossy().chars().count() > MAX_PATH_LEN {
            return Err(CoreError::InvalidPath(raw));
        }
        Ok(LongPath(normalized))
    }

    /// 内部パスへの参照。
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// `\\?\`（UNC は `\\?\UNC\`）を付与した拡張長パス文字列。
    #[must_use]
    pub fn extended_string(&self) -> String {
        let s = self.0.to_string_lossy();
        if s.starts_with("\\\\?\\") {
            s.into_owned()
        } else if let Some(rest) = s.strip_prefix("\\\\") {
            // UNC: \\server\share\... -> \\?\UNC\server\share\...
            format!("\\\\?\\UNC\\{rest}")
        } else {
            format!("\\\\?\\{s}")
        }
    }

    /// Win32 API へ渡す UTF-16（NUL 終端）表現。
    #[must_use]
    pub fn to_extended_wide(&self) -> Vec<u16> {
        let s = self.extended_string();
        encode_wide_nul(&s)
    }

    /// `std::fs` / Win32 の I/O へ渡すパス。
    ///
    /// Windows では `\\?\` 拡張長パスにして 260 字超でも I/O が成立するようにする
    /// （spec §ロングパス）。非 Windows では内部パスをそのまま借用する。
    /// エラーメッセージには `\\?\` を含めたくないため、メッセージ用には [`Self::as_path`]
    /// を使い分ける。
    #[must_use]
    pub fn fs_path(&self) -> Cow<'_, Path> {
        #[cfg(windows)]
        {
            Cow::Owned(PathBuf::from(self.extended_string()))
        }
        #[cfg(not(windows))]
        {
            Cow::Borrowed(self.as_path())
        }
    }

    /// Unicode の大文字小文字を無視した等価比較（NFC 正規化 + case-fold）。
    #[must_use]
    pub fn eq_ignore_case(&self, other: &LongPath) -> bool {
        let a = to_nfc(&self.0.to_string_lossy());
        let b = to_nfc(&other.0.to_string_lossy());
        unicase::UniCase::new(a) == unicase::UniCase::new(b)
    }

    /// 大文字小文字を無視した照合キー文字列（`Selection`/`LockSet` の内部 HashMap キー）。
    ///
    /// NFC 正規化してから小文字化する。[`Self::eq_ignore_case`] と正規化（NFC）を揃え、
    /// NFD で表記されたパス（濁点分解など）も同一視する（CLAUDE.md「Unicode 完全対応」）。
    /// CLI/GUI で重複していた実装の単一ソース。
    #[must_use]
    pub fn case_key(&self) -> String {
        to_nfc(&self.0.to_string_lossy()).to_lowercase()
    }

    /// 拡張子を小文字で取り出す（例: `"docx"`）。拡張子なしは `None`。
    #[must_use]
    pub fn extension_lower(&self) -> Option<String> {
        self.0
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
    }
}

/// 文字列を NFC 正規化する。冪等（`to_nfc(to_nfc(x)) == to_nfc(x)`）。
#[must_use]
pub fn to_nfc(s: &str) -> String {
    s.nfc().collect()
}

/// パスの各コンポーネントを小文字化した照合キー列を返す（大小無視の祖先判定用）。
///
/// `scan`（登録フォルダの親子正規化）と `select`（origin ツリー配下判定）で共有する。
/// NFC 正規化は行わない（コンポーネント単位の前方一致比較が目的のため）。
pub(crate) fn comp_keys(p: &Path) -> Vec<String> {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

/// `Path` を `std::fs` I/O へ渡せる実パスへ変換する。
///
/// [`LongPath`] へ正規化できれば `\\?\` 拡張長パスを、できなければ入力をそのまま返す
/// ベストエフォート変換。EXIF/メタデータ/画像処理/破損検査など、外部 crate へ生の
/// `&Path` を渡す経路で重複していたイディオムの単一ソース。
#[must_use]
pub(crate) fn to_fs_path(path: &Path) -> PathBuf {
    LongPath::new(path)
        .map(|lp| lp.fs_path().into_owned())
        .unwrap_or_else(|_| path.to_path_buf())
}

/// `.`/`..`/`.` を字句的に畳み込む（FS 非アクセス）。
fn normalize_lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(windows)]
fn encode_wide_nul(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(not(windows))]
fn encode_wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_absolute_gets_extended_prefix() {
        let lp = LongPath::new("C:\\dir\\file.txt").unwrap();
        assert_eq!(lp.extended_string(), "\\\\?\\C:\\dir\\file.txt");
    }

    #[test]
    fn rejects_path_over_max_len() {
        // 上限超過（4096 文字）は InvalidPath で弾く（F-6・境界検証）。
        let long = format!("C:\\{}", "a".repeat(MAX_PATH_LEN));
        assert!(LongPath::new(&long).is_err());
        // 上限ちょうど付近は受理（境界の許容側）。
        let ok = format!("C:\\{}", "a".repeat(MAX_PATH_LEN - 8));
        assert!(LongPath::new(&ok).is_ok());
    }

    #[test]
    fn accepts_long_multibyte_path_within_char_limit() {
        // 多バイト（日本語）でも文字数で判定するため byte 長 > 4096 でも受理される。
        // あ ≈ 3 byte。2000 文字 ≈ 6000 byte だが 4096 文字以内なので OK。
        let p = format!("C:\\{}", "あ".repeat(2000));
        assert!(LongPath::new(&p).is_ok());
    }

    #[test]
    fn unc_path_gets_unc_extended_prefix() {
        let lp = LongPath::new("\\\\server\\share\\f.txt").unwrap();
        assert_eq!(lp.extended_string(), "\\\\?\\UNC\\server\\share\\f.txt");
    }

    #[test]
    fn already_extended_is_unchanged() {
        let lp = LongPath::new("\\\\?\\C:\\a").unwrap();
        assert_eq!(lp.extended_string(), "\\\\?\\C:\\a");
    }

    #[test]
    fn parent_components_are_collapsed() {
        let lp = LongPath::new("C:\\a\\b\\..\\c").unwrap();
        assert_eq!(lp.as_path(), Path::new("C:\\a\\c"));
    }

    #[test]
    fn handles_japanese_and_emoji_names() {
        let lp = LongPath::new("C:\\写真\\😀.jpg").unwrap();
        assert!(lp.extended_string().contains("写真"));
        // NUL 終端 + 内容が含まれること
        let wide = lp.to_extended_wide();
        assert_eq!(*wide.last().unwrap(), 0);
        assert!(wide.len() > 1);
    }

    #[test]
    fn case_insensitive_equality() {
        let a = LongPath::new("C:\\Dir\\File.TXT").unwrap();
        let b = LongPath::new("C:\\dir\\file.txt").unwrap();
        assert!(a.eq_ignore_case(&b));
    }

    #[test]
    fn case_key_normalizes_nfc_and_case() {
        // NFC（合成済み é = U+00E9）と NFD（e + 結合アクセント U+0301）は同一キーになる。
        let nfc = LongPath::new("C:\\Caf\u{00e9}.txt").unwrap();
        let nfd = LongPath::new("C:\\cafe\u{0301}.txt").unwrap();
        assert_eq!(nfc.case_key(), nfd.case_key());
        // 大小無視も従来どおり効く。
        assert_eq!(
            LongPath::new("C:\\A").unwrap().case_key(),
            LongPath::new("C:\\a").unwrap().case_key()
        );
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // PBT-04: NFC 正規化は冪等。
        #[test]
        fn nfc_is_idempotent(s in ".{0,40}") {
            let once = to_nfc(&s);
            let twice = to_nfc(&once);
            prop_assert_eq!(once, twice);
        }

        // PBT-03: case-fold 比較は反射的（自分自身と常に等しい）。
        #[test]
        fn eq_ignore_case_is_reflexive(name in "[a-zA-Z0-9_ぁ-んァ-ヶ]{1,20}") {
            let p = LongPath::new(format!("C:\\{name}")).unwrap();
            prop_assert!(p.eq_ignore_case(&p));
        }

        // PBT-03: case-fold 比較は対称的。
        #[test]
        fn eq_ignore_case_is_symmetric(
            a in "[a-zA-Z0-9_]{1,20}",
            b in "[a-zA-Z0-9_]{1,20}",
        ) {
            let pa = LongPath::new(format!("C:\\{a}")).unwrap();
            let pb = LongPath::new(format!("C:\\{b}")).unwrap();
            prop_assert_eq!(pa.eq_ignore_case(&pb), pb.eq_ignore_case(&pa));
        }

        // 不変条件: ローカル絶対パスは常に \\?\ で始まる。
        #[test]
        fn local_absolute_always_has_extended_prefix(name in "[a-zA-Z0-9_]{1,20}") {
            let p = LongPath::new(format!("C:\\{name}")).unwrap();
            prop_assert!(p.extended_string().starts_with("\\\\?\\"));
        }
    }
}
