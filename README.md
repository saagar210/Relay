# Relay

Fast, secure file transfer app for sending files between devices on your network or across the internet.

## Features

- **Direct LAN transfers** — QUIC connections for maximum speed when devices are on the same network
- **Automatic relay fallback** — WebSocket relay through signaling server when NAT/firewalls block direct connections
- **End-to-end encryption** — SPAKE2 password-authenticated key exchange + AES-256-GCM encryption
- **Folder support** — Send entire directories while preserving nested structure
- **Real-time progress** — Track transfer speed, progress, and connection status
- **Zero configuration** — No port forwarding or network setup required

## How It Works

1. **Sender** creates a transfer code and starts listening
2. **Receiver** enters the code and connects to the signaling server
3. **Key exchange** via SPAKE2 protocol (password = transfer code)
4. **Connection attempt**: tries direct QUIC connection first
5. **Automatic fallback**: if QUIC fails (NAT/firewall), switches to encrypted WebSocket relay
6. **File transfer** happens over the secure connection (direct or relayed)

## Architecture

- **Client**: Tauri app (Rust backend + React/TypeScript frontend)
- **Server**: Go signaling server + WebSocket relay
- **Transport**: QUIC for direct transfers, WebSocket for relay fallback
- **Encryption**: SPAKE2 key exchange + AES-256-GCM chunk encryption

## Development

### Prerequisites
- Rust/Cargo
- Go 1.22+
- Node.js/pnpm
- macOS/Linux (Windows support TBD)

### Running the signaling server
```bash
cd server
go build -o relay-server .
./relay-server
```

Server flags:
- `--addr` — listen address (default: `:8080`)
- `--max-sessions` — max concurrent sessions (default: 1000)
- `--session-ttl` — session expiration (default: 1h)
- `--relay-rate-limit` — relay bandwidth limit in bytes/sec (default: 10 MB/s)

### Running the client
```bash
cd client
pnpm install
pnpm tauri dev
```

### Testing
```bash
# Go server tests (with race detector)
cd server && go test -race ./...

# Rust unit tests
cd client/src-tauri && cargo test

# Rust integration tests (requires server binary at server/relay-server)
cd client/src-tauri && cargo test --test signaling_e2e

# Frontend build check
cd client && pnpm build
```

## Status

**Completed:**
- Phase 1: Direct QUIC transfers on LAN
- Phase 2: Signaling server integration
- Phase 3: Relay fallback + folder support

**All tests passing** (12 Go tests, 26 Rust unit tests, 5 Rust integration tests)

## License

MIT
