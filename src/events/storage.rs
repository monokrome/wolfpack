use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::path::Path;

use crate::crypto::{self, Cipher, PublicKey};

use super::EventEnvelope;

pub const EVENT_MAGIC: &[u8; 4] = b"WOLF";
pub const EVENT_VERSION: u8 = 2; // Bumped for new format with cipher field

pub struct EventFile {
    pub cipher: Cipher,
    pub sender_public_key: PublicKey,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl EventFile {
    pub fn new(
        sender_public_key: PublicKey,
        device_id: &str,
        counter: u64,
        shared_secret: &[u8; 32],
        events: &[EventEnvelope],
    ) -> Result<Self> {
        let cipher = crypto::detect_preferred_cipher();
        let plaintext = serde_json::to_vec(events).context("Failed to serialize events")?;
        let (nonce, ciphertext) =
            crypto::encrypt(cipher, shared_secret, device_id, counter, &plaintext)?;

        Ok(Self {
            cipher,
            sender_public_key,
            nonce,
            ciphertext,
        })
    }

    pub fn decrypt(&self, shared_secret: &[u8; 32]) -> Result<Vec<EventEnvelope>> {
        let plaintext = crypto::decrypt(self.cipher, shared_secret, &self.nonce, &self.ciphertext)?;
        let events: Vec<EventEnvelope> =
            serde_json::from_slice(&plaintext).context("Failed to deserialize events")?;
        Ok(events)
    }

    pub fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        writer.write_all(EVENT_MAGIC)?;
        writer.write_all(&[EVENT_VERSION])?;
        writer.write_all(&[self.cipher as u8])?;
        writer.write_all(&self.sender_public_key)?;
        writer.write_all(&[self.nonce.len() as u8])?;
        writer.write_all(&self.nonce)?;
        writer.write_all(&self.ciphertext)?;
        Ok(())
    }

    pub fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != EVENT_MAGIC {
            bail!("Invalid event file magic: expected WOLF");
        }

        let mut version = [0u8; 1];
        reader.read_exact(&mut version)?;
        if version[0] != EVENT_VERSION {
            bail!(
                "Unsupported event file version: {} (expected {})",
                version[0],
                EVENT_VERSION
            );
        }

        let mut cipher_byte = [0u8; 1];
        reader.read_exact(&mut cipher_byte)?;
        let cipher = Cipher::from_byte(cipher_byte[0])
            .ok_or_else(|| anyhow::anyhow!("Unknown cipher type: {}", cipher_byte[0]))?;

        let mut sender_public_key = [0u8; 32];
        reader.read_exact(&mut sender_public_key)?;

        let mut nonce_len = [0u8; 1];
        reader.read_exact(&mut nonce_len)?;
        let mut nonce = vec![0u8; nonce_len[0] as usize];
        reader.read_exact(&mut nonce)?;

        let mut ciphertext = Vec::new();
        reader.read_to_end(&mut ciphertext)?;

        Ok(Self {
            cipher,
            sender_public_key,
            nonce,
            ciphertext,
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(path)
            .with_context(|| format!("Failed to create event file {}", path.display()))?;
        self.write_to(file)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open event file {}", path.display()))?;
        Self::read_from(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;
    use crate::events::{Event, VectorClock};
    use tempfile::tempdir;

    fn make_test_events() -> Vec<EventEnvelope> {
        vec![EventEnvelope::new(
            "test-device".to_string(),
            VectorClock::new(),
            Event::ExtensionAdded {
                id: "test@example.com".to_string(),
                name: "Test Extension".to_string(),
                url: None,
            },
        )]
    }

    #[test]
    fn test_event_file_roundtrip() {
        let alice = KeyPair::generate();
        let bob = KeyPair::generate();
        let shared_secret = alice.derive_shared_secret(&bob.public_key());

        let events = make_test_events();
        let event_file = EventFile::new(
            alice.public_key(),
            "test-device",
            1,
            &shared_secret,
            &events,
        )
        .unwrap();

        let mut buffer = Vec::new();
        event_file.write_to(&mut buffer).unwrap();

        let loaded = EventFile::read_from(&buffer[..]).unwrap();
        let decrypted = loaded.decrypt(&shared_secret).unwrap();

        assert_eq!(events.len(), decrypted.len());
        assert_eq!(events[0].event, decrypted[0].event);
    }

    #[test]
    fn test_event_file_save_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("events/test/0001.evt");

        let alice = KeyPair::generate();
        let bob = KeyPair::generate();
        let shared_secret = alice.derive_shared_secret(&bob.public_key());

        let events = make_test_events();
        let event_file = EventFile::new(
            alice.public_key(),
            "test-device",
            1,
            &shared_secret,
            &events,
        )
        .unwrap();
        event_file.save(&path).unwrap();

        let loaded = EventFile::load(&path).unwrap();
        assert_eq!(loaded.cipher, event_file.cipher);

        let decrypted = loaded.decrypt(&shared_secret).unwrap();
        assert_eq!(events[0].event, decrypted[0].event);
    }

    #[test]
    fn test_cipher_stored_in_file() {
        let alice = KeyPair::generate();
        let shared_secret = alice.derive_shared_secret(&alice.public_key());
        let events = make_test_events();

        let event_file =
            EventFile::new(alice.public_key(), "test", 1, &shared_secret, &events).unwrap();

        let mut buffer = Vec::new();
        event_file.write_to(&mut buffer).unwrap();

        // Verify cipher byte is at position 5 (after magic + version)
        let cipher_byte = buffer[5];
        assert!(cipher_byte == 1 || cipher_byte == 2); // AES or ChaCha
    }
}
