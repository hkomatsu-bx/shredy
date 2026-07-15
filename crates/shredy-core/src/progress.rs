//! 進捗通知と中断（設計 Q5=A, ガイド §9）。
//!
//! コアは進捗を `mpsc` チャネルで送出し、中断は共有 `AtomicBool` を各ループ境界で
//! 確認する。GUI フロントエンドでもそのまま再利用できる。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

/// 検索の 3 フェーズ（仕様 §3.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Loading,
    Sorting,
    Comparing,
}

/// フロントエンドへ送る進捗イベント。
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// フェーズ遷移。
    Phase(Phase),
    /// フェーズ内の進捗（処理済み / 総数）。
    Progress { phase: Phase, done: u64, total: u64 },
    /// アクセス不可などでスキップしたファイル（仕様 §6）。
    Skipped(PathBuf),
}

/// 中断フラグ。フロントエンド→コアで共有する。
pub type Cancel = Arc<AtomicBool>;

/// 進捗送出チャネル。
pub type ProgressTx = Sender<ProgressEvent>;

/// 新しい中断フラグ（未中断）を作る。
#[must_use]
pub fn new_cancel() -> Cancel {
    Arc::new(AtomicBool::new(false))
}

/// 中断が要求されているか。
///
/// `Relaxed` で十分: 中断フラグは他のメモリ操作と順序付けする必要がなく、「いずれ
/// 検出できればよい」という単一フラグのため。多少の検出遅延（次のループ境界まで）は
/// 仕様上許容され、誤検出・見落としのいずれも起きない。
#[must_use]
pub fn is_cancelled(cancel: &Cancel) -> bool {
    cancel.load(Ordering::Relaxed)
}

/// 中断を要求する。順序保証は不要なため `Relaxed`（[`is_cancelled`] と対）。
pub fn request_cancel(cancel: &Cancel) {
    cancel.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_starts_unset_and_can_be_requested() {
        let c = new_cancel();
        assert!(!is_cancelled(&c));
        request_cancel(&c);
        assert!(is_cancelled(&c));
    }
}
