use super::session::{ExitMode, SpawnOptions, TerminalSession};
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
    /// session_id → tab_id, for the dormant-writeback path in
    /// `commands::terminal::terminal_alive_sessions_worth_warning` and
    /// for the graceful-shutdown UX (which references tabs, not sessions).
    session_to_tab: RwLock<HashMap<String, String>>,
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
        let tab_id = opts.tab_id.clone();
        let session = TerminalSession::spawn(id.clone(), opts, output, handle)?;
        // Lock order: sessions → by_task → session_to_tab (always).
        let mut sessions = self.sessions.write();
        sessions.insert(id.clone(), Arc::new(session));
        if let Some(t) = task_id {
            self.by_task.write().entry(t).or_default().push(id.clone());
        }
        if let Some(t) = tab_id {
            self.session_to_tab.write().insert(id.clone(), t);
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
        // Lock order: sessions → by_task → session_to_tab.
        let removed = self.sessions.write().remove(id);
        {
            let mut by_task = self.by_task.write();
            for ids in by_task.values_mut() {
                ids.retain(|x| x != id);
            }
            by_task.retain(|_, v| !v.is_empty());
        }
        self.session_to_tab.write().remove(id);
        if let Some(arc) = removed {
            // Dropping the Arc (if this was the last ref) triggers
            // TerminalSession::Drop which SIGKILLs the child.
            drop(arc);
        }
        Ok(())
    }

    /// Graceful shutdown. Signals the child, awaits the waiter's
    /// post-exit dormant-write/scrollback persist, then removes the
    /// session from the registry. Safe to call from async command
    /// handlers via `spawn_blocking` if the caller needs to await this.
    pub fn shutdown_graceful(&self, id: &str, timeout_ms: u64) -> Result<ExitMode> {
        // Take an Arc clone WITHOUT removing from the registry yet — the
        // waiter still needs the session to exist for its in-flight
        // dormant-write path, and the reader/flusher threads hold their
        // own Arc clones anyway.
        let session = {
            let sessions = self.sessions.read();
            sessions.get(id).cloned()
        };
        let session = session.ok_or_else(|| anyhow!("no session {id}"))?;
        let mode = session.shutdown_graceful(timeout_ms);

        // Now teardown the registry entry. Arc drop triggers the
        // SIGKILL-by-pid fallback which is a no-op since the child's
        // already reaped.
        self.kill(id)?;
        Ok(mode)
    }

    /// Returns `(session_id, tab_id)` for every currently-live session
    /// whose exit would lose work worth warning about. Used by the quit
    /// dialog. Excludes sessions whose child has already exited
    /// (waiter flipped `child_exited`).
    pub fn alive_sessions(&self) -> Vec<AliveSession> {
        let sessions = self.sessions.read();
        let s2t = self.session_to_tab.read();
        sessions
            .iter()
            .filter(|(_, s)| s.is_alive())
            .map(|(sid, s)| AliveSession {
                session_id: sid.clone(),
                tab_id: s2t.get(sid).cloned(),
                pid: s.pid(),
            })
            .collect()
    }

    pub fn tab_id_of(&self, session_id: &str) -> Option<String> {
        self.session_to_tab.read().get(session_id).cloned()
    }

    /// Kill every session associated with a task. Called before the task's
    /// worktrees are removed so agents don't find their CWD yanked out.
    pub fn kill_by_task(&self, task_id: &str) -> usize {
        // Lock order: sessions → by_task → session_to_tab.
        let mut sessions = self.sessions.write();
        let ids = self.by_task.write().remove(task_id).unwrap_or_default();
        let mut s2t = self.session_to_tab.write();
        let mut killed = 0;
        for id in &ids {
            if let Some(arc) = sessions.remove(id) {
                drop(arc);
                killed += 1;
            }
            s2t.remove(id);
        }
        killed
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AliveSession {
    pub session_id: String,
    pub tab_id: Option<String>,
    pub pid: Option<u32>,
}
