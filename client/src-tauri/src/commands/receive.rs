use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::network::quic::QuicEndpoint;
use crate::transfer::code::TransferCode;
use crate::transfer::progress::ProgressEvent;
use crate::transfer::receiver;
use crate::transfer::session::{TransferRole, TransferSession};

use super::transfer::{AcceptChannelStore, SessionStore};

/// Start receiving: parse the code, connect to the sender.
#[tauri::command]
pub async fn start_receive(
    app: AppHandle,
    code: String,
    save_dir: String,
    sender_addr: String,
) -> Result<String, String> {
    let _parsed_code = TransferCode::parse(&code).map_err(|e| e.to_string())?;
    let save_path = PathBuf::from(&save_dir);

    if !save_path.is_dir() {
        return Err(format!("Save directory does not exist: {save_dir}"));
    }

    let peer_addr: SocketAddr = sender_addr
        .parse()
        .map_err(|e| format!("Invalid sender address: {e}"))?;

    info!("receive: connecting with code '{code}' to {peer_addr}");

    let session = TransferSession::new(TransferRole::Receiver, TransferCode::parse(&code).unwrap());
    let session_id = session.id.clone();
    let cancel_token = session.cancel_token.clone();

    // Store session
    let store = app.state::<SessionStore>().inner().clone();
    store.lock().await.insert(session_id.clone(), Arc::new(session));

    // Create accept/decline channel
    let (accept_tx, accept_rx) = oneshot::channel::<bool>();
    let accept_store = app.state::<AcceptChannelStore>().inner().clone();
    accept_store.lock().await.insert(session_id.clone(), accept_tx);

    // Phase 1: derive key from code directly (same as sender)
    let encryption_key = derive_simple_key(&code);

    let quic = QuicEndpoint::new(0).await.map_err(|e| e.to_string())?;

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

    // Run receive pipeline
    let app_handle2 = app.clone();
    tokio::spawn(async move {
        match receiver::run_receive(
            save_path,
            &quic,
            peer_addr,
            encryption_key,
            progress_tx,
            accept_rx,
            cancel_token,
        )
        .await
        {
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

fn derive_simple_key(code: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"relay-v1-key-derivation:");
    hasher.update(code.as_bytes());
    hasher.finalize().into()
}
