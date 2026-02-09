package main

import (
	"sync"
	"time"

	"github.com/gorilla/websocket"
)

// Session represents a signaling session between two peers.
type Session struct {
	Code      string
	Sender    *Peer
	Receiver  *Peer
	CreatedAt time.Time
	ExpiresAt time.Time
	mu        sync.Mutex
}

// Peer represents one side of a signaling session.
type Peer struct {
	Conn    *websocket.Conn
	Role    string
	Info    *PeerInfo // from register message (local_ip, local_port)
	Done    chan struct{}
	writeMu sync.Mutex
}

// WriteJSON sends a JSON message to the peer, safe for concurrent use.
func (p *Peer) WriteJSON(v interface{}) error {
	p.writeMu.Lock()
	defer p.writeMu.Unlock()
	return p.Conn.WriteJSON(v)
}

// BothConnected returns true when sender and receiver are both present.
func (s *Session) BothConnected() bool {
	return s.Sender != nil && s.Receiver != nil
}

// OtherPeer returns the peer that is not p, or nil.
func (s *Session) OtherPeer(p *Peer) *Peer {
	if p == s.Sender {
		return s.Receiver
	}
	return s.Sender
}

// Close gracefully closes both WebSocket connections and signals done.
func (s *Session) Close() {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.Sender != nil {
		s.Sender.Close()
		s.Sender = nil
	}
	if s.Receiver != nil {
		s.Receiver.Close()
		s.Receiver = nil
	}
}

// Close gracefully closes the peer's WebSocket connection.
func (p *Peer) Close() {
	if p.Conn != nil {
		p.writeMu.Lock()
		p.Conn.WriteMessage(
			websocket.CloseMessage,
			websocket.FormatCloseMessage(websocket.CloseNormalClosure, ""),
		)
		p.writeMu.Unlock()
		p.Conn.Close()
	}
	select {
	case <-p.Done:
		// already closed
	default:
		close(p.Done)
	}
}
