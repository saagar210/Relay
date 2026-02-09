# Relay — Implementation Plan

**Prepared by:** Senior Engineering Lead
**Date:** 2026-02-08
**Status:** APPROVED with modifications noted below

---

## Executive Summary

This plan implements Relay: a peer-to-peer encrypted file transfer desktop app (Tauri 2 + SolidJS) with a Go signaling server. The spec is well-structured but has several gaps that this plan addresses. Implementation is divided into 4 phases with ~25 work units. Estimated total: 6-8 weeks for a single engineer working full-time.

---

## Spec Review: Issues Found & Resolved

### Issue 1: QUIC NAT Traversal Is Not Realistic
**Problem:** The spec assumes QUIC (quinn) can do NAT hole-punching. Quinn does not support ICE/STUN/TURN-style NAT traversal. QUIC hole-punching requires both peers to simultaneously send packets to each other's public IP:port, which only works for "easy" NAT types (full-cone, address-restricted). Symmetric NAT (common in corporate/mobile networks) will fail.

**Resolution:** Phase 1 uses direct QUIC on LAN (no NAT needed). Phase 2 implements a lightweight STUN-like probe through the signaling server to exchange public endpoints, then attempts QUIC connection with a 5-second timeout. Phase 3 implements relay fallback for when direct connection fails. This is realistic and matches how Magic Wormhole works (it also falls back to relay frequently).

### Issue 2: spake2 Crate Is Stale
**Problem:** The `spake2` crate (v0.4.0) hasn't been updated in 2+ years and uses `ed25519-dalek`, not `ring`. No security audit.

**Resolution:** Still use `spake2` v0.4.0. The algorithm is simple and well-understood (it's a thin wrapper around curve25519). The staleness is acceptable because PAKE algorithms don't need frequent updates — the math doesn't change. The crate has no `unsafe` code and the dependency on `ed25519-dalek` is fine (that crate IS actively maintained). We'll pin the version and wrap it in our own `crypto::spake2` module so we can swap implementations later if needed.

### Issue 3: nhooyr/websocket Is Deprecated
**Problem:** Spec lists `nhooyr/websocket` as an option. It's deprecated and moved to `coder/websocket`.

**Resolution:** Use `gorilla/websocket` v1.5.0. Most battle-tested, excellent docs, stable API. For a server this simple (~400 LOC), maintainer churn is irrelevant.

### Issue 4: Wire Protocol Undefined
**Problem:** The spec defines data models but not the actual messages sent over QUIC between peers during file transfer.

**Resolution:** This plan defines a complete wire protocol in Phase 1 (see "Wire Protocol Design" section below).

### Issue 5: QUIC Requires TLS Certificates
**Problem:** Quinn requires TLS configuration. For P2P between strangers, there's no CA to trust.

**Resolution:** Each peer generates a self-signed certificate at session start. The certificate fingerprint is exchanged through the signaling server (encrypted via SPAKE2-derived key). Each peer then configures quinn to trust ONLY the expected fingerprint. This provides mutual authentication — even if someone intercepts the signaling traffic, they can't forge the SPAKE2 exchange without knowing the transfer code.

### Issue 6: Transfer Code Uniqueness
**Problem:** Spec doesn't address what happens if two senders generate the same code simultaneously on the signaling server.

**Resolution:** The signaling server checks for code collisions. If a sender connects with a code that's already in an active session, the server returns an error and the client generates a new code. With 655K combinations and ~100 simultaneous sessions max, collision probability is <0.02%. One retry is sufficient.

### Issue 7: Test Structure
**Problem:** Spec puts tests in a top-level `tests/` directory. Rust convention is unit tests inline and integration tests in `tests/` within the crate.

**Resolution:** Unit tests go inline (`#[cfg(test)] mod tests`). Integration tests go in `client/src-tauri/tests/`. Go tests go alongside source files per Go convention (`server/*_test.go`). No top-level `tests/` directory.

### Issue 8: Chunk Size and Memory
**Problem:** Spec says 64KB chunks but doesn't discuss memory pressure for large files or backpressure.

**Resolution:** Use 256KB chunks (better throughput on modern networks, still fits in memory easily). Implement a bounded channel (capacity: 32 chunks = 8MB max in-flight) between the file reader and network sender. This provides natural backpressure — if the network is slower than disk, the reader pauses. Same on the receiver side.

---

## Architecture Decisions (Final)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Desktop framework | Tauri 2 | Rust backend, small binary, native feel |
| Frontend | SolidJS + TypeScript (strict) | Fine-grained reactivity for progress updates |
| Styling | Tailwind CSS v3 | Utility-first, fast iteration |
| PAKE | spake2 v0.4.0 | Proven algorithm, acceptable crate maturity |
| Symmetric encryption | ring (AES-256-GCM) | Battle-tested, hardware-accelerated on ARM64 |
| File integrity | SHA-256 via ring | Already a dependency |
| Transport | quinn (QUIC) | Reliable, encrypted, multiplexed streams |
| Async runtime | tokio (via Tauri) | Required by quinn, Tauri uses it internally |
| Signaling server | Go + gorilla/websocket | Simple, fast, good concurrency |
| WebSocket library | gorilla/websocket v1.5.0 | Most mature, stable API |
| Chunk size | 256KB | Balances throughput vs memory |
| Nonce strategy | Counter-based (96-bit) | AES-GCM nonce = 4-byte fixed + 8-byte counter |

---

## Wire Protocol Design

All messages between peers over QUIC are length-prefixed and serialized with MessagePack (via `rmp-serde`). MessagePack is chosen over JSON for binary efficiency and over protobuf for simplicity.

### Message Format
```
[4 bytes: payload length (big-endian u32)] [payload: MessagePack-encoded Message]
```

### Message Types
```rust
#[derive(Serialize, Deserialize)]
enum PeerMessage {
    /// Sender → Receiver: Here's what I want to send
    FileOffer {
        session_id: String,
        files: Vec<FileInfo>,
    },
    /// Receiver → Sender: I accept (with save path confirmation)
    FileAccept,
    /// Receiver → Sender: I decline
    FileDecline,
    /// Sender → Receiver: One chunk of file data
    FileChunk {
        file_index: u16,
        chunk_index: u32,
        data: Vec<u8>,          // Encrypted (AES-256-GCM)
        nonce: [u8; 12],
    },
    /// Sender → Receiver: File complete, here's the checksum
    FileComplete {
        file_index: u16,
        sha256: [u8; 32],
    },
    /// Receiver → Sender: Checksum verified
    FileVerified {
        file_index: u16,
    },
    /// Either → Either: All files done
    TransferComplete,
    /// Either → Either: Cancel transfer
    Cancel {
        reason: String,
    },
    /// Either → Either: Keepalive (every 5s)
    Ping,
    Pong,
}

#[derive(Serialize, Deserialize)]
struct FileInfo {
    name: String,
    size: u64,
    relative_path: Option<String>,  // For folder support (Phase 3)
}
```

### Encryption Detail
- Each `FileChunk.data` is encrypted with AES-256-GCM using the SPAKE2-derived key
- Nonce: 4-byte fixed value (random per session) + 8-byte counter (increments per chunk)
- The nonce is sent alongside the chunk (not a secret — GCM security relies on uniqueness, not secrecy)
- The wrapping `PeerMessage` itself is NOT encrypted — it's inside the QUIC TLS tunnel already
- The file data inside `data` IS encrypted because we don't trust QUIC's self-signed cert alone

**Wait — why double encrypt?** The QUIC TLS layer uses a self-signed cert we generated. If an attacker somehow compromised the cert exchange (even though it's protected by SPAKE2), the file-level encryption with the SPAKE2-derived key is a second layer. Belt and suspenders. The performance cost is negligible (~2GB/s AES-GCM on M4 via hardware acceleration).

---

## Signaling Protocol Design

### WebSocket Messages (Client ↔ Server)

```json
// Client → Server: Register as sender or receiver
{"type": "register", "role": "sender", "peer_info": {"public_ip": "", "local_ip": "192.168.1.5", "local_port": 4433}}

// Server → Client: Peer has joined
{"type": "peer_joined", "peer_info": {"public_ip": "203.0.113.5", "public_port": 4433, "local_ip": "192.168.1.10", "local_port": 4433}}

// Client → Server → Client: SPAKE2 key exchange messages (forwarded verbatim)
{"type": "spake2", "message": "base64-encoded-spake2-message"}

// Client → Server → Client: QUIC cert fingerprint exchange (encrypted with SPAKE2 key)
{"type": "cert_fingerprint", "encrypted_fingerprint": "base64-aes-gcm-encrypted-sha256-of-cert"}

// Server → Client: Error
{"type": "error", "code": "CODE_IN_USE", "message": "Transfer code already active"}

// Client → Server: Done with signaling, going to direct P2P
{"type": "disconnect"}
```

### Server Behavior
1. First client to connect with a code becomes the "sender" (must register as sender)
2. Second client becomes the "receiver" (must register as receiver)
3. Third client gets rejected (code already has two peers)
4. Server detects client public IP from the TCP connection (for NAT traversal)
5. SPAKE2 and cert_fingerprint messages are forwarded to the other peer verbatim
6. After both peers send "disconnect", session is cleaned up
7. Sessions auto-expire after 10 minutes regardless

---

## Final File Structure

```
relay/
├── client/                              # Tauri desktop app
│   ├── src/                             # SolidJS frontend
│   │   ├── index.html
│   │   ├── index.tsx                    # SolidJS entry point
│   │   ├── App.tsx                      # Root component + routing
│   │   │
│   │   ├── components/
│   │   │   ├── SendView.tsx             # File selection + code display
│   │   │   ├── ReceiveView.tsx          # Code input + file acceptance
│   │   │   ├── TransferProgress.tsx     # Progress bars, speed, ETA
│   │   │   ├── CompletionView.tsx       # Transfer complete summary
│   │   │   ├── CodeDisplay.tsx          # Large code with copy button
│   │   │   ├── CodeInput.tsx            # Digit + word + word input
│   │   │   ├── FileList.tsx             # File names, sizes, status
│   │   │   ├── SpeedGraph.tsx           # Real-time speed chart (Phase 4)
│   │   │   ├── ConnectionStatus.tsx     # Bottom bar: P2P/relay/encryption
│   │   │   └── Settings.tsx             # Minimal settings panel
│   │   │
│   │   ├── stores/
│   │   │   ├── transfer.ts              # Core transfer state (SolidJS store)
│   │   │   └── settings.ts              # Persisted settings (localStorage)
│   │   │
│   │   ├── lib/
│   │   │   ├── tauri-bridge.ts          # Typed invoke() + event listeners
│   │   │   ├── wordlist.ts              # 256 English nouns
│   │   │   └── format.ts               # formatBytes, formatSpeed, formatETA
│   │   │
│   │   └── styles/
│   │       └── app.css                  # @tailwind base/components/utilities
│   │
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── main.rs                  # Tauri entry point
│   │   │   ├── lib.rs                   # Plugin registration, command setup
│   │   │   ├── error.rs                 # AppError enum (thiserror)
│   │   │   │
│   │   │   ├── transfer/
│   │   │   │   ├── mod.rs               # pub use, TransferSession struct
│   │   │   │   ├── session.rs           # Session lifecycle management
│   │   │   │   ├── sender.rs            # Send pipeline orchestration
│   │   │   │   ├── receiver.rs          # Receive pipeline orchestration
│   │   │   │   └── progress.rs          # ProgressTracker: speed calc, ETA, events
│   │   │   │
│   │   │   ├── crypto/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── spake2.rs            # SPAKE2 wrapper (key exchange)
│   │   │   │   ├── aes_gcm.rs           # encrypt_chunk / decrypt_chunk
│   │   │   │   └── checksum.rs          # Streaming SHA-256
│   │   │   │
│   │   │   ├── network/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── signaling.rs         # WebSocket client (tungstenite)
│   │   │   │   ├── quic.rs              # Quinn setup, connect, accept
│   │   │   │   └── relay.rs             # Relay fallback (Phase 3)
│   │   │   │
│   │   │   ├── protocol/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── messages.rs          # PeerMessage enum + serialization
│   │   │   │   ├── chunker.rs           # File → 256KB encrypted chunks
│   │   │   │   └── reassembler.rs       # Chunks → file + checksum verify
│   │   │   │
│   │   │   └── commands/
│   │   │       ├── mod.rs
│   │   │       ├── send.rs              # #[tauri::command] start_send
│   │   │       ├── receive.rs           # #[tauri::command] start_receive
│   │   │       └── transfer.rs          # cancel_transfer, get_status
│   │   │
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   ├── capabilities/
│   │   │   └── default.json             # Tauri 2 capability permissions
│   │   └── build.rs
│   │
│   ├── package.json
│   ├── vite.config.ts
│   ├── tailwind.config.ts
│   ├── tsconfig.json
│   └── index.html                       # Vite HTML entry (may be here or in src/)
│
├── server/                              # Go signaling server
│   ├── main.go                          # Entry point, flag parsing, startup
│   ├── server.go                        # Server struct, HTTP handler setup
│   ├── session.go                       # Session struct, lifecycle, cleanup goroutine
│   ├── handler.go                       # WebSocket upgrade + message routing
│   ├── relay.go                         # Relay mode: forward encrypted chunks (Phase 3)
│   ├── server_test.go                   # Server integration tests
│   ├── session_test.go                  # Session unit tests
│   ├── handler_test.go                  # WebSocket handler tests
│   ├── go.mod
│   ├── go.sum
│   ├── Dockerfile
│   └── fly.toml
│
├── .gitignore
├── README.md
└── IMPLEMENTATION_PLAN.md               # This document
```

**Changes from spec:**
- Added `error.rs` for centralized error handling (no `unwrap()` per your Rust standards)
- Added `capabilities/default.json` (Tauri 2 requires explicit permissions)
- Renamed `nat.rs` to removed — NAT probing happens in `signaling.rs` (server tells client its public IP)
- Split Go `server.go` into `server.go` + `handler.go` for clarity
- Go tests alongside source files (Go convention), not in separate directory
- Rust integration tests in `src-tauri/tests/` (Cargo convention)

---

## Phase 1: Local LAN Transfer (Weeks 1-2)

**Goal:** Two instances on the same network can transfer an encrypted file.

### Step 1.1: Project Scaffolding
**Dependencies:** None
**Estimated effort:** 2-3 hours

**Actions:**
1. Initialize Tauri 2 + SolidJS project:
   ```bash
   pnpm create tauri-app client --template solid-ts --manager pnpm
   ```
2. Add Tailwind CSS:
   ```bash
   cd client && pnpm add -D tailwindcss @tailwindcss/vite
   ```
3. Configure `vite.config.ts` to include Tailwind plugin
4. Create `app.css` with Tailwind directives
5. Verify `pnpm tauri dev` launches a window with SolidJS hot-reload

**Rust dependencies (Cargo.toml):**
```toml
[dependencies]
tauri = { version = "2", features = ["protocol-asset"] }
tauri-plugin-dialog = "2"          # File picker
tauri-plugin-shell = "2"           # Open folder
tokio = { version = "1", features = ["full"] }
quinn = "0.11"
ring = "0.17"
spake2 = "0.4"
rmp-serde = "1"                    # MessagePack serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"
uuid = { version = "1", features = ["v4"] }
rand = "0.8"
base64 = "0.22"
sha2 = "0.10"                      # SHA-256 for file checksums
tokio-tungstenite = "0.24"         # WebSocket client
tracing = "0.1"
tracing-subscriber = "0.3"
rcgen = "0.13"                     # Self-signed cert generation
```

**Acceptance criteria:** App launches, shows "Hello Relay" in a Tauri window. `cargo check` passes with all dependencies resolved.

### Step 1.2: Error Handling Foundation
**Dependencies:** Step 1.1
**Estimated effort:** 1 hour

**Create `src-tauri/src/error.rs`:**
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Crypto error: {0}")]
    Crypto(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Transfer error: {0}")]
    Transfer(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] rmp_serde::encode::Error),
    #[error("Deserialization error: {0}")]
    Deserialization(#[from] rmp_serde::decode::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(String),
    #[error("Session expired")]
    SessionExpired,
    #[error("Transfer cancelled")]
    Cancelled,
    #[error("Peer rejected transfer")]
    PeerRejected,
    #[error("Checksum mismatch for file: {0}")]
    ChecksumMismatch(String),
}

// Required for Tauri commands to return errors
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
```

**Acceptance criteria:** All modules import and use `AppResult<T>`. Zero `unwrap()` calls in non-test code.

### Step 1.3: Transfer Code Generation
**Dependencies:** Step 1.1
**Estimated effort:** 2-3 hours

**Create `client/src/lib/wordlist.ts`:**
- Curate 256 common, unambiguous English nouns (no homophones, no offensive words)
- Categories: animals, colors, foods, objects, places — easy to speak aloud
- Export as `const WORDLIST: string[]`

**Create code generation in Rust (`transfer/session.rs`):**
```rust
pub struct TransferCode {
    pub digit: u8,        // 0-9
    pub word1: String,    // From wordlist
    pub word2: String,    // From wordlist
}

impl TransferCode {
    pub fn generate() -> Self { /* random digit + 2 random words */ }
    pub fn to_string(&self) -> String { format!("{}-{}-{}", self.digit, self.word1, self.word2) }
    pub fn parse(code: &str) -> AppResult<Self> { /* validate format */ }
}
```

**The wordlist must be identical in Rust and TypeScript.** Store the canonical list in a `wordlist.txt` file at `client/src-tauri/` and:
- Rust: include via `include_str!` and parse at compile time
- TypeScript: generate from the same file via a build script, OR just maintain two copies (256 words is small, unlikely to change)

**Decision:** Maintain two copies. A build script adds complexity for 256 static words. Add a unit test that loads both lists and asserts equality.

**Acceptance criteria:** `TransferCode::generate()` produces valid codes. `TransferCode::parse()` round-trips correctly. Unit tests pass.

### Step 1.4: Crypto Module
**Dependencies:** Step 1.2
**Estimated effort:** 4-5 hours

**`crypto/spake2.rs` — Key Exchange:**
```rust
use spake2::{Ed25519Group, Identity, Password, Spake2};

pub struct KeyExchange {
    state: Option<Spake2<Ed25519Group>>,
    outbound_message: Vec<u8>,
}

impl KeyExchange {
    /// Start key exchange as sender (Side::A) or receiver (Side::B)
    pub fn new(code: &str, is_sender: bool) -> Self { ... }

    /// Get the message to send to the peer
    pub fn outbound_message(&self) -> &[u8] { ... }

    /// Process the peer's message, derive the shared key
    pub fn finish(self, peer_message: &[u8]) -> AppResult<[u8; 32]> { ... }
}
```

- `Identity` values: sender uses `b"relay-sender"`, receiver uses `b"relay-receiver"`
- The `Password` is the full transfer code string (e.g., `"7-guitar-palace"`)
- The derived key is 32 bytes, used directly as the AES-256-GCM key

**`crypto/aes_gcm.rs` — Encryption:**
```rust
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};

pub struct ChunkEncryptor {
    key: LessSafeKey,
    nonce_prefix: [u8; 4],   // Random per session
    counter: u64,
}

impl ChunkEncryptor {
    pub fn new(key_bytes: &[u8; 32]) -> AppResult<Self> { ... }
    pub fn encrypt_chunk(&mut self, plaintext: &[u8]) -> AppResult<(Vec<u8>, [u8; 12])> { ... }
}

pub struct ChunkDecryptor {
    key: LessSafeKey,
}

impl ChunkDecryptor {
    pub fn new(key_bytes: &[u8; 32]) -> AppResult<Self> { ... }
    pub fn decrypt_chunk(&self, ciphertext: &[u8], nonce: &[u8; 12]) -> AppResult<Vec<u8>> { ... }
}
```

- Nonce: `[4-byte prefix][8-byte counter]` — guarantees uniqueness per session
- AAD (Additional Authenticated Data): empty (file data is self-contained)
- ring's `seal_in_place_append_tag` appends the 16-byte auth tag to ciphertext

**`crypto/checksum.rs` — File Integrity:**
```rust
pub struct StreamingChecksum { /* wraps sha2::Sha256 */ }

impl StreamingChecksum {
    pub fn new() -> Self { ... }
    pub fn update(&mut self, data: &[u8]) { ... }
    pub fn finalize(self) -> [u8; 32] { ... }
}
```

**Tests (inline `#[cfg(test)]`):**
- `test_spake2_key_exchange`: Two KeyExchange instances derive the same key
- `test_spake2_wrong_code`: Different codes produce different keys (decrypt fails)
- `test_encrypt_decrypt_roundtrip`: Encrypt then decrypt returns original plaintext
- `test_encrypt_tampered_ciphertext`: Modified ciphertext fails decryption
- `test_checksum_consistency`: Same file produces same checksum

**Acceptance criteria:** All 5+ crypto tests pass. No `unwrap()` outside tests.

### Step 1.5: Protocol Layer
**Dependencies:** Step 1.4
**Estimated effort:** 4-5 hours

**`protocol/messages.rs`:**
- Define `PeerMessage` enum as specified in Wire Protocol section above
- Implement `encode(msg: &PeerMessage) -> AppResult<Vec<u8>>` using rmp-serde + length prefix
- Implement `decode(bytes: &[u8]) -> AppResult<PeerMessage>`
- Add `async fn read_message(stream: &mut RecvStream) -> AppResult<PeerMessage>` that reads length prefix then payload
- Add `async fn write_message(stream: &mut SendStream, msg: &PeerMessage) -> AppResult<()>`

**`protocol/chunker.rs`:**
```rust
pub struct FileChunker {
    file: tokio::fs::File,
    encryptor: ChunkEncryptor,
    checksum: StreamingChecksum,
    chunk_size: usize,          // 256 * 1024
    chunk_index: u32,
    bytes_read: u64,
}

impl FileChunker {
    pub async fn new(path: &Path, encryptor: ChunkEncryptor) -> AppResult<Self> { ... }
    /// Returns None when file is fully read
    pub async fn next_chunk(&mut self) -> AppResult<Option<(Vec<u8>, [u8; 12], u32)>> { ... }
    pub fn finalize(self) -> [u8; 32] { /* SHA-256 of plaintext */ }
}
```

**`protocol/reassembler.rs`:**
```rust
pub struct FileReassembler {
    file: tokio::fs::File,
    decryptor: ChunkDecryptor,
    checksum: StreamingChecksum,
    bytes_written: u64,
}

impl FileReassembler {
    pub async fn new(path: &Path, decryptor: ChunkDecryptor) -> AppResult<Self> { ... }
    pub async fn write_chunk(&mut self, data: &[u8], nonce: &[u8; 12]) -> AppResult<()> { ... }
    pub fn verify(self, expected: &[u8; 32]) -> AppResult<()> { ... }
}
```

**Tests:**
- `test_chunker_reassembler_roundtrip`: Create temp file, chunk it, reassemble, verify identical
- `test_chunker_empty_file`: Zero-byte file produces no chunks, checksum still valid
- `test_chunker_exact_chunk_boundary`: File size exactly N * 256KB
- `test_message_encode_decode_roundtrip`: All PeerMessage variants serialize/deserialize correctly

**Acceptance criteria:** Round-trip tests pass for files of various sizes (0B, 1B, 256KB-1, 256KB, 256KB+1, 10MB).

### Step 1.6: QUIC Networking (LAN Only)
**Dependencies:** Step 1.5
**Estimated effort:** 5-6 hours

**`network/quic.rs`:**
```rust
use quinn::{Endpoint, ServerConfig, ClientConfig, Connection};
use rcgen::generate_simple_self_signed;

pub struct QuicEndpoint {
    endpoint: Endpoint,
    cert_fingerprint: [u8; 32],  // SHA-256 of DER-encoded cert
}

impl QuicEndpoint {
    /// Create endpoint that can both listen and connect
    /// Generates self-signed cert, binds to 0.0.0.0:{port}
    pub async fn new(port: u16) -> AppResult<Self> { ... }

    /// Accept one incoming connection (for receiver)
    pub async fn accept(&self, expected_fingerprint: &[u8; 32]) -> AppResult<Connection> { ... }

    /// Connect to a peer (for sender)
    pub async fn connect(
        &self,
        addr: SocketAddr,
        expected_fingerprint: &[u8; 32],
    ) -> AppResult<Connection> { ... }

    pub fn cert_fingerprint(&self) -> &[u8; 32] { &self.cert_fingerprint }
    pub fn local_addr(&self) -> AppResult<SocketAddr> { ... }
}
```

**Key implementation details:**
- Use `rcgen` to generate a self-signed cert with a random subject name
- For `accept()`: custom `rustls::ServerCertVerifier` that checks fingerprint
- For `connect()`: custom `rustls::ClientCertVerifier` that checks fingerprint
- Both use `rustls::crypto::ring::default_provider()`
- Bind to port 0 (OS assigns available port) unless configured otherwise
- QUIC transport config: keepalive every 5s, idle timeout 30s

**Testing approach:**
- Integration test: create two QuicEndpoints on localhost, connect them, send/receive a message
- Test that mismatched fingerprints are rejected

**Acceptance criteria:** Two endpoints on localhost can establish a QUIC connection and exchange PeerMessages.

### Step 1.7: Transfer Orchestration (Sender + Receiver Pipelines)
**Dependencies:** Steps 1.3, 1.4, 1.5, 1.6
**Estimated effort:** 6-8 hours

**`transfer/session.rs` — Session Manager:**
```rust
pub struct TransferSession {
    pub id: String,
    pub role: TransferRole,
    pub code: TransferCode,
    pub state: Arc<RwLock<TransferState>>,
    pub files: Vec<FileTransfer>,
    cancel_token: CancellationToken,   // tokio_util
}
```

- State is `Arc<RwLock<>>` so the progress tracker can read it while transfer runs
- `CancellationToken` enables clean cancellation from the UI

**`transfer/sender.rs` — Send Pipeline:**
```rust
pub async fn run_send(
    session: &TransferSession,
    files: Vec<PathBuf>,
    quic: QuicEndpoint,
    encryption_key: [u8; 32],
    progress_tx: mpsc::Sender<ProgressEvent>,
) -> AppResult<()> {
    // 1. Accept incoming QUIC connection
    // 2. Open bidirectional stream
    // 3. Send FileOffer with file metadata
    // 4. Wait for FileAccept or FileDecline
    // 5. For each file:
    //    a. Create FileChunker
    //    b. Send FileChunk messages (with progress events)
    //    c. Send FileComplete with checksum
    //    d. Wait for FileVerified
    // 6. Send TransferComplete
}
```

**`transfer/receiver.rs` — Receive Pipeline:**
```rust
pub async fn run_receive(
    session: &TransferSession,
    save_dir: PathBuf,
    quic: QuicEndpoint,
    peer_addr: SocketAddr,
    encryption_key: [u8; 32],
    progress_tx: mpsc::Sender<ProgressEvent>,
    accept_rx: oneshot::Receiver<bool>,  // From UI: accept/decline
) -> AppResult<()> {
    // 1. Connect to sender via QUIC
    // 2. Open bidirectional stream
    // 3. Receive FileOffer
    // 4. Send file list to UI, wait for accept/decline via accept_rx
    // 5. Send FileAccept or FileDecline
    // 6. For each file:
    //    a. Create FileReassembler
    //    b. Receive FileChunk messages (with progress events)
    //    c. Receive FileComplete, verify checksum
    //    d. Send FileVerified
    // 7. Receive TransferComplete
}
```

**`transfer/progress.rs` — Progress Tracking:**
```rust
pub struct ProgressTracker {
    start_time: Instant,
    bytes_transferred: u64,
    bytes_total: u64,
    speed_samples: VecDeque<(Instant, u64)>,  // Last 10 samples for smoothing
}

impl ProgressTracker {
    pub fn update(&mut self, bytes: u64) { ... }
    pub fn speed_bps(&self) -> u64 { /* moving average */ }
    pub fn eta_seconds(&self) -> u32 { ... }
    pub fn percent(&self) -> f32 { ... }
}

pub enum ProgressEvent {
    StateChanged(TransferState),
    TransferProgress {
        bytes_transferred: u64,
        bytes_total: u64,
        speed_bps: u64,
        eta_seconds: u32,
        current_file: String,
    },
    FileCompleted(String),
    TransferComplete { duration_seconds: u32, average_speed: u64 },
    Error(String),
}
```

- Progress events are emitted every 100ms (throttled) or on state changes
- Speed calculation uses a 3-second sliding window average for smoothness

**Acceptance criteria:** Full send/receive pipeline works on localhost with a single file. Progress events fire correctly.

### Step 1.8: Tauri Commands (Bridge to Frontend)
**Dependencies:** Step 1.7
**Estimated effort:** 3-4 hours

**`commands/send.rs`:**
```rust
#[tauri::command]
pub async fn start_send(
    app: tauri::AppHandle,
    file_paths: Vec<String>,
) -> Result<String, String> {
    // 1. Generate TransferCode
    // 2. Create TransferSession
    // 3. Create QuicEndpoint
    // 4. Spawn tokio task: run_send pipeline
    // 5. Spawn tokio task: forward ProgressEvents as Tauri events
    // 6. Return transfer code to frontend
}
```

**`commands/receive.rs`:**
```rust
#[tauri::command]
pub async fn start_receive(
    app: tauri::AppHandle,
    code: String,
    save_dir: String,
) -> Result<(), String> {
    // 1. Parse TransferCode
    // 2. Create TransferSession
    // 3. Create QuicEndpoint
    // 4. Spawn tokio task: run_receive pipeline
    // 5. Spawn tokio task: forward ProgressEvents as Tauri events
    // 6. Return immediately (progress via events)
}

#[tauri::command]
pub async fn accept_transfer(session_id: String, accept: bool) -> Result<(), String> {
    // Send accept/decline through the oneshot channel
}
```

**`commands/transfer.rs`:**
```rust
#[tauri::command]
pub async fn cancel_transfer(session_id: String) -> Result<(), String> {
    // Trigger CancellationToken
}
```

**Tauri events emitted to frontend:**
- `transfer:progress` — ProgressEvent::TransferProgress
- `transfer:state` — ProgressEvent::StateChanged
- `transfer:file-completed` — ProgressEvent::FileCompleted
- `transfer:complete` — ProgressEvent::TransferComplete
- `transfer:error` — ProgressEvent::Error
- `transfer:offer` — File list for receiver to accept/decline

**Register commands in `lib.rs`:**
```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::send::start_send,
            commands::receive::start_receive,
            commands::receive::accept_transfer,
            commands::transfer::cancel_transfer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**`tauri.conf.json` permissions:** Add `dialog:allow-open` for file picker, `shell:allow-open` for "open folder" button.

Actually for Tauri 2, permissions go in `capabilities/default.json`:
```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:allow-open",
    "dialog:allow-save",
    "shell:allow-open"
  ]
}
```

**Acceptance criteria:** Frontend can invoke `start_send` and `start_receive`. Events flow from Rust to JS. Cancel works.

### Step 1.9: Frontend — Core UI
**Dependencies:** Step 1.8 (can start earlier with mock data)
**Estimated effort:** 8-10 hours

**`stores/transfer.ts`:**
```typescript
import { createStore } from "solid-js/store";

export type TransferState =
  | { phase: "idle" }
  | { phase: "waiting"; code: string }
  | { phase: "connecting" }
  | { phase: "offer"; files: FileInfo[] }          // Receiver: accept/decline
  | { phase: "transferring"; progress: TransferProgress }
  | { phase: "completed"; summary: TransferSummary }
  | { phase: "error"; message: string };

export interface FileInfo {
  name: string;
  size: number;
}

export interface TransferProgress {
  bytesTransferred: number;
  bytesTotal: number;
  speedBps: number;
  etaSeconds: number;
  currentFile: string;
  completedFiles: string[];
}

export interface TransferSummary {
  filesCount: number;
  totalBytes: number;
  durationSeconds: number;
  averageSpeed: number;
}

const [transfer, setTransfer] = createStore<{ state: TransferState }>({
  state: { phase: "idle" },
});

export { transfer, setTransfer };
```

**`lib/tauri-bridge.ts`:**
```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export async function startSend(filePaths: string[]): Promise<string> {
  return invoke<string>("start_send", { filePaths });
}

export async function startReceive(code: string, saveDir: string): Promise<void> {
  return invoke("start_receive", { code, saveDir });
}

export async function acceptTransfer(sessionId: string, accept: boolean): Promise<void> {
  return invoke("accept_transfer", { sessionId, accept });
}

export async function cancelTransfer(sessionId: string): Promise<void> {
  return invoke("cancel_transfer", { sessionId });
}

// Event listeners — call in App.tsx onMount
export function setupEventListeners(handlers: EventHandlers) {
  listen("transfer:progress", (e) => handlers.onProgress(e.payload));
  listen("transfer:state", (e) => handlers.onStateChange(e.payload));
  // ... etc
}
```

**Components (implement in this order):**

1. **`App.tsx`** — Router between views based on `transfer.state.phase`
   - `idle` → show Send/Receive buttons
   - `waiting` → SendView (showing code)
   - `connecting` → ReceiveView (showing spinner)
   - `offer` → ReceiveView (file acceptance dialog)
   - `transferring` → TransferProgress
   - `completed` → CompletionView
   - `error` → Error display with "Try Again" button

2. **`SendView.tsx`** —
   - "Select Files" button (uses Tauri dialog)
   - Drag-and-drop zone (Phase 4, just the button for now)
   - Shows selected file list (FileList component)
   - "Start Sending" button → calls `startSend`, transitions to `waiting`
   - Shows `CodeDisplay` when code is generated

3. **`ReceiveView.tsx`** —
   - `CodeInput` component for entering transfer code
   - "Connect" button → calls `startReceive`
   - When `offer` state: show file list + Accept/Decline buttons

4. **`CodeDisplay.tsx`** — Large, prominent code display
   - Large font for the code (monospace)
   - Copy button (uses `navigator.clipboard.writeText`)
   - "Waiting for receiver..." with animated dots

5. **`CodeInput.tsx`** — Three input fields
   - First field: single digit (0-9), auto-advance on input
   - Second/third fields: text with autocomplete from wordlist
   - Auto-lowercase, trim whitespace
   - Validation: highlight invalid fields

6. **`FileList.tsx`** — Reusable file list
   - File name, formatted size, icon based on type
   - Checkmark when completed
   - Error indicator if failed

7. **`TransferProgress.tsx`** — Main progress display
   - Overall progress bar (percentage)
   - Current file name + per-file progress
   - Speed display: "12.4 MB/s"
   - ETA: "2 min remaining"
   - Cancel button
   - Connection status indicator (hardcode "Direct" for Phase 1)

8. **`CompletionView.tsx`** — Transfer complete
   - Summary: file count, total size, duration, average speed
   - "Send More" / "Receive More" buttons → back to idle
   - "Open Folder" button (receiver only, uses Tauri shell:open)

9. **`ConnectionStatus.tsx`** — Bottom bar (stubbed for Phase 1)
   - "Encryption: AES-256-GCM" with lock icon
   - Connection type: "Direct" (always, in Phase 1)
   - Placeholder for latency display

**Styling approach:**
- Dark theme by default (light in Phase 4)
- Color palette: dark gray background (#0f0f0f), accent blue (#3b82f6), success green (#22c55e)
- Tailwind utility classes, no custom CSS beyond Tailwind config
- Animate progress bars with CSS transitions (SolidJS handles DOM updates efficiently)

**Acceptance criteria:** Full UI flow works end-to-end on LAN. Select files → generate code → enter code on second instance → accept → transfer → completion.

### Step 1.10: LAN Integration Test
**Dependencies:** Steps 1.7, 1.9
**Estimated effort:** 3-4 hours

**Test (`src-tauri/tests/lan_transfer.rs`):**
```rust
#[tokio::test]
async fn test_lan_transfer_single_file() {
    // 1. Create temp file with random content (1MB)
    // 2. Start sender pipeline
    // 3. Start receiver pipeline with sender's code + localhost addr
    // 4. Wait for completion
    // 5. Compare sent file with received file (byte-for-byte)
}

#[tokio::test]
async fn test_lan_transfer_cancel() {
    // 1. Start transfer of large file (100MB)
    // 2. Cancel after 1 second
    // 3. Verify both sides clean up gracefully
}

#[tokio::test]
async fn test_wrong_code_fails() {
    // 1. Sender generates code A
    // 2. Receiver uses code B
    // 3. SPAKE2 derives different keys
    // 4. First chunk decryption fails
    // 5. Verify clean error propagation
}
```

**Acceptance criteria:** All integration tests pass. Transfer works with files from 0B to 100MB.

---

## Phase 2: Signaling Server + Internet Transfer (Weeks 3-4)

**Goal:** Transfer files between any two devices on the internet.

### Step 2.1: Go Signaling Server — Core
**Dependencies:** None (can start in parallel with Phase 1)
**Estimated effort:** 6-8 hours

**`server/main.go`:**
```go
package main

import (
    "flag"
    "log"
    "net/http"
    "os"
    "time"
)

func main() {
    addr := flag.String("addr", ":8080", "listen address")
    maxSessions := flag.Int("max-sessions", 1000, "maximum concurrent sessions")
    sessionTTL := flag.Duration("session-ttl", 10*time.Minute, "session time-to-live")
    flag.Parse()

    srv := NewServer(*maxSessions, *sessionTTL)
    go srv.CleanupLoop(60 * time.Second)

    mux := http.NewServeMux()
    mux.HandleFunc("GET /ws/{code}", srv.HandleWebSocket)
    mux.HandleFunc("GET /health", srv.HandleHealth)

    log.Printf("Relay signaling server listening on %s", *addr)
    if err := http.ListenAndServe(*addr, mux); err != nil {
        log.Fatal(err)
    }
}
```

**`server/session.go`:**
```go
type Session struct {
    Code         string
    Sender       *Peer
    Receiver     *Peer
    CreatedAt    time.Time
    ExpiresAt    time.Time
    mu           sync.Mutex
}

type Peer struct {
    Conn     *websocket.Conn
    Role     string                  // "sender" or "receiver"
    Info     json.RawMessage         // PeerInfo, forwarded to other peer
    Done     chan struct{}
}

type Server struct {
    sessions    map[string]*Session
    mu          sync.RWMutex
    maxSessions int
    sessionTTL  time.Duration
}

func NewServer(maxSessions int, ttl time.Duration) *Server { ... }

func (s *Server) GetOrCreateSession(code string) (*Session, error) {
    // If session exists and has both peers → error: code in use
    // If session exists and has one peer → return it
    // If no session → create new one
    // If at capacity → error: server full
}

func (s *Server) RemoveSession(code string) { ... }

func (s *Server) CleanupLoop(interval time.Duration) {
    ticker := time.NewTicker(interval)
    for range ticker.C {
        s.mu.Lock()
        now := time.Now()
        for code, sess := range s.sessions {
            if now.After(sess.ExpiresAt) {
                // Close WebSocket connections gracefully
                sess.Close()
                delete(s.sessions, code)
            }
        }
        s.mu.Unlock()
    }
}
```

**`server/handler.go`:**
```go
func (s *Server) HandleWebSocket(w http.ResponseWriter, r *http.Request) {
    code := r.PathValue("code")
    if code == "" {
        http.Error(w, "missing code", http.StatusBadRequest)
        return
    }

    // Upgrade to WebSocket
    conn, err := upgrader.Upgrade(w, r, nil)
    if err != nil { return }
    defer conn.Close()

    // Read first message: must be "register"
    var msg SignalMessage
    if err := conn.ReadJSON(&msg); err != nil { return }
    if msg.Type != "register" { return }

    // Extract role and peer_info from register message
    // Get or create session
    session, err := s.GetOrCreateSession(code)
    if err != nil {
        conn.WriteJSON(SignalMessage{Type: "error", ...})
        return
    }

    // Add this peer to session
    // Detect public IP from r.RemoteAddr (handle X-Forwarded-For for proxies)
    peer := &Peer{Conn: conn, Role: role, Info: peerInfo, Done: make(chan struct{})}

    // If both peers now connected, notify each about the other
    if session.BothConnected() {
        session.Sender.Conn.WriteJSON(SignalMessage{Type: "peer_joined", ...})
        session.Receiver.Conn.WriteJSON(SignalMessage{Type: "peer_joined", ...})
    }

    // Message forwarding loop
    for {
        var msg SignalMessage
        if err := conn.ReadJSON(&msg); err != nil { break }

        switch msg.Type {
        case "spake2", "cert_fingerprint":
            // Forward to other peer verbatim
            other := session.OtherPeer(peer)
            if other != nil {
                other.Conn.WriteJSON(msg)
            }
        case "disconnect":
            return
        }
    }
}

func (s *Server) HandleHealth(w http.ResponseWriter, r *http.Request) {
    s.mu.RLock()
    count := len(s.sessions)
    s.mu.RUnlock()
    json.NewEncoder(w).Encode(map[string]interface{}{
        "status": "ok",
        "active_sessions": count,
    })
}
```

**`server/go.mod`:**
```
module github.com/yourusername/relay/server

go 1.22

require github.com/gorilla/websocket v1.5.0
```

**Tests (`server/server_test.go`, `server/handler_test.go`):**
- `TestSessionCreation`: Create session, verify fields
- `TestSessionExpiry`: Create session, advance time, verify cleanup
- `TestMaxSessions`: Hit capacity limit, verify error
- `TestWebSocketHandshake`: Two clients connect, verify peer_joined messages
- `TestSPAKE2Forwarding`: Messages forwarded correctly between peers
- `TestDuplicateCode`: Third client rejected
- `TestHealthEndpoint`: Returns 200 with session count

Use `httptest.NewServer` + gorilla's `websocket.Dialer` for WebSocket tests.

**Acceptance criteria:** All tests pass. Server handles 100 concurrent sessions without leaking goroutines.

### Step 2.2: Docker + Deployment Config
**Dependencies:** Step 2.1
**Estimated effort:** 1-2 hours

**`server/Dockerfile`:**
```dockerfile
FROM golang:1.22-alpine AS builder
WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -o relay-server .

FROM alpine:3.19
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/relay-server /usr/local/bin/
EXPOSE 8080
HEALTHCHECK CMD wget -q --spider http://localhost:8080/health || exit 1
CMD ["relay-server"]
```

**`server/fly.toml`:**
```toml
app = "relay-signal"
primary_region = "sjc"

[build]
  dockerfile = "Dockerfile"

[http_service]
  internal_port = 8080
  force_https = true

[[http_service.checks]]
  path = "/health"
  interval = 30000
  timeout = 5000
```

**Acceptance criteria:** `docker build` and `docker run` work. Health check responds.

### Step 2.3: Client Signaling Integration
**Dependencies:** Steps 1.7, 2.1
**Estimated effort:** 6-8 hours

**`network/signaling.rs`:**
```rust
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub struct SignalingClient {
    ws: WebSocketStream<...>,
    server_url: String,
}

impl SignalingClient {
    pub async fn connect(server_url: &str, code: &str) -> AppResult<Self> {
        let url = format!("{}/ws/{}", server_url, code);
        let (ws, _) = connect_async(&url).await
            .map_err(|e| AppError::WebSocket(e.to_string()))?;
        Ok(Self { ws, server_url: server_url.to_string() })
    }

    pub async fn register(&mut self, role: TransferRole, local_addr: SocketAddr) -> AppResult<()> {
        // Send register message with role and local peer info
    }

    pub async fn wait_for_peer(&mut self) -> AppResult<PeerInfo> {
        // Wait for "peer_joined" message, return peer's connection info
    }

    pub async fn exchange_spake2(&mut self, outbound: &[u8]) -> AppResult<Vec<u8>> {
        // Send our SPAKE2 message, receive peer's SPAKE2 message
    }

    pub async fn exchange_cert_fingerprint(
        &mut self,
        our_fingerprint: &[u8; 32],
        encryption_key: &[u8; 32],
    ) -> AppResult<[u8; 32]> {
        // Encrypt our fingerprint with SPAKE2 key, send it
        // Receive peer's encrypted fingerprint, decrypt it
    }

    pub async fn disconnect(&mut self) -> AppResult<()> {
        // Send disconnect message, close WebSocket
    }
}
```

**Update `transfer/sender.rs` and `transfer/receiver.rs`:**

The send/receive pipelines now have an additional signaling phase before QUIC:
```
1. Connect to signaling server via WebSocket
2. Register as sender/receiver
3. Wait for peer to join
4. Exchange SPAKE2 messages through server → derive shared key
5. Exchange QUIC cert fingerprints (encrypted with shared key)
6. Disconnect from signaling server
7. Attempt direct QUIC connection to peer's public IP:port
8. If direct fails after 5s, try peer's local IP:port (same LAN fallback)
9. If both fail, error (relay is Phase 3)
10. Proceed with file transfer over QUIC
```

**NAT traversal strategy (simple):**
- Server detects each client's public IP from the TCP connection
- Clients report their local IP and QUIC listen port
- Both sides attempt QUIC connection simultaneously (both call `connect` to each other's public address while also running `accept`)
- This works for easy NAT types (full-cone, address-restricted)
- For symmetric NAT, direct connection fails → relay fallback in Phase 3

**Settings for signaling server URL (`stores/settings.ts`):**
```typescript
const DEFAULT_SIGNAL_URL = "wss://relay-signal.fly.dev";

export const [settings, setSettings] = createStore({
  signalServerUrl: localStorage.getItem("signalServerUrl") || DEFAULT_SIGNAL_URL,
  defaultSaveDir: "",  // Empty = use system Downloads
});
```

**Acceptance criteria:** Two clients on different networks can complete a transfer through the signaling server, assuming favorable NAT. Signaling server never sees encryption keys or file content.

### Step 2.4: Internet Integration Test
**Dependencies:** Step 2.3
**Estimated effort:** 3-4 hours

**Test approach:**
- Run signaling server locally on a random port
- Two client instances connect through it (on localhost, simulating internet)
- Full transfer pipeline works end-to-end
- Test with signaling server restart mid-session (should error gracefully)

**Acceptance criteria:** Integration test passes. Error messages are clear when signaling server is unreachable.

---

## Phase 3: Relay Fallback + Multi-file (Weeks 5-6)

**Goal:** Reliable transfers regardless of NAT. Multiple files per session.

### Step 3.1: Server Relay Mode
**Dependencies:** Step 2.1
**Estimated effort:** 4-5 hours

**`server/relay.go`:**

When direct P2P fails, the client keeps the WebSocket connection to the signaling server open and sends file data through it.

```go
// Relay mode: after signaling phase, if clients send "relay_request",
// the server forwards binary messages between them.

func (s *Server) HandleRelay(session *Session) {
    // Both peers send: {"type": "relay_request"}
    // Server acknowledges: {"type": "relay_active"}
    // Then: binary WebSocket messages are forwarded bidirectionally
    // Data is still encrypted (AES-256-GCM) — server sees only ciphertext

    // Bandwidth limiting: cap at 10 MB/s per session (configurable)
    // Use a token bucket rate limiter per session
}
```

**Bandwidth limiter:**
```go
type RateLimiter struct {
    tokens    float64
    maxTokens float64
    refillRate float64    // bytes per second
    lastRefill time.Time
    mu        sync.Mutex
}
```

Cap relay bandwidth at 10 MB/s per session by default (configurable via flag). This prevents abuse while allowing reasonable transfer speeds.

**Client changes (`network/relay.rs`):**
```rust
pub struct RelayTransport {
    ws: WebSocketStream<...>,
    encryption_key: [u8; 32],
}

impl RelayTransport {
    /// Wraps the signaling WebSocket connection for relay mode
    pub async fn from_signaling(client: SignalingClient) -> AppResult<Self> { ... }

    /// Send an encrypted chunk through the relay
    pub async fn send_message(&mut self, msg: &PeerMessage) -> AppResult<()> {
        let encoded = protocol::encode(msg)?;
        self.ws.send(Message::Binary(encoded)).await?;
        Ok(())
    }

    /// Receive a message through the relay
    pub async fn recv_message(&mut self) -> AppResult<PeerMessage> {
        // Read binary WebSocket message, decode as PeerMessage
    }
}
```

**Update sender.rs and receiver.rs** to accept a trait/enum for transport:
```rust
enum Transport {
    Direct(quinn::Connection),
    Relayed(RelayTransport),
}
```

Both transports implement the same send/receive message interface, so the file transfer logic doesn't need to change.

**Connection attempt flow (updated):**
```
1. Complete signaling (SPAKE2, cert exchange)
2. Try direct QUIC to peer's public IP:port (5s timeout)
3. Try direct QUIC to peer's local IP:port (3s timeout, same LAN case)
4. If both fail: request relay mode from signaling server
5. Continue file transfer over relayed WebSocket
6. UI shows "Relayed" instead of "Direct P2P"
```

**Acceptance criteria:** Transfer completes via relay when direct connection is blocked. Rate limiter works. Server memory stays bounded.

### Step 3.2: Multi-File Transfer
**Dependencies:** Step 1.7
**Estimated effort:** 4-5 hours

**Changes to sender pipeline:**
- `FileOffer` now contains multiple `FileInfo` entries
- After `FileAccept`, sender transfers files sequentially (one at a time)
- Each file gets its own `FileChunker` instance
- `file_index` in `FileChunk` identifies which file a chunk belongs to

**Changes to receiver pipeline:**
- `FileOffer` shows multiple files in the acceptance dialog
- Receiver creates a `FileReassembler` per file
- `file_index` routes chunks to the correct reassembler
- Each file verified independently

**Changes to progress tracking:**
- `bytes_total` = sum of all file sizes
- `bytes_transferred` = cumulative across all files
- `current_file` updates as each file starts
- Per-file status shown in UI (pending/transferring/completed)

**Changes to UI:**
- `FileList.tsx` shows per-file status during transfer
- Progress bar shows overall progress
- Current file highlighted in list

### Step 3.3: Folder Support
**Dependencies:** Step 3.2
**Estimated effort:** 3-4 hours

**Sender:**
- When user selects a folder, walk directory tree recursively
- Each file's `relative_path` stores its path relative to the selected folder
- Skip hidden files (`.DS_Store`, `Thumbs.db`, etc.)
- Skip empty directories

**Receiver:**
- Recreate directory structure under save_dir
- `relative_path` determines subdirectory
- Sanitize paths: reject `..` components, absolute paths (security)

**Path sanitization is critical.** A malicious sender could craft a `relative_path` like `../../../etc/passwd`. The receiver MUST:
1. Reject any path containing `..`
2. Reject absolute paths
3. Normalize separators (handle Windows vs Unix)
4. Reject null bytes in paths

### Step 3.4: Connection Quality + Cancel
**Dependencies:** Steps 3.1, 3.2
**Estimated effort:** 3-4 hours

**`ConnectionStatus.tsx` — Update for real data:**
- "Direct P2P" (green indicator) or "Relayed" (yellow indicator)
- Latency: ping/pong round-trip time (from QUIC or relay keepalive)
- "Encrypted: AES-256-GCM" with lock icon

**Cancel mid-transfer:**
- Sender or receiver clicks Cancel
- Send `PeerMessage::Cancel` to peer
- Clean up partial files on receiver side
- Close QUIC connection / WebSocket
- Return to idle state

**File acceptance dialog:**
- Show file list with names and sizes
- Total transfer size
- Accept / Decline buttons
- Receiver can change save location before accepting

**Acceptance criteria:** Multi-file transfer works (10+ files). Folder structure preserved. Cancel cleans up partial files. Relay fallback works transparently.

---

## Phase 4: Polish & Launch (Weeks 7-8)

### Step 4.1: Speed Graph
**Dependencies:** Phase 1 complete
**Estimated effort:** 3-4 hours

**`SpeedGraph.tsx`:**
- Small real-time line chart showing transfer speed over last 30 seconds
- Use a Canvas element (no charting library — it's just a polyline)
- Update every 500ms from progress events
- Y-axis auto-scales, X-axis is time window
- Show peak speed annotation

### Step 4.2: Drag-and-Drop
**Dependencies:** Phase 1 complete
**Estimated effort:** 2-3 hours

- Listen for `dragover` and `drop` events on the main window
- Extract file paths from the drop event
- Transition directly to send flow with those files
- Visual feedback: highlight border when dragging over the window

**Note:** Tauri 2 has built-in drag-and-drop support via `tauri-plugin-fs`. Use `onDragDropEvent` from `@tauri-apps/api/window`.

### Step 4.3: Theme Support
**Dependencies:** Phase 1 complete
**Estimated effort:** 2-3 hours

- Dark theme (default) and light theme
- Tailwind `dark:` variants
- Toggle in Settings
- System preference detection via `prefers-color-scheme` media query
- Persist choice in localStorage

### Step 4.4: Keyboard Shortcuts + UX Polish
**Dependencies:** Phase 3 complete
**Estimated effort:** 3-4 hours

- `Cmd+O` / `Ctrl+O`: Open file picker
- `Cmd+V` / `Ctrl+V`: Paste code from clipboard into code input
- `Escape`: Cancel current transfer
- `Cmd+,` / `Ctrl+,`: Open settings

**UX polish:**
- Animated dots for "Waiting for receiver..."
- Smooth progress bar transitions (CSS `transition: width 200ms ease`)
- File type icons (generic file, image, video, document, archive)
- Completion animation (subtle confetti or checkmark scale-in)

### Step 4.5: App Icon + Branding
**Dependencies:** None
**Estimated effort:** 2-3 hours

- Design app icon: simple relay/transfer concept (two arrows, node-to-node)
- Generate icon in required sizes for macOS (.icns), Windows (.ico), Linux (.png)
- Set in `tauri.conf.json` under `bundle.icon`
- Window title: "Relay"
- About dialog: version, "Share files. No cloud. No accounts."

### Step 4.6: Comprehensive Test Suite
**Dependencies:** All phases complete
**Estimated effort:** 6-8 hours

**Rust tests (in addition to per-step tests):**
- Crypto: key exchange with all edge cases, encryption with max-size chunks
- Protocol: all message types serialize/deserialize, invalid messages rejected
- Transfer: multi-file, empty files, large files (1GB via streaming), cancel at various stages
- Network: connection timeout, server disconnect, relay fallback

**Go tests:**
- Concurrent session creation (100 goroutines)
- Session expiry under load
- WebSocket message ordering
- Relay bandwidth limiting
- Graceful shutdown

**Integration tests:**
- Full transfer: sender → signaling → receiver (localhost)
- Wrong code produces clear error
- Server crash recovery
- Transfer resume NOT supported (verify clean error, not corruption)

### Step 4.7: Build + Distribution
**Dependencies:** All phases complete
**Estimated effort:** 3-4 hours

**Tauri builds:**
```bash
pnpm tauri build          # Builds for current platform
```
- macOS: `.dmg` (universal binary: x86_64 + aarch64)
- Windows: `.msi` installer
- Linux: `.AppImage` + `.deb`

**Configure in `tauri.conf.json`:**
```json
{
  "bundle": {
    "active": true,
    "identifier": "com.relay.app",
    "icon": ["icons/icon.icns", "icons/icon.ico", "icons/icon.png"],
    "targets": "all"
  }
}
```

**Server Docker image:**
```bash
docker build -t relay-server ./server
```

**GitHub Actions CI (`.github/workflows/build.yml`):**
- On push to main: run tests (Rust + Go)
- On tag: build all platforms + create GitHub release with artifacts

### Step 4.8: README + Documentation
**Dependencies:** Step 4.7
**Estimated effort:** 2-3 hours

- README with: what it is, screenshot/GIF, how to use, how to build, how to self-host server
- Architecture diagram (ASCII or simple SVG)
- Security model explanation
- Self-hosting guide for signaling server

---

## Assumptions

1. **macOS is the primary development platform.** Windows and Linux builds will be tested before release but not during active development of each phase.

2. **The signaling server will be deployed to Fly.io** for production. During development, it runs locally.

3. **No mobile support.** Tauri 2 supports mobile, but it's out of scope. Quinn/QUIC on mobile has known issues.

4. **Single transfer per app instance.** You can't send and receive simultaneously. Keeps the state model simple.

5. **No transfer resume.** If a transfer fails, you start over. Resume requires persistent state (chunk tracking, partial file management) that isn't worth the complexity for V1.

6. **No relay for QUIC (only WebSocket).** The relay fallback uses WebSocket through the signaling server, not QUIC through a TURN-like server. This is simpler and sufficient — relay mode will be slower than direct P2P but functional.

7. **The 256-word list is static.** No internationalization. English-only codes.

8. **No auto-update mechanism.** Users download new versions manually or via GitHub releases. Can add Tauri's built-in updater later.

9. **File size is limited by available disk space**, not by the app. There's no artificial limit. Files stream through memory in 256KB chunks, so even 100GB+ files are fine.

10. **The signaling server is trusted not to DoS but not trusted with data.** It can deny service (refuse connections) but can never read file contents (SPAKE2 + AES-256-GCM).

---

## Dependency Graph (Critical Path)

```
Phase 1:
  1.1 Scaffolding ─────┬── 1.2 Error handling ──┬── 1.4 Crypto ──┐
                        │                         │                │
                        ├── 1.3 Transfer codes ───┘                ├── 1.5 Protocol ── 1.6 QUIC ── 1.7 Orchestration ── 1.8 Commands
                        │                                          │
                        └── 1.9 Frontend (can start with mocks) ───┘── 1.10 Integration test

Phase 2 (can start 2.1 in parallel with Phase 1):
  2.1 Go server ── 2.2 Docker ── 2.3 Client signaling ── 2.4 Integration test

Phase 3:
  3.1 Server relay ── 3.2 Multi-file ── 3.3 Folders ── 3.4 Connection quality

Phase 4:
  4.1-4.5 (all independent, can parallelize)
  4.6 Tests (after all features)
  4.7 Build (after 4.6)
  4.8 README (after 4.7)
```

**Critical path:** 1.1 → 1.4 → 1.5 → 1.6 → 1.7 → 1.8 → 2.3 → 3.1 → 3.2 → 4.6 → 4.7

**Parallelization opportunities:**
- Go server (2.1-2.2) can be built entirely in parallel with Phase 1
- Frontend (1.9) can start with mock data while Rust backend is built
- Phase 4 polish items (4.1-4.5) are all independent

---

## Risk Register

| Risk | Impact | Mitigation |
|------|--------|------------|
| Quinn NAT traversal fails in most real networks | High | Relay fallback is Phase 3 priority. Most real transfers will be relayed. That's fine — Magic Wormhole does this too. |
| spake2 crate has undiscovered vulnerability | Medium | Wrap in our own module for swappability. Algorithm itself is well-studied. |
| Tauri 2 breaking changes | Medium | Pin exact version. Tauri 2 is stable release. |
| Large file transfer OOM | Low | Streaming design (256KB chunks) prevents this. Bounded channel provides backpressure. |
| Signaling server abuse | Medium | Rate limit connections per IP. Session TTL prevents resource exhaustion. Max sessions cap. |
| Go gorilla/websocket maintenance drops | Low | For <500 LOC server, we can vendorise or switch to coder/websocket in an hour. |

---

## Sign-off

**Review complete.** This plan addresses all gaps found in the original spec:

1. **Added realistic NAT traversal strategy** — try direct, fall back to relay, don't pretend QUIC magically punches through all NATs
2. **Defined complete wire protocol** — MessagePack-based, all message types specified
3. **Defined signaling protocol** — including cert fingerprint exchange for QUIC authentication
4. **Resolved crate concerns** — spake2 is acceptable, gorilla/websocket over deprecated nhooyr
5. **Added path sanitization** — critical security for folder support
6. **Added bandwidth limiting** — relay mode can't be abused
7. **Increased chunk size** to 256KB — better throughput with negligible memory cost
8. **Clarified double-encryption rationale** — QUIC TLS + AES-GCM file-level encryption

**Judgment calls made:**
- Chose sequential file transfer over concurrent (simpler progress tracking, QUIC handles multiplexing internally)
- Chose MessagePack over protobuf (simpler for this project size, no codegen step)
- Chose Canvas for speed graph over a charting library (one simple polyline doesn't justify a dependency)
- Chose NOT to implement transfer resume (complexity vs. value tradeoff — restart is fine for V1)
- Chose counter-based nonces over random nonces (guaranteed uniqueness > probabilistic uniqueness)

A competent engineer can execute this plan without asking clarifying questions. Each step has clear inputs, outputs, acceptance criteria, and dependencies.
