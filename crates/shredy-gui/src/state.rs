//! UI スレッドで共有するアプリ状態と、その読み書き補助。

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use shredy_core::{
    AppConfig, Cancel, DiffEntry, DuplicateGroup, KeepFileRule, LockSet, LongPath, RecipeStep,
    RenamePlan, RenameStep, RenameUndo, UtilityItem,
};

use shredy_gui::bridge::summary;
use shredy_gui::viewmodel::{FilterMode, SelectionState};

/// 結果セットと保持ルールだけに依存する派生値。
#[derive(Debug, Clone)]
struct ResultCache {
    keep_rule: KeepFileRule,
    dup_file_count: usize,
    potential_reclaim: u64,
    file_row_indices: Vec<Vec<usize>>,
}

impl ResultCache {
    /// 現在の結果セットから派生値を作る。
    #[must_use]
    fn new(groups: &[DuplicateGroup], keep_rule: KeepFileRule) -> Self {
        Self {
            keep_rule,
            dup_file_count: summary::dup_file_count(groups),
            potential_reclaim: summary::potential_reclaim(groups, keep_rule),
            file_row_indices: build_file_row_indices(groups),
        }
    }

    #[must_use]
    fn dup_file_count(&self) -> usize {
        self.dup_file_count
    }

    #[must_use]
    fn potential_reclaim(&self) -> u64 {
        self.potential_reclaim
    }

    fn file_row_index(&self, gi: usize, fi: usize) -> Option<usize> {
        self.file_row_indices
            .get(gi)
            .and_then(|group| group.get(fi))
            .copied()
    }
}

fn build_file_row_indices(groups: &[DuplicateGroup]) -> Vec<Vec<usize>> {
    let mut flat = 0usize;
    groups
        .iter()
        .map(|group| {
            flat += 1; // グループヘッダ行。
            (0..group.files.len())
                .map(|_| {
                    let idx = flat;
                    flat += 1;
                    idx
                })
                .collect()
        })
        .collect()
}

/// UI スレッドで共有するアプリ状態。
pub(crate) struct AppState {
    pub(crate) config: AppConfig,
    pub(crate) config_path: PathBuf,
    pub(crate) config_dir: PathBuf,
    /// 結果セット。削除/移動/保存/再チェックのワーカーへ渡すスナップショットを
    /// ディープクローンではなく `Arc::clone`（参照カウント増のみ）で安価に取れるよう
    /// `Arc` で保持する。中身は `set_groups`/`clear_groups` で丸ごと差し替える（in-place
    /// 変更はしない＝共有読み取りのみ）。
    pub(crate) groups: Arc<Vec<DuplicateGroup>>,
    cache: ResultCache,
    pub(crate) selection: SelectionState,
    pub(crate) locks: LockSet,
    pub(crate) cancel: Option<Cancel>,
    /// 結果一覧の表示フィルタ（選択済み/未選択のみ。表示のみ・選択状態は不変, U12 polish3）。
    pub(crate) filter_mode: FilterMode,
    /// ユーティリティスキャン（§3.9）の結果（フラット一覧）。重複検索の `groups` とは分離し、
    /// ページ遷移時にクリアする。ワーカーへ安価に渡せるよう `Arc` で保持する。
    pub(crate) util_items: Arc<Vec<UtilityItem>>,
    /// ユーティリティ結果の削除選択（`util_items` の index 集合・複数可, §3.4）。
    pub(crate) util_checked: HashSet<usize>,
    /// ユーティリティ結果のハイライト行（単一・§3.4）。
    pub(crate) util_highlighted: Option<usize>,
    /// ユーティリティ結果のロック（削除/隔離から除外, §3.4）。パス基準（重複検索と共通の型）。
    pub(crate) util_locks: LockSet,
    // ── 画像メタデータ除去（§3.10.3・EXIF） ──
    /// EXIF 除去候補（対応拡張子の画像。検出ユーティリティの `util_items` とは分離）。
    pub(crate) exif_items: Arc<Vec<UtilityItem>>,
    /// 除去対象の選択（`exif_items` の index 集合・複数可）。
    pub(crate) exif_checked: HashSet<usize>,
    /// バックアップ（元ファイルを `.bak` へ退避）するか（セッション内トグル・既定 false）。
    pub(crate) exif_backup: bool,
    /// 一括リネーム（§3.10.1）の対象フォルダ（単一・重複検索の TargetFolders とは独立）。
    pub(crate) rename_root: Option<LongPath>,
    /// 一括リネームのレシピ（編集中のステップ列）。
    pub(crate) rename_steps: Vec<RecipeStep>,
    /// 選択中ステップの index（インライン編集の対象。範囲は `rename_steps` と同期）。
    pub(crate) rename_sel: Option<usize>,
    /// 新規追加モード（「＋ステップ追加」直後。方式を選ぶまでステップは追加せず変換も走らない
    /// ＝実機 FB 3 巡目「初期選択をなくしたい」）。
    pub(crate) rename_adding: bool,
    /// 直近のプレビュー計画（変更前→後・衝突）。
    pub(crate) rename_plan: Option<RenamePlan>,
    /// 実行の取り消しスタック（適用ごとに push・「元に戻す」で pop。モック準拠のマルチ Undo）。
    pub(crate) rename_undo: Vec<RenameUndo>,
    /// プレビュー計算の世代（自動プレビューで古いワーカー結果の上書きを防ぐ）。
    pub(crate) rename_preview_gen: u64,
    /// 保存済みリネームレシピ（名前→ステップ列。`shredy.recipes` に永続化・C-M4）。
    pub(crate) recipes: BTreeMap<String, Vec<RecipeStep>>,
    // ── フォルダ比較（§3.7） ──
    pub(crate) folder_diff_a: String,
    pub(crate) folder_diff_b: String,
    /// 表示フィルタ（0=全て / 1=OnlyA / 2=OnlyB / 3=Common / 4=SizeDiff）。
    pub(crate) folder_diff_filter: i32,
    pub(crate) folder_diff_entries: Vec<DiffEntry>,
    pub(crate) folder_diff_status: String,
    pub(crate) folder_diff_comparing: bool,
    pub(crate) folder_diff_cancel: Arc<AtomicBool>,
}

impl AppState {
    /// 起動時の設定から空の状態を作る。
    #[must_use]
    pub(crate) fn new(config: AppConfig, config_path: PathBuf, config_dir: PathBuf) -> Self {
        let cache = ResultCache::new(&[], config.keep_file_rule);
        Self {
            config,
            config_path,
            config_dir,
            groups: Arc::new(Vec::new()),
            cache,
            selection: SelectionState::new(),
            locks: LockSet::new(),
            cancel: None,
            filter_mode: FilterMode::default(),
            util_items: Arc::new(Vec::new()),
            util_checked: HashSet::new(),
            util_highlighted: None,
            util_locks: LockSet::new(),
            exif_items: Arc::new(Vec::new()),
            exif_checked: HashSet::new(),
            exif_backup: false,
            rename_root: None,
            rename_steps: Vec::new(),
            rename_sel: None,
            rename_adding: false,
            rename_plan: None,
            rename_undo: Vec::new(),
            rename_preview_gen: 0,
            recipes: BTreeMap::new(),
            folder_diff_a: String::new(),
            folder_diff_b: String::new(),
            folder_diff_filter: 0,
            folder_diff_entries: Vec::new(),
            folder_diff_status: String::new(),
            folder_diff_comparing: false,
            folder_diff_cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 選択中のステップ（範囲外なら `None`）。
    #[must_use]
    pub(crate) fn rename_selected(&self) -> Option<&RecipeStep> {
        self.rename_sel.and_then(|i| self.rename_steps.get(i))
    }

    /// レシピ末尾にステップを追加し、それを選択する（新規追加モードは解除）。
    pub(crate) fn rename_add_step(&mut self, step: RenameStep) {
        self.rename_steps.push(RecipeStep::new(step));
        self.rename_sel = Some(self.rename_steps.len() - 1);
        self.rename_adding = false;
    }

    /// 選択中ステップの方式・パラメータを置き換える（インライン編集・有効フラグは維持）。
    pub(crate) fn rename_replace_selected(&mut self, step: RenameStep) {
        if let Some(s) = self.rename_sel.and_then(|i| self.rename_steps.get_mut(i)) {
            s.step = step;
        }
    }

    /// 指定 index のステップを削除する（範囲外は無視）。
    ///
    /// 選択中を消した場合は末尾を選択する（モック準拠）。それ以外は選択ステップを追従させる。
    pub(crate) fn rename_remove_step(&mut self, idx: usize) {
        if idx >= self.rename_steps.len() {
            return;
        }
        self.rename_steps.remove(idx);
        let len = self.rename_steps.len();
        self.rename_sel = match self.rename_sel {
            _ if len == 0 => None,
            Some(s) if s == idx => Some(len - 1),
            Some(s) if s > idx => Some(s - 1),
            other => other,
        };
    }

    /// 指定 index のステップを上（`up=true`）または下へ 1 つ移動する（選択は追従）。
    pub(crate) fn rename_move_step(&mut self, idx: usize, up: bool) {
        let len = self.rename_steps.len();
        if idx >= len {
            return;
        }
        let swap = if up {
            idx.checked_sub(1)
        } else if idx + 1 < len {
            Some(idx + 1)
        } else {
            None
        };
        if let Some(j) = swap {
            self.rename_steps.swap(idx, j);
            self.rename_sel = match self.rename_sel {
                Some(s) if s == idx => Some(j),
                Some(s) if s == j => Some(idx),
                other => other,
            };
        }
    }

    /// 指定 index のステップの有効/無効を切り替える（範囲外は無視）。
    pub(crate) fn rename_toggle_step(&mut self, idx: usize) {
        if let Some(s) = self.rename_steps.get_mut(idx) {
            s.enabled = !s.enabled;
        }
    }

    /// レシピを空にする（選択・新規追加モードも解除）。
    pub(crate) fn rename_clear_steps(&mut self) {
        self.rename_steps.clear();
        self.rename_sel = None;
        self.rename_adding = false;
    }

    /// レシピを丸ごと置き換える（保存済みレシピの読込）。先頭を選択する。
    pub(crate) fn rename_set_steps(&mut self, steps: Vec<RecipeStep>) {
        self.rename_sel = if steps.is_empty() { None } else { Some(0) };
        self.rename_steps = steps;
    }

    /// ユーティリティ結果を置き換え、選択・ハイライト・ロックをリセットする。
    pub(crate) fn set_util_items(&mut self, items: Vec<UtilityItem>) {
        self.util_items = Arc::new(items);
        self.util_checked.clear();
        self.util_highlighted = None;
        self.util_locks = LockSet::new();
    }

    /// ユーティリティ結果と選択・ロックを空にする（ページ遷移時）。
    pub(crate) fn clear_util(&mut self) {
        self.util_items = Arc::new(Vec::new());
        self.util_checked.clear();
        self.util_highlighted = None;
        self.util_locks = LockSet::new();
    }

    /// EXIF 除去候補を置き換え、選択をリセットする。
    pub(crate) fn set_exif_items(&mut self, items: Vec<UtilityItem>) {
        self.exif_items = Arc::new(items);
        self.exif_checked.clear();
    }

    /// EXIF 候補と選択を空にする（ページ遷移時）。
    pub(crate) fn clear_exif(&mut self) {
        self.exif_items = Arc::new(Vec::new());
        self.exif_checked.clear();
    }

    /// EXIF 候補の選択をトグルする（範囲外 index は無視＝UI とのタイミングずれ対策）。
    pub(crate) fn exif_toggle_check(&mut self, idx: usize) {
        if idx >= self.exif_items.len() {
            return;
        }
        if !self.exif_checked.remove(&idx) {
            self.exif_checked.insert(idx);
        }
    }

    /// 全 EXIF 候補を選択する。
    pub(crate) fn exif_select_all(&mut self) {
        self.exif_checked = (0..self.exif_items.len()).collect();
    }

    /// EXIF 候補の選択をすべて解除する。
    pub(crate) fn exif_clear_checks(&mut self) {
        self.exif_checked.clear();
    }

    /// 選択中の EXIF 候補のパス（index 昇順）。
    #[must_use]
    pub(crate) fn exif_checked_paths(&self) -> Vec<LongPath> {
        let mut idxs: Vec<usize> = self.exif_checked.iter().copied().collect();
        idxs.sort_unstable();
        idxs.into_iter()
            .filter_map(|i| self.exif_items.get(i))
            .map(|it| it.path.clone())
            .collect()
    }

    /// index の項目がロック済みか。
    #[must_use]
    fn util_is_locked(&self, idx: usize) -> bool {
        self.util_items
            .get(idx)
            .map(|it| self.util_locks.is_locked(&it.path))
            .unwrap_or(false)
    }

    /// ユーティリティ行の削除選択をトグルする（複数可）。
    ///
    /// 範囲外 index・ロック済みは無視する（UI とのタイミングずれで `util_checked` に幽霊
    /// index が残り選択件数表示が過大になるのを防ぐ防御＋ロック保護）。
    pub(crate) fn util_toggle_check(&mut self, idx: usize) {
        if idx >= self.util_items.len() || self.util_is_locked(idx) {
            return;
        }
        if !self.util_checked.remove(&idx) {
            self.util_checked.insert(idx);
        }
    }

    /// 全ユーティリティ行を削除選択する（「1つ残す」は適用しない＝全件可・ロックは除外）。
    pub(crate) fn util_select_all(&mut self) {
        self.util_checked = (0..self.util_items.len())
            .filter(|&i| !self.util_is_locked(i))
            .collect();
    }

    /// ユーティリティの削除選択をすべて解除する。
    pub(crate) fn util_clear_checks(&mut self) {
        self.util_checked.clear();
    }

    /// 指定 index 群を削除選択する（加算・ロック済みは除外, コンテキストメニュー §3.4）。
    pub(crate) fn util_check_indices(&mut self, idxs: &[usize]) {
        // 範囲内かつ未ロックのみ追加する（中間 Vec は不要。判定の借用は挿入前に解放される）。
        for &i in idxs {
            if i < self.util_items.len() && !self.util_is_locked(i) {
                self.util_checked.insert(i);
            }
        }
    }

    /// 指定 index 群をロックする（加算・ロック後は削除選択から除外, §3.4）。
    ///
    /// ロックできた項目のみ削除選択を解除する（存在する index に対して対称に処理し、
    /// 「ロック未適用なのに選択だけ解除」という不整合を避ける）。
    pub(crate) fn util_lock_indices(&mut self, idxs: &[usize]) {
        for &i in idxs {
            // パスを取り出した時点で util_items の借用は解放され、続く mut 借用と競合しない。
            if let Some(path) = self.util_items.get(i).map(|it| it.path.clone()) {
                self.util_locks.lock(&path);
                self.util_checked.remove(&i);
            }
        }
    }

    /// ロックをすべて解除する（§3.4）。
    pub(crate) fn util_unlock_all(&mut self) {
        self.util_locks = LockSet::new();
    }

    /// 選択中ユーティリティ行のパス（index 昇順）。
    #[must_use]
    pub(crate) fn util_checked_paths(&self) -> Vec<LongPath> {
        let mut idxs: Vec<usize> = self.util_checked.iter().copied().collect();
        idxs.sort_unstable();
        idxs.into_iter()
            .filter_map(|i| self.util_items.get(i))
            .map(|it| it.path.clone())
            .collect()
    }

    /// ユーティリティ結果の合計サイズ（飽和加算）。
    #[must_use]
    pub(crate) fn util_total_size(&self) -> u64 {
        self.util_items
            .iter()
            .fold(0u64, |acc, it| acc.saturating_add(it.size))
    }

    /// 選択中ユーティリティ行の合計サイズ（飽和加算）。
    #[must_use]
    pub(crate) fn util_selected_size(&self) -> u64 {
        self.util_checked
            .iter()
            .filter_map(|&i| self.util_items.get(i))
            .fold(0u64, |acc, it| acc.saturating_add(it.size))
    }

    /// 結果セットを置き換え、結果依存サマリを同期する。
    pub(crate) fn set_groups(&mut self, groups: Vec<DuplicateGroup>) {
        self.set_groups_arc(Arc::new(groups));
    }

    /// 既存の `Arc<Vec<DuplicateGroup>>` をそのまま結果セットへ設定する（再確保なし）。
    ///
    /// ワーカーが取得済みのスナップショット（削除対象なしで内容不変の場合など）を、
    /// `Vec` へ複製し直さず共有のまま戻すために使う。
    pub(crate) fn set_groups_arc(&mut self, groups: Arc<Vec<DuplicateGroup>>) {
        self.groups = groups;
        self.refresh_result_cache();
    }

    /// 結果セットを空にし、結果依存サマリを同期する。
    pub(crate) fn clear_groups(&mut self) {
        // `Arc` は in-place の `clear()` ができないため空ベクタへ差し替える。
        self.groups = Arc::new(Vec::new());
        self.refresh_result_cache();
    }

    /// 重複ファイル総数（結果更新時に再計算済み）。
    #[must_use]
    pub(crate) fn dup_file_count(&self) -> usize {
        debug_assert_eq!(self.cache.keep_rule, self.config.keep_file_rule);
        self.cache.dup_file_count()
    }

    /// 全選択時に回収できる最大サイズ（結果更新時に再計算済み）。
    #[must_use]
    pub(crate) fn potential_reclaim(&self) -> u64 {
        debug_assert_eq!(self.cache.keep_rule, self.config.keep_file_rule);
        self.cache.potential_reclaim()
    }

    /// フィルタ OFF 時の `(group, file)` 座標に対応する平坦モデル index。
    #[must_use]
    pub(crate) fn file_row_index(&self, gi: usize, fi: usize) -> Option<usize> {
        self.cache.file_row_index(gi, fi)
    }

    /// 結果依存サマリ（重複総数・最大回収量・平坦 index）を再計算する。
    ///
    /// `cache` は `groups` と `config.keep_file_rule` に依存する。`groups` は
    /// `set_groups`/`clear_groups` 経由でのみ更新されるため同期は保たれるが、
    /// `config.keep_file_rule` を変更するコードパス（オプション設定 §3.8 等）を
    /// 将来追加する場合は、変更直後に本メソッドを呼んでキャッシュを同期すること。
    /// 同期漏れはリリースビルドで無言の誤値となる（debug ビルドは `debug_assert` で検出）。
    /// オプション設定（§3.8）で `keep_file_rule` を変更した直後にも呼ぶ。
    pub(crate) fn refresh_result_cache(&mut self) {
        self.cache = ResultCache::new(&self.groups, self.config.keep_file_rule);
    }
}

/// `AppState` の `Mutex` をロックする。汚染時は記録して内部値で続行する。
pub(crate) fn lock(state: &Arc<Mutex<AppState>>) -> std::sync::MutexGuard<'_, AppState> {
    state.lock().unwrap_or_else(|e| {
        // スレッドがパニックして Mutex が汚染された。状態が不整合の可能性を記録し続行する。
        tracing::error!("AppState の Mutex が汚染されています（スレッドがパニック）。状態が不整合の可能性があります");
        e.into_inner()
    })
}

/// 設定を保存する。`save_folder_check_state=false` の場合はチェック状態を保存しない。
pub(crate) fn save_config(st: &AppState) {
    let mut to_save = st.config.clone();
    if !to_save.save_folder_check_state {
        for tf in &mut to_save.target_folders {
            tf.checked = false;
        }
    }
    if let Err(e) = to_save.save(&st.config_path) {
        tracing::error!("設定の保存に失敗しました: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shredy_core::{FileEntry, FileSize};
    use std::time::{Duration, UNIX_EPOCH};

    fn entry(path: &str, size: u64, secs_after_epoch: u64) -> FileEntry {
        FileEntry {
            path: shredy_core::LongPath::new(path).unwrap(),
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
    fn result_cache_counts_files_and_potential_reclaim() {
        let groups = vec![group(vec![entry("C:\\a", 100, 1), entry("C:\\b", 200, 2)])];
        let cache = ResultCache::new(&groups, KeepFileRule::ByDateNewest);
        assert_eq!(cache.dup_file_count(), 2);
        assert_eq!(cache.potential_reclaim(), 100);
    }

    fn util_item(path: &str, size: u64, is_dir: bool) -> UtilityItem {
        UtilityItem {
            path: LongPath::new(path).unwrap(),
            size,
            modified: UNIX_EPOCH,
            is_dir,
            detail: String::new(),
        }
    }

    fn empty_state() -> AppState {
        AppState::new(
            AppConfig::default(),
            PathBuf::from("C:\\cfg\\shredy.config"),
            PathBuf::from("C:\\cfg"),
        )
    }

    #[test]
    fn util_select_all_checks_every_item_and_sizes_sum() {
        let mut st = empty_state();
        st.set_util_items(vec![
            util_item("C:\\a.tmp", 100, false),
            util_item("C:\\b.tmp", 250, false),
            util_item("C:\\empty", 0, true),
        ]);
        st.util_select_all();
        assert_eq!(st.util_checked.len(), 3);
        assert_eq!(st.util_total_size(), 350);
        assert_eq!(st.util_selected_size(), 350);
        // パスは index 昇順で返る。
        let paths = st.util_checked_paths();
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0].as_path(), std::path::Path::new("C:\\a.tmp"));
    }

    #[test]
    fn util_toggle_and_clear() {
        let mut st = empty_state();
        st.set_util_items(vec![
            util_item("C:\\a", 10, false),
            util_item("C:\\b", 20, false),
        ]);
        st.util_toggle_check(1);
        assert!(st.util_checked.contains(&1));
        assert_eq!(st.util_selected_size(), 20);
        // 再トグルで解除。
        st.util_toggle_check(1);
        assert!(st.util_checked.is_empty());
        st.util_toggle_check(0);
        st.util_clear_checks();
        assert!(st.util_checked.is_empty());
    }

    #[test]
    fn util_lock_excludes_from_selection() {
        let mut st = empty_state();
        st.set_util_items(vec![
            util_item("C:\\a", 10, false),
            util_item("C:\\b", 20, false),
        ]);
        // ロックすると削除選択から外れ、トグルでも選択できない。
        st.util_check_indices(&[0, 1]);
        assert_eq!(st.util_checked.len(), 2);
        st.util_lock_indices(&[0]);
        assert!(!st.util_checked.contains(&0), "ロックで選択解除される");
        st.util_toggle_check(0);
        assert!(
            !st.util_checked.contains(&0),
            "ロック済みはトグルで選択不可"
        );
        // 全選択もロックを除外。
        st.util_clear_checks();
        st.util_select_all();
        assert_eq!(st.util_checked.len(), 1);
        assert!(st.util_checked.contains(&1));
        // 全ロック解除で再び選択可能。
        st.util_unlock_all();
        st.util_toggle_check(0);
        assert!(st.util_checked.contains(&0));
    }

    #[test]
    fn util_check_indices_skips_out_of_range() {
        let mut st = empty_state();
        st.set_util_items(vec![util_item("C:\\a", 10, false)]);
        st.util_check_indices(&[0, 5, 99]);
        assert_eq!(st.util_checked.len(), 1);
        assert!(st.util_checked.contains(&0));
    }

    #[test]
    fn set_util_items_resets_selection() {
        let mut st = empty_state();
        st.set_util_items(vec![util_item("C:\\a", 10, false)]);
        st.util_toggle_check(0);
        st.util_highlighted = Some(0);
        // 新しい結果に差し替えると選択・ハイライトはリセット。
        st.set_util_items(vec![util_item("C:\\x", 5, false)]);
        assert!(st.util_checked.is_empty());
        assert_eq!(st.util_highlighted, None);
    }

    #[test]
    fn result_cache_maps_coordinates_to_flat_file_rows() {
        let groups = vec![
            group(vec![entry("C:\\a", 100, 1), entry("C:\\b", 200, 2)]),
            group(vec![entry("C:\\c", 300, 3)]),
        ];
        let cache = ResultCache::new(&groups, KeepFileRule::ByDateNewest);
        assert_eq!(cache.file_row_index(0, 0), Some(1));
        assert_eq!(cache.file_row_index(0, 1), Some(2));
        assert_eq!(cache.file_row_index(1, 0), Some(4));
        assert_eq!(cache.file_row_index(1, 1), None);
        assert_eq!(cache.file_row_index(2, 0), None);
    }
}
