//! 選択ロジックのマイクロベンチ（仕様 §3.4）。
//!
//! 大量の重複グループに対する全選択の所要時間を測る。`key()`（パス小文字化）の
//! アロケーションと「1つ残す」判定パスの影響を観察する。
//!
//! 実行: `cargo bench -p shredy-core --bench select_bench`

use std::time::SystemTime;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use shredy_core::{DuplicateGroup, FileEntry, FileSize, KeepFileRule, LockSet, LongPath, Selector};

/// `groups` 個 × `per_group` 件の合成重複グループを作る。
fn make_groups(groups: usize, per_group: usize) -> Vec<DuplicateGroup> {
    (0..groups)
        .map(|g| DuplicateGroup {
            representative: "blake3:x".into(),
            files: (0..per_group)
                .map(|i| FileEntry {
                    path: LongPath::new(format!("C:\\dir{g}\\sub\\file_{g}_{i}.bin"))
                        .expect("パス生成に失敗"),
                    size: FileSize(1),
                    modified: SystemTime::UNIX_EPOCH,
                })
                .collect(),
        })
        .collect()
}

fn bench_select_all(c: &mut Criterion) {
    let groups = make_groups(2_000, 5);
    let locks = LockSet::new();

    let mut group = c.benchmark_group("select_all_2000x5");
    // ByDateNewest: keep 判定で key() を呼ばない（「1つ残す」二重パスの影響を分離）。
    group.bench_with_input(
        BenchmarkId::from_parameter("by_date_newest"),
        &KeepFileRule::ByDateNewest,
        |b, &rule| {
            b.iter(|| Selector::select_all(&groups, rule, &locks));
        },
    );
    // ByPathAsc: keep 判定が key()（パス小文字化）を要する。
    group.bench_with_input(
        BenchmarkId::from_parameter("by_path_asc"),
        &KeepFileRule::ByPathAsc,
        |b, &rule| {
            b.iter(|| Selector::select_all(&groups, rule, &locks));
        },
    );
    // ByPathLengthShortest: 長さ比較のためのキー生成コストを観察。
    group.bench_with_input(
        BenchmarkId::from_parameter("by_path_len_shortest"),
        &KeepFileRule::ByPathLengthShortest,
        |b, &rule| {
            b.iter(|| Selector::select_all(&groups, rule, &locks));
        },
    );
    group.finish();
}

criterion_group!(benches, bench_select_all);
criterion_main!(benches);
