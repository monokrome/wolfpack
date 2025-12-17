use anyhow::{Context, Result};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::path::Path;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

pub type SecretKey = [u8; 32];
pub type PublicKey = [u8; 32];

#[derive(Clone)]
pub struct KeyPair {
    secret: StaticSecret,
    public: X25519Public,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredKeyPair {
    pub secret: String,
    pub public: String,
}

impl KeyPair {
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = X25519Public::from(&secret);
        Self { secret, public }
    }

    pub fn public_key(&self) -> PublicKey {
        *self.public.as_bytes()
    }

    pub fn secret_key(&self) -> SecretKey {
        self.secret.to_bytes()
    }

    pub fn derive_shared_secret(&self, their_public: &PublicKey) -> [u8; 32] {
        let their_public = X25519Public::from(*their_public);
        let shared = self.secret.diffie_hellman(&their_public);
        *shared.as_bytes()
    }

    pub fn from_bytes(secret: &SecretKey) -> Self {
        let secret = StaticSecret::from(*secret);
        let public = X25519Public::from(&secret);
        Self { secret, public }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let stored = StoredKeyPair {
            secret: hex::encode(self.secret_key()),
            public: hex::encode(self.public_key()),
        };
        let content = toml::to_string_pretty(&stored).context("Failed to serialize keypair")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(path, perms)?;
        }

        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read keypair from {}", path.display()))?;
        let stored: StoredKeyPair = toml::from_str(&content)
            .with_context(|| format!("Failed to parse keypair from {}", path.display()))?;

        let secret_bytes: [u8; 32] = hex::decode(&stored.secret)
            .context("Invalid secret key hex")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Secret key must be 32 bytes"))?;

        Ok(Self::from_bytes(&secret_bytes))
    }

    pub fn load_or_generate(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            let keypair = Self::generate();
            keypair.save(path)?;
            Ok(keypair)
        }
    }
}

pub fn public_key_to_hex(key: &PublicKey) -> String {
    hex::encode(key)
}

pub fn public_key_from_hex(s: &str) -> Result<PublicKey> {
    let bytes = hex::decode(s).context("Invalid public key hex")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Public key must be 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_keypair_generate() {
        let kp = KeyPair::generate();
        assert_eq!(kp.public_key().len(), 32);
        assert_eq!(kp.secret_key().len(), 32);
    }

    #[test]
    fn test_shared_secret() {
        let alice = KeyPair::generate();
        let bob = KeyPair::generate();

        let alice_shared = alice.derive_shared_secret(&bob.public_key());
        let bob_shared = bob.derive_shared_secret(&alice.public_key());

        assert_eq!(alice_shared, bob_shared);
    }

    #[test]
    fn test_keypair_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.toml");

        let original = KeyPair::generate();
        original.save(&path).unwrap();

        let loaded = KeyPair::load(&path).unwrap();
        assert_eq!(original.public_key(), loaded.public_key());
        assert_eq!(original.secret_key(), loaded.secret_key());
    }
}
