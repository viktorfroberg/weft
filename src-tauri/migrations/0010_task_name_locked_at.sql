-- v1.1 auto-renamed task titles.
--
-- Context. `tasks.name` started as "whatever the user typed in the
-- compose card" — fine for short prompts, ugly for multi-sentence
-- ones. v1.1 derives a short label locally (first line, ~60 chars)
-- and in the background fires `claude -p --model haiku` to generate
-- a better title. Peer tools (ChatGPT, Cursor) do this too, and
-- universally regret not shipping a rename affordance; see research
-- notes in the plan. This column is the "user renamed — hands off"
-- latch so a late-arriving LLM rename doesn't clobber a name the
-- user explicitly set.
--
-- Null = auto-rename is allowed. Unix-ms = user locked, background
-- rename should skip this row.

ALTER TABLE tasks ADD COLUMN name_locked_at INTEGER;
