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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
}
