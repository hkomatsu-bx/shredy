//! ユーティリティスキャン（仕様 §3.9）。
//!
//! 重複検索エンジンとは独立した補助スキャン群。各機能は対象フォルダを走査し、検出した
//! ファイル/フォルダを**グループ化しないフラット一覧**（[`UtilityItem`]）で返す。
//!
//! メタデータ系 4 機能（B-M1, 内容を読まずに判定）: 空ファイル・空フォルダ（§3.9.1）/
//! 一時ファイル（§3.9.2）/ 大容量ファイル（§3.9.3）/ 無効シンボリックリンク（§3.9.6）。
//! コンテンツ系 2 機能（B-M2, 先頭バイト/ZIP 構造を検証）: 破損ファイル（§3.9.4）/
//! 拡張子不正（§3.9.5）。

mod magic;
mod temp;

use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

use crate::config::{AppConfig, CorruptScanGroups, WrongExtensionGroups};
use crate::path::{to_fs_path, LongPath};
use crate::progress::{is_cancelled, Cancel, Phase, ProgressEvent, ProgressTx};
use crate::scan::{has_system_component, is_system_name, normalize_roots, MAX_DEPTH};

use magic::{
    corrupt_group_for_ext, detect_signature, signature_matches_ext, wrong_ext_group_for_ext,
};
pub use temp::{classify_temp, TempCategory};

/// 走査時に読み込むファイル先頭バイト数（マジックバイト判定用）。
const HEADER_READ_LEN: u64 = magic::HEADER_LEN as u64;

/// 走査中の進捗イベント送出間隔（件数）。`scan.rs` と同じ間引き方針。
const PROGRESS_REPORT_INTERVAL: u64 = 256;

/// ユーティリティスキャンの種別（仕様 §3.9）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UtilityKind {
    /// §3.9.1 空ファイル・空フォルダ。
    Empty,
    /// §3.9.2 一時ファイル。
    Temp,
    /// §3.9.3 大容量ファイル。
    LargeFile,
    /// §3.9.4 破損ファイル。
    Corrupt,
    /// §3.9.5 拡張子不正ファイル。
    WrongExtension,
    /// §3.9.6 無効シンボリックリンク。
    InvalidSymlink,
}

impl UtilityKind {
    /// 表示名（GUI メニュー名・ログ用。仕様 §3.9 の各機能名）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            UtilityKind::Empty => "空ファイル・フォルダ",
            UtilityKind::Temp => "一時ファイル",
            UtilityKind::LargeFile => "大容量ファイル",
            UtilityKind::Corrupt => "破損ファイル",
            UtilityKind::WrongExtension => "拡張子不正",
            UtilityKind::InvalidSymlink => "無効シンボリックリンク",
        }
    }
}

/// ユーティリティスキャンの検出結果 1 件（フラット一覧の 1 行）。
///
/// `detail` は種別固有の表示情報（一時=カテゴリ名・無効リンク=リンク先など）。情報が
/// なければ空文字。`is_dir` は §3.9.1 のファイル/フォルダ区別アイコン用。
#[derive(Debug, Clone)]
pub struct UtilityItem {
    pub path: LongPath,
    /// ファイルサイズ（バイト）。フォルダ・リンクは 0。
    pub size: u64,
    pub modified: SystemTime,
    pub is_dir: bool,
    pub detail: String,
}

/// ユーティリティスキャンのパラメータ（仕様 §5 の関連キー）。
#[derive(Debug, Clone)]
pub struct UtilityConfig {
    /// 大容量判定の下限（バイト, §3.9.3）。
    pub large_file_threshold: u64,
    /// 一時ファイルの追加パターン（§3.9.2）。
    pub additional_temp_patterns: Vec<String>,
    /// リパースポイントを辿るか（既定 false=安全優先）。
    pub follow_symlinks: bool,
    /// システムフォルダを除外するか。
    pub exclude_system_folders: bool,
    /// 破損検出の対象グループ（§3.9.4）。
    pub corrupt_scan_groups: CorruptScanGroups,
    /// 拡張子不正検出の対象グループ（§3.9.5）。
    pub wrong_extension_groups: WrongExtensionGroups,
}

impl UtilityConfig {
    /// アプリ設定から走査パラメータを組み立てる。
    #[must_use]
    pub fn from_app_config(cfg: &AppConfig) -> Self {
        Self {
            large_file_threshold: cfg.large_file_threshold_bytes,
            additional_temp_patterns: cfg.additional_temp_patterns.clone(),
            follow_symlinks: cfg.follow_symlinks,
            exclude_system_folders: cfg.exclude_system_folders,
            corrupt_scan_groups: cfg.corrupt_scan_groups,
            wrong_extension_groups: cfg.wrong_extension_groups,
        }
    }
}

/// ユーティリティスキャナ。
pub struct UtilityScanner {
    cfg: UtilityConfig,
}

impl UtilityScanner {
    #[must_use]
    pub fn new(cfg: UtilityConfig) -> Self {
        Self { cfg }
    }

    /// 指定種別で対象フォルダ群を走査し、検出結果のフラット一覧を返す。
    ///
    /// 中断要求時はその時点までの結果を返す（仕様: キャンセル即応）。登録フォルダの
    /// 親子重複は [`normalize_roots`] で正規化する。
    #[must_use]
    pub fn scan(
        &self,
        kind: UtilityKind,
        roots: &[LongPath],
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<UtilityItem> {
        let _ = progress.send(ProgressEvent::Phase(Phase::Loading));
        let roots = normalize_roots(roots);
        let label = kind.label();
        tracing::info!(
            roots = roots.len(),
            "ユーティリティスキャン - {label} - 開始"
        );
        let started = std::time::Instant::now();
        let mut items = match kind {
            UtilityKind::Empty => self.scan_empty(&roots, progress, cancel),
            UtilityKind::Temp => self.scan_temp(&roots, progress, cancel),
            UtilityKind::LargeFile => self.scan_large(&roots, progress, cancel),
            UtilityKind::Corrupt => self.scan_corrupt(&roots, progress, cancel),
            UtilityKind::WrongExtension => self.scan_wrong_extension(&roots, progress, cancel),
            UtilityKind::InvalidSymlink => self.scan_invalid_symlinks(&roots, progress, cancel),
        };
        // 大容量はサイズ降順（仕様 §3.9.3）。
        if matches!(kind, UtilityKind::LargeFile) {
            items.sort_by_key(|i| std::cmp::Reverse(i.size));
        }
        tracing::info!(
            found = items.len(),
            ms = started.elapsed().as_millis() as u64,
            "ユーティリティスキャン - {label} - 完了"
        );
        items
    }

    /// ルートがシステムフォルダ配下（または同名）なら走査対象外か。
    fn skip_root(&self, root: &LongPath) -> bool {
        self.cfg.exclude_system_folders && has_system_component(root.as_path())
    }

    /// 深度上限・システムフォルダ枝刈り付きの走査イテレータを構築する。
    ///
    /// `follow_links` は呼び出し側で指定する（無効リンク検出は常に false、それ以外は
    /// 設定値）。`filter_entry` でシステム名フォルダを降下ごと枝刈りする。
    ///
    /// 返り値は具体型（`impl Iterator` ではない）。一時ファイル走査が一時フォルダ検出時に
    /// `skip_current_dir()` を呼ぶため、`FilterEntry` を露出する必要がある。
    fn build_walk(
        &self,
        root: &LongPath,
        follow_links: bool,
    ) -> walkdir::FilterEntry<walkdir::IntoIter, impl FnMut(&walkdir::DirEntry) -> bool> {
        let exclude_system = self.cfg.exclude_system_folders;
        walkdir::WalkDir::new(root.as_path())
            .max_depth(MAX_DEPTH)
            .follow_links(follow_links)
            .into_iter()
            .filter_entry(move |e| !exclude_system || !is_system_name(e.file_name()))
    }

    // ── §3.9.1 空ファイル・空フォルダ ───────────────────────────────

    fn scan_empty(
        &self,
        roots: &[LongPath],
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<UtilityItem> {
        let mut items = Vec::new();
        for root in roots {
            if is_cancelled(cancel) {
                break;
            }
            if self.skip_root(root) {
                continue;
            }
            let fileless = self.empty_recurse(root.as_path(), 0, &mut items, progress, cancel);
            // ルート自身が（再帰的に）空 → ルートを空フォルダとして報告する。
            if fileless {
                if let Some(item) = dir_item(root.as_path()) {
                    items.push(item);
                }
            }
        }
        items
    }

    /// `dir` を再帰走査し、空ファイルと「境界の」空フォルダを `items` へ追加する。
    ///
    /// 戻り値はこのディレクトリの**サブツリーに通常ファイルが 1 つも無い**か
    /// （= ディレクトリツリーが空か）。`true` のとき呼び出し側が削除境界（親）で
    /// このフォルダを報告する。読めないディレクトリ・リンクを含む場合は実体巻き込みを
    /// 避けるため「空ではない」(`false`) を返す（保守的）。
    fn empty_recurse(
        &self,
        dir: &Path,
        depth: usize,
        items: &mut Vec<UtilityItem>,
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> bool {
        if depth >= MAX_DEPTH || is_cancelled(cancel) {
            return false;
        }
        let rd = match fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => {
                let _ = progress.send(ProgressEvent::Skipped(dir.to_path_buf()));
                return false;
            }
        };
        let mut subtree_fileless = true;
        // 子の空フォルダ候補（このディレクトリが境界になったときのみ報告する）。
        let mut empty_children: Vec<UtilityItem> = Vec::new();
        for ent in rd {
            if is_cancelled(cancel) {
                return false;
            }
            let ent = match ent {
                Ok(e) => e,
                Err(_) => {
                    subtree_fileless = false;
                    continue;
                }
            };
            let ft = match ent.file_type() {
                Ok(t) => t,
                Err(_) => {
                    subtree_fileless = false;
                    continue;
                }
            };
            let path = ent.path();
            if ft.is_symlink() {
                // リンクを含むフォルダは空とみなさない（リンク実体の巻き込みを避ける）。
                subtree_fileless = false;
            } else if ft.is_dir() {
                if self.cfg.exclude_system_folders && is_system_name(&ent.file_name()) {
                    // システムフォルダは降下せず、中身不明のため空扱いにしない。
                    subtree_fileless = false;
                    continue;
                }
                let child_fileless = self.empty_recurse(&path, depth + 1, items, progress, cancel);
                if child_fileless {
                    if let Some(item) = dir_item(&path) {
                        empty_children.push(item);
                    }
                } else {
                    subtree_fileless = false;
                }
            } else if ft.is_file() {
                // ファイルが存在する時点でこのサブツリーは空ではない。
                subtree_fileless = false;
                match ent.metadata() {
                    Ok(m) if m.len() == 0 => {
                        if let Some(item) = file_item(&path, 0) {
                            items.push(item);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        // サイズ不明はゼロバイト判定を避け、Skipped で通知する（仕様 §6）。
                        let _ = progress.send(ProgressEvent::Skipped(path.clone()));
                    }
                }
            }
        }
        // このディレクトリにファイル/リンク等があり「境界」になる場合のみ、空の子フォルダを
        // 報告する（自身が空なら親が境界になり、子は親の削除に含まれるため報告しない）。
        if !subtree_fileless {
            items.append(&mut empty_children);
        }
        subtree_fileless
    }

    // ── §3.9.2 一時ファイル ─────────────────────────────────────────

    fn scan_temp(
        &self,
        roots: &[LongPath],
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<UtilityItem> {
        let mut items = Vec::new();
        let mut count = 0u64;
        for root in roots {
            if self.skip_root(root) {
                continue;
            }
            let mut it = self.build_walk(root, self.cfg.follow_symlinks);
            while let Some(res) = it.next() {
                if is_cancelled(cancel) {
                    return items;
                }
                let ent = match res {
                    Ok(e) => e,
                    Err(err) => {
                        if let Some(p) = err.path() {
                            let _ = progress.send(ProgressEvent::Skipped(p.to_path_buf()));
                        }
                        continue;
                    }
                };
                let name = ent.file_name().to_string_lossy();
                let Some(cat) = classify_temp(&name, &self.cfg.additional_temp_patterns) else {
                    continue;
                };
                let is_dir = ent.file_type().is_dir();
                let path = ent.path();
                let item = if is_dir {
                    dir_item_labeled(path, cat.label())
                } else {
                    let size = ent.metadata().map(|m| m.len()).unwrap_or(0);
                    file_item_labeled(path, size, cat.label())
                };
                if let Some(item) = item {
                    items.push(item);
                    count += 1;
                    report_progress(progress, count);
                }
                // 一時フォルダ（node_modules 等）はそれ自体を 1 件として報告し、配下は
                // 走査しない（個々の中身を一覧に展開しない）。
                if is_dir {
                    it.skip_current_dir();
                }
            }
        }
        items
    }

    // ── §3.9.3 大容量ファイル ───────────────────────────────────────

    fn scan_large(
        &self,
        roots: &[LongPath],
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<UtilityItem> {
        let mut items = Vec::new();
        let mut count = 0u64;
        for root in roots {
            if self.skip_root(root) {
                continue;
            }
            let iter = self.build_walk(root, self.cfg.follow_symlinks);
            for res in iter {
                if is_cancelled(cancel) {
                    return items;
                }
                let ent = match res {
                    Ok(e) => e,
                    Err(err) => {
                        if let Some(p) = err.path() {
                            let _ = progress.send(ProgressEvent::Skipped(p.to_path_buf()));
                        }
                        continue;
                    }
                };
                if !ent.file_type().is_file() {
                    continue;
                }
                let size = match ent.metadata() {
                    Ok(m) => m.len(),
                    Err(_) => {
                        let _ = progress.send(ProgressEvent::Skipped(ent.path().to_path_buf()));
                        continue;
                    }
                };
                if size >= self.cfg.large_file_threshold {
                    if let Some(item) = file_item(ent.path(), size) {
                        items.push(item);
                        count += 1;
                        report_progress(progress, count);
                    }
                }
            }
        }
        items
    }

    // ── §3.9.6 無効シンボリックリンク ──────────────────────────────

    fn scan_invalid_symlinks(
        &self,
        roots: &[LongPath],
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<UtilityItem> {
        let mut items = Vec::new();
        let mut count = 0u64;
        for root in roots {
            if self.skip_root(root) {
                continue;
            }
            // リンク自体を検出するため follow_links は常に false にする。
            let iter = self.build_walk(root, false);
            for res in iter {
                if is_cancelled(cancel) {
                    return items;
                }
                let ent = match res {
                    Ok(e) => e,
                    Err(err) => {
                        if let Some(p) = err.path() {
                            let _ = progress.send(ProgressEvent::Skipped(p.to_path_buf()));
                        }
                        continue;
                    }
                };
                if !ent.path_is_symlink() {
                    continue;
                }
                let path = ent.path();
                // metadata() はリンクを辿る。`NotFound` のみがリンク先不在＝無効リンク。
                // アクセス拒否・タイムアウト等はリンク先が存在する可能性があり、無効と誤判定
                // すると誤削除につながるため Skipped 通知にとどめる（仕様 §6・安全優先）。
                match fs::metadata(path) {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        let target = fs::read_link(path)
                            .map(|t| t.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        if let Some(item) = link_item(path, target) {
                            items.push(item);
                            count += 1;
                            report_progress(progress, count);
                        }
                    }
                    Err(_) => {
                        let _ = progress.send(ProgressEvent::Skipped(path.to_path_buf()));
                    }
                }
            }
        }
        items
    }

    // ── §3.9.5 拡張子不正ファイル ──────────────────────────────────

    fn scan_wrong_extension(
        &self,
        roots: &[LongPath],
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<UtilityItem> {
        let mut items = Vec::new();
        let mut count = 0u64;
        for root in roots {
            if self.skip_root(root) {
                continue;
            }
            for res in self.build_walk(root, self.cfg.follow_symlinks) {
                if is_cancelled(cancel) {
                    return items;
                }
                let Some(ent) = accept_file(res, progress) else {
                    continue;
                };
                let path = ent.path();
                let ext = file_extension(path);
                // 対象グループ外の拡張子は I/O せずスキップ（性能）。
                match wrong_ext_group_for_ext(&ext) {
                    Some(g) if self.cfg.wrong_extension_groups.contains(g) => {}
                    _ => continue,
                }
                let header = match read_header(path) {
                    Ok(h) => h,
                    Err(_) => {
                        let _ = progress.send(ProgressEvent::Skipped(path.to_path_buf()));
                        continue;
                    }
                };
                // 実体を特定でき、かつ拡張子と不一致のときだけ報告する（None＝判定不能は
                // 報告しない＝誤検知回避）。
                let Some(sig) = detect_signature(&header) else {
                    continue;
                };
                if !signature_matches_ext(sig, &ext) {
                    let size = ent.metadata().map(|m| m.len()).unwrap_or(0);
                    let detail = format!("実体 {} / 拡張子 .{}", sig.label(), ext);
                    if let Some(item) = file_item_labeled(path, size, &detail) {
                        items.push(item);
                        count += 1;
                        report_progress(progress, count);
                    }
                }
            }
        }
        items
    }

    // ── §3.9.4 破損ファイル ─────────────────────────────────────────

    fn scan_corrupt(
        &self,
        roots: &[LongPath],
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<UtilityItem> {
        let mut items = Vec::new();
        let mut count = 0u64;
        for root in roots {
            if self.skip_root(root) {
                continue;
            }
            for res in self.build_walk(root, self.cfg.follow_symlinks) {
                if is_cancelled(cancel) {
                    return items;
                }
                let Some(ent) = accept_file(res, progress) else {
                    continue;
                };
                let path = ent.path();
                let ext = file_extension(path);
                // 対象グループ外の拡張子は検証しない。
                let group = match corrupt_group_for_ext(&ext) {
                    Some(g) if self.cfg.corrupt_scan_groups.contains(g) => g,
                    _ => continue,
                };
                // 検証で「破損」を確定した場合のみ報告する。読み取り不能（ロック・権限等）は
                // 破損と断定せず Skipped にとどめる（仕様 §6・誤検知回避）。
                match validate_not_corrupt(path, group) {
                    Ok(true) => {}
                    Ok(false) => {
                        let size = ent.metadata().map(|m| m.len()).unwrap_or(0);
                        if let Some(item) = file_item_labeled(path, size, "破損の可能性") {
                            items.push(item);
                            count += 1;
                            report_progress(progress, count);
                        }
                    }
                    Err(_) => {
                        let _ = progress.send(ProgressEvent::Skipped(path.to_path_buf()));
                    }
                }
            }
        }
        items
    }
}

/// 走査結果を「読み取り可能な通常ファイル」に絞る。ディレクトリ等は `None`、
/// 走査エラーは `Skipped` 通知のうえ `None` を返す。
fn accept_file(
    res: walkdir::Result<walkdir::DirEntry>,
    progress: &ProgressTx,
) -> Option<walkdir::DirEntry> {
    match res {
        Ok(e) if e.file_type().is_file() => Some(e),
        Ok(_) => None,
        Err(err) => {
            if let Some(p) = err.path() {
                let _ = progress.send(ProgressEvent::Skipped(p.to_path_buf()));
            }
            None
        }
    }
}

/// ファイルの拡張子（小文字・先頭ドットなし）。拡張子なしは空文字。
fn file_extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// 先頭 [`HEADER_READ_LEN`] バイトを読み込む（ロングパス対応）。
fn read_header(path: &Path) -> std::io::Result<Vec<u8>> {
    let fs_path = to_fs_path(path);
    let file = fs::File::open(fs_path)?;
    let mut buf = Vec::with_capacity(HEADER_READ_LEN as usize);
    file.take(HEADER_READ_LEN).read_to_end(&mut buf)?;
    Ok(buf)
}

/// 破損検出グループごとの妥当性検証。
///
/// `Ok(true)`=妥当 / `Ok(false)`=破損確定 / `Err`=読み取り不能（破損と断定しない）。
fn validate_not_corrupt(path: &Path, group: CorruptScanGroups) -> std::io::Result<bool> {
    if group == CorruptScanGroups::ZIP_LIKE {
        // ZIP/OOXML は中央ディレクトリを読めるか（ZipArchive 構造検証, 仕様 §3.9.4）。
        return validate_zip(path);
    }
    let header = read_header(path)?;
    let valid = match group {
        CorruptScanGroups::JPEG => magic::is_valid_jpeg_header(&header),
        CorruptScanGroups::PNG => magic::is_valid_png_header(&header),
        // HEIF/HEIC は ftyp ボックスの有無のみ検証（brand までは見ない, 仕様 §3.9.4）。
        CorruptScanGroups::HEIF => magic::has_ftyp_box(&header),
        // ZIP_LIKE は冒頭の guard で処理済み。bitflags は網羅 match できないため、
        // 未対応グループ（新グループ追加時の実装漏れ）は panic させず Err に倒し、
        // 呼び出し側で「破損と断定しない（Skipped）」へ安全側に流す（§3 panic 回避）。
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("未対応の破損検出グループ: {other:?}"),
            ));
        }
    };
    Ok(valid)
}

/// ZIP/OOXML を開いて中央ディレクトリを読めるか検証する。
///
/// `Ok(true)`=構造妥当 / `Ok(false)`=構造破損 / `Err`=開けない（呼び出し側で Skipped）。
/// オープン失敗を `Err` で伝え、read_header 系と挙動を揃える（ロック・権限を破損と断定しない）。
fn validate_zip(path: &Path) -> std::io::Result<bool> {
    let fs_path = to_fs_path(path);
    let file = fs::File::open(fs_path)?;
    match zip::ZipArchive::new(std::io::BufReader::new(file)) {
        Ok(_) => Ok(true),
        // I/O エラー（ディスク不良・読み取りタイムアウト等）は「読み取り不能」として
        // 伝播し、呼び出し側で Skipped にする（破損と断定しない・誤検知回避）。
        Err(zip::result::ZipError::Io(e)) => Err(e),
        // 構造エラー（InvalidArchive / UnsupportedArchive 等）は破損確定。
        Err(_) => Ok(false),
    }
}

/// 走査進捗を間引いて送出する。
fn report_progress(progress: &ProgressTx, count: u64) {
    if count.is_multiple_of(PROGRESS_REPORT_INTERVAL) {
        let _ = progress.send(ProgressEvent::Progress {
            phase: Phase::Loading,
            done: count,
            total: 0,
        });
    }
}

/// 通常ファイルの結果項目を作る（パス変換失敗時は `None`）。
fn file_item(path: &Path, size: u64) -> Option<UtilityItem> {
    file_item_labeled(path, size, "")
}

fn file_item_labeled(path: &Path, size: u64, detail: &str) -> Option<UtilityItem> {
    Some(UtilityItem {
        path: LongPath::new(path).ok()?,
        size,
        modified: modified_of(path),
        is_dir: false,
        detail: detail.to_string(),
    })
}

/// フォルダの結果項目を作る。
fn dir_item(path: &Path) -> Option<UtilityItem> {
    dir_item_labeled(path, "")
}

fn dir_item_labeled(path: &Path, detail: &str) -> Option<UtilityItem> {
    Some(UtilityItem {
        path: LongPath::new(path).ok()?,
        size: 0,
        modified: modified_of(path),
        is_dir: true,
        detail: detail.to_string(),
    })
}

/// 無効シンボリックリンクの結果項目を作る（detail=リンク先）。
fn link_item(path: &Path, target: String) -> Option<UtilityItem> {
    Some(UtilityItem {
        path: LongPath::new(path).ok()?,
        size: 0,
        // リンク自身のメタは取得できる（modified_of は symlink_metadata を使う）。
        modified: modified_of(path),
        is_dir: false,
        detail: target,
    })
}

/// パスの更新日時を取得する（取得失敗は UNIX_EPOCH）。リンクは辿らず自身のメタを使う。
fn modified_of(path: &Path) -> SystemTime {
    fs::symlink_metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::new_cancel;
    use std::sync::mpsc::channel;

    fn cfg() -> UtilityConfig {
        UtilityConfig {
            large_file_threshold: 1024,
            additional_temp_patterns: Vec::new(),
            follow_symlinks: false,
            exclude_system_folders: true,
            corrupt_scan_groups: CorruptScanGroups::all(),
            wrong_extension_groups: WrongExtensionGroups::all(),
        }
    }

    fn names(items: &[UtilityItem]) -> Vec<String> {
        let mut v: Vec<String> = items
            .iter()
            .map(|i| {
                i.path
                    .as_path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        v.sort();
        v
    }

    fn run(kind: UtilityKind, cfg: UtilityConfig, root: &Path) -> Vec<UtilityItem> {
        let scanner = UtilityScanner::new(cfg);
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        scanner.scan(kind, &[LongPath::new(root).unwrap()], &tx, &cancel)
    }

    #[test]
    fn empty_detects_zero_byte_files() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("zero.bin"), b"").unwrap();
        std::fs::write(base.join("nonzero.txt"), b"data").unwrap();

        let items = run(UtilityKind::Empty, cfg(), base);
        assert_eq!(names(&items), vec!["zero.bin".to_string()]);
        assert!(!items[0].is_dir);
    }

    #[test]
    fn empty_detects_recursively_empty_folder_at_boundary() {
        // base/has_file/(empty_branch/inner) : has_file が境界。empty_branch を 1 件報告。
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let has_file = base.join("has_file");
        std::fs::create_dir(&has_file).unwrap();
        std::fs::write(has_file.join("a.txt"), b"x").unwrap();
        let branch = has_file.join("empty_branch");
        std::fs::create_dir(&branch).unwrap();
        std::fs::create_dir(branch.join("inner")).unwrap();

        let items = run(UtilityKind::Empty, cfg(), base);
        // 報告は境界の最上位 empty_branch のみ（inner は含めない）。
        let dirs: Vec<&UtilityItem> = items.iter().filter(|i| i.is_dir).collect();
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].path.as_path().ends_with("empty_branch"));
    }

    #[test]
    fn empty_reports_root_when_entirely_empty() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::create_dir(base.join("sub")).unwrap();

        let items = run(UtilityKind::Empty, cfg(), base);
        // ルート全体が空 → ルートのみ 1 件報告（sub は含めない）。
        assert_eq!(items.len(), 1);
        assert!(items[0].is_dir);
        assert_eq!(items[0].path.as_path(), base);
    }

    #[test]
    fn temp_detects_by_category_and_prunes_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.tmp"), b"x").unwrap();
        std::fs::write(base.join("keep.txt"), b"x").unwrap();
        let nm = base.join("node_modules");
        std::fs::create_dir(&nm).unwrap();
        std::fs::write(nm.join("pkg.json"), b"{}").unwrap();

        let items = run(UtilityKind::Temp, cfg(), base);
        // a.tmp と node_modules（フォルダ）のみ。node_modules 配下 pkg.json は枝刈り。
        assert_eq!(
            names(&items),
            vec!["a.tmp".to_string(), "node_modules".to_string()]
        );
        let nm_item = items.iter().find(|i| i.is_dir).unwrap();
        assert_eq!(nm_item.detail, TempCategory::BuildArtifact.label());
    }

    #[test]
    fn large_detects_over_threshold_sorted_desc() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("small.bin"), vec![0u8; 100]).unwrap();
        std::fs::write(base.join("big.bin"), vec![0u8; 2048]).unwrap();
        std::fs::write(base.join("bigger.bin"), vec![0u8; 4096]).unwrap();

        let items = run(UtilityKind::LargeFile, cfg(), base);
        let got = names(&items);
        assert_eq!(got, vec!["big.bin".to_string(), "bigger.bin".to_string()]);
        // サイズ降順。
        assert!(items[0].size >= items[1].size);
        assert_eq!(items[0].size, 4096);
    }

    #[test]
    fn scan_respects_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("big.bin"), vec![0u8; 4096]).unwrap();
        let scanner = UtilityScanner::new(cfg());
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        crate::progress::request_cancel(&cancel);
        let items = scanner.scan(
            UtilityKind::LargeFile,
            &[LongPath::new(base).unwrap()],
            &tx,
            &cancel,
        );
        assert!(items.is_empty());
    }

    const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0];
    const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    #[test]
    fn wrong_extension_detects_mismatch_only() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        // .png 拡張子だが中身は JPEG → 報告。
        std::fs::write(base.join("fake.png"), JPEG_MAGIC).unwrap();
        // 正しい PNG → 報告しない。
        std::fs::write(base.join("real.png"), PNG_MAGIC).unwrap();
        // 対象グループ外の拡張子（.txt）は中身が JPEG でも無視。
        std::fs::write(base.join("note.txt"), JPEG_MAGIC).unwrap();
        // 実体を識別できない（None）→ 誤検知回避で報告しない。
        std::fs::write(base.join("unknown.png"), b"unknownbytes").unwrap();

        let items = run(UtilityKind::WrongExtension, cfg(), base);
        assert_eq!(names(&items), vec!["fake.png".to_string()]);
        assert!(items[0].detail.contains("JPEG"));
        assert!(items[0].detail.contains(".png"));
    }

    #[test]
    fn wrong_extension_respects_disabled_group() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("fake.png"), JPEG_MAGIC).unwrap();
        let mut c = cfg();
        c.wrong_extension_groups = WrongExtensionGroups::empty();
        let items = run(UtilityKind::WrongExtension, c, base);
        assert!(items.is_empty(), "IMAGE グループ無効時は報告しない");
    }

    #[test]
    fn corrupt_detects_bad_magic_and_skips_valid() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("good.jpg"), JPEG_MAGIC).unwrap();
        std::fs::write(base.join("bad.jpg"), b"not a jpeg at all").unwrap();
        std::fs::write(base.join("good.png"), PNG_MAGIC).unwrap();
        std::fs::write(base.join("bad.png"), b"xxxx").unwrap();

        let items = run(UtilityKind::Corrupt, cfg(), base);
        assert_eq!(
            names(&items),
            vec!["bad.jpg".to_string(), "bad.png".to_string()]
        );
    }

    #[test]
    fn corrupt_detects_broken_zip_via_structure() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        // 妥当な ZIP（.docx 拡張子）。
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            zw.start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zw.write_all(b"<doc/>").unwrap();
            zw.finish().unwrap();
        }
        std::fs::write(base.join("ok.docx"), &buf).unwrap();
        // PK で始まるが中央ディレクトリが無い → 構造破損。
        std::fs::write(base.join("broken.docx"), b"PK\x03\x04garbage-no-eocd").unwrap();

        let items = run(UtilityKind::Corrupt, cfg(), base);
        assert_eq!(names(&items), vec!["broken.docx".to_string()]);
    }

    #[test]
    fn corrupt_respects_disabled_group() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("bad.jpg"), b"not a jpeg").unwrap();
        let mut c = cfg();
        c.corrupt_scan_groups = CorruptScanGroups::PNG; // JPEG を無効化
        let items = run(UtilityKind::Corrupt, c, base);
        assert!(items.is_empty(), "JPEG グループ無効時は検証しない");
    }
}
