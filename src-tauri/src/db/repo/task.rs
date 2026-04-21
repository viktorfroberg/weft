use super::{new_id, now};
use crate::db::events::{DbEvent, Entity};
use crate::model::{Task, TaskStatus};
use crate::task as task_naming;
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTask {
    /// Optional "repo group" tag (points at a `workspaces` row). Since
    /// v1.0.7 tasks are first-class and pick their own repo set; this
    /// field just records which saved group the user picked (if any).
    pub workspace_id: Option<String>,
    pub name: String,
    pub agent_preset: Option<String>,
    /// Prompt typed in the compose card — becomes the agent's first user
    /// message. `None` = no prompt was typed (fallback to "untitled").
    pub initial_prompt: Option<String>,
}

pub struct TaskRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TaskRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert a task. Derives slug from name; if the slug collides with
    /// any existing task (globally, v1.0.7), appends `-2`, `-3`, ... until
    /// unique. Branch name defaults to `weft/<slug>`; callers creating
    /// tasks from tickets should use `insert_with_slug` + explicit branch.
    pub fn insert(&self, input: NewTask) -> Result<(Task, DbEvent)> {
        let base_slug = task_naming::derive_slug(&input.name);
        if base_slug.is_empty() {
            return Err(anyhow!("task name produces empty slug: {:?}", input.name));
        }
        let slug = self.unique_slug(&base_slug)?;
        let branch_name = format!("weft/{slug}");
        self.insert_with_slug(input, &slug, &branch_name)
    }

    /// Same as `insert`, but with a pre-derived, pre-uniquified slug AND
    /// an explicit branch_name. Used by the multi-repo fan-out service
    /// which needs the final slug BEFORE creating worktrees.
    pub fn insert_with_slug(
        &self,
        input: NewTask,
        slug: &str,
        branch_name: &str,
    ) -> Result<(Task, DbEvent)> {
        if slug.is_empty() {
            return Err(anyhow!("empty slug"));
        }
        if branch_name.is_empty() {
            return Err(anyhow!("empty branch_name"));
        }
        let slug = slug.to_string();
        let branch_name = branch_name.to_string();
        let id = new_id();
        let ts = now();
        let status = "idle";

        self.conn.execute(
            "INSERT INTO tasks (id, workspace_id, name, slug, branch_name, status, agent_preset, created_at, initial_prompt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                input.workspace_id,
                input.name,
                slug,
                branch_name,
                status,
                input.agent_preset,
                ts,
                input.initial_prompt,
            ],
        )?;

        let task = Task {
            id: id.clone(),
            workspace_id: input.workspace_id,
            name: input.name,
            slug,
            branch_name,
            status: TaskStatus::Idle,
            agent_preset: input.agent_preset,
            created_at: ts,
            completed_at: None,
            initial_prompt: input.initial_prompt,
            initial_prompt_consumed_at: None,
            name_locked_at: None,
        };
        Ok((task, DbEvent::insert(Entity::Task, id)))
    }

    /// User-initiated rename (pencil icon in task header). Sets
    /// `name_locked_at` so a late-arriving background LLM rename
    /// skips this row — user intent wins over automation.
    pub fn rename(&self, id: &str, new_name: &str) -> Result<DbEvent> {
        let ts = now();
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("refusing to rename task to empty string"));
        }
        self.conn.execute(
            "UPDATE tasks SET name = ?1, name_locked_at = ?2 WHERE id = ?3",
            params![trimmed, ts, id],
        )?;
        Ok(DbEvent::update(Entity::Task, id))
    }

    /// Background auto-rename from `task_naming::spawn_auto_rename`.
    /// Only writes if `name_locked_at IS NULL`, so a user rename that
    /// landed while the LLM was thinking is preserved. Returns `true`
    /// if the row was updated.
    pub fn auto_rename(&self, id: &str, new_name: &str) -> Result<bool> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Ok(false);
        }
        let rows = self.conn.execute(
            "UPDATE tasks SET name = ?1
             WHERE id = ?2 AND name_locked_at IS NULL",
            params![trimmed, id],
        )?;
        Ok(rows > 0)
    }

    /// Mark the `initial_prompt` as delivered to the agent. Called by
    /// the frontend after it has successfully written the prompt into
    /// the PTY, so a relaunch doesn't inject a duplicate user message.
    pub fn mark_initial_prompt_consumed(&self, id: &str) -> Result<DbEvent> {
        let ts = now();
        self.conn.execute(
            "UPDATE tasks SET initial_prompt_consumed_at = ?1
             WHERE id = ?2 AND initial_prompt_consumed_at IS NULL",
            params![ts, id],
        )?;
        Ok(DbEvent::update(Entity::Task, id))
    }

    pub fn get(&self, id: &str) -> Result<Option<Task>> {
        use rusqlite::OptionalExtension;
        let t = self
            .conn
            .query_row(
                "SELECT id, workspace_id, name, slug, branch_name, status, agent_preset, created_at, completed_at, initial_prompt, initial_prompt_consumed_at, name_locked_at
                 FROM tasks WHERE id = ?1",
                [id],
                row_to_task,
            )
            .optional()?;
        Ok(t)
    }

    /// List tasks belonging to a specific repo group. Kept for the
    /// Settings → Repo groups view; the primary sidebar uses `list_all`.
    pub fn list_for_workspace(&self, workspace_id: &str) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, workspace_id, name, slug, branch_name, status, agent_preset, created_at, completed_at, initial_prompt, initial_prompt_consumed_at, name_locked_at
             FROM tasks WHERE workspace_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([workspace_id], row_to_task)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Every task, newest first. The sidebar's flat-list view reads
    /// this + filters/groups client-side. Cheap at hundreds of rows;
    /// add a bounded variant if we ever grow past thousands.
    pub fn list_all(&self) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, workspace_id, name, slug, branch_name, status, agent_preset, created_at, completed_at, initial_prompt, initial_prompt_consumed_at, name_locked_at
             FROM tasks ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_task)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, id: &str) -> Result<DbEvent> {
        self.conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        Ok(DbEvent::delete(Entity::Task, id))
    }

    /// Return a globally-unique slug derived from `base`. Workspace
    /// scoping was dropped in v1.0.7 — two tasks in different "repo
    /// groups" can still share a repo, so a slug collision there
    /// would produce colliding branch names. Global uniqueness fixes
    /// that.
    ///
    /// Racey if called outside a transaction that also does the INSERT —
    /// two concurrent callers can both reserve the same slug. The INSERT
    /// hits a UNIQUE violation in that case; callers should translate
    /// that into a retry or user-facing error.
    pub fn unique_slug(&self, base: &str) -> Result<String> {
        let mut candidate = base.to_string();
        let mut n = 2;
        while self.slug_exists(&candidate)? {
            candidate = format!("{base}-{n}");
            n += 1;
            if n > 1000 {
                return Err(anyhow!("could not find unique slug for {base}"));
            }
        }
        Ok(candidate)
    }

    fn slug_exists(&self, slug: &str) -> Result<bool> {
        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM tasks WHERE slug = ?1",
                params![slug],
                |row| row.get(0),
            )
            .ok();
        Ok(exists.is_some())
    }
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    let status_str: String = row.get(5)?;
    let status = match status_str.as_str() {
        "idle" => TaskStatus::Idle,
        "working" => TaskStatus::Working,
        "waiting" => TaskStatus::Waiting,
        "error" => TaskStatus::Error,
        "done" => TaskStatus::Done,
        _ => TaskStatus::Idle,
    };
    Ok(Task {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        name: row.get(2)?,
        slug: row.get(3)?,
        branch_name: row.get(4)?,
        status,
        agent_preset: row.get(6)?,
        created_at: row.get(7)?,
        completed_at: row.get(8)?,
        initial_prompt: row.get(9)?,
        initial_prompt_consumed_at: row.get(10)?,
        name_locked_at: row.get(11)?,
    })
}
