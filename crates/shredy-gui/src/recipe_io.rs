//! リネームレシピの永続化（§3.10.1「名前を付けて保存・呼び出し・削除」/ C-M4）。
//!
//! 保存先は設定と同じフォルダ（§5 の方針）に固定名 `shredy.recipes`（JSON・
//! 名前→ステップ列のマップ）。読み込み失敗は空マップ＋警告ログ（起動を妨げない）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use shredy_core::RecipeStep;

/// レシピファイル名（設定フォルダ直下・固定名）。
pub(crate) const RECIPES_FILE_NAME: &str = "shredy.recipes";

/// レシピファイルの読み込み上限（10 MiB）。固定パスの設定ファイルだが、外部ツールや
/// 破損で巨大化したファイルを丸ごとメモリに読まないための防御（security 規約: 信頼境界の
/// 入力を無制限に読まない）。レシピ列は通常数 KB のため実用上の制約にならない。
const MAX_RECIPES_FILE_BYTES: u64 = 10 * 1024 * 1024;

fn recipes_path(dir: &Path) -> PathBuf {
    dir.join(RECIPES_FILE_NAME)
}

/// レシピを読み込む。ファイル無しは空、破損・過大は空＋警告ログ。
#[must_use]
pub(crate) fn load_recipes(dir: &Path) -> BTreeMap<String, Vec<RecipeStep>> {
    let path = recipes_path(dir);
    // 読み込み前にサイズを検査（巨大ファイルでメモリを使い果たさない）。
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > MAX_RECIPES_FILE_BYTES {
            tracing::warn!(
                path = %path.display(),
                bytes = meta.len(),
                "レシピファイルが大きすぎます（空として続行）"
            );
            return BTreeMap::new();
        }
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new(); // 未作成は正常（初回起動）。
    };
    match serde_json::from_str(&text) {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                "レシピファイルの解析に失敗しました（空として続行）: {e}"
            );
            BTreeMap::new()
        }
    }
}

/// レシピを保存する。失敗は呼び出し側へ理由を返す（ステータス表示用）。
pub(crate) fn save_recipes(
    dir: &Path,
    recipes: &BTreeMap<String, Vec<RecipeStep>>,
) -> Result<(), String> {
    let path = recipes_path(dir);
    let json = serde_json::to_string_pretty(recipes)
        .map_err(|e| format!("レシピのシリアライズに失敗: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("レシピの保存に失敗: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shredy_core::RenameStep;

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut map = BTreeMap::new();
        map.insert(
            "写真整理".to_string(),
            vec![RecipeStep::new(RenameStep::UrlDecode)],
        );
        save_recipes(dir.path(), &map).unwrap();
        let loaded = load_recipes(dir.path());
        assert_eq!(loaded, map);
    }

    #[test]
    fn missing_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_recipes(dir.path()).is_empty());
    }

    #[test]
    fn corrupt_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(RECIPES_FILE_NAME), "not json").unwrap();
        assert!(load_recipes(dir.path()).is_empty());
    }

    #[test]
    fn oversized_file_loads_empty_without_reading() {
        // 上限超過のファイルは解析せず空を返す（メモリ枯渇防御）。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(RECIPES_FILE_NAME);
        // 上限 + 1 バイトのファイルを用意（中身は有効 JSON でも読まずに弾く）。
        let len = MAX_RECIPES_FILE_BYTES + 1;
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(len).unwrap();
        assert!(load_recipes(dir.path()).is_empty());
    }
}
