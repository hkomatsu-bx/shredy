//! 依存（icu_segmenter 等）が `log` ファサードへ出す警告を抑制するロガー。
//!
//! parley のテキストレイアウトが日本語の行分割モデル未同梱で `icu_provider` の
//! `log::warn!("ICU4X data error: ...")` を多発させる（非致命的）。`icu*` ターゲットを
//! 落とし、それ以外は stderr へ通す。Slint より先に install して横取りされないようにする。

struct GuiLogFilter;

impl log::Log for GuiLogFilter {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        !metadata.target().starts_with("icu")
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }
    fn flush(&self) {}
}

static GUI_LOG_FILTER: GuiLogFilter = GuiLogFilter;

/// 依存の log 出力を捕捉・抑制する（icu の警告ノイズ対策）。最初に install する。
///
/// icu_provider は logging feature 有効時に `log::warn!` を使う（無効だと直接 eprintln）。
/// 本クレートが icu_provider/logging を有効化し、このフィルタで icu ターゲットを落とす。
pub(crate) fn install() {
    if let Err(e) = log::set_logger(&GUI_LOG_FILTER) {
        // 先行ロガーがある場合は icu 抑制が効かない可能性。原因を残す。
        eprintln!("log ロガーの install に失敗しました（先行ロガーが存在）: {e}");
    }
    log::set_max_level(log::LevelFilter::Warn);
}
