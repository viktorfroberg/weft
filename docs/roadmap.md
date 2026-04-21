# Roadmap

Pre-release. Built for one person's workflow. Shared in the open; happy to merge what makes sense.

## What's next

- **PR creation via `gh`.** Multi-repo dispatch: `pr_create(task_id, repo_ids[], title, body)` shells `gh pr create` per repo after a `gh auth status` precheck, cross-links sibling PR URLs in the body, opens each URL after create.
- **First signed release.** Apple Developer ID, GitHub Actions signed + notarized `.dmg`, `latest.json` auto-update, updater keypair, real app icon.

## Longer-term (not committed)

- **OAuth for Linear** once the signed release owns a redirect handler.
- **GitHub Issues as second provider** — the internal commands already take `provider_id: &str`.
- **Jira, Notion** — further siblings.
- **Background opacity + blur** via `NSVisualEffectView`. Deserves a proper design phase, not a flag.
- **User-configurable keyboard shortcuts.**
- **Structured agent session logs** in a panel next to the terminal.
- **Periodic Linear ticket-title refresh.** On-access lazy refresh ships today; a background tick would catch tasks open in long-running sessions.
- **Windows / Linux builds.** Tauri makes it possible; not a v1 concern.

## Non-goals

- Server-side anything. No cloud, no auth, no telemetry.
- Plugin system until ≥2 real use cases surface (YAGNI).
- Claude-Code-specific features. The agent layer stays CLI-generic.
- Auto-prefill commit / PR messages with ticket IDs. The agent writes prose; weft doesn't template it.
