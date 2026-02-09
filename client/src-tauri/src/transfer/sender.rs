// Sender pipeline — orchestrates the full send flow.
// Phase 1: Direct QUIC on LAN.
// Phase 2: Via signaling server.
// Phase 3: With relay fallback.

use std::path::PathBuf;

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::crypto::aes_gcm::ChunkEncryptor;
use crate::error::{AppError, AppResult};
use crate::network::quic::QuicEndpoint;
use crate::protocol::chunker::FileChunker;
use crate::protocol::messages::{self, FileInfo, PeerMessage};
use crate::transfer::progress::{ProgressEvent, ProgressTracker};

/// Run the sender pipeline for a direct LAN transfer.
pub async fn run_send(
    files: Vec<PathBuf>,
    quic: &QuicEndpoint,
    encryption_key: [u8; 32],
    progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    cancel: tokio_util::sync::CancellationToken,
) -> AppResult<()> {
    info!("sender: waiting for incoming QUIC connection");
    progress_tx
        .send(ProgressEvent::StateChanged {
            state: "connecting".into(),
        })
        .ok();

    // Accept one incoming connection
    let conn = tokio::select! {
        result = quic.accept_any() => result?,
        _ = cancel.cancelled() => return Err(AppError::Cancelled),
    };

    info!("sender: peer connected");
    progress_tx
        .send(ProgressEvent::StateChanged {
            state: "transferring".into(),
        })
        .ok();

    // Open a bidirectional stream
    let (mut send_stream, mut recv_stream) = conn
        .open_bi()
        .await
        .map_err(|e| AppError::Network(format!("failed to open stream: {e}")))?;

    // Build file info list
    let file_infos: Vec<FileInfo> = {
        let mut infos = Vec::with_capacity(files.len());
        for path in &files {
            let meta = tokio::fs::metadata(path).await?;
            infos.push(FileInfo {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".into()),
                size: meta.len(),
                relative_path: None,
            });
        }
        infos
    };

    let total_bytes: u64 = file_infos.iter().map(|f| f.size).sum();

    // Send file offer
    messages::write_message(
        &mut send_stream,
        &PeerMessage::FileOffer {
            files: file_infos.clone(),
        },
    )
    .await?;

    // Wait for accept/decline
    let response = messages::read_message(&mut recv_stream).await?;
    match response {
        PeerMessage::FileAccept => {
            info!("sender: peer accepted transfer");
        }
        PeerMessage::FileDecline => {
            warn!("sender: peer declined transfer");
            return Err(AppError::PeerRejected);
        }
        _ => {
            return Err(AppError::Transfer("unexpected message from peer".into()));
        }
    }

    let mut tracker = ProgressTracker::new(total_bytes);

    // Transfer each file
    for (file_index, path) in files.iter().enumerate() {
        let encryptor = ChunkEncryptor::new(&encryption_key)?;
        let mut chunker = FileChunker::new(path, encryptor).await?;
        let file_name = &file_infos[file_index].name;

        info!("sender: sending file '{file_name}'");

        // Send chunks
        while let Some((data, nonce, chunk_index)) = chunker.next_chunk().await? {
            // Check for cancellation
            if cancel.is_cancelled() {
                messages::write_message(&mut send_stream, &PeerMessage::Cancel {
                    reason: "cancelled by sender".into(),
                })
                .await
                .ok();
                return Err(AppError::Cancelled);
            }

            let chunk_len = data.len() as u64;
            messages::write_message(
                &mut send_stream,
                &PeerMessage::FileChunk {
                    file_index: file_index as u16,
                    chunk_index,
                    data,
                    nonce,
                },
            )
            .await?;

            tracker.update(chunk_len);
            progress_tx
                .send(ProgressEvent::TransferProgress {
                    bytes_transferred: tracker.bytes_transferred(),
                    bytes_total: tracker.bytes_total(),
                    speed_bps: tracker.speed_bps(),
                    eta_seconds: tracker.eta_seconds(),
                    current_file: file_name.clone(),
                    percent: tracker.percent(),
                })
                .ok();
        }

        // Send file complete with checksum
        let checksum = chunker.finalize();
        messages::write_message(
            &mut send_stream,
            &PeerMessage::FileComplete {
                file_index: file_index as u16,
                sha256: checksum,
            },
        )
        .await?;

        // Wait for verification
        let verify = messages::read_message(&mut recv_stream).await?;
        match verify {
            PeerMessage::FileVerified { .. } => {
                info!("sender: file '{file_name}' verified by receiver");
                progress_tx
                    .send(ProgressEvent::FileCompleted {
                        name: file_name.clone(),
                    })
                    .ok();
            }
            PeerMessage::Cancel { reason } => {
                return Err(AppError::Transfer(format!("peer cancelled: {reason}")));
            }
            _ => {
                return Err(AppError::Transfer("expected FileVerified message".into()));
            }
        }
    }

    // Send transfer complete
    messages::write_message(&mut send_stream, &PeerMessage::TransferComplete).await?;

    // Finish the send stream
    send_stream
        .finish()
        .map_err(|e| AppError::Network(format!("failed to finish stream: {e}")))?;

    progress_tx
        .send(ProgressEvent::TransferComplete {
            duration_seconds: tracker.elapsed_seconds(),
            average_speed: tracker.average_speed(),
            total_bytes,
            file_count: files.len() as u32,
        })
        .ok();

    info!("sender: transfer complete");
    Ok(())
}
