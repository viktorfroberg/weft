use serde::{Deserialize, Serialize};

/// A git repository the user has registered with weft.
///
/// Populated in Phase 2 from the local SQLite `projects` table; this struct
/// is the shared shape used across Rust core, Tauri commands, and the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub main_repo_path: String,
    pub default_branch: String,
    pub color: Option<String>,
    pub last_opened_at: i64,
    pub created_at: i64,
}
