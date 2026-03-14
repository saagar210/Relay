package main

import (
	"flag"
	"log"
	"net/http"
	"time"
)

func main() {
	cfg, err := loadConfigFromEnv()
	if err != nil {
		log.Fatalf("server configuration error: %v", err)
	}

	addr := flag.String("addr", cfg.Addr, "listen address")
	maxSessions := flag.Int("max-sessions", cfg.MaxSessions, "maximum concurrent sessions")
	sessionTTL := flag.Duration("session-ttl", cfg.SessionTTL, "session time-to-live")
	relayRateLimit := flag.Int64("relay-rate-limit", cfg.RelayRateLimit, "relay rate limit in bytes/sec (default 10 MB/s)")
	flag.Parse()

	srv := NewServer(*maxSessions, *sessionTTL, *relayRateLimit)

	go srv.CleanupLoop(60 * time.Second)

	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", srv.HealthHandler)
	// Transitional compatibility path used by existing perf probes and legacy integrations.
	mux.HandleFunc("GET /api/health", srv.HealthHandler)
	mux.HandleFunc("GET /ws/{code}", srv.WebSocketHandler)

	log.Printf("Relay signaling server starting on %s (max-sessions=%d, session-ttl=%s, relay-rate-limit=%d B/s)",
		*addr, *maxSessions, *sessionTTL, *relayRateLimit)

	if err := http.ListenAndServe(*addr, mux); err != nil {
		log.Fatalf("server failed: %v", err)
	}
}
