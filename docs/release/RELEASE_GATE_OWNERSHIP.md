# Release Gate Ownership

## Gate Owners

| Gate | Purpose | Primary owner | Backup owner | Escalation target |
| --- | --- | --- | --- | --- |
| G0 | Hygiene + branch/contract policy | Engineering lead | Release manager | PM |
| G1 | Fast correctness | Client and server leads | QA lead | Engineering lead |
| G2 | Integration/E2E | QA lead | Client lead | Engineering lead |
| G3 | Security/supply chain | Security owner | Engineering lead | PM + Security |
| G4 | Perf/reliability | Reliability owner | QA lead | Engineering lead |
| G5 | Release promotion | Release manager | Incident lead | PM |
| G6 | Docs closeout | PM | Engineering lead | Release manager |

## Response SLA

- Failing gate triage starts within 2 business hours.
- P0/P1 gate failures block release until resolved or explicitly accepted.
- Any accepted risk requires written PM + owner acknowledgment in release notes.
