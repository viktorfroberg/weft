use super::session::{SpawnOptions, TerminalSession};
use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::AppHandle;

/// App-lifetime registry of live PTY sessions keyed by session id. A
/// secondary task_id → [session_id] index lets us kill all PTYs for a task
/// before deleting its worktrees.
///
/// Lock ordering: ALWAYS acquire `sessions` before `by_task`. This avoids
/// deadlock with concurrent `kill` + `kill_by_task` calls. All mutator
/// methods below follow this order; if you add a new one, keep it
/// consistent or audit every call site.
#[derive(Default)]
pub struct TerminalManager {
    sessions: RwLock<HashMap<String, Arc<TerminalSession>>>,
    by_task: RwLock<HashMap<String, Vec<String>>>,
    /// Wired up post-Tauri-build via `set_app_handle` so the waiter
    /// thread in TerminalSession can emit `pty_exit` events when the
    /// child process exits. Without this, the frontend has no way to
    /// flip agent tab badges from spinner → ✓/✗.
    app_handle: RwLock<Option<AppHandle>>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.write() = Some(handle);
    }

    pub fn spawn(
        &self,
        id: String,
        task_id: Option<String>,
        opts: SpawnOptions,
        output: Channel<Vec<u8>>,
    ) -> Result<String> {
        let handle = self.app_handle.read().clone();
        let session = TerminalSession::spawn(id.clone(), opts, output, handle)?;
        // Lock order: sessions → by_task (always).
        let mut sessions = self.sessions.write();
        sessions.insert(id.clone(), Arc::new(session));
        if let Some(t) = task_id {
            self.by_task.write().entry(t).or_default().push(id.clone());
        }
        Ok(id)
    }

    pub fn write(&self, id: &str, bytes: &[u8]) -> Result<()> {
        let sessions = self.sessions.read();
        let s = sessions.get(id).ok_or_else(|| anyhow!("no session {id}"))?;
        s.write(bytes)
    }

    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<()> {
        let sessions = self.sessions.read();
        let s = sessions.get(id).ok_or_else(|| anyhow!("no session {id}"))?;
        s.resize(rows, cols)
    }

    pub fn kill(&self, id: &str) -> Result<()> {
        // Lock order: sessions → by_task.
        let removed = self.sessions.write().remove(id);
        {
            let mut by_task = self.by_task.write();
            for ids in by_task.values_mut() {
                ids.retain(|x| x != id);
            }
            by_task.retain(|_, v| !v.is_empty());
        }
        if let Some(arc) = removed {
            // Dropping the Arc (if this was the last ref) triggers
            // TerminalSession::Drop which SIGKILLs the child.
            drop(arc);
        }
        Ok(())
    }

    /// Kill every session associated with a task. Called before the task's
    /// worktrees are removed so agents don't find their CWD yanked out.
    pub fn kill_by_task(&self, task_id: &str) -> usize {
        // Lock order: sessions → by_task (same as `kill`).
        let mut sessions = self.sessions.write();
        let ids = self.by_task.write().remove(task_id).unwrap_or_default();
        let mut killed = 0;
        for id in &ids {
            if let Some(arc) = sessions.remove(id) {
                drop(arc);
                killed += 1;
            }
        }
        killed
    }
}
