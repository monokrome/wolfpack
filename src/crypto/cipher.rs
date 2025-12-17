use aes_gcm::{
    Aes256Gcm, Nonce as AesNonce,
    aead::{Aead, KeyInit},
};
use anyhow::Result;
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Cipher {
    Aes256Gcm = 1,
    XChaCha20Poly1305 = 2,
}

impl Cipher {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Cipher::Aes256Gcm),
            2 => Some(Cipher::XChaCha20Poly1305),
            _ => None,
        }
    }

    pub fn nonce_size(&self) -> usize {
        match self {
            Cipher::Aes256Gcm => 12,
            Cipher::XChaCha20Poly1305 => 24,
        }
    }
}

pub fn detect_preferred_cipher() -> Cipher {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("aes") {
            return Cipher::Aes256Gcm;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("aes") {
            return Cipher::Aes256Gcm;
        }
    }

    Cipher::XChaCha20Poly1305
}

pub fn derive_nonce_aes(device_id: &str, counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    let device_hash = Sha256::digest(device_id.as_bytes());
    nonce[0..4].copy_from_slice(&device_hash[0..4]);
    nonce[4..12].copy_from_slice(&counter.to_be_bytes());
    nonce
}

pub fn derive_nonce_xchacha(device_id: &str, counter: u64) -> [u8; 24] {
    let mut nonce = [0u8; 24];
    let device_hash = Sha256::digest(device_id.as_bytes());
    nonce[0..16].copy_from_slice(&device_hash[0..16]);
    nonce[16..24].copy_from_slice(&counter.to_be_bytes());
    nonce
}

pub fn encrypt(
    cipher: Cipher,
    key: &[u8; 32],
    device_id: &str,
    counter: u64,
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>)> {
    match cipher {
        Cipher::Aes256Gcm => {
            let nonce = derive_nonce_aes(device_id, counter);
            let aes = Aes256Gcm::new_from_slice(key)
                .map_err(|e| anyhow::anyhow!("Invalid key: {}", e))?;
            let ciphertext = aes
                .encrypt(AesNonce::from_slice(&nonce), plaintext)
                .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
            Ok((nonce.to_vec(), ciphertext))
        }
        Cipher::XChaCha20Poly1305 => {
            let nonce = derive_nonce_xchacha(device_id, counter);
            let chacha = XChaCha20Poly1305::new_from_slice(key)
                .map_err(|e| anyhow::anyhow!("Invalid key: {}", e))?;
            let ciphertext = chacha
                .encrypt(XNonce::from_slice(&nonce), plaintext)
                .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
            Ok((nonce.to_vec(), ciphertext))
        }
    }
}

pub fn decrypt(cipher: Cipher, key: &[u8; 32], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    match cipher {
        Cipher::Aes256Gcm => {
            if nonce.len() != 12 {
                anyhow::bail!("AES-GCM requires 12-byte nonce, got {}", nonce.len());
            }
            let aes = Aes256Gcm::new_from_slice(key)
                .map_err(|e| anyhow::anyhow!("Invalid key: {}", e))?;
            aes.decrypt(AesNonce::from_slice(nonce), ciphertext)
                .map_err(|_| anyhow::anyhow!("Decryption failed - invalid key or corrupted data"))
        }
        Cipher::XChaCha20Poly1305 => {
            if nonce.len() != 24 {
                anyhow::bail!("XChaCha20 requires 24-byte nonce, got {}", nonce.len());
            }
            let chacha = XChaCha20Poly1305::new_from_slice(key)
                .map_err(|e| anyhow::anyhow!("Invalid key: {}", e))?;
            chacha
                .decrypt(XNonce::from_slice(nonce), ciphertext)
                .map_err(|_| anyhow::anyhow!("Decryption failed - invalid key or corrupted data"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_roundtrip() {
        let key = [42u8; 32];
        let plaintext = b"Hello, wolfpack!";

        let (nonce, ciphertext) =
            encrypt(Cipher::Aes256Gcm, &key, "test-device", 1, plaintext).unwrap();

        let decrypted = decrypt(Cipher::Aes256Gcm, &key, &nonce, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_chacha_roundtrip() {
        let key = [42u8; 32];
        let plaintext = b"Hello, wolfpack!";

        let (nonce, ciphertext) =
            encrypt(Cipher::XChaCha20Poly1305, &key, "test-device", 1, plaintext).unwrap();

        let decrypted = decrypt(Cipher::XChaCha20Poly1305, &key, &nonce, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_nonce_determinism() {
        let n1 = derive_nonce_aes("device-a", 42);
        let n2 = derive_nonce_aes("device-a", 42);
        let n3 = derive_nonce_aes("device-a", 43);
        let n4 = derive_nonce_aes("device-b", 42);

        assert_eq!(n1, n2); // same inputs = same nonce
        assert_ne!(n1, n3); // different counter = different nonce
        assert_ne!(n1, n4); // different device = different nonce
    }

    #[test]
    fn test_wrong_cipher_fails() {
        let key = [42u8; 32];
        let plaintext = b"Secret";

        let (nonce, ciphertext) = encrypt(Cipher::Aes256Gcm, &key, "test", 1, plaintext).unwrap();

        // Try to decrypt AES ciphertext with ChaCha - should fail
        // (nonce size mismatch will be caught first)
        let result = decrypt(Cipher::XChaCha20Poly1305, &key, &nonce, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_preferred_cipher() {
        let cipher = detect_preferred_cipher();
        // Just verify it returns something valid
        assert!(cipher == Cipher::Aes256Gcm || cipher == Cipher::XChaCha20Poly1305);
        println!("Detected preferred cipher: {:?}", cipher);
    }
}
