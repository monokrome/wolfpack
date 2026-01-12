use anyhow::{Context, Result};
use prefer::{ConfigValue, FromValue};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub device: DeviceConfig,
    pub paths: PathConfig,
    pub sync: SyncConfig,
    pub api: ApiConfig,
    pub prefs: PrefsConfig,
}

#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PathConfig {
    pub profile: Option<PathBuf>,
    pub sync_dir: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct SyncConfig {
    /// Port for P2P connections (0 for random)
    pub listen_port: Option<u16>,
    /// Enable mDNS for local network discovery (default: false)
    pub enable_mdns: bool,
    /// Enable DHT for internet-wide discovery (default: false)
    pub enable_dht: bool,
    /// Bootstrap peers for DHT (multiaddr format)
    pub bootstrap_peers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// HTTP API port for web extension communication (default: 9778)
    pub port: Option<u16>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self { port: Some(9778) }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PrefsConfig {
    pub whitelist: Vec<String>,
}

// FromValue implementations for prefer integration

impl FromValue for Config {
    fn from_value(value: &ConfigValue) -> prefer::Result<Self> {
        let obj = value
            .as_object()
            .ok_or_else(|| prefer::Error::ConversionError {
                key: String::new(),
                type_name: "Config".into(),
                source: "expected object".into(),
            })?;

        Ok(Self {
            device: DeviceConfig::from_value(obj.get("device").unwrap_or(&ConfigValue::Null))?,
            paths: PathConfig::from_value(obj.get("paths").unwrap_or(&ConfigValue::Null))?,
            sync: obj
                .get("sync")
                .map(SyncConfig::from_value)
                .transpose()?
                .unwrap_or_default(),
            api: obj
                .get("api")
                .map(ApiConfig::from_value)
                .transpose()?
                .unwrap_or_default(),
            prefs: obj
                .get("prefs")
                .map(PrefsConfig::from_value)
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

impl FromValue for DeviceConfig {
    fn from_value(value: &ConfigValue) -> prefer::Result<Self> {
        let obj = value
            .as_object()
            .ok_or_else(|| prefer::Error::ConversionError {
                key: String::new(),
                type_name: "DeviceConfig".into(),
                source: "expected object".into(),
            })?;

        Ok(Self {
            id: obj
                .get("id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| uuid::Uuid::now_v7().to_string()),
            name: obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| {
                    std::env::var("HOSTNAME")
                        .or_else(|_| std::env::var("HOST"))
                        .unwrap_or_else(|_| "unknown".to_string())
                }),
        })
    }
}

impl FromValue for PathConfig {
    fn from_value(value: &ConfigValue) -> prefer::Result<Self> {
        let obj = value
            .as_object()
            .ok_or_else(|| prefer::Error::ConversionError {
                key: String::new(),
                type_name: "PathConfig".into(),
                source: "expected object".into(),
            })?;

        Ok(Self {
            profile: obj
                .get("profile")
                .and_then(|v| v.as_str())
                .map(PathBuf::from),
            sync_dir: obj
                .get("sync_dir")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .unwrap_or_else(Config::default_sync_dir),
        })
    }
}

impl FromValue for SyncConfig {
    fn from_value(value: &ConfigValue) -> prefer::Result<Self> {
        let obj = value
            .as_object()
            .ok_or_else(|| prefer::Error::ConversionError {
                key: String::new(),
                type_name: "SyncConfig".into(),
                source: "expected object".into(),
            })?;

        Ok(Self {
            listen_port: obj
                .get("listen_port")
                .and_then(|v| v.as_u64())
                .map(|n| n as u16),
            enable_mdns: obj
                .get("enable_mdns")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            enable_dht: obj
                .get("enable_dht")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            bootstrap_peers: obj
                .get("bootstrap_peers")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

impl FromValue for ApiConfig {
    fn from_value(value: &ConfigValue) -> prefer::Result<Self> {
        let obj = value
            .as_object()
            .ok_or_else(|| prefer::Error::ConversionError {
                key: String::new(),
                type_name: "ApiConfig".into(),
                source: "expected object".into(),
            })?;

        Ok(Self {
            port: obj
                .get("port")
                .and_then(|v| v.as_u64())
                .map(|n| n as u16)
                .or(Some(9778)),
        })
    }
}

impl FromValue for PrefsConfig {
    fn from_value(value: &ConfigValue) -> prefer::Result<Self> {
        let obj = value
            .as_object()
            .ok_or_else(|| prefer::Error::ConversionError {
                key: String::new(),
                type_name: "PrefsConfig".into(),
                source: "expected object".into(),
            })?;

        Ok(Self {
            whitelist: obj
                .get("whitelist")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

impl Config {
    /// Load config using prefer's multi-format support
    /// This allows users to use any supported format (TOML, JSON, YAML, etc.)
    pub async fn load_prefer(name: &str) -> Result<Self> {
        let prefer_config = prefer::load(name)
            .await
            .with_context(|| format!("Failed to load config '{}'", name))?;

        Config::from_value(prefer_config.data())
            .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))
    }

    /// Load config from a specific path (TOML format)
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;

        // Parse TOML to ConfigValue
        let toml_value: toml::Value = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;

        let config_value = toml_to_config_value(toml_value);
        Config::from_value(&config_value)
            .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = self.to_toml_string()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory {}", parent.display())
            })?;
        }
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    fn to_toml_string(&self) -> Result<String> {
        let mut content = String::new();

        content.push_str("[device]\n");
        content.push_str(&format!("id = \"{}\"\n", self.device.id));
        content.push_str(&format!("name = \"{}\"\n", self.device.name));
        content.push('\n');

        content.push_str("[paths]\n");
        if let Some(ref profile) = self.paths.profile {
            content.push_str(&format!("profile = \"{}\"\n", profile.display()));
        }
        content.push_str(&format!(
            "sync_dir = \"{}\"\n",
            self.paths.sync_dir.display()
        ));
        content.push('\n');

        content.push_str("[sync]\n");
        if let Some(port) = self.sync.listen_port {
            content.push_str(&format!("listen_port = {}\n", port));
        }
        content.push_str(&format!("enable_mdns = {}\n", self.sync.enable_mdns));
        content.push_str(&format!("enable_dht = {}\n", self.sync.enable_dht));
        if !self.sync.bootstrap_peers.is_empty() {
            content.push_str(&format!(
                "bootstrap_peers = [{}]\n",
                self.sync
                    .bootstrap_peers
                    .iter()
                    .map(|p| format!("\"{}\"", p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        content.push('\n');

        content.push_str("[api]\n");
        if let Some(port) = self.api.port {
            content.push_str(&format!("port = {}\n", port));
        }
        content.push('\n');

        content.push_str("[prefs]\n");
        if !self.prefs.whitelist.is_empty() {
            content.push_str(&format!(
                "whitelist = [{}]\n",
                self.prefs
                    .whitelist
                    .iter()
                    .map(|p| format!("\"{}\"", p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        Ok(content)
    }

    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("wolfpack")
            .join("config.toml")
    }

    pub fn default_sync_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("wolfpack")
            .join("sync")
    }

    pub fn default_state_db() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("wolfpack")
            .join("state.db")
    }

    /// Get the path to the state database
    pub fn state_db_path(&self) -> PathBuf {
        self.paths.sync_dir.join("state.db")
    }

    /// Get the LibreWolf profile directory
    pub fn profile_dir(&self) -> Result<PathBuf> {
        if let Some(ref profile) = self.paths.profile {
            return Ok(profile.clone());
        }

        crate::profile::find_profile()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device: DeviceConfig {
                id: uuid::Uuid::now_v7().to_string(),
                name: std::env::var("HOSTNAME")
                    .or_else(|_| std::env::var("HOST"))
                    .unwrap_or_else(|_| "unknown".to_string()),
            },
            paths: PathConfig {
                profile: None,
                sync_dir: Self::default_sync_dir(),
            },
            sync: SyncConfig::default(),
            api: ApiConfig::default(),
            prefs: PrefsConfig::default(),
        }
    }
}

/// Convert toml::Value to prefer::ConfigValue
fn toml_to_config_value(value: toml::Value) -> ConfigValue {
    match value {
        toml::Value::String(s) => ConfigValue::String(s),
        toml::Value::Integer(i) => ConfigValue::Integer(i),
        toml::Value::Float(f) => ConfigValue::Float(f),
        toml::Value::Boolean(b) => ConfigValue::Bool(b),
        toml::Value::Datetime(dt) => ConfigValue::String(dt.to_string()),
        toml::Value::Array(arr) => {
            ConfigValue::Array(arr.into_iter().map(toml_to_config_value).collect())
        }
        toml::Value::Table(table) => ConfigValue::Object(
            table
                .into_iter()
                .map(|(k, v)| (k, toml_to_config_value(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_config_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let config = Config::default();
        config.save(&path).unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(config.device.id, loaded.device.id);
    }

    #[test]
    fn test_config_default_values() {
        let config = Config::default();

        // Device should have a UUID
        assert!(!config.device.id.is_empty());
        assert!(uuid::Uuid::parse_str(&config.device.id).is_ok());

        // Device name should be set (from env or "unknown")
        assert!(!config.device.name.is_empty());

        // Sync dir should be set
        assert!(!config.paths.sync_dir.as_os_str().is_empty());

        // Profile should be None by default (auto-detect)
        assert!(config.paths.profile.is_none());

        // API should default to port 9778
        assert_eq!(config.api.port, Some(9778));

        // DHT should be disabled by default
        assert!(!config.sync.enable_dht);

        // Bootstrap peers should be empty
        assert!(config.sync.bootstrap_peers.is_empty());

        // Prefs whitelist should be empty
        assert!(config.prefs.whitelist.is_empty());
    }

    #[test]
    fn test_config_with_custom_values() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut config = Config::default();
        config.device.name = "test-device".to_string();
        config.paths.profile = Some(PathBuf::from("/custom/profile"));
        config.sync.enable_dht = true;
        config.sync.listen_port = Some(9999);
        config.sync.bootstrap_peers = vec!["/ip4/1.2.3.4/tcp/4001".to_string()];
        config.api.port = Some(8080);
        config.prefs.whitelist = vec!["browser.*".to_string(), "extensions.*".to_string()];

        config.save(&path).unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.device.name, "test-device");
        assert_eq!(loaded.paths.profile, Some(PathBuf::from("/custom/profile")));
        assert!(loaded.sync.enable_dht);
        assert_eq!(loaded.sync.listen_port, Some(9999));
        assert_eq!(loaded.sync.bootstrap_peers.len(), 1);
        assert_eq!(loaded.api.port, Some(8080));
        assert_eq!(loaded.prefs.whitelist.len(), 2);
    }

    #[test]
    fn test_config_load_nonexistent() {
        let result = Config::load(Path::new("/nonexistent/config.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_config_load_invalid_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml {{{").unwrap();

        let result = Config::load(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_save_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("subdir").join("nested").join("config.toml");

        let config = Config::default();
        config.save(&path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_default_paths() {
        let config_path = Config::default_path();
        assert!(config_path.to_string_lossy().contains("wolfpack"));
        assert!(config_path.to_string_lossy().ends_with("config.toml"));

        let sync_dir = Config::default_sync_dir();
        assert!(sync_dir.to_string_lossy().contains("wolfpack"));
        assert!(sync_dir.to_string_lossy().ends_with("sync"));

        let state_db = Config::default_state_db();
        assert!(state_db.to_string_lossy().contains("wolfpack"));
        assert!(state_db.to_string_lossy().ends_with("state.db"));
    }

    #[test]
    fn test_config_path_uses_xdg_config_home() {
        let config_path = Config::default_path();
        // Config should be in XDG_CONFIG_HOME (or ~/.config on Unix)
        // not in XDG_DATA_HOME (or ~/.local/share on Unix)
        #[cfg(target_os = "linux")]
        {
            let path_str = config_path.to_string_lossy();
            assert!(
                path_str.contains(".config") || path_str.contains("config"),
                "Config path should use XDG_CONFIG_HOME, got: {}",
                path_str
            );
            assert!(
                !path_str.contains(".local/share"),
                "Config path should not be in .local/share, got: {}",
                path_str
            );
        }
    }

    #[test]
    fn test_data_paths_use_xdg_data_home() {
        // Sync dir and state db should be in XDG_DATA_HOME
        let sync_dir = Config::default_sync_dir();
        let state_db = Config::default_state_db();

        #[cfg(target_os = "linux")]
        {
            let sync_str = sync_dir.to_string_lossy();
            let state_str = state_db.to_string_lossy();

            assert!(
                sync_str.contains(".local/share") || sync_str.contains("share"),
                "Sync dir should use XDG_DATA_HOME, got: {}",
                sync_str
            );
            assert!(
                state_str.contains(".local/share") || state_str.contains("share"),
                "State DB should use XDG_DATA_HOME, got: {}",
                state_str
            );
        }
    }

    #[test]
    fn test_state_db_path() {
        let mut config = Config::default();
        config.paths.sync_dir = PathBuf::from("/custom/sync");

        let state_db = config.state_db_path();
        assert_eq!(state_db, PathBuf::from("/custom/sync/state.db"));
    }

    #[test]
    fn test_api_config_default() {
        let api = ApiConfig::default();
        assert_eq!(api.port, Some(9778));
    }

    #[test]
    fn test_sync_config_default() {
        let sync = SyncConfig::default();
        assert!(!sync.enable_dht);
        assert!(sync.listen_port.is_none());
        assert!(sync.bootstrap_peers.is_empty());
    }

    #[test]
    fn test_prefs_config_default() {
        let prefs = PrefsConfig::default();
        assert!(prefs.whitelist.is_empty());
    }

    #[test]
    fn test_config_profile_dir_explicit() {
        let mut config = Config::default();
        config.paths.profile = Some(PathBuf::from("/explicit/profile"));

        let result = config.profile_dir().unwrap();
        assert_eq!(result, PathBuf::from("/explicit/profile"));
    }

    #[test]
    fn test_config_profile_dir_no_librewolf() {
        let config = Config::default();
        // This will fail on most test systems without LibreWolf installed
        // Just verify it returns an error rather than panicking
        let result = config.profile_dir();
        // Either succeeds (LibreWolf installed) or fails gracefully
        match result {
            Ok(path) => assert!(!path.as_os_str().is_empty()),
            Err(e) => {
                assert!(e.to_string().contains("LibreWolf") || e.to_string().contains("home"))
            }
        }
    }

    #[test]
    fn test_from_value_config() {
        use std::collections::HashMap;

        let mut device = HashMap::new();
        device.insert("id".to_string(), ConfigValue::String("test-id".to_string()));
        device.insert(
            "name".to_string(),
            ConfigValue::String("test-name".to_string()),
        );

        let mut paths = HashMap::new();
        paths.insert(
            "sync_dir".to_string(),
            ConfigValue::String("/tmp/sync".to_string()),
        );

        let mut root = HashMap::new();
        root.insert("device".to_string(), ConfigValue::Object(device));
        root.insert("paths".to_string(), ConfigValue::Object(paths));

        let config = Config::from_value(&ConfigValue::Object(root)).unwrap();
        assert_eq!(config.device.id, "test-id");
        assert_eq!(config.device.name, "test-name");
        assert_eq!(config.paths.sync_dir, PathBuf::from("/tmp/sync"));
    }
}
