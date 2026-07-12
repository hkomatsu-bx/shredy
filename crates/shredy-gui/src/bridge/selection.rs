//! GUI 選択状態（座標キー）と core `Selection`（パスキー）の橋渡し（設計 M2 §0）。
//!
//! 選択ロジック（「1つ残す」・配下/全選択）は core の [`Selector`] に委譲し、
//! GUI 側は座標 ↔ パスの写像のみを担う（純関数・Slint 非依存）。

use shredy_core::{DuplicateGroup, KeepFileRule, LockSet, LongPath, Selection, Selector};

use crate::viewmodel::SelectionState;

/// 起点フォルダを 1 階層上の親へ移す（コンテキストメニューの ↑ ボタン, §3.5a）。
///
/// 親が無い（ドライブルート等）・空文字の場合は元の値をそのまま返す（純関数）。
/// 入力は表示形式のパス（`entry.folder`＝`\\?\` プレフィックスなし）を前提とする。
/// `\\?\` 付きでも `Path::parent` は正しく機能するが、プレフィックスは保持される。
#[must_use]
pub fn parent_dir(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return path.to_string();
    }
    std::path::Path::new(trimmed)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| trimmed.to_string())
}

/// チェック済み座標を対応するパス列へ写像する（削除計画へ渡す用）。
#[must_use]
pub fn checked_paths(groups: &[DuplicateGroup], state: &SelectionState) -> Vec<LongPath> {
    groups
        .iter()
        .enumerate()
        .flat_map(|(gi, group)| {
            group
                .files
                .iter()
                .enumerate()
                .filter(move |(fi, _)| state.is_checked((gi, *fi)))
                .map(|(_, f)| f.path.clone())
        })
        .collect()
}

/// 「全選択」（core `Selector::select_all`＝「1つ残す」適用済）を `SelectionState` へ反映する。
///
/// core の選択結果（パス集合）を座標へ写し戻し、チェック状態を決定的に再構築する。
pub fn apply_select_all(
    groups: &[DuplicateGroup],
    rule: KeepFileRule,
    locks: &LockSet,
    state: &mut SelectionState,
) {
    let selection = Selector::select_all(groups, rule, locks);
    reflect(groups, &selection, state);
}

/// 起点フォルダ直下のファイルを選択する（`invert` で「直下以外」, §3.4）。
pub fn apply_select_direct(
    groups: &[DuplicateGroup],
    origin: &LongPath,
    invert: bool,
    rule: KeepFileRule,
    locks: &LockSet,
    state: &mut SelectionState,
) {
    let selection = Selector::select_direct(groups, origin, invert, rule, locks);
    reflect(groups, &selection, state);
}

/// 起点フォルダのツリー配下のファイルを選択する（`invert` で「ツリー配下以外」, §3.4）。
pub fn apply_select_tree(
    groups: &[DuplicateGroup],
    origin: &LongPath,
    invert: bool,
    rule: KeepFileRule,
    locks: &LockSet,
    state: &mut SelectionState,
) {
    let selection = Selector::select_tree(groups, origin, invert, rule, locks);
    reflect(groups, &selection, state);
}

/// フルパスへのワイルドカードでファイルを選択する（§3.4 パターン選択）。
pub fn apply_select_pattern(
    groups: &[DuplicateGroup],
    pattern: &str,
    rule: KeepFileRule,
    locks: &LockSet,
    state: &mut SelectionState,
) {
    let selection = Selector::select_pattern(groups, pattern, rule, locks);
    reflect(groups, &selection, state);
}

// ───────────────────────── 一括ロック（§3.4, U12 M8） ─────────────────────────

/// 全ロック（「1つ残す」適用＝各グループ keep を除く全件をロック, §3.4）。
///
/// ロックは**加算**（既存ロックは保持）。keep は `KeepFileRule` 由来で安定のため、
/// 連続呼び出しは冪等（残置候補が後から誤ってロックされることはない）。
pub fn apply_lock_all(groups: &[DuplicateGroup], rule: KeepFileRule, locks: &mut LockSet) {
    let selection = Selector::select_all(groups, rule, &LockSet::new());
    lock_matching(groups, &selection, locks);
}

/// 起点フォルダ直下をロックする（`invert` で「直下以外」, §3.4）。
pub fn apply_lock_direct(
    groups: &[DuplicateGroup],
    origin: &LongPath,
    invert: bool,
    rule: KeepFileRule,
    locks: &mut LockSet,
) {
    let selection = Selector::select_direct(groups, origin, invert, rule, &LockSet::new());
    lock_matching(groups, &selection, locks);
}

/// 起点フォルダのツリー配下をロックする（`invert` で「ツリー配下以外」, §3.4）。
pub fn apply_lock_tree(
    groups: &[DuplicateGroup],
    origin: &LongPath,
    invert: bool,
    rule: KeepFileRule,
    locks: &mut LockSet,
) {
    let selection = Selector::select_tree(groups, origin, invert, rule, &LockSet::new());
    lock_matching(groups, &selection, locks);
}

/// フルパスへのワイルドカードでロックする（§3.4 パターン）。
pub fn apply_lock_pattern(
    groups: &[DuplicateGroup],
    pattern: &str,
    rule: KeepFileRule,
    locks: &mut LockSet,
) {
    let selection = Selector::select_pattern(groups, pattern, rule, &LockSet::new());
    lock_matching(groups, &selection, locks);
}

/// 全ロック解除。
pub fn unlock_all(locks: &mut LockSet) {
    *locks = LockSet::new();
}

/// core `Selection`（操作対象パス）に含まれるファイルを `LockSet` へロックする。
///
/// 選択計算には空の `LockSet` を渡しているため（`select_*` は locked を除外する）、
/// 既存ロックの有無に依存せず対象全件をロックできる。
fn lock_matching(groups: &[DuplicateGroup], selection: &Selection, locks: &mut LockSet) {
    for group in groups {
        for f in &group.files {
            if selection.is_selected(&f.path) {
                locks.lock(&f.path);
            }
        }
    }
}

/// core `Selection` を `SelectionState` のチェック状態へ写し戻す（他は解除）。
fn reflect(groups: &[DuplicateGroup], selection: &Selection, state: &mut SelectionState) {
    for (gi, group) in groups.iter().enumerate() {
        for (fi, f) in group.files.iter().enumerate() {
            state.set_checked((gi, fi), selection.is_selected(&f.path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shredy_core::{FileEntry, FileSize};
    use std::time::{Duration, SystemTime};

    fn entry(path: &str, age: u64) -> FileEntry {
        FileEntry {
            path: LongPath::new(path).unwrap(),
            size: FileSize(1),
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(age),
        }
    }

    fn group(paths: &[(&str, u64)]) -> DuplicateGroup {
        DuplicateGroup {
            representative: "blake3:x".into(),
            files: paths.iter().map(|(p, a)| entry(p, *a)).collect(),
        }
    }

    #[test]
    fn parent_dir_goes_up_one_level() {
        assert_eq!(parent_dir("C:\\a\\b\\c"), "C:\\a\\b");
        assert_eq!(parent_dir("C:\\a"), "C:\\");
    }

    #[test]
    fn parent_dir_keeps_value_at_root_or_empty() {
        // ドライブルートは親なし → そのまま。
        assert_eq!(parent_dir("C:\\"), "C:\\");
        // 空文字・空白はそのまま返す。
        assert_eq!(parent_dir(""), "");
        assert_eq!(parent_dir("   "), "   ");
    }

    #[test]
    fn checked_paths_maps_coordinates_to_paths() {
        let groups = vec![group(&[("C:\\a", 1), ("C:\\b", 2)])];
        let mut state = SelectionState::new();
        state.set_checked((0, 1), true);
        let paths = checked_paths(&groups, &state);
        assert_eq!(paths, vec![LongPath::new("C:\\b").unwrap()]);
    }

    #[test]
    fn apply_select_all_keeps_one_per_group() {
        let groups = vec![group(&[("C:\\a", 1), ("C:\\b", 2), ("C:\\c", 3)])];
        let mut state = SelectionState::new();
        apply_select_all(
            &groups,
            KeepFileRule::ByDateNewest,
            &LockSet::new(),
            &mut state,
        );
        // 最新 (c=idx2) は残り、a/b が選択される（「1つ残す」）。
        assert_eq!(state.checked_count(), 2);
        assert!(state.is_checked((0, 0)));
        assert!(state.is_checked((0, 1)));
        assert!(!state.is_checked((0, 2)));
    }

    #[test]
    fn apply_lock_all_locks_all_but_keep_per_group() {
        let groups = vec![group(&[("C:\\a", 1), ("C:\\b", 2), ("C:\\c", 3)])];
        let mut locks = LockSet::new();
        apply_lock_all(&groups, KeepFileRule::ByDateNewest, &mut locks);
        // 最新 (c) は残り、a/b がロックされる。
        assert!(locks.is_locked(&LongPath::new("C:\\a").unwrap()));
        assert!(locks.is_locked(&LongPath::new("C:\\b").unwrap()));
        assert!(!locks.is_locked(&LongPath::new("C:\\c").unwrap()));
        assert_eq!(locks.len(), 2);
    }

    #[test]
    fn unlock_all_clears_locks() {
        let mut locks = LockSet::new();
        locks.lock(&LongPath::new("C:\\a").unwrap());
        unlock_all(&mut locks);
        assert!(locks.is_empty());
    }

    #[test]
    fn apply_lock_pattern_locks_matching() {
        let groups = vec![group(&[("C:\\keep\\a", 1), ("C:\\bak\\a", 2)])];
        let mut locks = LockSet::new();
        apply_lock_pattern(&groups, "*\\bak\\*", KeepFileRule::ByDateNewest, &mut locks);
        assert!(locks.is_locked(&LongPath::new("C:\\bak\\a").unwrap()));
        assert!(!locks.is_locked(&LongPath::new("C:\\keep\\a").unwrap()));
    }

    #[test]
    fn apply_select_all_overwrites_previous_checks() {
        let groups = vec![group(&[("C:\\a", 1), ("C:\\b", 2), ("C:\\c", 3)])];
        let mut state = SelectionState::new();
        // 事前に keep 対象 (c) を手動チェックしていても、全選択で解除される。
        state.set_checked((0, 2), true);
        apply_select_all(
            &groups,
            KeepFileRule::ByDateNewest,
            &LockSet::new(),
            &mut state,
        );
        assert!(!state.is_checked((0, 2)));
    }

    #[test]
    fn apply_select_direct_checks_only_direct_children() {
        let groups = vec![group(&[
            ("C:\\d\\a", 1),
            ("C:\\d\\sub\\b", 2),
            ("C:\\d\\c", 3),
        ])];
        let origin = LongPath::new("C:\\d").unwrap();
        let mut state = SelectionState::new();
        apply_select_direct(
            &groups,
            &origin,
            false,
            KeepFileRule::ByPathAsc,
            &LockSet::new(),
            &mut state,
        );
        // 直下 a/c が対象（「1つ残す」で 1 件は残る）。sub\b は対象外。
        assert!(!state.is_checked((0, 1)));
        assert!(state.checked_count() <= 2);
    }

    #[test]
    fn apply_select_tree_includes_subfolders() {
        let groups = vec![group(&[
            ("C:\\d\\a", 1),
            ("C:\\d\\sub\\b", 2),
            ("E:\\x", 3),
        ])];
        let origin = LongPath::new("C:\\d").unwrap();
        let mut state = SelectionState::new();
        apply_select_tree(
            &groups,
            &origin,
            false,
            KeepFileRule::ByPathAsc,
            &LockSet::new(),
            &mut state,
        );
        // ツリー外の E:\x は選択されない。
        assert!(!state.is_checked((0, 2)));
    }

    #[test]
    fn apply_select_pattern_matches_wildcard() {
        let groups = vec![group(&[
            ("C:\\a.txt", 1),
            ("C:\\b.txt", 2),
            ("C:\\c.log", 3),
        ])];
        let mut state = SelectionState::new();
        apply_select_pattern(
            &groups,
            "*.txt",
            KeepFileRule::ByPathAsc,
            &LockSet::new(),
            &mut state,
        );
        // .log は選択されない。.txt は対象（「1つ残す」適用）。
        assert!(!state.is_checked((0, 2)));
    }

    #[test]
    fn locked_files_excluded_from_select_all() {
        let groups = vec![group(&[("C:\\a", 1), ("C:\\b", 2), ("C:\\c", 3)])];
        let mut locks = LockSet::new();
        locks.lock(&LongPath::new("C:\\a").unwrap());
        let mut state = SelectionState::new();
        apply_select_all(&groups, KeepFileRule::ByDateNewest, &locks, &mut state);
        // ロック済み a は選択されない。
        assert!(!state.is_checked((0, 0)));
    }
}
