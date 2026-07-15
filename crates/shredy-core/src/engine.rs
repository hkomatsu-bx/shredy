//! 検索エンジン（仕様 §3.2 の 3 フェーズ・オーケストレーション）。
//!
//! 読み込み（走査）→ [`GroupingStrategy`] によるグルーピング。比較方式ごとの違い
//! （内容ハッシュ / 部分一致 / メタデータ）は戦略側に閉じ込め、エンジンは走査と
//! 戦略の橋渡しに徹する。

use crate::config::ScanConfig;
use crate::io::FileReader;
use crate::model::{DuplicateGroup, FileEntry, Method};
use crate::path::LongPath;
use crate::progress::{Cancel, ProgressTx};
use crate::scan::{Scanner, detect_nas_majority};
use crate::strategy::{ContentHashStrategy, GroupingStrategy};

/// NAS（対象の過半が UNC）検出時に自動適用する I/O 並列度の上限（NFR-6）。
const NAS_IO_PARALLELISM_CAP: usize = 8;

/// 検索エンジン。走査とグルーピング戦略を束ねる。
pub struct SearchEngine<'a> {
    fs: &'a dyn FileReader,
    cfg: &'a ScanConfig,
    strategy: Box<dyn GroupingStrategy>,
}

impl<'a> SearchEngine<'a> {
    /// ハッシュ比較方式（0-2）でエンジンを作る（既存 API 互換）。
    #[must_use]
    pub fn new(fs: &'a dyn FileReader, cfg: &'a ScanConfig, method: Method) -> Self {
        Self::with_strategy(fs, cfg, Box::new(ContentHashStrategy::new(method)))
    }

    /// 任意のグルーピング戦略でエンジンを作る（S3-S6 等）。
    #[must_use]
    pub fn with_strategy(
        fs: &'a dyn FileReader,
        cfg: &'a ScanConfig,
        strategy: Box<dyn GroupingStrategy>,
    ) -> Self {
        Self { fs, cfg, strategy }
    }

    /// 対象フォルダを走査して重複グループを返す（仕様 §3.2 の全フェーズ）。
    ///
    /// 中断時はその時点までに確定したグループを返す。
    pub fn search(
        &self,
        roots: &[crate::path::LongPath],
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<DuplicateGroup> {
        let started = std::time::Instant::now();
        self.log_search_start(roots);
        let scanner = Scanner::new(self.cfg);
        // 走査完了（件数・スキップ・所要時間）は scan.rs が「走査完了」ログを出すためここでは出さない。
        let entries = scanner.walk(roots, progress, cancel);
        // I/O 並列度: 明示指定があれば尊重、無指定で対象の過半が UNC(NAS) なら自動制限する
        // （NFR-6: NAS 過負荷の回避）。それ以外は rayon 既定（全コア）で比較する。
        let grouping_started = std::time::Instant::now();
        let groups = match self.effective_io_parallelism(roots) {
            Some(n) => match rayon::ThreadPoolBuilder::new().num_threads(n).build() {
                Ok(pool) => {
                    tracing::info!(io_parallelism = n, "I/O 並列度を制限して比較します");
                    pool.install(|| self.compare_entries(entries, progress, cancel))
                }
                Err(e) => {
                    tracing::warn!(error = %e, "I/O 並列度プールの構築に失敗、既定並列度で続行");
                    self.compare_entries(entries, progress, cancel)
                }
            },
            None => self.compare_entries(entries, progress, cancel),
        };
        let dup_files: usize = groups.iter().map(|g| g.files.len()).sum();
        tracing::info!(
            grouping_ms = grouping_started.elapsed().as_millis() as u64,
            "グルーピングが完了しました"
        );
        tracing::info!(
            groups = groups.len(),
            files = dup_files,
            total_ms = started.elapsed().as_millis() as u64,
            "重複検索が完了しました"
        );
        groups
    }

    /// 検索開始時に対象フォルダと主要オプション・比較方式をログへ記録する（仕様 §5.5）。
    fn log_search_start(&self, roots: &[LongPath]) {
        let folders: Vec<String> = roots
            .iter()
            .map(|r| r.as_path().to_string_lossy().into_owned())
            .collect();
        let c = self.cfg;
        tracing::info!(
            method = self.strategy.label(),
            folders = ?folders,
            min_size = ?c.min_size,
            max_size = ?c.max_size,
            include_extensions = ?c.include_extensions,
            exclude_patterns = ?c.exclude_patterns,
            exclude_system_folders = c.exclude_system_folders,
            exclude_zero_byte = c.exclude_zero_byte,
            follow_symlinks = c.follow_symlinks,
            io_parallelism = ?c.io_parallelism,
            office_similarity_level = ?c.office_similarity_level,
            image_similarity_threshold = c.image_similarity_threshold,
            "重複検索を開始します"
        );
    }

    /// 実効 I/O 並列度を決める（`None` は rayon 既定＝全コア）。
    ///
    /// 明示指定（`--io-parallelism`）があれば最小 1 として尊重し、無指定でも対象の
    /// 過半が UNC（NAS）なら [`NAS_IO_PARALLELISM_CAP`] へ自動制限する（仕様 NFR-6）。
    fn effective_io_parallelism(&self, roots: &[LongPath]) -> Option<usize> {
        if let Some(n) = self.cfg.io_parallelism {
            return Some(n.max(1));
        }
        if detect_nas_majority(roots) {
            return Some(NAS_IO_PARALLELISM_CAP);
        }
        None
    }

    /// 収集済みファイル一覧を戦略でグルーピングする（走査と分離してテスト可能）。
    pub fn compare_entries(
        &self,
        entries: Vec<FileEntry>,
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<DuplicateGroup> {
        self.strategy.group(entries, self.fs, progress, cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScanConfigBuilder;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::path::LongPath;
    use crate::progress::new_cancel;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    fn build(fs: &mut MockFs, specs: &[(&str, &[u8])]) -> Vec<FileEntry> {
        specs
            .iter()
            .map(|(path, content)| {
                let lp = LongPath::new(*path).unwrap();
                fs.add_file(&lp, content.to_vec());
                FileEntry {
                    path: lp,
                    size: FileSize(content.len() as u64),
                    modified: SystemTime::UNIX_EPOCH,
                }
            })
            .collect()
    }

    fn run(entries: Vec<FileEntry>, fs: &MockFs) -> Vec<DuplicateGroup> {
        let cfg = ScanConfigBuilder::new().build();
        let engine = SearchEngine::new(fs, &cfg, Method::Blake3);
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        engine.compare_entries(entries, &tx, &cancel)
    }

    #[test]
    fn detects_identical_files_as_one_group() {
        let mut fs = MockFs::new();
        let entries = build(
            &mut fs,
            &[("C:\\a", b"same"), ("C:\\b", b"same"), ("C:\\c", b"diff!")],
        );
        let groups = run(entries, &fs);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn same_size_different_content_not_grouped() {
        let mut fs = MockFs::new();
        // 同一サイズ (4) だが内容が異なる → グループ化されない。
        let entries = build(&mut fs, &[("C:\\a", b"aaaa"), ("C:\\b", b"bbbb")]);
        let groups = run(entries, &fs);
        assert!(groups.is_empty());
    }

    #[test]
    fn explicit_io_parallelism_is_honored() {
        let cfg = ScanConfigBuilder::new().io_parallelism(Some(3)).build();
        let fs = MockFs::new();
        let engine = SearchEngine::new(&fs, &cfg, Method::Blake3);
        assert_eq!(engine.effective_io_parallelism(&[]), Some(3));
    }

    #[test]
    fn nas_majority_auto_limits_when_unset() {
        let cfg = ScanConfigBuilder::new().build();
        let fs = MockFs::new();
        let engine = SearchEngine::new(&fs, &cfg, Method::Blake3);
        let nas = vec![
            LongPath::new("\\\\srv\\a").unwrap(),
            LongPath::new("\\\\srv\\b").unwrap(),
            LongPath::new("C:\\x").unwrap(),
        ];
        assert_eq!(engine.effective_io_parallelism(&nas), Some(8));
        // ローカルのみなら制限なし。
        assert_eq!(
            engine.effective_io_parallelism(&[LongPath::new("C:\\x").unwrap()]),
            None
        );
    }

    #[test]
    fn no_duplicates_yields_empty() {
        let mut fs = MockFs::new();
        let entries = build(
            &mut fs,
            &[("C:\\a", b"1"), ("C:\\b", b"22"), ("C:\\c", b"333")],
        );
        assert!(run(entries, &fs).is_empty());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use crate::config::ScanConfigBuilder;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::path::LongPath;
    use crate::progress::new_cancel;
    use proptest::prelude::*;
    use std::collections::BTreeSet;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    /// オラクル: 内容が完全一致するファイル群（2 件以上）を重複とみなす素朴な実装。
    fn oracle(specs: &[(String, Vec<u8>)]) -> BTreeSet<BTreeSet<String>> {
        let mut by_content: std::collections::HashMap<Vec<u8>, BTreeSet<String>> =
            std::collections::HashMap::new();
        for (path, content) in specs {
            by_content
                .entry(content.clone())
                .or_default()
                .insert(path.to_lowercase());
        }
        by_content.into_values().filter(|s| s.len() >= 2).collect()
    }

    fn engine_grouping(specs: &[(String, Vec<u8>)]) -> BTreeSet<BTreeSet<String>> {
        let mut fs = MockFs::new();
        let entries: Vec<FileEntry> = specs
            .iter()
            .map(|(path, content)| {
                let lp = LongPath::new(path.as_str()).unwrap();
                fs.add_file(&lp, content.clone());
                FileEntry {
                    path: lp,
                    size: FileSize(content.len() as u64),
                    modified: SystemTime::UNIX_EPOCH,
                }
            })
            .collect();
        let cfg = ScanConfigBuilder::new().build();
        let engine = SearchEngine::new(&fs, &cfg, Method::Blake3);
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        engine
            .compare_entries(entries, &tx, &cancel)
            .iter()
            .map(|g| {
                g.files
                    .iter()
                    .map(|f| f.path.as_path().to_string_lossy().to_lowercase())
                    .collect::<BTreeSet<_>>()
            })
            .collect()
    }

    proptest! {
        // PBT-05 (oracle): 2 段階ハッシュ + サイズ枝刈りのグルーピングは、
        // 内容一致による素朴なグルーピングと一致する。
        #[test]
        fn two_stage_matches_naive_oracle(
            // 小さなアルファベットから内容を生成し衝突を起こしやすくする。
            contents in proptest::collection::vec(
                proptest::collection::vec(0u8..3, 0..6), 0..8)
        ) {
            let specs: Vec<(String, Vec<u8>)> = contents
                .into_iter()
                .enumerate()
                .map(|(i, c)| (format!("C:\\f{i}"), c))
                .collect();
            prop_assert_eq!(engine_grouping(&specs), oracle(&specs));
        }
    }
}
