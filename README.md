# Relay

Fast, secure, peer-to-peer file transfer. Direct LAN, automatic relay fallback, end-to-end encrypted.

## Features

### Core
- **Direct LAN transfers** — QUIC connections for blazing-fast local network speeds
- **Automatic relay fallback** — Works anywhere (home, office, across internet)
- **Zero-knowledge architecture** — Files never touch cloud; signaling server is encrypted-blind
- **End-to-end encrypted** — SPAKE2 + AES-256-GCM, verified SHA-256 checksums
- **Multi-file transfers** — Send entire folders with nested structure preserved

### UX
- **Real-time progress** — Live speed graph, ETA, per-file status
- **Drag & drop** — Drop files into app or use traditional file picker
- **Keyboard shortcuts** — Power-user workflow (Cmd+O, Cmd+V, Escape, Cmd+,)
- **Dark/Light theme** — System preference detection + manual toggle
- **Zero configuration** — No port forwarding or network setup required

### Platform
- **Desktop-first** — macOS is the primary target today; Linux follows next
- **Self-hostable** — Run your own signaling server (Docker, Fly.io, bare metal)

## How It Works

1. **Sender** creates a transfer code and starts listening
2. **Receiver** enters the code and connects to the signaling server
3. **Key exchange** via SPAKE2 protocol (password = transfer code)
4. **Connection attempt**: tries direct QUIC connection first
5. **Automatic fallback**: if QUIC fails (NAT/firewall), switches to encrypted WebSocket relay
6. **File transfer** happens over the secure connection (direct or relayed)

## Architecture

- **Client**: Tauri app (Rust backend + Solid/TypeScript frontend)
- **Server**: Go signaling server + WebSocket relay
- **Transport**: QUIC for direct transfers, WebSocket for relay fallback
- **Encryption**: SPAKE2 key exchange + AES-256-GCM chunk encryption

## Development

### Prerequisites
- Rust/Cargo
- Go 1.22+
- Node.js/pnpm
- macOS

### Normal dev mode: run the signaling server
```bash
cd server
go build -o relay-server .
./relay-server
```

Server flags:
- `--addr` — listen address (default: `:8080`)
- `--max-sessions` — max concurrent sessions (default: 1000)
- `--session-ttl` — session expiration (default: 10m)
- `--relay-rate-limit` — relay bandwidth limit in bytes/sec (default: 10 MB/s)

The server also reads matching environment variables for containerized deployments:
`RELAY_ADDR`, `RELAY_MAX_SESSIONS`, `RELAY_SESSION_TTL`, and `RELAY_RATE_LIMIT`.

### Normal dev mode: run the client
```bash
cd client
pnpm install
pnpm tauri dev
```

### Lean dev mode (low disk)
Use this when you want to keep local disk usage low while still running the app with the normal dev command flow.

```bash
./scripts/dev-lean.sh
```

What lean mode does:
- Starts the server and client using the same documented tools (`go build`, `pnpm tauri dev`).
- Redirects heavy build caches to a temporary directory for the current run.
- Cleans heavy build artifacts automatically when you stop the app.

Tradeoff:
- Lower persistent disk usage.
- Slightly slower startup than normal dev, because temporary build caches are rebuilt each run.

### Cleanup commands
Targeted cleanup (heavy build artifacts only, keeps dependencies):

```bash
./scripts/cleanup-heavy.sh
```

Full local reproducible cleanup (includes dependency and language build/test caches):

```bash
./scripts/cleanup-full.sh
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

⚠️ **macOS release-candidate**

**Completed Phases:**
- Phase 1: Direct QUIC transfers ✓
- Phase 2: Signaling + key exchange ✓
- Phase 3: Relay fallback + folders ✓
- Phase 4: Local launch + verification hardening in progress

**Current verified test count: 54 passing** (31 Rust unit + 5 integration + 18 Go)

## Documentation

- [Architecture](docs/ARCHITECTURE.md) — System design, data flow, module responsibilities
- [Security Model](docs/SECURITY.md) — Cryptography, threat model, vulnerability disclosure
- [Execution Contract](docs/EXECUTION_CONTRACT.md) — Deterministic completion and verification rules
- [Release Gates](docs/RELEASE_GATES.md) — Gate definitions (`G0`-`G6`) and promotion policy
- [Release Readiness Hub](docs/release/README.md) — Channel policy, rollback, and canary decision templates
- [Local macOS Smoke Checklist](docs/LOCAL_SMOKE.md) — Human validation flow for launch-critical behavior
- [Self-Hosting Guide](docs/SELF_HOSTING.md) — Deploy your own signaling server
- [Troubleshooting](docs/TROUBLESHOOTING.md) — Common issues and debugging

## Keyboard Shortcuts

- **Cmd+O / Ctrl+O** — Open file picker
- **Cmd+V / Ctrl+V** — Paste transfer code from clipboard
- **Escape** — Cancel transfer or go back
- **Cmd+, / Ctrl+,** — Toggle settings

## Deployment

### Self-Hosting (Docker)

```bash
docker run -d -p 8080:8080 \
  -e RELAY_MAX_SESSIONS=5000 \
  ghcr.io/your-org/relay-server:latest
```

See [Self-Hosting Guide](docs/SELF_HOSTING.md) for full details.

### CI/CD

GitHub Actions automatically:
- Runs tests on every push
- Enforces hard-fail vulnerability scanning on supported branches
- Builds release artifacts for the supported desktop targets in scope
- Publishes Docker images on release tags

## Contributing

Contributions welcome! Please:
1. Search [existing issues](https://github.com/your-org/relay/issues)
2. Open a PR with tests
3. Follow existing code style

## License

MIT
