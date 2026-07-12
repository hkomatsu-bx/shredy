//! `DuplicateGroup` / `FileEntry` → 表示用文字列への純変換ヘルパ（設計 §6）。
//!
//! Slint 生成型へ依存しない純関数のみを置き、単体テスト可能にする。`view.rs` が
//! これらを呼んで平坦化済みの `ListRow`（ヘッダ＋ファイル行）を **1 パスで** 直接構築する
//! （中間表現を挟まず、毎 `refresh_ui` の二重アロケーションを避ける）。

use std::time::SystemTime;

use shredy_core::{DuplicateGroup, FileEntry, KeepFileRule};

use crate::bridge::summary;

/// グループヘッダ文字列（最短ファイル名 · N 件重複 · 削除可サイズ, U12 M7）。
///
/// 表示名はグループ内で最短のファイル名（同名なら任意の 1 件）。削除可サイズは
/// 「1つ残す」適用後に削除できる合計（[`summary::group_deletable`]）。
#[must_use]
pub fn group_header(group: &DuplicateGroup, rule: KeepFileRule) -> String {
    let count = group.files.len();
    let title = group
        .files
        .iter()
        .map(|f| folder_and_name(f).1)
        .min_by_key(|n| n.chars().count())
        .unwrap_or_default();
    let deletable = format_size(summary::group_deletable(group, rule));
    format!("{title} · {count} 件重複 · {deletable} 削除可")
}

/// `SystemTime` をローカル時刻の `yyyy/mm/dd HH:MM:SS` へ整形する（U12 M7 / polish2）。
///
/// chrono の `From<SystemTime>` はパニックせず常に値を返す（UNIX epoch 以前など範囲外は
/// クランプされる）。したがって本関数も常に 19 桁の日時文字列を返す。
#[must_use]
pub fn format_modified(modified: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Local> = modified.into();
    dt.format("%Y/%m/%d %H:%M:%S").to_string()
}

/// ファイルの (フォルダ, ファイル名) を表示用文字列で返す。
#[must_use]
pub fn folder_and_name(entry: &FileEntry) -> (String, String) {
    let path = entry.path.as_path();
    let folder = path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    (folder, name)
}

/// ファイルのフルパス表示文字列。
#[must_use]
pub fn path_string(entry: &FileEntry) -> String {
    entry.path.as_path().to_string_lossy().into_owned()
}

const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

/// バイト数を人間可読な文字列へ（2 進接頭辞）。0 は `"0 B"`。
#[must_use]
pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    // f64 は 2^53 超で精度を失うが、現実的なファイルサイズ（最大 TB 台）は十分その範囲内。
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shredy_core::{FileSize, LongPath};
    use std::time::SystemTime;

    fn entry(path: &str, size: u64) -> FileEntry {
        FileEntry {
            path: LongPath::new(path).unwrap(),
            size: FileSize(size),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    fn group(paths: &[(&str, u64)]) -> DuplicateGroup {
        DuplicateGroup {
            representative: "blake3:abc".into(),
            files: paths.iter().map(|(p, s)| entry(p, *s)).collect(),
        }
    }

    #[test]
    fn splits_folder_and_name() {
        let (folder, name) = folder_and_name(&entry("C:\\dir\\sub\\file.txt", 10));
        assert_eq!(name, "file.txt");
        assert_eq!(folder, "C:\\dir\\sub");
    }

    #[test]
    fn path_string_returns_full_path() {
        assert_eq!(path_string(&entry("C:\\dir\\a.txt", 1)), "C:\\dir\\a.txt");
    }

    #[test]
    fn formats_zero_size_as_zero_bytes() {
        assert_eq!(format_size(0), "0 B");
    }

    #[test]
    fn formats_sizes_with_binary_units() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn header_includes_shortest_name_count_and_deletable() {
        // 2 件・各 2048。ByDateNewest で 1 件残し 2048 削除可。最短名は "b.txt"。
        let header = group_header(
            &group(&[("C:\\dir\\longer_name.txt", 2048), ("C:\\dir\\b.txt", 2048)]),
            KeepFileRule::ByDateNewest,
        );
        assert!(header.contains("b.txt"));
        assert!(header.contains("2 件重複"));
        assert!(header.contains("2.0 KB 削除可"));
    }

    #[test]
    fn header_of_empty_group_is_zero() {
        let g = DuplicateGroup {
            representative: "blake3:x".into(),
            files: Vec::new(),
        };
        let header = group_header(&g, KeepFileRule::ByDateNewest);
        assert!(header.contains("0 件重複"));
        assert!(header.contains("0 B 削除可"));
    }

    #[test]
    fn format_modified_produces_datetime_shape() {
        let s =
            format_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000));
        // ローカル TZ 依存だが書式は yyyy/mm/dd HH:MM:SS 固定。
        assert_eq!(s.len(), 19);
        assert_eq!(&s[4..5], "/");
        assert_eq!(&s[13..14], ":");
    }
}
