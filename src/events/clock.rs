use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VectorClock(HashMap<String, u64>);

impl VectorClock {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn get(&self, device: &str) -> u64 {
        *self.0.get(device).unwrap_or(&0)
    }

    pub fn increment(&mut self, device: &str) {
        let count = self.0.entry(device.to_string()).or_insert(0);
        *count += 1;
    }

    pub fn set(&mut self, device: &str, value: u64) {
        self.0.insert(device.to_string(), value);
    }

    pub fn merge(&mut self, other: &VectorClock) {
        for (device, &count) in &other.0 {
            let current = self.0.entry(device.clone()).or_insert(0);
            *current = (*current).max(count);
        }
    }

    pub fn compare(&self, other: &VectorClock) -> Option<Ordering> {
        let mut less = false;
        let mut greater = false;

        let all_keys: std::collections::HashSet<_> = self.0.keys().chain(other.0.keys()).collect();

        for key in all_keys {
            let self_val = self.get(key);
            let other_val = other.get(key);

            match self_val.cmp(&other_val) {
                Ordering::Less => less = true,
                Ordering::Greater => greater = true,
                Ordering::Equal => {}
            }

            if less && greater {
                return None;
            }
        }

        match (less, greater) {
            (false, false) => Some(Ordering::Equal),
            (true, false) => Some(Ordering::Less),
            (false, true) => Some(Ordering::Greater),
            (true, true) => None,
        }
    }

    pub fn happens_before(&self, other: &VectorClock) -> bool {
        matches!(self.compare(other), Some(Ordering::Less))
    }

    pub fn concurrent_with(&self, other: &VectorClock) -> bool {
        self.compare(other).is_none()
    }

    pub fn devices(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &u64)> {
        self.0.iter()
    }

    pub fn entries(&self) -> impl Iterator<Item = (String, u64)> + '_ {
        self.0.iter().map(|(k, &v)| (k.clone(), v))
    }
}

impl From<HashMap<String, u64>> for VectorClock {
    fn from(map: HashMap<String, u64>) -> Self {
        Self(map)
    }
}

impl From<VectorClock> for HashMap<String, u64> {
    fn from(clock: VectorClock) -> Self {
        clock.0
    }
}

impl VectorClock {
    pub fn to_hashmap(&self) -> HashMap<String, u64> {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_increment() {
        let mut clock = VectorClock::new();
        assert_eq!(clock.get("A"), 0);

        clock.increment("A");
        assert_eq!(clock.get("A"), 1);

        clock.increment("A");
        assert_eq!(clock.get("A"), 2);
    }

    #[test]
    fn test_merge() {
        let mut clock1 = VectorClock::new();
        clock1.set("A", 2);
        clock1.set("B", 1);

        let mut clock2 = VectorClock::new();
        clock2.set("A", 1);
        clock2.set("B", 3);
        clock2.set("C", 1);

        clock1.merge(&clock2);

        assert_eq!(clock1.get("A"), 2);
        assert_eq!(clock1.get("B"), 3);
        assert_eq!(clock1.get("C"), 1);
    }

    #[test]
    fn test_compare() {
        let mut clock1 = VectorClock::new();
        clock1.set("A", 1);
        clock1.set("B", 2);

        let mut clock2 = VectorClock::new();
        clock2.set("A", 1);
        clock2.set("B", 2);

        assert_eq!(clock1.compare(&clock2), Some(Ordering::Equal));

        clock2.set("B", 3);
        assert_eq!(clock1.compare(&clock2), Some(Ordering::Less));
        assert_eq!(clock2.compare(&clock1), Some(Ordering::Greater));
    }

    #[test]
    fn test_concurrent() {
        let mut clock1 = VectorClock::new();
        clock1.set("A", 2);
        clock1.set("B", 1);

        let mut clock2 = VectorClock::new();
        clock2.set("A", 1);
        clock2.set("B", 2);

        assert!(clock1.concurrent_with(&clock2));
        assert!(clock2.concurrent_with(&clock1));
    }

    #[test]
    fn test_new_clock_is_empty() {
        let clock = VectorClock::new();
        assert_eq!(clock.get("any"), 0);
        assert_eq!(clock.devices().count(), 0);
    }

    #[test]
    fn test_set_and_get() {
        let mut clock = VectorClock::new();
        clock.set("device-a", 5);
        clock.set("device-b", 10);

        assert_eq!(clock.get("device-a"), 5);
        assert_eq!(clock.get("device-b"), 10);
        assert_eq!(clock.get("device-c"), 0);
    }

    #[test]
    fn test_set_overwrites() {
        let mut clock = VectorClock::new();
        clock.set("device", 5);
        assert_eq!(clock.get("device"), 5);

        clock.set("device", 3);
        assert_eq!(clock.get("device"), 3);
    }

    #[test]
    fn test_happens_before() {
        let mut clock1 = VectorClock::new();
        clock1.set("A", 1);

        let mut clock2 = VectorClock::new();
        clock2.set("A", 2);

        assert!(clock1.happens_before(&clock2));
        assert!(!clock2.happens_before(&clock1));
    }

    #[test]
    fn test_happens_before_equal() {
        let mut clock1 = VectorClock::new();
        clock1.set("A", 1);

        let mut clock2 = VectorClock::new();
        clock2.set("A", 1);

        assert!(!clock1.happens_before(&clock2));
        assert!(!clock2.happens_before(&clock1));
    }

    #[test]
    fn test_devices_iterator() {
        let mut clock = VectorClock::new();
        clock.set("device-a", 1);
        clock.set("device-b", 2);

        let devices: Vec<&String> = clock.devices().collect();
        assert_eq!(devices.len(), 2);
        assert!(devices.iter().any(|d| *d == "device-a"));
        assert!(devices.iter().any(|d| *d == "device-b"));
    }

    #[test]
    fn test_iter() {
        let mut clock = VectorClock::new();
        clock.set("a", 1);
        clock.set("b", 2);

        let entries: HashMap<String, u64> = clock.iter().map(|(k, &v)| (k.clone(), v)).collect();
        assert_eq!(entries.get("a"), Some(&1));
        assert_eq!(entries.get("b"), Some(&2));
    }

    #[test]
    fn test_entries() {
        let mut clock = VectorClock::new();
        clock.set("x", 10);
        clock.set("y", 20);

        let entries: Vec<(String, u64)> = clock.entries().collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_from_hashmap() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), 5u64);
        map.insert("b".to_string(), 10u64);

        let clock: VectorClock = map.into();
        assert_eq!(clock.get("a"), 5);
        assert_eq!(clock.get("b"), 10);
    }

    #[test]
    fn test_into_hashmap() {
        let mut clock = VectorClock::new();
        clock.set("a", 5);
        clock.set("b", 10);

        let map: HashMap<String, u64> = clock.into();
        assert_eq!(map.get("a"), Some(&5));
        assert_eq!(map.get("b"), Some(&10));
    }

    #[test]
    fn test_to_hashmap() {
        let mut clock = VectorClock::new();
        clock.set("a", 5);
        clock.set("b", 10);

        let map = clock.to_hashmap();
        assert_eq!(map.get("a"), Some(&5));
        assert_eq!(map.get("b"), Some(&10));

        // Original clock should still be usable
        assert_eq!(clock.get("a"), 5);
    }

    #[test]
    fn test_compare_disjoint_keys() {
        let mut clock1 = VectorClock::new();
        clock1.set("A", 1);

        let mut clock2 = VectorClock::new();
        clock2.set("B", 1);

        // Both have a key the other doesn't - concurrent
        assert!(clock1.concurrent_with(&clock2));
    }

    #[test]
    fn test_compare_empty_clocks() {
        let clock1 = VectorClock::new();
        let clock2 = VectorClock::new();

        assert_eq!(clock1.compare(&clock2), Some(Ordering::Equal));
    }

    #[test]
    fn test_compare_one_empty() {
        let clock1 = VectorClock::new();
        let mut clock2 = VectorClock::new();
        clock2.set("A", 1);

        assert_eq!(clock1.compare(&clock2), Some(Ordering::Less));
        assert_eq!(clock2.compare(&clock1), Some(Ordering::Greater));
    }

    #[test]
    fn test_merge_with_empty() {
        let mut clock1 = VectorClock::new();
        clock1.set("A", 5);

        let clock2 = VectorClock::new();
        clock1.merge(&clock2);

        assert_eq!(clock1.get("A"), 5);
    }

    #[test]
    fn test_merge_into_empty() {
        let mut clock1 = VectorClock::new();

        let mut clock2 = VectorClock::new();
        clock2.set("A", 5);
        clock2.set("B", 10);

        clock1.merge(&clock2);

        assert_eq!(clock1.get("A"), 5);
        assert_eq!(clock1.get("B"), 10);
    }

    #[test]
    fn test_serialization() {
        let mut clock = VectorClock::new();
        clock.set("device-a", 5);
        clock.set("device-b", 10);

        let json = serde_json::to_string(&clock).unwrap();
        let parsed: VectorClock = serde_json::from_str(&json).unwrap();

        assert_eq!(clock, parsed);
    }

    #[test]
    fn test_default_is_new() {
        let clock1 = VectorClock::default();
        let clock2 = VectorClock::new();

        assert_eq!(clock1, clock2);
    }
}
