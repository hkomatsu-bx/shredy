//! 重複検索サマリの集計（設計 U12 M6）。
//!
//! Slint 非依存の純関数。core 結果（`DuplicateGroup`）と選択状態から、サマリバー／
//! ステータスバーが表示する件数・回収サイズを算出する。「1つ残す」基準は core の
//! [`Selector::keep_target`] を再利用する（ロジック重複を避ける）。

use shredy_core::{DuplicateGroup, KeepFileRule, Selector};

use crate::viewmodel::SelectionState;

/// 重複ファイル総数（全グループのファイル数合計）。
#[must_use]
pub fn dup_file_count(groups: &[DuplicateGroup]) -> usize {
    groups.iter().map(|g| g.files.len()).sum()
}

/// グループ内で「1つ残す」を適用したとき削除できるサイズ（合計 − 残置 1 件）。
#[must_use]
pub fn group_deletable(group: &DuplicateGroup, rule: KeepFileRule) -> u64 {
    let total: u64 = group.files.iter().map(|f| f.size.0).sum();
    let keep = Selector::keep_target(group, rule).map_or(0, |f| f.size.0);
    total.saturating_sub(keep)
}

/// 全グループで「1つ残す」を適用したときに回収できる最大サイズ。
#[must_use]
pub fn potential_reclaim(groups: &[DuplicateGroup], rule: KeepFileRule) -> u64 {
    groups.iter().map(|g| group_deletable(g, rule)).sum()
}

/// 現在の削除選択（チェック）で回収できるサイズ合計。
#[must_use]
pub fn total_reclaim(groups: &[DuplicateGroup], selection: &SelectionState) -> u64 {
    selection
        .iter_checked()
        .filter_map(|(gi, fi)| groups.get(gi).and_then(|g| g.files.get(fi)))
        .map(|f| f.size.0)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shredy_core::path::LongPath;
    use shredy_core::{FileEntry, FileSize};
    use std::time::{Duration, UNIX_EPOCH};

    fn entry(path: &str, size: u64, secs_after_epoch: u64) -> FileEntry {
        FileEntry {
            path: LongPath::new(path).unwrap(),
            size: FileSize(size),
            modified: UNIX_EPOCH + Duration::from_secs(secs_after_epoch),
        }
    }

    fn group(files: Vec<FileEntry>) -> DuplicateGroup {
        DuplicateGroup {
            representative: "test".into(),
            files,
        }
    }

    #[test]
    fn dup_file_count_sums_all_files() {
        let groups = vec![
            group(vec![entry("C:\\a", 10, 1), entry("C:\\b", 10, 2)]),
            group(vec![
                entry("C:\\c", 5, 1),
                entry("C:\\d", 5, 2),
                entry("C:\\e", 5, 3),
            ]),
        ];
        assert_eq!(dup_file_count(&groups), 5);
    }

    #[test]
    fn group_deletable_excludes_one_kept_file() {
        // ByDateNewest: 最新（secs=2）の 100 を残し、100 が削除可。
        let g = group(vec![entry("C:\\a", 100, 1), entry("C:\\b", 100, 2)]);
        assert_eq!(group_deletable(&g, KeepFileRule::ByDateNewest), 100);
    }

    #[test]
    fn potential_reclaim_sums_group_deletables() {
        let groups = vec![
            group(vec![entry("C:\\a", 100, 1), entry("C:\\b", 100, 2)]), // 100 削除可
            group(vec![
                entry("C:\\c", 50, 1),
                entry("C:\\d", 50, 2),
                entry("C:\\e", 50, 3),
            ]), // 100 削除可（150 − 50 keep）
        ];
        assert_eq!(potential_reclaim(&groups, KeepFileRule::ByDateNewest), 200);
    }

    #[test]
    fn total_reclaim_sums_checked_file_sizes() {
        let groups = vec![group(vec![
            entry("C:\\a", 100, 1),
            entry("C:\\b", 200, 2),
            entry("C:\\c", 300, 3),
        ])];
        let mut sel = SelectionState::new();
        sel.set_checked((0, 0), true);
        sel.set_checked((0, 2), true);
        assert_eq!(total_reclaim(&groups, &sel), 400);
    }

    #[test]
    fn total_reclaim_is_zero_without_selection() {
        let groups = vec![group(vec![entry("C:\\a", 100, 1)])];
        assert_eq!(total_reclaim(&groups, &SelectionState::new()), 0);
    }

    #[test]
    fn total_reclaim_ignores_stale_checked_coordinates() {
        let groups = vec![group(vec![entry("C:\\a", 100, 1)])];
        let mut sel = SelectionState::new();
        sel.set_checked((0, 0), true);
        sel.set_checked((9, 9), true);
        assert_eq!(total_reclaim(&groups, &sel), 100);
    }
}
