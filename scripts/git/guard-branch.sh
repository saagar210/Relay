#!/usr/bin/env bash
set -euo pipefail

# codex-os-managed
branch="$(git rev-parse --abbrev-ref HEAD)"
pattern='^codex/(feat|fix|chore|refactor|docs|test|perf|ci|spike|hotfix)/[a-z0-9]+(-[a-z0-9]+)*$'

if [[ "${CI:-}" == "true" || -n "${GITHUB_ACTIONS:-}" ]]; then
  if [[ -n "${GITHUB_HEAD_REF:-}" ]]; then
    branch="${GITHUB_HEAD_REF}"
  elif [[ -n "${GITHUB_REF_NAME:-}" ]]; then
    branch="${GITHUB_REF_NAME}"
  fi
fi

if [[ "$branch" == "HEAD" ]]; then
  if [[ "${CI:-}" == "true" || -n "${GITHUB_ACTIONS:-}" ]]; then
    echo "Detached HEAD in CI; skipping local branch-name enforcement."
    exit 0
  fi
  echo "Detached HEAD is not allowed for local development."
  echo "Checkout a codex/<type>/<slug> branch."
  exit 1
fi

if [[ "$branch" == "main" || "$branch" == "master" ]]; then
  if [[ "${CI:-}" == "true" || -n "${GITHUB_ACTIONS:-}" ]]; then
    echo "Running on protected branch in CI context: $branch"
    exit 0
  fi
  echo "Direct work on $branch is blocked."
  exit 1
fi

if ! [[ "$branch" =~ $pattern ]]; then
  echo "Invalid branch: $branch"
  echo "Expected: codex/<type>/<slug>"
  exit 1
fi
