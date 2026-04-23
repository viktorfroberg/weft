//! Startup reconciliation: reconcile `task_worktrees` rows against disk.
//!
//! If a row says `status = "ready"` but the worktree directory is gone
//! (user `rm -rf`'d it, disk full, etc.) we mark it `missing` so the UI
//! can flag it and the user can decide to recreate or clean up.
//!
//! We do NOT auto-delete anything here — reconciliation is observe-only.

use crate::db::events::DbEvent;
use crate::db::repo::TaskWorktreeRepo;
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, Default)]
pub struct ReconcileReport {
    pub total: usize,
    pub still_ready: usize,
    pub marked_missing: Vec<(String, String)>, // (task_id, project_id)
    pub events: Vec<DbEvent>,
}

pub fn reconcile_worktrees(conn: &Connection) -> Result<ReconcileReport> {
    let repo = TaskWorktreeRepo::new(conn);
    let rows = repo.list_all()?;

    let mut report = ReconcileReport {
        total: rows.len(),
        ..Default::default()
    };

    for row in rows {
        // Only rows currently flagged as `ready` are interesting; `creating`
        // / `failed` / `cleaned` are all pre-known states.
        if row.status != "ready" {
            continue;
        }
        if Path::new(&row.worktree_path).exists() {
            report.still_ready += 1;
            continue;
        }
        let event = repo.update_status(&row.task_id, &row.project_id, "missing")?;
        report
            .marked_missing
            .push((row.task_id.clone(), row.project_id.clone()));
        report.events.push(event);
    }

    if !report.marked_missing.is_empty() {
        tracing::warn!(
            missing = report.marked_missing.len(),
            total = report.total,
            "startup reconcile: some worktrees are missing on disk"
        );
    } else {
        tracing::info!(
            total = report.total,
            ready = report.still_ready,
            "startup reconcile: all worktrees accounted for"
        );
    }

    // Opportunistic warm-worktree link health scan. We only log —
    // actual remediation (re-apply after main checkout moved) is a
    // user action from Settings. Scanning on boot puts the diagnostic
    // trail in the log before the user even notices something broke.
    if let Err(e) = reconcile_warm_link_health(conn) {
        tracing::warn!(error = %e, "warm-link health scan failed");
    }

    if let Err(e) = reconcile_scrollback(conn) {
        tracing::warn!(error = %e, "scrollback reconcile failed");
    }

    Ok(report)
}

/// Remove scrollback files on disk that no longer have a matching
/// `terminal_tabs` row. Handles the `task_delete` cascade (DB row goes
/// away, but the file on disk isn't cleaned up as a side effect) and
/// any orphans from crashes between mark_dormant and file write.
pub fn reconcile_scrollback(conn: &Connection) -> Result<()> {
    use crate::db::repo::TerminalTabRepo;

    let Some(base) = dirs::data_dir() else {
        return Ok(());
    };
    let dir = base.join("weft").join("scrollback");
    if !dir.exists() {
        return Ok(());
    }

    let known: std::collections::HashSet<String> =
        TerminalTabRepo::new(conn).list_all_ids()?.into_iter().collect();

    let mut removed = 0usize;
    for entry in std::fs::read_dir(&dir)? {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "scrollback readdir entry failed");
                continue;
            }
        };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let tab_id = match name.strip_suffix(".bin") {
            Some(id) => id,
            None => continue,
        };
        if known.contains(tab_id) {
            continue;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!(path = %path.display(), error = %e, "scrollback unlink failed");
        } else {
            removed += 1;
        }
    }
    if removed > 0 {
        tracing::info!(removed, "startup reconcile: removed orphan scrollback files");
    }
    Ok(())
}

fn reconcile_warm_link_health(conn: &Connection) -> Result<()> {
    use crate::db::repo::{ProjectLinkRepo, ProjectRepo};
    use crate::services::project_links_health::{classify_worktree, LinkStatus};

    let projects = ProjectRepo::new(conn).list()?;
    let mut total_dangling = 0usize;
    let mut total_mismatched = 0usize;
    for project in &projects {
        let links = ProjectLinkRepo::new(conn).list_for_project(&project.id)?;
        if links.is_empty() {
            continue;
        }
        let rows = TaskWorktreeRepo::new(conn).list_all()?;
        for row in rows {
            if row.project_id != project.id || row.status != "ready" {
                continue;
            }
            for link in &links {
                let target = Path::new(&row.worktree_path).join(&link.path);
                match classify_worktree(&target, link) {
                    LinkStatus::Dangling => total_dangling += 1,
                    LinkStatus::Mismatched => total_mismatched += 1,
                    _ => {}
                }
            }
        }
    }
    if total_dangling + total_mismatched > 0 {
        tracing::warn!(
            dangling = total_dangling,
            mismatched = total_mismatched,
            "startup reconcile: warm-worktree links need attention — re-apply via Settings → Projects"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::{NewTaskWorktree, TaskWorktreeRepo};
    use crate::db::repo::{
        NewProject, NewTask, NewWorkspace, NewWorkspaceRepo, ProjectRepo, TaskRepo,
        WorkspaceRepoRepo, WorkspacesRepo,
    };
    use rusqlite::Connection;

    fn mk_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/0001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0002_schema.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0004_task_tickets_and_branch.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0006_initial_prompt.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0010_task_name_locked_at.sql"))
            .unwrap();
        conn
    }

    #[test]
    fn reconcile_marks_missing_worktree() {
        let conn = mk_db();
        // Set up one workspace + one project + one task
        let (ws, _) = WorkspacesRepo::new(&conn)
            .insert(NewWorkspace {
                name: "w".into(),
                sort_order: None,
            })
            .unwrap();
        let (p, _) = ProjectRepo::new(&conn)
            .insert(NewProject {
                name: "p".into(),
                main_repo_path: "/tmp/fake-repo-for-reconcile-test".into(),
                default_branch: "main".into(),
                color: None,
            })
            .unwrap();
        WorkspaceRepoRepo::new(&conn)
            .insert(NewWorkspaceRepo {
                workspace_id: ws.id.clone(),
                project_id: p.id.clone(),
                base_branch: None,
                sort_order: None,
            })
            .unwrap();
        let (t, _) = TaskRepo::new(&conn)
            .insert(NewTask {
                workspace_id: Some(ws.id),
                name: "z".into(),
                agent_preset: None,
                initial_prompt: None,
            })
            .unwrap();

        // Fake task_worktree pointing at a path that doesn't exist.
        TaskWorktreeRepo::new(&conn)
            .insert(NewTaskWorktree {
                task_id: t.id.clone(),
                project_id: p.id.clone(),
                worktree_path: "/tmp/definitely-not-a-real-path-xyzabc".into(),
                task_branch: "weft/z".into(),
                base_branch: "main".into(),
                status: "ready".into(),
            })
            .unwrap();

        let report = reconcile_worktrees(&conn).unwrap();
        assert_eq!(report.total, 1);
        assert_eq!(report.still_ready, 0);
        assert_eq!(report.marked_missing.len(), 1);
        assert_eq!(report.events.len(), 1);

        // Row should now have status="missing"
        let rows = TaskWorktreeRepo::new(&conn).list_for_task(&t.id).unwrap();
        assert_eq!(rows[0].status, "missing");
    }

    #[test]
    fn reconcile_leaves_existing_worktrees_alone() {
        let conn = mk_db();
        let td = tempfile::tempdir().unwrap();
        let wt_path = td.path().join("worktree_exists");
        std::fs::create_dir_all(&wt_path).unwrap();

        let (ws, _) = WorkspacesRepo::new(&conn)
            .insert(NewWorkspace {
                name: "w".into(),
                sort_order: None,
            })
            .unwrap();
        let (p, _) = ProjectRepo::new(&conn)
            .insert(NewProject {
                name: "p".into(),
                main_repo_path: "/tmp/fake".into(),
                default_branch: "main".into(),
                color: None,
            })
            .unwrap();
        WorkspaceRepoRepo::new(&conn)
            .insert(NewWorkspaceRepo {
                workspace_id: ws.id.clone(),
                project_id: p.id.clone(),
                base_branch: None,
                sort_order: None,
            })
            .unwrap();
        let (t, _) = TaskRepo::new(&conn)
            .insert(NewTask {
                workspace_id: Some(ws.id),
                name: "z".into(),
                agent_preset: None,
                initial_prompt: None,
            })
            .unwrap();
        TaskWorktreeRepo::new(&conn)
            .insert(NewTaskWorktree {
                task_id: t.id.clone(),
                project_id: p.id,
                worktree_path: wt_path.to_string_lossy().into_owned(),
                task_branch: "weft/z".into(),
                base_branch: "main".into(),
                status: "ready".into(),
            })
            .unwrap();

        let report = reconcile_worktrees(&conn).unwrap();
        assert_eq!(report.still_ready, 1);
        assert_eq!(report.marked_missing.len(), 0);
    }
}
