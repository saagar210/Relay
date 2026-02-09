// Sender pipeline — orchestrates the full send flow.
// Phase 1: Direct QUIC on LAN.
// Phase 2: Via signaling server.
// Phase 3: With relay fallback + folder support.

use std::path::PathBuf;

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::crypto::aes_gcm::ChunkEncryptor;
use crate::error::{AppError, AppResult};
use crate::network::transport::Transport;
use crate::protocol::chunker::FileChunker;
use crate::protocol::messages::{FileInfo, PeerMessage};
use crate::transfer::progress::{ProgressEvent, ProgressTracker};

/// Run the sender pipeline over an established transport (QUIC or relay).
///
/// `files` — absolute paths to each file on disk (one per FileInfo entry).
/// `file_infos` — metadata including name, size, and optional relative_path for folders.
pub async fn run_send(
    files: Vec<PathBuf>,
    file_infos: Vec<FileInfo>,
    transport: &mut Transport,
    encryption_key: [u8; 32],
    progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    cancel: tokio_util::sync::CancellationToken,
) -> AppResult<()> {
    info!("sender: starting transfer ({} files)", files.len());
    progress_tx
        .send(ProgressEvent::StateChanged {
            state: "transferring".into(),
        })
        .ok();

    let total_bytes: u64 = file_infos.iter().map(|f| f.size).sum();

    // Send file offer
    transport
        .send_peer_message(&PeerMessage::FileOffer {
            files: file_infos.clone(),
        })
        .await?;

    // Wait for accept/decline
    let response = transport.recv_peer_message().await?;
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
            if cancel.is_cancelled() {
                transport
                    .send_peer_message(&PeerMessage::Cancel {
                        reason: "cancelled by sender".into(),
                    })
                    .await
                    .ok();
                return Err(AppError::Cancelled);
            }

            let chunk_len = data.len() as u64;
            transport
                .send_peer_message(&PeerMessage::FileChunk {
                    file_index: file_index as u16,
                    chunk_index,
                    data,
                    nonce,
                })
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
        transport
            .send_peer_message(&PeerMessage::FileComplete {
                file_index: file_index as u16,
                sha256: checksum,
            })
            .await?;

        // Wait for verification
        let verify = transport.recv_peer_message().await?;
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
    transport
        .send_peer_message(&PeerMessage::TransferComplete)
        .await?;

    // Finish the send side
    transport.finish_send().await?;

    // Wait briefly for the receiver to process the TransferComplete message
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

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
