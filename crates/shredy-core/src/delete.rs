//! 削除・移動・削除履歴ログ・手動削除リスト（仕様 §3.6 / §3.6a）。
//!
//! 安全第一: ロック済み・使用中ファイルは除外し、各重複グループに最低 1 件を残す
//! （fail-closed, SECURITY-11/15）。実削除はごみ箱（`trash`）が既定。

use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::model::{DuplicateGroup, KeepFileRule};
use crate::path::LongPath;
use crate::select::{LockSet, Selection, Selector};

/// 削除モード（仕様 §3.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    /// ごみ箱へ移動（既定。復元可能）。
    RecycleBin,
    /// 完全削除（復元不可）。
    Permanent,
}

/// 削除対象から除外した理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// ロック済み。
    Locked,
    /// リパースポイント（シンボリックリンク/ジャンクション。実体巻き込み防止で除外）。
    Reparse,
    /// 他プロセスが使用中。
    InUse,
    /// 読み取り専用（`DeleteReadOnly=false` 時に除外）。
    ReadOnly,
    /// 「1つ残す」保証のため残した。
    KeptForSafety,
}

/// 削除計画（実行前に確認できる）。
#[derive(Debug, Default)]
pub struct DeletePlan {
    pub to_delete: Vec<LongPath>,
    pub skipped: Vec<(LongPath, SkipReason)>,
}

/// 削除実行の結果。
#[derive(Debug, Default)]
pub struct DeleteReport {
    /// 削除に成功したファイルとそのサイズ。
    pub deleted: Vec<(LongPath, u64)>,
    /// 削除に失敗したファイルとエラーメッセージ。
    pub failed: Vec<(LongPath, String)>,
}

/// 削除・移動の実行器（I/O 抽象を介して安全判定する）。
pub struct Deleter<'a> {
    fs: &'a dyn crate::io::FileReader,
}

impl<'a> Deleter<'a> {
    #[must_use]
    pub fn new(fs: &'a dyn crate::io::FileReader) -> Self {
        Self { fs }
    }

    /// 選択結果から安全な削除計画を作る。
    ///
    /// 安全第一（多重防御, SECURITY-11/15）として、選択済みでも以下は削除対象から除外する:
    /// - ロックマーク（[`LockSet`](crate::select::LockSet)）が付いたファイル → [`SkipReason::Locked`]
    /// - リパースポイント（シンボリックリンク/ジャンクション） → [`SkipReason::Reparse`]
    /// - 使用中（他プロセスがオープン中）のファイル → [`SkipReason::InUse`]
    ///
    /// さらにグループ全件が削除対象になる場合は keep を 1 件残す（「1つ残す」, 仕様 §3.4/§3.6）。
    ///
    /// `Selector::select_*` 経由の `Selection` は既にロック済みを除外しているが、
    /// [`Selection::from_paths`](crate::select::Selection::from_paths) で直接構築する経路
    /// （GUI の手動チェック等）では `locks` をここで参照し、呼び出し側の事前除外漏れがあっても
    /// ロック済みファイルを削除しないことを本関数が保証する。
    ///
    /// `delete_read_only=false`（既定）のときは読み取り専用ファイルを削除対象から除外する
    /// （[`SkipReason::ReadOnly`], 設定 `DeleteReadOnly`）。`true` のときは削除を許可し、
    /// [`execute`](Self::execute) が削除直前に読み取り専用属性を解除する。
    #[must_use]
    pub fn plan(
        &self,
        groups: &[DuplicateGroup],
        selection: &Selection,
        rule: KeepFileRule,
        locks: &LockSet,
        delete_read_only: bool,
    ) -> DeletePlan {
        let mut plan = DeletePlan::default();
        for group in groups {
            let keep = Selector::keep_target(group, rule).map(|f| f.path.clone());
            // グループ内で削除対象になり得る（選択済み）ファイル。
            let mut deletable: Vec<&LongPath> = Vec::new();
            for f in &group.files {
                if !selection.is_selected(&f.path) {
                    continue;
                }
                match self.skip_reason_for(&f.path, locks, delete_read_only) {
                    Some(reason) => plan.skipped.push((f.path.clone(), reason)),
                    None => deletable.push(&f.path),
                }
            }
            // fail-closed: 全件削除になるなら keep を 1 件残す。
            let total = group.files.len();
            if deletable.len() >= total {
                if let Some(keep) = &keep {
                    deletable.retain(|p| *p != keep);
                    plan.skipped.push((keep.clone(), SkipReason::KeptForSafety));
                }
            }
            for p in deletable {
                plan.to_delete.push(p.clone());
            }
        }
        plan
    }

    /// 1 ファイルの削除可否を安全フィルタで判定する（多重防御, SECURITY-11/15）。
    ///
    /// 削除不可なら理由を返し、削除可なら `None`。メタデータは reparse/read-only 判定で
    /// 共用し syscall を 1 回に集約する。判定順（ロック→リパース→読み取り専用→使用中）は
    /// [`plan`](Self::plan) と一致させる。
    fn skip_reason_for(
        &self,
        path: &LongPath,
        locks: &LockSet,
        delete_read_only: bool,
    ) -> Option<SkipReason> {
        // 多重防御: 選択されていてもロック済みは絶対に削除しない。
        if locks.is_locked(path) {
            return Some(SkipReason::Locked);
        }
        let meta = self.fs.metadata(path).ok();
        // 多重防御: リパースポイント（シンボリックリンク/ジャンクション）は実体を巻き込まない
        // よう除外する（spec §安全優先。follow_symlinks=true 時の防御。fail-closed）。
        if meta.as_ref().map(|m| m.is_reparse).unwrap_or(false) {
            return Some(SkipReason::Reparse);
        }
        // 読み取り専用は既定で除外（DeleteReadOnly=true で許可）。
        if !delete_read_only && meta.as_ref().map(|m| m.is_read_only).unwrap_or(false) {
            return Some(SkipReason::ReadOnly);
        }
        // 使用中判定はファイルハンドル前提のため、ディレクトリ（空フォルダ §3.9.1）には適用しない
        // （ディレクトリに対する is_file_in_use は誤って使用中/失敗を返し、空フォルダ削除を妨げる）。
        let is_dir = meta.as_ref().map(|m| m.is_dir).unwrap_or(false);
        if !is_dir && self.fs.is_in_use(path).unwrap_or(true) {
            return Some(SkipReason::InUse);
        }
        None
    }

    /// パスリストから削除計画を作る（「1つ残す」を適用しない＝選択全件が対象）。
    ///
    /// 重複グループを伴わないユーティリティスキャン（§3.9）の削除用。[`plan`](Self::plan) と
    /// 同じ per-file 安全フィルタ（ロック/リパース/使用中/読み取り専用）を適用するが、
    /// グループ単位の「1つ残す」保証は行わない（空ファイル・一時ファイル等は全件削除してよい）。
    #[must_use]
    pub fn plan_paths(
        &self,
        paths: &[LongPath],
        locks: &LockSet,
        delete_read_only: bool,
    ) -> DeletePlan {
        let mut plan = DeletePlan::default();
        for p in paths {
            match self.skip_reason_for(p, locks, delete_read_only) {
                Some(reason) => plan.skipped.push((p.clone(), reason)),
                None => plan.to_delete.push(p.clone()),
            }
        }
        plan
    }

    /// 計画に従って削除を実行する。
    ///
    /// `plan` に読み取り専用ファイルが含まれるのは `DeleteReadOnly=true` の場合のみ
    /// （[`plan`](Self::plan) が除外する）。読み取り専用属性が付いたままだとごみ箱/完全
    /// 削除が失敗するため、削除直前に属性を解除する（best-effort）。
    pub fn execute(&self, plan: &DeletePlan, mode: DeleteMode) -> DeleteReport {
        let mut report = DeleteReport::default();
        for path in &plan.to_delete {
            // metadata は FileReader 経由＝`symlink_metadata`（リンク非追跡, io.rs）。size /
            // is_read_only / is_dir をここで一括取得し、Permanent 分岐での再 stat を避ける。
            let meta = self.fs.metadata(path).ok();
            let size = meta.as_ref().map(|m| m.size.0).unwrap_or(0);
            let meta_is_dir = meta.as_ref().map(|m| m.is_dir);
            if meta.as_ref().map(|m| m.is_read_only).unwrap_or(false) {
                clear_read_only(path);
            }
            let result = match mode {
                // ごみ箱はシェル API（IFileOperation）経由。`\\?\` 拡張長パスはシェル API が
                // 受け付けないため `as_path()` を渡す（長パスのごみ箱削除はシェル側の制約）。
                DeleteMode::RecycleBin => trash::delete(path.as_path()).map_err(|e| e.to_string()),
                // 完全削除は std 経由なので `fs_path()`（`\\?\`）で 260 字超も削除できる。
                // ユーティリティの空フォルダ（§3.9.1）はディレクトリのため `remove_dir_all` を使う
                // （`remove_file` はディレクトリに失敗する）。リパースポイントは plan で除外済みのため
                // 実体巻き込みの懸念はなく、空フォルダ検出は配下にファイルが無いものだけを報告する。
                DeleteMode::Permanent => remove_permanent(path.fs_path().as_ref(), meta_is_dir)
                    .map_err(|e| e.to_string()),
            };
            match result {
                Ok(()) => report.deleted.push((path.clone(), size)),
                Err(e) => report.failed.push((path.clone(), e)),
            }
        }
        report
    }

    /// ファイルを移動する。移動先に同名があれば一意名へリネームして上書きを防ぐ（仕様 §3.6）。
    ///
    /// `roots`（登録対象フォルダ）が与えられた場合は**フォルダ構造を再現**する: 各ファイルが
    /// 属する登録フォルダからの相対パスを `dest_dir` 配下へ再現する（仕様 §3.10）。
    /// 例: 登録 `D:\Photos` の `D:\Photos\2024\a.jpg` → `dest\2024\a.jpg`。
    /// どの登録フォルダにも属さない場合は `dest_dir` 直下へ移動する（フォールバック）。
    pub fn move_to(&self, paths: &[LongPath], dest_dir: &Path, roots: &[LongPath]) -> DeleteReport {
        let mut report = DeleteReport::default();
        for path in paths {
            let size = self.fs.metadata(path).map(|m| m.size.0).unwrap_or(0);
            let Some(file_name) = path.as_path().file_name() else {
                report
                    .failed
                    .push((path.clone(), "no file name".to_string()));
                continue;
            };
            // 構造再現: 登録フォルダ基準の相対サブフォルダ（ファイル名を除く）を dest 配下に作る。
            let sub_dir = relative_parent_under_roots(path, roots);
            let target_dir = match &sub_dir {
                Some(rel) => dest_dir.join(rel),
                None => dest_dir.to_path_buf(),
            };
            if let Err(e) = std::fs::create_dir_all(&target_dir) {
                report.failed.push((
                    path.clone(),
                    format!("移動先フォルダの作成に失敗しました: {e}"),
                ));
                continue;
            }
            let Some(target) = unique_destination(&target_dir, Path::new(file_name)) else {
                report.failed.push((
                    path.clone(),
                    "移動先に一意なファイル名を確保できませんでした".to_string(),
                ));
                continue;
            };
            // 移動元は std 経由なので `fs_path()`（`\\?\`）で長パスにも対応する。
            match std::fs::rename(path.fs_path(), &target) {
                Ok(()) => report.deleted.push((path.clone(), size)),
                Err(e) => report.failed.push((path.clone(), e.to_string())),
            }
        }
        report
    }

    /// プランを実行し、監査ログ（`shredy_delete.log`）の記録と完了 tracing を一括で行う。
    ///
    /// 「init（実行直前に開始ログを生成）→ 実行 → deleted が空でなければ append → 完了 INFO」
    /// という配管は CLI/GUI に逐語重複していた（仕様 §3.6 の不変条件）。本メソッドへ集約し、
    /// 片側だけ修正される乖離を防ぐ。`save_log=false` なら一切書かない。`now_secs` は init と
    /// append で共通の 1 値を使う（タイムスタンプの再取得差をなくす）。ログ書き込み失敗は
    /// 監査記録の欠落として握り潰さず warn ログに残す（処理自体は継続する）。
    pub fn execute_logged(
        &self,
        plan: &DeletePlan,
        op: FileOp<'_>,
        config_dir: &Path,
        save_log: bool,
        now_secs: i64,
    ) -> DeleteReport {
        let start_msg = op.start_message();
        // 開始ログ（何を・何件対象に・何件を安全フィルタで除外したか）。
        tracing::info!(
            op = %start_msg,
            targets = plan.to_delete.len(),
            skipped = plan.skipped.len(),
            "削除/移動を開始します"
        );
        // 安全フィルタ除外（ロック/使用中/リパース/読み取り専用/「1つ残す」）の内訳。件数が
        // 多くなり得るため DEBUG（既定 INFO では上の集計のみ、詳細はレベル昇格で確認）。
        for (path, reason) in &plan.skipped {
            tracing::debug!(
                path = %path.as_path().display(),
                reason = ?reason,
                "削除対象から除外しました"
            );
        }
        if save_log {
            if let Err(e) = DeleteLog::init(config_dir, &start_msg, now_secs) {
                tracing::warn!(error = %e, "削除ログの初期化に失敗しました");
            }
        }
        let report = match op {
            FileOp::Delete(mode) => self.execute(plan, mode),
            FileOp::Move { dest, roots } => self.move_to(&plan.to_delete, dest, roots),
        };
        if save_log && !report.deleted.is_empty() {
            // 削除履歴は不可逆操作の監査記録。失敗は握り潰さずログに残す。
            if let Err(e) = DeleteLog::append(config_dir, &report.deleted, now_secs) {
                tracing::warn!(error = %e, "削除ログの書き込みに失敗しました");
            }
        }
        // 個々の失敗（パス＋理由）を WARN で記録。不可逆操作の監査として件数だけでなく
        // 「どのファイルがなぜ失敗したか」を残す。
        for (path, err) in &report.failed {
            tracing::warn!(
                path = %path.as_path().display(),
                error = %err,
                "削除/移動に失敗しました"
            );
        }
        tracing::info!(
            deleted = report.deleted.len(),
            failed = report.failed.len(),
            "削除/移動を完了しました"
        );
        report
    }
}

/// 削除/移動の操作種別（`execute_logged` の監査ログメッセージと実行分岐に使う）。
pub enum FileOp<'a> {
    /// 削除（ごみ箱 / 完全削除）。
    Delete(DeleteMode),
    /// 指定フォルダへの移動。`dest` は実行に使うパス（`\\?\` 等の解決は呼び出し側）。
    /// `roots` は登録対象フォルダ。移動時はこの基準で相対パス構造を `dest` 配下へ再現する
    /// （仕様 §3.6・§3.10）。空のときは構造再現せず `dest` 直下へ移動する。
    Move {
        dest: &'a Path,
        roots: &'a [LongPath],
    },
}

impl FileOp<'_> {
    /// 監査ログの開始メッセージ（CLI/GUI で同一文言だったものを集約）。
    fn start_message(&self) -> String {
        match self {
            FileOp::Delete(mode) => format!("削除を開始 (mode={mode:?})"),
            FileOp::Move { .. } => "移動を開始".to_string(),
        }
    }
}

/// 移動の構造再現用: ファイルが属する登録フォルダ（最長一致）からの相対**親**フォルダを返す。
///
/// 例: roots=[`D:\Photos`]・path=`D:\Photos\2024\a.jpg` → `Some("2024")`。
/// ファイルが登録フォルダ直下なら相対親は空のため `None`（=`dest` 直下）。どの登録フォルダにも
/// 属さない場合も `None`。複数の登録フォルダがネストする場合は最長一致（最も深い）を採る。
fn relative_parent_under_roots(path: &LongPath, roots: &[LongPath]) -> Option<PathBuf> {
    let p = path.as_path();
    let rel = roots
        .iter()
        .filter_map(|r| {
            p.strip_prefix(r.as_path())
                .ok()
                .map(|rel| (r.as_path().components().count(), rel))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, rel)| rel)?;
    // 相対パスからファイル名を除いた親（サブフォルダ）。空なら None。
    match rel.parent() {
        Some(parent) if parent.as_os_str().is_empty() => None,
        Some(parent) => Some(parent.to_path_buf()),
        None => None,
    }
}

/// 読み取り専用属性を解除する（best-effort）。失敗しても削除は試行する。
///
/// `DeleteReadOnly=true` で読み取り専用ファイルを削除する際に呼ぶ。属性が残ったままだと
/// `trash::delete` / `remove_file` が失敗するため、削除直前に解除する。
///
/// ここでの `std::fs::metadata` 読みは `set_permissions` に渡す [`std::fs::Permissions`] を
/// 得るために構造上必要（std は属性のみの直接クリア手段を持たない）。呼び出しは読み取り専用
/// エントリに限られる稀な経路のため、[`Deleter::execute`] の常用パスの stat 重複（H-3）とは別物。
//
// clippy::permissions_set_readonly_false は Unix で `set_readonly(false)` が全ユーザー
// 書き込み可（0o666）を意味する点を警告する。本ツールは Windows 専用で、ここでは
// FILE_ATTRIBUTE_READONLY を外して削除直前に書き込み可能化する正当な用途のため許可する。
/// 完全削除（ディレクトリは再帰削除・ファイルは単体削除）。
///
/// `meta_is_dir` が既知（`symlink_metadata` 由来）ならそれを使い再 stat を避ける。不明な
/// 稀なケースのみ補助的に `symlink_metadata` する。リパースポイントは plan で除外済み。
fn remove_permanent(fp: &Path, meta_is_dir: Option<bool>) -> std::io::Result<()> {
    let is_dir = match meta_is_dir {
        Some(d) => d,
        None => std::fs::symlink_metadata(fp)
            .map(|m| m.is_dir())
            .unwrap_or(false),
    };
    if is_dir {
        std::fs::remove_dir_all(fp)
    } else {
        std::fs::remove_file(fp)
    }
}

#[allow(clippy::permissions_set_readonly_false)]
fn clear_read_only(path: &LongPath) {
    let fs_path = path.fs_path();
    if let Ok(md) = std::fs::metadata(fs_path.as_ref()) {
        let mut perms = md.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            if let Err(e) = std::fs::set_permissions(fs_path.as_ref(), perms) {
                tracing::warn!(error = %e, "読み取り専用属性の解除に失敗しました");
            }
        }
    }
}

/// 移動先に同名ファイルが存在する場合、`name (1).ext` のように一意名を作る。
///
/// 一意名を確保できなかった場合（候補が全滅）は `None` を返す。`None` を既存名へ
/// フォールバックさせると `rename` が**既存ファイルを上書き**してしまう（データ損失）
/// ため、呼び出し側で失敗として扱う（fail-closed, 仕様 §3.6 安全優先）。
fn unique_destination(dir: &Path, file_name: &Path) -> Option<PathBuf> {
    // 注: exists() と後続 rename の間に別プロセスが同名を作る TOCTOU 競合は理論上残るが、
    // 重複整理の文脈では稀かつ低影響のため設計上許容する。
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return Some(candidate);
    }
    let stem = file_name
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = file_name
        .extension()
        .map(|e| e.to_string_lossy().into_owned());
    for i in 1..u32::MAX {
        let new_name = match &ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None => format!("{stem} ({i})"),
        };
        let candidate = dir.join(new_name);
        if !candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

// ---- 削除履歴ログ（shredy_delete.log, 仕様 §3.6） ----

/// 削除履歴ログのファイル名（日付なし・追記・ローテ/purge なし。仕様 §3.6/§5）。
pub const DELETE_LOG_FILE: &str = "shredy_delete.log";

/// 削除履歴ログ。
pub struct DeleteLog;

impl DeleteLog {
    /// Unix 秒から `YYYYMMDD` 形式の日付スタンプを作る（UTC）。
    ///
    /// 手動削除リスト（§3.6a）のファイル名スタンプ等に用いる。
    #[must_use]
    pub fn date_stamp(secs: i64) -> String {
        crate::util::date_stamp(secs)
    }

    /// 削除ログを初期化し、開始ログを 1 行記録する（仕様 §3.6）。
    ///
    /// これにより削除機能の初期化時に `shredy_delete.log` が常に生成される。
    pub fn init(dir: &Path, msg: &str, now_secs: i64) -> Result<()> {
        Self::write_line(dir, &format!("[START]\t{msg}"), now_secs)
    }

    /// 削除記録をログへ追記する（削除日時・パス・サイズ）。
    pub fn append(dir: &Path, records: &[(LongPath, u64)], now_secs: i64) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let path = dir.join(DELETE_LOG_FILE);
        let datetime = crate::util::format_datetime_local(now_secs);
        let mut file = Self::open_append(&path)?;
        use std::io::Write;
        for (p, size) in records {
            writeln!(
                file,
                "{datetime}\t{}\t{size}",
                p.as_path().to_string_lossy()
            )
            .map_err(|e| CoreError::io(&path, e))?;
        }
        Ok(())
    }

    /// 1 行（日時プレフィックス付き）を追記する。
    fn write_line(dir: &Path, body: &str, now_secs: i64) -> Result<()> {
        let path = dir.join(DELETE_LOG_FILE);
        let datetime = crate::util::format_datetime_local(now_secs);
        let mut file = Self::open_append(&path)?;
        use std::io::Write;
        writeln!(file, "{datetime}\t{body}").map_err(|e| CoreError::io(&path, e))
    }

    fn open_append(path: &Path) -> Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| CoreError::io(path, e))
    }
}

// ---- 手動削除リスト（§3.6a） ----

/// 手動削除モードの削除リストと PowerShell コマンド生成（仕様 §3.6a）。
pub struct ExecListWriter;

impl ExecListWriter {
    /// 選択ファイルのパスを 1 行 1 パス（UTF-8）で書き出し、生成パスを返す。
    pub fn write_list(dir: &Path, paths: &[LongPath], stamp: &str) -> Result<PathBuf> {
        std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
        let path = dir.join(format!("exec_list_{stamp}.txt"));
        // パス毎の String 確保と中間 Vec を避け、Cow を直接 1 本の String へ連結する。
        let mut body = String::new();
        for (i, p) in paths.iter().enumerate() {
            if i > 0 {
                body.push('\n');
            }
            body.push_str(&p.as_path().to_string_lossy());
        }
        std::fs::write(&path, body).map_err(|e| CoreError::io(&path, e))?;
        Ok(path)
    }

    /// 削除リストを処理する PowerShell コマンドを生成する（独立プロセス用, §3.6a）。
    ///
    /// `dest` が指定された場合は移動コマンドを返す（削除モードより優先）。
    #[must_use]
    pub fn powershell_command(list: &Path, mode: DeleteMode, dest: Option<&str>) -> String {
        let list = list.to_string_lossy();
        if let Some(d) = dest {
            return format!(
                "Get-Content '{list}' | ForEach-Object {{ Move-Item -LiteralPath $_ -Destination '{d}' -Force }}"
            );
        }
        match mode {
            DeleteMode::RecycleBin => format!(
                "Add-Type -AssemblyName Microsoft.VisualBasic; Get-Content '{list}' | ForEach-Object {{ [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($_, 'OnlyErrorDialogs', 'SendToRecycleBin') }}"
            ),
            DeleteMode::Permanent => format!(
                "Get-Content '{list}' | ForEach-Object {{ Remove-Item -LiteralPath $_ -Force }}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;
    use crate::model::{DuplicateGroup, FileEntry, FileSize};
    use std::time::SystemTime;

    fn group(fs: &mut MockFs, paths: &[&str], in_use: &[&str]) -> DuplicateGroup {
        let files = paths
            .iter()
            .map(|p| {
                let lp = LongPath::new(*p).unwrap();
                fs.add_file(&lp, b"x".to_vec());
                if in_use.contains(p) {
                    fs.set_in_use(&lp, true);
                }
                FileEntry {
                    path: lp,
                    size: FileSize(1),
                    modified: SystemTime::UNIX_EPOCH,
                }
            })
            .collect();
        DuplicateGroup {
            representative: "blake3:x".into(),
            files,
        }
    }

    #[test]
    fn plan_keeps_one_when_all_selected() {
        let mut fs = MockFs::new();
        let g = group(&mut fs, &["C:\\a", "C:\\b"], &[]);
        let groups = vec![g];
        let sel = Selector::select_all(
            &groups,
            KeepFileRule::ByPathAsc,
            &crate::select::LockSet::new(),
        );
        let deleter = Deleter::new(&fs);
        let plan = deleter.plan(
            &groups,
            &sel,
            KeepFileRule::ByPathAsc,
            &LockSet::new(),
            false,
        );
        // 2 件中 1 件のみ削除対象、1 件は残る。
        assert_eq!(plan.to_delete.len(), 1);
    }

    #[test]
    fn plan_never_deletes_locked_file_even_if_selected() {
        // 多重防御: from_paths でロック済みパスが選択へ混入しても、plan が除外する。
        let mut fs = MockFs::new();
        let g = group(&mut fs, &["C:\\a", "C:\\b", "C:\\c"], &[]);
        let groups = vec![g];
        // ロック除外を意図的に行わず、全件を直接選択集合へ入れる。
        let sel = Selection::from_paths(groups[0].files.iter().map(|f| f.path.clone()));
        let mut locks = LockSet::new();
        let locked = LongPath::new("C:\\a").unwrap();
        locks.lock(&locked);

        let deleter = Deleter::new(&fs);
        let plan = deleter.plan(&groups, &sel, KeepFileRule::ByDateNewest, &locks, false);
        // ロック済みは削除対象に含まれず、Locked として skip される。
        assert!(!plan.to_delete.contains(&locked));
        assert!(plan
            .skipped
            .iter()
            .any(|(p, r)| p == &locked && *r == SkipReason::Locked));
    }

    #[test]
    fn permanent_delete_removes_empty_directory() {
        // §3.9.1: 空フォルダ（配下にファイルなし・空サブフォルダのみ）を完全削除できる。
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("empty_tree");
        std::fs::create_dir_all(target.join("inner")).unwrap();
        let lp = LongPath::new(&target).unwrap();
        let fs = crate::io::RealFs;
        let deleter = Deleter::new(&fs);
        let plan = deleter.plan_paths(&[lp], &LockSet::new(), false);
        let report = deleter.execute(&plan, DeleteMode::Permanent);
        assert_eq!(report.deleted.len(), 1);
        assert!(report.failed.is_empty());
        assert!(!target.exists(), "空フォルダが削除されていない");
    }

    #[test]
    fn plan_paths_deletes_all_without_keep_one() {
        // ユーティリティ削除: グループの「1つ残す」を適用せず選択全件を対象にする。
        let mut fs = MockFs::new();
        let paths: Vec<LongPath> = ["C:\\a", "C:\\b", "C:\\c"]
            .iter()
            .map(|p| {
                let lp = LongPath::new(*p).unwrap();
                fs.add_file(&lp, b"x".to_vec());
                lp
            })
            .collect();
        let deleter = Deleter::new(&fs);
        let plan = deleter.plan_paths(&paths, &LockSet::new(), false);
        assert_eq!(plan.to_delete.len(), 3, "全件が削除対象（keep-one なし）");
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn plan_paths_applies_safety_filters() {
        // plan と同じ安全フィルタ（ロック/使用中）を適用する。
        let mut fs = MockFs::new();
        let a = LongPath::new("C:\\a").unwrap();
        let b = LongPath::new("C:\\b").unwrap();
        let c = LongPath::new("C:\\c").unwrap();
        for p in [&a, &b, &c] {
            fs.add_file(p, b"x".to_vec());
        }
        fs.set_in_use(&b, true);
        let mut locks = LockSet::new();
        locks.lock(&c);

        let deleter = Deleter::new(&fs);
        let plan = deleter.plan_paths(&[a.clone(), b.clone(), c.clone()], &locks, false);
        assert_eq!(plan.to_delete, vec![a]);
        assert!(plan
            .skipped
            .iter()
            .any(|(p, r)| p == &b && *r == SkipReason::InUse));
        assert!(plan
            .skipped
            .iter()
            .any(|(p, r)| p == &c && *r == SkipReason::Locked));
    }

    #[test]
    fn plan_skips_in_use_files() {
        let mut fs = MockFs::new();
        let g = group(&mut fs, &["C:\\a", "C:\\b", "C:\\c"], &["C:\\a"]);
        let groups = vec![g];
        let sel = Selector::select_all(
            &groups,
            KeepFileRule::ByDateNewest,
            &crate::select::LockSet::new(),
        );
        let deleter = Deleter::new(&fs);
        let plan = deleter.plan(
            &groups,
            &sel,
            KeepFileRule::ByDateNewest,
            &LockSet::new(),
            false,
        );
        assert!(plan
            .skipped
            .iter()
            .any(|(p, r)| p == &LongPath::new("C:\\a").unwrap() && *r == SkipReason::InUse));
    }

    #[test]
    fn plan_skips_read_only_when_delete_read_only_false() {
        let mut fs = MockFs::new();
        let g = group(&mut fs, &["C:\\a", "C:\\b", "C:\\c"], &[]);
        let ro = LongPath::new("C:\\a").unwrap();
        fs.set_read_only(&ro, true);
        let groups = vec![g];
        let sel = Selection::from_paths(groups[0].files.iter().map(|f| f.path.clone()));
        let deleter = Deleter::new(&fs);
        // delete_read_only=false: 読み取り専用は ReadOnly として除外される。
        let plan = deleter.plan(
            &groups,
            &sel,
            KeepFileRule::ByDateNewest,
            &LockSet::new(),
            false,
        );
        assert!(!plan.to_delete.contains(&ro));
        assert!(plan
            .skipped
            .iter()
            .any(|(p, r)| p == &ro && *r == SkipReason::ReadOnly));
        // delete_read_only=true: 読み取り専用も削除対象になり得る。
        let plan = deleter.plan(
            &groups,
            &sel,
            KeepFileRule::ByDateNewest,
            &LockSet::new(),
            true,
        );
        assert!(plan.skipped.iter().all(|(_, r)| *r != SkipReason::ReadOnly));
    }

    #[test]
    fn plan_skips_reparse_point_even_if_selected() {
        // 多重防御: リパースポイントは選択されていても削除対象に入れない（実体巻き込み防止）。
        let mut fs = MockFs::new();
        let g = group(&mut fs, &["C:\\a", "C:\\b", "C:\\c"], &[]);
        let link = LongPath::new("C:\\a").unwrap();
        fs.add_reparse_point(&link); // C:\a をリパースポイント化
        let groups = vec![g];
        let sel = Selection::from_paths(groups[0].files.iter().map(|f| f.path.clone()));
        let deleter = Deleter::new(&fs);
        let plan = deleter.plan(
            &groups,
            &sel,
            KeepFileRule::ByDateNewest,
            &LockSet::new(),
            false,
        );
        assert!(!plan.to_delete.contains(&link));
        assert!(plan
            .skipped
            .iter()
            .any(|(p, r)| p == &link && *r == SkipReason::Reparse));
    }

    #[test]
    fn permanent_delete_removes_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("victim.txt");
        std::fs::write(&f, b"bye").unwrap();
        let lp = LongPath::new(&f).unwrap();
        let fs = crate::io::RealFs;
        let deleter = Deleter::new(&fs);
        let plan = DeletePlan {
            to_delete: vec![lp],
            skipped: vec![],
        };
        let report = deleter.execute(&plan, DeleteMode::Permanent);
        assert_eq!(report.deleted.len(), 1);
        assert!(!f.exists());
    }

    #[test]
    fn execute_logged_writes_delete_log_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("victim.txt");
        std::fs::write(&f, b"bye").unwrap();
        let fs = crate::io::RealFs;
        let deleter = Deleter::new(&fs);
        let plan = DeletePlan {
            to_delete: vec![LongPath::new(&f).unwrap()],
            skipped: vec![],
        };
        let report = deleter.execute_logged(
            &plan,
            FileOp::Delete(DeleteMode::Permanent),
            dir.path(),
            true,
            0,
        );
        assert_eq!(report.deleted.len(), 1);
        assert!(!f.exists());
        // save_log=true なので shredy_delete.log が生成され削除が記録される。
        let log = std::fs::read_to_string(dir.path().join(DELETE_LOG_FILE)).unwrap();
        assert!(log.contains("victim.txt"));
    }

    #[test]
    fn execute_logged_skips_delete_log_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("victim.txt");
        std::fs::write(&f, b"bye").unwrap();
        let fs = crate::io::RealFs;
        let deleter = Deleter::new(&fs);
        let plan = DeletePlan {
            to_delete: vec![LongPath::new(&f).unwrap()],
            skipped: vec![],
        };
        let report = deleter.execute_logged(
            &plan,
            FileOp::Delete(DeleteMode::Permanent),
            dir.path(),
            false,
            0,
        );
        assert_eq!(report.deleted.len(), 1);
        // save_log=false なのでログファイルは作られない。
        assert!(!dir.path().join(DELETE_LOG_FILE).exists());
    }

    #[test]
    fn move_auto_renames_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        std::fs::write(&src, b"1").unwrap();
        let dest = dir.path().join("dest");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("a.txt"), b"existing").unwrap();

        let fs = crate::io::RealFs;
        let deleter = Deleter::new(&fs);
        // roots 空 = 構造再現なし（dest 直下へ移動）。
        let report = deleter.move_to(&[LongPath::new(&src).unwrap()], &dest, &[]);
        assert_eq!(report.deleted.len(), 1);
        assert!(dest.join("a (1).txt").exists());
        assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"existing");
    }

    #[test]
    fn move_reproduces_folder_structure_under_root() {
        // 登録 root 配下の相対構造を dest 配下へ再現する（仕様 §3.10）。
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("src");
        let nested = root.join("2024").join("DAY1");
        std::fs::create_dir_all(&nested).unwrap();
        let src = nested.join("a.txt");
        std::fs::write(&src, b"1").unwrap();
        let dest = tmp.path().join("out");

        let fs = crate::io::RealFs;
        let deleter = Deleter::new(&fs);
        let roots = [LongPath::new(&root).unwrap()];
        let report = deleter.move_to(&[LongPath::new(&src).unwrap()], &dest, &roots);
        assert_eq!(report.deleted.len(), 1);
        // dest/2024/DAY1/a.txt に再現される。
        assert!(dest.join("2024").join("DAY1").join("a.txt").exists());
        assert!(!src.exists());
    }

    #[test]
    fn relative_parent_picks_longest_matching_root() {
        let path = LongPath::new("D:\\Photos\\2024\\a.jpg").unwrap();
        let roots = [
            LongPath::new("D:\\Photos").unwrap(),
            LongPath::new("D:\\Photos\\2024").unwrap(),
        ];
        // 最長一致 = D:\Photos\2024 → 相対親は空 → None（dest 直下）。
        assert_eq!(relative_parent_under_roots(&path, &roots), None);
        // root が D:\Photos のみなら相対親 = 2024。
        assert_eq!(
            relative_parent_under_roots(&path, &roots[..1]),
            Some(PathBuf::from("2024"))
        );
        // どの root にも属さない → None。
        assert_eq!(
            relative_parent_under_roots(&path, &[LongPath::new("E:\\Other").unwrap()]),
            None
        );
    }

    #[test]
    fn date_stamp_epoch_is_19700101() {
        assert_eq!(DeleteLog::date_stamp(0), "19700101");
    }

    #[test]
    fn delete_log_appends_records_to_fixed_file() {
        let dir = tempfile::tempdir().unwrap();
        let recs = vec![(LongPath::new("C:\\x").unwrap(), 42u64)];
        DeleteLog::append(dir.path(), &recs, 0).unwrap();
        let content = std::fs::read_to_string(dir.path().join(DELETE_LOG_FILE)).unwrap();
        assert!(content.contains("C:\\x"));
        assert!(content.contains("42"));
    }

    #[test]
    fn delete_log_init_writes_start_line_and_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        DeleteLog::init(dir.path(), "shredy-test 0.0.0", 0).unwrap();
        let content = std::fs::read_to_string(dir.path().join(DELETE_LOG_FILE)).unwrap();
        assert!(content.contains("[START]"));
        assert!(content.contains("shredy-test 0.0.0"));
    }

    #[test]
    fn powershell_commands_match_spec() {
        let list = Path::new("C:\\tmp\\exec.txt");
        let recycle = ExecListWriter::powershell_command(list, DeleteMode::RecycleBin, None);
        assert!(recycle.contains("SendToRecycleBin"));
        let perm = ExecListWriter::powershell_command(list, DeleteMode::Permanent, None);
        assert!(perm.contains("Remove-Item -LiteralPath"));
        let mv = ExecListWriter::powershell_command(list, DeleteMode::Permanent, Some("D:\\move"));
        assert!(mv.contains("Move-Item") && mv.contains("D:\\move"));
    }

    #[test]
    fn exec_list_written_one_path_per_line() {
        let dir = tempfile::tempdir().unwrap();
        let paths = vec![
            LongPath::new("C:\\a").unwrap(),
            LongPath::new("C:\\b").unwrap(),
        ];
        let p = ExecListWriter::write_list(dir.path(), &paths, "20260606_120000").unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        assert_eq!(content.lines().count(), 2);
    }
}
