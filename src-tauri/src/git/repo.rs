use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;

/// True if the given path is the root (or inside) a git working tree.
pub fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve the default branch of a repo. Tries `origin/HEAD` first (works on
/// clones that were made against a remote), falls back to the currently
/// checked-out branch if the remote symref is missing.
pub fn default_branch(repo_path: &Path) -> Result<String> {
    // Try `git symbolic-ref refs/remotes/origin/HEAD` first.
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .output()
        .context("run git symbolic-ref")?;

    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        // Output shape: "origin/main" — strip the remote prefix.
        if let Some(branch) = s.strip_prefix("origin/") {
            return Ok(branch.to_string());
        }
        return Ok(s);
    }

    // Fall back to current HEAD.
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("run git rev-parse HEAD")?;

    if !out.status.success() {
        return Err(anyhow!(
            "could not determine default branch: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Create a throwaway git repo in a tempdir with one initial commit on
    /// the given branch. Returns the tempdir path.
    fn mk_repo(branch: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path();

        for args in [
            vec!["init", "-b", branch],
            vec!["config", "user.email", "test@test"],
            vec!["config", "user.name", "test"],
            vec!["commit", "--allow-empty", "-m", "initial"],
        ] {
            let ok = Command::new("git")
                .arg("-C")
                .arg(path)
                .args(&args)
                .status()
                .expect("git")
                .success();
            assert!(ok, "git {} failed in tempdir", args.join(" "));
        }

        dir
    }

    #[test]
    fn is_git_repo_true_for_repo() {
        let dir = mk_repo("main");
        assert!(is_git_repo(dir.path()));
    }

    #[test]
    fn is_git_repo_false_for_plain_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_git_repo(dir.path()));
    }

    #[test]
    fn default_branch_reads_head_when_no_origin() {
        let dir = mk_repo("trunk");
        assert_eq!(default_branch(dir.path()).unwrap(), "trunk");
    }
}
