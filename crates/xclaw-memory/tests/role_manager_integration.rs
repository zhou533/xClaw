//! Integration tests for FsRoleManager.

use xclaw_core::types::RoleId;
use xclaw_memory::error::MemoryError;
use xclaw_memory::role::config::RoleConfig;
use xclaw_memory::role::manager::{FsRoleManager, RoleManager};

fn setup() -> (tempfile::TempDir, FsRoleManager) {
    let tmp = tempfile::TempDir::new().unwrap();
    let mgr = FsRoleManager::new(tmp.path());
    (tmp, mgr)
}

fn test_config(name: &str) -> RoleConfig {
    RoleConfig {
        name: name.to_string(),
        description: vec![format!("{name} role")],
        system_prompt: format!("You are {name}"),
        tools: vec!["shell".to_string()],
        meta: Default::default(),
        memory_dir: format!("roles/{name}"),
    }
}

#[tokio::test]
async fn full_lifecycle_create_get_list_delete() {
    let (tmp, mgr) = setup();

    // Create
    mgr.create_role(test_config("alpha")).await.unwrap();
    mgr.create_role(test_config("beta")).await.unwrap();

    // Verify roles.yaml and directories exist
    assert!(tmp.path().join("roles.yaml").exists());
    assert!(tmp.path().join("roles/alpha/memory").is_dir());
    assert!(tmp.path().join("roles/beta/memory").is_dir());

    // Get
    let alpha = mgr.get_role(&RoleId::new("alpha").unwrap()).await.unwrap();
    assert_eq!(alpha.name, "alpha");
    assert_eq!(alpha.tools, vec!["shell"]);
    assert!(alpha.system_prompt.contains("alpha"));

    // List (sorted)
    let roles = mgr.list_roles().await.unwrap();
    let names: Vec<&str> = roles.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "beta"]);

    // Delete (directory preserved, entry removed from roles.yaml)
    mgr.delete_role(&RoleId::new("alpha").unwrap())
        .await
        .unwrap();
    assert!(tmp.path().join("roles/alpha").is_dir()); // directory preserved

    // List after delete
    let roles = mgr.list_roles().await.unwrap();
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0].name, "beta");
}

#[tokio::test]
async fn create_duplicate_fails() {
    let (_tmp, mgr) = setup();
    mgr.create_role(test_config("dup")).await.unwrap();
    let err = mgr.create_role(test_config("dup")).await.unwrap_err();
    assert!(matches!(err, MemoryError::RoleAlreadyExists(_)));
}

#[tokio::test]
async fn get_nonexistent_fails() {
    let (_tmp, mgr) = setup();
    let err = mgr
        .get_role(&RoleId::new("ghost").unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, MemoryError::RoleNotFound(_)));
}

#[tokio::test]
async fn delete_default_forbidden() {
    let (_tmp, mgr) = setup();
    mgr.create_role(RoleConfig::default_config()).await.unwrap();
    let err = mgr.delete_role(&RoleId::default()).await.unwrap_err();
    assert!(matches!(err, MemoryError::InvalidRoleId(_)));
}

#[tokio::test]
async fn delete_nonexistent_fails() {
    let (_tmp, mgr) = setup();
    let err = mgr
        .delete_role(&RoleId::new("ghost").unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, MemoryError::RoleNotFound(_)));
}

#[tokio::test]
async fn create_invalid_name_fails() {
    let (_tmp, mgr) = setup();
    let err = mgr.create_role(test_config("INVALID")).await.unwrap_err();
    assert!(matches!(err, MemoryError::InvalidRoleId(_)));
}
