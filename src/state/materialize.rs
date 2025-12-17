use anyhow::Result;
use tracing::debug;

use crate::events::{Event, EventEnvelope, PrefValue};

use super::StateDb;

fn pref_to_storage(value: &PrefValue) -> (String, &'static str) {
    match value {
        PrefValue::Bool(b) => (b.to_string(), "bool"),
        PrefValue::Int(i) => (i.to_string(), "int"),
        PrefValue::String(s) => (s.clone(), "string"),
    }
}

pub fn materialize_events(
    db: &StateDb,
    events: &[EventEnvelope],
    this_device: &str,
) -> Result<usize> {
    let mut applied = 0;

    for envelope in events {
        if db.is_event_applied(envelope.id)? {
            continue;
        }

        apply_event(db, &envelope.event, this_device)?;
        db.mark_event_applied(
            envelope.id,
            &envelope.device,
            &envelope.timestamp.to_rfc3339(),
        )?;
        applied += 1;
        debug!(event_id = %envelope.id, event_type = ?std::mem::discriminant(&envelope.event), "Applied event");
    }

    Ok(applied)
}

#[allow(clippy::too_many_lines)] // Match arms for each event type - well-structured dispatcher
fn apply_event(db: &StateDb, event: &Event, this_device: &str) -> Result<()> {
    match event {
        Event::ExtensionAdded { id, name, url } => {
            db.add_extension(id, name, url.as_deref())?;
        }
        Event::ExtensionRemoved { id } => {
            db.remove_extension(id)?;
        }
        Event::ExtensionInstalled {
            id,
            name,
            version,
            source,
            xpi_data,
        } => {
            // Store extension metadata
            db.add_extension(id, name, None)?;
            // Store the XPI data for installation
            db.store_extension_xpi(id, version, source, xpi_data)?;
        }
        Event::ExtensionUninstalled { id } => {
            db.remove_extension(id)?;
            db.remove_extension_xpi(id)?;
        }
        Event::ContainerAdded {
            id,
            name,
            color,
            icon,
        } => {
            db.add_container(id, name, color, icon)?;
        }
        Event::ContainerRemoved { id } => {
            db.remove_container(id)?;
        }
        Event::ContainerUpdated {
            id,
            name,
            color,
            icon,
        } => {
            // For updates, we need to preserve existing values
            // This is a simplified approach - just update if we have new values
            if let (Some(name), Some(color), Some(icon)) = (name, color, icon) {
                db.add_container(id, name, color, icon)?;
            }
        }
        Event::HandlerSet { protocol, handler } => {
            db.set_handler(protocol, handler)?;
        }
        Event::HandlerRemoved { protocol } => {
            db.remove_handler(protocol)?;
        }
        Event::SearchEngineAdded { id, name, url } => {
            db.add_search_engine(id, name, url)?;
        }
        Event::SearchEngineRemoved { id } => {
            db.remove_search_engine(id)?;
        }
        Event::SearchEngineDefault { id } => {
            db.set_default_search_engine(id)?;
        }
        Event::PrefSet { key, value } => {
            let (value_str, type_str) = pref_to_storage(value);
            db.set_pref(key, &value_str, type_str)?;
        }
        Event::PrefRemoved { key } => {
            db.remove_pref(key)?;
        }
        Event::TabSent {
            to_device,
            url,
            title,
        } => {
            // Only store if this tab is for us
            if to_device == this_device {
                let id = uuid::Uuid::now_v7().to_string();
                let sent_at = chrono::Utc::now().to_rfc3339();
                db.add_pending_tab(&id, url, title.as_deref(), to_device, &sent_at)?;
            }
        }
        Event::TabReceived { event_id } => {
            db.remove_pending_tab(&event_id.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{ExtensionSource, VectorClock};

    #[test]
    fn test_materialize_extension_events() {
        let db = StateDb::open_in_memory().unwrap();

        let events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::ExtensionAdded {
                id: "ext1@test.com".to_string(),
                name: "Test Extension".to_string(),
                url: Some("https://example.com".to_string()),
            },
        )];

        let applied = materialize_events(&db, &events, "device-b").unwrap();
        assert_eq!(applied, 1);

        let extensions = db.get_extensions().unwrap();
        assert_eq!(extensions.len(), 1);
        assert_eq!(extensions[0].0, "ext1@test.com");

        // Applying same events again should be idempotent
        let applied = materialize_events(&db, &events, "device-b").unwrap();
        assert_eq!(applied, 0);
    }

    #[test]
    fn test_materialize_tab_sent() {
        let db = StateDb::open_in_memory().unwrap();

        let events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::TabSent {
                to_device: "device-b".to_string(),
                url: "https://example.com".to_string(),
                title: Some("Example".to_string()),
            },
        )];

        materialize_events(&db, &events, "device-b").unwrap();

        let tabs = db.get_pending_tabs().unwrap();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].url, "https://example.com");
    }

    #[test]
    fn test_materialize_extension_removed() {
        let db = StateDb::open_in_memory().unwrap();

        // First add an extension
        let add_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::ExtensionAdded {
                id: "ext1@test.com".to_string(),
                name: "Test Extension".to_string(),
                url: None,
            },
        )];
        materialize_events(&db, &add_events, "device-b").unwrap();
        assert_eq!(db.get_extensions().unwrap().len(), 1);

        // Then remove it
        let mut clock = VectorClock::new();
        clock.increment("device-a");
        let remove_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            clock,
            Event::ExtensionRemoved {
                id: "ext1@test.com".to_string(),
            },
        )];
        materialize_events(&db, &remove_events, "device-b").unwrap();
        assert_eq!(db.get_extensions().unwrap().len(), 0);
    }

    #[test]
    fn test_materialize_extension_installed() {
        let db = StateDb::open_in_memory().unwrap();

        let events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::ExtensionInstalled {
                id: "ext1@test.com".to_string(),
                name: "Test Extension".to_string(),
                version: "1.0.0".to_string(),
                source: ExtensionSource::Local {
                    original_path: "/path/to/ext.xpi".to_string(),
                },
                xpi_data: "base64data".to_string(),
            },
        )];

        materialize_events(&db, &events, "device-b").unwrap();

        let extensions = db.get_extensions().unwrap();
        assert_eq!(extensions.len(), 1);

        let xpi = db.get_extension_xpi("ext1@test.com").unwrap();
        assert!(xpi.is_some());
        let (version, data) = xpi.unwrap();
        assert_eq!(version, "1.0.0");
        assert_eq!(data, "base64data");
    }

    #[test]
    fn test_materialize_extension_uninstalled() {
        let db = StateDb::open_in_memory().unwrap();

        // First install
        let install_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::ExtensionInstalled {
                id: "ext1@test.com".to_string(),
                name: "Test Extension".to_string(),
                version: "1.0.0".to_string(),
                source: ExtensionSource::Local {
                    original_path: "/path".to_string(),
                },
                xpi_data: "data".to_string(),
            },
        )];
        materialize_events(&db, &install_events, "device-b").unwrap();

        // Then uninstall
        let mut clock = VectorClock::new();
        clock.increment("device-a");
        let uninstall_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            clock,
            Event::ExtensionUninstalled {
                id: "ext1@test.com".to_string(),
            },
        )];
        materialize_events(&db, &uninstall_events, "device-b").unwrap();

        assert_eq!(db.get_extensions().unwrap().len(), 0);
        assert!(db.get_extension_xpi("ext1@test.com").unwrap().is_none());
    }

    #[test]
    fn test_materialize_container_events() {
        let db = StateDb::open_in_memory().unwrap();

        // Add container
        let events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::ContainerAdded {
                id: "1".to_string(),
                name: "Work".to_string(),
                color: "blue".to_string(),
                icon: "briefcase".to_string(),
            },
        )];
        let applied = materialize_events(&db, &events, "device-b").unwrap();
        assert_eq!(applied, 1);

        // Remove container
        let mut clock = VectorClock::new();
        clock.increment("device-a");
        let remove_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            clock,
            Event::ContainerRemoved {
                id: "1".to_string(),
            },
        )];
        materialize_events(&db, &remove_events, "device-b").unwrap();
    }

    #[test]
    fn test_materialize_container_updated() {
        let db = StateDb::open_in_memory().unwrap();

        // Add container first
        let add_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::ContainerAdded {
                id: "1".to_string(),
                name: "Work".to_string(),
                color: "blue".to_string(),
                icon: "briefcase".to_string(),
            },
        )];
        materialize_events(&db, &add_events, "device-b").unwrap();

        // Update container
        let mut clock = VectorClock::new();
        clock.increment("device-a");
        let update_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            clock,
            Event::ContainerUpdated {
                id: "1".to_string(),
                name: Some("Work Updated".to_string()),
                color: Some("red".to_string()),
                icon: Some("circle".to_string()),
            },
        )];
        materialize_events(&db, &update_events, "device-b").unwrap();
    }

    #[test]
    fn test_materialize_handler_events() {
        let db = StateDb::open_in_memory().unwrap();

        // Set handler
        let events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::HandlerSet {
                protocol: "mailto".to_string(),
                handler: "thunderbird".to_string(),
            },
        )];
        materialize_events(&db, &events, "device-b").unwrap();

        // Remove handler
        let mut clock = VectorClock::new();
        clock.increment("device-a");
        let remove_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            clock,
            Event::HandlerRemoved {
                protocol: "mailto".to_string(),
            },
        )];
        materialize_events(&db, &remove_events, "device-b").unwrap();
    }

    #[test]
    fn test_materialize_search_engine_events() {
        let db = StateDb::open_in_memory().unwrap();

        // Add search engine
        let events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::SearchEngineAdded {
                id: "ddg".to_string(),
                name: "DuckDuckGo".to_string(),
                url: "https://duckduckgo.com/?q=%s".to_string(),
            },
        )];
        materialize_events(&db, &events, "device-b").unwrap();

        // Set default
        let mut clock = VectorClock::new();
        clock.increment("device-a");
        let default_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            clock.clone(),
            Event::SearchEngineDefault {
                id: "ddg".to_string(),
            },
        )];
        materialize_events(&db, &default_events, "device-b").unwrap();

        // Remove search engine
        clock.increment("device-a");
        let remove_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            clock,
            Event::SearchEngineRemoved {
                id: "ddg".to_string(),
            },
        )];
        materialize_events(&db, &remove_events, "device-b").unwrap();
    }

    #[test]
    fn test_materialize_pref_events() {
        let db = StateDb::open_in_memory().unwrap();

        // Set prefs of different types
        let events = vec![
            EventEnvelope::new(
                "device-a".to_string(),
                VectorClock::new(),
                Event::PrefSet {
                    key: "browser.bool".to_string(),
                    value: PrefValue::Bool(true),
                },
            ),
            EventEnvelope::new(
                "device-a".to_string(),
                VectorClock::new(),
                Event::PrefSet {
                    key: "browser.int".to_string(),
                    value: PrefValue::Int(42),
                },
            ),
            EventEnvelope::new(
                "device-a".to_string(),
                VectorClock::new(),
                Event::PrefSet {
                    key: "browser.string".to_string(),
                    value: PrefValue::String("value".to_string()),
                },
            ),
        ];
        let applied = materialize_events(&db, &events, "device-b").unwrap();
        assert_eq!(applied, 3);

        // Remove pref
        let mut clock = VectorClock::new();
        clock.increment("device-a");
        let remove_events = vec![EventEnvelope::new(
            "device-a".to_string(),
            clock,
            Event::PrefRemoved {
                key: "browser.bool".to_string(),
            },
        )];
        materialize_events(&db, &remove_events, "device-b").unwrap();
    }

    #[test]
    fn test_materialize_tab_sent_to_other_device() {
        let db = StateDb::open_in_memory().unwrap();

        // Tab sent to a different device should not create pending tab
        let events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::TabSent {
                to_device: "device-c".to_string(),
                url: "https://example.com".to_string(),
                title: None,
            },
        )];

        materialize_events(&db, &events, "device-b").unwrap();

        let tabs = db.get_pending_tabs().unwrap();
        assert!(tabs.is_empty());
    }

    #[test]
    fn test_materialize_tab_received() {
        let db = StateDb::open_in_memory().unwrap();

        // First add a pending tab directly
        db.add_pending_tab(
            "tab-uuid",
            "https://example.com",
            Some("Example"),
            "device-a",
            "2024-01-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(db.get_pending_tabs().unwrap().len(), 1);

        // Then mark it received
        let events = vec![EventEnvelope::new(
            "device-a".to_string(),
            VectorClock::new(),
            Event::TabReceived {
                event_id: uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap(),
            },
        )];
        materialize_events(&db, &events, "device-b").unwrap();

        // Note: TabReceived removes by event_id converted to string
        // Our test tab has "tab-uuid" so it won't match
    }

    #[test]
    fn test_pref_to_storage() {
        let (val, typ) = pref_to_storage(&PrefValue::Bool(true));
        assert_eq!(val, "true");
        assert_eq!(typ, "bool");

        let (val, typ) = pref_to_storage(&PrefValue::Bool(false));
        assert_eq!(val, "false");
        assert_eq!(typ, "bool");

        let (val, typ) = pref_to_storage(&PrefValue::Int(42));
        assert_eq!(val, "42");
        assert_eq!(typ, "int");

        let (val, typ) = pref_to_storage(&PrefValue::Int(-100));
        assert_eq!(val, "-100");
        assert_eq!(typ, "int");

        let (val, typ) = pref_to_storage(&PrefValue::String("hello".to_string()));
        assert_eq!(val, "hello");
        assert_eq!(typ, "string");
    }

    #[test]
    fn test_materialize_multiple_events_ordering() {
        let db = StateDb::open_in_memory().unwrap();

        let mut clock = VectorClock::new();

        // Multiple events in sequence
        let events = vec![
            EventEnvelope::new(
                "device-a".to_string(),
                clock.clone(),
                Event::ExtensionAdded {
                    id: "ext1@test.com".to_string(),
                    name: "Extension 1".to_string(),
                    url: None,
                },
            ),
            {
                clock.increment("device-a");
                EventEnvelope::new(
                    "device-a".to_string(),
                    clock.clone(),
                    Event::ExtensionAdded {
                        id: "ext2@test.com".to_string(),
                        name: "Extension 2".to_string(),
                        url: None,
                    },
                )
            },
            {
                clock.increment("device-a");
                EventEnvelope::new(
                    "device-a".to_string(),
                    clock.clone(),
                    Event::ExtensionRemoved {
                        id: "ext1@test.com".to_string(),
                    },
                )
            },
        ];

        let applied = materialize_events(&db, &events, "device-b").unwrap();
        assert_eq!(applied, 3);

        let extensions = db.get_extensions().unwrap();
        assert_eq!(extensions.len(), 1);
        assert_eq!(extensions[0].0, "ext2@test.com");
    }

    #[test]
    fn test_materialize_empty_events() {
        let db = StateDb::open_in_memory().unwrap();
        let events: Vec<EventEnvelope> = vec![];
        let applied = materialize_events(&db, &events, "device-b").unwrap();
        assert_eq!(applied, 0);
    }
}
