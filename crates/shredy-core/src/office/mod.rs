//! Office ファイル類似度比較（S7, 仕様 §3.11）。
//!
//! 可視コンテンツのみを正規化抽出し、メタデータ・リビジョン ID を無視して
//! ハッシュ完全一致でグルーピングする。新形式（.docx/.xlsx/.pptx）は ZIP+XML 正規化、
//! 旧形式（.doc/.xls/.ppt）は OLE Compound Document を解析する。段階的類似度は提供しない。

mod legacy;
mod ooxml;

use rayon::prelude::*;

use crate::config::OfficeSimilarityLevel;
use crate::error::{CoreError, Result};
use crate::group::group_by_hash;
use crate::io::FileReader;
use crate::model::{DuplicateGroup, FileEntry, FileHash};
#[cfg(test)]
use crate::path::LongPath;
use crate::progress::{is_cancelled, Cancel, Phase, ProgressEvent, ProgressTx};
use crate::strategy::GroupingStrategy;

/// 1 ファイルの読み込み上限（256 MiB）。超過は除外しメモリを保護する。
pub const OFFICE_READ_CAP: u64 = 256 * 1024 * 1024;

/// Office 類似度（S7）。可視コンテンツの正規化ハッシュで完全一致を束ねる。
pub struct OfficeSimilarityStrategy {
    level: OfficeSimilarityLevel,
}

impl OfficeSimilarityStrategy {
    /// 比較精度（ContentOnly / IncludeStyle）を指定して戦略を作る。
    #[must_use]
    pub fn new(level: OfficeSimilarityLevel) -> Self {
        Self { level }
    }
}

impl GroupingStrategy for OfficeSimilarityStrategy {
    fn label(&self) -> &'static str {
        "Office 類似度"
    }

    fn group(
        &self,
        entries: Vec<FileEntry>,
        fs: &dyn FileReader,
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<DuplicateGroup> {
        let _ = progress.send(ProgressEvent::Phase(Phase::Comparing));
        let level = self.level;
        let items: Vec<(FileEntry, FileHash)> = entries
            .par_iter()
            .filter_map(|e| {
                if is_cancelled(cancel) {
                    return None;
                }
                // Office 拡張子でない、または解析失敗は除外（走査層の通知とは別レイヤ）。
                // 「なぜ結果に出ないか」を追えるよう除外理由を debug ログに残す。
                let content = match fingerprint(e, fs, level) {
                    Ok(c) => c,
                    Err(err) => {
                        tracing::debug!(
                            path = %e.path.as_path().display(),
                            error = %err,
                            "Office 指紋抽出をスキップ"
                        );
                        return None;
                    }
                };
                Some((e.clone(), content))
            })
            .collect();
        group_by_hash(items, "office")
    }
}

/// 1 ファイルから正規化コンテンツを抽出し BLAKE3 ハッシュを返す。
fn fingerprint(
    entry: &FileEntry,
    fs: &dyn FileReader,
    level: OfficeSimilarityLevel,
) -> Result<FileHash> {
    let ext = entry
        .path
        .extension_lower()
        .ok_or_else(|| CoreError::Office("no extension".into()))?;
    let bytes = crate::io::read_all_capped(&entry.path, fs, OFFICE_READ_CAP)?;
    let content = extract_content(&ext, &bytes, level)?;
    let hash = blake3::hash(content.as_bytes());
    Ok(FileHash::B256(*hash.as_bytes()))
}

/// 拡張子に応じて正規化コンテンツ文字列を抽出する。非 Office 拡張子は `Err`。
fn extract_content(ext: &str, bytes: &[u8], level: OfficeSimilarityLevel) -> Result<String> {
    match ext {
        "docx" | "xlsx" | "pptx" => ooxml::extract(bytes, level),
        "xls" => legacy::extract_xls(bytes).map(|s| normalize_whitespace(&s)),
        "doc" => legacy::extract_doc(bytes).map(|s| normalize_whitespace(&s)),
        "ppt" => legacy::extract_ppt(bytes).map(|s| normalize_whitespace(&s)),
        other => Err(CoreError::Office(format!("unsupported extension: {other}"))),
    }
}

/// 連続する空白・改行・タブを単一スペースへ畳み前後をトリムする（仕様 §3.11 NormalizeWhitespace）。
fn normalize_whitespace(s: &str) -> String {
    // 中間 `Vec<&str>` を確保せず、1 本の String へ語を直接連結する。
    let mut out = String::with_capacity(s.len());
    for word in s.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScanConfigBuilder;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::progress::new_cancel;
    use std::io::{Cursor, Write};
    use std::sync::mpsc::channel;
    use std::time::SystemTime;
    use zip::write::SimpleFileOptions;

    fn build_docx(text: &str, rsid: &str) -> Vec<u8> {
        let xml = format!(
            r#"<w:document xmlns:w="x"><w:p w:rsidR="{rsid}"><w:r><w:t>{text}</w:t></w:r></w:p></w:document>"#
        );
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            zw.start_file("word/document.xml", opts).unwrap();
            zw.write_all(xml.as_bytes()).unwrap();
            zw.start_file("docProps/core.xml", opts).unwrap();
            zw.write_all(format!("<core>{rsid}</core>").as_bytes())
                .unwrap();
            zw.finish().unwrap();
        }
        buf
    }

    fn entry(fs: &mut MockFs, path: &str, content: Vec<u8>) -> FileEntry {
        let lp = LongPath::new(path).unwrap();
        let len = content.len() as u64;
        fs.add_file(&lp, content);
        FileEntry {
            path: lp,
            size: FileSize(len),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    fn run(
        strategy: &dyn GroupingStrategy,
        fs: &MockFs,
        entries: Vec<FileEntry>,
    ) -> Vec<DuplicateGroup> {
        let (tx, _rx) = channel();
        strategy.group(entries, fs, &tx, &new_cancel())
    }

    #[test]
    fn normalize_whitespace_collapses_runs() {
        assert_eq!(normalize_whitespace("  a\t\n b  \r\n c "), "a b c");
    }

    #[test]
    fn docx_with_different_rsid_group_together() {
        let mut fs = MockFs::new();
        let a = entry(
            &mut fs,
            "C:\\a.docx",
            build_docx("Quarterly Report", "00AB"),
        );
        let b = entry(
            &mut fs,
            "C:\\b.docx",
            build_docx("Quarterly Report", "FF99"),
        );
        let c = entry(
            &mut fs,
            "C:\\c.docx",
            build_docx("Different Content", "00AB"),
        );
        let s = OfficeSimilarityStrategy::new(OfficeSimilarityLevel::ContentOnly);
        let groups = run(&s, &fs, vec![a, b, c]);
        assert_eq!(groups.len(), 1, "rsid 違いの同一内容のみ束なる");
        assert_eq!(groups[0].files.len(), 2);
        assert!(groups[0].representative.starts_with("office:"));
    }

    #[test]
    fn non_office_files_are_skipped() {
        let mut fs = MockFs::new();
        let a = entry(&mut fs, "C:\\a.txt", b"hello".to_vec());
        let b = entry(&mut fs, "C:\\b.txt", b"hello".to_vec());
        let s = OfficeSimilarityStrategy::new(OfficeSimilarityLevel::ContentOnly);
        assert!(run(&s, &fs, vec![a, b]).is_empty());
    }

    #[test]
    fn corrupt_docx_is_skipped_not_panicked() {
        let mut fs = MockFs::new();
        let a = entry(&mut fs, "C:\\a.docx", b"not really a zip".to_vec());
        let b = entry(&mut fs, "C:\\b.docx", b"not really a zip".to_vec());
        let s = OfficeSimilarityStrategy::new(OfficeSimilarityLevel::ContentOnly);
        assert!(run(&s, &fs, vec![a, b]).is_empty());
    }

    #[test]
    fn extract_content_routes_by_extension() {
        let docx = build_docx("hi", "0001");
        assert!(extract_content("docx", &docx, OfficeSimilarityLevel::ContentOnly).is_ok());
        assert!(extract_content("png", &docx, OfficeSimilarityLevel::ContentOnly).is_err());
    }

    #[test]
    fn make_strategy_now_supports_office() {
        let cfg = ScanConfigBuilder::new().build();
        assert!(crate::strategy::make_strategy(
            crate::config::CompareMethod::OfficeSimilarity,
            &cfg,
            None
        )
        .is_some());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::progress::new_cancel;
    use proptest::prelude::*;
    use std::collections::BTreeSet;
    use std::io::{Cursor, Write};
    use std::sync::mpsc::channel;
    use std::time::SystemTime;
    use zip::write::SimpleFileOptions;

    fn docx_with_rsid(text: &str, rsid: &str) -> Vec<u8> {
        let xml = format!(
            r#"<w:document xmlns:w="x"><w:p w:rsidR="{rsid}"><w:r><w:t>{text}</w:t></w:r></w:p></w:document>"#
        );
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            zw.start_file("word/document.xml", SimpleFileOptions::default())
                .unwrap();
            zw.write_all(xml.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        buf
    }

    proptest! {
        // PBT-U9: 同一可視テキストで rsid のみ異なる .docx 群は、テキストごとに
        // 1 グループへ束なる（rsid は無視される）。
        #[test]
        fn same_text_varied_rsid_groups_by_text(
            texts in proptest::collection::vec("[A-C]{1,4}", 0..6),
            rsids in proptest::collection::vec("[0-9A-F]{4}", 0..6),
        ) {
            let mut fs = MockFs::new();
            let entries: Vec<FileEntry> = texts
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let rsid = rsids.get(i).map(String::as_str).unwrap_or("0000");
                    let lp = LongPath::new(format!("C:\\f{i}.docx")).unwrap();
                    fs.add_file(&lp, docx_with_rsid(t, rsid));
                    FileEntry { path: lp, size: FileSize(1), modified: SystemTime::UNIX_EPOCH }
                })
                .collect();

            // オラクル: テキスト値でまとめ 2 件以上のみ。
            let mut by_text: std::collections::HashMap<String, BTreeSet<String>> =
                std::collections::HashMap::new();
            for (i, t) in texts.iter().enumerate() {
                by_text.entry(t.clone()).or_default().insert(format!("c:\\f{i}.docx"));
            }
            let oracle: BTreeSet<BTreeSet<String>> =
                by_text.into_values().filter(|s| s.len() >= 2).collect();

            let s = OfficeSimilarityStrategy::new(OfficeSimilarityLevel::ContentOnly);
            let (tx, _rx) = channel();
            let groups = s.group(entries, &fs, &tx, &new_cancel());
            let got: BTreeSet<BTreeSet<String>> = groups
                .into_iter()
                .map(|g| g.files.iter()
                    .map(|f| f.path.as_path().to_string_lossy().to_lowercase())
                    .collect())
                .collect();
            prop_assert_eq!(got, oracle);
        }
    }
}
