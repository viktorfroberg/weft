# weft-cli

Ops harness that ships alongside the desktop app. Same SQLite DB, same on-disk worktrees, same Rust code paths. Useful for scripting, CI, and debugging without launching the UI.

> The CLI talks directly to the DB — if the UI is running, pick one. Concurrent writes are serialized by the SQLite mutex but the UI's in-memory caches won't notice CLI changes until its next `db_event` poll tick.

## Build

```bash
cd src-tauri
cargo build --release --bin weft-cli
```

Binary lands at `src-tauri/target/release/weft-cli`. Symlink or `cp` to your `$PATH`.

## Commands

### Projects (repos)

```bash
weft-cli projects add --path ~/src/my-repo
weft-cli projects list
weft-cli projects rm --id <uuid>
```

### Workspaces

```bash
weft-cli workspaces new --name "chat widget"
weft-cli workspaces list
weft-cli workspaces add-repo --workspace <id> --project <id>
weft-cli workspaces rm --id <id>
```

### Tasks

```bash
weft-cli task new --workspace <id> --name "SSO handoff"
weft-cli task list --workspace <id>
weft-cli task show --id <id>
weft-cli task add-repo --task <id> --project <id>
weft-cli task remove-repo --task <id> --project <id>
weft-cli task cleanup --task <id>        # removes worktrees + DB row
```

### Maintenance

```bash
weft-cli reconcile                 # startup-scan: mark orphan worktrees, remove missing
weft-cli db migrate                # re-run pending migrations (should be a no-op if UI booted)
weft-cli db inspect --table tasks  # quick table dump
```

### Integrations

```bash
weft-cli integrations list
weft-cli integrations set --provider linear --token lin_api_xxx    # writes to Keychain
weft-cli integrations clear --provider linear
weft-cli integrations test --provider linear
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | OK |
| 1 | Generic error (message on stderr) |
| 2 | Not found (entity id doesn't exist) |
| 3 | Conflict (duplicate name, concurrent task create, etc.) |
| 4 | Integration auth failure |

## Examples

### Scripted task create from CI

```bash
WORKSPACE_ID=$(weft-cli workspaces list --json | jq -r '.[] | select(.name=="chat widget") | .id')
TASK_ID=$(weft-cli task new --workspace "$WORKSPACE_ID" --name "nightly lint" --json | jq -r '.id')
# …run your lint in the worktree paths via `weft-cli task show --id $TASK_ID --json`…
weft-cli task cleanup --task "$TASK_ID"
```

### Reconcile after manual worktree deletion

Deleted a worktree with `rm -rf`? Tell weft:

```bash
weft-cli reconcile
```

Orphan task_worktrees rows get marked `missing` rather than silently inconsistent. `+ Add repo` from the UI can re-create them using the branch name from `tasks.branch_name`.
