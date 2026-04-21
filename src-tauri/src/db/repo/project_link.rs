//! `project_links` — per-project list of repo-relative paths that warm
//! worktrees materialize at `task_create` phase 2.5. See migration 0005.
//!
//! Rows are flat `(project_id, path, link_type)`. Preset provenance +
//! non-APFS fallback tracking both live in-memory on AppState; the DB
//! only stores the user's declared intent, not runtime state.

use super::now;
use crate::db::events::{DbEvent, Entity};
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// How a link should be materialized into the worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    /// Symlink from worktree → main checkout. Writes reach main. Use for
    /// deps (node_modules), env files, anything that's shared read-state.
    Symlink,
    /// APFS clonefile copy — per-worktree, diverges on write, shares
    /// blocks until then. Use for build caches (target/, .next/).
    Clone,
}

impl LinkType {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkType::Symlink => "symlink",
            LinkType::Clone => "clone",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "symlink" => Ok(LinkType::Symlink),
            "clone" => Ok(LinkType::Clone),
            other => Err(anyhow!("unknown link_type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLinkRow {
    pub project_id: String,
    pub path: String,
    pub link_type: LinkType,
}

pub struct ProjectLinkRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ProjectLinkRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_for_project(&self, project_id: &str) -> Result<Vec<ProjectLinkRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT project_id, path, link_type
             FROM project_links WHERE project_id = ?1 ORDER BY path ASC",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            let link_type_s: String = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                link_type_s,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (project_id, path, link_type_s) = r?;
            out.push(ProjectLinkRow {
                project_id,
                path,
                link_type: LinkType::parse(&link_type_s)?,
            });
        }
        Ok(out)
    }

    /// Full replace — used by both preset-apply and manual edit. Single
    /// tx so the list never half-updates. Returns one `DbEvent::update`
    /// scoped to the project id; consumers invalidate the whole list.
    pub fn replace(
        &self,
        project_id: &str,
        links: &[ProjectLinkInput],
    ) -> Result<DbEvent> {
        self.conn.execute(
            "DELETE FROM project_links WHERE project_id = ?1",
            params![project_id],
        )?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO project_links (project_id, path, link_type)
             VALUES (?1, ?2, ?3)",
        )?;
        for l in links {
            if l.path.is_empty() {
                return Err(anyhow!("empty path"));
            }
            if l.path.starts_with('/') || l.path.contains("..") {
                return Err(anyhow!(
                    "path must be repo-relative without .. components: {}",
                    l.path
                ));
            }
            stmt.execute(params![project_id, l.path, l.link_type.as_str()])?;
        }
        Ok(DbEvent::update(Entity::ProjectLink, project_id.to_string()))
    }
}

/// What the command layer passes in. No timestamp — inserted server-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLinkInput {
    pub path: String,
    pub link_type: LinkType,
}

// Silence unused-warn in case `now()` isn't pulled in by future code.
#[allow(dead_code)]
fn _keep_now_in_scope() -> i64 {
    now()
}
