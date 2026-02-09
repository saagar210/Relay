// Relay transport — wraps a WebSocket connection for relayed file transfer.
//
// When direct QUIC fails (NAT, firewalls), both peers fall back to relaying
// through the signaling server. This module provides the same send/recv
// interface as QUIC streams but over WebSocket binary frames.
//
// Wire format: same as QUIC — 4-byte big-endian length prefix + MessagePack payload.

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tracing::debug;

use crate::error::{AppError, AppResult};
use crate::protocol::messages::PeerMessage;

/// The underlying WebSocket stream type (same as signaling).
pub type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A relay stream wrapping a WebSocket for peer-to-peer message exchange.
pub struct RelayStream {
    ws: WsStream,
}

impl RelayStream {
    /// Wrap an existing WebSocket connection as a relay stream.
    pub fn new(ws: WsStream) -> Self {
        Self { ws }
    }

    /// Send a PeerMessage as a binary WebSocket frame.
    /// Format: 4-byte big-endian length + MessagePack payload.
    pub async fn send_message(&mut self, msg: &PeerMessage) -> AppResult<()> {
        let payload = rmp_serde::to_vec(msg)
            .map_err(|e| AppError::Serialization(format!("relay encode: {e}")))?;

        let len = payload.len() as u32;
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(&payload);

        self.ws
            .send(Message::Binary(frame.into()))
            .await
            .map_err(|e| AppError::WebSocket(format!("relay send: {e}")))?;

        Ok(())
    }

    /// Receive a PeerMessage from a binary WebSocket frame.
    pub async fn recv_message(&mut self) -> AppResult<PeerMessage> {
        loop {
            let raw = self
                .ws
                .next()
                .await
                .ok_or_else(|| AppError::WebSocket("relay connection closed".into()))?
                .map_err(|e| AppError::WebSocket(format!("relay recv: {e}")))?;

            match raw {
                Message::Binary(data) => {
                    if data.len() < 4 {
                        return Err(AppError::Transfer(
                            "relay message too short (< 4 bytes)".into(),
                        ));
                    }

                    let len =
                        u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;

                    if data.len() != 4 + len {
                        return Err(AppError::Transfer(format!(
                            "relay message length mismatch: header says {len}, got {} payload bytes",
                            data.len() - 4
                        )));
                    }

                    let msg: PeerMessage = rmp_serde::from_slice(&data[4..]).map_err(|e| {
                        AppError::Serialization(format!("relay decode: {e}"))
                    })?;

                    return Ok(msg);
                }
                Message::Close(_) => {
                    return Err(AppError::WebSocket("relay connection closed by peer".into()));
                }
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                    continue;
                }
                Message::Text(text) => {
                    // During relay mode, we might get a JSON error from the server
                    debug!("relay: ignoring text message: {text}");
                    continue;
                }
            }
        }
    }

    /// Close the relay WebSocket connection.
    pub async fn close(&mut self) -> AppResult<()> {
        self.ws.close(None).await.ok();
        Ok(())
    }
}
