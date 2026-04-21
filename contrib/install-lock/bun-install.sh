#!/bin/bash
# Serialized `bun install` via weft's install-lock endpoint.
# Use in place of `bun install` in agent hooks / post-pull scripts.

source "$(dirname "$0")/_common.sh"
weft_run_locked bun install "$@"
