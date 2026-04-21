//! `integrations.json` — which providers the user has connected.
//!
//! This file holds NO secrets. Tokens live in the Keychain
//! (see `keychain.rs`). This file exists so the UI can ask "which
//! integrations are currently connected?" without probing the Keychain
//! for every provider on every settings render. When a token is stored
//! or removed, we keep this in sync.
//!
//! Shape:
//! ```json
//! { "connected_providers": ["linear"] }
//! ```
//!
//! Format-wise: pretty-printed JSON, mode 0600 (even though there's no
//! secret here, habit is cheap and it matches other weft app-data files).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Scope filter for the Linear backlog fetch. Lives in `integrations.json`
/// (no secret), defaults to `Actionable` so the launcher strip surfaces
/// "what should I do today" instead of every assigned ticket including
/// Backlog rows the user has not picked up.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum BacklogScope {
    /// state.type IN ["started"] — only tickets currently In Progress.
    InProgress,
    /// state.type IN ["started", "unstarted"] — In Progress + Todo. Default.
    #[default]
    Actionable,
    /// state.type NOT IN ["completed", "canceled"] — every open assigned issue,
    /// including the long Backlog tail. Matches pre-settings behavior.
    AllOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LinearSettings {
    #[serde(default)]
    pub backlog_scope: BacklogScope,
    /// Cached display name from `viewer { name }` — written on successful
    /// connect/test, surfaced as the Home greeting ("Good morning, Viktor").
    /// Optional because users without Linear connected won't have one.
    #[serde(default)]
    pub viewer_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntegrationsFile {
    #[serde(default)]
    pub connected_providers: Vec<String>,
    #[serde(default)]
    pub linear: LinearSettings,
}

fn file_path() -> Result<PathBuf> {
    Ok(crate::db::data_dir()?.join("integrations.json"))
}

/// Read the file, creating a default if missing.
pub fn load() -> Result<IntegrationsFile> {
    let path = file_path()?;
    if !path.exists() {
        return Ok(IntegrationsFile::default());
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let parsed: IntegrationsFile = serde_json::from_str(&raw)
        .with_context(|| format!("parse {}", path.display()))?;
    Ok(parsed)
}

fn save(file: &IntegrationsFile) -> Result<()> {
    let path = file_path()?;
    let json = serde_json::to_string_pretty(file).context("serialize integrations.json")?;
    fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
    // Best-effort chmod 0600. Doesn't fail the call on non-Unix or on
    // filesystems that don't honor modes.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn mark_connected(provider_id: &str) -> Result<()> {
    let mut f = load()?;
    if !f.connected_providers.iter().any(|p| p == provider_id) {
        f.connected_providers.push(provider_id.to_string());
    }
    save(&f)
}

pub fn mark_disconnected(provider_id: &str) -> Result<()> {
    let mut f = load()?;
    f.connected_providers.retain(|p| p != provider_id);
    save(&f)
}

pub fn is_connected(provider_id: &str) -> Result<bool> {
    let f = load()?;
    Ok(f.connected_providers.iter().any(|p| p == provider_id))
}

pub fn linear_settings() -> Result<LinearSettings> {
    Ok(load()?.linear)
}

pub fn set_linear_settings(s: LinearSettings) -> Result<()> {
    let mut f = load()?;
    f.linear = s;
    save(&f)
}

/// Update only the cached viewer name without disturbing user-controlled
/// fields like `backlog_scope`. Called from `integration_set_token` and
/// `integration_test` after a successful auth check.
pub fn set_linear_viewer_name(name: Option<String>) -> Result<()> {
    let mut f = load()?;
    f.linear.viewer_name = name;
    save(&f)
}
