-- v1.0.2: ticket integrations + branch-name as source of truth.
--
-- Two changes, one migration because they're coupled: linking tickets to a
-- task implies a non-default branch name (`feature/<slug>`), and we need a
-- single place that holds the branch string. Reconstructing it from
-- `"weft/" + slug` at read time worked when there was only one convention;
-- once two coexist, every derivation becomes a potential bug.

-- Source-of-truth branch name on the task row. Nullable on insert during the
-- backfill below; the Rust task_create path populates it for all new rows.
ALTER TABLE tasks ADD COLUMN branch_name TEXT;

-- Backfill existing tasks with the old convention so reads keep working.
UPDATE tasks SET branch_name = 'weft/' || slug WHERE branch_name IS NULL;

-- task_tickets: link rows only. Title/body/status are NOT cached — Linear
-- is the source of truth for mutable fields, and weft fetches them live on
-- render. Offline degrades to ID-only chips; that's fine.
CREATE TABLE task_tickets (
    task_id       TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    provider      TEXT NOT NULL,   -- "linear" today; GitHub/Jira/Notion later
    external_id   TEXT NOT NULL,   -- e.g. "ABC-123" (provider's native id)
    url           TEXT NOT NULL,   -- persisted so we can open/link even offline
    linked_at     INTEGER NOT NULL,
    PRIMARY KEY (task_id, provider, external_id)
);

CREATE INDEX idx_task_tickets_provider ON task_tickets(provider, external_id);
