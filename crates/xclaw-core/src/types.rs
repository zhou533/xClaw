//! Shared types used across the xClaw workspace.

use serde::{Deserialize, Serialize};

/// Opaque session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque role identifier (snake_case, e.g. "default", "secretary").
///
/// Used as the filesystem directory name under `~/.xclaw/roles/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoleId(String);

impl RoleId {
    /// Create a new `RoleId`, validating snake_case format.
    ///
    /// Valid: `^[a-z][a-z0-9_]*$`
    pub fn new(id: impl Into<String>) -> Result<Self, crate::error::XClawError> {
        let id = id.into();
        if is_valid_role_id(&id) {
            Ok(Self(id))
        } else {
            Err(crate::error::XClawError::Memory(format!(
                "invalid role id: '{id}' (must match ^[a-z][a-z0-9_]*$)"
            )))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RoleId {
    fn default() -> Self {
        Self("default".to_string())
    }
}

impl std::fmt::Display for RoleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_valid_role_id(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── SessionId ──

    #[test]
    fn session_id_new_and_as_str() {
        let id = SessionId::new("sess-123");
        assert_eq!(id.as_str(), "sess-123");
    }

    #[test]
    fn session_id_round_trips() {
        let id = SessionId::new("abc");
        let json = serde_json::to_string(&id).unwrap();
        let back: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    // ── RoleId ──

    #[test]
    fn role_id_valid_names() {
        assert!(RoleId::new("default").is_ok());
        assert!(RoleId::new("secretary").is_ok());
        assert!(RoleId::new("my_role_2").is_ok());
        assert!(RoleId::new("a").is_ok());
    }

    #[test]
    fn role_id_rejects_invalid_names() {
        assert!(RoleId::new("").is_err());
        assert!(RoleId::new("Default").is_err());
        assert!(RoleId::new("123abc").is_err());
        assert!(RoleId::new("has space").is_err());
        assert!(RoleId::new("has-dash").is_err());
        assert!(RoleId::new("_leading").is_err());
        assert!(RoleId::new("UPPER").is_err());
    }

    #[test]
    fn role_id_default() {
        let id = RoleId::default();
        assert_eq!(id.as_str(), "default");
    }

    #[test]
    fn role_id_display() {
        let id = RoleId::new("secretary").unwrap();
        assert_eq!(format!("{id}"), "secretary");
    }

    #[test]
    fn role_id_round_trips() {
        let id = RoleId::new("coder").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        let back: RoleId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
