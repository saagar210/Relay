use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::code::TransferCode;

/// A transfer session (either sending or receiving).
pub struct TransferSession {
    pub id: String,
    pub role: TransferRole,
    pub code: TransferCode,
    pub state: Arc<RwLock<TransferState>>,
    pub cancel_token: CancellationToken,
}

impl TransferSession {
    pub fn new(role: TransferRole, code: TransferCode) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            code,
            state: Arc::new(RwLock::new(TransferState::WaitingForPeer)),
            cancel_token: CancellationToken::new(),
        }
    }

    pub async fn set_state(&self, state: TransferState) {
        *self.state.write().await = state;
    }

    pub async fn get_state(&self) -> TransferState {
        self.state.read().await.clone()
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TransferRole {
    Sender,
    Receiver,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum TransferState {
    WaitingForPeer,
    Exchanging,
    Connecting,
    Transferring {
        bytes_sent: u64,
        bytes_total: u64,
        speed_bps: u64,
        eta_seconds: u32,
    },
    Completed,
    Failed {
        reason: String,
    },
    Cancelled,
}
