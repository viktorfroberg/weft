//! `project_links_reapply` — walk every active worktree for a project
//! and re-materialize its current link config. Used from the Settings
//! Projects tab after the user edits the list (preset apply, path add/
//! remove, etc.) so existing in-flight worktrees pick up the change
//! without recreating the task.
//!
//! Idempotent: `apply_links` skips any path whose target already
//! exists in the worktree, so re-running it on a settled worktree is a
//! no-op. New paths land, removed paths are not removed here (that
//! would require knowing the *previous* list; doing that would be
//! wrong if the user hand-unlinked one in a worktree).
//!
//! For targeted un-apply use `remove_links` directly from a command.

use crate::db::repo::{ProjectLinkRepo, ProjectRepo, TaskWorktreeRepo};
use crate::services::worktree_links::apply_links;
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct ReapplyReport {
    pub worktrees_touched: usize,
    pub worktrees_failed: Vec<String>,
}

pub fn reapply_for_project(
    db: &Arc<Mutex<Connection>>,
    project_id: &str,
    clone_fallbacks: Arc<Mutex<HashSet<(String, String)>>>,
) -> Result<ReapplyReport> {
    // Phase 1: short DB read — pull the project, the link config, and
    // every ready worktree currently attached.
    let (project_path, links, worktrees) = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let project = ProjectRepo::new(&conn)
            .get(project_id)?
            .ok_or_else(|| anyhow!("project {project_id} missing"))?;
        let links = ProjectLinkRepo::new(&conn).list_for_project(project_id)?;
        // `list_all` for simplicity — filter to ready-status rows for
        // this project. Phase-1 short-lock; we do disk work unlocked.
        let wts = TaskWorktreeRepo::new(&conn)
            .list_all()?
            .into_iter()
            .filter(|w| w.project_id == project_id && w.status == "ready")
            .map(|w| PathBuf::from(w.worktree_path))
            .collect::<Vec<_>>();
        (PathBuf::from(project.main_repo_path), links, wts)
    };

    if links.is_empty() {
        // Nothing to apply. Still succeed — the UI's "refresh all"
        // button acts as "sync links with current config", which is a
        // no-op when the config is empty.
        return Ok(ReapplyReport {
            worktrees_touched: 0,
            worktrees_failed: Vec::new(),
        });
    }

    // Phase 2: disk work — apply into each worktree. One worktree's
    // failure doesn't block the others; failures go into `failed`.
    let mut touched = 0;
    let mut failed: Vec<String> = Vec::new();
    for wt in &worktrees {
        match apply_links(
            project_id,
            &project_path,
            wt,
            &links,
            Arc::clone(&clone_fallbacks),
        ) {
            Ok(_) => {
                touched += 1;
            }
            Err(e) => {
                tracing::warn!(
                    target: "weft::links",
                    project_id = project_id,
                    worktree = %wt.display(),
                    error = %e,
                    "reapply failed for worktree"
                );
                failed.push(wt.display().to_string());
            }
        }
    }

    Ok(ReapplyReport {
        worktrees_touched: touched,
        worktrees_failed: failed,
    })
}

/// `warm_up_main_checkout` — shells the project's declared install
/// command in the main-checkout directory so subsequent task_create
/// calls can symlink into a populated `node_modules` / `.venv` / etc.
///
/// Chooses the command from the lockfiles present:
///   - `bun.lockb`     → `bun install`
///   - `pnpm-lock.yaml`→ `pnpm install`
///   - `yarn.lock`     → `yarn install`
///   - `package-lock.json` / `package.json` → `npm install`
///   - `Cargo.toml`    → `cargo fetch`
///   - `pyproject.toml` + venv → `pip install -e .` (best-effort)
///
/// Returns the command string actually run + captured stdout/stderr.
/// Runs synchronously — the UI shows "Warming up…" until it returns.
/// Typical wall-clock: 5-90 seconds depending on project size.
pub struct WarmupResult {
    pub command: String,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn warm_up_main_checkout(main_path: &Path) -> Result<WarmupResult> {
    let (cmd, args): (&str, Vec<&str>) = if main_path.join("bun.lockb").exists() {
        ("bun", vec!["install"])
    } else if main_path.join("pnpm-lock.yaml").exists() {
        ("pnpm", vec!["install"])
    } else if main_path.join("yarn.lock").exists() {
        ("yarn", vec!["install"])
    } else if main_path.join("package-lock.json").exists()
        || main_path.join("package.json").exists()
    {
        ("npm", vec!["install"])
    } else if main_path.join("Cargo.toml").exists() {
        ("cargo", vec!["fetch"])
    } else if main_path.join(".venv").exists() && main_path.join("pyproject.toml").exists() {
        ("pip", vec!["install", "-e", "."])
    } else {
        return Err(anyhow!(
            "no recognized lockfile in {}. Supported: bun.lockb, pnpm-lock.yaml, yarn.lock, package-lock.json, package.json, Cargo.toml, pyproject.toml (with .venv)",
            main_path.display()
        ));
    };

    let output = std::process::Command::new(cmd)
        .args(&args)
        .current_dir(main_path)
        .output()
        .map_err(|e| anyhow!("spawn {cmd}: {e}"))?;

    Ok(WarmupResult {
        command: format!("{cmd} {}", args.join(" ")),
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
