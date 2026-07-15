//! ログ初期化と実行時レベル変更（仕様 §5/§5.5, ガイド §13.6）。
//!
//! `tracing` + `tracing-appender` で固定名 `shredy.log` へ追記する（日付ローテ・purge なし）。
//! ファイルは**遅延生成**：`LogLevel` を通過するイベントが初めて書き込まれた時に作成する
//! （起動時には作らない）。起動マーカーも通常の `INFO` イベントとして出す。
//! `LogLevel` の変更は `reload` レイヤーで再起動なしに反映する。

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tracing::level_filters::LevelFilter;
use tracing::subscriber::set_global_default;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::reload;

use crate::error::{CoreError, Result};

/// ログレベルの実行時変更ハンドルと、非ブロッキング書き込みのワーカガード。
///
/// `WorkerGuard` を保持している間だけバックグラウンド書き込みが有効。
/// アプリ実行中は本ハンドルを保持し続けること。
pub struct LogReloadHandle {
    handle: reload::Handle<LevelFilter, Registry>,
    _guard: WorkerGuard,
}

impl LogReloadHandle {
    /// ログ出力レベルを実行時に変更する（再起動不要）。
    pub fn set_level(&self, level: LevelFilter) -> Result<()> {
        self.handle
            .modify(|f| *f = level)
            .map_err(|e| CoreError::Config(format!("log level reload failed: {e}")))
    }
}

/// 動作ログのファイル名（日付なし・追記・ローテ/purge なし。仕様 §5/§5.5）。
pub const ACTION_LOG_FILE: &str = "shredy.log";

/// ログを初期化する。`dir/shredy.log` へ追記出力する（固定名・ローテなし・遅延生成）。
///
/// ファイルは `LogLevel` を通過するイベントが初めて書き込まれた時に作成される（起動時には
/// 作らない, 仕様 §5.5）。`startup_msg`（アプリ名・バージョン等）は通常の `INFO` イベントとして
/// 記録するため、`LogLevel` が `INFO` 以上の時のみ起動行が残る。
pub fn init_logging(
    dir: &Path,
    level: LevelFilter,
    startup_msg: &str,
    console: bool,
) -> Result<LogReloadHandle> {
    // 遅延ライター：初回書き込み時にだけ `dir`／`shredy.log` を作成する。
    let writer = LazyAppendWriter::new(dir.join(ACTION_LOG_FILE));
    let (non_blocking, guard) = tracing_appender::non_blocking(writer);

    let (filter, handle) = reload::Layer::new(level);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_timer(LocalTime)
        .with_writer(non_blocking);
    // CLI のみ: 同じ内容を stderr にもミラーする（`console=true`）。GUI はコンソールを持たない
    // ため `false` で無効化する。ファイル層と同じ reload フィルタ配下に置くため、実行時の
    // ログレベル変更（[`LogReloadHandle::set_level`]）はファイルと stderr の双方へ同時に効く。
    let console_layer = console.then(|| {
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_timer(LocalTime)
            .with_writer(std::io::stderr)
    });

    // `try_init` は `tracing-log` の `LogTracer`（= `log::set_logger`）も設定しようとするため、
    // GUI が先に `log` ロガー（icu 抑制）を設定済みだと競合して失敗する。tracing subscriber だけを
    // 設定する `set_global_default` を使い、log→tracing ブリッジは張らない（log は別管理）。
    let subscriber = Registry::default()
        .with(filter)
        .with(file_layer)
        .with(console_layer);
    set_global_default(subscriber)
        .map_err(|e| CoreError::Config(format!("logging init failed: {e}")))?;

    // 起動マーカーは通常の INFO イベントとして出す（LogLevel を通過した時のみ記録＝遅延生成）。
    tracing::info!(app = startup_msg, "起動");

    Ok(LogReloadHandle {
        handle,
        _guard: guard,
    })
}

/// tracing 行のタイムスタンプをローカル時刻 `YYYY-MM-DD HH:MM:SS.mmm` で出すタイマ（仕様 §5.5）。
struct LocalTime;

impl tracing_subscriber::fmt::time::FormatTime for LocalTime {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        w.write_str(&crate::util::format_local_now_millis())
    }
}

/// 初回書き込み時にだけファイル（と親ディレクトリ）を生成し、以降は追記する遅延ライター。
///
/// `LogLevel` を通過するイベントが発生するまで `shredy.log` を作らない（仕様 §5.5）。
/// `tracing_appender::non_blocking` がワーカスレッドで単独所有するため内部同期は不要。
struct LazyAppendWriter {
    path: PathBuf,
    file: Option<File>,
    /// 一度オープンに失敗したら立てる。以降は再試行せずログを捨てる（毎回 open する
    /// ホットループと I/O エラーの連発を避ける。ログ喪失は許容）。
    failed: bool,
}

impl LazyAppendWriter {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            file: None,
            failed: false,
        }
    }

    /// 親ディレクトリを作成し、追記モードでファイルを開く。
    fn open(&self) -> io::Result<File> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
    }
}

impl Write for LazyAppendWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.file.is_none() {
            if self.failed {
                // 初回オープンに失敗済み。再試行せず破棄する（書き込めたことにする）。
                return Ok(buf.len());
            }
            match self.open() {
                Ok(f) => self.file = Some(f),
                Err(e) => {
                    self.failed = true;
                    return Err(e);
                }
            }
        }
        // 直前の open ブロックで必ず Some にしているため None 分岐は到達不能。
        // ライブラリ層で panic させないため expect を使わず、万一の None は
        // 「書けたことにして」ログ書き込みで本処理を止めない（§3 panic 回避）。
        match self.file.as_mut() {
            Some(f) => f.write(buf),
            None => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.file.as_mut() {
            Some(f) => f.flush(),
            None => Ok(()),
        }
    }
}
