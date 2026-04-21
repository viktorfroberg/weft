use crate::git;
use std::path::Path;

#[tauri::command]
pub fn git_is_repo(path: String) -> bool {
    git::is_git_repo(Path::new(&path))
}

#[tauri::command]
pub fn git_default_branch(path: String) -> Result<String, String> {
    git::default_branch(Path::new(&path)).map_err(|e| e.to_string())
}
