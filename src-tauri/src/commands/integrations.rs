//! Tauri commands for integration providers.
//!
//! All command wrappers take `provider_id: String` so the wire protocol
//! is already generic — adding a provider #2 doesn't reshape any of the
//! Tauri-exposed entry points, only the internal `match` arms.

use crate::integrations::{
    keychain, linear, store, store::LinearSettings, AuthStatus, ProviderInfo, Ticket,
};
use crate::AppState;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::State;

/// Shared across command invocations for the app lifetime — one cache
/// per running process, not per-call.
fn backlog_cache() -> Arc<linear::BacklogCache> {
    static CELL: OnceLock<Arc<linear::BacklogCache>> = OnceLock::new();
    CELL.get_or_init(linear::BacklogCache::new).clone()
}

/// Rate limiter for `integration_test`. Settings UI re-renders frequently
/// and some layouts call test on mount; without this we'd hammer the API.
fn last_test_at() -> &'static Mutex<Option<Instant>> {
    static CELL: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(None))
}
const TEST_MIN_INTERVAL: Duration = Duration::from_secs(5);

#[tauri::command]
pub fn integration_list() -> Result<Vec<ProviderInfo>, String> {
    let connected = store::load().map_err(|e| e.to_string())?.connected_providers;
    Ok(crate::integrations::known_providers()
        .into_iter()
        .map(|(id, name)| ProviderInfo {
            id: id.to_string(),
            display_name: name.to_string(),
            connected: connected.iter().any(|c| c == id),
        })
        .collect())
}

#[tauri::command]
pub async fn integration_set_token(
    provider_id: String,
    token: String,
) -> Result<AuthStatus, String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("token is empty".into());
    }

    let status = match provider_id.as_str() {
        "linear" => linear::test_auth(&token).await,
        other => return Err(format!("unknown provider: {other}")),
    };

    if !status.ok {
        // Don't persist a failing token.
        return Ok(status);
    }

    keychain::set_token(&provider_id, &token).map_err(|e| e.to_string())?;
    // If the connected-list write fails, roll the Keychain write back so
    // the two views of "is this provider connected?" never disagree —
    // otherwise a partially-applied state leaves a token in Keychain but
    // the UI would correctly show the provider as disconnected, with no
    // obvious path to recovery.
    if let Err(e) = store::mark_connected(&provider_id) {
        let _ = keychain::delete_token(&provider_id);
        return Err(e.to_string());
    }
    if provider_id == "linear" {
        // Best-effort: don't fail the connect over a name-cache write.
        let _ = store::set_linear_viewer_name(extract_viewer_name(status.viewer.as_deref()));
    }
    Ok(status)
}

/// `viewer` arrives as "Name <email>"; pull just the display-name half so
/// we can greet the user by first-name on Home without leaking email into
/// the UI string.
fn extract_viewer_name(viewer: Option<&str>) -> Option<String> {
    let v = viewer?;
    let name = match v.find(" <") {
        Some(i) => &v[..i],
        None => v,
    }
    .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[tauri::command]
pub fn integration_clear(provider_id: String) -> Result<(), String> {
    keychain::delete_token(&provider_id).map_err(|e| e.to_string())?;
    store::mark_disconnected(&provider_id).map_err(|e| e.to_string())?;
    // Invalidate the cached backlog so a reconnect with a different
    // account doesn't show the previous user's tickets.
    if provider_id == "linear" {
        // We don't know which token was previously connected, so nuke all
        // cached entries. Acceptable — cache is just a rate-limit helper.
        backlog_cache().clear();
    }
    Ok(())
}

#[tauri::command]
pub async fn integration_test(provider_id: String) -> Result<AuthStatus, String> {
    {
        let mut last = last_test_at().lock();
        if let Some(at) = *last {
            if at.elapsed() < TEST_MIN_INTERVAL {
                return Ok(AuthStatus {
                    ok: false,
                    viewer: None,
                    error: Some(format!(
                        "slow down — test throttled to 1/{}s",
                        TEST_MIN_INTERVAL.as_secs()
                    )),
                });
            }
        }
        *last = Some(Instant::now());
    }

    let token = match keychain::get_token(&provider_id) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return Ok(AuthStatus {
                ok: false,
                viewer: None,
                error: Some("not connected".into()),
            })
        }
        Err(e) => return Err(e.to_string()),
    };

    let status = match provider_id.as_str() {
        "linear" => linear::test_auth(&token).await,
        other => return Err(format!("unknown provider: {other}")),
    };
    if provider_id == "linear" && status.ok {
        let _ = store::set_linear_viewer_name(extract_viewer_name(status.viewer.as_deref()));
    }
    Ok(status)
}

#[tauri::command]
pub async fn ticket_list_backlog(provider_id: String) -> Result<Vec<Ticket>, String> {
    let token = require_token(&provider_id)?;
    let cache = backlog_cache();
    match provider_id.as_str() {
        "linear" => {
            let scope = store::linear_settings()
                .map_err(|e| e.to_string())?
                .backlog_scope;
            linear::viewer_backlog(&cache, &token, scope)
                .await
                .map_err(|e| e.to_string())
        }
        other => Err(format!("unknown provider: {other}")),
    }
}

#[tauri::command]
pub fn linear_settings_get() -> Result<LinearSettings, String> {
    store::linear_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn linear_settings_set(settings: LinearSettings) -> Result<(), String> {
    store::set_linear_settings(settings).map_err(|e| e.to_string())?;
    // Cached backlog is keyed by (token, scope) so old-scope entries simply
    // age out after 30s, but a one-shot clear gives the UI an immediate
    // refetch on the new scope without waiting.
    backlog_cache().clear();
    Ok(())
}

#[tauri::command]
pub async fn ticket_get(
    provider_id: String,
    external_id: String,
) -> Result<Option<Ticket>, String> {
    let token = require_token(&provider_id)?;
    match provider_id.as_str() {
        "linear" => linear::issue_by_identifier(&token, &external_id)
            .await
            .map_err(|e| e.to_string()),
        other => Err(format!("unknown provider: {other}")),
    }
}

fn require_token(provider_id: &str) -> Result<String, String> {
    match keychain::get_token(provider_id).map_err(|e| e.to_string())? {
        Some(t) => Ok(t),
        None => Err(format!("{provider_id} not connected")),
    }
}

// Silence `unused` for now — we'll hook it up when tickets become
// linkable to tasks (task #36 / #37). Keeps the file typecheck-clean.
#[allow(dead_code)]
fn _unused_state(_state: State<'_, AppState>) {}
