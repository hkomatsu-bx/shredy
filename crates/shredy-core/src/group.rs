//! 重複グルーピング（フィンガープリント一致でファイルを束ねる）。

use std::collections::HashMap;
use std::hash::Hash;

use crate::model::{DuplicateGroup, FileEntry, FileHash};

/// `(ファイル, キー)` の列を、キー一致ごとに [`DuplicateGroup`] へ束ねる汎用関数。
///
/// 2 件以上のグループのみ返す。代表文字列は `representative(&key)` で生成する。
/// 内容ハッシュ・メタデータ・部分一致など全方式の共通土台（仕様 §3.3）。
#[must_use]
pub fn group_by_key<K, F>(items: Vec<(FileEntry, K)>, representative: F) -> Vec<DuplicateGroup>
where
    K: Hash + Eq,
    F: Fn(&K) -> String,
{
    // 最大でも要素数だけ異なるキーが現れる。再ハッシュを避けるため事前確保する。
    let mut map: HashMap<K, Vec<FileEntry>> = HashMap::with_capacity(items.len());
    for (entry, key) in items {
        map.entry(key).or_default().push(entry);
    }
    map.into_iter()
        .filter(|(_, files)| files.len() >= 2)
        .map(|(key, files)| DuplicateGroup {
            representative: representative(&key),
            files,
        })
        .collect()
}

/// `(ファイル, ハッシュ)` の列を、ハッシュ一致ごとに [`DuplicateGroup`] へ束ねる。
///
/// 2 件以上のグループのみ返す。代表は `"{label}:{hex}"`。
#[must_use]
pub fn group_by_hash(items: Vec<(FileEntry, FileHash)>, label: &str) -> Vec<DuplicateGroup> {
    group_by_key(items, |hash| format!("{label}:{}", to_hex(hash)))
}

/// `FileHash` を 16 進文字列へ（クレート内部ヘルパ。外部公開不要, L-2）。
#[must_use]
pub(crate) fn to_hex(hash: &FileHash) -> String {
    use std::fmt::Write;
    let bytes: &[u8] = match hash {
        FileHash::B256(a) => a,
        FileHash::B128(a) => a,
    };
    // 1 回だけ確保し追記する（バイト毎の `format!` 確保を避ける）。
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // String への write は失敗しない。
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileSize;
    use crate::path::LongPath;
    use std::time::SystemTime;

    fn entry(path: &str) -> FileEntry {
        FileEntry {
            path: LongPath::new(path).unwrap(),
            size: FileSize(1),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn groups_only_duplicates() {
        let h1 = FileHash::B128([1u8; 16]);
        let h2 = FileHash::B128([2u8; 16]);
        let items = vec![
            (entry("C:\\a"), h1.clone()),
            (entry("C:\\b"), h1.clone()),
            (entry("C:\\c"), h2.clone()),
        ];
        let groups = group_by_hash(items, "xxh3");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
        assert!(groups[0].representative.starts_with("xxh3:"));
    }

    #[test]
    fn group_by_key_groups_by_arbitrary_key() {
        let items = vec![
            (entry("C:\\a"), "k1".to_string()),
            (entry("C:\\b"), "k1".to_string()),
            (entry("C:\\c"), "k2".to_string()),
        ];
        let groups = group_by_key(items, |k| format!("meta:{k}"));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
        assert_eq!(groups[0].representative, "meta:k1");
    }

    #[test]
    fn hex_is_lowercase_and_full_length() {
        assert_eq!(to_hex(&FileHash::B128([0xab; 16])), "ab".repeat(16));
    }
}
