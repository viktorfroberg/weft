-- v1.0.6 — Warm worktrees.
--
-- Per-project list of repo-relative paths to materialize into every
-- new worktree at task_create phase 2.5.
--
--   link_type = 'symlink'  — cheap, writes reach main checkout (deps, env files).
--   link_type = 'clone'    — APFS clonefile(3), per-worktree divergent cache (build output).
--
-- Opt-in: fresh projects start with zero rows. Presets bulk-insert.
-- Non-APFS fallback (clone → symlink) is tracked in-memory on AppState,
-- not persisted, since it's a per-session machine-level concern.

CREATE TABLE project_links (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  path       TEXT NOT NULL,
  link_type  TEXT NOT NULL CHECK (link_type IN ('symlink', 'clone')),
  PRIMARY KEY (project_id, path)
);

CREATE INDEX idx_project_links_project ON project_links(project_id);
