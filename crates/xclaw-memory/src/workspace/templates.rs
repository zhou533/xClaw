//! Bootstrap template embedding and idempotent file seeding.
//!
//! Template files are compiled into the binary via `include_str!`.
//! At runtime, `ensure_bootstrap_templates` writes them into a role
//! directory only when the target file does not yet exist.

use std::path::Path;

use crate::workspace::types::MemoryFileKind;

// ─── Embedded templates ───────────────────────────────────────────────────────

const AGENTS_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/template/bootstrap/AGENTS.md"
));

const BOOTSTRAP_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/template/bootstrap/BOOTSTRAP.md"
));

const SOUL_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/template/bootstrap/SOUL.md"
));

const IDENTITY_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/template/bootstrap/IDENTITY.md"
));

const USER_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/template/bootstrap/USER.md"
));

const TOOLS_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/template/bootstrap/TOOLS.md"
));

const MEMORY_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/template/bootstrap/MEMORY.md"
));

// ─── Public API ───────────────────────────────────────────────────────────────

/// Return the bootstrap template content for `kind`, or `None` if the kind
/// has no associated template (e.g. `Heartbeat`).
pub fn bootstrap_template(kind: MemoryFileKind) -> Option<&'static str> {
    match kind {
        MemoryFileKind::Agents => Some(AGENTS_TEMPLATE),
        MemoryFileKind::Bootstrap => Some(BOOTSTRAP_TEMPLATE),
        MemoryFileKind::Soul => Some(SOUL_TEMPLATE),
        MemoryFileKind::Identity => Some(IDENTITY_TEMPLATE),
        MemoryFileKind::User => Some(USER_TEMPLATE),
        MemoryFileKind::Tools => Some(TOOLS_TEMPLATE),
        MemoryFileKind::LongTerm => Some(MEMORY_TEMPLATE),
        MemoryFileKind::Heartbeat => None,
    }
}

/// Seed **all** template files into `role_dir`, including `BOOTSTRAP.md`.
///
/// Call this only when creating a brand-new role. For an existing role that
/// may be missing some persistent templates, use
/// [`supplement_missing_templates`] instead — it deliberately skips
/// `BOOTSTRAP.md` because that file is a one-time initialization guide that
/// the agent deletes once onboarding is complete.
///
/// Files that already exist are left untouched (idempotent).
/// Write failures are logged as warnings but do **not** abort the operation.
/// Uses `create_new(true)` to avoid TOCTOU races between the existence check
/// and the write.
pub async fn seed_new_role_templates(role_dir: &Path) {
    write_templates(role_dir, false).await;
}

/// Re-seed only the **persistent** template files into `role_dir`.
///
/// `BOOTSTRAP.md` is intentionally excluded: it is a one-time initialization
/// guide that the agent deletes after onboarding. Re-creating it on an
/// already-initialized role would restart the onboarding flow.
pub async fn supplement_missing_templates(role_dir: &Path) {
    write_templates(role_dir, true).await;
}

/// Shared implementation: write missing template files into `role_dir`.
///
/// When `skip_bootstrap` is `true`, `MemoryFileKind::Bootstrap` is excluded.
async fn write_templates(role_dir: &Path, skip_bootstrap: bool) {
    use tokio::fs::OpenOptions;
    use tokio::io::AsyncWriteExt;

    for kind in MemoryFileKind::all() {
        if skip_bootstrap && *kind == MemoryFileKind::Bootstrap {
            continue;
        }

        let Some(content) = bootstrap_template(*kind) else {
            continue;
        };

        let dest = role_dir.join(kind.filename());

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .await
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(content.as_bytes()).await {
                    tracing::warn!(
                        path = %dest.display(),
                        error = %e,
                        "failed to write bootstrap template; skipping"
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                tracing::warn!(
                    path = %dest.display(),
                    error = %e,
                    "failed to create bootstrap template; skipping"
                );
            }
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── bootstrap_template ────────────────────────────────────────────────────

    #[test]
    fn agents_template_is_some_and_non_empty() {
        let t = bootstrap_template(MemoryFileKind::Agents);
        assert!(t.is_some(), "Agents should have a template");
        assert!(!t.unwrap().is_empty(), "Agents template must not be empty");
    }

    #[test]
    fn bootstrap_template_is_some_and_non_empty() {
        let t = bootstrap_template(MemoryFileKind::Bootstrap);
        assert!(t.is_some(), "Bootstrap should have a template");
        assert!(
            !t.unwrap().is_empty(),
            "Bootstrap template must not be empty"
        );
    }

    #[test]
    fn soul_template_is_some_and_non_empty() {
        let t = bootstrap_template(MemoryFileKind::Soul);
        assert!(t.is_some(), "Soul should have a template");
        assert!(!t.unwrap().is_empty(), "Soul template must not be empty");
    }

    #[test]
    fn identity_template_is_some_and_non_empty() {
        let t = bootstrap_template(MemoryFileKind::Identity);
        assert!(t.is_some(), "Identity should have a template");
        assert!(
            !t.unwrap().is_empty(),
            "Identity template must not be empty"
        );
    }

    #[test]
    fn user_template_is_some_and_non_empty() {
        let t = bootstrap_template(MemoryFileKind::User);
        assert!(t.is_some(), "User should have a template");
        assert!(!t.unwrap().is_empty(), "User template must not be empty");
    }

    #[test]
    fn tools_template_is_some_and_non_empty() {
        let t = bootstrap_template(MemoryFileKind::Tools);
        assert!(t.is_some(), "Tools should have a template");
        assert!(!t.unwrap().is_empty(), "Tools template must not be empty");
    }

    #[test]
    fn long_term_template_is_some_and_non_empty() {
        let t = bootstrap_template(MemoryFileKind::LongTerm);
        assert!(t.is_some(), "LongTerm should have a template");
        assert!(
            !t.unwrap().is_empty(),
            "LongTerm template must not be empty"
        );
    }

    #[test]
    fn heartbeat_returns_none() {
        let t = bootstrap_template(MemoryFileKind::Heartbeat);
        assert!(t.is_none(), "Heartbeat has no bootstrap template");
    }

    #[test]
    fn exactly_seven_kinds_have_templates() {
        let count = MemoryFileKind::all()
            .iter()
            .filter(|k| bootstrap_template(**k).is_some())
            .count();
        assert_eq!(count, 7, "exactly 7 of the 8 kinds should have templates");
    }

    // ── seed_new_role_templates ─────────────────────────────────────────────

    #[tokio::test]
    async fn seed_writes_all_seven_template_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        seed_new_role_templates(tmp.path()).await;

        for kind in MemoryFileKind::all() {
            if bootstrap_template(*kind).is_none() {
                continue;
            }
            let path = tmp.path().join(kind.filename());
            assert!(
                path.exists(),
                "{} should have been created",
                kind.filename()
            );
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(!content.is_empty(), "{} must not be empty", kind.filename());
        }
    }

    #[tokio::test]
    async fn seed_includes_bootstrap_md() {
        let tmp = tempfile::TempDir::new().unwrap();
        seed_new_role_templates(tmp.path()).await;

        let path = tmp.path().join("BOOTSTRAP.md");
        assert!(path.exists(), "seed must create BOOTSTRAP.md for new roles");
    }

    #[tokio::test]
    async fn seed_does_not_overwrite_existing_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let agents_path = tmp.path().join("AGENTS.md");

        std::fs::write(&agents_path, "CUSTOM CONTENT").unwrap();

        seed_new_role_templates(tmp.path()).await;

        let content = std::fs::read_to_string(&agents_path).unwrap();
        assert_eq!(
            content, "CUSTOM CONTENT",
            "existing file must not be overwritten"
        );
    }

    #[tokio::test]
    async fn seed_idempotent_on_second_call() {
        let tmp = tempfile::TempDir::new().unwrap();

        seed_new_role_templates(tmp.path()).await;

        let agents_path = tmp.path().join("AGENTS.md");
        std::fs::write(&agents_path, "SENTINEL").unwrap();

        seed_new_role_templates(tmp.path()).await;

        let content = std::fs::read_to_string(&agents_path).unwrap();
        assert_eq!(
            content, "SENTINEL",
            "second call must not touch existing files"
        );
    }

    #[tokio::test]
    async fn seed_heartbeat_file_is_not_created() {
        let tmp = tempfile::TempDir::new().unwrap();
        seed_new_role_templates(tmp.path()).await;

        let heartbeat_path = tmp.path().join("HEARTBEAT.md");
        assert!(
            !heartbeat_path.exists(),
            "HEARTBEAT.md must not be created (no template)"
        );
    }

    // ── supplement_missing_templates ─────────────────────────────────────────

    #[tokio::test]
    async fn supplement_skips_bootstrap_md() {
        let tmp = tempfile::TempDir::new().unwrap();
        supplement_missing_templates(tmp.path()).await;

        let path = tmp.path().join("BOOTSTRAP.md");
        assert!(
            !path.exists(),
            "supplement must NOT create BOOTSTRAP.md on existing roles"
        );
    }

    #[tokio::test]
    async fn supplement_does_not_recreate_deleted_bootstrap() {
        let tmp = tempfile::TempDir::new().unwrap();

        // Simulate: new role was seeded, then agent deleted BOOTSTRAP.md
        seed_new_role_templates(tmp.path()).await;
        std::fs::remove_file(tmp.path().join("BOOTSTRAP.md")).unwrap();

        // Upgrade path — must not bring it back
        supplement_missing_templates(tmp.path()).await;

        assert!(
            !tmp.path().join("BOOTSTRAP.md").exists(),
            "supplement must not re-create deleted BOOTSTRAP.md"
        );
    }

    #[tokio::test]
    async fn supplement_creates_other_missing_templates() {
        let tmp = tempfile::TempDir::new().unwrap();
        supplement_missing_templates(tmp.path()).await;

        // Persistent templates should be created
        assert!(tmp.path().join("AGENTS.md").exists());
        assert!(tmp.path().join("SOUL.md").exists());
        assert!(tmp.path().join("TOOLS.md").exists());
        assert!(tmp.path().join("IDENTITY.md").exists());
        assert!(tmp.path().join("USER.md").exists());
        assert!(tmp.path().join("MEMORY.md").exists());
    }
}
