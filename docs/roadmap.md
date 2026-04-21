# Roadmap

Pre-release. Built for one person's workflow. Shared in the open; happy to merge what makes sense.

## Shipped

| Release | Highlight |
|---|---|
| **v0.x** | Tauri v2 scaffold, SQLite + event bus, 3-phase worktree fan-out, `portable-pty` terminals, Monaco multi-repo diff, axum hook server, native macOS menu, Settings, light/dark/system theme. |
| **v1.0.1** | Agent presets + `agent_launch`. Per-task terminal tabs (Shell + Agent). ⌘L launch. |
| **v1.0.1b** | Tasks-first refactor — dynamic repo membership, `+ Add repo` / `× remove`, `.code-workspace` multi-root open. |
| **UI redesign pass 1** | 40px drag-region toolbar, hashed-oklch ProjectBadge, hero quick-create, sidebar v2 (workspaces as collapsible groups with nested tasks). |
| **v1.0.2** | Linear ticket integration — personal token in Keychain, `task_tickets` join, `branch_name` source of truth, `.weft/tickets.md` in worktrees, picker in hero, chips in TaskView, graceful offline degradation. |
| **Modernization pass 1** | Rust edition 2024, dep bumps, React Compiler, top-level ErrorBoundary. |
| **Foundation pass 2** | TanStack Router (code-based, hash history) + TanStack Query migration. `db_event` → single `invalidateQueries` bridge. Zustand reserved for UI-only state. |
| **Features C–H** | Diff stats bar, xterm ⌘F search, ⌘1–⌘9 worktree focus, `.weft/context.md` editor, agent status badges + exit toasts, `useMutation` migration. |
| **Rolling polish** | Command palette ⌘K, recent-tasks ⌘⇧O, onboarding overlay, toast system (Sonner), Monaco lazy-load, Vite manual chunks, rAF PTY coalescing, Rust tracing + crash breadcrumb, panic hook, PTY-exit waiter refactor. |
| **v1.0.5** | **Appearance & themes.** 4 Base24 schemes (Tokyo Night, One Dark, Catppuccin Latte, GitHub Light). 15 curated presets. Paste-in import (base16/base24 YAML + iTerm `.itermcolors`). Per-scheme Monaco themes. Font family / weight / size / line-height / ligatures. Padding, cursor, bell (visual + audible synth via Web Audio). Unified `applyTheme` for atomic class + CSS vars + Monaco updates. Live Settings preview (no PTY). |
| **v1.0.6** | Warm-worktree links — `project_links` per project (symlink / APFS clone targets like `node_modules`). `apply_links` at worktree create; health scan + re-apply UI. install-lock endpoint for shared `node_modules` across worktrees. |
| **v1.0.7** | Home mission-control dashboard. Task compose card with ticket picker. Flat sidebar with status filters. `tasks.workspace_id` made optional — tasks are fully ad-hoc. Repo groups are a preset, not navigation. |
| **v1.1** | **Agent-multiplicity + shared context.** Auto-launch default agent on task create. ⌘T / `+` picker for extra terminal + agent tabs. Per-preset `bootstrap_prompt_template` + delivery mode (argv vs `--append-system-prompt`) so a second agent orients without replaying the user's prompt. `.weft/context.md` auto-regenerates on task mutations with a user + agent-editable notes block; task-root `CLAUDE.md` mirror for Claude's memory walk-up. Ticket title/status cached at link time, manual refresh button. Background `claude -p --model haiku` auto-rename of task titles, user rename latch (`tasks.name_locked_at`). Inline task title rename in TaskView header. Changes-panel show/hide toggle (⌘\) replaces Work/Review split. Branch cleanup on task delete (`branch_delete_if_merged` preserves diverged branches). Task-root dir cleaned up to avoid orphan CLAUDE.md. |

See [`CHANGELOG.md`](../CHANGELOG.md) for per-release change lists.

## Active

_Nothing in flight right now. See Queued for what's next._

## Queued

### v1.0.3 — PR creation via `gh`
Multi-repo PR dispatch. `pr_create(task_id, repo_ids[], title, body)` shells `gh pr create` per repo after pre-checking `gh auth status`. Cross-links sibling PR URLs in the body when a task spans multiple repos. Opens each URL post-create.

Not doing: ticket URL injection in the body. Linear's GitHub app auto-links from the branch name.

### v1.0.4 — Dogfood week
Real multi-repo task end-to-end with the Linear flow. `DOGFOOD.md` papercut list. Gate: v1.1 release only after dogfood triage.

### v1.2 — First signed release
Apple Developer ID, GitHub Actions signed + notarized `.dmg`, `latest.json` auto-update, updater keypair, real app icon.

## Longer-term (not committed)

- **OAuth for Linear** — replaces the personal token flow once the signed release can own the redirect handler.
- **GitHub Issues as second provider** — mostly a URL/auth swap; UI pieces reused. The internal commands already take `provider_id: &str`.
- **Jira, Notion** — further siblings.
- **Background opacity + blur** — `NSVisualEffectView` via a Tauri plugin. Its own phase — deserves proper design, not a quick flag.
- **Agent preset CRUD UI** — currently SQLite-only.
- **User-configurable keyboard shortcuts**.
- **Structured agent session logs** in a panel next to the terminal.
- **`POST /v1/task_context_append`** hook endpoint so agents can post typed note updates instead of file-editing `.weft/context.md` directly.
- **Lazy refresh of cached Linear ticket titles** (>24h stale re-fetch background pass). Plumbed via `task_tickets.title_fetched_at`; currently only manual refresh surfaces it.
- **Windows / Linux builds** — Tauri makes this possible. Not a v1 concern.

## Non-goals

- Server-side anything. No cloud, no auth, no telemetry.
- Plugin system until ≥2 real use cases surface (YAGNI).
- Claude-Code-specific features. The agent layer stays CLI-generic.
- Auto-prefill commit / PR messages with ticket IDs. The agent writes prose; weft doesn't template it.
