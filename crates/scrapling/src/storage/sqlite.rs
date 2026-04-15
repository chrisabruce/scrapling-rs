//! SQLite-backed persistent element storage.
//!
//! This module provides [`SqliteStorage`], a thread-safe, WAL-mode SQLite
//! backend for the adaptive selection system. It mirrors Python scrapling's
//! `SQLiteStorageSystem`.
//!
//! # Thread safety
//!
//! The database connection is protected by a [`Mutex`], making it safe to
//! share across threads. WAL (Write-Ahead Logging) mode allows concurrent
//! reads even while a write is in progress.
//!
//! # URL isolation
//!
//! Each stored element is scoped to a base URL (extracted from the full URL),
//! so data from different websites is automatically isolated.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use super::{ElementData, StorageSystem};

/// A thread-safe, SQLite-backed storage system for adaptive element
/// relocation.
///
/// # Examples
///
/// ```no_run
/// use scrapling::storage::sqlite::SqliteStorage;
/// use scrapling::storage::{ElementData, StorageSystem};
///
/// let storage = SqliteStorage::new("elements.db", Some("https://example.com")).unwrap();
/// // storage.save(&data, "div.price").unwrap();
/// // let restored = storage.retrieve("div.price").unwrap();
/// ```
pub struct SqliteStorage {
    conn: Mutex<Connection>,
    base_url: String,
}

impl SqliteStorage {
    /// Open (or create) a SQLite database at `path` with WAL mode enabled.
    ///
    /// # Parameters
    ///
    /// - `path` — filesystem path for the `.db` file.
    /// - `url` — optional URL to scope stored data by website. If `None`,
    ///   the key `"default"` is used.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Storage`] if the database cannot be opened or
    /// the schema cannot be created.
    pub fn new(path: impl AsRef<Path>, url: Option<&str>) -> crate::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS storage (
                id INTEGER PRIMARY KEY,
                url TEXT,
                identifier TEXT,
                element_data TEXT,
                UNIQUE (url, identifier)
            )",
        )?;

        let base_url = url
            .map(extract_base_url)
            .unwrap_or_else(|| "default".to_owned());

        Ok(Self {
            conn: Mutex::new(conn),
            base_url,
        })
    }
}

impl StorageSystem for SqliteStorage {
    fn save(&self, data: &ElementData, identifier: &str) -> crate::Result<()> {
        let json = serde_json::to_string(data)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO storage (url, identifier, element_data) VALUES (?1, ?2, ?3)",
            (&self.base_url, identifier, &json),
        )?;
        Ok(())
    }

    fn retrieve(&self, identifier: &str) -> crate::Result<Option<ElementData>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT element_data FROM storage WHERE url = ?1 AND identifier = ?2")?;
        let result = stmt.query_row((&self.base_url, identifier), |row| row.get::<_, String>(0));
        match result {
            Ok(json) => {
                let data: ElementData = serde_json::from_str(&json)?;
                Ok(Some(data))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Extract a base domain from a URL for storage scoping.
///
/// Falls back to the full URL (lowercased) if parsing fails.
fn extract_base_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => parsed.host_str().unwrap_or("default").to_lowercase(),
        Err(_) => url.to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_data() -> ElementData {
        ElementData {
            tag: "div".to_owned(),
            attributes: HashMap::from([("class".to_owned(), "price".to_owned())]),
            text: Some("$42.99".to_owned()),
            path: vec!["html".to_owned(), "body".to_owned(), "div".to_owned()],
            parent_name: Some("body".to_owned()),
            parent_attribs: Some(HashMap::new()),
            parent_text: None,
            siblings: vec!["p".to_owned()],
            children: vec!["span".to_owned()],
        }
    }

    #[test]
    fn roundtrip_save_retrieve() {
        let storage = SqliteStorage::new(":memory:", Some("https://example.com")).unwrap();
        let data = sample_data();
        storage.save(&data, "div.price").unwrap();

        let restored = storage.retrieve("div.price").unwrap().unwrap();
        assert_eq!(restored.tag, "div");
        assert_eq!(restored.text, Some("$42.99".to_owned()));
        assert_eq!(restored.attributes["class"], "price");
    }

    #[test]
    fn retrieve_missing_returns_none() {
        let storage = SqliteStorage::new(":memory:", None).unwrap();
        let result = storage.retrieve("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn upsert_replaces() {
        let storage = SqliteStorage::new(":memory:", None).unwrap();
        let mut data = sample_data();
        storage.save(&data, "key1").unwrap();

        data.text = Some("$99.99".to_owned());
        storage.save(&data, "key1").unwrap();

        let restored = storage.retrieve("key1").unwrap().unwrap();
        assert_eq!(restored.text, Some("$99.99".to_owned()));
    }

    #[test]
    fn url_isolation() {
        let s1 = SqliteStorage::new(":memory:", Some("https://site-a.com")).unwrap();
        let s2 = SqliteStorage::new(":memory:", Some("https://site-b.com")).unwrap();

        let data = sample_data();
        s1.save(&data, "key").unwrap();

        assert!(s1.retrieve("key").unwrap().is_some());
        assert!(s2.retrieve("key").unwrap().is_none());
    }
}
