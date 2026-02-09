package main

import (
	"flag"
	"log"
	"net/http"
	"time"
)

func main() {
	addr := flag.String("addr", ":8080", "listen address")
	maxSessions := flag.Int("max-sessions", 1000, "maximum concurrent sessions")
	sessionTTL := flag.Duration("session-ttl", 10*time.Minute, "session time-to-live")
	relayRateLimit := flag.Int64("relay-rate-limit", 10*1024*1024, "relay rate limit in bytes/sec (default 10 MB/s)")
	flag.Parse()

	srv := NewServer(*maxSessions, *sessionTTL, *relayRateLimit)

	go srv.CleanupLoop(60 * time.Second)

	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", srv.HealthHandler)
	mux.HandleFunc("GET /ws/{code}", srv.WebSocketHandler)

	log.Printf("Relay signaling server starting on %s (max-sessions=%d, session-ttl=%s, relay-rate-limit=%d B/s)",
		*addr, *maxSessions, *sessionTTL, *relayRateLimit)

	if err := http.ListenAndServe(*addr, mux); err != nil {
		log.Fatalf("server failed: %v", err)
	}
}
