package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestHealthEndpoint(t *testing.T) {
	srv := NewServer(100, 10*time.Minute)

	req := httptest.NewRequest(http.MethodGet, "/health", nil)
	w := httptest.NewRecorder()
	srv.HealthHandler(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", w.Code)
	}

	var resp HealthResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}
	if resp.Status != "ok" {
		t.Errorf("expected status ok, got %s", resp.Status)
	}
	if resp.ActiveSessions != 0 {
		t.Errorf("expected 0 active sessions, got %d", resp.ActiveSessions)
	}
}

func TestSessionCreation(t *testing.T) {
	srv := NewServer(100, 10*time.Minute)

	sess, err := srv.GetOrCreateSession("abc123", "sender")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if sess.Code != "abc123" {
		t.Errorf("expected code abc123, got %s", sess.Code)
	}
	if srv.SessionCount() != 1 {
		t.Errorf("expected 1 session, got %d", srv.SessionCount())
	}
}

func TestSessionExpiry(t *testing.T) {
	// Use a very short TTL so sessions expire immediately.
	srv := NewServer(100, 1*time.Millisecond)

	_, err := srv.GetOrCreateSession("expire-me", "sender")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Wait for the session to expire.
	time.Sleep(10 * time.Millisecond)
	srv.cleanupExpired()

	if srv.SessionCount() != 0 {
		t.Errorf("expected 0 sessions after cleanup, got %d", srv.SessionCount())
	}
}

func TestMaxSessions(t *testing.T) {
	srv := NewServer(2, 10*time.Minute)

	_, err := srv.GetOrCreateSession("s1", "sender")
	if err != nil {
		t.Fatalf("unexpected error creating session 1: %v", err)
	}
	_, err = srv.GetOrCreateSession("s2", "sender")
	if err != nil {
		t.Fatalf("unexpected error creating session 2: %v", err)
	}

	// Third session should fail.
	_, err = srv.GetOrCreateSession("s3", "sender")
	if err == nil {
		t.Fatal("expected error when exceeding max sessions, got nil")
	}
}
