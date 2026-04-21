use crate::db::data_dir;
use crate::services::worktrees_base_dir;
use crate::AppState;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub hook_port: Option<u16>,
    pub hook_manifest_path: String,
    pub data_dir: String,
    pub worktrees_dir: String,
    pub db_path: String,
    pub default_shell: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> Result<AppInfo, String> {
    let dd = data_dir().map_err(|e| e.to_string())?;
    let wt = worktrees_base_dir().map_err(|e| e.to_string())?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let hook_port = *state.hook_port.lock().map_err(|e| e.to_string())?;
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        hook_port,
        hook_manifest_path: dd.join("hooks.json").to_string_lossy().into_owned(),
        data_dir: dd.to_string_lossy().into_owned(),
        worktrees_dir: wt.to_string_lossy().into_owned(),
        db_path: dd.join("weft.db").to_string_lossy().into_owned(),
        default_shell: shell,
    })
}
