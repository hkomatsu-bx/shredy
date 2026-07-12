//! ビルド時にフルバージョン文字列 `MAJOR.MINOR.REVISION.BUILDNUMBER` を環境変数 `SHREDY_VERSION`
//! として注入する（仕様 §5.5 バージョン採番）。
//!
//! - `MAJOR.MINOR.REVISION`: Cargo.toml の version（`CARGO_PKG_VERSION_MAJOR` / `_MINOR` / `_PATCH`）。
//! - `BUILDNUMBER`: git のコミット数（`git rev-list --count HEAD`）。
//!   git が無い配布物（xcopy 展開物など）や非リポジトリでは `0` にフォールバックする。

use std::path::Path;
use std::process::Command;

fn main() {
    let major = std::env::var("CARGO_PKG_VERSION_MAJOR").unwrap_or_else(|_| "0".into());
    let minor = std::env::var("CARGO_PKG_VERSION_MINOR").unwrap_or_else(|_| "0".into());
    let revision = std::env::var("CARGO_PKG_VERSION_PATCH").unwrap_or_else(|_| "0".into());
    let build_number = git_commit_count().unwrap_or_else(|| "0".into());

    println!("cargo:rustc-env=SHREDY_VERSION={major}.{minor}.{revision}.{build_number}");

    // 新しいコミットで BUILDNUMBER が更新されるよう HEAD とコミットログを監視する（best-effort）。
    // 配布物（.git 不在）では存在チェックで握りつぶす。
    for marker in ["../../.git/HEAD", "../../.git/logs/HEAD"] {
        if Path::new(marker).exists() {
            println!("cargo:rerun-if-changed={marker}");
        }
    }
}

/// `git rev-list --count HEAD` でコミット数を取得する。git 不在・非リポジトリ時は `None`。
fn git_commit_count() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let trimmed = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
