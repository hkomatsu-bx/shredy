//! CLI 実行フロー（仕様 §4）。
//!
//! CLI は重複検索と結果一覧表示に特化する（安全優先・責務分離）。削除・移動・手動削除リスト等の
//! 破壊的操作は GUI（`shredy-gui`）が担い、CLI には持たせない。

use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

use anyhow::{Context, Result, anyhow, bail};
use shredy_core::progress::{Cancel, new_cancel, request_cancel};
use shredy_core::{
    AppConfig, CACHE_FILE_NAME, CompareMethod, DuplicateGroup, GroupingStrategy, LongPath, RealFs,
    ScanConfig, ScanConfigBuilder, SearchEngine, load_hash_cache, make_strategy, save_hash_cache,
};

use crate::args::CliArgs;
use crate::report;

/// CLI のエントリポイント。設定ロード → 検索 → 結果表示 → キャッシュ保存を束ねる。
pub fn run(args: CliArgs) -> Result<()> {
    let exe_dir = current_exe_dir();
    let (config, config_dir) = load_config(&args, &exe_dir)?;
    let startup_msg = format!("shredy-cli {}", shredy_core::VERSION);
    // CLI はログ内容を stderr にもミラーする（console=true）。利用者が実行画面で
    // 何が実行され・結果・エラーを確認できるようにする（ファイル shredy.log にも記録）。
    let _log_guard = shredy_core::init_logging(
        &config_dir,
        config.log_level.to_filter(),
        &startup_msg,
        true,
    )
    .ok();

    let method = resolve_compare_method(&args, &config)?;
    let roots = resolve_roots(&args, &config)?;
    let scan_cfg = ScanConfigBuilder::from_app_config(&config)
        .io_parallelism(args.io_parallelism)
        .build();

    // ハッシュキャッシュ（内容ハッシュ方式 S0-2 のみ）。方式変更時はロード内で全無効化。
    let cache_path = config_dir.join(CACHE_FILE_NAME);
    let cache = load_hash_cache(&config, method, &cache_path);

    let strategy = make_strategy(method, &scan_cfg, cache.clone()).ok_or_else(|| {
        anyhow!("比較方式 {method:?} は本バージョンでは未対応です（対応: S0-S8）")
    })?;

    // 中断: Ctrl-C でフラグを立てる（即時中断, NFR-4）。
    let cancel = new_cancel();
    {
        let c = cancel.clone();
        if let Err(e) = ctrlc::set_handler(move || request_cancel(&c)) {
            tracing::warn!(error = %e, "Ctrl-C ハンドラの登録に失敗しました（中断操作は無効で続行）");
        }
    }

    let fs = RealFs;
    // 検索開始（対象フォルダ・方式・オプション）/ 完了 / 走査・グルーピング所要時間は
    // コア（engine）が tracing で記録し、console=true で stderr にも表示される（仕様 §5.5）。
    let groups = run_search(&fs, &scan_cfg, strategy, &roots, &cancel);
    report::print_groups(&groups);

    // 検索で更新されたキャッシュを保存（中断時も best-effort）。
    save_hash_cache(cache.as_ref(), &cache_path);

    tracing::info!("アプリケーションを終了します");
    Ok(())
}

/// 実行ファイルのあるディレクトリ（取得失敗時はカレント）。
fn current_exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 設定をロード・検証し、設定と設定ディレクトリを返す（仕様 §5）。
fn load_config(args: &CliArgs, exe_dir: &Path) -> Result<(AppConfig, PathBuf)> {
    let config_path = AppConfig::resolve_path(exe_dir, args.config.as_deref());
    let config = if config_path.exists() {
        AppConfig::load(&config_path).context("設定ファイルの読み込みに失敗しました")?
    } else {
        AppConfig::default()
    };
    config
        .validate()
        .map_err(|e| anyhow!("設定が不正です: {e}"))?;
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| exe_dir.to_path_buf());
    Ok((config, config_dir))
}

/// 進捗チャネルを排出しつつ検索する。
///
/// CLI は進捗・所要時間の表示を独自に持たない。フェーズ・件数・所要 ms は engine が tracing で
/// 記録し、`console=true`（[`run`] の `init_logging`）により stderr にも出るためそれで足りる
/// （二重表示を避ける）。本スレッドは表示しないが、engine が送る `ProgressEvent`（大量になり得る
/// `Skipped`/`Progress` を含む）を排出して未消費イベントがメモリに蓄積するのを防ぐために存在する。
fn run_search(
    fs: &RealFs,
    scan_cfg: &ScanConfig,
    strategy: Box<dyn GroupingStrategy>,
    roots: &[LongPath],
    cancel: &Cancel,
) -> Vec<DuplicateGroup> {
    let (tx, rx) = channel();
    // 排出専用（表示はしない。理由は関数 doc を参照）。
    let drain = std::thread::spawn(move || rx.into_iter().for_each(drop));
    let engine = SearchEngine::with_strategy(fs, scan_cfg, strategy);
    let groups = engine.search(roots, &tx, cancel);
    drop(tx);
    if drain.join().is_err() {
        tracing::warn!("進捗排出スレッドがパニックで終了しました");
    }
    groups
}

fn resolve_compare_method(args: &CliArgs, config: &AppConfig) -> Result<CompareMethod> {
    let Some(s) = args.method else {
        return Ok(config.compare_method);
    };
    // `-S` 値 → 方式の対応は core（CompareMethod::from_index）が単一の真実源。
    CompareMethod::from_index(s).ok_or_else(|| anyhow::anyhow!("不正な比較方式: -S{s}"))
}

fn resolve_roots(args: &CliArgs, config: &AppConfig) -> Result<Vec<LongPath>> {
    let mut roots = Vec::new();
    for f in &args.folders {
        roots.push(LongPath::new(f).context("対象フォルダのパスが不正です")?);
    }
    if roots.is_empty() {
        for tf in &config.target_folders {
            if tf.checked {
                roots.push(LongPath::new(&tf.path).context("登録フォルダのパスが不正です")?);
            }
        }
    }
    if roots.is_empty() {
        bail!(
            "対象フォルダが指定されていません（引数または設定 TargetFolders を確認してください）"
        );
    }
    Ok(roots)
}
