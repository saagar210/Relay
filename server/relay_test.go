package main

import (
	"strings"
	"testing"
	"time"

	"github.com/gorilla/websocket"
)

func TestRateLimiter(t *testing.T) {
	// 1 MB/s rate limit
	limiter := NewRateLimiter(1024 * 1024)

	// Small request should complete instantly
	start := time.Now()
	limiter.Wait(1024) // 1KB
	elapsed := time.Since(start)
	if elapsed > 100*time.Millisecond {
		t.Errorf("small request took too long: %v", elapsed)
	}

	// Drain the bucket
	limiter.Wait(2 * 1024 * 1024) // drain the 2MB bucket

	// Next request should be rate-limited
	start = time.Now()
	limiter.Wait(512 * 1024) // 512KB should take ~0.5s at 1MB/s
	elapsed = time.Since(start)
	if elapsed < 200*time.Millisecond {
		t.Errorf("expected rate limiting, but request completed in %v", elapsed)
	}
}

func TestRelayRequest(t *testing.T) {
	_, ts := newTestServer(t, 100, 10*time.Minute)
	defer ts.Close()

	sender := dialWS(t, ts, "relay-test")
	defer sender.Close()
	receiver := dialWS(t, ts, "relay-test")
	defer receiver.Close()

	register(sender, "sender")
	register(receiver, "receiver")

	// Drain peer_joined
	readMsg(t, sender)
	readMsg(t, receiver)

	// Sender requests relay
	if err := sender.WriteJSON(SignalMessage{Type: "relay_request"}); err != nil {
		t.Fatalf("send relay_request failed: %v", err)
	}

	// Receiver should get the relay_request forwarded
	msg := readMsg(t, receiver)
	if msg.Type != "relay_request" {
		t.Errorf("expected relay_request, got %s", msg.Type)
	}

	// Receiver also requests relay
	if err := receiver.WriteJSON(SignalMessage{Type: "relay_request"}); err != nil {
		t.Fatalf("recv relay_request failed: %v", err)
	}

	// Both should get relay_active
	sMsg := readMsg(t, sender)
	if sMsg.Type != "relay_active" {
		t.Errorf("sender expected relay_active, got %s", sMsg.Type)
	}

	rMsg := readMsg(t, receiver)
	if rMsg.Type != "relay_active" {
		t.Errorf("receiver expected relay_active, got %s", rMsg.Type)
	}

	// Both send relay_ready to acknowledge
	sender.WriteJSON(SignalMessage{Type: "relay_ready"})
	receiver.WriteJSON(SignalMessage{Type: "relay_ready"})
}

func TestRelayBinaryForwarding(t *testing.T) {
	_, ts := newTestServer(t, 100, 10*time.Minute)
	defer ts.Close()

	sender := dialWS(t, ts, "binary-relay-test")
	defer sender.Close()
	receiver := dialWS(t, ts, "binary-relay-test")
	defer receiver.Close()

	register(sender, "sender")
	register(receiver, "receiver")

	// Drain peer_joined
	readMsg(t, sender)
	readMsg(t, receiver)

	// Both request relay
	sender.WriteJSON(SignalMessage{Type: "relay_request"})
	readMsg(t, receiver) // drain forwarded relay_request

	receiver.WriteJSON(SignalMessage{Type: "relay_request"})
	readMsg(t, sender)   // drain relay_active
	readMsg(t, receiver) // drain relay_active

	// Both acknowledge with relay_ready
	sender.WriteJSON(SignalMessage{Type: "relay_ready"})
	receiver.WriteJSON(SignalMessage{Type: "relay_ready"})

	// Give the server time to start the relay loop
	time.Sleep(100 * time.Millisecond)

	// Now in relay mode — send binary data
	t.Log("sending binary data from sender")
	testData := []byte("hello from sender via relay")
	if err := sender.WriteMessage(websocket.BinaryMessage, testData); err != nil {
		t.Fatalf("send binary failed: %v", err)
	}

	t.Log("reading binary data from receiver")
	// Receiver should get the binary message
	msgType, data, err := receiver.ReadMessage()
	if err != nil {
		t.Fatalf("recv binary failed: %v", err)
	}
	if msgType != websocket.BinaryMessage {
		t.Errorf("expected binary message, got type %d", msgType)
	}
	if string(data) != "hello from sender via relay" {
		t.Errorf("data mismatch: got %q", string(data))
	}

	// Reverse direction: receiver → sender
	testData2 := []byte("hello from receiver via relay")
	if err := receiver.WriteMessage(websocket.BinaryMessage, testData2); err != nil {
		t.Fatalf("send binary (reverse) failed: %v", err)
	}

	msgType2, data2, err := sender.ReadMessage()
	if err != nil {
		t.Fatalf("recv binary (reverse) failed: %v", err)
	}
	if msgType2 != websocket.BinaryMessage {
		t.Errorf("expected binary message, got type %d", msgType2)
	}
	if string(data2) != "hello from receiver via relay" {
		t.Errorf("data mismatch: got %q", string(data2))
	}
}

// dialWS for relay tests needs the standard test helper (already defined in handler_test.go)
// These tests use the shared newTestServer/dialWS/register/readMsg helpers.

// Verify we haven't broken the test helper
func TestNewTestServerHasRelay(t *testing.T) {
	srv, ts := newTestServer(t, 100, 10*time.Minute)
	defer ts.Close()

	if srv.relayLimiter == nil {
		t.Fatal("expected relayLimiter to be set")
	}

	// Health check still works
	wsURL := "ws" + strings.TrimPrefix(ts.URL, "http") + "/ws/health-check"
	conn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("dial failed: %v", err)
	}
	conn.Close()
}
