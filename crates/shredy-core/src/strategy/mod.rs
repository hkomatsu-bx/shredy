//! 比較方式ごとのグルーピング戦略（仕様 §3.3）。
//!
//! [`GroupingStrategy`] で方式を差し替える。S0-2 は内容ハッシュ（サイズ別バケット →
//! 2 段階ハッシュ）、S3 は範囲ハッシュ（異サイズ間の一致を検出）、S4-6 はメタデータ
//! キーで束ねる（内容読み込みなし）。[`SearchEngine`](crate::engine::SearchEngine) は
//! 走査結果をこの戦略へ渡すだけの薄いオーケストレータになる。
//!
//! 各方式の実装は方式別サブモジュール（`content_hash`=S0-2、`partial_match`=S3、
//! `metadata`=S4-6）へ分離する。Office/画像類似度（S7-8）は [`crate::office`] /
//! [`crate::imgsim`] が [`GroupingStrategy`] を実装する。

use std::sync::{Arc, RwLock};

use crate::cache::HashCache;
use crate::config::{CompareMethod, ScanConfig};
use crate::io::FileReader;
use crate::model::{DuplicateGroup, FileEntry, Method};
use crate::progress::{Cancel, ProgressTx};

mod content_hash;
mod metadata;
mod partial_match;

pub use content_hash::ContentHashStrategy;
pub use metadata::{MetadataKind, MetadataStrategy};
pub use partial_match::{PARTIAL_READ_CAP, PartialMatchStrategy};

/// 候補ファイル群を重複グループへ束ねる戦略。
pub trait GroupingStrategy: Send + Sync {
    /// 走査済みファイル一覧をグルーピングする。中断時はその時点までを返す。
    fn group(
        &self,
        entries: Vec<FileEntry>,
        fs: &dyn FileReader,
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<DuplicateGroup>;

    /// 比較方式の表示名（ログ・診断用）。GUI の表示ラベルと一致させる。
    fn label(&self) -> &'static str;
}

/// 比較方式が内容ハッシュ（S0-2）なら対応する [`Method`] を返す。ハッシュキャッシュ
/// を使うのはこの 3 方式のみ（部分一致・メタデータ・類似度は対象外）。
#[must_use]
pub fn content_hash_method(method: CompareMethod) -> Option<Method> {
    match method {
        CompareMethod::HashBlake3 => Some(Method::Blake3),
        CompareMethod::HashSha256 => Some(Method::Sha256),
        CompareMethod::HashXxh128 => Some(Method::Xxh3),
        _ => None,
    }
}

/// `CompareMethod` から戦略を生成する。全 9 方式を実装済み（`Option` は将来の
/// 方式追加余地と呼び出し側 API 安定のために維持する）。
///
/// `cache` を渡すと内容ハッシュ方式（S0-2）に注入する。他方式では無視される。
#[must_use]
pub fn make_strategy(
    method: CompareMethod,
    cfg: &ScanConfig,
    cache: Option<Arc<RwLock<HashCache>>>,
) -> Option<Box<dyn GroupingStrategy>> {
    let content = |m: Method| -> Box<dyn GroupingStrategy> {
        let s = ContentHashStrategy::new(m);
        Box::new(match &cache {
            Some(c) => s.with_cache(c.clone()),
            None => s,
        })
    };
    let strategy: Box<dyn GroupingStrategy> = match method {
        CompareMethod::HashBlake3 => content(Method::Blake3),
        CompareMethod::HashSha256 => content(Method::Sha256),
        CompareMethod::HashXxh128 => content(Method::Xxh3),
        CompareMethod::PartialMatch => Box::new(PartialMatchStrategy::new(
            cfg.partial_match_offset,
            cfg.partial_match_size,
        )),
        CompareMethod::NameSizeDate => Box::new(MetadataStrategy::new(MetadataKind::NameSizeDate)),
        CompareMethod::NameSize => Box::new(MetadataStrategy::new(MetadataKind::NameSize)),
        CompareMethod::NameOnly => Box::new(MetadataStrategy::new(MetadataKind::NameOnly)),
        CompareMethod::OfficeSimilarity => Box::new(crate::office::OfficeSimilarityStrategy::new(
            cfg.office_similarity_level,
        )),
        CompareMethod::ImageSimilarity => Box::new(crate::imgsim::ImageSimilarityStrategy::new(
            cfg.image_similarity_threshold,
        )),
    };
    Some(strategy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::path::LongPath;
    use crate::progress::new_cancel;
    use std::sync::mpsc::channel;
    use std::time::{Duration, SystemTime};

    fn entry(path: &str, size: u64, modified: SystemTime) -> FileEntry {
        FileEntry {
            path: LongPath::new(path).unwrap(),
            size: FileSize(size),
            modified,
        }
    }

    fn run(
        strategy: &dyn GroupingStrategy,
        fs: &dyn FileReader,
        entries: Vec<FileEntry>,
    ) -> Vec<DuplicateGroup> {
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        strategy.group(entries, fs, &tx, &cancel)
    }

    #[test]
    fn make_strategy_maps_all_methods() {
        let cfg = crate::config::ScanConfigBuilder::new().build();
        assert!(make_strategy(CompareMethod::HashBlake3, &cfg, None).is_some());
        assert!(make_strategy(CompareMethod::PartialMatch, &cfg, None).is_some());
        assert!(make_strategy(CompareMethod::NameOnly, &cfg, None).is_some());
        assert!(make_strategy(CompareMethod::OfficeSimilarity, &cfg, None).is_some());
        assert!(make_strategy(CompareMethod::ImageSimilarity, &cfg, None).is_some());
    }

    #[test]
    fn metadata_name_only_groups_case_insensitively() {
        let fs = MockFs::new();
        let t = SystemTime::UNIX_EPOCH;
        let entries = vec![
            entry("C:\\x\\Report.TXT", 10, t),
            entry("C:\\y\\report.txt", 999, t), // サイズ違いでも名前一致で束なる
            entry("C:\\z\\other.txt", 10, t),
        ];
        let s = MetadataStrategy::new(MetadataKind::NameOnly);
        let groups = run(&s, &fs, entries);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn metadata_name_size_date_distinguishes_by_date() {
        let fs = MockFs::new();
        let t0 = SystemTime::UNIX_EPOCH;
        let t1 = SystemTime::UNIX_EPOCH + Duration::from_secs(3600);
        // 同名同サイズだが日付違い。
        let a = entry("C:\\a\\f.bin", 100, t0);
        let b = entry("C:\\b\\f.bin", 100, t1);
        // S4（日付込み）→ 束ならない。
        let s4 = MetadataStrategy::new(MetadataKind::NameSizeDate);
        assert!(run(&s4, &fs, vec![a.clone(), b.clone()]).is_empty());
        // S5（日付無視）→ 束なる。
        let s5 = MetadataStrategy::new(MetadataKind::NameSize);
        let g = run(&s5, &fs, vec![a, b]);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].files.len(), 2);
    }

    #[test]
    fn partial_match_groups_same_window_across_sizes() {
        let mut fs = MockFs::new();
        let t = SystemTime::UNIX_EPOCH;
        // 先頭 4 バイトが同一だが全体サイズが異なる 2 ファイル。
        let a = entry("C:\\a", 8, t);
        let b = entry("C:\\b", 12, t);
        let c = entry("C:\\c", 8, t);
        fs.add_file(&a.path, b"ABCD0000".to_vec());
        fs.add_file(&b.path, b"ABCD11112222".to_vec());
        fs.add_file(&c.path, b"ZZZZ0000".to_vec());
        // offset=0, size=4 の窓で a と b が一致。
        let s = PartialMatchStrategy::new(0, 4);
        let groups = run(&s, &fs, vec![a, b, c]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
        assert!(groups[0].representative.starts_with("partial-xxh3:"));
    }

    #[test]
    fn partial_match_with_offset_compares_inner_range() {
        let mut fs = MockFs::new();
        let t = SystemTime::UNIX_EPOCH;
        let a = entry("C:\\a", 8, t);
        let b = entry("C:\\b", 8, t);
        // 先頭は違うが offset=4 以降が同一。
        fs.add_file(&a.path, b"0000SAME".to_vec());
        fs.add_file(&b.path, b"1111SAME".to_vec());
        let s = PartialMatchStrategy::new(4, 4);
        let groups = run(&s, &fs, vec![a, b]);
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn partial_match_size_zero_uses_min_size_window() {
        let mut fs = MockFs::new();
        let t = SystemTime::UNIX_EPOCH;
        let a = entry("C:\\a", 6, t);
        let b = entry("C:\\b", 9, t);
        fs.add_file(&a.path, b"ABCDEF".to_vec());
        fs.add_file(&b.path, b"ABCDEFXYZ".to_vec());
        // size=0 → 最小サイズ 6 が窓長。先頭 6 バイト一致で束なる。
        let s = PartialMatchStrategy::new(0, 0);
        let groups = run(&s, &fs, vec![a, b]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn partial_match_excludes_files_smaller_than_offset() {
        let mut fs = MockFs::new();
        let t = SystemTime::UNIX_EPOCH;
        let a = entry("C:\\a", 2, t); // offset=4 を満たせない
        let b = entry("C:\\b", 2, t);
        fs.add_file(&a.path, b"AB".to_vec());
        fs.add_file(&b.path, b"AB".to_vec());
        let s = PartialMatchStrategy::new(4, 4);
        assert!(run(&s, &fs, vec![a, b]).is_empty());
    }

    // ───────────────── ハッシュキャッシュ（S0-2）─────────────────

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 読み込み回数を数える `FileReader` ラッパ（キャッシュ命中の検証用）。
    struct CountingFs {
        inner: MockFs,
        reads: AtomicUsize,
    }

    impl CountingFs {
        fn new(inner: MockFs) -> Self {
            Self {
                inner,
                reads: AtomicUsize::new(0),
            }
        }
        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }
    }

    impl FileReader for CountingFs {
        fn metadata(&self, p: &LongPath) -> crate::error::Result<crate::io::FileMeta> {
            self.inner.metadata(p)
        }
        fn read_prefix(&self, p: &LongPath, len: u64) -> crate::error::Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.inner.read_prefix(p, len)
        }
        fn read_range(&self, p: &LongPath, o: u64, l: u64) -> crate::error::Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.inner.read_range(p, o, l)
        }
        fn open_read(&self, p: &LongPath) -> crate::error::Result<Box<dyn std::io::Read + '_>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.inner.open_read(p)
        }
        fn is_reparse_point(&self, p: &LongPath) -> crate::error::Result<bool> {
            self.inner.is_reparse_point(p)
        }
        fn is_in_use(&self, p: &LongPath) -> crate::error::Result<bool> {
            self.inner.is_in_use(p)
        }
    }

    fn content_entry(fs: &mut MockFs, path: &str, content: &[u8]) -> FileEntry {
        let lp = LongPath::new(path).unwrap();
        fs.add_file(&lp, content.to_vec());
        FileEntry {
            path: lp,
            size: FileSize(content.len() as u64),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    /// グループを比較可能な正規形へ（代表＋ソート済みパス）。
    fn normalize(groups: &[DuplicateGroup]) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = groups
            .iter()
            .map(|g| {
                let mut paths: Vec<String> = g
                    .files
                    .iter()
                    .map(|f| f.path.as_path().to_string_lossy().into_owned())
                    .collect();
                paths.sort();
                (g.representative.clone(), paths)
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn warm_cache_avoids_rereading_files() {
        let mut mock = MockFs::new();
        let a = content_entry(&mut mock, "C:\\a", b"hello world");
        let b = content_entry(&mut mock, "C:\\b", b"hello world");
        let c = content_entry(&mut mock, "C:\\c", b"different!!");
        let fs = CountingFs::new(mock);
        let cache = Arc::new(RwLock::new(HashCache::for_method(Method::Blake3)));
        let s = ContentHashStrategy::new(Method::Blake3).with_cache(cache.clone());

        // cold: 読み込みが発生し、重複 1 グループ（a, b）。
        let cold = run(&s, &fs, vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(cold.len(), 1);
        assert_eq!(cold[0].files.len(), 2);
        assert!(fs.reads() > 0, "cold 実行で読み込みが発生していない");

        // warm: 同じ cache を再利用。読み込み 0 で結果不変。
        let before = fs.reads();
        let warm = run(&s, &fs, vec![a, b, c]);
        assert_eq!(fs.reads(), before, "warm 実行で再読込が発生した");
        assert_eq!(normalize(&cold), normalize(&warm));
    }

    #[test]
    fn warm_cache_skips_partial_reread_for_unique_large_files() {
        use crate::compare::PARTIAL_LEN;
        // PARTIAL_LEN 超・同サイズ・先頭から内容が異なる 2 ファイル（＝非重複だが候補）。
        let big = PARTIAL_LEN as usize + 1024;
        let mut mock = MockFs::new();
        let a = content_entry(&mut mock, "C:\\a.bin", &vec![b'a'; big]);
        let b = content_entry(&mut mock, "C:\\b.bin", &vec![b'b'; big]);
        let fs = CountingFs::new(mock);
        let cache = Arc::new(RwLock::new(HashCache::for_method(Method::Blake3)));
        let s = ContentHashStrategy::new(Method::Blake3).with_cache(cache.clone());

        // cold: 部分ハッシュの読み込みが発生し、重複なし。
        let cold = run(&s, &fs, vec![a.clone(), b.clone()]);
        assert!(cold.is_empty(), "非重複なのでグループ化されない");
        assert!(
            fs.reads() > 0,
            "cold で部分ハッシュ読み込みが発生していない"
        );

        // warm: 部分ハッシュキャッシュ命中で再読込ゼロ（最適化の検証）。
        let before = fs.reads();
        let warm = run(&s, &fs, vec![a, b]);
        assert_eq!(fs.reads(), before, "warm で部分ハッシュの再読込が発生した");
        assert!(warm.is_empty());
    }

    #[test]
    fn warm_cache_skips_full_read_for_new_size_colliding_unique_file() {
        use crate::compare::PARTIAL_LEN;
        // PARTIAL_LEN 超の重複ペア（a,b）をキャッシュ済みにし、warm でそれと「同サイズ・
        // 別内容」の新規ユニークファイル c を追加する。c は (size, partial) がペアと衝突
        // しないため、部分ハッシュ読み込み 1 回のみで一意と判定され、全体ハッシュの再読込
        // （旧 size 粒度ロジックでは発生していた）は起きないことを検証する。
        let big = PARTIAL_LEN as usize + 1024;
        let mut mock = MockFs::new();
        let a = content_entry(&mut mock, "C:\\a.bin", &vec![b'a'; big]);
        let b = content_entry(&mut mock, "C:\\b.bin", &vec![b'a'; big]); // a と同一内容＝重複
        let c = content_entry(&mut mock, "C:\\c.bin", &vec![b'c'; big]); // 同サイズ・別内容
        let fs = CountingFs::new(mock);
        let cache = Arc::new(RwLock::new(HashCache::for_method(Method::Blake3)));
        let s = ContentHashStrategy::new(Method::Blake3).with_cache(cache.clone());

        // cold: a,b で 1 グループ（c はまだ無し）。a,b の全体＋部分ハッシュがキャッシュされる。
        let cold = run(&s, &fs, vec![a.clone(), b.clone()]);
        assert_eq!(cold.len(), 1);

        // warm: c を追加。a,b は全体命中（読み込み 0）。c は部分読み込み 1 回のみで一意確定。
        let before = fs.reads();
        let warm = run(&s, &fs, vec![a, b, c]);
        assert_eq!(warm.len(), 1, "重複は a,b の 1 グループのまま（c は一意）");
        assert_eq!(warm[0].files.len(), 2);
        assert_eq!(
            fs.reads() - before,
            1,
            "c は部分読み込み 1 回のみ。全体ハッシュの再読込は起きない"
        );
    }

    #[test]
    fn cached_grouping_matches_uncached() {
        let mut mock = MockFs::new();
        // 大文字小文字や内容違いを混ぜる。サイズ衝突する別内容も含める。
        let specs: &[(&str, &[u8])] = &[
            ("C:\\a", b"AAAA"),
            ("C:\\b", b"AAAA"),
            ("C:\\c", b"BBBB"), // a,b と同サイズ別内容
            ("C:\\d", b"AAAA"),
            ("C:\\e", b"unique-content"),
        ];
        let entries: Vec<FileEntry> = specs
            .iter()
            .map(|(p, c)| content_entry(&mut mock, p, c))
            .collect();
        let fs = CountingFs::new(mock);

        let no_cache = ContentHashStrategy::new(Method::Xxh3);
        let g_uncached = run(&no_cache, &fs, entries.clone());

        let cache = Arc::new(RwLock::new(HashCache::for_method(Method::Xxh3)));
        let cached = ContentHashStrategy::new(Method::Xxh3).with_cache(cache.clone());
        let g_cold = run(&cached, &fs, entries.clone());
        let g_warm = run(&cached, &fs, entries);

        // キャッシュ無し・cold・warm の検出結果はすべて一致する。
        assert_eq!(normalize(&g_uncached), normalize(&g_cold));
        assert_eq!(normalize(&g_cold), normalize(&g_warm));
        // a, b, d が 1 グループ（3 件）。
        assert_eq!(g_warm.len(), 1);
        assert_eq!(g_warm[0].files.len(), 3);
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::path::LongPath;
    use crate::progress::new_cancel;
    use proptest::prelude::*;
    use std::collections::BTreeSet;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    /// グループ集合を「パス小文字集合の集合」へ正規化する。
    fn grouping(groups: Vec<DuplicateGroup>) -> BTreeSet<BTreeSet<String>> {
        groups
            .into_iter()
            .map(|g| {
                g.files
                    .iter()
                    .map(|f| f.path.as_path().to_string_lossy().to_lowercase())
                    .collect::<BTreeSet<_>>()
            })
            .collect()
    }

    fn run(
        strategy: &dyn GroupingStrategy,
        fs: &dyn FileReader,
        entries: Vec<FileEntry>,
    ) -> Vec<DuplicateGroup> {
        let (tx, _rx) = channel();
        strategy.group(entries, fs, &tx, &new_cancel())
    }

    proptest! {
        // PBT-U8a: 名前のみ (S6) のグルーピングは、小文字名で束ねる素朴な
        // オラクルと一致する（2 件以上のみ）。
        #[test]
        fn metadata_name_only_matches_oracle(
            names in proptest::collection::vec("[a-c]{1,3}", 0..8)
        ) {
            let fs = MockFs::new();
            let entries: Vec<FileEntry> = names
                .iter()
                .enumerate()
                .map(|(i, n)| FileEntry {
                    path: LongPath::new(format!("C:\\d{i}\\{n}.txt")).unwrap(),
                    size: FileSize(1),
                    modified: SystemTime::UNIX_EPOCH,
                })
                .collect();

            // オラクル: 小文字名でまとめ 2 件以上。
            let mut by_name: std::collections::HashMap<String, BTreeSet<String>> =
                std::collections::HashMap::new();
            for (i, n) in names.iter().enumerate() {
                let path = format!("c:\\d{i}\\{n}.txt");
                by_name.entry(n.to_lowercase()).or_default().insert(path);
            }
            let oracle: BTreeSet<BTreeSet<String>> =
                by_name.into_values().filter(|s| s.len() >= 2).collect();

            let s = MetadataStrategy::new(MetadataKind::NameOnly);
            prop_assert_eq!(grouping(run(&s, &fs, entries)), oracle);
        }

        // PBT-U8b: 部分一致 (S3, offset=0/size=4) は、先頭 4 バイト一致で
        // 束ねる素朴なオラクルと一致する。
        #[test]
        fn partial_match_matches_window_oracle(
            contents in proptest::collection::vec(
                proptest::collection::vec(0u8..3, 0..6), 0..8)
        ) {
            let mut fs = MockFs::new();
            let entries: Vec<FileEntry> = contents
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let lp = LongPath::new(format!("C:\\f{i}")).unwrap();
                    fs.add_file(&lp, c.clone());
                    FileEntry { path: lp, size: FileSize(c.len() as u64), modified: SystemTime::UNIX_EPOCH }
                })
                .collect();

            // オラクル: size>0 のみ対象、先頭 min(4,len) バイトをキーに束ねる。
            let mut by_window: std::collections::HashMap<Vec<u8>, BTreeSet<String>> =
                std::collections::HashMap::new();
            for (i, c) in contents.iter().enumerate() {
                if c.is_empty() {
                    continue; // size 0 < offset+1 → 対象外
                }
                let w = c[..c.len().min(4)].to_vec();
                by_window.entry(w).or_default().insert(format!("c:\\f{i}"));
            }
            let oracle: BTreeSet<BTreeSet<String>> =
                by_window.into_values().filter(|s| s.len() >= 2).collect();

            let s = PartialMatchStrategy::new(0, 4);
            prop_assert_eq!(grouping(run(&s, &fs, entries)), oracle);
        }
    }
}
