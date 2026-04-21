-- v1.0.7.x: deliver the task's initial prompt as Claude Code's
-- positional CLI argument so Claude auto-submits it on startup.
--
-- Context. Before this migration the default Claude Code preset's
-- argv was `["--name", "{slug}", "{each_path:--add-dir}"]` and weft
-- tried to inject the user's first message into the live PTY via
-- `terminal_write(sessionId, bytes)`. Claude Code's Ink-based TUI
-- parses keypress *events* from its raw-mode stdin, not raw bytes —
-- so piped `\r` / `\n` / escape-sequences land in the input buffer
-- but never trigger submit (upstream issues claude-code #15553 and
-- #6009, both wontfix). The user's prompt stayed visible but
-- unsubmitted.
--
-- Fix. The `{prompt}` template token (see agent_launch.rs) expands
-- to the composed initial message as a single argv entry, or to
-- nothing when the task has no unconsumed prompt. This matches the
-- officially-supported `claude "<prompt>"` invocation.
--
-- Only touches the `Claude Code` preset (by name) — custom presets
-- the user may have added are left as-is so their args_json isn't
-- clobbered. A fresh DB gets the updated value via 0003_agent_presets
-- directly in a follow-up; this migration exists to upgrade existing
-- users without a wipe.
UPDATE agent_presets
SET args_json = '["--name", "{slug}", "{each_path:--add-dir}", "{prompt}"]'
WHERE name = 'Claude Code'
  AND args_json = '["--name", "{slug}", "{each_path:--add-dir}"]';
