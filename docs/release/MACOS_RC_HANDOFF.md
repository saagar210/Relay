# macOS Release-Candidate Handoff

This note captures the scoped work completed for local macOS release-candidate readiness in the current branch.

## Current state

- Local server launch: verified
- Local Tauri launch: verified
- Packaged debug app and DMG: verified
- Repo verification contract: passed locally via `.codex/scripts/run_verify_commands.sh`
- Remaining gaps: external or manual-release-facing, not repo-local correctness blockers

## Scoped changes from this readiness pass

### 1. Server configuration now matches self-hosting docs

Files:
- `server/config.go`
- `server/config_test.go`
- `server/main.go`
- `docs/SELF_HOSTING.md`
- `README.md`

Outcome:
- `relay-server` now reads `RELAY_ADDR`, `RELAY_MAX_SESSIONS`, `RELAY_SESSION_TTL`, and `RELAY_RATE_LIMIT`
- invalid env values fail fast with clear errors
- CLI flags still override env vars
- self-hosting docs now match actual runtime behavior

### 2. Rust supply-chain blocker cleared

Files:
- `client/src-tauri/Cargo.lock`

Outcome:
- patched `quinn-proto` from `0.11.13` to `0.11.14`
- `cargo audit --deny warnings` now passes

Lockfile rationale:
- this is a targeted security remediation to clear a failing verification gate

### 3. Tauri packaging cleanup

Files:
- `client/src-tauri/tauri.conf.json`

Outcome:
- bundle identifier changed from `com.relay.app` to `com.relay.desktop`
- local debug bundle rebuild succeeded afterward

### 4. Release/docs parity cleanup

Files:
- `README.md`
- `docs/ARCHITECTURE.md`
- `docs/RELEASE_GATES.md`
- `docs/EXECUTION_CONTRACT.md`
- `docs/LOCAL_SMOKE.md`
- `docs/release/README.md`
- `.codex/verify.commands`

Outcome:
- docs now describe the current frontend stack and macOS-first scope more truthfully
- local manual smoke validation is now explicitly documented
- the smoke checklist is part of the repo’s release/readiness contract

## Verification summary

Commands run successfully in this readiness pass:

```bash
bash scripts/git/guard-branch.sh
cd server && go test -race ./...
cd client && pnpm install --frozen-lockfile
cd client && pnpm typecheck
cd client && pnpm build
cd client/src-tauri && cargo test --lib --bins
cd client/src-tauri && cargo test --test signaling_e2e -- --test-threads=1
cd client/src-tauri && cargo audit --deny warnings
cd client && pnpm tauri build --debug
.codex/scripts/run_verify_commands.sh
```

Additional notes:
- one intermediate verification rerun briefly tripped the build-time budget, but repeated timings returned to normal and the final full-contract rerun passed
- `pnpm audit --audit-level high` reported `2 low | 1 moderate`, which is below the repo’s blocking threshold

## Remaining external blockers

These are not fully solvable inside this session alone:

1. Real manual two-peer smoke
   - especially relay fallback with real operator-facing UI evidence
   - use `docs/LOCAL_SMOKE.md`

2. Apple signing and notarization
   - still requires release credentials and signing inputs

3. Real release evidence
   - `.codex/evidence/g3` and `.codex/evidence/g5` are still template-level unless populated by an actual release process

## Safe staging plan

This repository already contains many unrelated modified and untracked files.
Stage only the files below if you want just this readiness work:

```text
.codex/verify.commands
README.md
client/src-tauri/Cargo.lock
client/src-tauri/tauri.conf.json
docs/ARCHITECTURE.md
docs/EXECUTION_CONTRACT.md
docs/LOCAL_SMOKE.md
docs/RELEASE_GATES.md
docs/SELF_HOSTING.md
docs/release/MACOS_RC_HANDOFF.md
docs/release/README.md
server/config.go
server/config_test.go
server/main.go
```

## Recommended commit plan

Use small atomic commits by concern:

1. `fix(server): honor documented relay env configuration`
   - `server/config.go`
   - `server/config_test.go`
   - `server/main.go`
   - `docs/SELF_HOSTING.md`
   - relevant `README.md` lines if you want env docs grouped here

2. `build(tauri): use valid macOS bundle identifier`
   - `client/src-tauri/tauri.conf.json`

3. `build(rust): patch quinn-proto security advisory`
   - `client/src-tauri/Cargo.lock`

4. `docs(release): align macOS readiness docs and smoke workflow`
   - `README.md`
   - `docs/ARCHITECTURE.md`
   - `docs/EXECUTION_CONTRACT.md`
   - `docs/LOCAL_SMOKE.md`
   - `docs/RELEASE_GATES.md`
   - `docs/release/README.md`
   - `docs/release/MACOS_RC_HANDOFF.md`
   - `.codex/verify.commands`

## Suggested PR title

`fix(release): align Relay with local macOS readiness`

## Suggested PR body

```md
## What
- align the local macOS release-candidate path across server config, packaging, and docs
- add a repo-local macOS smoke checklist for operator validation
- clear the Rust advisory blocking the verification contract

## Why
- the repo launched and tested locally, but several release-facing details were out of sync with actual behavior
- self-hosting docs relied on env vars the server did not read yet
- Tauri packaging used a poor bundle identifier and Rust verification was blocked by a known advisory
- the repo had strong automated checks but no explicit local smoke checklist for the human release pass

## How
- load relay server defaults from documented `RELAY_*` env vars and keep CLI flags authoritative
- update the Tauri bundle identifier to a valid desktop-oriented value
- refresh `Cargo.lock` to pull in patched `quinn-proto 0.11.14`
- wire `docs/LOCAL_SMOKE.md` into the release/docs verification path
- reconcile README and release docs with the current macOS-first scope and Solid-based frontend

## Testing
- `bash scripts/git/guard-branch.sh`
- `cd server && go test -race ./...`
- `cd client && pnpm install --frozen-lockfile`
- `cd client && pnpm typecheck`
- `cd client && pnpm build`
- `cd client/src-tauri && cargo test --lib --bins`
- `cd client/src-tauri && cargo test --test signaling_e2e -- --test-threads=1`
- `cd client/src-tauri && cargo audit --deny warnings`
- `cd client && pnpm tauri build --debug`
- `.codex/scripts/run_verify_commands.sh`

## Performance impact
- no bundle-size regression observed
- final build-time gate passed locally after rerun
- no runtime performance changes intentionally introduced

## Risk / Notes
- this repo already has unrelated modified/untracked files; review should focus only on the scoped files in this PR
- manual two-peer UI smoke and Apple signing/notarization still require external follow-through
- lockfile changed only to remediate the Rust advisory and unblock `cargo audit`
```
