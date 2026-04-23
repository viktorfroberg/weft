//! Application services — orchestrate repos + filesystem + git to implement
//! features that span multiple primitives.
//!
//! Rule: services don't know about Tauri. Commands call services and emit
//! resulting `DbEvent`s.

pub mod agent_launch;
pub mod fonts;
pub mod open_in_editor;
pub mod project_link_presets;
pub mod project_links_health;
pub mod project_links_reapply;
pub mod reconcile;
pub mod task_context;
pub mod task_create;
pub mod task_naming;
pub mod task_repos;
pub mod task_tickets;
pub mod worktree_links;

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Base directory where all task worktrees live. Matches the old weft README
/// convention (`~/.weft/worktrees/<task-slug>/<project-name>/`) so the path
/// pattern is familiar to Viktor from the prior prototype.
pub fn worktrees_base_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("no home dir")?;
    let dir = home.join(".weft").join("worktrees");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}
