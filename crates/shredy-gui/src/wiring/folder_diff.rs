//! フォルダ比較ダイアログ（§3.7）の結線。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use slint::ComponentHandle;

use shredy_core::FolderDiffScanner;
use shredy_gui::bridge::folder_diff::status_text;

use crate::state::{lock, AppState};
use crate::view::{dispatch_ui, refresh_folder_diff};
use crate::AppWindow;

use super::with_ui;

/// フォルダ比較ダイアログのコールバックをすべて結線する。
pub(crate) fn wire_folder_diff(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    wire_select_a(ui, state);
    wire_select_b(ui, state);
    wire_compare(ui, state);
    wire_cancel(ui, state);
    wire_filter(ui, state);
}

fn wire_select_a(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_folder_diff_select_a(move || {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        with_ui(&ui_weak, |ui| {
            let mut st = lock(&state);
            st.folder_diff_a = path.to_string_lossy().into_owned();
            ui.set_folder_diff_a(st.folder_diff_a.as_str().into());
        });
    });
}

fn wire_select_b(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_folder_diff_select_b(move || {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        with_ui(&ui_weak, |ui| {
            let mut st = lock(&state);
            st.folder_diff_b = path.to_string_lossy().into_owned();
            ui.set_folder_diff_b(st.folder_diff_b.as_str().into());
        });
    });
}

fn wire_compare(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_folder_diff_compare(move || {
        let (folder_a, folder_b, cancel) = {
            let mut st = lock(&state);
            // 前回の比較が残っていればキャンセルし、新しいトークンに差し替える。
            st.folder_diff_cancel.store(true, Ordering::Relaxed);
            let cancel = Arc::new(AtomicBool::new(false));
            st.folder_diff_cancel = Arc::clone(&cancel);
            st.folder_diff_comparing = true;
            st.folder_diff_status = "比較中...".to_owned();
            (st.folder_diff_a.clone(), st.folder_diff_b.clone(), cancel)
        };

        with_ui(&ui_weak, |ui| {
            ui.set_folder_diff_comparing(true);
            ui.set_folder_diff_status("比較中...".into());
        });

        let state_c = Arc::clone(&state);
        let ui_weak_c = ui_weak.clone();
        thread::spawn(move || {
            let result = FolderDiffScanner::compare(
                &PathBuf::from(&folder_a),
                &PathBuf::from(&folder_b),
                &cancel,
            );
            let cancelled = cancel.load(Ordering::Relaxed);
            let status = if cancelled {
                format!("中断 — {}", status_text(&result.summary))
            } else {
                status_text(&result.summary)
            };
            let entries = result.entries;

            dispatch_ui(&ui_weak_c, move |ui| {
                let mut st = lock(&state_c);
                st.folder_diff_entries = entries;
                st.folder_diff_status = status;
                st.folder_diff_comparing = false;
                refresh_folder_diff(&ui, &st);
            });
        });
    });
}

fn wire_cancel(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let state = Arc::clone(state);
    ui.on_folder_diff_cancel(move || {
        lock(&state)
            .folder_diff_cancel
            .store(true, Ordering::Relaxed);
    });
}

fn wire_filter(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_folder_diff_filter_changed(move |filter| {
        with_ui(&ui_weak, |ui| {
            let mut st = lock(&state);
            st.folder_diff_filter = filter;
            refresh_folder_diff(&ui, &st);
        });
    });
}
