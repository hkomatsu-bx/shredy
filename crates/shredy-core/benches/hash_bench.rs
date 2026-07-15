//! ハッシュ計算スループットのマイクロベンチ（仕様 §3.3 方式 0-2）。
//!
//! 比較方式 BLAKE3 / SHA-256 / XXH3-128 のファイル全体ハッシュ速度を計測する。
//! I/O 影響を排除するためインメモリの `MockFs` を用い、純粋なハッシュ処理を測る。
//!
//! 実行: `cargo bench -p shredy-core`

use std::time::SystemTime;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use shredy_core::compare::{Comparator, HashComparator};
use shredy_core::{FileEntry, FileSize, LongPath, Method, MockFs};

/// 指定サイズのインメモリファイルと、その `FileEntry` を用意する。
fn make_entry(size: usize) -> (MockFs, FileEntry) {
    let mut fs = MockFs::new();
    let path = LongPath::new("C:\\bench.bin").expect("パス生成に失敗");
    // 非圧縮ハッシュなので内容パターンは速度に影響しないが、ゼロ埋めを避けて擬似データを使う。
    let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    fs.add_file(&path, data);
    let entry = FileEntry {
        path,
        size: FileSize(size as u64),
        modified: SystemTime::UNIX_EPOCH,
    };
    (fs, entry)
}

fn bench_full_hash(c: &mut Criterion) {
    const SIZE: usize = 4 * 1024 * 1024; // 4 MiB
    let (fs, entry) = make_entry(SIZE);

    let mut group = c.benchmark_group("full_hash_4MiB");
    group.throughput(Throughput::Bytes(SIZE as u64));

    for (label, method) in [
        ("blake3", Method::Blake3),
        ("sha256", Method::Sha256),
        ("xxh3", Method::Xxh3),
    ] {
        let comparator = HashComparator::new(method);
        group.bench_with_input(BenchmarkId::from_parameter(label), &method, |b, _| {
            b.iter(|| comparator.full(&entry, &fs).expect("ハッシュ計算に失敗"));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_full_hash);
criterion_main!(benches);
