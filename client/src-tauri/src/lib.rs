pub mod commands;
pub mod crypto;
pub mod error;
pub mod network;
pub mod protocol;
pub mod transfer;

use commands::{receive, send, transfer as transfer_cmds};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter("relay=debug")
        .init();

    let (session_store, accept_store) = transfer_cmds::create_stores();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(session_store)
        .manage(accept_store)
        .invoke_handler(tauri::generate_handler![
            send::start_send,
            receive::start_receive,
            receive::accept_transfer,
            transfer_cmds::cancel_transfer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
