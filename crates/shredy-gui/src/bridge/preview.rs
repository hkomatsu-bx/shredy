//! ファイルプレビューの分類とテキスト読み込み（§3.5, U12 polish）。
//!
//! プレビュー対象は画像とテキスト。それ以外は「関連付けアプリで開く」で扱う。
//! パフォーマンス保護のためサイズ上限を設ける（画像 30MB 未満 / テキスト 1MB 未満かつ
//! 先頭 8192 文字まで）。

use shredy_core::LongPath;

/// プレビュー種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Image,
    Text,
    /// 非対応形式（関連付けアプリで開く）。
    Other,
}

impl PreviewKind {
    /// Slint へ渡す int 表現（0=Image / 1=Text / 2=Other）。
    #[must_use]
    pub fn as_i32(self) -> i32 {
        match self {
            PreviewKind::Image => 0,
            PreviewKind::Text => 1,
            PreviewKind::Other => 2,
        }
    }
}

/// 画像プレビュー上限（30MB 未満）。
pub const IMAGE_MAX_BYTES: u64 = 30 * 1024 * 1024;
/// テキストプレビュー上限（1MB 未満）。
pub const TEXT_MAX_BYTES: u64 = 1024 * 1024;
/// テキストプレビューの先頭読み込み文字数。
pub const TEXT_MAX_CHARS: usize = 8192;

// 画像プレビュー対応拡張子。`heic` feature 有効時のみ HEIC/HEIF を含む（core でデコードして
// RGBA8 から描画する。feature を外すと HEIC は `Other`＝「関連付けで開く」に分類される）。
// core `imgsim::SUPPORTED_EXTENSIONS` と同型の feature 分岐。
#[cfg(feature = "heic")]
const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "bmp", "gif", "webp", "tif", "tiff", "heic", "heif",
];
#[cfg(not(feature = "heic"))]
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "bmp", "gif", "webp", "tif", "tiff"];
const TEXT_EXTS: &[&str] = &[
    "txt", "json", "md", "markdown", "log", "csv", "tsv", "xml", "yaml", "yml", "toml", "ini",
    "cfg", "conf", "rs", "py", "js", "ts", "html", "htm", "css", "c", "h", "cpp", "hpp", "sh",
    "bat", "ps1",
];

/// 拡張子（小文字）を取り出す。
fn ext_lower(path: &LongPath) -> Option<String> {
    path.as_path()
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

/// 拡張子からプレビュー種別を判定する。
#[must_use]
pub fn classify(path: &LongPath) -> PreviewKind {
    match ext_lower(path).as_deref() {
        Some(e) if IMAGE_EXTS.contains(&e) => PreviewKind::Image,
        Some(e) if TEXT_EXTS.contains(&e) => PreviewKind::Text,
        _ => PreviewKind::Other,
    }
}

/// テキストプレビューを読み込む（1MB 未満かつ先頭 `TEXT_MAX_CHARS` 文字まで）。
///
/// UTF-8 を優先し、不正なら Shift-JIS（CP932）として解読する（日本語環境の非 UTF-8 対応, polish3）。
/// 上限超過・読み込み失敗・いずれの符号化でも解読不能（＝バイナリ相当）は `None`
/// （呼び出し側は「関連付けで開く」へフォールバック）。
#[must_use]
pub fn read_text_preview(path: &LongPath) -> Option<String> {
    // 260 字超パスでも読めるよう `\\?\` 経由（[`LongPath::fs_path`]）でアクセスする。
    let p = path.fs_path();
    let len = std::fs::metadata(&p).ok()?.len();
    if len >= TEXT_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(&p).ok()?;
    let text = decode_text(&bytes)?;
    Some(text.chars().take(TEXT_MAX_CHARS).collect())
}

/// バイト列を UTF-8 優先・Shift-JIS フォールバックで文字列へ解読する。
///
/// どちらでも不正バイトが残る場合（バイナリ相当）は `None`。
fn decode_text(bytes: &[u8]) -> Option<String> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        // 先頭 UTF-8 BOM（U+FEFF）はメモ帳等が付与する。表示に残さず除去する。
        return Some(s.trim_start_matches('\u{FEFF}').to_string());
    }
    // Shift-JIS（CP932）で解読。不正バイトが U+FFFD に置換された場合はバイナリとみなす。
    let (cow, _, had_errors) = encoding_rs::SHIFT_JIS.decode(bytes);
    if had_errors {
        None
    } else {
        Some(cow.into_owned())
    }
}

/// 画像をプレビュー可能か（サイズ上限内か）。
///
/// 260 字超パスでもサイズ取得できるよう `\\?\` 経由（[`LongPath::fs_path`]）で `metadata` する。
#[must_use]
pub fn image_within_limit(path: &LongPath) -> bool {
    let fs_path = path.fs_path();
    std::fs::metadata(fs_path.as_ref())
        .map(|m| m.len() < IMAGE_MAX_BYTES)
        .unwrap_or(false)
}

/// HEIC/HEIF 拡張子か（`heic` feature 有効時のみ存在＝それ以外では描画分岐に入らない）。
#[cfg(feature = "heic")]
#[must_use]
pub(crate) fn is_heic(path: &LongPath) -> bool {
    matches!(ext_lower(path).as_deref(), Some("heic") | Some("heif"))
}

/// プレビュー画像を読み込み `slint::Image` を返す。HEIC/HEIF（`heic` feature 時）は core で
/// デコードして RGBA8 から構築、それ以外は slint の標準ローダ（OS デコーダ）。
/// 失敗は `None`（呼び出し側は「関連付けで開く」へフォールバック）。
#[must_use]
pub fn load_image_preview(path: &LongPath) -> Option<slint::Image> {
    #[cfg(feature = "heic")]
    if is_heic(path) {
        return load_heic_image(path);
    }
    // OS デコーダ（slint 標準ローダ）も 260 字超パスを開けるよう `\\?\` 経由で読む。
    let fs_path = path.fs_path();
    slint::Image::load_from_path(fs_path.as_ref()).ok()
}

/// HEIC/HEIF を core でデコードし RGBA8 から `slint::Image` を構築する。
///
/// ロングパス対応のため `\\?\` 経由（[`LongPath::fs_path`]）で読む。読み込み・デコード失敗、
/// および（起こり得ないが防御的に）バッファ長不整合は `tracing` に記録して `None` を返す
/// （無音握り潰しを避けつつ UI スレッドでの panic も避ける）。入力サイズは呼び出し側の
/// `image_within_limit`（30MB）で、展開後は core の `max_pixels`/`max_memory_bytes`（512MiB）で
/// 二重に上限保護される。
#[cfg(feature = "heic")]
fn load_heic_image(path: &LongPath) -> Option<slint::Image> {
    let bytes = match std::fs::read(path.fs_path()) {
        Ok(b) => b,
        Err(e) => {
            tracing::debug!(path = %path.as_path().display(), error = %e, "HEIC プレビュー読込失敗");
            return None;
        }
    };
    let (w, h, rgba) = match shredy_core::imgsim::decode_heic_rgba8(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(path = %path.as_path().display(), error = %e, "HEIC デコード失敗");
            return None;
        }
    };
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
    {
        let dst = buf.make_mut_bytes();
        // core 側 `RgbaImage::from_raw` の不変条件で w*h*4 == rgba.len() が保証されるが、
        // copy_from_slice の panic を避けるため防御的に検証する。
        if dst.len() != rgba.len() {
            tracing::warn!(
                expected = dst.len(),
                got = rgba.len(),
                "HEIC RGBA8 バッファ長不整合"
            );
            return None;
        }
        dst.copy_from_slice(&rgba);
    }
    Some(slint::Image::from_rgba8(buf))
}

/// プレビューパネルに表示する EXIF 情報（§3.1#1）。各項目は無ければ空文字。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewExif {
    /// 撮影日時（`YYYY-MM-DD HH:MM:SS`）。
    pub datetime: String,
    /// カメラ（メーカー＋機種）。
    pub camera: String,
    /// レンズ。
    pub lens: String,
    /// 撮影位置（十進度の `緯度, 経度`）。
    pub gps: String,
}

/// 画像から EXIF を読み、プレビュー表示用に整形する。EXIF が無ければ `None`。
#[must_use]
pub fn read_preview_exif(path: &LongPath) -> Option<PreviewExif> {
    let info = shredy_core::exif::read_exif(path.as_path())?;
    let datetime = info
        .datetime
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    let camera = match (info.make, info.model) {
        (Some(make), Some(model)) => format!("{make} {model}"),
        (Some(s), None) | (None, Some(s)) => s,
        (None, None) => String::new(),
    };
    Some(PreviewExif {
        datetime,
        camera,
        lens: info.lens.unwrap_or_default(),
        gps: info.gps.unwrap_or_default(),
    })
}

/// 形式固有メタデータ読み取りのファイルサイズ上限（64 MiB）。
/// `extract_metadata` はファイル全体をメモリへ読むため、巨大 ZIP での
/// UI スレッドのブロックと大量メモリ確保を避ける（上限以上はメタ表示なし）。
/// 値は通常運用の Office 文書（数十 MiB 級）を許容しつつ、core 側の
/// ZIP 爆弾対策（単一エントリ展開上限 64 MiB）と揃えた。
const FORMAT_META_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// プレビューに表示する形式固有メタデータ（Office/ZIP, §3.1#2）を (ラベル, 値) 列で返す。
/// 非対応形式・サイズ上限超過・抽出失敗は空（行を表示しない）。
#[must_use]
pub fn read_preview_format_meta(path: &LongPath) -> Vec<(String, String)> {
    let supported = path
        .as_path()
        .extension()
        .is_some_and(|e| shredy_core::metadata::ext_supports_metadata(&e.to_string_lossy()));
    // サイズチェックは GUI 側の直接 I/O なので `\\?\` 経由（260 字超対応）。
    let within_cap = supported
        && std::fs::metadata(path.fs_path())
            .map(|m| m.len() < FORMAT_META_MAX_BYTES)
            .unwrap_or(false);
    if !within_cap {
        return Vec::new();
    }
    // `extract_metadata` は core 内部で LongPath 変換するため `as_path()` を渡す
    // （`fs_path()` を渡すと `\\?\` の二重変換になる）。
    shredy_core::metadata::extract_metadata(path.as_path())
        .map(format_meta_rows)
        .unwrap_or_default()
}

/// `FormatMetadata` を表示行へ平坦化する（先頭に「形式」行）。
fn format_meta_rows(m: shredy_core::metadata::FormatMetadata) -> Vec<(String, String)> {
    std::iter::once(("形式".to_owned(), m.format))
        .chain(m.fields)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lp(p: &str) -> LongPath {
        LongPath::new(p).unwrap()
    }

    #[test]
    fn format_meta_rows_prepends_format() {
        let m = shredy_core::metadata::FormatMetadata {
            format: "Word".to_string(),
            fields: vec![("ページ数".to_string(), "3".to_string())],
        };
        assert_eq!(
            format_meta_rows(m),
            vec![
                ("形式".to_string(), "Word".to_string()),
                ("ページ数".to_string(), "3".to_string()),
            ]
        );
    }

    #[test]
    fn format_meta_empty_for_unsupported_or_missing() {
        // 非対応拡張子は I/O なしで空。対応拡張子でも実在しないファイルは空。
        assert!(read_preview_format_meta(&lp("C:\\a\\photo.jpg")).is_empty());
        assert!(read_preview_format_meta(&lp("C:\\no\\such\\file.zip")).is_empty());
    }

    #[test]
    fn format_meta_reads_zip() {
        // 空 ZIP（EOCD のみ 22 バイト）でも「形式」「エントリ数」を返す。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.zip");
        let eocd: [u8; 22] = [
            0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        std::fs::write(&path, eocd).unwrap();
        let rows = read_preview_format_meta(&LongPath::new(&path).unwrap());
        assert_eq!(
            rows.len(),
            4,
            "形式＋エントリ数＋展開後/圧縮後サイズの 4 行"
        );
        assert_eq!(rows[0], ("形式".to_string(), "ZIP".to_string()));
        assert_eq!(rows[1], ("エントリ数".to_string(), "0".to_string()));
        assert_eq!(rows[2].0, "展開後サイズ");
        assert_eq!(rows[3].0, "圧縮後サイズ");
    }

    #[test]
    fn classifies_images() {
        assert_eq!(classify(&lp("C:\\a\\photo.JPG")), PreviewKind::Image);
        assert_eq!(classify(&lp("C:\\a\\b.png")), PreviewKind::Image);
        assert_eq!(classify(&lp("C:\\a\\c.webp")), PreviewKind::Image);
    }

    #[cfg(feature = "heic")]
    #[test]
    fn classifies_heic_as_image() {
        // heic feature 有効時は HEIC/HEIF も画像プレビュー対象（大小文字無関係）。
        assert_eq!(classify(&lp("C:\\a\\photo.heic")), PreviewKind::Image);
        assert_eq!(classify(&lp("C:\\a\\photo.HEIF")), PreviewKind::Image);
        assert!(is_heic(&lp("C:\\a\\x.heic")));
        assert!(is_heic(&lp("C:\\a\\x.HEIF")));
        assert!(!is_heic(&lp("C:\\a\\x.jpg")));
    }

    #[test]
    fn classifies_text() {
        assert_eq!(classify(&lp("C:\\a\\note.txt")), PreviewKind::Text);
        assert_eq!(classify(&lp("C:\\a\\data.JSON")), PreviewKind::Text);
        assert_eq!(classify(&lp("C:\\a\\readme.md")), PreviewKind::Text);
    }

    #[test]
    fn classifies_other() {
        assert_eq!(classify(&lp("C:\\a\\app.exe")), PreviewKind::Other);
        assert_eq!(classify(&lp("C:\\a\\movie.mp4")), PreviewKind::Other);
        assert_eq!(classify(&lp("C:\\a\\noext")), PreviewKind::Other);
    }

    #[test]
    fn reads_text_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.txt");
        let body: String = "あ".repeat(10_000); // 10000 文字
        std::fs::write(&path, &body).unwrap();
        let lp = LongPath::new(path.to_string_lossy().as_ref()).unwrap();
        let preview = read_text_preview(&lp).unwrap();
        assert_eq!(preview.chars().count(), TEXT_MAX_CHARS);
    }

    #[test]
    fn read_text_none_for_missing() {
        assert!(read_text_preview(&lp("C:\\does\\not\\exist.txt")).is_none());
    }

    #[test]
    fn decode_prefers_utf8() {
        let s = "日本語 ABC";
        assert_eq!(decode_text(s.as_bytes()).as_deref(), Some(s));
    }

    #[test]
    fn decode_falls_back_to_shift_jis() {
        // UTF-8 として不正な Shift-JIS バイト列を生成し、解読できることを確認。
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("日本語テキスト");
        assert!(std::str::from_utf8(&bytes).is_err()); // UTF-8 では不正
        assert_eq!(decode_text(&bytes).as_deref(), Some("日本語テキスト"));
    }

    #[test]
    fn decode_strips_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("テキスト".as_bytes());
        assert_eq!(decode_text(&bytes).as_deref(), Some("テキスト"));
    }

    #[test]
    fn decode_returns_none_for_binary() {
        // 妥当な Shift-JIS でも UTF-8 でもないバイト列はバイナリ相当 → None。
        let binary = [0xFF, 0xFE, 0x00, 0x80, 0x81, 0x00, 0xFF];
        assert!(decode_text(&binary).is_none());
    }

    // 260 字超パス（§3.4#4）。GUI 側の直接 I/O が `\\?\` 経由かを保証する。
    // `as_path()` のままだと `metadata`/`read` が失敗しプレビューがスキップされる回帰を防ぐ。
    #[cfg(windows)]
    #[test]
    fn image_within_limit_handles_long_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("{}.png", "a".repeat(250)));
        assert!(path.as_os_str().len() > 260, "テスト前提: パスは 260 字超");
        let lp = LongPath::new(&path).unwrap();
        std::fs::write(lp.fs_path(), b"img").unwrap();
        assert!(image_within_limit(&lp));
    }

    #[cfg(windows)]
    #[test]
    fn read_text_preview_handles_long_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("{}.txt", "b".repeat(250)));
        assert!(path.as_os_str().len() > 260, "テスト前提: パスは 260 字超");
        let lp = LongPath::new(&path).unwrap();
        std::fs::write(lp.fs_path(), "日本語テキスト").unwrap();
        assert_eq!(read_text_preview(&lp).as_deref(), Some("日本語テキスト"));
    }
}
