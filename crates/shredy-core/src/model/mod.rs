//! コアのドメイン型（Effective Rust 第1章: 型で不正状態を表現不能にする）。
//!
//! U1 では型定義のみを行い、各型を使う具体ロジックは後続ユニットで実装する。

use std::time::SystemTime;

use crate::path::LongPath;

/// ファイル比較方式。MVP はハッシュ全体比較 0-2 のみ。
///
/// 後続ユニットで `Partial`/`NameSize*`/`OfficeSimilarity`/`ImageSimilarity` を
/// 加法的に追加する（仕様 §3.3）。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum Method {
    /// BLAKE3 全体ハッシュ（既定）。
    #[default]
    Blake3,
    /// SHA-256 全体ハッシュ。
    Sha256,
    /// XXH3-128 全体ハッシュ。
    Xxh3,
}

/// 重複グループ内で「残す 1 件」を決める基準（仕様 §3.4）。
///
/// JSON 値は仕様 §5 の PascalCase（`ByDateNewest` 等）と一致する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum KeepFileRule {
    ByDateNewest,
    ByDateOldest,
    ByPathAsc,
    ByPathDesc,
    ByPathLengthShortest,
    ByPathLengthLongest,
}

/// ファイルサイズ（バイト）。生の `u64` 取り違え防止の newtype（Item 6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileSize(pub u64);

/// 知覚ハッシュ（pHash, 64-bit）。後続の画像類似度用に定義のみ（仕様 §3.12）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PHash(pub u64);

/// 内容ハッシュ。方式によりバイト長が異なるため enum で表現する
/// （BLAKE3/SHA-256 = 32 byte, XXH3-128 = 16 byte）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FileHash {
    /// 256-bit（BLAKE3 / SHA-256）。
    B256([u8; 32]),
    /// 128-bit（XXH3-128）。
    B128([u8; 16]),
}

/// 走査で収集した 1 ファイルの情報。
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: LongPath,
    pub size: FileSize,
    pub modified: SystemTime,
}

/// 同一とみなされたファイル群（2 件以上）。
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// 代表指紋（例: `"blake3:<hex>"`）。具体形式は U4 で確定する。
    pub representative: String,
    pub files: Vec<FileEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_method_is_blake3() {
        assert_eq!(Method::default(), Method::Blake3);
    }

    #[test]
    fn file_size_orders_numerically() {
        assert!(FileSize(10) < FileSize(20));
    }

    #[test]
    fn file_hash_variants_differ_by_length() {
        assert_ne!(FileHash::B256([0u8; 32]), FileHash::B128([0u8; 16]));
    }
}
