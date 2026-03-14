# Compatibility Policy

## Current Policy

- Peer protocol compatibility: `N-1 minor (same major)`.
- Current protocol constant: `client/src-tauri/src/protocol/version.rs`.
- Legacy peers without explicit version metadata are treated as legacy-compatible.

## Change Rules

1. New protocol fields must be additive and optional first.
2. Any breaking protocol behavior needs a deprecation window and migration note.
3. Deprecation windows should remain active for at least one stable release cycle.

## Validation Requirements

- Unit coverage for compatibility checks.
- Integration coverage for mixed-version sender/receiver scenarios.
- Release notes must include compatibility-impact callout for protocol-affecting changes.
