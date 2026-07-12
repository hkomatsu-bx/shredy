//! 一括リネームの実行とアンドゥ（仕様 §3.10.1 動作モード・安全策）。
//!
//! [`execute_rename`] は [`RenamePlan`] のうち**変更があり問題のない**エントリだけを、その場
//! 改名／コピー＆改名／移動＆改名のいずれかで適用する。安全策:
//!
//! - 出力先に既存ファイルがある場合は**上書きせずスキップ**（データ保全）。
//! - ディレクトリも対象になり得るため、子を親より先に処理（**深い順**）し、親フォルダ改名で
//!   子のパスが無効化されるのを防ぐ。
//! - 適用結果から [`RenameUndo`] を返し、直前の操作を元に戻せる（移動/改名は逆向き・コピーは
//!   作成物の削除）。

use std::fs;
use std::path::{Path, PathBuf};

use crate::path::LongPath;

use super::plan::{RenameEntry, RenamePlan};

/// 実行モード（仕様 §3.10.1 動作モード）。
#[derive(Debug, Clone)]
pub enum RenameMode {
    /// その場で改名する。
    InPlace,
    /// 出力先フォルダへコピーして改名する。
    Copy { dest: LongPath },
    /// 出力先フォルダへ移動して改名する。
    Move { dest: LongPath },
}

/// 実行レポート（成功＝(変更前, 変更後) / 失敗＝(変更前, 理由)）。
#[derive(Debug, Default)]
pub struct RenameReport {
    pub succeeded: Vec<(LongPath, LongPath)>,
    pub failed: Vec<(LongPath, String)>,
}

impl RenameReport {
    #[must_use]
    pub fn ok_count(&self) -> usize {
        self.succeeded.len()
    }

    #[must_use]
    pub fn fail_count(&self) -> usize {
        self.failed.len()
    }
}

/// アンドゥ操作。
enum UndoOp {
    /// 改名・移動を逆向きに戻す（`to`→`from`）。
    Revert { from: LongPath, to: LongPath },
    /// コピーで作成したファイル/フォルダを削除する。
    RemoveCopy { path: LongPath, is_dir: bool },
}

/// 直前のリネームを元に戻すための情報（仕様 §3.10.1 Undo）。
#[derive(Default)]
pub struct RenameUndo {
    ops: Vec<UndoOp>,
}

/// アンドゥ結果。
#[derive(Debug, Default)]
pub struct UndoReport {
    /// 元に戻した（または削除した）件数。
    pub reverted: usize,
    /// 戻せなかった項目（パス・理由）。
    pub failed: Vec<(LongPath, String)>,
}

impl RenameUndo {
    /// 元に戻せる操作があるか。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// 元に戻せる操作数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// 記録した操作を**逆順**で元に戻す。
    pub fn undo(self) -> UndoReport {
        let mut report = UndoReport::default();
        for op in self.ops.into_iter().rev() {
            let result = match &op {
                UndoOp::Revert { from, to } => move_path(to.as_path(), from.as_path()),
                UndoOp::RemoveCopy { path, is_dir } => remove_path(path.as_path(), *is_dir),
            };
            match result {
                Ok(()) => report.reverted += 1,
                Err(e) => {
                    let path = match op {
                        UndoOp::Revert { to, .. } => to,
                        UndoOp::RemoveCopy { path, .. } => path,
                    };
                    report.failed.push((path, e));
                }
            }
        }
        report
    }
}

/// 計画を実行し、(レポート, アンドゥ情報) を返す。
///
/// 変更がない・問題（空名・重複）のあるエントリはスキップする。
#[must_use]
pub fn execute_rename(plan: &RenamePlan, mode: &RenameMode) -> (RenameReport, RenameUndo) {
    let mut report = RenameReport::default();
    let mut undo = RenameUndo::default();

    // 適用対象＝変更があり問題のないエントリ。深い順（子→親）に処理する。
    let mut targets: Vec<&RenameEntry> = plan
        .entries
        .iter()
        .filter(|e| e.changed && e.issue.is_none())
        .collect();
    targets.sort_by_key(|e| std::cmp::Reverse(e.path.as_path().components().count()));

    // 出力先（Copy/Move）は事前に作成する。失敗すれば全件失敗扱い。
    if let RenameMode::Copy { dest } | RenameMode::Move { dest } = mode {
        if let Err(e) = fs::create_dir_all(dest.fs_path()) {
            for entry in &targets {
                report
                    .failed
                    .push((entry.path.clone(), format!("出力先を作成できません: {e}")));
            }
            return (report, undo);
        }
    }

    for entry in targets {
        let Some(dest_path) = target_path(entry, mode) else {
            report
                .failed
                .push((entry.path.clone(), "出力先パスを構築できません".into()));
            continue;
        };
        // 上書き防止: 出力先が既に存在する場合はスキップ（データ保全）。
        // exists() と後続の rename/copy の間に別プロセスが同名を作る TOCTOU は理論上残るが、
        // fail-safe に倒れる: Copy は `create_new`（後述）で既存を上書きしない。InPlace/Move の
        // `std::fs::rename` は Windows で MOVEFILE_REPLACE_EXISTING を立てないため、既存があれば
        // 上書きせず失敗し、その旨が `report.failed` に記録される（データ損失は起きない）。
        if fs_path(dest_path.as_path()).exists() {
            report.failed.push((
                entry.path.clone(),
                "出力先が既に存在します（スキップ）".into(),
            ));
            continue;
        }
        let result = match mode {
            RenameMode::InPlace | RenameMode::Move { .. } => {
                move_path(entry.path.as_path(), dest_path.as_path()).map(|()| UndoOp::Revert {
                    from: entry.path.clone(),
                    to: dest_path.clone(),
                })
            }
            RenameMode::Copy { .. } => {
                copy_path(entry.path.as_path(), dest_path.as_path(), entry.is_dir).map(|()| {
                    UndoOp::RemoveCopy {
                        path: dest_path.clone(),
                        is_dir: entry.is_dir,
                    }
                })
            }
        };
        match result {
            Ok(op) => {
                undo.ops.push(op);
                report.succeeded.push((entry.path.clone(), dest_path));
            }
            Err(e) => report.failed.push((entry.path.clone(), e)),
        }
    }
    (report, undo)
}

/// エントリの出力先パスを構築する（InPlace=親フォルダ・Copy/Move=dest 直下）。
fn target_path(entry: &RenameEntry, mode: &RenameMode) -> Option<LongPath> {
    let dir: PathBuf = match mode {
        RenameMode::InPlace => entry.path.as_path().parent()?.to_path_buf(),
        RenameMode::Copy { dest } | RenameMode::Move { dest } => dest.as_path().to_path_buf(),
    };
    LongPath::new(dir.join(&entry.renamed)).ok()
}

/// パスを移動/改名する（同一ボリュームは rename・失敗時はファイルのみ copy+remove で代替）。
fn move_path(from: &Path, to: &Path) -> Result<(), String> {
    let from_fs = fs_path(from);
    let to_fs = fs_path(to);
    match fs::rename(&from_fs, &to_fs) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            // クロスデバイス等で rename 不可ならファイルは copy+remove で代替する。
            if from_fs.is_file() {
                copy_file_new(from, to)?;
                if let Err(e) = fs::remove_file(&from_fs) {
                    // 元ファイルを消せない → 作成したコピーを撤去し元状態へ戻す（孤立コピー防止）。
                    let _ = fs::remove_file(&to_fs);
                    return Err(format!("元ファイル削除失敗（コピーを撤去）: {e}"));
                }
                Ok(())
            } else {
                Err(format!("移動失敗: {rename_err}"))
            }
        }
    }
}

/// パスをコピーする（ファイル＝上書きせずコピー・フォルダ＝再帰コピー）。
fn copy_path(from: &Path, to: &Path, is_dir: bool) -> Result<(), String> {
    if is_dir {
        copy_dir_recursive(from, to)
    } else {
        copy_file_new(from, to)
    }
}

/// 上書きせずにファイルをコピーする（出力先が既に存在すれば失敗＝データ保全）。
///
/// `create_new` により exists() 事前チェック後の TOCTOU でも既存ファイルを上書きしない。
fn copy_file_new(from: &Path, to: &Path) -> Result<(), String> {
    let mut src =
        fs::File::open(fs_path(from)).map_err(|e| format!("コピー元を開けません: {e}"))?;
    let mut dst = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(fs_path(to))
        .map_err(|e| format!("コピー失敗: {e}"))?;
    std::io::copy(&mut src, &mut dst)
        .map(|_| ())
        .map_err(|e| format!("コピー失敗: {e}"))
}

/// ディレクトリを再帰的にコピーする。
///
/// シンボリックリンク/ジャンクションはリンク実体の巻き込みを避けてスキップする
/// （仕様・安全優先＝実体巻き込み防止）。
fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(fs_path(to)).map_err(|e| format!("フォルダ作成失敗: {e}"))?;
    for ent in fs::read_dir(fs_path(from)).map_err(|e| format!("読み取り失敗: {e}"))? {
        let ent = ent.map_err(|e| format!("読み取り失敗: {e}"))?;
        let ft = ent.file_type().map_err(|e| format!("種別取得失敗: {e}"))?;
        if ft.is_symlink() {
            continue;
        }
        let child_to = to.join(ent.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&ent.path(), &child_to)?;
        } else if ft.is_file() {
            copy_file_new(&ent.path(), &child_to)?;
        }
    }
    Ok(())
}

/// パスを削除する（アンドゥでのコピー作成物の削除）。
fn remove_path(path: &Path, is_dir: bool) -> Result<(), String> {
    let p = fs_path(path);
    if is_dir {
        fs::remove_dir_all(p).map_err(|e| format!("削除失敗: {e}"))
    } else {
        fs::remove_file(p).map_err(|e| format!("削除失敗: {e}"))
    }
}

/// ロングパス対応の OS パス（`\\?\` プレフィックス）を得る。変換失敗時は素のパスで続行する。
fn fs_path(path: &Path) -> PathBuf {
    match LongPath::new(path) {
        Ok(lp) => lp.fs_path().into_owned(),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "ロングパス変換に失敗・素のパスで続行");
            path.to_path_buf()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::new_cancel;
    use crate::rename::plan::{build_plan, RenameTargets};
    use crate::rename::recipe::{RecipeStep, RenameRecipe};
    use crate::rename::step::{RenameStep, SeqPosition};

    fn targets(root: &Path) -> RenameTargets {
        RenameTargets {
            root: LongPath::new(root).unwrap(),
            include_subfolders: false,
            include_folders: false,
            filters: Vec::new(),
        }
    }

    fn prefix_seq() -> RenameRecipe {
        RenameRecipe::new(vec![RecipeStep::new(RenameStep::Sequence {
            position: SeqPosition::Prefix,
            digits: 3,
            start: 1,
            sep: "_".into(),
        })])
    }

    #[test]
    fn in_place_renames_files() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"x").unwrap();
        std::fs::write(base.join("b.txt"), b"y").unwrap();

        let plan = build_plan(&targets(base), &prefix_seq(), None, &new_cancel());
        let (report, undo) = execute_rename(&plan, &RenameMode::InPlace);

        assert_eq!(report.ok_count(), 2);
        assert_eq!(report.fail_count(), 0);
        assert!(base.join("001_a.txt").exists());
        assert!(base.join("002_b.txt").exists());
        assert!(!base.join("a.txt").exists());
        assert_eq!(undo.len(), 2);
    }

    #[test]
    fn in_place_undo_restores_originals() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"x").unwrap();

        let plan = build_plan(&targets(base), &prefix_seq(), None, &new_cancel());
        let (_report, undo) = execute_rename(&plan, &RenameMode::InPlace);
        assert!(base.join("001_a.txt").exists());

        let undo_report = undo.undo();
        assert_eq!(undo_report.reverted, 1);
        assert!(undo_report.failed.is_empty());
        assert!(base.join("a.txt").exists());
        assert!(!base.join("001_a.txt").exists());
    }

    #[test]
    fn copy_mode_keeps_originals_and_undo_removes_copies() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"x").unwrap();
        let dest = base.join("out");

        let plan = build_plan(&targets(base), &prefix_seq(), None, &new_cancel());
        let mode = RenameMode::Copy {
            dest: LongPath::new(&dest).unwrap(),
        };
        let (report, undo) = execute_rename(&plan, &mode);

        assert_eq!(report.ok_count(), 1);
        assert!(base.join("a.txt").exists()); // 元ファイルは残る
        assert!(dest.join("001_a.txt").exists());

        let undo_report = undo.undo();
        assert_eq!(undo_report.reverted, 1);
        assert!(!dest.join("001_a.txt").exists()); // コピーは削除
        assert!(base.join("a.txt").exists()); // 元は無事
    }

    #[test]
    fn move_mode_relocates_files() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"x").unwrap();
        let dest = base.join("out");

        let plan = build_plan(&targets(base), &prefix_seq(), None, &new_cancel());
        let mode = RenameMode::Move {
            dest: LongPath::new(&dest).unwrap(),
        };
        let (report, _undo) = execute_rename(&plan, &mode);

        assert_eq!(report.ok_count(), 1);
        assert!(!base.join("a.txt").exists()); // 元は移動済
        assert!(dest.join("001_a.txt").exists());
    }

    #[test]
    fn does_not_overwrite_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"new").unwrap();
        // 既存の出力先（上書きされてはならない・フィルタ対象外）。
        std::fs::write(base.join("001_a.txt"), b"existing").unwrap();

        // a.txt だけを対象にする（連番で "001_a.txt" へ＝既存と衝突）。
        let tgt = RenameTargets {
            root: LongPath::new(base).unwrap(),
            include_subfolders: false,
            include_folders: false,
            filters: vec!["a.txt".into()],
        };
        let plan = build_plan(&tgt, &prefix_seq(), None, &new_cancel());
        let (report, _undo) = execute_rename(&plan, &RenameMode::InPlace);

        assert_eq!(report.ok_count(), 0);
        assert_eq!(report.fail_count(), 1);
        // 既存ファイルの内容が保持される（上書きされない）。
        assert_eq!(
            std::fs::read_to_string(base.join("001_a.txt")).unwrap(),
            "existing"
        );
    }

    #[test]
    fn copy_mode_does_not_overwrite_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"new").unwrap();
        let dest = base.join("out");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("001_a.txt"), b"existing").unwrap();

        let dest_lp = LongPath::new(&dest).unwrap();
        let plan = build_plan(&targets(base), &prefix_seq(), Some(&dest_lp), &new_cancel());
        let (report, _undo) = execute_rename(&plan, &RenameMode::Copy { dest: dest_lp });

        assert_eq!(report.ok_count(), 0);
        assert_eq!(report.fail_count(), 1);
        // 既存のコピー先内容は保持される（上書きされない）。
        assert_eq!(
            std::fs::read_to_string(dest.join("001_a.txt")).unwrap(),
            "existing"
        );
    }

    #[test]
    fn in_place_renames_child_before_parent_folder() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let sub = base.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("f.txt"), b"x").unwrap();

        // フォルダ自身も対象・サブフォルダ含む。
        let tgt = RenameTargets {
            root: LongPath::new(base).unwrap(),
            include_subfolders: true,
            include_folders: true,
            filters: Vec::new(),
        };
        let plan = build_plan(&tgt, &prefix_seq(), None, &new_cancel());
        let (report, _undo) = execute_rename(&plan, &RenameMode::InPlace);

        // 子（f.txt）と親（sub）の両方が改名され、最終配置が一貫する。
        assert_eq!(report.fail_count(), 0);
        assert!(base.join("001_sub").join("002_f.txt").exists());
    }
}
