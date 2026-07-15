//! メタデータ比較（S4-6）。内容を読み込まずキー一致で束ねる。

use std::time::UNIX_EPOCH;

use crate::group::group_by_key;
use crate::io::FileReader;
use crate::model::{DuplicateGroup, FileEntry};
use crate::path::to_nfc;
use crate::progress::{Cancel, Phase, ProgressEvent, ProgressTx, is_cancelled};

use super::GroupingStrategy;

/// メタデータ比較の種別（仕様 §3.3 方式 4-6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataKind {
    /// 名前＋サイズ＋更新日時（S4）。
    NameSizeDate,
    /// 名前＋サイズ（S5）。
    NameSize,
    /// 名前のみ（S6, 大小無視）。
    NameOnly,
}

/// メタデータ比較（S4-6）。内容を読み込まずキー一致で束ねる。
pub struct MetadataStrategy {
    kind: MetadataKind,
}

/// メタデータ比較キー。名前は NFC 正規化 + 小文字化（仕様 §6 Unicode）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MetaKey {
    name: String,
    size: Option<u64>,
    date: Option<u64>,
}

impl MetadataStrategy {
    /// 指定種別の戦略を作る。
    #[must_use]
    pub fn new(kind: MetadataKind) -> Self {
        Self { kind }
    }

    fn name_of(e: &FileEntry) -> String {
        let name = e
            .path
            .as_path()
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        to_nfc(&name).to_lowercase()
    }

    fn key(&self, e: &FileEntry) -> MetaKey {
        let name = Self::name_of(e);
        match self.kind {
            MetadataKind::NameSizeDate => {
                let secs = e
                    .modified
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                MetaKey {
                    name,
                    size: Some(e.size.0),
                    date: Some(secs),
                }
            }
            MetadataKind::NameSize => MetaKey {
                name,
                size: Some(e.size.0),
                date: None,
            },
            MetadataKind::NameOnly => MetaKey {
                name,
                size: None,
                date: None,
            },
        }
    }

    /// 代表キー生成に使う英語プレフィックス（`label()` の日本語表示名とは別概念）。
    fn key_prefix(&self) -> &'static str {
        match self.kind {
            MetadataKind::NameSizeDate => "name-size-date",
            MetadataKind::NameSize => "name-size",
            MetadataKind::NameOnly => "name",
        }
    }
}

impl GroupingStrategy for MetadataStrategy {
    fn label(&self) -> &'static str {
        match self.kind {
            MetadataKind::NameSizeDate => "名前+サイズ+日付",
            MetadataKind::NameSize => "名前+サイズ",
            MetadataKind::NameOnly => "名前のみ",
        }
    }

    fn group(
        &self,
        entries: Vec<FileEntry>,
        _fs: &dyn FileReader,
        progress: &ProgressTx,
        cancel: &Cancel,
    ) -> Vec<DuplicateGroup> {
        let _ = progress.send(ProgressEvent::Phase(Phase::Comparing));
        if is_cancelled(cancel) {
            return Vec::new();
        }
        let label = self.key_prefix();
        let items: Vec<(FileEntry, MetaKey)> = entries
            .into_iter()
            .map(|e| {
                // key を先に算出してから e を move し、不要な FileEntry clone を避ける。
                let key = self.key(&e);
                (e, key)
            })
            .collect();
        group_by_key(items, move |k| {
            let size = k.size.map(|s| s.to_string()).unwrap_or_default();
            let date = k.date.map(|d| d.to_string()).unwrap_or_default();
            format!("{label}:{}|{size}|{date}", k.name)
        })
    }
}
