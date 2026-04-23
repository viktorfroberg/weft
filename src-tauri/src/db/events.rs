use serde::{Deserialize, Serialize};

/// Broadcast name Tauri uses. Keep in sync with `src/lib/events.ts`.
pub const DB_EVENT_CHANNEL: &str = "db_event";

/// Entity kind a db event refers to. String rather than enum at the wire
/// so it's cheap to extend with new entities without a schema-side lockstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entity {
    Project,
    Workspace,
    WorkspaceRepo,
    Task,
    TaskWorktree,
    WorkspaceSection,
    Settings,
    ProjectLink,
    Preset,
    TerminalTab,
    AgentSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Insert,
    Update,
    Delete,
}

/// Emitted after any successful write to SQLite. Consumers (React Zustand
/// slices) listen on `DB_EVENT_CHANNEL` and invalidate by entity+id.
///
/// `id` is the stringified PK. For composite-key tables (`workspace_repos`,
/// `task_worktrees`) we pack both halves as `"<a>:<b>"` — consumers that
/// need the pieces parse it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbEvent {
    pub entity: Entity,
    pub id: String,
    pub op: Op,
}

impl DbEvent {
    pub fn insert(entity: Entity, id: impl Into<String>) -> Self {
        Self {
            entity,
            id: id.into(),
            op: Op::Insert,
        }
    }
    pub fn update(entity: Entity, id: impl Into<String>) -> Self {
        Self {
            entity,
            id: id.into(),
            op: Op::Update,
        }
    }
    pub fn delete(entity: Entity, id: impl Into<String>) -> Self {
        Self {
            entity,
            id: id.into(),
            op: Op::Delete,
        }
    }
}
