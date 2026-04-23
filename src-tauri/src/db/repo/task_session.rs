use super::now;
use crate::db::events::{DbEvent, Entity};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// One captured external agent-session id per (task, source). The
/// `source` matches `HookEvent.source` (e.g. `"claude_code"`,
/// `"claude"`); we treat it as opaque on the storage side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSessionRow {
    pub task_id: String,
    pub source: String,
    pub external_session_id: String,
    pub last_seen_at: i64,
}

pub struct TaskSessionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TaskSessionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, task_id: &str, source: &str) -> Result<Option<AgentSessionRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT task_id, source, external_session_id, last_seen_at
                   FROM task_agent_sessions
                  WHERE task_id = ?1 AND source = ?2",
                params![task_id, source],
                row_to_session,
            )
            .optional()?;
        Ok(row)
    }

    pub fn list_for_task(&self, task_id: &str) -> Result<Vec<AgentSessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT task_id, source, external_session_id, last_seen_at
               FROM task_agent_sessions
              WHERE task_id = ?1
              ORDER BY source",
        )?;
        let rows = stmt.query_map([task_id], row_to_session)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Idempotent UPSERT on (task_id, source). Returns the DbEvent so the
    /// caller can fan it out on `db_event` after committing the write.
    pub fn upsert(
        &self,
        task_id: &str,
        source: &str,
        external_session_id: &str,
    ) -> Result<DbEvent> {
        let ts = now();
        self.conn.execute(
            "INSERT INTO task_agent_sessions
               (task_id, source, external_session_id, last_seen_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(task_id, source) DO UPDATE SET
               external_session_id = excluded.external_session_id,
               last_seen_at = excluded.last_seen_at",
            params![task_id, source, external_session_id, ts],
        )?;
        // Composite-key id format mirrors `workspace_repos` /
        // `task_worktrees`: <a>:<b>. Frontend consumers split if needed.
        let id = format!("{task_id}:{source}");
        Ok(DbEvent::update(Entity::AgentSession, id))
    }
}

fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<AgentSessionRow> {
    Ok(AgentSessionRow {
        task_id: row.get(0)?,
        source: row.get(1)?,
        external_session_id: row.get(2)?,
        last_seen_at: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("../../../migrations/0001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../../migrations/0002_schema.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../../migrations/0003_agent_presets.sql"))
            .unwrap();
        conn.execute_batch(include_str!(
            "../../../migrations/0004_task_tickets_and_branch.sql"
        ))
        .unwrap();
        conn.execute_batch(include_str!(
            "../../../migrations/0012_agent_sessions_and_resume.sql"
        ))
        .unwrap();
        // Insert a parent task row so the FK is satisfied.
        conn.execute(
            "INSERT INTO tasks (id, workspace_id, name, slug, branch_name, status, agent_preset, created_at, completed_at)
             VALUES ('t1', NULL, 'name', 'slug', 'weft/slug', 'idle', NULL, 0, NULL)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn upsert_inserts_then_updates() {
        let conn = mk_db();
        let repo = TaskSessionRepo::new(&conn);
        repo.upsert("t1", "claude_code", "sess-1").unwrap();
        let r = repo.get("t1", "claude_code").unwrap().unwrap();
        assert_eq!(r.external_session_id, "sess-1");
        let first_seen = r.last_seen_at;
        std::thread::sleep(std::time::Duration::from_millis(1100));
        repo.upsert("t1", "claude_code", "sess-2").unwrap();
        let r = repo.get("t1", "claude_code").unwrap().unwrap();
        assert_eq!(r.external_session_id, "sess-2");
        assert!(r.last_seen_at >= first_seen);
    }

    #[test]
    fn distinct_sources_coexist() {
        let conn = mk_db();
        let repo = TaskSessionRepo::new(&conn);
        repo.upsert("t1", "claude_code", "sid-c").unwrap();
        repo.upsert("t1", "codex", "sid-x").unwrap();
        let all = repo.list_for_task("t1").unwrap();
        assert_eq!(all.len(), 2);
    }
}
