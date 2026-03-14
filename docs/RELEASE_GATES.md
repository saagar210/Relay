# Release Gates (G0-G6)

This document defines the minimum release gates for Relay and maps them to concrete checks.

## Gate model

- `G0 Hygiene`: repository hygiene and contract files are valid.
- `G1 Fast Correctness`: fast unit/integration correctness checks are green.
- `G2 Integration/E2E`: critical end-to-end transfer path passes.
- `G3 Security/Supply Chain`: dependency, signing, and secret hygiene checks pass.
- `G4 Perf/Reliability`: performance and reliability thresholds are respected.
- `G5 Release Promotion`: canary and rollback drill approvals are complete.
- `G6 Docs Closeout`: architecture/security/release docs are in sync with shipped behavior.

## Current enforcement map

- `.codex/verify.commands` enforces deterministic local and CI checks for `G0` through `G6`.
- `docs/LOCAL_SMOKE.md` defines the human macOS smoke pass that complements automated checks before a release-ready claim.
- `.codex/config/operational-gates.json` defines protocol policy, gates, and thresholds validated in CI.
- `.codex/config/release-materials.required.json` defines required signing/release env inputs for enforced release runs.
- `client/src-tauri/release-channels.json` defines release rings (`internal`, `beta`, `stable`) validated in CI.
- `scripts/ci/validate-operational-config.mjs` validates both config files for schema and policy correctness.
- GitHub Actions workflows enforce build/test checks against `master` and `main` (transitional compatibility).
- CI vulnerability scanning is hard-fail via `govulncheck`, `cargo audit`, and `pnpm audit`.
- `scripts/ci/check-release-evidence.mjs` enforces G3/G5 evidence in enforced mode and template presence pre-credentials.
- `scripts/ci/check-release-materials.mjs` enforces required release materials in release preflight.
- Detailed release governance templates live in `docs/release/`.

## Promotion policy

Promotion from `internal` -> `beta` -> `stable` is blocked when any gate is red or not run.

## Ownership

- Engineering: `G0`, `G1`, `G2`
- Security: `G3`
- Reliability: `G4`
- Release manager: `G5`
- PM + engineering: `G6`
