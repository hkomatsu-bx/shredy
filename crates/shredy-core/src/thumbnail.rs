//! サムネイルキャッシュ削除の候補列挙（仕様 §3.10.4・ユーティリティ機能）。
//!
//! 削除候補を列挙する探索のみを提供する:
//! - 指定フォルダ配下の `Thumbs.db`（レガシー形式・再帰）
//! - Windows のシステムサムネイルキャッシュ
//!   `%LocalAppData%\Microsoft\Windows\Explorer\thumbcache_*.db`
//!
//! 実削除は既存の [`Deleter`](crate::delete::Deleter)（`plan_paths` → `execute`）を再利用する。
//! 解放バイト数は [`DeleteReport`](crate::delete::DeleteReport) の `deleted` から集計できる。
//! 本モジュールは「どのファイルを消すか」だけを決め、削除ロジックを重複させない（DRY）。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use walkdir::WalkDir;

use crate::path::LongPath;
use crate::progress::{Cancel, is_cancelled};
use crate::scan::MAX_DEPTH;
use crate::utility::UtilityItem;

/// レガシー形式のサムネイルキャッシュファイル名（小文字）。
const THUMBS_DB: &str = "thumbs.db";

/// 指定フォルダ配下の `Thumbs.db` を再帰的に列挙する（ファイル名は大小無視）。
///
/// 中断要求時はその時点までの結果を返す。リパースポイントは辿らない（実体巻き込み防止）。
/// 走査エラー（アクセス拒否等）は個別スキップする。
#[must_use]
pub fn find_thumbs_db(folders: &[LongPath], cancel: &Cancel) -> Vec<LongPath> {
    let mut out = Vec::new();
    for root in folders {
        let walker = WalkDir::new(root.as_path())
            .max_depth(MAX_DEPTH)
            .follow_links(false);
        for entry in walker.into_iter().flatten() {
            if is_cancelled(cancel) {
                return out;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.file_name().to_string_lossy().to_ascii_lowercase() != THUMBS_DB {
                continue;
            }
            if let Ok(lp) = LongPath::new(entry.path()) {
                out.push(lp);
            }
        }
    }
    out
}

/// Windows のシステムサムネイルキャッシュ格納フォルダ
/// （`%LocalAppData%\Microsoft\Windows\Explorer`）。環境変数が無い場合は `None`。
#[must_use]
pub fn system_thumbcache_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    if local.is_empty() {
        return None;
    }
    Some(
        Path::new(&local)
            .join("Microsoft")
            .join("Windows")
            .join("Explorer"),
    )
}

/// システムサムネイルキャッシュ（`thumbcache_*.db`）を列挙する。
///
/// 格納フォルダが取得できない・読み取れない場合は空を返す（best-effort）。
/// Explorer フォルダ直下のみ（非再帰）でエントリ数も限定的なため中断（`Cancel`）は受けない。
#[must_use]
pub fn find_system_thumbcaches() -> Vec<LongPath> {
    let Some(dir) = system_thumbcache_dir() else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if is_thumbcache_name(&name)
            && let Ok(lp) = LongPath::new(entry.path())
        {
            out.push(lp);
        }
    }
    out
}

/// ファイル名（小文字化済み）が `thumbcache_*.db` か。
#[must_use]
fn is_thumbcache_name(lower: &str) -> bool {
    lower.starts_with("thumbcache_") && lower.ends_with(".db")
}

/// サムネイルキャッシュの削除候補（フォルダ配下の `Thumbs.db` ＋ システムキャッシュ）を
/// まとめて列挙する。
#[must_use]
pub fn find_thumbnail_caches(folders: &[LongPath], cancel: &Cancel) -> Vec<LongPath> {
    let mut out = find_thumbs_db(folders, cancel);
    out.extend(find_system_thumbcaches());
    out
}

/// サムネイルキャッシュ削除候補を [`UtilityItem`] の一覧として列挙する（GUI ユーティリティ用）。
///
/// 各項目の `detail` で発生源を区別する:
/// - フォルダ配下の `Thumbs.db` → `"フォルダ内 Thumbs.db"`
/// - システムサムネイルキャッシュ（ユーザー全体・対象フォルダ無関係）→ `"システムキャッシュ"`
///
/// サイズ・更新日時はファイルを stat して埋める（取得失敗時は `0` / `UNIX_EPOCH`）。
/// 削除は既存の [`Deleter`](crate::delete::Deleter) を再利用するため本関数は列挙のみを担う。
#[must_use]
pub fn find_thumbnail_cache_items(folders: &[LongPath], cancel: &Cancel) -> Vec<UtilityItem> {
    let mut items: Vec<UtilityItem> = find_thumbs_db(folders, cancel)
        .into_iter()
        .map(|path| cache_item(path, "フォルダ内 Thumbs.db"))
        .collect();
    items.extend(
        find_system_thumbcaches()
            .into_iter()
            .map(|path| cache_item(path, "システムキャッシュ")),
    );
    items
}

/// サムネイルキャッシュ候補 1 件を [`UtilityItem`] へ変換する（size/modified は stat 結果）。
fn cache_item(path: LongPath, detail: &str) -> UtilityItem {
    let meta = std::fs::metadata(&*path.fs_path()).ok();
    let size = meta.as_ref().map_or(0, std::fs::Metadata::len);
    let modified = meta
        .and_then(|m| m.modified().ok())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    UtilityItem {
        path,
        size,
        modified,
        is_dir: false,
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::new_cancel;

    #[test]
    fn thumbcache_name_matches_pattern() {
        assert!(is_thumbcache_name("thumbcache_32.db"));
        assert!(is_thumbcache_name("thumbcache_1024.db"));
        assert!(is_thumbcache_name("thumbcache_idx.db"));
        assert!(!is_thumbcache_name("thumbs.db"));
        assert!(!is_thumbcache_name("thumbcache_32.dat"));
        assert!(!is_thumbcache_name("mythumbcache_32.db"));
    }

    #[test]
    fn finds_thumbs_db_recursively_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("Thumbs.db"), b"x").unwrap();
        std::fs::write(base.join("keep.txt"), b"y").unwrap();
        std::fs::create_dir(base.join("sub")).unwrap();
        std::fs::write(base.join("sub").join("THUMBS.DB"), b"z").unwrap();

        let cancel = new_cancel();
        let found = find_thumbs_db(&[LongPath::new(base).unwrap()], &cancel);
        assert_eq!(found.len(), 2, "大小無視で 2 件の Thumbs.db を検出する");
    }

    #[test]
    fn find_thumbs_db_returns_empty_for_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        let cancel = new_cancel();
        assert!(find_thumbs_db(&[LongPath::new(dir.path()).unwrap()], &cancel).is_empty());
    }

    #[test]
    fn cache_items_label_folder_thumbs_db() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Thumbs.db"), b"xyz").unwrap();
        let cancel = new_cancel();
        let items = find_thumbnail_cache_items(&[LongPath::new(dir.path()).unwrap()], &cancel);
        // システムキャッシュは環境依存のため、フォルダ由来の項目だけを検証する。
        let folder: Vec<_> = items
            .iter()
            .filter(|i| i.detail == "フォルダ内 Thumbs.db")
            .collect();
        assert_eq!(folder.len(), 1, "Thumbs.db を 1 件 UtilityItem 化する");
        assert!(!folder[0].is_dir);
        assert_eq!(folder[0].size, 3, "stat で実サイズを埋める");
    }
}
