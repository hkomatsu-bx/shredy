//! コア層のエラー型（Effective Rust Item 4, ガイド §7）。
//!
//! panic させず `Result` を返す。エラーには発生源パスと元エラー（`#[source]`）を
//! 付与して文脈を保持する。

use std::path::PathBuf;

/// Shredy コアの統一エラー型。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// ファイル I/O 失敗。発生源パスと元 I/O エラーを保持する。
    #[error("I/O error at {path:?}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// パスが不正・正規化できない。
    #[error("invalid path: {0:?}")]
    InvalidPath(PathBuf),

    /// 設定・初期化に関するエラー。
    #[error("config error: {0}")]
    Config(String),

    /// Office ファイルの解析・正規化に失敗（S7, 仕様 §3.11）。
    #[error("office parse error: {0}")]
    Office(String),

    /// 画像のデコード・pHash 計算に失敗（S8, 仕様 §3.12）。
    #[error("image error: {0}")]
    Image(String),
}

impl CoreError {
    /// I/O エラーを発生源パス付きで構築するヘルパ。
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.into(),
            source,
        }
    }
}

/// コア層の `Result` エイリアス。
pub type Result<T> = std::result::Result<T, CoreError>;
