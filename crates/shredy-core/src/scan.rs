//! ファイル走査（仕様 §3.1/§3.2, §6, ガイド §13.2）。
//!
//! 対象フォルダを `walkdir` で再帰列挙し、親子重複の正規化・各種フィルタ・深度上限・
//! NAS 判定・キャンセル・エラースキップを行う。フィルタ判定は純粋関数として切り出し、
//! 単体テスト可能にする。

use std::path::{Component, Path};
use std::time::SystemTime;

use walkdir::WalkDir;

use crate::config::ScanConfig;
use crate::model::{FileEntry, FileSize};
use crate::path::{LongPath, comp_keys};
use crate::progress::{Cancel, Phase, ProgressEvent, ProgressTx, is_cancelled};

/// 再帰深度の上限（NFR-5: シンボリックリンクループ防止）。
pub const MAX_DEPTH: usize = 64;

/// 走査中の進捗イベント送出間隔（ファイル件数）。頻繁すぎるチャネル送信で UI を
/// 飽和させないための間引き値。
const PROGRESS_REPORT_INTERVAL: u64 = 256;

const SYSTEM_FOLDER_NAMES: &[&str] = &[
    "windows",
    "system32",
    "system",
    "program files",
    "program files (x86)",
];

/// `walk` の走査中の集計状態（収集ファイル・件数・スキップ数）。
#[derive(Default)]
struct WalkAcc {
    out: Vec<FileEntry>,
    count: u64,
    skipped: u64,
}

/// 登録フォルダの親子重複を正規化する（仕様 §3.1）。
///
/// あるルートが別ルートの配下（または同一）なら配下側を除外し、二重列挙を防ぐ。
/// パス比較は大文字小文字を無視する。冪等。
#[must_use]
pub fn normalize_roots(roots: &[LongPath]) -> Vec<LongPath> {
    let mut kept: Vec<LongPath> = Vec::new();
    for r in roots {
        if kept.iter().any(|k| is_ancestor_or_same(k, r)) {
            continue;
        }
        kept.retain(|k| !is_ancestor_or_same(r, k));
        kept.push(r.clone());
    }
    kept
}

/// `b` が `a` と同一、または `a` の配下か（大小無視）。
fn is_ancestor_or_same(a: &LongPath, b: &LongPath) -> bool {
    let ac = comp_keys(a.as_path());
    let bc = comp_keys(b.as_path());
    ac.len() <= bc.len() && ac.iter().zip(bc.iter()).all(|(x, y)| x == y)
}

/// UNC パス（`\\server\share\...`）か。
#[must_use]
pub fn is_unc(path: &LongPath) -> bool {
    path.as_path().to_string_lossy().starts_with("\\\\")
}

/// 対象の過半が UNC（NAS）か（NFR-6: I/O 並列度制限の判定）。
#[must_use]
pub fn detect_nas_majority(roots: &[LongPath]) -> bool {
    if roots.is_empty() {
        return false;
    }
    let unc = roots.iter().filter(|r| is_unc(r)).count();
    unc * 2 > roots.len()
}

/// `*`（0 文字以上）・`?`（1 文字）の簡易ワイルドカード照合（大小無視）。
#[must_use]
pub fn matches_wildcard(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let s: Vec<char> = name.to_lowercase().chars().collect();
    matches_wildcard_lower(&p, &s)
}

/// 小文字化済みパターン・名前での照合本体（アロケーションなし）。
///
/// パターンと名前の小文字化＋`Vec<char>` 化を呼び出し側に切り出し、走査ホットパスで
/// 不変パターンを前処理（[`Scanner::new`]）して使い回せるようにする。
fn matches_wildcard_lower(p: &[char], s: &[char]) -> bool {
    let (mut pi, mut si) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut mark = 0usize;
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(st) = star {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn file_ext(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn norm_ext(e: &str) -> String {
    e.trim_start_matches('.').to_lowercase()
}

/// サイズ・ゼロバイトのフィルタ（アロケーションなし）。
///
/// `passes_filters` と [`Scanner::passes`]（前処理済み高速経路）で共有し、サイズ判定の
/// 二重定義を避ける。
fn passes_size_filters(size: u64, cfg: &ScanConfig) -> bool {
    if cfg.exclude_zero_byte && size == 0 {
        return false;
    }
    if let Some(min) = cfg.min_size
        && size < min
    {
        return false;
    }
    if let Some(max) = cfg.max_size
        && size > max
    {
        return false;
    }
    true
}

/// ファイル名・サイズが検索条件を満たすか（仕様 §3.2 フィルタ）。
///
/// 設定値から毎回正規化する参照実装。走査ホットパスでは設定不変な拡張子・パターンを
/// 前処理した [`Scanner::passes`] を使う（本関数は単体テスト・単発判定用）。
#[must_use]
pub fn passes_filters(name: &str, size: u64, cfg: &ScanConfig) -> bool {
    if !passes_size_filters(size, cfg) {
        return false;
    }
    if !cfg.include_extensions.is_empty() {
        let ext = file_ext(name);
        if !cfg.include_extensions.iter().any(|e| norm_ext(e) == ext) {
            return false;
        }
    }
    if cfg
        .exclude_patterns
        .iter()
        .any(|p| matches_wildcard(p, name))
    {
        return false;
    }
    true
}

pub(crate) fn has_system_component(p: &Path) -> bool {
    p.components().any(|c| {
        if let Component::Normal(os) = c {
            is_system_name(os)
        } else {
            false
        }
    })
}

/// 単一コンポーネント名（ファイル/フォルダ名）がシステムフォルダ名と一致するか（大小無視）。
pub(crate) fn is_system_name(name: &std::ffi::OsStr) -> bool {
    let lower = name.to_string_lossy().to_lowercase();
    SYSTEM_FOLDER_NAMES.contains(&lower.as_str())
}

/// メタデータが OneDrive オンデマンド（実体がクラウドのみ・未ダウンロード）ファイルを示すか
/// （仕様 §7.1）。非 Windows では属性が存在しないため常に `false`。
fn is_onedrive_on_demand(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        attrs_indicate_on_demand(meta.file_attributes())
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}

/// 属性ビットが OneDrive オンデマンド（`RECALL_ON_DATA_ACCESS` / `RECALL_ON_OPEN`）を示すか。
///
/// 純粋判定として分離し、属性取得（Windows 専用）から独立に単体テストできるようにする。
#[cfg(windows)]
#[must_use]
fn attrs_indicate_on_demand(attrs: u32) -> bool {
    /// クラウドのみ（データアクセス時に呼び戻し）。
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    /// オープン時に呼び戻し。
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    (attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN)) != 0
}

/// ファイル走査器。
pub struct Scanner<'a> {
    cfg: &'a ScanConfig,
    /// 小文字化済みの除外パターン（走査中不変）。ファイル毎の再小文字化を避ける。
    exclude_lower: Vec<Vec<char>>,
    /// 正規化済み（小文字・先頭ドット除去）の対象拡張子（走査中不変）。
    include_exts: Vec<String>,
}

impl<'a> Scanner<'a> {
    /// 検索条件から走査器を作る。メタデータは走査（`walkdir`）が提供する分を使う
    /// ため、ファイル毎の追加 stat は行わない。除外パターン・対象拡張子は走査中不変の
    /// ため、ここで 1 度だけ前処理しファイル毎の再正規化を避ける。
    #[must_use]
    pub fn new(cfg: &'a ScanConfig) -> Self {
        let exclude_lower = cfg
            .exclude_patterns
            .iter()
            .map(|p| p.to_lowercase().chars().collect())
            .collect();
        let include_exts = cfg.include_extensions.iter().map(|e| norm_ext(e)).collect();
        Self {
            cfg,
            exclude_lower,
            include_exts,
        }
    }

    /// ファイル名・サイズがフィルタを通過するか（前処理済みの高速経路）。
    ///
    /// [`passes_filters`] と同一の判定を、前処理済みパターン・拡張子と「1 回だけ小文字化
    /// した名前」で行う。サイズ判定は `passes_size_filters` を共有する。
    fn passes(&self, name: &str, size: u64) -> bool {
        if !passes_size_filters(size, self.cfg) {
            return false;
        }
        if !self.include_exts.is_empty() {
            let ext = file_ext(name);
            if !self.include_exts.contains(&ext) {
                return false;
            }
        }
        if !self.exclude_lower.is_empty() {
            let name_lower: Vec<char> = name.to_lowercase().chars().collect();
            if self
                .exclude_lower
                .iter()
                .any(|p| matches_wildcard_lower(p, &name_lower))
            {
                return false;
            }
        }
        true
    }

    /// 対象フォルダ群を再帰走査し、フィルタを通過したファイル一覧を返す。
    ///
    /// 中断要求時はその時点までの結果を返す。I/O エラーは個別スキップしてログ用に
    /// `ProgressEvent::Skipped` を送る（仕様 §6）。
    pub fn walk(
        &self,
        roots: &[LongPath],
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<FileEntry> {
        let _ = progress.send(ProgressEvent::Phase(Phase::Loading));
        let started = std::time::Instant::now();
        let mut acc = WalkAcc::default();

        for root in normalize_roots(roots) {
            // ルート自身（またはその祖先）がシステムフォルダなら配下は全件除外対象。走査を
            // 始めず丸ごとスキップする（祖先コンポーネントは filter_entry では検出できないため、
            // ここで判定して per-file 判定（旧 accept_entry）と同一結果にする）。
            if self.cfg.exclude_system_folders && has_system_component(root.as_path()) {
                continue;
            }
            let exclude_system = self.cfg.exclude_system_folders;
            let walker = WalkDir::new(root.as_path())
                .max_depth(MAX_DEPTH)
                .follow_links(self.cfg.follow_symlinks);
            // システムフォルダはディレクトリ降下ごと枝刈りして全列挙 I/O を避ける。同名の
            // ファイルも除外し、従来の has_system_component（全コンポーネント走査）と同結果にする。
            let iter = walker
                .into_iter()
                .filter_entry(move |e| !exclude_system || !is_system_name(e.file_name()));
            for entry in iter {
                if is_cancelled(cancel) {
                    tracing::info!(
                        scanned = acc.count,
                        skipped = acc.skipped,
                        ms = started.elapsed().as_millis() as u64,
                        "走査を中断しました"
                    );
                    return acc.out;
                }
                self.handle_entry(entry, &mut acc, progress);
            }
        }
        tracing::info!(
            scanned = acc.count,
            skipped = acc.skipped,
            ms = started.elapsed().as_millis() as u64,
            "走査完了"
        );
        acc.out
    }

    /// 走査 1 エントリを処理し集計へ反映する（`walk` のループ本体を平坦化, §16）。
    ///
    /// 走査エラーは個別スキップ通知、通過ファイルは収集＋間引き進捗通知する。
    fn handle_entry(
        &self,
        entry: walkdir::Result<walkdir::DirEntry>,
        acc: &mut WalkAcc,
        progress: &ProgressTx,
    ) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                // 走査エラー（アクセス拒否・ループ等）は個別スキップしてログ用に通知する。
                // metadata エラーと挙動を揃える（仕様 §6, H-3）。件数は多くなり得るため詳細は
                // DEBUG で残し、集計は walk 完了の INFO で出す。
                tracing::debug!(
                    path = ?err.path(),
                    error = %err,
                    "走査をスキップしました（アクセス不可・ループ等）"
                );
                if let Some(p) = err.path() {
                    let _ = progress.send(ProgressEvent::Skipped(p.to_path_buf()));
                }
                acc.skipped += 1;
                return;
            }
        };
        match self.accept_entry(&entry) {
            Ok(Some(file)) => {
                acc.out.push(file);
                acc.count += 1;
                if acc.count.is_multiple_of(PROGRESS_REPORT_INTERVAL) {
                    let _ = progress.send(ProgressEvent::Progress {
                        phase: Phase::Loading,
                        done: acc.count,
                        total: 0,
                    });
                }
            }
            Ok(None) => {}
            Err(p) => {
                tracing::debug!(
                    path = %p.display(),
                    "メタデータ取得に失敗しスキップしました"
                );
                let _ = progress.send(ProgressEvent::Skipped(p));
                acc.skipped += 1;
            }
        }
    }

    /// 1 エントリを評価する（システムフォルダ除外は呼び出し側の filter_entry が担当）。
    ///
    /// - 通過したファイル → `Ok(Some(FileEntry))`
    /// - ディレクトリ・フィルタ不通過・パス変換失敗 → `Ok(None)`
    /// - metadata 取得失敗 → `Err(path)`（呼び出し側で `Skipped` 通知＋集計, 仕様 §6）
    fn accept_entry(
        &self,
        entry: &walkdir::DirEntry,
    ) -> Result<Option<FileEntry>, std::path::PathBuf> {
        if !entry.file_type().is_file() {
            return Ok(None);
        }
        let path = entry.path();
        // システムフォルダ配下・同名ファイルは walk() の filter_entry とルート判定で
        // 既に除外済みのため、ここでの per-file 再判定は不要。
        // 正常な Unicode 名なら `to_string_lossy` は借用を返すため、確保を避けて借用のまま渡す。
        let name = entry.file_name().to_string_lossy();
        // `walkdir` の DirEntry が保持するメタデータを使う（Windows では readdir 由来で
        // 追加 stat 不要）。取得失敗は呼び出し側でスキップ通知する（仕様 §6）。
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => return Err(path.to_path_buf()),
        };
        // OneDrive オンデマンド（クラウドのみ）ファイルは実体が無く、ハッシュ計算で不要な
        // ダウンロードを誘発するため、有効時は走査から除外する（仕様 §7.1, 既定 OFF）。
        if self.cfg.exclude_onedrive_on_demand && is_onedrive_on_demand(&meta) {
            return Ok(None);
        }
        if !self.passes(&name, meta.len()) {
            return Ok(None);
        }
        let Ok(lp) = LongPath::new(path) else {
            return Ok(None);
        };
        Ok(Some(FileEntry {
            path: lp,
            size: FileSize(meta.len()),
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScanConfigBuilder;
    use crate::progress::new_cancel;
    use std::sync::mpsc::channel;

    fn lp(s: &str) -> LongPath {
        LongPath::new(s).unwrap()
    }

    #[test]
    fn normalize_removes_child_of_registered_parent() {
        let roots = vec![lp("C:\\a"), lp("C:\\a\\b"), lp("D:\\x")];
        let got = normalize_roots(&roots);
        assert_eq!(got, vec![lp("C:\\a"), lp("D:\\x")]);
    }

    #[test]
    fn normalize_is_case_insensitive() {
        let roots = vec![lp("C:\\Photos"), lp("C:\\photos\\2024")];
        assert_eq!(normalize_roots(&roots), vec![lp("C:\\Photos")]);
    }

    #[test]
    fn normalize_parent_added_after_child_replaces_it() {
        let roots = vec![lp("C:\\a\\b"), lp("C:\\a")];
        assert_eq!(normalize_roots(&roots), vec![lp("C:\\a")]);
    }

    #[test]
    fn nas_majority_detected() {
        assert!(detect_nas_majority(&[
            lp("\\\\srv\\s"),
            lp("\\\\srv\\t"),
            lp("C:\\a")
        ]));
        assert!(!detect_nas_majority(&[
            lp("\\\\srv\\s"),
            lp("C:\\a"),
            lp("D:\\b")
        ]));
        assert!(!detect_nas_majority(&[]));
    }

    #[cfg(windows)]
    #[test]
    fn onedrive_recall_attrs_detected() {
        // RECALL_ON_DATA_ACCESS / RECALL_ON_OPEN を検出し、通常属性（ARCHIVE 等）は除外しない。
        assert!(attrs_indicate_on_demand(0x0040_0000)); // RECALL_ON_DATA_ACCESS
        assert!(attrs_indicate_on_demand(0x0004_0000)); // RECALL_ON_OPEN
        assert!(attrs_indicate_on_demand(0x0040_0000 | 0x20)); // + ARCHIVE
        assert!(!attrs_indicate_on_demand(0x20)); // ARCHIVE のみ
        assert!(!attrs_indicate_on_demand(0));
    }

    #[test]
    fn wildcard_matches() {
        assert!(matches_wildcard("*.exe", "foo.exe"));
        assert!(matches_wildcard("*.EXE", "foo.exe"));
        assert!(!matches_wildcard("*.exe", "foo.txt"));
        assert!(matches_wildcard("?.txt", "a.txt"));
        assert!(!matches_wildcard("?.txt", "ab.txt"));
        assert!(matches_wildcard("a*c", "abbbc"));
        assert!(matches_wildcard("*", "anything"));
    }

    #[test]
    fn filters_apply() {
        let cfg = ScanConfigBuilder::new().build();
        assert!(passes_filters("a.txt", 10, &cfg));

        let mut z = ScanConfigBuilder::new().build();
        z.exclude_zero_byte = true;
        assert!(!passes_filters("a.txt", 0, &z));

        let mut ext = ScanConfigBuilder::new().build();
        ext.include_extensions = vec!["jpg".into(), ".png".into()];
        assert!(passes_filters("a.JPG", 1, &ext));
        assert!(!passes_filters("a.txt", 1, &ext));

        let mut ex = ScanConfigBuilder::new().build();
        ex.exclude_patterns = vec!["*.dll".into()];
        assert!(!passes_filters("x.dll", 1, &ex));
    }

    #[test]
    fn walk_notifies_skipped_on_unreadable_root() {
        // 存在しないルートでは walkdir がルートで Err を返す（アクセス拒否と同じ経路）。
        // H-3: 走査エラーも metadata エラーと同様に Skipped で通知し一貫させる（仕様 §6）。
        let cfg = ScanConfigBuilder::new().build();
        let scanner = Scanner::new(&cfg);
        let (tx, rx) = channel();
        let cancel = new_cancel();
        let missing = LongPath::new("C:\\__shredy_nonexistent_dir_xyz__").unwrap();
        let found = scanner.walk(&[missing], &tx, &cancel);
        assert!(found.is_empty());
        drop(tx);
        let skipped: Vec<_> = rx
            .into_iter()
            .filter(|e| matches!(e, ProgressEvent::Skipped(_)))
            .collect();
        assert_eq!(
            skipped.len(),
            1,
            "走査エラーは Skipped 1 件として通知される"
        );
    }

    #[test]
    fn walk_collects_files_and_respects_zero_byte_filter() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"hello").unwrap();
        std::fs::create_dir(base.join("sub")).unwrap();
        std::fs::write(base.join("sub").join("b.txt"), b"world!!").unwrap();
        std::fs::write(base.join("empty.bin"), b"").unwrap();

        let cfg = ScanConfigBuilder::new().build();
        let scanner = Scanner::new(&cfg);
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        let root = LongPath::new(base).unwrap();
        let found = scanner.walk(&[root], &tx, &cancel);
        assert_eq!(found.len(), 3);

        let mut z = ScanConfigBuilder::new().build();
        z.exclude_zero_byte = true;
        let scanner2 = Scanner::new(&z);
        let (tx2, _rx2) = channel();
        let root2 = LongPath::new(base).unwrap();
        let found2 = scanner2.walk(&[root2], &tx2, &cancel);
        assert_eq!(found2.len(), 2);
    }

    #[test]
    fn excludes_files_under_system_named_subdir() {
        // システム名のサブフォルダ（例: "Windows"）配下は降下ごと枝刈りされ、配下ファイルは
        // 列挙されない。同名外のファイルは通常どおり収集される。
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("keep.txt"), b"keep").unwrap();
        std::fs::create_dir(base.join("Windows")).unwrap();
        std::fs::write(base.join("Windows").join("hidden.txt"), b"hidden").unwrap();

        let mut cfg = ScanConfigBuilder::new().build();
        cfg.exclude_system_folders = true;
        let scanner = Scanner::new(&cfg);
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        let found = scanner.walk(&[LongPath::new(base).unwrap()], &tx, &cancel);
        let names: Vec<String> = found
            .iter()
            .map(|f| {
                f.path
                    .as_path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["keep.txt".to_string()]);
    }

    #[test]
    fn includes_system_named_subdir_when_exclusion_disabled() {
        // exclude_system_folders=false なら、システム名のフォルダも通常どおり走査される。
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::create_dir(base.join("System32")).unwrap();
        std::fs::write(base.join("System32").join("a.txt"), b"a").unwrap();

        let mut cfg = ScanConfigBuilder::new().build();
        cfg.exclude_system_folders = false;
        let scanner = Scanner::new(&cfg);
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        let found = scanner.walk(&[LongPath::new(base).unwrap()], &tx, &cancel);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn skips_entire_root_when_root_is_inside_system_folder() {
        // ルート自身がシステムフォルダ配下（祖先にシステム名）なら配下を全件除外する。
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let sysroot = base.join("Windows").join("Temp");
        std::fs::create_dir_all(&sysroot).unwrap();
        std::fs::write(sysroot.join("x.txt"), b"x").unwrap();

        let mut cfg = ScanConfigBuilder::new().build();
        cfg.exclude_system_folders = true;
        let scanner = Scanner::new(&cfg);
        let (tx, _rx) = channel();
        let cancel = new_cancel();
        let found = scanner.walk(&[LongPath::new(&sysroot).unwrap()], &tx, &cancel);
        assert!(
            found.is_empty(),
            "システムフォルダ配下のルートは全件除外される"
        );
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    fn lp(s: &str) -> LongPath {
        LongPath::new(s).unwrap()
    }

    proptest! {
        // PBT-04: normalize_roots は冪等。
        #[test]
        fn normalize_is_idempotent(names in proptest::collection::vec("[a-z]{1,5}", 0..6)) {
            let roots: Vec<LongPath> = names.iter().map(|n| lp(&format!("C:\\{n}"))).collect();
            let once = normalize_roots(&roots);
            let twice = normalize_roots(&once);
            prop_assert_eq!(once, twice);
        }
    }
}
