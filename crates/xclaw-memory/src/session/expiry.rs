//! Session expiry logic — pure functions for testability.

use crate::error::MemoryError;
use crate::session::policy::SessionPolicy;
use crate::session::time_util::{epoch_to_ymd_hms, parse_iso8601_to_epoch, ymd_hms_to_epoch};

/// Determine whether a session entry is expired under the given policy.
///
/// `updated_at` is an ISO 8601 UTC string. `now` is Unix epoch seconds.
/// Returns `true` if the session should be renewed.
///
/// **Daily reset**: if the most recent reset boundary (today's or yesterday's
/// `reset_at_hour`) falls between `updated_at` and `now`, the session is expired.
///
/// **Idle timeout**: if `now - updated_at > idle_minutes * 60`, expired.
///
/// The two policies are OR — either triggers expiry.
pub(crate) fn is_expired(
    updated_at: &str,
    now_epoch_secs: u64,
    policy: &SessionPolicy,
) -> Result<bool, MemoryError> {
    let updated_epoch = parse_iso8601_to_epoch(updated_at)?;

    // Daily reset check.
    let daily_expired = is_daily_expired(updated_epoch, now_epoch_secs, policy.reset_at_hour);

    // Idle check.
    let idle_expired = match policy.idle_minutes {
        Some(mins) => {
            let threshold = now_epoch_secs.saturating_sub(mins.saturating_mul(60));
            updated_epoch < threshold
        }
        None => false,
    };

    Ok(daily_expired || idle_expired)
}

/// Check daily reset expiry.
///
/// Algorithm:
/// 1. Compute today's reset point = today 00:00 UTC + reset_at_hour.
/// 2. If `now >= reset_point` and `updated < reset_point` → expired.
/// 3. If `now < reset_point` (haven't reached today's reset yet),
///    use yesterday's reset point instead.
fn is_daily_expired(updated_epoch: u64, now_epoch: u64, reset_at_hour: u8) -> bool {
    let (y, m, d, _, _, _) = epoch_to_ymd_hms(now_epoch);
    let today_reset = ymd_hms_to_epoch(y, m, d, reset_at_hour as u32, 0, 0);

    if now_epoch >= today_reset {
        // Past today's reset: expired if updated before it.
        updated_epoch < today_reset
    } else {
        // Before today's reset: use yesterday's reset point.
        let yesterday_reset = today_reset.saturating_sub(86_400);
        updated_epoch < yesterday_reset
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(hour: u8, idle: Option<u64>) -> SessionPolicy {
        SessionPolicy {
            reset_at_hour: hour,
            idle_minutes: idle,
        }
    }

    // ── Daily expiry ──

    #[test]
    fn daily_updated_before_reset_and_now_after_reset_is_expired() {
        // reset_at_hour = 4. updated at 03:00, now at 05:00 same day.
        let updated = "2026-03-30T03:00:00Z";
        let now = ymd_hms_to_epoch(2026, 3, 30, 5, 0, 0);
        assert!(is_expired(updated, now, &policy(4, None)).unwrap());
    }

    #[test]
    fn daily_updated_after_reset_same_day_is_not_expired() {
        // reset_at_hour = 4. updated at 05:00, now at 10:00 same day.
        let updated = "2026-03-30T05:00:00Z";
        let now = ymd_hms_to_epoch(2026, 3, 30, 10, 0, 0);
        assert!(!is_expired(updated, now, &policy(4, None)).unwrap());
    }

    #[test]
    fn daily_cross_midnight_now_before_reset_updated_yesterday_after_reset() {
        // reset_at_hour = 4. updated at 2026-03-29T05:00:00Z, now at 2026-03-30T03:00:00Z.
        // Yesterday's reset = 2026-03-29T04:00:00Z. updated (05:00) >= yesterday reset → NOT expired.
        let updated = "2026-03-29T05:00:00Z";
        let now = ymd_hms_to_epoch(2026, 3, 30, 3, 0, 0);
        assert!(!is_expired(updated, now, &policy(4, None)).unwrap());
    }

    #[test]
    fn daily_cross_midnight_now_before_reset_updated_before_yesterday_reset() {
        // reset_at_hour = 4. updated at 2026-03-29T02:00:00Z, now at 2026-03-30T03:00:00Z.
        // Yesterday's reset = 2026-03-29T04:00:00Z. updated (02:00) < yesterday reset → expired.
        let updated = "2026-03-29T02:00:00Z";
        let now = ymd_hms_to_epoch(2026, 3, 30, 3, 0, 0);
        assert!(is_expired(updated, now, &policy(4, None)).unwrap());
    }

    #[test]
    fn daily_updated_exactly_at_reset_point_is_not_expired() {
        // updated_at == reset_point → NOT expired (boundary: "before" means strictly less).
        let updated = "2026-03-30T04:00:00Z";
        let now = ymd_hms_to_epoch(2026, 3, 30, 10, 0, 0);
        assert!(!is_expired(updated, now, &policy(4, None)).unwrap());
    }

    // ── Boundary: reset_at_hour extremes ──

    #[test]
    fn daily_reset_hour_0_updated_yesterday_23() {
        // reset_at_hour = 0. Today's reset = 2026-03-30T00:00:00Z.
        // updated at 23:00 yesterday, now at 01:00 today → expired.
        let updated = "2026-03-29T23:00:00Z";
        let now = ymd_hms_to_epoch(2026, 3, 30, 1, 0, 0);
        assert!(is_expired(updated, now, &policy(0, None)).unwrap());
    }

    #[test]
    fn daily_reset_hour_23_updated_at_22_now_at_23() {
        // reset_at_hour = 23. Today's reset = 2026-03-30T23:00:00Z.
        // updated at 22:00, now at 23:30 → expired (updated < reset, now >= reset).
        let updated = "2026-03-30T22:00:00Z";
        let now = ymd_hms_to_epoch(2026, 3, 30, 23, 30, 0);
        assert!(is_expired(updated, now, &policy(23, None)).unwrap());
    }

    // ── Idle expiry ──

    #[test]
    fn idle_exceeded_is_expired() {
        // idle_minutes = 30. updated 45 min ago → expired.
        let now = ymd_hms_to_epoch(2026, 3, 30, 10, 0, 0);
        let updated = "2026-03-30T09:15:00Z"; // 45 min before now
        assert!(is_expired(updated, now, &policy(4, Some(30))).unwrap());
    }

    #[test]
    fn idle_not_exceeded_is_not_expired() {
        // idle_minutes = 30. updated 10 min ago → not expired (daily also not expired).
        let now = ymd_hms_to_epoch(2026, 3, 30, 10, 0, 0);
        let updated = "2026-03-30T09:50:00Z"; // 10 min before now
        assert!(!is_expired(updated, now, &policy(4, Some(30))).unwrap());
    }

    #[test]
    fn idle_none_disables_idle_check() {
        // idle_minutes = None. updated 2 hours ago, but daily not expired → not expired.
        let now = ymd_hms_to_epoch(2026, 3, 30, 10, 0, 0);
        let updated = "2026-03-30T08:00:00Z";
        assert!(!is_expired(updated, now, &policy(4, None)).unwrap());
    }

    // ── Combined (OR) ──

    #[test]
    fn combo_daily_not_expired_idle_expired_yields_expired() {
        // Daily: updated after today's reset → not expired.
        // Idle: updated 2 hours ago, idle_minutes = 60 → expired.
        // OR → expired.
        let now = ymd_hms_to_epoch(2026, 3, 30, 10, 0, 0);
        let updated = "2026-03-30T05:00:00Z"; // after reset(4), but 5 hours idle
        assert!(is_expired(updated, now, &policy(4, Some(60))).unwrap());
    }

    #[test]
    fn combo_daily_expired_idle_not_expired_yields_expired() {
        // Daily: updated before today's reset → expired.
        // Idle: updated 10 min ago, idle_minutes = 30 → not expired.
        // OR → expired.
        let now = ymd_hms_to_epoch(2026, 3, 30, 4, 10, 0);
        let updated = "2026-03-30T03:50:00Z"; // before reset(4), but only 20 min idle
        // Note: now is 04:10, reset is 04:00, updated is 03:50 < 04:00 → daily expired.
        // idle: 20 min < 30 → not expired. OR → expired.
        assert!(is_expired(updated, now, &policy(4, Some(30))).unwrap());
    }

    // ── Error ──

    #[test]
    fn invalid_updated_at_returns_error() {
        let now = ymd_hms_to_epoch(2026, 3, 30, 10, 0, 0);
        let result = is_expired("not-a-date", now, &policy(4, None));
        assert!(result.is_err());
    }
}
