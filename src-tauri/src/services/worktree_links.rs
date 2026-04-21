//! Warm-worktree link application.
//!
//! Materializes `project_links` rows into a freshly-created worktree.
//! Called from `task_create` phase 2.5 (after `git worktree add`, before
//! persist) and from `task_repos::add_repo_to_task`.
//!
//! Contract: **atomic per worktree.** If any single link fails to apply,
//! every link this call already created is unwound before the `Err`
//! returns. The caller (task_create) records the successfully-linked
//! paths on its `RollbackEntry` so a *later*-phase failure can tear them
//! down before `git worktree remove --force` (which would otherwise
//! refuse when clones contain untracked files).
//!
//! Method semantics:
//! - `symlink`: `symlink(source, target)`. Cheap. Writes reach main.
//! - `clone`: `cp -c -R source target.weft-tmp && rename(target.weft-tmp, target)`.
//!   Atomic at rename time. Falls back to symlink on ENOTSUP/EXDEV
//!   (non-APFS / cross-fs) and records the fallback in AppState's
//!   `clone_fallbacks` set so subsequent tasks skip the retry.
//!
//! Info/exclude: successful links are appended to the **worktree-specific**
//! `info/exclude` (via `git rev-parse --git-dir`, NOT `--git-common-dir`
//! which is what `task_tickets.rs` uses deliberately for the opposite
//! scope — `.weft/` should be universally ignored, warm-env links should
//! only be ignored in the worktree they live in).

use crate::db::repo::{LinkType, ProjectLinkRow};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

/// One materialized link on disk. Returned so the caller can record
/// them for rollback (remove before `git worktree remove`).
#[derive(Debug, Clone)]
pub struct AppliedLink {
    pub path: PathBuf,
    pub effective_type: LinkType,
}

/// Result breakdown for observability. Not every element corresponds to
/// an on-disk artifact (skipped entries don't).
#[derive(Debug, Clone, Copy)]
pub enum Outcome {
    Applied,
    /// Source didn't exist — e.g. main checkout hasn't run `bun install`
    /// yet. Not an error; logged as info.
    SkippedSourceMissing,
    /// Target already existed (e.g. a previous `apply_links` partially
    /// ran and created this one). Idempotent.
    SkippedTargetPresent,
}

/// Shape returned to the caller for logging. Does not block the return.
#[derive(Debug, Clone)]
pub struct LinkReport {
    pub path: PathBuf,
    pub outcome: Outcome,
    pub effective_type: Option<LinkType>,
}

/// `apply_links` — all-or-nothing. Returns the applied links (those the
/// caller must remove at rollback / cleanup) plus a report.
pub fn apply_links(
    project_id: &str,
    main_path: &Path,
    worktree_path: &Path,
    links: &[ProjectLinkRow],
    fallbacks: Arc<Mutex<HashSet<(String, String)>>>,
) -> Result<(Vec<AppliedLink>, Vec<LinkReport>)> {
    // Sweep any leftover `.weft-tmp` from a previously-interrupted clone
    // in this worktree. Not an error if none.
    sweep_tmp_files(worktree_path).ok();

    let mut applied: Vec<AppliedLink> = Vec::new();
    let mut reports: Vec<LinkReport> = Vec::new();

    for link in links {
        let rel = Path::new(&link.path);
        let src = main_path.join(rel);
        let tgt = worktree_path.join(rel);

        // Source missing → skip silently.
        if !src.exists() {
            reports.push(LinkReport {
                path: rel.to_path_buf(),
                outcome: Outcome::SkippedSourceMissing,
                effective_type: None,
            });
            tracing::info!(
                target: "weft::links",
                path = %link.path,
                "source missing; skipping link (warm up main checkout to enable)"
            );
            continue;
        }

        // Target already present — idempotent skip.
        if tgt.exists() || tgt.symlink_metadata().is_ok() {
            reports.push(LinkReport {
                path: rel.to_path_buf(),
                outcome: Outcome::SkippedTargetPresent,
                effective_type: None,
            });
            continue;
        }

        // Ensure parent dir exists inside the worktree (for nested paths).
        if let Some(parent) = tgt.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create parent of {}", tgt.display()))?;
            }
        }

        // Resolve effective type (apply fallback cache).
        let requested = link.link_type;
        let fallback_key = (project_id.to_string(), link.path.clone());
        let mut effective = requested;
        if requested == LinkType::Clone
            && fallbacks.lock().unwrap().contains(&fallback_key)
        {
            effective = LinkType::Symlink;
        }

        // Apply. On failure, unwind what we've done so far before returning Err.
        let result = match effective {
            LinkType::Symlink => apply_symlink(&src, &tgt),
            LinkType::Clone => {
                match apply_clone(&src, &tgt) {
                    Ok(()) => Ok(()),
                    Err(e) if is_unsupported_fs(&e) => {
                        // Remember for this session, then fall back.
                        fallbacks.lock().unwrap().insert(fallback_key);
                        effective = LinkType::Symlink;
                        tracing::warn!(
                            target: "weft::links",
                            project_id = project_id,
                            path = %link.path,
                            "clonefile unsupported on this volume; falling back to symlink (this session only)"
                        );
                        apply_symlink(&src, &tgt)
                    }
                    Err(e) => Err(e),
                }
            }
        };

        if let Err(e) = result {
            unwind(&applied, worktree_path);
            return Err(e.context(format!("apply_links: failed at path {}", link.path)));
        }

        applied.push(AppliedLink {
            path: rel.to_path_buf(),
            effective_type: effective,
        });
        reports.push(LinkReport {
            path: rel.to_path_buf(),
            outcome: Outcome::Applied,
            effective_type: Some(effective),
        });
    }

    // Append successful paths to the worktree-specific info/exclude.
    if !applied.is_empty() {
        if let Err(e) = append_to_worktree_exclude(worktree_path, &applied) {
            tracing::warn!(
                target: "weft::links",
                error = %e,
                worktree = %worktree_path.display(),
                "info/exclude update failed (links applied regardless)"
            );
        }
    }

    Ok((applied, reports))
}

/// Defensive baseline excludes written to every weft worktree's
/// **common** `info/exclude` on creation. These cover the canonical UX
/// papercut: a repo whose committed `.gitignore` uses `node_modules/`
/// (trailing slash, directory-only) won't match weft's warm-up symlink,
/// so `git status` lists it as untracked. Pre-writing the bare path
/// catches that case AND covers repos that simply forgot to gitignore
/// it.
///
/// Why common-dir, not per-worktree: git's `info/exclude` lookup only
/// reads `$GIT_COMMON_DIR/info/exclude`; the per-worktree
/// `worktrees/<name>/info/exclude` isn't consulted, so writing there is
/// silently ineffective (empirically verified against git 2.50 — the
/// file exists but `git check-ignore -v` never cites it). The trade-off
/// is the main checkout's exclude grows too, which is fine for
/// universally-ignored paths like `/node_modules`; `append_if_missing`
/// keeps it idempotent on repeat launches.
const BASELINE_EXCLUDES: &[&str] = &["/node_modules"];

pub fn write_baseline_excludes(worktree_path: &Path) {
    if let Err(e) = append_baseline_to_common_exclude(worktree_path) {
        tracing::warn!(
            target: "weft::links",
            error = %e,
            worktree = %worktree_path.display(),
            "baseline info/exclude write failed (non-fatal)"
        );
    }
}

fn append_baseline_to_common_exclude(worktree_path: &Path) -> Result<()> {
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .context("git rev-parse --git-common-dir")?;
    if !out.status.success() {
        anyhow::bail!(
            "git rev-parse --git-common-dir failed in {}",
            worktree_path.display()
        );
    }
    let git_dir = String::from_utf8(out.stdout)?.trim().to_string();
    let git_dir_path = if Path::new(&git_dir).is_absolute() {
        PathBuf::from(&git_dir)
    } else {
        worktree_path.join(&git_dir)
    };
    let info_dir = git_dir_path.join("info");
    fs::create_dir_all(&info_dir).with_context(|| format!("create {}", info_dir.display()))?;
    let exclude_path = info_dir.join("exclude");

    let existing = fs::read_to_string(&exclude_path).unwrap_or_default();
    let existing_lines: HashSet<&str> = existing.lines().map(str::trim).collect();

    let missing: Vec<&str> = BASELINE_EXCLUDES
        .iter()
        .copied()
        .filter(|p| !existing_lines.contains(*p))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude_path)
        .with_context(|| format!("open {}", exclude_path.display()))?;
    writeln!(file, "# weft baseline excludes")?;
    for pattern in missing {
        writeln!(file, "{pattern}")?;
    }
    Ok(())
}

/// Drop a `.weft/project-id` breadcrumb into the worktree so shell
/// wrappers (e.g. `contrib/install-lock/_common.sh`) can discover the
/// project id without the user plumbing an env var through their
/// agent config. Idempotent, failure non-fatal — call this right
/// after `git worktree add` succeeds.
pub fn write_project_id_breadcrumb(worktree_path: &Path, project_id: &str) {
    let weft_dir = worktree_path.join(".weft");
    if let Err(e) = fs::create_dir_all(&weft_dir) {
        tracing::warn!(
            target: "weft::links",
            error = %e,
            "create .weft/ failed (non-fatal)"
        );
        return;
    }
    if let Err(e) = fs::write(weft_dir.join("project-id"), project_id) {
        tracing::warn!(
            target: "weft::links",
            error = %e,
            "write .weft/project-id failed (non-fatal)"
        );
    }
}

/// Remove every previously-applied link on disk. Called both from
/// `apply_links` on mid-sequence failure and from task cleanup /
/// `rollback_disk` before `git worktree remove`.
pub fn unwind(applied: &[AppliedLink], worktree_path: &Path) {
    for link in applied {
        let target = worktree_path.join(&link.path);
        let _ = remove_any(&target);
    }
}

/// Public helper for cleanup paths that don't have the full
/// `AppliedLink` slice (e.g. `cleanup_task` reading from the DB
/// configuration). Best-effort per path.
pub fn remove_links(worktree_path: &Path, paths: &[PathBuf]) {
    for p in paths {
        let target = worktree_path.join(p);
        let _ = remove_any(&target);
    }
}

fn remove_any(target: &Path) -> std::io::Result<()> {
    // A symlink: `remove_file` unlinks it (doesn't follow). A real
    // directory (clone): `remove_dir_all`.
    match target.symlink_metadata() {
        Ok(meta) if meta.file_type().is_symlink() => fs::remove_file(target),
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(target),
        Ok(_) => fs::remove_file(target),
        Err(_) => Ok(()),
    }
}

fn apply_symlink(src: &Path, tgt: &Path) -> Result<()> {
    symlink(src, tgt).with_context(|| {
        format!("symlink {} -> {}", tgt.display(), src.display())
    })
}

/// `cp -c -R` handles clonefile + recursion + xattrs + permissions.
/// Writing to `.weft-tmp` then renaming gives us atomic semantics.
fn apply_clone(src: &Path, tgt: &Path) -> Result<()> {
    let tmp_name = format!(
        "{}.weft-tmp",
        tgt.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("clone")
    );
    let tmp = tgt.with_file_name(tmp_name);

    // Clean any stale tmp from previous failures.
    let _ = remove_any(&tmp);

    let out = Command::new("cp")
        .arg("-c")
        .arg("-R")
        .arg(src)
        .arg(&tmp)
        .output()
        .context("spawn cp -c -R")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        // Clean the partial temp before bubbling up.
        let _ = remove_any(&tmp);
        anyhow::bail!(
            "cp -c -R {} {} failed: {}",
            src.display(),
            tmp.display(),
            stderr.trim()
        );
    }

    fs::rename(&tmp, tgt).with_context(|| {
        format!("rename {} -> {}", tmp.display(), tgt.display())
    })?;

    Ok(())
}

/// Detect "not supported on this filesystem" — cp's stderr on non-APFS
/// sources contains "Operation not supported" or "cross-device link".
fn is_unsupported_fs(e: &anyhow::Error) -> bool {
    let msg = format!("{e:?}").to_lowercase();
    msg.contains("operation not supported")
        || msg.contains("cross-device")
        || msg.contains("exdev")
        || msg.contains("notsup")
}

fn sweep_tmp_files(worktree_path: &Path) -> Result<()> {
    let mut stack = vec![worktree_path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".weft-tmp") {
                    let _ = remove_any(&path);
                    continue;
                }
            }
            // Skip .git and anything symlinked — we only sweep real
            // subtrees in the worktree.
            if let Ok(meta) = path.symlink_metadata() {
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name == ".git" {
                        continue;
                    }
                    stack.push(path);
                }
            }
        }
    }
    Ok(())
}

/// Worktree-specific info/exclude. Crucially uses `--git-dir` (returns
/// `<common>/worktrees/<name>`), **not** `--git-common-dir` (returns the
/// shared `<common>` used by every worktree + main checkout). The
/// latter is what `services/task_tickets.rs` uses deliberately for the
/// `.weft/` universal-ignore; here we want the opposite scope.
fn append_to_worktree_exclude(
    worktree_path: &Path,
    applied: &[AppliedLink],
) -> Result<()> {
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["rev-parse", "--git-dir"])
        .output()
        .context("git rev-parse --git-dir")?;
    if !out.status.success() {
        anyhow::bail!("git rev-parse --git-dir failed in {}", worktree_path.display());
    }
    let git_dir = String::from_utf8(out.stdout)?.trim().to_string();
    let git_dir_path = if Path::new(&git_dir).is_absolute() {
        PathBuf::from(&git_dir)
    } else {
        worktree_path.join(&git_dir)
    };
    let info_dir = git_dir_path.join("info");
    fs::create_dir_all(&info_dir).with_context(|| format!("create {}", info_dir.display()))?;
    let exclude_path = info_dir.join("exclude");

    let existing = fs::read_to_string(&exclude_path).unwrap_or_default();
    let existing_lines: HashSet<&str> = existing.lines().map(str::trim).collect();

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude_path)
        .with_context(|| format!("open {}", exclude_path.display()))?;

    let mut wrote_header = false;
    for link in applied {
        let pattern = format!("/{}", link.path.display());
        if existing_lines.contains(pattern.as_str()) {
            continue;
        }
        if !wrote_header {
            writeln!(file, "# weft warm-worktree links")?;
            wrote_header = true;
        }
        writeln!(file, "{pattern}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    fn empty_fallbacks() -> Arc<Mutex<HashSet<(String, String)>>> {
        Arc::new(Mutex::new(HashSet::new()))
    }

    fn touch(dir: &Path, rel: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, b"").unwrap();
    }

    fn mkdir(dir: &Path, rel: &str) {
        fs::create_dir_all(dir.join(rel)).unwrap();
    }

    #[test]
    fn symlink_applies_and_reports_applied() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&wt).unwrap();
        mkdir(&main, "node_modules");
        touch(&main, "node_modules/pkg/index.js");

        let links = vec![ProjectLinkRow {
            project_id: "p1".into(),
            path: "node_modules".into(),
            link_type: LinkType::Symlink,
        }];

        let (applied, reports) =
            apply_links("p1", &main, &wt, &links, empty_fallbacks()).unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].effective_type, LinkType::Symlink);
        assert!(matches!(reports[0].outcome, Outcome::Applied));
        let link_meta = wt.join("node_modules").symlink_metadata().unwrap();
        assert!(link_meta.file_type().is_symlink());
    }

    #[test]
    fn source_missing_is_skipped_not_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&wt).unwrap();
        // Intentionally don't create node_modules in main.

        let links = vec![ProjectLinkRow {
            project_id: "p1".into(),
            path: "node_modules".into(),
            link_type: LinkType::Symlink,
        }];

        let (applied, reports) =
            apply_links("p1", &main, &wt, &links, empty_fallbacks()).unwrap();
        assert!(applied.is_empty());
        assert!(matches!(reports[0].outcome, Outcome::SkippedSourceMissing));
        assert!(!wt.join("node_modules").exists());
    }

    #[test]
    fn target_already_present_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&wt).unwrap();
        mkdir(&main, "node_modules");
        mkdir(&wt, "node_modules"); // pre-existing

        let links = vec![ProjectLinkRow {
            project_id: "p1".into(),
            path: "node_modules".into(),
            link_type: LinkType::Symlink,
        }];

        let (applied, reports) =
            apply_links("p1", &main, &wt, &links, empty_fallbacks()).unwrap();
        assert!(applied.is_empty());
        assert!(matches!(reports[0].outcome, Outcome::SkippedTargetPresent));
        // Target unchanged (still a plain directory, not a symlink).
        let meta = wt.join("node_modules").symlink_metadata().unwrap();
        assert!(!meta.file_type().is_symlink());
    }

    #[test]
    fn unwind_removes_symlinks_created_before_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&wt).unwrap();
        mkdir(&main, "node_modules");
        touch(&main, ".env");
        // `nested/deep` source exists but we'll make the wt target
        // parent read-only to force a failure on the 3rd link.
        mkdir(&main, "nested/deep");
        mkdir(&wt, "nested");
        let mut perms = fs::metadata(wt.join("nested")).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o555); // read + execute only; no write
        fs::set_permissions(wt.join("nested"), perms).unwrap();

        let links = vec![
            ProjectLinkRow {
                project_id: "p1".into(),
                path: "node_modules".into(),
                link_type: LinkType::Symlink,
            },
            ProjectLinkRow {
                project_id: "p1".into(),
                path: ".env".into(),
                link_type: LinkType::Symlink,
            },
            ProjectLinkRow {
                project_id: "p1".into(),
                path: "nested/deep".into(),
                link_type: LinkType::Symlink,
            },
        ];

        let result =
            apply_links("p1", &main, &wt, &links, empty_fallbacks());

        // Restore perms so tempdir cleanup works.
        let mut perms = fs::metadata(wt.join("nested")).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(wt.join("nested"), perms).unwrap();

        assert!(result.is_err(), "expected failure on readonly parent");
        // Unwind contract: no surviving links from the first two.
        assert!(!wt.join("node_modules").symlink_metadata().is_ok(),
            "first link should have been unwound");
        assert!(!wt.join(".env").symlink_metadata().is_ok(),
            "second link should have been unwound");
    }

    #[test]
    fn reapply_is_idempotent_via_target_present_skip() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&wt).unwrap();
        touch(&main, ".env");

        let links = vec![ProjectLinkRow {
            project_id: "p1".into(),
            path: ".env".into(),
            link_type: LinkType::Symlink,
        }];

        apply_links("p1", &main, &wt, &links, empty_fallbacks()).unwrap();
        let (applied_again, reports_again) =
            apply_links("p1", &main, &wt, &links, empty_fallbacks()).unwrap();
        assert!(applied_again.is_empty());
        assert!(matches!(reports_again[0].outcome, Outcome::SkippedTargetPresent));
    }

    #[test]
    fn remove_links_unwinds_symlinks_and_clones() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&wt).unwrap();
        touch(&main, ".env");
        // Simulate a clone (plain directory) at wt/build
        mkdir(&wt, "build");
        touch(&wt, "build/out.js");
        // Symlink wt/.env -> main/.env
        symlink(main.join(".env"), wt.join(".env")).unwrap();

        remove_links(
            &wt,
            &[PathBuf::from(".env"), PathBuf::from("build")],
        );

        assert!(!wt.join(".env").symlink_metadata().is_ok());
        assert!(!wt.join("build").exists());
    }
}
