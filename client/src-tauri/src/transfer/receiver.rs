// Receiver pipeline — orchestrates the full receive flow.

use std::path::{Path, PathBuf};

use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::crypto::aes_gcm::ChunkDecryptor;
use crate::error::{AppError, AppResult};
use crate::network::transport::Transport;
use crate::protocol::messages::PeerMessage;
use crate::protocol::reassembler::FileReassembler;
use crate::transfer::progress::{FileOfferInfo, ProgressEvent, ProgressTracker};

/// Run the receiver pipeline over an established transport (QUIC or relay).
pub async fn run_receive(
    save_dir: PathBuf,
    transport: &mut Transport,
    encryption_key: [u8; 32],
    progress_tx: mpsc::UnboundedSender<ProgressEvent>,
    accept_rx: oneshot::Receiver<bool>,
    cancel: tokio_util::sync::CancellationToken,
) -> AppResult<()> {
    info!("receiver: waiting for file offer");
    progress_tx
        .send(ProgressEvent::StateChanged {
            state: "transferring".into(),
        })
        .ok();

    // Receive file offer
    let offer = transport.recv_peer_message().await?;
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
            relative_path: f.relative_path.clone(),
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
        transport
            .send_peer_message(&PeerMessage::FileDecline)
            .await?;
        return Err(AppError::Cancelled);
    }

    transport
        .send_peer_message(&PeerMessage::FileAccept)
        .await?;
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
        // Determine file path: use relative_path for folder transfers, name for flat files
        let file_path = if let Some(ref rel_path) = file_info.relative_path {
            let safe_rel = sanitize_path(rel_path)?;
            let full = save_dir.join(&safe_rel);
            // Create parent directories for nested files
            if let Some(parent) = full.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            full
        } else {
            let safe_name = sanitize_filename(&file_info.name);
            save_dir.join(&safe_name)
        };

        let decryptor = ChunkDecryptor::new(&encryption_key)?;
        let reassembler = FileReassembler::new(&file_path, decryptor).await?;
        reassemblers.push(Some(reassembler));
    }

    // Receive chunks until TransferComplete
    loop {
        let msg = tokio::select! {
            result = transport.recv_peer_message() => result?,
            _ = cancel.cancelled() => {
                transport.send_peer_message(&PeerMessage::Cancel {
                    reason: "cancelled by receiver".into(),
                }).await.ok();
                // Clean up partial files
                for file_info in &files {
                    let file_path = if let Some(ref rel_path) = file_info.relative_path {
                        if let Ok(safe_rel) = sanitize_path(rel_path) {
                            save_dir.join(&safe_rel)
                        } else {
                            continue;
                        }
                    } else {
                        let safe_name = sanitize_filename(&file_info.name);
                        save_dir.join(&safe_name)
                    };
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

                transport
                    .send_peer_message(&PeerMessage::FileVerified { file_index })
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

/// Sanitize a relative path for folder transfers.
/// Each component is validated individually: no `..`, no absolute paths, no null bytes.
/// Returns the sanitized relative path.
pub fn sanitize_path(rel_path: &str) -> AppResult<PathBuf> {
    let path = Path::new(rel_path);

    // Reject absolute paths
    if path.is_absolute() || rel_path.starts_with('/') || rel_path.starts_with('\\') {
        return Err(AppError::Transfer(format!(
            "absolute path not allowed: {rel_path}"
        )));
    }

    // Reject null bytes
    if rel_path.contains('\0') {
        return Err(AppError::Transfer("null byte in path".into()));
    }

    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(c) => {
                let s = c.to_string_lossy();
                // Sanitize each component: remove path separators
                let clean = s.replace(['/', '\\'], "_").replace('\0', "");
                if !clean.is_empty() {
                    safe.push(&clean);
                }
            }
            std::path::Component::ParentDir => {
                return Err(AppError::Transfer(format!(
                    "path traversal not allowed: {rel_path}"
                )));
            }
            std::path::Component::CurDir => {
                // Skip "." components
                continue;
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(AppError::Transfer(format!(
                    "absolute path component not allowed: {rel_path}"
                )));
            }
        }
    }

    if safe.as_os_str().is_empty() {
        return Err(AppError::Transfer(format!(
            "path resolves to empty: {rel_path}"
        )));
    }

    Ok(safe)
}

/// Sanitize a flat filename: remove path separators, reject traversal attacks.
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

    #[test]
    fn test_sanitize_path_valid() {
        let p = sanitize_path("docs/readme.md").unwrap();
        assert_eq!(p, PathBuf::from("docs/readme.md"));

        let p = sanitize_path("a/b/c/file.txt").unwrap();
        assert_eq!(p, PathBuf::from("a/b/c/file.txt"));

        let p = sanitize_path("file.txt").unwrap();
        assert_eq!(p, PathBuf::from("file.txt"));
    }

    #[test]
    fn test_sanitize_path_traversal() {
        assert!(sanitize_path("../etc/passwd").is_err());
        assert!(sanitize_path("foo/../../etc/passwd").is_err());
        assert!(sanitize_path("..").is_err());
    }

    #[test]
    fn test_sanitize_path_absolute() {
        assert!(sanitize_path("/etc/passwd").is_err());
        assert!(sanitize_path("\\Windows\\System32").is_err());
    }

    #[test]
    fn test_sanitize_path_null_bytes() {
        assert!(sanitize_path("foo\0bar").is_err());
    }

    #[test]
    fn test_sanitize_path_dot_components() {
        let p = sanitize_path("./foo/./bar.txt").unwrap();
        assert_eq!(p, PathBuf::from("foo/bar.txt"));
    }

    #[test]
    fn test_sanitize_path_empty() {
        assert!(sanitize_path("").is_err());
    }

    #[test]
    fn test_sanitize_path_windows_separators() {
        let p = sanitize_path("docs\\readme.md").unwrap();
        // On Unix, backslash is treated as part of the filename and sanitized
        // On Windows, it's treated as a separator
        assert!(!p.as_os_str().is_empty());
    }
}
