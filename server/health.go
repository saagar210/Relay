package main

import (
	"encoding/json"
	"net/http"
)

// HealthResponse is the JSON body returned by the health endpoint.
type HealthResponse struct {
	Status         string `json:"status"`
	ActiveSessions int    `json:"active_sessions"`
}

// HealthHandler responds to GET /health with server status.
func (s *Server) HealthHandler(w http.ResponseWriter, r *http.Request) {
	resp := HealthResponse{
		Status:         "ok",
		ActiveSessions: s.SessionCount(),
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(resp)
}
