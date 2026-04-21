//! Shared task context. Two artifacts, both auto-regenerated on every
//! task mutation (task create, ticket link/unlink, repo add/remove):
//!
//! 1. **Agent-agnostic primary:** `<wt>/.weft/context.md` in every ready
//!    worktree of the task. Two fenced sections — an `weft:auto` block
//!    composed from DB state (user prompt, linked tickets, repos) and a
//!    `weft:notes` block that user and agents may both hand-edit. Auto
//!    regen replaces only the auto block, preserving notes byte-for-byte.
//!
//! 2. **Claude-specific mirror:** `~/.weft/worktrees/<slug>/CLAUDE.md`
//!    at the task-root directory (ABOVE per-repo worktrees). Claude's
//!    memory loader walks from cwd up to `/`, so this file is
//!    auto-discovered alongside any repo-local CLAUDE.md — both are
//!    loaded, not overridden. Mirror content only (no notes block)
//!    because notes live in the per-worktree sidecars where agents
//!    write them.
//!
//! **Do not call `git add .weft/`.** The `.weft/` directory is
//! globally gitignored via common `info/exclude` (see task #62) —
//! that's deliberate so agents doing `git add -A` don't stage weft
//! plumbing into user commits. Do not remove the exclude.
//!
//! **Concurrency.** Per-worktree writes go through an advisory flock
//! on `<wt>/.weft/context.md.lock` + atomic rename from a
//! `.tmp.<pid>` sibling, so a user linking a ticket while an agent is
//! mid-write to the notes block doesn't truncate or clobber the
//! agent's edit. Both readers and writers serialize on the same lock.
//!
//! **Malformed splicer input** (unmatched markers, duplicated fences,
//! out-of-order) is not silently merged. The file is renamed to
//! `.context.md.corrupt.<unix_ts>` and a fresh one is written. A warn
//! log records the quarantine path so the user/agent can recover.

use anyhow::{anyhow, Context, Result};
use fs2::FileExt;
use rusqlite::Connection;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::db::repo::{ProjectRepo, TaskRepo, TaskTicketRepo, TaskTicketRow, TaskWorktreeRepo};

const CONTEXT_FILENAME: &str = "context.md";
const CLAUDE_FILENAME: &str = "CLAUDE.md";
const AUTO_BEGIN: &str = "<!-- weft:auto-begin — regenerated on task changes, do not hand-edit -->";
const AUTO_END: &str = "<!-- weft:auto-end -->";
const NOTES_BEGIN: &str = "<!-- weft:notes-begin — free-form scratch space; user AND agents may edit; preserved across regeneration -->";
const NOTES_END: &str = "<!-- weft:notes-end -->";

// ---------------------------------------------------------------------------
// Fence splicer — small state machine. Parses a context.md into
// (auto_block, notes_block). On any malformed input the caller gets a
// `Quarantine` variant with a reason and the whole original contents;
// writer quarantines the file and starts fresh.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedSections {
    /// Clean input: both fences present exactly once, in order.
    Parsed { auto: String, notes: String },
    /// Missing both fences — the whole file is pre-v1 user content.
    /// Treat as notes, synthesize a fresh auto block.
    LegacyAllNotes { notes: String },
    /// Some structural problem we refuse to auto-fix (prevents silent
    /// data loss). Caller renames existing file to `.corrupt.<ts>` and
    /// writes a fresh one.
    Quarantine {
        reason: &'static str,
        original: String,
    },
    /// File didn't exist — fresh write.
    Empty,
}

pub fn parse_sections(raw: &str) -> ParsedSections {
    if raw.is_empty() {
        return ParsedSections::Empty;
    }
    // Strip BOM; don't trim surrounding whitespace (preserves notes
    // formatting at file start).
    let text = raw.strip_prefix('\u{FEFF}').unwrap_or(raw);

    // Count markers first — a single regex-free walk suffices since
    // markers are full-line fixed strings. Duplicates = corruption.
    let auto_begins: Vec<usize> = text.match_indices(AUTO_BEGIN).map(|(i, _)| i).collect();
    let auto_ends: Vec<usize> = text.match_indices(AUTO_END).map(|(i, _)| i).collect();
    let notes_begins: Vec<usize> = text.match_indices(NOTES_BEGIN).map(|(i, _)| i).collect();
    let notes_ends: Vec<usize> = text.match_indices(NOTES_END).map(|(i, _)| i).collect();

    let no_fences = auto_begins.is_empty()
        && auto_ends.is_empty()
        && notes_begins.is_empty()
        && notes_ends.is_empty();
    if no_fences {
        return ParsedSections::LegacyAllNotes {
            notes: text.to_string(),
        };
    }

    // Exactly one of each marker — clean parse.
    if auto_begins.len() == 1
        && auto_ends.len() == 1
        && notes_begins.len() == 1
        && notes_ends.len() == 1
    {
        let ab = auto_begins[0];
        let ae = auto_ends[0];
        let nb = notes_begins[0];
        let ne = notes_ends[0];
        if ab < ae && ae < nb && nb < ne {
            // Auto inside: between AUTO_BEGIN line end and AUTO_END line start.
            // Notes inside: between NOTES_BEGIN line end and NOTES_END line start.
            let auto = slice_between(text, ab + AUTO_BEGIN.len(), ae).trim_matches('\n').to_string();
            let notes = slice_between(text, nb + NOTES_BEGIN.len(), ne)
                .trim_matches('\n')
                .to_string();
            return ParsedSections::Parsed { auto, notes };
        }
        return ParsedSections::Quarantine {
            reason: "fence markers out of expected order",
            original: raw.to_string(),
        };
    }

    // Anything else = malformed. Avoids swallowing duplicate-fence
    // files as "legacy all notes" on the next regen.
    ParsedSections::Quarantine {
        reason: "unmatched or duplicated fence markers",
        original: raw.to_string(),
    }
}

fn slice_between(s: &str, start: usize, end: usize) -> &str {
    if start <= end && end <= s.len() {
        &s[start..end]
    } else {
        ""
    }
}

/// Assemble a full context.md file body from an auto block (already
/// rendered) and a preserved notes body.
pub fn render_sidecar(auto_body: &str, notes_body: &str) -> String {
    let notes_inner = notes_body.trim_matches('\n');
    format!(
        "{begin}\n{auto}\n{end}\n\n{nb}\n{notes}\n{ne}\n",
        begin = AUTO_BEGIN,
        auto = auto_body.trim_matches('\n'),
        end = AUTO_END,
        nb = NOTES_BEGIN,
        notes = notes_inner,
        ne = NOTES_END,
    )
}

/// CLAUDE.md mirror — derived from the auto block only. No notes
/// block here: notes live in the per-worktree sidecars where agents
/// actually write them. Keeping CLAUDE.md as a pure mirror avoids
/// agent-vs-user write races on a file that Claude may re-read on
/// every memory refresh.
pub fn render_claude_mirror(auto_body: &str) -> String {
    format!("{}\n", auto_body.trim_matches('\n'))
}

// ---------------------------------------------------------------------------
// Auto-block composer. Pure — reads nothing, just formats.
// ---------------------------------------------------------------------------

pub struct TicketForContext {
    pub external_id: String,
    pub url: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub title_fetched_at: Option<i64>,
}

pub struct RepoForContext {
    pub name: String,
    pub task_branch: String,
    pub base_branch: String,
}

pub fn compose_auto_block(
    slug: &str,
    initial_prompt: Option<&str>,
    tickets: &[TicketForContext],
    repos: &[RepoForContext],
    now_unix_ms: i64,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Task: {slug}\n\n"));
    match initial_prompt.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => {
            out.push_str(p);
            out.push('\n');
        }
        None => {
            out.push_str("No initial prompt was provided.\n");
        }
    }
    if !tickets.is_empty() {
        out.push_str("\n## Linked tickets\n");
        for t in tickets {
            let title = t
                .title
                .as_deref()
                .map(|s| s.replace(['\r', '\n'], " "))
                .unwrap_or_else(|| "(title unavailable)".to_string());
            let status_tail = t
                .status
                .as_deref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            let cache_tail = t
                .title_fetched_at
                .map(|ts| format!(" · cached {}", humanize_age(now_unix_ms - ts)))
                .unwrap_or_default();
            out.push_str(&format!(
                "- [{id}]({url}) — \"{title}\"{status}{cache}\n",
                id = t.external_id,
                url = t.url,
                title = title,
                status = status_tail,
                cache = cache_tail,
            ));
        }
    }
    if !repos.is_empty() {
        out.push_str("\n## Repos\n");
        for r in repos {
            out.push_str(&format!(
                "- {name} (branch `{tb}` from `{bb}`)\n",
                name = r.name,
                tb = r.task_branch,
                bb = r.base_branch,
            ));
        }
    }
    out
}

fn humanize_age(ms_elapsed: i64) -> String {
    if ms_elapsed < 0 {
        return "just now".into();
    }
    let secs = ms_elapsed / 1000;
    if secs < 60 {
        return "just now".into();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m ago");
    }
    let hrs = mins / 60;
    if hrs < 24 {
        return format!("{hrs}h ago");
    }
    let days = hrs / 24;
    format!("{days}d ago")
}

// ---------------------------------------------------------------------------
// Atomic write + advisory flock
// ---------------------------------------------------------------------------

fn atomic_write_with_lock(path: &Path, body: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create dir {}", parent.display()))?;
    }
    let lock_path = path.with_extension("md.lock");
    // Touch the lock file — exists for the lifetime of the worktree.
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("open lock {}", lock_path.display()))?;
    // Exclusive flock. Short retry loop — agents write fast, user
    // mutations are rare; sub-100ms is fine.
    let mut tries = 0;
    loop {
        match lock.try_lock_exclusive() {
            Ok(()) => break,
            Err(_) if tries < 20 => {
                std::thread::sleep(std::time::Duration::from_millis(25));
                tries += 1;
            }
            Err(e) => return Err(anyhow!("flock {}: {e}", lock_path.display())),
        }
    }
    let tmp = path.with_file_name(format!(
        "{}.tmp.{}",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("context"),
        std::process::id()
    ));
    let write_result = (|| -> Result<()> {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .with_context(|| format!("open {}", tmp.display()))?;
        f.write_all(body).with_context(|| format!("write {}", tmp.display()))?;
        f.sync_all().ok();
        drop(f);
        fs::rename(&tmp, path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))
    })();
    // Best-effort unlock; OS releases on drop anyway.
    let _ = fs2::FileExt::unlock(&lock);
    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    write_result
}

fn read_existing_with_lock(path: &Path) -> Result<Option<String>> {
    let lock_path = path.with_extension("md.lock");
    // Best-effort: if the lock file doesn't exist yet, no reader/writer
    // contention is possible. Just read directly.
    let lock = match OpenOptions::new().read(true).write(true).open(&lock_path) {
        Ok(f) => Some(f),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(anyhow!("open lock {}: {e}", lock_path.display())),
    };
    if let Some(ref lock_file) = lock {
        let mut tries = 0;
        loop {
            match lock_file.try_lock_exclusive() {
                Ok(()) => break,
                Err(_) if tries < 20 => {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    tries += 1;
                }
                Err(e) => return Err(anyhow!("flock {}: {e}", lock_path.display())),
            }
        }
    }
    let content = match fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            if let Some(l) = lock {
                let _ = fs2::FileExt::unlock(&l);
            }
            return Err(anyhow!("read {}: {e}", path.display()));
        }
    };
    if let Some(l) = lock {
        let _ = fs2::FileExt::unlock(&l);
    }
    Ok(content)
}

// ---------------------------------------------------------------------------
// Public API — refresh + compose_first_turn + legacy read/write
// ---------------------------------------------------------------------------

/// Rewrite `.weft/context.md` in every ready worktree of the task and
/// mirror to the task-root `CLAUDE.md`. Preserves each sidecar's
/// `weft:notes` block. Non-fatal per path — logs and continues on
/// individual failures so a single fs hiccup doesn't fail the
/// enclosing mutation.
pub fn refresh_task_context(db: &Arc<Mutex<Connection>>, task_id: &str) -> Result<()> {
    // Short DB read — gather everything we need before touching fs.
    let (slug, initial_prompt, tickets, repos, worktree_paths) = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let task = TaskRepo::new(&conn)
            .get(task_id)?
            .ok_or_else(|| anyhow!("refresh_task_context: task {task_id} not found"))?;
        let ticket_rows: Vec<TaskTicketRow> = TaskTicketRepo::new(&conn).list_for_task(task_id)?;
        let tickets: Vec<TicketForContext> = ticket_rows
            .into_iter()
            .map(|r| TicketForContext {
                external_id: r.external_id,
                url: r.url,
                title: r.title,
                status: r.status,
                title_fetched_at: r.title_fetched_at,
            })
            .collect();
        let wt_rows = TaskWorktreeRepo::new(&conn).list_for_task(task_id)?;
        let mut repos: Vec<RepoForContext> = Vec::new();
        let mut worktree_paths: Vec<PathBuf> = Vec::new();
        let repo_access = ProjectRepo::new(&conn);
        for wt in &wt_rows {
            if wt.status != "ready" {
                continue;
            }
            worktree_paths.push(PathBuf::from(&wt.worktree_path));
            let name = repo_access
                .get(&wt.project_id)?
                .map(|p| p.name)
                .unwrap_or_else(|| wt.project_id.clone());
            repos.push(RepoForContext {
                name,
                task_branch: wt.task_branch.clone(),
                base_branch: wt.base_branch.clone(),
            });
        }
        (
            task.slug,
            task.initial_prompt,
            tickets,
            repos,
            worktree_paths,
        )
    };

    let auto_block = compose_auto_block(
        &slug,
        initial_prompt.as_deref(),
        &tickets,
        &repos,
        now_unix_ms(),
    );

    // Per-worktree sidecar write. Preserves existing notes.
    for wt_path in &worktree_paths {
        let path = wt_path.join(".weft").join(CONTEXT_FILENAME);
        if let Err(e) = write_sidecar_preserving_notes(&path, &auto_block) {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "refresh_task_context: sidecar write failed (non-fatal)"
            );
        }
    }

    // Task-root CLAUDE.md mirror. Primary worktree's parent = task root.
    if let Some(first) = worktree_paths.first() {
        if let Some(task_root) = first.parent() {
            let path = task_root.join(CLAUDE_FILENAME);
            let body = render_claude_mirror(&auto_block);
            if let Err(e) = atomic_write_with_lock(&path, body.as_bytes()) {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "refresh_task_context: CLAUDE.md mirror write failed (non-fatal)"
                );
            }
        }
    }

    Ok(())
}

fn write_sidecar_preserving_notes(path: &Path, auto_block: &str) -> Result<()> {
    let existing = read_existing_with_lock(path)?.unwrap_or_default();
    let notes_body = match parse_sections(&existing) {
        ParsedSections::Parsed { notes, .. } => notes,
        ParsedSections::LegacyAllNotes { notes } => notes,
        ParsedSections::Empty => String::new(),
        ParsedSections::Quarantine { reason, original } => {
            // Rename the corrupt file to a timestamped sibling so the
            // user/agent can recover by hand. Then treat notes as empty.
            let quarantine = path.with_file_name(format!(
                "{}.corrupt.{}",
                path.file_name().and_then(|s| s.to_str()).unwrap_or("context"),
                now_unix_ms() / 1000
            ));
            if let Err(e) = fs::write(&quarantine, original) {
                tracing::warn!(
                    path = %quarantine.display(),
                    error = %e,
                    "refresh_task_context: quarantine write failed"
                );
            } else {
                tracing::warn!(
                    path = %path.display(),
                    quarantine = %quarantine.display(),
                    reason = %reason,
                    "refresh_task_context: context.md was malformed — quarantined and rewriting fresh"
                );
            }
            String::new()
        }
    };
    let body = render_sidecar(auto_block, &notes_body);
    atomic_write_with_lock(path, body.as_bytes())
}

/// Compose the first user turn for the very first agent launch of a
/// task — the content that used to come from `composeInitialMessage`
/// in `src/lib/launch-agent.ts`. Single-sourced in Rust so we don't
/// need to hit Linear on launch (titles are already cached).
pub fn compose_first_turn(
    db: &Arc<Mutex<Connection>>,
    task_id: &str,
) -> Result<Option<String>> {
    let (initial_prompt, tickets) = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let task = TaskRepo::new(&conn)
            .get(task_id)?
            .ok_or_else(|| anyhow!("compose_first_turn: task {task_id} not found"))?;
        let tickets = TaskTicketRepo::new(&conn).list_for_task(task_id)?;
        (task.initial_prompt, tickets)
    };
    let Some(prompt_raw) = initial_prompt else {
        return Ok(None);
    };
    let prompt = prompt_raw.trim();
    if prompt.is_empty() {
        return Ok(None);
    }
    if tickets.is_empty() {
        return Ok(Some(prompt.to_string()));
    }
    let mut out = String::new();
    out.push_str(prompt);
    out.push_str("\n\n");
    out.push_str(if tickets.len() == 1 {
        "Linked ticket:\n"
    } else {
        "Linked tickets:\n"
    });
    for t in &tickets {
        let title = t
            .title
            .as_deref()
            .map(|s| s.replace(['\r', '\n'], " "))
            .unwrap_or_else(|| "(title unavailable)".to_string());
        let status_tail = t
            .status
            .as_deref()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        out.push_str(&format!("- {}: {}{}\n", t.external_id, title, status_tail));
        out.push_str(&format!("  {}\n", t.url));
    }
    Ok(Some(out.trim_end().to_string()))
}

/// Backwards-compatible read. Returns the file contents as-is from the
/// primary worktree — the ContextDialog then parses sections to show
/// the user the auto preview + editable notes.
pub fn read_task_context(
    db: &Arc<Mutex<Connection>>,
    task_id: &str,
) -> Result<String> {
    let worktrees = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        TaskWorktreeRepo::new(&conn).list_for_task(task_id)?
    };
    let Some(wt) = worktrees.iter().find(|w| w.status == "ready") else {
        return Ok(String::new());
    };
    let path = PathBuf::from(&wt.worktree_path)
        .join(".weft")
        .join(CONTEXT_FILENAME);
    Ok(read_existing_with_lock(&path)?.unwrap_or_default())
}

/// Write a user-authored notes body. The UI hands in just the notes
/// content (the auto block is read-only to users); this updates every
/// sidecar and re-renders the task-root CLAUDE.md. Deleting is done
/// by passing an empty string — the notes block stays but shrinks to
/// zero length.
pub fn write_task_context(
    db: &Arc<Mutex<Connection>>,
    task_id: &str,
    notes_body: &str,
) -> Result<()> {
    // 1. For every ready worktree: read, splice notes = caller input,
    //    write back. We can't just call `refresh_task_context` with
    //    new notes because it reads notes from disk — it preserves
    //    whatever was there, not what the user just typed.
    let worktree_paths: Vec<PathBuf> = {
        let conn = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        TaskWorktreeRepo::new(&conn)
            .list_for_task(task_id)?
            .into_iter()
            .filter(|w| w.status == "ready")
            .map(|w| PathBuf::from(w.worktree_path))
            .collect()
    };
    if worktree_paths.is_empty() {
        return Ok(());
    }
    // Re-fetch task state for the auto block (cheap, same as refresh).
    // Doing this first so auto is consistent with caller-supplied notes.
    refresh_task_context(db, task_id)?;
    // Now re-open each sidecar and replace the notes body. The prior
    // `refresh_task_context` already wrote a fresh auto block; we
    // just swap notes in.
    for wt_path in &worktree_paths {
        let path = wt_path.join(".weft").join(CONTEXT_FILENAME);
        match read_existing_with_lock(&path)? {
            Some(existing) => {
                let parsed = parse_sections(&existing);
                let auto = match parsed {
                    ParsedSections::Parsed { auto, .. } => auto,
                    // Shouldn't happen — refresh just wrote a clean
                    // file. If it does, synthesize.
                    _ => String::new(),
                };
                let body = render_sidecar(&auto, notes_body);
                if let Err(e) = atomic_write_with_lock(&path, body.as_bytes()) {
                    tracing::warn!(path = %path.display(), error = %e, "write_task_context: failed");
                }
            }
            None => {
                // File disappeared between refresh and now. Skip.
            }
        }
    }
    Ok(())
}

fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn basic_parsed() -> String {
        render_sidecar("# Task: s\n\nHello.", "already-noted")
    }

    #[test]
    fn splicer_parses_clean_file() {
        let body = basic_parsed();
        match parse_sections(&body) {
            ParsedSections::Parsed { auto, notes } => {
                assert!(auto.contains("Task: s"));
                assert!(notes.contains("already-noted"));
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn splicer_empty_file_is_empty_variant() {
        assert!(matches!(parse_sections(""), ParsedSections::Empty));
    }

    #[test]
    fn splicer_legacy_no_fences_treated_as_notes() {
        let legacy = "some older hand-written context\nwith two lines";
        match parse_sections(legacy) {
            ParsedSections::LegacyAllNotes { notes } => {
                assert_eq!(notes, legacy);
            }
            other => panic!("expected LegacyAllNotes, got {other:?}"),
        }
    }

    #[test]
    fn splicer_missing_auto_end_quarantined() {
        let body = format!("{}\n## Task\n\n{}\nnotes\n{}\n", AUTO_BEGIN, NOTES_BEGIN, NOTES_END);
        match parse_sections(&body) {
            ParsedSections::Quarantine { .. } => {}
            other => panic!("expected Quarantine, got {other:?}"),
        }
    }

    #[test]
    fn splicer_missing_notes_begin_quarantined() {
        let body = format!("{}\n## Task\n{}\nnotes body\n{}\n", AUTO_BEGIN, AUTO_END, NOTES_END);
        match parse_sections(&body) {
            ParsedSections::Quarantine { .. } => {}
            other => panic!("expected Quarantine, got {other:?}"),
        }
    }

    #[test]
    fn splicer_duplicate_auto_begin_quarantined() {
        let body = format!(
            "{b}\n# t\n{b}\ndup\n{e}\n\n{nb}\nn\n{ne}\n",
            b = AUTO_BEGIN,
            e = AUTO_END,
            nb = NOTES_BEGIN,
            ne = NOTES_END
        );
        match parse_sections(&body) {
            ParsedSections::Quarantine { .. } => {}
            other => panic!("expected Quarantine, got {other:?}"),
        }
    }

    #[test]
    fn splicer_out_of_order_notes_first_quarantined() {
        // notes fences ahead of auto fences = structural wrongness.
        let body = format!(
            "{nb}\nn\n{ne}\n\n{ab}\nt\n{ae}\n",
            nb = NOTES_BEGIN,
            ne = NOTES_END,
            ab = AUTO_BEGIN,
            ae = AUTO_END
        );
        match parse_sections(&body) {
            ParsedSections::Quarantine { .. } => {}
            other => panic!("expected Quarantine, got {other:?}"),
        }
    }

    #[test]
    fn splicer_bom_stripped() {
        let body = format!("\u{FEFF}{}", basic_parsed());
        match parse_sections(&body) {
            ParsedSections::Parsed { auto, .. } => {
                assert!(auto.contains("Task: s"));
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn splicer_zero_length_notes_preserved() {
        let body = render_sidecar("# t", "");
        match parse_sections(&body) {
            ParsedSections::Parsed { notes, .. } => assert_eq!(notes, ""),
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn render_sidecar_roundtrips_through_parser() {
        let body = render_sidecar("auto\nbody\nhere", "notes body");
        match parse_sections(&body) {
            ParsedSections::Parsed { auto, notes } => {
                assert_eq!(auto, "auto\nbody\nhere");
                assert_eq!(notes, "notes body");
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn compose_auto_block_includes_prompt_tickets_repos() {
        let tickets = vec![TicketForContext {
            external_id: "PRO-1".into(),
            url: "https://linear.app/x".into(),
            title: Some("Fix login".into()),
            status: Some("Todo".into()),
            title_fetched_at: Some(now_unix_ms() - 10 * 60 * 1000),
        }];
        let repos = vec![RepoForContext {
            name: "admin".into(),
            task_branch: "feature/pro-1".into(),
            base_branch: "main".into(),
        }];
        let out = compose_auto_block("pro-1", Some("Fix the thing"), &tickets, &repos, now_unix_ms());
        assert!(out.contains("# Task: pro-1"));
        assert!(out.contains("Fix the thing"));
        assert!(out.contains("[PRO-1](https://linear.app/x)"));
        assert!(out.contains("Fix login"));
        assert!(out.contains("(Todo)"));
        assert!(out.contains("cached"));
        assert!(out.contains("admin (branch `feature/pro-1` from `main`)"));
    }

    #[test]
    fn compose_auto_block_without_prompt_or_tickets() {
        let out = compose_auto_block("s", None, &[], &[], now_unix_ms());
        assert!(out.contains("No initial prompt"));
        assert!(!out.contains("Linked tickets"));
        assert!(!out.contains("## Repos"));
    }

    #[test]
    fn atomic_write_creates_and_overwrites_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("context.md");
        atomic_write_with_lock(&path, b"v1").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "v1");
        atomic_write_with_lock(&path, b"v2 longer content").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "v2 longer content");
    }

    #[test]
    fn write_sidecar_preserves_notes_across_auto_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("context.md");
        let initial = render_sidecar("# Task: v1\n\nOld auto", "USER NOTES HERE");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        atomic_write_with_lock(&path, initial.as_bytes()).unwrap();
        write_sidecar_preserving_notes(&path, "# Task: v2\n\nNew auto").unwrap();
        let got = fs::read_to_string(&path).unwrap();
        match parse_sections(&got) {
            ParsedSections::Parsed { auto, notes } => {
                assert!(auto.contains("Task: v2"));
                assert!(auto.contains("New auto"));
                assert_eq!(notes, "USER NOTES HERE");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn write_sidecar_quarantines_malformed_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("context.md");
        fs::write(&path, format!("{}\nsketch\n(missing end)", AUTO_BEGIN)).unwrap();
        write_sidecar_preserving_notes(&path, "# Task: x\n\nfresh").unwrap();
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        let has_corrupt = entries.iter().any(|n| n.starts_with("context.md.corrupt."));
        assert!(has_corrupt, "expected quarantine file, got {entries:?}");
        let current = fs::read_to_string(&path).unwrap();
        match parse_sections(&current) {
            ParsedSections::Parsed { auto, notes } => {
                assert!(auto.contains("fresh"));
                assert_eq!(notes, "");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
