#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

VERIFY_FILE="${1:-.codex/verify.commands}"
if [[ ! -f "$VERIFY_FILE" ]]; then
  echo "missing verify commands file: $VERIFY_FILE" >&2
  exit 1
fi

executed=0
failed=0

while IFS= read -r cmd || [[ -n "$cmd" ]]; do
  [[ -z "${cmd//[[:space:]]/}" ]] && continue
  [[ "$cmd" =~ ^[[:space:]]*# ]] && continue

  executed=$((executed + 1))
  echo ">>> $cmd"
  if ! bash -lc "$cmd"; then
    failed=1
    break
  fi
done < "$VERIFY_FILE"

if [[ "$executed" -eq 0 ]]; then
  echo "No verification commands were executed from $VERIFY_FILE" >&2
  exit 3
fi

exit "$failed"
