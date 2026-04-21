use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Options controlling how a new worktree is created.
#[derive(Debug, Clone)]
pub struct WorktreeOptions {
    /// Source repo (where we run `git worktree add`).
    pub repo_path: PathBuf,
    /// Target worktree directory. Must not already exist.
    pub target_path: PathBuf,
    /// Branch to check out in the worktree. Created if missing.
    pub branch: String,
    /// Branch/commit to branch from if `branch` doesn't exist yet.
    pub base_branch: String,
}

/// Create a git worktree. Idempotent only in the "target exists and is a
/// registered worktree" sense — this function does NOT recover that case; it
/// returns an error. Recovery is Phase 4's reconciliation job.
pub fn worktree_add(opts: &WorktreeOptions) -> Result<()> {
    if opts.target_path.exists() {
        return Err(anyhow!(
            "target worktree path already exists: {}",
            opts.target_path.display()
        ));
    }

    // If the branch already exists (e.g. left over from a prior task with
    // the same slug), check it out; otherwise create from base.
    let branch_exists = Command::new("git")
        .arg("-C")
        .arg(&opts.repo_path)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(&opts.branch)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&opts.repo_path).arg("worktree").arg("add");

    if branch_exists {
        cmd.arg(&opts.target_path).arg(&opts.branch);
    } else {
        cmd.arg("-b")
            .arg(&opts.branch)
            .arg(&opts.target_path)
            .arg(&opts.base_branch);
    }

    let out = cmd.output().context("spawn git worktree add")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git worktree add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// Outcome of `branch_delete_if_merged`. Callers use this to decide
/// whether to surface a warning to the user ("hey, we kept your branch
/// because it had unique commits").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchDeleteOutcome {
    /// Branch didn't exist (already gone, or never created — e.g. the
    /// worktree was marked failed before `git worktree add` ran). Treat
    /// as success; nothing to warn about.
    NotFound,
    /// Branch was at-or-behind `base_branch` and had no unique commits,
    /// so we removed it cleanly. This is the happy path when the user
    /// deletes a task whose agent never committed anything.
    Deleted,
    /// Branch had commits reachable from its tip but NOT from
    /// `base_branch` — that's user work we refuse to destroy. The
    /// branch stays; caller surfaces a warning so the user can decide
    /// whether to merge, rebase, or manually `git branch -D`.
    PreservedHasUniqueCommits,
}

/// Delete `branch` in `repo_path` IFF it has no commits that `base_branch`
/// doesn't already contain. Symmetric with `worktree_remove`: we yank the
/// ref only if doing so is lossless. Called from task cleanup so re-using
/// the same ticket slug later doesn't pick up a stale branch tip and
/// surface phantom "changes" against the fresh `base_branch`.
///
/// Never deletes an unmerged branch — we'd rather leak a ref than silently
/// destroy uncommitted-to-main work. The caller logs/toasts the
/// `PreservedHasUniqueCommits` outcome.
pub fn branch_delete_if_merged(
    repo_path: &Path,
    branch: &str,
    base_branch: &str,
) -> Result<BranchDeleteOutcome> {
    // Does the branch exist at all?
    let exists = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(branch)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !exists {
        return Ok(BranchDeleteOutcome::NotFound);
    }

    // `git merge-base --is-ancestor <branch> <base>` exits 0 iff every
    // commit reachable from <branch> is also reachable from <base>.
    // That's exactly "safe to delete" — no lost commits.
    let is_ancestor = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["merge-base", "--is-ancestor", branch, base_branch])
        .status()
        .context("spawn git merge-base --is-ancestor")?;
    if !is_ancestor.success() {
        return Ok(BranchDeleteOutcome::PreservedHasUniqueCommits);
    }

    // Safe delete. Use `-D` (force) rather than `-d` because `-d` refuses
    // when the branch isn't merged into the CURRENT HEAD — but we just
    // verified it's an ancestor of `base_branch`, which is a stronger
    // guarantee than "merged into whatever is currently checked out".
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["branch", "-D", branch])
        .output()
        .context("spawn git branch -D")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git branch -D {branch} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(BranchDeleteOutcome::Deleted)
}

/// Remove a worktree. Pass `force = true` to bypass the "uncommitted changes"
/// safety — intended for cleanup paths where the caller has already warned
/// the user.
pub fn worktree_remove(repo_path: &Path, worktree_path: &Path, force: bool) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(repo_path)
        .arg("worktree")
        .arg("remove")
        .arg(worktree_path);
    if force {
        cmd.arg("--force");
    }

    let out = cmd.output().context("spawn git worktree remove")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git worktree remove failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn mk_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@test"],
            vec!["config", "user.name", "test"],
            vec!["commit", "--allow-empty", "-m", "initial"],
        ] {
            Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(&args)
                .status()
                .unwrap();
        }
        dir
    }

    #[test]
    fn worktree_add_creates_directory_and_branch() {
        let repo = mk_repo();
        let target = repo.path().parent().unwrap().join("weft-wt-test");
        // Ensure no leftover
        let _ = std::fs::remove_dir_all(&target);

        let opts = WorktreeOptions {
            repo_path: repo.path().to_path_buf(),
            target_path: target.clone(),
            branch: "weft/slug-test".to_string(),
            base_branch: "main".to_string(),
        };

        worktree_add(&opts).expect("worktree add");
        assert!(target.exists(), "target worktree dir should exist");

        // Branch should be registered
        let out = Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["branch", "--list", "weft/slug-test"])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("weft/slug-test"));

        // Cleanup
        worktree_remove(repo.path(), &target, false).expect("remove");
        assert!(!target.exists());
    }

    fn run(dir: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap()
    }

    #[test]
    fn branch_delete_removes_merged_branch() {
        let repo = mk_repo();
        run(repo.path(), &["branch", "weft/merged"]);
        let out = branch_delete_if_merged(repo.path(), "weft/merged", "main").unwrap();
        assert_eq!(out, BranchDeleteOutcome::Deleted);
        let list = run(repo.path(), &["branch", "--list", "weft/merged"]);
        assert!(String::from_utf8_lossy(&list.stdout).trim().is_empty());
    }

    #[test]
    fn branch_delete_preserves_branch_with_unique_commits() {
        let repo = mk_repo();
        run(repo.path(), &["branch", "weft/diverged"]);
        run(repo.path(), &["checkout", "weft/diverged"]);
        run(repo.path(), &["commit", "--allow-empty", "-m", "unique"]);
        run(repo.path(), &["checkout", "main"]);

        let out =
            branch_delete_if_merged(repo.path(), "weft/diverged", "main").unwrap();
        assert_eq!(out, BranchDeleteOutcome::PreservedHasUniqueCommits);
        let list = run(repo.path(), &["branch", "--list", "weft/diverged"]);
        assert!(String::from_utf8_lossy(&list.stdout).contains("weft/diverged"));
    }

    #[test]
    fn branch_delete_reports_not_found_for_missing_branch() {
        let repo = mk_repo();
        let out =
            branch_delete_if_merged(repo.path(), "weft/never-existed", "main").unwrap();
        assert_eq!(out, BranchDeleteOutcome::NotFound);
    }

    #[test]
    fn worktree_add_fails_if_target_exists() {
        let repo = mk_repo();
        let target = repo.path().parent().unwrap().join("weft-wt-exists");
        std::fs::create_dir_all(&target).unwrap();

        let opts = WorktreeOptions {
            repo_path: repo.path().to_path_buf(),
            target_path: target.clone(),
            branch: "weft/x".to_string(),
            base_branch: "main".to_string(),
        };
        let err = worktree_add(&opts).unwrap_err();
        assert!(err.to_string().contains("already exists"));

        let _ = std::fs::remove_dir_all(&target);
    }
}
