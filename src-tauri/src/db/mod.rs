use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;

const MIGRATIONS: &[(&str, &str)] = &[(
    "0001_init",
    include_str!("../../migrations/0001_init.sql"),
)];

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
