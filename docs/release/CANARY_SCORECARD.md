# Canary Scorecard

Use this template for each release candidate promotion decision.

## Build Identity

- Build version:
- Commit/branch:
- Channel under evaluation:
- Evaluation window:

## Gate Snapshot

- G0 Hygiene: pass/fail
- G1 Fast correctness: pass/fail
- G2 Integration/E2E: pass/fail
- G3 Security/supply chain: pass/fail
- G4 Perf/reliability: pass/fail
- G5 Release promotion checks: pass/fail
- G6 Docs closeout: pass/fail

## Operational Signals

- Error-rate delta vs baseline:
- Reconnect failure-rate delta:
- Transfer success-rate delta:
- Support/incident observations:

## Decision

- Result: promote | hold | rollback
- Approver:
- Timestamp:
- Notes:
