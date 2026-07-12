//! ハッシュキャッシュ（`shredy.cache`, 仕様 §5）のロード/保存ヘルパ（CLI/GUI 共通）。
//!
//! `UseHashCache` 有効かつ内容ハッシュ方式（S0-2）のときのみキャッシュを用い、warm 再検索の
//! 再ハッシュを避ける。方式変更時は [`HashCache::load_for_method`] がタグ不一致で全無効化する。
//! 以前は CLI（bin）と GUI に重複していたが、ログ統一後は差異がなくなったため core へ集約した。

use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::cache::HashCache;
use crate::config::{AppConfig, CompareMethod};
use crate::strategy::content_hash_method;

/// ハッシュキャッシュの既定ファイル名（仕様 §5）。CLI/GUI で重複していたリテラルの単一ソース。
pub const CACHE_FILE_NAME: &str = "shredy.cache";

/// 設定と方式に応じてハッシュキャッシュをロードする。
///
/// `UseHashCache` 有効かつ内容ハッシュ方式（S0-2）のときのみ `Some`。読み込み失敗時は
/// 方式タグ付きの空キャッシュへフォールバックし警告ログを残す（仕様 §5）。
#[must_use]
pub fn load_hash_cache(
    config: &AppConfig,
    method: CompareMethod,
    cache_path: &Path,
) -> Option<Arc<RwLock<HashCache>>> {
    if !config.use_hash_cache {
        return None;
    }
    let hash_method = content_hash_method(method)?;
    let cache = HashCache::load_for_method(cache_path, hash_method).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "ハッシュキャッシュの読み込みに失敗、新規作成します");
        HashCache::for_method(hash_method)
    });
    Some(Arc::new(RwLock::new(cache)))
}

/// ハッシュキャッシュを保存する。失敗は警告ログのみ（検索結果には影響しない）。
pub fn save_hash_cache(cache: Option<&Arc<RwLock<HashCache>>>, cache_path: &Path) {
    let Some(cache) = cache else {
        return;
    };
    let guard = cache.read().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = guard.save(cache_path) {
        tracing::warn!(error = %e, "ハッシュキャッシュの保存に失敗しました");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Method;

    fn config(use_cache: bool) -> AppConfig {
        AppConfig {
            use_hash_cache: use_cache,
            ..AppConfig::default()
        }
    }

    #[test]
    fn returns_none_when_cache_disabled() {
        let cfg = config(false);
        let path = Path::new("nonexistent.cache");
        assert!(load_hash_cache(&cfg, CompareMethod::HashBlake3, path).is_none());
    }

    #[test]
    fn returns_some_for_content_hash_method() {
        let cfg = config(true);
        // 存在しないパスでも空キャッシュへフォールバックして Some を返す。
        let path = Path::new("nonexistent.cache");
        let cache = load_hash_cache(&cfg, CompareMethod::HashBlake3, path)
            .expect("内容ハッシュ方式 + 有効ならキャッシュを返す");
        assert_eq!(cache.read().unwrap().method(), Some(Method::Blake3));
    }

    #[test]
    fn returns_none_for_metadata_method() {
        let cfg = config(true);
        let path = Path::new("nonexistent.cache");
        // メタデータ方式（S4-6）はキャッシュ対象外。
        assert!(load_hash_cache(&cfg, CompareMethod::NameSize, path).is_none());
    }

    #[test]
    fn save_is_noop_for_none() {
        // None を渡すと何もしない（パスへ書き込まない）。
        let path = Path::new("should_not_be_created.cache");
        save_hash_cache(None, path);
        assert!(!path.exists());
    }

    #[test]
    fn save_then_load_roundtrips_method_tag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shredy.cache");
        let cache = Arc::new(RwLock::new(HashCache::for_method(Method::Xxh3)));
        save_hash_cache(Some(&cache), &path);
        assert!(path.exists());

        let reloaded = HashCache::load_for_method(&path, Method::Xxh3).unwrap();
        assert_eq!(reloaded.method(), Some(Method::Xxh3));
    }
}
