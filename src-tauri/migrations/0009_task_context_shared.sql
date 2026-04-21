-- v1.1 shared task context across multiple agents.
--
-- Context. Once the ⌘T picker landed, a single weft task could host more
-- than one agent tab. The first agent gets the user's compose-card text
-- (+ ticket summary) inlined as a positional CLI arg via `{prompt}` and
-- consumed_at flips; subsequent tabs launched into the same task saw
-- nothing. This migration adds the pieces needed for a shared,
-- agent-agnostic brief materialized as `.weft/context.md` per worktree
-- (plus a mirror `CLAUDE.md` at the task-root for Claude's walk-up
-- memory discovery), plus a per-preset bootstrap prompt that orients
-- every non-first agent launch.
--
-- All new columns are nullable so pre-0009 presets/ticket rows keep
-- working unchanged.

-- Per-preset bootstrap prompt used when {prompt} resolves empty
-- (second-agent path). Delivery mode controls whether it rides as a
-- positional argv token (portable default) or via the agent's
-- own system-prompt flag (Claude's `--append-system-prompt`), so
-- Claude doesn't burn a user turn replying to the orientation text.
ALTER TABLE agent_presets ADD COLUMN bootstrap_prompt_template TEXT;
ALTER TABLE agent_presets ADD COLUMN bootstrap_delivery TEXT
    CHECK (bootstrap_delivery IN ('argv', 'append_system_prompt'));

-- Cache ticket title/status at link time so refresh_task_context can
-- rebuild the context sidecar without hitting Linear on every task
-- mutation. `title_fetched_at` seeds a future staleness policy; v1
-- surfaces it via a manual "Refresh titles" button.
ALTER TABLE task_tickets ADD COLUMN title TEXT;
ALTER TABLE task_tickets ADD COLUMN status TEXT;
ALTER TABLE task_tickets ADD COLUMN title_fetched_at INTEGER;

-- Claude Code: deliver bootstrap as a system prompt append so the
-- transcript stays clean (no "OK, I'll read the context..." from the
-- model). Template references BOTH `.weft/context.md` AND `CLAUDE.md`
-- because Claude's memory loader walks from cwd up to `/`, so the
-- task-root CLAUDE.md mirror is auto-discovered alongside the repo's
-- own CLAUDE.md (if any) — both concatenated, never overridden.
UPDATE agent_presets
SET bootstrap_prompt_template =
'You have joined a weft task in progress. A shared brief for this task is in `.weft/context.md` at the root of the repo worktree you are running in, and mirrored as `CLAUDE.md` at the task-root one directory above (Claude auto-loads it via walk-up memory discovery). The auto block covers user intent, linked Linear tickets, and repo layout. The notes block is shared scratch space — you and any other agents in this task can append findings there so later tabs see them. Read the context before acting, then wait for the user''s instructions.',
    bootstrap_delivery = 'append_system_prompt',
    args_json = '["--name", "{slug}", "{prompt}", "--append-system-prompt", "{bootstrap}", "{each_path:--add-dir}"]'
WHERE name = 'Claude Code';

-- Every other existing preset gets a portable argv-delivered template.
-- References to CLAUDE.md removed because non-Claude agents would be
-- misled. Leaves existing bootstrap_prompt_template values untouched so
-- user-edited presets are safe.
UPDATE agent_presets
SET bootstrap_prompt_template =
'You have joined a weft task in progress. A shared brief for this task is in `.weft/context.md` at the root of the repo worktree you are running in. The auto block covers user intent, linked tickets, and repo layout. The notes block is shared scratch space — you and any other agents in this task can append findings there so later tabs see them. Read it before acting, then wait for the user''s instructions.',
    bootstrap_delivery = 'argv'
WHERE name != 'Claude Code' AND bootstrap_prompt_template IS NULL;
