//! 内容ハッシュ比較（`compare_all`）のマイクロベンチ（仕様 §3.3）。
//!
//! `MockFs`（インメモリ）で I/O を排除し、同一サイズ・同一内容の候補群に対する
//! グルーピングの CPU コストを測る。特に warm（全件キャッシュ命中）は Step0 の
//! キャッシュキー生成（`CacheKey::new` の文字列確保）と HashMap 参照のコストを観察する。
//!
//! 実行: `cargo bench -p shredy-core --bench compare_bench`

use std::sync::mpsc::channel;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use shredy_core::progress::new_cancel;
use shredy_core::{
    ContentHashStrategy, FileEntry, FileSize, GroupingStrategy, HashCache, LongPath, Method,
};

/// 同一サイズ・同一内容の候補 `n` 件（全員が重複＝全件ハッシュ・記録される）。
fn make_candidates(n: usize, size: usize) -> (Vec<FileEntry>, Vec<u8>) {
    let data = vec![0xABu8; size];
    let entries = (0..n)
        .map(|i| FileEntry {
            path: LongPath::new(format!("C:\\dir\\sub\\dup_{i:06}.bin")).expect("パス生成"),
            size: FileSize(size as u64),
            modified: SystemTime::UNIX_EPOCH,
        })
        .collect();
    (entries, data)
}

fn build_fs(entries: &[FileEntry], data: &[u8]) -> shredy_core::MockFs {
    let mut fs = shredy_core::MockFs::new();
    for e in entries {
        fs.add_file(&e.path, data.to_vec());
    }
    fs
}

fn run_once(strategy: &dyn GroupingStrategy, fs: &shredy_core::MockFs, entries: Vec<FileEntry>) {
    let (tx, _rx) = channel();
    let _ = strategy.group(entries, fs, &tx, &new_cancel());
}

fn bench_compare_all(c: &mut Criterion) {
    const N: usize = 10_000;
    const SIZE: usize = 1024; // 部分長(64KiB)以下 → 部分=全体で確定する経路。
    let (entries, data) = make_candidates(N, SIZE);
    let fs = build_fs(&entries, &data);

    let mut group = c.benchmark_group("compare_all_10k_dup");

    // cold: 空キャッシュから（ハッシュ計算＋記録を含む）。
    group.bench_with_input(BenchmarkId::new("cold", N), &N, |b, _| {
        b.iter(|| {
            let cache = Arc::new(RwLock::new(HashCache::for_method(Method::Blake3)));
            let s = ContentHashStrategy::new(Method::Blake3).with_cache(cache);
            run_once(&s, &fs, entries.clone());
        });
    });

    // warm: 全件キャッシュ命中（Step0 のキー生成＋参照が支配的）。
    let warm_cache = Arc::new(RwLock::new(HashCache::for_method(Method::Blake3)));
    {
        let s = ContentHashStrategy::new(Method::Blake3).with_cache(Arc::clone(&warm_cache));
        run_once(&s, &fs, entries.clone()); // 1 回流してキャッシュを温める。
    }
    group.bench_with_input(BenchmarkId::new("warm", N), &N, |b, _| {
        b.iter(|| {
            let s = ContentHashStrategy::new(Method::Blake3).with_cache(Arc::clone(&warm_cache));
            run_once(&s, &fs, entries.clone());
        });
    });

    group.finish();
}

criterion_group!(benches, bench_compare_all);
criterion_main!(benches);
