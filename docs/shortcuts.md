# Keyboard shortcuts

All driven by `src/lib/shortcuts.ts` and the native macOS menu. A settings UI for user-configurable bindings is on the roadmap.

## Global

| Key | Action |
|---|---|
| **⌘K** | Command palette — fuzzy-search projects, repo groups, tasks, commands |
| **⌘⇧O** | Recent tasks quick-switcher |
| **⌘1 – ⌘9** | Jump to task (in sidebar status order); in task view, focus the Nth worktree's diff section |
| **⌘B** | Toggle sidebar |
| **⌘P** | Add repo (register a project) |
| **⌘⇧N** | New task (compose overlay) |
| **⌘,** | Open Settings |
| **⌘/** | Show keyboard shortcuts overlay |
| **Esc** | Back / close dialog / close palette |
| **⌘⌥I** | Open DevTools (dev builds) |

## Task view

| Key | Action |
|---|---|
| **⌘L** | Launch default agent as a new tab (Claude Code by default) |
| **⌘T** | New terminal tab picker — pick Shell or any configured agent preset |
| **⌘\\** | Toggle the right-side changes panel (show ↔ hide) |
| **⌘1 – ⌘9** | Focus the Nth worktree's diff section in the Changes panel |
| **⌘F** | Open scrollback search in the active terminal |
| **⌘↵** | In the commit-message textarea: commit across all repos |

Double-click the task title at the top of the TaskView to rename inline; Enter commits, Esc cancels. A manual rename sets `tasks.name_locked_at` so the background auto-rename won't overwrite it.

## Terminal

Standard xterm.js bindings apply (copy, paste, selection). Bell style is configurable under **Settings → Appearance → Bell**.

## Command palette (⌘K)

- Fuzzy-ranks projects, repo groups, tasks, and common commands (new task, settings, …).
- **Enter** activates the top result. **↑/↓** navigates.
- **→** expands a preview in place (repo-group projects, task worktrees).

## Recent-tasks switcher (⌘⇧O)

MRU list of 20 recently-opened tasks. Enter to navigate. Survives restart via the `recentTaskIds` pref.
