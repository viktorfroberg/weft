# Contributing

Thanks for caring enough to read this. weft is a personal project shared in the open — a few ground rules keep it manageable.

## Scope

Before opening a PR, skim the [roadmap](docs/roadmap.md). If what you want to build isn't on it, open a **discussion or an issue first**. Saves everyone time vs. a cold PR that doesn't fit.

Solidly in-scope:
- Bug fixes with a clear reproduction.
- Rust test coverage for services / git ops / slug logic.
- Documentation — typos, clarity, missing examples, broken links.
- Improvements to the hook protocol / agent preset surface.

Needs discussion first:
- Any Tauri permission changes.
- New dependencies. weft runs lean on purpose; every dep is a commitment.
- Bigger UI surface additions (sidebars, panels, settings).

Out of scope for now:
- Windows / Linux builds.
- Server-side anything.
- Plugin systems.

## Local dev

See [`docs/development.md`](docs/development.md). TL;DR:

```bash
bun install
bun run tauri dev
cd src-tauri && cargo test
bun x tsc --noEmit
```

Both checks must pass before a PR.

## PR guidelines

- **Small, focused changes.** One concern per PR.
- **Rust changes ship with tests** where there's logic to test (services, git helpers, slug derivation, reconcile).
- **Frontend changes include a screenshot** or short loom if they touch UI — makes review much faster.
- **Don't refactor in the same PR as a feature.** Split them.
- **No backwards-compat shims** for internal code paths. Rename the thing and update all callers.
- **Follow the critical-patterns list** in [architecture.md](docs/architecture.md) — regressing the 3-phase fan-out, the PTY Channel pipe, or the Zustand sentinel pattern breaks things in subtle ways.

## Commit messages

Human-readable, present tense, start with the *why* not the *what*:

- ✅ `fix terminal padding flush-against-tabs by moving to host wrapper`
- ❌ `updated Terminal.tsx`

Don't auto-prefix with ticket IDs. Linear's GitHub app links tickets from the branch name — body redundancy is noise.

## Opening an issue

Use one of the templates in `.github/ISSUE_TEMPLATE/`. Include:

- weft version (`src-tauri/tauri.conf.json` version, or git SHA if built from source)
- macOS version
- Steps to reproduce
- `~/Library/Application Support/weft/crash.log` if the app panicked
- DevTools console output if the symptom is UI-side

## Security

If you find something sensitive (token leak, Keychain bypass, PTY escape), email [viktor@lavendla.se](mailto:viktor@lavendla.se) instead of opening a public issue. No bug bounty; but I'll credit you in the fix commit.
