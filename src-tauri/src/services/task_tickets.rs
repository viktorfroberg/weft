//! Link/unlink tickets to tasks. Since v1.1 the link path also fetches
//! the provider's title + workflow state at link time and persists it
//! into `task_tickets` (new columns from migration 0009) so later code
//! paths (context sidecar, UI chips) can render without hitting the
//! provider again. A manual "Refresh titles" command re-runs the fetch
//! when a user suspects upstream drift.
//!
//! Historical note: until April 2026 this module also wrote
//! `.weft/tickets.md` per worktree. Dropped when the frontend started
//! inlining ticket info into the first agent message. As of v1.1 that
//! same ticket data flows through `refresh_task_context` into
//! `.weft/context.md` — but sidecar writes still happen via the
//! context service, not here.

use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::db::events::DbEvent;
use crate::db::repo::{TaskRepo, TaskTicketRepo};
use crate::integrations::{self, TicketLink};
use crate::services::task_context;

/// Insert the link row synchronously so the caller has a committed
/// event to emit, then spawn a best-effort background task to enrich
/// the cache columns from Linear and refresh the context sidecar.
/// Returns the insert event immediately — the UI gets its optimistic
/// update without waiting on the network.
pub fn link_ticket(
    db: &Arc<Mutex<Connection>>,
    task_id: &str,
    link: TicketLink,
) -> Result<DbEvent> {
    let event = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        TaskTicketRepo::new(&conn).insert(task_id, &link)?
    };
    // Context sidecar reflects the new link immediately (with a null
    // title — the enrich pass will fill it). Non-fatal.
    if let Err(e) = task_context::refresh_task_context(db, task_id) {
        tracing::warn!(
            task = %task_id,
            error = %e,
            "link_ticket: refresh_task_context failed (non-fatal)"
        );
    }
    spawn_title_enrich(db.clone(), task_id.to_string(), link);
    Ok(event)
}

pub fn unlink_ticket(
    db: &Arc<Mutex<Connection>>,
    task_id: &str,
    provider: &str,
    external_id: &str,
) -> Result<DbEvent> {
    let event = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        TaskTicketRepo::new(&conn).delete(task_id, provider, external_id)?
    };
    if let Err(e) = task_context::refresh_task_context(db, task_id) {
        tracing::warn!(
            task = %task_id,
            error = %e,
            "unlink_ticket: refresh_task_context failed (non-fatal)"
        );
    }
    Ok(event)
}

/// Re-fetch every ticket linked to this task from its provider and
/// update the cached title/status columns. Used by the ContextDialog's
/// "Refresh titles" button when the user suspects the upstream ticket
/// has been renamed since link time. Returns the count of rows
/// actually updated. Fires `refresh_task_context` at the end so the
/// sidecar + CLAUDE.md mirror pick up the new cache.
pub async fn refresh_ticket_titles(
    db: Arc<Mutex<Connection>>,
    task_id: String,
) -> Result<usize> {
    let rows = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        TaskTicketRepo::new(&conn).list_for_task(&task_id)?
    };
    let mut updated = 0usize;
    for row in rows {
        match enrich_one(row.provider.as_str(), row.external_id.as_str()).await {
            Ok(Some((title, status))) => {
                let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
                TaskTicketRepo::new(&conn).update_cached_metadata(
                    &task_id,
                    &row.provider,
                    &row.external_id,
                    title.as_deref(),
                    status.as_deref(),
                )?;
                updated += 1;
            }
            Ok(None) => {
                tracing::info!(
                    task = %task_id,
                    ticket = %row.external_id,
                    "refresh_ticket_titles: provider returned no issue"
                );
            }
            Err(e) => {
                tracing::warn!(
                    task = %task_id,
                    ticket = %row.external_id,
                    error = %e,
                    "refresh_ticket_titles: provider fetch failed"
                );
            }
        }
    }
    if updated > 0 {
        if let Err(e) = task_context::refresh_task_context(&db, &task_id) {
            tracing::warn!(
                task = %task_id,
                error = %e,
                "refresh_ticket_titles: refresh_task_context failed"
            );
        }
    }
    Ok(updated)
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Fire-and-forget enrichment. On success updates the cache and
/// re-renders the sidecar; on failure just logs. The event for the
/// link insert was already emitted by the caller, so the UI will show
/// a placeholder title immediately and the enriched title when the
/// next `task` db_event fires (triggered here by the metadata update).
fn spawn_title_enrich(db: Arc<Mutex<Connection>>, task_id: String, link: TicketLink) {
    tokio::spawn(async move {
        let provider = link.provider.as_str();
        let external_id = link.external_id.as_str();
        match enrich_one(provider, external_id).await {
            Ok(Some((title, status))) => {
                {
                    let conn = match db.lock() {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    let _ = TaskTicketRepo::new(&conn).update_cached_metadata(
                        &task_id,
                        provider,
                        external_id,
                        title.as_deref(),
                        status.as_deref(),
                    );
                }
                if let Err(e) = task_context::refresh_task_context(&db, &task_id) {
                    tracing::warn!(
                        task = %task_id,
                        error = %e,
                        "title_enrich: refresh_task_context failed"
                    );
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::info!(
                    task = %task_id,
                    ticket = %external_id,
                    error = %e,
                    "title_enrich: provider fetch failed (will retry via manual refresh)"
                );
            }
        }
    });
}

async fn enrich_one(
    provider: &str,
    external_id: &str,
) -> Result<Option<(Option<String>, Option<String>)>> {
    match provider {
        "linear" => {
            let token = match integrations::keychain::get_token(provider)? {
                Some(t) => t,
                None => return Ok(None),
            };
            let ticket = integrations::linear::issue_by_identifier(&token, external_id).await?;
            Ok(ticket.map(|t| (Some(t.title), t.status)))
        }
        _ => Ok(None),
    }
}

#[allow(dead_code)]
fn _keep_task_repo_linked(conn: &Connection) {
    let _ = TaskRepo::new(conn);
}
