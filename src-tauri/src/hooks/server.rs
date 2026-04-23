use crate::db::events::{DbEvent, Entity, DB_EVENT_CHANNEL};
use crate::db::data_dir;
use crate::hooks::events::HookEvent;
use crate::hooks::install_lock::{InstallLockStore, LockAction, LockRequest, LockResponse};
use crate::hooks::status::{task_status_as_str, StatusStore};
use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use rusqlite::Connection;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub const DEFAULT_PORT: u16 = 17293;

/// Per-launch shared secret agents must present as `Authorization: Bearer <token>`.
/// Written alongside the port in `hooks.json` so locally-spawned agents can
/// read both atomically. Prevents a rogue localhost process (e.g. a
/// postinstall script) from spam-flipping task status.
#[derive(Clone)]
pub struct AppCtx {
    pub store: Arc<StatusStore>,
    pub db: Arc<Mutex<Connection>>,
    pub app: Option<AppHandle>,
    pub token: String,
    /// Per-project install serializer. Same Arc lives on `AppState` so
    /// (future) Tauri-side callers can check lock state without going
    /// through HTTP.
    pub install_locks: Arc<InstallLockStore>,
}

pub struct HookServerHandle {
    pub port: u16,
    pub token: String,
    pub join: JoinHandle<()>,
}

#[derive(Serialize)]
struct HooksManifest<'a> {
    port: u16,
    token: &'a str,
    pid: u32,
    started_at: i64,
}

fn random_token() -> String {
    // v7 is time-ordered + has a random portion; stringified it's a
    // perfectly good 32-char opaque secret for local-machine auth.
    // (We already pull uuid with v7 feature for ids.)
    uuid::Uuid::now_v7().simple().to_string()
}

/// Start the hook-ingest HTTP server. See `hooks::mod` for IPC rationale.
pub async fn start_server(mut ctx: AppCtx) -> Result<HookServerHandle> {
    let token = random_token();
    ctx.token = token.clone();

    let listener = match TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT))).await
    {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "hook server: preferred port {DEFAULT_PORT} unavailable, picking ephemeral"
            );
            TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
                .await
                .context("bind ephemeral port")?
        }
    };
    let port = listener.local_addr()?.port();

    // Write the manifest. hooks.json is the canonical discovery file for
    // agents; hooks.port is kept as a compatibility shim for anything that
    // already reads the old path.
    let dir = data_dir()?;
    let manifest = HooksManifest {
        port,
        token: &token,
        pid: std::process::id(),
        started_at: time::OffsetDateTime::now_utc().unix_timestamp(),
    };
    let manifest_path = dir.join("hooks.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)
        .with_context(|| format!("write {}", manifest_path.display()))?;
    // Token is a secret — restrict to owner-only on unix. Macs default to
    // user-only Application Support but umask 022 lets other users read
    // individual files.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(
            &manifest_path,
            std::fs::Permissions::from_mode(0o600),
        );
    }
    let port_path = dir.join("hooks.port");
    std::fs::write(&port_path, port.to_string())
        .with_context(|| format!("write {}", port_path.display()))?;
    tracing::info!(port, manifest = %manifest_path.display(), "hook server listening");

    let app = Router::new()
        .route("/healthz", axum::routing::get(healthz))
        .route("/v1/events", post(ingest_event))
        .route("/v1/claude_native", post(ingest_claude_native))
        .route("/v1/install-lock", post(install_lock))
        .route("/v1/task_context_append", post(append_context_note))
        .with_state(ctx);

    let join = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "hook server exited");
        }
    });

    Ok(HookServerHandle { port, token, join })
}

async fn healthz() -> &'static str {
    "ok"
}

fn token_ok(headers: &HeaderMap, expected: &str) -> bool {
    // Empty expected = dev/test; skip auth.
    if expected.is_empty() {
        return true;
    }
    token_ok_strict(headers, expected)
}

/// Like `token_ok` but never honors the empty-expected dev bypass.
/// Used by mutating endpoints that write to disk — losing a status
/// event in dev is annoying, corrupting `.weft/context.md` isn't.
fn token_ok_strict(headers: &HeaderMap, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let Some(auth) = headers.get("authorization") else {
        return false;
    };
    let Ok(value) = auth.to_str() else {
        return false;
    };
    let prefix = "Bearer ";
    if !value.starts_with(prefix) {
        return false;
    }
    let supplied = &value[prefix.len()..];
    if supplied.len() != expected.len() {
        return false;
    }
    supplied
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

async fn ingest_event(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(event): Json<HookEvent>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !token_ok(&headers, &ctx.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid token"})),
        );
    }

    // Capture the agent's external session id, if the payload carries
    // one. Claude Code's hooks include `session_id` at the top level of
    // every payload — the user's hook command pulls it out of stdin
    // (see docs/agents.md) and forwards it as `detail.session_id`.
    //
    // Auth split: writing this row drives the resume mechanism, so we
    // bypass the empty-bearer dev-mode tolerance that the status path
    // accepts. Forging session ids on a dev box would let any localhost
    // process hijack which Claude session a tab resumes into.
    if let Some(sid) = event
        .detail
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if token_ok_strict(&headers, &ctx.token) {
            let conn = ctx.db.lock().unwrap_or_else(|e| e.into_inner());
            let repo = crate::db::repo::TaskSessionRepo::new(&conn);
            match repo.upsert(&event.task_id, &event.source, sid) {
                Ok(db_event) => {
                    if let Some(app) = ctx.app.as_ref() {
                        if let Err(e) = app.emit(DB_EVENT_CHANNEL, &db_event) {
                            tracing::warn!(error = %e, "session_id db_event emit failed");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        task = %event.task_id,
                        source = %event.source,
                        error = %e,
                        "session_id upsert failed"
                    );
                }
            }
        }
    }

    let result = ctx.store.apply(&event);

    if result.changed {
        let status_str = task_status_as_str(result.to);
        let task_id = event.task_id.clone();
        let rows_updated = {
            // Recover from a poisoned lock: the data is still consistent
            // because rusqlite doesn't break on panic, only the mutex
            // does. Taking `into_inner` lets us keep serving hooks even
            // if some other code path panicked while holding db.
            let conn = ctx
                .db
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match conn.execute(
                "UPDATE tasks SET status = ?1 WHERE id = ?2",
                rusqlite::params![status_str, task_id],
            ) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(task = %task_id, error = %e, "db update failed");
                    0
                }
            }
        };
        if rows_updated == 0 {
            tracing::debug!(task = %task_id, "hook for unknown task_id — ignoring status write");
        } else if let Some(app) = ctx.app.as_ref() {
            let db_event = DbEvent::update(Entity::Task, task_id.clone());
            if let Err(e) = app.emit(DB_EVENT_CHANNEL, &db_event) {
                tracing::warn!(error = %e, "emit db_event failed");
            }
        }
    }

    tracing::debug!(
        source = %event.source,
        task = %event.task_id,
        kind = ?event.kind,
        from = ?result.from,
        to = ?result.to,
        changed = result.changed,
        "hook event"
    );
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": result.to,
            "changed": result.changed,
        })),
    )
}

/// Accept a Claude Code hook payload as-is. Claude pipes its native
/// JSON to the hook command's stdin; users `curl --data-binary @-` it
/// here verbatim. weft does the field extraction server-side so the
/// hook config has zero parser dependencies (no `jq`, no `python -c`).
///
/// Wire shape:
///   POST /v1/claude_native
///   Authorization: Bearer <WEFT_HOOKS_TOKEN>
///   X-Weft-Task-Id: <WEFT_TASK_ID>
///   Content-Type: application/json
///   Body: Claude's raw hook payload (any of the documented event kinds)
///
/// Claude payload fields we care about (rest is ignored / stored in
/// `detail`):
///   - `session_id`  → upserted to `task_agent_sessions` (for `--resume`)
///   - `hook_event_name` → mapped to `EventKind` for `StatusStore.apply`
///
/// Auth: status updates use the lenient bearer (`token_ok`) for parity
/// with `/v1/events`. The session_id capture path requires
/// `token_ok_strict` — same split as `/v1/events` for the same reason
/// (a forged session id corrupts the resume target).
async fn ingest_claude_native(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    if !token_ok(&headers, &ctx.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid token"})),
        );
    }

    let Some(task_id) = headers
        .get("x-weft-task-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "missing X-Weft-Task-Id header",
            })),
        );
    };

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("invalid json: {e}"),
                })),
            );
        }
    };

    let hook_event_name: String = payload
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let kind = map_claude_hook_to_kind(&hook_event_name);

    // Capture session_id under strict bearer (no empty-token dev bypass).
    if let Some(sid) = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if token_ok_strict(&headers, &ctx.token) {
            let conn = ctx.db.lock().unwrap_or_else(|e| e.into_inner());
            let repo = crate::db::repo::TaskSessionRepo::new(&conn);
            match repo.upsert(task_id, "claude_code", sid) {
                Ok(db_event) => {
                    if let Some(app) = ctx.app.as_ref() {
                        if let Err(e) = app.emit(DB_EVENT_CHANNEL, &db_event) {
                            tracing::warn!(error = %e, "claude_native session_id db_event emit failed");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        task = %task_id,
                        error = %e,
                        "claude_native session_id upsert failed"
                    );
                }
            }
        }
    }

    // Status update: synthesize a HookEvent and apply via the same path
    // as /v1/events so behavior stays in lockstep.
    let event = HookEvent {
        source: "claude_code".to_string(),
        task_id: task_id.to_string(),
        kind,
        detail: payload,
        timestamp: time::OffsetDateTime::now_utc().unix_timestamp(),
    };
    let result = ctx.store.apply(&event);

    if result.changed {
        let status_str = task_status_as_str(result.to);
        let task_id_owned = event.task_id.clone();
        let rows_updated = {
            let conn = ctx.db.lock().unwrap_or_else(|e| e.into_inner());
            match conn.execute(
                "UPDATE tasks SET status = ?1 WHERE id = ?2",
                rusqlite::params![status_str, task_id_owned],
            ) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(task = %task_id_owned, error = %e, "claude_native db update failed");
                    0
                }
            }
        };
        if rows_updated == 0 {
            tracing::debug!(task = %task_id_owned, "claude_native hook for unknown task — ignoring");
        } else if let Some(app) = ctx.app.as_ref() {
            let db_event = DbEvent::update(Entity::Task, task_id_owned);
            if let Err(e) = app.emit(DB_EVENT_CHANNEL, &db_event) {
                tracing::warn!(error = %e, "claude_native task db_event emit failed");
            }
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": result.to,
            "changed": result.changed,
            "hook": hook_event_name,
        })),
    )
}

/// Claude Code's `hook_event_name` → weft's coarse `EventKind`. The
/// granularity gap is intentional: weft's UI only cares about
/// "is it working / waiting / errored / done", not which tool fired.
/// All Claude work events collapse to `Active` so the spinner stays on
/// during a long tool chain. `Stop` is the moment Claude is done
/// generating and is waiting on the next user turn → `WaitingInput`.
fn map_claude_hook_to_kind(name: &str) -> crate::hooks::events::EventKind {
    use crate::hooks::events::EventKind;
    match name {
        "Stop" | "Notification" => EventKind::WaitingInput,
        "SessionEnd" => EventKind::SessionEnd,
        // Everything else (SessionStart, PreToolUse, PostToolUse,
        // UserPromptSubmit, SubagentStop, PreCompact, future events
        // we haven't seen) → Active.
        _ => EventKind::Active,
    }
}

/// Max body for a single appended note. 4 KiB is comfortable for
/// status updates / short paragraphs; larger payloads suggest the
/// caller should be editing the notes block via ContextDialog, not
/// firing discrete HTTP notes.
const MAX_NOTE_LEN: usize = 4096;

#[derive(serde::Deserialize)]
struct AppendNoteReq {
    task_id: String,
    note: String,
}

async fn append_context_note(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(req): Json<AppendNoteReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !token_ok_strict(&headers, &ctx.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid token"})),
        );
    }
    if req.task_id.trim().is_empty() || req.note.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "task_id and note are required"})),
        );
    }
    if req.note.len() > MAX_NOTE_LEN {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": format!("note exceeds {MAX_NOTE_LEN}-byte cap")
            })),
        );
    }

    match crate::services::task_context::append_note(&ctx.db, &req.task_id, &req.note) {
        Ok(()) => {
            if let Some(app) = ctx.app.as_ref() {
                let ev = DbEvent::update(Entity::Task, req.task_id.clone());
                if let Err(e) = app.emit(DB_EVENT_CHANNEL, &ev) {
                    tracing::warn!(error = %e, "task_context_append: emit db_event failed");
                }
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({"appended": true})),
            )
        }
        Err(e) => {
            let msg = e.to_string();
            // Crude mapping — the service uses `anyhow!` strings.
            let status = if msg.contains("task not found") {
                StatusCode::NOT_FOUND
            } else if msg.contains("no ready worktrees") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            tracing::warn!(task = %req.task_id, error = %msg, "task_context_append failed");
            (status, Json(serde_json::json!({"error": msg})))
        }
    }
}

async fn install_lock(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(req): Json<LockRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !token_ok(&headers, &ctx.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid token"})),
        );
    }
    match req.kind {
        LockAction::Acquire => match ctx
            .install_locks
            .acquire(&req.project_id, &req.holder_id)
            .await
        {
            Ok(()) => (
                StatusCode::OK,
                Json(serde_json::to_value(LockResponse { ok: true }).unwrap()),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"ok": false, "error": e.to_string()})),
            ),
        },
        LockAction::Release => {
            ctx.install_locks.release(&req.project_id, &req.holder_id);
            (
                StatusCode::OK,
                Json(serde_json::to_value(LockResponse { ok: true }).unwrap()),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::events::EventKind;
    use std::net::SocketAddr;

    fn mk_ctx(token: &str) -> AppCtx {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../migrations/0001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0002_schema.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0004_task_tickets_and_branch.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0006_initial_prompt.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0010_task_name_locked_at.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0003_agent_presets.sql"))
            .unwrap();
        conn.execute_batch(include_str!(
            "../../migrations/0012_agent_sessions_and_resume.sql"
        ))
        .unwrap();
        AppCtx {
            store: Arc::new(StatusStore::new()),
            db: Arc::new(Mutex::new(conn)),
            app: None,
            token: token.into(),
            install_locks: Arc::new(InstallLockStore::new()),
        }
    }

    async fn start_test_server(ctx: AppCtx) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = Router::new()
            .route("/healthz", axum::routing::get(healthz))
            .route("/v1/events", post(ingest_event))
            .route("/v1/claude_native", post(ingest_claude_native))
            .route("/v1/install-lock", post(install_lock))
            .with_state(ctx);
        let join = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (port, join)
    }

    #[tokio::test]
    async fn healthz_responds_ok_without_auth() {
        let ctx = mk_ctx("secret");
        let (port, join) = start_test_server(ctx).await;
        let body = http_get(port, "/healthz").await;
        assert_eq!(body, "ok");
        join.abort();
    }

    #[tokio::test]
    async fn rejects_unauthed_event_post() {
        let ctx = mk_ctx("secret");
        let (port, join) = start_test_server(ctx).await;
        let event = HookEvent {
            source: "test".into(),
            task_id: "t1".into(),
            kind: EventKind::Active,
            detail: serde_json::Value::Null,
            timestamp: 1,
        };
        let resp = http_post_json(port, "/v1/events", &event, None).await;
        assert!(resp.contains("invalid token"), "got: {resp}");
        join.abort();
    }

    #[tokio::test]
    async fn accepts_authed_event_post() {
        let ctx = mk_ctx("secret");

        // Seed a task so the DB UPDATE has a row to hit.
        use crate::db::repo::{NewTask, NewWorkspace, TaskRepo, WorkspacesRepo};
        let task_id = {
            let conn = ctx.db.lock().unwrap();
            let (ws, _) = WorkspacesRepo::new(&conn)
                .insert(NewWorkspace {
                    name: "w".into(),
                    sort_order: None,
                })
                .unwrap();
            let (task, _) = TaskRepo::new(&conn)
                .insert(NewTask {
                    workspace_id: Some(ws.id),
                    name: "t".into(),
                    agent_preset: None,
                    initial_prompt: None,
                })
                .unwrap();
            task.id
        };

        let (port, join) = start_test_server(ctx.clone()).await;
        let event = HookEvent {
            source: "test".into(),
            task_id: task_id.clone(),
            kind: EventKind::Active,
            detail: serde_json::Value::Null,
            timestamp: 1,
        };
        let resp = http_post_json(port, "/v1/events", &event, Some("secret")).await;
        assert!(resp.contains("working"), "got: {resp}");

        let conn = ctx.db.lock().unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM tasks WHERE id = ?1",
                [&task_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "working");

        join.abort();
    }

    #[tokio::test]
    async fn captures_session_id_with_real_bearer() {
        let ctx = mk_ctx("secret");

        use crate::db::repo::{
            NewTask, NewWorkspace, TaskRepo, TaskSessionRepo, WorkspacesRepo,
        };
        let task_id = {
            let conn = ctx.db.lock().unwrap();
            let (ws, _) = WorkspacesRepo::new(&conn)
                .insert(NewWorkspace {
                    name: "w".into(),
                    sort_order: None,
                })
                .unwrap();
            let (task, _) = TaskRepo::new(&conn)
                .insert(NewTask {
                    workspace_id: Some(ws.id),
                    name: "t".into(),
                    agent_preset: None,
                    initial_prompt: None,
                })
                .unwrap();
            task.id
        };

        let (port, join) = start_test_server(ctx.clone()).await;
        let event = HookEvent {
            source: "claude_code".into(),
            task_id: task_id.clone(),
            kind: EventKind::Active,
            detail: serde_json::json!({"session_id": "claude-sess-abc"}),
            timestamp: 1,
        };
        let _ = http_post_json(port, "/v1/events", &event, Some("secret")).await;

        let conn = ctx.db.lock().unwrap();
        let row = TaskSessionRepo::new(&conn)
            .get(&task_id, "claude_code")
            .unwrap()
            .expect("session row should be written");
        assert_eq!(row.external_session_id, "claude-sess-abc");

        join.abort();
    }

    #[tokio::test]
    async fn claude_native_extracts_session_id_and_status() {
        let ctx = mk_ctx("secret");

        use crate::db::repo::{
            NewTask, NewWorkspace, TaskRepo, TaskSessionRepo, WorkspacesRepo,
        };
        let task_id = {
            let conn = ctx.db.lock().unwrap();
            let (ws, _) = WorkspacesRepo::new(&conn)
                .insert(NewWorkspace { name: "w".into(), sort_order: None })
                .unwrap();
            let (task, _) = TaskRepo::new(&conn)
                .insert(NewTask {
                    workspace_id: Some(ws.id),
                    name: "t".into(),
                    agent_preset: None,
                    initial_prompt: None,
                })
                .unwrap();
            task.id
        };

        let (port, join) = start_test_server(ctx.clone()).await;

        // Claude's native PreToolUse-shaped payload (abbreviated to
        // the fields weft cares about; everything else just rides in
        // detail and is ignored).
        let raw = serde_json::json!({
            "session_id": "claude-sess-xyz",
            "transcript_path": "/tmp/whatever.jsonl",
            "cwd": "/tmp",
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
        });
        let _ = http_post_claude_native(port, &task_id, &raw, Some("secret")).await;

        // session_id captured.
        let conn = ctx.db.lock().unwrap();
        let row = TaskSessionRepo::new(&conn)
            .get(&task_id, "claude_code")
            .unwrap()
            .expect("session row should be written");
        assert_eq!(row.external_session_id, "claude-sess-xyz");

        // Status flipped to working (PreToolUse → Active).
        let status: String = conn
            .query_row("SELECT status FROM tasks WHERE id = ?1", [&task_id], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "working");

        join.abort();
    }

    #[tokio::test]
    async fn claude_native_stop_event_flips_to_waiting() {
        let ctx = mk_ctx("secret");
        use crate::db::repo::{NewTask, NewWorkspace, TaskRepo, WorkspacesRepo};
        let task_id = {
            let conn = ctx.db.lock().unwrap();
            let (ws, _) = WorkspacesRepo::new(&conn)
                .insert(NewWorkspace { name: "w".into(), sort_order: None })
                .unwrap();
            let (task, _) = TaskRepo::new(&conn)
                .insert(NewTask {
                    workspace_id: Some(ws.id),
                    name: "t".into(),
                    agent_preset: None,
                    initial_prompt: None,
                })
                .unwrap();
            task.id
        };

        let (port, join) = start_test_server(ctx.clone()).await;
        let raw = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "Stop",
        });
        let _ = http_post_claude_native(port, &task_id, &raw, Some("secret")).await;

        let conn = ctx.db.lock().unwrap();
        let status: String = conn
            .query_row("SELECT status FROM tasks WHERE id = ?1", [&task_id], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "waiting");
        join.abort();
    }

    #[tokio::test]
    async fn claude_native_rejects_missing_task_header() {
        let ctx = mk_ctx("secret");
        let (port, join) = start_test_server(ctx).await;
        let raw = serde_json::json!({"session_id": "s1", "hook_event_name": "Stop"});
        let resp = http_post_claude_native_no_task_header(port, &raw, Some("secret")).await;
        assert!(resp.contains("X-Weft-Task-Id"), "got: {resp}");
        join.abort();
    }

    async fn http_post_claude_native(
        port: u16,
        task_id: &str,
        body: &serde_json::Value,
        token: Option<&str>,
    ) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let body_s = serde_json::to_string(body).unwrap();
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "POST /v1/claude_native HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-Weft-Task-Id: {task_id}\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body_s}",
            body_s.len()
        );
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let s = String::from_utf8_lossy(&buf).to_string();
        s.split("\r\n\r\n").nth(1).unwrap_or("").to_string()
    }

    async fn http_post_claude_native_no_task_header(
        port: u16,
        body: &serde_json::Value,
        token: Option<&str>,
    ) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let body_s = serde_json::to_string(body).unwrap();
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "POST /v1/claude_native HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body_s}",
            body_s.len()
        );
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let s = String::from_utf8_lossy(&buf).to_string();
        s.split("\r\n\r\n").nth(1).unwrap_or("").to_string()
    }

    #[tokio::test]
    async fn empty_bearer_does_not_write_session_id() {
        // ctx with empty token — the status path's dev bypass kicks in
        // (token_ok returns true), but session_id capture uses
        // token_ok_strict and should refuse.
        let ctx = mk_ctx("");

        use crate::db::repo::{
            NewTask, NewWorkspace, TaskRepo, TaskSessionRepo, WorkspacesRepo,
        };
        let task_id = {
            let conn = ctx.db.lock().unwrap();
            let (ws, _) = WorkspacesRepo::new(&conn)
                .insert(NewWorkspace {
                    name: "w".into(),
                    sort_order: None,
                })
                .unwrap();
            let (task, _) = TaskRepo::new(&conn)
                .insert(NewTask {
                    workspace_id: Some(ws.id),
                    name: "t".into(),
                    agent_preset: None,
                    initial_prompt: None,
                })
                .unwrap();
            task.id
        };

        let (port, join) = start_test_server(ctx.clone()).await;
        let event = HookEvent {
            source: "claude_code".into(),
            task_id: task_id.clone(),
            kind: EventKind::Active,
            detail: serde_json::json!({"session_id": "should-not-stick"}),
            timestamp: 1,
        };
        let _ = http_post_json(port, "/v1/events", &event, None).await;

        let conn = ctx.db.lock().unwrap();
        let row = TaskSessionRepo::new(&conn)
            .get(&task_id, "claude_code")
            .unwrap();
        assert!(row.is_none(), "session row should NOT be written without strict bearer");

        join.abort();
    }

    async fn http_get(port: u16, path: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let s = String::from_utf8_lossy(&buf).to_string();
        s.split("\r\n\r\n").nth(1).unwrap_or("").to_string()
    }

    async fn http_post_json<T: serde::Serialize>(
        port: u16,
        path: &str,
        body: &T,
        token: Option<&str>,
    ) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let body_s = serde_json::to_string(body).unwrap();
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body_s}",
            body_s.len()
        );
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let s = String::from_utf8_lossy(&buf).to_string();
        s.split("\r\n\r\n").nth(1).unwrap_or("").to_string()
    }
}
