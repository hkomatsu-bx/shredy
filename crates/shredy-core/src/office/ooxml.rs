//! 新形式 Office（.docx / .xlsx / .pptx）の正規化抽出（仕様 §3.11）。
//!
//! ZIP 内の可視コンテンツ XML のみを選択し、リビジョン ID（`w:rsid*`）や
//! `docProps/` 等の揮発メタデータを除去した正規化テキストを連結する。
//! 同一可視コンテンツであれば SharePoint/OneDrive 同期で書き換わっても同一ハッシュになる。

use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

use crate::config::OfficeSimilarityLevel;
use crate::error::{CoreError, Result};

/// 単一 XML エントリの展開後サイズ上限（64 MiB）。ZIP 爆弾によるメモリ枯渇を防ぐ。
const ENTRY_DECOMPRESS_CAP: u64 = 64 * 1024 * 1024;

/// 新形式 Office の判定（ZIP 内エントリの prefix で形式を確定する）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OoxmlKind {
    /// Word（`word/`）。
    Word,
    /// Excel（`xl/`）。
    Excel,
    /// PowerPoint（`ppt/`）。
    PowerPoint,
}

/// .docx / .xlsx / .pptx から正規化済みコンテンツ文字列を抽出する。
///
/// ZIP として開けない（=新形式でない）場合や対象 XML が皆無の場合は `Err`。
pub fn extract(bytes: &[u8], level: OfficeSimilarityLevel) -> Result<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| CoreError::Office(format!("zip open failed: {e}")))?;

    // エントリ名を辞書順にそろえ、ZIP 格納順の差異を正規化する。`file_names()` は
    // 中央ディレクトリのみを読むため、各エントリを開く（by_index）必要がない。
    let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    names.sort();

    let kind = detect_kind(&names);

    let mut out = String::new();
    // XML 展開バッファはエントリ間で再利用する（エントリ毎の確保・解放を避ける）。
    let mut xml: Vec<u8> = Vec::new();
    for name in &names {
        if !is_content_entry(kind, name, level) {
            continue;
        }
        let file = archive
            .by_name(name)
            .map_err(|e| CoreError::Office(format!("zip entry {name}: {e}")))?;
        // 展開後サイズ上限（中央ディレクトリ宣言値での早期判定 + take で実読込も制限）。
        if file.size() > ENTRY_DECOMPRESS_CAP {
            return Err(CoreError::Office(format!("zip entry {name} too large")));
        }
        xml.clear();
        file.take(ENTRY_DECOMPRESS_CAP)
            .read_to_end(&mut xml)
            .map_err(|e| CoreError::Office(format!("read {name}: {e}")))?;
        let normalized =
            normalize_xml(&xml).map_err(|e| CoreError::Office(format!("normalize {name}: {e}")))?;
        out.push_str(name);
        out.push('\n');
        out.push_str(&normalized);
        out.push('\n');
    }

    if out.is_empty() {
        return Err(CoreError::Office(
            "no Office Open XML content entries found".to_string(),
        ));
    }
    Ok(out)
}

/// ZIP 内エントリ名から形式を判定する（拡張子偽装に頑健）。
fn detect_kind(names: &[String]) -> Option<OoxmlKind> {
    if names.iter().any(|n| n.starts_with("word/")) {
        Some(OoxmlKind::Word)
    } else if names.iter().any(|n| n.starts_with("xl/")) {
        Some(OoxmlKind::Excel)
    } else if names.iter().any(|n| n.starts_with("ppt/")) {
        Some(OoxmlKind::PowerPoint)
    } else {
        None
    }
}

/// 当該エントリが正規化対象（可視コンテンツ）か判定する（仕様 §3.11）。
///
/// `docProps/`・`_rels/`・`xl/calcChain.xml` は常に除外。`IncludeStyle` のときのみ
/// styles/numbering/slideLayouts を追加する。
fn is_content_entry(kind: Option<OoxmlKind>, name: &str, level: OfficeSimilarityLevel) -> bool {
    if name.starts_with("docProps/") || name.contains("_rels/") || name == "xl/calcChain.xml" {
        return false;
    }
    let include_style = matches!(level, OfficeSimilarityLevel::IncludeStyle);
    match kind {
        Some(OoxmlKind::Word) => {
            name == "word/document.xml"
                || (include_style && (name == "word/styles.xml" || name == "word/numbering.xml"))
        }
        Some(OoxmlKind::Excel) => {
            (name.starts_with("xl/worksheets/") && name.ends_with(".xml"))
                || name == "xl/sharedStrings.xml"
                || (include_style && name == "xl/styles.xml")
        }
        Some(OoxmlKind::PowerPoint) => {
            (name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
                || (include_style
                    && name.starts_with("ppt/slideLayouts/")
                    && name.ends_with(".xml"))
        }
        None => false,
    }
}

/// XML を正規化する。rsid 系属性と XML 宣言を除去し、空白のみのテキストを畳む。
fn normalize_xml(xml: &[u8]) -> std::result::Result<String, quick_xml::Error> {
    let mut reader = Reader::from_reader(xml);
    let config = reader.config_mut();
    config.trim_text(false);

    let mut writer = Writer::new(Vec::new());
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,
            // XML 宣言は除外（保存ツール差を吸収）。
            Event::Decl(_) => {}
            Event::Start(e) => writer.write_event(Event::Start(strip_rsid(&e)?))?,
            Event::Empty(e) => writer.write_event(Event::Empty(strip_rsid(&e)?))?,
            // 空白のみのテキスト（インデント）は無視し、実コンテンツのみ保持する。
            Event::Text(t) => {
                if !t.iter().all(|b| b.is_ascii_whitespace()) {
                    writer.write_event(Event::Text(t))?;
                }
            }
            other => writer.write_event(other)?,
        }
        buf.clear();
    }
    let bytes = writer.into_inner();
    String::from_utf8(bytes).map_err(|e| quick_xml::Error::Encoding(e.utf8_error().into()))
}

/// 開始/空要素から rsid 系属性（ローカル名が `rsid` で始まる）を除去した複製を作る。
fn strip_rsid<'a>(e: &BytesStart<'a>) -> std::result::Result<BytesStart<'a>, quick_xml::Error> {
    let name = e.name();
    let name_str = String::from_utf8_lossy(name.as_ref()).into_owned();
    let mut out = BytesStart::new(name_str);
    for attr in e.attributes() {
        let attr = attr.map_err(quick_xml::Error::from)?;
        if attr.key.local_name().as_ref().starts_with(b"rsid") {
            continue;
        }
        out.push_attribute(attr);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    /// 名前→内容のエントリ群から ZIP（.docx 等）バイト列を構築する。
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

    const DOC_RSID_A: &str = r#"<?xml version="1.0"?><w:document xmlns:w="x"><w:p w:rsidR="00AB" w:rsidRPr="00CD"><w:r><w:t>Hello</w:t></w:r></w:p></w:document>"#;
    const DOC_RSID_B: &str = r#"<?xml version="1.0" standalone="yes"?><w:document xmlns:w="x"><w:p w:rsidR="9999" w:rsidRDefault="1234"><w:r><w:t>Hello</w:t></w:r></w:p></w:document>"#;

    #[test]
    fn docx_same_content_different_rsid_normalizes_equal() {
        let a = build_zip(&[
            ("word/document.xml", DOC_RSID_A),
            ("docProps/core.xml", "<core>2020</core>"),
        ]);
        let b = build_zip(&[
            ("word/document.xml", DOC_RSID_B),
            ("docProps/core.xml", "<core>2024-different</core>"),
        ]);
        let na = extract(&a, OfficeSimilarityLevel::ContentOnly).unwrap();
        let nb = extract(&b, OfficeSimilarityLevel::ContentOnly).unwrap();
        assert_eq!(na, nb, "rsid と docProps の差異は無視されるべき");
        assert!(na.contains("Hello"));
    }

    #[test]
    fn docx_different_text_normalizes_differently() {
        let a = build_zip(&[("word/document.xml", DOC_RSID_A)]);
        let b = build_zip(&[(
            "word/document.xml",
            r#"<w:document xmlns:w="x"><w:p><w:r><w:t>Goodbye</w:t></w:r></w:p></w:document>"#,
        )]);
        let na = extract(&a, OfficeSimilarityLevel::ContentOnly).unwrap();
        let nb = extract(&b, OfficeSimilarityLevel::ContentOnly).unwrap();
        assert_ne!(na, nb);
    }

    #[test]
    fn include_style_picks_up_styles_xml() {
        let entries: &[(&str, &str)] = &[
            ("word/document.xml", DOC_RSID_A),
            ("word/styles.xml", "<styles><bold/></styles>"),
        ];
        let zip = build_zip(entries);
        let content_only = extract(&zip, OfficeSimilarityLevel::ContentOnly).unwrap();
        let include_style = extract(&zip, OfficeSimilarityLevel::IncludeStyle).unwrap();
        assert!(!content_only.contains("styles.xml"));
        assert!(include_style.contains("word/styles.xml"));
        assert!(include_style.contains("bold"));
    }

    #[test]
    fn xlsx_collects_worksheets_and_shared_strings() {
        let zip = build_zip(&[
            (
                "xl/worksheets/sheet1.xml",
                "<worksheet><c>1</c></worksheet>",
            ),
            ("xl/sharedStrings.xml", "<sst><si>hi</si></sst>"),
            ("xl/calcChain.xml", "<calcChain>VOLATILE</calcChain>"),
        ]);
        let out = extract(&zip, OfficeSimilarityLevel::ContentOnly).unwrap();
        assert!(out.contains("sheet1.xml"));
        assert!(out.contains("sharedStrings.xml"));
        assert!(!out.contains("calcChain"), "calcChain は揮発のため除外");
    }

    #[test]
    fn non_office_zip_yields_error() {
        let zip = build_zip(&[("random.txt", "not office")]);
        assert!(extract(&zip, OfficeSimilarityLevel::ContentOnly).is_err());
    }

    #[test]
    fn non_zip_bytes_yield_error() {
        assert!(extract(b"not a zip at all", OfficeSimilarityLevel::ContentOnly).is_err());
    }

    #[test]
    fn whitespace_indentation_is_ignored() {
        let pretty = build_zip(&[(
            "word/document.xml",
            "<w:document xmlns:w=\"x\">\n  <w:p>\n    <w:t>Hi</w:t>\n  </w:p>\n</w:document>",
        )]);
        let compact = build_zip(&[(
            "word/document.xml",
            "<w:document xmlns:w=\"x\"><w:p><w:t>Hi</w:t></w:p></w:document>",
        )]);
        let a = extract(&pretty, OfficeSimilarityLevel::ContentOnly).unwrap();
        let b = extract(&compact, OfficeSimilarityLevel::ContentOnly).unwrap();
        assert_eq!(a, b);
    }
}
