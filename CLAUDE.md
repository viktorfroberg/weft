# weft — repo notes for Claude Code

Multi-repo AI-agent orchestration, macOS-only Tauri app. Personal project under `github.com/viktorfroberg` (not Lavendla). ELv2-free — references Superset for UI patterns but copies no code.

## Stack

- **Tauri v2** (Rust backend + native WebView). macOS 12+.
- **Backend**: Rust — `rusqlite` (SQLite, `~/Library/Application Support/weft/weft.db`), `portable-pty` (terminals), shell-out to `git`, `axum` (hook server on `:17293`).
- **Frontend**: React 19 + Vite + TypeScript + Tailwind v4 + shadcn/ui (Nova preset, Lucide, Geist Variable). Monaco diff editor, xterm.js, react-resizable-panels.
- **State**: Zustand stores per entity. Event bus (`db_event`) broadcasts writes; frontend stores debounce-refetch (16ms) to stay in sync.

## Repo layout

```
src-tauri/
  migrations/*.sql        — applied at startup
  src/
    commands/             — Tauri command handlers
    db/repo/              — rusqlite repos (Project, Workspace, Task, TaskWorktree, Preset)
    db/events.rs          — DbEvent type emitted after writes
    git/                  — shell-out primitives (worktree, status, diff, commit)
    hooks/                — axum :17293 event-ingest with Bearer auth, StatusStore
    model/                — shared structs (Task, Project, Workspace, TaskStatus)
    services/
      task_create.rs      — atomic 3-phase fan-out (plan → git unlocked → persist)
      task_repos.rs       — dynamic add/remove repo on existing task
      open_in_editor.rs   — generates `.code-workspace` for VS Code/Cursor
      agent_launch.rs     — resolves preset template → command + args + env
      reconcile.rs        — startup scan for orphan task_worktrees
    terminal/             — PTY sessions (reader+flusher threads, Channel<Vec<u8>>)
    menu.rs               — native macOS menu bar
    bin/weft_cli.rs       — ops harness sharing the same SQLite
src/
  components/             — React components
  stores/                 — Zustand slices (tasks, projects, workspaces, ui, prefs, route, terminal_tabs, changes)
  lib/                    — commands.ts (typed invoke wrappers), events.ts, menu.ts, theme.ts, colors.ts, dialog.ts, notifications.ts
  App.tsx                 — root, toolbar + sidebar + main grid, event bus router, shortcuts
```

## Critical patterns (don't break these)

- **Zustand selector stability**: NEVER `useStore((s) => s.x[id] ?? [])` — the inline `[]` is a new ref every call and triggers `getSnapshot should be cached` → infinite loop. Use a module-level `const EMPTY_X: never[] = []` sentinel. Every `useStore` selector that can fall back MUST use a stable ref.
- **TanStack Query `staleTime: Infinity` vs just-mutated reads**: weft's queryClient pins staleTime, so `queryClient.fetchQuery` hands back cached data that won't yet include a row you just inserted. For "I just created X, now I need it fresh" flows call the command function directly (`tasksListAll()`) and `queryClient.setQueryData` to seed the cache. See `src/lib/launch-agent.ts` for the pattern.
- **PTY IPC**: output goes through `Channel<Vec<u8>>`, NEVER `emit` (JSON+base64 stalls). Reader + flusher thread pair in `terminal/session.rs`.
- **DB mutex scoping**: `create_task_with_worktrees` (and `task_repos`) never hold the mutex across `git` ops. 3-phase: short lock → unlocked git → short lock. Don't regress.
- **Drag region**: full-width `Toolbar` component owns `data-tauri-drag-region`. Every interactive child MUST carry `data-tauri-drag-region="false"` or clicks get eaten. Requires `core:window:allow-start-dragging` in `capabilities/default.json`.
- **Hook auth**: per-launch random token in `~/Library/Application Support/weft/hooks.json`. Agents POST with `Authorization: Bearer <token>`. Token is injected into terminal env as `WEFT_HOOKS_TOKEN`.
- **Worktree path convention**: `~/.weft/worktrees/<task-slug>/<project-name>/`. Branch: `weft/<slug>` (default) or `feature/<ticket-ids>` when created from Linear tickets. Slug is **globally unique** since v1.0.7.
- **Task-root CLAUDE.md** at `~/.weft/worktrees/<slug>/CLAUDE.md` (one level ABOVE repo checkouts) is picked up by Claude's memory walk-up. This is intentional; do not move it inside the worktree or it'll collide with the repo's own CLAUDE.md.
- **`.weft/` is in the common `info/exclude`**, so `git add -A` doesn't stage our sidecars. Do not undo this.
- **`--add-dir` is variadic**: commander.js eats every trailing non-flag arg. Keep `{prompt}` (or any other positional) BEFORE `{each_path:--add-dir}` in `args_json`. Migration 0008 exists because we got this wrong the first time.
- **Per-worktree `info/exclude` is not consulted by git** — only the common one is. `services/worktree_links.rs::append_baseline_to_common_exclude` writes to `--git-common-dir`, not `--git-dir`.
- **ResizeObserver + WebKit display:none**: the xterm host inside `TaskPanelPool` can measure 0×0 when its parent is hidden. `src/components/Terminal.tsx` pairs `ResizeObserver` with `IntersectionObserver` so `fit()` fires on display flips too.
- **Fence splicer in `task_context.rs`** quarantines malformed `.weft/context.md` to `.context.md.corrupt.<ts>` rather than silently merging — malformed input goes through the state machine, not a regex.

## Mental model

- **Project** = registered git repo (one per physical repo). User-facing term: "repo".
- **Workspace** (table) / **Repo group** (UI) = optional preset — a named group of projects a task typically spans. NOT navigation. `tasks.workspace_id` is nullable.
- **Task** = first-class unit of work. Has a branch. Membership in repos is via `task_worktrees` (dynamic at runtime via `task_add_repo` / `task_remove_repo`) — NOT locked to the parent workspace.
- **Agent** = CLI process (Claude Code seeded; any CLI plausible) spawned in a PTY tab. First launch gets the user's prompt via `{prompt}`; subsequent launches get the preset's `bootstrap_prompt_template` (Claude uses `--append-system-prompt` so orientation doesn't burn a user turn).

`/add-dir` inside Claude and weft's "+ Add repo" are different features: the first just gives Claude runtime access to a path; the second creates an isolated worktree on the task branch. Do not conflate.

## Running

```bash
bun install                # first time
bun run tauri dev          # dev (Vite + Rust)
bun run tauri build        # release (unsigned until Apple Dev ID wired)
cd src-tauri && cargo test # Rust tests (88+ pass)
```

Verifying a change end-to-end:
1. `bun run tauri dev`, let it build
2. Register 1–2 local git repos via + Add repo
3. Quick-create a task via Home's compose card (type prompt + pick repos + Enter)
4. Default agent auto-launches; ⌘T opens more tabs; ⌘L re-launches the default
5. DevTools (⌘⌥I) Console — must be error-free; any "getSnapshot should be cached" is a Zustand selector bug

## Related

- Plan / roadmap: `/Users/viktorfroberg/.claude/plans/hi-so-we-had-wondrous-scroll.md`
- UI redesign spec: `/Users/viktorfroberg/.claude/plans/weft-ui-redesign.md`
- Design decisions as they landed: `DESIGN.md` at repo root
- Prior Superset reference clone: `/Users/viktorfroberg/hacks/lavset/superset-spike/` (read for patterns, do not copy — ELv2)
