# Release Readiness Hub

This folder contains pre-GA release governance artifacts that can be completed
before signing credentials are available.

## Documents

- `RELEASE_GATE_OWNERSHIP.md` - owners, backups, and escalation paths.
- `RELEASE_CHANNELS.md` - internal/beta/stable promotion policy.
- `COMPATIBILITY_POLICY.md` - protocol compatibility rules and deprecation window.
- `ROLLBACK_PLAYBOOK.md` - rollback triggers and response checklist.
- `CANARY_SCORECARD.md` - go/hold/rollback decision template.

## Usage

1. Confirm gate ownership and release channel policy.
2. Run the local operator pass in `docs/LOCAL_SMOKE.md` for each macOS candidate build.
3. Use `CANARY_SCORECARD.md` for each candidate build.
4. If abort criteria are hit, execute `ROLLBACK_PLAYBOOK.md`.
5. Record final decision in the release ticket and update release notes.
6. Store machine-checkable evidence in `.codex/evidence/g3` and `.codex/evidence/g5` for enforced release checks.
7. Ensure required signing and provenance env inputs are configured per `.codex/config/release-materials.required.json`.
