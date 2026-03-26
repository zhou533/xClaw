//! Semantic search interface (placeholder — no implementation yet).
//!
//! Future integration point for SQLite FTS5, sqlite-vss, or qdrant.

use xclaw_core::types::RoleId;

use crate::error::MemoryError;

/// A single search result with relevance score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub content: String,
    pub score: f64,
    pub source: String,
}

/// Semantic search over role memory (not yet implemented).
pub trait MemorySearcher: Send + Sync {
    fn search(
        &self,
        role: &RoleId,
        query: &str,
        limit: usize,
    ) -> impl std::future::Future<Output = Result<Vec<SearchResult>, MemoryError>> + Send;

    fn index(
        &self,
        role: &RoleId,
    ) -> impl std::future::Future<Output = Result<(), MemoryError>> + Send;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_result_construction() {
        let result = SearchResult {
            content: "test".to_string(),
            score: 0.95,
            source: "MEMORY.md".to_string(),
        };
        assert_eq!(result.score, 0.95);
        assert_eq!(result.source, "MEMORY.md");
    }
}
