//! 選択状態のアプリ側保持（仕様 §3.4 の「二概念」）。
//!
//! Slint 非依存の純ロジック。「行選択（ハイライト・単一）」と
//! 「削除選択（チェック・複数）」を別概念として保持し、UI とコア配線の橋渡しに使う。

use std::collections::HashSet;

/// グループ・行の複合キー（`(group_index, file_index)`）。
pub type RowKey = (usize, usize);

/// 結果一覧の表示フィルタ（表示のみ・選択状態は不変, U12 polish3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterMode {
    /// すべてのグループを表示。
    #[default]
    Off,
    /// チェック（削除選択）が 1 件以上あるグループのみ表示。
    CheckedOnly,
    /// チェックが 1 件も無いグループのみ表示。
    UncheckedOnly,
}

impl FilterMode {
    /// Slint へ渡す int 表現（0=Off / 1=CheckedOnly / 2=UncheckedOnly）。
    #[must_use]
    pub fn as_i32(self) -> i32 {
        match self {
            FilterMode::Off => 0,
            FilterMode::CheckedOnly => 1,
            FilterMode::UncheckedOnly => 2,
        }
    }

    /// Slint からの int を復元する（範囲外は Off）。
    #[must_use]
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => FilterMode::CheckedOnly,
            2 => FilterMode::UncheckedOnly,
            _ => FilterMode::Off,
        }
    }

    /// メニュー項目の再選択でトグルする（同一モードなら Off へ戻す）。
    #[must_use]
    pub fn toggled(self, picked: FilterMode) -> Self {
        if self == picked {
            FilterMode::Off
        } else {
            picked
        }
    }

    /// `has_checked`（そのグループにチェックが 1 件以上あるか）から、このグループを表示するか判定する。
    #[must_use]
    pub fn group_visible(self, has_checked: bool) -> bool {
        match self {
            FilterMode::Off => true,
            FilterMode::CheckedOnly => has_checked,
            FilterMode::UncheckedOnly => !has_checked,
        }
    }
}

/// 重複検索ページの選択状態。
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    // ハイライト（行フォーカス）の真実源（U12 M7 で昇格）。`view::refresh_ui` が
    // ここから Slint の highlighted_group/file を反映する。§3.4 改の「フォーカス→トグル」
    // 判定もこの値を基準に行う。
    highlighted: Option<RowKey>,
    checked: HashSet<RowKey>,
}

impl SelectionState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 行をハイライトする（単一・他行は自動解除）。シングルクリック相当。
    pub fn highlight(&mut self, key: RowKey) {
        self.highlighted = Some(key);
    }

    /// 現在ハイライト中の行。
    #[must_use]
    pub fn highlighted(&self) -> Option<RowKey> {
        self.highlighted
    }

    /// 削除選択（チェック）をトグルする（複数可）。ダブルクリック / チェックボックス相当。
    pub fn toggle_check(&mut self, key: RowKey) {
        if !self.checked.remove(&key) {
            self.checked.insert(key);
        }
    }

    /// 指定行のチェックを明示的に設定する（全選択など決定的に組む用途）。
    pub fn set_checked(&mut self, key: RowKey, checked: bool) {
        if checked {
            self.checked.insert(key);
        } else {
            self.checked.remove(&key);
        }
    }

    /// 指定行がチェックされているか。
    #[must_use]
    pub fn is_checked(&self, key: RowKey) -> bool {
        self.checked.contains(&key)
    }

    /// 削除選択をすべて解除する（「選択解除」アクション）。ハイライトは保持する。
    pub fn clear_checks(&mut self) {
        self.checked.clear();
    }

    /// チェック済み件数。
    #[must_use]
    pub fn checked_count(&self) -> usize {
        self.checked.len()
    }

    /// チェック済み行キーを任意順で走査する（集計用・非確保）。
    pub fn iter_checked(&self) -> impl Iterator<Item = RowKey> + '_ {
        self.checked.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_is_single_and_replaces_previous() {
        let mut s = SelectionState::new();
        s.highlight((0, 1));
        assert_eq!(s.highlighted(), Some((0, 1)));
        s.highlight((1, 0));
        // 単一: 直前のハイライトは置き換わる。
        assert_eq!(s.highlighted(), Some((1, 0)));
    }

    #[test]
    fn check_toggles_and_allows_multiple() {
        let mut s = SelectionState::new();
        s.toggle_check((0, 0));
        s.toggle_check((0, 2));
        assert!(s.is_checked((0, 0)));
        assert!(s.is_checked((0, 2)));
        assert_eq!(s.checked_count(), 2);
        // 同じ行を再トグルで解除。
        s.toggle_check((0, 0));
        assert!(!s.is_checked((0, 0)));
        assert_eq!(s.checked_count(), 1);
    }

    #[test]
    fn highlight_does_not_affect_checks() {
        let mut s = SelectionState::new();
        s.toggle_check((0, 0));
        s.highlight((0, 0));
        // ハイライトはチェック状態に影響しない（二概念の独立性）。
        assert!(s.is_checked((0, 0)));
        assert_eq!(s.highlighted(), Some((0, 0)));
    }

    #[test]
    fn clear_checks_keeps_highlight() {
        let mut s = SelectionState::new();
        s.highlight((1, 1));
        s.toggle_check((0, 0));
        s.toggle_check((0, 1));
        s.clear_checks();
        assert_eq!(s.checked_count(), 0);
        assert_eq!(s.highlighted(), Some((1, 1)));
    }

    #[test]
    fn set_checked_is_deterministic() {
        let mut s = SelectionState::new();
        s.set_checked((0, 0), true);
        assert!(s.is_checked((0, 0)));
        // 冪等: true を二度設定しても 1 件。
        s.set_checked((0, 0), true);
        assert_eq!(s.checked_count(), 1);
        s.set_checked((0, 0), false);
        assert!(!s.is_checked((0, 0)));
        // 未選択を false にしても問題ない。
        s.set_checked((9, 9), false);
        assert_eq!(s.checked_count(), 0);
    }

    #[test]
    fn iter_checked_yields_checked_rows_without_allocating_sorted_vec() {
        let mut s = SelectionState::new();
        s.toggle_check((1, 0));
        s.toggle_check((0, 2));
        let mut keys: Vec<RowKey> = s.iter_checked().collect();
        keys.sort_unstable();
        assert_eq!(keys, vec![(0, 2), (1, 0)]);
    }

    #[test]
    fn filter_mode_group_visibility() {
        // Off は常に表示。
        assert!(FilterMode::Off.group_visible(true));
        assert!(FilterMode::Off.group_visible(false));
        // 選択済みのみ: チェックありだけ表示。
        assert!(FilterMode::CheckedOnly.group_visible(true));
        assert!(!FilterMode::CheckedOnly.group_visible(false));
        // 未選択のみ: チェックなしだけ表示。
        assert!(!FilterMode::UncheckedOnly.group_visible(true));
        assert!(FilterMode::UncheckedOnly.group_visible(false));
    }

    #[test]
    fn filter_mode_i32_roundtrips() {
        for m in [
            FilterMode::Off,
            FilterMode::CheckedOnly,
            FilterMode::UncheckedOnly,
        ] {
            assert_eq!(FilterMode::from_i32(m.as_i32()), m);
        }
        // 範囲外は Off。
        assert_eq!(FilterMode::from_i32(99), FilterMode::Off);
    }

    #[test]
    fn filter_mode_toggle_returns_to_off_on_reselect() {
        // 同一モードの再選択で Off へ。
        assert_eq!(
            FilterMode::CheckedOnly.toggled(FilterMode::CheckedOnly),
            FilterMode::Off
        );
        // 別モードの選択は切り替え。
        assert_eq!(
            FilterMode::CheckedOnly.toggled(FilterMode::UncheckedOnly),
            FilterMode::UncheckedOnly
        );
        // Off から選択でそのモードへ。
        assert_eq!(
            FilterMode::Off.toggled(FilterMode::CheckedOnly),
            FilterMode::CheckedOnly
        );
        // Off の再選択は Off のまま（無害・呼び出し元仕様外だが定義しておく）。
        assert_eq!(FilterMode::Off.toggled(FilterMode::Off), FilterMode::Off);
    }
}
