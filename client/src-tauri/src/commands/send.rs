use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::crypto::spake::KeyExchange;
use crate::network::quic::QuicEndpoint;
use crate::transfer::code::TransferCode;
use crate::transfer::progress::ProgressEvent;
use crate::transfer::sender;
use crate::transfer::session::{TransferRole, TransferSession};

use super::transfer::SessionStore;

#[derive(serde::Serialize)]
pub struct SendStarted {
    pub code: String,
    pub session_id: String,
    pub port: u16,
}

/// Start a send operation: generate code, set up QUIC listener, wait for receiver.
#[tauri::command]
pub async fn start_send(
    app: AppHandle,
    file_paths: Vec<String>,
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
    let store = app
        .state::<SessionStore>()
        .inner()
        .clone();
    store.lock().await.insert(session_id.clone(), Arc::new(session));

    // Set up QUIC endpoint (OS-assigned port)
    let quic = QuicEndpoint::new(0)
        .await
        .map_err(|e| e.to_string())?;
    let port = quic.local_addr().map_err(|e| e.to_string())?.port();

    // For Phase 1 (LAN): the receiver needs to know our code + address.
    // The code is shared out-of-band. The address must be discovered.
    // For now, return the port and the code.

    // Derive encryption key from code via SPAKE2
    // In Phase 1, we do a simplified flow: both sides derive key from code directly.
    // In Phase 2, SPAKE2 messages go through the signaling server.
    // Phase 2 will use this for proper SPAKE2 exchange via signaling server
    let _key_exchange = KeyExchange::new(&code_str);

    // For Phase 1 LAN demo: use a fixed key derived from the code.
    // This is NOT secure for production (no MITM protection without signaling).
    // Phase 2 will implement proper SPAKE2 exchange.
    let encryption_key = derive_simple_key(&code_str);

    // Spawn the send pipeline
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

    // Run the send pipeline in background
    let app_handle2 = app.clone();
    tokio::spawn(async move {
        match sender::run_send(files, &quic, encryption_key, progress_tx, cancel_token).await {
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

/// Phase 1: Derive a key directly from the code (NO SPAKE2 exchange).
/// This is a placeholder until signaling server enables proper SPAKE2.
fn derive_simple_key(code: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"relay-v1-key-derivation:");
    hasher.update(code.as_bytes());
    hasher.finalize().into()
}
