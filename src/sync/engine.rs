use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::crypto::PublicKey;
use crate::events::{Event, EventLog};
use crate::net::EncryptedEvent;
use crate::profile::{
    Container, Handler, WriteQueue, find_profile, is_browser_running, read_containers,
    read_extensions, read_handlers, read_prefs, write_containers, write_handlers, write_user_js,
};
use crate::state::{PendingTab, StateDb, materialize_events};

use super::diff::{diff_containers, diff_extensions, diff_handlers, diff_prefs};

pub struct SyncEngine {
    config: Config,
    profile_path: PathBuf,
    event_log: EventLog,
    state_db: StateDb,
    write_queue: WriteQueue,
    known_devices: Vec<(String, PublicKey)>,
}

impl SyncEngine {
    pub fn new(config: Config, event_log: EventLog, state_db: StateDb) -> Result<Self> {
        let profile_path = config
            .paths
            .profile
            .clone()
            .map(Ok)
            .unwrap_or_else(find_profile)?;
        let write_queue = WriteQueue::new(profile_path.clone());
        Ok(Self {
            config,
            profile_path,
            event_log,
            state_db,
            write_queue,
            known_devices: Vec::new(),
        })
    }

    pub fn add_known_device(&mut self, device_id: String, public_key: PublicKey) {
        self.known_devices.push((device_id, public_key));
    }

    pub fn sync_dir(&self) -> PathBuf {
        self.config.paths.sync_dir.clone()
    }

    pub fn profile_path(&self) -> &PathBuf {
        &self.profile_path
    }

    pub fn device_id(&self) -> &str {
        &self.config.device.id
    }

    /// Process incoming events from the sync directory
    pub fn process_incoming(&mut self) -> Result<usize> {
        let events = self.event_log.read_all_events(&self.known_devices)?;
        let applied = materialize_events(&self.state_db, &events, &self.config.device.id)?;

        if applied > 0 {
            info!(count = applied, "Applied incoming events");
            // Update vector clock from merged events
            let (_, new_clock) = super::merge_events(&[], &events, self.event_log.clock());
            self.event_log.set_clock(new_clock.clone());
            self.state_db.save_vector_clock(&new_clock)?;
        }

        Ok(applied)
    }

    /// Scan profile for changes and generate outbound events
    pub fn scan_profile(&mut self) -> Result<Vec<Event>> {
        let mut events = Vec::new();

        // Scan extensions
        let current_extensions = read_extensions(&self.profile_path)?;
        let known_extensions = self.state_db.get_extensions()?;
        let known_ids: Vec<String> = known_extensions
            .iter()
            .map(|(id, _, _)| id.clone())
            .collect();

        let ext_events = diff_extensions(&current_extensions, &known_ids);
        events.extend(ext_events);

        // Scan containers
        let current_containers = read_containers(&self.profile_path)?;
        let container_events = self.diff_containers_from_profile(&current_containers)?;
        events.extend(container_events);

        // Scan handlers
        let current_handlers = read_handlers(&self.profile_path)?;
        let handler_events = self.diff_handlers_from_profile(&current_handlers)?;
        events.extend(handler_events);

        // Scan prefs (if whitelist is configured)
        if !self.config.prefs.whitelist.is_empty() {
            let current_prefs = read_prefs(&self.profile_path, &self.config.prefs.whitelist)?;
            let pref_events = self.diff_prefs_from_profile(&current_prefs)?;
            events.extend(pref_events);
        }

        Ok(events)
    }

    /// Write events to the sync directory
    pub fn write_events(&mut self, events: Vec<Event>) -> Result<Option<PathBuf>> {
        if events.is_empty() {
            return Ok(None);
        }

        let path = self.event_log.write_events(events, &self.known_devices)?;
        info!(path = %path.display(), "Wrote events to sync directory");
        Ok(Some(path))
    }

    /// Apply materialized state to the profile
    pub fn apply_to_profile(&mut self) -> Result<Vec<String>> {
        let browser_running = is_browser_running(&self.profile_path);

        if browser_running {
            warn!("Browser is running, queuing writes for later");
            self.queue_profile_writes()?;
            return Ok(Vec::new());
        }

        // Flush any queued writes first
        let mut applied = self.write_queue.flush()?;

        // Then apply current state
        let profile_applied = self.write_profile_state()?;
        applied.extend(profile_applied);

        Ok(applied)
    }

    /// Flush queued writes (call when browser closes)
    pub fn flush_write_queue(&mut self) -> Result<Vec<String>> {
        self.write_queue.flush()
    }

    fn queue_profile_writes(&mut self) -> Result<()> {
        // Queue containers
        let containers = self.get_materialized_containers()?;
        if !containers.is_empty() {
            self.write_queue.queue_containers(containers);
        }

        // Queue handlers
        let handlers = self.get_materialized_handlers()?;
        if !handlers.is_empty() {
            self.write_queue.queue_handlers(handlers);
        }

        // Queue prefs
        let prefs = self.get_materialized_prefs()?;
        if !prefs.is_empty() {
            self.write_queue.queue_prefs(prefs);
        }

        Ok(())
    }

    fn write_profile_state(&self) -> Result<Vec<String>> {
        let mut written = Vec::new();

        let containers = self.get_materialized_containers()?;
        if !containers.is_empty() {
            write_containers(&self.profile_path, &containers)?;
            written.push("containers.json".to_string());
        }

        let handlers = self.get_materialized_handlers()?;
        if !handlers.is_empty() {
            write_handlers(&self.profile_path, &handlers)?;
            written.push("handlers.json".to_string());
        }

        let prefs = self.get_materialized_prefs()?;
        if !prefs.is_empty() {
            write_user_js(&self.profile_path, &prefs)?;
            written.push("user.js".to_string());
        }

        Ok(written)
    }

    fn get_materialized_containers(&self) -> Result<Vec<Container>> {
        let conn = self.state_db.connection();
        let mut stmt = conn.prepare("SELECT id, name, color, icon FROM containers")?;
        let rows = stmt.query_map([], |row| {
            Ok(Container {
                user_context_id: row.get::<_, String>(0)?.parse().unwrap_or(0),
                name: row.get(1)?,
                color: row.get(2)?,
                icon: row.get(3)?,
                is_public: true,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn get_materialized_handlers(&self) -> Result<Vec<Handler>> {
        let conn = self.state_db.connection();
        let mut stmt = conn.prepare("SELECT protocol, handler FROM handlers")?;
        let rows = stmt.query_map([], |row| {
            Ok(Handler {
                protocol: row.get(0)?,
                handler: row.get(1)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn get_materialized_prefs(
        &self,
    ) -> Result<std::collections::HashMap<String, crate::events::PrefValue>> {
        use crate::events::PrefValue;
        let conn = self.state_db.connection();
        let mut stmt = conn.prepare("SELECT key, value, value_type FROM prefs")?;
        let mut prefs = std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            let value_type: String = row.get(2)?;
            Ok((key, value, value_type))
        })?;
        for row in rows {
            let (key, value, value_type) = row?;
            let pref_value = match value_type.as_str() {
                "bool" => PrefValue::Bool(value.parse().unwrap_or(false)),
                "int" => PrefValue::Int(value.parse().unwrap_or(0)),
                _ => PrefValue::String(value),
            };
            prefs.insert(key, pref_value);
        }
        Ok(prefs)
    }

    fn query_container_ids(&self) -> Result<Vec<String>> {
        let conn = self.state_db.connection();
        let mut stmt = conn.prepare("SELECT id FROM containers")?;
        let known_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(known_ids)
    }

    fn diff_containers_from_profile(&self, current: &[Container]) -> Result<Vec<Event>> {
        let known_ids = self.query_container_ids()?;
        Ok(diff_containers(current, &known_ids))
    }

    fn query_handlers(&self) -> Result<HashMap<String, String>> {
        let conn = self.state_db.connection();
        let mut stmt = conn.prepare("SELECT protocol, handler FROM handlers")?;
        let known: HashMap<String, String> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(known)
    }

    fn diff_handlers_from_profile(&self, current: &[Handler]) -> Result<Vec<Event>> {
        let known = self.query_handlers()?;
        Ok(diff_handlers(current, &known))
    }

    fn query_prefs(&self) -> Result<HashMap<String, crate::events::PrefValue>> {
        use crate::events::PrefValue;

        let conn = self.state_db.connection();
        let mut stmt = conn.prepare("SELECT key, value, value_type FROM prefs")?;
        let mut known: HashMap<String, PrefValue> = HashMap::new();
        let rows = stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            let value_type: String = row.get(2)?;
            Ok((key, value, value_type))
        })?;
        for row in rows {
            let (key, value, value_type) = row?;
            let pref_value = match value_type.as_str() {
                "bool" => PrefValue::Bool(value.parse().unwrap_or(false)),
                "int" => PrefValue::Int(value.parse().unwrap_or(0)),
                _ => PrefValue::String(value),
            };
            known.insert(key, pref_value);
        }
        Ok(known)
    }

    fn diff_prefs_from_profile(
        &self,
        current: &HashMap<String, crate::events::PrefValue>,
    ) -> Result<Vec<Event>> {
        let known = self.query_prefs()?;
        Ok(diff_prefs(current, &known))
    }

    /// Full sync cycle: process incoming, scan profile, write outbound
    pub fn sync(&mut self) -> Result<SyncResult> {
        debug!("Starting sync cycle");

        let incoming = self.process_incoming()?;
        let events = self.scan_profile()?;
        let outbound = events.len();
        let path = self.write_events(events)?;
        let mut applied = self.apply_to_profile()?;

        // Handle extension installation/removal
        let installed_extensions = self.install_pending_extensions()?;
        let removed_extensions = self.remove_uninstalled_extensions()?;

        for ext_id in installed_extensions {
            applied.push(format!("extensions/{}.xpi", ext_id));
        }
        for ext_id in removed_extensions {
            applied.push(format!("extensions/{}.xpi (removed)", ext_id));
        }

        Ok(SyncResult {
            incoming_applied: incoming,
            outbound_written: outbound,
            profile_files_written: applied,
            event_file: path,
        })
    }

    /// Send a tab to another device
    pub fn send_tab(&mut self, to_device: &str, url: &str, title: Option<&str>) -> Result<PathBuf> {
        let event = Event::TabSent {
            to_device: to_device.to_string(),
            url: url.to_string(),
            title: title.map(String::from),
        };

        self.write_events(vec![event])?
            .ok_or_else(|| anyhow::anyhow!("Failed to write tab event"))
    }

    /// Get pending tabs for this device
    pub fn get_pending_tabs(&self) -> Result<Vec<PendingTab>> {
        self.state_db.get_pending_tabs()
    }

    /// Mark a tab as received (acknowledged)
    pub fn acknowledge_tab(&mut self, tab_id: &str) -> Result<PathBuf> {
        let event_id = uuid::Uuid::parse_str(tab_id)?;
        let event = Event::TabReceived { event_id };

        self.state_db.remove_pending_tab(tab_id)?;
        self.write_events(vec![event])?
            .ok_or_else(|| anyhow::anyhow!("Failed to write tab acknowledgment"))
    }

    /// Open a URL in the default browser
    pub fn open_tab(&self, url: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(url)
                .spawn()
                .map_err(|e| anyhow::anyhow!("Failed to open URL: {}", e))?;
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(url)
                .spawn()
                .map_err(|e| anyhow::anyhow!("Failed to open URL: {}", e))?;
        }

        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", url])
                .spawn()
                .map_err(|e| anyhow::anyhow!("Failed to open URL: {}", e))?;
        }

        Ok(())
    }

    /// Get the current vector clock
    pub fn get_vector_clock(&self) -> HashMap<String, u64> {
        self.event_log.clock().to_hashmap()
    }

    /// Get events since the given clock (for P2P sync)
    pub fn get_events_since(
        &self,
        _remote_clock: &HashMap<String, u64>,
    ) -> Result<Vec<EncryptedEvent>> {
        // TODO: Implement proper event filtering based on clock comparison
        // For now, return empty - events will be re-sent on demand
        Ok(Vec::new())
    }

    /// Apply events received from a remote peer
    pub fn apply_remote_events(&mut self, events: Vec<EncryptedEvent>) -> Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }

        // TODO: Decrypt and apply events
        // For now, just count them
        info!("Would apply {} remote events", events.len());
        Ok(events.len())
    }

    /// Receive a tab from another device (via P2P)
    pub fn receive_tab(&mut self, url: &str, title: Option<&str>, from_device: &str) -> Result<()> {
        let tab_id = uuid::Uuid::now_v7().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.state_db
            .add_pending_tab(&tab_id, url, title, from_device, &now)?;
        info!("Received tab from {}: {}", from_device, url);
        Ok(())
    }

    /// Install any extensions that are in the database but not yet installed to the profile
    #[allow(clippy::cognitive_complexity)] // Loop with multiple conditions
    pub fn install_pending_extensions(&self) -> Result<Vec<String>> {
        let extensions = self.state_db.get_extensions()?;
        let extensions_dir = self.profile_path.join("extensions");
        let mut installed = Vec::new();

        for (id, name, _url) in extensions {
            let xpi_path = extensions_dir.join(format!("{}.xpi", id));

            // Skip if already installed
            if xpi_path.exists() {
                continue;
            }

            // Check if we have XPI data
            if let Some((version, xpi_data)) = self.state_db.get_extension_xpi(&id)? {
                info!("Installing extension {} v{}", name, version);
                crate::extensions::install_to_profile(&xpi_data, &self.profile_path, &id)?;
                installed.push(id);
            }
        }

        if !installed.is_empty() {
            info!(count = installed.len(), "Installed pending extensions");
        }

        Ok(installed)
    }

    /// Remove extensions that have been uninstalled (in db but marked for removal)
    #[allow(clippy::cognitive_complexity)] // Loop with file system checks
    pub fn remove_uninstalled_extensions(&self) -> Result<Vec<String>> {
        let extensions_dir = self.profile_path.join("extensions");
        let mut removed = Vec::new();

        if !extensions_dir.exists() {
            return Ok(removed);
        }

        // Get list of extensions in the database
        let known_extensions = self.state_db.get_extensions()?;
        let known_ids: std::collections::HashSet<_> = known_extensions
            .iter()
            .map(|(id, _, _)| id.clone())
            .collect();

        // Find XPI files that aren't in the database
        for entry in std::fs::read_dir(&extensions_dir)? {
            let entry = entry?;
            let path = entry.path();

            let is_xpi = path.extension().map(|e| e == "xpi").unwrap_or(false);
            let stem = path.file_stem().and_then(|s| s.to_str());

            if is_xpi && let Some(stem) = stem {
                // Check if this is a managed extension (has XPI data in our db or is tracked)
                let has_xpi_data = self.state_db.get_extension_xpi(stem)?.is_some();
                if has_xpi_data && !known_ids.contains(stem) {
                    info!("Removing uninstalled extension {}", stem);
                    std::fs::remove_file(&path)?;
                    self.state_db.remove_extension_xpi(stem)?;
                    removed.push(stem.to_string());
                }
            }
        }

        if !removed.is_empty() {
            info!(count = removed.len(), "Removed uninstalled extensions");
        }

        Ok(removed)
    }
}

#[derive(Debug)]
pub struct SyncResult {
    pub incoming_applied: usize,
    pub outbound_written: usize,
    pub profile_files_written: Vec<String>,
    pub event_file: Option<PathBuf>,
}
