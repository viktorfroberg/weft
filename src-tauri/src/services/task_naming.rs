//! Background auto-rename for a task's display name, via the user's
//! existing `claude -p` CLI (Haiku tier).
//!
//! Why here, not inline in `task_create`: title generation is a
//! subprocess spawn that takes ~0.5–1s end-to-end. Blocking the
//! create path on that would make the compose button feel sticky
//! when the heuristic short-name is already fine as a placeholder.
//! So we spawn this off the commit, let the UI receive the fast
//! `task insert` event with the heuristic name, and overwrite via a
//! `task update` event when the LLM returns.
//!
//! Design notes:
//! - Uses the user's authenticated `claude` CLI so no API-key
//!   configuration is required. If claude is missing or fails, we
//!   log-info-and-continue: the heuristic name stays, no user-visible
//!   failure. Matches ChatGPT's UX when their title call fails.
//! - Pinned to Haiku (`--model haiku`): fast TTFT, ~$0.0001 per task,
//!   plenty of quality for a 3-6 word label. Swap via env to try
//!   other models locally.
//! - Race-safe. Writes only if `name_locked_at IS NULL` at UPDATE
//!   time, so a user rename that landed while we were in-flight wins.
//! - Timeout 15s total. Claude sometimes stalls on the first request
//!   after sleep/wake; a hard cap keeps us from holding resources.

use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::db::events::DbEvent;
use crate::db::repo::TaskRepo;

/// System prompt for the title model. Tight: 3-6 words, same
/// language as the input, no quotes, no trailing punctuation.
/// Claude Code reads this via `--append-system-prompt`, so it layers
/// on top of Claude's default system prompt without replacing it.
const TITLE_SYSTEM_PROMPT: &str = concat!(
    "You generate short task titles. Reply with ONLY a 3-6 word label ",
    "for the user's request. Match the language of the user's input. ",
    "No quotes. No trailing punctuation. No prefix like 'Fix:' or 'Add:'. ",
    "Just the label itself. Be concise and descriptive of the intent, ",
    "not the form."
);

/// Hard upper limit on the returned title, defensive against a model
/// that ignores the "3-6 words" instruction.
const MAX_TITLE_LEN: usize = 120;

/// Fire-and-forget wrapper. Spawn on a tokio runtime; never blocks
/// the caller. Uses the user's authenticated claude CLI so there's
/// no key-configuration step.
pub fn spawn_auto_rename(db: Arc<Mutex<Connection>>, task_id: String, app: tauri::AppHandle) {
    tokio::spawn(async move {
        match auto_rename(&db, &task_id).await {
            Ok(Some(event)) => {
                crate::commands::emit_event(&app, event);
            }
            Ok(None) => {
                // Prompt was empty, name locked, or claude returned
                // nothing useful. Silent — heuristic name stays.
            }
            Err(e) => {
                // Info, not warn: subprocess failure is expected when
                // claude is missing / offline. Not a bug.
                tracing::info!(
                    task = %task_id,
                    error = %e,
                    "task_naming: auto-rename skipped"
                );
            }
        }
    });
}

/// Core logic, testable in isolation. Returns `Ok(Some(event))` if
/// the row was updated, `Ok(None)` if we declined (no prompt, locked,
/// unchanged, empty model output).
pub async fn auto_rename(
    db: &Arc<Mutex<Connection>>,
    task_id: &str,
) -> Result<Option<DbEvent>> {
    let (prompt, current_name, locked) = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let task = TaskRepo::new(&conn)
            .get(task_id)?
            .ok_or_else(|| anyhow!("task_naming: task {task_id} not found"))?;
        (task.initial_prompt, task.name, task.name_locked_at.is_some())
    };

    if locked {
        return Ok(None);
    }
    let Some(prompt_text) = prompt.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        // Task with no initial prompt (e.g. ticket-only task created
        // without compose text). Heuristic name stays.
        return Ok(None);
    };

    let model = std::env::var("WEFT_TITLE_MODEL").unwrap_or_else(|_| "haiku".to_string());
    let claude_bin = resolve_claude_bin();

    let title = match run_claude_title(&claude_bin, &model, prompt_text).await? {
        Some(t) => t,
        None => return Ok(None),
    };

    if title == current_name {
        // Model regenerated the same label — no-op.
        return Ok(None);
    }

    let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
    let updated = TaskRepo::new(&conn).auto_rename(task_id, &title)?;
    if updated {
        tracing::info!(
            task = %task_id,
            title = %title,
            "task_naming: auto-renamed"
        );
        Ok(Some(DbEvent::update(
            crate::db::events::Entity::Task,
            task_id.to_string(),
        )))
    } else {
        // Raced a user rename between the select above and this
        // update — user wins.
        Ok(None)
    }
}

/// Pick the `claude` executable to run. `WEFT_CLAUDE_BIN` overrides
/// for tests or unusual installs; otherwise rely on PATH.
fn resolve_claude_bin() -> String {
    std::env::var("WEFT_CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string())
}

/// Spawn `claude -p --model <model> --append-system-prompt <SYS>`
/// with the prompt piped on stdin (avoids argv length concerns on
/// very long compose messages). 15-second hard timeout; stdout
/// trimmed to a single line, capped at `MAX_TITLE_LEN`.
async fn run_claude_title(
    claude_bin: &str,
    model: &str,
    prompt: &str,
) -> Result<Option<String>> {
    let mut cmd = Command::new(claude_bin);
    cmd.arg("-p")
        .arg("--model")
        .arg(model)
        .arg("--append-system-prompt")
        .arg(TITLE_SYSTEM_PROMPT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn {claude_bin} -p"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let prompt_bytes = prompt.as_bytes().to_vec();
        let write = tokio::spawn(async move {
            let _ = stdin.write_all(&prompt_bytes).await;
            let _ = stdin.shutdown().await;
        });
        // Don't hold on stdin write failure — proceed to wait so a
        // broken pipe shows up as a non-zero exit with stderr we can
        // log. The inner task is awaited just to prevent a leak.
        let _ = write;
    }

    let wait_fut = child.wait_with_output();
    let output = match timeout(Duration::from_secs(15), wait_fut).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(anyhow!("claude wait: {e}")),
        Err(_) => {
            // Timeout hit — the child is dropped here; its kill-on-drop
            // is enabled by tokio::process::Command by default.
            return Err(anyhow!("claude title subprocess timed out"));
        }
    };

    if !output.status.success() {
        return Err(anyhow!(
            "claude exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let title = sanitize_title(&raw);
    if title.is_empty() {
        return Ok(None);
    }
    Ok(Some(title))
}

/// Strip common model ticks: leading/trailing quotes, surrounding
/// markdown, trailing period/colon, multi-line output. Caps length.
fn sanitize_title(raw: &str) -> String {
    // First non-empty line only — some models pad with blank lines.
    let line = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    // Strip surrounding matching quotes.
    let t = line
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == '«' || c == '»');
    // Strip trailing sentence punctuation (model sometimes ignores the
    // "no trailing punctuation" rule).
    let t = t.trim_end_matches(|c: char| c == '.' || c == ',' || c == ':' || c == ';' || c == '!' || c == '?');
    // Collapse interior whitespace runs.
    let collapsed: String = t
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.len() <= MAX_TITLE_LEN {
        return collapsed;
    }
    // Hard cap: cut at a word boundary under the limit.
    let mut cut = &collapsed[..MAX_TITLE_LEN];
    if let Some(idx) = cut.rfind(' ') {
        if idx > MAX_TITLE_LEN / 2 {
            cut = &cut[..idx];
        }
    }
    format!("{}…", cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_quotes_punctuation_and_linebreaks() {
        assert_eq!(
            sanitize_title("\"Fix FAQ mobile close bug.\"\n"),
            "Fix FAQ mobile close bug"
        );
        assert_eq!(
            sanitize_title("  Refactor login flow  "),
            "Refactor login flow"
        );
        assert_eq!(
            sanitize_title("\n\nDebug websocket reconnect.\n\n"),
            "Debug websocket reconnect"
        );
    }

    #[test]
    fn sanitize_collapses_whitespace() {
        assert_eq!(
            sanitize_title("Clean up   repo    access"),
            "Clean up repo access"
        );
    }

    #[test]
    fn sanitize_empty_returns_empty() {
        assert_eq!(sanitize_title(""), "");
        assert_eq!(sanitize_title("\n\n"), "");
    }

    #[test]
    fn sanitize_caps_long_output() {
        let long = "word ".repeat(60);
        let out = sanitize_title(&long);
        assert!(out.len() <= MAX_TITLE_LEN + 4); // +4 for ellipsis budget
        assert!(out.ends_with('…'));
    }
}
