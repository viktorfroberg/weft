-- Per-task external agent session ids. Captured from hook event payloads
-- (Claude Code's hook events include `session_id` at the top level of every
-- payload — the `/v1/events` ingest plumbs it from `detail.session_id`).
--
-- Polymorphic on `source` so future agents (codex, aider) can land here
-- without a schema change.
CREATE TABLE task_agent_sessions (
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  source TEXT NOT NULL,
  external_session_id TEXT NOT NULL,
  last_seen_at INTEGER NOT NULL,
  PRIMARY KEY (task_id, source)
);

-- Resume eligibility lives on the preset, not on a string-match of
-- `preset.command` (users edit that string freely — `claude`,
-- `/usr/local/bin/claude`, `npx claude-code`, wrapper scripts...).
ALTER TABLE agent_presets ADD COLUMN supports_resume INTEGER NOT NULL DEFAULT 0;

-- Seed Claude preset is the only resume-capable agent today.
UPDATE agent_presets
   SET supports_resume = 1
 WHERE id = '019d9c50-0000-7000-0000-000000000001';
