#!/usr/bin/env bash
set -euo pipefail

# codex-os-managed
max_bytes="${ASSET_MAX_BYTES:-350000}"
roots=()
if [[ -d public ]]; then
  roots+=("public")
fi
if [[ -d dist/assets ]]; then
  roots+=("dist/assets")
fi
if [[ ${#roots[@]} -eq 0 ]]; then
  echo "No asset roots found (expected public/ or dist/assets/)."
  exit 1
fi

fail=0
checked=0
while IFS= read -r file; do
  checked=$((checked + 1))
  size=$(wc -c < "$file")
  if (( size > max_bytes )); then
    echo "Asset too large (>${max_bytes} bytes): $file"
    fail=1
  fi
done < <(
  find "${roots[@]}" -type f \
    \( -name "*.png" -o -name "*.jpg" -o -name "*.jpeg" -o -name "*.webp" -o -name "*.avif" -o -name "*.svg" -o -name "*.ico" -o -name "*.gif" -o -name "*.woff" -o -name "*.woff2" -o -name "*.ttf" -o -name "*.otf" -o -name "*.js" -o -name "*.css" \)
)

if (( checked == 0 )); then
  echo "No matching assets found for budget checks."
  exit 1
fi

exit $fail
