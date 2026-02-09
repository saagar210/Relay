use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::crypto::spake::KeyExchange;
use crate::network::quic::QuicEndpoint;
use crate::network::relay::RelayStream;
use crate::network::signaling::SignalingClient;
use crate::network::transport::Transport;
use crate::protocol::messages::FileInfo;
use crate::transfer::code::TransferCode;
use crate::transfer::progress::ProgressEvent;
use crate::transfer::sender;
use crate::transfer::session::{TransferRole, TransferSession};

use super::transfer::SessionStore;

const DEFAULT_SIGNAL_URL: &str = "ws://localhost:8080";

/// Timeout for the sender waiting for a QUIC connection from the receiver.
const SENDER_QUIC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Hidden files/directories to skip during folder expansion.
const HIDDEN_ENTRIES: &[&str] = &[".DS_Store", ".git", "Thumbs.db", ".gitignore", "__MACOSX"];

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
    let input_paths: Vec<PathBuf> = file_paths.into_iter().map(PathBuf::from).collect();

    // Validate paths exist
    for f in &input_paths {
        if !f.exists() {
            return Err(format!("Path not found: {}", f.display()));
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
            input_paths,
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

/// What happened during the QUIC/relay race.
enum RaceOutcome {
    /// Direct QUIC connection succeeded.
    QuicConnected(quinn::Connection),
    /// QUIC failed or peer requested relay — need to fall back.
    FallbackToRelay,
}

/// Full send flow with signaling server for peer discovery, SPAKE2 key exchange,
/// and fallback to relay if QUIC fails.
async fn run_send_with_signaling(
    input_paths: Vec<PathBuf>,
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

    // 6. Race: wait for QUIC connection from receiver OR a relay request.
    info!(
        "send: waiting for QUIC connection (timeout {}s) or relay request",
        SENDER_QUIC_TIMEOUT.as_secs()
    );

    let race_outcome: RaceOutcome = tokio::select! {
        result = async {
            tokio::time::timeout(SENDER_QUIC_TIMEOUT, quic.accept_any()).await
        } => {
            match result {
                Ok(Ok(conn)) => {
                    info!("send: direct QUIC connection established");
                    RaceOutcome::QuicConnected(conn)
                }
                Ok(Err(e)) => {
                    warn!("send: QUIC accept failed: {e}, falling back to relay");
                    RaceOutcome::FallbackToRelay
                }
                Err(_) => {
                    warn!("send: QUIC accept timed out, falling back to relay");
                    RaceOutcome::FallbackToRelay
                }
            }
        }

        result = signaling.check_for_relay_request() => {
            match result {
                Ok(true) => {
                    info!("send: peer requested relay");
                    RaceOutcome::FallbackToRelay
                }
                Ok(false) | Err(_) => {
                    warn!("send: signaling message during QUIC wait, falling back to relay");
                    RaceOutcome::FallbackToRelay
                }
            }
        }

        _ = cancel.cancelled() => {
            signaling.disconnect().await.ok();
            return Err(crate::error::AppError::Cancelled);
        }
    };

    // 7. Build transport based on race outcome.
    let mut transport = match race_outcome {
        RaceOutcome::QuicConnected(conn) => {
            // Direct connection — disconnect signaling, we don't need it anymore.
            signaling.disconnect().await.ok();

            progress_tx
                .send(ProgressEvent::ConnectionTypeChanged {
                    connection_type: "direct".into(),
                })
                .ok();

            let (send, recv) = conn.open_bi().await.map_err(|e| {
                crate::error::AppError::Network(format!("failed to open stream: {e}"))
            })?;
            Transport::Direct { send, recv }
        }
        RaceOutcome::FallbackToRelay => {
            // Request relay, then hand off the WebSocket for data transfer.
            signaling.request_relay().await?;

            progress_tx
                .send(ProgressEvent::ConnectionTypeChanged {
                    connection_type: "relay".into(),
                })
                .ok();

            let ws = signaling.into_ws();
            Transport::Relayed {
                ws: RelayStream::new(ws),
            }
        }
    };

    // Expand directories into individual files
    let (files, file_infos) = expand_paths(&input_paths).await?;

    // 8. Run transfer over the established transport
    sender::run_send(files, file_infos, &mut transport, encryption_key, progress_tx, cancel).await
}

/// Expand input paths: directories become their recursive file listing,
/// plain files pass through as-is.
async fn expand_paths(
    input_paths: &[PathBuf],
) -> Result<(Vec<PathBuf>, Vec<FileInfo>), crate::error::AppError> {
    let mut files = Vec::new();
    let mut infos = Vec::new();

    for path in input_paths {
        let meta = tokio::fs::metadata(path).await?;
        if meta.is_dir() {
            let dir_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "folder".into());

            let expanded = expand_directory(path, &dir_name).await?;
            for (file_path, relative_path) in expanded {
                let file_meta = tokio::fs::metadata(&file_path).await?;
                let name = file_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".into());

                infos.push(FileInfo {
                    name,
                    size: file_meta.len(),
                    relative_path: Some(relative_path),
                });
                files.push(file_path);
            }
        } else {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".into());
            infos.push(FileInfo {
                name,
                size: meta.len(),
                relative_path: None,
            });
            files.push(path.clone());
        }
    }

    Ok((files, infos))
}

/// Recursively walk a directory, returning (absolute_path, relative_path) pairs.
/// Skips hidden files and common junk files.
pub async fn expand_directory(
    dir: &PathBuf,
    prefix: &str,
) -> Result<Vec<(PathBuf, String)>, crate::error::AppError> {
    let mut result = Vec::new();
    let mut stack: Vec<(PathBuf, String)> = vec![(dir.clone(), prefix.to_string())];

    while let Some((current_dir, current_prefix)) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&current_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files and known junk
            if name.starts_with('.') || HIDDEN_ENTRIES.contains(&name.as_str()) {
                continue;
            }

            let path = entry.path();
            let relative = format!("{current_prefix}/{name}");

            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                stack.push((path, relative));
            } else if file_type.is_file() {
                result.push((path, relative));
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_expand_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Create nested structure
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("readme.txt"), "hello").unwrap();
        std::fs::write(root.join("docs/guide.md"), "guide").unwrap();
        std::fs::write(root.join(".DS_Store"), "junk").unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/config"), "git config").unwrap();

        let result = expand_directory(&root.to_path_buf(), "test-folder")
            .await
            .unwrap();

        // Should have readme.txt and docs/guide.md, NOT .DS_Store or .git/*
        assert_eq!(result.len(), 2);

        let rel_paths: Vec<&str> = result.iter().map(|(_, r)| r.as_str()).collect();
        assert!(rel_paths.contains(&"test-folder/readme.txt"));
        assert!(rel_paths.contains(&"test-folder/docs/guide.md"));
    }
}
