pub mod commands;
pub mod db;
pub mod debug;
pub mod git;
pub mod hooks;
pub mod integrations;
pub mod menu;
pub mod model;
pub mod services;
pub mod task;
pub mod terminal;

use crate::debug::install_panic_hook;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Tauri-managed state. Held for the process lifetime.
pub struct AppState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    pub hook_status: Arc<hooks::StatusStore>,
    pub hook_port: Arc<Mutex<Option<u16>>>,
    pub hook_token: Arc<Mutex<Option<String>>>,
    pub terminals: Arc<terminal::TerminalManager>,
    /// Per-session record of `(project_id, path)` pairs where a `clone`
    /// link type fell back to `symlink` because the source volume isn't
    /// APFS-cloneable. Prevents retrying `clonefile(2)` on every task
    /// and spamming the log. Reset on app restart.
    pub clone_fallbacks: Arc<Mutex<HashSet<(String, String)>>>,
    /// Per-project install serializer. Shared between the hook HTTP
    /// handler and (future) Tauri-side callers. Agent install wrappers
    /// in `contrib/install-lock/` use this via `/v1/install-lock`.
    pub install_locks: Arc<hooks::InstallLockStore>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                // weft=debug surfaces the command-boundary traces that
                // make it possible to diagnose a UI hang from the dev
                // terminal alone (DevTools can't open when the webview
                // locks up). Set RUST_LOG to override.
                .unwrap_or_else(|_| "weft=debug,warn".into()),
        )
        .with_target(true)
        .with_file(false)
        .init();

    install_panic_hook();

    let db_conn = db::open_and_migrate().expect("failed to open/migrate database");

    // Write our PID so a concurrent `weft-cli` invocation can warn the user
    // that the UI won't see its writes until refetch. Best-effort only —
    // a failed write doesn't block the app.
    if let Err(e) = db::write_app_pid() {
        tracing::warn!(error = %e, "could not write weft.pid");
    }

    if let Err(e) = services::reconcile::reconcile_worktrees(&db_conn) {
        tracing::warn!(error = %e, "startup reconcile failed");
    }

    let db = Arc::new(Mutex::new(db_conn));

    // Hydrate the in-memory StatusStore from SQLite before the hook server
    // starts accepting events. Prevents spurious "Idle → Working" transitions
    // on startup when agents re-post their current state.
    let store = Arc::new(hooks::StatusStore::new());
    hooks::status::hydrate_from_db(&store, &db.lock().expect("db lock"));

    let hook_port = Arc::new(Mutex::new(None::<u16>));
    let hook_token = Arc::new(Mutex::new(None::<String>));
    let install_locks = Arc::new(hooks::InstallLockStore::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            db: Arc::clone(&db),
            hook_status: Arc::clone(&store),
            hook_port: Arc::clone(&hook_port),
            hook_token: Arc::clone(&hook_token),
            terminals: Arc::new(terminal::TerminalManager::new()),
            clone_fallbacks: Arc::new(Mutex::new(HashSet::new())),
            install_locks: Arc::clone(&install_locks),
        })
        .setup(move |app| {
            // Native macOS menu bar — wires ⌘-shortcut discoverability to
            // every global action. Menu items emit "menu" events with the
            // item id as payload.
            if let Err(e) = menu::build_and_install(app.handle()) {
                tracing::warn!(error = %e, "install menu failed");
            }

            // Hook server needs AppHandle to emit db_events when it writes
            // task status changes through to SQLite. Start it here, not in
            // `run()`, so AppHandle is available.
            let handle = app.handle().clone();
            let ctx = hooks::server::AppCtx {
                store: Arc::clone(&store),
                db: Arc::clone(&db),
                app: Some(handle),
                token: String::new(), // filled in by start_server
                install_locks: Arc::clone(&install_locks),
            };
            let port_cell = Arc::clone(&hook_port);
            let token_cell = Arc::clone(&hook_token);

            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name("weft-hooks")
                .build()
                .expect("build hook-server runtime");

            let handle = runtime
                .block_on(hooks::start_server(ctx))
                .expect("start hook server");
            *port_cell.lock().expect("hook_port lock") = Some(handle.port);
            *token_cell.lock().expect("hook_token lock") = Some(handle.token.clone());

            // Leak: server + runtime live for the app lifetime.
            std::mem::forget(handle);
            std::mem::forget(runtime);

            // Wire the AppHandle into TerminalManager so the waiter
            // thread can emit `pty_exit` events on child exit.
            {
                use tauri::Manager;
                let state_handle = app.state::<AppState>();
                state_handle.terminals.set_app_handle(app.handle().clone());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::projects::projects_list,
            commands::projects::project_create,
            commands::projects::project_delete,
            commands::projects::project_set_color,
            commands::projects::project_rename,
            commands::workspaces::workspaces_list,
            commands::workspaces::workspace_create,
            commands::workspaces::workspace_delete,
            commands::workspaces::workspace_repos_list,
            commands::workspaces::workspace_add_repo,
            commands::workspaces::workspace_remove_repo,
            commands::tasks::tasks_list,
            commands::tasks::tasks_list_all,
            commands::tasks::task_project_ids,
            commands::tasks::task_worktrees_list,
            commands::tasks::task_create,
            commands::tasks::task_delete,
            commands::tasks::task_add_repo,
            commands::tasks::task_remove_repo,
            commands::tasks::task_open_in_editor,
            commands::tasks::task_tickets_list,
            commands::tasks::task_tickets_by_provider,
            commands::tasks::task_consume_initial_prompt,
            commands::tasks::task_link_ticket,
            commands::tasks::task_unlink_ticket,
            commands::tasks::task_refresh_ticket_titles,
            commands::tasks::task_rename,
            commands::tasks::task_context_get,
            commands::tasks::task_context_set,
            commands::git::git_is_repo,
            commands::git::git_default_branch,
            commands::terminal::terminal_spawn,
            commands::terminal::terminal_write,
            commands::terminal::terminal_resize,
            commands::terminal::terminal_kill,
            commands::terminal::agent_launch,
            commands::presets::presets_list,
            commands::presets::preset_default,
            commands::changes::task_changes_by_repo,
            commands::changes::worktree_file_sides,
            commands::changes::worktree_commit,
            commands::changes::worktree_discard,
            commands::changes::task_commit_all,
            commands::settings::app_info,
            commands::integrations::integration_list,
            commands::integrations::integration_set_token,
            commands::integrations::integration_clear,
            commands::integrations::integration_test,
            commands::integrations::ticket_list_backlog,
            commands::integrations::ticket_get,
            commands::integrations::linear_settings_get,
            commands::integrations::linear_settings_set,
            commands::project_links::project_links_list,
            commands::project_links::project_links_set,
            commands::project_links::project_links_preset_apply,
            commands::project_links::project_links_presets_list,
            commands::project_links::project_links_detect_preset,
            commands::project_links::project_links_reapply,
            commands::project_links::project_links_warm_up_main,
            commands::project_links::project_links_health,
            commands::devlog::dev_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
