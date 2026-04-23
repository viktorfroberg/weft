//! Command resolution for PTY spawn.
//!
//! When an agent preset's `command` is a bare name (e.g. `claude`,
//! `codex`, `aider`), we resolve it to an absolute path BEFORE handing
//! it to portable-pty. portable-pty uses `execvp(3)` under the hood
//! which searches the inherited PATH — and PATH is exactly the thing
//! that's wrong on macOS Tauri builds:
//!
//! - **Launched from Finder/Dock**: PATH ≈ `/usr/bin:/bin:/usr/sbin:/sbin`
//!   (the LaunchServices env). Anything in `/opt/homebrew/bin`,
//!   `~/.bun/bin`, `~/.cargo/bin`, `~/.nvm/.../bin` etc is invisible.
//! - **Launched from `bun run tauri dev`**: PATH is your shell's PATH,
//!   so it usually works. But if you've added a tool only in a new
//!   shell window after starting dev, the running app won't see it.
//!
//! Strategy: search the inherited PATH first, then a fallback set of
//! common dev install dirs. If the command is still not found, surface
//! a clear error that names the searched paths so the user can tell
//! whether the tool is missing entirely vs. just out-of-PATH.
//!
//! When `command` contains a path separator (`/`), we trust it
//! verbatim — same semantics as `execvp`.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Extra dirs to fall back to if the inherited PATH doesn't contain
/// the command. Order matters: homebrew first (most users install CLI
/// tools there or via npm-global symlinks), then the major language
/// install roots. `~` is expanded against `dirs::home_dir()`.
const FALLBACK_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "~/.bun/bin",
    "~/.cargo/bin",
    "~/.local/bin",
    "~/.npm-global/bin",
    "~/.volta/bin/shims",
    "~/.volta/bin",
];

/// Resolve `command` to an absolute path the OS can exec. Returns an
/// error that names every dir we searched if the command can't be found.
pub fn resolve_command(command: &str) -> Result<PathBuf> {
    if command.contains('/') {
        let p = PathBuf::from(command);
        if !p.is_file() {
            return Err(anyhow!(
                "command path does not exist: {} (working dir for relative paths: {:?})",
                command,
                std::env::current_dir().ok(),
            ));
        }
        return Ok(p);
    }

    let mut tried: Vec<PathBuf> = Vec::new();

    if let Ok(path_env) = std::env::var("PATH") {
        for raw in std::env::split_paths(&path_env) {
            tried.push(raw.clone());
            if let Some(found) = check_dir(&raw, command) {
                return Ok(found);
            }
        }
    }

    let home = dirs::home_dir();
    for fallback in FALLBACK_DIRS {
        let dir = expand_tilde(fallback, home.as_deref());
        if tried.iter().any(|t| t == &dir) {
            continue;
        }
        tried.push(dir.clone());
        if let Some(found) = check_dir(&dir, command) {
            return Ok(found);
        }
    }

    let pretty = tried
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    Err(anyhow!(
        "command '{command}' not found. Searched: {pretty}. \
         If '{command}' is installed in a non-standard dir, set the preset's \
         command to its absolute path."
    ))
}

fn check_dir(dir: &Path, command: &str) -> Option<PathBuf> {
    let candidate = dir.join(command);
    if !candidate.is_file() {
        return None;
    }
    if !is_executable(&candidate) {
        return None;
    }
    Some(candidate)
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_p: &Path) -> bool {
    true
}

fn expand_tilde(s: &str, home: Option<&Path>) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(h) = home {
            return h.join(rest);
        }
    }
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_path_passes_through_when_exists() {
        // /bin/sh is virtually guaranteed on every unix.
        let resolved = resolve_command("/bin/sh").unwrap();
        assert_eq!(resolved, PathBuf::from("/bin/sh"));
    }

    #[test]
    fn absolute_path_errors_when_missing() {
        let err = resolve_command("/definitely/not/a/real/path/zxqwerty").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("does not exist"), "got: {msg}");
    }

    #[test]
    fn bare_name_resolves_via_path() {
        // `sh` is on PATH on every unix; test environment has /bin in PATH.
        let resolved = resolve_command("sh").unwrap();
        assert!(resolved.is_absolute(), "got: {}", resolved.display());
        assert!(resolved.ends_with("sh"), "got: {}", resolved.display());
    }

    #[test]
    fn bare_name_missing_lists_searched_dirs() {
        let err = resolve_command("definitely-not-an-executable-zxqwerty").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not found"), "got: {msg}");
        assert!(msg.contains("Searched:"), "got: {msg}");
    }

    #[test]
    fn tilde_expansion() {
        let p = expand_tilde("~/foo", Some(Path::new("/users/test")));
        assert_eq!(p, PathBuf::from("/users/test/foo"));
        let p = expand_tilde("/abs/foo", Some(Path::new("/users/test")));
        assert_eq!(p, PathBuf::from("/abs/foo"));
    }
}
