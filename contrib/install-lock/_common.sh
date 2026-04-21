#!/bin/bash
# Common install-lock helpers. Sourced by per-tool wrappers.
# Requires: WEFT_HOOKS_URL, WEFT_HOOKS_TOKEN, WEFT_PROJECT_ID.

set -euo pipefail

: "${WEFT_HOOKS_URL:?WEFT_HOOKS_URL not set — run inside a weft PTY}"
: "${WEFT_HOOKS_TOKEN:?WEFT_HOOKS_TOKEN not set — run inside a weft PTY}"

# If WEFT_PROJECT_ID isn't set, walk up from $PWD looking for
# `.weft/project-id` — weft writes one to each worktree's root when
# it creates the worktree. This makes the wrappers work out of the
# box; an agent config can override by exporting WEFT_PROJECT_ID.
if [ -z "${WEFT_PROJECT_ID:-}" ]; then
  _dir="$(pwd)"
  while [ "$_dir" != "/" ] && [ -n "$_dir" ]; do
    if [ -f "$_dir/.weft/project-id" ]; then
      WEFT_PROJECT_ID="$(cat "$_dir/.weft/project-id")"
      export WEFT_PROJECT_ID
      break
    fi
    _dir="$(dirname "$_dir")"
  done
fi
: "${WEFT_PROJECT_ID:?WEFT_PROJECT_ID not set and .weft/project-id not found — run inside a weft worktree}"

# /v1/events → /v1/install-lock on the same host+port.
WEFT_LOCK_URL="${WEFT_HOOKS_URL%/v1/events}/v1/install-lock"
HOLDER_ID="${HOSTNAME:-unknown}:$$"

weft_acquire() {
  curl -fsS -X POST "$WEFT_LOCK_URL" \
    -H "Authorization: Bearer $WEFT_HOOKS_TOKEN" \
    -H 'Content-Type: application/json' \
    -d "{\"project_id\":\"$WEFT_PROJECT_ID\",\"holder_id\":\"$HOLDER_ID\",\"kind\":\"acquire\"}" \
    >/dev/null
}

weft_release() {
  curl -fsS -X POST "$WEFT_LOCK_URL" \
    -H "Authorization: Bearer $WEFT_HOOKS_TOKEN" \
    -H 'Content-Type: application/json' \
    -d "{\"project_id\":\"$WEFT_PROJECT_ID\",\"holder_id\":\"$HOLDER_ID\",\"kind\":\"release\"}" \
    >/dev/null || true
}

weft_run_locked() {
  # Always release, even on SIGINT / crash / non-zero exit.
  trap 'weft_release' EXIT INT TERM
  echo "[weft] acquiring install-lock for $WEFT_PROJECT_ID…" >&2
  weft_acquire
  echo "[weft] acquired → running: $*" >&2
  "$@"
}
