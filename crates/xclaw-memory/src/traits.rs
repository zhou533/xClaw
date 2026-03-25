//! Memory store trait and associated types.

use serde::{Deserialize, Serialize};

use xclaw_core::error::XClawError;
use xclaw_core::types::SessionId;

/// A single entry stored in the memory system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

/// Persistent memory storage for conversation history and context.
pub trait MemoryStore: Send + Sync {
    fn store(
        &self,
        session: &SessionId,
        entry: MemoryEntry,
    ) -> impl std::future::Future<Output = Result<(), XClawError>> + Send;

    fn recall(
        &self,
        session: &SessionId,
        query: &str,
    ) -> impl std::future::Future<Output = Result<Vec<MemoryEntry>, XClawError>> + Send;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct StubMemory;

    impl MemoryStore for StubMemory {
        async fn store(&self, _session: &SessionId, _entry: MemoryEntry) -> Result<(), XClawError> {
            Ok(())
        }

        async fn recall(
            &self,
            _session: &SessionId,
            _query: &str,
        ) -> Result<Vec<MemoryEntry>, XClawError> {
            Ok(vec![])
        }
    }

    #[test]
    fn memory_entry_serializes() {
        let entry = MemoryEntry {
            role: "user".to_string(),
            content: "hello".to_string(),
            timestamp: 1700000000,
        };
        let v = serde_json::to_value(&entry).unwrap();
        assert_eq!(v["role"], "user");
        assert_eq!(v["timestamp"], 1700000000);
    }

    #[tokio::test]
    async fn stub_memory_store_and_recall() {
        let mem = StubMemory;
        let sid = SessionId::new("s1");
        let entry = MemoryEntry {
            role: "user".to_string(),
            content: "hi".to_string(),
            timestamp: 0,
        };
        mem.store(&sid, entry).await.unwrap();
        let results = mem.recall(&sid, "hi").await.unwrap();
        assert!(results.is_empty());
    }
}
