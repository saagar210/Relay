package main

import (
	"strings"
	"testing"
	"time"
)

func TestLoadConfigFromEnvDefaults(t *testing.T) {
	t.Setenv("RELAY_ADDR", "")
	t.Setenv("RELAY_MAX_SESSIONS", "")
	t.Setenv("RELAY_SESSION_TTL", "")
	t.Setenv("RELAY_RATE_LIMIT", "")

	cfg, err := loadConfigFromEnv()
	if err != nil {
		t.Fatalf("expected default config, got error: %v", err)
	}

	if cfg.Addr != ":8080" {
		t.Fatalf("expected default addr, got %q", cfg.Addr)
	}
	if cfg.MaxSessions != 1000 {
		t.Fatalf("expected default max sessions, got %d", cfg.MaxSessions)
	}
	if cfg.SessionTTL != 10*time.Minute {
		t.Fatalf("expected default ttl, got %s", cfg.SessionTTL)
	}
	if cfg.RelayRateLimit != 10*1024*1024 {
		t.Fatalf("expected default relay limit, got %d", cfg.RelayRateLimit)
	}
}

func TestLoadConfigFromEnvOverrides(t *testing.T) {
	t.Setenv("RELAY_ADDR", "127.0.0.1:9090")
	t.Setenv("RELAY_MAX_SESSIONS", "25")
	t.Setenv("RELAY_SESSION_TTL", "45m")
	t.Setenv("RELAY_RATE_LIMIT", "2048")

	cfg, err := loadConfigFromEnv()
	if err != nil {
		t.Fatalf("expected env overrides, got error: %v", err)
	}

	if cfg.Addr != "127.0.0.1:9090" {
		t.Fatalf("expected env addr, got %q", cfg.Addr)
	}
	if cfg.MaxSessions != 25 {
		t.Fatalf("expected env max sessions, got %d", cfg.MaxSessions)
	}
	if cfg.SessionTTL != 45*time.Minute {
		t.Fatalf("expected env ttl, got %s", cfg.SessionTTL)
	}
	if cfg.RelayRateLimit != 2048 {
		t.Fatalf("expected env relay limit, got %d", cfg.RelayRateLimit)
	}
}

func TestLoadConfigFromEnvRejectsInvalidValues(t *testing.T) {
	t.Setenv("RELAY_MAX_SESSIONS", "abc")

	_, err := loadConfigFromEnv()
	if err == nil {
		t.Fatal("expected invalid env error, got nil")
	}
	if !strings.Contains(err.Error(), "RELAY_MAX_SESSIONS") {
		t.Fatalf("expected env name in error, got %v", err)
	}
}
