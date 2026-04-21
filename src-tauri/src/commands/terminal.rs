use crate::db::repo::PresetRepo;
use crate::services::agent_launch::resolve_launch;
use crate::terminal::SpawnOptions;
use crate::AppState;
use std::path::PathBuf;
use tauri::ipc::Channel;
use tauri::State;
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
    };
    state
        .terminals
        .spawn(id.clone(), input.task_id, opts, channel)
        .map_err(|e| e.to_string())?;
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
    task_id: String,
    preset_id: Option<String>,
    rows: u16,
    cols: u16,
    initial_prompt: Option<String>,
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
    };

    state
        .terminals
        .spawn(session_id.clone(), Some(resolved.task_id), opts, channel)
        .map_err(|e| e.to_string())?;

    tracing::info!(
        preset = %resolved.preset_name,
        session = %session_id,
        "agent launched"
    );
    Ok(session_id)
}
