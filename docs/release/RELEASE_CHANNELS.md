# Release Channels

Relay uses a phased promotion model with three channels.

## Channels

| Channel | Ring | Audience | Promotion requirement |
| --- | --- | --- | --- |
| internal | 0 | team-only validation | No open P0/P1 issues |
| beta | 1 | limited external users | Internal soak + canary scorecard pass |
| stable | 2 | general availability | Beta metrics pass + rollback drill pass |

## Promotion Rules

1. Promotion is allowed only when gates G0-G6 are green.
2. Any abort condition in canary scorecard triggers immediate hold.
3. Stable promotion requires explicit release manager approval.

## Abort Conditions

- Error-rate or reconnect failure breaches configured thresholds.
- Security scan fails or critical vulnerability appears.
- Upgrade/rollback path does not complete within runbook limits.
