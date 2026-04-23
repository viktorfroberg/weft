-- Persistent terminal tab rows. A tab outlives its session: when the
-- PTY dies we mark the row `dormant`; explicit user-close hard-deletes.
-- See `services/reconcile.rs::reconcile_scrollback` for the on-disk
-- cleanup of the associated scrollback file under
-- `~/Library/Application Support/weft/scrollback/<id>.bin`.
CREATE TABLE terminal_tabs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK(kind IN ('shell','agent')),
  label TEXT NOT NULL,
  preset_id TEXT,
  sort_order INTEGER NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('live','dormant')) DEFAULT 'live',
  closed_at INTEGER,
  last_exit_code INTEGER,
  cwd TEXT,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_terminal_tabs_task ON terminal_tabs(task_id, sort_order);
