# Self-Hosting Relay Server

## Quick Start

### Docker (Recommended)

```bash
docker run -d \
  --name relay-server \
  -p 8080:8080 \
  -e RELAY_ADDR=":8080" \
  -e RELAY_MAX_SESSIONS=1000 \
  -e RELAY_SESSION_TTL=10m \
  -e RELAY_RATE_LIMIT=10485760 \
  ghcr.io/your-org/relay-server:latest
```

### Docker Compose

```yaml
version: '3.8'
services:
  relay-server:
    image: ghcr.io/your-org/relay-server:latest
    ports:
      - "8080:8080"
    environment:
      - RELAY_ADDR=:8080
      - RELAY_MAX_SESSIONS=1000
      - RELAY_SESSION_TTL=10m
      - RELAY_RATE_LIMIT=10485760
    restart: unless-stopped
```

### Fly.io (Global Edge)

```bash
cd server
flyctl launch
flyctl deploy
```

**Checked-in `fly.toml` example**:
```toml
app = "relay-signal" # rename this to your own Fly app if needed

[build]
  dockerfile = "Dockerfile"

[env]
  RELAY_ADDR = ":8080"
  RELAY_MAX_SESSIONS = "5000"
  RELAY_SESSION_TTL = "30m"
  RELAY_RATE_LIMIT = "52428800"  # 50 MB/s

[[services]]
  http_checks = []
  internal_port = 8080
  protocol = "tcp"

  [[services.ports]]
    handlers = ["http"]
    port = 80
    force_https = true

  [[services.ports]]
    handlers = ["tls", "http"]
    port = 443

  [services.concurrency]
    hard_limit = 5000
    soft_limit = 1000

  [[services.http_checks]]
    interval = 10000
    grace_period = "5s"
    method = "get"
    path = "/health"
    protocol = "http"
    timeout = 2000
```

### Bare Metal (Linux)

```bash
# Build server
cd server
go build -o relay-server .

# Run with systemd
sudo nano /etc/systemd/system/relay-server.service
```

**`relay-server.service`**:
```ini
[Unit]
Description=Relay Signaling Server
After=network.target

[Service]
Type=simple
User=relay
WorkingDirectory=/opt/relay
ExecStart=/opt/relay/relay-server \
  --addr=:8080 \
  --max-sessions=5000 \
  --session-ttl=30m \
  --relay-rate-limit=52428800
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable relay-server
sudo systemctl start relay-server
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RELAY_ADDR` | `:8080` | Listen address (e.g., `:8080`, `0.0.0.0:3000`) |
| `RELAY_MAX_SESSIONS` | `1000` | Max concurrent transfer sessions |
| `RELAY_SESSION_TTL` | `10m` | Session expiration (e.g., `10m`, `1h`) |
| `RELAY_RATE_LIMIT` | `10485760` | Relay bandwidth limit in bytes/sec (10 MB/s default) |

These values are now read directly by `relay-server`, which makes the Docker and Fly examples behave the same way as the bare command-line flow.

### Command-Line Flags

```bash
./relay-server \
  --addr=:8080 \
  --max-sessions=5000 \
  --session-ttl=30m \
  --relay-rate-limit=52428800
```

Command-line flags override environment variables when both are supplied.

## TLS/HTTPS Setup

### Using Caddy (Reverse Proxy)

```caddyfile
relay.example.com {
  reverse_proxy localhost:8080
}
```

Start Caddy:
```bash
caddy run --config Caddyfile
```

Caddy automatically provisions Let's Encrypt certificates.

### Using Nginx

```nginx
server {
  listen 443 ssl http2;
  server_name relay.example.com;

  ssl_certificate /etc/letsencrypt/live/relay.example.com/fullchain.pem;
  ssl_certificate_key /etc/letsencrypt/live/relay.example.com/privkey.pem;

  location / {
    proxy_pass http://localhost:8080;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
  }
}
```

## Monitoring

### Health Check

```bash
curl http://localhost:8080/health
# {"status":"ok","active_sessions":42}
```

### Metrics

- **Active sessions**: GET `/health` response
- **Logs**: Server logs connection events, errors, relay activity

### Prometheus (optional)

Relay server does not currently export Prometheus metrics. To add:

1. Import `github.com/prometheus/client_golang/prometheus`
2. Expose `/metrics` endpoint
3. Track: active_sessions, relay_bytes_total, session_ttl_expired_total

## Scaling

### Horizontal Scaling

- Deploy multiple server instances behind a load balancer
- Use DNS round-robin for client distribution
- **Note**: Sessions are in-memory (not shared across instances)

### Vertical Scaling

- Increase `--max-sessions` for higher concurrency
- Increase `--relay-rate-limit` for higher bandwidth
- Monitor memory usage (each session uses ~1-2 KB)

## Security

- **Firewall**: Restrict to ports 80/443 only
- **Rate Limiting**: Tune `--relay-rate-limit` to prevent abuse
- **Session TTL**: Lower `--session-ttl` for tighter security (default: 10 min)
- **Updates**: Regularly update Go dependencies (`go get -u ./...`)

## Troubleshooting

### Server won't start

```bash
# Check if port is in use
sudo lsof -i :8080

# Run with verbose logging
./relay-server --addr=:8080 2>&1 | tee relay.log
```

### High memory usage

- Check active sessions: `curl http://localhost:8080/health`
- Reduce `--max-sessions` or `--session-ttl`

### WebSocket connection refused

- Verify firewall allows ports 80/443
- Check reverse proxy config (Caddy/Nginx)
- Ensure WebSocket upgrade headers are proxied

## Client Configuration

Point clients to your server:

1. Open Relay app → Settings
2. Set "Signaling Server URL" to: `wss://relay.example.com` (or `ws://localhost:8080` for testing)
3. Save and restart transfer

## Example Deployments

- **Fly.io free tier**: 1 instance, handles ~100 concurrent sessions
- **DigitalOcean $6/month droplet**: 5000 sessions, 50 MB/s relay limit
- **AWS EC2 t3.small**: 10k+ sessions with auto-scaling
