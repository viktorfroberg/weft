//! Dynamic task ↔ repo membership (add / remove at runtime, not just at
//! task creation).
//!
//! Rationale: early versions of weft locked a task's repo list to whatever
//! its parent workspace had at creation time. Real usage proved that too
//! rigid — mid-task you realize you also need the API repo, and forcing
//! the user to kill the task and recreate it is ceremony. This service
//! lets tasks grow/shrink their repo membership while running.
//!
//! The ops mirror `task_create`'s primitives (short DB read → unlocked git
//! op → short DB write) but at one-repo granularity. If git fails, we
//! don't touch the DB. If the DB write fails after git succeeds, we roll
//! back the worktree.

use crate::db::events::DbEvent;
use crate::db::repo::{
    NewTaskWorktree, ProjectLinkRepo, ProjectLinkRow, ProjectRepo, TaskRepo, TaskWorktreeRepo,
};
use crate::git::{worktree_add, worktree_remove, WorktreeOptions};
use crate::services::worktree_links::{
    apply_links, remove_links, write_baseline_excludes, write_project_id_breadcrumb,
};
use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct AddRepoOutput {
    pub worktree_path: PathBuf,
    pub task_branch: String,
    pub base_branch: String,
    pub event: DbEvent,
}

pub fn add_repo_to_task(
    db: &Arc<Mutex<Connection>>,
    worktrees_base: &Path,
    task_id: &str,
    project_id: &str,
    base_branch_override: Option<String>,
    clone_fallbacks: Arc<Mutex<HashSet<(String, String)>>>,
) -> Result<AddRepoOutput> {
    // Phase 1: short DB read — fetch task + project, derive paths, load
    // any warm-env links the project has configured.
    let (task, project, branch, worktree_path, base_branch, links) = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;

        let task = TaskRepo::new(&conn)
            .get(task_id)?
            .ok_or_else(|| anyhow!("task {task_id} not found"))?;

        let project = ProjectRepo::new(&conn)
            .get(project_id)?
            .ok_or_else(|| anyhow!("project {project_id} not found"))?;

        // Reject if already attached — no-op is easier to explain than a
        // silent dup.
        let existing = TaskWorktreeRepo::new(&conn).list_for_task(&task.id)?;
        if existing.iter().any(|w| w.project_id == project.id) {
            return Err(anyhow!(
                "project {} already attached to task {}",
                project.name,
                task.slug
            ));
        }

        // Read from the task row, not reconstructed — the task may use
        // `feature/<slug>` (ticket-linked) or `weft/<slug>` (default).
        let branch = task.branch_name.clone();
        let worktree_path = worktrees_base.join(&task.slug).join(&project.name);
        let base_branch = base_branch_override
            .unwrap_or_else(|| project.default_branch.clone());
        // Mid-task +Add repo always applies the project's current
        // warm-env config (no per-task override surface here; the
        // task's original `warm_links` choice applied at create time).
        let links: Vec<ProjectLinkRow> =
            ProjectLinkRepo::new(&conn).list_for_project(&project.id)?;

        (task, project, branch, worktree_path, base_branch, links)
    };
    // Lock released.

    // Phase 2: git op, no DB lock.
    let opts = WorktreeOptions {
        repo_path: PathBuf::from(&project.main_repo_path),
        target_path: worktree_path.clone(),
        branch: branch.clone(),
        base_branch: base_branch.clone(),
    };
    worktree_add(&opts).with_context(|| {
        format!(
            "add worktree for {} at {}",
            project.name,
            worktree_path.display()
        )
    })?;

    // Project-id breadcrumb — picked up by install-lock wrappers. Not
    // dependent on warm links.
    write_project_id_breadcrumb(&worktree_path, &project.id);

    // Defensive baseline excludes (`/node_modules`, ...) so the changes
    // panel stays clean regardless of repo .gitignore patterns or
    // warm-link config. Same call as task_create.
    write_baseline_excludes(&worktree_path);

    // Phase 2.5: warm-env links. Atomic — on failure, unwind this
    // worktree (apply_links already cleaned its own partials).
    let mut linked_paths: Vec<PathBuf> = Vec::new();
    if !links.is_empty() {
        match apply_links(
            &project.id,
            Path::new(&project.main_repo_path),
            &worktree_path,
            &links,
            Arc::clone(&clone_fallbacks),
        ) {
            Ok((applied, _reports)) => {
                linked_paths = applied.into_iter().map(|a| a.path).collect();
            }
            Err(e) => {
                tracing::warn!(
                    target: "weft::links",
                    project = %project.name,
                    error = %e,
                    "warm-env links failed; rolling back worktree"
                );
                let _ = worktree_remove(&opts.repo_path, &worktree_path, true);
                return Err(e);
            }
        }
    }

    // Phase 3: DB write. Roll back worktree (+ links) on failure.
    let event = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let insert_res = TaskWorktreeRepo::new(&conn).insert(NewTaskWorktree {
            task_id: task.id.clone(),
            project_id: project.id.clone(),
            worktree_path: worktree_path.to_string_lossy().into_owned(),
            task_branch: branch.clone(),
            base_branch: base_branch.clone(),
            status: "ready".into(),
        });
        match insert_res {
            Ok((_, ev)) => ev,
            Err(e) => {
                // Roll back disk side: links first, then worktree.
                if !linked_paths.is_empty() {
                    remove_links(&worktree_path, &linked_paths);
                }
                let _ = worktree_remove(&opts.repo_path, &worktree_path, true);
                return Err(e.context("insert task_worktree row"));
            }
        }
    };

    // Rewrite the shared context sidecar + CLAUDE.md mirror so the
    // newly-added repo shows up in the auto block. Non-fatal.
    if let Err(e) = crate::services::task_context::refresh_task_context(db, task_id) {
        tracing::warn!(
            task = %task_id,
            error = %e,
            "add_repo_to_task: refresh_task_context failed (non-fatal)"
        );
    }

    Ok(AddRepoOutput {
        worktree_path,
        task_branch: branch,
        base_branch,
        event,
    })
}

pub fn remove_repo_from_task(
    db: &Arc<Mutex<Connection>>,
    task_id: &str,
    project_id: &str,
) -> Result<DbEvent> {
    // Phase 1: DB read. Include current project link config — if any of
    // those paths live in the worktree as symlinks, we want to unlink
    // them (not follow + delete main-checkout targets) before
    // `worktree_remove --force`.
    let (repo_path, worktree_path, linked_paths) = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let rows = TaskWorktreeRepo::new(&conn).list_for_task(task_id)?;
        let row = rows
            .into_iter()
            .find(|w| w.project_id == project_id)
            .ok_or_else(|| {
                anyhow!("project {project_id} not attached to task {task_id}")
            })?;
        let project = ProjectRepo::new(&conn)
            .get(project_id)?
            .ok_or_else(|| anyhow!("project {project_id} missing"))?;
        let links = ProjectLinkRepo::new(&conn)
            .list_for_project(project_id)?
            .into_iter()
            .map(|l| PathBuf::from(l.path))
            .collect::<Vec<_>>();
        (
            PathBuf::from(project.main_repo_path),
            PathBuf::from(row.worktree_path),
            links,
        )
    };

    // Phase 2: links first (preserves main-checkout targets behind
    // symlinks), then the worktree. Force removal — user already opted
    // in via confirm dialog in the UI. If the worktree is gone already,
    // that's fine.
    if !linked_paths.is_empty() {
        remove_links(&worktree_path, &linked_paths);
    }
    if let Err(e) = worktree_remove(&repo_path, &worktree_path, true) {
        tracing::warn!(
            path = %worktree_path.display(),
            error = %e,
            "worktree_remove failed — continuing to delete row anyway"
        );
    }

    // Phase 3: delete the row.
    let event = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        // No single `delete_for_task_project` on the repo yet — do it
        // inline. Safe because only this row matches the composite PK.
        conn.execute(
            "DELETE FROM task_worktrees WHERE task_id = ?1 AND project_id = ?2",
            rusqlite::params![task_id, project_id],
        )?;
        DbEvent::delete(
            crate::db::events::Entity::TaskWorktree,
            format!("{task_id}:{project_id}"),
        )
    };

    // Regenerate the shared context sidecar in whichever worktrees
    // remain so the removed repo drops out of the `## Repos` block.
    // Non-fatal.
    if let Err(e) = crate::services::task_context::refresh_task_context(db, task_id) {
        tracing::warn!(
            task = %task_id,
            error = %e,
            "remove_repo_from_task: refresh_task_context failed (non-fatal)"
        );
    }

    Ok(event)
}
