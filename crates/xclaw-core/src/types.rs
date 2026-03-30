//! Shared types used across the xClaw workspace.

use serde::{Deserialize, Serialize};

/// Opaque session identifier.
///
/// Only accepts alphanumeric characters and hyphens (e.g. UUID v4 strings).
/// This prevents path traversal when the ID is used as a filesystem filename.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Create a new `SessionId`, validating that it contains only safe characters.
    ///
    /// Valid: non-empty, only `[a-zA-Z0-9-]`.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        assert!(
            !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "SessionId must be non-empty and contain only [a-zA-Z0-9-], got: '{id}'"
        );
        Self(id)
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

/// Composite key identifying a session: `{role_id}:{scope}`.
///
/// The scope is an arbitrary non-empty string (e.g. "cli", "telegram:12345").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionKey {
    role_id: RoleId,
    scope: String,
}

impl SessionKey {
    /// Create a new `SessionKey` from a validated `RoleId` and a non-empty scope.
    pub fn new(
        role_id: RoleId,
        scope: impl Into<String>,
    ) -> Result<Self, crate::error::XClawError> {
        let scope = scope.into();
        if scope.is_empty() {
            return Err(crate::error::XClawError::Session(
                "session key scope must not be empty".to_string(),
            ));
        }
        Ok(Self { role_id, scope })
    }

    /// Parse a raw string `{role_id}:{scope}`, splitting on the first `:`.
    pub fn parse(raw: &str) -> Result<Self, crate::error::XClawError> {
        let Some(colon_pos) = raw.find(':') else {
            return Err(crate::error::XClawError::Session(format!(
                "invalid session key (missing ':'): {raw}"
            )));
        };
        let role_part = &raw[..colon_pos];
        let scope_part = &raw[colon_pos + 1..];

        let role_id = RoleId::new(role_part)?;
        Self::new(role_id, scope_part)
    }

    pub fn role_id(&self) -> &RoleId {
        &self.role_id
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }
}

impl std::fmt::Display for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.role_id, self.scope)
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

    // ── SessionKey ──

    #[test]
    fn session_key_parse_valid() {
        let key = SessionKey::parse("default:cli").unwrap();
        assert_eq!(key.role_id().as_str(), "default");
        assert_eq!(key.scope(), "cli");
    }

    #[test]
    fn session_key_parse_multiple_colons() {
        let key = SessionKey::parse("coder:project:sub").unwrap();
        assert_eq!(key.role_id().as_str(), "coder");
        assert_eq!(key.scope(), "project:sub");
    }

    #[test]
    fn session_key_parse_no_colon() {
        let result = SessionKey::parse("nocolon");
        assert!(result.is_err());
    }

    #[test]
    fn session_key_parse_empty_scope() {
        let result = SessionKey::parse("default:");
        assert!(result.is_err());
    }

    #[test]
    fn session_key_parse_invalid_role_id() {
        let result = SessionKey::parse("INVALID:cli");
        assert!(result.is_err());
    }

    #[test]
    fn session_key_display_roundtrip() {
        let key = SessionKey::parse("default:cli").unwrap();
        let display = format!("{key}");
        let parsed = SessionKey::parse(&display).unwrap();
        assert_eq!(parsed, key);
    }

    #[test]
    fn session_key_new_valid() {
        let role = RoleId::new("secretary").unwrap();
        let key = SessionKey::new(role, "workspace").unwrap();
        assert_eq!(key.role_id().as_str(), "secretary");
        assert_eq!(key.scope(), "workspace");
    }

    #[test]
    fn session_key_new_empty_scope() {
        let role = RoleId::new("default").unwrap();
        let result = SessionKey::new(role, "");
        assert!(result.is_err());
    }

    #[test]
    fn session_key_serde_roundtrip() {
        let key = SessionKey::parse("default:cli").unwrap();
        let json = serde_json::to_string(&key).unwrap();
        let back: SessionKey = serde_json::from_str(&json).unwrap();
        assert_eq!(back, key);
    }
}
