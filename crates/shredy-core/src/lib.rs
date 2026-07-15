//! `shredy-core` — Shredy のコアエンジン。
//!
//! 重複・類似ファイルの検索（走査・ハッシュ比較・グルーピング）、選択、安全な削除、
//! 設定・キャッシュの永続化を提供する純 Rust ライブラリ。フロントエンド（CLI/GUI）から
//! 利用される。
//!
//! U1（ワークスペース基盤）では横断基盤モジュールを提供する：
//! [`error`]・[`model`]・[`path`]・[`io`]・[`progress`]・[`logging`]。
//! 走査・比較・選択・削除は後続ユニットで追加する。

#![deny(unsafe_op_in_unsafe_fn)]

/// アプリケーションのフルバージョン文字列（`MAJOR.MINOR.REVISION.BUILDNUMBER`、例: `0.12.0.106`）。
///
/// `MAJOR.MINOR.REVISION` は Cargo.toml、`BUILDNUMBER` はビルド時の git コミット数（`build.rs` が
/// `SHREDY_VERSION` として注入。git 不在の配布物では `0`）。CLI `--version`・GUI 表示・
/// 起動ログはすべてこの値を唯一の情報源とする（バージョンの二重管理を避ける）。
pub const VERSION: &str = env!("SHREDY_VERSION");

/// [`VERSION`] を返す。
pub fn version() -> &'static str {
    VERSION
}

pub mod cache;
pub mod cache_io;
pub mod compare;
pub mod config;
pub mod delete;
pub mod engine;
pub mod error;
pub mod exif;
pub mod folder_diff;
pub mod group;
pub mod image_cleanup;
pub mod imgsim;
pub mod io;
pub mod logging;
pub mod metadata;
pub mod model;
pub mod office;
pub mod path;
pub mod progress;
pub mod rename;
pub mod scan;
pub mod select;
pub mod strategy;
pub mod thumbnail;
pub mod util;
pub mod utility;

// 主要な公開型を re-export（Item 24: API に現れる型を入口で提供）。
pub use cache::{CacheKey, HashCache};
pub use cache_io::{CACHE_FILE_NAME, load_hash_cache, save_hash_cache};
pub use compare::{Comparator, HashComparator};
pub use config::OfficeSimilarityLevel;
pub use config::{
    AppConfig, AppTheme, CompareMethod, FileSizeUnit, LogLevelConfig, ScanConfig, ScanConfigBuilder,
};
pub use delete::{
    DeleteLog, DeleteMode, DeletePlan, DeleteReport, Deleter, ExecListWriter, FileOp,
};
pub use engine::SearchEngine;
pub use error::{CoreError, Result};
pub use exif::{ExifDateFormat, ExifInfo};
pub use folder_diff::{
    DiffEntry, DiffKind, FolderDiffResult, FolderDiffScanner, FolderDiffSummary,
};
pub use image_cleanup::{
    StripOptions, StripOutcome, StripSummary, ext_supports_exif_removal, scan_exif_candidates,
    strip_image_metadata, strip_many,
};
pub use imgsim::ImageSimilarityStrategy;
pub use io::{FileMeta, FileReader, MockFs, RealFs};
pub use logging::{LogReloadHandle, init_logging};
pub use metadata::{FormatMetadata, ext_supports_metadata, extract_metadata};
pub use model::{DuplicateGroup, FileEntry, FileHash, FileSize, KeepFileRule, Method, PHash};
pub use office::OfficeSimilarityStrategy;
pub use path::LongPath;
pub use progress::{Cancel, Phase, ProgressEvent, ProgressTx};
pub use rename::{
    CaseConvert, CaseTarget, RecipeStep, RenameEntry, RenameIssue, RenameMode, RenamePlan,
    RenameRecipe, RenameReport, RenameStep, RenameTargets, RenameUndo, SeqPosition, UndoReport,
    build_plan, execute_rename, parse_filters,
};
pub use scan::Scanner;
pub use select::{LockSet, Selection, Selector};
pub use strategy::{
    ContentHashStrategy, GroupingStrategy, MetadataKind, MetadataStrategy, PartialMatchStrategy,
    content_hash_method, make_strategy,
};
pub use thumbnail::{
    find_system_thumbcaches, find_thumbnail_cache_items, find_thumbnail_caches, find_thumbs_db,
    system_thumbcache_dir,
};
pub use util::now_unix_secs;
pub use utility::{
    TempCategory, UtilityConfig, UtilityItem, UtilityKind, UtilityScanner, classify_temp,
};
