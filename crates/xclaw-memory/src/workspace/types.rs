//! Memory file types.

use std::collections::HashMap;

/// Kind of memory file within a role directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryFileKind {
    /// `AGENTS.md` — collaboration guardrails and coding standards.
    Agents,
    /// `SOUL.md` — AI persona and tone.
    Soul,
    /// `TOOLS.md` — extra tool guidance beyond the default whitelist.
    Tools,
    /// `IDENTITY.md` — AI self-identity framework.
    Identity,
    /// `USER.md` — human user preferences and tech stack.
    User,
    /// `HEARTBEAT.md` — heartbeat / long-poll action reference.
    Heartbeat,
    /// `BOOTSTRAP.md` — new-workspace bootstrap instructions.
    Bootstrap,
    /// `MEMORY.md` — distilled long-term knowledge.
    LongTerm,
}

impl MemoryFileKind {
    /// The filename on disk for this kind.
    pub fn filename(&self) -> &'static str {
        match self {
            Self::Agents => "AGENTS.md",
            Self::Soul => "SOUL.md",
            Self::Tools => "TOOLS.md",
            Self::Identity => "IDENTITY.md",
            Self::User => "USER.md",
            Self::Heartbeat => "HEARTBEAT.md",
            Self::Bootstrap => "BOOTSTRAP.md",
            Self::LongTerm => "MEMORY.md",
        }
    }

    /// All memory file kinds.
    pub fn all() -> &'static [MemoryFileKind] {
        &[
            Self::Agents,
            Self::Soul,
            Self::Tools,
            Self::Identity,
            Self::User,
            Self::Heartbeat,
            Self::Bootstrap,
            Self::LongTerm,
        ]
    }

    /// Parse a kind from a string (case-insensitive).
    pub fn from_str_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "agents" => Some(Self::Agents),
            "soul" => Some(Self::Soul),
            "tools" => Some(Self::Tools),
            "identity" => Some(Self::Identity),
            "user" => Some(Self::User),
            "heartbeat" => Some(Self::Heartbeat),
            "bootstrap" => Some(Self::Bootstrap),
            "long_term" => Some(Self::LongTerm),
            _ => None,
        }
    }
}

impl std::fmt::Display for MemoryFileKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.filename())
    }
}

/// Snapshot of all memory files for a role.
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    pub files: HashMap<MemoryFileKind, Option<String>>,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_mapping() {
        assert_eq!(MemoryFileKind::Agents.filename(), "AGENTS.md");
        assert_eq!(MemoryFileKind::Soul.filename(), "SOUL.md");
        assert_eq!(MemoryFileKind::Tools.filename(), "TOOLS.md");
        assert_eq!(MemoryFileKind::Identity.filename(), "IDENTITY.md");
        assert_eq!(MemoryFileKind::User.filename(), "USER.md");
        assert_eq!(MemoryFileKind::Heartbeat.filename(), "HEARTBEAT.md");
        assert_eq!(MemoryFileKind::Bootstrap.filename(), "BOOTSTRAP.md");
        assert_eq!(MemoryFileKind::LongTerm.filename(), "MEMORY.md");
    }

    #[test]
    fn all_returns_eight_kinds() {
        assert_eq!(MemoryFileKind::all().len(), 8);
    }

    #[test]
    fn from_str_name_case_insensitive() {
        assert_eq!(
            MemoryFileKind::from_str_name("soul"),
            Some(MemoryFileKind::Soul)
        );
        assert_eq!(
            MemoryFileKind::from_str_name("AGENTS"),
            Some(MemoryFileKind::Agents)
        );
        assert_eq!(
            MemoryFileKind::from_str_name("Tools"),
            Some(MemoryFileKind::Tools)
        );
        assert_eq!(
            MemoryFileKind::from_str_name("long_term"),
            Some(MemoryFileKind::LongTerm)
        );
        assert_eq!(MemoryFileKind::from_str_name("unknown"), None);
    }

    #[test]
    fn display_shows_filename() {
        assert_eq!(format!("{}", MemoryFileKind::Soul), "SOUL.md");
        assert_eq!(format!("{}", MemoryFileKind::LongTerm), "MEMORY.md");
    }
}
