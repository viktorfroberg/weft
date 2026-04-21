# Install-lock agent wrappers

When two weft tasks share a project with symlinked `node_modules` (the Node/Bun preset), running `bun install` / `npm ci` / `pnpm install` concurrently corrupts the shared tree. The wrappers in this folder serialize installs via weft's hook server (`:17293` endpoint `/v1/install-lock`), so agents in parallel tasks queue instead of race.

Wrappers are shell scripts that:

1. POST `{kind: "acquire", project_id, holder_id}` to the hook server.
2. Run the actual install command.
3. POST `{kind: "release", project_id, holder_id}` on exit (including on SIGINT, via a `trap`).

The hook server blocks the acquire POST until the lock is free. If a prior holder crashed without releasing, the lock is stolen after 15 minutes (see `src-tauri/src/hooks/install_lock.rs`).

## Environment

Every PTY weft spawns already has these set:

```
WEFT_TASK_ID       # e.g. "01f5...ea92" (UUID)
WEFT_HOOKS_URL     # e.g. "http://127.0.0.1:17293/v1/events"
WEFT_HOOKS_TOKEN   # per-launch Bearer token
```

The wrappers derive the install-lock URL from `WEFT_HOOKS_URL` and need one extra variable — `WEFT_PROJECT_ID` — which you point the wrapper at via an agent-specific mechanism:

- **Claude Code**: set `WEFT_PROJECT_ID` in a `SessionStart` hook (see `claude-code.sh`).
- **Your own**: export it before invoking the wrapper.

## Using

```bash
# Instead of:
bun install

# Run:
./contrib/install-lock/bun-install.sh
```

…with `WEFT_PROJECT_ID` set in the environment (typically via an agent-config hook).

See `bun-install.sh`, `npm-ci.sh`, `pnpm-install.sh`, `cargo-fetch.sh` in this directory.

## Safety

- Lock is **per-project, per-session** — two tasks in different projects don't block each other.
- Holder id is `hostname:pid` — opaque to the server, used only to match a release to its acquire.
- Watchdog steals stale locks after 15 minutes to recover from agent crashes.
- Acquire includes a 15-minute hard ceiling; if you hit that, something else is deeply wrong.

## What if I don't use these wrappers?

The feature is opt-in. Direct `bun install` skips the lock and races as before. Nothing stops you — the lock is cooperative, not enforced.
