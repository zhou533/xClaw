//! Time utility functions for the session subsystem.
//!
//! All functions operate on UTC. No external date/time dependency.

use crate::error::MemoryError;

/// Convert Unix epoch seconds to (year, month, day, hour, min, sec) UTC.
///
/// Uses the Gregorian calendar algorithm from
/// <https://howardhinnant.github.io/date_algorithms.html>.
pub(crate) fn epoch_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let sec = (secs % 60) as u32;
    let mins = secs / 60;
    let min = (mins % 60) as u32;
    let hours = mins / 60;
    let hour = (hours % 24) as u32;
    let days = hours / 24;

    // Civil date from Julian Day Number
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y } as u32;

    (year, month, day, hour, min, sec)
}

/// Convert (year, month, day, hour, min, sec) UTC to Unix epoch seconds.
///
/// Inverse of [`epoch_to_ymd_hms`]. Uses the same Hinnant algorithm in reverse.
pub(crate) fn ymd_hms_to_epoch(y: u32, m: u32, d: u32, h: u32, min: u32, sec: u32) -> u64 {
    // Adjust year for months Jan/Feb (they belong to the previous "era year").
    let y = if m <= 2 { y as u64 - 1 } else { y as u64 };
    let m = if m <= 2 { m as u64 + 9 } else { m as u64 - 3 };

    let era = y / 400;
    let yoe = y % 400;
    let doy = (153 * m + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    days * 86_400 + h as u64 * 3600 + min as u64 * 60 + sec as u64
}

/// Parse an ISO 8601 UTC timestamp (`YYYY-MM-DDThh:mm:ssZ`) to Unix epoch seconds.
pub(crate) fn parse_iso8601_to_epoch(s: &str) -> Result<u64, MemoryError> {
    // Expected format: "2026-03-28T10:00:00Z" (exactly 20 chars).
    let bytes = s.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return Err(MemoryError::TimeParse(format!(
            "invalid ISO 8601 format: {s}"
        )));
    }

    let year = parse_u32(&s[0..4])?;
    let month = parse_u32(&s[5..7])?;
    let day = parse_u32(&s[8..10])?;
    let hour = parse_u32(&s[11..13])?;
    let min = parse_u32(&s[14..16])?;
    let sec = parse_u32(&s[17..19])?;

    if year < 1970
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || min > 59
        || sec > 59
    {
        return Err(MemoryError::TimeParse(format!(
            "out-of-range component in: {s}"
        )));
    }

    Ok(ymd_hms_to_epoch(year, month, day, hour, min, sec))
}

/// Current UTC time as an ISO 8601 string (`YYYY-MM-DDThh:mm:ssZ`).
pub(crate) fn now_utc() -> String {
    let secs = now_epoch_secs();
    let (year, month, day, hour, min, sec) = epoch_to_ymd_hms(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, min, sec
    )
}

/// Current Unix epoch seconds (UTC).
pub(crate) fn now_epoch_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_u32(s: &str) -> Result<u32, MemoryError> {
    s.parse::<u32>()
        .map_err(|_| MemoryError::TimeParse(format!("invalid number: {s}")))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── epoch_to_ymd_hms ──

    #[test]
    fn epoch_zero_is_1970_01_01() {
        assert_eq!(epoch_to_ymd_hms(0), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn known_date_2026_03_30() {
        // 2026-03-30T12:30:45Z = ?
        let epoch = ymd_hms_to_epoch(2026, 3, 30, 12, 30, 45);
        assert_eq!(epoch_to_ymd_hms(epoch), (2026, 3, 30, 12, 30, 45));
    }

    // ── ymd_hms_to_epoch ──

    #[test]
    fn epoch_zero_roundtrip() {
        assert_eq!(ymd_hms_to_epoch(1970, 1, 1, 0, 0, 0), 0);
    }

    #[test]
    fn roundtrip_consistency() {
        // A selection of dates spanning different eras.
        let dates = [
            (1970, 1, 1, 0, 0, 0),
            (2000, 1, 1, 0, 0, 0),
            (2024, 2, 29, 23, 59, 59), // leap year
            (2026, 3, 30, 12, 30, 45),
            (2099, 12, 31, 23, 59, 59),
        ];
        for (y, m, d, h, mi, s) in dates {
            let epoch = ymd_hms_to_epoch(y, m, d, h, mi, s);
            let back = epoch_to_ymd_hms(epoch);
            assert_eq!(
                back,
                (y, m, d, h, mi, s),
                "roundtrip failed for {y}-{m}-{d}T{h}:{mi}:{s}"
            );
        }
    }

    #[test]
    fn leap_year_feb_29() {
        let epoch = ymd_hms_to_epoch(2024, 2, 29, 0, 0, 0);
        assert_eq!(epoch_to_ymd_hms(epoch), (2024, 2, 29, 0, 0, 0));
    }

    #[test]
    fn non_leap_year_mar_01_follows_feb_28() {
        let feb28 = ymd_hms_to_epoch(2025, 2, 28, 23, 59, 59);
        let mar01 = ymd_hms_to_epoch(2025, 3, 1, 0, 0, 0);
        assert_eq!(mar01 - feb28, 1); // exactly 1 second apart
    }

    // ── parse_iso8601_to_epoch ──

    #[test]
    fn parse_valid_iso8601() {
        let epoch = parse_iso8601_to_epoch("2026-03-30T12:30:45Z").unwrap();
        assert_eq!(epoch_to_ymd_hms(epoch), (2026, 3, 30, 12, 30, 45));
    }

    #[test]
    fn parse_epoch_zero() {
        let epoch = parse_iso8601_to_epoch("1970-01-01T00:00:00Z").unwrap();
        assert_eq!(epoch, 0);
    }

    #[test]
    fn parse_invalid_format_returns_error() {
        assert!(parse_iso8601_to_epoch("not-a-date").is_err());
        assert!(parse_iso8601_to_epoch("2026-03-30 12:30:45").is_err()); // space instead of T
        assert!(parse_iso8601_to_epoch("2026-03-30T12:30:45").is_err()); // missing Z
    }

    #[test]
    fn parse_out_of_range_month_returns_error() {
        assert!(parse_iso8601_to_epoch("2026-13-01T00:00:00Z").is_err());
        assert!(parse_iso8601_to_epoch("2026-00-01T00:00:00Z").is_err());
    }

    #[test]
    fn parse_pre_1970_returns_error() {
        assert!(parse_iso8601_to_epoch("1969-12-31T23:59:59Z").is_err());
    }

    #[test]
    fn parse_out_of_range_day_returns_error() {
        assert!(parse_iso8601_to_epoch("2026-03-00T00:00:00Z").is_err());
        assert!(parse_iso8601_to_epoch("2026-03-32T00:00:00Z").is_err());
    }

    // ── now_utc / now_epoch_secs ──

    #[test]
    fn now_utc_format_is_valid_iso8601() {
        let s = now_utc();
        assert_eq!(s.len(), 20);
        assert!(s.ends_with('Z'));
        // Should be parseable by our own parser.
        parse_iso8601_to_epoch(&s).unwrap();
    }

    #[test]
    fn now_epoch_secs_is_recent() {
        let secs = now_epoch_secs();
        // Should be after 2025-01-01 (epoch ~1735689600).
        assert!(secs > 1_735_000_000);
    }
}
