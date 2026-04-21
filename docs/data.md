# Data layout

Everything weft writes to disk, where, and why.

## File system

```
~/Library/Application Support/weft/
  weft.db                 # SQLite: projects, workspaces, workspace_repos, tasks,
                          #         task_worktrees, task_tickets, agent_presets,
                          #         project_links, hooks_events (truncated), schema_version
  hooks.json              # Per-launch Bearer token for the hook server (rotated on every start)
  hooks.port              # Port the axum hook server bound to (17293 if free, else next free)
  integrations.json       # { "connected_providers": ["linear"] } — never holds secrets
  crash.log               # Rust panic stack traces, captured by the panic hook

~/.weft/
  worktrees/
    <task-slug>/
      CLAUDE.md           # Task-root mirror of the auto block from .weft/context.md.
                          # Claude's memory loader walks from cwd up to `/`, so this
                          # file auto-loads alongside any repo-local CLAUDE.md.
      <project-name>/     # git worktree on branch tasks.branch_name
        .weft/
          context.md      # Shared task brief: auto block (prompt + cached tickets +
                          # repo layout) + user/agent-editable notes block
          project-id      # Breadcrumb for contrib scripts (plain text UUID)
        …repo files…

macOS Keychain
  dev.weft.integration.linear / default         # Linear personal API token
  dev.weft.integration.<provider> / default     # future providers

Browser localStorage (Tauri WebView)
  weft-prefs              # Zustand persist — theme, schemes, fonts, cursor, bell,
                          # padding, workflow toggles, MRU. Version: 4
```

## SQLite schema

Applied at startup from `src-tauri/migrations/*.sql`. Migrations are idempotent via the `schema_version` table.

### projects

```sql
CREATE TABLE projects (
  id             TEXT PRIMARY KEY,            -- uuid
  name           TEXT NOT NULL,
  main_repo_path TEXT NOT NULL UNIQUE,        -- absolute path on disk
  default_branch TEXT NOT NULL,               -- auto-detected at add time
  color          TEXT,                        -- optional palette hint
  created_at     INTEGER NOT NULL
);
```

### workspaces + workspace_repos

```sql
CREATE TABLE workspaces (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  sort_order INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE workspace_repos (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id   TEXT NOT NULL REFERENCES projects(id)   ON DELETE CASCADE,
  base_branch  TEXT,                          -- per-project override; fall back to default_branch
  sort_order   INTEGER,
  PRIMARY KEY (workspace_id, project_id)
);
```

### tasks + task_worktrees

```sql
CREATE TABLE tasks (
  id                          TEXT PRIMARY KEY,
  workspace_id                TEXT REFERENCES workspaces(id) ON DELETE SET NULL, -- NULLABLE
  name                        TEXT NOT NULL,
  slug                        TEXT NOT NULL UNIQUE,               -- globally unique
  branch_name                 TEXT NOT NULL,                       -- source-of-truth
  agent_preset                TEXT,                                -- references agent_presets.id
  status                      TEXT NOT NULL DEFAULT 'idle',        -- idle|working|waiting|error|done
  created_at                  INTEGER NOT NULL,
  completed_at                INTEGER,
  initial_prompt              TEXT,                                -- compose-card prompt
  initial_prompt_consumed_at  INTEGER,                             -- set on first agent launch
  name_locked_at              INTEGER                              -- set when user renames;
                                                                    -- blocks background LLM rename
);

CREATE TABLE task_worktrees (
  task_id       TEXT NOT NULL REFERENCES tasks(id)    ON DELETE CASCADE,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  worktree_path TEXT NOT NULL,
  task_branch   TEXT NOT NULL,
  base_branch   TEXT NOT NULL,
  status        TEXT NOT NULL DEFAULT 'ready',         -- ready | missing | error
  created_at    INTEGER NOT NULL,
  PRIMARY KEY (task_id, project_id)
);
```

Task branch name is the **single source of truth** — stored as a full string on `tasks`, never reconstructed from a prefix + slug. All path↔branch derivations read this column. Slugs are globally unique so two tasks across different repo groups that happen to share a name won't collide on branch names.

### task_tickets

```sql
CREATE TABLE task_tickets (
  task_id          TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  provider         TEXT NOT NULL,                     -- "linear" today
  external_id      TEXT NOT NULL,                     -- e.g. "PRO-2406"
  url              TEXT NOT NULL,
  linked_at        INTEGER NOT NULL,
  title            TEXT,                              -- cached at link time
  status           TEXT,                              -- cached workflow state
  title_fetched_at INTEGER,                           -- when the cache was last written
  PRIMARY KEY (task_id, provider, external_id)
);
```

Title + status are cached at link time (and refreshable from the ContextDialog) so `task_context::refresh_task_context` never blocks on Linear when a sidecar regenerates.

### agent_presets

```sql
CREATE TABLE agent_presets (
  id                        TEXT PRIMARY KEY,
  name                      TEXT NOT NULL UNIQUE,
  command                   TEXT NOT NULL,
  args_json                 TEXT NOT NULL,            -- JSON array of arg templates
  env_json                  TEXT NOT NULL DEFAULT '{}',
  is_default                INTEGER NOT NULL DEFAULT 0,
  sort_order                INTEGER NOT NULL DEFAULT 0,
  created_at                INTEGER NOT NULL,
  bootstrap_prompt_template TEXT,                     -- orientation text for subsequent agent tabs
  bootstrap_delivery        TEXT CHECK (bootstrap_delivery IN ('argv','append_system_prompt'))
);
```

See [`docs/agents.md`](agents.md) for token syntax and the seeded Claude Code args.

### project_links

```sql
CREATE TABLE project_links (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  path       TEXT NOT NULL,                    -- relative path (e.g. "node_modules")
  link_type  TEXT NOT NULL,                    -- "symlink" | "clone"
  PRIMARY KEY (project_id, path)
);
```

Warm-worktree links applied per task worktree on create (and re-apply on demand from Settings → Projects). Covers the "agent shouldn't `npm install` per worktree" use case. See `services/worktree_links.rs`.

### hooks_events

Agent status events received on `localhost:17293`. Ring-buffered at 1000 rows — older entries truncate on insert.

## localStorage prefs (v4)

```ts
{
  theme: "light" | "dark" | "system",
  schemeLight: string,       // default "catppuccin-latte"
  schemeDark: string,        // default "tokyo-night"
  userSchemes: ColorScheme[],// paste-imports + preset additions

  terminalFontFamily: "jetbrains-mono" | "fira-code" | "geist-mono" | "source-code-pro" | "system",
  terminalFontWeight: 400 | 500 | 600,
  terminalFontSize: number,  // 10-20
  terminalLineHeight: number,// 1.0-1.5
  terminalLigatures: boolean,
  terminalPadX: number,      // 0-24
  terminalPadY: number,      // 0-24
  boldIsBright: boolean,

  cursorStyle: "block" | "bar" | "underline",
  cursorBlink: boolean,
  bellStyle: "off" | "visual" | "audible" | "both",

  autoLaunchAgentOnTickets: boolean, // default true — auto-spawn default preset on task create
  autoRenameTasks: boolean,          // default true — fire background `claude -p` rename
  hasCompletedOnboarding: boolean,
  userName: string,                  // "Good morning, Viktor" in Home
  recentTaskIds: string[],           // MRU capped at 20
}
```

Persisted by Zustand's `persist` middleware. `applyInitialTheme()` in `src/main.tsx` reads this synchronously before React mounts so there's no FOUC / wrong-scheme flash on boot.

Migrations live in `src-tauri/migrations/*.sql`, numbered in order, applied idempotently at startup via the `schema_version` table. Pre-release — no migration-history doc until v1.0.0.

## Clearing state

Full reset (destructive — removes all tasks, workspaces, worktrees):

```bash
rm -rf ~/Library/Application\ Support/weft/
rm -rf ~/.weft/
# Clear localStorage from DevTools → Application → Local Storage → weft-prefs
# Remove Keychain entries
security delete-generic-password -s dev.weft.integration.linear
```

Targeted — just reset prefs:

```bash
# DevTools → Application → Local Storage → right-click weft-prefs → Delete
```

Targeted — drop agent presets (forces re-seed next boot):

```bash
sqlite3 ~/Library/Application\ Support/weft/weft.db 'DELETE FROM agent_presets;'
```
