pub mod events;
pub mod repo;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;

/// Write the current process id to `<data_dir>/weft.pid` so the CLI can
/// detect a concurrently-running desktop app. Stale pid files are
/// tolerated — `read_app_pid` only reports a pid; `is_process_alive`
/// decides whether to act on it.
pub fn write_app_pid() -> Result<()> {
    let path = data_dir()?.join("weft.pid");
    std::fs::write(&path, std::process::id().to_string())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Best-effort read; returns None if the file is missing or unparseable.
pub fn read_app_pid() -> Option<u32> {
    let path = data_dir().ok()?.join("weft.pid");
    let contents = std::fs::read_to_string(&path).ok()?;
    contents.trim().parse::<u32>().ok()
}

/// Unix-only liveness check via `kill(pid, 0)`.
#[cfg(unix)]
pub fn is_process_alive(pid: u32) -> bool {
    use std::process::Command;
    // kill -0 succeeds iff the process exists and we're allowed to signal it.
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn is_process_alive(_pid: u32) -> bool {
    // Windows / other — conservatively assume alive. weft is macOS-only today.
    true
}

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../../migrations/0001_init.sql")),
    ("0002_schema", include_str!("../../migrations/0002_schema.sql")),
    (
        "0003_agent_presets",
        include_str!("../../migrations/0003_agent_presets.sql"),
    ),
    (
        "0004_task_tickets_and_branch",
        include_str!("../../migrations/0004_task_tickets_and_branch.sql"),
    ),
    (
        "0005_project_links",
        include_str!("../../migrations/0005_project_links.sql"),
    ),
    (
        "0006_initial_prompt",
        include_str!("../../migrations/0006_initial_prompt.sql"),
    ),
    (
        "0007_claude_preset_prompt_arg",
        include_str!("../../migrations/0007_claude_preset_prompt_arg.sql"),
    ),
    (
        "0008_claude_preset_prompt_before_addir",
        include_str!("../../migrations/0008_claude_preset_prompt_before_addir.sql"),
    ),
    (
        "0009_task_context_shared",
        include_str!("../../migrations/0009_task_context_shared.sql"),
    ),
    (
        "0010_task_name_locked_at",
        include_str!("../../migrations/0010_task_name_locked_at.sql"),
    ),
    (
        "0011_terminal_tabs",
        include_str!("../../migrations/0011_terminal_tabs.sql"),
    ),
    (
        "0012_agent_sessions_and_resume",
        include_str!("../../migrations/0012_agent_sessions_and_resume.sql"),
    ),
];

/// Platform data dir for weft (created if missing). Shared by the DB and
/// the hook server's port-file.
pub fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("no platform data dir")?;
    let dir = base.join("weft");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

pub fn open_and_migrate() -> Result<Connection> {
    let db_path = data_dir()?.join("weft.db");
    tracing::info!(path = %db_path.display(), "opening database");
    let conn = Connection::open(&db_path).context("open sqlite db")?;

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS schema_migrations (
            name TEXT PRIMARY KEY,
            applied_at INTEGER NOT NULL
         );",
    )
    .context("set pragmas + ensure migrations table")?;

    for (name, sql) in MIGRATIONS {
        let already: bool = conn
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE name = ?1",
                [name],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if already {
            tracing::debug!(migration = name, "already applied, skipping");
            continue;
        }
        tracing::info!(migration = name, "applying");
        conn.execute_batch(sql)
            .with_context(|| format!("apply migration {name}"))?;
        conn.execute(
            "INSERT INTO schema_migrations (name, applied_at) VALUES (?1, strftime('%s','now'))",
            [name],
        )?;
    }

    Ok(conn)
}
