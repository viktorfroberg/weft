use crate::hooks::events::{EventKind, HookEvent};
use crate::model::TaskStatus;
use std::collections::HashMap;
use std::sync::Mutex;

/// Result of applying one event to the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionResult {
    pub from: TaskStatus,
    pub to: TaskStatus,
    pub changed: bool,
}

/// In-memory map of task id → current status. Phase 7 writes-through to
/// SQLite and emits a `db_event` for `task` when it changes — see the hook
/// server for the wiring.
#[derive(Debug, Default)]
pub struct StatusStore {
    // pub(super) so `hydrate_from_db` can seed without a dedicated setter.
    pub(super) inner: Mutex<HashMap<String, TaskStatus>>,
}

impl StatusStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, task_id: &str) -> TaskStatus {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(task_id)
            .copied()
            .unwrap_or(TaskStatus::Idle)
    }

    /// Apply an event and return the transition. Unknown/tombstoned event
    /// kinds leave state untouched and return `changed: false`.
    pub fn apply(&self, event: &HookEvent) -> TransitionResult {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let from = map.get(&event.task_id).copied().unwrap_or(TaskStatus::Idle);
        let to = next_status(from, event.kind);
        let changed = from != to;
        if changed {
            map.insert(event.task_id.clone(), to);
        }
        TransitionResult { from, to, changed }
    }

    /// Explicit reset (e.g. task being deleted). Returns prior status if any.
    pub fn clear(&self, task_id: &str) -> Option<TaskStatus> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).remove(task_id)
    }
}

/// State-machine rules. Intentionally "last event wins" for simplicity —
/// richer logic (e.g. Error sticky until cleared) can layer on without
/// changing the trait/API shape.
fn next_status(_prev: TaskStatus, kind: EventKind) -> TaskStatus {
    match kind {
        EventKind::Active => TaskStatus::Working,
        EventKind::WaitingInput => TaskStatus::Waiting,
        EventKind::Error => TaskStatus::Error,
        EventKind::SessionEnd => TaskStatus::Idle,
    }
}

/// Stringify for the SQLite `tasks.status` column.
pub fn task_status_as_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Idle => "idle",
        TaskStatus::Working => "working",
        TaskStatus::Waiting => "waiting",
        TaskStatus::Error => "error",
        TaskStatus::Done => "done",
    }
}

fn parse_status(s: &str) -> TaskStatus {
    match s {
        "idle" => TaskStatus::Idle,
        "working" => TaskStatus::Working,
        "waiting" => TaskStatus::Waiting,
        "error" => TaskStatus::Error,
        "done" => TaskStatus::Done,
        _ => TaskStatus::Idle,
    }
}

/// Seed the in-memory store from the SQLite `tasks` table. Called once at
/// app startup so the first incoming hook event compares against the
/// persisted status, not a default `Idle`. Silent on errors — a missing
/// hydration is degraded, not fatal.
pub fn hydrate_from_db(store: &StatusStore, conn: &rusqlite::Connection) {
    let Ok(mut stmt) = conn.prepare("SELECT id, status FROM tasks") else {
        tracing::warn!("hydrate: prepare failed");
        return;
    };
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let status: String = row.get(1)?;
        Ok((id, status))
    });
    let Ok(rows) = rows else {
        tracing::warn!("hydrate: query failed");
        return;
    };
    let mut inner = store.inner.lock().unwrap_or_else(|e| e.into_inner());
    for row in rows.flatten() {
        inner.insert(row.0, parse_status(&row.1));
    }
    tracing::info!(count = inner.len(), "StatusStore hydrated from DB");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(task_id: &str, kind: EventKind) -> HookEvent {
        HookEvent {
            source: "test".into(),
            task_id: task_id.into(),
            kind,
            detail: json!({}),
            timestamp: 0,
        }
    }

    #[test]
    fn default_is_idle() {
        let store = StatusStore::new();
        assert_eq!(store.get("nope"), TaskStatus::Idle);
    }

    #[test]
    fn active_event_transitions_to_working() {
        let store = StatusStore::new();
        let r = store.apply(&ev("t1", EventKind::Active));
        assert_eq!(r.from, TaskStatus::Idle);
        assert_eq!(r.to, TaskStatus::Working);
        assert!(r.changed);
        assert_eq!(store.get("t1"), TaskStatus::Working);
    }

    #[test]
    fn same_event_twice_reports_no_change() {
        let store = StatusStore::new();
        store.apply(&ev("t1", EventKind::Active));
        let r = store.apply(&ev("t1", EventKind::Active));
        assert_eq!(r.from, TaskStatus::Working);
        assert_eq!(r.to, TaskStatus::Working);
        assert!(!r.changed);
    }

    #[test]
    fn waiting_then_session_end_goes_to_idle() {
        let store = StatusStore::new();
        store.apply(&ev("t1", EventKind::WaitingInput));
        assert_eq!(store.get("t1"), TaskStatus::Waiting);
        store.apply(&ev("t1", EventKind::SessionEnd));
        assert_eq!(store.get("t1"), TaskStatus::Idle);
    }

    #[test]
    fn error_event_sets_error() {
        let store = StatusStore::new();
        let r = store.apply(&ev("t1", EventKind::Error));
        assert_eq!(r.to, TaskStatus::Error);
    }

    #[test]
    fn multiple_tasks_tracked_independently() {
        let store = StatusStore::new();
        store.apply(&ev("a", EventKind::Active));
        store.apply(&ev("b", EventKind::WaitingInput));
        assert_eq!(store.get("a"), TaskStatus::Working);
        assert_eq!(store.get("b"), TaskStatus::Waiting);
    }

    #[test]
    fn clear_removes_entry() {
        let store = StatusStore::new();
        store.apply(&ev("t1", EventKind::Active));
        let prior = store.clear("t1");
        assert_eq!(prior, Some(TaskStatus::Working));
        assert_eq!(store.get("t1"), TaskStatus::Idle);
    }
}
