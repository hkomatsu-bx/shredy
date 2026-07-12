//! 形式固有メタデータの抽出（仕様 §3.1#2 / プレビュー §3.5 の将来拡張）。
//!
//! プレビューパネルでの表示用に、ファイル形式ごとの代表的なメタデータを (ラベル, 値) の列で返す。
//! 既存の `zip` / `quick-xml` を流用し新規依存を増やさない。優先度は Office > ZIP。
//!
//! 対応:
//! - **Office Open XML**（.docx / .xlsx / .pptx）: 作成者・タイトル・作成/更新日時（`docProps/core.xml`）、
//!   ページ数・単語数（`docProps/app.xml`）、シート数・スライド数（エントリ計数）。
//! - **ZIP**（.zip）: エントリ数・展開後/圧縮後サイズ。
//!
//! 非対応（将来拡張）: PDF（純 Rust クレートが未成熟）・動画（C 依存回避のため）。

use std::io::{Cursor, Read};
use std::path::Path;

use chrono::{DateTime, FixedOffset};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zip::ZipArchive;

use crate::path::to_fs_path;

/// 単一 ZIP エントリの展開上限（64 MiB）。ZIP 爆弾によるメモリ枯渇を防ぐ（ooxml と同方針）。
const ENTRY_CAP: u64 = 64 * 1024 * 1024;

/// ZIP のサイズ集計を行うエントリ数の上限。これを超える異常なエントリ数（ZIP 爆弾）では
/// 全エントリのローカルヘッダ走査（線形・低速）を避け、エントリ数のみ報告する。
const MAX_ZIP_ENTRIES: usize = 100_000;

/// 形式固有メタデータ（表示用）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormatMetadata {
    /// 形式名（例: `Word` / `Excel` / `PowerPoint` / `ZIP`）。
    pub format: String,
    /// 表示用の (ラベル, 値) 列。
    pub fields: Vec<(String, String)>,
}

/// 拡張子が形式固有メタデータ抽出の対象か（小文字・先頭ドット無しで判定）。
#[must_use]
pub fn ext_supports_metadata(ext: &str) -> bool {
    matches!(
        ext.trim_start_matches('.').to_ascii_lowercase().as_str(),
        "docx" | "xlsx" | "pptx" | "zip"
    )
}

/// ファイルから形式固有メタデータを抽出する。非対応・読み取り不能・抽出項目なしは `None`。
#[must_use]
pub fn extract_metadata(path: &Path) -> Option<FormatMetadata> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if !ext_supports_metadata(&ext) {
        return None;
    }
    let fs_path = to_fs_path(path);
    let bytes = match std::fs::read(&fs_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::debug!(path = %path.display(), error = %e, "メタデータ用にファイルを読み取れませんでした");
            return None;
        }
    };
    match ext.as_str() {
        "docx" | "xlsx" | "pptx" => office_metadata(&bytes, &ext),
        "zip" => zip_metadata(&bytes),
        _ => None,
    }
}

/// Office Open XML（docx/xlsx/pptx）のメタデータを抽出する。
fn office_metadata(bytes: &[u8], ext: &str) -> Option<FormatMetadata> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).ok()?;

    let app = read_zip_entry(&mut archive, "docProps/app.xml");
    let core = read_zip_entry(&mut archive, "docProps/core.xml");

    let mut fields: Vec<(String, String)> = Vec::new();
    if let Some(core) = &core {
        push_field(&mut fields, "タイトル", xml_text_of(core, "title"));
        push_field(&mut fields, "作成者", xml_text_of(core, "creator"));
        let created = xml_text_of(core, "created").map(|s| format_ooxml_datetime(&s));
        let modified = xml_text_of(core, "modified").map(|s| format_ooxml_datetime(&s));
        push_field(&mut fields, "作成日時", created);
        push_field(&mut fields, "更新日時", modified);
    }

    let format = match ext {
        "docx" => {
            if let Some(app) = &app {
                push_field(&mut fields, "ページ数", xml_text_of(app, "Pages"));
                push_field(&mut fields, "単語数", xml_text_of(app, "Words"));
            }
            "Word"
        }
        "xlsx" => {
            let sheets = archive
                .file_names()
                .filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
                .count();
            if sheets > 0 {
                fields.push(("シート数".to_string(), sheets.to_string()));
            }
            "Excel"
        }
        "pptx" => {
            let slides = archive
                .file_names()
                .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
                .count();
            if slides > 0 {
                fields.push(("スライド数".to_string(), slides.to_string()));
            }
            "PowerPoint"
        }
        _ => "Office",
    };

    if fields.is_empty() {
        return None;
    }
    Some(FormatMetadata {
        format: format.to_string(),
        fields,
    })
}

/// ZIP アーカイブのメタデータ（エントリ数・サイズ）を抽出する。
fn zip_metadata(bytes: &[u8]) -> Option<FormatMetadata> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).ok()?;
    let count = archive.len();
    let mut fields = vec![("エントリ数".to_string(), count.to_string())];
    // 異常なエントリ数（ZIP 爆弾）では全エントリ走査を避け、エントリ数のみ報告する。
    if count <= MAX_ZIP_ENTRIES {
        // size()/compressed_size() は中央ディレクトリの宣言値（内容は読み出さない）。
        let mut uncompressed: u64 = 0;
        let mut compressed: u64 = 0;
        for i in 0..count {
            if let Ok(f) = archive.by_index(i) {
                uncompressed = uncompressed.saturating_add(f.size());
                compressed = compressed.saturating_add(f.compressed_size());
            }
        }
        fields.push(("展開後サイズ".to_string(), format_bytes(uncompressed)));
        fields.push(("圧縮後サイズ".to_string(), format_bytes(compressed)));
    }
    Some(FormatMetadata {
        format: "ZIP".to_string(),
        fields,
    })
}

/// ZIP エントリを上限付きで読み出す（存在しない・上限超過・読み取り失敗は `None`）。
///
/// `size()` は中央ディレクトリの宣言値で偽造され得るため、`take(ENTRY_CAP)` で実読み込みも
/// 制限するのが本来の防御（宣言値チェックは早期棄却の最適化）。
fn read_zip_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let f = archive.by_name(name).ok()?;
    if f.size() > ENTRY_CAP {
        return None;
    }
    let mut buf = Vec::new();
    f.take(ENTRY_CAP).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// XML から指定ローカル名の最初の要素のテキストを取り出す（名前空間接頭辞は無視）。
fn xml_text_of(xml: &[u8], local: &str) -> Option<String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut capture = false;
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Eof => break,
            Event::Start(e) if e.local_name().as_ref() == local.as_bytes() => {
                capture = true;
            }
            Event::Text(t) if capture => {
                let decoded = t.decode().ok()?;
                let s = quick_xml::escape::unescape(&decoded)
                    .ok()?
                    .trim()
                    .to_string();
                return (!s.is_empty()).then_some(s);
            }
            Event::End(_) => capture = false,
            _ => {}
        }
        buf.clear();
    }
    None
}

/// `Option<String>` が非空なら (ラベル, 値) を追加する。
fn push_field(fields: &mut Vec<(String, String)>, label: &str, value: Option<String>) {
    if let Some(v) = value {
        if !v.is_empty() {
            fields.push((label.to_string(), v));
        }
    }
}

/// JST（UTC+9）のオフセット秒。
const JST_OFFSET_SECS: i32 = 9 * 3600;

/// OOXML の W3CDTF 日時（ISO 8601、例 `2024-03-09T10:00:00Z`）を JST の
/// `YYYY/MM/DD HH:MM:SS` へ整形する。タイムゾーンを含まない等で解析できない場合は
/// 元の文字列をそのまま返す（情報を失わない）。
fn format_ooxml_datetime(raw: &str) -> String {
    let (Ok(dt), Some(jst)) = (
        DateTime::parse_from_rfc3339(raw.trim()),
        FixedOffset::east_opt(JST_OFFSET_SECS),
    ) else {
        return raw.to_string();
    };
    dt.with_timezone(&jst)
        .format("%Y/%m/%d %H:%M:%S")
        .to_string()
}

/// バイト数を人間可読に整形する（B/KB/MB/GB/TB、小数 1 桁）。
fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", UNITS[u])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn build_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            for (name, content) in entries {
                zw.start_file(*name, opts).unwrap();
                zw.write_all(content.as_bytes()).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    fn write_temp(ext: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("doc.{ext}"));
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    const CORE: &str = r#"<cp:coreProperties xmlns:cp="x" xmlns:dc="y" xmlns:dcterms="z"><dc:title>My Title</dc:title><dc:creator>Alice</dc:creator><dcterms:created>2024-03-09T10:00:00Z</dcterms:created><dcterms:modified>2024-03-10T11:00:00Z</dcterms:modified></cp:coreProperties>"#;

    #[test]
    fn ext_filter_matches_supported_only() {
        assert!(ext_supports_metadata("docx"));
        assert!(ext_supports_metadata(".ZIP"));
        assert!(!ext_supports_metadata("pdf"));
        assert!(!ext_supports_metadata("mp4"));
        assert!(!ext_supports_metadata("txt"));
    }

    #[test]
    fn docx_extracts_core_and_app_fields() {
        let app = r#"<Properties xmlns="x"><Pages>3</Pages><Words>120</Words></Properties>"#;
        let bytes = build_zip(&[
            ("word/document.xml", "<w:document/>"),
            ("docProps/app.xml", app),
            ("docProps/core.xml", CORE),
        ]);
        let (_d, path) = write_temp("docx", &bytes);
        let md = extract_metadata(&path).unwrap();
        assert_eq!(md.format, "Word");
        assert!(md.fields.contains(&("タイトル".into(), "My Title".into())));
        assert!(md.fields.contains(&("作成者".into(), "Alice".into())));
        assert!(md.fields.contains(&("ページ数".into(), "3".into())));
        assert!(md.fields.contains(&("単語数".into(), "120".into())));
        // 作成/更新日時は JST へ変換し YYYY/MM/DD HH:MM:SS で表示（UTC 10:00 → JST 19:00）。
        assert!(md
            .fields
            .contains(&("作成日時".into(), "2024/03/09 19:00:00".into())));
        assert!(md
            .fields
            .contains(&("更新日時".into(), "2024/03/10 20:00:00".into())));
    }

    #[test]
    fn ooxml_datetime_converts_to_jst() {
        // Z（UTC）と明示オフセットの双方を JST へ正規化する。
        assert_eq!(
            format_ooxml_datetime("2024-03-09T10:00:00Z"),
            "2024/03/09 19:00:00"
        );
        assert_eq!(
            format_ooxml_datetime("2024-03-09T19:00:00+09:00"),
            "2024/03/09 19:00:00"
        );
        // フラクショナル秒付き（RFC3339 合法）も秒以下を切り捨てて表示する。
        assert_eq!(
            format_ooxml_datetime("2024-03-09T10:00:00.5Z"),
            "2024/03/09 19:00:00"
        );
        // 解析できない値は元の文字列のまま返す。
        assert_eq!(format_ooxml_datetime("不明"), "不明");
        assert_eq!(format_ooxml_datetime("2024-03-09"), "2024-03-09");
    }

    #[test]
    fn xlsx_counts_worksheets() {
        let bytes = build_zip(&[
            ("xl/worksheets/sheet1.xml", "<worksheet/>"),
            ("xl/worksheets/sheet2.xml", "<worksheet/>"),
            ("xl/worksheets/_rels/sheet1.xml.rels", "<rels/>"),
            ("docProps/core.xml", CORE),
        ]);
        let (_d, path) = write_temp("xlsx", &bytes);
        let md = extract_metadata(&path).unwrap();
        assert_eq!(md.format, "Excel");
        assert!(
            md.fields.contains(&("シート数".into(), "2".into())),
            "_rels は計数せず 2 シート"
        );
    }

    #[test]
    fn pptx_counts_slides() {
        let bytes = build_zip(&[
            ("ppt/slides/slide1.xml", "<sld/>"),
            ("ppt/slides/slide2.xml", "<sld/>"),
            ("ppt/slides/slide3.xml", "<sld/>"),
        ]);
        let (_d, path) = write_temp("pptx", &bytes);
        let md = extract_metadata(&path).unwrap();
        assert_eq!(md.format, "PowerPoint");
        assert!(md.fields.contains(&("スライド数".into(), "3".into())));
    }

    #[test]
    fn zip_reports_entry_count_and_sizes() {
        let bytes = build_zip(&[("a.txt", "hello"), ("b.txt", "world!!")]);
        let (_d, path) = write_temp("zip", &bytes);
        let md = extract_metadata(&path).unwrap();
        assert_eq!(md.format, "ZIP");
        assert!(md.fields.contains(&("エントリ数".into(), "2".into())));
    }

    #[test]
    fn unsupported_extension_returns_none() {
        let (_d, path) = write_temp("txt", b"plain");
        assert!(extract_metadata(&path).is_none());
    }

    #[test]
    fn format_bytes_scales_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    }
}
