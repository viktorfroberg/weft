//! Agent launch: resolves a preset template against a task's context
//! (slug, branch, worktree paths) into a concrete command + args + env
//! for `TerminalManager::spawn`.
//!
//! Template placeholders supported in `args_json` strings and `env_json`
//! values:
//!   {slug}              → task.slug
//!   {branch}            → task.branch_name (`weft/<slug>` default, or
//!                         `feature/<slug>` for ticket-linked tasks)
//!   {primary}           → first ready worktree path
//!   {each_path:<flag>}  → expands to multiple args, one `<flag> <path>` pair
//!                         per ready worktree. Valid only as a standalone
//!                         arg in args_json (NOT usable inside env_json).
//!   {prompt}            → the task's composed initial prompt (user text +
//!                         linked-ticket summary). **Drops the entire argv
//!                         token** when empty / absent — so Claude's
//!                         positional prompt arg isn't passed as `""`,
//!                         which would submit a blank first turn. Caller
//!                         supplies via `resolve_launch(..., initial_prompt)`.
//!                         Valid only as a standalone arg in args_json.
//!
//! Design note: this is deliberately minimal. Enough to express Claude
//! Code's `--name X --add-dir p1 --add-dir p2` invocation without a full
//! template DSL. New agents come by example — add a preset row, adjust
//! placeholders if needed.

use crate::db::repo::{
    AgentPreset, BootstrapDelivery, ProjectRepo, TaskRepo, TaskWorktreeRepo,
};
use crate::model::Task;
use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;

/// Resolved command + args + env ready for `portable-pty`.
#[derive(Debug, Clone)]
pub struct ResolvedLaunch {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Working directory. Always the primary worktree path.
    pub cwd: PathBuf,
    /// What the session id maps back to (for `TerminalManager` task index).
    pub task_id: String,
    /// For logging / UI.
    pub preset_name: String,
    /// Per-launch flag: when true, the bootstrap branch in `expand_args`
    /// is short-circuited. Used by `resolve_launch_resume` so a Claude
    /// `--resume` doesn't get a bootstrap orientation injected on top
    /// of a real conversation history.
    pub suppress_bootstrap: bool,
}

/// Gather task context, apply preset template, return ready-to-spawn struct.
/// Does NOT spawn — the caller passes this into `TerminalManager::spawn`.
///
/// `initial_prompt` fills the `{prompt}` template token. Pass `None` on
/// relaunch (or when the task has no unconsumed prompt) so the token is
/// stripped from argv — Claude Code treats an empty positional prompt as
/// "submit a blank first turn", which we never want.
pub fn resolve_launch(
    conn: &Connection,
    task_id: &str,
    preset: &AgentPreset,
    extra_env: &[(String, String)],
    initial_prompt: Option<&str>,
) -> Result<ResolvedLaunch> {
    let task = TaskRepo::new(conn)
        .get(task_id)?
        .ok_or_else(|| anyhow!("task {task_id} not found"))?;

    let worktrees = TaskWorktreeRepo::new(conn).list_for_task(&task.id)?;
    let ready_paths: Vec<String> = worktrees
        .iter()
        .filter(|w| w.status == "ready")
        .map(|w| w.worktree_path.clone())
        .collect();

    if ready_paths.is_empty() {
        return Err(anyhow!(
            "task {} has no ready worktrees — cannot launch agent",
            task.slug
        ));
    }

    // Pick the primary worktree. Prefer sort_order=0 in workspace_repos if
    // we can, but for v1.0.1 just use the first ready worktree.
    let primary = ready_paths[0].clone();
    let primary_project_id = worktrees
        .iter()
        .find(|w| w.status == "ready")
        .map(|w| w.project_id.clone())
        .unwrap_or_default();

    // Sanity: project row still exists for the primary worktree.
    let _ = ProjectRepo::new(conn).get(&primary_project_id)?;

    // Prefer the branch recorded on a ready worktree (stamped at creation
    // time). Fall back to `task.branch_name` — the source of truth — never
    // reconstruct from slug since ticket-linked tasks use `feature/<slug>`.
    let task_branch = worktrees
        .iter()
        .find(|w| w.status == "ready")
        .map(|w| w.task_branch.clone())
        .unwrap_or_else(|| task.branch_name.clone());

    let ctx = TemplateContext {
        slug: task.slug.clone(),
        branch: task_branch,
        primary: primary.clone(),
        ready_paths: ready_paths.clone(),
        initial_prompt: initial_prompt
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        bootstrap_template: preset.bootstrap_prompt_template.clone(),
        bootstrap_delivery: preset.bootstrap_delivery.unwrap_or(BootstrapDelivery::Argv),
    };

    let raw_args: Vec<String> = serde_json::from_str(&preset.args_json)
        .with_context(|| format!("parse args_json for preset {}", preset.id))?;
    let args = expand_args(&raw_args, &ctx);

    let raw_env: HashMap<String, String> = serde_json::from_str(&preset.env_json)
        .with_context(|| format!("parse env_json for preset {}", preset.id))?;
    let mut env: Vec<(String, String)> = raw_env
        .into_iter()
        .map(|(k, v)| (k, substitute(&v, &ctx)))
        .collect();

    // Always pass-through weft's own env so the agent can report status
    // back via the hook server. Mirrors what `TaskView` injects into the
    // shell spawn — keep the two surfaces in sync.
    env.push(("WEFT_TASK_ID".into(), task.id.clone()));
    env.push(("WEFT_TASK_SLUG".into(), task.slug.clone()));
    env.push(("WEFT_TASK_BRANCH".into(), ctx.branch.clone()));
    env.push((
        "WEFT_HOOKS_URL".into(),
        "http://127.0.0.1:17293/v1/events".into(),
    ));
    // No-deps Claude path: the agent's hook command can `curl
    // --data-binary @-` Claude's raw stdin payload here and weft does
    // the field extraction server-side. Removes the user-side `jq` /
    // `python` requirement that the `/v1/events` path needed for
    // session_id capture.
    env.push((
        "WEFT_HOOKS_URL_CLAUDE".into(),
        "http://127.0.0.1:17293/v1/claude_native".into(),
    ));
    // Bearer token for the hook server. Without this, the agent's status
    // POSTs will 401. Caller injects the token via `extra_env` because the
    // service doesn't hold AppState.
    for (k, v) in extra_env {
        env.push((k.clone(), v.clone()));
    }

    Ok(ResolvedLaunch {
        command: preset.command.clone(),
        args,
        env,
        cwd: PathBuf::from(primary),
        task_id: task.id.clone(),
        preset_name: preset.name.clone(),
        suppress_bootstrap: false,
    })
}

/// Resume an external agent session (today: Claude Code) by splicing
/// `--resume <id>` ahead of the preset's normal args. Used by the
/// dormant-tab reopen path when a captured `task_agent_sessions` row
/// exists for `(task_id, source)`.
///
/// Behavior:
/// 1. `preset.supports_resume` MUST be true. Errors otherwise — this
///    is a programmer-bug guard, not a runtime user condition; the UI
///    only calls this when it has already confirmed the flag.
/// 2. Drops `{prompt}` and `{bootstrap}` tokens entirely (and any
///    preceding literal flag). Resume implies "no new system or user
///    turn — just reattach".
/// 3. Splices `["--resume", session_id]` immediately after the
///    executable, before the preset's other args. Claude's CLI accepts
///    `--resume <id>` in any position, but front-loading keeps it
///    visible in `ps` output for debugging.
/// 4. Sets `suppress_bootstrap = true` for downstream sanity (the
///    bootstrap branches in `expand_args` already short-circuit when
///    no `{prompt}` / `{bootstrap}` token is present, but the flag
///    documents intent and lets future call sites short-circuit
///    earlier).
pub fn resolve_launch_resume(
    conn: &Connection,
    task_id: &str,
    preset: &AgentPreset,
    extra_env: &[(String, String)],
    external_session_id: &str,
) -> Result<ResolvedLaunch> {
    if !preset.supports_resume {
        return Err(anyhow!(
            "preset {} does not support --resume",
            preset.name
        ));
    }
    if external_session_id.trim().is_empty() {
        return Err(anyhow!("external_session_id is empty"));
    }

    // Reuse the normal launch path with `initial_prompt = None` so
    // {prompt} / {bootstrap} tokens drop. Then splice `--resume <id>`
    // at the front of args.
    let mut resolved = resolve_launch(conn, task_id, preset, extra_env, None)?;
    resolved.suppress_bootstrap = true;
    let mut new_args = vec!["--resume".to_string(), external_session_id.to_string()];
    new_args.append(&mut resolved.args);
    resolved.args = new_args;
    Ok(resolved)
}

struct TemplateContext {
    slug: String,
    branch: String,
    primary: String,
    ready_paths: Vec<String>,
    /// Already trimmed + empty-filtered at construction time. `None`
    /// means the `{prompt}` token falls through to bootstrap (argv
    /// delivery mode) or drops (append_system_prompt mode).
    initial_prompt: Option<String>,
    /// Per-preset orientation text for second-agent / reload launches.
    /// Used by the `{prompt}` fallback (argv mode) and by `{bootstrap}`
    /// (append_system_prompt mode).
    bootstrap_template: Option<String>,
    /// Where bootstrap rides in argv. Defaults to `Argv` (portable,
    /// agent-agnostic). `AppendSystemPrompt` keeps bootstrap out of
    /// Claude's visible user transcript.
    bootstrap_delivery: BootstrapDelivery,
}

fn expand_args(raw: &[String], ctx: &TemplateContext) -> Vec<String> {
    let mut out = Vec::with_capacity(raw.len());
    for (i, tmpl) in raw.iter().enumerate() {
        if let Some(flag) = each_path_flag(tmpl) {
            for path in &ctx.ready_paths {
                out.push(flag.to_string());
                out.push(path.clone());
            }
        } else if tmpl == "{prompt}" {
            match resolve_prompt_token(ctx) {
                Some(p) => out.push(p),
                None => drop_preceding_literal_flag(&mut out, raw, i),
            }
        } else if tmpl == "{bootstrap}" {
            match resolve_bootstrap_token(ctx) {
                Some(p) => out.push(p),
                None => drop_preceding_literal_flag(&mut out, raw, i),
            }
        } else {
            out.push(substitute(tmpl, ctx));
        }
    }
    out
}

/// `{prompt}` resolution precedence: the task's fresh user prompt
/// (first-launch) wins. If that's absent AND bootstrap is delivered
/// via argv (the portable path), the bootstrap template fills the
/// slot. AppendSystemPrompt mode leaves `{prompt}` empty and expects
/// `{bootstrap}` elsewhere in args_json.
fn resolve_prompt_token(ctx: &TemplateContext) -> Option<String> {
    if let Some(p) = ctx.initial_prompt.as_deref() {
        return Some(p.to_string());
    }
    if matches!(ctx.bootstrap_delivery, BootstrapDelivery::Argv) {
        if let Some(t) = ctx.bootstrap_template.as_deref() {
            let s = substitute(t, ctx);
            if !s.trim().is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// `{bootstrap}` only fires when no fresh user prompt is available —
/// the first-turn prompt always wins. Returns the expanded template or
/// `None` (in which case the caller drops any preceding `--flag` to
/// avoid orphaning it).
fn resolve_bootstrap_token(ctx: &TemplateContext) -> Option<String> {
    if ctx.initial_prompt.is_some() {
        return None;
    }
    let t = ctx.bootstrap_template.as_deref()?;
    let s = substitute(t, ctx);
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

/// When a `{prompt}` or `{bootstrap}` token resolves empty, a literal
/// `--flag` immediately preceding it (e.g. `"--append-system-prompt"`)
/// would be left stranded. Pop it so we never hand the child process a
/// flag without its value. Only removes flags whose template was a
/// plain literal — variadic `{each_path:<flag>}` is left alone because
/// it already handles its own pairing.
fn drop_preceding_literal_flag(out: &mut Vec<String>, raw: &[String], current_index: usize) {
    if current_index == 0 {
        return;
    }
    let prev_raw = &raw[current_index - 1];
    if !prev_raw.starts_with("--") {
        return;
    }
    if each_path_flag(prev_raw).is_some() {
        return;
    }
    if prev_raw.contains('{') {
        // The previous template was itself a template with placeholders;
        // we don't want to retract something we may have intentionally
        // expanded. Conservative: only drop raw literal flags.
        return;
    }
    if out.last().map(|s| s.as_str()) == Some(prev_raw.as_str()) {
        out.pop();
    }
}

/// If the template is exactly `{each_path:<flag>}`, return `<flag>`.
fn each_path_flag(tmpl: &str) -> Option<&str> {
    tmpl.strip_prefix("{each_path:")
        .and_then(|rest| rest.strip_suffix('}'))
}

fn substitute(tmpl: &str, ctx: &TemplateContext) -> String {
    tmpl.replace("{slug}", &ctx.slug)
        .replace("{branch}", &ctx.branch)
        .replace("{primary}", &ctx.primary)
}

/// Fetch the `Task` struct — helper used by callers that don't want to
/// wrangle `TaskRepo` themselves.
#[allow(dead_code)]
pub fn fetch_task(conn: &Connection, task_id: &str) -> Result<Task> {
    TaskRepo::new(conn)
        .get(task_id)?
        .ok_or_else(|| anyhow!("task {task_id} not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_preset(args_json: &str) -> AgentPreset {
        AgentPreset {
            id: "p".into(),
            name: "test".into(),
            command: "claude".into(),
            args_json: args_json.into(),
            env_json: "{}".into(),
            is_default: true,
            sort_order: 0,
            created_at: 0,
            bootstrap_prompt_template: None,
            bootstrap_delivery: None,
            supports_resume: false,
        }
    }

    fn mk_ctx() -> TemplateContext {
        TemplateContext {
            slug: "chat-widget".into(),
            branch: "weft/chat-widget".into(),
            primary: "/tmp/wt/admin".into(),
            ready_paths: vec!["/tmp/wt/admin".into(), "/tmp/wt/api".into()],
            initial_prompt: None,
            bootstrap_template: None,
            bootstrap_delivery: BootstrapDelivery::Argv,
        }
    }

    #[test]
    fn expand_literal_and_placeholder() {
        let p = mk_preset(r#"["--name","{slug}"]"#);
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = mk_ctx();
        let out = expand_args(&raw, &ctx);
        assert_eq!(out, vec!["--name", "chat-widget"]);
        let _ = p.env_json; // silence unused
    }

    #[test]
    fn expand_each_path() {
        let p = mk_preset(r#"["{each_path:--add-dir}"]"#);
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = mk_ctx();
        let out = expand_args(&raw, &ctx);
        assert_eq!(
            out,
            vec!["--add-dir", "/tmp/wt/admin", "--add-dir", "/tmp/wt/api"]
        );
    }

    #[test]
    fn expand_mixed_claude_code_template() {
        let p = mk_preset(r#"["--name","{slug}","{each_path:--add-dir}"]"#);
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = mk_ctx();
        let out = expand_args(&raw, &ctx);
        assert_eq!(
            out,
            vec![
                "--name",
                "chat-widget",
                "--add-dir",
                "/tmp/wt/admin",
                "--add-dir",
                "/tmp/wt/api",
            ]
        );
    }

    fn ctx_with(initial: Option<&str>, bootstrap: Option<&str>, delivery: BootstrapDelivery) -> TemplateContext {
        TemplateContext {
            slug: "x".into(),
            branch: "weft/x".into(),
            primary: "/tmp/wt".into(),
            ready_paths: vec!["/tmp/wt".into()],
            initial_prompt: initial.map(str::to_string),
            bootstrap_template: bootstrap.map(str::to_string),
            bootstrap_delivery: delivery,
        }
    }

    #[test]
    fn each_path_with_no_paths_emits_nothing() {
        let p = mk_preset(r#"["--flag","{each_path:--add-dir}","tail"]"#);
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = TemplateContext {
            slug: "x".into(),
            branch: "weft/x".into(),
            primary: "".into(),
            ready_paths: vec![],
            initial_prompt: None,
            bootstrap_template: None,
            bootstrap_delivery: BootstrapDelivery::Argv,
        };
        let out = expand_args(&raw, &ctx);
        assert_eq!(out, vec!["--flag", "tail"]);
    }

    #[test]
    fn prompt_token_pushes_single_argv_entry_when_present() {
        let p = mk_preset(
            r#"["--name","{slug}","{each_path:--add-dir}","{prompt}"]"#,
        );
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = ctx_with(
            Some("fix the bug\n\nLinked ticket:\n- PRO-1"),
            None,
            BootstrapDelivery::Argv,
        );
        let out = expand_args(&raw, &ctx);
        assert_eq!(
            out,
            vec![
                "--name",
                "x",
                "--add-dir",
                "/tmp/wt",
                "fix the bug\n\nLinked ticket:\n- PRO-1",
            ]
        );
    }

    #[test]
    fn prompt_token_dropped_when_no_prompt_no_bootstrap() {
        let p = mk_preset(
            r#"["--name","{slug}","{each_path:--add-dir}","{prompt}"]"#,
        );
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = ctx_with(None, None, BootstrapDelivery::Argv);
        let out = expand_args(&raw, &ctx);
        assert_eq!(out, vec!["--name", "x", "--add-dir", "/tmp/wt"]);
    }

    #[test]
    fn prompt_token_falls_through_to_bootstrap_in_argv_mode() {
        let p = mk_preset(r#"["--name","{slug}","{prompt}"]"#);
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = ctx_with(None, Some("join via {slug}"), BootstrapDelivery::Argv);
        let out = expand_args(&raw, &ctx);
        assert_eq!(out, vec!["--name", "x", "join via x"]);
    }

    #[test]
    fn prompt_present_wins_over_bootstrap_in_argv_mode() {
        let p = mk_preset(r#"["--name","{slug}","{prompt}"]"#);
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = ctx_with(
            Some("user intent"),
            Some("bootstrap text"),
            BootstrapDelivery::Argv,
        );
        let out = expand_args(&raw, &ctx);
        assert_eq!(out, vec!["--name", "x", "user intent"]);
    }

    #[test]
    fn prompt_token_does_not_fall_through_in_append_mode() {
        // AppendSystemPrompt keeps {prompt} strictly for the user turn;
        // bootstrap rides on {bootstrap} elsewhere in args_json.
        let p = mk_preset(r#"["--name","{slug}","{prompt}"]"#);
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = ctx_with(
            None,
            Some("bootstrap text"),
            BootstrapDelivery::AppendSystemPrompt,
        );
        let out = expand_args(&raw, &ctx);
        assert_eq!(out, vec!["--name", "x"]);
    }

    #[test]
    fn bootstrap_token_expands_when_no_user_prompt() {
        let p = mk_preset(
            r#"["--name","{slug}","{prompt}","--append-system-prompt","{bootstrap}"]"#,
        );
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = ctx_with(
            None,
            Some("read .weft/context.md in {primary}"),
            BootstrapDelivery::AppendSystemPrompt,
        );
        let out = expand_args(&raw, &ctx);
        assert_eq!(
            out,
            vec![
                "--name",
                "x",
                "--append-system-prompt",
                "read .weft/context.md in /tmp/wt",
            ]
        );
    }

    #[test]
    fn bootstrap_yields_to_user_prompt_in_append_mode() {
        let p = mk_preset(
            r#"["--name","{slug}","{prompt}","--append-system-prompt","{bootstrap}"]"#,
        );
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = ctx_with(
            Some("user intent"),
            Some("bootstrap text"),
            BootstrapDelivery::AppendSystemPrompt,
        );
        let out = expand_args(&raw, &ctx);
        // --append-system-prompt is orphaned when bootstrap yields —
        // drop it so we don't hand claude `--append-system-prompt`
        // with no value.
        assert_eq!(out, vec!["--name", "x", "user intent"]);
    }

    #[test]
    fn resume_helper_rejects_non_resume_preset() {
        // Build a real DB so resolve_launch (which resume calls into)
        // has its task + worktree rows.
        use crate::db::repo::{
            NewProject, NewTask, NewTaskWorktree, NewWorkspace, NewWorkspaceRepo, ProjectRepo,
            TaskRepo, TaskWorktreeRepo, WorkspaceRepoRepo, WorkspacesRepo,
        };
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
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

        let (ws, _) = WorkspacesRepo::new(&conn)
            .insert(NewWorkspace { name: "w".into(), sort_order: None })
            .unwrap();
        let (p, _) = ProjectRepo::new(&conn)
            .insert(NewProject {
                name: "p".into(),
                main_repo_path: "/tmp/fake".into(),
                default_branch: "main".into(),
                color: None,
            })
            .unwrap();
        WorkspaceRepoRepo::new(&conn)
            .insert(NewWorkspaceRepo {
                workspace_id: ws.id.clone(),
                project_id: p.id.clone(),
                base_branch: None,
                sort_order: None,
            })
            .unwrap();
        let (t, _) = TaskRepo::new(&conn)
            .insert(NewTask {
                workspace_id: Some(ws.id),
                name: "z".into(),
                agent_preset: None,
                initial_prompt: None,
            })
            .unwrap();
        let td = tempfile::tempdir().unwrap();
        TaskWorktreeRepo::new(&conn)
            .insert(NewTaskWorktree {
                task_id: t.id.clone(),
                project_id: p.id,
                worktree_path: td.path().to_string_lossy().into_owned(),
                task_branch: "weft/z".into(),
                base_branch: "main".into(),
                status: "ready".into(),
            })
            .unwrap();

        let mut p = mk_preset(r#"["--name","{slug}"]"#);
        p.supports_resume = false;
        let err = resolve_launch_resume(&conn, &t.id, &p, &[], "sid").unwrap_err();
        assert!(format!("{err}").contains("does not support --resume"));
    }

    #[test]
    fn resume_helper_splices_resume_args_and_drops_prompt() {
        use crate::db::repo::{
            NewProject, NewTask, NewTaskWorktree, NewWorkspace, NewWorkspaceRepo, ProjectRepo,
            TaskRepo, TaskWorktreeRepo, WorkspaceRepoRepo, WorkspacesRepo,
        };
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
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

        let (ws, _) = WorkspacesRepo::new(&conn)
            .insert(NewWorkspace { name: "w".into(), sort_order: None })
            .unwrap();
        let (p, _) = ProjectRepo::new(&conn)
            .insert(NewProject {
                name: "p".into(),
                main_repo_path: "/tmp/fake".into(),
                default_branch: "main".into(),
                color: None,
            })
            .unwrap();
        WorkspaceRepoRepo::new(&conn)
            .insert(NewWorkspaceRepo {
                workspace_id: ws.id.clone(),
                project_id: p.id.clone(),
                base_branch: None,
                sort_order: None,
            })
            .unwrap();
        let (t, _) = TaskRepo::new(&conn)
            .insert(NewTask {
                workspace_id: Some(ws.id),
                name: "z".into(),
                agent_preset: None,
                initial_prompt: None,
            })
            .unwrap();
        let td = tempfile::tempdir().unwrap();
        TaskWorktreeRepo::new(&conn)
            .insert(NewTaskWorktree {
                task_id: t.id.clone(),
                project_id: p.id,
                worktree_path: td.path().to_string_lossy().into_owned(),
                task_branch: "weft/z".into(),
                base_branch: "main".into(),
                status: "ready".into(),
            })
            .unwrap();

        let mut preset = mk_preset(r#"["--name","{slug}","{prompt}"]"#);
        preset.supports_resume = true;
        let resolved =
            resolve_launch_resume(&conn, &t.id, &preset, &[], "sess-xyz").unwrap();
        // --resume must be first; {prompt} must be dropped (no initial prompt).
        assert_eq!(resolved.args, vec!["--resume", "sess-xyz", "--name", "z"]);
        assert!(resolved.suppress_bootstrap);
    }

    #[test]
    fn orphan_flag_dropped_on_empty_bootstrap() {
        let p = mk_preset(
            r#"["--name","{slug}","--append-system-prompt","{bootstrap}","{each_path:--add-dir}"]"#,
        );
        let raw: Vec<String> = serde_json::from_str(&p.args_json).unwrap();
        let ctx = ctx_with(None, None, BootstrapDelivery::AppendSystemPrompt);
        let out = expand_args(&raw, &ctx);
        // No bootstrap template -> both the flag and its argv slot drop.
        assert_eq!(out, vec!["--name", "x", "--add-dir", "/tmp/wt"]);
    }
}
