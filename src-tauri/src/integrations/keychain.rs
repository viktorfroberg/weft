//! macOS Keychain wrapper for integration tokens.
//!
//! One generic password entry per provider, under service name
//! `dev.weft.integration.<provider_id>` / account `default`. We never log
//! the token value, not even at trace level — use the `connected` flag
//! from `integrations.json` for observability.
//!
//! `security-framework` v3 target is macOS-only. That's fine; weft itself
//! is macOS-only for v1. When we eventually go cross-platform this file
//! grows a `#[cfg(target_os = "...")]` branch per platform.

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
use std::collections::HashMap;
use std::sync::OnceLock;

const ACCOUNT: &str = "default";

fn service_name(provider_id: &str) -> String {
    format!("dev.weft.integration.{provider_id}")
}

/// Process-lifetime in-memory cache for tokens already pulled from the
/// Keychain this session. Without this, every `require_token` call (one
/// per `ticket_list_backlog`, `ticket_get`, `integration_test`) hits
/// `SecKeychainItemCopyContent`, and macOS re-prompts whenever the ACL
/// is invalid for the running binary — which happens on every `tauri dev`
/// rebuild because the binary signature changes. The cache is populated
/// on first successful read and invalidated on `set_token` / `delete_token`,
/// so the user sees at most one prompt per provider per app launch.
///
/// The cache holds plaintext tokens. That's fine: this process can already
/// read them from Keychain at will, and the Keychain itself sits inside
/// the user's login session. We never serialize the cache and never log
/// values.
fn token_cache() -> &'static RwLock<HashMap<String, String>> {
    static CELL: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Read the token for a provider. `None` if no entry exists (not an
/// error — just "user hasn't connected yet").
pub fn get_token(provider_id: &str) -> Result<Option<String>> {
    if let Some(cached) = token_cache().read().get(provider_id).cloned() {
        return Ok(Some(cached));
    }
    let service = service_name(provider_id);
    match get_generic_password(&service, ACCOUNT) {
        Ok(bytes) => {
            let s = String::from_utf8(bytes)
                .map_err(|e| anyhow!("keychain value is not utf8: {e}"))?;
            token_cache().write().insert(provider_id.to_string(), s.clone());
            Ok(Some(s))
        }
        Err(e) => {
            // `security-framework`'s error type exposes code() but not all
            // variants are re-exported conveniently. -25300 = errSecItemNotFound.
            // Anything else we surface.
            if e.code() == -25300 {
                Ok(None)
            } else {
                Err(anyhow!("keychain read failed: {e}"))
            }
        }
    }
}

/// Write (overwrite) the token for a provider.
pub fn set_token(provider_id: &str, token: &str) -> Result<()> {
    let service = service_name(provider_id);
    set_generic_password(&service, ACCOUNT, token.as_bytes())
        .map_err(|e| anyhow!("keychain write failed: {e}"))?;
    token_cache()
        .write()
        .insert(provider_id.to_string(), token.to_string());
    Ok(())
}

/// Delete the token for a provider. Idempotent: already-missing is not
/// an error (matches the user's mental model of "disconnect").
pub fn delete_token(provider_id: &str) -> Result<()> {
    let service = service_name(provider_id);
    let res = match delete_generic_password(&service, ACCOUNT) {
        Ok(()) => Ok(()),
        Err(e) if e.code() == -25300 => Ok(()),
        Err(e) => Err(anyhow!("keychain delete failed: {e}")),
    };
    token_cache().write().remove(provider_id);
    res
}
