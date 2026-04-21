use crate::db::repo::{AgentPreset, NewAgentPreset, PresetPatch, PresetRepo};
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn presets_list(state: State<'_, AppState>) -> Result<Vec<AgentPreset>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    PresetRepo::new(&conn).list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn preset_default(
    state: State<'_, AppState>,
) -> Result<Option<AgentPreset>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    PresetRepo::new(&conn)
        .get_default()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn preset_create(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewAgentPreset,
) -> Result<AgentPreset, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (preset, event) = PresetRepo::new(&conn)
        .insert(input)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(preset)
}

#[tauri::command]
pub fn preset_update(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: PresetPatch,
) -> Result<AgentPreset, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (preset, event) = PresetRepo::new(&conn)
        .update(&id, patch)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(preset)
}

#[tauri::command]
pub fn preset_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let event = PresetRepo::new(&conn)
        .delete(&id)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}

#[tauri::command]
pub fn preset_set_default(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let event = PresetRepo::new(&conn)
        .set_default(&id)
        .map_err(|e| e.to_string())?;
    super::emit_event(&app, event);
    Ok(())
}
