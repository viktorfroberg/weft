//! Git operations for weft. All ops shell out to the user's installed `git`
//! so they match whatever version + config they already have.
//!
//! Phase 1 covers **single-repo** primitives. Multi-repo fan-out lives in
//! Phase 4 and composes these.

pub mod commit;
pub mod repo;
pub mod status;
pub mod worktree;

pub use commit::{commit_all, discard_all};
pub use repo::{default_branch, is_git_repo};
pub use status::{file_diff, file_sides, task_changes, FileChange, FileChangeKind};
pub use worktree::{
    branch_delete_if_merged, worktree_add, worktree_remove, BranchDeleteOutcome,
    WorktreeOptions,
};
