//! 重複検索結果の保存・読み込み（§3.2「結果保存」/「見直す」, U12 polish）。
//!
//! Slint 非依存の純関数。`DuplicateGroup` を JSON で永続化し、後から読み込んで結果として
//! 復元する。実体の再走査はせず保存内容を表示する（存在確認は「再チェック」で別途）。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use shredy_core::{DuplicateGroup, FileEntry, FileSize, LongPath};

/// 保存用ファイル DTO。`LongPath`/`SystemTime` を素直に直列化できる形へ落とす。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedFile {
    path: String,
    size: u64,
    /// UNIX epoch からの秒（負値・取得不能は 0）。
    modified_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedGroup {
    representative: String,
    files: Vec<SavedFile>,
}

/// 保存ファイル全体（将来の互換のためバージョンを持つ）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedList {
    version: u32,
    groups: Vec<SavedGroup>,
}

const SAVE_VERSION: u32 = 1;

fn to_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn from_secs(secs: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs.max(0) as u64)
}

/// 重複グループを JSON 文字列へ直列化する。
pub fn to_json(groups: &[DuplicateGroup]) -> Result<String, serde_json::Error> {
    let saved = SavedList {
        version: SAVE_VERSION,
        groups: groups
            .iter()
            .map(|g| SavedGroup {
                representative: g.representative.clone(),
                files: g
                    .files
                    .iter()
                    .map(|f| SavedFile {
                        path: f.path.as_path().to_string_lossy().into_owned(),
                        size: f.size.0,
                        modified_secs: to_secs(f.modified),
                    })
                    .collect(),
            })
            .collect(),
    };
    serde_json::to_string_pretty(&saved)
}

/// JSON 文字列から重複グループを復元する。`\\?\` 化できないパスを含む行は除外し、
/// 2 件未満になったグループも除外する（重複として表示する意味がないため）。
pub fn from_json(text: &str) -> Result<Vec<DuplicateGroup>, serde_json::Error> {
    let saved: SavedList = serde_json::from_str(text)?;
    let groups = saved
        .groups
        .into_iter()
        .filter_map(|g| {
            let files: Vec<FileEntry> = g
                .files
                .into_iter()
                .filter_map(|f| {
                    LongPath::new(&f.path).ok().map(|path| FileEntry {
                        path,
                        size: FileSize(f.size),
                        modified: from_secs(f.modified_secs),
                    })
                })
                .collect();
            if files.len() >= 2 {
                Some(DuplicateGroup {
                    representative: g.representative,
                    files,
                })
            } else {
                None
            }
        })
        .collect();
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(rep: &str, files: &[(&str, u64, i64)]) -> DuplicateGroup {
        DuplicateGroup {
            representative: rep.into(),
            files: files
                .iter()
                .map(|(p, s, m)| FileEntry {
                    path: LongPath::new(p).unwrap(),
                    size: FileSize(*s),
                    modified: from_secs(*m),
                })
                .collect(),
        }
    }

    #[test]
    fn roundtrips_groups_through_json() {
        let groups = vec![
            group("blake3:a", &[("C:\\a", 10, 100), ("C:\\b", 10, 200)]),
            group(
                "blake3:c",
                &[("C:\\c", 5, 1), ("C:\\d", 5, 2), ("C:\\e", 5, 3)],
            ),
        ];
        let json = to_json(&groups).unwrap();
        let back = from_json(&json).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].files.len(), 2);
        assert_eq!(back[1].files.len(), 3);
        assert_eq!(back[0].representative, "blake3:a");
        assert_eq!(back[0].files[0].path, LongPath::new("C:\\a").unwrap());
        assert_eq!(back[0].files[1].size.0, 10);
        assert_eq!(back[1].files[2].modified, from_secs(3));
    }

    #[test]
    fn drops_groups_with_fewer_than_two_files() {
        // 復元時に 1 件だけのグループは除外される。
        let json = r#"{"version":1,"groups":[{"representative":"x","files":[{"path":"C:\\only","size":1,"modified_secs":0}]}]}"#;
        let back = from_json(json).unwrap();
        assert!(back.is_empty());
    }

    #[test]
    fn invalid_json_is_error() {
        assert!(from_json("not json").is_err());
    }
}
