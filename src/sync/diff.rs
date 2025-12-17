use std::collections::{HashMap, HashSet};

use crate::events::{Event, PrefValue};
use crate::profile::{Container, Extension, Handler};

/// Diff extensions: compare current extensions with known IDs
pub fn diff_extensions(current: &[Extension], previous: &[String]) -> Vec<Event> {
    let mut events = Vec::new();

    let current_ids: std::collections::HashSet<_> = current.iter().map(|e| &e.id).collect();
    let previous_ids: std::collections::HashSet<_> = previous.iter().collect();

    // Added extensions
    for ext in current {
        if !previous_ids.contains(&ext.id) {
            events.push(Event::ExtensionAdded {
                id: ext.id.clone(),
                name: ext.name.clone(),
                url: ext.url.clone(),
            });
        }
    }

    // Removed extensions
    for id in previous {
        if !current_ids.contains(id) {
            events.push(Event::ExtensionRemoved { id: id.clone() });
        }
    }

    events
}

/// Diff containers: compare current containers with known container IDs
pub fn diff_containers(current: &[Container], known_ids: &[String]) -> Vec<Event> {
    let mut events = Vec::new();

    let current_ids: HashSet<_> = current
        .iter()
        .map(|c| c.user_context_id.to_string())
        .collect();
    let known_set: HashSet<_> = known_ids.iter().cloned().collect();

    // Added containers
    for container in current {
        let id = container.user_context_id.to_string();
        if !known_set.contains(&id) {
            events.push(Event::ContainerAdded {
                id,
                name: container.name.clone(),
                color: container.color.clone(),
                icon: container.icon.clone(),
            });
        }
    }

    // Removed containers
    for id in known_ids {
        if !current_ids.contains(id) {
            events.push(Event::ContainerRemoved { id: id.clone() });
        }
    }

    events
}

/// Diff handlers: compare current handlers with known handlers (protocol -> handler)
pub fn diff_handlers(current: &[Handler], known: &HashMap<String, String>) -> Vec<Event> {
    let mut events = Vec::new();

    let current_protocols: HashSet<_> = current.iter().map(|h| h.protocol.clone()).collect();

    // Check for new or changed handlers
    for handler in current {
        match known.get(&handler.protocol) {
            None => events.push(Event::HandlerSet {
                protocol: handler.protocol.clone(),
                handler: handler.handler.clone(),
            }),
            Some(existing) if existing != &handler.handler => {
                events.push(Event::HandlerSet {
                    protocol: handler.protocol.clone(),
                    handler: handler.handler.clone(),
                });
            }
            _ => {}
        }
    }

    // Check for removed handlers
    for protocol in known.keys() {
        if !current_protocols.contains(protocol) {
            events.push(Event::HandlerRemoved {
                protocol: protocol.clone(),
            });
        }
    }

    events
}

/// Diff prefs: compare current prefs with known prefs
pub fn diff_prefs(
    current: &HashMap<String, PrefValue>,
    known: &HashMap<String, PrefValue>,
) -> Vec<Event> {
    let mut events = Vec::new();

    // Check for new or changed prefs
    for (key, value) in current {
        match known.get(key) {
            None => events.push(Event::PrefSet {
                key: key.clone(),
                value: value.clone(),
            }),
            Some(existing) if existing != value => {
                events.push(Event::PrefSet {
                    key: key.clone(),
                    value: value.clone(),
                });
            }
            _ => {}
        }
    }

    // Check for removed prefs
    for key in known.keys() {
        if !current.contains_key(key) {
            events.push(Event::PrefRemoved { key: key.clone() });
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_extension(id: &str, name: &str, url: Option<&str>) -> Extension {
        Extension {
            id: id.to_string(),
            name: name.to_string(),
            url: url.map(String::from),
        }
    }

    #[test]
    fn test_diff_extensions_no_changes() {
        let current = vec![
            make_extension("ext1@test.com", "Extension 1", None),
            make_extension("ext2@test.com", "Extension 2", Some("https://example.com")),
        ];
        let previous = vec!["ext1@test.com".to_string(), "ext2@test.com".to_string()];

        let events = diff_extensions(&current, &previous);
        assert!(events.is_empty());
    }

    #[test]
    fn test_diff_extensions_added() {
        let current = vec![
            make_extension("ext1@test.com", "Extension 1", None),
            make_extension("ext2@test.com", "Extension 2", Some("https://example.com")),
        ];
        let previous = vec!["ext1@test.com".to_string()];

        let events = diff_extensions(&current, &previous);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::ExtensionAdded { id, name, url } => {
                assert_eq!(id, "ext2@test.com");
                assert_eq!(name, "Extension 2");
                assert_eq!(url, &Some("https://example.com".to_string()));
            }
            _ => panic!("Expected ExtensionAdded event"),
        }
    }

    #[test]
    fn test_diff_extensions_removed() {
        let current = vec![make_extension("ext1@test.com", "Extension 1", None)];
        let previous = vec!["ext1@test.com".to_string(), "ext2@test.com".to_string()];

        let events = diff_extensions(&current, &previous);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::ExtensionRemoved { id } => {
                assert_eq!(id, "ext2@test.com");
            }
            _ => panic!("Expected ExtensionRemoved event"),
        }
    }

    #[test]
    fn test_diff_extensions_added_and_removed() {
        let current = vec![
            make_extension("ext1@test.com", "Extension 1", None),
            make_extension("ext3@test.com", "Extension 3", None),
        ];
        let previous = vec!["ext1@test.com".to_string(), "ext2@test.com".to_string()];

        let events = diff_extensions(&current, &previous);
        assert_eq!(events.len(), 2);

        let added = events
            .iter()
            .filter(|e| matches!(e, Event::ExtensionAdded { .. }))
            .count();
        let removed = events
            .iter()
            .filter(|e| matches!(e, Event::ExtensionRemoved { .. }))
            .count();
        assert_eq!(added, 1);
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_diff_extensions_empty_current() {
        let current: Vec<Extension> = vec![];
        let previous = vec!["ext1@test.com".to_string(), "ext2@test.com".to_string()];

        let events = diff_extensions(&current, &previous);
        assert_eq!(events.len(), 2);
        assert!(
            events
                .iter()
                .all(|e| matches!(e, Event::ExtensionRemoved { .. }))
        );
    }

    #[test]
    fn test_diff_extensions_empty_previous() {
        let current = vec![
            make_extension("ext1@test.com", "Extension 1", None),
            make_extension("ext2@test.com", "Extension 2", None),
        ];
        let previous: Vec<String> = vec![];

        let events = diff_extensions(&current, &previous);
        assert_eq!(events.len(), 2);
        assert!(
            events
                .iter()
                .all(|e| matches!(e, Event::ExtensionAdded { .. }))
        );
    }

    #[test]
    fn test_diff_extensions_both_empty() {
        let current: Vec<Extension> = vec![];
        let previous: Vec<String> = vec![];

        let events = diff_extensions(&current, &previous);
        assert!(events.is_empty());
    }

    // Container diff tests

    fn make_container(id: u32, name: &str, color: &str, icon: &str) -> Container {
        Container {
            user_context_id: id,
            name: name.to_string(),
            color: color.to_string(),
            icon: icon.to_string(),
            is_public: true,
        }
    }

    #[test]
    fn test_diff_containers_no_changes() {
        let current = vec![
            make_container(1, "Personal", "blue", "fingerprint"),
            make_container(2, "Work", "orange", "briefcase"),
        ];
        let known = vec!["1".to_string(), "2".to_string()];

        let events = diff_containers(&current, &known);
        assert!(events.is_empty());
    }

    #[test]
    fn test_diff_containers_added() {
        let current = vec![
            make_container(1, "Personal", "blue", "fingerprint"),
            make_container(2, "Work", "orange", "briefcase"),
        ];
        let known = vec!["1".to_string()];

        let events = diff_containers(&current, &known);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::ContainerAdded {
                id,
                name,
                color,
                icon,
            } => {
                assert_eq!(id, "2");
                assert_eq!(name, "Work");
                assert_eq!(color, "orange");
                assert_eq!(icon, "briefcase");
            }
            _ => panic!("Expected ContainerAdded event"),
        }
    }

    #[test]
    fn test_diff_containers_removed() {
        let current = vec![make_container(1, "Personal", "blue", "fingerprint")];
        let known = vec!["1".to_string(), "2".to_string()];

        let events = diff_containers(&current, &known);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::ContainerRemoved { id } => {
                assert_eq!(id, "2");
            }
            _ => panic!("Expected ContainerRemoved event"),
        }
    }

    #[test]
    fn test_diff_containers_added_and_removed() {
        let current = vec![
            make_container(1, "Personal", "blue", "fingerprint"),
            make_container(3, "Shopping", "pink", "cart"),
        ];
        let known = vec!["1".to_string(), "2".to_string()];

        let events = diff_containers(&current, &known);
        assert_eq!(events.len(), 2);

        let added = events
            .iter()
            .filter(|e| matches!(e, Event::ContainerAdded { .. }))
            .count();
        let removed = events
            .iter()
            .filter(|e| matches!(e, Event::ContainerRemoved { .. }))
            .count();
        assert_eq!(added, 1);
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_diff_containers_empty() {
        let current: Vec<Container> = vec![];
        let known: Vec<String> = vec![];

        let events = diff_containers(&current, &known);
        assert!(events.is_empty());
    }

    // Handler diff tests

    fn make_handler(protocol: &str, handler: &str) -> Handler {
        Handler {
            protocol: protocol.to_string(),
            handler: handler.to_string(),
        }
    }

    #[test]
    fn test_diff_handlers_no_changes() {
        let current = vec![
            make_handler("mailto", "gmail.com"),
            make_handler("web+custom", "example.com"),
        ];
        let known: HashMap<String, String> = [
            ("mailto".to_string(), "gmail.com".to_string()),
            ("web+custom".to_string(), "example.com".to_string()),
        ]
        .into_iter()
        .collect();

        let events = diff_handlers(&current, &known);
        assert!(events.is_empty());
    }

    #[test]
    fn test_diff_handlers_added() {
        let current = vec![
            make_handler("mailto", "gmail.com"),
            make_handler("web+custom", "example.com"),
        ];
        let known: HashMap<String, String> = [("mailto".to_string(), "gmail.com".to_string())]
            .into_iter()
            .collect();

        let events = diff_handlers(&current, &known);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::HandlerSet { protocol, handler } => {
                assert_eq!(protocol, "web+custom");
                assert_eq!(handler, "example.com");
            }
            _ => panic!("Expected HandlerSet event"),
        }
    }

    #[test]
    fn test_diff_handlers_changed() {
        let current = vec![make_handler("mailto", "outlook.com")];
        let known: HashMap<String, String> = [("mailto".to_string(), "gmail.com".to_string())]
            .into_iter()
            .collect();

        let events = diff_handlers(&current, &known);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::HandlerSet { protocol, handler } => {
                assert_eq!(protocol, "mailto");
                assert_eq!(handler, "outlook.com");
            }
            _ => panic!("Expected HandlerSet event"),
        }
    }

    #[test]
    fn test_diff_handlers_removed() {
        let current = vec![make_handler("mailto", "gmail.com")];
        let known: HashMap<String, String> = [
            ("mailto".to_string(), "gmail.com".to_string()),
            ("web+custom".to_string(), "example.com".to_string()),
        ]
        .into_iter()
        .collect();

        let events = diff_handlers(&current, &known);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::HandlerRemoved { protocol } => {
                assert_eq!(protocol, "web+custom");
            }
            _ => panic!("Expected HandlerRemoved event"),
        }
    }

    #[test]
    fn test_diff_handlers_empty() {
        let current: Vec<Handler> = vec![];
        let known: HashMap<String, String> = HashMap::new();

        let events = diff_handlers(&current, &known);
        assert!(events.is_empty());
    }

    // Pref diff tests

    #[test]
    fn test_diff_prefs_no_changes() {
        let current: HashMap<String, PrefValue> = [
            (
                "browser.startup.homepage".to_string(),
                PrefValue::String("https://example.com".to_string()),
            ),
            (
                "browser.tabs.warnOnClose".to_string(),
                PrefValue::Bool(true),
            ),
        ]
        .into_iter()
        .collect();

        let known = current.clone();

        let events = diff_prefs(&current, &known);
        assert!(events.is_empty());
    }

    #[test]
    fn test_diff_prefs_added() {
        let current: HashMap<String, PrefValue> = [
            (
                "browser.startup.homepage".to_string(),
                PrefValue::String("https://example.com".to_string()),
            ),
            (
                "browser.tabs.warnOnClose".to_string(),
                PrefValue::Bool(true),
            ),
        ]
        .into_iter()
        .collect();

        let known: HashMap<String, PrefValue> = [(
            "browser.startup.homepage".to_string(),
            PrefValue::String("https://example.com".to_string()),
        )]
        .into_iter()
        .collect();

        let events = diff_prefs(&current, &known);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::PrefSet { key, value } => {
                assert_eq!(key, "browser.tabs.warnOnClose");
                assert_eq!(value, &PrefValue::Bool(true));
            }
            _ => panic!("Expected PrefSet event"),
        }
    }

    #[test]
    fn test_diff_prefs_changed() {
        let current: HashMap<String, PrefValue> = [(
            "browser.startup.homepage".to_string(),
            PrefValue::String("https://new.com".to_string()),
        )]
        .into_iter()
        .collect();

        let known: HashMap<String, PrefValue> = [(
            "browser.startup.homepage".to_string(),
            PrefValue::String("https://old.com".to_string()),
        )]
        .into_iter()
        .collect();

        let events = diff_prefs(&current, &known);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::PrefSet { key, value } => {
                assert_eq!(key, "browser.startup.homepage");
                assert_eq!(value, &PrefValue::String("https://new.com".to_string()));
            }
            _ => panic!("Expected PrefSet event"),
        }
    }

    #[test]
    fn test_diff_prefs_removed() {
        let current: HashMap<String, PrefValue> = [(
            "browser.startup.homepage".to_string(),
            PrefValue::String("https://example.com".to_string()),
        )]
        .into_iter()
        .collect();

        let known: HashMap<String, PrefValue> = [
            (
                "browser.startup.homepage".to_string(),
                PrefValue::String("https://example.com".to_string()),
            ),
            (
                "browser.tabs.warnOnClose".to_string(),
                PrefValue::Bool(true),
            ),
        ]
        .into_iter()
        .collect();

        let events = diff_prefs(&current, &known);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::PrefRemoved { key } => {
                assert_eq!(key, "browser.tabs.warnOnClose");
            }
            _ => panic!("Expected PrefRemoved event"),
        }
    }

    #[test]
    fn test_diff_prefs_int_value() {
        let current: HashMap<String, PrefValue> = [(
            "browser.cache.disk.capacity".to_string(),
            PrefValue::Int(1024),
        )]
        .into_iter()
        .collect();

        let known: HashMap<String, PrefValue> = [(
            "browser.cache.disk.capacity".to_string(),
            PrefValue::Int(512),
        )]
        .into_iter()
        .collect();

        let events = diff_prefs(&current, &known);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::PrefSet { key, value } => {
                assert_eq!(key, "browser.cache.disk.capacity");
                assert_eq!(value, &PrefValue::Int(1024));
            }
            _ => panic!("Expected PrefSet event"),
        }
    }

    #[test]
    fn test_diff_prefs_empty() {
        let current: HashMap<String, PrefValue> = HashMap::new();
        let known: HashMap<String, PrefValue> = HashMap::new();

        let events = diff_prefs(&current, &known);
        assert!(events.is_empty());
    }
}
