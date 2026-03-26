//! Daily memory: append-only journal (memory/YYYY-MM-DD.md).

use std::path::PathBuf;

use tokio::io::AsyncWriteExt;
use xclaw_core::types::RoleId;

use crate::error::MemoryError;

/// Daily memory: append-only journal per day.
pub trait DailyMemory: Send + Sync {
    fn append(
        &self,
        role: &RoleId,
        entry: &str,
    ) -> impl std::future::Future<Output = Result<(), MemoryError>> + Send;

    fn load_day(
        &self,
        role: &RoleId,
        date: &str,
    ) -> impl std::future::Future<Output = Result<String, MemoryError>> + Send;

    fn list_days(
        &self,
        role: &RoleId,
    ) -> impl std::future::Future<Output = Result<Vec<String>, MemoryError>> + Send;
}

/// Filesystem-backed daily memory.
pub struct FsDailyMemory {
    base_dir: PathBuf,
}

impl FsDailyMemory {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn memory_dir(&self, role: &RoleId) -> PathBuf {
        self.base_dir
            .join("roles")
            .join(role.as_str())
            .join("memory")
    }

    fn day_path(&self, role: &RoleId, date: &str) -> PathBuf {
        self.memory_dir(role).join(format!("{date}.md"))
    }
}

fn validate_date(date: &str) -> Result<(), MemoryError> {
    if date.len() == 10
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[7] == b'-'
        && date.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
    {
        Ok(())
    } else {
        Err(MemoryError::InvalidDate(date.to_string()))
    }
}

/// Get today's date as YYYY-MM-DD.
pub fn today() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple date calculation (UTC)
    let days = now / 86400;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn days_to_ymd(days_since_epoch: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days_since_epoch + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

impl DailyMemory for FsDailyMemory {
    async fn append(&self, role: &RoleId, entry: &str) -> Result<(), MemoryError> {
        let date = today();
        validate_date(&date)?;

        let memory_dir = self.memory_dir(role);
        tokio::fs::create_dir_all(&memory_dir).await?;

        let path = self.day_path(role, &date);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        // Add newline separator if file is not empty
        let metadata = file.metadata().await?;
        if metadata.len() > 0 {
            file.write_all(b"\n").await?;
        }
        file.write_all(entry.as_bytes()).await?;
        file.flush().await?;
        Ok(())
    }

    async fn load_day(&self, role: &RoleId, date: &str) -> Result<String, MemoryError> {
        validate_date(date)?;
        let path = self.day_path(role, date);
        if !path.exists() {
            return Ok(String::new());
        }
        let content = tokio::fs::read_to_string(&path).await?;
        Ok(content)
    }

    async fn list_days(&self, role: &RoleId) -> Result<Vec<String>, MemoryError> {
        let memory_dir = self.memory_dir(role);
        if !memory_dir.exists() {
            return Ok(vec![]);
        }

        let mut dates = Vec::new();
        let mut entries = tokio::fs::read_dir(&memory_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(date) = name.strip_suffix(".md")
                && validate_date(date).is_ok()
            {
                dates.push(date.to_string());
            }
        }
        dates.sort();
        Ok(dates)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(tmp: &std::path::Path) -> FsDailyMemory {
        FsDailyMemory::new(tmp)
    }

    #[test]
    fn validate_date_accepts_valid() {
        assert!(validate_date("2026-03-25").is_ok());
        assert!(validate_date("2000-01-01").is_ok());
    }

    #[test]
    fn validate_date_rejects_invalid() {
        assert!(validate_date("not-a-date").is_err());
        assert!(validate_date("2026/03/25").is_err());
        assert!(validate_date("26-03-25").is_err());
        assert!(validate_date("").is_err());
    }

    #[test]
    fn today_returns_valid_date() {
        let d = today();
        assert!(validate_date(&d).is_ok());
        assert_eq!(d.len(), 10);
    }

    #[tokio::test]
    async fn append_and_load_day() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dm = setup(tmp.path());
        let role = RoleId::default();

        dm.append(&role, "entry 1").await.unwrap();
        dm.append(&role, "entry 2").await.unwrap();

        let date = today();
        let content = dm.load_day(&role, &date).await.unwrap();
        assert!(content.contains("entry 1"));
        assert!(content.contains("entry 2"));
        // Entries separated by newline
        assert!(content.contains("entry 1\nentry 2"));
    }

    #[tokio::test]
    async fn load_day_nonexistent_returns_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dm = setup(tmp.path());
        let content = dm.load_day(&RoleId::default(), "2020-01-01").await.unwrap();
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn load_day_invalid_date_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dm = setup(tmp.path());
        let result = dm.load_day(&RoleId::default(), "bad").await;
        assert!(matches!(result, Err(MemoryError::InvalidDate(_))));
    }

    #[tokio::test]
    async fn list_days_returns_sorted() {
        let tmp = tempfile::TempDir::new().unwrap();
        let role = RoleId::default();

        // Manually create date files
        let mem_dir = tmp.path().join("roles/default/memory");
        tokio::fs::create_dir_all(&mem_dir).await.unwrap();
        tokio::fs::write(mem_dir.join("2026-03-25.md"), "a")
            .await
            .unwrap();
        tokio::fs::write(mem_dir.join("2026-03-20.md"), "b")
            .await
            .unwrap();
        tokio::fs::write(mem_dir.join("2026-03-28.md"), "c")
            .await
            .unwrap();
        tokio::fs::write(mem_dir.join("not-a-date.md"), "skip")
            .await
            .unwrap();

        let dm = setup(tmp.path());
        let days = dm.list_days(&role).await.unwrap();
        assert_eq!(days, vec!["2026-03-20", "2026-03-25", "2026-03-28"]);
    }

    #[tokio::test]
    async fn list_days_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dm = setup(tmp.path());
        let days = dm.list_days(&RoleId::default()).await.unwrap();
        assert!(days.is_empty());
    }
}
