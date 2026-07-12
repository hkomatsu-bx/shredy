//! 検索結果グループの編集（削除反映）— 純関数（設計 M2 §4）。
//!
//! 削除に成功したパスをグループから取り除き、重複が解消した（残り 1 件以下の）
//! グループを一覧から落とす。Slint 非依存で単体テスト可能にする。

use std::collections::HashSet;

use shredy_core::{DuplicateGroup, LongPath};

/// パスを大文字小文字無視のキー文字列へ（実装は core `LongPath::case_key` に集約）。
fn key(path: &LongPath) -> String {
    path.case_key()
}

/// 削除済みパスをグループから除去し、残り 1 件以下のグループを除外した新しい一覧を返す。
///
/// 重複グループは 2 件以上で成立するため、削除後に 1 件だけ残ったグループは
/// 「重複解消」とみなして一覧から落とす（イミュータブル: 新規 Vec を返す）。
#[must_use]
pub fn remove_deleted(groups: &[DuplicateGroup], deleted: &[LongPath]) -> Vec<DuplicateGroup> {
    let removed: HashSet<String> = deleted.iter().map(key).collect();
    groups
        .iter()
        .filter_map(|group| {
            let files: Vec<_> = group
                .files
                .iter()
                .filter(|f| !removed.contains(&key(&f.path)))
                .cloned()
                .collect();
            if files.len() >= 2 {
                Some(DuplicateGroup {
                    representative: group.representative.clone(),
                    files,
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shredy_core::{FileEntry, FileSize};
    use std::time::SystemTime;

    fn entry(path: &str) -> FileEntry {
        FileEntry {
            path: LongPath::new(path).unwrap(),
            size: FileSize(1),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    fn group(paths: &[&str]) -> DuplicateGroup {
        DuplicateGroup {
            representative: "blake3:x".into(),
            files: paths.iter().map(|p| entry(p)).collect(),
        }
    }

    #[test]
    fn removes_deleted_and_keeps_remaining_duplicates() {
        let groups = vec![group(&["C:\\a", "C:\\b", "C:\\c"])];
        let out = remove_deleted(&groups, &[LongPath::new("C:\\a").unwrap()]);
        // 3 件中 1 件削除 → 2 件残り、グループは維持。
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].files.len(), 2);
    }

    #[test]
    fn drops_group_resolved_to_single_file() {
        let groups = vec![group(&["C:\\a", "C:\\b"])];
        let out = remove_deleted(&groups, &[LongPath::new("C:\\a").unwrap()]);
        // 2 件中 1 件削除 → 残り 1 件は重複でない → グループ消滅。
        assert!(out.is_empty());
    }

    #[test]
    fn matches_case_insensitively() {
        let groups = vec![group(&["C:\\Dir\\A", "C:\\Dir\\B", "C:\\Dir\\C"])];
        let out = remove_deleted(&groups, &[LongPath::new("c:\\dir\\a").unwrap()]);
        assert_eq!(out[0].files.len(), 2);
    }

    #[test]
    fn empty_deleted_returns_equivalent_groups() {
        let groups = vec![group(&["C:\\a", "C:\\b"])];
        let out = remove_deleted(&groups, &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].files.len(), 2);
    }
}
