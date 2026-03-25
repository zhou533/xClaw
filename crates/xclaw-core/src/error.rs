//! Global error types shared across xClaw crates.

/// Top-level error type for the xClaw system.
#[derive(Debug, thiserror::Error)]
pub enum XClawError {
    #[error("agent error: {0}")]
    Agent(String),

    #[error("memory error: {0}")]
    Memory(String),

    #[error("skill error: {0}")]
    Skill(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("channel error: {0}")]
    Channel(String),

    #[error("session error: {0}")]
    Session(String),

    #[error("internal error: {0}")]
    Internal(String),
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_error_display() {
        let e = XClawError::Agent("loop failed".to_string());
        assert_eq!(e.to_string(), "agent error: loop failed");
    }

    #[test]
    fn memory_error_display() {
        let e = XClawError::Memory("sqlite locked".to_string());
        assert_eq!(e.to_string(), "memory error: sqlite locked");
    }

    #[test]
    fn skill_error_display() {
        let e = XClawError::Skill("not found".to_string());
        assert_eq!(e.to_string(), "skill error: not found");
    }

    #[test]
    fn config_error_display() {
        let e = XClawError::Config("missing key".to_string());
        assert_eq!(e.to_string(), "config error: missing key");
    }

    #[test]
    fn session_error_display() {
        let e = XClawError::Session("expired".to_string());
        assert_eq!(e.to_string(), "session error: expired");
    }

    #[test]
    fn internal_error_display() {
        let e = XClawError::Internal("unexpected".to_string());
        assert_eq!(e.to_string(), "internal error: unexpected");
    }
}
