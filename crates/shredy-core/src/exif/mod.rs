//! EXIF メタデータ（共有モジュール）。
//!
//! 一括リネーム（§3.10.1）の EXIF 命名と、将来のプレビューパネル（§3.5）の双方から
//! 再利用することを見越した配置。データモデル [`ExifInfo`]・撮影日時の表示整形・画像からの
//! 抽出（[`read_exif`]、`kamadak-exif`）を提供する。動画はライブラリ非対応のため対象外。

use std::path::Path;

use chrono::NaiveDateTime;
use kamadak_exif::{Exif, Field, In, Reader, Tag, Value};
use serde::{Deserialize, Serialize};

use crate::path::to_fs_path;

/// 画像から抽出した EXIF メタデータ（命名・表示に使うフィールドのみ）。
///
/// 実行時に画像から抽出する一時データであり永続化しない（レシピは命名パターンと書式だけを
/// 保存する）。そのため `serde` 派生は持たない。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExifInfo {
    /// 撮影日時（EXIF `DateTimeOriginal`）。
    pub datetime: Option<NaiveDateTime>,
    /// カメラメーカー（EXIF `Make`）。
    pub make: Option<String>,
    /// 機種（EXIF `Model`）。
    pub model: Option<String>,
    /// レンズ（EXIF `LensModel`）。
    pub lens: Option<String>,
    /// 撮影位置（EXIF GPS）。`"緯度, 経度"`（十進度・小数 6 桁）。GPS が無ければ `None`。
    pub gps: Option<String>,
}

impl ExifInfo {
    /// 命名に使えるフィールドを持つか（= この画像を EXIF リネーム対象とするか）。
    ///
    /// EXIF を持たないファイルは命名対象外（仕様 §3.10.1: 変更しない）。`gps` は命名トークンでは
    /// なくプレビュー表示専用のため、ここには **含めない**（GPS だけ持つ稀な画像をリネーム処理へ
    /// 誘導しない）。GPS は datetime/make/model/lens のいずれかが在ればこの判定を通り、併せて
    /// `read_exif` から返される。
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.datetime.is_some()
            || self.make.is_some()
            || self.model.is_some()
            || self.lens.is_some()
    }
}

/// 撮影日時の表示書式（仕様 §3.10.1 EXIF 命名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExifDateFormat {
    /// `YYYYMMDD_HHMMSS`
    DateTimeCompact,
    /// `YYYY-MM-DD`
    DateDash,
    /// `YYYYMMDD`
    DateCompact,
}

impl ExifDateFormat {
    /// `chrono` の strftime 書式文字列。
    #[must_use]
    pub fn pattern(self) -> &'static str {
        match self {
            ExifDateFormat::DateTimeCompact => "%Y%m%d_%H%M%S",
            ExifDateFormat::DateDash => "%Y-%m-%d",
            ExifDateFormat::DateCompact => "%Y%m%d",
        }
    }
}

/// 撮影日時を指定書式で整形する。日時が無ければ空文字。
#[must_use]
pub fn format_datetime(dt: Option<NaiveDateTime>, fmt: ExifDateFormat) -> String {
    dt.map(|d| d.format(fmt.pattern()).to_string())
        .unwrap_or_default()
}

/// 拡張子が EXIF 抽出対象（静止画）か。
///
/// 動画（mp4 等）は `kamadak-exif` の対象外のため含めない（仕様 §3.10.1 の「対象は画像・動画」
/// のうち画像のみ＝既知の制限）。HEIF/HEIC はメタデータ抽出のみ可能（ピクセルのデコードは不可）。
#[must_use]
pub fn ext_supports_exif(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "tif" | "tiff" | "png" | "webp" | "heif" | "heic"
    )
}

/// 画像から EXIF を読み取る。EXIF が無い・読めない・対応フィールドが空なら `None`。
///
/// ロングパス対応のため `\\?\` プレフィックス経由で開く。失敗（非対応形式・破損・権限等）は
/// すべて `None` に倒す（命名は安全側＝無変更へ）。
#[must_use]
pub fn read_exif(path: &Path) -> Option<ExifInfo> {
    let fs_path = to_fs_path(path);
    let file = match std::fs::File::open(fs_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::debug!(path = %path.display(), error = %e, "EXIF 用にファイルを開けませんでした");
            return None;
        }
    };
    let mut reader = std::io::BufReader::new(file);
    let exif = match Reader::new().read_from_container(&mut reader) {
        Ok(x) => x,
        Err(e) => {
            // EXIF 非保持の画像（PNG・非写真 JPEG 等）では通常失敗する。詳細は DEBUG のみ。
            tracing::debug!(path = %path.display(), error = %e, "EXIF を読み取れませんでした");
            return None;
        }
    };

    let make = exif
        .get_field(Tag::Make, In::PRIMARY)
        .and_then(field_string);
    let model = exif
        .get_field(Tag::Model, In::PRIMARY)
        .and_then(field_string);
    let lens = exif
        .get_field(Tag::LensModel, In::PRIMARY)
        .and_then(field_string);
    let datetime = exif
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)
        .or_else(|| exif.get_field(Tag::DateTime, In::PRIMARY))
        .and_then(field_string)
        .and_then(|s| parse_exif_datetime(&s));

    let gps = read_gps(&exif);

    let info = ExifInfo {
        datetime,
        make,
        model,
        lens,
        gps,
    };
    info.is_present().then_some(info)
}

/// GPS 緯度・経度（EXIF GPS IFD）を `"緯度, 経度"`（十進度・小数 6 桁）へ整形する。
/// 緯度か経度のいずれかが欠ける場合は `None`。
fn read_gps(exif: &Exif) -> Option<String> {
    let lat = gps_coord(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, 'S')?;
    let lon = gps_coord(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, 'W')?;
    Some(format!("{lat:.6}, {lon:.6}"))
}

/// GPS 座標 1 軸を十進度で返す。度分秒（rational[3]）＋方位参照（S/W で負）から計算する。
fn gps_coord(exif: &Exif, coord_tag: Tag, ref_tag: Tag, negative_ref: char) -> Option<f64> {
    let field = exif.get_field(coord_tag, In::PRIMARY)?;
    let Value::Rational(dms) = &field.value else {
        return None;
    };
    if dms.len() < 3 {
        return None;
    }
    let reference = exif
        .get_field(ref_tag, In::PRIMARY)
        .and_then(field_string)
        .unwrap_or_default();
    let negative = reference
        .trim()
        .to_ascii_uppercase()
        .starts_with(negative_ref);
    Some(dms_to_decimal(
        dms[0].to_f64(),
        dms[1].to_f64(),
        dms[2].to_f64(),
        negative,
    ))
}

/// 度分秒を十進度へ変換する（`negative` で南緯・西経を負にする）。
fn dms_to_decimal(deg: f64, min: f64, sec: f64, negative: bool) -> f64 {
    let value = deg + min / 60.0 + sec / 3600.0;
    if negative {
        -value
    } else {
        value
    }
}

/// EXIF フィールドを文字列化する（ASCII は生バイトから NUL/空白を除去・空は `None`）。
fn field_string(field: &Field) -> Option<String> {
    let raw = match &field.value {
        Value::Ascii(parts) => {
            let bytes = parts.first()?;
            String::from_utf8_lossy(bytes).into_owned()
        }
        _ => field.display_value().to_string(),
    };
    let trimmed = raw.trim().trim_end_matches('\0').trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// EXIF の撮影日時文字列（`YYYY:MM:DD HH:MM:SS`）を [`NaiveDateTime`] へ。書式不一致は `None`。
fn parse_exif_datetime(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s.trim(), "%Y:%m:%d %H:%M:%S").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn sample_dt() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2024, 3, 9)
            .unwrap()
            .and_hms_opt(14, 5, 6)
            .unwrap()
    }

    #[test]
    fn formats_datetime_in_each_style() {
        let dt = Some(sample_dt());
        assert_eq!(
            format_datetime(dt, ExifDateFormat::DateTimeCompact),
            "20240309_140506"
        );
        assert_eq!(format_datetime(dt, ExifDateFormat::DateDash), "2024-03-09");
        assert_eq!(format_datetime(dt, ExifDateFormat::DateCompact), "20240309");
    }

    #[test]
    fn missing_datetime_formats_to_empty() {
        assert_eq!(format_datetime(None, ExifDateFormat::DateCompact), "");
    }

    #[test]
    fn is_present_reflects_any_field() {
        assert!(!ExifInfo::default().is_present());
        assert!(ExifInfo {
            model: Some("X100V".into()),
            ..Default::default()
        }
        .is_present());
        // gps は命名トークンでないため is_present に含めない（GPS だけの画像は rename 対象外）。
        assert!(!ExifInfo {
            gps: Some("35.0, 139.0".into()),
            ..Default::default()
        }
        .is_present());
    }

    #[test]
    fn ext_supports_exif_matches_still_images_only() {
        assert!(ext_supports_exif("jpg"));
        assert!(ext_supports_exif("JPEG"));
        assert!(ext_supports_exif("tiff"));
        assert!(!ext_supports_exif("mp4"));
        assert!(!ext_supports_exif("txt"));
    }

    #[test]
    fn dms_to_decimal_converts_and_signs() {
        // 35°39'30" ≈ 35.658333
        let v = dms_to_decimal(35.0, 39.0, 30.0, false);
        assert!((v - 35.658333).abs() < 1e-5);
        // 南緯・西経は負。
        let neg = dms_to_decimal(35.0, 39.0, 30.0, true);
        assert!((neg + 35.658333).abs() < 1e-5);
        // 分・秒が 0 のときは度そのもの。
        assert_eq!(dms_to_decimal(139.0, 0.0, 0.0, false), 139.0);
    }

    #[test]
    fn parse_exif_datetime_handles_standard_and_invalid() {
        assert_eq!(
            parse_exif_datetime("2024:03:09 14:05:06"),
            Some(sample_dt())
        );
        assert!(parse_exif_datetime("not a date").is_none());
    }

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

    #[test]
    fn read_exif_extracts_make_from_tiff() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.tif");
        std::fs::write(&path, minimal_tiff_with_make(b"TST\0")).unwrap();

        let info = read_exif(&path).expect("EXIF を読み取れること");
        assert_eq!(info.make.as_deref(), Some("TST"));
    }

    #[test]
    fn read_exif_extracts_make_from_jpeg_app1() {
        // JPEG コンテナ（SOI + APP1 "Exif\0\0" + TIFF + EOI）。実機の .jpg と同じ経路。
        let tiff = minimal_tiff_with_make(b"TST\0");
        let mut jpeg = Vec::new();
        jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI
        jpeg.extend_from_slice(&[0xFF, 0xE1]); // APP1
        let payload_len = (2 + 6 + tiff.len()) as u16; // 長さ自身 + "Exif\0\0" + TIFF
        jpeg.extend_from_slice(&payload_len.to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);
        jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.jpg");
        std::fs::write(&path, &jpeg).unwrap();

        let info = read_exif(&path).expect("JPEG APP1 から EXIF を読み取れること");
        assert_eq!(info.make.as_deref(), Some("TST"));
    }

    #[test]
    fn read_exif_returns_none_for_non_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, b"just text, no exif").unwrap();
        assert!(read_exif(&path).is_none());
    }
}
