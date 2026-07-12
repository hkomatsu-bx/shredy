//! 一括リネーム（仕様 §3.10.1）。重複検索エンジンとは独立した補助機能。
//!
//! レシピ（[`RenameRecipe`]＝[`RenameStep`] のパイプライン）を対象フォルダ配下のファイルへ
//! 上から順に適用し、変換前→変換後の計画（[`RenamePlan`]）を組み立てる。本モジュールは
//! 変換ロジック（純粋）と計画（列挙・禁止文字除去・衝突検出）までを担う。EXIF 抽出と実際の
//! 改名・コピー・移動・Undo は C-M2 で追加する。
//!
//! - [`step`] — 11 種のリネーム方式（純関数変換）
//! - [`recipe`] — ステップのパイプライン
//! - [`plan`] — 対象列挙・before→after・衝突検出
//! - [`execute`] — 実行（改名/コピー/移動）とアンドゥ（C-M2）

pub(crate) mod execute;
pub(crate) mod plan;
pub(crate) mod recipe;
pub(crate) mod step;

pub use execute::{execute_rename, RenameMode, RenameReport, RenameUndo, UndoReport};
pub use plan::{build_plan, parse_filters, RenameEntry, RenameIssue, RenamePlan, RenameTargets};
pub use recipe::{RecipeStep, RenameRecipe};
pub use step::{CaseConvert, CaseTarget, NamePair, RenameStep, SeqPosition};
