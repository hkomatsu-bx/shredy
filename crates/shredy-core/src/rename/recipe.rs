//! リネームレシピ（ステップのパイプライン・仕様 §3.10.1）。
//!
//! レシピは [`RecipeStep`] の列。**上から順に**適用し、無効ステップはスキップする。連番は
//! 対象集合内の 0 始まり序数（開始番号を加算）で決まる。`serde` 派生はレシピ永続化（C-M4）用。

use serde::{Deserialize, Serialize};

use crate::exif::ExifInfo;

use super::step::{NamePair, RegexCache, RenameStep, StepCtx, build_regex_cache};

/// レシピ内の 1 ステップ（有効/無効フラグ付き）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeStep {
    pub step: RenameStep,
    pub enabled: bool,
}

impl RecipeStep {
    /// 有効なステップを作る。
    #[must_use]
    pub fn new(step: RenameStep) -> Self {
        Self {
            step,
            enabled: true,
        }
    }
}

/// リネームレシピ。`steps` を上から順に適用する。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameRecipe {
    pub steps: Vec<RecipeStep>,
}

impl RenameRecipe {
    /// ステップ列からレシピを作る。
    #[must_use]
    pub fn new(steps: Vec<RecipeStep>) -> Self {
        Self { steps }
    }

    /// 1 ファイルへレシピを適用し、変換後の名前（本体＋拡張子）を返す。
    ///
    /// `index` は対象集合内の 0 始まり序数（連番用）、`exif` は EXIF（無ければ `None`）、
    /// `regex` は [`compile_regexes`](Self::compile_regexes) で事前構築したコンパイル済み
    /// 正規表現。`original` を消費せず新しい [`NamePair`] を返す（イミュータブル）。
    #[must_use]
    pub(crate) fn apply(
        &self,
        original: &NamePair,
        index: u64,
        exif: Option<&ExifInfo>,
        regex: &RegexCache,
    ) -> NamePair {
        let ctx = StepCtx {
            index,
            exif,
            original_stem: &original.stem,
            regex,
        };
        self.steps
            .iter()
            .filter(|s| s.enabled)
            .fold(original.clone(), |acc, s| s.step.apply(&acc, &ctx))
    }

    /// 有効ステップ内の正規表現を 1 度だけコンパイルする（[`apply`](Self::apply) へ渡す）。
    ///
    /// 対象ファイルごとの再コンパイルを避けるため、[`build_plan`](crate::rename::build_plan)
    /// はループ前に 1 度だけ呼ぶ。
    #[must_use]
    pub(crate) fn compile_regexes(&self) -> RegexCache {
        build_regex_cache(self.steps.iter().filter(|s| s.enabled).map(|s| &s.step))
    }

    /// 有効な EXIF 命名ステップを含むか（含まない場合は EXIF 読み取りを省略できる）。
    #[must_use]
    pub(crate) fn needs_exif(&self) -> bool {
        self.steps
            .iter()
            .filter(|s| s.enabled)
            .any(|s| matches!(s.step, RenameStep::ExifName { .. }))
    }

    /// 有効な全ステップの妥当性を検証する（最初のエラーを返す）。
    pub fn validate(&self) -> Result<(), String> {
        for s in self.steps.iter().filter(|s| s.enabled) {
            s.step.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rename::step::{CaseConvert, CaseTarget, SeqPosition};

    fn pair(stem: &str, ext: &str) -> NamePair {
        NamePair {
            stem: stem.to_string(),
            ext: ext.to_string(),
        }
    }

    /// 正規表現キャッシュを構築してから適用するテストヘルパ。
    fn run(recipe: &RenameRecipe, name: &NamePair, index: u64) -> NamePair {
        let cache = recipe.compile_regexes();
        recipe.apply(name, index, None, &cache)
    }

    #[test]
    fn applies_steps_top_to_bottom() {
        // 置換（空白→_）→ 小文字化 → 末尾連番。
        let recipe = RenameRecipe::new(vec![
            RecipeStep::new(RenameStep::ReplaceText {
                from: " ".into(),
                to: "_".into(),
            }),
            RecipeStep::new(RenameStep::ChangeCase {
                target: CaseTarget::Stem,
                convert: CaseConvert::Lower,
            }),
            RecipeStep::new(RenameStep::Sequence {
                position: SeqPosition::Suffix,
                digits: 2,
                start: 1,
                sep: "_".into(),
            }),
        ]);
        let out = run(&recipe, &pair("My Photo", "JPG"), 0);
        assert_eq!(out.stem, "my_photo_01");
        assert_eq!(out.ext, "JPG");
    }

    #[test]
    fn disabled_steps_are_skipped() {
        let recipe = RenameRecipe::new(vec![
            RecipeStep {
                step: RenameStep::ChangeCase {
                    target: CaseTarget::Stem,
                    convert: CaseConvert::Upper,
                },
                enabled: false,
            },
            RecipeStep::new(RenameStep::ReplaceText {
                from: "a".into(),
                to: "b".into(),
            }),
        ]);
        let out = run(&recipe, &pair("abc", "txt"), 0);
        // 大文字化はスキップ、置換のみ適用。
        assert_eq!(out.stem, "bbc");
    }

    #[test]
    fn index_drives_sequence_per_file() {
        let recipe = RenameRecipe::new(vec![RecipeStep::new(RenameStep::Sequence {
            position: SeqPosition::Prefix,
            digits: 3,
            start: 1,
            sep: "_".into(),
        })]);
        assert_eq!(run(&recipe, &pair("x", "txt"), 0).stem, "001_x");
        assert_eq!(run(&recipe, &pair("x", "txt"), 9).stem, "010_x");
    }

    #[test]
    fn validate_surfaces_invalid_regex() {
        let recipe = RenameRecipe::new(vec![RecipeStep::new(RenameStep::ReplaceRegex {
            pattern: "(".into(),
            replace: "x".into(),
        })]);
        assert!(recipe.validate().is_err());
    }
}
