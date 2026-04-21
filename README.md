<div align="center">

<img src=".github/assets/logo-mark.png" alt="weft" width="96" height="96">

# weft

**Multi-repo agent orchestration for macOS.**

Run parallel AI coding agents across many git repos at once. Every task gets isolated worktrees in every repo — each on a shared task branch — with a built-in multi-repo diff and per-repo commit flow.

</div>

<br>

## tl;dr

```bash
git clone https://github.com/viktorfroberg/weft
cd weft && bun install
bun run tauri dev
```

Register a repo → optionally group repos into a preset → create a task → weft fans out isolated worktrees in every repo on `weft/<slug>` (or `feature/<ticket-id>`). The default agent auto-launches with the full task context; ⌘T opens more terminal or agent tabs. Each agent sees all worktrees, writes diffs you review and commit across repos from one panel.

---

## Why

AI-assisted work increasingly spans multiple repos per task. A feature might touch an admin UI, an API, and a shared library at the same time. Existing agent tooling either locks you to one repo (losing task context) or dumps everything into one session (losing isolation).

weft organizes work by **task**, not by repo:

- **Task-scoped worktrees.** Every task gets a git worktree in every attached repo — agents and your terminal see all of them.
- **Dynamic repo membership.** `+ Add repo` attaches another worktree mid-task, `×` removes one.
- **Agent-agnostic.** Claude Code today, any CLI tomorrow. Preset templates (`{prompt}`, `{bootstrap}`, `{each_path:<flag>}`, …) + hook server with Bearer-auth status events.
- **Shared task context.** An auto-regenerated `.weft/context.md` in every worktree (plus a task-root `CLAUDE.md` mirror for Claude's memory walk-up) gives every agent in the task the same user intent, linked tickets, repos, and a notes block agents can append to.
- **Auto-titled tasks.** Compose-card prompts get a short label on create, then a background `claude -p --model haiku` rewrites it to a proper title. Double-click the title to override — the user rename locks and the LLM stops touching it.
- **First-class Linear ticket linking.** Branch auto-derives from ticket IDs (`feature/<team>-<n>`); ticket title + status is cached at link time and inlined into the agent's first turn.
- **Coherent theming.** Curated Base24 schemes + presets + paste-in import (iTerm `.itermcolors`, base16/base24 YAML). Chrome, terminal, and Monaco diff all move as one palette.
- **Local-only.** No server, no cloud, no auth, no telemetry. Your prefs and DB live under `~/Library/Application Support/weft/`.
- **Tauri v2 + Rust.** Native WebView. `Channel<Vec<u8>>`-based PTY IPC. No Electron overhead.

---

## Docs

| | |
|---|---|
| **[Architecture](docs/architecture.md)** | Layer-by-layer tour: 3-phase worktree fan-out, PTY session design, SQLite event bus, chrome/terminal/Monaco runtime theming. |
| **[Agents](docs/agents.md)** | Launch Claude Code from weft. Preset template syntax, bootstrap prompts, hook protocol. Integrate your own CLI. |
| **[Themes](docs/themes.md)** | Bundled schemes, presets, importing iTerm `.itermcolors` / base24 YAML, creating a user scheme. Font + cursor + bell + padding knobs. |
| **[Integrations](docs/integrations.md)** | Linear ticket linking flow, Keychain token storage, cached titles on link, `.weft/context.md` + task-root `CLAUDE.md` as agent context. |
| **[Shortcuts](docs/shortcuts.md)** | Keyboard reference. Command palette (⌘K), recent tasks (⌘⇧O), new tab picker (⌘T), worktree focus (⌘1–9). |
| **[CLI](docs/cli.md)** | `weft-cli` — ops harness sharing the same SQLite. Scriptable project/task create + reconcile. |
| **[Data layout](docs/data.md)** | Where state lives: SQLite, worktrees, Keychain, localStorage. Migration schema history. |
| **[Development](docs/development.md)** | Build from source, tests, signing & notarization, code layout, release flow. |
| **[Roadmap](docs/roadmap.md)** | Shipped releases and what's next. |

---

## Status

Pre-release, personal-workflow tool. Unsigned dev builds only. Shared in the open in case it's useful to other devs working the same way. See [`docs/roadmap.md`](docs/roadmap.md) for what's shipped and what's next.

## License

MIT. See [LICENSE](LICENSE).

<br>

<details>
<summary><b>Aesthetic reference</b></summary>

Patterns pulled from (rough priority): Linear, Warp, Zed, Ghostty, Cursor, Raycast, Superset. Dense, monospace-adjacent, dev-tool feel — closer to Linear/Zed than to Notion/Airtable. Geist Variable for UI; scheme-configurable mono for terminal and Monaco.

</details>
