# Architecture

weft is a **local-only macOS Tauri v2 app** with a Rust backend and a React/Tailwind frontend. No network except the local hook server and optional Linear API calls.

```
┌─────────────────────────────── weft (Tauri v2 window) ────────────────────────────────┐
│                                                                                       │
│    React 19 + TanStack Router/Query + Tailwind v4 + shadcn (Nova)                     │
│      Toolbar · Sidebar · Outlet (Workspace | Task | Settings)                         │
│      xterm.js (canvas + ligatures)   ·   Monaco (lazy)   ·   react-resizable-panels   │
│                         │                     ▲                                       │
│                         │ invoke()            │ Channel<Uint8Array>                   │
│                         ▼                     │                                       │
│    Rust core (edition 2024)                                                           │
│      commands/      — Tauri IPC boundary                                              │
│      services/      — task_create, task_repos, agent_launch, open_in_editor,          │
│                       task_tickets, task_context (fence splicer + refresh),           │
│                       task_naming (background `claude -p` rename),                    │
│                       worktree_links, project_links_health / _reapply,                │
│                       project_link_presets, reconcile                                 │
│      db/repo/       — rusqlite wrappers + `db_event` broadcast                        │
│      git/           — shell-out to `git worktree / status / diff / commit`            │
│                       plus `branch_delete_if_merged` (used on task delete)            │
│      terminal/      — portable-pty sessions, reader+flusher threads, pid-based kill   │
│      hooks/         — axum server (localhost:17293) for agent status events           │
│      integrations/  — linear.rs, keychain.rs, store.rs                                │
│                                                                                       │
└───────────────────────────────────────────────────────────────────────────────────────┘
              │                                    │
              ▼                                    ▼
  ~/.weft/worktrees/<slug>/                 ~/Library/Application Support/weft/
    CLAUDE.md (task-root mirror for           weft.db  (SQLite)
      Claude memory walk-up)                  hooks.json (Bearer token)
    <repo>/                                   integrations.json (connected providers)
      .weft/context.md (shared brief +        Keychain: integration tokens
        agent-editable notes block)
```

## Mental model

| Term | Meaning |
|---|---|
| **Project** | A registered git repo on disk. One row per physical repo. User-facing term: **repo**. |
| **Repo group** (`workspaces` table) | A named *preset* — a group of projects a task typically spans. **Not navigation.** Optional on tasks (`tasks.workspace_id` is nullable; tasks can pick any repos ad-hoc). |
| **Task** | A first-class unit of work with a branch (`weft/<slug>` or `feature/<ticket-ids>`). Membership in repos is via `task_worktrees` rows — dynamic at runtime. |
| **Worktree** | A git worktree on the task branch, at `~/.weft/worktrees/<slug>/<repo>/`. One per (task, project). |
| **Agent** | A CLI process (Claude Code, Codex, any) spawned in a PTY tab alongside the shell, with `--add-dir` pointing at every task worktree plus `WEFT_TASK_ID` / `WEFT_HOOKS_URL` env. Multiple agents can share one task via ⌘T; the first launch gets the user's prompt as argv, subsequent launches get the preset's bootstrap template. |

> `/add-dir` inside Claude Code and weft's **+ Add repo** are different features. The first gives the agent runtime access to a path. The second creates a new isolated worktree on the task branch. Don't conflate.

## Critical patterns (don't break these)

### 3-phase worktree fan-out

`services/task_create.rs` never holds the SQLite mutex across git ops. Three phases:

1. **Plan** (short lock) — read workspace + projects, compute worktree paths, persist a pending task row.
2. **Git** (unlocked) — multi-second worktree + branch creation per project, disk rollback on any failure.
3. **Persist** (short lock) — insert `task_worktrees` rows in a single transaction.

Holding the lock across git would gate every other Tauri command for seconds. Two review passes cemented this; don't regress.

### Terminal PTY

`terminal/session.rs` runs a reader thread (PTY stdout → `Channel<Vec<u8>>`) and a flusher thread (coalesces 64KB / 8ms). Output is streamed as raw bytes through Tauri's typed channel — **never `emit`**, which would serialize each chunk through JSON+base64 and stall on fast producers.

Child lifetime: pid-based. The `TerminalSession` stores an `Option<u32>` pid, un-mutexed. The waiter thread owns the `Child` by value, `.wait()`s without any app-wide lock, and emits a `pty_exit` event when done. `Drop` SIGKILLs via `libc::kill(pid, SIGKILL)`. This replaced an earlier design that deadlocked in `Drop` vs. waiter.

### Event bus → TanStack Query invalidation

Rust writes emit `db_event` (entity, op). The frontend listens once via `DbEventBridge`, coalesces events at 16ms, and calls `queryClient.invalidateQueries()` per entity. No per-store refetch plumbing, no match-arm router. Adding a new entity is two lines: `qk.xyz(id)` key factory entry + one case in the bridge.

### Zustand selector stability

UI-only state (`ui.ts`, `prefs.ts`, `terminal_tabs.ts`, `pty_exits.ts`) still uses Zustand. **Every** `useStore((s) => s.x ?? [])` must use a module-level sentinel (`const EMPTY_X: never[] = []`) for the fallback — inline `[]` creates a new reference every call and triggers `getSnapshot should be cached` → infinite render loop.

### Drag region

The 40px toolbar owns `data-tauri-drag-region`. Every interactive child (buttons, icons, inputs) MUST carry `data-tauri-drag-region="false"` or clicks get eaten by window-drag. Capability `core:window:allow-start-dragging` must be in `capabilities/default.json`.

### Runtime theming

`src/lib/themes/apply.ts` owns a single `applyTheme(theme, scheme)` fn. It toggles `.dark` on `<html>`, writes `data-scheme`, sets every shadcn/Nova CSS var, and calls `monaco.editor.setTheme(…)` — **all in one synchronous pass**. Splitting class toggle and CSS var writes across two effects caused mid-frame desync on system theme flip.

**Tailwind v4 gotcha:** `@theme inline` in `src/index.css` is load-bearing. Without `inline`, utilities like `bg-background` bake the resolved value at build time and runtime CSS-var overrides silently stop working. There's a guard comment; don't drop `inline` without migrating the theming layer first.

## Frontend layout

```
src/
  App.tsx                — QueryClientProvider · DbEventBridge · RouterProvider · ConfirmDialogHost · Toaster
  router.tsx             — code-based TanStack Router tree (hash history)
  main.tsx               — applyInitialTheme() synchronously before React renders, font CSS imports
  query.ts               — QueryClient + qk key factory (projects, workspaces, tasks, changes, …)
  components/
    Shell.tsx            — Toolbar + Sidebar + Outlet, owns useThemeApplier()
    Toolbar.tsx          — breadcrumb + launch button (drag region)
    Sidebar.tsx          — workspaces as collapsible groups with nested tasks
    WorkspaceView.tsx    — hero quick-create (name + optional ticket picker)
    TaskView/            — folder: index, TicketsStrip, ContextDialog
    Terminal.tsx         — xterm wrapper: scheme, font, cursor, ligatures, bell, padding
    TerminalPreview.tsx  — no-PTY xterm for Settings live preview
    ChangesPanel.tsx     — multi-repo diff list with per-repo commit/discard
    DiffViewer.tsx       — lazy-loaded Monaco diff editor, per-scheme theme
    SettingsView/        — AppearanceTab, IntegrationsTab, WorkflowTab, AdvancedTab
  lib/
    themes/              — schemes.ts, derive.ts, apply.ts, fonts.ts, bell.ts, presets.ts, import/
    commands.ts          — typed Tauri invoke wrappers
    db-event-bridge.ts   — single Rust→Query invalidation bridge
    active-route.ts      — router compat layer (Route union + useActiveRoute hook)
    launch-agent.ts      — shared ⌘L + Toolbar-button helper
    shortcuts.ts         — global keyboard routing
  stores/                — Zustand (UI-only after Pass 2)
```

## Backend layout

```
src-tauri/
  Cargo.toml             — edition "2024", axum 0.8, rusqlite 0.33, security-framework 3, fs2
  migrations/            — 0001_init.sql … 0010_task_name_locked_at.sql (see docs/data.md)
  src/
    lib.rs               — Tauri app setup, plugin wiring, state, crash log
    commands/            — one module per Tauri command surface
    services/            — task_create (3-phase fan-out + cleanup_task + branch delete),
                           task_repos, agent_launch, task_tickets (cache titles at link),
                           task_context (fence splicer, refresh_task_context,
                             compose_first_turn, CLAUDE.md mirror),
                           task_naming (background `claude -p --model haiku` rename),
                           worktree_links, project_link_presets,
                           project_links_health / _reapply, open_in_editor, reconcile
    db/
      mod.rs             — Connection setup, migrations, event bus
      repo/              — Project, Workspace, Task, TaskWorktree, Preset, TaskTicket,
                           ProjectLink repos
      events.rs          — DbEvent struct emitted after every write
    git/                 — worktree_add, worktree_remove, branch_delete_if_merged,
                           status, diff, commit, file_sides, discard
    terminal/            — session (pid-based), manager (tracks live sessions)
    hooks/               — axum server, StatusStore, Bearer auth
    integrations/        — linear (GraphQL), keychain (security-framework), store (json)
    model/               — shared structs
    menu.rs              — native macOS menu bar
    debug.rs             — panic hook, Timed macro for tracing
    bin/weft_cli.rs      — ops harness sharing the same SQLite
```

## Data contracts

### `DbEvent`

```rust
pub struct DbEvent {
    pub entity: String,     // "task" | "workspace" | "project" | "task_worktree" | …
    pub op: String,         // "insert" | "update" | "delete"
    pub id: Option<String>,
}
```

Broadcast via Tauri's `emit` right after every DB write. Frontend's `DbEventBridge` coalesces at 16ms, maps entity → query key, calls `invalidateQueries`.

### `PtyExitEvent`

Emitted by the waiter thread when a PTY child exits:

```rust
pub struct PtyExitEvent {
    pub session_id: String,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub success: bool,
}
```

Drives the agent status badges on tabs + exit toasts. See `terminal/session.rs`.

### `DbEventEntity` table

| Entity | Emitted on | Invalidates |
|---|---|---|
| `project` | CRUD on projects | `qk.projects()` |
| `workspace` | workspace CRUD, repo attach/detach | `qk.workspaces()` + `qk.projects()` |
| `task` | task CRUD | `qk.tasks(workspaceId)` + `qk.recentTasks()` |
| `task_worktree` | worktree attach/detach, status change | `qk.taskWorktrees(taskId)` + `qk.changes(taskId)` |
| `task_ticket` | ticket link/unlink | `qk.taskTickets(taskId)` |
| `agent_preset` | preset CRUD | `qk.agentPresets()` |
| `integration` | token save/clear | `qk.integrations()` |

## Related

- [Agents](agents.md) — launch flow, preset template syntax, hook protocol.
- [Themes](themes.md) — runtime theming pipeline in depth.
- [Data layout](data.md) — file + SQLite schema reference.
- [Development](development.md) — build, test, debug.
