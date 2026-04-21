-- v1.0.7.x follow-up to 0007: move `{prompt}` in front of the
-- `{each_path:--add-dir}` expansion.
--
-- Why. Claude Code's `--add-dir <directories...>` is a variadic option
-- (commander.js). Commander consumes every subsequent non-flag argv
-- token as another directory until it hits a named flag or end of
-- argv. So `claude --add-dir /a /b "prompt"` treats `"prompt"` as a
-- third directory, not as the positional `[prompt]` — empirically
-- confirmed with `claude --add-dir /tmp /tmp/fake "hello" -p`, which
-- returned "Input must be provided either through stdin or as a prompt
-- argument when using --print". Putting `{prompt}` in front of the
-- variadic makes claude parse it as the prompt positional first, and
-- `--add-dir …` still consumes only the real paths after.
--
-- Matches the exact string written by 0007 so custom presets (and any
-- hand-edited args_json) stay untouched. Idempotent on fresh DBs where
-- the row was never set to the 0007 form.
UPDATE agent_presets
SET args_json = '["--name", "{slug}", "{prompt}", "{each_path:--add-dir}"]'
WHERE name = 'Claude Code'
  AND args_json = '["--name", "{slug}", "{each_path:--add-dir}", "{prompt}"]';
