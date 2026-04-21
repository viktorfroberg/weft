//! Git status / diff operations scoped to a single worktree.
//!
//! "Status" here means "what has this task changed relative to its base
//! branch" — combines committed-but-unpushed deltas with any uncommitted
//! edits. Single-repo; Phase 6 aggregates across worktrees in the service
//! layer.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
    TypeChanged,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub kind: FileChangeKind,
    /// For renames/copies, the pre-change path. None otherwise.
    pub from_path: Option<String>,
}

/// All files this worktree has changed relative to `base_branch`, INCLUDING
/// uncommitted edits. Shape: one entry per file, even if both staged and
/// unstaged changes exist — we collapse the two states because the user
/// just wants to see "what did the agent produce?"
pub fn task_changes(worktree: &Path, base_branch: &str) -> Result<Vec<FileChange>> {
    let mut changes = Vec::new();

    // 1. Committed deltas: `git diff --name-status <base>..HEAD`
    //    Shows files modified in commits since branching off base.
    let committed = run_name_status(worktree, &["diff", "--name-status", &format!("{base_branch}..HEAD")])?;
    changes.extend(committed);

    // 2. Working-tree deltas (staged + unstaged): `git status --porcelain=v1`
    //    Merged into same list so a committed-then-further-edited file
    //    appears once, with kind reflecting whichever is newer (HEAD edit).
    let wd = working_dir_changes(worktree)?;
    for c in wd {
        if !changes.iter().any(|x| x.path == c.path) {
            changes.push(c);
        }
    }

    // Stable order — sort by path so the UI doesn't jitter when refetching.
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

/// Parse `git diff --name-status` output into FileChanges.
fn run_name_status(worktree: &Path, args: &[&str]) -> Result<Vec<FileChange>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(args)
        .output()
        .with_context(|| format!("git {}", args.join(" ")))?;
    if !out.status.success() {
        // If base..HEAD has no diff (brand-new branch, no commits yet), git
        // may exit non-zero with empty stderr in some versions. Treat empty
        // stderr + non-success as "nothing".
        let err = String::from_utf8_lossy(&out.stderr);
        if err.trim().is_empty() {
            return Ok(vec![]);
        }
        return Err(anyhow!("git {} failed: {}", args.join(" "), err));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut changes = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let status = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("").to_string();
        let (kind, from_path) = classify_name_status(status, parts.next());
        if path.is_empty() {
            continue;
        }
        changes.push(FileChange {
            path: from_path.as_ref().cloned().map(|_| parts.last().unwrap_or(&path).to_string()).unwrap_or(path.clone()),
            kind,
            from_path,
        });
    }
    // Fix: my iteration above mangled renames. Reparse cleanly.
    let clean = text
        .lines()
        .filter_map(|line| {
            if line.is_empty() {
                return None;
            }
            let mut it = line.split('\t');
            let s = it.next()?;
            let a = it.next()?.to_string();
            let b = it.next();
            let (kind, from_path, path) = if s.starts_with('R') || s.starts_with('C') {
                let kind = if s.starts_with('R') {
                    FileChangeKind::Renamed
                } else {
                    FileChangeKind::Copied
                };
                let to = b.unwrap_or(&a).to_string();
                (kind, Some(a), to)
            } else {
                let kind = match s.chars().next().unwrap_or(' ') {
                    'A' => FileChangeKind::Added,
                    'M' => FileChangeKind::Modified,
                    'D' => FileChangeKind::Deleted,
                    'T' => FileChangeKind::TypeChanged,
                    'U' => FileChangeKind::Conflicted,
                    _ => FileChangeKind::Other,
                };
                (kind, None, a)
            };
            Some(FileChange {
                path,
                kind,
                from_path,
            })
        })
        .collect();
    // Prefer the clean parse over my first attempt.
    let _ = changes;
    Ok(clean)
}

/// Classify a single-letter or two-letter status code. Used only when
/// parsing working-dir output; name-status uses simpler codes.
fn classify_name_status(code: &str, _extra: Option<&str>) -> (FileChangeKind, Option<String>) {
    match code.chars().next().unwrap_or(' ') {
        'A' => (FileChangeKind::Added, None),
        'M' => (FileChangeKind::Modified, None),
        'D' => (FileChangeKind::Deleted, None),
        'T' => (FileChangeKind::TypeChanged, None),
        'R' => (FileChangeKind::Renamed, None),
        'C' => (FileChangeKind::Copied, None),
        'U' => (FileChangeKind::Conflicted, None),
        _ => (FileChangeKind::Other, None),
    }
}

fn working_dir_changes(worktree: &Path) -> Result<Vec<FileChange>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .context("git status")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git status failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut changes = Vec::new();
    for line in text.lines() {
        if line.len() < 4 {
            continue;
        }
        // Porcelain v1: `XY path` where X is index status, Y is worktree status.
        let xy = &line[..2];
        let path = line[3..].to_string();

        if xy == "??" {
            changes.push(FileChange {
                path,
                kind: FileChangeKind::Untracked,
                from_path: None,
            });
            continue;
        }
        if xy.starts_with("UU") || xy.contains('U') {
            changes.push(FileChange {
                path,
                kind: FileChangeKind::Conflicted,
                from_path: None,
            });
            continue;
        }

        // Prefer the worktree (Y) status for "what did the user/agent just do".
        // Fall back to index (X) if worktree is clean.
        let y = xy.chars().nth(1).unwrap_or(' ');
        let x = xy.chars().next().unwrap_or(' ');
        let primary = if y != ' ' { y } else { x };
        let kind = match primary {
            'A' => FileChangeKind::Added,
            'M' => FileChangeKind::Modified,
            'D' => FileChangeKind::Deleted,
            'T' => FileChangeKind::TypeChanged,
            'R' => FileChangeKind::Renamed,
            'C' => FileChangeKind::Copied,
            _ => FileChangeKind::Other,
        };
        changes.push(FileChange {
            path,
            kind,
            from_path: None,
        });
    }
    Ok(changes)
}

/// Raw unified-diff text for a single file, comparing base_branch → worktree.
/// Includes both committed and uncommitted changes (i.e. "everything this
/// task did to this file"). Returns empty string if unchanged.
pub fn file_diff(worktree: &Path, base_branch: &str, file: &str) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["diff", base_branch])
        .arg("--")
        .arg(file)
        .output()
        .context("git diff")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.trim().is_empty() {
            return Ok(String::new());
        }
        return Err(anyhow!("git diff failed: {}", err));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Return the two sides of a file for a Monaco diff view:
///   (base_content, current_content)
/// Either side can be empty (added/deleted files). Binary files return
/// `None` for the relevant side — callers should fall back to "binary file"
/// placeholders in the UI.
pub fn file_sides(
    worktree: &Path,
    base_branch: &str,
    file: &str,
) -> Result<(Option<String>, Option<String>)> {
    let base = read_blob_at(worktree, base_branch, file)?;
    let current = read_current(worktree, file)?;
    Ok((base, current))
}

fn read_blob_at(worktree: &Path, rev: &str, file: &str) -> Result<Option<String>> {
    // Guard against `file` being interpreted as a git revision / option.
    // Path traversal out of the worktree is fine for `git show` (it reads
    // from the repo index, not the filesystem), but option-looking paths
    // could confuse git's argument parser.
    if file.starts_with('-') {
        return Ok(None);
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["show", &format!("{rev}:{file}")])
        .output()
        .context("git show")?;
    if !out.status.success() {
        // File didn't exist at base → this is an "added" case.
        return Ok(None);
    }
    if !is_utf8(&out.stdout) {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
}

fn read_current(worktree: &Path, file: &str) -> Result<Option<String>> {
    let path = worktree.join(file);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    if !is_utf8(&bytes) {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Cheap UTF-8 / binary heuristic: if the file has NUL bytes in the first
/// 8KB it's binary. Not a perfect check but matches what most diff tools do.
fn is_utf8(bytes: &[u8]) -> bool {
    let probe = &bytes[..bytes.len().min(8192)];
    !probe.contains(&0)
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
    fn untracked_file_shows_in_changes() {
        let repo = mk_repo();
        std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();
        let changes = task_changes(repo.path(), "main").unwrap();
        let new = changes.iter().find(|c| c.path == "new.txt").unwrap();
        assert_eq!(new.kind, FileChangeKind::Untracked);
    }

    #[test]
    fn modified_file_shows_as_modified() {
        let repo = mk_repo();
        std::fs::write(repo.path().join("a.txt"), "hello\nworld\n").unwrap();
        let changes = task_changes(repo.path(), "main").unwrap();
        let a = changes.iter().find(|c| c.path == "a.txt").unwrap();
        assert_eq!(a.kind, FileChangeKind::Modified);
    }

    #[test]
    fn file_sides_returns_both_versions_for_modified_file() {
        let repo = mk_repo();
        std::fs::write(repo.path().join("a.txt"), "hello\nworld\n").unwrap();
        let (base, current) = file_sides(repo.path(), "main", "a.txt").unwrap();
        assert_eq!(base.as_deref(), Some("hello\n"));
        assert_eq!(current.as_deref(), Some("hello\nworld\n"));
    }

    #[test]
    fn file_sides_returns_none_base_for_added_file() {
        let repo = mk_repo();
        std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();
        let (base, current) = file_sides(repo.path(), "main", "new.txt").unwrap();
        assert_eq!(base, None);
        assert_eq!(current.as_deref(), Some("new\n"));
    }

    #[test]
    fn diff_of_committed_change() {
        let repo = mk_repo();
        // Branch off, edit, commit.
        Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["checkout", "-b", "feature"])
            .status()
            .unwrap();
        std::fs::write(repo.path().join("a.txt"), "hello\nfeature\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["commit", "-am", "feat"])
            .status()
            .unwrap();

        let diff = file_diff(repo.path(), "main", "a.txt").unwrap();
        assert!(diff.contains("+feature"));
    }
}
