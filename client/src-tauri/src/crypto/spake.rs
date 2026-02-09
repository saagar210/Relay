use spake2::{Ed25519Group, Identity, Password, Spake2};

use crate::error::{AppError, AppResult};

/// Shared identity for symmetric SPAKE2 (both sides use the same).
const SYMMETRIC_ID: &[u8] = b"relay-symmetric";

pub struct KeyExchange {
    state: Option<Spake2<Ed25519Group>>,
    outbound_msg: Vec<u8>,
}

impl KeyExchange {
    /// Start a SPAKE2 key exchange.
    /// `code` is the transfer code (e.g., "7-guitar-palace").
    /// Symmetric mode: both sides use the same identity.
    pub fn new(code: &str) -> Self {
        let password = Password::new(code.as_bytes());
        let id = Identity::new(SYMMETRIC_ID);

        let (state, outbound_msg) =
            Spake2::<Ed25519Group>::start_symmetric(&password, &id);

        Self {
            state: Some(state),
            outbound_msg,
        }
    }

    /// Get the outbound message to send to the peer via signaling.
    pub fn outbound_message(&self) -> &[u8] {
        &self.outbound_msg
    }

    /// Consume the peer's message and derive the shared 32-byte key.
    pub fn finish(mut self, peer_message: &[u8]) -> AppResult<[u8; 32]> {
        let state = self
            .state
            .take()
            .ok_or_else(|| AppError::Crypto("key exchange already consumed".into()))?;

        let shared_key = state
            .finish(peer_message)
            .map_err(|e| AppError::Crypto(format!("SPAKE2 finish failed: {e:?}")))?;

        let mut key = [0u8; 32];
        key.copy_from_slice(&shared_key[..32]);
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_exchange_same_code() {
        let code = "7-guitar-palace";
        let sender = KeyExchange::new(code);
        let receiver = KeyExchange::new(code);

        let sender_msg = sender.outbound_message().to_vec();
        let receiver_msg = receiver.outbound_message().to_vec();

        let sender_key = sender.finish(&receiver_msg).unwrap();
        let receiver_key = receiver.finish(&sender_msg).unwrap();

        assert_eq!(sender_key, receiver_key, "both sides must derive the same key");
    }

    #[test]
    fn test_key_exchange_different_codes() {
        let sender = KeyExchange::new("7-guitar-palace");
        let receiver = KeyExchange::new("3-banana-mountain");

        let sender_msg = sender.outbound_message().to_vec();
        let receiver_msg = receiver.outbound_message().to_vec();

        let sender_key = sender.finish(&receiver_msg).unwrap();
        let receiver_key = receiver.finish(&sender_msg).unwrap();

        assert_ne!(sender_key, receiver_key, "different codes must produce different keys");
    }
}
