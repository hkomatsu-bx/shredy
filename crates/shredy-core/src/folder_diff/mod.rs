//! §3.7 フォルダ比較 — 2 フォルダを再帰走査し相対パス単位で差分を分類する。

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ─── 公開型 ──────────────────────────────────────────────────────────────────

/// 比較結果の分類。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffKind {
    /// 相対パスが A にのみ存在する。
    OnlyA,
    /// 相対パスが B にのみ存在する。
    OnlyB,
    /// 相対パスが両方に存在し、ファイルサイズが一致する。
    Common,
    /// 相対パスが両方に存在するが、ファイルサイズが異なる。
    SizeDiff,
}

/// 1 エントリ（相対パス単位の比較結果）。
#[derive(Debug, Clone)]
pub struct DiffEntry {
    /// ルートからの相対パス（`/` 区切りに正規化）。
    pub rel_path: String,
    /// 分類。
    pub kind: DiffKind,
    /// A 側のファイルサイズ。`OnlyB` のみ `None`。
    pub size_a: Option<u64>,
    /// B 側のファイルサイズ。`OnlyA` のみ `None`。
    pub size_b: Option<u64>,
}

/// 分類ごとの件数サマリ。
#[derive(Debug, Default, Clone)]
pub struct FolderDiffSummary {
    pub only_a: usize,
    pub only_b: usize,
    pub common: usize,
    pub size_diff: usize,
}

impl FolderDiffSummary {
    pub fn total(&self) -> usize {
        self.only_a + self.only_b + self.common + self.size_diff
    }
}

/// [`FolderDiffScanner::compare`] の返り値。
pub struct FolderDiffResult {
    /// 相対パス昇順でソートされた差分エントリ一覧。
    pub entries: Vec<DiffEntry>,
    /// 分類ごとの件数。
    pub summary: FolderDiffSummary,
}

// ─── スキャナー ───────────────────────────────────────────────────────────────

/// フォルダ比較スキャナー（ステートレス）。
pub struct FolderDiffScanner;

impl FolderDiffScanner {
    /// `folder_a` と `folder_b` を再帰走査して差分を返す。
    ///
    /// - ハッシュ比較は行わない（サイズのみ）。
    /// - 読み取り失敗エントリは `tracing::warn!` を出力してスキップ。
    /// - `cancel` が `true` になった時点でスキャンを中断し、中断時点の結果を返す。
    pub fn compare(folder_a: &Path, folder_b: &Path, cancel: &Arc<AtomicBool>) -> FolderDiffResult {
        let map_a = collect_files(folder_a, cancel);
        let map_b = collect_files(folder_b, cancel);

        let mut entries: Vec<DiffEntry> = Vec::with_capacity(map_a.len() + map_b.len());

        // A 側を走査して Common / SizeDiff / OnlyA を分類。
        for (rel, &size_a) in &map_a {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            match map_b.get(rel) {
                None => entries.push(DiffEntry {
                    rel_path: rel.clone(),
                    kind: DiffKind::OnlyA,
                    size_a: Some(size_a),
                    size_b: None,
                }),
                Some(&size_b) if size_a == size_b => entries.push(DiffEntry {
                    rel_path: rel.clone(),
                    kind: DiffKind::Common,
                    size_a: Some(size_a),
                    size_b: Some(size_b),
                }),
                Some(&size_b) => entries.push(DiffEntry {
                    rel_path: rel.clone(),
                    kind: DiffKind::SizeDiff,
                    size_a: Some(size_a),
                    size_b: Some(size_b),
                }),
            }
        }

        // B 側にのみ存在するエントリを追加。
        for (rel, &size_b) in &map_b {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if !map_a.contains_key(rel) {
                entries.push(DiffEntry {
                    rel_path: rel.clone(),
                    kind: DiffKind::OnlyB,
                    size_a: None,
                    size_b: Some(size_b),
                });
            }
        }

        // 相対パス昇順で安定ソート（一覧表示を一貫させる）。
        entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        let summary = build_summary(&entries);
        FolderDiffResult { entries, summary }
    }
}

// ─── 内部ヘルパー ─────────────────────────────────────────────────────────────

/// `root` 配下のファイルを再帰走査し、`HashMap<相対パス, サイズ>` を返す。
///
/// - シンボリックリンクは追跡しない（`follow_links(false)`）。
/// - 走査・メタデータ取得エラーは `warn!` してスキップ。
/// - 相対パスはスラッシュ区切りに正規化（`\` → `/`）。
fn collect_files(root: &Path, cancel: &Arc<AtomicBool>) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(target: "folder_diff", "walk error, skipping: {e}");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let size = match entry.metadata() {
            Ok(m) => m.len(),
            Err(e) => {
                tracing::warn!(target: "folder_diff", "metadata error, skipping {}: {e}", entry.path().display());
                continue;
            }
        };
        if let Ok(rel) = entry.path().strip_prefix(root) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            map.insert(rel_str, size);
        }
    }
    map
}

fn build_summary(entries: &[DiffEntry]) -> FolderDiffSummary {
    entries
        .iter()
        .fold(FolderDiffSummary::default(), |mut s, e| {
            match e.kind {
                DiffKind::OnlyA => s.only_a += 1,
                DiffKind::OnlyB => s.only_b += 1,
                DiffKind::Common => s.common += 1,
                DiffKind::SizeDiff => s.size_diff += 1,
            }
            s
        })
}

// ─── テスト ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn no_cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    // ── T1: 空フォルダ同士 ─────────────────────────────────────────────────
    #[test]
    fn empty_folders_yield_zero_entries() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        let result = FolderDiffScanner::compare(dir_a.path(), dir_b.path(), &no_cancel());
        assert_eq!(result.entries.len(), 0);
        assert_eq!(result.summary.total(), 0);
    }

    // ── T2: A のみにファイルあり ───────────────────────────────────────────
    #[test]
    fn file_only_in_a_classified_as_only_a() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        fs::write(dir_a.path().join("hello.txt"), b"hello").unwrap();

        let result = FolderDiffScanner::compare(dir_a.path(), dir_b.path(), &no_cancel());
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].kind, DiffKind::OnlyA);
        assert_eq!(result.entries[0].rel_path, "hello.txt");
        assert_eq!(result.entries[0].size_a, Some(5));
        assert_eq!(result.entries[0].size_b, None);
        assert_eq!(result.summary.only_a, 1);
    }

    // ── T3: B のみにファイルあり ───────────────────────────────────────────
    #[test]
    fn file_only_in_b_classified_as_only_b() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        fs::write(dir_b.path().join("world.txt"), b"world!").unwrap();

        let result = FolderDiffScanner::compare(dir_a.path(), dir_b.path(), &no_cancel());
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].kind, DiffKind::OnlyB);
        assert_eq!(result.entries[0].size_a, None);
        assert_eq!(result.entries[0].size_b, Some(6));
        assert_eq!(result.summary.only_b, 1);
    }

    // ── T4: 同名・同サイズ → Common ───────────────────────────────────────
    #[test]
    fn same_name_same_size_is_common() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        fs::write(dir_a.path().join("file.txt"), b"data").unwrap();
        fs::write(dir_b.path().join("file.txt"), b"data").unwrap();

        let result = FolderDiffScanner::compare(dir_a.path(), dir_b.path(), &no_cancel());
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].kind, DiffKind::Common);
        assert_eq!(result.summary.common, 1);
    }

    // ── T5: 同名・異サイズ → SizeDiff ─────────────────────────────────────
    #[test]
    fn same_name_different_size_is_size_diff() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        fs::write(dir_a.path().join("doc.txt"), b"short").unwrap();
        fs::write(dir_b.path().join("doc.txt"), b"much longer content").unwrap();

        let result = FolderDiffScanner::compare(dir_a.path(), dir_b.path(), &no_cancel());
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].kind, DiffKind::SizeDiff);
        assert_eq!(result.entries[0].size_a, Some(5));
        assert_eq!(result.entries[0].size_b, Some(19));
        assert_eq!(result.summary.size_diff, 1);
    }

    // ── T6: サブディレクトリ内ファイルの相対パス ──────────────────────────
    #[test]
    fn subdir_file_has_slash_separated_rel_path() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        let sub = dir_a.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("nested.txt"), b"nested").unwrap();

        let result = FolderDiffScanner::compare(dir_a.path(), dir_b.path(), &no_cancel());
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].rel_path, "sub/nested.txt");
        assert_eq!(result.entries[0].kind, DiffKind::OnlyA);
    }

    // ── T7: 全 4 種類混在 → summary.total() が正しい ─────────────────────
    #[test]
    fn mixed_entries_summary_is_correct() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();

        // OnlyA
        fs::write(dir_a.path().join("a_only.txt"), b"a").unwrap();
        // OnlyB
        fs::write(dir_b.path().join("b_only.txt"), b"bb").unwrap();
        // Common
        fs::write(dir_a.path().join("common.txt"), b"same").unwrap();
        fs::write(dir_b.path().join("common.txt"), b"same").unwrap();
        // SizeDiff
        fs::write(dir_a.path().join("diff.txt"), b"small").unwrap();
        fs::write(dir_b.path().join("diff.txt"), b"larger content").unwrap();

        let result = FolderDiffScanner::compare(dir_a.path(), dir_b.path(), &no_cancel());
        let s = &result.summary;
        assert_eq!(s.only_a, 1);
        assert_eq!(s.only_b, 1);
        assert_eq!(s.common, 1);
        assert_eq!(s.size_diff, 1);
        assert_eq!(s.total(), 4);
        assert_eq!(result.entries.len(), 4);
    }

    // ── T8: キャンセルで中断 ──────────────────────────────────────────────
    #[test]
    fn cancel_flag_stops_scan() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        for i in 0..100u32 {
            fs::write(dir_a.path().join(format!("file{i}.txt")), b"x").unwrap();
        }
        let cancel = Arc::new(AtomicBool::new(true)); // 最初から true
        let result = FolderDiffScanner::compare(dir_a.path(), dir_b.path(), &cancel);
        // キャンセル時でもパニックせず結果を返す（件数は不問）
        let _ = result.summary.total();
    }

    // ── T9: エントリが rel_path 昇順にソートされている ────────────────────
    #[test]
    fn entries_are_sorted_by_rel_path() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        for name in ["z.txt", "a.txt", "m.txt"] {
            fs::write(dir_a.path().join(name), b"x").unwrap();
        }

        let result = FolderDiffScanner::compare(dir_a.path(), dir_b.path(), &no_cancel());
        let paths: Vec<&str> = result.entries.iter().map(|e| e.rel_path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "m.txt", "z.txt"]);
    }
}
