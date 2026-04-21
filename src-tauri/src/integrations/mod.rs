//! External ticket providers (Linear for v1.0.2; GitHub/Jira/Notion later).
//!
//! Not a trait yet — we have exactly one provider. Tauri command wrappers
//! take `provider_id: &str` so the wire shape is generic and the layout
//! on disk (keychain service names, `integrations.json` entries, DB
//! `task_tickets.provider` column) already keys by provider id. When
//! provider #2 arrives we extract the trait with a concrete second case
//! in hand, avoiding the "one-impl, leaky-normalization" shape a
//! speculative trait would have locked in.

pub mod keychain;
pub mod linear;
pub mod store;

use serde::{Deserialize, Serialize};

/// The shape an individual chip in the UI wants. Titles/status change
/// upstream; we fetch live (short TTL cache) and never persist mutable
/// fields to SQLite.
///
/// `priority` is Linear's numeric scale: 0 = No priority, 1 = Urgent,
/// 2 = High, 3 = Medium, 4 = Low. We surface it as the raw int so the
/// frontend can render its own color/label and so a future provider can
/// map onto the same scale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub provider: String,
    pub external_id: String,
    pub title: String,
    pub url: String,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub priority: Option<u8>,
    pub cycle_name: Option<String>,
    pub cycle_number: Option<i32>,
}

/// Minimal info needed to persist a task→ticket link row. Titles are NOT
/// stored in the DB — see migration 0004 comment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketLink {
    pub provider: String,
    pub external_id: String,
    pub url: String,
}

/// Result of `integration_test`. `ok=true` means auth succeeded right
/// now; `viewer` is the authenticated user (for UI display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub ok: bool,
    pub viewer: Option<String>,
    pub error: Option<String>,
}

/// One entry per known provider. `connected` flips when the user pastes
/// a token and it passes `integration_test`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    pub connected: bool,
}

/// All providers weft knows how to drive. Hardcoded list — adding one is
/// a code change (new module + new entry here + a new match arm in the
/// Tauri command dispatcher).
pub fn known_providers() -> Vec<(&'static str, &'static str)> {
    vec![("linear", "Linear")]
}
