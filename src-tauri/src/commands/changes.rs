use crate::db::repo::TaskWorktreeRepo;
use crate::git::{commit_all, discard_all, file_sides, task_changes, FileChange};
use crate::AppState;
use serde::Serialize;
use std::path::Path;
use tauri::State;

#[derive(Serialize)]
pub struct RepoChanges {
    pub project_id: String,
    pub worktree_path: String,
    pub base_branch: String,
    pub task_branch: String,
    pub changes: Vec<FileChange>,
    /// Non-null when the status call failed (e.g. worktree missing). UI
    /// surfaces the error per-repo so one broken worktree doesn't gut the
    /// whole view.
    pub error: Option<String>,
}

/// Aggregate all changed files across a task's worktrees. Returns one
/// `RepoChanges` per `task_worktrees` row — errors are captured per-repo,
/// not propagated, so the UI can render partial results.
#[tauri::command]
pub fn task_changes_by_repo(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<RepoChanges>, String> {
    crate::timed!("task_changes_by_repo");
    // Read the worktree list under a short-held lock, then release
    // before shelling out to git. Holding the DB mutex across N git
    // status calls would block every other command — and with the
    // frontend's query bus that means `tasks_list`, `task_worktrees_list`,
    // and `integration_list` all queue behind a slow `git status`.
    let rows = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        TaskWorktreeRepo::new(&conn)
            .list_for_task(&task_id)
            .map_err(|e| e.to_string())?
    };
    tracing::debug!(
        target: "weft::cmd",
        task_id = %task_id,
        worktrees = rows.len(),
        "task_changes_by_repo: got worktrees, shelling out to git (unlocked)",
    );

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let path = Path::new(&r.worktree_path);
        let git_started = std::time::Instant::now();
        let (changes, error) = match task_changes(path, &r.base_branch) {
            Ok(list) => (list, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        let elapsed = git_started.elapsed();
        if elapsed.as_millis() >= 250 {
            tracing::info!(
                target: "weft::git",
                project_id = %r.project_id,
                worktree = %r.worktree_path,
                elapsed = ?elapsed,
                "task_changes: slow git call",
            );
        }
        out.push(RepoChanges {
            project_id: r.project_id,
            worktree_path: r.worktree_path,
            base_branch: r.base_branch,
            task_branch: r.task_branch,
            changes,
            error,
        });
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct FileSides {
    pub base: Option<String>,
    pub current: Option<String>,
    pub base_branch: String,
    pub path: String,
}

/// Return both sides of a file for a Monaco diff view.
#[tauri::command]
pub fn worktree_file_sides(
    worktree_path: String,
    base_branch: String,
    file: String,
) -> Result<FileSides, String> {
    let (base, current) =
        file_sides(Path::new(&worktree_path), &base_branch, &file).map_err(|e| e.to_string())?;
    Ok(FileSides {
        base,
        current,
        base_branch,
        path: file,
    })
}

#[derive(Serialize)]
pub struct CommitResult {
    pub project_id: String,
    pub ok: bool,
    pub sha: Option<String>,
    pub error: Option<String>,
}

/// Commit everything in a single worktree. Caller is a per-repo button or
/// the `commit_all_repos` fan-out below.
#[tauri::command]
pub fn worktree_commit(
    project_id: String,
    worktree_path: String,
    message: String,
) -> CommitResult {
    match commit_all(Path::new(&worktree_path), &message) {
        Ok(sha) => CommitResult {
            project_id,
            ok: true,
            sha: Some(sha),
            error: None,
        },
        Err(e) => CommitResult {
            project_id,
            ok: false,
            sha: None,
            error: Some(e.to_string()),
        },
    }
}

/// Revert tracked + remove untracked in a single worktree. Destructive.
#[tauri::command]
pub fn worktree_discard(worktree_path: String) -> Result<(), String> {
    discard_all(Path::new(&worktree_path)).map_err(|e| e.to_string())
}

/// Commit every worktree of a task that has changes. Returns one result per
/// repo with the same shape as `worktree_commit` — the UI surfaces partial
/// success (some commits ok, others failed) explicitly.
#[tauri::command]
pub fn task_commit_all(
    state: State<'_, AppState>,
    task_id: String,
    message: String,
) -> Result<Vec<CommitResult>, String> {
    let rows = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        TaskWorktreeRepo::new(&conn)
            .list_for_task(&task_id)
            .map_err(|e| e.to_string())?
    };

    let results: Vec<CommitResult> = rows
        .into_iter()
        .map(|r| match commit_all(Path::new(&r.worktree_path), &message) {
            Ok(sha) => CommitResult {
                project_id: r.project_id,
                ok: true,
                sha: Some(sha),
                error: None,
            },
            Err(e) => {
                let msg = e.to_string();
                // "nothing to commit" is expected for clean repos — treat as
                // success-shaped (ok=false but not an error the user needs
                // to act on). Keep the string so the UI can show a subdued
                // "clean" state.
                CommitResult {
                    project_id: r.project_id,
                    ok: false,
                    sha: None,
                    error: Some(msg),
                }
            }
        })
        .collect();

    Ok(results)
}
