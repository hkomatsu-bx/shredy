//! 画像メタデータ除去（§3.10.3・EXIF）ページの結線。
//!
//! 検出ユーティリティ（削除）とは別系統の「その場変換（上書き）」操作。対象フォルダは
//! 重複検索・検出ユーティリティと共通の TargetFolders を再利用する（フォルダ管理コールバックは
//! `folders`/`utility` 側で結線済み）。スキャン（候補列挙）と除去はワーカースレッドで実行し
//! UI を固めない。除去は core の [`strip_many`] を直列実行する（同一ファイルの同時実行は不可）。

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use slint::ComponentHandle;

use shredy_core::progress::{is_cancelled, new_cancel};
use shredy_core::{StripOptions, scan_exif_candidates, strip_many};

use crate::AppWindow;
use crate::config_io::resolve_roots;
use crate::state::{AppState, lock};
use crate::view::{dispatch_ui, refresh_exif};

use super::with_ui;

/// EXIF 除去ページのコールバックをすべて結線する。
pub(crate) fn wire_exif(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // 初期表示。
    {
        let st = lock(state);
        refresh_exif(ui, &st);
    }
    wire_exif_scan(ui, state);
    wire_exif_selection(ui, state);
    wire_exif_strip(ui, state);
    wire_exif_reset(ui, state);
}

/// 「スキャン開始」: 対象フォルダ配下の EXIF 除去候補（対応拡張子の画像）を列挙する。
fn wire_exif_scan(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_exif_scan(move || {
        with_ui(&ui_weak, |ui| {
            perform_exif_scan(&ui, &state);
        });
    });
}

/// 選択操作（全選択 / 全解除 / チェック切替 / バックアップ切替）を結線する。
fn wire_exif_selection(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_exif_select_all(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.exif_select_all();
                refresh_exif(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_exif_clear_selection(move || {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.exif_clear_checks();
                refresh_exif(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_exif_check_toggled(move |idx| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                if let Ok(i) = usize::try_from(idx) {
                    st.exif_toggle_check(i);
                }
                refresh_exif(&ui, &st);
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = Arc::clone(state);
        ui.on_exif_set_backup(move |on| {
            with_ui(&ui_weak, |ui| {
                let mut st = lock(&state);
                st.exif_backup = on;
                refresh_exif(&ui, &st);
            });
        });
    }
}

/// 「EXIF を除去」: 選択中の候補を直列に処理する。
fn wire_exif_strip(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_exif_request_strip(move || {
        with_ui(&ui_weak, |ui| {
            perform_exif_strip(&ui, &state);
        });
    });
}

/// ページ遷移時に EXIF 候補・選択をクリアする（前ページの結果が残らないように）。
fn wire_exif_reset(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_exif_reset(move || {
        with_ui(&ui_weak, |ui| {
            let mut st = lock(&state);
            st.clear_exif();
            refresh_exif(&ui, &st);
        });
    });
}

/// EXIF 除去候補をスキャンする（ワーカースレッド・中断対応）。
fn perform_exif_scan(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // 別ワーカー実行中は起動しない（UI でも弾くが、scan/strip が独立コールバックのため二重防護）。
    if ui.get_is_searching() {
        return;
    }
    let roots = {
        let st = lock(state);
        resolve_roots(&st.config)
    };
    if roots.is_empty() {
        ui.set_status_text("対象フォルダが未設定です（「フォルダ追加」で追加してください）".into());
        return;
    }

    ui.set_is_searching(true);
    ui.set_status_text("スキャン中…".into());
    let cancel = new_cancel();
    {
        let mut st = lock(state);
        st.cancel = Some(Arc::clone(&cancel));
        st.clear_exif();
        refresh_exif(ui, &st);
    }

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let started = Instant::now();
        let items = scan_exif_candidates(&roots, &cancel);
        let cancelled = is_cancelled(&cancel);
        let secs = started.elapsed().as_secs_f64();
        let count = items.len();
        dispatch_ui(&ui_weak, move |ui| {
            let mut st = lock(&state);
            st.set_exif_items(items);
            st.cancel = None;
            refresh_exif(&ui, &st);
            ui.set_is_searching(false);
            let status = if cancelled {
                format!("中断しました（{count} 件検出・{secs:.1} 秒）")
            } else {
                format!("完了 · {count} 件の候補画像 · 処理時間 {secs:.1} 秒")
            };
            ui.set_status_text(status.into());
        });
    });
}

/// 選択中の候補の EXIF を直列に除去する（ワーカースレッド・中断対応）。
fn perform_exif_strip(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    // 別ワーカー実行中は起動しない（UI でも弾くが、scan/strip が独立コールバックのため二重防護）。
    if ui.get_is_searching() {
        return;
    }
    let (paths, backup) = {
        let st = lock(state);
        (st.exif_checked_paths(), st.exif_backup)
    };
    if paths.is_empty() {
        ui.set_status_text("除去対象がありません".into());
        return;
    }

    ui.set_is_searching(true);
    ui.set_status_text("EXIF を除去中…".into());
    let cancel = new_cancel();
    {
        let mut st = lock(state);
        st.cancel = Some(Arc::clone(&cancel));
    }

    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let opts = StripOptions {
            backup,
            dry_run: false,
        };
        let summary = strip_many(&paths, opts, &cancel);
        let cancelled = is_cancelled(&cancel);
        dispatch_ui(&ui_weak, move |ui| {
            let mut st = lock(&state);
            st.cancel = None;
            // 処理後は選択を解除する（候補一覧自体は残す＝再選択・再実行可能）。
            st.exif_clear_checks();
            refresh_exif(&ui, &st);
            ui.set_is_searching(false);
            let head = if cancelled { "中断" } else { "完了" };
            ui.set_status_text(
                format!(
                    "{head} · 除去 {} / メタ無し {} / 非対応 {} / 失敗 {}",
                    summary.removed, summary.no_metadata, summary.unsupported, summary.failed
                )
                .into(),
            );
        });
    });
}
