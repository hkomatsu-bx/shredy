//! ファイル選択（仕様 §3.4）。
//!
//! 重複グループに対する各種選択操作・「1つ残す」ルール・`KeepFileRule`・ロックを扱う。
//! いかなる選択操作でも、各重複グループに最低 1 ファイルを残すことを保証する（安全第一）。

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;

use crate::model::{DuplicateGroup, FileEntry, KeepFileRule};
use crate::path::{LongPath, comp_keys};
use crate::scan::matches_wildcard;

/// パスを大文字小文字無視のキー文字列へ（実装は [`LongPath::case_key`] に集約）。
fn key(path: &LongPath) -> String {
    path.case_key()
}

fn cmp_ignore_case(a: &str, b: &str) -> Ordering {
    let mut ac = a.chars().flat_map(char::to_lowercase);
    let mut bc = b.chars().flat_map(char::to_lowercase);
    loop {
        match (ac.next(), bc.next()) {
            (Some(a), Some(b)) => match a.cmp(&b) {
                Ordering::Equal => {}
                ord => return ord,
            },
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn cmp_path_ignore_case(a: &LongPath, b: &LongPath) -> Ordering {
    let a = a.as_path().to_string_lossy();
    let b = b.as_path().to_string_lossy();
    cmp_ignore_case(a.as_ref(), b.as_ref())
}

fn path_eq_ignore_case(path: &Path, value: &str) -> bool {
    let path = path.to_string_lossy();
    cmp_ignore_case(path.as_ref(), value) == Ordering::Equal
}

/// 保護マーク集合（選択・削除対象から恒久的に除外, 仕様 §3.4）。
#[derive(Debug, Clone, Default)]
pub struct LockSet {
    locked: HashSet<String>,
}

impl LockSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// ファイルをロックする。
    pub fn lock(&mut self, path: &LongPath) {
        self.locked.insert(key(path));
    }

    /// ロックを解除する。
    pub fn unlock(&mut self, path: &LongPath) {
        self.locked.remove(&key(path));
    }

    /// ロックされているか。
    #[must_use]
    pub fn is_locked(&self, path: &LongPath) -> bool {
        self.locked.contains(&key(path))
    }

    fn contains_key(&self, key: &str) -> bool {
        self.locked.contains(key)
    }

    /// ロック件数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.locked.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.locked.is_empty()
    }
}

/// 削除対象として選択されたファイル集合（仕様 §3.4 の「削除選択」）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    selected: HashSet<String>,
}

impl Selection {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// パス列から選択集合を構築する（フロントエンドの手動チェック → 削除計画の橋渡し）。
    ///
    /// キーは他の選択操作と同じく大文字小文字無視（小文字化）で正規化する。
    /// 「1つ残す」・使用中除外は [`crate::delete::Deleter::plan`] が担保するため、
    /// ここでは与えられたパスをそのまま選択集合へ入れる。
    #[must_use]
    pub fn from_paths(paths: impl IntoIterator<Item = LongPath>) -> Self {
        let mut sel = Self::new();
        for p in paths {
            sel.insert(&p);
        }
        sel
    }

    /// 選択されているか。
    #[must_use]
    pub fn is_selected(&self, path: &LongPath) -> bool {
        self.selected.contains(&key(path))
    }

    /// 選択件数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    fn insert(&mut self, path: &LongPath) {
        self.selected.insert(key(path));
    }

    fn insert_key(&mut self, key: String) {
        self.selected.insert(key);
    }

    fn remove(&mut self, path: &LongPath) {
        self.selected.remove(&key(path));
    }
}

/// 選択ロジック。
pub struct Selector;

impl Selector {
    /// グループ内で「残す 1 件」を返す（`KeepFileRule` に従う）。空グループでは `None`。
    #[must_use]
    pub fn keep_target(group: &DuplicateGroup, rule: KeepFileRule) -> Option<&FileEntry> {
        match rule {
            KeepFileRule::ByDateNewest => group.files.iter().max_by_key(|f| f.modified),
            KeepFileRule::ByDateOldest => group.files.iter().min_by_key(|f| f.modified),
            KeepFileRule::ByPathAsc => group
                .files
                .iter()
                .min_by(|a, b| cmp_path_ignore_case(&a.path, &b.path)),
            KeepFileRule::ByPathDesc => group
                .files
                .iter()
                .max_by(|a, b| cmp_path_ignore_case(&a.path, &b.path)),
            // パス長は実パス長で比較する（小文字化は長さに本質的でなく、key() の
            // String 確保を避ける）。
            KeepFileRule::ByPathLengthShortest => group
                .files
                .iter()
                .min_by_key(|f| f.path.as_path().as_os_str().len()),
            KeepFileRule::ByPathLengthLongest => group
                .files
                .iter()
                .max_by_key(|f| f.path.as_path().as_os_str().len()),
        }
    }

    /// 全選択（「1つ残す」適用, 仕様 §3.4）。各グループの keep を除く全ファイルを選択する。
    #[must_use]
    pub fn select_all(groups: &[DuplicateGroup], rule: KeepFileRule, locks: &LockSet) -> Selection {
        Self::select_where(groups, rule, locks, false, |_| true)
    }

    /// 起点フォルダ直下のファイルを選択する（`invert` で「直下以外」）。
    #[must_use]
    pub fn select_direct(
        groups: &[DuplicateGroup],
        origin: &LongPath,
        invert: bool,
        rule: KeepFileRule,
        locks: &LockSet,
    ) -> Selection {
        let origin_key = key(origin);
        Self::select_where(groups, rule, locks, invert, |f| {
            is_direct_child(f, &origin_key)
        })
    }

    /// 起点フォルダのツリー配下のファイルを選択する（`invert` で「ツリー配下以外」）。
    #[must_use]
    pub fn select_tree(
        groups: &[DuplicateGroup],
        origin: &LongPath,
        invert: bool,
        rule: KeepFileRule,
        locks: &LockSet,
    ) -> Selection {
        let origin_keys = comp_keys(origin.as_path());
        Self::select_where(groups, rule, locks, invert, |f| {
            is_under_tree(f, &origin_keys)
        })
    }

    /// パス（フルパス）に対するワイルドカードで選択する（仕様 §3.4 パターン選択）。
    #[must_use]
    pub fn select_pattern(
        groups: &[DuplicateGroup],
        pattern: &str,
        rule: KeepFileRule,
        locks: &LockSet,
    ) -> Selection {
        Self::select_where(groups, rule, locks, false, |f| {
            matches_wildcard(pattern, &f.path.as_path().to_string_lossy())
        })
    }

    /// 起点基準の述語 `base` に一致するファイルを選択し、ロック除外と「1つ残す」を適用する。
    ///
    /// `invert = false` は `base(f)` が真のファイルを選択する。`invert = true` は反転し、
    /// `base(f)` が偽のファイルを選択するが、**`base(f)` が真のファイルを 1 件以上含む
    /// グループに限る**（起点フォルダと無関係なグループへ選択が波及しない, 仕様 §3.4 反転）。
    /// これにより「起点配下のコピーを残し、同一グループ内の配下外コピーを選択」する挙動になる。
    fn select_where(
        groups: &[DuplicateGroup],
        rule: KeepFileRule,
        locks: &LockSet,
        invert: bool,
        base: impl Fn(&FileEntry) -> bool,
    ) -> Selection {
        let mut sel = Selection::new();
        for group in groups {
            // 反転時、起点に属するファイル（base 一致）を含まないグループは対象外とし、
            // 選択の波及を防ぐ。非反転では base 非一致のグループは自然に何も選択しない。
            if invert && !group.files.iter().any(&base) {
                continue;
            }
            // 選択条件（非反転は base、反転は !base）かつ非ロックの件数をループ内で数え、
            // 二度目の走査（is_selected で key() を再確保）なしに「実質全件選択か」を判定する。
            // グループ内のパスは一意なので件数比較で十分。ロック済みは insert_key されない。
            let mut selected_in_group = 0usize;
            for f in &group.files {
                if base(f) != invert {
                    let path_key = key(&f.path);
                    if !locks.contains_key(&path_key) {
                        sel.insert_key(path_key);
                        selected_in_group += 1;
                    }
                }
            }
            // 「1つ残す」: 選択条件通過かつ非ロックの件数がグループ全ファイル数と一致した
            // 場合のみ keep を選択解除する。ロック済みが 1 件でもあれば
            // selected_in_group < files.len() となりこの条件は成立しない（安全側に倒れる）。
            if selected_in_group == group.files.len()
                && let Some(keep) = Self::keep_target(group, rule)
            {
                sel.remove(&keep.path);
            }
        }
        sel
    }
}

/// ファイルの親ディレクトリが `origin_key`（小文字）と一致するか。
fn is_direct_child(f: &FileEntry, origin_key: &str) -> bool {
    f.path
        .as_path()
        .parent()
        .map(|p| path_eq_ignore_case(p, origin_key))
        .unwrap_or(false)
}

/// ファイルが origin のツリー配下（origin 自身を親祖先に持つ）か。
fn is_under_tree(f: &FileEntry, origin_keys: &[String]) -> bool {
    let fk = comp_keys(f.path.as_path());
    // origin はファイルの厳密な祖先（= ファイル本体は origin より深い）。
    origin_keys.len() < fk.len() && origin_keys.iter().zip(fk.iter()).all(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileSize;
    use std::time::{Duration, SystemTime};

    fn entry(path: &str, age_secs: u64) -> FileEntry {
        FileEntry {
            path: LongPath::new(path).unwrap(),
            size: FileSize(1),
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(age_secs),
        }
    }

    fn group(paths: &[(&str, u64)]) -> DuplicateGroup {
        DuplicateGroup {
            representative: "blake3:x".into(),
            files: paths.iter().map(|(p, a)| entry(p, *a)).collect(),
        }
    }

    #[test]
    fn select_all_keeps_one_per_group() {
        let g = group(&[("C:\\a", 1), ("C:\\b", 2), ("C:\\c", 3)]);
        let groups = vec![g];
        let sel = Selector::select_all(&groups, KeepFileRule::ByDateNewest, &LockSet::new());
        // 3 件中 2 件選択、最新 (c) は残る。
        assert_eq!(sel.len(), 2);
        assert!(!sel.is_selected(&LongPath::new("C:\\c").unwrap()));
        assert!(sel.is_selected(&LongPath::new("C:\\a").unwrap()));
    }

    #[test]
    fn keep_target_respects_rule() {
        let g = group(&[("C:\\zzz\\a", 1), ("C:\\a", 9)]);
        assert_eq!(
            Selector::keep_target(&g, KeepFileRule::ByDateNewest)
                .unwrap()
                .path,
            LongPath::new("C:\\a").unwrap()
        );
        assert_eq!(
            Selector::keep_target(&g, KeepFileRule::ByPathLengthShortest)
                .unwrap()
                .path,
            LongPath::new("C:\\a").unwrap()
        );
    }

    #[test]
    fn keep_target_path_rules_compare_case_insensitively() {
        let g = group(&[("C:\\B", 1), ("C:\\a", 2)]);
        assert_eq!(
            Selector::keep_target(&g, KeepFileRule::ByPathAsc)
                .unwrap()
                .path,
            LongPath::new("C:\\a").unwrap()
        );
        assert_eq!(
            Selector::keep_target(&g, KeepFileRule::ByPathDesc)
                .unwrap()
                .path,
            LongPath::new("C:\\B").unwrap()
        );
    }

    #[test]
    fn keep_target_path_rules_handle_non_ascii() {
        // 全角・ひらがな・絵文字を含むパスでも char 単位の大小無視比較が
        // to_lowercase ベースと同じ順序で安定する（あ U+3042 < ア U+30A2）。
        let g = group(&[("C:\\ア", 1), ("C:\\あ", 2)]);
        assert_eq!(
            Selector::keep_target(&g, KeepFileRule::ByPathAsc)
                .unwrap()
                .path,
            LongPath::new("C:\\あ").unwrap()
        );
        assert_eq!(
            Selector::keep_target(&g, KeepFileRule::ByPathDesc)
                .unwrap()
                .path,
            LongPath::new("C:\\ア").unwrap()
        );
    }

    #[test]
    fn from_paths_selects_given_paths_case_insensitively() {
        let sel = Selection::from_paths([
            LongPath::new("C:\\dir\\A.txt").unwrap(),
            LongPath::new("C:\\dir\\b.txt").unwrap(),
        ]);
        assert_eq!(sel.len(), 2);
        // 大小無視で一致する。
        assert!(sel.is_selected(&LongPath::new("C:\\DIR\\a.TXT").unwrap()));
        assert!(sel.is_selected(&LongPath::new("C:\\dir\\b.txt").unwrap()));
        assert!(!sel.is_selected(&LongPath::new("C:\\dir\\c.txt").unwrap()));
    }

    #[test]
    fn from_paths_empty_yields_empty_selection() {
        let sel = Selection::from_paths(std::iter::empty());
        assert!(sel.is_empty());
    }

    #[test]
    fn locked_files_are_not_selected() {
        let g = group(&[("C:\\a", 1), ("C:\\b", 2), ("C:\\c", 3)]);
        let groups = vec![g];
        let mut locks = LockSet::new();
        locks.lock(&LongPath::new("C:\\a").unwrap());
        let sel = Selector::select_all(&groups, KeepFileRule::ByDateNewest, &locks);
        assert!(!sel.is_selected(&LongPath::new("C:\\a").unwrap()));
    }

    #[test]
    fn select_direct_only_direct_children() {
        let g = group(&[("C:\\d\\a", 1), ("C:\\d\\sub\\b", 2), ("C:\\d\\c", 3)]);
        let groups = vec![g];
        let origin = LongPath::new("C:\\d").unwrap();
        let sel = Selector::select_direct(
            &groups,
            &origin,
            false,
            KeepFileRule::ByPathAsc,
            &LockSet::new(),
        );
        // 直下 a, c が対象だが「1つ残す」で 1 件は残る。sub\b は対象外。
        assert!(!sel.is_selected(&LongPath::new("C:\\d\\sub\\b").unwrap()));
        assert!(sel.len() <= 2);
    }

    #[test]
    fn select_direct_matches_origin_case_insensitively() {
        let g = group(&[("C:\\D\\a", 1), ("C:\\other\\b", 2)]);
        let groups = vec![g];
        let origin = LongPath::new("c:\\d").unwrap();
        let sel = Selector::select_direct(
            &groups,
            &origin,
            false,
            KeepFileRule::ByPathAsc,
            &LockSet::new(),
        );
        assert!(sel.is_selected(&LongPath::new("C:\\D\\a").unwrap()));
        assert!(!sel.is_selected(&LongPath::new("C:\\other\\b").unwrap()));
    }

    #[test]
    fn select_tree_includes_subfolders() {
        let g = group(&[("C:\\d\\a", 1), ("C:\\d\\sub\\b", 2), ("E:\\x", 3)]);
        let groups = vec![g];
        let origin = LongPath::new("C:\\d").unwrap();
        let sel = Selector::select_tree(
            &groups,
            &origin,
            false,
            KeepFileRule::ByPathAsc,
            &LockSet::new(),
        );
        assert!(!sel.is_selected(&LongPath::new("E:\\x").unwrap()));
    }

    #[test]
    fn select_tree_invert_only_affects_groups_containing_origin() {
        // roadmap §2 不具合の回帰テスト。起点 C:\foo の「ツリー配下以外」は、C:\foo 配下の
        // ファイルを含む重複グループにのみ適用し、無関係なグループへ波及しない
        // （ベンチマーク先の挙動と一致）。
        let groups = vec![
            // Group #1: どちらも C:\foo 配下でない → 対象外（選択なし）。
            group(&[
                ("C:\\hoge\\fuga\\hage.txt", 1),
                ("C:\\hoge\\fuga\\hige.txt", 2),
            ]),
            // Group #2: C:\foo 配下の (3) を含む → 配下外の (4) を選択、(3) は保持。
            group(&[
                ("C:\\foo\\bar\\bazz.txt", 1),
                ("C:\\hoge\\fuga\\bazz.txt", 2),
            ]),
            // Group #3: C:\foo 配下の (6) を含む → 配下外の (5) を選択、(6) は保持。
            group(&[("C:\\home\\you\\me.txt", 1), ("C:\\foo\\bar\\me.txt", 2)]),
        ];
        let origin = LongPath::new("C:\\foo").unwrap();
        let sel = Selector::select_tree(
            &groups,
            &origin,
            true,
            KeepFileRule::ByPathAsc,
            &LockSet::new(),
        );
        // Group #1 は起点配下を含まない → 何も選択されない。
        assert!(!sel.is_selected(&LongPath::new("C:\\hoge\\fuga\\hage.txt").unwrap()));
        assert!(!sel.is_selected(&LongPath::new("C:\\hoge\\fuga\\hige.txt").unwrap()));
        // Group #2: 配下外 (4) を選択、配下 (3) は保持。
        assert!(sel.is_selected(&LongPath::new("C:\\hoge\\fuga\\bazz.txt").unwrap()));
        assert!(!sel.is_selected(&LongPath::new("C:\\foo\\bar\\bazz.txt").unwrap()));
        // Group #3: 配下外 (5) を選択、配下 (6) は保持。
        assert!(sel.is_selected(&LongPath::new("C:\\home\\you\\me.txt").unwrap()));
        assert!(!sel.is_selected(&LongPath::new("C:\\foo\\bar\\me.txt").unwrap()));
        // 合計 2 件（ベンチマーク先と一致）。
        assert_eq!(sel.len(), 2);
    }

    #[test]
    fn select_direct_invert_only_affects_groups_containing_direct_child() {
        // 「直下以外」も同様に、起点直下ファイルを含むグループにのみ適用する。
        let groups = vec![
            // 起点直下を含まない → 対象外。
            group(&[("C:\\a\\x", 1), ("C:\\b\\x", 2)]),
            // C:\foo 直下の C:\foo\y を含む → 直下外の C:\c\y を選択、C:\foo\y は保持。
            group(&[("C:\\foo\\y", 1), ("C:\\c\\y", 2)]),
        ];
        let origin = LongPath::new("C:\\foo").unwrap();
        let sel = Selector::select_direct(
            &groups,
            &origin,
            true,
            KeepFileRule::ByPathAsc,
            &LockSet::new(),
        );
        assert!(!sel.is_selected(&LongPath::new("C:\\a\\x").unwrap()));
        assert!(!sel.is_selected(&LongPath::new("C:\\b\\x").unwrap()));
        assert!(sel.is_selected(&LongPath::new("C:\\c\\y").unwrap()));
        assert!(!sel.is_selected(&LongPath::new("C:\\foo\\y").unwrap()));
        assert_eq!(sel.len(), 1);
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use crate::model::FileSize;
    use proptest::prelude::*;
    use std::time::SystemTime;

    fn make_group(n: usize) -> DuplicateGroup {
        DuplicateGroup {
            representative: "blake3:x".into(),
            files: (0..n)
                .map(|i| FileEntry {
                    path: LongPath::new(format!("C:\\g\\f{i}")).unwrap(),
                    size: FileSize(1),
                    modified: SystemTime::UNIX_EPOCH,
                })
                .collect(),
        }
    }

    proptest! {
        // PBT-03 (不変条件): 全選択しても各グループに最低 1 件は残る。
        #[test]
        fn select_all_always_leaves_one(n in 2usize..12) {
            let groups = vec![make_group(n)];
            let sel = Selector::select_all(&groups, KeepFileRule::ByPathAsc, &LockSet::new());
            let remaining = groups[0].files.iter().filter(|f| !sel.is_selected(&f.path)).count();
            prop_assert!(remaining >= 1);
        }
    }
}
