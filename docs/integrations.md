# Integrations

weft connects to external ticketing systems so tasks can derive their branch/slug from real ticket IDs, and agents can read ticket content without manual copy-paste.

**Linear** ships in v1.0.2. GitHub Issues, Jira, Notion are sibling candidates.

## Linear

### Setup

1. Create a Linear personal API key at **[linear.app/settings/api](https://linear.app/settings/api)**. Keys start with `lin_api_`.
2. In weft: **Settings** → **Integrations** → paste the key → **Save & test**. A green check + your Linear display name confirms it worked.
3. Disconnect any time via the same panel — weft deletes the Keychain entry and clears the connected-providers flag.

### Where your token lives

Not in `integrations.json`. Not in SQLite. The token lives in the **macOS Keychain** (`security` framework) under service name `dev.weft.integration.linear`, account `default`.

`~/Library/Application Support/weft/integrations.json` only tracks *which* providers are connected, never secrets:

```json
{ "connected_providers": ["linear"] }
```

Verify the token isn't leaking:

```bash
grep -r "lin_api_" ~/Library/Application\ Support/weft/  # should return nothing
security find-generic-password -s dev.weft.integration.linear  # should find the Keychain entry
```

### Linking tickets to a task

Home's compose card (and the Task view's **+ Link ticket** button) opens the ticket picker:

- Search-as-you-type over your viewer's open issues (Linear GraphQL `viewer.assignedIssues`, 30s in-memory cache).
- Multi-select. Selected tickets appear as chips on the compose card.
- Task name defaults to a short heuristic from your prompt (later refined by the background `claude -p --model haiku` auto-rename — see [agents.md](agents.md#auto-rename)).
- Offline? The picker falls back to manual entry — type any ticket ID + URL.

On task create with tickets linked:

- Branch derives from ticket IDs: `feature/abc-123` for one ticket, `feature/abc-123-124` when they share a team prefix, `feature/abc-12-xyz-9` when they don't.
- `task_tickets` join rows persist in the same 3-phase transaction. The Linear title + status is fetched and cached into the row right after link (new in migration 0009).
- The ticket summary is inlined into the **first agent launch's** positional prompt (`{prompt}` token), so Claude's first user turn already sees the ticket ID, URL, title, and status.
- The auto-generated `.weft/context.md` in every worktree — plus the task-root `CLAUDE.md` mirror — lists linked tickets under `## Linked tickets`. This is what any second or later agent reads to orient itself.

### Retroactive link / unlink

Task view has a **Tickets** strip above the terminal. Chips show `ABC-123 · <cached title>`. Click opens the Linear URL. `×` unlinks. **+ Link ticket** adds more.

Every link/unlink triggers `services/task_context::refresh_task_context`, which re-renders the auto block in `.weft/context.md` + the task-root `CLAUDE.md`. The user-editable notes block in `.weft/context.md` is preserved byte-for-byte. The ContextDialog (FileText icon in the task header) has a **Refresh tickets** button that re-hits Linear for current title + status on every linked ticket, so you can bust the cache when upstream has drifted.

### Graceful degradation

- **Offline?** Chips show ID only, still clickable, still unlinkable.
- **Ticket deleted in Linear?** Chip shows `ABC-999 (unavailable)` in muted styling.
- **Token revoked mid-task?** Existing links stay (DB is source of truth); cached title + status survive until the next successful refresh. New link attempts surface a toast.
- **Stale titles?** Cached at link time, refreshable manually from the ContextDialog. A background staleness policy (>24h re-fetch) is queued but not yet shipped.

### Implementation pointers

| Purpose | File |
|---|---|
| GraphQL client | `src-tauri/src/integrations/linear.rs` |
| Keychain wrapper | `src-tauri/src/integrations/keychain.rs` |
| Connected-providers index | `src-tauri/src/integrations/store.rs` |
| Tauri commands | `src-tauri/src/commands/integrations.rs`, `commands/tickets.rs` |
| `task_tickets` table (v1 join rows) | `src-tauri/migrations/0004_task_tickets_and_branch.sql` |
| Cached title / status / fetched_at columns | `src-tauri/migrations/0009_task_context_shared.sql` |
| Ticket link/unlink + cache writer + manual refresh | `src-tauri/src/services/task_tickets.rs` |
| Context sidecar + CLAUDE.md mirror regen | `src-tauri/src/services/task_context.rs` |
| Frontend picker | `src/components/TicketPicker.tsx` |
| Frontend chips | `src/components/TaskView/TicketsStrip.tsx` |
| Frontend Settings UI | `src/components/SettingsView/IntegrationsTab.tsx` |

### What weft does NOT do

- **Status sync back to Linear.** One-way read only; weft doesn't move your tickets to "In progress." Keep that manual for now.
- **Comments / subtasks.** Not surfaced.
- **OAuth.** Personal tokens only. OAuth comes with the first signed release.
- **Ticket auto-prefill in commits / PRs.** The agent writes those, not weft. Linear's GitHub app auto-links the PR when the branch name contains the ticket IDs — that's the whole pipeline.

## GitHub Issues, Jira, Notion

Not yet. When a second provider lands, the internal Tauri commands already take `provider_id: &str` — the wire is generic. A `TicketProvider` trait materializes at that point (YAGNI until then).

See [roadmap](roadmap.md) for where these fit.
