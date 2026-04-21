-- Agent presets: templates for spawning coding agents in task terminals.
--
-- `args_json` is a JSON array of strings. Each string is either a literal
-- arg or contains template placeholders resolved at launch time:
--   {slug}       → task slug
--   {branch}     → task branch (e.g. "weft/chat-widget")
--   {primary}    → first ready worktree path
--   {each_path:<flag>}  → expands to `<flag> path1 <flag> path2 ...` for every
--                         ready task worktree. Emits ZERO args if no paths.
--
-- `env_json` is a flat JSON object of env vars to set in the agent process.
-- Values support the same placeholders as args.

CREATE TABLE agent_presets (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    command         TEXT NOT NULL,
    args_json       TEXT NOT NULL DEFAULT '[]',
    env_json        TEXT NOT NULL DEFAULT '{}',
    is_default      INTEGER NOT NULL DEFAULT 0,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL
);

CREATE INDEX idx_agent_presets_sort_order ON agent_presets(sort_order);

-- Seed the one preset we support end-to-end in v1.0.1.
-- `--name <slug>` gives the session a readable label in Claude Code.
-- `{each_path:--add-dir}` expands across every worktree in the task so the
-- agent has read/write access to the whole multi-repo surface.
INSERT INTO agent_presets (
    id, name, command, args_json, env_json,
    is_default, sort_order, created_at
) VALUES (
    '019d9c50-0000-7000-0000-000000000001',
    'Claude Code',
    'claude',
    '["--name", "{slug}", "{each_path:--add-dir}"]',
    '{}',
    1,
    0,
    strftime('%s', 'now')
);
