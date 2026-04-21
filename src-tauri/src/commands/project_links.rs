//! Project warm-worktree link management (v1.0.6).
//!
//! CRUD on `project_links` rows + preset-apply + repo list of all
//! available presets. The frontend's ProjectsTab calls these.

use crate::db::repo::{LinkType, ProjectLinkInput, ProjectLinkRepo, ProjectLinkRow, ProjectRepo};
use crate::services::project_link_presets::{
    detect_preset_from_path, list_descriptors, preset_inputs, PresetDescriptor,
};
use crate::services::project_links_reapply::{
    reapply_for_project, warm_up_main_checkout,
};
use crate::AppState;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn project_links_list(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<ProjectLinkRow>, String> {
    crate::timed!("project_links_list");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    ProjectLinkRepo::new(&conn)
        .list_for_project(&project_id)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
pub struct LinkInputDto {
    pub path: String,
    pub link_type: LinkType,
}

impl From<LinkInputDto> for ProjectLinkInput {
    fn from(v: LinkInputDto) -> Self {
        ProjectLinkInput {
            path: v.path,
            link_type: v.link_type,
        }
    }
}

/// Full replace. Caller sends the complete desired list; backend drops
/// the old rows and inserts the new ones in a single transaction.
#[tauri::command]
pub fn project_links_set(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    links: Vec<LinkInputDto>,
) -> Result<(), String> {
    crate::timed!("project_links_set");
    let inputs: Vec<ProjectLinkInput> = links.into_iter().map(Into::into).collect();
    let event = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        ProjectLinkRepo::new(&conn)
            .replace(&project_id, &inputs)
            .map_err(|e| e.to_string())?
    };
    super::emit_event(&app, event);
    Ok(())
}

/// Apply a named preset (`node` / `next` / `rust` / `python`). Custom
/// is not a valid preset — the frontend calls `project_links_set`
/// directly for user-custom lists.
#[tauri::command]
pub fn project_links_preset_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    preset_id: String,
) -> Result<(), String> {
    crate::timed!("project_links_preset_apply");
    let inputs = preset_inputs(&preset_id)
        .ok_or_else(|| format!("unknown preset: {preset_id}"))
        .map_err(|e| e.to_string())?;
    let event = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        ProjectLinkRepo::new(&conn)
            .replace(&project_id, &inputs)
            .map_err(|e| e.to_string())?
    };
    super::emit_event(&app, event);
    Ok(())
}

/// List all available presets for the UI segmented control.
#[tauri::command]
pub fn project_links_presets_list() -> Vec<PresetDescriptor> {
    list_descriptors()
}

/// Pre-flight detection for AddProjectDialog — scan a path for
/// lockfiles / config files and suggest a preset. Returns `None` when
/// the repo doesn't match any known stack.
#[tauri::command]
pub fn project_links_detect_preset(path: String) -> Result<Option<String>, String> {
    let p = std::path::Path::new(&path);
    if !p.is_dir() {
        return Err(anyhow!("not a directory: {path}").to_string());
    }
    Ok(detect_preset_from_path(p).map(String::from))
}

#[derive(Serialize)]
pub struct ReapplyResponse {
    pub worktrees_touched: usize,
    pub worktrees_failed: Vec<String>,
}

/// Re-materialize a project's links into every active worktree. Used
/// after the user edits a project's link config under Settings →
/// Projects, so in-flight tasks pick up the change.
#[tauri::command]
pub fn project_links_reapply(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<ReapplyResponse, String> {
    crate::timed!("project_links_reapply");
    let report = reapply_for_project(
        &state.db,
        &project_id,
        Arc::clone(&state.clone_fallbacks),
    )
    .map_err(|e| e.to_string())?;
    Ok(ReapplyResponse {
        worktrees_touched: report.worktrees_touched,
        worktrees_failed: report.worktrees_failed,
    })
}

#[derive(Serialize)]
pub struct WarmupResponse {
    pub command: String,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Run the project's install command in its main checkout so future
/// tasks can symlink into a populated `node_modules` / `target` / etc.
/// Synchronous — the Settings UI shows "Warming up…" until it returns.
#[tauri::command]
pub fn project_links_warm_up_main(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<WarmupResponse, String> {
    crate::timed!("project_links_warm_up_main");
    let main_path = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        ProjectRepo::new(&conn)
            .get(&project_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("project {project_id} not found"))?
            .main_repo_path
    };
    let res =
        warm_up_main_checkout(std::path::Path::new(&main_path)).map_err(|e| e.to_string())?;
    Ok(WarmupResponse {
        command: res.command,
        success: res.success,
        stdout: res.stdout,
        stderr: res.stderr,
    })
}

use crate::services::project_links_health::{
    health_for_project, summarize, HealthSummary, LinkHealth,
};

#[derive(Serialize)]
pub struct HealthResponse {
    pub rows: Vec<LinkHealth>,
    pub summary: HealthSummary,
}

/// Stat-check every configured link across every active worktree for
/// a project. Returns per-link status + an aggregate summary. Cheap
/// enough to call on Settings page render.
#[tauri::command]
pub fn project_links_health(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<HealthResponse, String> {
    crate::timed!("project_links_health");
    let rows = health_for_project(&state.db, &project_id).map_err(|e| e.to_string())?;
    let summary = summarize(&rows);
    Ok(HealthResponse { rows, summary })
}
