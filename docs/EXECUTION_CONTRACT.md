# Execution Contract

This repository uses a deterministic verification contract for completion.

## Source of truth

- Commands: `.codex/verify.commands`
- Runner: `.codex/scripts/run_verify_commands.sh`
- Operational policy: `.codex/config/operational-gates.json`
- Release materials policy: `.codex/config/release-materials.required.json`
- Release channel rings: `client/src-tauri/release-channels.json`

Any completion claim must be backed by a successful run of the verification contract.

## Branch and scope policy

- Default operational branch is `master`.
- Transitional compatibility branch is `main` for CI trigger parity.
- Delivery happens on `codex/<type>/<slug>` branches.
- macOS is the current GA target.
- Linux completion is a post-GA milestone.

## Required completion posture

- No open P0/P1 correctness defects.
- Verification contract passes locally and in CI.
- Vulnerability scanning is hard-fail in CI.
- Release evidence checks run in all verification passes.
- Release gates are not `fail` and not `not-run`.
- Required docs are updated:
  - `docs/ARCHITECTURE.md`
  - `docs/SECURITY.md`
  - `docs/RELEASE_GATES.md`
  - `docs/EXECUTION_CONTRACT.md`
  - `docs/LOCAL_SMOKE.md`
  - `docs/release/*` governance artifacts

## Protocol compatibility policy

- Peer protocol policy is `N-1 minor (same major)`.
- Current protocol version is defined in `client/src-tauri/src/protocol/version.rs`.
- New protocol changes must remain backward-compatible inside the active window.
