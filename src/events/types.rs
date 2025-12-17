use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::VectorClock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    // Extensions (legacy - just tracking, no data)
    ExtensionAdded {
        id: String,
        name: String,
        url: Option<String>,
    },
    ExtensionRemoved {
        id: String,
    },

    // Extension with full XPI data (from source install)
    ExtensionInstalled {
        id: String,
        name: String,
        version: String,
        source: ExtensionSource,
        /// Zstd-compressed XPI, base64 encoded
        xpi_data: String,
    },
    ExtensionUninstalled {
        id: String,
    },

    // Containers
    ContainerAdded {
        id: String,
        name: String,
        color: String,
        icon: String,
    },
    ContainerRemoved {
        id: String,
    },
    ContainerUpdated {
        id: String,
        name: Option<String>,
        color: Option<String>,
        icon: Option<String>,
    },

    // Protocol handlers
    HandlerSet {
        protocol: String,
        handler: String,
    },
    HandlerRemoved {
        protocol: String,
    },

    // Search engines
    SearchEngineAdded {
        id: String,
        name: String,
        url: String,
    },
    SearchEngineRemoved {
        id: String,
    },
    SearchEngineDefault {
        id: String,
    },

    // Preferences
    PrefSet {
        key: String,
        value: PrefValue,
    },
    PrefRemoved {
        key: String,
    },

    // Tabs
    TabSent {
        to_device: String,
        url: String,
        title: Option<String>,
    },
    TabReceived {
        event_id: Uuid,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PrefValue {
    Bool(bool),
    Int(i64),
    String(String),
}

/// Source of an extension installation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ExtensionSource {
    /// Built from a git repository
    Git {
        url: String,
        /// Tag, branch, or commit hash
        ref_spec: String,
        /// Build command used (for reference/updates)
        build_cmd: Option<String>,
    },
    /// Downloaded from AMO (addons.mozilla.org)
    Amo { amo_slug: String },
    /// Local file (path is just metadata, XPI is in event)
    Local { original_path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub device: String,
    pub clock: VectorClock,
    pub event: Event,
}

impl EventEnvelope {
    pub fn new(device: String, clock: VectorClock, event: Event) -> Self {
        Self {
            id: Uuid::now_v7(),
            timestamp: Utc::now(),
            device,
            clock,
            event,
        }
    }
}

impl Event {
    pub fn is_tab_for_device(&self, device: &str) -> bool {
        match self {
            Event::TabSent { to_device, .. } => to_device == device,
            _ => false,
        }
    }

    pub fn entity_id(&self) -> Option<&str> {
        match self {
            Event::ExtensionAdded { id, .. }
            | Event::ExtensionRemoved { id }
            | Event::ExtensionInstalled { id, .. }
            | Event::ExtensionUninstalled { id } => Some(id),
            Event::ContainerAdded { id, .. }
            | Event::ContainerRemoved { id }
            | Event::ContainerUpdated { id, .. } => Some(id),
            Event::HandlerSet { protocol, .. } | Event::HandlerRemoved { protocol } => {
                Some(protocol)
            }
            Event::SearchEngineAdded { id, .. }
            | Event::SearchEngineRemoved { id }
            | Event::SearchEngineDefault { id } => Some(id),
            Event::PrefSet { key, .. } | Event::PrefRemoved { key } => Some(key),
            Event::TabSent { .. } | Event::TabReceived { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = Event::ExtensionAdded {
            id: "uBlock0@raymondhill.net".to_string(),
            name: "uBlock Origin".to_string(),
            url: Some("https://addons.mozilla.org/firefox/addon/ublock-origin/".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn test_pref_value_serialization() {
        let cases = vec![
            (PrefValue::Bool(true), "true"),
            (PrefValue::Int(42), "42"),
            (PrefValue::String("test".to_string()), "\"test\""),
        ];

        for (value, expected) in cases {
            let json = serde_json::to_string(&value).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_all_event_types_serialize() {
        let events = vec![
            Event::ExtensionAdded {
                id: "ext@test.com".to_string(),
                name: "Test".to_string(),
                url: None,
            },
            Event::ExtensionRemoved {
                id: "ext@test.com".to_string(),
            },
            Event::ExtensionInstalled {
                id: "ext@test.com".to_string(),
                name: "Test".to_string(),
                version: "1.0.0".to_string(),
                source: ExtensionSource::Local {
                    original_path: "/path".to_string(),
                },
                xpi_data: "data".to_string(),
            },
            Event::ExtensionUninstalled {
                id: "ext@test.com".to_string(),
            },
            Event::ContainerAdded {
                id: "1".to_string(),
                name: "Work".to_string(),
                color: "blue".to_string(),
                icon: "briefcase".to_string(),
            },
            Event::ContainerRemoved {
                id: "1".to_string(),
            },
            Event::ContainerUpdated {
                id: "1".to_string(),
                name: Some("Work Updated".to_string()),
                color: None,
                icon: None,
            },
            Event::HandlerSet {
                protocol: "mailto".to_string(),
                handler: "thunderbird".to_string(),
            },
            Event::HandlerRemoved {
                protocol: "mailto".to_string(),
            },
            Event::SearchEngineAdded {
                id: "ddg".to_string(),
                name: "DuckDuckGo".to_string(),
                url: "https://duckduckgo.com/?q=%s".to_string(),
            },
            Event::SearchEngineRemoved {
                id: "ddg".to_string(),
            },
            Event::SearchEngineDefault {
                id: "ddg".to_string(),
            },
            Event::PrefSet {
                key: "browser.startup.homepage".to_string(),
                value: PrefValue::String("https://example.com".to_string()),
            },
            Event::PrefRemoved {
                key: "browser.startup.homepage".to_string(),
            },
            Event::TabSent {
                to_device: "device-b".to_string(),
                url: "https://example.com".to_string(),
                title: Some("Example".to_string()),
            },
            Event::TabReceived {
                event_id: Uuid::nil(),
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: Event = serde_json::from_str(&json).unwrap();
            assert_eq!(event, parsed);
        }
    }

    #[test]
    fn test_extension_source_serialization() {
        let sources = vec![
            ExtensionSource::Git {
                url: "https://github.com/example/ext.git".to_string(),
                ref_spec: "v1.0.0".to_string(),
                build_cmd: Some("npm run build".to_string()),
            },
            ExtensionSource::Git {
                url: "https://github.com/example/ext.git".to_string(),
                ref_spec: "main".to_string(),
                build_cmd: None,
            },
            ExtensionSource::Amo {
                amo_slug: "ublock-origin".to_string(),
            },
            ExtensionSource::Local {
                original_path: "/path/to/ext.xpi".to_string(),
            },
        ];

        for source in sources {
            let json = serde_json::to_string(&source).unwrap();
            let parsed: ExtensionSource = serde_json::from_str(&json).unwrap();
            assert_eq!(source, parsed);
        }
    }

    #[test]
    fn test_event_envelope_creation() {
        let clock = VectorClock::new();
        let event = Event::ExtensionAdded {
            id: "ext@test.com".to_string(),
            name: "Test".to_string(),
            url: None,
        };

        let envelope = EventEnvelope::new("device-a".to_string(), clock, event);

        assert_eq!(envelope.device, "device-a");
        assert!(!envelope.id.is_nil());
    }

    #[test]
    fn test_is_tab_for_device() {
        let tab_event = Event::TabSent {
            to_device: "device-b".to_string(),
            url: "https://example.com".to_string(),
            title: None,
        };

        assert!(tab_event.is_tab_for_device("device-b"));
        assert!(!tab_event.is_tab_for_device("device-a"));

        let other_event = Event::ExtensionAdded {
            id: "ext@test.com".to_string(),
            name: "Test".to_string(),
            url: None,
        };
        assert!(!other_event.is_tab_for_device("device-a"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_entity_id() {
        let cases: Vec<(Event, Option<&str>)> = vec![
            (
                Event::ExtensionAdded {
                    id: "ext@test.com".to_string(),
                    name: "Test".to_string(),
                    url: None,
                },
                Some("ext@test.com"),
            ),
            (
                Event::ExtensionRemoved {
                    id: "ext@test.com".to_string(),
                },
                Some("ext@test.com"),
            ),
            (
                Event::ExtensionInstalled {
                    id: "ext@test.com".to_string(),
                    name: "Test".to_string(),
                    version: "1.0".to_string(),
                    source: ExtensionSource::Local {
                        original_path: "/path".to_string(),
                    },
                    xpi_data: "".to_string(),
                },
                Some("ext@test.com"),
            ),
            (
                Event::ExtensionUninstalled {
                    id: "ext@test.com".to_string(),
                },
                Some("ext@test.com"),
            ),
            (
                Event::ContainerAdded {
                    id: "1".to_string(),
                    name: "Work".to_string(),
                    color: "blue".to_string(),
                    icon: "briefcase".to_string(),
                },
                Some("1"),
            ),
            (
                Event::ContainerRemoved {
                    id: "1".to_string(),
                },
                Some("1"),
            ),
            (
                Event::ContainerUpdated {
                    id: "1".to_string(),
                    name: None,
                    color: None,
                    icon: None,
                },
                Some("1"),
            ),
            (
                Event::HandlerSet {
                    protocol: "mailto".to_string(),
                    handler: "app".to_string(),
                },
                Some("mailto"),
            ),
            (
                Event::HandlerRemoved {
                    protocol: "mailto".to_string(),
                },
                Some("mailto"),
            ),
            (
                Event::SearchEngineAdded {
                    id: "ddg".to_string(),
                    name: "DDG".to_string(),
                    url: "url".to_string(),
                },
                Some("ddg"),
            ),
            (
                Event::SearchEngineRemoved {
                    id: "ddg".to_string(),
                },
                Some("ddg"),
            ),
            (
                Event::SearchEngineDefault {
                    id: "ddg".to_string(),
                },
                Some("ddg"),
            ),
            (
                Event::PrefSet {
                    key: "some.pref".to_string(),
                    value: PrefValue::Bool(true),
                },
                Some("some.pref"),
            ),
            (
                Event::PrefRemoved {
                    key: "some.pref".to_string(),
                },
                Some("some.pref"),
            ),
            (
                Event::TabSent {
                    to_device: "device".to_string(),
                    url: "url".to_string(),
                    title: None,
                },
                None,
            ),
            (
                Event::TabReceived {
                    event_id: Uuid::nil(),
                },
                None,
            ),
        ];

        for (event, expected) in cases {
            assert_eq!(event.entity_id(), expected);
        }
    }

    #[test]
    fn test_pref_value_types() {
        // Bool
        let b = PrefValue::Bool(false);
        assert_eq!(serde_json::to_string(&b).unwrap(), "false");

        // Negative int
        let n = PrefValue::Int(-42);
        assert_eq!(serde_json::to_string(&n).unwrap(), "-42");

        // String with special chars
        let s = PrefValue::String("hello \"world\"".to_string());
        let json = serde_json::to_string(&s).unwrap();
        let parsed: PrefValue = serde_json::from_str(&json).unwrap();
        assert_eq!(s, parsed);
    }
}
