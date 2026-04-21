use super::{new_id, now};
use crate::db::events::{DbEvent, Entity};
use crate::model::Project;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProject {
    pub name: String,
    pub main_repo_path: String,
    pub default_branch: String,
    pub color: Option<String>,
}

pub struct ProjectRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ProjectRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, input: NewProject) -> Result<(Project, DbEvent)> {
        let id = new_id();
        let ts = now();
        self.conn.execute(
            "INSERT INTO projects (id, name, main_repo_path, default_branch, color, last_opened_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                input.name,
                input.main_repo_path,
                input.default_branch,
                input.color,
                ts,
                ts,
            ],
        )?;
        let project = Project {
            id: id.clone(),
            name: input.name,
            main_repo_path: input.main_repo_path,
            default_branch: input.default_branch,
            color: input.color,
            last_opened_at: ts,
            created_at: ts,
        };
        Ok((project, DbEvent::insert(Entity::Project, id)))
    }

    pub fn list(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, main_repo_path, default_branch, color, last_opened_at, created_at
             FROM projects ORDER BY last_opened_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_project)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: &str) -> Result<Option<Project>> {
        let p = self
            .conn
            .query_row(
                "SELECT id, name, main_repo_path, default_branch, color, last_opened_at, created_at
                 FROM projects WHERE id = ?1",
                [id],
                row_to_project,
            )
            .optional()?;
        Ok(p)
    }

    pub fn delete(&self, id: &str) -> Result<DbEvent> {
        self.conn
            .execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(DbEvent::delete(Entity::Project, id))
    }

    pub fn set_color(&self, id: &str, color: Option<&str>) -> Result<DbEvent> {
        self.conn.execute(
            "UPDATE projects SET color = ?1 WHERE id = ?2",
            params![color, id],
        )?;
        Ok(DbEvent::update(Entity::Project, id))
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<DbEvent> {
        self.conn.execute(
            "UPDATE projects SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(DbEvent::update(Entity::Project, id))
    }
}

fn row_to_project(row: &rusqlite::Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        main_repo_path: row.get(2)?,
        default_branch: row.get(3)?,
        color: row.get(4)?,
        last_opened_at: row.get(5)?,
        created_at: row.get(6)?,
    })
}
