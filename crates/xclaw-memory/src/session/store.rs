//! `SessionStore` trait — defines the contract for session persistence.
//!
//! All methods use `impl Future` return style (not dyn-safe).
//! Implementations must be `Send + Sync`.
//!
//! **Concurrency contract**: callers must ensure that no two concurrent writes
//! target the same role's session index. Read operations are safe to overlap.

use std::future::Future;

use xclaw_core::types::{RoleId, SessionId, SessionKey};

use crate::error::MemoryError;
use crate::session::types::{SessionEntry, SessionSummary, TranscriptRecord};

/// Persistence backend for the session subsystem.
pub trait SessionStore: Send + Sync {
    /// Find an existing session by key, or create a new one.
    fn get_or_create(
        &self,
        key: &SessionKey,
    ) -> impl Future<Output = Result<SessionEntry, MemoryError>> + Send;

    /// Look up a session by its unique ID.
    fn get_by_id(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
    ) -> impl Future<Output = Result<Option<SessionEntry>, MemoryError>> + Send;

    /// Look up a session by its composite key.
    fn get_by_key(
        &self,
        key: &SessionKey,
    ) -> impl Future<Output = Result<Option<SessionEntry>, MemoryError>> + Send;

    /// List all sessions for a given role.
    fn list_sessions(
        &self,
        role_id: &RoleId,
    ) -> impl Future<Output = Result<Vec<SessionEntry>, MemoryError>> + Send;

    /// Append a transcript record to the session's JSONL file.
    fn append_transcript(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
        record: &TranscriptRecord,
    ) -> impl Future<Output = Result<(), MemoryError>> + Send;

    /// Load the full transcript for a session.
    fn load_transcript(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
    ) -> impl Future<Output = Result<Vec<TranscriptRecord>, MemoryError>> + Send;

    /// Load the last `n` records of a session's transcript.
    fn load_transcript_tail(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
        n: usize,
    ) -> impl Future<Output = Result<Vec<TranscriptRecord>, MemoryError>> + Send;

    /// Compute summary statistics for a session.
    fn session_summary(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
    ) -> impl Future<Output = Result<SessionSummary, MemoryError>> + Send;

    /// Force-create a new session for the given key, regardless of expiry.
    fn reset_session(
        &self,
        key: &SessionKey,
    ) -> impl Future<Output = Result<SessionEntry, MemoryError>> + Send;

    /// Delete a session (remove from index + delete JSONL file).
    fn delete_session(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
    ) -> impl Future<Output = Result<(), MemoryError>> + Send;
}
