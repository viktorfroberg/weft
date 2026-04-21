//! Per-project install-lock endpoint.
//!
//! Two weft tasks running concurrent agents in the same project share
//! one main-checkout `node_modules` (because warm worktrees symlink
//! it). Unserialized `bun install` / `npm ci` / `pnpm install` races
//! corrupt the tree. We don't want to document our way out of that
//! ("hint text calls it out"); instead, the hook server exposes a
//! simple acquire/release endpoint that serializes installs within a
//! project.
//!
//! Agents wrap their install command through a small shell helper
//! (`contrib/install-lock/*.sh`) that acquires before the install and
//! releases after. Agents that don't use the helper are no-ops — the
//! lock is opt-in; the lock server doesn't try to detect uncoordinated
//! installs.
//!
//! Semantics:
//!   - `acquire`: blocks until the lock is free (or stolen after a
//!     15-minute watchdog). Returns 200 once acquired.
//!   - `release`: frees the lock and wakes one waiter.
//!
//! Holder identity is opaque (e.g. `"<hostname>:<pid>"`) — the server
//! doesn't verify it, but a mismatched release is rejected to catch
//! accidental cross-project releases in the shell helpers.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio::time::timeout;

/// A holder can stall its install for no more than this long before
/// the next acquire steals the lock. 15 minutes covers even a cold
/// pnpm install of a mid-size monorepo with slow networking.
pub const STALE_AFTER: Duration = Duration::from_secs(900);

/// Per-project lock state. `holder` is None when free. `notify` fires
/// on every release so queued acquirers can re-check.
struct LockState {
    holder: Option<Holder>,
    notify: Arc<Notify>,
}

struct Holder {
    id: String,
    acquired_at: Instant,
}

/// Entire install-lock store. Stored on `AppState` and `AppCtx` as an
/// `Arc`, so handlers on either side (a Tauri command path could also
/// hit this later if we want a UI-visible "currently installing" dot)
/// share the same mutex graph.
pub struct InstallLockStore {
    per_project: StdMutex<HashMap<String, Arc<StdMutex<LockState>>>>,
}

impl InstallLockStore {
    pub fn new() -> Self {
        Self {
            per_project: StdMutex::new(HashMap::new()),
        }
    }

    fn get_or_init(&self, project_id: &str) -> Arc<StdMutex<LockState>> {
        let mut map = self.per_project.lock().expect("install-lock map poisoned");
        Arc::clone(map.entry(project_id.to_string()).or_insert_with(|| {
            Arc::new(StdMutex::new(LockState {
                holder: None,
                notify: Arc::new(Notify::new()),
            }))
        }))
    }

    /// Block until the project's lock is acquired. Returns the holder
    /// id that now owns the lock. If another holder has been sitting
    /// on the lock past `STALE_AFTER`, the lock is stolen — this is
    /// deliberate: a crashed agent that never sent `release` shouldn't
    /// block the next acquire forever.
    pub async fn acquire(&self, project_id: &str, holder_id: &str) -> Result<()> {
        let state = self.get_or_init(project_id);
        loop {
            let notify = {
                let mut s = state.lock().expect("lock state poisoned");
                match &s.holder {
                    None => {
                        s.holder = Some(Holder {
                            id: holder_id.to_string(),
                            acquired_at: Instant::now(),
                        });
                        tracing::debug!(
                            target: "weft::install-lock",
                            project_id = project_id,
                            holder = holder_id,
                            "acquired"
                        );
                        return Ok(());
                    }
                    Some(h) if h.acquired_at.elapsed() > STALE_AFTER => {
                        let stolen_from = h.id.clone();
                        s.holder = Some(Holder {
                            id: holder_id.to_string(),
                            acquired_at: Instant::now(),
                        });
                        tracing::warn!(
                            target: "weft::install-lock",
                            project_id = project_id,
                            stolen_from = %stolen_from,
                            holder = holder_id,
                            "stale lock stolen"
                        );
                        return Ok(());
                    }
                    Some(_) => s.notify.clone(),
                }
            };
            // Re-check every minute OR when someone releases. Short
            // re-check interval ensures steal-on-stale fires even when
            // no release arrives (crash scenarios).
            let _ = timeout(Duration::from_secs(60), notify.notified()).await;
        }
    }

    /// Release if the caller actually holds it. A mismatched release
    /// (wrong `holder_id`) is a no-op — we log and return Ok so the
    /// shell helper doesn't fail noisily on a double-release after a
    /// watchdog steal.
    pub fn release(&self, project_id: &str, holder_id: &str) {
        let state = self.get_or_init(project_id);
        let mut s = state.lock().expect("lock state poisoned");
        match &s.holder {
            Some(h) if h.id == holder_id => {
                s.holder = None;
                s.notify.notify_one();
                tracing::debug!(
                    target: "weft::install-lock",
                    project_id = project_id,
                    holder = holder_id,
                    "released"
                );
            }
            Some(h) => {
                tracing::warn!(
                    target: "weft::install-lock",
                    project_id = project_id,
                    current = %h.id,
                    tried = holder_id,
                    "release by non-holder — ignoring (watchdog may have stolen)"
                );
            }
            None => {
                tracing::debug!(
                    target: "weft::install-lock",
                    project_id = project_id,
                    holder = holder_id,
                    "release with no holder — no-op"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LockRequest {
    pub project_id: String,
    pub holder_id: String,
    pub kind: LockAction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockAction {
    Acquire,
    Release,
}

#[derive(Debug, Serialize)]
pub struct LockResponse {
    pub ok: bool,
}
