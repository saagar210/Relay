// WebSocket signaling client for peer discovery and key exchange.
//
// Protocol:
// 1. Connect to Go signaling server at /ws/{code}
// 2. Send "register" with role + local peer info
// 3. Wait for "peer_joined" with peer's network info
// 4. Exchange SPAKE2 messages (forwarded by server)
// 5. Exchange cert fingerprints (encrypted with SPAKE2-derived key)
// 6. Send "disconnect" and close

use std::net::SocketAddr;

use base64::prelude::*;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info};

use crate::crypto::aes_gcm::{ChunkDecryptor, ChunkEncryptor};
use crate::error::{AppError, AppResult};

/// Information about a peer's network addresses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub public_ip: String,
    #[serde(default)]
    pub public_port: u16,
    #[serde(default)]
    pub local_ip: String,
    #[serde(default)]
    pub local_port: u16,
}

/// Message format matching the Go server's SignalMessage.
#[derive(Debug, Serialize, Deserialize)]
struct SignalMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peer_info: Option<PeerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Value>,
}

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// WebSocket client for the signaling server.
pub struct SignalingClient {
    ws: WsStream,
}

impl SignalingClient {
    /// Connect to the signaling server for the given transfer code.
    pub async fn connect(server_url: &str, code: &str) -> AppResult<Self> {
        // Normalize URL: strip trailing slash, build ws path
        let base = server_url.trim_end_matches('/');
        let url = format!("{base}/ws/{code}");
        info!("signaling: connecting to {url}");

        let (ws, _response) = connect_async(&url)
            .await
            .map_err(|e| AppError::WebSocket(format!("failed to connect: {e}")))?;

        info!("signaling: connected");
        Ok(Self { ws })
    }

    /// Register with the signaling server as sender or receiver.
    pub async fn register(
        &mut self,
        role: &str,
        local_addr: Option<SocketAddr>,
    ) -> AppResult<()> {
        let peer_info = local_addr.map(|addr| {
            let ip = addr.ip();
            // Replace unspecified (0.0.0.0) with actual local IP
            let local_ip = if ip.is_unspecified() {
                get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string())
            } else {
                ip.to_string()
            };
            PeerInfo {
                public_ip: String::new(), // server detects this
                public_port: 0,
                local_ip,
                local_port: addr.port(),
            }
        });

        let msg = SignalMessage {
            msg_type: "register".into(),
            role: Some(role.into()),
            peer_info,
            message: None,
            code: None,
            payload: None,
        };

        self.send_json(&msg).await?;
        info!("signaling: registered as {role}");
        Ok(())
    }

    /// Wait for the peer to join. Returns the peer's network info.
    pub async fn wait_for_peer(&mut self) -> AppResult<PeerInfo> {
        loop {
            let msg = self.recv_json().await?;
            match msg.msg_type.as_str() {
                "peer_joined" => {
                    let info = msg.peer_info.ok_or_else(|| {
                        AppError::WebSocket("peer_joined missing peer_info".into())
                    })?;
                    info!("signaling: peer joined (public={}:{})", info.public_ip, info.public_port);
                    return Ok(info);
                }
                "error" => {
                    let err_msg = msg.message.unwrap_or_else(|| "unknown error".into());
                    return Err(AppError::WebSocket(format!("server error: {err_msg}")));
                }
                other => {
                    debug!("signaling: ignoring message type '{other}' while waiting for peer");
                }
            }
        }
    }

    /// Exchange SPAKE2 messages through the signaling server.
    /// Sends our outbound message, receives the peer's message.
    pub async fn exchange_spake2(&mut self, outbound: &[u8]) -> AppResult<Vec<u8>> {
        // Send our SPAKE2 message
        let encoded = BASE64_STANDARD.encode(outbound);
        let msg = SignalMessage {
            msg_type: "spake2".into(),
            message: Some(encoded),
            role: None,
            code: None,
            peer_info: None,
            payload: None,
        };
        self.send_json(&msg).await?;
        debug!("signaling: sent SPAKE2 message ({} bytes)", outbound.len());

        // Wait for peer's SPAKE2 message
        loop {
            let msg = self.recv_json().await?;
            match msg.msg_type.as_str() {
                "spake2" => {
                    let encoded = msg.message.ok_or_else(|| {
                        AppError::WebSocket("spake2 message missing payload".into())
                    })?;
                    let decoded = BASE64_STANDARD
                        .decode(&encoded)
                        .map_err(|e| AppError::WebSocket(format!("bad base64: {e}")))?;
                    debug!("signaling: received SPAKE2 message ({} bytes)", decoded.len());
                    return Ok(decoded);
                }
                "error" => {
                    let err_msg = msg.message.unwrap_or_else(|| "unknown error".into());
                    return Err(AppError::WebSocket(format!("server error: {err_msg}")));
                }
                other => {
                    debug!("signaling: ignoring '{other}' during SPAKE2 exchange");
                }
            }
        }
    }

    /// Exchange QUIC certificate fingerprints, encrypted with the SPAKE2-derived key.
    /// Returns the peer's cert fingerprint.
    pub async fn exchange_cert_fingerprint(
        &mut self,
        our_fingerprint: &[u8; 32],
        encryption_key: &[u8; 32],
    ) -> AppResult<[u8; 32]> {
        // Encrypt our fingerprint
        let encryptor = ChunkEncryptor::new(encryption_key)?;
        let (ciphertext, nonce) = encryptor.encrypt_one(our_fingerprint)?;

        // Pack nonce + ciphertext and base64-encode
        let mut packed = Vec::with_capacity(12 + ciphertext.len());
        packed.extend_from_slice(&nonce);
        packed.extend_from_slice(&ciphertext);
        let encoded = BASE64_STANDARD.encode(&packed);

        let msg = SignalMessage {
            msg_type: "cert_fingerprint".into(),
            message: Some(encoded),
            role: None,
            code: None,
            peer_info: None,
            payload: None,
        };
        self.send_json(&msg).await?;
        debug!("signaling: sent cert fingerprint");

        // Wait for peer's cert fingerprint
        loop {
            let msg = self.recv_json().await?;
            match msg.msg_type.as_str() {
                "cert_fingerprint" => {
                    let encoded = msg.message.ok_or_else(|| {
                        AppError::WebSocket("cert_fingerprint missing payload".into())
                    })?;
                    let packed = BASE64_STANDARD
                        .decode(&encoded)
                        .map_err(|e| AppError::WebSocket(format!("bad base64: {e}")))?;

                    if packed.len() < 12 {
                        return Err(AppError::WebSocket("cert_fingerprint too short".into()));
                    }

                    let nonce: [u8; 12] = packed[..12]
                        .try_into()
                        .map_err(|_| AppError::WebSocket("bad nonce".into()))?;
                    let ciphertext = &packed[12..];

                    let decryptor = ChunkDecryptor::new(encryption_key)?;
                    let plaintext = decryptor.decrypt_one(ciphertext, &nonce)?;

                    if plaintext.len() != 32 {
                        return Err(AppError::WebSocket(format!(
                            "cert fingerprint wrong size: {} (expected 32)",
                            plaintext.len()
                        )));
                    }

                    let mut fingerprint = [0u8; 32];
                    fingerprint.copy_from_slice(&plaintext);
                    debug!("signaling: received peer cert fingerprint");
                    return Ok(fingerprint);
                }
                "error" => {
                    let err_msg = msg.message.unwrap_or_else(|| "unknown error".into());
                    return Err(AppError::WebSocket(format!("server error: {err_msg}")));
                }
                other => {
                    debug!("signaling: ignoring '{other}' during fingerprint exchange");
                }
            }
        }
    }

    /// Send a disconnect message and close the WebSocket.
    pub async fn disconnect(mut self) -> AppResult<()> {
        let msg = SignalMessage {
            msg_type: "disconnect".into(),
            role: None,
            message: None,
            code: None,
            peer_info: None,
            payload: None,
        };
        self.send_json(&msg).await.ok(); // best-effort
        self.ws.close(None).await.ok();
        info!("signaling: disconnected");
        Ok(())
    }

    // -- Internal helpers --

    async fn send_json(&mut self, msg: &SignalMessage) -> AppResult<()> {
        let json = serde_json::to_string(msg)
            .map_err(|e| AppError::WebSocket(format!("serialize: {e}")))?;
        self.ws
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| AppError::WebSocket(format!("send: {e}")))?;
        Ok(())
    }

    async fn recv_json(&mut self) -> AppResult<SignalMessage> {
        loop {
            let raw = self
                .ws
                .next()
                .await
                .ok_or_else(|| AppError::WebSocket("connection closed".into()))?
                .map_err(|e| AppError::WebSocket(format!("recv: {e}")))?;

            match raw {
                Message::Text(text) => {
                    let msg: SignalMessage = serde_json::from_str(&text)
                        .map_err(|e| AppError::WebSocket(format!("deserialize: {e}")))?;
                    return Ok(msg);
                }
                Message::Close(_) => {
                    return Err(AppError::WebSocket("server closed connection".into()));
                }
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                    // tokio-tungstenite handles ping/pong automatically
                    continue;
                }
                Message::Binary(_) => {
                    debug!("signaling: ignoring binary message");
                    continue;
                }
            }
        }
    }
}

/// Get the local network IP by connecting a UDP socket to a public address.
/// This doesn't send any data — it just lets the OS pick the right interface.
fn get_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}
