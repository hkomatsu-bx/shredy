//! 画像メタデータ（EXIF）除去（仕様 §3.10.3・ユーティリティ機能）。
//!
//! `img-parts` を用いて JPEG / PNG / WebP の EXIF を「再エンコードなし」でセグメント・
//! チャンク単位に除去する（Kataduke の Magick.NET `.Strip()` に相当する純 Rust 実装）。
//! 画素は一切再圧縮しないため画質は劣化しない。
//!
//! 除去対象は **EXIF のみ**。ICC プロファイル（色再現に必要）は意図的に保持する。
//! 一方 **XMP は除去しない**（`img-parts` の EXIF 除去 API の対象外）。XMP に GPS・作成者等が
//! 重複保存されている場合は残るため、完全なプライバシー除去には別途 XMP 除去が必要（既知の制限）。
//!
//! 同一ファイルへの同時呼び出しは未サポート（一時ファイル名が衝突する。GUI/CLI は直列実行前提）。
//!
//! 非対応（既知の制限）:
//! - **TIFF**: EXIF が TIFF の IFD 構造そのものに内在し、独立した EXIF ブロックとして
//!   除去できない（`img-parts` 非対応）。対応には画素デコード＋再エンコードが必要で別扱い。
//! - **HEIC/HEIF**: デコード手段が未確定（C 依存回避、仕様 §3.10.3 注記）。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use img_parts::{Bytes, DynImage, ImageEXIF};
use walkdir::WalkDir;

use crate::error::{CoreError, Result};
use crate::path::{to_fs_path, LongPath};
use crate::progress::{is_cancelled, Cancel};
use crate::scan::MAX_DEPTH;
use crate::utility::UtilityItem;

/// EXIF 除去オプション。
#[derive(Debug, Clone, Copy, Default)]
pub struct StripOptions {
    /// 上書き前に `<path>.bak` へ元ファイルを退避する（仕様 §3.10.3 `--backup`）。
    pub backup: bool,
    /// 書き込まず、除去対象かの判定のみ行う（件数集計・プレビュー用、仕様 §3.10.3 `--dry-run`）。
    pub dry_run: bool,
}

/// 1 ファイルの EXIF 除去結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripOutcome {
    /// EXIF を除去した（`dry_run` 時は「除去対象だった」）。
    Removed,
    /// EXIF が無く変更不要だった。
    NoMetadata,
    /// 非対応形式・解析不能のためスキップした（ファイルは変更しない）。
    Unsupported,
}

/// 拡張子が EXIF 除去の対象候補か（小文字・先頭ドット無しで判定）。
///
/// 最終的な対応可否は [`DynImage`] の解析結果で決まる（本関数は GUI/CLI 向けの事前フィルタ）。
#[must_use]
pub fn ext_supports_exif_removal(ext: &str) -> bool {
    matches!(
        ext.trim_start_matches('.').to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "webp"
    )
}

/// 画像から EXIF を除去する。
///
/// - 対応形式（JPEG/PNG/WebP）で EXIF があれば除去して書き戻し、[`StripOutcome::Removed`]。
/// - EXIF が無ければ [`StripOutcome::NoMetadata`]（ファイルは変更しない）。
/// - 非対応形式・解析不能なら [`StripOutcome::Unsupported`]（ファイルは変更しない）。
///
/// 書き込みは同一ディレクトリの一時ファイルへ出力してからリネームし、途中失敗による
/// 元ファイル破損を避ける（安全優先）。
pub fn strip_image_metadata(path: &Path, opts: StripOptions) -> Result<StripOutcome> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !ext_supports_exif_removal(&ext) {
        return Ok(StripOutcome::Unsupported);
    }

    let fs_path = to_fs_path(path);

    let raw = std::fs::read(&fs_path).map_err(|e| CoreError::io(&fs_path, e))?;
    let parsed = DynImage::from_bytes(Bytes::from(raw))
        .map_err(|e| CoreError::Image(format!("画像の解析に失敗しました: {e}")))?;
    let Some(mut img) = parsed else {
        return Ok(StripOutcome::Unsupported);
    };

    if img.exif().is_none() {
        return Ok(StripOutcome::NoMetadata);
    }
    if opts.dry_run {
        return Ok(StripOutcome::Removed);
    }

    img.set_exif(None);
    let mut encoded: Vec<u8> = Vec::new();
    img.encoder()
        .write_to(&mut encoded)
        .map_err(|e| CoreError::Image(format!("画像の書き出しに失敗しました: {e}")))?;

    if opts.backup {
        let bak = backup_path(&fs_path);
        std::fs::copy(&fs_path, &bak).map_err(|e| CoreError::io(&bak, e))?;
        // 書き込み失敗時は元ファイルが無変更のため、作成したバックアップも残さない。
        if let Err(e) = write_atomic(&fs_path, &encoded) {
            // バックアップ削除の失敗は呼び出し側へ伝える手段がなく、本質的エラー（書き込み
            // 失敗 e）の伝播を優先するため best-effort で無視する。
            let _ = std::fs::remove_file(&bak);
            return Err(e);
        }
    } else {
        write_atomic(&fs_path, &encoded)?;
    }
    Ok(StripOutcome::Removed)
}

/// EXIF 除去候補（対応拡張子の画像）をフォルダ配下から再帰列挙する（GUI ユーティリティ用）。
///
/// 拡張子のみで候補化し（[`ext_supports_exif_removal`]）、ファイルは読まない（高速）。実際に
/// EXIF を持つかは [`strip_image_metadata`] 実行時に判定される。`detail` は拡張子（大文字）。
/// リパースポイントは辿らず（実体巻き込み防止）、中断要求時はその時点までの結果を返す。
#[must_use]
pub fn scan_exif_candidates(folders: &[LongPath], cancel: &Cancel) -> Vec<UtilityItem> {
    let mut out = Vec::new();
    for root in folders {
        let walker = WalkDir::new(root.as_path())
            .max_depth(MAX_DEPTH)
            .follow_links(false);
        for entry in walker.into_iter().flatten() {
            if is_cancelled(cancel) {
                return out;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if !ext_supports_exif_removal(&ext) {
                continue;
            }
            if let Ok(lp) = LongPath::new(path) {
                let meta = entry.metadata().ok();
                let size = meta.as_ref().map_or(0, std::fs::Metadata::len);
                let modified = meta
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                out.push(UtilityItem {
                    path: lp,
                    size,
                    modified,
                    is_dir: false,
                    detail: ext.to_ascii_uppercase(),
                });
            }
        }
    }
    out
}

/// 複数画像の EXIF 一括除去の結果集計（仕様 §3.10.3）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StripSummary {
    /// EXIF を除去した件数。
    pub removed: usize,
    /// EXIF が無く変更不要だった件数。
    pub no_metadata: usize,
    /// 非対応・解析不能でスキップした件数。
    pub unsupported: usize,
    /// I/O 等のエラーで失敗した件数。
    pub failed: usize,
}

/// 指定パス群の EXIF を順に除去する（同一ファイルの同時実行は不可のため直列実行）。
///
/// 各ファイルの結果を [`StripSummary`] に集計する。エラーは握りつぶさず `failed` に計上して
/// 続行する（1 件の失敗で全体を止めない）。中断要求時はその時点までの集計を返す。
#[must_use]
pub fn strip_many(paths: &[LongPath], opts: StripOptions, cancel: &Cancel) -> StripSummary {
    let mut summary = StripSummary::default();
    for path in paths {
        if is_cancelled(cancel) {
            return summary;
        }
        match strip_image_metadata(&path.fs_path(), opts) {
            Ok(StripOutcome::Removed) => summary.removed += 1,
            Ok(StripOutcome::NoMetadata) => summary.no_metadata += 1,
            Ok(StripOutcome::Unsupported) => summary.unsupported += 1,
            Err(e) => {
                tracing::warn!(path = %path.as_path().display(), error = %e, "EXIF 除去に失敗しました");
                summary.failed += 1;
            }
        }
    }
    summary
}

/// `<path>.bak` を返す（既存と衝突した場合は上書きし、直近の元ファイルを保持する）。
fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

/// 同一ディレクトリの一時ファイルへ書いてからリネームし、途中失敗時の破損を避ける。
fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let mut tmp = dir.map(Path::to_path_buf).unwrap_or_default();
    let mut tmp_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    tmp_name.push(".shredy-tmp");
    tmp.push(&tmp_name);
    std::fs::write(&tmp, data).map_err(|e| CoreError::io(&tmp, e))?;
    // Windows の `rename` は既存の宛先を置換する（MOVEFILE_REPLACE_EXISTING 相当）。
    // rename 失敗時は書き込み済みの一時ファイルを残さない（best-effort クリーンアップ）。
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(CoreError::io(path, e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Make フィールドのみを持つ最小 TIFF（リトルエンディアン・IFD0 に 1 エントリ）。
    fn minimal_tiff_with_make(make: &[u8; 4]) -> Vec<u8> {
        let mut t = Vec::new();
        t.extend_from_slice(b"II"); // リトルエンディアン
        t.extend_from_slice(&[0x2a, 0x00]); // マジック 42
        t.extend_from_slice(&[0x08, 0, 0, 0]); // IFD0 オフセット = 8
        t.extend_from_slice(&[0x01, 0x00]); // エントリ数 = 1
        t.extend_from_slice(&[0x0f, 0x01]); // タグ Make (0x010F)
        t.extend_from_slice(&[0x02, 0x00]); // 型 ASCII (2)
        t.extend_from_slice(&[0x04, 0, 0, 0]); // 個数 = 4
        t.extend_from_slice(make); // 値（4 バイトに収まる）
        t.extend_from_slice(&[0x00, 0, 0, 0]); // 次 IFD = 0
        t
    }

    /// 実画素を持つ有効な JPEG に EXIF(APP1) を注入した、構造的に正しい JPEG を作る。
    fn jpeg_with_exif() -> Vec<u8> {
        let mut base = Vec::new();
        let img = image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]));
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut base), image::ImageFormat::Jpeg)
            .unwrap();

        let tiff = minimal_tiff_with_make(b"TST\0");
        let mut app1 = Vec::new();
        app1.extend_from_slice(&[0xFF, 0xE1]); // APP1
        let payload_len = (2 + 6 + tiff.len()) as u16; // 長さ自身 + "Exif\0\0" + TIFF
        app1.extend_from_slice(&payload_len.to_be_bytes());
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);

        // SOI(先頭 2 バイト) の直後に APP1 を挿入する。
        let mut out = Vec::with_capacity(base.len() + app1.len());
        out.extend_from_slice(&base[..2]);
        out.extend_from_slice(&app1);
        out.extend_from_slice(&base[2..]);
        out
    }

    #[test]
    fn ext_filter_matches_supported_only() {
        assert!(ext_supports_exif_removal("jpg"));
        assert!(ext_supports_exif_removal("JPEG"));
        assert!(ext_supports_exif_removal(".png"));
        assert!(ext_supports_exif_removal("webp"));
        assert!(!ext_supports_exif_removal("tif")); // TIFF は非対応
        assert!(!ext_supports_exif_removal("heic"));
        assert!(!ext_supports_exif_removal("txt"));
    }

    #[test]
    fn strips_exif_from_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        std::fs::write(&path, jpeg_with_exif()).unwrap();
        // 前提: EXIF を読み取れる。
        assert!(crate::exif::read_exif(&path).is_some());

        let outcome = strip_image_metadata(&path, StripOptions::default()).unwrap();
        assert_eq!(outcome, StripOutcome::Removed);
        // 除去後は EXIF が読めない。
        assert!(crate::exif::read_exif(&path).is_none());
    }

    #[test]
    fn dry_run_reports_without_modifying() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        let original = jpeg_with_exif();
        std::fs::write(&path, &original).unwrap();

        let outcome = strip_image_metadata(
            &path,
            StripOptions {
                dry_run: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(outcome, StripOutcome::Removed);
        // dry_run はファイルを変更しない。
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(crate::exif::read_exif(&path).is_some());
    }

    #[test]
    fn backup_preserves_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        let original = jpeg_with_exif();
        std::fs::write(&path, &original).unwrap();

        let outcome = strip_image_metadata(
            &path,
            StripOptions {
                backup: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(outcome, StripOutcome::Removed);
        let bak = path.with_file_name("photo.jpg.bak");
        assert_eq!(std::fs::read(&bak).unwrap(), original);
        assert!(crate::exif::read_exif(&path).is_none());
    }

    #[test]
    fn jpeg_without_exif_reports_no_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.jpg");
        let mut base = Vec::new();
        let img = image::RgbImage::from_pixel(2, 2, image::Rgb([0, 0, 0]));
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut base), image::ImageFormat::Jpeg)
            .unwrap();
        std::fs::write(&path, &base).unwrap();

        let outcome = strip_image_metadata(&path, StripOptions::default()).unwrap();
        assert_eq!(outcome, StripOutcome::NoMetadata);
    }

    #[test]
    fn unsupported_extension_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, b"hello").unwrap();
        let outcome = strip_image_metadata(&path, StripOptions::default()).unwrap();
        assert_eq!(outcome, StripOutcome::Unsupported);
    }

    #[test]
    fn scan_finds_only_supported_images_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.jpg"), jpeg_with_exif()).unwrap();
        std::fs::write(dir.path().join("b.txt"), b"x").unwrap();
        // png は拡張子のみで候補化する（中身は問わない）。
        std::fs::write(dir.path().join("c.PNG"), b"fake").unwrap();
        let cancel = crate::progress::new_cancel();
        let items = scan_exif_candidates(&[LongPath::new(dir.path()).unwrap()], &cancel);
        assert_eq!(items.len(), 2, "jpg と png のみ候補化（txt は除外）");
        assert!(items.iter().all(|i| !i.is_dir));
    }

    #[test]
    fn strip_many_tallies_outcomes() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("withexif.jpg");
        std::fs::write(&p1, jpeg_with_exif()).unwrap();
        let p2 = dir.path().join("plain.jpg");
        let mut base = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 2, image::Rgb([0, 0, 0])))
            .write_to(&mut Cursor::new(&mut base), image::ImageFormat::Jpeg)
            .unwrap();
        std::fs::write(&p2, &base).unwrap();

        let paths = vec![LongPath::new(&p1).unwrap(), LongPath::new(&p2).unwrap()];
        let cancel = crate::progress::new_cancel();
        let summary = strip_many(&paths, StripOptions::default(), &cancel);
        assert_eq!(summary.removed, 1, "EXIF ありを 1 件除去");
        assert_eq!(summary.no_metadata, 1, "EXIF 無しを 1 件");
        assert_eq!(summary.unsupported, 0);
        assert_eq!(summary.failed, 0);
    }
}
