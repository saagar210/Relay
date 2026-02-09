use std::path::Path;

use tokio::io::AsyncWriteExt;

use crate::crypto::aes_gcm::ChunkDecryptor;
use crate::crypto::checksum::StreamingChecksum;
use crate::error::{AppError, AppResult};

/// Receives encrypted chunks, decrypts them, writes to a file, and verifies checksum.
pub struct FileReassembler {
    file: tokio::fs::File,
    decryptor: ChunkDecryptor,
    checksum: StreamingChecksum,
    bytes_written: u64,
}

impl FileReassembler {
    pub async fn new(path: &Path, decryptor: ChunkDecryptor) -> AppResult<Self> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let file = tokio::fs::File::create(path).await?;
        Ok(Self {
            file,
            decryptor,
            checksum: StreamingChecksum::new(),
            bytes_written: 0,
        })
    }

    /// Decrypt and write one chunk.
    pub async fn write_chunk(&mut self, ciphertext: &[u8], nonce: &[u8; 12]) -> AppResult<()> {
        let plaintext = self.decryptor.decrypt_chunk(ciphertext, nonce)?;

        self.checksum.update(&plaintext);
        self.file.write_all(&plaintext).await?;
        self.bytes_written += plaintext.len() as u64;

        Ok(())
    }

    /// Verify the file's SHA-256 checksum matches the expected value.
    pub fn verify(self, expected: &[u8; 32]) -> AppResult<()> {
        let actual = self.checksum.finalize();
        if actual != *expected {
            return Err(AppError::ChecksumMismatch(format!(
                "expected {}, got {}",
                hex(&expected[..8]),
                hex(&actual[..8]),
            )));
        }
        Ok(())
    }

    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
