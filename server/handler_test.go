package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gorilla/websocket"
)

// newTestServer creates a test HTTP server with the WebSocket handler wired up.
func newTestServer(t *testing.T, maxSessions int, ttl time.Duration) (*Server, *httptest.Server) {
	t.Helper()
	srv := NewServer(maxSessions, ttl, 10*1024*1024)
	mux := http.NewServeMux()
	mux.HandleFunc("GET /ws/{code}", srv.WebSocketHandler)
	mux.HandleFunc("GET /health", srv.HealthHandler)
	ts := httptest.NewServer(mux)
	return srv, ts
}

// dialWS opens a WebSocket to the test server for the given code.
func dialWS(t *testing.T, ts *httptest.Server, code string) *websocket.Conn {
	t.Helper()
	wsURL := "ws" + strings.TrimPrefix(ts.URL, "http") + "/ws/" + code
	conn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("dial failed: %v", err)
	}
	return conn
}

// register sends a register message and returns any error.
func register(conn *websocket.Conn, role string) error {
	return conn.WriteJSON(SignalMessage{Type: "register", Role: role})
}

// readMsg reads the next SignalMessage from the connection.
func readMsg(t *testing.T, conn *websocket.Conn) SignalMessage {
	t.Helper()
	var msg SignalMessage
	if err := conn.ReadJSON(&msg); err != nil {
		t.Fatalf("readMsg failed: %v", err)
	}
	return msg
}

func TestWebSocketHandshake(t *testing.T) {
	_, ts := newTestServer(t, 100, 10*time.Minute)
	defer ts.Close()

	sender := dialWS(t, ts, "handshake-test")
	defer sender.Close()

	receiver := dialWS(t, ts, "handshake-test")
	defer receiver.Close()

	if err := register(sender, "sender"); err != nil {
		t.Fatalf("sender register failed: %v", err)
	}

	if err := register(receiver, "receiver"); err != nil {
		t.Fatalf("receiver register failed: %v", err)
	}

	// Both should receive peer_joined.
	senderMsg := readMsg(t, sender)
	if senderMsg.Type != "peer_joined" {
		t.Errorf("sender expected peer_joined, got %s", senderMsg.Type)
	}
	if senderMsg.PeerInfo == nil {
		t.Error("sender peer_joined missing peer_info")
	}

	receiverMsg := readMsg(t, receiver)
	if receiverMsg.Type != "peer_joined" {
		t.Errorf("receiver expected peer_joined, got %s", receiverMsg.Type)
	}
	if receiverMsg.PeerInfo == nil {
		t.Error("receiver peer_joined missing peer_info")
	}
}

func TestSPAKE2Forwarding(t *testing.T) {
	_, ts := newTestServer(t, 100, 10*time.Minute)
	defer ts.Close()

	sender := dialWS(t, ts, "forward-test")
	defer sender.Close()
	receiver := dialWS(t, ts, "forward-test")
	defer receiver.Close()

	register(sender, "sender")
	register(receiver, "receiver")

	// Drain peer_joined messages.
	readMsg(t, sender)
	readMsg(t, receiver)

	// Sender sends a spake2 message.
	payload := json.RawMessage(`{"key":"test-value"}`)
	if err := sender.WriteJSON(SignalMessage{Type: "spake2", Payload: payload}); err != nil {
		t.Fatalf("send spake2 failed: %v", err)
	}

	// Receiver should get it.
	msg := readMsg(t, receiver)
	if msg.Type != "spake2" {
		t.Errorf("expected spake2, got %s", msg.Type)
	}

	var p map[string]string
	if err := json.Unmarshal(msg.Payload, &p); err != nil {
		t.Fatalf("unmarshal payload: %v", err)
	}
	if p["key"] != "test-value" {
		t.Errorf("expected payload key=test-value, got %s", p["key"])
	}
}

func TestDuplicateCode(t *testing.T) {
	_, ts := newTestServer(t, 100, 10*time.Minute)
	defer ts.Close()

	c1 := dialWS(t, ts, "dup-test")
	defer c1.Close()
	c2 := dialWS(t, ts, "dup-test")
	defer c2.Close()

	register(c1, "sender")
	register(c2, "receiver")

	// Drain peer_joined.
	readMsg(t, c1)
	readMsg(t, c2)

	// Third client tries to join as sender (already taken).
	c3 := dialWS(t, ts, "dup-test")
	defer c3.Close()
	register(c3, "sender")

	msg := readMsg(t, c3)
	if msg.Type != "error" {
		t.Errorf("expected error, got %s", msg.Type)
	}
	if msg.Code != "CODE_IN_USE" {
		t.Errorf("expected CODE_IN_USE, got %s", msg.Code)
	}
}

func TestDisconnect(t *testing.T) {
	srv, ts := newTestServer(t, 100, 10*time.Minute)
	defer ts.Close()

	sender := dialWS(t, ts, "disconnect-test")
	defer sender.Close()
	receiver := dialWS(t, ts, "disconnect-test")
	defer receiver.Close()

	register(sender, "sender")
	register(receiver, "receiver")

	// Drain peer_joined.
	readMsg(t, sender)
	readMsg(t, receiver)

	// Sender sends disconnect.
	if err := sender.WriteJSON(SignalMessage{Type: "disconnect"}); err != nil {
		t.Fatalf("send disconnect failed: %v", err)
	}

	// Receiver should get peer_disconnected.
	msg := readMsg(t, receiver)
	if msg.Type != "peer_disconnected" {
		t.Errorf("expected peer_disconnected, got %s", msg.Type)
	}

	// Give cleanup a moment.
	time.Sleep(50 * time.Millisecond)

	// Close receiver too so the session is fully cleaned up.
	receiver.Close()
	time.Sleep(50 * time.Millisecond)

	if srv.SessionCount() != 0 {
		t.Errorf("expected 0 sessions after disconnect, got %d", srv.SessionCount())
	}
}
