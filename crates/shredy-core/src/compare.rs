//! 比較方式（仕様 §3.3 の方式 0-2）とフィンガープリント生成（ガイド §8/§13.9）。
//!
//! `Comparator` トレイトで方式を差し替え可能にする。`HashComparator` は
//! BLAKE3 / SHA-256 / XXH3-128 を提供し、先頭 64KB の部分ハッシュ（枝刈り用）と
//! ファイル全体のハッシュを計算する。

use std::io::Read;

use sha2::Digest;

use crate::error::{CoreError, Result};
use crate::io::FileReader;
use crate::model::{FileEntry, FileHash, Method};

/// 2 段階最適化で先読みする先頭バイト数（64KB）。
pub const PARTIAL_LEN: u64 = 64 * 1024;

const STREAM_CHUNK: usize = 64 * 1024;

thread_local! {
    /// 全体ハッシュ用の再利用バッファ。ファイル毎の 64KB 確保＋ゼロ初期化を避けるため、
    /// rayon ワーカーごとに 1 つ確保し `full` 呼び出し間で使い回す。書き込んだ範囲しか
    /// 読まない（`&buf[..n]`）ため、初期値がゼロである必要はない。
    static FULL_HASH_BUF: std::cell::RefCell<Vec<u8>> =
        std::cell::RefCell::new(vec![0u8; STREAM_CHUNK]);
}

/// 比較方式の抽象。実装を差し替えて方式を切り替える（Item 12/13）。
pub trait Comparator: Send + Sync {
    /// 先頭 [`PARTIAL_LEN`] バイトのハッシュ（2 段階最適化の枝刈り用）。
    fn partial(&self, entry: &FileEntry, fs: &dyn FileReader) -> Result<FileHash>;

    /// ファイル全体のハッシュ。
    fn full(&self, entry: &FileEntry, fs: &dyn FileReader) -> Result<FileHash>;

    /// 代表ハッシュ文字列のラベル（例: `"blake3"`）。
    fn label(&self) -> &'static str;
}

/// ハッシュ全体比較（仕様 §3.3 方式 0-2）。
#[derive(Debug, Clone, Copy)]
pub struct HashComparator {
    method: Method,
}

impl HashComparator {
    /// 指定方式のコンパレータを作る。
    #[must_use]
    pub fn new(method: Method) -> Self {
        Self { method }
    }

    /// ハッシュ方式を返す（ログ表示等）。
    #[must_use]
    pub fn method(&self) -> Method {
        self.method
    }
}

impl Comparator for HashComparator {
    fn partial(&self, entry: &FileEntry, fs: &dyn FileReader) -> Result<FileHash> {
        let bytes = fs.read_prefix(&entry.path, PARTIAL_LEN)?;
        let mut hasher = AnyHasher::new(self.method);
        hasher.update(&bytes);
        Ok(hasher.finish())
    }

    fn full(&self, entry: &FileEntry, fs: &dyn FileReader) -> Result<FileHash> {
        let mut reader = fs.open_read(&entry.path)?;
        let mut hasher = AnyHasher::new(self.method);
        FULL_HASH_BUF.with(|cell| -> Result<()> {
            let mut buf = cell.borrow_mut();
            loop {
                let n = reader
                    .read(&mut buf[..])
                    .map_err(|e| CoreError::io(entry.path.as_path(), e))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            Ok(())
        })?;
        Ok(hasher.finish())
    }

    fn label(&self) -> &'static str {
        match self.method {
            Method::Blake3 => "blake3",
            Method::Sha256 => "sha256",
            Method::Xxh3 => "xxh3",
        }
    }
}

/// 方式ごとのハッシュ計算をまとめる内部 enum。
// 各ハッシャの内部サイズ差は大きいが、1 ファイル処理中のみ存在する短命な値のため許容する。
#[allow(clippy::large_enum_variant)]
enum AnyHasher {
    Blake3(blake3::Hasher),
    Sha256(sha2::Sha256),
    Xxh3(xxhash_rust::xxh3::Xxh3),
}

impl AnyHasher {
    fn new(method: Method) -> Self {
        match method {
            Method::Blake3 => AnyHasher::Blake3(blake3::Hasher::new()),
            Method::Sha256 => AnyHasher::Sha256(sha2::Sha256::new()),
            Method::Xxh3 => AnyHasher::Xxh3(xxhash_rust::xxh3::Xxh3::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            AnyHasher::Blake3(h) => {
                h.update(bytes);
            }
            AnyHasher::Sha256(h) => h.update(bytes),
            AnyHasher::Xxh3(h) => h.update(bytes),
        }
    }

    fn finish(self) -> FileHash {
        match self {
            AnyHasher::Blake3(h) => FileHash::B256(*h.finalize().as_bytes()),
            AnyHasher::Sha256(h) => {
                let digest = h.finalize();
                let mut out = [0u8; 32];
                out.copy_from_slice(&digest);
                FileHash::B256(out)
            }
            AnyHasher::Xxh3(h) => FileHash::B128(h.digest128().to_le_bytes()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;
    use crate::model::FileSize;
    use crate::path::LongPath;
    use std::time::SystemTime;

    fn entry(path: &str, fs: &mut MockFs, content: &[u8]) -> FileEntry {
        let lp = LongPath::new(path).unwrap();
        fs.add_file(&lp, content.to_vec());
        FileEntry {
            path: lp,
            size: FileSize(content.len() as u64),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn identical_content_yields_identical_hash() {
        let mut fs = MockFs::new();
        let a = entry("C:\\a", &mut fs, b"hello world");
        let b = entry("C:\\b", &mut fs, b"hello world");
        let c = HashComparator::new(Method::Blake3);
        assert_eq!(c.full(&a, &fs).unwrap(), c.full(&b, &fs).unwrap());
    }

    #[test]
    fn different_content_yields_different_hash() {
        let mut fs = MockFs::new();
        let a = entry("C:\\a", &mut fs, b"hello");
        let b = entry("C:\\b", &mut fs, b"world");
        let c = HashComparator::new(Method::Sha256);
        assert_ne!(c.full(&a, &fs).unwrap(), c.full(&b, &fs).unwrap());
    }

    #[test]
    fn xxh3_produces_128bit() {
        let mut fs = MockFs::new();
        let a = entry("C:\\a", &mut fs, b"data");
        let c = HashComparator::new(Method::Xxh3);
        assert!(matches!(c.full(&a, &fs).unwrap(), FileHash::B128(_)));
    }

    #[test]
    fn label_matches_method() {
        assert_eq!(HashComparator::new(Method::Blake3).label(), "blake3");
        assert_eq!(HashComparator::new(Method::Sha256).label(), "sha256");
        assert_eq!(HashComparator::new(Method::Xxh3).label(), "xxh3");
    }
}
