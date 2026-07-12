//! マジックバイト（先頭シグネチャ）の判定（仕様 §3.9.4 / §3.9.5）。
//!
//! ファイル先頭の数バイトから形式を識別する純関数と、拡張子↔形式・拡張子↔対象グループの
//! 対応を提供する。判定はファイル I/O から切り出し、バイト列で単体テスト可能にする。

use crate::config::{CorruptScanGroups, WrongExtensionGroups};

/// シグネチャ判定に必要な先頭バイト数（RIFF/WEBP は 0..4 と 8..12、ftyp は 4..12）。
pub(crate) const HEADER_LEN: usize = 16;

/// マジックバイトで識別できるファイル形式（仕様 §3.9.4 / §3.9.5 の表に対応）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Signature {
    Jpeg,
    Png,
    Gif,
    Bmp,
    Tiff,
    WebP,
    Pdf,
    /// ZIP / Office Open XML（docx 等）。
    Zip,
    /// OLE Compound（旧 Office: doc/xls/ppt）。
    Ole,
    Mp3,
    /// MP4 / MOV（ftyp ボックスを持つ動画コンテナ）。
    Mp4,
    Heic,
    Heif,
}

impl Signature {
    /// 表示用ラベル（GUI の「実体」列）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Signature::Jpeg => "JPEG",
            Signature::Png => "PNG",
            Signature::Gif => "GIF",
            Signature::Bmp => "BMP",
            Signature::Tiff => "TIFF",
            Signature::WebP => "WebP",
            Signature::Pdf => "PDF",
            Signature::Zip => "ZIP/OOXML",
            Signature::Ole => "OLE(旧Office)",
            Signature::Mp3 => "MP3",
            Signature::Mp4 => "MP4/MOV",
            Signature::Heic => "HEIC",
            Signature::Heif => "HEIF",
        }
    }
}

const PNG_SIG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
const OLE_SIG: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// ファイル先頭バイトから形式を識別する。識別できなければ `None`。
///
/// より特異な（長い・複合的な）シグネチャから順に判定する。`ftyp` ボックスは
/// ブランド（オフセット 8..12）で HEIC/HEIF/MP4 を切り分ける（仕様 §3.9.5）。
#[must_use]
pub(crate) fn detect_signature(header: &[u8]) -> Option<Signature> {
    if header.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Signature::Jpeg);
    }
    if header.starts_with(PNG_SIG) {
        return Some(Signature::Png);
    }
    if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        return Some(Signature::Gif);
    }
    if header.starts_with(&[0x49, 0x49, 0x2A, 0x00])
        || header.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
    {
        return Some(Signature::Tiff);
    }
    if header.len() >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        return Some(Signature::WebP);
    }
    // BMP は 2 バイトと最短のため、新形式追加時は判定順に注意（先行判定を優先）。
    if header.starts_with(b"BM") {
        return Some(Signature::Bmp);
    }
    if header.starts_with(b"%PDF") {
        return Some(Signature::Pdf);
    }
    if header.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        return Some(Signature::Zip);
    }
    if header.starts_with(OLE_SIG) {
        return Some(Signature::Ole);
    }
    if header.starts_with(b"ID3") || header.starts_with(&[0xFF, 0xFB]) {
        return Some(Signature::Mp3);
    }
    if has_ftyp_box(header) {
        // ブランド（8..12）で HEIC/HEIF を識別。ブランドが読めない（12 バイト未満）場合や
        // 未知ブランドは汎用 MP4/MOV 動画コンテナ扱いとする（`has_ftyp_box` は 8 バイトで真に
        // なり得るため、ブランド長の前提を `get` の `Some` 分岐で明示的に閉じる）。
        return Some(match header.get(8..12) {
            Some(b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx") => Signature::Heic,
            Some(b"heif" | b"mif1" | b"msf1") => Signature::Heif,
            _ => Signature::Mp4,
        });
    }
    None
}

/// オフセット 4 に `ftyp` ボックスがあるか（仕様 §3.9.4 HEIF 検証）。
#[must_use]
pub(crate) fn has_ftyp_box(header: &[u8]) -> bool {
    header.len() >= 8 && &header[4..8] == b"ftyp"
}

/// JPEG の先頭マジック（`FF D8 FF`）を満たすか（仕様 §3.9.4）。
#[must_use]
pub(crate) fn is_valid_jpeg_header(header: &[u8]) -> bool {
    header.starts_with(&[0xFF, 0xD8, 0xFF])
}

/// PNG の 8 バイトシグネチャを満たすか（仕様 §3.9.4）。
#[must_use]
pub(crate) fn is_valid_png_header(header: &[u8]) -> bool {
    header.starts_with(PNG_SIG)
}

/// 検出シグネチャが拡張子と整合するか（仕様 §3.9.5）。
///
/// `ext` は小文字・先頭ドットなし前提（呼び出し側が [`super`] の `file_extension` で正規化済み）。
/// 未知の拡張子は判定不能として `true`（不一致報告しない＝誤検知回避）を返す。
#[must_use]
pub(crate) fn signature_matches_ext(sig: Signature, ext: &str) -> bool {
    match ext {
        "jpg" | "jpeg" | "jpe" => sig == Signature::Jpeg,
        "png" => sig == Signature::Png,
        "gif" => sig == Signature::Gif,
        "bmp" | "dib" => sig == Signature::Bmp,
        "tif" | "tiff" => sig == Signature::Tiff,
        "webp" => sig == Signature::WebP,
        "pdf" => sig == Signature::Pdf,
        "zip" => sig == Signature::Zip,
        // OOXML は実体が ZIP。
        "docx" | "xlsx" | "pptx" => sig == Signature::Zip,
        // 旧 Office は OLE Compound。
        "doc" | "xls" | "ppt" => sig == Signature::Ole,
        "mp3" => sig == Signature::Mp3,
        "mp4" | "m4v" | "mov" | "qt" => sig == Signature::Mp4,
        "heic" => sig == Signature::Heic,
        "heif" => sig == Signature::Heif,
        // 未知拡張子は判定しない（不一致として報告しない）。
        _ => true,
    }
}

/// 拡張子が属する拡張子不正検出グループ（仕様 §3.9.5 / §5）。
///
/// `ext` は小文字・先頭ドットなし前提。Image は spec §5 の対象（JPEG/PNG/HEIC/HEIF）に
/// 揃える（GIF/BMP/TIFF/WebP は対象外）。
#[must_use]
pub(crate) fn wrong_ext_group_for_ext(ext: &str) -> Option<WrongExtensionGroups> {
    match ext {
        "jpg" | "jpeg" | "jpe" | "png" | "heic" | "heif" => Some(WrongExtensionGroups::IMAGE),
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => Some(WrongExtensionGroups::OFFICE),
        "mp4" | "m4v" | "mov" | "qt" => Some(WrongExtensionGroups::VIDEO),
        "pdf" => Some(WrongExtensionGroups::PDF),
        "mp3" => Some(WrongExtensionGroups::AUDIO),
        "zip" => Some(WrongExtensionGroups::ARCHIVE),
        _ => None,
    }
}

/// 拡張子が属する破損検出グループ（仕様 §3.9.4）。`ext` は小文字・先頭ドットなし前提。
#[must_use]
pub(crate) fn corrupt_group_for_ext(ext: &str) -> Option<CorruptScanGroups> {
    match ext {
        "zip" | "docx" | "xlsx" | "pptx" => Some(CorruptScanGroups::ZIP_LIKE),
        "jpg" | "jpeg" | "jpe" => Some(CorruptScanGroups::JPEG),
        "png" => Some(CorruptScanGroups::PNG),
        "heic" | "heif" => Some(CorruptScanGroups::HEIF),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_image_signatures() {
        assert_eq!(
            detect_signature(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(Signature::Jpeg)
        );
        assert_eq!(detect_signature(PNG_SIG), Some(Signature::Png));
        assert_eq!(detect_signature(b"GIF89a..."), Some(Signature::Gif));
        assert_eq!(detect_signature(b"BM......"), Some(Signature::Bmp));
        assert_eq!(
            detect_signature(&[0x49, 0x49, 0x2A, 0x00]),
            Some(Signature::Tiff)
        );
        assert_eq!(
            detect_signature(b"RIFF\0\0\0\0WEBPxxxx"),
            Some(Signature::WebP)
        );
    }

    #[test]
    fn detects_doc_and_archive_signatures() {
        assert_eq!(detect_signature(b"%PDF-1.7"), Some(Signature::Pdf));
        assert_eq!(
            detect_signature(&[0x50, 0x4B, 0x03, 0x04]),
            Some(Signature::Zip)
        );
        assert_eq!(detect_signature(OLE_SIG), Some(Signature::Ole));
        assert_eq!(detect_signature(b"ID3\x03\x00"), Some(Signature::Mp3));
    }

    #[test]
    fn detects_ftyp_brands() {
        // オフセット 4 = "ftyp"、ブランドで切り分け。
        let heic = b"\0\0\0\x18ftypheic\0\0\0\0";
        let heif = b"\0\0\0\x18ftypmif1\0\0\0\0";
        let mp4 = b"\0\0\0\x18ftypisom\0\0\0\0";
        assert_eq!(detect_signature(heic), Some(Signature::Heic));
        assert_eq!(detect_signature(heif), Some(Signature::Heif));
        assert_eq!(detect_signature(mp4), Some(Signature::Mp4));
    }

    #[test]
    fn unknown_header_is_none() {
        assert_eq!(detect_signature(b"hello world!!!!!"), None);
        assert_eq!(detect_signature(&[]), None);
    }

    #[test]
    fn header_validators() {
        assert!(is_valid_jpeg_header(&[0xFF, 0xD8, 0xFF, 0x00]));
        assert!(!is_valid_jpeg_header(b"not jpeg"));
        assert!(is_valid_png_header(PNG_SIG));
        assert!(!is_valid_png_header(b"\x89PNGxxxx"));
        assert!(has_ftyp_box(b"\0\0\0\x18ftypheic"));
        assert!(!has_ftyp_box(b"\xFF\xD8\xFF\xE0junk"));
    }

    #[test]
    fn signature_matches_extension() {
        // 入力は小文字前提（呼び出し側が file_extension で正規化）。
        assert!(signature_matches_ext(Signature::Jpeg, "jpg"));
        assert!(signature_matches_ext(Signature::Jpeg, "jpeg"));
        assert!(!signature_matches_ext(Signature::Png, "jpg"));
        // OOXML=ZIP / 旧 Office=OLE。
        assert!(signature_matches_ext(Signature::Zip, "docx"));
        assert!(signature_matches_ext(Signature::Ole, "doc"));
        assert!(!signature_matches_ext(Signature::Zip, "doc"));
        // ftyp 動画。
        assert!(signature_matches_ext(Signature::Mp4, "mov"));
        // 未知拡張子は不一致報告しない。
        assert!(signature_matches_ext(Signature::Jpeg, "xyz"));
    }

    #[test]
    fn ext_group_mapping() {
        assert_eq!(
            wrong_ext_group_for_ext("jpg"),
            Some(WrongExtensionGroups::IMAGE)
        );
        assert_eq!(
            wrong_ext_group_for_ext("docx"),
            Some(WrongExtensionGroups::OFFICE)
        );
        assert_eq!(
            wrong_ext_group_for_ext("mp4"),
            Some(WrongExtensionGroups::VIDEO)
        );
        assert_eq!(
            wrong_ext_group_for_ext("pdf"),
            Some(WrongExtensionGroups::PDF)
        );
        assert_eq!(
            wrong_ext_group_for_ext("mp3"),
            Some(WrongExtensionGroups::AUDIO)
        );
        assert_eq!(
            wrong_ext_group_for_ext("zip"),
            Some(WrongExtensionGroups::ARCHIVE)
        );
        assert_eq!(wrong_ext_group_for_ext("txt"), None);

        assert_eq!(
            corrupt_group_for_ext("docx"),
            Some(CorruptScanGroups::ZIP_LIKE)
        );
        assert_eq!(corrupt_group_for_ext("jpeg"), Some(CorruptScanGroups::JPEG));
        assert_eq!(corrupt_group_for_ext("png"), Some(CorruptScanGroups::PNG));
        assert_eq!(corrupt_group_for_ext("heic"), Some(CorruptScanGroups::HEIF));
        assert_eq!(corrupt_group_for_ext("txt"), None);
    }
}
