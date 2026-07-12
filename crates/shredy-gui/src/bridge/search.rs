//! コア検索エンジンの駆動（設計 §2.2）。
//!
//! Slint 非依存。借用エンジン (`SearchEngine<'a>`) はワーカースレッド内で構築する
//! 前提のため、ここでは「所有データから組み立てて実行する」純粋な関数のみを提供し、
//! スレッド生成・UI 反映（`upgrade_in_event_loop`）は `main.rs` が担う。

use std::sync::{Arc, RwLock};

use shredy_core::path::LongPath;
use shredy_core::{
    make_strategy, Cancel, CompareMethod, DuplicateGroup, FileEntry, FileReader, HashCache,
    ProgressTx, ScanConfig, SearchEngine,
};

/// 対象フォルダを走査して重複グループを返す（走査 → 比較）。
///
/// 中断時はその時点までに確定したグループを返す（コア準拠）。
/// 比較方式から戦略を構築できない場合は空を返す。
///
/// `cache` を渡すと内容ハッシュ方式（S0-2）へ注入され warm 再検索を高速化する
/// （他方式では無視される, M4a）。
#[must_use]
pub fn execute(
    fs: &dyn FileReader,
    cfg: &ScanConfig,
    method: CompareMethod,
    roots: &[LongPath],
    cache: Option<Arc<RwLock<HashCache>>>,
    progress: &ProgressTx,
    cancel: &Cancel,
) -> Vec<DuplicateGroup> {
    let Some(strategy) = make_strategy(method, cfg, cache) else {
        return Vec::new();
    };
    let engine = SearchEngine::with_strategy(fs, cfg, strategy);
    engine.search(roots, progress, cancel)
}

/// 収集済みエントリを比較・グルーピングする（走査を分離してテスト可能, コア委譲）。
#[must_use]
pub fn compare(
    fs: &dyn FileReader,
    cfg: &ScanConfig,
    method: CompareMethod,
    entries: Vec<FileEntry>,
    cache: Option<Arc<RwLock<HashCache>>>,
    progress: &ProgressTx,
    cancel: &Cancel,
) -> Vec<DuplicateGroup> {
    let Some(strategy) = make_strategy(method, cfg, cache) else {
        return Vec::new();
    };
    let engine = SearchEngine::with_strategy(fs, cfg, strategy);
    engine.compare_entries(entries, progress, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shredy_core::progress::new_cancel;
    use shredy_core::{FileSize, MockFs, ProgressEvent, ScanConfigBuilder};
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    fn entry(path: &LongPath, size: u64) -> FileEntry {
        FileEntry {
            path: path.clone(),
            size: FileSize(size),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn compare_groups_identical_content_via_mockfs() {
        let mut fs = MockFs::new();
        let a = LongPath::new("C:\\a").unwrap();
        let b = LongPath::new("C:\\b").unwrap();
        let c = LongPath::new("C:\\c").unwrap();
        fs.add_file(&a, b"same".to_vec());
        fs.add_file(&b, b"same".to_vec());
        fs.add_file(&c, b"diff".to_vec());
        let entries = vec![entry(&a, 4), entry(&b, 4), entry(&c, 4)];

        let cfg = ScanConfigBuilder::new().build();
        let (tx, rx) = channel();
        let cancel = new_cancel();
        let groups = compare(
            &fs,
            &cfg,
            CompareMethod::HashBlake3,
            entries,
            None,
            &tx,
            &cancel,
        );
        drop(tx);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
        // 進捗イベントが少なくとも 1 件は流れる（比較フェーズ）。
        assert!(rx.iter().any(|e| matches!(e, ProgressEvent::Phase(_))));
    }

    #[test]
    fn compare_with_cache_yields_same_groups_as_without() {
        use shredy_core::{HashCache, Method};
        use std::sync::{Arc, RwLock};

        let mut fs = MockFs::new();
        let a = LongPath::new("C:\\a").unwrap();
        let b = LongPath::new("C:\\b").unwrap();
        let c = LongPath::new("C:\\c").unwrap();
        fs.add_file(&a, b"same".to_vec());
        fs.add_file(&b, b"same".to_vec());
        fs.add_file(&c, b"diff".to_vec());
        let entries = vec![entry(&a, 4), entry(&b, 4), entry(&c, 4)];

        let cfg = ScanConfigBuilder::new().build();
        let cancel = new_cancel();
        let (tx0, _rx0) = channel();
        let without = compare(
            &fs,
            &cfg,
            CompareMethod::HashBlake3,
            entries.clone(),
            None,
            &tx0,
            &cancel,
        );

        // 同一入力をキャッシュ有りで実行 → 検出結果は不変（検出不変量）。
        let cache = Some(Arc::new(RwLock::new(HashCache::for_method(Method::Blake3))));
        let (tx1, _rx1) = channel();
        let with = compare(
            &fs,
            &cfg,
            CompareMethod::HashBlake3,
            entries,
            cache,
            &tx1,
            &cancel,
        );

        assert_eq!(without.len(), with.len());
        assert_eq!(without[0].files.len(), with[0].files.len());
    }

    #[test]
    fn compare_returns_empty_without_duplicates() {
        let mut fs = MockFs::new();
        let a = LongPath::new("C:\\a").unwrap();
        let b = LongPath::new("C:\\b").unwrap();
        fs.add_file(&a, b"aaaa".to_vec());
        fs.add_file(&b, b"bbbb".to_vec());
        let entries = vec![entry(&a, 4), entry(&b, 4)];

        let cfg = ScanConfigBuilder::new().build();
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        let groups = compare(
            &fs,
            &cfg,
            CompareMethod::HashBlake3,
            entries,
            None,
            &tx,
            &cancel,
        );
        assert!(groups.is_empty());
    }
}
