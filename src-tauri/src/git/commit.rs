//! Commit and discard ops for a single worktree.
//!
//! Intentionally narrow:
//! - `commit_all` stages everything (tracked + untracked) and commits. The
//!   "git add -p + selective commit" UX is outside v0.2.
//! - `discard_all` reverts tracked files and removes untracked — destructive,
//!   callers must confirm with the user.
//!
//! Both functions shell out to `git`, matching the rest of the codebase.

use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;

/// Stage everything and create a commit. Returns the new commit SHA.
///
/// Fails (clean — no commit created) if:
/// - The worktree has nothing to commit (empty `git status`)
/// - `git commit` fails for any reason (hooks rejecting, etc.)
pub fn commit_all(worktree: &Path, message: &str) -> Result<String> {
    if message.trim().is_empty() {
        return Err(anyhow!("commit message is empty"));
    }

    // Short-circuit if nothing to commit. This is cheaper than letting
    // `git commit` fail and gives a friendlier error.
    let status_out = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["status", "--porcelain=v1"])
        .output()
        .context("git status")?;
    if !status_out.status.success() {
        return Err(anyhow!(
            "git status failed: {}",
            String::from_utf8_lossy(&status_out.stderr)
        ));
    }
    if status_out.stdout.is_empty() {
        return Err(anyhow!("nothing to commit"));
    }

    let add = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["add", "-A"])
        .output()
        .context("git add")?;
    if !add.status.success() {
        return Err(anyhow!(
            "git add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        ));
    }

    let commit = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["commit", "-m", message])
        .output()
        .context("git commit")?;
    if !commit.status.success() {
        return Err(anyhow!(
            "git commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        ));
    }

    let sha_out = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .context("git rev-parse")?;
    Ok(String::from_utf8_lossy(&sha_out.stdout).trim().to_string())
}

/// Revert tracked files + remove untracked. Destructive; caller must confirm.
pub fn discard_all(worktree: &Path) -> Result<()> {
    // Reset tracked files to HEAD.
    let reset = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["reset", "--hard", "HEAD"])
        .output()
        .context("git reset --hard")?;
    if !reset.status.success() {
        return Err(anyhow!(
            "git reset failed: {}",
            String::from_utf8_lossy(&reset.stderr)
        ));
    }

    // Remove untracked files + directories. -d includes dirs, -f is force.
    // NOT using -x so gitignored files (node_modules, target) are preserved.
    let clean = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["clean", "-fd"])
        .output()
        .context("git clean -fd")?;
    if !clean.status.success() {
        return Err(anyhow!(
            "git clean failed: {}",
            String::from_utf8_lossy(&clean.stderr)
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
        ] {
            Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(&args)
                .status()
                .unwrap();
        }
        std::fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .status()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["commit", "-m", "initial"])
            .status()
            .unwrap();
        dir
    }

    #[test]
    fn commit_all_stages_and_commits_mixed_changes() {
        let repo = mk_repo();
        std::fs::write(repo.path().join("a.txt"), "hello\nmodified\n").unwrap();
        std::fs::write(repo.path().join("new.txt"), "brand new\n").unwrap();

        let sha = commit_all(repo.path(), "test: mixed").unwrap();
        assert_eq!(sha.len(), 40, "SHA looks like a commit sha: {sha}");

        // Working tree should be clean now.
        let status = Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["status", "--porcelain=v1"])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&status.stdout).trim().is_empty());
    }

    #[test]
    fn commit_all_fails_when_nothing_to_commit() {
        let repo = mk_repo();
        let err = commit_all(repo.path(), "test: empty").unwrap_err();
        assert!(err.to_string().contains("nothing to commit"));
    }

    #[test]
    fn commit_all_rejects_empty_message() {
        let repo = mk_repo();
        std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
        let err = commit_all(repo.path(), "   ").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn discard_all_reverts_tracked_and_removes_untracked() {
        let repo = mk_repo();
        // Modify tracked, add untracked.
        std::fs::write(repo.path().join("a.txt"), "munged\n").unwrap();
        std::fs::write(repo.path().join("new.txt"), "junk\n").unwrap();

        discard_all(repo.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
            "hello\n"
        );
        assert!(!repo.path().join("new.txt").exists());
    }
}
