use crate::db::repo::{NewTask, TaskRepo, TaskTicketRepo, TaskTicketRow, TaskWorktreeRepo, TaskWorktreeRow};
use crate::integrations::TicketLink;
use crate::model::Task;
use crate::services::{
    open_in_editor::open_task_in_editor,
    task_context::{read_task_context, write_task_context},
    task_create::{
        cleanup_task, create_task_with_worktrees, CreateTaskInput, CreatedWorktree,
        PreservedBranch,
    },
    task_repos::{add_repo_to_task, remove_repo_from_task},
    task_tickets,
    worktrees_base_dir,
};
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn tasks_list(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<Task>, String> {
    crate::timed!("tasks_list");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    TaskRepo::new(&conn)
        .list_for_workspace(&workspace_id)
        .map_err(|e| e.to_string())
}

/// v1.0.7: flat task list across all repo groups. Drives the
/// sidebar (grouped client-side by status) and Home dashboard.
#[tauri::command]
pub fn tasks_list_all(state: State<'_, AppState>) -> Result<Vec<Task>, String> {
    crate::timed!("tasks_list_all");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    TaskRepo::new(&conn).list_all().map_err(|e| e.to_string())
}

/// Called by the frontend after it has written the task's `initial_prompt`
/// into the spawned agent's PTY. Sets `initial_prompt_consumed_at` so a
/// relaunch / tab-reload doesn't inject the same user message again.
/// No-ops when already consumed (idempotent via the `IS NULL` guard in
/// the repo method). Emits a `task` update event so TaskView UI can clear
/// its pending-prompt indicator.
#[tauri::command]
pub fn task_consume_initial_prompt(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<(), String> {
    crate::timed!("task_consume_initial_prompt");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let event = TaskRepo::new(&conn)
        .mark_initial_prompt_consumed(&task_id)
        .map_err(|e| e.to_string())?;
    drop(conn);
    super::emit_event(&app, event);
    Ok(())
}

/// v1.0.7: derive the list of project ids a task currently touches,
/// for rendering repo color-dots in the sidebar. Pulled from
/// `task_worktrees` rather than a workspace link — repo membership is
/// dynamic (see `+ Add repo` / `× remove`).
#[tauri::command]
pub fn task_project_ids(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<String>, String> {
    crate::timed!("task_project_ids");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let rows = TaskWorktreeRepo::new(&conn)
        .list_for_task(&task_id)
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|w| w.project_id).collect())
}

#[tauri::command]
pub fn task_worktrees_list(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<TaskWorktreeRow>, String> {
    crate::timed!("task_worktrees_list");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    TaskWorktreeRepo::new(&conn)
        .list_for_task(&task_id)
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct TaskCreateResponse {
    pub task: Task,
    pub worktrees: Vec<WorktreeSummary>,
}

#[derive(Serialize)]
pub struct WorktreeSummary {
    pub project_id: String,
    pub project_name: String,
    pub worktree_path: String,
    pub task_branch: String,
    pub base_branch: String,
}

impl From<CreatedWorktree> for WorktreeSummary {
    fn from(w: CreatedWorktree) -> Self {
        Self {
            project_id: w.project_id,
            project_name: w.project_name,
            worktree_path: w.worktree_path.to_string_lossy().into_owned(),
            task_branch: w.task_branch,
            base_branch: w.base_branch,
        }
    }
}

/// Atomic workspace → task → N worktrees fan-out. Replaces the old Phase 2
/// "insert task row only" command.
///
/// `tickets` is optional; when provided, the task slug/branch derive from
/// ticket IDs and the links are persisted in the same transaction.
/// v1.0.7 args:
///   - `project_ids`: explicit repo selection. Takes precedence over
///     the repo set implied by `input.workspace_id`.
///   - `base_branches`: optional per-project base-branch overrides.
#[tauri::command]
pub async fn task_create(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewTask,
    tickets: Option<Vec<TicketLink>>,
    warm_links: Option<bool>,
    project_ids: Option<Vec<String>>,
    base_branches: Option<std::collections::HashMap<String, String>>,
    initial_prompt: Option<String>,
    auto_rename: Option<bool>,
) -> Result<TaskCreateResponse, String> {
    crate::timed!("task_create");
    tracing::info!(
        target: "weft::svc",
        workspace_id = ?input.workspace_id,
        name = %input.name,
        tickets = tickets.as_ref().map(|t| t.len()).unwrap_or(0),
        project_ids = project_ids.as_ref().map(|p| p.len()).unwrap_or(0),
        warm_links = warm_links.unwrap_or(true),
        has_initial_prompt = initial_prompt.is_some(),
        "task_create",
    );
    let base = worktrees_base_dir().map_err(|e| e.to_string())?;
    let tickets = tickets.unwrap_or_default();
    let had_tickets = !tickets.is_empty();
    let out = create_task_with_worktrees(
        &state.db,
        &base,
        CreateTaskInput {
            workspace_id: input.workspace_id,
            name: input.name,
            agent_preset: input.agent_preset,
            project_ids: project_ids.unwrap_or_default(),
            base_branches: base_branches.unwrap_or_default(),
            tickets,
            warm_links: warm_links.unwrap_or(true),
            initial_prompt,
        },
        std::sync::Arc::clone(&state.clone_fallbacks),
    )
    .map_err(|e| e.to_string())?;

    for event in out.events {
        super::emit_event(&app, event);
    }

    let _ = had_tickets;
    let response = TaskCreateResponse {
        task: out.task,
        worktrees: out.worktrees.into_iter().map(Into::into).collect(),
    };

    // Background auto-rename via Haiku. Gated by the frontend's
    // `prefs.autoRenameTasks` (default true); also skipped when the
    // user set `WEFT_DISABLE_AUTO_RENAME=1`. Fails closed — if claude
    // isn't installed or the call times out, the heuristic short name
    // the UI already picked stays put.
    let env_disabled = std::env::var("WEFT_DISABLE_AUTO_RENAME")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if auto_rename.unwrap_or(true) && !env_disabled {
        crate::services::task_naming::spawn_auto_rename(
            std::sync::Arc::clone(&state.db),
            response.task.id.clone(),
            app.clone(),
        );
    }

    Ok(response)
}

/// Shape returned to the frontend after deleting a task. Non-empty
/// `preserved_branches` tells the TaskView delete handler to toast —
/// we refused to delete those branches because they had commits the
/// user's `base_branch` didn't contain (i.e. real work not yet merged),
/// and silently destroying them would be a nasty surprise.
#[derive(Debug, Serialize)]
pub struct TaskDeleteResponse {
    pub preserved_branches: Vec<PreservedBranch>,
}

/// User-initiated rename via the task header's pencil icon. Sets
/// `tasks.name_locked_at` so any in-flight background LLM rename
/// leaves the row alone — user intent wins over automation.
/// Returns the new name after trim so the frontend can reflect
/// exactly what landed on disk.
#[tauri::command]
pub fn task_rename(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    name: String,
) -> Result<String, String> {
    crate::timed!("task_rename");
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("name cannot be empty".to_string());
    }
    let event = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        TaskRepo::new(&conn)
            .rename(&task_id, &trimmed)
            .map_err(|e| e.to_string())?
    };
    super::emit_event(&app, event);
    Ok(trimmed)
}

#[tauri::command]
pub fn task_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<TaskDeleteResponse, String> {
    // Kill any live PTY sessions for this task FIRST. Otherwise the agent
    // process keeps running with its CWD about to be yanked out from under
    // it — guaranteed chaos.
    let killed = state.terminals.kill_by_task(&id);
    if killed > 0 {
        tracing::info!(task = %id, sessions = killed, "killed PTY sessions before delete");
    }

    let report = cleanup_task(&state.db, &id).map_err(|e| e.to_string())?;
    for event in report.events {
        super::emit_event(&app, event);
    }
    Ok(TaskDeleteResponse {
        preserved_branches: report.preserved_branches,
    })
}

/// Attach a project to an existing task — creates a worktree and a
/// task_worktrees row. The counterpart to removing via
/// `task_remove_repo`; together these let the user reshape a task's repo
/// membership after it's been created.
#[tauri::command]
pub fn task_add_repo(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    project_id: String,
    base_branch: Option<String>,
) -> Result<WorktreeSummary, String> {
    let base = worktrees_base_dir().map_err(|e| e.to_string())?;
    let out = add_repo_to_task(
        &state.db,
        &base,
        &task_id,
        &project_id,
        base_branch,
        std::sync::Arc::clone(&state.clone_fallbacks),
    )
        .map_err(|e| e.to_string())?;

    super::emit_event(&app, out.event);

    // Return shape matches CreatedWorktree so the frontend can display it
    // with the same component as task_create's fan-out.
    let project_name = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::db::repo::ProjectRepo::new(&conn)
            .get(&project_id)
            .ok()
            .flatten()
            .map(|p| p.name)
            .unwrap_or_else(|| "repo".into())
    };

    Ok(WorktreeSummary {
        project_id,
        project_name,
        worktree_path: out.worktree_path.to_string_lossy().into_owned(),
        task_branch: out.task_branch,
        base_branch: out.base_branch,
    })
}

#[tauri::command]
pub fn task_remove_repo(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    project_id: String,
) -> Result<(), String> {
    let event = remove_repo_from_task(&state.db, &task_id, &project_id)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}

/// Generate a `.code-workspace` for this task and launch the given editor
/// (default "code"). Multi-repo tasks open as one editor window with every
/// ready worktree as a root folder.
#[tauri::command]
pub fn task_open_in_editor(
    state: State<'_, AppState>,
    task_id: String,
    editor: Option<String>,
) -> Result<String, String> {
    let base = worktrees_base_dir().map_err(|e| e.to_string())?;
    let editor = editor.unwrap_or_else(|| "code".to_string());
    let out = open_task_in_editor(&state.db, &base, &task_id, &editor)
        .map_err(|e| e.to_string())?;
    Ok(out.workspace_file.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Ticket linking (v1.0.2)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn task_tickets_list(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<TaskTicketRow>, String> {
    crate::timed!("task_tickets_list");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    TaskTicketRepo::new(&conn)
        .list_for_task(&task_id)
        .map_err(|e| e.to_string())
}

/// All ticket↔task links across every task for the given provider.
/// Home backlog strip uses this to jump straight to an existing task
/// instead of starting a new one when the user clicks a ticket card.
#[tauri::command]
pub fn task_tickets_by_provider(
    state: State<'_, AppState>,
    provider: String,
) -> Result<Vec<TaskTicketRow>, String> {
    crate::timed!("task_tickets_by_provider");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    TaskTicketRepo::new(&conn)
        .list_for_provider(&provider)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn task_link_ticket(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    link: TicketLink,
) -> Result<(), String> {
    let event = task_tickets::link_ticket(&state.db, &task_id, link)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}

// ---------------------------------------------------------------------------
// Context file (user-authored agent hints)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn task_context_get(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<String, String> {
    crate::timed!("task_context_get");
    read_task_context(&state.db, &task_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn task_context_set(
    state: State<'_, AppState>,
    task_id: String,
    content: String,
) -> Result<(), String> {
    crate::timed!("task_context_set");
    write_task_context(&state.db, &task_id, &content).map_err(|e| e.to_string())
}

/// Manual "Refresh titles" button. Re-hits each linked ticket's
/// provider and updates the cached `title`/`status` columns, then
/// regenerates the context sidecar so the CLAUDE.md mirror picks up
/// the new titles. Emits a synthetic task-level event so the
/// ContextDialog's live-refresh observer refetches.
#[tauri::command]
pub async fn task_refresh_ticket_titles(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<usize, String> {
    crate::timed!("task_refresh_ticket_titles");
    let updated = task_tickets::refresh_ticket_titles(
        std::sync::Arc::clone(&state.db),
        task_id.clone(),
    )
    .await
    .map_err(|e| e.to_string())?;
    // Synthetic "task updated" bump so the UI bridge invalidates task
    // queries and the ContextDialog refetches its preview.
    super::emit_event(
        &app,
        crate::db::events::DbEvent::update(crate::db::events::Entity::Task, task_id),
    );
    Ok(updated)
}

/// Fire-once refresh triggered when a task route becomes active. Backend
/// filters by `title_fetched_at` staleness (default 24h) and short-
/// circuits with zero Linear calls if nothing is stale — safe to call on
/// every route change. Emits the task-update event only when at least
/// one ticket actually refreshed.
#[tauri::command]
pub async fn task_refresh_ticket_titles_if_stale(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<usize, String> {
    crate::timed!("task_refresh_ticket_titles_if_stale");
    let updated = task_tickets::refresh_stale_ticket_titles(
        std::sync::Arc::clone(&state.db),
        task_id.clone(),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    if updated > 0 {
        super::emit_event(
            &app,
            crate::db::events::DbEvent::update(crate::db::events::Entity::Task, task_id),
        );
    }
    Ok(updated)
}

#[tauri::command]
pub fn task_unlink_ticket(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    provider: String,
    external_id: String,
) -> Result<(), String> {
    let event = task_tickets::unlink_ticket(&state.db, &task_id, &provider, &external_id)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}
