//! `shredy.exe` — Shredy の CLI フロントエンド（仕様 §4）。
//!
//! **重複ファイル検索と結果一覧表示に特化**したフロントエンド。削除・移動・手動削除リスト等の
//! 破壊的操作は GUI（`shredy-gui`）が担い、CLI には持たせない（安全優先・責務分離）。

mod args;
mod report;
mod run;

fn main() -> anyhow::Result<()> {
    run::run(args::CliArgs::parse_args())
}
