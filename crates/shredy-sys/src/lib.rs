//! Thin Windows interop helpers for Shredy.
//!
//! 低レベル Win32 連携を本 crate に局所化する（Effective Rust Item 16「unsafe を最小化」）。
//! リパースポイント判定・ファイルロック検出は `std::os::windows` で完結し、タイムゾーン
//! オフセット取得のみ `windows-sys` 経由で `GetTimeZoneInformation` を `unsafe` で呼ぶ。
//! `unsafe` ブロックには個別に `// SAFETY:` で不変条件を明記する。

#![deny(unsafe_op_in_unsafe_fn)]

use std::path::Path;

/// Win32 の `FILE_ATTRIBUTE_REPARSE_POINT`。
pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Returns true if the given Win32 file-attribute bitmask marks a reparse point
/// (directory symlink / junction).
#[must_use]
pub fn is_reparse_point(attrs: u32) -> bool {
    attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// Detects whether `path` is currently in use by another process.
///
/// Windows では共有モード 0（排他）で開き、共有違反が返ればロック中と判定する。
///
/// # Errors
/// 共有違反（`ERROR_SHARING_VIOLATION` / `ERROR_LOCK_VIOLATION`）以外の OS エラー
/// （アクセス拒否・パス不在など）は判定不能としてそのまま `Err` で伝播する。
#[cfg(windows)]
pub fn is_file_in_use(path: &Path) -> std::io::Result<bool> {
    use std::os::windows::fs::OpenOptionsExt;

    // share_mode(0) = no sharing. 既に開かれていれば ERROR_SHARING_VIOLATION (32)
    // または ERROR_LOCK_VIOLATION (33) になる。
    match std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
    {
        Ok(_) => Ok(false),
        Err(e) => match e.raw_os_error() {
            Some(32) | Some(33) => Ok(true),
            _ => Err(e),
        },
    }
}

/// Non-Windows フォールバック（本プロジェクトは Windows 専用だが、
/// ロジックのクロスビルド/テストのために提供する）。
#[cfg(not(windows))]
pub fn is_file_in_use(_path: &Path) -> std::io::Result<bool> {
    Ok(false)
}

/// 現在のローカルタイムゾーンの UTC オフセット（秒）。`local = utc + offset` が成り立つ。
///
/// Windows の `GetTimeZoneInformation` を用い、夏時間適用中は daylight bias を加味する。
/// 取得に失敗した場合は 0（UTC 扱い）を返す。
#[cfg(windows)]
#[must_use]
pub fn local_utc_offset_secs() -> i32 {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

    // 戻り値コード（winbase.h）: 0=UNKNOWN, 1=STANDARD, 2=DAYLIGHT, 0xFFFFFFFF=INVALID。
    const TIME_ZONE_ID_INVALID: u32 = 0xFFFF_FFFF;
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;

    // SAFETY: TIME_ZONE_INFORMATION は整数フィールドのみで構成され、全ゼロが有効な初期
    // 状態（不正なビットパターンを生まない）ため zeroed での初期化は健全。
    let mut tz: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: &mut tz は初期化済みの有効な構造体を指す。Win32 は当該領域のみ書き込み別名
    // 参照を保持しない。書き込み内容の有効性は戻り値 ret で判定する。
    let ret = unsafe { GetTimeZoneInformation(&mut tz) };
    if ret == TIME_ZONE_ID_INVALID {
        return 0;
    }
    // Bias は「UTC = local + bias（分）」。夏時間中は DaylightBias、それ以外（STANDARD/UNKNOWN）は
    // StandardBias を加算。UNKNOWN は DST 状態不明だが StandardBias（ほぼ全 TZ で 0）で代替する。
    let extra = if ret == TIME_ZONE_ID_DAYLIGHT {
        tz.DaylightBias
    } else {
        tz.StandardBias
    };
    -(tz.Bias + extra) * 60
}

/// Non-Windows フォールバック（UTC 扱い）。
#[cfg(not(windows))]
#[must_use]
pub fn local_utc_offset_secs() -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reparse_bit_set_is_detected() {
        // Arrange / Act / Assert
        assert!(is_reparse_point(FILE_ATTRIBUTE_REPARSE_POINT));
        assert!(is_reparse_point(0x0010 | FILE_ATTRIBUTE_REPARSE_POINT));
    }

    #[test]
    fn reparse_bit_absent_is_not_detected() {
        assert!(!is_reparse_point(0));
        assert!(!is_reparse_point(0x0010)); // FILE_ATTRIBUTE_DIRECTORY のみ
    }
}
