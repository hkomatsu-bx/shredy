//! ファイル I/O 抽象（設計 Q3=A, ガイド §11）。
//!
//! コアを実ファイルシステムから切り離す `FileReader` トレイトと、実装
//! `RealFs`・テスト用 `MockFs` を提供する。これにより UNC/権限/使用中などの
//! 分岐をモックで単体テストできる。

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::error::{CoreError, Result};
use crate::model::FileSize;
use crate::path::LongPath;

/// ファイルのメタデータ（コアが必要とする最小集合）。
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub size: FileSize,
    pub modified: SystemTime,
    pub is_reparse: bool,
    /// 読み取り専用属性（§3.6 削除可否判定 `DeleteReadOnly` に使う）。
    pub is_read_only: bool,
    /// ディレクトリか（§3.9.1 空フォルダ削除の判定。ファイル用の使用中チェックを回避する）。
    pub is_dir: bool,
}

/// コアが依存する I/O 抽象。実装を差し替えてテスト可能にする。
///
/// 比較フェーズで `rayon` 並列から共有するため `Send + Sync` を要求する。
pub trait FileReader: Send + Sync {
    /// メタデータを取得する（シンボリックリンク自体の情報）。
    fn metadata(&self, path: &LongPath) -> Result<FileMeta>;

    /// 先頭 `len` バイトを読み込む（2 段階ハッシュの部分読み用）。
    fn read_prefix(&self, path: &LongPath, len: u64) -> Result<Vec<u8>>;

    /// `offset` から最大 `len` バイトを読み込む（部分一致 S3 用, 仕様 §3.3）。
    ///
    /// `offset` がファイル長を超える場合は空を返す。読み込めるのは
    /// `offset..min(offset+len, ファイル長)` の範囲。
    fn read_range(&self, path: &LongPath, offset: u64, len: u64) -> Result<Vec<u8>>;

    /// 読み込み用ストリームを開く（全体ハッシュのストリーミング用）。
    fn open_read(&self, path: &LongPath) -> Result<Box<dyn Read + '_>>;

    /// リパースポイント（ディレクトリのシンボリックリンク/ジャンクション）か。
    fn is_reparse_point(&self, path: &LongPath) -> Result<bool>;

    /// 他プロセスが使用中か。
    fn is_in_use(&self, path: &LongPath) -> Result<bool>;
}

/// ファイル全体を上限 `cap` バイトまで読み込む（zip/cfb・画像デコーダへ全バイトを
/// 渡すため）。Office（§3.7）と画像類似度（§3.12）で重複していた実装の単一ソース。
pub(crate) fn read_all_capped(path: &LongPath, fs: &dyn FileReader, cap: u64) -> Result<Vec<u8>> {
    let reader = fs.open_read(path)?;
    let mut buf = Vec::new();
    reader
        .take(cap)
        .read_to_end(&mut buf)
        .map_err(|e| CoreError::io(path.as_path(), e))?;
    Ok(buf)
}

/// 実ファイルシステム実装（std + shredy-sys）。
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl FileReader for RealFs {
    fn metadata(&self, path: &LongPath) -> Result<FileMeta> {
        // 安全注意: `symlink_metadata`（リンクを追跡しない）を使うこと。`metadata` に
        // 変えるとリパースポイント（ジャンクション/シンボリックリンク）の実体を辿り、
        // 削除時に実体を巻き込む致命的バグになる（仕様 §安全優先, §3.6）。
        let fs_path = path.fs_path();
        let md = std::fs::symlink_metadata(fs_path.as_ref())
            .map_err(|e| CoreError::io(path.as_path(), e))?;
        let modified = md
            .modified()
            .map_err(|e| CoreError::io(path.as_path(), e))?;
        let is_reparse = reparse_flag(&md);
        Ok(FileMeta {
            size: FileSize(md.len()),
            modified,
            is_reparse,
            is_read_only: md.permissions().readonly(),
            is_dir: md.file_type().is_dir(),
        })
    }

    fn read_prefix(&self, path: &LongPath, len: u64) -> Result<Vec<u8>> {
        let file =
            std::fs::File::open(path.fs_path()).map_err(|e| CoreError::io(path.as_path(), e))?;
        // 実ファイルサイズと要求長の小さい方で事前確保し、`read_to_end` の再確保を避ける
        // （`Take<File>` はサイズヒントを持たないため自前で見積もる）。
        let cap = file.metadata().map(|m| m.len().min(len)).unwrap_or(len);
        let mut buf = Vec::with_capacity(cap as usize);
        file.take(len)
            .read_to_end(&mut buf)
            .map_err(|e| CoreError::io(path.as_path(), e))?;
        Ok(buf)
    }

    fn read_range(&self, path: &LongPath, offset: u64, len: u64) -> Result<Vec<u8>> {
        use std::io::{Seek, SeekFrom};
        let mut file =
            std::fs::File::open(path.fs_path()).map_err(|e| CoreError::io(path.as_path(), e))?;
        // offset 以降の残量と要求長の小さい方で事前確保する。
        let cap = file
            .metadata()
            .map(|m| m.len().saturating_sub(offset).min(len))
            .unwrap_or(len);
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| CoreError::io(path.as_path(), e))?;
        }
        let mut buf = Vec::with_capacity(cap as usize);
        file.take(len)
            .read_to_end(&mut buf)
            .map_err(|e| CoreError::io(path.as_path(), e))?;
        Ok(buf)
    }

    fn open_read(&self, path: &LongPath) -> Result<Box<dyn Read + '_>> {
        let file =
            std::fs::File::open(path.fs_path()).map_err(|e| CoreError::io(path.as_path(), e))?;
        Ok(Box::new(file))
    }

    fn is_reparse_point(&self, path: &LongPath) -> Result<bool> {
        Ok(self.metadata(path)?.is_reparse)
    }

    fn is_in_use(&self, path: &LongPath) -> Result<bool> {
        shredy_sys::is_file_in_use(path.fs_path().as_ref())
            .map_err(|e| CoreError::io(path.as_path(), e))
    }
}

#[cfg(windows)]
fn reparse_flag(md: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    shredy_sys::is_reparse_point(md.file_attributes())
}

#[cfg(not(windows))]
fn reparse_flag(md: &std::fs::Metadata) -> bool {
    md.file_type().is_symlink()
}

/// テスト用のインメモリ実装。パスごとにメタ・内容・使用中状態を注入できる。
#[derive(Debug, Default, Clone)]
pub struct MockFs {
    entries: HashMap<PathBuf, MockEntry>,
}

#[derive(Debug, Clone)]
struct MockEntry {
    content: Vec<u8>,
    modified: SystemTime,
    is_reparse: bool,
    in_use: bool,
    read_only: bool,
    is_dir: bool,
}

impl MockFs {
    /// 空の `MockFs` を作る。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 通常ファイルを登録する。
    pub fn add_file(&mut self, path: &LongPath, content: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.insert(
            path.as_path().to_path_buf(),
            MockEntry {
                content: content.into(),
                modified: SystemTime::UNIX_EPOCH,
                is_reparse: false,
                in_use: false,
                read_only: false,
                is_dir: false,
            },
        );
        self
    }

    /// 使用中フラグを設定する。
    pub fn set_in_use(&mut self, path: &LongPath, in_use: bool) -> &mut Self {
        if let Some(e) = self.entries.get_mut(path.as_path()) {
            e.in_use = in_use;
        }
        self
    }

    /// 読み取り専用フラグを設定する（`DeleteReadOnly` のテスト用）。
    pub fn set_read_only(&mut self, path: &LongPath, read_only: bool) -> &mut Self {
        if let Some(e) = self.entries.get_mut(path.as_path()) {
            e.read_only = read_only;
        }
        self
    }

    /// リパースポイントとして登録する。
    pub fn add_reparse_point(&mut self, path: &LongPath) -> &mut Self {
        self.entries.insert(
            path.as_path().to_path_buf(),
            MockEntry {
                content: Vec::new(),
                modified: SystemTime::UNIX_EPOCH,
                is_reparse: true,
                in_use: false,
                read_only: false,
                is_dir: false,
            },
        );
        self
    }

    fn get(&self, path: &LongPath) -> Result<&MockEntry> {
        self.entries.get(path.as_path()).ok_or_else(|| {
            CoreError::io(
                path.as_path(),
                std::io::Error::from(std::io::ErrorKind::NotFound),
            )
        })
    }
}

impl FileReader for MockFs {
    fn metadata(&self, path: &LongPath) -> Result<FileMeta> {
        let e = self.get(path)?;
        Ok(FileMeta {
            size: FileSize(e.content.len() as u64),
            modified: e.modified,
            is_reparse: e.is_reparse,
            is_read_only: e.read_only,
            is_dir: e.is_dir,
        })
    }

    fn read_prefix(&self, path: &LongPath, len: u64) -> Result<Vec<u8>> {
        let e = self.get(path)?;
        let n = (len as usize).min(e.content.len());
        Ok(e.content[..n].to_vec())
    }

    fn read_range(&self, path: &LongPath, offset: u64, len: u64) -> Result<Vec<u8>> {
        let e = self.get(path)?;
        let start = (offset as usize).min(e.content.len());
        let end = start.saturating_add(len as usize).min(e.content.len());
        Ok(e.content[start..end].to_vec())
    }

    fn open_read(&self, path: &LongPath) -> Result<Box<dyn Read + '_>> {
        let e = self.get(path)?;
        // 内容はクローンせず借用する（`'_` は `&self` に紐づく）。これによりベンチ
        // とテストが純粋なハッシュ処理を測れる（不要な memcpy を排除）。
        Ok(Box::new(std::io::Cursor::new(e.content.as_slice())))
    }

    fn is_reparse_point(&self, path: &LongPath) -> Result<bool> {
        Ok(self.get(path)?.is_reparse)
    }

    fn is_in_use(&self, path: &LongPath) -> Result<bool> {
        Ok(self.get(path)?.in_use)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lp(s: &str) -> LongPath {
        LongPath::new(s).unwrap()
    }

    #[test]
    fn mock_returns_not_found_for_unknown_path() {
        let fs = MockFs::new();
        let err = fs.metadata(&lp("C:\\missing")).unwrap_err();
        assert!(matches!(err, CoreError::Io { .. }));
    }

    #[test]
    fn mock_read_prefix_truncates_to_requested_len() {
        let mut fs = MockFs::new();
        let p = lp("C:\\a.bin");
        fs.add_file(&p, vec![1u8, 2, 3, 4, 5]);
        assert_eq!(fs.read_prefix(&p, 3).unwrap(), vec![1, 2, 3]);
        assert_eq!(fs.read_prefix(&p, 100).unwrap(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn mock_read_range_clamps_offset_and_len() {
        let mut fs = MockFs::new();
        let p = lp("C:\\a.bin");
        fs.add_file(&p, vec![10u8, 11, 12, 13, 14]);
        // offset+len が範囲内。
        assert_eq!(fs.read_range(&p, 1, 3).unwrap(), vec![11, 12, 13]);
        // len が末尾を超える → 末尾までにクランプ。
        assert_eq!(fs.read_range(&p, 3, 100).unwrap(), vec![13, 14]);
        // offset がファイル長以上 → 空。
        assert!(fs.read_range(&p, 10, 5).unwrap().is_empty());
        // offset=0 は read_prefix と一致。
        assert_eq!(
            fs.read_range(&p, 0, 2).unwrap(),
            fs.read_prefix(&p, 2).unwrap()
        );
    }

    #[test]
    fn real_fs_read_range_matches_mock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("r.bin");
        std::fs::write(&path, [0u8, 1, 2, 3, 4, 5, 6, 7]).unwrap();
        let fs = RealFs;
        let lp = LongPath::new(&path).unwrap();
        assert_eq!(fs.read_range(&lp, 2, 3).unwrap(), vec![2, 3, 4]);
        assert_eq!(fs.read_range(&lp, 6, 100).unwrap(), vec![6, 7]);
        assert!(fs.read_range(&lp, 20, 4).unwrap().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn real_fs_handles_path_over_max_path() {
        // 260 字超のパスでも RealFs が stat/read できることを検証する（C2: `\\?\` 適用）。
        // `as_path()` のままでは MAX_PATH 制限で失敗する経路。
        let dir = tempfile::tempdir().unwrap();
        let seg = "d".repeat(50);
        let mut deep = dir.path().to_path_buf();
        for _ in 0..6 {
            deep.push(&seg); // 合計 300 字超
        }
        let deep_lp = LongPath::new(&deep).unwrap();
        // 作成・書き込みも `\\?\`（fs_path）経由でないと 260 字制限に掛かる。
        std::fs::create_dir_all(deep_lp.fs_path()).unwrap();
        let file_lp = LongPath::new(deep.join("f.bin")).unwrap();
        std::fs::write(file_lp.fs_path(), b"hello").unwrap();

        assert!(
            file_lp.as_path().to_string_lossy().chars().count() > 260,
            "テスト用パスが 260 字を超えていない"
        );
        let fs = RealFs;
        assert_eq!(fs.metadata(&file_lp).unwrap().size.0, 5);
        assert_eq!(fs.read_prefix(&file_lp, 100).unwrap(), b"hello");
        assert!(!fs.is_in_use(&file_lp).unwrap());
    }

    #[test]
    fn mock_reports_in_use_and_reparse() {
        let mut fs = MockFs::new();
        let f = lp("C:\\locked");
        fs.add_file(&f, b"x".to_vec());
        fs.set_in_use(&f, true);
        assert!(fs.is_in_use(&f).unwrap());

        let r = lp("C:\\junction");
        fs.add_reparse_point(&r);
        assert!(fs.is_reparse_point(&r).unwrap());
    }
}
