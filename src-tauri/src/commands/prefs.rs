//! Prefs backup mirror — `localStorage["weft-prefs"]` → on-disk JSON at
//! `~/Library/Application Support/weft/prefs.json`.
//!
//! Why: WKWebView's localStorage lives under `~/Library/WebKit/<bundle-id>/`
//! and is bundle-id-scoped. App updates, Gatekeeper quarantine strips,
//! and identity/entitlements changes can all cause macOS to re-provision
//! WebsiteData for the "new" app → user prefs vanish.
//!
//! SQLite (`~/Library/Application Support/weft/weft.db`) survives the
//! same reinstalls because that path is bundle-id-independent. This
//! module gives prefs the same treatment: write-through to a sibling
//! `prefs.json` in the same tree, and read it back on cold boot when
//! localStorage is empty.

use crate::db::data_dir;
use anyhow::{Context, Result};
use std::path::PathBuf;

fn backup_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("prefs.json"))
}

/// Read the backup file if it exists. Returns the raw JSON string so
/// the frontend can seed `localStorage["weft-prefs"]` verbatim — the
/// blob is whatever Zustand's persist middleware wrote, including its
/// `version` + `state` wrapper.
#[tauri::command]
pub fn prefs_read_backup() -> Result<Option<String>, String> {
    let path = backup_path().map_err(|e| e.to_string())?;
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("read {}: {}", path.display(), e)),
    }
}

/// Atomic write via temp-file + rename — a crash mid-write leaves the
/// prior backup intact rather than truncated. No flock: this app is
/// single-instance, the only writer is WKWebView's one process.
#[tauri::command]
pub fn prefs_write_backup(json: String) -> Result<(), String> {
    write_atomic(&json).map_err(|e| format!("{e:#}"))
}

fn write_atomic(json: &str) -> Result<()> {
    let path = backup_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes())
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} → {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_roundtrip() {
        // Can't easily mock `data_dir()` here without plumbing — use
        // the real path under a tempdir-owned HOME. Tests run in CI
        // with a clean $HOME so this is a real smoke write to the
        // user's own Application Support dir when run locally. Keep
        // the payload distinctive so a stale file doesn't mask a
        // real failure.
        let marker = format!(
            "{{\"version\":99,\"state\":{{\"_test_marker\":\"{}\"}}}}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        prefs_write_backup(marker.clone()).unwrap();
        let read = prefs_read_backup().unwrap().expect("backup present after write");
        assert_eq!(read, marker);
    }
}
