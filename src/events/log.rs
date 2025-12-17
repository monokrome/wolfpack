use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use super::{EventEnvelope, EventFile, VectorClock};
use crate::crypto::{KeyPair, PublicKey};

pub struct EventLog {
    base_path: PathBuf,
    device_id: String,
    keypair: KeyPair,
    clock: VectorClock,
}

impl EventLog {
    pub fn new(base_path: PathBuf, device_id: String, keypair: KeyPair) -> Self {
        Self {
            base_path,
            device_id,
            keypair,
            clock: VectorClock::new(),
        }
    }

    pub fn device_events_path(&self, device: &str) -> PathBuf {
        self.base_path.join("events").join(device)
    }

    pub fn next_event_number(&self, device: &str) -> Result<u32> {
        let path = self.device_events_path(device);
        if !path.exists() {
            return Ok(1);
        }

        let mut max = 0u32;
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(num_str) = name.strip_suffix(".evt")
                && let Ok(num) = num_str.parse::<u32>()
            {
                max = max.max(num);
            }
        }
        Ok(max + 1)
    }

    pub fn write_events(
        &mut self,
        events: Vec<super::types::Event>,
        known_devices: &[(String, PublicKey)],
    ) -> Result<PathBuf> {
        if events.is_empty() {
            anyhow::bail!("Cannot write empty event list");
        }

        self.clock.increment(&self.device_id);

        let envelopes: Vec<EventEnvelope> = events
            .into_iter()
            .map(|event| EventEnvelope::new(self.device_id.clone(), self.clock.clone(), event))
            .collect();

        let shared_secret = self.derive_group_secret(known_devices);
        let counter = self.clock.get(&self.device_id);
        let event_file = EventFile::new(
            self.keypair.public_key(),
            &self.device_id,
            counter,
            &shared_secret,
            &envelopes,
        )?;

        let event_num = self.next_event_number(&self.device_id)?;
        let path = self
            .device_events_path(&self.device_id)
            .join(format!("{:04}.evt", event_num));

        event_file.save(&path)?;
        Ok(path)
    }

    pub fn read_device_events(
        &self,
        device: &str,
        known_devices: &[(String, PublicKey)],
    ) -> Result<Vec<EventEnvelope>> {
        let path = self.device_events_path(device);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let shared_secret = self.derive_group_secret(known_devices);
        let mut all_events = Vec::new();

        let mut entries: Vec<_> = fs::read_dir(&path)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            if entry.path().extension().is_some_and(|ext| ext == "evt") {
                let event_file = EventFile::load(&entry.path())
                    .with_context(|| format!("Failed to load {}", entry.path().display()))?;
                let events = event_file.decrypt(&shared_secret)?;
                all_events.extend(events);
            }
        }

        Ok(all_events)
    }

    pub fn read_all_events(
        &self,
        known_devices: &[(String, PublicKey)],
    ) -> Result<Vec<EventEnvelope>> {
        let events_path = self.base_path.join("events");
        if !events_path.exists() {
            return Ok(Vec::new());
        }

        let mut all_events = Vec::new();

        for entry in fs::read_dir(&events_path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let device = entry.file_name().to_string_lossy().to_string();
                let device_events = self.read_device_events(&device, known_devices)?;
                all_events.extend(device_events);
            }
        }

        all_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(all_events)
    }

    pub fn clock(&self) -> &VectorClock {
        &self.clock
    }

    pub fn set_clock(&mut self, clock: VectorClock) {
        self.clock = clock;
    }

    fn derive_group_secret(&self, known_devices: &[(String, PublicKey)]) -> [u8; 32] {
        if known_devices.is_empty() {
            return self
                .keypair
                .derive_shared_secret(&self.keypair.public_key());
        }

        let mut combined = [0u8; 32];
        for (_device_id, public_key) in known_devices {
            let shared = self.keypair.derive_shared_secret(public_key);
            for (i, byte) in shared.iter().enumerate() {
                combined[i] ^= byte;
            }
        }
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;
    use tempfile::tempdir;

    #[test]
    fn test_event_log_write_read() {
        let dir = tempdir().unwrap();
        let keypair = KeyPair::generate();
        let device_id = "test-device".to_string();

        let mut log = EventLog::new(dir.path().to_path_buf(), device_id.clone(), keypair.clone());

        let events = vec![Event::ExtensionAdded {
            id: "test@example.com".to_string(),
            name: "Test".to_string(),
            url: None,
        }];

        let known_devices = vec![(device_id.clone(), keypair.public_key())];
        log.write_events(events, &known_devices).unwrap();

        let read_events = log.read_device_events(&device_id, &known_devices).unwrap();
        assert_eq!(read_events.len(), 1);
        assert!(matches!(read_events[0].event, Event::ExtensionAdded { .. }));
    }

    #[test]
    fn test_next_event_number() {
        let dir = tempdir().unwrap();
        let keypair = KeyPair::generate();
        let device_id = "test-device".to_string();

        let mut log = EventLog::new(dir.path().to_path_buf(), device_id.clone(), keypair.clone());
        let known_devices = vec![(device_id.clone(), keypair.public_key())];

        assert_eq!(log.next_event_number(&device_id).unwrap(), 1);

        let events = vec![Event::ExtensionAdded {
            id: "test@example.com".to_string(),
            name: "Test".to_string(),
            url: None,
        }];
        log.write_events(events, &known_devices).unwrap();

        assert_eq!(log.next_event_number(&device_id).unwrap(), 2);
    }
}
