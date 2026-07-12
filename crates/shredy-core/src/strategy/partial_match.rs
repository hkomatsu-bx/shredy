//! 部分一致（S3）。範囲指定 XXH3-128 で異サイズ間の一致も検出する。

use rayon::prelude::*;

use crate::group::group_by_hash;
use crate::io::FileReader;
use crate::model::{DuplicateGroup, FileEntry, FileHash};
use crate::progress::{is_cancelled, Cancel, Phase, ProgressEvent, ProgressTx};

use super::GroupingStrategy;

/// 部分一致 (S3) の 1 ファイル読み込み上限（64 MiB, 仕様 §3.3）。
pub const PARTIAL_READ_CAP: u64 = 64 * 1024 * 1024;

/// 部分一致（S3）。`offset` から範囲を XXH3-128 でハッシュし、異サイズ間の一致も検出する。
pub struct PartialMatchStrategy {
    offset: u64,
    /// チェックサイズ。0 は「候補集合の最小ファイルサイズを共通窓長」を意味する。
    size: u64,
}

impl PartialMatchStrategy {
    /// 開始位置とチェックサイズから戦略を作る。
    #[must_use]
    pub fn new(offset: u64, size: u64) -> Self {
        Self { offset, size }
    }

    /// 実効窓長 `L` を決める（64 MiB 上限）。0 は対象なし。
    fn window_len(&self, targets: &[FileEntry]) -> u64 {
        if self.size > 0 {
            self.size.min(PARTIAL_READ_CAP)
        } else {
            let min_size = targets.iter().map(|e| e.size.0).min().unwrap_or(0);
            min_size.saturating_sub(self.offset).min(PARTIAL_READ_CAP)
        }
    }
}

impl GroupingStrategy for PartialMatchStrategy {
    fn label(&self) -> &'static str {
        "部分一致"
    }

    fn group(
        &self,
        entries: Vec<FileEntry>,
        fs: &dyn FileReader,
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<DuplicateGroup> {
        let _ = progress.send(ProgressEvent::Phase(Phase::Comparing));
        // offset を満たせる（範囲が取れる）ファイルのみ対象。
        let targets: Vec<FileEntry> = entries
            .into_iter()
            .filter(|e| e.size.0 > self.offset)
            .collect();
        if targets.len() < 2 {
            return Vec::new();
        }
        let window = self.window_len(&targets);
        if window == 0 {
            return Vec::new();
        }
        // 注: 窓より短いファイルは read_range が EOF で短く返すため、その短い領域が
        // 完全一致する者同士のみ束なる（長いファイルとは別ハッシュ）。これは意図した
        // 挙動で、窓未満を除外はしない（prop test partial_match_matches_window_oracle）。
        let items: Vec<(FileEntry, FileHash)> = targets
            .into_par_iter()
            .filter_map(|e| {
                if is_cancelled(cancel) {
                    return None;
                }
                let bytes = fs.read_range(&e.path, self.offset, window).ok()?;
                let mut hasher = xxhash_rust::xxh3::Xxh3::new();
                hasher.update(&bytes);
                let hash = FileHash::B128(hasher.digest128().to_le_bytes());
                Some((e, hash))
            })
            .collect();
        group_by_hash(items, "partial-xxh3")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::path::LongPath;
    use crate::progress::new_cancel;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

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

    fn run(s: &PartialMatchStrategy, fs: &MockFs, entries: Vec<FileEntry>) -> Vec<DuplicateGroup> {
        let (tx, _rx) = channel();
        s.group(entries, fs, &tx, &new_cancel())
    }

    #[test]
    fn short_files_group_by_truncated_window_separately_from_long() {
        // offset=0, size=100。窓未満の短いファイルは「短い領域が一致する者同士」で
        // 束なり、窓を満たす長いファイルとは別ハッシュになる（短く切り詰められるため）。
        let mut fs = MockFs::new();
        let prefix = vec![7u8; 100];
        let mut a = prefix.clone();
        a.extend_from_slice(&[1u8; 100]); // 200B, 先頭100=prefix
        let mut b = prefix.clone();
        b.extend_from_slice(&[2u8; 100]); // 200B, 先頭100=prefix
        let short1 = vec![7u8; 50]; // 窓(100)未満
        let short2 = vec![7u8; 50];

        let ea = entry(&mut fs, "C:\\a", a);
        let eb = entry(&mut fs, "C:\\b", b);
        let es1 = entry(&mut fs, "C:\\s1", short1);
        let es2 = entry(&mut fs, "C:\\s2", short2);

        let s = PartialMatchStrategy::new(0, 100);
        let groups = run(&s, &fs, vec![ea, eb, es1, es2]);
        // {a,b}（先頭100一致）と {s1,s2}（短い50が一致）の 2 グループ。
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|g| g.files.len() == 2));
    }

    #[test]
    fn default_window_uses_min_size_and_groups_shared_prefix() {
        // size=0（既定）。窓 = 最小サイズ。先頭が同一の異サイズ 2 件が束なる。
        let mut fs = MockFs::new();
        let mut a = vec![9u8; 80];
        a.extend_from_slice(&[1u8; 40]); // 120B
        let b = vec![9u8; 80]; // 80B, a の先頭80と一致
        let ea = entry(&mut fs, "C:\\a", a);
        let eb = entry(&mut fs, "C:\\b", b);
        let s = PartialMatchStrategy::new(0, 0);
        let groups = run(&s, &fs, vec![ea, eb]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
    }
}
