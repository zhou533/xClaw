//! Typed error types for the xclaw-memory crate.

use xclaw_core::error::XClawError;

/// Memory subsystem errors.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    YamlParse(String),

    #[error("role not found: {0}")]
    RoleNotFound(String),

    #[error("role already exists: {0}")]
    RoleAlreadyExists(String),

    #[error("invalid role id: {0}")]
    InvalidRoleId(String),

    #[error("invalid date format: {0} (expected YYYY-MM-DD)")]
    InvalidDate(String),
}

impl From<MemoryError> for XClawError {
    fn from(err: MemoryError) -> Self {
        XClawError::Memory(err.to_string())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let mem_err = MemoryError::from(io_err);
        assert!(mem_err.to_string().contains("gone"));
    }

    #[test]
    fn yaml_parse_display() {
        let err = MemoryError::YamlParse("bad yaml".into());
        assert_eq!(err.to_string(), "YAML parse error: bad yaml");
    }

    #[test]
    fn role_not_found_display() {
        let err = MemoryError::RoleNotFound("ghost".into());
        assert_eq!(err.to_string(), "role not found: ghost");
    }

    #[test]
    fn role_already_exists_display() {
        let err = MemoryError::RoleAlreadyExists("default".into());
        assert_eq!(err.to_string(), "role already exists: default");
    }

    #[test]
    fn invalid_role_id_display() {
        let err = MemoryError::InvalidRoleId("BAD".into());
        assert_eq!(err.to_string(), "invalid role id: BAD");
    }

    #[test]
    fn invalid_date_display() {
        let err = MemoryError::InvalidDate("not-a-date".into());
        assert!(err.to_string().contains("not-a-date"));
    }

    #[test]
    fn converts_to_xclaw_error() {
        let mem_err = MemoryError::RoleNotFound("test".into());
        let xclaw_err: XClawError = mem_err.into();
        assert!(matches!(xclaw_err, XClawError::Memory(_)));
        assert!(xclaw_err.to_string().contains("role not found: test"));
    }
}
