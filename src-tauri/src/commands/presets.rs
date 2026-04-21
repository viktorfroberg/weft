use crate::db::repo::{AgentPreset, PresetRepo};
use crate::AppState;
use tauri::State;

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
