use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

/// A tab pending to be opened (sent from another device)
#[derive(Debug, Clone)]
pub struct PendingTab {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub from_device: String,
}

const SCHEMA: &str = r#"
    CREATE TABLE IF NOT EXISTS applied_events (
        id TEXT PRIMARY KEY,
        device TEXT NOT NULL,
        timestamp TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS extensions (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        url TEXT,
        added_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS containers (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        color TEXT NOT NULL,
        icon TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS handlers (
        protocol TEXT PRIMARY KEY,
        handler TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS search_engines (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        url TEXT NOT NULL,
        is_default INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS prefs (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        value_type TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS pending_tabs (
        id TEXT PRIMARY KEY,
        url TEXT NOT NULL,
        title TEXT,
        sent_by TEXT NOT NULL,
        sent_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS vector_clock (
        device TEXT PRIMARY KEY,
        counter INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS extension_xpi (
        id TEXT PRIMARY KEY,
        version TEXT NOT NULL,
        source_type TEXT NOT NULL,
        source_data TEXT NOT NULL,
        xpi_data TEXT NOT NULL,
        installed_at TEXT NOT NULL
    );
"#;

pub struct StateDb {
    conn: Connection,
}

impl StateDb {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn is_event_applied(&self, event_id: uuid::Uuid) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM applied_events WHERE id = ?",
            [event_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn mark_event_applied(
        &self,
        event_id: uuid::Uuid,
        device: &str,
        timestamp: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO applied_events (id, device, timestamp) VALUES (?, ?, ?)",
            [&event_id.to_string(), device, timestamp],
        )?;
        Ok(())
    }

    pub fn add_extension(&self, id: &str, name: &str, url: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO extensions (id, name, url, added_at) VALUES (?, ?, ?, datetime('now'))",
            rusqlite::params![id, name, url],
        )?;
        Ok(())
    }

    pub fn remove_extension(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM extensions WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn get_extensions(&self) -> Result<Vec<(String, String, Option<String>)>> {
        let mut stmt = self.conn.prepare("SELECT id, name, url FROM extensions")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn add_container(&self, id: &str, name: &str, color: &str, icon: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO containers (id, name, color, icon) VALUES (?, ?, ?, ?)",
            [id, name, color, icon],
        )?;
        Ok(())
    }

    pub fn remove_container(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM containers WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn set_handler(&self, protocol: &str, handler: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO handlers (protocol, handler) VALUES (?, ?)",
            [protocol, handler],
        )?;
        Ok(())
    }

    pub fn remove_handler(&self, protocol: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM handlers WHERE protocol = ?", [protocol])?;
        Ok(())
    }

    pub fn set_pref(&self, key: &str, value: &str, value_type: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO prefs (key, value, value_type) VALUES (?, ?, ?)",
            [key, value, value_type],
        )?;
        Ok(())
    }

    pub fn remove_pref(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM prefs WHERE key = ?", [key])?;
        Ok(())
    }

    pub fn add_search_engine(&self, id: &str, name: &str, url: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO search_engines (id, name, url, is_default) VALUES (?, ?, ?, 0)",
            rusqlite::params![id, name, url],
        )?;
        Ok(())
    }

    pub fn remove_search_engine(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM search_engines WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn set_default_search_engine(&self, id: &str) -> Result<()> {
        self.conn
            .execute("UPDATE search_engines SET is_default = 0", [])?;
        self.conn.execute(
            "UPDATE search_engines SET is_default = 1 WHERE id = ?",
            [id],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_pending_tab(
        &self,
        id: &str,
        url: &str,
        title: Option<&str>,
        sent_by: &str,
        sent_at: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO pending_tabs (id, url, title, sent_by, sent_at) VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![id, url, title, sent_by, sent_at],
        )?;
        Ok(())
    }

    pub fn remove_pending_tab(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM pending_tabs WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn get_pending_tabs(&self) -> Result<Vec<PendingTab>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, url, title, sent_by FROM pending_tabs ORDER BY sent_at")?;
        let rows = stmt.query_map([], |row| {
            Ok(PendingTab {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                from_device: row.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn save_vector_clock(&self, clock: &crate::events::VectorClock) -> Result<()> {
        self.conn.execute("DELETE FROM vector_clock", [])?;
        for (device, counter) in clock.entries() {
            self.conn.execute(
                "INSERT INTO vector_clock (device, counter) VALUES (?, ?)",
                rusqlite::params![device, counter],
            )?;
        }
        Ok(())
    }

    pub fn load_vector_clock(&self) -> Result<crate::events::VectorClock> {
        let mut clock = crate::events::VectorClock::new();
        let mut stmt = self
            .conn
            .prepare("SELECT device, counter FROM vector_clock")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;
        for row in rows {
            let (device, counter) = row?;
            clock.set(&device, counter);
        }
        Ok(clock)
    }

    pub fn store_extension_xpi(
        &self,
        id: &str,
        version: &str,
        source: &crate::events::ExtensionSource,
        xpi_data: &str,
    ) -> Result<()> {
        let (source_type, source_data) = match source {
            crate::events::ExtensionSource::Git {
                url,
                ref_spec,
                build_cmd,
            } => (
                "git",
                serde_json::json!({
                    "url": url,
                    "ref_spec": ref_spec,
                    "build_cmd": build_cmd
                })
                .to_string(),
            ),
            crate::events::ExtensionSource::Amo { amo_slug } => (
                "amo",
                serde_json::json!({ "amo_slug": amo_slug }).to_string(),
            ),
            crate::events::ExtensionSource::Local { original_path } => (
                "local",
                serde_json::json!({ "original_path": original_path }).to_string(),
            ),
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO extension_xpi (id, version, source_type, source_data, xpi_data, installed_at) VALUES (?, ?, ?, ?, ?, datetime('now'))",
            rusqlite::params![id, version, source_type, source_data, xpi_data],
        )?;
        Ok(())
    }

    pub fn remove_extension_xpi(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM extension_xpi WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn get_extension_xpi(&self, id: &str) -> Result<Option<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT version, xpi_data FROM extension_xpi WHERE id = ?")?;
        let result = stmt.query_row([id], |row| Ok((row.get(0)?, row.get(1)?)));
        match result {
            Ok(data) => Ok(Some(data)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{ExtensionSource, VectorClock};

    #[test]
    fn test_open_in_memory() {
        let db = StateDb::open_in_memory().unwrap();
        assert!(db.connection().is_autocommit());
    }

    #[test]
    fn test_open_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test.db");
        let db = StateDb::open(&path).unwrap();
        assert!(db.connection().is_autocommit());
        assert!(path.exists());
    }

    #[test]
    fn test_extensions_crud() {
        let db = StateDb::open_in_memory().unwrap();

        // Add extensions
        db.add_extension("ext1@test.com", "Extension 1", Some("https://example.com"))
            .unwrap();
        db.add_extension("ext2@test.com", "Extension 2", None)
            .unwrap();

        // Get extensions
        let extensions = db.get_extensions().unwrap();
        assert_eq!(extensions.len(), 2);

        let ext1 = extensions.iter().find(|(id, _, _)| id == "ext1@test.com");
        assert!(ext1.is_some());
        let (_, name, url) = ext1.unwrap();
        assert_eq!(name, "Extension 1");
        assert_eq!(url, &Some("https://example.com".to_string()));

        // Update extension (replace)
        db.add_extension("ext1@test.com", "Updated Extension", None)
            .unwrap();
        let extensions = db.get_extensions().unwrap();
        let ext1 = extensions
            .iter()
            .find(|(id, _, _)| id == "ext1@test.com")
            .unwrap();
        assert_eq!(ext1.1, "Updated Extension");

        // Remove extension
        db.remove_extension("ext1@test.com").unwrap();
        let extensions = db.get_extensions().unwrap();
        assert_eq!(extensions.len(), 1);
        assert!(extensions.iter().all(|(id, _, _)| id != "ext1@test.com"));
    }

    #[test]
    fn test_containers_crud() {
        let db = StateDb::open_in_memory().unwrap();

        // Add containers
        db.add_container("1", "Work", "blue", "briefcase").unwrap();
        db.add_container("2", "Personal", "green", "circle")
            .unwrap();

        // Update container (replace)
        db.add_container("1", "Work Updated", "red", "briefcase")
            .unwrap();

        // Remove container
        db.remove_container("2").unwrap();

        // Verify via direct query
        let conn = db.connection();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM containers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let name: String = conn
            .query_row("SELECT name FROM containers WHERE id = '1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(name, "Work Updated");
    }

    #[test]
    fn test_handlers_crud() {
        let db = StateDb::open_in_memory().unwrap();

        // Set handlers
        db.set_handler("mailto", "thunderbird").unwrap();
        db.set_handler("tel", "phone-app").unwrap();

        // Update handler
        db.set_handler("mailto", "evolution").unwrap();

        // Remove handler
        db.remove_handler("tel").unwrap();

        // Verify
        let conn = db.connection();
        let handler: String = conn
            .query_row(
                "SELECT handler FROM handlers WHERE protocol = 'mailto'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(handler, "evolution");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM handlers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_prefs_crud() {
        let db = StateDb::open_in_memory().unwrap();

        // Set prefs
        db.set_pref("browser.startup.homepage", "https://example.com", "string")
            .unwrap();
        db.set_pref("browser.tabs.loadInBackground", "true", "bool")
            .unwrap();
        db.set_pref("browser.tabs.maxOpenBeforeWarn", "15", "int")
            .unwrap();

        // Update pref
        db.set_pref("browser.startup.homepage", "https://updated.com", "string")
            .unwrap();

        // Remove pref
        db.remove_pref("browser.tabs.loadInBackground").unwrap();

        // Verify
        let conn = db.connection();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM prefs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        let value: String = conn
            .query_row(
                "SELECT value FROM prefs WHERE key = 'browser.startup.homepage'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "https://updated.com");
    }

    #[test]
    fn test_search_engines_crud() {
        let db = StateDb::open_in_memory().unwrap();

        // Add search engines
        db.add_search_engine("google", "Google", "https://google.com/search?q=%s")
            .unwrap();
        db.add_search_engine("ddg", "DuckDuckGo", "https://duckduckgo.com/?q=%s")
            .unwrap();

        // Set default
        db.set_default_search_engine("ddg").unwrap();

        // Verify default
        let conn = db.connection();
        let is_default: i64 = conn
            .query_row(
                "SELECT is_default FROM search_engines WHERE id = 'ddg'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(is_default, 1);

        // Change default
        db.set_default_search_engine("google").unwrap();
        let ddg_default: i64 = conn
            .query_row(
                "SELECT is_default FROM search_engines WHERE id = 'ddg'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let google_default: i64 = conn
            .query_row(
                "SELECT is_default FROM search_engines WHERE id = 'google'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ddg_default, 0);
        assert_eq!(google_default, 1);

        // Remove search engine
        db.remove_search_engine("ddg").unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_engines", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_pending_tabs_crud() {
        let db = StateDb::open_in_memory().unwrap();

        // Add pending tabs
        db.add_pending_tab(
            "tab1",
            "https://example.com",
            Some("Example"),
            "device-a",
            "2024-01-01T00:00:00Z",
        )
        .unwrap();
        db.add_pending_tab(
            "tab2",
            "https://another.com",
            None,
            "device-b",
            "2024-01-01T00:01:00Z",
        )
        .unwrap();

        // Get pending tabs
        let tabs = db.get_pending_tabs().unwrap();
        assert_eq!(tabs.len(), 2);

        let tab1 = tabs.iter().find(|t| t.id == "tab1").unwrap();
        assert_eq!(tab1.url, "https://example.com");
        assert_eq!(tab1.title, Some("Example".to_string()));
        assert_eq!(tab1.from_device, "device-a");

        let tab2 = tabs.iter().find(|t| t.id == "tab2").unwrap();
        assert_eq!(tab2.title, None);

        // Remove pending tab
        db.remove_pending_tab("tab1").unwrap();
        let tabs = db.get_pending_tabs().unwrap();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].id, "tab2");
    }

    #[test]
    fn test_applied_events() {
        let db = StateDb::open_in_memory().unwrap();

        let event_id = uuid::Uuid::now_v7();

        // Check not applied
        assert!(!db.is_event_applied(event_id).unwrap());

        // Mark applied
        db.mark_event_applied(event_id, "device-a", "2024-01-01T00:00:00Z")
            .unwrap();

        // Check applied
        assert!(db.is_event_applied(event_id).unwrap());

        // Marking again should be idempotent (INSERT OR IGNORE)
        db.mark_event_applied(event_id, "device-a", "2024-01-01T00:00:00Z")
            .unwrap();
        assert!(db.is_event_applied(event_id).unwrap());
    }

    #[test]
    fn test_vector_clock_persistence() {
        let db = StateDb::open_in_memory().unwrap();

        let mut clock = VectorClock::new();
        clock.increment("device-a");
        clock.increment("device-a");
        clock.increment("device-b");

        db.save_vector_clock(&clock).unwrap();

        let loaded = db.load_vector_clock().unwrap();
        assert_eq!(loaded.get("device-a"), 2);
        assert_eq!(loaded.get("device-b"), 1);
        assert_eq!(loaded.get("device-c"), 0);
    }

    #[test]
    fn test_vector_clock_overwrite() {
        let db = StateDb::open_in_memory().unwrap();

        let mut clock1 = VectorClock::new();
        clock1.increment("device-a");
        db.save_vector_clock(&clock1).unwrap();

        let mut clock2 = VectorClock::new();
        clock2.increment("device-b");
        clock2.increment("device-b");
        db.save_vector_clock(&clock2).unwrap();

        let loaded = db.load_vector_clock().unwrap();
        assert_eq!(loaded.get("device-a"), 0); // Should be gone
        assert_eq!(loaded.get("device-b"), 2);
    }

    #[test]
    fn test_extension_xpi_git_source() {
        let db = StateDb::open_in_memory().unwrap();

        let source = ExtensionSource::Git {
            url: "https://github.com/example/ext.git".to_string(),
            ref_spec: "v1.0.0".to_string(),
            build_cmd: Some("npm run build".to_string()),
        };

        db.store_extension_xpi("ext@test.com", "1.0.0", &source, "base64xpidata")
            .unwrap();

        let result = db.get_extension_xpi("ext@test.com").unwrap();
        assert!(result.is_some());
        let (version, xpi_data) = result.unwrap();
        assert_eq!(version, "1.0.0");
        assert_eq!(xpi_data, "base64xpidata");
    }

    #[test]
    fn test_extension_xpi_amo_source() {
        let db = StateDb::open_in_memory().unwrap();

        let source = ExtensionSource::Amo {
            amo_slug: "ublock-origin".to_string(),
        };

        db.store_extension_xpi("ublock@example.com", "1.50.0", &source, "amodata")
            .unwrap();

        let result = db.get_extension_xpi("ublock@example.com").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_extension_xpi_local_source() {
        let db = StateDb::open_in_memory().unwrap();

        let source = ExtensionSource::Local {
            original_path: "/path/to/extension.xpi".to_string(),
        };

        db.store_extension_xpi("local@test.com", "1.0.0", &source, "localdata")
            .unwrap();

        let result = db.get_extension_xpi("local@test.com").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_extension_xpi_not_found() {
        let db = StateDb::open_in_memory().unwrap();

        let result = db.get_extension_xpi("nonexistent@test.com").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_extension_xpi_remove() {
        let db = StateDb::open_in_memory().unwrap();

        let source = ExtensionSource::Local {
            original_path: "/path/to/ext.xpi".to_string(),
        };
        db.store_extension_xpi("ext@test.com", "1.0.0", &source, "data")
            .unwrap();

        assert!(db.get_extension_xpi("ext@test.com").unwrap().is_some());

        db.remove_extension_xpi("ext@test.com").unwrap();

        assert!(db.get_extension_xpi("ext@test.com").unwrap().is_none());
    }

    #[test]
    fn test_extension_xpi_update() {
        let db = StateDb::open_in_memory().unwrap();

        let source = ExtensionSource::Local {
            original_path: "/path/to/ext.xpi".to_string(),
        };

        db.store_extension_xpi("ext@test.com", "1.0.0", &source, "olddata")
            .unwrap();
        db.store_extension_xpi("ext@test.com", "2.0.0", &source, "newdata")
            .unwrap();

        let result = db.get_extension_xpi("ext@test.com").unwrap().unwrap();
        assert_eq!(result.0, "2.0.0");
        assert_eq!(result.1, "newdata");
    }
}
