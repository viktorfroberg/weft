#!/bin/bash
# Serialized `pnpm install` via weft's install-lock endpoint.

source "$(dirname "$0")/_common.sh"
weft_run_locked pnpm install "$@"
