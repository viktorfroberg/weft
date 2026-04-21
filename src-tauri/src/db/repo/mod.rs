//! Thin `rusqlite` wrappers for each entity.
//!
//! Each write-op method returns a `DbEvent` describing the change, alongside
//! the affected row. Command handlers emit the event after the repo returns
//! so an error path never leaks a spurious event. Nothing in these repos
//! knows about Tauri.

pub mod preset;
pub mod project;
pub mod project_link;
pub mod task;
pub mod task_ticket;
pub mod task_worktree;
pub mod workspace;

#[cfg(test)]
mod tests;

pub use preset::{AgentPreset, BootstrapDelivery, PresetRepo};
pub use project::{NewProject, ProjectRepo};
pub use project_link::{LinkType, ProjectLinkInput, ProjectLinkRepo, ProjectLinkRow};
pub use task::{NewTask, TaskRepo};
pub use task_ticket::{TaskTicketRepo, TaskTicketRow};
pub use task_worktree::{NewTaskWorktree, TaskWorktreeRepo, TaskWorktreeRow};
pub use workspace::{NewWorkspace, NewWorkspaceRepo, WorkspaceRepoRepo, WorkspacesRepo};

/// Convenience: Unix-seconds timestamp.
pub(crate) fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// ULID-like sortable id. UUID v7 is time-ordered so `ORDER BY id` ~ `ORDER BY created_at`.
pub(crate) fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
