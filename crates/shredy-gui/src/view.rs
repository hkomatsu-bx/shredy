//! Slint モデルへの UI 反映と、UI スレッドへのディスパッチ／表示用文言。

use slint::{Model, ModelRc, SharedString, VecModel};

use shredy_core::{
    CompareMethod, DiffKind, DuplicateGroup, FileEntry, KeepFileRule, LongPath, Phase, RenameEntry,
    UtilityItem, UtilityKind,
};

use shredy_gui::bridge::{convert, folder_diff as fdiff, rename, summary};

use crate::state::AppState;
use crate::{
    AppWindow, DiffRow, ExifRow, FolderRow, ListRow, Page, RecipeRow, RenamePreviewRow,
    RenameStepRow, UtilRow,
};

/// 状態（core 結果 + 選択 + 設定）から Slint モデルを **全再構築** して反映する。
///
/// 結果行・設定フラグ・フォルダ一覧・サマリのすべてを更新する。検索完了・削除・全選択・
/// フィルタ切替などの **結果行が変わる** 構造変化時に使う。結果行が変わらない操作
/// （フォルダのチェック切替・比較方式変更など）は [`refresh_folders`] / [`refresh_summary`]
/// を直接呼び、全 2000+ 行の再構築（`chrono` 整形含む, U12 perf）を避ける。
/// 単一行のチェック/ロックのトグルは [`update_file_row`] で差分更新する。
pub(crate) fn refresh_ui(ui: &AppWindow, st: &AppState) {
    refresh_rows(ui, st);
    refresh_status_flags(ui, st);
    refresh_folders(ui, st); // refresh_summary を内包する。
}

/// 結果行モデルを全再構築し、行集合に依存する UI（チェック数・結果保持フラグ・ハイライト）
/// を反映する。結果行が変わる操作でのみ呼ぶ（最も重い処理）。
fn refresh_rows(ui: &AppWindow, st: &AppState) {
    ui.set_rows(ModelRc::new(VecModel::from(build_list_rows(st))));
    ui.set_checked_count(st.selection.checked_count() as i32);
    // ナビゲーションガード用：結果保持中フラグ（設計 §2.3）。
    ui.set_has_results(!st.groups.is_empty());
    // ハイライト行（§3.4 改: 真実源は SelectionState。Slint へ反映）。
    sync_highlight(ui, st);
}

/// ステータスバーの主要設定 ON/OFF（U12 M5）。ログは LogLevel が None（=OFF）なら無効表示。
pub(crate) fn refresh_status_flags(ui: &AppWindow, st: &AppState) {
    ui.set_confirm_on(st.config.confirm_before_delete);
    ui.set_cache_on(st.config.use_hash_cache);
    ui.set_log_on(st.config.log_level.to_filter() != tracing::level_filters::LevelFilter::OFF);
}

/// 登録フォルダ一覧モデルを再構築し、サマリ（フォルダチェック数等）を同期する。
///
/// 結果行には依存しないため、フォルダの追加/削除/チェック切替では本関数を呼び、
/// 結果行の全再構築（`refresh_rows`）を避ける。
pub(crate) fn refresh_folders(ui: &AppWindow, st: &AppState) {
    let folders: Vec<FolderRow> = st
        .config
        .target_folders
        .iter()
        .map(|tf| FolderRow {
            path: tf.path.clone().into(),
            checked: tf.checked,
        })
        .collect();
    ui.set_target_folders(ModelRc::new(VecModel::from(folders)));
    refresh_summary(ui, st);
}

/// ハイライト行（§3.4 改）を Slint の `highlighted_group/file` へ反映する**単一の真実源**。
///
/// 真実源は `SelectionState::highlighted()`。差分更新（行クリック/ダブルクリック）でも
/// 全再構築（`refresh_ui`）でも本関数を経由し、Slint プロパティの直接操作を一箇所に集約する
/// （二重管理の解消, polish3 / H-M4b）。`ListView` のモデルは不変なのでデリゲートが
/// リアクティブに再描画され、全再構築は発生しない。
pub(crate) fn sync_highlight(ui: &AppWindow, st: &AppState) {
    match st.selection.highlighted() {
        Some((g, f)) => {
            ui.set_highlighted_group(g as i32);
            ui.set_highlighted_file(f as i32);
        }
        None => {
            ui.set_highlighted_group(-1);
            ui.set_highlighted_file(-1);
        }
    }
}

/// サマリバー・ステータス選択サマリ・比較方式/保持ルールのラベルを反映する（U12 M6）。
///
/// 結果行モデルには触れない安価な更新。比較方式変更（結果行は不変）ではこれのみ呼ぶ。
pub(crate) fn refresh_summary(ui: &AppWindow, st: &AppState) {
    let rule = st.config.keep_file_rule;
    let potential = st.potential_reclaim();
    let total = summary::total_reclaim(&st.groups, &st.selection);
    let checked = st.selection.checked_count();

    // Slint の int は i32。非現実的だが上限超過時は飽和させる（負値化を防ぐ）。
    ui.set_summary_groups(i32::try_from(st.groups.len()).unwrap_or(i32::MAX));
    ui.set_summary_files(i32::try_from(st.dup_file_count()).unwrap_or(i32::MAX));
    ui.set_summary_potential(convert::format_size(potential).into());
    ui.set_recycle_on(st.config.use_recycle_bin);
    ui.set_compare_method_label(compare_method_label(st.config.compare_method).into());
    ui.set_compare_method_index(compare_method_index(st.config.compare_method));
    ui.set_keep_rule_label(keep_rule_label(rule).into());
    ui.set_filter_mode(st.filter_mode.as_i32());
    ui.set_folders_checked(
        st.config
            .target_folders
            .iter()
            .filter(|t| t.checked)
            .count() as i32,
    );

    let sel_info = if checked > 0 {
        format!(
            "{checked} 件を削除選択中 — {} を回収",
            convert::format_size(total)
        )
    } else if !st.groups.is_empty() {
        format!(
            "全選択（1つ残す）で {} を回収できます",
            convert::format_size(potential)
        )
    } else {
        String::new()
    };
    ui.set_selection_info(sel_info.into());
}

// ── ユーティリティスキャン（§3.9）の UI 反映 ─────────────────────

/// ユーティリティ結果（フラット一覧）と件数・サイズ・選択サマリを反映する。
///
/// 重複検索の `refresh_ui` とは別系統（結果が `UtilityItem` のフラット一覧で構造が異なる）。
/// 行数は通常控えめなため毎回全再構築する（チェック切替も含む）。
pub(crate) fn refresh_util(ui: &AppWindow, st: &AppState) {
    let rows: Vec<UtilRow> = st
        .util_items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            util_row(
                i,
                it,
                st.util_checked.contains(&i),
                st.util_locks.is_locked(&it.path),
            )
        })
        .collect();
    ui.set_util_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_util_found_count(i32::try_from(st.util_items.len()).unwrap_or(i32::MAX));
    ui.set_util_checked_count(i32::try_from(st.util_checked.len()).unwrap_or(i32::MAX));
    ui.set_util_total_size(convert::format_size(st.util_total_size()).into());
    ui.set_util_selected_size(convert::format_size(st.util_selected_size()).into());
    ui.set_util_highlighted(st.util_highlighted.map(|i| i as i32).unwrap_or(-1));
    // ナビゲーションガード用：ユーティリティ結果保持中フラグ（§2.3）。
    ui.set_util_has_results(!st.util_items.is_empty());
}

/// EXIF 除去候補ページ（§3.10.3）の表示状態を更新する。
pub(crate) fn refresh_exif(ui: &AppWindow, st: &AppState) {
    let rows: Vec<ExifRow> = st
        .exif_items
        .iter()
        .enumerate()
        .map(|(i, it)| exif_row(i, it, st.exif_checked.contains(&i)))
        .collect();
    ui.set_exif_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_exif_found_count(i32::try_from(st.exif_items.len()).unwrap_or(i32::MAX));
    ui.set_exif_checked_count(i32::try_from(st.exif_checked.len()).unwrap_or(i32::MAX));
    ui.set_exif_backup(st.exif_backup);
}

/// 1 EXIF 候補を表示行へ変換する。
fn exif_row(idx: usize, it: &UtilityItem, checked: bool) -> ExifRow {
    let (folder, name) = util_folder_and_name(it);
    ExifRow {
        idx: i32::try_from(idx).unwrap_or(i32::MAX),
        name: name.into(),
        folder: folder.into(),
        size_text: convert::format_size(it.size).into(),
        ext: it.detail.clone().into(),
        checked,
    }
}

/// 1 ユーティリティ項目を表示行へ変換する。フォルダ・リンクはサイズを `"—"` 表示にする。
fn util_row(idx: usize, it: &UtilityItem, checked: bool, locked: bool) -> UtilRow {
    let (folder, name) = util_folder_and_name(it);
    let size_text = if it.is_dir {
        SharedString::from("—")
    } else {
        convert::format_size(it.size).into()
    };
    UtilRow {
        idx: idx as i32,
        name: name.into(),
        folder: folder.into(),
        size_text,
        modified: convert::format_modified(it.modified).into(),
        detail: it.detail.clone().into(),
        is_dir: it.is_dir,
        checked,
        locked,
    }
}

/// ユーティリティ項目の (フォルダ, 名前) 表示文字列。
fn util_folder_and_name(it: &UtilityItem) -> (String, String) {
    let p = it.path.as_path();
    let folder = p
        .parent()
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned());
    (folder, name)
}

/// 単一ユーティリティ行のチェック/ロック状態を差分更新する（全再構築を避ける, M-2）。
///
/// `util_rows` は `util_items` と 1:1（フィルタによる行の出入りがない）ため、トグル対象の
/// 1 行のみ `set_row_data` で更新できる。多数行が変わる一括選択/ロック/全選択は `refresh_util`
/// を使う。チェック数・選択サイズの安価なサマリは併せて反映する。
pub(crate) fn update_util_row(ui: &AppWindow, st: &AppState, idx: usize) {
    let Some(it) = st.util_items.get(idx) else {
        return;
    };
    let rows = ui.get_util_rows();
    let Some(vm) = rows.as_any().downcast_ref::<VecModel<UtilRow>>() else {
        // モデル種別が想定外（通常起きない）。安全側で全再構築へフォールバック。
        refresh_util(ui, st);
        return;
    };
    let Some(mut row) = vm.row_data(idx) else {
        return;
    };
    row.checked = st.util_checked.contains(&idx);
    row.locked = st.util_locks.is_locked(&it.path);
    vm.set_row_data(idx, row);
    ui.set_util_checked_count(i32::try_from(st.util_checked.len()).unwrap_or(i32::MAX));
    ui.set_util_selected_size(convert::format_size(st.util_selected_size()).into());
}

// ── フォルダ比較（§3.7）の UI 反映 ──────────────────────────────

/// フォルダ比較結果（フィルタ後）を ListView モデルへ反映し、ステータスと比較中フラグを同期する。
pub(crate) fn refresh_folder_diff(ui: &AppWindow, st: &AppState) {
    let rows: Vec<DiffRow> = fdiff::filter_entries(&st.folder_diff_entries, st.folder_diff_filter)
        .into_iter()
        .map(|e| DiffRow {
            rel_path: e.rel_path.as_str().into(),
            kind: match e.kind {
                DiffKind::OnlyA => 0,
                DiffKind::OnlyB => 1,
                DiffKind::Common => 2,
                DiffKind::SizeDiff => 3,
            },
            size_a: e
                .size_a
                .map(convert::format_size)
                .unwrap_or_default()
                .into(),
            size_b: e
                .size_b
                .map(convert::format_size)
                .unwrap_or_default()
                .into(),
        })
        .collect();
    ui.set_folder_diff_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_folder_diff_status(st.folder_diff_status.as_str().into());
    ui.set_folder_diff_comparing(st.folder_diff_comparing);
}

// ── 一括リネーム（§3.10.1）の UI 反映 ────────────────────────────

/// 一括リネームのステップ一覧・インライン編集フォーム・プレビュー・レシピ一覧を反映する。
///
/// ステップ編集（追加/削除/並べ替え/有効切替/選択）やプレビュー再計算のたびに呼ぶ。
/// 行数は通常控えめなため毎回全再構築する。
pub(crate) fn refresh_rename(ui: &AppWindow, st: &AppState) {
    ui.set_rename_steps(ModelRc::new(VecModel::from(build_rename_step_rows(st))));
    ui.set_rename_sel(st.rename_sel.map(|i| i as i32).unwrap_or(-1));
    refresh_rename_edit_form(ui, st);

    // プレビュー（# / 変更前 / → / 変更後・パス併記。コピー/移動は出力先を表示）。
    let mode = ui.get_rename_mode();
    let dest = ui.get_rename_dest().to_string();
    let changed_only = ui.get_rename_show_changed_only();
    let (rows, total, changed, conflicts) = match &st.rename_plan {
        Some(plan) => (
            build_rename_rows(&plan.entries, mode, &dest, changed_only),
            plan.entries.len(),
            plan.changed_count(),
            plan.conflict_count(),
        ),
        None => (Vec::new(), 0, 0, 0),
    };
    ui.set_rename_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_rename_total_count(i32::try_from(total).unwrap_or(i32::MAX));
    ui.set_rename_changed_count(i32::try_from(changed).unwrap_or(i32::MAX));
    ui.set_rename_conflict_count(i32::try_from(conflicts).unwrap_or(i32::MAX));
    ui.set_rename_root(
        st.rename_root
            .as_ref()
            .map(|p| p.as_path().to_string_lossy().into_owned())
            .unwrap_or_else(|| "（未選択）".to_string())
            .into(),
    );
    ui.set_rename_undo_depth(i32::try_from(st.rename_undo.len()).unwrap_or(i32::MAX));
    ui.set_recipe_rows(ModelRc::new(VecModel::from(build_recipe_rows(st))));
}

/// ステップ列を表示行へ変換する（方式名＋設定要約＋有効フラグ）。
fn build_rename_step_rows(st: &AppState) -> Vec<RenameStepRow> {
    st.rename_steps
        .iter()
        .enumerate()
        .map(|(i, s)| RenameStepRow {
            idx: i as i32,
            name: rename::method_name(&s.step).into(),
            summary: rename::step_summary(&s.step).into(),
            enabled: s.enabled,
        })
        .collect()
}

/// 選択中ステップ（または新規追加モード）をインライン編集フォームへ展開する。
///
/// モック: 行選択→その場で編集。同値 set は Slint 側で変更通知にならないため、編集中の
/// フォーカスを乱さない。新規追加モード（-2）はカテゴリ/方式とも未選択のエディタを出す
/// （方式選択で実追加）、選択なし（-1）はエディタ非表示。
fn refresh_rename_edit_form(ui: &AppWindow, st: &AppState) {
    if st.rename_adding {
        ui.set_rename_edit_method(-2);
        return;
    }
    let Some(s) = st.rename_selected() else {
        ui.set_rename_edit_method(-1);
        return;
    };
    let f = rename::decompose(&s.step);
    // EXIF: パターン中のトークンに応じて書式/連番パラメータの表示を切替（モック準拠）。
    // t1 をムーブする前に判定する（後続で参照できなくなるため）。
    let uses_date = f.t1.contains("{撮影日時}");
    let uses_seq = f.t1.contains("{連番}");
    ui.set_rename_edit_method(f.method);
    ui.set_rename_edit_t1(f.t1.into());
    ui.set_rename_edit_t2(f.t2.into());
    ui.set_rename_edit_n1(f.n1);
    ui.set_rename_edit_n2(f.n2);
    ui.set_rename_edit_fmt(f.fmt);
    ui.set_rename_edit_pos(f.pos);
    ui.set_rename_edit_target(f.target);
    ui.set_rename_edit_conv(f.conv);
    ui.set_rename_edit_uses_date(uses_date);
    ui.set_rename_edit_uses_seq(uses_seq);
}

/// 保存済みレシピ一覧（フライアウト・C-M4）の行モデルを作る。
fn build_recipe_rows(st: &AppState) -> Vec<RecipeRow> {
    st.recipes
        .iter()
        .map(|(name, steps)| RecipeRow {
            name: name.clone().into(),
            count: i32::try_from(steps.len()).unwrap_or(i32::MAX),
        })
        .collect()
}

/// 計画エントリをフォルダ見出し付きのプレビュー行へ変換する（実機 FB 3 巡目）。
///
/// エントリはフルパス昇順（`build_plan` が保証）のため、親フォルダが変わる箇所に見出し行を
/// 挟む。ファイル行は名前のみ（dir 併記をやめ、ファイル名が切れないようにする）。フルパスは
/// `tip`（行ホバーのツールチップ）で示す。コピー/移動モード（`mode != 0`）では見出しに
/// 「→ 出力先」を併記する。`changed_only` で変更/衝突行のみに絞り込む（番号は元の連番を
/// 維持・空になったグループの見出しは出さない）。
fn build_rename_rows(
    entries: &[RenameEntry],
    mode: i32,
    dest: &str,
    changed_only: bool,
) -> Vec<RenamePreviewRow> {
    let dest_dir = (mode != 0 && !dest.trim().is_empty())
        .then(|| format!("{}\\", dest.trim().trim_end_matches('\\')));
    let mut rows = Vec::with_capacity(entries.len() + HEADER_ROWS_RESERVE);
    let mut last_dir: Option<std::path::PathBuf> = None;
    for (i, e) in entries.iter().enumerate() {
        if changed_only && !e.changed && e.issue.is_none() {
            continue;
        }
        let dir = e.path.as_path().parent().map(std::path::Path::to_path_buf);
        if dir != last_dir {
            rows.push(dir_header_row(dir.as_deref(), dest_dir.as_deref()));
            last_dir = dir;
        }
        let before_dir = e
            .path
            .as_path()
            .parent()
            .map(|p| format!("{}\\", p.display()))
            .unwrap_or_default();
        // 出力先はループ不変なので借用する（未指定時は変更前フォルダ）。
        let after_dir = dest_dir.as_deref().unwrap_or(&before_dir);
        // tip は行ホバーのツールチップ本文＝行データとして必須のペイロード（遅延生成不可）。
        // プレビューはフォルダ単位で表示され実質数百行が上限のため、行ごとの format! は許容する。
        let tip = format!("{before_dir}{}\n→ {after_dir}{}", e.original, e.renamed);
        rows.push(RenamePreviewRow {
            is_header: false,
            header: SharedString::new(),
            num: i32::try_from(i + 1).unwrap_or(i32::MAX),
            is_dir: e.is_dir,
            before_name: e.original.clone().into(),
            after_name: e.renamed.clone().into(),
            tip: tip.into(),
            changed: e.changed,
            conflict: e.issue.is_some(),
        });
    }
    rows
}

/// プレビュー行ベクタの初期容量に上乗せするフォルダ見出し行の概算予約数。
///
/// 見出し数はフォルダ数（事前に判らない）だが、典型ケース（少数フォルダ）ではこの固定予約で
/// 再確保を避けられる。多数フォルダでも超過時に一度だけ再確保されるのみ。
const HEADER_ROWS_RESERVE: usize = 8;

/// フォルダ見出し行を作る。表示用に `\\?\` ロングパスプレフィックスを剥がし（見た目ノイズ）、
/// コピー/移動モード（`dest_dir` が `Some`）では「→ 出力先」を併記する。
fn dir_header_row(dir: Option<&std::path::Path>, dest_dir: Option<&str>) -> RenamePreviewRow {
    let dir_text = dir
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir_text = dir_text
        .strip_prefix(r"\\?\")
        .map(str::to_string)
        .unwrap_or(dir_text);
    let header = match dest_dir {
        Some(dest) => format!("{dir_text}  →  {}", dest.trim_end_matches('\\')),
        None => dir_text,
    };
    RenamePreviewRow {
        is_header: true,
        header: header.into(),
        num: 0,
        is_dir: false,
        before_name: SharedString::new(),
        after_name: SharedString::new(),
        tip: SharedString::new(),
        changed: false,
        conflict: false,
    }
}

/// ページ → ユーティリティスキャン種別（重複検索・一括・設定は `None`）。
#[must_use]
pub(crate) fn page_to_kind(p: Page) -> Option<UtilityKind> {
    match p {
        Page::UtilEmpty => Some(UtilityKind::Empty),
        Page::UtilTemp => Some(UtilityKind::Temp),
        Page::UtilLarge => Some(UtilityKind::LargeFile),
        Page::UtilCorrupt => Some(UtilityKind::Corrupt),
        Page::UtilExt => Some(UtilityKind::WrongExtension),
        Page::UtilSymlink => Some(UtilityKind::InvalidSymlink),
        _ => None,
    }
}

/// 比較方式 → 表示名（アクションバー比較方式プルダウン）。
pub(crate) fn compare_method_label(m: CompareMethod) -> &'static str {
    match m {
        CompareMethod::HashBlake3 => "ハッシュ (BLAKE3)",
        CompareMethod::HashSha256 => "ハッシュ (SHA-256)",
        CompareMethod::HashXxh128 => "ハッシュ (XXH3-128)",
        CompareMethod::PartialMatch => "部分一致",
        CompareMethod::NameSizeDate => "名前+サイズ+日付",
        CompareMethod::NameSize => "名前+サイズ",
        CompareMethod::NameOnly => "名前のみ",
        CompareMethod::OfficeSimilarity => "Office 類似度",
        CompareMethod::ImageSimilarity => "画像類似度",
    }
}

/// 比較方式 → プルダウンの index（チェック表示用・`compare_method_from_index` と対）。
///
/// 対応表は core（`CompareMethod::index`）が単一の真実源。Slint の int は i32 のため変換する。
pub(crate) fn compare_method_index(m: CompareMethod) -> i32 {
    i32::from(m.index())
}

/// プルダウン index → 比較方式（範囲外は `None`）。core（`CompareMethod::from_index`）へ委譲。
pub(crate) fn compare_method_from_index(i: i32) -> Option<CompareMethod> {
    u8::try_from(i).ok().and_then(CompareMethod::from_index)
}

/// 保持ルール → 表示名（サマリチップ）。
pub(crate) fn keep_rule_label(r: KeepFileRule) -> &'static str {
    match r {
        KeepFileRule::ByDateNewest => "更新日（最新）を保持",
        KeepFileRule::ByDateOldest => "更新日（最古）を保持",
        KeepFileRule::ByPathAsc => "パス順（昇順）を保持",
        KeepFileRule::ByPathDesc => "パス順（降順）を保持",
        KeepFileRule::ByPathLengthShortest => "パス長（短）を保持",
        KeepFileRule::ByPathLengthLongest => "パス長（長）を保持",
    }
}

/// 状態から平坦化した結果行（ヘッダ＋ファイル）を構築する。
///
/// 表示フィルタ（§3.5a）: Off なら全グループ、選択済みのみ/未選択のみは各グループの
/// チェック有無で絞り込む。各行は元グループ座標 `gi`/`fi` を保持し、絞り込み後も座標を保つ。
fn build_list_rows(st: &AppState) -> Vec<ListRow> {
    let rule = st.config.keep_file_rule;
    let mut rows = Vec::with_capacity(st.groups.iter().map(|g| 1 + g.files.len()).sum());
    for (gi, group) in st.groups.iter().enumerate() {
        let has_checked = (0..group.files.len()).any(|fi| st.selection.is_checked((gi, fi)));
        if !st.filter_mode.group_visible(has_checked) {
            continue;
        }
        rows.push(header_row(gi, group, rule));
        for (fi, entry) in group.files.iter().enumerate() {
            rows.push(file_row(gi, fi, entry, st));
        }
    }
    rows
}

/// グループ見出し行（`is_header=true`、`header` のみ有効）。
fn header_row(gi: usize, group: &DuplicateGroup, rule: KeepFileRule) -> ListRow {
    ListRow {
        is_header: true,
        header: convert::group_header(group, rule).into(),
        gi: gi as i32,
        fi: -1,
        name: SharedString::new(),
        folder: SharedString::new(),
        size_text: SharedString::new(),
        modified: SharedString::new(),
        checked: false,
        locked: false,
        has_error: false,
    }
}

/// ファイル行。ロック判定は core の `FileEntry.path` を直接使う（表示文字列を再パースしない）。
fn file_row(gi: usize, fi: usize, entry: &FileEntry, st: &AppState) -> ListRow {
    let (folder, name) = convert::folder_and_name(entry);
    ListRow {
        is_header: false,
        header: SharedString::new(),
        gi: gi as i32,
        fi: fi as i32,
        name: name.into(),
        folder: folder.into(),
        size_text: convert::format_size(entry.size.0).into(),
        modified: convert::format_modified(entry.modified).into(),
        checked: st.selection.is_checked((gi, fi)),
        locked: st.locks.is_locked(&entry.path),
        has_error: false,
    }
}

/// 単一ファイル行のチェック/ロック状態を差分更新する（全再構築を避ける, U12 perf）。
///
/// `ListView` のモデルを丸ごと作り直さず、当該行のみ `set_row_data` で更新する。
/// 選択済みフィルタ ON 時は行の出入りで平坦 index がずれるため、呼び出し側は
/// [`refresh_ui`] を使うこと（本関数は filter OFF 前提）。
pub(crate) fn update_file_row(ui: &AppWindow, st: &AppState, g: i32, f: i32) {
    // Slint からの i32 座標。負値は不正座標として弾く（`as usize` の桁あふれ回避, §4）。
    let (Ok(gi), Ok(fi)) = (usize::try_from(g), usize::try_from(f)) else {
        return;
    };
    let Some(entry) = st.groups.get(gi).and_then(|grp| grp.files.get(fi)) else {
        return;
    };
    // flat row index キャッシュは set_groups/clear_groups が groups と同期させる。
    // 同期されていれば groups.get(gi) を通った座標は必ず Some を返す。
    let Some(idx) = st.file_row_index(gi, fi) else {
        return;
    };
    let rows = ui.get_rows();
    let Some(vm) = rows.as_any().downcast_ref::<VecModel<ListRow>>() else {
        // モデル種別が想定外（通常起きない）。安全側で何もしない。
        return;
    };
    let Some(mut row) = vm.row_data(idx) else {
        return;
    };
    row.checked = st.selection.is_checked((gi, fi));
    row.locked = st.locks.is_locked(&entry.path);
    vm.set_row_data(idx, row);
    // チェック数・サマリ（選択サマリ/回収量）は安価なので併せて更新する。
    ui.set_checked_count(st.selection.checked_count() as i32);
    refresh_summary(ui, st);
}

/// ワーカースレッドから UI スレッドへクロージャを送る。失敗はログのみ。
pub(crate) fn dispatch_ui(
    ui_weak: &slint::Weak<AppWindow>,
    f: impl FnOnce(AppWindow) + Send + 'static,
) {
    if let Err(e) = ui_weak.upgrade_in_event_loop(f) {
        match e {
            slint::EventLoopError::EventLoopTerminated => {
                tracing::debug!("イベントループ終了中のため UI 更新をスキップしました");
            }
            other => tracing::error!("UI スレッドへの送信に失敗しました: {other}"),
        }
    }
}

/// `(group, file)` 座標から core 結果のパスを引く。
///
/// `g`/`f` は Slint の `grp.gi`/`fi`（常に ≥0 の絶対 index）。万一負値が来ても
/// `as usize` は巨大値へラップし `.get` が `None` を返すため境界安全。
pub(crate) fn file_path_at(st: &AppState, g: i32, f: i32) -> Option<LongPath> {
    st.groups
        .get(g as usize)
        .and_then(|grp| grp.files.get(f as usize))
        .map(|file| file.path.clone())
}

/// エクスプローラーでファイルを選択表示する（`explorer.exe /select,<path>`）。
///
/// `OsString` で連結し UTF-8 変換を挟まない（空白・非 UTF-8 パスも安全）。
/// `Command::arg` はシェルを介さないためコマンドインジェクションの懸念はない。
/// ファイルの格納フォルダ（親ディレクトリ）をエクスプローラーで開く（§3.5, polish3 指摘）。
///
/// 旧実装は `/select` でファイルを選択表示していたが、要望によりフォルダを開く挙動へ変更。
/// 親が取得できない場合はパス自体を開く（フォールバック）。
pub(crate) fn open_in_explorer(path: &LongPath) {
    let p = path.as_path();
    let target = p.parent().unwrap_or(p);
    // explorer.exe は `\\?\` / `\\?\UNC\` 拡張長パスを解釈できないため簡略化する
    // （canonicalize 由来・UNC で付与され得る。clean パスでは無変換）。
    if let Err(e) = std::process::Command::new("explorer.exe")
        .arg(dunce::simplified(target))
        .spawn()
    {
        tracing::warn!("エクスプローラーの起動に失敗しました: {e}");
    }
}

/// 関連付けアプリでファイルを開く（`explorer.exe <path>` は既定ハンドラへ委譲する, §3.5）。
///
/// `Command::arg` はシェルを介さず `OsStr` を直接渡すため、空白・非 UTF-8 パスも安全で
/// コマンドインジェクションの懸念はない（`open_in_explorer` と同方針）。
pub(crate) fn open_with_default(path: &LongPath) {
    if let Err(e) = std::process::Command::new("explorer.exe")
        .arg(dunce::simplified(path.as_path()))
        .spawn()
    {
        tracing::warn!("関連付けアプリの起動に失敗しました: {e}");
    }
}

/// 検索フェーズのステータス文言。
pub(crate) fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Loading => "検索中… (読み込み)",
        Phase::Sorting => "検索中… (ソート)",
        Phase::Comparing => "検索中… (比較)",
    }
}

/// 検索完了/中断時のステータス文言（処理時間つき, polish2）。
pub(crate) fn done_status(group_count: usize, cancelled: bool, secs: f64) -> String {
    if cancelled {
        format!("中断しました（{group_count} グループ検出・{secs:.1} 秒）")
    } else {
        format!("完了 · {group_count} グループ検出 · 処理時間 {secs:.1} 秒")
    }
}

/// 削除確認ダイアログの本文（ごみ箱 / 完全削除で文言を変える）。
pub(crate) fn delete_dialog_text(count: usize, recycle: bool) -> String {
    if recycle {
        format!("{count} 件をごみ箱へ移動します。よろしいですか?")
    } else {
        format!("{count} 件を完全に削除します（元に戻せません）。よろしいですか?")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_method_index_roundtrips_for_all_variants() {
        // index ↔ CompareMethod が全 9 方式で一意・対称であること（Slint メニューと整合）。
        let all = [
            CompareMethod::HashBlake3,
            CompareMethod::HashSha256,
            CompareMethod::HashXxh128,
            CompareMethod::PartialMatch,
            CompareMethod::NameSizeDate,
            CompareMethod::NameSize,
            CompareMethod::NameOnly,
            CompareMethod::OfficeSimilarity,
            CompareMethod::ImageSimilarity,
        ];
        for m in all {
            assert_eq!(compare_method_from_index(compare_method_index(m)), Some(m));
        }
        // 範囲外は None。
        assert_eq!(compare_method_from_index(9), None);
        assert_eq!(compare_method_from_index(-1), None);
    }
}
