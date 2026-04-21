use serde::{Deserialize, Serialize};

/// A named collection of projects the user works on together.
/// Tasks created in a workspace get worktrees for every attached project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Junction row linking a workspace to one of its projects, with an optional
/// per-workspace base-branch override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRepo {
    pub workspace_id: String,
    pub project_id: String,
    pub base_branch: Option<String>,
    pub sort_order: i64,
    pub added_at: i64,
}
