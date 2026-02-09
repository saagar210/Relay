// Receiver pipeline — orchestrates the full receive flow.

use std::net::SocketAddr;
use std::path::PathBuf;

use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::crypto::aes_gcm::ChunkDecryptor;
use crate::error::{AppError, AppResult};
use crate::network::quic::QuicEndpoint;
use crate::protocol::messages::{self, PeerMessage};
use crate::protocol::reassembler::FileReassembler;
use crate::transfer::progress::{FileOfferInfo, ProgressEvent, ProgressTracker};

/// Run the receiver pipeline for a direct LAN transfer.
pub async fn run_receive(
    save_dir: PathBuf,
    quic: &QuicEndpoint,
    peer_addr: SocketAddr,
    encryption_key: [u8; 32],
    progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    accept_rx: oneshot::Receiver<bool>,
    cancel: tokio_util::sync::CancellationToken,
) -> AppResult<()> {
    info!("receiver: connecting to sender at {peer_addr}");
    progress_tx
        .send(ProgressEvent::StateChanged {
            state: "connecting".into(),
        })
        .ok();

    // Connect to sender
    info!("receiver: attempting QUIC connect to {peer_addr}");
    let conn = tokio::select! {
        result = quic.connect(peer_addr) => result?,
        _ = cancel.cancelled() => return Err(AppError::Cancelled),
    };

    info!("receiver: connected to sender at {}", conn.remote_address());

    // Accept the bidirectional stream opened by the sender
    let (mut send_stream, mut recv_stream) = conn
        .accept_bi()
        .await
        .map_err(|e| AppError::Network(format!("failed to accept stream: {e}")))?;

    // Receive file offer
    let offer = messages::read_message(&mut recv_stream).await?;
    let files = match offer {
        PeerMessage::FileOffer { files } => files,
        _ => return Err(AppError::Transfer("expected FileOffer message".into())),
    };

    info!("receiver: got offer for {} file(s)", files.len());

    // Notify frontend about the offer
    let offer_infos: Vec<FileOfferInfo> = files
        .iter()
        .map(|f| FileOfferInfo {
            name: f.name.clone(),
            size: f.size,
        })
        .collect();
    progress_tx
        .send(ProgressEvent::FileOffer {
            session_id: String::new(), // filled by command layer
            files: offer_infos,
        })
        .ok();

    // Wait for user acceptance
    let accepted = tokio::select! {
        result = accept_rx => result.unwrap_or(false),
        _ = cancel.cancelled() => false,
    };

    if !accepted {
        messages::write_message(&mut send_stream, &PeerMessage::FileDecline).await?;
        return Err(AppError::Cancelled);
    }

    messages::write_message(&mut send_stream, &PeerMessage::FileAccept).await?;
    progress_tx
        .send(ProgressEvent::StateChanged {
            state: "transferring".into(),
        })
        .ok();

    let total_bytes: u64 = files.iter().map(|f| f.size).sum();
    let mut tracker = ProgressTracker::new(total_bytes);

    // Create reassemblers for each file
    let mut reassemblers: Vec<Option<FileReassembler>> = Vec::new();
    for file_info in &files {
        // Sanitize filename
        let safe_name = sanitize_filename(&file_info.name);
        let file_path = save_dir.join(&safe_name);

        let decryptor = ChunkDecryptor::new(&encryption_key)?;
        let reassembler = FileReassembler::new(&file_path, decryptor).await?;
        reassemblers.push(Some(reassembler));
    }

    // Receive chunks until TransferComplete
    loop {
        let msg = tokio::select! {
            result = messages::read_message(&mut recv_stream) => result?,
            _ = cancel.cancelled() => {
                messages::write_message(&mut send_stream, &PeerMessage::Cancel {
                    reason: "cancelled by receiver".into(),
                }).await.ok();
                // Clean up partial files
                for file_info in &files {
                    let safe_name = sanitize_filename(&file_info.name);
                    let file_path = save_dir.join(&safe_name);
                    tokio::fs::remove_file(&file_path).await.ok();
                }
                return Err(AppError::Cancelled);
            },
        };

        match msg {
            PeerMessage::FileChunk {
                file_index,
                data,
                nonce,
                ..
            } => {
                let idx = file_index as usize;
                if idx >= reassemblers.len() {
                    return Err(AppError::Transfer(format!(
                        "invalid file index: {file_index}"
                    )));
                }
                let reassembler = reassemblers[idx]
                    .as_mut()
                    .ok_or_else(|| AppError::Transfer("file already completed".into()))?;

                // data.len() before decryption includes the auth tag (16 bytes)
                // Actual plaintext size is data.len() - 16
                let plaintext_size = if data.len() > 16 { data.len() - 16 } else { data.len() };
                reassembler.write_chunk(&data, &nonce).await?;

                tracker.update(plaintext_size as u64);
                progress_tx
                    .send(ProgressEvent::TransferProgress {
                        bytes_transferred: tracker.bytes_transferred(),
                        bytes_total: tracker.bytes_total(),
                        speed_bps: tracker.speed_bps(),
                        eta_seconds: tracker.eta_seconds(),
                        current_file: files[idx].name.clone(),
                        percent: tracker.percent(),
                    })
                    .ok();
            }
            PeerMessage::FileComplete {
                file_index,
                sha256,
            } => {
                let idx = file_index as usize;
                let reassembler = reassemblers[idx]
                    .take()
                    .ok_or_else(|| AppError::Transfer("file already completed".into()))?;

                reassembler.verify(&sha256)?;
                info!("receiver: file '{}' verified", files[idx].name);

                messages::write_message(
                    &mut send_stream,
                    &PeerMessage::FileVerified { file_index },
                )
                .await?;

                progress_tx
                    .send(ProgressEvent::FileCompleted {
                        name: files[idx].name.clone(),
                    })
                    .ok();
            }
            PeerMessage::TransferComplete => {
                info!("receiver: transfer complete");
                break;
            }
            PeerMessage::Cancel { reason } => {
                warn!("receiver: sender cancelled: {reason}");
                return Err(AppError::Transfer(format!("sender cancelled: {reason}")));
            }
            _ => {
                return Err(AppError::Transfer("unexpected message during transfer".into()));
            }
        }
    }

    progress_tx
        .send(ProgressEvent::TransferComplete {
            duration_seconds: tracker.elapsed_seconds(),
            average_speed: tracker.average_speed(),
            total_bytes,
            file_count: files.len() as u32,
        })
        .ok();

    Ok(())
}

/// Sanitize a filename: remove path separators, reject traversal attacks.
fn sanitize_filename(name: &str) -> String {
    let name = name
        .replace(['/', '\\'], "_")
        .replace("..", "_")
        .replace('\0', "");
    if name.is_empty() {
        "unnamed_file".to_string()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("hello.txt"), "hello.txt");
        assert_eq!(sanitize_filename("../etc/passwd"), "__etc_passwd");
        assert_eq!(sanitize_filename("foo/bar.txt"), "foo_bar.txt");
        assert_eq!(sanitize_filename(""), "unnamed_file");
        assert_eq!(sanitize_filename("hello\0world"), "helloworld");
    }
}
