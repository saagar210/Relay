package main

import (
	"fmt"
	"os"
	"strconv"
	"time"
)

type serverConfig struct {
	Addr           string
	MaxSessions    int
	SessionTTL     time.Duration
	RelayRateLimit int64
}

func loadConfigFromEnv() (serverConfig, error) {
	cfg := serverConfig{
		Addr:           ":8080",
		MaxSessions:    1000,
		SessionTTL:     10 * time.Minute,
		RelayRateLimit: 10 * 1024 * 1024,
	}

	if value, ok := os.LookupEnv("RELAY_ADDR"); ok && value != "" {
		cfg.Addr = value
	}

	if value, ok := os.LookupEnv("RELAY_MAX_SESSIONS"); ok && value != "" {
		parsed, err := strconv.Atoi(value)
		if err != nil {
			return serverConfig{}, fmt.Errorf("invalid RELAY_MAX_SESSIONS: %w", err)
		}
		cfg.MaxSessions = parsed
	}

	if value, ok := os.LookupEnv("RELAY_SESSION_TTL"); ok && value != "" {
		parsed, err := time.ParseDuration(value)
		if err != nil {
			return serverConfig{}, fmt.Errorf("invalid RELAY_SESSION_TTL: %w", err)
		}
		cfg.SessionTTL = parsed
	}

	if value, ok := os.LookupEnv("RELAY_RATE_LIMIT"); ok && value != "" {
		parsed, err := strconv.ParseInt(value, 10, 64)
		if err != nil {
			return serverConfig{}, fmt.Errorf("invalid RELAY_RATE_LIMIT: %w", err)
		}
		cfg.RelayRateLimit = parsed
	}

	return cfg, nil
}
