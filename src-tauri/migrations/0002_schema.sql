-- Phase 2 schema. Local-only (no organization_id, no tenancy).

DROP TABLE IF EXISTS _placeholder;

-- -----------------------------------------------------------------------
-- projects: a git repository the user has registered
-- -----------------------------------------------------------------------
CREATE TABLE projects (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    main_repo_path  TEXT NOT NULL UNIQUE,
    default_branch  TEXT NOT NULL,
    color           TEXT,
    last_opened_at  INTEGER NOT NULL,
    created_at      INTEGER NOT NULL
);

CREATE INDEX idx_projects_last_opened_at ON projects(last_opened_at DESC);

-- -----------------------------------------------------------------------
-- workspaces: a named grouping of projects worked on together
-- -----------------------------------------------------------------------
CREATE TABLE workspaces (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE INDEX idx_workspaces_sort_order ON workspaces(sort_order);

-- -----------------------------------------------------------------------
-- workspace_repos: junction enabling multi-repo workspaces
-- -----------------------------------------------------------------------
CREATE TABLE workspace_repos (
    workspace_id    TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    base_branch     TEXT,                  -- NULL = inherit project.default_branch
    sort_order      INTEGER NOT NULL DEFAULT 0,
    added_at        INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, project_id)
);

CREATE INDEX idx_workspace_repos_project_id ON workspace_repos(project_id);

-- -----------------------------------------------------------------------
-- tasks: a first-class unit of work (v1.0.7 — workspace is now an
-- optional "repo group" tag rather than a parent). Slug uniqueness is
-- global so branch names never collide across tasks that may share a
-- repo. Workspace deletion sets the tag to NULL, keeping the task.
-- -----------------------------------------------------------------------
CREATE TABLE tasks (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
    name            TEXT NOT NULL,
    slug            TEXT NOT NULL UNIQUE,
    status          TEXT NOT NULL DEFAULT 'idle',
    status_detail   TEXT,
    agent_preset    TEXT,
    notes           TEXT,
    created_at      INTEGER NOT NULL,
    completed_at    INTEGER
);

CREATE INDEX idx_tasks_workspace_id ON tasks(workspace_id);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_created_at ON tasks(created_at DESC);

-- -----------------------------------------------------------------------
-- task_worktrees: one row per (task, project) — Phase 4 fan-out target
-- -----------------------------------------------------------------------
CREATE TABLE task_worktrees (
    task_id         TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    worktree_path   TEXT NOT NULL,
    task_branch     TEXT NOT NULL,
    base_branch     TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'creating',  -- creating | ready | failed | cleaned
    created_at      INTEGER NOT NULL,
    PRIMARY KEY (task_id, project_id)
);

CREATE INDEX idx_task_worktrees_project_id ON task_worktrees(project_id);

-- -----------------------------------------------------------------------
-- workspace_sections: optional sidebar grouping. Phase 3 UI.
-- -----------------------------------------------------------------------
CREATE TABLE workspace_sections (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    is_collapsed    INTEGER NOT NULL DEFAULT 0,
    color           TEXT
);

-- -----------------------------------------------------------------------
-- settings: singleton row for user preferences
-- -----------------------------------------------------------------------
CREATE TABLE settings (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    data            TEXT NOT NULL DEFAULT '{}',   -- JSON blob; shape evolves
    updated_at      INTEGER NOT NULL
);

INSERT INTO settings (id, data, updated_at) VALUES (1, '{}', strftime('%s','now'));
