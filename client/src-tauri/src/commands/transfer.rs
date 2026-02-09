use std::collections::HashMap;
use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tokio::sync::{oneshot, Mutex};
use tracing::info;

use crate::transfer::session::TransferSession;

/// Type alias for the shared session store.
pub type SessionStore = Arc<Mutex<HashMap<String, Arc<TransferSession>>>>;

/// Type alias for pending accept/decline channels.
pub type AcceptChannelStore = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

/// Create the default stores to be managed by Tauri.
pub fn create_stores() -> (SessionStore, AcceptChannelStore) {
    (
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(Mutex::new(HashMap::new())),
    )
}

/// Cancel an active transfer.
#[tauri::command]
pub async fn cancel_transfer(app: AppHandle, session_id: String) -> Result<(), String> {
    let store = app.state::<SessionStore>().inner().clone();
    let sessions = store.lock().await;

    if let Some(session) = sessions.get(&session_id) {
        info!("cancelling transfer {session_id}");
        session.cancel();
        Ok(())
    } else {
        Err(format!("session not found: {session_id}"))
    }
}
