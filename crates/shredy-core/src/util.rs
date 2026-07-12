//! フロントエンド横断の小さなユーティリティ（時刻・日付フォーマット）。

use std::time::{SystemTime, UNIX_EPOCH};

/// 現在時刻を UNIX 秒で返す（システム時刻が UNIX エポック以前など取得失敗時は 0）。
///
/// 削除履歴ログ・動作ログのタイムスタンプなど、フロントエンド（CLI/GUI）が共通で使う。
#[must_use]
pub fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// UNIX 秒を `YYYY-MM-DD HH:MM:SS`（UTC）へ整形する。ログのタイムスタンプに用いる。
#[must_use]
pub(crate) fn format_datetime(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let tod = secs.rem_euclid(86_400);
    let (h, min, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}:{s:02}")
}

/// UNIX 秒をローカル時刻 `YYYY-MM-DD HH:MM:SS` へ整形する（削除履歴ログ等）。
///
/// タイムゾーンオフセットは [`shredy_sys::local_utc_offset_secs`] から取得する（仕様 §5.5）。
#[must_use]
pub(crate) fn format_datetime_local(secs: i64) -> String {
    format_datetime(secs + i64::from(shredy_sys::local_utc_offset_secs()))
}

/// 現在のローカル時刻を `YYYY-MM-DD HH:MM:SS.mmm`（ミリ秒付き）で返す（tracing タイマ向け）。
#[must_use]
pub(crate) fn format_local_now_millis() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64 + i64::from(shredy_sys::local_utc_offset_secs());
    let millis = now.subsec_millis();
    format!("{}.{millis:03}", format_datetime(secs))
}

/// UNIX 秒を `YYYYMMDD`（UTC）の日付スタンプへ整形する。
#[must_use]
pub(crate) fn date_stamp(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}{m:02}{d:02}")
}

// Howard Hinnant の暦アルゴリズム（pure, テスト可能）。エポック日数 → (年, 月, 日)。
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // civil_from_days の逆変換（テスト専用：プロダクションでは day→civil のみ使う）。
    fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
        let doy = (153 * mp + 2) / 5 + d as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    #[test]
    fn date_stamp_epoch_is_19700101() {
        assert_eq!(date_stamp(0), "19700101");
    }

    #[test]
    fn format_datetime_epoch_is_midnight() {
        assert_eq!(format_datetime(0), "1970-01-01 00:00:00");
    }

    proptest! {
        // 暦変換の round-trip（days → (y,m,d) → days）は恒等。
        #[test]
        fn civil_round_trip(days in -50_000i64..50_000) {
            let (y, m, d) = civil_from_days(days);
            prop_assert_eq!(days_from_civil(y, m, d), days);
        }
    }
}
