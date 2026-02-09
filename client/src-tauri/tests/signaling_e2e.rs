// End-to-end integration test: signaling → SPAKE2 → cert fingerprint → QUIC → file transfer.
//
// Requires the Go signaling server binary. If not available, tests are skipped.
// Build with: cd server && go build -o relay-server .
// Or run: RELAY_SERVER_BIN=/path/to/relay-server cargo test --test signaling_e2e

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;

use relay_lib::crypto::spake::KeyExchange;
use relay_lib::network::quic::QuicEndpoint;
use relay_lib::network::relay::RelayStream;
use relay_lib::network::signaling::SignalingClient;
use relay_lib::network::transport::Transport;
use relay_lib::protocol::messages::FileInfo;
use relay_lib::transfer::code::TransferCode;
use relay_lib::transfer::progress::ProgressEvent;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// Find or build the Go signaling server binary.
fn find_server_binary() -> Option<PathBuf> {
    // Check env var first
    if let Ok(path) = std::env::var("RELAY_SERVER_BIN") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // Try the default build location
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("server")
        .join("relay-server");

    if default_path.exists() {
        return Some(default_path);
    }

    // Try to build it
    let server_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("server");

    let status = Command::new("go")
        .arg("build")
        .arg("-o")
        .arg("relay-server")
        .arg(".")
        .current_dir(&server_dir)
        .status()
        .ok()?;

    if status.success() {
        let path = server_dir.join("relay-server");
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Start the Go signaling server on a random port.
struct TestServer {
    child: Child,
    addr: String,
}

impl TestServer {
    fn start(binary: &PathBuf) -> Self {
        let port = 10000 + (std::process::id() % 50000) as u16;
        let addr = format!("127.0.0.1:{port}");

        let child = Command::new(binary)
            .arg("-addr")
            .arg(&addr)
            .arg("-session-ttl")
            .arg("30s")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to start signaling server");

        std::thread::sleep(Duration::from_millis(500));

        Self {
            child,
            addr: format!("ws://{addr}"),
        }
    }

    fn ws_url(&self) -> &str {
        &self.addr
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Test: Two clients connect through signaling, exchange SPAKE2 keys, get the same key.
#[tokio::test]
async fn test_signaling_spake2_exchange() {
    let binary = match find_server_binary() {
        Some(b) => b,
        None => {
            eprintln!("SKIP: Go signaling server binary not found");
            return;
        }
    };

    let server = TestServer::start(&binary);
    let code = TransferCode::generate().to_code_string();

    let ws_url = server.ws_url().to_string();
    let code_s = code.clone();
    let sender_task = tokio::spawn(async move {
        let mut client = SignalingClient::connect(&ws_url, &code_s).await.unwrap();
        client.register("sender", None).await.unwrap();
        let _peer = client.wait_for_peer().await.unwrap();

        let kx = KeyExchange::new(&code_s);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = client.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();
        client.disconnect().await.unwrap();
        key
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let ws_url = server.ws_url().to_string();
    let code_r = code.clone();
    let receiver_task = tokio::spawn(async move {
        let mut client = SignalingClient::connect(&ws_url, &code_r).await.unwrap();
        client.register("receiver", None).await.unwrap();
        let _peer = client.wait_for_peer().await.unwrap();

        let kx = KeyExchange::new(&code_r);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = client.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();
        client.disconnect().await.unwrap();
        key
    });

    let (sender_key, receiver_key) = tokio::join!(sender_task, receiver_task);
    assert_eq!(sender_key.unwrap(), receiver_key.unwrap());
}

/// Test: Basic QUIC connectivity between two endpoints.
#[tokio::test]
async fn test_quic_basic_connectivity() {
    let server_quic = QuicEndpoint::new(0).await.unwrap();
    let server_addr = server_quic.local_addr().unwrap();
    let connect_addr: SocketAddr = format!("127.0.0.1:{}", server_addr.port())
        .parse()
        .unwrap();

    let server_handle = tokio::spawn(async move {
        let conn = server_quic.accept_any().await.unwrap();
        let (send, recv) = conn.open_bi().await.unwrap();
        let mut transport = Transport::Direct { send, recv };

        use relay_lib::protocol::messages::{FileInfo, PeerMessage};
        transport
            .send_peer_message(&PeerMessage::FileOffer {
                files: vec![FileInfo {
                    name: "test.txt".into(),
                    size: 100,
                    relative_path: None,
                }],
            })
            .await
            .unwrap();

        let response = transport.recv_peer_message().await.unwrap();
        eprintln!("server: got response: {:?}", response);
        transport.finish_send().await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let client_quic = QuicEndpoint::new(0).await.unwrap();
    let conn = client_quic.connect(connect_addr).await.unwrap();
    let (send, recv) = conn.accept_bi().await.unwrap();
    let mut transport = Transport::Direct { send, recv };

    use relay_lib::protocol::messages::PeerMessage;
    let offer = transport.recv_peer_message().await.unwrap();
    eprintln!("client: received: {:?}", offer);

    transport
        .send_peer_message(&PeerMessage::FileDecline)
        .await
        .unwrap();

    server_handle.await.unwrap();
}

/// Test: Full end-to-end file transfer through signaling server (QUIC direct).
#[tokio::test]
async fn test_full_file_transfer() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("relay=debug,quinn=info")
        .try_init();
    let binary = match find_server_binary() {
        Some(b) => b,
        None => {
            eprintln!("SKIP: Go signaling server binary not found");
            return;
        }
    };

    let server = TestServer::start(&binary);
    let code = TransferCode::generate().to_code_string();

    let temp_dir = tempfile::tempdir().unwrap();
    let send_file = temp_dir.path().join("test-file.txt");
    let test_data = "Hello from Relay! This is a test file for end-to-end transfer.\n".repeat(100);
    std::fs::write(&send_file, &test_data).unwrap();

    let recv_dir = tempfile::tempdir().unwrap();
    let ws_url = server.ws_url().to_string();

    // Sender
    let code_s = code.clone();
    let ws_url_s = ws_url.clone();
    let send_file_clone = send_file.clone();
    let sender_handle = tokio::spawn(async move {
        let quic = QuicEndpoint::new(0).await.unwrap();
        let local_addr = quic.local_addr().unwrap();
        let register_addr: SocketAddr =
            format!("127.0.0.1:{}", local_addr.port()).parse().unwrap();

        let mut signaling = SignalingClient::connect(&ws_url_s, &code_s).await.unwrap();
        signaling
            .register("sender", Some(register_addr))
            .await
            .unwrap();
        let _peer = signaling.wait_for_peer().await.unwrap();

        let kx = KeyExchange::new(&code_s);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = signaling.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();

        let _peer_fp = signaling
            .exchange_cert_fingerprint(&quic.cert_fingerprint(), &key)
            .await
            .unwrap();
        signaling.disconnect().await.unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Accept QUIC connection and create transport
        let conn = quic.accept_any().await.unwrap();
        let (send, recv) = conn.open_bi().await.unwrap();
        let mut transport = Transport::Direct { send, recv };

        let file_meta = tokio::fs::metadata(&send_file_clone).await.unwrap();
        let file_infos = vec![FileInfo {
            name: "test-file.txt".into(),
            size: file_meta.len(),
            relative_path: None,
        }];

        let (progress_tx, _) = mpsc::unbounded_channel::<ProgressEvent>();
        let cancel = CancellationToken::new();

        relay_lib::transfer::sender::run_send(
            vec![send_file_clone],
            file_infos,
            &mut transport,
            key,
            progress_tx,
            cancel,
        )
        .await
        .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Receiver
    let code_r = code.clone();
    let ws_url_r = ws_url.clone();
    let recv_path = recv_dir.path().to_path_buf();
    let receiver_handle = tokio::spawn(async move {
        let mut signaling = SignalingClient::connect(&ws_url_r, &code_r).await.unwrap();
        signaling.register("receiver", None).await.unwrap();
        let peer_info = signaling.wait_for_peer().await.unwrap();

        let kx = KeyExchange::new(&code_r);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = signaling.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();

        let quic = QuicEndpoint::new(0).await.unwrap();
        let _peer_fp = signaling
            .exchange_cert_fingerprint(&quic.cert_fingerprint(), &key)
            .await
            .unwrap();
        signaling.disconnect().await.unwrap();

        let sender_addr: SocketAddr =
            format!("{}:{}", peer_info.local_ip, peer_info.local_port)
                .parse()
                .unwrap();

        let conn = quic.connect(sender_addr).await.unwrap();
        let (send, recv) = conn.accept_bi().await.unwrap();
        let mut transport = Transport::Direct { send, recv };

        let (progress_tx, _) = mpsc::unbounded_channel::<ProgressEvent>();
        let (accept_tx, accept_rx) = oneshot::channel::<bool>();
        let cancel = CancellationToken::new();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = accept_tx.send(true);
        });

        relay_lib::transfer::receiver::run_receive(
            recv_path.clone(),
            &mut transport,
            key,
            progress_tx,
            accept_rx,
            cancel,
        )
        .await
        .unwrap();

        recv_path
    });

    let (sender_result, receiver_result) = tokio::join!(sender_handle, receiver_handle);
    sender_result.unwrap();
    let recv_path = receiver_result.unwrap();

    let received_file = recv_path.join("test-file.txt");
    assert!(received_file.exists(), "received file should exist");
    let received_data = std::fs::read_to_string(&received_file).unwrap();
    assert_eq!(received_data, test_data);
}

/// Test: Relay fallback — force relay mode (skip QUIC), transfer file, verify integrity.
#[tokio::test]
async fn test_relay_fallback() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("relay=debug,quinn=info")
        .try_init();
    let binary = match find_server_binary() {
        Some(b) => b,
        None => {
            eprintln!("SKIP: Go signaling server binary not found");
            return;
        }
    };

    let server = TestServer::start(&binary);
    let code = TransferCode::generate().to_code_string();

    let temp_dir = tempfile::tempdir().unwrap();
    let send_file = temp_dir.path().join("relay-test.txt");
    let test_data = "Relay fallback test data — verifying integrity through the relay server.\n"
        .repeat(50);
    std::fs::write(&send_file, &test_data).unwrap();

    let recv_dir = tempfile::tempdir().unwrap();
    let ws_url = server.ws_url().to_string();

    // Sender: connect via signaling, then request relay directly (skip QUIC)
    let code_s = code.clone();
    let ws_url_s = ws_url.clone();
    let send_file_clone = send_file.clone();
    let sender_handle = tokio::spawn(async move {
        let mut signaling = SignalingClient::connect(&ws_url_s, &code_s).await.unwrap();
        signaling.register("sender", None).await.unwrap();
        let _peer = signaling.wait_for_peer().await.unwrap();

        let kx = KeyExchange::new(&code_s);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = signaling.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();

        // Skip cert fingerprint exchange — not needed for relay
        // Both sides immediately request relay

        signaling.request_relay().await.unwrap();

        let ws = signaling.into_ws();
        let mut transport = Transport::Relayed {
            ws: RelayStream::new(ws),
        };

        let file_meta = tokio::fs::metadata(&send_file_clone).await.unwrap();
        let file_infos = vec![FileInfo {
            name: "relay-test.txt".into(),
            size: file_meta.len(),
            relative_path: None,
        }];

        let (progress_tx, _) = mpsc::unbounded_channel::<ProgressEvent>();
        let cancel = CancellationToken::new();

        relay_lib::transfer::sender::run_send(
            vec![send_file_clone],
            file_infos,
            &mut transport,
            key,
            progress_tx,
            cancel,
        )
        .await
        .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Receiver: same — connect, request relay
    let code_r = code.clone();
    let ws_url_r = ws_url.clone();
    let recv_path = recv_dir.path().to_path_buf();
    let receiver_handle = tokio::spawn(async move {
        let mut signaling = SignalingClient::connect(&ws_url_r, &code_r).await.unwrap();
        signaling.register("receiver", None).await.unwrap();
        let _peer = signaling.wait_for_peer().await.unwrap();

        let kx = KeyExchange::new(&code_r);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = signaling.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();

        signaling.request_relay().await.unwrap();

        let ws = signaling.into_ws();
        let mut transport = Transport::Relayed {
            ws: RelayStream::new(ws),
        };

        let (progress_tx, _) = mpsc::unbounded_channel::<ProgressEvent>();
        let (accept_tx, accept_rx) = oneshot::channel::<bool>();
        let cancel = CancellationToken::new();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = accept_tx.send(true);
        });

        relay_lib::transfer::receiver::run_receive(
            recv_path.clone(),
            &mut transport,
            key,
            progress_tx,
            accept_rx,
            cancel,
        )
        .await
        .unwrap();

        recv_path
    });

    let (sender_result, receiver_result) = tokio::join!(sender_handle, receiver_handle);
    sender_result.unwrap();
    let recv_path = receiver_result.unwrap();

    let received_file = recv_path.join("relay-test.txt");
    assert!(received_file.exists(), "received file should exist");
    let received_data = std::fs::read_to_string(&received_file).unwrap();
    assert_eq!(received_data, test_data, "file content must match through relay");
}

/// Test: Folder transfer — create nested temp directory, transfer via QUIC, verify structure.
#[tokio::test]
async fn test_folder_transfer() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("relay=debug,quinn=info")
        .try_init();
    let binary = match find_server_binary() {
        Some(b) => b,
        None => {
            eprintln!("SKIP: Go signaling server binary not found");
            return;
        }
    };

    let server = TestServer::start(&binary);
    let code = TransferCode::generate().to_code_string();

    // Create a nested temp directory to send
    let send_dir = tempfile::tempdir().unwrap();
    let root = send_dir.path().join("my-project");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("README.md"), "# My Project\n").unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("docs/guide.md"), "# Guide\nHello\n").unwrap();
    // Hidden files should be skipped
    std::fs::write(root.join(".DS_Store"), "junk").unwrap();

    let recv_dir = tempfile::tempdir().unwrap();
    let ws_url = server.ws_url().to_string();

    // Expand the directory into files + infos
    let (files, file_infos) = {
        use relay_lib::commands::send::expand_directory;
        let expanded = expand_directory(&root, "my-project").await.unwrap();

        let mut paths = Vec::new();
        let mut infos = Vec::new();
        for (path, rel) in expanded {
            let meta = std::fs::metadata(&path).unwrap();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            infos.push(FileInfo {
                name,
                size: meta.len(),
                relative_path: Some(rel),
            });
            paths.push(path);
        }
        (paths, infos)
    };

    assert_eq!(files.len(), 3, "should have 3 files (not .DS_Store)");

    // Sender
    let code_s = code.clone();
    let ws_url_s = ws_url.clone();
    let files_s = files.clone();
    let infos_s = file_infos.clone();
    let sender_handle = tokio::spawn(async move {
        let quic = QuicEndpoint::new(0).await.unwrap();
        let local_addr = quic.local_addr().unwrap();
        let register_addr: SocketAddr =
            format!("127.0.0.1:{}", local_addr.port()).parse().unwrap();

        let mut signaling = SignalingClient::connect(&ws_url_s, &code_s).await.unwrap();
        signaling
            .register("sender", Some(register_addr))
            .await
            .unwrap();
        let _peer = signaling.wait_for_peer().await.unwrap();

        let kx = KeyExchange::new(&code_s);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = signaling.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();

        let _peer_fp = signaling
            .exchange_cert_fingerprint(&quic.cert_fingerprint(), &key)
            .await
            .unwrap();
        signaling.disconnect().await.unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let conn = quic.accept_any().await.unwrap();
        let (send, recv) = conn.open_bi().await.unwrap();
        let mut transport = Transport::Direct { send, recv };

        let (progress_tx, _) = mpsc::unbounded_channel::<ProgressEvent>();
        let cancel = CancellationToken::new();

        relay_lib::transfer::sender::run_send(
            files_s,
            infos_s,
            &mut transport,
            key,
            progress_tx,
            cancel,
        )
        .await
        .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Receiver
    let code_r = code.clone();
    let ws_url_r = ws_url.clone();
    let recv_path = recv_dir.path().to_path_buf();
    let receiver_handle = tokio::spawn(async move {
        let mut signaling = SignalingClient::connect(&ws_url_r, &code_r).await.unwrap();
        signaling.register("receiver", None).await.unwrap();
        let peer_info = signaling.wait_for_peer().await.unwrap();

        let kx = KeyExchange::new(&code_r);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = signaling.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();

        let quic = QuicEndpoint::new(0).await.unwrap();
        let _peer_fp = signaling
            .exchange_cert_fingerprint(&quic.cert_fingerprint(), &key)
            .await
            .unwrap();
        signaling.disconnect().await.unwrap();

        let sender_addr: SocketAddr =
            format!("{}:{}", peer_info.local_ip, peer_info.local_port)
                .parse()
                .unwrap();

        let conn = quic.connect(sender_addr).await.unwrap();
        let (send, recv) = conn.accept_bi().await.unwrap();
        let mut transport = Transport::Direct { send, recv };

        let (progress_tx, _) = mpsc::unbounded_channel::<ProgressEvent>();
        let (accept_tx, accept_rx) = oneshot::channel::<bool>();
        let cancel = CancellationToken::new();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = accept_tx.send(true);
        });

        relay_lib::transfer::receiver::run_receive(
            recv_path.clone(),
            &mut transport,
            key,
            progress_tx,
            accept_rx,
            cancel,
        )
        .await
        .unwrap();

        recv_path
    });

    let (sender_result, receiver_result) = tokio::join!(sender_handle, receiver_handle);
    sender_result.unwrap();
    let recv_path = receiver_result.unwrap();

    // Verify directory structure was preserved
    let readme = recv_path.join("my-project/README.md");
    let main_rs = recv_path.join("my-project/src/main.rs");
    let guide = recv_path.join("my-project/docs/guide.md");
    let ds_store = recv_path.join("my-project/.DS_Store");

    assert!(readme.exists(), "README.md should exist at {}", readme.display());
    assert!(main_rs.exists(), "src/main.rs should exist at {}", main_rs.display());
    assert!(guide.exists(), "docs/guide.md should exist at {}", guide.display());
    assert!(!ds_store.exists(), ".DS_Store should NOT exist");

    assert_eq!(std::fs::read_to_string(&readme).unwrap(), "# My Project\n");
    assert_eq!(std::fs::read_to_string(&main_rs).unwrap(), "fn main() {}\n");
    assert_eq!(std::fs::read_to_string(&guide).unwrap(), "# Guide\nHello\n");
}
