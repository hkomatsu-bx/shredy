//! 検索ライフサイクルの結線（設計 §2.2）。
//!
//! 借用エンジン (`SearchEngine<'a>`) は非 `Send` のため、ワーカースレッド内で
//! `RealFs` から構築する。結果 (`Vec<DuplicateGroup>`, `Send`) は
//! `upgrade_in_event_loop` で UI スレッドへ戻す。検索結果の破棄（ナビゲーション）も
//! 検索状態のリセットという同型操作のため本モジュールに置く。

use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

use slint::{ComponentHandle, ModelRc, VecModel};

use shredy_core::progress::{is_cancelled, new_cancel, request_cancel};
use shredy_core::{Cancel, HashCache, LongPath, ProgressEvent, RealFs, ScanConfigBuilder};

use shredy_gui::bridge::search;
use shredy_gui::viewmodel::SelectionState;

use crate::cache_io;
use crate::config_io::resolve_roots;
use crate::state::{lock, AppState};
use crate::view::{dispatch_ui, done_status, phase_label, refresh_ui};
use crate::{AppWindow, ListRow};

use super::with_ui;

/// 「スキャン開始」: ワーカースレッドで検索し、進捗・結果を UI へ戻す（設計 §2.2）。
pub(crate) fn wire_scan(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_scan_clicked(move || {
        with_ui(&ui_weak, |ui| {
            // スキャン条件は現在の設定から都度解決する（フォルダ編集を反映）。
            // ハッシュキャッシュも設定から都度ロードする（方式変更はロード内で全無効化, M4a）。
            let (roots, scan_cfg, method, cache, cache_path) = {
                let st = lock(&state);
                let cache_path = st.config_dir.join(cache_io::CACHE_FILE_NAME);
                let cache =
                    cache_io::load_hash_cache(&st.config, st.config.compare_method, &cache_path);
                (
                    resolve_roots(&st.config),
                    ScanConfigBuilder::from_app_config(&st.config).build(),
                    st.config.compare_method,
                    cache,
                    cache_path,
                )
            };
            if roots.is_empty() {
                ui.set_status_text(
                    "対象フォルダが未設定です（「フォルダ追加」で追加してください）".into(),
                );
                return;
            }

            let cancel = reset_for_new_scan(&ui, &state);
            let (tx, rx) = channel::<ProgressEvent>();
            spawn_progress_pump(ui_weak.clone(), rx);
            spawn_worker(
                ui_weak.clone(),
                Arc::clone(&state),
                scan_cfg,
                roots,
                method,
                cache,
                cache_path,
                cancel,
                tx,
            );
        });
    });
}

/// 検索開始前に UI と状態を初期化し、新しい中断フラグを返す。
fn reset_for_new_scan(ui: &AppWindow, state: &Arc<Mutex<AppState>>) -> Cancel {
    ui.set_is_searching(true);
    ui.set_status_text("検索中…".into());
    ui.set_rows(ModelRc::new(VecModel::from(Vec::<ListRow>::new())));
    ui.set_checked_count(0);

    let cancel = new_cancel();
    let mut st = lock(state);
    st.cancel = Some(Arc::clone(&cancel));
    st.clear_groups();
    st.selection = SelectionState::new();
    cancel
}

/// 進捗ポンプ: フェーズ遷移を状態テキストへ反映する（`tx` drop で終了）。
fn spawn_progress_pump(
    ui_weak: slint::Weak<AppWindow>,
    rx: std::sync::mpsc::Receiver<ProgressEvent>,
) {
    thread::spawn(move || {
        for ev in rx {
            if let ProgressEvent::Phase(phase) = ev {
                let text = phase_label(phase);
                dispatch_ui(&ui_weak, move |ui| ui.set_status_text(text.into()));
            }
        }
    });
}

/// ワーカー: 借用エンジンをスレッド内で構築・検索し、結果を UI へ戻す。
///
/// `cache` を検索へ注入し、完了後（中断時も best-effort）に `cache_path` へ保存する（M4a）。
// 引数はすべてワーカースレッドへ move する必要があり、構造体化するとライフタイムが
// 複雑化するため許容する。
#[allow(clippy::too_many_arguments)]
fn spawn_worker(
    ui_weak: slint::Weak<AppWindow>,
    state: Arc<Mutex<AppState>>,
    cfg: shredy_core::ScanConfig,
    roots: Vec<LongPath>,
    method: shredy_core::CompareMethod,
    cache: Option<Arc<RwLock<HashCache>>>,
    cache_path: PathBuf,
    cancel: Cancel,
    tx: std::sync::mpsc::Sender<ProgressEvent>,
) {
    thread::spawn(move || {
        let started = std::time::Instant::now();
        let fs = RealFs;
        tracing::info!(?method, roots = roots.len(), "重複検索を開始します");
        // `cache.clone()` は Arc の参照カウント増加のみ。保存（下記）で cache を再利用するため複製する。
        let groups = search::execute(&fs, &cfg, method, &roots, cache.clone(), &tx, &cancel);
        drop(tx);
        // 検索で更新されたキャッシュを保存（warm 再検索を高速化）。中断時も best-effort で到達する。
        cache_io::save_hash_cache(cache.as_ref(), &cache_path);
        // 検索完了・走査数等はコア（engine/scan）が shredy.log へ直接出力する。
        // ここでは UI 表示用に経過秒・中断有無のみ算出する。
        let cancelled = is_cancelled(&cancel);
        let secs = started.elapsed().as_secs_f64();
        dispatch_ui(&ui_weak, move |ui| {
            let mut st = lock(&state);
            st.set_groups(groups);
            st.selection = SelectionState::new();
            st.cancel = None;
            let count = st.groups.len();
            refresh_ui(&ui, &st);
            ui.set_is_searching(false);
            ui.set_status_text(done_status(count, cancelled, secs).into());
        });
    });
}

/// 「中断」: 共有中断フラグを立てる（コアがループ境界で即応）。
pub(crate) fn wire_cancel(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let state = Arc::clone(state);
    ui.on_cancel_clicked(move || {
        if let Some(cancel) = lock(&state).cancel.as_ref() {
            request_cancel(cancel);
        }
    });
}

/// 「破棄して移動」: 検索結果と選択を破棄して UI を初期状態へ戻す（設計 §2.3）。
///
/// ページ遷移自体は Slint 側（`navigate-to` / 破棄確認オーバーレイ）が担い、ここでは
/// 状態のリセットのみを行う。`reset_for_new_scan` の結果クリアと同型の純粋操作。
///
/// `refresh_ui` が `has_results=false` を反映してから Slint 側が `active_page` を変更する
/// 順序に依存する（Slint コールバックは同期実行されるため成立する）。
pub(crate) fn wire_navigation(ui: &AppWindow, state: &Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Arc::clone(state);
    ui.on_discard_results(move || {
        with_ui(&ui_weak, |ui| {
            let mut st = lock(&state);
            st.clear_groups();
            st.selection = SelectionState::new();
            refresh_ui(&ui, &st);
        });
    });
}
