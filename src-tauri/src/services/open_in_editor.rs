//! "Open in Editor" for multi-repo tasks.
//!
//! VS Code and Cursor both support multi-root workspaces via a
//! `.code-workspace` JSON file. We generate one that lists every ready
//! worktree of the task as a folder, write it next to the task's worktree
//! tree, and shell out to the chosen editor.
//!
//! Default editor is `code`. Settings can override to `cursor`, `zed`,
//! etc. — any CLI that accepts a single file path and opens it as a
//! workspace.

use crate::db::repo::{ProjectRepo, TaskRepo, TaskWorktreeRepo};
use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Serialize)]
struct CodeWorkspaceFile {
    folders: Vec<Folder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct Folder {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

pub struct OpenInEditorOutput {
    pub workspace_file: PathBuf,
    pub editor: String,
}

/// Generate the workspace file and launch the editor. `editor_cmd` is the
/// name of a binary on PATH (`code`, `cursor`, `zed`, etc.).
pub fn open_task_in_editor(
    db: &Arc<Mutex<Connection>>,
    worktrees_base: &Path,
    task_id: &str,
    editor_cmd: &str,
) -> Result<OpenInEditorOutput> {
    // Collect ready worktrees + their project names so we can label
    // folders humanely in the editor.
    let (task_slug, folders): (String, Vec<Folder>) = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let task = TaskRepo::new(&conn)
            .get(task_id)?
            .ok_or_else(|| anyhow!("task {task_id} not found"))?;

        let worktrees = TaskWorktreeRepo::new(&conn).list_for_task(&task.id)?;
        let mut folders = Vec::new();
        for w in worktrees {
            if w.status != "ready" {
                continue;
            }
            let project_name = ProjectRepo::new(&conn)
                .get(&w.project_id)?
                .map(|p| p.name)
                .unwrap_or_else(|| "repo".into());
            folders.push(Folder {
                path: w.worktree_path,
                name: Some(project_name),
            });
        }
        (task.slug, folders)
    };

    if folders.is_empty() {
        return Err(anyhow!(
            "task has no ready worktrees — nothing to open"
        ));
    }

    // Write the workspace file next to the task dir so it's stable across
    // relaunches and the editor "recent" list shows it.
    let task_dir = worktrees_base.join(&task_slug);
    std::fs::create_dir_all(&task_dir)
        .with_context(|| format!("create {}", task_dir.display()))?;
    let workspace_file = task_dir.join("weft.code-workspace");

    let payload = CodeWorkspaceFile {
        folders,
        settings: None,
    };
    let json = serde_json::to_string_pretty(&payload)
        .context("serialize code-workspace json")?;
    std::fs::write(&workspace_file, json)
        .with_context(|| format!("write {}", workspace_file.display()))?;

    // Launch. We swallow stdout/stderr because editors typically
    // fork-detach; we just want the exit status to indicate "did the CLI
    // exist at all".
    let status = Command::new(editor_cmd)
        .arg(&workspace_file)
        .status()
        .with_context(|| format!("spawn {editor_cmd}"))?;
    if !status.success() {
        return Err(anyhow!(
            "{editor_cmd} exited with non-zero status. Is it installed and on PATH? (VS Code: 'Install \"code\" command in PATH'; Cursor: similar.)"
        ));
    }

    Ok(OpenInEditorOutput {
        workspace_file,
        editor: editor_cmd.to_string(),
    })
}
