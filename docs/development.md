# Development

## Prerequisites

- macOS 12+
- [Rust](https://rustup.rs) 1.80+ — `rust-toolchain.toml` pins edition 2024
- [Bun](https://bun.sh) 1.3+
- Xcode Command Line Tools (`xcode-select --install`)

## Clone & run

```bash
git clone https://github.com/viktorfroberg/weft
cd weft
bun install
bun run tauri dev        # Vite + Rust dev build
```

First run takes a couple of minutes for the Rust release-mode builds of dependencies. Subsequent incrementals are fast.

## Tests

```bash
cd src-tauri
cargo test               # ~46 Rust tests — services, git ops, slug derivation, reconcile
```

Frontend has no unit tests today. Smoke is the end-to-end checklist in the [README](../README.md) + `bun x tsc --noEmit` for type safety.

## Logging

Rust logs use `tracing`. Env-controlled per crate:

```bash
RUST_LOG=weft=debug bun run tauri dev          # weft-only
RUST_LOG=weft::svc=trace,weft=debug bun run tauri dev  # trace service layer
```

Every Tauri command is traced with entry/exit + duration via the `timed!` macro in `src-tauri/src/debug.rs`.

Frontend forwards its warn/error logs to Rust tracing via the `dev_log` Tauri command — set a breakpoint in the DevTools console, or just tail `bun run tauri dev` output.

## Crash log

Panic hook writes to `~/Library/Application Support/weft/crash.log`. Include its contents in bug reports.

## Code layout

### Backend (`src-tauri/src/`)

```
lib.rs           — Tauri app setup, plugin wiring, AppState, panic hook install
main.rs          — thin entry
debug.rs         — panic hook, Timed struct, timed! macro
menu.rs          — native macOS menu bar
model/           — shared structs (Task, TaskStatus, Project, Workspace)
commands/        — one file per Tauri command module (tasks, workspaces, projects,
                   changes, agent, integrations, tickets, devlog)
services/
  task_create.rs     — atomic 3-phase fan-out (plan → unlocked git → persist);
                       cleanup_task with `branch_delete_if_merged` on task delete
  task_repos.rs      — dynamic add/remove repo on existing task
  task_tickets.rs    — link/unlink + Linear title/status cache + manual refresh
  task_context.rs    — fence splicer + refresh_task_context + compose_first_turn +
                       per-worktree `.weft/context.md` + task-root `CLAUDE.md` mirror
  task_naming.rs     — background `claude -p --model haiku` rename after task create;
                       race-safe UPDATE gated by `tasks.name_locked_at`
  agent_launch.rs    — resolve preset template → command + args + env; {prompt}
                       + {bootstrap} tokens; orphan-flag drop on empty expansion
  worktree_links.rs  — per-task warm-env links (symlink/clone), baseline excludes
  project_link_presets.rs — first-run suggestions for node_modules etc.
  project_links_health.rs / project_links_reapply.rs — health scan + retrofit
  open_in_editor.rs  — generate .code-workspace for VS Code/Cursor
  reconcile.rs       — startup orphan-scan
db/
  mod.rs             — Connection setup, migrations
  events.rs          — DbEvent broadcast
  repo/              — rusqlite repos per entity
git/                 — shell-out to `git worktree / status / diff / commit`
terminal/
  session.rs         — pid-based PTY session, reader + flusher threads
  manager.rs         — tracks live sessions, kill-by-task
hooks/
  server.rs          — axum :17293 with Bearer auth
  store.rs           — StatusStore ring buffer
integrations/
  linear.rs          — GraphQL client, in-memory caches
  keychain.rs        — security-framework wrapper
  store.rs           — integrations.json read/write
bin/weft_cli.rs      — ops harness sharing the same SQLite
```

### Frontend (`src/`)

```
App.tsx              — providers: QueryClient · DbEventBridge · RouterProvider · Toaster
router.tsx           — code-based TanStack Router tree (hash history)
main.tsx             — applyInitialTheme() + font CSS imports + createRoot
query.ts             — QueryClient singleton + qk key factory
components/
  Shell.tsx          — layout root, mounts useThemeApplier()
  Toolbar.tsx        — drag-region + `weft` crumb + status dot on task routes
  Sidebar.tsx        — flat task list with status filters; repo-group groupings
  TaskComposeCard.tsx — Home compose: prompt + repo picker + ticket chips
  NewTabPicker.tsx   — ⌘T / `+` button dialog to add another terminal/agent tab
  InlineTaskRename.tsx — double-click rename used by TaskView title
  TaskView/          — index.tsx · TicketsStrip.tsx · ContextDialog.tsx
  Terminal.tsx       — xterm wrapper (scheme, font, cursor, ligatures, bell, padding)
  TerminalPreview.tsx — no-PTY preview for Settings
  TerminalTabStrip.tsx — tab bar, agent status badges, session-id correlation
  ChangesPanel.tsx   — multi-repo diff + commit textarea
  DiffViewer.tsx     — lazy Monaco diff editor
  SettingsView/      — index · AppearanceTab · IntegrationsTab · WorkflowTab · AdvancedTab · AddSchemeDialog
lib/
  themes/            — schemes · derive · apply · fonts · bell · presets · import/
  commands.ts        — typed invoke wrappers
  events.ts          — onDbEvent · onPtyExit listeners
  db-event-bridge.ts — DbEventBridge React component
  active-route.ts    — TanStack Router compat layer
  shortcuts.ts       — global keyboard routing
  launch-agent.ts    — ⌘L + Toolbar button helper
  theme.ts           — useEffectiveTheme · useActiveScheme · useThemeApplier · applyInitialTheme
  colors.ts          — hashed oklch ProjectBadge palette
stores/
  ui.ts · prefs.ts · terminal_tabs.ts · pty_exits.ts   (UI-only; server state in TanStack Query)
```

## Build for release

Unsigned:

```bash
bun run tauri build      # produces .app + .dmg under src-tauri/target/release/bundle/
```

Signed + notarized (requires Apple Developer ID, `$99/yr`):

```bash
export APPLE_CERTIFICATE="..."                 # base64 of .p12
export APPLE_CERTIFICATE_PASSWORD="..."
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="app-specific-password"  # appleid.apple.com → App-Specific Passwords
export APPLE_TEAM_ID="TEAMID"

bun run tauri build
```

### Updater keypair

The Tauri updater plugin verifies releases with a signing keypair. Generate once, commit the **public** key.

```bash
bun run tauri signer generate -w ~/.tauri/weft.key
# Public key goes in src-tauri/tauri.conf.json → plugins.updater.pubkey
# Private key lives in CI secrets (never commit it)
```

## Release flow

Not yet wired — v1.2 milestone. Sketch:

1. Bump `src-tauri/tauri.conf.json` version + add a CHANGELOG entry.
2. Tag `vX.Y.Z`.
3. GitHub Action builds signed `.dmg` + `latest.json`.
4. Publish release with both artifacts attached.
5. In-app updater points at `https://github.com/viktorfroberg/weft/releases/latest/download/latest.json`.

## Verifying a change end-to-end

1. `bun run tauri dev`, let it build.
2. Add two local git repos (**⌘P** × 2).
3. New workspace (**⌘N**), check both repos.
4. Hero quick-create a task on the workspace page.
5. **⌘L** to launch an agent.
6. Make a change in a repo from outside weft (echo > a file, `git add`).
7. Verify the Changes panel picks it up in both per-repo sections; Monaco diff opens.
8. Commit all, verify `✓ <sha7>` badge per repo.
9. Switch dark scheme via **Settings → Appearance**. Terminal + sidebar + Monaco all retint.
10. DevTools Console (**⌘⌥I**) must be error-free — any `getSnapshot should be cached` is a Zustand selector bug, see [Architecture](architecture.md#zustand-selector-stability).

## Related

- [Architecture](architecture.md) for the mental model and critical patterns.
- [Themes](themes.md) for the theming pipeline.
- [Agents](agents.md) for the launch/hook protocol.
- [Data layout](data.md) for SQLite schema + migrations.
