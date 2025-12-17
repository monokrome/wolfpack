use crate::events::{EventEnvelope, VectorClock};

pub fn merge_events(
    local: &[EventEnvelope],
    remote: &[EventEnvelope],
    local_clock: &VectorClock,
) -> (Vec<EventEnvelope>, VectorClock) {
    let mut merged = Vec::new();
    let mut clock = local_clock.clone();

    // Collect all unique events by ID
    let mut seen = std::collections::HashSet::new();

    for event in local.iter().chain(remote.iter()) {
        if seen.insert(event.id) {
            merged.push(event.clone());
            clock.merge(&event.clock);
        }
    }

    // Sort by timestamp for deterministic ordering
    merged.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    (merged, clock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;

    #[test]
    fn test_merge_events() {
        let mut clock1 = VectorClock::new();
        clock1.set("A", 1);

        let mut clock2 = VectorClock::new();
        clock2.set("B", 1);

        let event1 = EventEnvelope::new(
            "A".to_string(),
            clock1.clone(),
            Event::ExtensionAdded {
                id: "ext1".to_string(),
                name: "Ext 1".to_string(),
                url: None,
            },
        );

        let event2 = EventEnvelope::new(
            "B".to_string(),
            clock2.clone(),
            Event::ExtensionAdded {
                id: "ext2".to_string(),
                name: "Ext 2".to_string(),
                url: None,
            },
        );

        let (merged, new_clock) = merge_events(&[event1], &[event2], &clock1);

        assert_eq!(merged.len(), 2);
        assert_eq!(new_clock.get("A"), 1);
        assert_eq!(new_clock.get("B"), 1);
    }
}
