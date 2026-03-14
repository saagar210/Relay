# Rollback Playbook

Use this when canary or GA health degrades after promotion.

## Trigger Conditions

- Security hard-fail after release cut.
- Reconnect/transfer failure rate above allowed threshold.
- Critical regression in transfer integrity or updater behavior.

## Immediate Actions

1. Pause further promotion (stop at current ring).
2. Announce incident status to release channel.
3. Execute rollback for affected artifact/channel.
4. Verify previous known-good build health.

## Verification Checklist

- Transfer smoke test passes on current rollback target.
- Signaling + relay path checks pass.
- No active critical vulnerability findings.
- User-visible status page/notes are updated.

## Exit Criteria

- System stabilized on prior known-good release.
- Root-cause owner assigned with follow-up timeline.
- Post-incident summary captured in release notes.
