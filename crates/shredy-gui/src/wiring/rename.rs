//! 一括リネーム（§3.10.1）の結線。
//!
//! 重複検索・ユーティリティとは独立した系統。対象は単一フォルダ。レシピ（ステップ列）は
//! `AppState` が保持し、プレビュー（[`build_plan`]）と適用（[`execute_rename`]）・Undo は
//! ワーカースレッドで実行して UI を固めない。
//!
//! 編集モデルは**インライン編集**（モック準拠）: 「ステップ追加」は既定ステップ（連番）を
//! 追加して選択し、行選択でフォーム（`edit_*`）へ展開、フォーム変更のたび選択中ステップを
//! 再構築する。プレビューは**自動**（明示ボタンなし）で、`rename_preview_gen`（世代カウンタ）
//! が古いワーカー結果の上書きを防ぐ。適用/Undo 後は自動再計算しない（同じレシピの再適用
//! プレビューが出ると紛らわしいため）。適用は **Undo スタック**へ積む（マルチ Undo）。

use std::sync::{Arc, Mutex};
use std::thread;

use slint::ComponentHandle;

use shredy_core::progress::new_cancel;
use shredy_core::{build_plan, execute_rename, parse_filters, LongPath, RenameMode, RenameTargets};

use shredy_gui::bridge::rename as br;

use crate::recipe_io::save_recipes;
use crate::state::{lock, AppState};
use crate::view::{dispatch_ui, refresh_rename};
use crate::AppWindow;

use super::with_ui;

/// 一括リネーム関連のコールバックをすべて結線する。
pub(crate) fn wire_rename(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    wire_rename_folder(ui, state);
    wire_rename_steps(ui, state);
    wire_rename_edit(ui, state);
    wire_rename_run(ui, state);
    wire_rename_recipes(ui, state);
}

/// 対象フォルダ選択・出力先参照を結線する。
fn wire_rename_folder(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_pick_folder(move || {
            let Some(picked) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            with_ui(&ui_weak, |ui| {
                {
                    let mut st = lock(&state);
                    st.rename_root = LongPath::new(&picked).ok();
                    refresh_rename(&ui, &st);
                }
                maybe_auto_preview(&ui, &state);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_browse_dest(move || {
            let Some(picked) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            with_ui(&ui_weak, |ui| {
                ui.set_rename_dest(picked.to_string_lossy().into_owned().into());
                maybe_auto_preview(&ui, &state);
            });
        });
    }
}

/// ステップ列の操作（追加/選択/削除/並べ替え/有効切替/全消去）を結線する。
fn wire_rename_steps(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    wire_rename_add_select(ui, state);
    wire_rename_step_ops(ui, state);
}

/// 「ステップ追加」（新規追加モードへ）と行選択（編集フォーム展開）を結線する。
///
/// いずれもプレビューは変えないため自動プレビューは伴わない（追加は方式選択時に実追加される）。
fn wire_rename_add_select(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        // 「ステップ追加」= 新規追加モードへ（ステップはまだ追加しない・変換も走らない。
        // エディタが未選択状態で開き、方式を選んだ時点で実追加＝実機 FB 3 巡目）。
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_add_step(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.rename_adding = true;
                st.rename_sel = None;
                refresh_rename(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_select_step(move |idx| {
            let Ok(idx) = usize::try_from(idx) else {
                return;
            };
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                if idx < st.rename_steps.len() {
                    st.rename_sel = Some(idx);
                    st.rename_adding = false; // 行選択で新規追加モードは取りやめ。
                }
                refresh_rename(&ui, &st);
            });
        });
    }
}

/// ステップの削除/並べ替え/有効切替/全消去を結線する（いずれも自動プレビューを伴う）。
fn wire_rename_step_ops(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_remove_step(move |idx| {
            let Ok(idx) = usize::try_from(idx) else {
                return;
            };
            with_step_mutation(&ui_weak, &state, move |st| st.rename_remove_step(idx));
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_move_step(move |idx, up| {
            let Ok(idx) = usize::try_from(idx) else {
                return;
            };
            with_step_mutation(&ui_weak, &state, move |st| st.rename_move_step(idx, up));
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_toggle_step(move |idx| {
            let Ok(idx) = usize::try_from(idx) else {
                return;
            };
            with_step_mutation(&ui_weak, &state, move |st| st.rename_toggle_step(idx));
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_clear_steps(move || {
            with_step_mutation(&ui_weak, &state, |st| st.rename_clear_steps());
        });
    }
}

/// 「状態変更 → 反映 → 自動プレビュー」の定型を畳んだヘルパー（ステップ操作で共通）。
///
/// `mutate` で `AppState` を変更し、UI を再構築してから自動プレビューを走らせる。
fn with_step_mutation(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    mutate: impl FnOnce(&mut AppState),
) {
    with_ui(ui_weak, |ui| {
        {
            let mut st = lock(state);
            mutate(&mut st);
            refresh_rename(&ui, &st);
        }
        maybe_auto_preview(&ui, state);
    });
}

/// インライン編集（方式切替・フォーム変更→選択中ステップの再構築）を結線する。
fn wire_rename_edit(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    wire_rename_set_method(ui, state);
    wire_rename_edit_changed(ui, state);
}

/// カテゴリ/方式チップを結線する。新規追加モードなら新ステップを実追加、選択中なら新方式の
/// 既定パラメータへ置き換える（モック準拠＋実機 FB 3 巡目）。
fn wire_rename_set_method(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_rename_set_method(move |method| {
        with_ui(&ui_weak, |ui| {
            let Some(step) = br::make_step(&br::default_form(method)) else {
                return;
            };
            {
                let mut st = lock(&state);
                if st.rename_adding {
                    st.rename_add_step(step); // adding は内部で解除される。
                } else {
                    st.rename_replace_selected(step);
                }
                refresh_rename(&ui, &st);
            }
            maybe_auto_preview(&ui, &state);
        });
    });
}

/// フォーム変更（無引数）を結線する。値は two-way 束縛済みの edit_* プロパティから読む
/// （LineEdit のユーザー編集で宣言束縛が破棄され Rust の set が反映されなくなる問題を、
/// in-out + 双方向束縛で回避。実機フィードバック 2026-06-11）。
fn wire_rename_edit_changed(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_rename_edit_changed(move || {
        with_ui(&ui_weak, |ui| {
            let form = br::RenameForm {
                method: ui.get_rename_edit_method(),
                t1: ui.get_rename_edit_t1().to_string(),
                t2: ui.get_rename_edit_t2().to_string(),
                n1: ui.get_rename_edit_n1(),
                n2: ui.get_rename_edit_n2(),
                fmt: ui.get_rename_edit_fmt(),
                pos: ui.get_rename_edit_pos(),
                target: ui.get_rename_edit_target(),
                conv: ui.get_rename_edit_conv(),
            };
            let Some(step) = br::make_step(&form) else {
                return; // 方式 index 範囲外（通常到達しない）。
            };
            {
                let mut st = lock(&state);
                st.rename_replace_selected(step);
                refresh_rename(&ui, &st);
            }
            maybe_auto_preview(&ui, &state);
        });
    });
}

/// オプション変更・表示フィルタ・フォルダを開く・適用・Undo を結線する。
fn wire_rename_run(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    wire_rename_view(ui, state);
    wire_rename_actions(ui, state);
}

/// 表示系（オプション変更・「変更のみ」フィルタ・プレビュー行のフォルダを開く）を結線する。
fn wire_rename_view(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_options_changed(move || {
            with_ui(&ui_weak, |ui| maybe_auto_preview(&ui, &state));
        });
    }
    {
        // 「変更のみ」フィルタ（表示のみ＝計画は不変。再スキャンせず行だけ作り直す）。
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_filter_toggled(move || {
            with_ui(&ui_weak, |ui| {
                let st = lock(&state);
                refresh_rename(&ui, &st);
            });
        });
    }
    {
        // プレビュー行の保存フォルダを開く（num は計画エントリの 1 始まり通し番号）。
        let state = Arc::clone(state);
        ui.on_rename_open_entry(move |num| {
            if num <= 0 {
                return; // 見出し行（num=0）や不正値は対象外。
            }
            let idx = (num - 1) as usize;
            let path = {
                let st = lock(&state);
                st.rename_plan
                    .as_ref()
                    .and_then(|p| p.entries.get(idx))
                    .map(|e| e.path.clone())
            };
            match path {
                // open_in_explorer は親フォルダ（保存フォルダ）を開く仕様。
                Some(path) => {
                    tracing::info!(num, path = %path.as_path().display(), "保存フォルダを開きます");
                    crate::view::open_in_explorer(&path);
                }
                None => tracing::warn!(num, "対象行が見つかりません（プレビュー未計算？）"),
            }
        });
    }
}

/// リネームの実行（適用）と Undo を結線する（実行系）。
fn wire_rename_actions(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_apply(move || {
            with_ui(&ui_weak, |ui| perform_rename_apply(&ui, &state));
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_rename_undo(move || {
            with_ui(&ui_weak, |ui| perform_rename_undo(&ui, &state));
        });
    }
}

/// レシピの保存/読込/削除（C-M4）を結線する。
fn wire_rename_recipes(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    wire_recipe_save(ui, state);
    wire_recipe_load_delete(ui, state);
}

/// レシピの「名前を付けて保存」を結線する。
fn wire_recipe_save(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_recipe_save(move |name| {
            let name = name.trim().to_string();
            if name.is_empty() {
                return;
            }
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                if st.rename_steps.is_empty() {
                    ui.set_status_text("保存するステップがありません".into());
                    return;
                }
                // MutexGuard 経由ではフィールドの分割借用が効かないため先にクローンする。
                let steps = st.rename_steps.clone();
                st.recipes.insert(name.clone(), steps);
                match save_recipes(&st.config_dir, &st.recipes) {
                    Ok(()) => ui.set_status_text(format!("レシピ「{name}」を保存しました").into()),
                    Err(e) => ui.set_status_text(e.into()),
                }
                refresh_rename(&ui, &st);
            });
        });
    }
}

/// レシピの読込・削除を結線する。
fn wire_recipe_load_delete(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_recipe_load(move |name| {
            with_ui(&ui_weak, |ui| {
                {
                    let mut st = lock(&state);
                    let Some(steps) = st.recipes.get(name.as_str()).cloned() else {
                        return;
                    };
                    st.rename_set_steps(steps);
                    ui.set_status_text(format!("レシピ「{name}」を読み込みました").into());
                    refresh_rename(&ui, &st);
                }
                maybe_auto_preview(&ui, &state);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_recipe_delete(move |name| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                if st.recipes.remove(name.as_str()).is_some() {
                    if let Err(e) = save_recipes(&st.config_dir, &st.recipes) {
                        ui.set_status_text(e.into());
                    }
                }
                refresh_rename(&ui, &st);
            });
        });
    }
}

/// 対象フォルダ・レシピ・オプションを束ねた、ワーカーへ渡すスナップショット。
struct RenameJob {
    targets: RenameTargets,
    recipe: shredy_core::RenameRecipe,
    dest: Option<LongPath>,
    mode: i32,
}

/// UI プロパティと状態からジョブを組む。対象未選択・レシピ不正・出力先不足は `Err(理由)`。
///
/// 日時空欄=今日・EXIF パターン空欄=既定 はここで解決する（編集フォームは生値を保持）。
fn build_job(ui: &AppWindow, state: &Arc<Mutex<AppState>>) -> Result<RenameJob, String> {
    let sub = ui.get_rename_include_subfolders();
    let folders = ui.get_rename_include_folders();
    let filter = ui.get_rename_filter().to_string();
    let mode = ui.get_rename_mode();
    let dest_str = ui.get_rename_dest().to_string();

    let (root, steps) = {
        let st = lock(state);
        (
            st.rename_root.clone(),
            br::resolve_runtime_defaults(&st.rename_steps),
        )
    };
    let root = root.ok_or_else(|| "対象フォルダを選択してください".to_string())?;
    let recipe = shredy_core::RenameRecipe::new(steps);
    recipe
        .validate()
        .map_err(|e| format!("レシピが不正です: {e}"))?;

    // コピー/移動は出力先が必須。
    let dest = if mode == 1 || mode == 2 {
        let trimmed = dest_str.trim();
        if trimmed.is_empty() {
            return Err("コピー/移動の出力先を指定してください".to_string());
        }
        Some(LongPath::new(trimmed).map_err(|_| "出力先のパスが不正です".to_string())?)
    } else {
        None
    };

    Ok(RenameJob {
        targets: RenameTargets {
            root,
            include_subfolders: sub,
            include_folders: folders,
            filters: parse_filters(&filter),
        },
        recipe,
        dest,
        mode,
    })
}

/// ジョブの mode コード（1=コピー / 2=移動 / その他=その場）から実行モードを決める。
///
/// コピー/移動の出力先は [`build_job`] で必須チェック済みのため出力先欠落は到達不能だが、
/// 不変条件が将来壊れても panic させず安全側（その場リネーム）へ倒す（§3 panic 回避）。
fn resolve_rename_mode(job: &RenameJob) -> RenameMode {
    match (job.mode, job.dest.clone()) {
        (1, Some(dest)) => RenameMode::Copy { dest },
        (2, Some(dest)) => RenameMode::Move { dest },
        _ => RenameMode::InPlace,
    }
}

/// 自動プレビュー: 対象フォルダと有効ステップが揃っていれば再計算、不足なら計画をクリアする。
fn maybe_auto_preview(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ready = {
        let st = lock(state);
        st.rename_root.is_some() && st.rename_steps.iter().any(|s| s.enabled)
    };
    if ready {
        perform_rename_preview(ui, state);
    } else {
        let mut st = lock(state);
        st.rename_plan = None;
        refresh_rename(ui, &st);
        // not-ready に落ち着いたら反映中インジケータも確実に消す。これがないと、直前の
        // プレビュー開始で立てた `rename_previewing` を消す担当（最新世代ワーカー）が
        // 不在になり得て、インジケータが貼り付く（H-1）。
        ui.set_rename_previewing(false);
    }
}

/// プレビューをワーカーで作成し、計画を反映する。
///
/// `is_searching` は立てない（編集のたびに走る読み取り専用計算で UI を固めない）。連続編集で
/// ワーカーが並走しても、世代カウンタにより最後に開始した計算だけが反映される。
fn perform_rename_preview(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let job = match build_job(ui, state) {
        Ok(j) => j,
        Err(msg) => {
            ui.set_status_text(msg.into());
            return;
        }
    };
    let gen = {
        let mut st = lock(state);
        st.rename_preview_gen += 1;
        st.rename_preview_gen
    };
    // 反映中インジケータ（大きいフォルダで計算に時間がかかる場合のフィードバック）。
    ui.set_rename_previewing(true);

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let cancel = new_cancel();
        let plan = build_plan(&job.targets, &job.recipe, job.dest.as_ref(), &cancel);
        let total = plan.entries.len();
        let changed = plan.changed_count();
        let conflicts = plan.conflict_count();
        dispatch_ui(&ui_weak, move |ui| {
            let mut st = lock(&state);
            if st.rename_preview_gen != gen {
                return; // より新しい計算が開始済み＝この結果は破棄（インジケータも維持）。
            }
            st.rename_plan = Some(plan);
            refresh_rename(&ui, &st);
            ui.set_rename_previewing(false);
            let status = if conflicts > 0 {
                format!("対象 {total} 件中 {changed} 件が変更 · {conflicts} 件が名前重複")
            } else {
                format!("対象 {total} 件中 {changed} 件が変更されます")
            };
            ui.set_status_text(status.into());
        });
    });
}

/// 計画を実行（改名/コピー/移動）し、Undo スタックへ積む。衝突があれば実行しない。
fn perform_rename_apply(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let job = match build_job(ui, state) {
        Ok(j) => j,
        Err(msg) => {
            ui.set_status_text(msg.into());
            return;
        }
    };
    ui.set_is_searching(true);
    ui.set_status_text("リネーム実行中…".into());

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let cancel = new_cancel();
        let plan = build_plan(&job.targets, &job.recipe, job.dest.as_ref(), &cancel);

        // 衝突（空名・同名重複）があれば適用せず、計画だけ提示する（安全策）。
        if plan.has_conflicts() {
            let conflicts = plan.conflict_count();
            dispatch_ui(&ui_weak, move |ui| {
                let mut st = lock(&state);
                st.rename_plan = Some(plan);
                refresh_rename(&ui, &st);
                ui.set_is_searching(false);
                ui.set_status_text(
                    format!("名前が重複しています（{conflicts} 件）。修正してください").into(),
                );
            });
            return;
        }

        let mode = resolve_rename_mode(&job);
        tracing::info!(mode = job.mode, "一括リネームを開始します");
        let (report, undo) = execute_rename(&plan, &mode);
        let ok = report.ok_count();
        let fail = report.fail_count();
        tracing::info!(ok, fail, mode = job.mode, "一括リネームを実行しました");
        // 個々の失敗（パス＋理由）を WARN で記録する（件数だけでなく内訳を残す）。
        for (path, err) in &report.failed {
            tracing::warn!(
                path = %path.as_path().display(),
                error = %err,
                "リネームに失敗しました"
            );
        }
        dispatch_ui(&ui_weak, move |ui| {
            let mut st = lock(&state);
            if !undo.is_empty() {
                st.rename_undo.push(undo);
            }
            st.rename_plan = None; // 実行後は計画が陳腐化するためクリア（自動再計算もしない）。
            refresh_rename(&ui, &st);
            ui.set_is_searching(false);
            ui.set_status_text(format!("完了 · {ok} 件をリネーム · 失敗 {fail} 件").into());
        });
    });
}

/// 直前のリネームを 1 回分元に戻す（Undo スタックから pop・ワーカー）。
fn perform_rename_undo(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let undo = {
        let mut st = lock(state);
        st.rename_undo.pop()
    };
    let Some(undo) = undo else {
        ui.set_status_text("元に戻せる操作がありません".into());
        return;
    };
    ui.set_is_searching(true);
    ui.set_status_text("元に戻しています…".into());

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let report = undo.undo();
        let reverted = report.reverted;
        let failed = report.failed.len();
        tracing::info!(reverted, failed, "一括リネームを元に戻しました");
        dispatch_ui(&ui_weak, move |ui| {
            let mut st = lock(&state);
            st.rename_plan = None;
            refresh_rename(&ui, &st);
            ui.set_is_searching(false);
            ui.set_status_text(format!("{reverted} 件を元に戻しました（失敗 {failed} 件）").into());
        });
    });
}
