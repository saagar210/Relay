// Transport abstraction — wraps either a direct QUIC stream or a relayed WebSocket connection.
//
// Both sender and receiver pipelines use `Transport` instead of raw QUIC streams,
// allowing seamless fallback from direct QUIC to relay mode.

use crate::error::{AppError, AppResult};
use crate::network::relay::RelayStream;
use crate::protocol::messages::PeerMessage;

use quinn::{RecvStream, SendStream};

/// A bidirectional transport for exchanging PeerMessages.
pub enum Transport {
    /// Direct QUIC connection (LAN or public IP).
    Direct {
        send: SendStream,
        recv: RecvStream,
    },
    /// Relayed through the signaling server's WebSocket.
    Relayed {
        ws: RelayStream,
    },
}

impl Transport {
    /// Send a PeerMessage to the remote peer.
    pub async fn send_peer_message(&mut self, msg: &PeerMessage) -> AppResult<()> {
        match self {
            Transport::Direct { send, .. } => {
                crate::protocol::messages::write_message(send, msg).await
            }
            Transport::Relayed { ws } => ws.send_message(msg).await,
        }
    }

    /// Receive a PeerMessage from the remote peer.
    pub async fn recv_peer_message(&mut self) -> AppResult<PeerMessage> {
        match self {
            Transport::Direct { recv, .. } => {
                crate::protocol::messages::read_message(recv).await
            }
            Transport::Relayed { ws } => ws.recv_message().await,
        }
    }

    /// Signal that we're done sending (QUIC finish / WebSocket close).
    pub async fn finish_send(&mut self) -> AppResult<()> {
        match self {
            Transport::Direct { send, .. } => {
                send.finish()
                    .map_err(|e| AppError::Network(format!("failed to finish stream: {e}")))?;
                Ok(())
            }
            Transport::Relayed { ws } => ws.close().await,
        }
    }

    /// Whether this transport is going through the relay server.
    pub fn is_relayed(&self) -> bool {
        matches!(self, Transport::Relayed { .. })
    }
}
