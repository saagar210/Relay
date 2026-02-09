use quinn::{RecvStream, SendStream};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// All messages exchanged between peers over QUIC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PeerMessage {
    /// Sender → Receiver: here's what I want to send.
    FileOffer { files: Vec<FileInfo> },

    /// Receiver → Sender: I accept the transfer.
    FileAccept,

    /// Receiver → Sender: I decline the transfer.
    FileDecline,

    /// Sender → Receiver: one encrypted chunk of file data.
    FileChunk {
        file_index: u16,
        chunk_index: u32,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        nonce: [u8; 12],
    },

    /// Sender → Receiver: file transfer complete, verify checksum.
    FileComplete { file_index: u16, sha256: [u8; 32] },

    /// Receiver → Sender: checksum verified.
    FileVerified { file_index: u16 },

    /// Either → Either: all files transferred successfully.
    TransferComplete,

    /// Either → Either: cancel the transfer.
    Cancel { reason: String },

    /// Keepalive
    Ping,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    /// For folder support (Phase 3): relative path within the folder.
    pub relative_path: Option<String>,
}

/// Read one length-prefixed MessagePack message from a QUIC receive stream.
pub async fn read_message(stream: &mut RecvStream) -> AppResult<PeerMessage> {
    // Read 4-byte length prefix (big-endian u32)
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| AppError::Network(format!("failed to read message length: {e}")))?;

    let len = u32::from_be_bytes(len_buf) as usize;

    // Sanity check: max message size 256MB (generous for large chunks)
    if len > 256 * 1024 * 1024 {
        return Err(AppError::Transfer(format!(
            "message too large: {len} bytes"
        )));
    }

    // Read the payload
    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|e| AppError::Network(format!("failed to read message payload: {e}")))?;

    // Deserialize
    rmp_serde::from_slice(&payload)
        .map_err(|e| AppError::Serialization(format!("failed to decode message: {e}")))
}

/// Write one length-prefixed MessagePack message to a QUIC send stream.
pub async fn write_message(stream: &mut SendStream, msg: &PeerMessage) -> AppResult<()> {
    let payload =
        rmp_serde::to_vec(msg).map_err(|e| AppError::Serialization(format!("encode: {e}")))?;

    let len = payload.len() as u32;
    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| AppError::Network(format!("failed to write message length: {e}")))?;

    stream
        .write_all(&payload)
        .await
        .map_err(|e| AppError::Network(format!("failed to write message payload: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_all_variants() {
        let messages = vec![
            PeerMessage::FileOffer {
                files: vec![FileInfo {
                    name: "test.txt".into(),
                    size: 1024,
                    relative_path: None,
                }],
            },
            PeerMessage::FileAccept,
            PeerMessage::FileDecline,
            PeerMessage::FileChunk {
                file_index: 0,
                chunk_index: 42,
                data: vec![1, 2, 3, 4],
                nonce: [0u8; 12],
            },
            PeerMessage::FileComplete {
                file_index: 0,
                sha256: [0xAB; 32],
            },
            PeerMessage::FileVerified { file_index: 0 },
            PeerMessage::TransferComplete,
            PeerMessage::Cancel {
                reason: "test".into(),
            },
            PeerMessage::Ping,
            PeerMessage::Pong,
        ];

        for msg in &messages {
            let encoded = rmp_serde::to_vec(msg).unwrap();
            let decoded: PeerMessage = rmp_serde::from_slice(&encoded).unwrap();
            // Just verify it doesn't panic — we can't easily PartialEq an enum with Vec<u8>
            let re_encoded = rmp_serde::to_vec(&decoded).unwrap();
            assert_eq!(encoded, re_encoded, "roundtrip failed for {msg:?}");
        }
    }
}
