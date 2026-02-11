# DECISIONS

## 2026-02-10 — Add explicit `negotiating` UI transport state
- Context: Status bar is visible during `waiting`/`connecting`, but previous default was `direct` before transport resolution.
- Decision: Extend frontend-only store union to `direct | relay | negotiating` and default/reset to `negotiating`.
- Alternatives considered:
  - keep `direct` default (rejected: misleading)
  - hide status bar until transport event arrives (rejected: loses useful context)
- Consequence: clearer and truthful status before backend emits final transport mode.

## 2026-02-10 — Keep backend contract unchanged
- Context: potential mismatch concerns between frontend and backend transport labels.
- Decision: do not change Rust/server event schema; only tighten TS bridge typing to current observed values (`direct|relay`).
- Alternatives considered:
  - change backend payload to include a third state (rejected: unnecessary scope expansion)
- Consequence: minimal, safe patch with strong compile-time guarantees on frontend boundary.

## 2026-02-10 — Proceed despite cargo test environment gap
- Context: `cargo test` fails due missing `glib-2.0` pkg-config dependency in container.
- Decision: document as environment limitation and continue with feasible verification commands.
- Alternatives considered:
  - install system packages in-session (rejected: non-repo mutation, unstable reproducibility)
- Consequence: known yellow status for Rust/Tauri suite in this environment only.
