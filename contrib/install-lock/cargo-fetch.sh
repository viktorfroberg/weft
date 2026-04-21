#!/bin/bash
# Serialized `cargo fetch` via weft's install-lock endpoint.
# For warm Rust worktrees where `target/` is APFS-cloned but you also
# want to serialize registry-index / crate-download updates.

source "$(dirname "$0")/_common.sh"
weft_run_locked cargo fetch "$@"
