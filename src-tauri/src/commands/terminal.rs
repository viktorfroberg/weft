use crate::commands::emit_event;
use crate::db::repo::{
    AgentSessionRow, NewTerminalTab, PresetRepo, TabKind, TaskSessionRepo, TerminalTabRepo,
    TerminalTabRow,
};
use crate::services::agent_launch::{resolve_launch, resolve_launch_resume};
use crate::terminal::{scrollback_path, ExitMode, SpawnOptions, DEFAULT_SHUTDOWN_MS};
use crate::AppState;
use std::path::PathBuf;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct SpawnInput {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    pub rows: u16,
    pub cols: u16,
    /// Optional task this session belongs to. When the task is deleted,
    /// `TerminalManager::kill_by_task` tears down every session tagged here.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Optional persistent-tab binding. When present, the waiter thread
    /// flips the row to dormant + persists scrollback on child exit.
    #[serde(default)]
    pub tab_id: Option<String>,
}

/// Spawns a PTY. Returns a freshly-generated session id that the frontend
/// uses for subsequent write/resize/kill calls.
///
/// `channel` is a Tauri v2 `Channel<Vec<u8>>` — the frontend constructs it
/// with `new Channel()` and passes it in as part of the invoke payload. The
/// Rust reader thread streams bytes into it.
#[tauri::command]
pub fn terminal_spawn(
    state: State<'_, AppState>,
    app: AppHandle,
    input: SpawnInput,
    channel: Channel<Vec<u8>>,
) -> Result<String, String> {
    crate::timed!("terminal_spawn");
    let id = Uuid::now_v7().to_string();
    tracing::info!(
        target: "weft::pty",
        session_id = %id,
        task_id = ?input.task_id,
        command = %input.command,
        cwd = %input.cwd,
        rows = input.rows,
        cols = input.cols,
        "spawn",
    );
    let opts = SpawnOptions {
        command: input.command,
        args: input.args,
        cwd: PathBuf::from(input.cwd),
        env: input.env,
        rows: input.rows,
        cols: input.cols,
        tab_id: input.tab_id.clone(),
    };
    state
        .terminals
        .spawn(id.clone(), input.task_id, opts, channel)
        .map_err(|e| format!("{e:#}"))?;

    // If this spawn is bound to a persistent tab, ensure the row is
    // `live` now (was dormant before, user just resumed).
    if let Some(tab_id) = input.tab_id.as_ref() {
        let maybe_ev = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            TerminalTabRepo::new(&conn).mark_live(tab_id).ok()
        };
        if let Some(ev) = maybe_ev {
            emit_event(&app, ev);
        }
    }
    Ok(id)
}

#[tauri::command]
pub fn terminal_write(
    state: State<'_, AppState>,
    id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    state.terminals.write(&id, &data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn terminal_resize(
    state: State<'_, AppState>,
    id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    state
        .terminals
        .resize(&id, rows, cols)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn terminal_kill(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::timed!("terminal_kill");
    tracing::info!(target: "weft::pty", session_id = %id, "kill");
    state.terminals.kill(&id).map_err(|e| e.to_string())
}

/// Graceful PTY teardown. Signals the child (SIGHUP → SIGTERM → SIGKILL
/// over `timeout_ms`), awaits the waiter's dormant writeback + scrollback
/// persist, then removes the session. `timeout_ms = None` uses
/// `DEFAULT_SHUTDOWN_MS` (5 s). Returns which signal the child eventually
/// responded to — a `Kill` return indicates a misbehaving child worth
/// logging upstream.
#[tauri::command]
pub async fn terminal_shutdown_graceful(
    state: State<'_, AppState>,
    id: String,
    timeout_ms: Option<u64>,
) -> Result<ExitMode, String> {
    crate::timed!("terminal_shutdown_graceful");
    let terminals = std::sync::Arc::clone(&state.terminals);
    let id_moved = id.clone();
    let timeout = timeout_ms.unwrap_or(DEFAULT_SHUTDOWN_MS);
    // Graceful shutdown polls atomics with std::thread::sleep — do it on
    // a blocking task so we don't occupy a tokio worker.
    let mode = tokio::task::spawn_blocking(move || {
        terminals.shutdown_graceful(&id_moved, timeout)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    tracing::info!(target: "weft::pty", session_id = %id, ?mode, "shutdown_graceful done");
    Ok(mode)
}

#[derive(serde::Serialize)]
pub struct AliveSessionView {
    pub session_id: String,
    pub tab_id: Option<String>,
    pub task_id: Option<String>,
    pub label: Option<String>,
    pub kind: Option<TabKind>,
}

/// Enumerate live PTY sessions whose exit is worth warning the user
/// about on quit. Filters out bare idle shells (no foreground child
/// process besides the shell itself).
#[tauri::command]
pub fn terminal_alive_sessions_worth_warning(
    state: State<'_, AppState>,
) -> Result<Vec<AliveSessionView>, String> {
    let raw = state.terminals.alive_sessions();
    if raw.is_empty() {
        return Ok(vec![]);
    }

    // Resolve tab metadata in a single DB lock.
    let tabs: Vec<(String, TerminalTabRow)> = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let repo = TerminalTabRepo::new(&conn);
        raw.iter()
            .filter_map(|s| s.tab_id.as_ref().map(|t| (s.session_id.clone(), t.clone())))
            .filter_map(|(sid, tid)| repo.get(&tid).ok().flatten().map(|row| (sid, row)))
            .collect()
    };

    let mut out = Vec::<AliveSessionView>::new();
    for s in &raw {
        let tab = tabs
            .iter()
            .find(|(sid, _)| sid == &s.session_id)
            .map(|(_, r)| r);
        let worth = match tab {
            // Agents always warn while live.
            Some(row) if row.kind == TabKind::Agent => true,
            // Shell: warn only if it has a non-self descendant process.
            Some(row) if row.kind == TabKind::Shell => {
                s.pid.map(shell_has_foreground_job).unwrap_or(false)
            }
            // Sessions not bound to a persistent tab: conservatively
            // warn (unknown provenance is better flagged than silently
            // killed).
            None => true,
            _ => false,
        };
        if !worth {
            continue;
        }
        out.push(AliveSessionView {
            session_id: s.session_id.clone(),
            tab_id: s.tab_id.clone(),
            task_id: tab.map(|r| r.task_id.clone()),
            label: tab.map(|r| r.label.clone()),
            kind: tab.map(|r| r.kind),
        });
    }
    Ok(out)
}

/// macOS/Linux: does any other process have this shell pid as its
/// parent? True = foreground job running, warn on close. False = idle
/// prompt. Best-effort — if `ps` fails we return false (skip warning)
/// because a spurious warning is worse than a missed one for bare shells.
#[cfg(unix)]
pub(crate) fn shell_has_foreground_job(shell_pid: u32) -> bool {
    use std::process::Command;
    let out = match Command::new("ps")
        .args(["-o", "ppid=", "-A"])
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return false,
    };
    let target = shell_pid.to_string();
    std::str::from_utf8(&out)
        .ok()
        .map(|s| s.lines().any(|line| line.trim() == target))
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub(crate) fn shell_has_foreground_job(_shell_pid: u32) -> bool {
    true
}

// ---------- Persistent tab commands ----------

#[derive(serde::Deserialize)]
pub struct TabCreateInput {
    pub task_id: String,
    pub kind: TabKind,
    pub label: String,
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[tauri::command]
pub fn tab_list(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<TerminalTabRow>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    TerminalTabRepo::new(&conn)
        .list_for_task(&task_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn tab_create(
    state: State<'_, AppState>,
    app: AppHandle,
    input: TabCreateInput,
) -> Result<TerminalTabRow, String> {
    let id = Uuid::now_v7().to_string();
    let (row, ev) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        TerminalTabRepo::new(&conn)
            .insert(NewTerminalTab {
                id,
                task_id: input.task_id,
                kind: input.kind,
                label: input.label,
                preset_id: input.preset_id,
                cwd: input.cwd,
            })
            .map_err(|e| e.to_string())?
    };
    emit_event(&app, ev);
    Ok(row)
}

#[tauri::command]
pub fn tab_delete(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
) -> Result<(), String> {
    // Unlink the scrollback file inline. Reconciler catches anything we
    // miss (e.g. race with waiter persisting right now), but this is the
    // happy-path cleanup that keeps the dir lean.
    if let Ok(path) = scrollback_path(&id) {
        let _ = std::fs::remove_file(&path);
    }
    let ev = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        TerminalTabRepo::new(&conn)
            .delete(&id)
            .map_err(|e| e.to_string())?
    };
    emit_event(&app, ev);
    Ok(())
}

/// Read the persisted scrollback for a dormant tab. Returns an empty
/// Vec if no file exists (tab never wrote output, or reconciler removed
/// an orphan). The bytes are sanitized at record time — safe to `term.write`.
#[tauri::command]
pub fn tab_scrollback_read(id: String) -> Result<Vec<u8>, String> {
    let path = scrollback_path(&id).map_err(|e| e.to_string())?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.to_string()),
    }
}

/// Force-exit the process. Called from the frontend's QuitConfirmDialog
/// after `RunEvent::ExitRequested` was prevented. We call
/// `std::process::exit` directly rather than re-entering Tauri's exit
/// flow — per Tauri v2 docs: after `prevent_exit`, don't try to
/// re-trigger the same exit event, just exit the process.
#[tauri::command]
pub fn app_exit(code: i32) {
    tracing::info!(code, "app_exit from frontend");
    std::process::exit(code);
}

/// Launch a coding agent inside a new PTY session attached to `task_id`.
/// Uses the preset's template to build command + args + env, cwd set to
/// the primary worktree. If `preset_id` is None, the default preset is
/// used. Returns the new session id.
///
/// `initial_prompt` fills the preset's `{prompt}` template token (if
/// present). Passing `None` on relaunch drops the token entirely so
/// Claude doesn't auto-submit a stale first message.
#[tauri::command]
pub fn agent_launch(
    state: State<'_, AppState>,
    app: AppHandle,
    task_id: String,
    preset_id: Option<String>,
    rows: u16,
    cols: u16,
    initial_prompt: Option<String>,
    tab_id: Option<String>,
    channel: Channel<Vec<u8>>,
) -> Result<String, String> {
    crate::timed!("agent_launch");
    tracing::info!(
        target: "weft::pty",
        %task_id,
        ?preset_id,
        has_initial_prompt = initial_prompt.is_some(),
        "agent_launch",
    );
    // Pick up the hook auth token so agents can POST status events back.
    let token = state
        .hook_token
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let extra_env: Vec<(String, String)> = token
        .into_iter()
        .map(|t| ("WEFT_HOOKS_TOKEN".to_string(), t))
        .collect();

    // Caller-provided `initial_prompt` wins (legacy / explicit paths).
    // Otherwise ask the context service to compose the first-turn
    // message from `tasks.initial_prompt` + cached ticket titles —
    // but only if `initial_prompt_consumed_at` is still null, i.e.
    // this is genuinely the first agent launch for the task. After
    // that, we fall through to the preset's bootstrap_prompt_template.
    let effective_prompt: Option<String> = if initial_prompt.is_some() {
        initial_prompt
    } else {
        let consumed = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            crate::db::repo::TaskRepo::new(&conn)
                .get(&task_id)
                .map_err(|e| e.to_string())?
                .and_then(|t| t.initial_prompt_consumed_at)
                .is_some()
        };
        if consumed {
            None
        } else {
            crate::services::task_context::compose_first_turn(&state.db, &task_id)
                .map_err(|e| e.to_string())?
        }
    };

    let resolved = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let preset_repo = PresetRepo::new(&conn);
        let preset = match preset_id {
            Some(id) => preset_repo.get(&id).map_err(|e| e.to_string())?,
            None => preset_repo.get_default().map_err(|e| e.to_string())?,
        }
        .ok_or_else(|| "no agent preset available".to_string())?;

        resolve_launch(
            &conn,
            &task_id,
            &preset,
            &extra_env,
            effective_prompt.as_deref(),
        )
        .map_err(|e| e.to_string())?
    };

    let session_id = Uuid::now_v7().to_string();
    let opts = SpawnOptions {
        command: resolved.command,
        args: resolved.args,
        cwd: resolved.cwd,
        env: resolved.env,
        rows,
        cols,
        tab_id: tab_id.clone(),
    };

    state
        .terminals
        .spawn(session_id.clone(), Some(resolved.task_id), opts, channel)
        .map_err(|e| format!("{e:#}"))?;

    // Persistent-tab resume: flip the row back to live so the UI stops
    // rendering the dormant transcript overlay.
    if let Some(tid) = tab_id.as_ref() {
        let maybe_ev = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            TerminalTabRepo::new(&conn).mark_live(tid).ok()
        };
        if let Some(ev) = maybe_ev {
            emit_event(&app, ev);
        }
    }

    tracing::info!(
        preset = %resolved.preset_name,
        session = %session_id,
        "agent launched"
    );
    Ok(session_id)
}

/// Resume a previously-captured external agent session. Splices
/// `--resume <external_session_id>` into the preset's args and spawns
/// a fresh PTY in the task's primary worktree. Used by the dormant-tab
/// reopen path when a `task_agent_sessions` row exists for the source.
///
/// `tab_id` binds the new session to the persistent tab so the waiter
/// flips it back to dormant + persists scrollback on next exit.
#[tauri::command]
pub fn agent_launch_resume(
    state: State<'_, AppState>,
    app: AppHandle,
    task_id: String,
    preset_id: Option<String>,
    rows: u16,
    cols: u16,
    external_session_id: String,
    tab_id: Option<String>,
    channel: Channel<Vec<u8>>,
) -> Result<String, String> {
    crate::timed!("agent_launch_resume");
    tracing::info!(
        target: "weft::pty",
        %task_id,
        ?preset_id,
        sid = %external_session_id,
        ?tab_id,
        "agent_launch_resume",
    );

    let token = state
        .hook_token
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let extra_env: Vec<(String, String)> = token
        .into_iter()
        .map(|t| ("WEFT_HOOKS_TOKEN".to_string(), t))
        .collect();

    let resolved = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let preset_repo = PresetRepo::new(&conn);
        let preset = match preset_id {
            Some(id) => preset_repo.get(&id).map_err(|e| e.to_string())?,
            None => preset_repo.get_default().map_err(|e| e.to_string())?,
        }
        .ok_or_else(|| "no agent preset available".to_string())?;
        resolve_launch_resume(&conn, &task_id, &preset, &extra_env, &external_session_id)
            .map_err(|e| format!("{e:#}"))?
    };

    let session_id = Uuid::now_v7().to_string();
    let opts = SpawnOptions {
        command: resolved.command,
        args: resolved.args,
        cwd: resolved.cwd,
        env: resolved.env,
        rows,
        cols,
        tab_id: tab_id.clone(),
    };

    state
        .terminals
        .spawn(session_id.clone(), Some(resolved.task_id), opts, channel)
        .map_err(|e| format!("{e:#}"))?;

    if let Some(tid) = tab_id.as_ref() {
        let maybe_ev = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            TerminalTabRepo::new(&conn).mark_live(tid).ok()
        };
        if let Some(ev) = maybe_ev {
            emit_event(&app, ev);
        }
    }

    tracing::info!(
        preset = %resolved.preset_name,
        session = %session_id,
        sid = %external_session_id,
        "agent resumed"
    );
    Ok(session_id)
}

/// Look up the captured external agent session id for a task+source.
/// Returns null when nothing has been captured yet (the user's hook
/// adapter doesn't forward `session_id`, or no agent has run for this
/// task yet).
#[tauri::command]
pub fn task_agent_session_get(
    state: State<'_, AppState>,
    task_id: String,
    source: String,
) -> Result<Option<AgentSessionRow>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    TaskSessionRepo::new(&conn)
        .get(&task_id, &source)
        .map_err(|e| e.to_string())
}
