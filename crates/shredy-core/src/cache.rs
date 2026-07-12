//! ハッシュキャッシュ（`shredy.cache`, 仕様 §5）。
//!
//! パス + サイズ + 更新日時をキーに内容ハッシュを永続化し、再検索時の再ハッシュを避ける。
//! 比較方式の変更時は [`HashCache::invalidate_all`] で全無効化する（仕様 §3.3）。

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::{FileHash, FileSize, Method};

/// キャッシュのキー（パス + サイズ + 更新日時）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey(String);

impl CacheKey {
    /// パス・サイズ・更新日時から安定なキー文字列を生成する。
    ///
    /// 更新日時はサブ秒（ナノ秒）まで含める。秒精度だと同一秒内に同サイズで上書き
    /// されたファイルでキーが衝突し、古いハッシュへ誤命中する恐れがあるため。
    /// （注: キー形式の変更で旧 `shredy.cache` は初回のみ全ミスになるが、再ハッシュで
    /// 自動再生成されるだけで結果は正しい。）
    #[must_use]
    pub fn new(path: &Path, size: FileSize, modified: SystemTime) -> Self {
        let (secs, nanos) = modified
            .duration_since(UNIX_EPOCH)
            .map(|d| (d.as_secs(), d.subsec_nanos()))
            .unwrap_or((0, 0));
        CacheKey(format!(
            "{}|{}|{}.{}",
            path.to_string_lossy(),
            size.0,
            secs,
            nanos
        ))
    }
}

/// 内容ハッシュの永続キャッシュ。
///
/// `method` は格納ハッシュがどの方式（BLAKE3/SHA-256/XXH3）で計算されたかを記録する。
/// 方式が変わると過去のハッシュは無意味になるため [`HashCache::load_for_method`] で
/// 全無効化する（仕様 §3.3）。`#[serde(default)]` で旧形式（method 無し）とも互換。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HashCache {
    #[serde(default)]
    method: Option<Method>,
    /// 全体（内容）ハッシュ。キー = パス|サイズ|更新日時。
    entries: HashMap<String, FileHash>,
    /// 先頭 `PARTIAL_LEN` バイトの部分ハッシュ。非重複・同サイズの大ファイルが毎回
    /// 部分ハッシュのために再読込されるのを避ける（`#[serde(default)]` で旧形式と互換）。
    #[serde(default)]
    partial_entries: HashMap<String, FileHash>,
    /// 前回ロード（または保存）以降に内容が変化したか。warm 再検索で 1 件も変わらない
    /// 場合に [`HashCache::save`] の全量再シリアライズ・再書き込みを省くためのフラグ。
    /// 永続化対象ではない（`#[serde(skip)]`、ロード直後は常に `false`）。
    #[serde(skip)]
    dirty: bool,
}

impl HashCache {
    /// 空のキャッシュ。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定方式タグ付きの空キャッシュ（読み込み失敗時のフォールバック用）。
    ///
    /// 方式タグを確実に永続化するため `dirty = true` で作る（破損キャッシュの上書き・
    /// 新規方式タグの記録のため、保存をスキップさせない）。
    #[must_use]
    pub fn for_method(method: Method) -> Self {
        Self {
            method: Some(method),
            entries: HashMap::new(),
            partial_entries: HashMap::new(),
            dirty: true,
        }
    }

    /// キャッシュファイルを読み込む。ファイルが無い場合は空のキャッシュを返す。
    ///
    /// `BufReader` 経由でストリーミングデシリアライズし、ファイル全体を `String` へ
    /// 読み込む中間バッファ（数万エントリで数十 MB）の二重確保を避ける。
    pub fn load(path: &Path) -> Result<HashCache> {
        match std::fs::File::open(path) {
            Ok(file) => serde_json::from_reader(std::io::BufReader::new(file))
                .map_err(|e| CoreError::Config(format!("invalid cache: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashCache::new()),
            Err(e) => Err(CoreError::io(path, e)),
        }
    }

    /// 指定方式で使うキャッシュをロードする。保存時の方式と異なる場合は全エントリを
    /// 無効化して方式を更新する（仕様 §3.3「方式変更で再検索」）。
    pub fn load_for_method(path: &Path, method: Method) -> Result<HashCache> {
        let mut cache = Self::load(path)?;
        if cache.method != Some(method) {
            cache.invalidate_all();
            cache.method = Some(method);
        }
        Ok(cache)
    }

    /// このキャッシュが対象とする方式。
    #[must_use]
    pub fn method(&self) -> Option<Method> {
        self.method
    }

    /// キャッシュファイルへ保存する。
    ///
    /// 前回ロード/保存以降に変化が無ければ（`dirty == false`）何も書かない。warm 再検索で
    /// 全件キャッシュ命中の場合、同一内容を再シリアライズ・再書き込みするのを避ける。
    /// 書き込み時は `BufWriter` 経由でストリーミング直列化し、全文 `String`（数万エントリで
    /// 数十 MB）の中間バッファ確保を避ける。
    pub fn save(&self, path: &Path) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let file = std::fs::File::create(path).map_err(|e| CoreError::io(path, e))?;
        let mut writer = std::io::BufWriter::new(file);
        serde_json::to_writer(&mut writer, self)
            .map_err(|e| CoreError::Config(format!("serialize cache failed: {e}")))?;
        writer.flush().map_err(|e| CoreError::io(path, e))
    }

    /// キーに対応するハッシュを取得する。
    #[must_use]
    pub fn get(&self, key: &CacheKey) -> Option<&FileHash> {
        self.entries.get(&key.0)
    }

    /// ハッシュを登録する。
    pub fn put(&mut self, key: CacheKey, hash: FileHash) {
        self.entries.insert(key.0, hash);
        self.dirty = true;
    }

    /// キーに対応する部分ハッシュ（先頭 `PARTIAL_LEN` バイト）を取得する。
    #[must_use]
    pub fn get_partial(&self, key: &CacheKey) -> Option<&FileHash> {
        self.partial_entries.get(&key.0)
    }

    /// 部分ハッシュを登録する。
    pub fn put_partial(&mut self, key: CacheKey, hash: FileHash) {
        self.partial_entries.insert(key.0, hash);
        self.dirty = true;
    }

    /// 全エントリ（全体・部分とも）を無効化する（比較方式変更時）。
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
        self.partial_entries.clear();
        // クリア後の状態（空エントリ・新方式タグ）を確実に永続化させる。
        self.dirty = true;
    }

    /// 登録件数（全体ハッシュ＋部分ハッシュの合計）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len() + self.partial_entries.len()
    }

    /// 全体・部分のいずれにもエントリが無いか。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.partial_entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> CacheKey {
        CacheKey::new(Path::new("C:\\a.bin"), FileSize(10), SystemTime::UNIX_EPOCH)
    }

    #[test]
    fn put_then_get_returns_hash() {
        let mut c = HashCache::new();
        let h = FileHash::B128([7u8; 16]);
        c.put(key(), h.clone());
        assert_eq!(c.get(&key()), Some(&h));
    }

    #[test]
    fn invalidate_all_clears_entries() {
        let mut c = HashCache::new();
        c.put(key(), FileHash::B256([0u8; 32]));
        c.put_partial(key(), FileHash::B256([1u8; 32]));
        assert!(!c.is_empty());
        assert_eq!(c.len(), 2);
        c.invalidate_all();
        assert!(c.is_empty());
        assert!(c.get_partial(&key()).is_none());
    }

    #[test]
    fn different_size_yields_different_key() {
        let k1 = CacheKey::new(Path::new("C:\\a"), FileSize(1), SystemTime::UNIX_EPOCH);
        let k2 = CacheKey::new(Path::new("C:\\a"), FileSize(2), SystemTime::UNIX_EPOCH);
        assert_ne!(k1, k2);
    }

    #[test]
    fn subsecond_mtime_difference_yields_different_key() {
        use std::time::Duration;
        // 同一秒内・同サイズでも、サブ秒が異なれば別キーになる（誤命中防止）。
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let k1 = CacheKey::new(Path::new("C:\\a"), FileSize(10), base);
        let k2 = CacheKey::new(
            Path::new("C:\\a"),
            FileSize(10),
            base + Duration::from_millis(500),
        );
        assert_ne!(k1, k2);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shredy.cache");
        let mut c = HashCache::new();
        c.put(key(), FileHash::B256([1u8; 32]));
        c.save(&path).unwrap();
        let loaded = HashCache::load(&path).unwrap();
        assert_eq!(loaded.get(&key()), Some(&FileHash::B256([1u8; 32])));
    }

    #[test]
    fn save_skips_when_not_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shredy.cache");
        // 欠損ファイルのロードは空のクリーン（dirty=false）キャッシュを返す。
        let c = HashCache::load(&path).unwrap();
        c.save(&path).unwrap();
        assert!(
            !path.exists(),
            "無変更なら保存をスキップしファイルを作らない"
        );
    }

    #[test]
    fn save_writes_after_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shredy.cache");
        let mut c = HashCache::load(&path).unwrap(); // クリーン
        c.put(key(), FileHash::B128([1u8; 16])); // dirty
        c.save(&path).unwrap();
        assert!(path.exists(), "変更後は保存される");
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let c = HashCache::load(&dir.path().join("nope.cache")).unwrap();
        assert!(c.is_empty());
    }

    #[test]
    fn load_for_method_keeps_entries_when_method_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shredy.cache");
        let mut c = HashCache::for_method(Method::Blake3);
        c.put(key(), FileHash::B256([1u8; 32]));
        c.save(&path).unwrap();

        let loaded = HashCache::load_for_method(&path, Method::Blake3).unwrap();
        assert_eq!(loaded.method(), Some(Method::Blake3));
        assert_eq!(loaded.get(&key()), Some(&FileHash::B256([1u8; 32])));
    }

    #[test]
    fn load_for_method_invalidates_on_method_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shredy.cache");
        let mut c = HashCache::for_method(Method::Blake3);
        c.put(key(), FileHash::B256([1u8; 32]));
        c.save(&path).unwrap();

        // 方式が変われば過去ハッシュは無意味 → 全無効化して新方式を記録（仕様 §3.3）。
        let loaded = HashCache::load_for_method(&path, Method::Xxh3).unwrap();
        assert_eq!(loaded.method(), Some(Method::Xxh3));
        assert!(loaded.is_empty());
    }
}
