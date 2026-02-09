use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::SecureRandom;

use crate::error::{AppError, AppResult};

/// Encrypts file chunks with AES-256-GCM.
/// Uses a counter-based nonce: [4-byte random prefix][8-byte counter].
pub struct ChunkEncryptor {
    key: LessSafeKey,
    nonce_prefix: [u8; 4],
    counter: u64,
}

impl ChunkEncryptor {
    pub fn new(key_bytes: &[u8; 32]) -> AppResult<Self> {
        let unbound = UnboundKey::new(&AES_256_GCM, key_bytes)
            .map_err(|_| AppError::Crypto("failed to create AES-256-GCM key".into()))?;

        let mut nonce_prefix = [0u8; 4];
        ring::rand::SystemRandom::new()
            .fill(&mut nonce_prefix)
            .map_err(|_| AppError::Crypto("failed to generate nonce prefix".into()))?;

        Ok(Self {
            key: LessSafeKey::new(unbound),
            nonce_prefix,
            counter: 0,
        })
    }

    /// Returns the nonce prefix so the receiver can be told (not secret, just unique).
    pub fn nonce_prefix(&self) -> [u8; 4] {
        self.nonce_prefix
    }

    /// Encrypt a chunk of plaintext. Returns (ciphertext_with_tag, nonce).
    /// The ciphertext includes the 16-byte authentication tag appended by AES-GCM.
    pub fn encrypt_chunk(&mut self, plaintext: &[u8]) -> AppResult<(Vec<u8>, [u8; 12])> {
        let nonce_bytes = self.make_nonce();
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut in_out = plaintext.to_vec();
        self.key
            .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| AppError::Crypto("AES-GCM encryption failed".into()))?;

        self.counter += 1;
        Ok((in_out, nonce_bytes))
    }

    /// Encrypt a single small payload (convenience for non-streaming use).
    /// Returns (ciphertext_with_tag, nonce).
    pub fn encrypt_one(mut self, plaintext: &[u8]) -> AppResult<(Vec<u8>, [u8; 12])> {
        self.encrypt_chunk(plaintext)
    }

    fn make_nonce(&self) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&self.nonce_prefix);
        nonce[4..].copy_from_slice(&self.counter.to_be_bytes());
        nonce
    }
}

/// Decrypts file chunks with AES-256-GCM.
pub struct ChunkDecryptor {
    key: LessSafeKey,
}

impl ChunkDecryptor {
    pub fn new(key_bytes: &[u8; 32]) -> AppResult<Self> {
        let unbound = UnboundKey::new(&AES_256_GCM, key_bytes)
            .map_err(|_| AppError::Crypto("failed to create AES-256-GCM key".into()))?;
        Ok(Self {
            key: LessSafeKey::new(unbound),
        })
    }

    /// Decrypt a single small payload (convenience for non-streaming use).
    pub fn decrypt_one(self, ciphertext: &[u8], nonce: &[u8; 12]) -> AppResult<Vec<u8>> {
        self.decrypt_chunk(ciphertext, nonce)
    }

    /// Decrypt a chunk. `ciphertext` includes the 16-byte auth tag at the end.
    pub fn decrypt_chunk(&self, ciphertext: &[u8], nonce: &[u8; 12]) -> AppResult<Vec<u8>> {
        let nonce = Nonce::assume_unique_for_key(*nonce);
        let mut in_out = ciphertext.to_vec();
        let plaintext = self
            .key
            .open_in_place(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| AppError::Crypto("AES-GCM decryption failed (tampered or wrong key)".into()))?;
        Ok(plaintext.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [42u8; 32];
        let mut encryptor = ChunkEncryptor::new(&key).unwrap();
        let decryptor = ChunkDecryptor::new(&key).unwrap();

        let plaintext = b"Hello, Relay! This is a test chunk of data.";
        let (ciphertext, nonce) = encryptor.encrypt_chunk(plaintext).unwrap();

        let decrypted = decryptor.decrypt_chunk(&ciphertext, &nonce).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_multiple_chunks() {
        let key = [99u8; 32];
        let mut encryptor = ChunkEncryptor::new(&key).unwrap();
        let decryptor = ChunkDecryptor::new(&key).unwrap();

        for i in 0..100 {
            let plaintext = format!("chunk number {i}");
            let (ciphertext, nonce) = encryptor.encrypt_chunk(plaintext.as_bytes()).unwrap();
            let decrypted = decryptor.decrypt_chunk(&ciphertext, &nonce).unwrap();
            assert_eq!(decrypted, plaintext.as_bytes());
        }
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = [42u8; 32];
        let mut encryptor = ChunkEncryptor::new(&key).unwrap();
        let decryptor = ChunkDecryptor::new(&key).unwrap();

        let (mut ciphertext, nonce) = encryptor.encrypt_chunk(b"secret data").unwrap();
        // Flip a byte
        ciphertext[0] ^= 0xff;

        let result = decryptor.decrypt_chunk(&ciphertext, &nonce);
        assert!(result.is_err(), "tampered ciphertext must fail decryption");
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = [42u8; 32];
        let key2 = [99u8; 32];
        let mut encryptor = ChunkEncryptor::new(&key1).unwrap();
        let decryptor = ChunkDecryptor::new(&key2).unwrap();

        let (ciphertext, nonce) = encryptor.encrypt_chunk(b"secret data").unwrap();
        let result = decryptor.decrypt_chunk(&ciphertext, &nonce);
        assert!(result.is_err(), "wrong key must fail decryption");
    }

    #[test]
    fn test_empty_plaintext() {
        let key = [42u8; 32];
        let mut encryptor = ChunkEncryptor::new(&key).unwrap();
        let decryptor = ChunkDecryptor::new(&key).unwrap();

        let (ciphertext, nonce) = encryptor.encrypt_chunk(b"").unwrap();
        let decrypted = decryptor.decrypt_chunk(&ciphertext, &nonce).unwrap();
        assert!(decrypted.is_empty());
    }
}
