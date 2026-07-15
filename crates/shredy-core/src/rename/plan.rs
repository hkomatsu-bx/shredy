//! 一括リネームの対象列挙と計画（before→after・禁止文字除去・衝突検出）。仕様 §3.10.1。
//!
//! [`build_plan`] は対象を列挙してレシピを適用し、各ファイルの変換前→変換後と問題（空名・
//! 同一出力フォルダ内の重複）を [`RenamePlan`] にまとめる。ファイルの読み書きは行わない
//! （EXIF 抽出と実際の改名は C-M2）。連番を決定的にするため対象はフルパス昇順で並べる。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::exif::{ext_supports_exif, read_exif};
use crate::path::LongPath;
use crate::progress::{Cancel, is_cancelled};
use crate::scan::{MAX_DEPTH, matches_wildcard};

use super::recipe::RenameRecipe;
use super::step::NamePair;

/// 対象選択の条件（仕様 §3.10.1 対象選択）。
#[derive(Debug, Clone)]
pub struct RenameTargets {
    /// 対象フォルダ（単一）。
    pub root: LongPath,
    /// サブフォルダ配下も対象にするか。
    pub include_subfolders: bool,
    /// フォルダ（サブフォルダ）自身も対象に含めるか（false＝ファイルのみ）。
    pub include_folders: bool,
    /// ワイルドカードフィルタ（空なら全件。`*`/`?`・大小無視）。
    pub filters: Vec<String>,
}

impl RenameTargets {
    /// 対象（ファイル、オプションでフォルダ）をフルパス昇順で列挙する。
    fn collect(&self, cancel: &Cancel) -> Vec<(LongPath, bool)> {
        let max_depth = if self.include_subfolders {
            MAX_DEPTH
        } else {
            1
        };
        let mut out: Vec<(LongPath, bool)> = Vec::new();
        for res in WalkDir::new(self.root.as_path())
            .max_depth(max_depth)
            .follow_links(false)
        {
            if is_cancelled(cancel) {
                break;
            }
            let Ok(ent) = res else { continue };
            // ルート自身は対象外（リネームしない）。
            if ent.depth() == 0 {
                continue;
            }
            let ft = ent.file_type();
            let is_dir = ft.is_dir();
            if is_dir && !self.include_folders {
                continue;
            }
            // 通常ファイルとフォルダのみ対象（シンボリックリンク等は除外）。
            if !is_dir && !ft.is_file() {
                continue;
            }
            let name = ent.file_name().to_string_lossy();
            if !self.matches(&name) {
                continue;
            }
            if let Ok(lp) = LongPath::new(ent.path()) {
                out.push((lp, is_dir));
            }
        }
        out.sort_by(|a, b| a.0.as_path().cmp(b.0.as_path()));
        out
    }

    /// フィルタ一致（フィルタ空なら常に true、いずれかのパターンに一致で true）。
    fn matches(&self, name: &str) -> bool {
        if self.filters.is_empty() {
            return true;
        }
        self.filters.iter().any(|f| matches_wildcard(f, name))
    }
}

/// フィルタ文字列（`*.jpg;*.png` 形式）を個別パターンへ分解する。空・空白は除く。
#[must_use]
pub fn parse_filters(input: &str) -> Vec<String> {
    input
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// エントリ単位の問題（適用ブロック要因）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameIssue {
    /// 変換後の名前が空（全文字が削除・除去された）。
    EmptyName,
    /// 同一出力フォルダ内で変換後名が重複する。
    Duplicate,
}

/// before→after の 1 エントリ。
#[derive(Debug, Clone)]
pub struct RenameEntry {
    /// 元のフルパス。
    pub path: LongPath,
    /// フォルダか（仕様 §3.10.1「フォルダ自身も対象」）。
    pub is_dir: bool,
    /// 変換前のファイル名。
    pub original: String,
    /// 変換後のファイル名（禁止文字除去済み）。
    pub renamed: String,
    /// 名前が変化するか。
    pub changed: bool,
    /// 問題があれば設定（適用をブロックする）。
    pub issue: Option<RenameIssue>,
}

/// リネーム計画（全エントリ）。
#[derive(Debug, Clone, Default)]
pub struct RenamePlan {
    pub entries: Vec<RenameEntry>,
}

impl RenamePlan {
    /// 名前が変化するエントリ数。
    #[must_use]
    pub fn changed_count(&self) -> usize {
        self.entries.iter().filter(|e| e.changed).count()
    }

    /// 問題のあるエントリ数。
    #[must_use]
    pub fn conflict_count(&self) -> usize {
        self.entries.iter().filter(|e| e.issue.is_some()).count()
    }

    /// 適用をブロックすべき問題があるか。
    #[must_use]
    pub fn has_conflicts(&self) -> bool {
        self.entries.iter().any(|e| e.issue.is_some())
    }
}

/// 対象を列挙してレシピを適用し、計画（禁止文字除去・衝突検出込み）を返す。
///
/// `dest` を `Some` にするとコピー/移動モード（全出力が `dest` 直下）として衝突を判定する。
/// `None` はその場リネーム（各エントリの親フォルダ内で判定）。EXIF は C-M2 まで `None`。
#[must_use]
pub fn build_plan(
    targets: &RenameTargets,
    recipe: &RenameRecipe,
    dest: Option<&LongPath>,
    cancel: &Cancel,
) -> RenamePlan {
    let files = targets.collect(cancel);
    // 正規表現は対象ごとに再コンパイルせず 1 度だけコンパイルして使い回す。
    let regex = recipe.compile_regexes();
    // EXIF 命名ステップが無ければ EXIF 読み取り I/O を省略する。
    let needs_exif = recipe.needs_exif();
    let mut entries: Vec<RenameEntry> = Vec::with_capacity(files.len());
    for (index, (path, is_dir)) in files.iter().enumerate() {
        let original = file_name_of(path.as_path());
        let pair = split_name(&original, *is_dir);
        // EXIF 命名がある静止画のみ EXIF を読む（フォルダ・非対応形式は None＝無変更）。
        let exif = (needs_exif && !*is_dir && ext_supports_exif(&pair.ext))
            .then(|| read_exif(path.as_path()))
            .flatten();
        let out = recipe.apply(&pair, index as u64, exif.as_ref(), &regex);
        let renamed = sanitize(&assemble(&out));
        let changed = renamed != original;
        let issue = if renamed.is_empty() {
            Some(RenameIssue::EmptyName)
        } else {
            None
        };
        entries.push(RenameEntry {
            path: path.clone(),
            is_dir: *is_dir,
            original,
            renamed,
            changed,
            issue,
        });
    }
    detect_duplicates(&mut entries, dest);
    RenamePlan { entries }
}

/// ファイル名を本体と拡張子に分割する。フォルダ・先頭ドット名は拡張子なし扱い。
fn split_name(name: &str, is_dir: bool) -> NamePair {
    if is_dir {
        return NamePair {
            stem: name.to_string(),
            ext: String::new(),
        };
    }
    match name.rfind('.') {
        // 先頭ドット（隠しファイル `.gitignore` 等）は拡張子扱いしない。
        Some(pos) if pos > 0 => NamePair {
            stem: name[..pos].to_string(),
            ext: name[pos + 1..].to_string(),
        },
        _ => NamePair {
            stem: name.to_string(),
            ext: String::new(),
        },
    }
}

/// 本体＋拡張子を 1 つのファイル名へ。拡張子が空ならドットなし。
fn assemble(p: &NamePair) -> String {
    if p.ext.is_empty() {
        p.stem.clone()
    } else {
        format!("{}.{}", p.stem, p.ext)
    }
}

/// 禁止文字（`\ / : * ? " < > |`）を除去する（仕様 §3.10.1）。
///
/// `matches!` で分岐し、`&[char]` の線形探索を避ける（コンパイラがジャンプテーブルへ最適化）。
fn sanitize(name: &str) -> String {
    name.chars()
        .filter(|&c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect()
}

/// パスのファイル名（末尾要素）を取り出す。
fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 同一出力フォルダ内で変換後名が重複するエントリへ [`RenameIssue::Duplicate`] を付ける。
///
/// 大文字小文字を無視して比較する（Windows の既定）。既存の問題（空名）は上書きしない。
fn detect_duplicates(entries: &mut [RenameEntry], dest: Option<&LongPath>) {
    // (出力フォルダ, 小文字名) のキーを **エントリごとに 1 度だけ** 計算する（出力フォルダの
    // PathBuf 生成と名前の小文字化は安くないため、集計と判定で 2 度計算しない）。フォルダは
    // 非 Unicode 損失を避けるため `PathBuf` をそのままキーにする（同一スキャン内の兄弟は同じ
    // 親パスを返すため大小無視不要）。名前のみ小文字化して Windows 既定の大小無視一致を取る。
    let keys: Vec<(PathBuf, String)> = entries
        .iter()
        .map(|e| (output_dir_key(e, dest), e.renamed.to_lowercase()))
        .collect();
    // 件数は計算済みキーへの参照で集計する（キーの再確保を避ける）。
    let mut counts: HashMap<&(PathBuf, String), usize> = HashMap::with_capacity(keys.len());
    for (e, key) in entries.iter().zip(keys.iter()) {
        if e.renamed.is_empty() {
            continue;
        }
        *counts.entry(key).or_insert(0) += 1;
    }
    for (e, key) in entries.iter_mut().zip(keys.iter()) {
        if e.issue.is_some() || e.renamed.is_empty() {
            continue;
        }
        if counts.get(&key).copied().unwrap_or(0) > 1 {
            e.issue = Some(RenameIssue::Duplicate);
        }
    }
}

/// 出力フォルダ（dest 指定時は全エントリ共通、未指定はその場＝親フォルダ）。
fn output_dir_key(e: &RenameEntry, dest: Option<&LongPath>) -> PathBuf {
    match dest {
        Some(d) => d.as_path().to_path_buf(),
        None => e
            .path
            .as_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::new_cancel;
    use crate::rename::recipe::RecipeStep;
    use crate::rename::step::{RenameStep, SeqPosition};

    fn targets(root: &Path, sub: bool, folders: bool, filters: Vec<String>) -> RenameTargets {
        RenameTargets {
            root: LongPath::new(root).unwrap(),
            include_subfolders: sub,
            include_folders: folders,
            filters,
        }
    }

    fn seq_recipe() -> RenameRecipe {
        RenameRecipe::new(vec![RecipeStep::new(RenameStep::Sequence {
            position: SeqPosition::Prefix,
            digits: 3,
            start: 1,
            sep: "_".into(),
        })])
    }

    fn renamed_of<'a>(plan: &'a RenamePlan, original: &str) -> Option<&'a str> {
        plan.entries
            .iter()
            .find(|e| e.original == original)
            .map(|e| e.renamed.as_str())
    }

    #[test]
    fn parse_filters_splits_and_trims() {
        assert_eq!(parse_filters("*.jpg; *.png ;;"), vec!["*.jpg", "*.png"]);
        assert!(parse_filters("   ").is_empty());
    }

    #[test]
    fn build_plan_numbers_files_in_path_order() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("b.txt"), b"x").unwrap();
        std::fs::write(base.join("a.txt"), b"x").unwrap();
        std::fs::write(base.join("c.txt"), b"x").unwrap();

        let plan = build_plan(
            &targets(base, false, false, Vec::new()),
            &seq_recipe(),
            None,
            &new_cancel(),
        );
        // フルパス昇順で連番（a→001, b→002, c→003）。
        assert_eq!(renamed_of(&plan, "a.txt"), Some("001_a.txt"));
        assert_eq!(renamed_of(&plan, "b.txt"), Some("002_b.txt"));
        assert_eq!(renamed_of(&plan, "c.txt"), Some("003_c.txt"));
        assert_eq!(plan.changed_count(), 3);
        assert!(!plan.has_conflicts());
    }

    #[test]
    fn filter_limits_targets() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("keep.jpg"), b"x").unwrap();
        std::fs::write(base.join("skip.txt"), b"x").unwrap();

        let plan = build_plan(
            &targets(base, false, false, parse_filters("*.jpg")),
            &seq_recipe(),
            None,
            &new_cancel(),
        );
        assert_eq!(plan.entries.len(), 1);
        assert_eq!(plan.entries[0].original, "keep.jpg");
    }

    #[test]
    fn subfolders_respected_only_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("top.txt"), b"x").unwrap();
        let sub = base.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("deep.txt"), b"x").unwrap();

        let flat = build_plan(
            &targets(base, false, false, Vec::new()),
            &seq_recipe(),
            None,
            &new_cancel(),
        );
        assert_eq!(flat.entries.len(), 1);

        let deep = build_plan(
            &targets(base, true, false, Vec::new()),
            &seq_recipe(),
            None,
            &new_cancel(),
        );
        assert_eq!(deep.entries.len(), 2);
    }

    #[test]
    fn detects_duplicate_outputs_in_same_folder() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"x").unwrap();
        std::fs::write(base.join("b.txt"), b"x").unwrap();

        // 本体を定数文字列へ → 両方 "same.txt" に衝突。
        let recipe = RenameRecipe::new(vec![RecipeStep::new(RenameStep::ReplaceRegex {
            pattern: ".*".into(),
            replace: "same".into(),
        })]);
        let plan = build_plan(
            &targets(base, false, false, Vec::new()),
            &recipe,
            None,
            &new_cancel(),
        );
        assert!(plan.has_conflicts());
        assert_eq!(plan.conflict_count(), 2);
        assert!(
            plan.entries
                .iter()
                .all(|e| e.issue == Some(RenameIssue::Duplicate))
        );
    }

    #[test]
    fn sanitizes_illegal_characters() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("x.txt"), b"x").unwrap();

        let recipe = RenameRecipe::new(vec![RecipeStep::new(RenameStep::ReplaceRegex {
            pattern: "x".into(),
            replace: "a:b*c".into(),
        })]);
        let plan = build_plan(
            &targets(base, false, false, Vec::new()),
            &recipe,
            None,
            &new_cancel(),
        );
        // ':' と '*' は除去され "abc.txt"。
        assert_eq!(plan.entries[0].renamed, "abc.txt");
    }

    #[test]
    fn duplicate_detection_uses_dest_for_copy_or_move() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let suba = base.join("a");
        let subb = base.join("b");
        std::fs::create_dir(&suba).unwrap();
        std::fs::create_dir(&subb).unwrap();
        std::fs::write(suba.join("same.txt"), b"x").unwrap();
        std::fs::write(subb.join("same.txt"), b"x").unwrap();

        // 無変換レシピ → 両方 "same.txt"。別フォルダなのでその場(None)では衝突しない。
        let recipe = RenameRecipe::default();
        let inplace = build_plan(
            &targets(base, true, false, Vec::new()),
            &recipe,
            None,
            &new_cancel(),
        );
        assert!(!inplace.has_conflicts());

        // dest 集約（コピー/移動）では同一フォルダ扱い → 衝突。
        let dest = LongPath::new(base.join("out")).unwrap();
        let to_dest = build_plan(
            &targets(base, true, false, Vec::new()),
            &recipe,
            Some(&dest),
            &new_cancel(),
        );
        assert!(to_dest.has_conflicts());
        assert_eq!(to_dest.conflict_count(), 2);
    }

    #[test]
    fn build_plan_respects_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"x").unwrap();
        let cancel = new_cancel();
        crate::progress::request_cancel(&cancel);
        let plan = build_plan(
            &targets(base, false, false, Vec::new()),
            &seq_recipe(),
            None,
            &cancel,
        );
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn split_name_uses_last_dot_for_compound_extensions() {
        // `.tar.gz` は最後のドットで分割（Path::extension と同じ）＝ stem=".tar" ext="gz"。
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("archive.tar.gz"), b"x").unwrap();
        let plan = build_plan(
            &targets(base, false, false, Vec::new()),
            &seq_recipe(),
            None,
            &new_cancel(),
        );
        assert_eq!(plan.entries[0].renamed, "001_archive.tar.gz");
    }

    #[test]
    fn empty_name_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("gone"), b"x").unwrap();

        // 本体・拡張子を全削除 → 空名。
        let recipe = RenameRecipe::new(vec![RecipeStep::new(RenameStep::ReplaceRegex {
            pattern: ".*".into(),
            replace: "".into(),
        })]);
        let plan = build_plan(
            &targets(base, false, false, Vec::new()),
            &recipe,
            None,
            &new_cancel(),
        );
        assert_eq!(plan.entries[0].issue, Some(RenameIssue::EmptyName));
    }
}
