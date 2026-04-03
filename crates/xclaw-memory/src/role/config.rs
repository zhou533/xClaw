//! Role configuration (AIOS-compatible).
//!
//! All roles are stored in a single `roles.yaml` file as a YAML map,
//! keyed by `role_id`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::MemoryError;

/// All roles in a single file: `role_id` → `RoleConfig`.
pub type RolesFile = BTreeMap<String, RoleConfig>;

/// AIOS-compatible role configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub name: String,
    #[serde(default)]
    pub description: Vec<String>,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub meta: RoleMeta,
    /// Directory for this role's memory files (relative to base_dir).
    #[serde(default)]
    pub memory_dir: String,
}

/// Role metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleMeta {
    #[serde(default = "default_author")]
    pub author: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_license")]
    pub license: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

fn default_author() -> String {
    "user".to_string()
}

fn default_version() -> String {
    "1.0.0".to_string()
}

fn default_license() -> String {
    "private".to_string()
}

impl Default for RoleMeta {
    fn default() -> Self {
        Self {
            author: default_author(),
            version: default_version(),
            license: default_license(),
            created_at: None,
        }
    }
}

impl RoleConfig {
    /// Build the default role configuration.
    pub fn default_config() -> Self {
        Self {
            name: "default".to_string(),
            description: vec!["Default AI assistant".to_string()],
            system_prompt: String::new(),
            tools: vec![],
            meta: RoleMeta::default(),
            memory_dir: "roles/default".to_string(),
        }
    }

    /// Parse a `RoleConfig` from YAML content.
    pub fn from_yaml(content: &str) -> Result<Self, MemoryError> {
        serde_yml::from_str(content).map_err(|e| MemoryError::YamlParse(e.to_string()))
    }

    /// Serialize this config to YAML.
    pub fn to_yaml(&self) -> Result<String, MemoryError> {
        serde_yml::to_string(self).map_err(|e| MemoryError::YamlParse(e.to_string()))
    }
}

/// Parse a `RolesFile` from YAML content.
pub fn parse_roles_file(content: &str) -> Result<RolesFile, MemoryError> {
    serde_yml::from_str(content).map_err(|e| MemoryError::YamlParse(e.to_string()))
}

/// Serialize a `RolesFile` to YAML.
pub fn serialize_roles_file(roles: &RolesFile) -> Result<String, MemoryError> {
    serde_yml::to_string(roles).map_err(|e| MemoryError::YamlParse(e.to_string()))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_fields() {
        let cfg = RoleConfig::default_config();
        assert_eq!(cfg.name, "default");
        assert_eq!(cfg.description, vec!["Default AI assistant"]);
        assert!(cfg.system_prompt.is_empty());
        assert!(cfg.tools.is_empty());
        assert_eq!(cfg.meta.author, "user");
        assert_eq!(cfg.meta.version, "1.0.0");
        assert_eq!(cfg.memory_dir, "roles/default");
    }

    #[test]
    fn round_trip_yaml() {
        let cfg = RoleConfig {
            name: "secretary".to_string(),
            description: vec!["日程管理".to_string(), "邮件处理".to_string()],
            system_prompt: "你是私人秘书".to_string(),
            tools: vec!["shell".to_string(), "file_read".to_string()],
            meta: RoleMeta {
                author: "user".to_string(),
                version: "1.0.0".to_string(),
                license: "private".to_string(),
                created_at: Some("2026-03-25".to_string()),
            },
            memory_dir: "roles/secretary".to_string(),
        };
        let yaml = cfg.to_yaml().unwrap();
        let parsed = RoleConfig::from_yaml(&yaml).unwrap();
        assert_eq!(parsed.name, "secretary");
        assert_eq!(parsed.description.len(), 2);
        assert_eq!(parsed.tools, vec!["shell", "file_read"]);
        assert_eq!(parsed.meta.created_at.as_deref(), Some("2026-03-25"));
        assert_eq!(parsed.memory_dir, "roles/secretary");
    }

    #[test]
    fn minimal_yaml_uses_defaults() {
        let yaml = "name: test\n";
        let cfg = RoleConfig::from_yaml(yaml).unwrap();
        assert_eq!(cfg.name, "test");
        assert!(cfg.description.is_empty());
        assert!(cfg.system_prompt.is_empty());
        assert!(cfg.tools.is_empty());
        assert_eq!(cfg.meta.author, "user");
    }

    #[test]
    fn aios_compatible_yaml() {
        let yaml = r#"
name: secretary
description:
  - "负责日程管理"
  - "邮件处理"
system_prompt: |
  你是用户的私人秘书
tools:
  - shell
  - file_read
meta:
  author: user
  version: "1.0.0"
  license: private
  created_at: "2026-03-25"
"#;
        let cfg = RoleConfig::from_yaml(yaml).unwrap();
        assert_eq!(cfg.name, "secretary");
        assert_eq!(cfg.description.len(), 2);
        assert!(cfg.system_prompt.contains("私人秘书"));
        assert_eq!(cfg.tools, vec!["shell", "file_read"]);
    }

    #[test]
    fn minimal_yaml_defaults_memory_dir_to_empty() {
        let yaml = "name: test\n";
        let cfg = RoleConfig::from_yaml(yaml).unwrap();
        assert!(cfg.memory_dir.is_empty());
    }

    #[test]
    fn invalid_yaml_returns_error() {
        let result = RoleConfig::from_yaml("{{{{invalid}}}}");
        assert!(result.is_err());
    }

    // ── RolesFile tests ──────────────────────────────────────────────────────

    #[test]
    fn roles_file_roundtrip() {
        use std::collections::BTreeMap;

        let mut roles = BTreeMap::new();
        roles.insert(
            "default".to_string(),
            RoleConfig {
                name: "default".to_string(),
                memory_dir: "roles/default".to_string(),
                ..RoleConfig::default_config()
            },
        );

        let yaml = serialize_roles_file(&roles).unwrap();
        let parsed = parse_roles_file(&yaml).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["default"].name, "default");
        assert_eq!(parsed["default"].memory_dir, "roles/default");
    }

    #[test]
    fn roles_file_with_multiple_entries() {
        use std::collections::BTreeMap;

        let mut roles = BTreeMap::new();
        roles.insert(
            "admin".to_string(),
            RoleConfig {
                name: "admin".to_string(),
                memory_dir: "roles/admin".to_string(),
                ..RoleConfig::default_config()
            },
        );
        roles.insert(
            "secretary".to_string(),
            RoleConfig {
                name: "secretary".to_string(),
                memory_dir: "roles/secretary".to_string(),
                ..RoleConfig::default_config()
            },
        );

        let yaml = serialize_roles_file(&roles).unwrap();
        let parsed = parse_roles_file(&yaml).unwrap();
        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains_key("admin"));
        assert!(parsed.contains_key("secretary"));
    }
}
