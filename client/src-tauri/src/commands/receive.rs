use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::crypto::spake::KeyExchange;
use crate::network::quic::QuicEndpoint;
use crate::network::signaling::{PeerInfo, SignalingClient};
use crate::transfer::code::TransferCode;
use crate::transfer::progress::ProgressEvent;
use crate::transfer::receiver;
use crate::transfer::session::{TransferRole, TransferSession};

use super::transfer::{AcceptChannelStore, SessionStore};

const DEFAULT_SIGNAL_URL: &str = "ws://localhost:8080";


/// Start receiving: parse code, connect to signaling server, discover sender, transfer.
#[tauri::command]
pub async fn start_receive(
    app: AppHandle,
    code: String,
    save_dir: String,
    signal_server_url: Option<String>,
) -> Result<String, String> {
    let _parsed_code = TransferCode::parse(&code).map_err(|e| e.to_string())?;
    let save_path = PathBuf::from(&save_dir);

    if !save_path.is_dir() {
        // Create it if it doesn't exist
        tokio::fs::create_dir_all(&save_path)
            .await
            .map_err(|e| format!("Cannot create save directory: {e}"))?;
    }

    info!("receive: starting with code '{code}'");

    let session = TransferSession::new(
        TransferRole::Receiver,
        TransferCode::parse(&code).map_err(|e| e.to_string())?,
    );
    let session_id = session.id.clone();
    let cancel_token = session.cancel_token.clone();

    // Store session
    let store = app.state::<SessionStore>().inner().clone();
    store.lock().await.insert(session_id.clone(), Arc::new(session));

    // Create accept/decline channel
    let (accept_tx, accept_rx) = oneshot::channel::<bool>();
    let accept_store = app.state::<AcceptChannelStore>().inner().clone();
    accept_store.lock().await.insert(session_id.clone(), accept_tx);

    let server_url = signal_server_url.unwrap_or_else(|| DEFAULT_SIGNAL_URL.into());

    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<ProgressEvent>();
    let app_handle = app.clone();

    // Forward progress events
    tokio::spawn(async move {
        while let Some(event) = progress_rx.recv().await {
            if let Err(e) = app_handle.emit("transfer:progress", &event) {
                error!("failed to emit progress event: {e}");
            }
        }
    });

    let code_clone = code.clone();

    // Run receive pipeline
    let app_handle2 = app.clone();
    tokio::spawn(async move {
        let result = run_receive_with_signaling(
            save_path,
            &code_clone,
            &server_url,
            progress_tx.clone(),
            accept_rx,
            cancel_token,
        )
        .await;

        match result {
            Ok(()) => {
                info!("receive pipeline completed successfully");
            }
            Err(e) => {
                error!("receive pipeline failed: {e}");
                app_handle2
                    .emit(
                        "transfer:progress",
                        &ProgressEvent::Error {
                            message: e.to_string(),
                        },
                    )
                    .ok();
            }
        }
    });

    Ok(session_id)
}

/// Full receive flow with signaling server for peer discovery and SPAKE2 key exchange.
async fn run_receive_with_signaling(
    save_dir: PathBuf,
    code: &str,
    server_url: &str,
    progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    accept_rx: oneshot::Receiver<bool>,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), crate::error::AppError> {
    progress_tx
        .send(ProgressEvent::StateChanged {
            state: "connecting".into(),
        })
        .ok();

    // 1. Connect to signaling server
    let mut signaling = SignalingClient::connect(server_url, code).await?;

    // 2. Register as receiver (no local QUIC addr — receiver connects to sender)
    signaling.register("receiver", None).await?;

    // 3. Wait for sender to join (or if we joined second, get their info immediately)
    let peer_info = signaling.wait_for_peer().await?;
    info!("receive: sender discovered via signaling");

    // 4. SPAKE2 key exchange
    let key_exchange = KeyExchange::new(code);
    let outbound = key_exchange.outbound_message().to_vec();
    let peer_spake2 = signaling.exchange_spake2(&outbound).await?;
    let encryption_key = key_exchange.finish(&peer_spake2)?;
    info!("receive: SPAKE2 key exchange complete");

    // 5. Exchange cert fingerprints
    let quic = QuicEndpoint::new(0).await?;
    let _peer_fingerprint = signaling
        .exchange_cert_fingerprint(&quic.cert_fingerprint(), &encryption_key)
        .await?;
    info!("receive: cert fingerprint exchange complete");

    // 6. Disconnect from signaling server
    signaling.disconnect().await?;

    // 7. Connect to sender via QUIC
    // Try the sender's addresses: first local (same LAN), then public
    let peer_addr = resolve_peer_addr(&peer_info)?;
    info!("receive: connecting to sender at {peer_addr}");

    receiver::run_receive(
        save_dir,
        &quic,
        peer_addr,
        encryption_key,
        progress_tx,
        accept_rx,
        cancel,
    )
    .await
}

/// Determine the best address to connect to the sender.
/// For now: prefer local IP (LAN), fall back to public IP.
fn resolve_peer_addr(peer_info: &PeerInfo) -> Result<SocketAddr, crate::error::AppError> {
    use crate::error::AppError;

    // Try local address first (same LAN — most reliable)
    if !peer_info.local_ip.is_empty() && peer_info.local_port > 0 {
        if let Ok(addr) = format!("{}:{}", peer_info.local_ip, peer_info.local_port).parse() {
            return Ok(addr);
        }
    }

    // Fall back to public address
    if !peer_info.public_ip.is_empty() && peer_info.public_port > 0 {
        if let Ok(addr) = format!("{}:{}", peer_info.public_ip, peer_info.public_port).parse() {
            return Ok(addr);
        }
    }

    Err(AppError::Network(
        "no usable address for sender (signaling server did not provide network info)".into(),
    ))
}

/// Accept or decline an incoming file offer.
#[tauri::command]
pub async fn accept_transfer(
    app: AppHandle,
    session_id: String,
    accept: bool,
) -> Result<(), String> {
    let accept_store = app.state::<AcceptChannelStore>().inner().clone();
    let mut channels = accept_store.lock().await;

    if let Some(tx) = channels.remove(&session_id) {
        tx.send(accept).map_err(|_| "channel closed".to_string())
    } else {
        Err(format!("no pending accept for session {session_id}"))
    }
}
