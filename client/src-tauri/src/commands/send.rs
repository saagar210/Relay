use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::crypto::spake::KeyExchange;
use crate::network::quic::QuicEndpoint;
use crate::network::signaling::SignalingClient;
use crate::transfer::code::TransferCode;
use crate::transfer::progress::ProgressEvent;
use crate::transfer::sender;
use crate::transfer::session::{TransferRole, TransferSession};

use super::transfer::SessionStore;

const DEFAULT_SIGNAL_URL: &str = "ws://localhost:8080";

#[derive(serde::Serialize)]
pub struct SendStarted {
    pub code: String,
    pub session_id: String,
    pub port: u16,
}

/// Start a send operation: generate code, connect to signaling, exchange keys, transfer.
#[tauri::command]
pub async fn start_send(
    app: AppHandle,
    file_paths: Vec<String>,
    signal_server_url: Option<String>,
) -> Result<SendStarted, String> {
    let files: Vec<PathBuf> = file_paths.into_iter().map(PathBuf::from).collect();

    // Validate files exist
    for f in &files {
        if !f.exists() {
            return Err(format!("File not found: {}", f.display()));
        }
    }

    let code = TransferCode::generate();
    let code_str = code.to_code_string();
    info!("send: generated code '{code_str}'");

    let session = TransferSession::new(TransferRole::Sender, code);
    let session_id = session.id.clone();
    let cancel_token = session.cancel_token.clone();

    // Store session
    let store = app.state::<SessionStore>().inner().clone();
    store.lock().await.insert(session_id.clone(), Arc::new(session));

    // Set up QUIC endpoint (OS-assigned port)
    let quic = QuicEndpoint::new(0).await.map_err(|e| e.to_string())?;
    let port = quic.local_addr().map_err(|e| e.to_string())?.port();
    let local_addr = quic.local_addr().map_err(|e| e.to_string())?;

    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<ProgressEvent>();
    let app_handle = app.clone();

    // Forward progress events to frontend
    tokio::spawn(async move {
        while let Some(event) = progress_rx.recv().await {
            if let Err(e) = app_handle.emit("transfer:progress", &event) {
                error!("failed to emit progress event: {e}");
            }
        }
    });

    let server_url = signal_server_url.unwrap_or_else(|| DEFAULT_SIGNAL_URL.into());
    let code_clone = code_str.clone();

    // Run the send pipeline in background
    let app_handle2 = app.clone();
    tokio::spawn(async move {
        let result = run_send_with_signaling(
            files,
            quic,
            local_addr,
            &code_clone,
            &server_url,
            progress_tx.clone(),
            cancel_token,
        )
        .await;

        match result {
            Ok(()) => {
                info!("send pipeline completed successfully");
            }
            Err(e) => {
                error!("send pipeline failed: {e}");
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

    Ok(SendStarted {
        code: code_str,
        session_id,
        port,
    })
}

/// Full send flow with signaling server for peer discovery and SPAKE2 key exchange.
async fn run_send_with_signaling(
    files: Vec<PathBuf>,
    quic: QuicEndpoint,
    local_addr: std::net::SocketAddr,
    code: &str,
    server_url: &str,
    progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), crate::error::AppError> {
    progress_tx
        .send(ProgressEvent::StateChanged {
            state: "connecting".into(),
        })
        .ok();

    // 1. Connect to signaling server
    let mut signaling = SignalingClient::connect(server_url, code).await?;

    // 2. Register as sender with our QUIC listen address
    signaling.register("sender", Some(local_addr)).await?;

    // 3. Wait for receiver to join
    let _peer_info = signaling.wait_for_peer().await?;
    info!("send: peer discovered via signaling server");

    // 4. SPAKE2 key exchange
    let key_exchange = KeyExchange::new(code);
    let outbound = key_exchange.outbound_message().to_vec();
    let peer_spake2 = signaling.exchange_spake2(&outbound).await?;
    let encryption_key = key_exchange.finish(&peer_spake2)?;
    info!("send: SPAKE2 key exchange complete");

    // 5. Exchange cert fingerprints (encrypted with SPAKE2 key)
    let _peer_fingerprint = signaling
        .exchange_cert_fingerprint(&quic.cert_fingerprint(), &encryption_key)
        .await?;
    info!("send: cert fingerprint exchange complete");

    // 6. Disconnect from signaling server
    signaling.disconnect().await?;

    // 7. Wait for receiver to connect via QUIC
    // Sender listens — the receiver will connect to us using the address
    // exchanged through the signaling server.
    info!("send: waiting for QUIC connection from receiver");
    sender::run_send(files, &quic, encryption_key, progress_tx, cancel).await
}
