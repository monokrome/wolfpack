use anyhow::{Context, Result};
use rand::Rng;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const TOKEN_LENGTH: usize = 32;

/// Manages API tokens for HTTP API authentication
pub struct ApiTokenManager {
    token_path: PathBuf,
    token: String,
}

impl ApiTokenManager {
    /// Load existing token or generate a new one
    pub fn load_or_create(data_dir: &Path) -> Result<Self> {
        let token_path = data_dir.join("api.token");

        let token = if token_path.exists() {
            fs::read_to_string(&token_path)
                .with_context(|| format!("Failed to read {}", token_path.display()))?
                .trim()
                .to_string()
        } else {
            let token = generate_token();
            save_token(&token_path, &token)?;
            token
        };

        Ok(Self { token_path, token })
    }

    /// Get the current token
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Validate a token
    pub fn validate(&self, token: &str) -> bool {
        constant_time_eq(self.token.as_bytes(), token.as_bytes())
    }

    /// Regenerate the token (invalidates old one)
    pub fn regenerate(&mut self) -> Result<&str> {
        self.token = generate_token();
        save_token(&self.token_path, &self.token)?;
        Ok(&self.token)
    }

    /// Path to the token file
    pub fn token_path(&self) -> &Path {
        &self.token_path
    }
}

fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..TOKEN_LENGTH).map(|_| rng.r#gen()).collect();
    hex::encode(bytes)
}

fn save_token(path: &Path, token: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, token)?;

    // Set permissions to 600 (owner read/write only)
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)?;

    Ok(())
}

/// Constant-time string comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_token_generation() {
        let token = generate_token();
        assert_eq!(token.len(), TOKEN_LENGTH * 2); // hex encoded
    }

    #[test]
    fn test_token_persistence() {
        let dir = tempdir().unwrap();

        let manager1 = ApiTokenManager::load_or_create(dir.path()).unwrap();
        let token1 = manager1.token().to_string();

        let manager2 = ApiTokenManager::load_or_create(dir.path()).unwrap();
        let token2 = manager2.token().to_string();

        assert_eq!(token1, token2);
    }

    #[test]
    fn test_token_validation() {
        let dir = tempdir().unwrap();
        let manager = ApiTokenManager::load_or_create(dir.path()).unwrap();

        assert!(manager.validate(manager.token()));
        assert!(!manager.validate("wrong-token"));
    }
}
