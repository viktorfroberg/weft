use crate::db::repo::{NewWorkspace, NewWorkspaceRepo, WorkspaceRepoRepo, WorkspacesRepo};
use crate::model::{Workspace, WorkspaceRepo};
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn workspaces_list(state: State<'_, AppState>) -> Result<Vec<Workspace>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    WorkspacesRepo::new(&conn).list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn workspace_create(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewWorkspace,
) -> Result<Workspace, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (ws, event) = WorkspacesRepo::new(&conn)
        .insert(input)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(ws)
}

#[tauri::command]
pub fn workspace_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let event = WorkspacesRepo::new(&conn)
        .delete(&id)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}

#[tauri::command]
pub fn workspace_repos_list(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<WorkspaceRepo>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    WorkspaceRepoRepo::new(&conn)
        .list_for_workspace(&workspace_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn workspace_add_repo(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewWorkspaceRepo,
) -> Result<WorkspaceRepo, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (row, event) = WorkspaceRepoRepo::new(&conn)
        .insert(input)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(row)
}

#[tauri::command]
pub fn workspace_remove_repo(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
    project_id: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let event = WorkspaceRepoRepo::new(&conn)
        .delete(&workspace_id, &project_id)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}
