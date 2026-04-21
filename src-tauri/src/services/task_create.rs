//! Fan-out service: workspace → task → N worktrees, atomic (all-or-nothing).
//!
//! Three-phase design so the DB mutex is only held for short reads/writes,
//! NOT across long-running git operations. Before this redesign, creating
//! a 5-repo task held `state.db` for tens of seconds — every other command
//! (UI query, hook POST) queued behind it.
//!
//! 1. **Plan** (short DB read): fetch workspace_repos + projects, derive
//!    slug + branch + target paths.
//! 2. **Execute** (no DB lock): run `git worktree add` per repo; rollback
//!    already-created worktrees on any failure.
//! 3. **Persist** (short DB write): open tx, insert task + all
//!    task_worktrees rows, commit. If commit fails: rollback all disk
//!    worktrees.

use crate::db::events::DbEvent;
use crate::db::repo::{
    NewTask, NewTaskWorktree, ProjectLinkRepo, ProjectLinkRow, ProjectRepo, TaskRepo,
    TaskTicketRepo, TaskWorktreeRepo, WorkspaceRepoRepo,
};
use crate::git::{
    branch_delete_if_merged, worktree_add, worktree_remove, BranchDeleteOutcome,
    WorktreeOptions,
};
use crate::integrations::TicketLink;
use crate::model::{Project, Task};
use crate::services::worktree_links::{
    apply_links, remove_links, write_baseline_excludes, write_project_id_breadcrumb,
};
use crate::task as task_naming;
use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct CreateTaskInput {
    /// Optional "repo group" tag pointing at a `workspaces` row. When
    /// provided AND `project_ids` is empty, the fan-out falls back to
    /// the group's `workspace_repos` as the repo set. When `project_ids`
    /// is provided, this is purely a label for filtering/display.
    pub workspace_id: Option<String>,
    pub name: String,
    pub agent_preset: Option<String>,
    /// Explicit repo selection (v1.0.7). Takes precedence over any
    /// `workspace_id`-derived list. Empty = use the workspace fallback
    /// (error if no workspace_id either).
    pub project_ids: Vec<String>,
    /// Optional per-project base-branch overrides (project_id → branch).
    /// Missing entries fall back to the workspace_repos override (if
    /// any), then `projects.default_branch`.
    pub base_branches: std::collections::HashMap<String, String>,
    /// Tickets to link to the new task. When non-empty, the slug is derived
    /// from their IDs (with shared team-prefix dedupe) and the branch is
    /// `feature/<slug>` instead of the default `weft/<slug>`.
    pub tickets: Vec<TicketLink>,
    /// Apply the project's configured `project_links` (warm env) to every
    /// new worktree. Defaults to `true` — the task-create dialog's
    /// "Create cold (skip warm env)" disclosure sets this false.
    pub warm_links: bool,
    /// Prompt the user typed in Home's compose card. Persisted on the
    /// task row; weft writes it into the spawned agent's PTY as the
    /// first user message, then marks it consumed so relaunches don't
    /// duplicate. `None` or empty = no prompt to deliver.
    pub initial_prompt: Option<String>,
}

#[derive(Debug)]
pub struct CreateTaskOutput {
    pub task: Task,
    pub worktrees: Vec<CreatedWorktree>,
    pub events: Vec<DbEvent>,
}

#[derive(Debug, Clone)]
pub struct CreatedWorktree {
    pub project_id: String,
    pub project_name: String,
    pub worktree_path: PathBuf,
    pub task_branch: String,
    pub base_branch: String,
}

/// Track worktrees we've created so we can roll them back on failure.
/// `linked_paths` records warm-env symlinks/clones materialized after
/// `git worktree add` succeeded — they must be removed before
/// `git worktree remove --force` (which won't remove un-git-tracked
/// directories like APFS clones).
struct RollbackEntry {
    repo_path: PathBuf,
    worktree_path: PathBuf,
    linked_paths: Vec<PathBuf>,
}

struct PlanItem {
    project: Project,
    base_branch: String,
    worktree_path: PathBuf,
    /// Warm-env link spec for this project. Empty when the task opted
    /// out via `warm_links = false` or the project has no links configured.
    links: Vec<ProjectLinkRow>,
}

pub fn create_task_with_worktrees(
    db: &Arc<Mutex<Connection>>,
    worktrees_base: &Path,
    input: CreateTaskInput,
    clone_fallbacks: Arc<Mutex<HashSet<(String, String)>>>,
) -> Result<CreateTaskOutput> {
    // Phase 1: plan (short DB read). Crucially, reserve the unique slug
    // HERE so worktree paths (phase 2) and the task row (phase 3) agree.
    // Previously phase 3 re-ran unique_slug, which could hand back a
    // different slug than phase 2 used for the paths. See review S1.
    let (slug, branch, task_dir, plan_items) = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;

        // Resolve the repo set. Three cases:
        //   1. `project_ids` provided — use directly (v1.0.7 ad-hoc path).
        //   2. Empty `project_ids`, `workspace_id: Some` — fall back to
        //      the repo group's `workspace_repos` list.
        //   3. Both empty — error. No repos, no task.
        //
        // `base_branch_overrides` combines the caller's explicit overrides
        // with any per-workspace defaults.
        let (project_ids, mut base_branch_overrides): (
            Vec<String>,
            std::collections::HashMap<String, String>,
        ) = if !input.project_ids.is_empty() {
            (input.project_ids.clone(), input.base_branches.clone())
        } else if let Some(ws_id) = input.workspace_id.as_deref() {
            let ws_repos = WorkspaceRepoRepo::new(&conn).list_for_workspace(ws_id)?;
            if ws_repos.is_empty() {
                return Err(anyhow!("repo group {ws_id} has no repos attached"));
            }
            let mut overrides = input.base_branches.clone();
            for wr in &ws_repos {
                if let Some(b) = wr.base_branch.as_ref() {
                    overrides.entry(wr.project_id.clone()).or_insert_with(|| b.clone());
                }
            }
            let ids = ws_repos.into_iter().map(|wr| wr.project_id).collect();
            (ids, overrides)
        } else {
            return Err(anyhow!(
                "task needs at least one repo (pass project_ids or workspace_id)"
            ));
        };
        // Drop mut once we're done building it.
        let _ = &mut base_branch_overrides;

        // Derive the base slug. Tickets take precedence over the name-derived
        // slug so `feature/abc-123-124` reads from ticket IDs as intended;
        // name still populates the human-readable `tasks.name` column.
        let base_slug = if !input.tickets.is_empty() {
            let ids: Vec<&str> = input.tickets.iter().map(|t| t.external_id.as_str()).collect();
            task_naming::derive_slug_from_tickets(&ids)
        } else {
            task_naming::derive_slug(&input.name)
        };
        if base_slug.is_empty() {
            return Err(anyhow!(
                "could not derive slug (name {:?}, tickets {})",
                input.name,
                input.tickets.len()
            ));
        }
        let slug = TaskRepo::new(&conn).unique_slug(&base_slug)?;

        // Branch prefix: ticket-linked tasks use `feature/`, matching
        // Viktor's start-of-session ritual (branches named after the
        // tickets). Ticketless tasks keep the `weft/` default.
        let branch = if input.tickets.is_empty() {
            format!("weft/{slug}")
        } else {
            format!("feature/{slug}")
        };
        let task_dir = worktrees_base.join(&slug);

        let plan_items: Vec<PlanItem> = project_ids
            .iter()
            .map(|pid| {
                let project = ProjectRepo::new(&conn)
                    .get(pid)?
                    .ok_or_else(|| anyhow!("project {pid} missing"))?;
                let base_branch = base_branch_overrides
                    .get(pid)
                    .cloned()
                    .unwrap_or_else(|| project.default_branch.clone());
                let worktree_path = task_dir.join(&project.name);
                // Pull warm-env links. Empty when the task is cold or
                // the project has nothing configured. Cheap read in the
                // same short-lock phase.
                let links = if input.warm_links {
                    ProjectLinkRepo::new(&conn).list_for_project(&project.id)?
                } else {
                    Vec::new()
                };
                Ok(PlanItem {
                    project,
                    base_branch,
                    worktree_path,
                    links,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        (slug, branch, task_dir, plan_items)
    };
    // Lock released.

    // Phase 2: execute git ops (NO DB lock)
    let mut created: Vec<RollbackEntry> = Vec::new();
    let mut worktrees_out: Vec<CreatedWorktree> = Vec::new();
    for item in &plan_items {
        let opts = WorktreeOptions {
            repo_path: PathBuf::from(&item.project.main_repo_path),
            target_path: item.worktree_path.clone(),
            branch: branch.clone(),
            base_branch: item.base_branch.clone(),
        };
        if let Err(e) = worktree_add(&opts) {
            rollback_disk(&created);
            return Err(e.context(format!(
                "create worktree for project {} (at {})",
                item.project.name,
                item.worktree_path.display()
            )));
        }
        // IMPORTANT: register immediately so any later failure (incl. DB
        // commit) rolls this worktree back.
        let mut linked_paths: Vec<PathBuf> = Vec::new();

        // Drop the project-id breadcrumb unconditionally — doesn't
        // depend on warm links. Agent install wrappers in
        // `contrib/install-lock/` read it to auto-discover the
        // project without the user having to set an env var.
        write_project_id_breadcrumb(&item.worktree_path, &item.project.id);

        // Defensive baseline excludes (`/node_modules`, etc.) so the
        // changes panel doesn't list dependency dirs the user clearly
        // didn't author. Runs unconditionally — covers warm-link
        // symlinks AND repos whose `.gitignore` uses trailing-slash
        // patterns that don't match symlinks.
        write_baseline_excludes(&item.worktree_path);

        // Phase 2.5: materialize warm-env links (symlink / APFS clone).
        // `apply_links` is atomic per worktree — on any failure, it
        // unwinds its own partials before returning Err. We only record
        // successful AppliedLink entries for outer rollback.
        if !item.links.is_empty() {
            match apply_links(
                &item.project.id,
                Path::new(&item.project.main_repo_path),
                &item.worktree_path,
                &item.links,
                Arc::clone(&clone_fallbacks),
            ) {
                Ok((applied, reports)) => {
                    for r in &reports {
                        tracing::debug!(
                            target: "weft::links",
                            project = %item.project.name,
                            path = %r.path.display(),
                            outcome = ?r.outcome,
                            "link"
                        );
                    }
                    linked_paths = applied.into_iter().map(|a| a.path).collect();
                }
                Err(e) => {
                    // apply_links already unwound its partials in this
                    // worktree. Now rollback the worktree itself + any
                    // previous repos' worktrees.
                    tracing::warn!(
                        target: "weft::links",
                        project = %item.project.name,
                        error = %e,
                        "warm-env links failed; rolling back worktree"
                    );
                    // Force-remove this worktree (no links remain in it).
                    let _ = worktree_remove(
                        Path::new(&item.project.main_repo_path),
                        &item.worktree_path,
                        true,
                    );
                    rollback_disk(&created);
                    return Err(e);
                }
            }
        }

        created.push(RollbackEntry {
            repo_path: PathBuf::from(&item.project.main_repo_path),
            worktree_path: item.worktree_path.clone(),
            linked_paths,
        });
        worktrees_out.push(CreatedWorktree {
            project_id: item.project.id.clone(),
            project_name: item.project.name.clone(),
            worktree_path: item.worktree_path.clone(),
            task_branch: branch.clone(),
            base_branch: item.base_branch.clone(),
        });
    }

    // Keep `task_dir` in scope for potential cleanup of the empty parent dir.
    let _ = task_dir;

    // Phase 3: persist (short DB write tx). If anything in here fails, roll
    // back disk worktrees too — DB rollback is automatic via tx drop.
    let persist_result = (|| -> Result<(Task, Vec<DbEvent>)> {
        let mut conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let tx = conn.transaction().context("open transaction")?;

        let (task, task_event) = TaskRepo::new(&tx).insert_with_slug(
            NewTask {
                workspace_id: input.workspace_id.clone(),
                name: input.name.clone(),
                agent_preset: input.agent_preset.clone(),
                initial_prompt: input
                    .initial_prompt
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            },
            &slug,
            &branch,
        )?;

        let mut events = vec![task_event];

        for (item, created_wt) in plan_items.iter().zip(worktrees_out.iter()) {
            let (_, tw_event) = TaskWorktreeRepo::new(&tx).insert(NewTaskWorktree {
                task_id: task.id.clone(),
                project_id: item.project.id.clone(),
                worktree_path: created_wt.worktree_path.to_string_lossy().into_owned(),
                task_branch: branch.clone(),
                base_branch: item.base_branch.clone(),
                status: "ready".to_string(),
            })?;
            events.push(tw_event);
        }

        // Link any tickets the caller passed in, same tx. If the tx rolls
        // back these rows vanish with it — keeps the "no orphan link rows"
        // invariant.
        for link in &input.tickets {
            let ticket_event = TaskTicketRepo::new(&tx).insert(&task.id, link)?;
            events.push(ticket_event);
        }

        tx.commit().context("commit transaction")?;
        Ok((task, events))
    })();

    let (task, events) = match persist_result {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "task persist failed — rolling back disk worktrees");
            rollback_disk(&created);
            return Err(e);
        }
    };

    // Seed the shared context sidecar + task-root CLAUDE.md mirror. At
    // this point every worktree exists on disk and its `task_worktrees`
    // row is committed, so `refresh_task_context` can compose from DB
    // state. Non-fatal — a bad fs write here shouldn't fail task create
    // (which already succeeded).
    if let Err(e) = crate::services::task_context::refresh_task_context(db, &task.id) {
        tracing::warn!(
            task = %task.id,
            error = %e,
            "task_create: refresh_task_context failed (non-fatal)"
        );
    }

    Ok(CreateTaskOutput {
        task,
        worktrees: worktrees_out,
        events,
    })
}

fn rollback_disk(created: &[RollbackEntry]) {
    for entry in created.iter().rev() {
        // Remove warm-env links (symlinks + APFS clones) BEFORE
        // `git worktree remove`. Force-remove still tears down the
        // worktree tree itself, but clones would block a non-forced
        // removal path, and we want to leave the main checkout's
        // symlink targets untouched — `remove_links` unlinks without
        // following symlinks.
        if !entry.linked_paths.is_empty() {
            remove_links(&entry.worktree_path, &entry.linked_paths);
        }
        if let Err(e) = worktree_remove(&entry.repo_path, &entry.worktree_path, true) {
            tracing::warn!(
                path = %entry.worktree_path.display(),
                error = %e,
                "rollback: worktree_remove failed"
            );
        }
    }
}

/// A task branch that `cleanup_task` refused to delete because it had
/// commits not reachable from its `base_branch`. Surfaced to the
/// frontend so the user can decide whether to merge/rebase/force-delete
/// themselves — we'd rather leak a ref than silently destroy work.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PreservedBranch {
    pub project_id: String,
    pub project_name: String,
    pub branch: String,
    pub base_branch: String,
    pub repo_path: String,
}

/// Return value of `cleanup_task`. Events fan out through the existing
/// db_event bus; `preserved_branches` is a side-channel so the frontend
/// can toast ("kept feature/pro-2406 in 2 repos — has uncommitted work").
#[derive(Debug, Default)]
pub struct CleanupReport {
    pub events: Vec<DbEvent>,
    pub preserved_branches: Vec<PreservedBranch>,
}

/// Cleanup: remove all worktrees for a task, delete each task branch
/// IFF it's fully merged into its base branch, and delete the task row.
///
/// Keeping orphan branches around is what caused phantom "changes" on
/// task recreate with the same ticket slug: the old branch tip pinned
/// at an ancestor commit made `git diff <base>..HEAD` in the new
/// worktree surface all the drift-ahead as if the agent had reverted
/// them. Deleting on cleanup fixes that, while `branch_delete_if_merged`
/// keeps us from clobbering unmerged user work.
pub fn cleanup_task(
    db: &Arc<Mutex<Connection>>,
    task_id: &str,
) -> Result<CleanupReport> {
    // Phase 1: collect worktree + link + branch info (short lock)
    let to_remove = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let worktrees = TaskWorktreeRepo::new(&conn).list_for_task(task_id)?;
        worktrees
            .into_iter()
            .map(|wt| {
                let project = ProjectRepo::new(&conn).get(&wt.project_id)?;
                let (repo_path, project_name) = project
                    .map(|p| (PathBuf::from(p.main_repo_path), p.name))
                    .unwrap_or_default();
                // Re-read the project's current link config; that's the
                // set of paths to unlink in the worktree. If the config
                // changed since task_create, any remaining on-disk
                // links for dropped-from-config paths are still fine —
                // `git worktree remove --force` will take them down.
                let links = ProjectLinkRepo::new(&conn)
                    .list_for_project(&wt.project_id)?
                    .into_iter()
                    .map(|l| PathBuf::from(l.path))
                    .collect::<Vec<_>>();
                Ok((
                    wt.project_id.clone(),
                    project_name,
                    repo_path,
                    PathBuf::from(wt.worktree_path),
                    wt.task_branch,
                    wt.base_branch,
                    links,
                ))
            })
            .collect::<Result<Vec<_>>>()?
    };

    // Phase 2: remove links first (preserves main-checkout targets for
    // symlinks), then worktrees, then the task branch (NO DB lock).
    let mut preserved_branches = Vec::new();
    for (project_id, project_name, repo_path, wt_path, task_branch, base_branch, linked_paths)
        in &to_remove
    {
        if repo_path.as_os_str().is_empty() {
            continue;
        }
        if !linked_paths.is_empty() {
            remove_links(wt_path, linked_paths);
        }
        if let Err(e) = worktree_remove(repo_path, wt_path, true) {
            tracing::warn!(
                path = %wt_path.display(),
                error = %e,
                "cleanup: worktree_remove failed (leaving row to reconcile)"
            );
            // Skip branch delete — git refuses while the worktree is
            // still registered, and we already logged the failure.
            continue;
        }
        match branch_delete_if_merged(repo_path, task_branch, base_branch) {
            Ok(BranchDeleteOutcome::Deleted) => {
                tracing::info!(
                    repo = %repo_path.display(),
                    branch = %task_branch,
                    "cleanup: deleted merged task branch"
                );
            }
            Ok(BranchDeleteOutcome::PreservedHasUniqueCommits) => {
                tracing::warn!(
                    repo = %repo_path.display(),
                    branch = %task_branch,
                    base = %base_branch,
                    "cleanup: kept task branch with unmerged commits"
                );
                preserved_branches.push(PreservedBranch {
                    project_id: project_id.clone(),
                    project_name: project_name.clone(),
                    branch: task_branch.clone(),
                    base_branch: base_branch.clone(),
                    repo_path: repo_path.to_string_lossy().into_owned(),
                });
            }
            Ok(BranchDeleteOutcome::NotFound) => {}
            Err(e) => {
                tracing::warn!(
                    repo = %repo_path.display(),
                    branch = %task_branch,
                    error = %e,
                    "cleanup: branch delete failed (non-fatal)"
                );
            }
        }
    }

    // Phase 2.5: remove the task-root directory (the weft-owned parent
    // of every repo worktree in this task). At this point each worktree
    // has been removed; the only remaining content is our task-root
    // `CLAUDE.md` mirror and possibly an empty `.weft/` dir. Without
    // this step, deleted tasks leak `~/.weft/worktrees/<slug>/` dirs
    // forever. Non-fatal: a missing parent dir (task with no worktrees
    // ever succeeded) is fine.
    if let Some((_, _, _, first_wt_path, _, _, _)) = to_remove.first() {
        if let Some(task_root) = first_wt_path.parent() {
            // Safety: task_root must be under `worktrees_base_dir()`. We
            // derived it from a worktree path we own, so that invariant
            // holds unless the user hand-crafted the task_worktrees row.
            if let Ok(base) = crate::services::worktrees_base_dir() {
                if task_root.starts_with(&base) && task_root != base {
                    if let Err(e) = std::fs::remove_dir_all(task_root) {
                        tracing::warn!(
                            dir = %task_root.display(),
                            error = %e,
                            "cleanup: task-root removal failed (non-fatal)"
                        );
                    }
                }
            }
        }
    }

    // Phase 3: delete task row (short lock). CASCADE drops task_worktrees.
    let event = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        TaskRepo::new(&conn).delete(task_id)?
    };
    Ok(CleanupReport {
        events: vec![event],
        preserved_branches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::{
        NewProject, NewWorkspace, NewWorkspaceRepo, WorkspacesRepo,
    };
    use rusqlite::Connection;
    use std::process::Command;

    fn mk_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/0001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0002_schema.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0003_agent_presets.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0004_task_tickets_and_branch.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0005_project_links.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0006_initial_prompt.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0009_task_context_shared.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0010_task_name_locked_at.sql"))
            .unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn test_fallbacks() -> Arc<Mutex<HashSet<(String, String)>>> {
        Arc::new(Mutex::new(HashSet::new()))
    }

    fn mk_repo(dir: &Path) {
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@test"],
            vec!["config", "user.name", "test"],
            vec!["commit", "--allow-empty", "-m", "initial"],
        ] {
            Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(&args)
                .status()
                .unwrap();
        }
    }

    fn setup_workspace_with_repos(
        db: &Arc<Mutex<Connection>>,
        n: usize,
    ) -> (String, tempfile::TempDir, Vec<tempfile::TempDir>) {
        let workspace_dir = tempfile::tempdir().unwrap();
        let mut repos = Vec::new();
        let conn = db.lock().unwrap();
        let (ws, _) = WorkspacesRepo::new(&conn)
            .insert(NewWorkspace {
                name: "test-ws".into(),
                sort_order: None,
            })
            .unwrap();

        for i in 0..n {
            let repo_dir = tempfile::tempdir().unwrap();
            mk_repo(repo_dir.path());
            let (p, _) = ProjectRepo::new(&conn)
                .insert(NewProject {
                    name: format!("repo{i}"),
                    main_repo_path: repo_dir.path().to_string_lossy().into_owned(),
                    default_branch: "main".into(),
                    color: None,
                })
                .unwrap();
            WorkspaceRepoRepo::new(&conn)
                .insert(NewWorkspaceRepo {
                    workspace_id: ws.id.clone(),
                    project_id: p.id,
                    base_branch: None,
                    sort_order: Some(i as i64),
                })
                .unwrap();
            repos.push(repo_dir);
        }
        (ws.id, workspace_dir, repos)
    }

    #[test]
    fn fanout_creates_worktree_per_repo() {
        let db = mk_db();
        let (ws_id, base_td, _repos) = setup_workspace_with_repos(&db, 3);
        let base = base_td.path().join("wt");

        let out = create_task_with_worktrees(
            &db,
            &base,
            CreateTaskInput {
                workspace_id: Some(ws_id),
                name: "my task".into(),
                agent_preset: None,
                project_ids: vec![],
                base_branches: Default::default(),
                tickets: vec![],
                warm_links: true,
                initial_prompt: None,
            },
            test_fallbacks(),
        )
        .expect("fanout");

        assert_eq!(out.worktrees.len(), 3);
        for w in &out.worktrees {
            assert!(w.worktree_path.exists(), "{}", w.worktree_path.display());
            assert_eq!(w.task_branch, "weft/my-task");
        }
        let conn = db.lock().unwrap();
        let rows = TaskWorktreeRepo::new(&conn).list_for_task(&out.task.id).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.status == "ready"));
        assert_eq!(out.events.len(), 4);
        drop(conn);

        cleanup_task(&db, &out.task.id).expect("cleanup");
        let conn = db.lock().unwrap();
        let rows_after = TaskWorktreeRepo::new(&conn)
            .list_for_task(&out.task.id)
            .unwrap();
        assert!(rows_after.is_empty());
        for w in &out.worktrees {
            assert!(!w.worktree_path.exists(), "should be removed");
        }
    }

    #[test]
    fn fanout_rolls_back_on_failure_mid_way() {
        let db = mk_db();
        let (ws_id, base_td, mut repos) = setup_workspace_with_repos(&db, 3);
        let base = base_td.path().join("wt");

        let bad_repo = repos.remove(1);
        std::fs::remove_dir_all(bad_repo.path().join(".git")).unwrap();

        let err = create_task_with_worktrees(
            &db,
            &base,
            CreateTaskInput {
                workspace_id: Some(ws_id),
                name: "will-fail".into(),
                agent_preset: None,
                project_ids: vec![],
                base_branches: Default::default(),
                tickets: vec![],
                warm_links: true,
                initial_prompt: None,
            },
            test_fallbacks(),
        )
        .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("worktree")
                || err.to_string().to_lowercase().contains("git")
                || err.to_string().to_lowercase().contains("repo"),
            "err should mention worktree/git/repo: {err}"
        );

        let conn = db.lock().unwrap();
        let all_tw = TaskWorktreeRepo::new(&conn).list_all().unwrap();
        assert!(all_tw.is_empty(), "task_worktree rows should be rolled back");

        let task_dir = base.join("will-fail");
        if task_dir.exists() {
            let entries = std::fs::read_dir(&task_dir).unwrap().count();
            assert_eq!(
                entries, 0,
                "rolled-back task dir should be empty, found {entries} entries"
            );
        }
    }

    #[test]
    fn fanout_rejects_empty_workspace() {
        let db = mk_db();
        let ws_id = {
            let conn = db.lock().unwrap();
            let (ws, _) = WorkspacesRepo::new(&conn)
                .insert(NewWorkspace {
                    name: "empty".into(),
                    sort_order: None,
                })
                .unwrap();
            ws.id
        };
        let base = tempfile::tempdir().unwrap();

        let err = create_task_with_worktrees(
            &db,
            base.path(),
            CreateTaskInput {
                workspace_id: Some(ws_id),
                name: "x".into(),
                agent_preset: None,
                project_ids: vec![],
                base_branches: Default::default(),
                tickets: vec![],
                warm_links: true,
                initial_prompt: None,
            },
            test_fallbacks(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("no repos"));
    }

    #[test]
    fn fanout_with_tickets_derives_feature_branch_and_persists_links() {
        use crate::db::repo::TaskTicketRepo;
        use crate::integrations::TicketLink;

        let db = mk_db();
        let (ws_id, base_td, _repos) = setup_workspace_with_repos(&db, 2);
        let base = base_td.path().join("wt");

        let tickets = vec![
            TicketLink {
                provider: "linear".into(),
                external_id: "ABC-123".into(),
                url: "https://linear.app/x/issue/ABC-123".into(),
            },
            TicketLink {
                provider: "linear".into(),
                external_id: "ABC-124".into(),
                url: "https://linear.app/x/issue/ABC-124".into(),
            },
        ];

        let out = create_task_with_worktrees(
            &db,
            &base,
            CreateTaskInput {
                workspace_id: Some(ws_id),
                // Name is still required but the slug should come from
                // ticket IDs (with shared team-prefix dedupe), NOT the name.
                name: "some freeform description".into(),
                agent_preset: None,
                project_ids: vec![],
                base_branches: Default::default(),
                tickets: tickets.clone(),
                warm_links: true,
                initial_prompt: None,
            },
            test_fallbacks(),
        )
        .expect("fanout");

        // Shared "abc-" prefix → dedupe → `abc-123-124`.
        assert_eq!(out.task.slug, "abc-123-124");
        assert_eq!(out.task.branch_name, "feature/abc-123-124");
        assert_eq!(out.worktrees.len(), 2);
        for w in &out.worktrees {
            assert_eq!(w.task_branch, "feature/abc-123-124");
        }

        // Ticket rows landed in the same tx.
        let conn = db.lock().unwrap();
        let rows = TaskTicketRepo::new(&conn)
            .list_for_task(&out.task.id)
            .unwrap();
        assert_eq!(rows.len(), 2);
        let ids: Vec<&str> = rows.iter().map(|r| r.external_id.as_str()).collect();
        assert!(ids.contains(&"ABC-123"));
        assert!(ids.contains(&"ABC-124"));
        // Events: 1 task + 2 worktrees + 2 tickets = 5.
        assert_eq!(out.events.len(), 5);
    }
}
