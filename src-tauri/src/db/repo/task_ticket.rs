//! `task_tickets` — link rows. Since migration 0009 the row also
//! carries a cached `title`/`status` fetched at link time; that's what
//! the context-sidecar writer + UI ticket chips consume so no code path
//! has to hit Linear on every task mutation. See the migration comment
//! for the staleness trade-off.

use super::now;
use crate::db::events::{DbEvent, Entity};
use crate::integrations::TicketLink;
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTicketRow {
    pub task_id: String,
    pub provider: String,
    pub external_id: String,
    pub url: String,
    pub linked_at: i64,
    /// Cached ticket title at link time (or from the last manual
    /// refresh). `None` for rows created before migration 0009 or when
    /// Linear was unreachable at link time.
    pub title: Option<String>,
    /// Cached workflow-state name (e.g. "Todo", "In Review").
    pub status: Option<String>,
    /// Unix-ms the cache was last written. Consumers use this to show
    /// "cached <N>d ago" chips and drive a future auto-refresh policy.
    pub title_fetched_at: Option<i64>,
}

pub struct TaskTicketRepo<'a> {
    conn: &'a Connection,
}

const SELECT_COLS: &str =
    "task_id, provider, external_id, url, linked_at, title, status, title_fetched_at";

impl<'a> TaskTicketRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert a link row. Title/status/title_fetched_at start `NULL` —
    /// `services/task_tickets::link_ticket` fills them right after by
    /// calling `update_cached_metadata` with the Linear fetch result.
    pub fn insert(&self, task_id: &str, link: &TicketLink) -> Result<DbEvent> {
        if link.external_id.is_empty() {
            return Err(anyhow!("empty external_id"));
        }
        if link.provider.is_empty() {
            return Err(anyhow!("empty provider"));
        }
        let ts = now();
        self.conn.execute(
            "INSERT OR IGNORE INTO task_tickets
                (task_id, provider, external_id, url, linked_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![task_id, link.provider, link.external_id, link.url, ts],
        )?;
        let id = format!("{}:{}:{}", task_id, link.provider, link.external_id);
        Ok(DbEvent::insert(Entity::Task, id))
    }

    /// Write Linear-fetched metadata into the cache columns.
    /// Idempotent; no event emitted because the consumers that care
    /// (context sidecar, UI chip) already poll after link_ticket.
    pub fn update_cached_metadata(
        &self,
        task_id: &str,
        provider: &str,
        external_id: &str,
        title: Option<&str>,
        status: Option<&str>,
    ) -> Result<()> {
        let ts = now();
        self.conn.execute(
            "UPDATE task_tickets
                SET title = ?4, status = ?5, title_fetched_at = ?6
             WHERE task_id = ?1 AND provider = ?2 AND external_id = ?3",
            params![task_id, provider, external_id, title, status, ts],
        )?;
        Ok(())
    }

    pub fn delete(&self, task_id: &str, provider: &str, external_id: &str) -> Result<DbEvent> {
        self.conn.execute(
            "DELETE FROM task_tickets
             WHERE task_id = ?1 AND provider = ?2 AND external_id = ?3",
            params![task_id, provider, external_id],
        )?;
        let id = format!("{task_id}:{provider}:{external_id}");
        Ok(DbEvent::delete(Entity::Task, id))
    }

    pub fn list_for_task(&self, task_id: &str) -> Result<Vec<TaskTicketRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SELECT_COLS} FROM task_tickets WHERE task_id = ?1 ORDER BY linked_at ASC"
        ))?;
        let rows = stmt.query_map([task_id], row_to_ticket)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// All ticket↔task links for one provider. Powers the Home backlog
    /// strip's "this ticket already has a weft task — jump to it" flow.
    /// Most-recently-linked first.
    pub fn list_for_provider(&self, provider: &str) -> Result<Vec<TaskTicketRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SELECT_COLS} FROM task_tickets WHERE provider = ?1 ORDER BY linked_at DESC"
        ))?;
        let rows = stmt.query_map([provider], row_to_ticket)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn row_to_ticket(row: &rusqlite::Row) -> rusqlite::Result<TaskTicketRow> {
    Ok(TaskTicketRow {
        task_id: row.get(0)?,
        provider: row.get(1)?,
        external_id: row.get(2)?,
        url: row.get(3)?,
        linked_at: row.get(4)?,
        title: row.get(5)?,
        status: row.get(6)?,
        title_fetched_at: row.get(7)?,
    })
}
