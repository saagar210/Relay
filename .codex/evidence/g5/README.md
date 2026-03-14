# G5 Evidence Templates

Required files during enforced release runs:

- `canary-result.json`
- `rollback-drill.json`

Expected shape (canary-result.json):

```json
{
  "result": "promote",
  "approved": true,
  "approver": "name",
  "timestamp": "2026-03-01T00:00:00Z"
}
```

Expected shape (rollback-drill.json):

```json
{
  "passed": true,
  "rollback_minutes": 7,
  "operator": "name",
  "timestamp": "2026-03-01T00:00:00Z"
}
```
