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
        .route("/v1/install-lock", post(install_lock))
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
    // Constant-time compare to avoid timing oracles — cheap and correct.
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
