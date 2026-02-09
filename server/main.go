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
	flag.Parse()

	srv := NewServer(*maxSessions, *sessionTTL)

	go srv.CleanupLoop(60 * time.Second)

	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", srv.HealthHandler)
	mux.HandleFunc("GET /ws/{code}", srv.WebSocketHandler)

	log.Printf("Relay signaling server starting on %s (max-sessions=%d, session-ttl=%s)",
		*addr, *maxSessions, *sessionTTL)

	if err := http.ListenAndServe(*addr, mux); err != nil {
		log.Fatalf("server failed: %v", err)
	}
}
