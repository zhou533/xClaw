//! Filesystem-backed `SessionStore` implementation.
//!
//! Layout on disk:
//! ```text
//! {base_dir}/
//!   roles/
//!     {role_id}/
//!       sessions/
//!         sessions.json       ← index (atomic write via tempfile)
//!         {session_id}.jsonl  ← transcript (append-only)
//! ```

use std::io::Write;
use std::path::PathBuf;

use xclaw_core::types::{RoleId, SessionId, SessionKey};

use crate::error::MemoryError;
use crate::session::store::SessionStore;
use crate::session::types::{SessionEntry, SessionIndex, SessionSummary, TranscriptRecord};

// ─── FsSessionStore ───────────────────────────────────────────────────────────

/// A `SessionStore` that persists sessions and transcripts to the local filesystem.
pub struct FsSessionStore {
    base_dir: PathBuf,
}

impl FsSessionStore {
    /// Create a new store rooted at `base_dir`.
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    // ── Private helpers ──

    fn sessions_dir(&self, role: &RoleId) -> PathBuf {
        self.base_dir
            .join("roles")
            .join(role.as_str())
            .join("sessions")
    }

    fn index_path(&self, role: &RoleId) -> PathBuf {
        self.sessions_dir(role).join("sessions.json")
    }

    fn transcript_path(&self, role: &RoleId, session_id: &SessionId) -> PathBuf {
        self.sessions_dir(role)
            .join(format!("{}.jsonl", session_id.as_str()))
    }

    fn read_index(&self, role: &RoleId) -> Result<SessionIndex, MemoryError> {
        let path = self.index_path(role);
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str::<SessionIndex>(&content)
                .map_err(|e| MemoryError::IndexCorrupted(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SessionIndex::empty()),
            Err(e) => Err(MemoryError::Io(e)),
        }
    }

    fn write_index(&self, role: &RoleId, index: &SessionIndex) -> Result<(), MemoryError> {
        let dir = self.sessions_dir(role);
        std::fs::create_dir_all(&dir)?;

        let json = serde_json::to_string_pretty(index)
            .map_err(|e| MemoryError::JsonParse(e.to_string()))?;

        // Atomic write: write to a temp file in the same directory, then persist.
        let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
        tmp.write_all(json.as_bytes())?;
        tmp.flush()?;

        let index_path = self.index_path(role);
        tmp.persist(&index_path)
            .map_err(|e| MemoryError::Io(e.error))?;

        Ok(())
    }

    fn now_utc() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let (year, month, day, hour, min, sec) = epoch_to_ymd_hms(secs);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            year, month, day, hour, min, sec
        )
    }
}

/// Convert Unix epoch seconds to (year, month, day, hour, min, sec) UTC.
///
/// Uses the Gregorian calendar algorithm (no external dependency).
fn epoch_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let sec = (secs % 60) as u32;
    let mins = secs / 60;
    let min = (mins % 60) as u32;
    let hours = mins / 60;
    let hour = (hours % 24) as u32;
    let days = hours / 24;

    // Civil date from Julian Day Number
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y } as u32;

    (year, month, day, hour, min, sec)
}

// ─── SessionStore impl ────────────────────────────────────────────────────────

impl SessionStore for FsSessionStore {
    fn get_or_create(
        &self,
        key: &SessionKey,
    ) -> impl std::future::Future<Output = Result<SessionEntry, MemoryError>> + Send {
        let role_id = key.role_id().clone();
        let key = key.clone();
        async move {
            let index = self.read_index(&role_id)?;

            // Return existing session if found.
            let key_str = key.to_string();
            if let Some(entry) = index
                .sessions
                .iter()
                .find(|e| e.session_key.to_string() == key_str)
            {
                return Ok(entry.clone());
            }

            // Create a new session.
            let session_id = SessionId::new(uuid::Uuid::new_v4().to_string());
            let now = Self::now_utc();
            let transcript_rel = format!("{}.jsonl", session_id.as_str());
            let transcript_path = self
                .sessions_dir(&role_id)
                .join(&transcript_rel)
                .to_string_lossy()
                .into_owned();

            let entry = SessionEntry {
                session_id,
                session_key: key,
                transcript_path,
                created_at: now.clone(),
                updated_at: now,
            };

            // Immutable update: build a new index with the new entry appended.
            let new_sessions: Vec<SessionEntry> = index
                .sessions
                .iter()
                .cloned()
                .chain(std::iter::once(entry.clone()))
                .collect();
            let new_index = SessionIndex {
                version: index.version,
                sessions: new_sessions,
            };
            self.write_index(&role_id, &new_index)?;

            Ok(entry)
        }
    }

    fn get_by_id(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
    ) -> impl std::future::Future<Output = Result<Option<SessionEntry>, MemoryError>> + Send {
        let role_id = role_id.clone();
        let session_id = session_id.clone();
        async move {
            let index = self.read_index(&role_id)?;
            let entry = index
                .sessions
                .into_iter()
                .find(|e| e.session_id == session_id);
            Ok(entry)
        }
    }

    fn get_by_key(
        &self,
        key: &SessionKey,
    ) -> impl std::future::Future<Output = Result<Option<SessionEntry>, MemoryError>> + Send {
        let role_id = key.role_id().clone();
        let key_str = key.to_string();
        async move {
            let index = self.read_index(&role_id)?;
            let entry = index
                .sessions
                .into_iter()
                .find(|e| e.session_key.to_string() == key_str);
            Ok(entry)
        }
    }

    fn list_sessions(
        &self,
        role_id: &RoleId,
    ) -> impl std::future::Future<Output = Result<Vec<SessionEntry>, MemoryError>> + Send {
        let role_id = role_id.clone();
        async move {
            let index = self.read_index(&role_id)?;
            Ok(index.sessions)
        }
    }

    fn append_transcript(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
        record: &TranscriptRecord,
    ) -> impl std::future::Future<Output = Result<(), MemoryError>> + Send {
        let role_id = role_id.clone();
        let session_id = session_id.clone();
        let record = record.clone();
        async move {
            // The session must exist in the index.
            let index = self.read_index(&role_id)?;
            if !index.sessions.iter().any(|e| e.session_id == session_id) {
                return Err(MemoryError::SessionNotFound(
                    session_id.as_str().to_string(),
                ));
            }

            let path = self.transcript_path(&role_id, &session_id);
            // Ensure parent directory exists.
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let line = serde_json::to_string(&record)
                .map_err(|e| MemoryError::JsonParse(e.to_string()))?;

            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            writeln!(file, "{}", line)?;

            // Update `updated_at` in the index (immutable rebuild).
            let now = Self::now_utc();
            let new_sessions: Vec<SessionEntry> = index
                .sessions
                .into_iter()
                .map(|e| {
                    if e.session_id == session_id {
                        SessionEntry {
                            updated_at: now.clone(),
                            ..e
                        }
                    } else {
                        e
                    }
                })
                .collect();
            let new_index = SessionIndex {
                version: index.version,
                sessions: new_sessions,
            };
            self.write_index(&role_id, &new_index)?;

            Ok(())
        }
    }

    fn load_transcript(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
    ) -> impl std::future::Future<Output = Result<Vec<TranscriptRecord>, MemoryError>> + Send {
        let role_id = role_id.clone();
        let session_id = session_id.clone();
        async move {
            let path = self.transcript_path(&role_id, &session_id);
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(e) => return Err(MemoryError::Io(e)),
            };

            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            let mut records = Vec::with_capacity(total);

            for (idx, line) in lines.iter().enumerate() {
                let line_num = idx + 1; // 1-based for error messages
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<TranscriptRecord>(line) {
                    Ok(rec) => records.push(rec),
                    Err(e) => {
                        if line_num == total {
                            // Last line — tolerate partial write, warn and skip.
                            tracing::warn!(
                                line = line_num,
                                error = %e,
                                "skipping corrupt last line in transcript"
                            );
                        } else {
                            return Err(MemoryError::TranscriptParse {
                                line: line_num,
                                message: e.to_string(),
                            });
                        }
                    }
                }
            }

            Ok(records)
        }
    }

    fn load_transcript_tail(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
        n: usize,
    ) -> impl std::future::Future<Output = Result<Vec<TranscriptRecord>, MemoryError>> + Send {
        let role_id = role_id.clone();
        let session_id = session_id.clone();
        async move {
            let all = self.load_transcript(&role_id, &session_id).await?;
            let start = all.len().saturating_sub(n);
            Ok(all[start..].to_vec())
        }
    }

    fn session_summary(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
    ) -> impl std::future::Future<Output = Result<SessionSummary, MemoryError>> + Send {
        let role_id = role_id.clone();
        let session_id = session_id.clone();
        async move {
            let entry = self
                .get_by_id(&role_id, &session_id)
                .await?
                .ok_or_else(|| MemoryError::SessionNotFound(session_id.as_str().to_string()))?;

            let records = self.load_transcript(&role_id, &session_id).await?;
            let message_count = records.len();
            let first_message_at = records.first().map(|r| r.timestamp.clone());
            let last_message_at = records.last().map(|r| r.timestamp.clone());

            Ok(SessionSummary {
                session_id: entry.session_id,
                session_key: entry.session_key,
                message_count,
                first_message_at,
                last_message_at,
            })
        }
    }

    fn delete_session(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
    ) -> impl std::future::Future<Output = Result<(), MemoryError>> + Send {
        let role_id = role_id.clone();
        let session_id = session_id.clone();
        async move {
            let index = self.read_index(&role_id)?;

            // Verify the session exists.
            if !index.sessions.iter().any(|e| e.session_id == session_id) {
                return Err(MemoryError::SessionNotFound(
                    session_id.as_str().to_string(),
                ));
            }

            // Immutable update: new index without the deleted session.
            let new_sessions: Vec<SessionEntry> = index
                .sessions
                .into_iter()
                .filter(|e| e.session_id != session_id)
                .collect();
            let new_index = SessionIndex {
                version: index.version,
                sessions: new_sessions,
            };
            self.write_index(&role_id, &new_index)?;

            // Remove the transcript file (ignore not-found).
            let transcript = self.transcript_path(&role_id, &session_id);
            match std::fs::remove_file(&transcript) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(MemoryError::Io(e)),
            }

            Ok(())
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "fs_store_tests.rs"]
mod tests;
