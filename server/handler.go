package main

import (
	"encoding/json"
	"log"
	"net"
	"net/http"

	"github.com/gorilla/websocket"
)

// SignalMessage is the envelope for all signaling messages.
type SignalMessage struct {
	Type     string          `json:"type"`
	Role     string          `json:"role,omitempty"`
	Payload  json.RawMessage `json:"payload,omitempty"`
	Message  string          `json:"message,omitempty"`
	Code     string          `json:"code,omitempty"`
	PeerInfo *PeerInfo       `json:"peer_info,omitempty"`
}

// PeerInfo carries network information about a peer.
type PeerInfo struct {
	PublicIP   string `json:"public_ip"`
	PublicPort int    `json:"public_port"`
	LocalIP    string `json:"local_ip,omitempty"`
	LocalPort  int    `json:"local_port,omitempty"`
}

var upgrader = websocket.Upgrader{
	ReadBufferSize:  4096,
	WriteBufferSize: 4096,
	CheckOrigin:     func(r *http.Request) bool { return true },
}

// WebSocketHandler handles the /ws/{code} endpoint.
func (s *Server) WebSocketHandler(w http.ResponseWriter, r *http.Request) {
	code := r.PathValue("code")
	if code == "" {
		http.Error(w, "missing session code", http.StatusBadRequest)
		return
	}

	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("upgrade error: %v", err)
		return
	}

	// Read the register message.
	var reg SignalMessage
	if err := conn.ReadJSON(&reg); err != nil {
		log.Printf("read register error: %v", err)
		conn.Close()
		return
	}

	if reg.Type != "register" || (reg.Role != "sender" && reg.Role != "receiver") {
		sendErrorConn(conn, "INVALID_MESSAGE", "first message must be register with role sender or receiver")
		conn.Close()
		return
	}

	sess, err := s.GetOrCreateSession(code, reg.Role)
	if err != nil {
		sendErrorConn(conn, "CODE_IN_USE", err.Error())
		conn.Close()
		return
	}

	peer := &Peer{
		Conn: conn,
		Role: reg.Role,
		Info: reg.PeerInfo,
		Done: make(chan struct{}),
	}

	// Attach peer to session.
	sess.mu.Lock()
	if reg.Role == "sender" {
		sess.Sender = peer
	} else {
		sess.Receiver = peer
	}
	bothConnected := sess.BothConnected()
	sess.mu.Unlock()

	// If both peers are now connected, notify each.
	if bothConnected {
		s.notifyPeersJoined(sess)
	}

	// Message forwarding loop.
	s.forwardLoop(sess, peer, code)
}

func (s *Server) notifyPeersJoined(sess *Session) {
	sess.mu.Lock()
	sender := sess.Sender
	receiver := sess.Receiver
	sess.mu.Unlock()

	if sender == nil || receiver == nil {
		return
	}

	senderInfo := buildPeerInfo(sender)
	receiverInfo := buildPeerInfo(receiver)

	// Tell the sender about the receiver.
	_ = sender.WriteJSON(SignalMessage{
		Type:     "peer_joined",
		PeerInfo: receiverInfo,
	})

	// Tell the receiver about the sender.
	_ = receiver.WriteJSON(SignalMessage{
		Type:     "peer_joined",
		PeerInfo: senderInfo,
	})
}

// buildPeerInfo merges the peer's registered info (local_ip, local_port)
// with the detected public IP from the WebSocket connection.
func buildPeerInfo(p *Peer) *PeerInfo {
	detected := peerInfoFromConn(p.Conn)

	if p.Info == nil {
		return detected
	}

	// Use detected public IP, but keep the registered local info
	return &PeerInfo{
		PublicIP:   detected.PublicIP,
		PublicPort: p.Info.LocalPort, // QUIC port, not WebSocket port
		LocalIP:    p.Info.LocalIP,
		LocalPort:  p.Info.LocalPort,
	}
}

func (s *Server) forwardLoop(sess *Session, peer *Peer, code string) {
	defer func() {
		sess.mu.Lock()
		other := sess.OtherPeer(peer)
		if peer.Role == "sender" {
			sess.Sender = nil
		} else {
			sess.Receiver = nil
		}
		empty := sess.Sender == nil && sess.Receiver == nil
		sess.mu.Unlock()

		peer.Close()

		// Notify the other peer about the disconnect.
		if other != nil {
			_ = other.WriteJSON(SignalMessage{
				Type:    "peer_disconnected",
				Message: peer.Role + " disconnected",
			})
		}

		if empty {
			s.RemoveSession(code)
		}
	}()

	for {
		var msg SignalMessage
		if err := peer.Conn.ReadJSON(&msg); err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseNormalClosure, websocket.CloseGoingAway) {
				log.Printf("read error on session %s: %v", code, err)
			}
			return
		}

		switch msg.Type {
		case "disconnect":
			return

		case "spake2", "cert_fingerprint":
			sess.mu.Lock()
			other := sess.OtherPeer(peer)
			sess.mu.Unlock()

			if other != nil {
				if err := other.WriteJSON(msg); err != nil {
					log.Printf("forward error on session %s: %v", code, err)
					return
				}
			}

		default:
			sendError(peer, "UNKNOWN_TYPE", "unsupported message type: "+msg.Type)
		}
	}
}

func peerInfoFromConn(conn *websocket.Conn) *PeerInfo {
	addr := conn.RemoteAddr().String()
	host, port, err := net.SplitHostPort(addr)
	if err != nil {
		return &PeerInfo{PublicIP: addr}
	}
	portNum := 0
	if p, err := net.LookupPort("tcp", port); err == nil {
		portNum = p
	}
	return &PeerInfo{
		PublicIP:   host,
		PublicPort: portNum,
	}
}

func sendErrorConn(conn *websocket.Conn, code string, message string) {
	_ = conn.WriteJSON(SignalMessage{
		Type:    "error",
		Code:    code,
		Message: message,
	})
}

func sendError(p *Peer, code string, message string) {
	_ = p.WriteJSON(SignalMessage{
		Type:    "error",
		Code:    code,
		Message: message,
	})
}
