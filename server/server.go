package main

import (
	"fmt"
	"log"
	"sync"
	"time"
)

// Server manages signaling sessions between peers.
type Server struct {
	sessions     map[string]*Session
	mu           sync.RWMutex
	maxSessions  int
	sessionTTL   time.Duration
	relayLimiter *RateLimiter
}

// NewServer creates a Server with the given capacity, TTL, and relay rate limit.
func NewServer(maxSessions int, sessionTTL time.Duration, relayRateLimit int64) *Server {
	return &Server{
		sessions:     make(map[string]*Session),
		maxSessions:  maxSessions,
		sessionTTL:   sessionTTL,
		relayLimiter: NewRateLimiter(relayRateLimit),
	}
}

// GetOrCreateSession returns the session for code, creating one if needed.
// Returns an error if the requested role is already taken.
func (s *Server) GetOrCreateSession(code string, role string) (*Session, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	sess, exists := s.sessions[code]
	if exists {
		sess.mu.Lock()
		defer sess.mu.Unlock()

		if role == "sender" && sess.Sender != nil {
			return nil, fmt.Errorf("sender already connected for code %s", code)
		}
		if role == "receiver" && sess.Receiver != nil {
			return nil, fmt.Errorf("receiver already connected for code %s", code)
		}
		return sess, nil
	}

	// Check capacity before creating a new session.
	if len(s.sessions) >= s.maxSessions {
		return nil, fmt.Errorf("max sessions reached (%d)", s.maxSessions)
	}

	now := time.Now()
	sess = &Session{
		Code:      code,
		CreatedAt: now,
		ExpiresAt: now.Add(s.sessionTTL),
		relayDone: make(chan struct{}),
	}
	s.sessions[code] = sess
	return sess, nil
}

// RemoveSession deletes a session by code.
func (s *Server) RemoveSession(code string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.sessions, code)
}

// SessionCount returns the number of active sessions.
func (s *Server) SessionCount() int {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return len(s.sessions)
}

// CleanupLoop periodically removes expired sessions.
func (s *Server) CleanupLoop(interval time.Duration) {
	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	for range ticker.C {
		s.cleanupExpired()
	}
}

func (s *Server) cleanupExpired() {
	now := time.Now()
	s.mu.Lock()
	defer s.mu.Unlock()

	for code, sess := range s.sessions {
		if now.After(sess.ExpiresAt) {
			log.Printf("cleaning up expired session %s", code)
			sess.Close()
			delete(s.sessions, code)
		}
	}
}
