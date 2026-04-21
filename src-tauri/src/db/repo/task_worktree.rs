use super::now;
use crate::db::events::{DbEvent, Entity};
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// A row in `task_worktrees`. Separate from the model-only structs because
/// this table is Phase 4-specific and doesn't need a public model type yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskWorktreeRow {
    pub task_id: String,
    pub project_id: String,
    pub worktree_path: String,
    pub task_branch: String,
    pub base_branch: String,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewTaskWorktree {
    pub task_id: String,
    pub project_id: String,
    pub worktree_path: String,
    pub task_branch: String,
    pub base_branch: String,
    pub status: String,
}

pub struct TaskWorktreeRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TaskWorktreeRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    fn packed_id(task_id: &str, project_id: &str) -> String {
        format!("{task_id}:{project_id}")
    }

    pub fn insert(&self, input: NewTaskWorktree) -> Result<(TaskWorktreeRow, DbEvent)> {
        let ts = now();
        self.conn.execute(
            "INSERT INTO task_worktrees
             (task_id, project_id, worktree_path, task_branch, base_branch, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                input.task_id,
                input.project_id,
                input.worktree_path,
                input.task_branch,
                input.base_branch,
                input.status,
                ts,
            ],
        )?;
        let row = TaskWorktreeRow {
            task_id: input.task_id.clone(),
            project_id: input.project_id.clone(),
            worktree_path: input.worktree_path,
            task_branch: input.task_branch,
            base_branch: input.base_branch,
            status: input.status,
            created_at: ts,
        };
        let id = Self::packed_id(&input.task_id, &input.project_id);
        Ok((row, DbEvent::insert(Entity::TaskWorktree, id)))
    }

    pub fn list_for_task(&self, task_id: &str) -> Result<Vec<TaskWorktreeRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT task_id, project_id, worktree_path, task_branch, base_branch, status, created_at
             FROM task_worktrees WHERE task_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map([task_id], row_to_task_worktree)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_all(&self) -> Result<Vec<TaskWorktreeRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT task_id, project_id, worktree_path, task_branch, base_branch, status, created_at
             FROM task_worktrees",
        )?;
        let rows = stmt.query_map([], row_to_task_worktree)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn update_status(
        &self,
        task_id: &str,
        project_id: &str,
        status: &str,
    ) -> Result<DbEvent> {
        self.conn.execute(
            "UPDATE task_worktrees SET status = ?1 WHERE task_id = ?2 AND project_id = ?3",
            params![status, task_id, project_id],
        )?;
        Ok(DbEvent::update(
            Entity::TaskWorktree,
            Self::packed_id(task_id, project_id),
        ))
    }
}

fn row_to_task_worktree(row: &rusqlite::Row) -> rusqlite::Result<TaskWorktreeRow> {
    Ok(TaskWorktreeRow {
        task_id: row.get(0)?,
        project_id: row.get(1)?,
        worktree_path: row.get(2)?,
        task_branch: row.get(3)?,
        base_branch: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
    })
}
