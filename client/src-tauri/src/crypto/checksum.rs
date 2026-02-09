use sha2::{Digest, Sha256};

/// Streaming SHA-256 checksum calculator.
/// Feed it data incrementally, finalize when done.
pub struct StreamingChecksum {
    hasher: Sha256,
}

impl StreamingChecksum {
    pub fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    pub fn finalize(self) -> [u8; 32] {
        let result = self.hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}

impl Default for StreamingChecksum {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_checksum() {
        let cs = StreamingChecksum::new();
        let hash = cs.finalize();
        // SHA-256 of empty input is well-known
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(hex(&hash), expected);
    }

    #[test]
    fn test_streaming_matches_oneshot() {
        let data = b"Hello, Relay!";

        // One-shot
        let mut h = Sha256::new();
        h.update(data);
        let oneshot: [u8; 32] = h.finalize().into();

        // Streaming in chunks
        let mut cs = StreamingChecksum::new();
        cs.update(&data[..5]);
        cs.update(&data[5..]);
        let streaming = cs.finalize();

        assert_eq!(oneshot, streaming);
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
