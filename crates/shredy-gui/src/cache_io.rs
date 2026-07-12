//! ハッシュキャッシュのロード/保存は core へ集約済み（[`shredy_core::cache_io`]）。
//!
//! ログ統一後に CLI と差異がなくなったため共通化した。GUI 既存の呼び出し
//! （`cache_io::load_hash_cache` / `save_hash_cache`）を温存するための再エクスポート。

pub(crate) use shredy_core::{load_hash_cache, save_hash_cache, CACHE_FILE_NAME};
