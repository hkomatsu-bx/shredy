//! `shredy-gui` のロジック層（Slint 非依存）。
//!
//! 描画に依存しない変換・選択・コア配線をライブラリとして公開し、単体テスト可能にする
//! （設計 §6）。`.slint` ビューと結線する `main.rs` はこのライブラリを利用する。

pub mod bridge;
pub mod viewmodel;
