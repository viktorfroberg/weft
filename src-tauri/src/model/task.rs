use serde::{Deserialize, Serialize};

/// A first-class unit of work. Spawns one worktree per attached repo.
/// `workspace_id` is an optional tag pointing at the "repo group"
/// preset the task was born from (if any) — it doesn't gate access or
/// membership, and deleting the group sets this to NULL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub workspace_id: Option<String>,
    pub name: String,
    pub slug: String,
    /// Source-of-truth branch name. `weft/<slug>` by default; `feature/<slug>`
    /// when the task was created from linked tickets. NEVER reconstruct by
    /// string-concat — read this field.
    pub branch_name: String,
    pub status: TaskStatus,
    pub agent_preset: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    /// The prompt the user typed in Home's compose card. Written once at
    /// task_create time; consumed when we inject it as the agent's first
    /// user message (see `initial_prompt_consumed_at`). `None` for rows
    /// created before migration 0006 or for tasks created with an empty
    /// compose box.
    pub initial_prompt: Option<String>,
    /// Unix-millis marker set when weft has written the `initial_prompt`
    /// into the agent's PTY. Used as the "already delivered" flag so
    /// subsequent relaunches don't re-inject into Claude's conversation.
    pub initial_prompt_consumed_at: Option<i64>,
    /// Unix-millis marker set when the user explicitly renamed the task
    /// (pencil icon in the header). Null = auto-rename is allowed. A
    /// late-arriving LLM rename from `task_naming::spawn_auto_rename`
    /// skips rows with this set, so the user's explicit choice sticks.
    pub name_locked_at: Option<i64>,
}

/// Coarse task status driven by the event-ingest hook server.
///
/// Finer-grained substates (e.g. tool use in progress, permission prompt)
/// belong on a per-event detail field; this enum is what the UI shows as the
/// status dot color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Just created; worktrees being provisioned or nothing has run yet.
    Idle,
    /// Agent process is actively doing work.
    Working,
    /// Agent is waiting on user input (permission prompt, plan approval).
    Waiting,
    /// An error surfaced from agent or worktree ops.
    Error,
    /// User marked the task complete; worktrees cleaned up.
    Done,
}
