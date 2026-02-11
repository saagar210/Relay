# CHANGELOG DRAFT

## Theme: Connection Status Accuracy
- Added frontend-only `negotiating` transport state so the status indicator is truthful before direct/relay selection is finalized.
- Connection status badge now has explicit mapping for three states:
  - `Negotiating` (neutral gray)
  - `Direct P2P` (green)
  - `Relay` (yellow)
- App now resets to `negotiating` while handling `stateChanged=connecting`.

## Theme: Contract Hardening
- Tightened `ConnectionTypeChangedEvent` typing in TypeScript bridge to `direct | relay` to match backend contract.

## Theme: Operational Traceability
- Added codex session artifacts: plan, decisions, checkpoints, verification log, and changelog draft for interruption-safe continuation.
