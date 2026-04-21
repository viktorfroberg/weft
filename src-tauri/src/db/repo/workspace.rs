use super::{new_id, now};
use crate::db::events::{DbEvent, Entity};
use crate::model::{Workspace, WorkspaceRepo};
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkspace {
    pub name: String,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkspaceRepo {
    pub workspace_id: String,
    pub project_id: String,
    pub base_branch: Option<String>,
    pub sort_order: Option<i64>,
}

pub struct WorkspacesRepo<'a> {
    conn: &'a Connection,
}

impl<'a> WorkspacesRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, input: NewWorkspace) -> Result<(Workspace, DbEvent)> {
        let id = new_id();
        let ts = now();
        let sort_order = input.sort_order.unwrap_or(0);
        self.conn.execute(
            "INSERT INTO workspaces (id, name, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![id, input.name, sort_order, ts],
        )?;
        let ws = Workspace {
            id: id.clone(),
            name: input.name,
            sort_order,
            created_at: ts,
            updated_at: ts,
        };
        Ok((ws, DbEvent::insert(Entity::Workspace, id)))
    }

    pub fn list(&self) -> Result<Vec<Workspace>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, sort_order, created_at, updated_at
             FROM workspaces ORDER BY sort_order, created_at",
        )?;
        let rows = stmt.query_map([], row_to_workspace)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, id: &str) -> Result<DbEvent> {
        self.conn
            .execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
        Ok(DbEvent::delete(Entity::Workspace, id))
    }
}

pub struct WorkspaceRepoRepo<'a> {
    conn: &'a Connection,
}

impl<'a> WorkspaceRepoRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Pack the composite PK as `"<workspace_id>:<project_id>"` for event ids.
    fn packed_id(workspace_id: &str, project_id: &str) -> String {
        format!("{workspace_id}:{project_id}")
    }

    pub fn insert(&self, input: NewWorkspaceRepo) -> Result<(WorkspaceRepo, DbEvent)> {
        let ts = now();
        let sort_order = input.sort_order.unwrap_or(0);
        self.conn.execute(
            "INSERT INTO workspace_repos (workspace_id, project_id, base_branch, sort_order, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                input.workspace_id,
                input.project_id,
                input.base_branch,
                sort_order,
                ts,
            ],
        )?;
        let id = Self::packed_id(&input.workspace_id, &input.project_id);
        let row = WorkspaceRepo {
            workspace_id: input.workspace_id,
            project_id: input.project_id,
            base_branch: input.base_branch,
            sort_order,
            added_at: ts,
        };
        Ok((row, DbEvent::insert(Entity::WorkspaceRepo, id)))
    }

    pub fn list_for_workspace(&self, workspace_id: &str) -> Result<Vec<WorkspaceRepo>> {
        let mut stmt = self.conn.prepare(
            "SELECT workspace_id, project_id, base_branch, sort_order, added_at
             FROM workspace_repos WHERE workspace_id = ?1 ORDER BY sort_order, added_at",
        )?;
        let rows = stmt.query_map([workspace_id], |row| {
            Ok(WorkspaceRepo {
                workspace_id: row.get(0)?,
                project_id: row.get(1)?,
                base_branch: row.get(2)?,
                sort_order: row.get(3)?,
                added_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, workspace_id: &str, project_id: &str) -> Result<DbEvent> {
        self.conn.execute(
            "DELETE FROM workspace_repos WHERE workspace_id = ?1 AND project_id = ?2",
            params![workspace_id, project_id],
        )?;
        Ok(DbEvent::delete(
            Entity::WorkspaceRepo,
            Self::packed_id(workspace_id, project_id),
        ))
    }
}

fn row_to_workspace(row: &rusqlite::Row) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        sort_order: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}
