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
use relay_lib::network::signaling::SignalingClient;
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
        // Use port 0 is not supported by the Go server, so pick a random high port
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

        // Give the server a moment to start
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

    // Sender connects
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

    // Small delay to ensure sender registers first
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Receiver connects
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
    let sender_key = sender_key.unwrap();
    let receiver_key = receiver_key.unwrap();

    assert_eq!(
        sender_key, receiver_key,
        "sender and receiver must derive the same SPAKE2 key"
    );
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
        eprintln!("server: accepted connection from {}", conn.remote_address());

        let (mut send, mut recv) = conn.open_bi().await.unwrap();
        eprintln!("server: opened bi stream");

        // Write a simple message
        use relay_lib::protocol::messages::{self, PeerMessage, FileInfo};
        messages::write_message(
            &mut send,
            &PeerMessage::FileOffer {
                files: vec![FileInfo {
                    name: "test.txt".into(),
                    size: 100,
                    relative_path: None,
                }],
            },
        )
        .await
        .unwrap();
        eprintln!("server: wrote FileOffer");

        let response = messages::read_message(&mut recv).await.unwrap();
        eprintln!("server: got response: {:?}", response);
        send.finish().unwrap();
    });

    // Small delay to ensure server is accepting
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client_quic = QuicEndpoint::new(0).await.unwrap();
    let conn = client_quic.connect(connect_addr).await.unwrap();
    eprintln!("client: connected to {}", conn.remote_address());

    let (mut send, mut recv) = conn.accept_bi().await.unwrap();
    eprintln!("client: accepted bi stream");

    use relay_lib::protocol::messages::{self, PeerMessage};
    let offer = messages::read_message(&mut recv).await.unwrap();
    eprintln!("client: received: {:?}", offer);

    messages::write_message(&mut send, &PeerMessage::FileDecline).await.unwrap();
    eprintln!("client: sent FileDecline");

    server_handle.await.unwrap();
    eprintln!("QUIC basic connectivity test passed!");
}

/// Test: Full end-to-end file transfer through signaling server.
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

    // Create a temp file to send
    let temp_dir = tempfile::tempdir().unwrap();
    let send_file = temp_dir.path().join("test-file.txt");
    let test_data = "Hello from Relay! This is a test file for end-to-end transfer.\n".repeat(100);
    std::fs::write(&send_file, &test_data).unwrap();

    // Create a temp dir to receive into
    let recv_dir = tempfile::tempdir().unwrap();

    let ws_url = server.ws_url().to_string();

    // Sender side
    let code_s = code.clone();
    let ws_url_s = ws_url.clone();
    let send_file_clone = send_file.clone();
    let sender_handle = tokio::spawn(async move {
        // Set up QUIC endpoint
        let quic = QuicEndpoint::new(0).await.unwrap();
        let local_addr = quic.local_addr().unwrap();
        // For localhost testing, register with 127.0.0.1 explicitly
        let register_addr: SocketAddr =
            format!("127.0.0.1:{}", local_addr.port()).parse().unwrap();

        // Connect to signaling
        let mut signaling = SignalingClient::connect(&ws_url_s, &code_s).await.unwrap();
        signaling
            .register("sender", Some(register_addr))
            .await
            .unwrap();

        let _peer = signaling.wait_for_peer().await.unwrap();

        // SPAKE2 exchange
        let kx = KeyExchange::new(&code_s);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = signaling.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();

        // Cert fingerprint exchange
        let _peer_fp = signaling
            .exchange_cert_fingerprint(&quic.cert_fingerprint(), &key)
            .await
            .unwrap();

        signaling.disconnect().await.unwrap();

        eprintln!("SENDER KEY: {:?}", &key[..8]);
        eprintln!("SENDER QUIC ADDR: {}", local_addr);

        // Small delay to allow receiver to also finish signaling
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Run send pipeline
        let (progress_tx, _progress_rx) = mpsc::unbounded_channel::<ProgressEvent>();
        let cancel = CancellationToken::new();

        eprintln!("SENDER: starting run_send, waiting for QUIC connection on port {}", local_addr.port());
        let result = relay_lib::transfer::sender::run_send(
            vec![send_file_clone],
            &quic,
            key,
            progress_tx,
            cancel,
        )
        .await;
        if let Err(e) = &result {
            eprintln!("SENDER ERROR: {e}");
        }
        result.unwrap();
    });

    // Small delay for sender to register
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Receiver side
    let code_r = code.clone();
    let ws_url_r = ws_url.clone();
    let recv_path = recv_dir.path().to_path_buf();
    let receiver_handle = tokio::spawn(async move {
        // Connect to signaling
        let mut signaling = SignalingClient::connect(&ws_url_r, &code_r).await.unwrap();
        signaling.register("receiver", None).await.unwrap();

        let peer_info = signaling.wait_for_peer().await.unwrap();

        // SPAKE2 exchange
        let kx = KeyExchange::new(&code_r);
        let outbound = kx.outbound_message().to_vec();
        let peer_msg = signaling.exchange_spake2(&outbound).await.unwrap();
        let key = kx.finish(&peer_msg).unwrap();

        // Cert fingerprint exchange
        let quic = QuicEndpoint::new(0).await.unwrap();
        let _peer_fp = signaling
            .exchange_cert_fingerprint(&quic.cert_fingerprint(), &key)
            .await
            .unwrap();

        signaling.disconnect().await.unwrap();

        eprintln!("RECEIVER KEY: {:?}", &key[..8]);
        eprintln!("PEER INFO: {:?}", peer_info);

        // Resolve sender address
        let sender_addr: SocketAddr = if !peer_info.local_ip.is_empty() && peer_info.local_port > 0
        {
            format!("{}:{}", peer_info.local_ip, peer_info.local_port)
                .parse()
                .unwrap()
        } else {
            panic!("no sender address from signaling");
        };

        // Run receive pipeline with auto-accept
        let (progress_tx, _progress_rx) = mpsc::unbounded_channel::<ProgressEvent>();
        let (accept_tx, accept_rx) = oneshot::channel::<bool>();
        let cancel = CancellationToken::new();

        // Auto-accept the transfer in a separate task
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = accept_tx.send(true);
        });

        eprintln!("RECEIVER: connecting to sender at {sender_addr}");
        let result = relay_lib::transfer::receiver::run_receive(
            recv_path.clone(),
            &quic,
            sender_addr,
            key,
            progress_tx,
            accept_rx,
            cancel,
        )
        .await;
        if let Err(e) = &result {
            eprintln!("RECEIVER ERROR: {e}");
        }
        result.unwrap();

        recv_path
    });

    // Wait for both to complete
    let (sender_result, receiver_result) = tokio::join!(sender_handle, receiver_handle);
    sender_result.unwrap();
    let recv_path = receiver_result.unwrap();

    // Verify the received file
    let received_file = recv_path.join("test-file.txt");
    assert!(received_file.exists(), "received file should exist");

    let received_data = std::fs::read_to_string(&received_file).unwrap();
    assert_eq!(
        received_data, test_data,
        "received file content must match original"
    );
}
