//! 内容ハッシュ比較（S0-2）。サイズ別バケット → 先頭 64KB 部分ハッシュ → 全体ハッシュ。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use rayon::prelude::*;

use crate::cache::{CacheKey, HashCache};
use crate::compare::{Comparator, HashComparator, PARTIAL_LEN};
use crate::group::group_by_hash;
use crate::io::FileReader;
use crate::model::{DuplicateGroup, FileEntry, FileHash, Method};
use crate::progress::{Cancel, Phase, ProgressEvent, ProgressTx, is_cancelled};

use super::GroupingStrategy;

/// 内容ハッシュ比較（S0-2）。サイズ別バケット → 先頭 64KB 部分ハッシュ → 全体ハッシュ。
pub struct ContentHashStrategy {
    comparator: HashComparator,
    /// 永続ハッシュキャッシュ（任意）。命中時は再ハッシュを避ける（仕様 §5）。
    cache: Option<Arc<RwLock<HashCache>>>,
}

/// [`ContentHashStrategy::classify_by_cache`] のキャッシュ振り分け結果。
///
/// - `resolved`: 全体ハッシュ確定済み（PASS2 でさらに増える）
/// - `cached_parts` / `unprunable_sizes`: 全体命中分の枝刈り粒度（(size, partial) / size）
/// - `by_part`: 部分ハッシュ命中で (size, partial) ごとに束ねた候補
/// - `need_partial`: 全体・部分とも未命中で後段の並列部分ハッシュへ回す候補
/// - `partial_hits`: 部分ハッシュ命中件数（ログ用）
struct CacheClassification {
    resolved: Vec<(FileEntry, FileHash)>,
    cached_parts: HashSet<(u64, FileHash)>,
    unprunable_sizes: HashSet<u64>,
    by_part: HashMap<(u64, FileHash), Vec<FileEntry>>,
    need_partial: Vec<FileEntry>,
    partial_hits: u64,
}

impl ContentHashStrategy {
    /// 指定ハッシュ方式の戦略を作る。
    #[must_use]
    pub fn new(method: Method) -> Self {
        Self {
            comparator: HashComparator::new(method),
            cache: None,
        }
    }

    /// ハッシュキャッシュを注入する（命中で再読込・再ハッシュを省く）。
    #[must_use]
    pub fn with_cache(mut self, cache: Arc<RwLock<HashCache>>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// 全候補（サイズ衝突するファイル全部）を 2 つのグローバル並列パスで比較する。
    ///
    /// バケットごとの逐次処理をやめ、PASS1（部分ハッシュ）/ PASS2（全体ハッシュ）を
    /// それぞれ全候補にわたる単一の `rayon` パスにすることで、小バケットが多くても
    /// 全コアを使い切る。キャッシュ有無・cold/warm をこの 1 経路で扱う。
    fn compare_all(
        &self,
        candidates: Vec<FileEntry>,
        fs: &dyn FileReader,
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<DuplicateGroup> {
        let total = candidates.len() as u64;

        // Step 0: キャッシュを 1 度の read ロック・1 パスで引き、候補を 3 つに振り分ける。
        // - 全体ハッシュ命中 → resolved（確定）。後段の枝刈りのため、PARTIAL_LEN 超の命中は
        //   その部分ハッシュ（既知なら）を cached_parts に記録し (size, partial) 粒度で扱う。
        //   部分ハッシュ不明（旧形式キャッシュ等）の命中サイズは粒度を上げられないため
        //   unprunable_sizes に記録し、従来どおり size 粒度で枝刈り不可とする。
        // - 全体未命中だが部分ハッシュ命中（PARTIAL_LEN 超のみ）→ by_part へ直接（64KB 再読込を省く）。
        // - いずれも未命中 → need_partial（後段で並列に部分ハッシュ）。
        // 部分ハッシュキャッシュは記録側と対称に PARTIAL_LEN 超だけを参照する（PARTIAL_LEN 以下は
        // 部分＝全体として全体側に入り Step 0 の get で命中するため）。
        let CacheClassification {
            mut resolved,
            cached_parts,
            unprunable_sizes,
            mut by_part,
            need_partial,
            partial_hits,
        } = self.classify_by_cache(candidates);
        if is_cancelled(cancel) {
            return group_by_hash(resolved, self.comparator.label());
        }

        // 後段の debug! 用に命中数・未命中数を確定（resolved は PASS2 でさらに増える）。
        let cache_hits = resolved.len() as u64;
        let miss_count = partial_hits + need_partial.len() as u64;

        // PASS 1: 全体・部分とも未命中のものだけ並列で部分ハッシュし、PARTIAL_LEN 超を記録する。
        let computed = self.partial_pass(need_partial, fs, cancel);
        // キャンセルで computed が部分集合でも、各部分ハッシュ値は独立して正しいため記録してよい
        // （`full_pass` → `record_all` と対称）。
        self.record_partial_all(&computed);
        for (e, phash) in computed {
            by_part.entry((e.size.0, phash)).or_default().push(e);
        }

        // PASS2 対象を決める。サイズ ≤ 部分長なら部分 = 全体で即確定（再読込不要）。
        let mut need_full: Vec<FileEntry> = Vec::new();
        // サイズ ≤ 部分長で即確定する分。write ロックを 1 度だけ取って一括記録するため貯める。
        let mut small_resolved: Vec<(FileEntry, FileHash)> = Vec::new();
        for ((size, phash), bucket) in by_part {
            if size <= PARTIAL_LEN {
                // 部分ハッシュ = 全内容ハッシュ。全件確定（単独も含めキャッシュへ記録）。
                small_resolved.extend(bucket.into_iter().map(|e| (e, phash.clone())));
            } else {
                // 全体ハッシュが要るのは「重複の可能性が残る」場合のみ:
                // - 同 (size, partial) の未確定が 2 件以上 → 相互に重複し得る
                // - 同 (size, partial) のキャッシュ確定済みがある → それと重複し得る
                // - 同サイズに部分不明のキャッシュ確定済みがある → (size, partial) を比較できず保守的に確定
                // いずれでもなければ (size, partial) が一意＝重複なしなので破棄。
                let needs_full = bucket.len() >= 2
                    || unprunable_sizes.contains(&size)
                    || cached_parts.contains(&(size, phash));
                if needs_full {
                    need_full.extend(bucket);
                }
            }
        }
        // 1 ファイルずつ write ロックを取らず、1 回でまとめて記録する（`record_all` と対称）。
        self.record_all(&small_resolved);
        resolved.extend(small_resolved);
        if is_cancelled(cancel) {
            return group_by_hash(resolved, self.comparator.label());
        }

        tracing::debug!(
            method = self.comparator.label(),
            candidates = total,
            cache_hits,
            miss_count,
            partial_hits,
            need_full = need_full.len(),
            "内容ハッシュ比較"
        );

        // PASS 2: 全体ハッシュ要のものを一括並列で算出（命中時はキャッシュへ記録）。
        resolved.extend(self.full_pass(need_full, fs, cancel));

        let _ = progress.send(ProgressEvent::Progress {
            phase: Phase::Comparing,
            done: total,
            total,
        });
        // 同一全体ハッシュ＝同一内容（よって同一サイズ）。全体ハッシュで束ねれば十分。
        group_by_hash(resolved, self.comparator.label())
    }

    /// Step 0: キャッシュを 1 度の read ロック・1 パスで引き候補を振り分ける（`compare_all` の前段）。
    ///
    /// 全体命中 → resolved、部分のみ命中 → by_part 直行、未命中 → need_partial。詳細は
    /// [`CacheClassification`] と `compare_all` のコメントを参照。
    fn classify_by_cache(&self, candidates: Vec<FileEntry>) -> CacheClassification {
        let cap = candidates.len();
        let mut resolved: Vec<(FileEntry, FileHash)> = Vec::new();
        let mut cached_parts: HashSet<(u64, FileHash)> = HashSet::new();
        let mut unprunable_sizes: HashSet<u64> = HashSet::new();
        // 候補数を上限に異なる (size, partial) キーが現れる。再ハッシュを避け事前確保する。
        let mut by_part: HashMap<(u64, FileHash), Vec<FileEntry>> = HashMap::with_capacity(cap);
        let mut need_partial: Vec<FileEntry> = Vec::new();
        let mut partial_hits = 0u64;
        match &self.cache {
            Some(cache) => {
                let guard = cache.read().unwrap_or_else(|e| e.into_inner());
                for f in candidates {
                    let key = cache_key(&f);
                    if let Some(h) = guard.get(&key) {
                        let h = h.clone();
                        // 全体命中。PARTIAL_LEN 超なら (size, partial) で枝刈りできるよう
                        // 部分ハッシュを引く。不明なら size 粒度のフォールバックへ。
                        if f.size.0 > PARTIAL_LEN {
                            match guard.get_partial(&key) {
                                Some(p) => {
                                    cached_parts.insert((f.size.0, p.clone()));
                                }
                                None => {
                                    unprunable_sizes.insert(f.size.0);
                                }
                            }
                        }
                        resolved.push((f, h));
                    } else if f.size.0 > PARTIAL_LEN {
                        match guard.get_partial(&key) {
                            Some(h) => {
                                partial_hits += 1;
                                by_part.entry((f.size.0, h.clone())).or_default().push(f);
                            }
                            None => need_partial.push(f),
                        }
                    } else {
                        need_partial.push(f);
                    }
                }
            }
            None => need_partial = candidates,
        }
        CacheClassification {
            resolved,
            cached_parts,
            unprunable_sizes,
            by_part,
            need_partial,
            partial_hits,
        }
    }

    /// 候補を一括並列で部分ハッシュする（読み取り失敗は除外）。
    fn partial_pass(
        &self,
        files: Vec<FileEntry>,
        fs: &dyn FileReader,
        cancel: &Cancel,
    ) -> Vec<(FileEntry, FileHash)> {
        files
            .into_par_iter()
            .filter_map(|e| {
                if is_cancelled(cancel) {
                    return None;
                }
                let hash = self.comparator.partial(&e, fs).ok()?;
                Some((e, hash))
            })
            .collect()
    }

    /// 候補を一括並列で全体ハッシュする（読み取り失敗は除外）。キャッシュがあれば記録。
    ///
    /// 並列ハッシュ中は書き込みロックを取らず、collect 後に [`record_all`](Self::record_all)
    /// で 1 回だけまとめて記録する。これにより rayon ワーカー間の `RwLock` 書き込み競合
    /// （コア数に比例してシリアライズされスループットを下げる）を避ける。
    fn full_pass(
        &self,
        files: Vec<FileEntry>,
        fs: &dyn FileReader,
        cancel: &Cancel,
    ) -> Vec<(FileEntry, FileHash)> {
        let results: Vec<(FileEntry, FileHash)> = files
            .into_par_iter()
            .filter_map(|e| {
                if is_cancelled(cancel) {
                    return None;
                }
                let hash = self.comparator.full(&e, fs).ok()?;
                Some((e, hash))
            })
            .collect();
        self.record_all(&results);
        results
    }

    /// 全体ハッシュ結果をまとめてキャッシュへ記録する（書き込みロックは 1 回だけ取得）。
    fn record_all(&self, results: &[(FileEntry, FileHash)]) {
        if results.is_empty() {
            return; // 空なら write ロックを取らない。
        }
        if let Some(cache) = &self.cache {
            let mut guard = cache.write().unwrap_or_else(|e| e.into_inner());
            for (entry, hash) in results {
                guard.put(cache_key(entry), hash.clone());
            }
        }
    }

    /// 計算した部分ハッシュをまとめてキャッシュへ記録する（書き込みロックは 1 回だけ）。
    ///
    /// `PARTIAL_LEN` 以下のファイルは部分＝全体ハッシュとして全体側に記録され、次回は
    /// Step 0 で命中するため、部分ハッシュは `PARTIAL_LEN` 超のものだけ記録する。
    fn record_partial_all(&self, results: &[(FileEntry, FileHash)]) {
        if results.is_empty() {
            return; // 空なら write ロックを取らない。
        }
        if let Some(cache) = &self.cache {
            let mut guard = cache.write().unwrap_or_else(|e| e.into_inner());
            for (entry, hash) in results {
                if entry.size.0 > PARTIAL_LEN {
                    guard.put_partial(cache_key(entry), hash.clone());
                }
            }
        }
    }
}

/// `FileEntry` からキャッシュキーを作る。
fn cache_key(entry: &FileEntry) -> CacheKey {
    CacheKey::new(entry.path.as_path(), entry.size, entry.modified)
}

impl GroupingStrategy for ContentHashStrategy {
    fn label(&self) -> &'static str {
        match self.comparator.method() {
            Method::Blake3 => "ハッシュ (BLAKE3)",
            Method::Sha256 => "ハッシュ (SHA-256)",
            Method::Xxh3 => "ハッシュ (XXH3-128)",
        }
    }

    fn group(
        &self,
        entries: Vec<FileEntry>,
        fs: &dyn FileReader,
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<DuplicateGroup> {
        // フェーズ 2: ソート（サイズ別。同一サイズ 2 件以上のみが候補）。同サイズの
        // 候補を 1 本に集約し、比較フェーズで全コアを使い切れるようにする。
        let _ = progress.send(ProgressEvent::Phase(Phase::Sorting));
        let mut by_size: HashMap<u64, Vec<FileEntry>> = HashMap::new();
        for e in entries {
            by_size.entry(e.size.0).or_default().push(e);
        }
        let candidates: Vec<FileEntry> = by_size
            .into_values()
            .filter(|v| v.len() >= 2)
            .flatten()
            .collect();

        // フェーズ 3: 比較（全候補を 2 つのグローバル並列パスで処理）。
        let _ = progress.send(ProgressEvent::Phase(Phase::Comparing));
        self.compare_all(candidates, fs, progress, cancel)
    }
}
