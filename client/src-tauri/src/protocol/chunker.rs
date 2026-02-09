use std::path::Path;

use tokio::io::AsyncReadExt;

use crate::crypto::aes_gcm::ChunkEncryptor;
use crate::crypto::checksum::StreamingChecksum;
use crate::error::AppResult;

/// Chunk size: 256KB
pub const CHUNK_SIZE: usize = 256 * 1024;

/// Reads a file in chunks, encrypts each chunk, and computes a SHA-256 checksum.
pub struct FileChunker {
    file: tokio::fs::File,
    encryptor: ChunkEncryptor,
    checksum: StreamingChecksum,
    chunk_index: u32,
    buf: Vec<u8>,
}

impl FileChunker {
    pub async fn new(path: &Path, encryptor: ChunkEncryptor) -> AppResult<Self> {
        let file = tokio::fs::File::open(path).await?;
        Ok(Self {
            file,
            encryptor,
            checksum: StreamingChecksum::new(),
            chunk_index: 0,
            buf: vec![0u8; CHUNK_SIZE],
        })
    }

    /// Read the next chunk, encrypt it.
    /// Returns `None` when the file is fully read.
    /// Returns `Some((encrypted_data, nonce, chunk_index))`.
    pub async fn next_chunk(&mut self) -> AppResult<Option<(Vec<u8>, [u8; 12], u32)>> {
        let bytes_read = self.file.read(&mut self.buf).await?;
        if bytes_read == 0 {
            return Ok(None);
        }

        let plaintext = &self.buf[..bytes_read];

        // Update checksum with plaintext before encryption
        self.checksum.update(plaintext);

        // Encrypt
        let (ciphertext, nonce) = self.encryptor.encrypt_chunk(plaintext)?;

        let index = self.chunk_index;
        self.chunk_index += 1;

        Ok(Some((ciphertext, nonce, index)))
    }

    /// Finalize and return the SHA-256 checksum of the original (plaintext) file.
    pub fn finalize(self) -> [u8; 32] {
        self.checksum.finalize()
    }
}
