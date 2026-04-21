//! Health check for warm-worktree links.
//!
//! Walks every active worktree for a project and stat-checks each
//! configured link. Surfaces dangling symlinks (target missing after
//! main checkout moved) and method-mismatches (user expected a
//! symlink but a previous fallback created a plain directory, etc).
//!
//! Called from:
//!   - Settings ProjectsTab for a health badge per project.
//!   - `reconcile.rs` at startup to flag broken state early.

use crate::db::repo::{ProjectLinkRepo, ProjectLinkRow, ProjectRepo, TaskWorktreeRepo};
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize)]
pub struct LinkHealth {
    pub task_id: String,
    pub worktree_path: String,
    pub path: String,
    pub expected_type: String,
    pub status: LinkStatus,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    /// Link exists and target resolves (symlink target readable, or
    /// clone directory present).
    Ok,
    /// No on-disk entry at all. User may have removed it manually, or
    /// `git clean -fdx` took the symlink.
    Missing,
    /// Symlink exists but its target is unreachable (main checkout
    /// moved / deleted).
    Dangling,
    /// On-disk entry type doesn't match `link_type`. E.g. config says
    /// symlink but a real dir exists — usually means a manual replace.
    Mismatched,
}

pub fn health_for_project(
    db: &Arc<Mutex<Connection>>,
    project_id: &str,
) -> Result<Vec<LinkHealth>> {
    let (links, worktrees) = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let _project = ProjectRepo::new(&conn)
            .get(project_id)?
            .ok_or_else(|| anyhow!("project {project_id} missing"))?;
        let links: Vec<ProjectLinkRow> =
            ProjectLinkRepo::new(&conn).list_for_project(project_id)?;
        let wts = TaskWorktreeRepo::new(&conn)
            .list_all()?
            .into_iter()
            .filter(|w| w.project_id == project_id && w.status == "ready")
            .map(|w| (w.task_id, PathBuf::from(w.worktree_path)))
            .collect::<Vec<_>>();
        (links, wts)
    };

    let mut out: Vec<LinkHealth> = Vec::new();
    for (task_id, wt) in &worktrees {
        for link in &links {
            let target = wt.join(&link.path);
            let status = classify(&target, link);
            out.push(LinkHealth {
                task_id: task_id.clone(),
                worktree_path: wt.display().to_string(),
                path: link.path.clone(),
                expected_type: link.link_type.as_str().to_string(),
                status,
            });
        }
    }
    Ok(out)
}

/// Public classifier — used by `reconcile.rs` on boot (plain
/// `&Connection` context, no Arc), as well as the internal
/// `health_for_project` walk. Takes a single target path + its link
/// spec; returns the status.
pub fn classify_worktree(target: &std::path::Path, link: &ProjectLinkRow) -> LinkStatus {
    classify(target, link)
}

fn classify(target: &std::path::Path, link: &ProjectLinkRow) -> LinkStatus {
    use crate::db::repo::LinkType;
    let Ok(meta) = target.symlink_metadata() else {
        return LinkStatus::Missing;
    };
    if meta.file_type().is_symlink() {
        if link.link_type != LinkType::Symlink {
            return LinkStatus::Mismatched;
        }
        // Follow the symlink. If metadata() errors, target is dangling.
        match fs::metadata(target) {
            Ok(_) => LinkStatus::Ok,
            Err(_) => LinkStatus::Dangling,
        }
    } else if meta.is_dir() {
        if link.link_type != LinkType::Clone {
            return LinkStatus::Mismatched;
        }
        LinkStatus::Ok
    } else if meta.is_file() {
        // File case — either a file `.env` symlink target, or a single
        // file clone. Both valid; treat as ok iff not mismatched.
        if link.link_type == LinkType::Symlink {
            // Plain file exists instead of a symlink — user removed +
            // replaced the link with real content. Mismatched.
            LinkStatus::Mismatched
        } else {
            LinkStatus::Ok
        }
    } else {
        LinkStatus::Ok
    }
}

/// Aggregate summary for a project — one overall status.
#[derive(Debug, Clone, Serialize)]
pub struct HealthSummary {
    pub total: usize,
    pub ok: usize,
    pub missing: usize,
    pub dangling: usize,
    pub mismatched: usize,
}

pub fn summarize(rows: &[LinkHealth]) -> HealthSummary {
    let mut s = HealthSummary {
        total: rows.len(),
        ok: 0,
        missing: 0,
        dangling: 0,
        mismatched: 0,
    };
    for r in rows {
        match r.status {
            LinkStatus::Ok => s.ok += 1,
            LinkStatus::Missing => s.missing += 1,
            LinkStatus::Dangling => s.dangling += 1,
            LinkStatus::Mismatched => s.mismatched += 1,
        }
    }
    s
}
