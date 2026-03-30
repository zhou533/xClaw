//! Session renewal policy configuration.

use serde::{Deserialize, Serialize};

use crate::error::MemoryError;

/// Default daily reset hour (UTC).
pub const DEFAULT_RESET_AT_HOUR: u8 = 4;

/// Controls when a session is considered expired and should be renewed.
///
/// Daily and idle policies are OR — either one triggers renewal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionPolicy {
    /// Daily reset hour (0–23 UTC). Sessions whose `updated_at` falls before
    /// the most recent reset boundary are considered expired.
    pub reset_at_hour: u8,
    /// Idle timeout in minutes. `None` disables idle expiry.
    pub idle_minutes: Option<u64>,
}

impl SessionPolicy {
    /// Create a validated policy.
    ///
    /// Returns `Err` if `reset_at_hour` is outside 0–23.
    pub fn new(reset_at_hour: u8, idle_minutes: Option<u64>) -> Result<Self, MemoryError> {
        if reset_at_hour > 23 {
            return Err(MemoryError::TimeParse(format!(
                "reset_at_hour must be 0–23, got {reset_at_hour}"
            )));
        }
        Ok(Self {
            reset_at_hour,
            idle_minutes,
        })
    }
}

impl Default for SessionPolicy {
    fn default() -> Self {
        Self {
            reset_at_hour: DEFAULT_RESET_AT_HOUR,
            idle_minutes: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uses_hour_4_no_idle() {
        let p = SessionPolicy::default();
        assert_eq!(p.reset_at_hour, 4);
        assert_eq!(p.idle_minutes, None);
    }

    #[test]
    fn new_valid_hour() {
        let p = SessionPolicy::new(23, Some(30)).unwrap();
        assert_eq!(p.reset_at_hour, 23);
        assert_eq!(p.idle_minutes, Some(30));
    }

    #[test]
    fn new_invalid_hour_returns_error() {
        assert!(SessionPolicy::new(24, None).is_err());
        assert!(SessionPolicy::new(255, None).is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let p = SessionPolicy::new(6, Some(30)).unwrap();
        let json = serde_json::to_string(&p).unwrap();
        let back: SessionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}
