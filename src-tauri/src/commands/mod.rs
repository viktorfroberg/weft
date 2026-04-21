//! Tauri command handlers. Each write-op command:
//!   1. Calls the appropriate repo method (returns `(row, DbEvent)` or just `DbEvent`)
//!   2. Emits the event on `DB_EVENT_CHANNEL` via the Tauri AppHandle
//!   3. Returns the row (or deleted id) to the caller
//!
//! Read-op commands just query and return.

pub mod changes;
pub mod devlog;
pub mod git;
pub mod integrations;
pub mod presets;
pub mod project_links;
pub mod projects;
pub mod settings;
pub mod tasks;
pub mod terminal;
pub mod workspaces;

use crate::db::events::{DbEvent, DB_EVENT_CHANNEL};
use tauri::{AppHandle, Emitter};

/// Fire-and-forget emit. If this fails the write already happened, so we
/// log and carry on — consumers will catch up next time.
pub(crate) fn emit_event(app: &AppHandle, event: DbEvent) {
    if let Err(e) = app.emit(DB_EVENT_CHANNEL, &event) {
        tracing::warn!(error = %e, ?event, "failed to emit db_event");
    }
}
