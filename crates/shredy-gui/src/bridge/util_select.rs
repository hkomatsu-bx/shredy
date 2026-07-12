//! ユーティリティ結果（フラット一覧）に対する起点フォルダ基準の選択/ロック対象算出
//! （§3.4 のコンテキストメニュー）。重複検索は core `Selector` を使うが、ユーティリティは
//! グループを持たないフラット一覧のため、ここで同等の判定（直下/ツリー配下）を純関数で行う。
//!
//! 判定は core `Selector` と同じ意味論: 直下＝ファイルの親が起点と一致 / ツリー配下＝起点が
//! ファイルの厳密な祖先（大小無視・パスコンポーネント単位）。

use shredy_core::UtilityItem;

/// 起点フォルダ**直下**の項目 index（`invert` で「直下以外」）。
#[must_use]
pub fn direct_indices(items: &[UtilityItem], origin: &str, invert: bool) -> Vec<usize> {
    let okeys = comp_keys(origin);
    select_where(items, invert, |it| is_direct_child(it, &okeys))
}

/// 起点フォルダの**ツリー配下**の項目 index（`invert` で「ツリー配下以外」）。
#[must_use]
pub fn tree_indices(items: &[UtilityItem], origin: &str, invert: bool) -> Vec<usize> {
    let okeys = comp_keys(origin);
    select_where(items, invert, |it| is_under_tree(it, &okeys))
}

fn select_where(
    items: &[UtilityItem],
    invert: bool,
    pred: impl Fn(&UtilityItem) -> bool,
) -> Vec<usize> {
    (0..items.len())
        .filter(|&i| pred(&items[i]) != invert)
        .collect()
}

/// 拡張長プレフィックス `\\?\`（UNC は `\\?\UNC\`）を取り除く。
///
/// `LongPath::as_path()` は通常プレフィックスなしだが、起点フォルダはメニューで手編集でき
/// `\\?\` 付き入力もあり得る。両辺を同形へそろえ、ドライブ文字単位の比較を安定させる。
fn strip_verbatim(s: &str) -> &str {
    s.strip_prefix("\\\\?\\UNC\\")
        .map(|_| s) // UNC verbatim はそのまま（稀・誤一致を避けるため変形しない）
        .unwrap_or_else(|| s.strip_prefix("\\\\?\\").unwrap_or(s))
}

/// パス文字列を小文字のコンポーネント列へ（大小無視比較用）。起点は 1 度だけ算出する。
fn comp_keys(path: &str) -> Vec<String> {
    std::path::Path::new(strip_verbatim(path.trim()))
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

/// 項目パスが起点キー列を前方一致（祖先）に持つかと、項目のコンポーネント数を返す。
///
/// 項目側は `Vec<String>` を確保せず、コンポーネントを 1 つずつ小文字化して比較する。
/// 不一致が確定した時点・起点長を超えた時点で小文字化を打ち切るため、一覧が大きいほど
/// 確保・走査コストを抑えられる（10k 件規模のコンテキストメニュー選択, H-2）。
fn ancestor_prefix(it: &UtilityItem, okeys: &[String]) -> (bool, usize) {
    let s = it.path.as_path().to_string_lossy();
    let path = strip_verbatim(s.trim());
    let mut matched = true;
    let mut count = 0usize;
    for comp in std::path::Path::new(path).components() {
        // 前方の起点長ぶんのみ比較する（以降はコンポーネント数の計上だけ行う）。
        if matched && count < okeys.len() {
            let c = comp.as_os_str().to_string_lossy().to_lowercase();
            if c != okeys[count] {
                matched = false;
            }
        }
        count += 1;
    }
    // 起点長ぶんを取りこぼさず一致した場合のみ前方一致成立（短すぎる項目は不成立）。
    (matched && count >= okeys.len(), count)
}

/// 項目の親が起点と一致するか（直下）。
fn is_direct_child(it: &UtilityItem, okeys: &[String]) -> bool {
    let (prefix, count) = ancestor_prefix(it, okeys);
    prefix && count == okeys.len() + 1
}

/// 起点が項目の厳密な祖先か（ツリー配下＝直下＋それより深い）。
fn is_under_tree(it: &UtilityItem, okeys: &[String]) -> bool {
    let (prefix, count) = ancestor_prefix(it, okeys);
    prefix && count > okeys.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shredy_core::LongPath;
    use std::time::SystemTime;

    fn item(path: &str) -> UtilityItem {
        UtilityItem {
            path: LongPath::new(path).unwrap(),
            size: 0,
            modified: SystemTime::UNIX_EPOCH,
            is_dir: false,
            detail: String::new(),
        }
    }

    fn items() -> Vec<UtilityItem> {
        vec![
            item("C:\\a\\x.txt"),      // 0: 直下
            item("C:\\a\\y.txt"),      // 1: 直下
            item("C:\\a\\sub\\z.txt"), // 2: ツリー配下（直下ではない）
            item("C:\\b\\w.txt"),      // 3: 対象外
        ]
    }

    #[test]
    fn direct_matches_only_immediate_children() {
        assert_eq!(direct_indices(&items(), "C:\\a", false), vec![0, 1]);
    }

    #[test]
    fn direct_is_case_insensitive() {
        assert_eq!(direct_indices(&items(), "c:\\A", false), vec![0, 1]);
    }

    #[test]
    fn direct_invert_excludes_immediate_children() {
        assert_eq!(direct_indices(&items(), "C:\\a", true), vec![2, 3]);
    }

    #[test]
    fn tree_includes_subfolders() {
        assert_eq!(tree_indices(&items(), "C:\\a", false), vec![0, 1, 2]);
    }

    #[test]
    fn tree_invert_excludes_subtree() {
        assert_eq!(tree_indices(&items(), "C:\\a", true), vec![3]);
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(direct_indices(&items(), "C:\\nope", false).is_empty());
    }

    #[test]
    fn verbatim_prefix_on_origin_still_matches() {
        // 起点フォルダを手編集で `\\?\` 付きにしても、項目（プレフィックスなし）と一致する。
        assert_eq!(direct_indices(&items(), "\\\\?\\C:\\a", false), vec![0, 1]);
        assert_eq!(tree_indices(&items(), "\\\\?\\C:\\a", false), vec![0, 1, 2]);
    }

    #[test]
    fn different_drive_does_not_match() {
        // ドライブ文字は比較対象（クロスドライブ誤一致を防ぐ）。
        assert!(direct_indices(&items(), "D:\\a", false).is_empty());
    }
}
