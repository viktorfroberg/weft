use crate::db::repo::{NewProject, ProjectRepo};
use crate::model::Project;
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn projects_list(state: State<'_, AppState>) -> Result<Vec<Project>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    ProjectRepo::new(&conn).list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn project_create(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewProject,
) -> Result<Project, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (project, event) = ProjectRepo::new(&conn)
        .insert(input)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(project)
}

#[tauri::command]
pub fn project_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let event = ProjectRepo::new(&conn)
        .delete(&id)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}

#[tauri::command]
pub fn project_set_color(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    color: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let event = ProjectRepo::new(&conn)
        .set_color(&id, color.as_deref())
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}

#[tauri::command]
pub fn project_rename(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let event = ProjectRepo::new(&conn)
        .rename(&id, trimmed)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}
