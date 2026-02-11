# Delta Plan — Phase 1 Completion Follow-up

## A) Executive Summary
### Current state
- Relay is a two-app monorepo: SolidJS/Tauri client and Go signaling/relay server. (`client/`, `server/`)
- Frontend transfer flow is state-driven in `client/src/App.tsx` with `transfer` store as the contract boundary.
- Backend emits `connectionTypeChanged` with values `direct`/`relay` from Rust command flows.
- Previous patch wired connection type updates and surfaced settings from Home.
- Connection status bar is shown during `waiting|connecting|transferring` and therefore appears before transport is settled.
- Rust/Tauri tests require system GUI libs (`glib-2.0`) in this environment; full cargo test is not currently runnable.

### Key risks
- Transport UI can be misleading if default state is a concrete transport before negotiation completes.
- Type drift between Rust event payload and TypeScript store types can regress silently.
- Limited CI-like verification in this environment for Tauri desktop dependencies.

### Improvement themes (priority)
1. Make connection status semantically accurate during negotiation.
2. Tighten frontend event typings to match backend contract.
3. Maintain auditable engineering trail for interruption-safe continuation.

## B) Constraints & Invariants (Repo-derived)
- Must keep existing transfer command/event contract (`transfer:progress`) intact.
- Must not modify protocol/encryption semantics in this scope.
- Must preserve existing UI flow (send/receive/progress/completion/screens).
- Inference from current code/tests: backend emits `direct|relay` only after transport choice finalizes.
- Non-goals: no new networking features, no backend refactor, no CI pipeline redesign.

## C) Proposed Changes by Theme (Prioritized)
### Theme 1 — Accurate transport state
- Current approach: frontend defaults `connectionType` to `direct` even before negotiation.
- Proposed: introduce explicit `negotiating` state and show neutral status until event arrives.
- Why: avoids false direct badge during waiting/connecting.
- Tradeoffs: one extra UI state branch; minimal complexity.
- Scope boundary: frontend store + status component + state-change handler only.
- Migration: update union type + default values, then render mapping.

### Theme 2 — Contract typing hardening
- Current approach: `connection_type` typed as generic `string`.
- Proposed: narrow to `"direct" | "relay"` in bridge types.
- Why: catches drift and impossible states at compile-time.
- Tradeoffs: stricter typing may require edits if backend changes schema.
- Scope boundary: `client/src/lib/tauri-bridge.ts` only.
- Migration: update type and keep defensive normalization in `App.tsx`.

### Theme 3 — Session audit artifacts
- Current approach: no codex session artifacts were present.
- Proposed: add and maintain `codex/*` planning/log files with checkpoints and verification ledger.
- Why: resume hardening and explicit decision trail.
- Tradeoffs: documentation overhead.
- Scope boundary: documentation files under `codex/` only.

## D) File/Module Delta (Exact)
### ADD
- `codex/SESSION_LOG.md` — step-by-step execution journal.
- `codex/DECISIONS.md` — judgment calls and alternatives.
- `codex/CHECKPOINTS.md` — checkpoint snapshots + rehydration summaries.
- `codex/VERIFICATION.md` — command/result ledger.
- `codex/CHANGELOG_DRAFT.md` — delivery draft.

### MODIFY
- `client/src/stores/transfer.ts` — transport state union + defaults.
- `client/src/components/ConnectionStatus.tsx` — negotiating UI rendering.
- `client/src/App.tsx` — stateChanged handler sets negotiating.
- `client/src/lib/tauri-bridge.ts` — typed connection contract.

### REMOVE/DEPRECATE
- None.

### Boundary rules
- Allowed dependencies: `App.tsx` ↔ store/types/components.
- Forbidden in this delta: changes to Rust event schema, protocol messages, server behavior.

## E) Data Models & API Contracts (Delta)
- Current contract definition: `ConnectionTypeChangedEvent` in `client/src/lib/tauri-bridge.ts`.
- Proposed delta:
  - `connection_type` narrowed to `"direct" | "relay"`.
  - UI-side store contract adds `"negotiating"` as local-only state (`client/src/stores/transfer.ts`).
- Compatibility:
  - Backward: backend unchanged; frontend still normalizes unknown values to `direct` defensively.
  - Forward: if backend adds values, TS compile/runtime fallback highlights required updates.
- Persisted data migration: none.
- Versioning strategy: internal contract lockstep within repo.

## F) Implementation Sequence (Dependency-Explicit)
1. **Objective**: establish baseline and environment constraints.
   - Files: `codex/VERIFICATION.md`, `codex/SESSION_LOG.md`, `codex/CHECKPOINTS.md`
   - Verification: `go test -race ./...`, `pnpm build`, `cargo test`.
   - Rollback: docs-only revert.

2. **Objective**: implement explicit negotiating transport state.
   - Files: `client/src/stores/transfer.ts`, `client/src/components/ConnectionStatus.tsx`, `client/src/App.tsx`.
   - Preconditions: baseline frontend build passes.
   - Verification: `cd client && pnpm build`.
   - Rollback: revert three files.

3. **Objective**: tighten bridge typing to backend event values.
   - Files: `client/src/lib/tauri-bridge.ts`.
   - Dependencies: Step 2 complete.
   - Verification: `cd client && pnpm build`.
   - Rollback: revert file.

4. **Objective**: full verification + docs + screenshots.
   - Files: `codex/*` + screenshot artifacts generated via browser tool.
   - Verification: rerun baseline command set.
   - Rollback: remove only docs/artifacts if needed.

## G) Error Handling & Edge Cases
- Current pattern: frontend event switch + defensive fallback; backend emits events asynchronously.
- Improvement:
  - explicit local `negotiating` prevents misleading status pre-resolution.
  - retain default fallback normalization in App event handler for robustness.
- Edge cases covered:
  - no `connectionTypeChanged` event yet -> status remains neutral.
  - relay fallback event -> status flips to relay.
  - reconnect/new transfer -> reset sets negotiating.

## H) Integration & Testing Strategy
- Integration point: Rust `ProgressEvent::ConnectionTypeChanged` -> Tauri event bridge -> `App.tsx` -> `transfer` store -> `ConnectionStatus` UI.
- Unit tests: none existing in frontend harness; use build + manual UI check.
- Regression checks:
  - `pnpm build` must pass after each code step.
  - server go tests remain green.
  - cargo test recorded as environment-limited baseline exception.
- Definition of Done:
  - status shows Negotiating before transport resolution.
  - status shows Direct/Relay once event arrives.
  - all feasible verification commands pass.

## I) Assumptions & Judgment Calls
### Assumptions
- Backend event payload remains `direct|relay` in current implementation.
- No hidden reviewer comment requires backend schema changes.

### Judgment calls
- Added `negotiating` local UI state instead of reusing `direct` default to avoid misleading UX.
- Kept fallback normalization to `direct` for defensive runtime behavior.
- Did not modify Rust/server event schema to keep scope minimal and reversible.
