use super::command_resolve::resolve_command;
use super::recorder::ScrollbackRecorder;
use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};

/// Event name emitted when a PTY's child process exits. Frontend
/// subscribes via the Tauri event API; payload is `PtyExitEvent`.
pub const PTY_EXIT_EVENT: &str = "pty_exit";

#[derive(serde::Serialize, Clone)]
pub struct PtyExitEvent {
    pub session_id: String,
    pub code: Option<i32>,
    pub success: bool,
}

const FLUSH_BYTES: usize = 64 * 1024;
const FLUSH_INTERVAL: Duration = Duration::from_millis(8);
const READ_CHUNK: usize = 8 * 1024;

/// Graceful-shutdown default timeout. Split 50/50 between SIGHUP→SIGTERM
/// and SIGTERM→SIGKILL escalations.
pub const DEFAULT_SHUTDOWN_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitMode {
    /// Child exited before we signaled (already dead) or from SIGHUP.
    Hup,
    /// Child responded to SIGTERM after SIGHUP's window expired.
    Term,
    /// Child ignored SIGHUP + SIGTERM; we force-killed it.
    Kill,
}

#[derive(Debug, Clone)]
pub struct SpawnOptions {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub rows: u16,
    pub cols: u16,
    /// Persistent tab this session is bound to. When present, the waiter
    /// thread flips the row to `dormant` and persists scrollback to
    /// `~/Library/Application Support/weft/scrollback/<tab_id>.bin`
    /// on child exit. `None` means "ephemeral session" — legacy path
    /// for anything that doesn't participate in the persistent-tab model.
    pub tab_id: Option<String>,
}

/// One live PTY session. Owns the master PTY handle, the write end, and
/// the child's pid — the child is explicitly SIGKILL'd in `Drop` so
/// `kill_by_task` doesn't leave zombies holding worktree fds.
///
/// Reader/flusher thread pair delivers output to the frontend with 8ms
/// latency bound even under slow trickles of output (see `reader_loop` +
/// `flusher_loop`).
///
/// **Drop safety note** — the child process is NOT held behind a Mutex
/// here. An earlier iteration used `Arc<Mutex<Option<Child>>>` shared
/// with the waiter thread; the waiter called `child.wait()` while
/// holding the MutexGuard, which deadlocked any `Drop` path that tried
/// to re-lock to SIGKILL. The deadlock manifested as UI freezes when
/// `terminal_kill` was invoked (never returned → Tauri IPC queued →
/// JS event loop starved). See the `waiter` thread in `spawn()` for
/// the lock-scoping pattern that fixes it, and `Drop` for the
/// pid-based SIGKILL that doesn't require any lock.
pub struct TerminalSession {
    #[allow(dead_code)]
    id: String,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Child pid stored up-front so `Drop` can SIGKILL without needing
    /// to lock anything (the waiter thread owns the `Child` value).
    /// `None` means the spawn didn't surface a pid — we skip the kill
    /// but still clean everything else up via master drop.
    pid: Option<u32>,
    /// Set by the waiter *only* on `child.wait()` return. Graceful
    /// shutdown polls this (not the reader-EOF `done` flag, which flips
    /// for reasons other than child death) to know whether to escalate.
    child_exited: Arc<AtomicBool>,
    /// Set by the waiter after it has written the dormant row + persisted
    /// scrollback. Graceful shutdown awaits this so `tab_delete` racing
    /// right after can safely unlink the scrollback file without a
    /// second writer racing.
    dormant_written: Arc<AtomicBool>,
}

impl TerminalSession {
    pub fn spawn(
        id: String,
        opts: SpawnOptions,
        output_channel: Channel<Vec<u8>>,
        app_handle: Option<AppHandle>,
    ) -> Result<Self> {
        // Pre-flight 1: cwd must exist and be a directory. Without this
        // check the underlying portable-pty error is `Os { code: 2 }`
        // which is identical to "command not found" — leaving the user
        // unable to tell which side of the spawn went wrong. The
        // recover path is different (re-create worktree vs install
        // CLI), so we surface the distinction explicitly.
        if !opts.cwd.exists() {
            bail!(
                "spawn cwd does not exist: {} \
                 (worktree may have been removed; check `task_worktrees.status`)",
                opts.cwd.display()
            );
        }
        if !opts.cwd.is_dir() {
            bail!("spawn cwd is not a directory: {}", opts.cwd.display());
        }

        // Pre-flight 2: resolve `command` to an absolute path. This
        // catches the macOS "Tauri-app PATH ⊊ shell PATH" footgun and
        // produces a clear "claude not found, searched: <list>" rather
        // than "spawn command in pty: Os { code: 2 }".
        let resolved_command = resolve_command(&opts.command)
            .with_context(|| format!("resolve command '{}'", opts.command))?;

        tracing::info!(
            target: "weft::pty",
            session_id = %id,
            command = %resolved_command.display(),
            cwd = %opts.cwd.display(),
            args = opts.args.len(),
            env_keys = opts.env.len(),
            "pre-flight ok, opening pty",
        );

        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: opts.rows,
                cols: opts.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open pty")?;

        let mut cmd = CommandBuilder::new(resolved_command.as_os_str());
        for arg in &opts.args {
            cmd.arg(arg);
        }
        cmd.cwd(&opts.cwd);
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| {
                format!(
                    "portable-pty spawn failed for {} (cwd: {})",
                    resolved_command.display(),
                    opts.cwd.display()
                )
            })?;
        drop(pair.slave);

        // Snapshot the pid BEFORE we hand the Child to the waiter thread.
        // Stored un-locked on TerminalSession so Drop can SIGKILL without
        // needing to coordinate with the waiter.
        let pid = child.process_id();

        let reader = pair
            .master
            .try_clone_reader()
            .context("clone pty reader")?;
        let writer = pair.master.take_writer().context("take pty writer")?;

        let master = Arc::new(Mutex::new(pair.master));
        let writer = Arc::new(Mutex::new(writer));

        let buffer: Arc<Mutex<Vec<u8>>> =
            Arc::new(Mutex::new(Vec::with_capacity(FLUSH_BYTES)));
        let done = Arc::new(AtomicBool::new(false));
        let channel = Arc::new(output_channel);
        let recorder: Arc<StdMutex<Option<ScrollbackRecorder>>> =
            Arc::new(StdMutex::new(Some(ScrollbackRecorder::new())));

        spawn_reader_thread(
            id.clone(),
            reader,
            Arc::clone(&buffer),
            Arc::clone(&done),
            Arc::clone(&channel),
            Arc::clone(&recorder),
        )?;
        spawn_flusher_thread(
            id.clone(),
            buffer,
            Arc::clone(&done),
            channel,
        )?;

        let child_exited = Arc::new(AtomicBool::new(false));
        let dormant_written = Arc::new(AtomicBool::new(false));

        // Waiter thread: reap the child, flip `child_exited`, then do the
        // post-exit dormant-row write + scrollback persist *before*
        // flipping `dormant_written`. Graceful shutdown callers await
        // `dormant_written` so tab_delete racing after can safely unlink.
        let done_for_wait = Arc::clone(&done);
        let child_exited_w = Arc::clone(&child_exited);
        let dormant_written_w = Arc::clone(&dormant_written);
        let recorder_for_wait = Arc::clone(&recorder);
        let id_w = id.clone();
        let tab_id_w = opts.tab_id.clone();
        let handle_for_wait = app_handle.clone();
        thread::Builder::new()
            .name(format!("pty-waiter-{id_w}"))
            .spawn(move || {
                let mut child = child;
                let status = child.wait().ok();
                tracing::info!(id = %id_w, ?status, "child exited");

                // ORDER MATTERS:
                // 1. Flip child_exited so graceful-shutdown pollers can
                //    stop sending signals.
                // 2. Flip `done` so the reader/flusher drain.
                // 3. Persist scrollback + write dormant row.
                // 4. Flip dormant_written last so graceful-shutdown
                //    callers that awaited it see a fully-settled state.
                child_exited_w.store(true, Ordering::Release);
                done_for_wait.store(true, Ordering::Release);

                let exit_code: Option<i32> = status.as_ref().map(|s| s.exit_code() as i32);
                let success = status.as_ref().map(|s| s.success()).unwrap_or(false);

                // Drain the recorder. Taking the Option out replaces it
                // with None — subsequent reader chunks (shouldn't happen
                // post-EOF but belt-and-suspenders) become no-ops.
                let transcript: Option<Vec<u8>> = {
                    let mut guard = recorder_for_wait.lock().unwrap_or_else(|p| p.into_inner());
                    guard.take().map(|r| r.finalize())
                };

                // Only persist + write dormant if this session is bound
                // to a persistent tab AND the row still exists. The row
                // going missing means the user explicitly × + confirmed
                // the tab; we don't want to resurrect it as dormant, and
                // we don't want to write a scrollback file that nothing
                // will ever clean up.
                if let (Some(tab_id), Some(handle)) = (tab_id_w.as_ref(), handle_for_wait.as_ref()) {
                    let state = handle.state::<crate::AppState>();
                    let wrote = {
                        let conn = match state.db.lock() {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::warn!(
                                    id = %id_w,
                                    tab = %tab_id,
                                    error = %e,
                                    "waiter: db lock poisoned, skipping dormant write"
                                );
                                dormant_written_w.store(true, Ordering::Release);
                                return_pty_exit(&id_w, handle_for_wait.as_ref(), exit_code, success);
                                return;
                            }
                        };
                        let repo = crate::db::repo::TerminalTabRepo::new(&conn);
                        match repo.mark_dormant(tab_id, exit_code) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::warn!(
                                    id = %id_w,
                                    tab = %tab_id,
                                    error = %e,
                                    "waiter: mark_dormant failed"
                                );
                                false
                            }
                        }
                    };

                    if wrote {
                        if let Some(bytes) = transcript {
                            if let Err(e) = write_scrollback(tab_id, &bytes) {
                                tracing::warn!(
                                    id = %id_w,
                                    tab = %tab_id,
                                    error = %e,
                                    "waiter: scrollback persist failed"
                                );
                            }
                        }
                        // Emit a db_event so the frontend store refetches
                        // and flips the tab to dormant in the UI.
                        let ev = crate::db::events::DbEvent::update(
                            crate::db::events::Entity::TerminalTab,
                            tab_id.clone(),
                        );
                        if let Err(e) = handle.emit(crate::db::events::DB_EVENT_CHANNEL, ev) {
                            tracing::warn!(
                                id = %id_w,
                                error = %e,
                                "waiter: db_event emit failed"
                            );
                        }
                    } else {
                        tracing::debug!(
                            id = %id_w,
                            tab = %tab_id,
                            "waiter: tab row gone (user-deleted) — skipping dormant write + scrollback"
                        );
                    }
                }

                dormant_written_w.store(true, Ordering::Release);
                return_pty_exit(&id_w, handle_for_wait.as_ref(), exit_code, success);
            })
            .context("spawn waiter thread")?;

        Ok(Self {
            id,
            master,
            writer,
            pid,
            child_exited,
            dormant_written,
        })
    }

    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        let mut w = self.writer.lock();
        w.write_all(bytes).context("pty write")?;
        Ok(())
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.master
            .lock()
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow!("pty resize: {e}"))
    }

    /// True when the child has exited (waiter flipped the flag). The
    /// reader's `done` flag is NOT equivalent — it also flips on reader
    /// EOF caused by a closed channel, which does not imply child death.
    pub fn is_alive(&self) -> bool {
        !self.child_exited.load(Ordering::Acquire)
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Graceful shutdown: SIGHUP, poll `child_exited`, escalate to
    /// SIGTERM, then SIGKILL. After the child is dead, awaits the
    /// waiter's post-exit dormant-row + scrollback work via
    /// `dormant_written`. Returns which signal the child eventually
    /// responded to. A `Kill` return is a misbehaving-child telemetry
    /// signal worth logging upstream.
    pub fn shutdown_graceful(&self, timeout_ms: u64) -> ExitMode {
        let pid = match self.pid {
            Some(p) => p,
            None => {
                // No pid → we can't signal. Just wait for the waiter's
                // bookkeeping to settle (it may already be done) and
                // return Hup as a neutral "assumed exited" value.
                await_flag(&self.dormant_written, Duration::from_millis(500));
                return ExitMode::Hup;
            }
        };

        let half = Duration::from_millis(timeout_ms / 2);

        // Phase 1: SIGHUP.
        signal_pid(pid, libc::SIGHUP);
        if await_flag(&self.child_exited, half) {
            await_flag(&self.dormant_written, Duration::from_millis(500));
            return ExitMode::Hup;
        }

        // Phase 2: SIGTERM.
        signal_pid(pid, libc::SIGTERM);
        if await_flag(&self.child_exited, half) {
            await_flag(&self.dormant_written, Duration::from_millis(500));
            return ExitMode::Term;
        }

        // Phase 3: SIGKILL. Always wins eventually.
        signal_pid(pid, libc::SIGKILL);
        await_flag(&self.child_exited, Duration::from_millis(200));
        await_flag(&self.dormant_written, Duration::from_millis(500));
        ExitMode::Kill
    }
}

impl Drop for TerminalSession {
    /// Explicit teardown: SIGKILL the child so its agent process doesn't
    /// hold the worktree's file descriptors open after the session is
    /// dropped. Without this, `kill_by_task` + `worktree_remove` raced
    /// with a still-alive agent and the remove failed.
    ///
    /// Uses `libc::kill(pid, SIGKILL)` directly so Drop acquires zero
    /// locks. The waiter thread owns the `Child` value — trying to
    /// re-acquire it here (as an earlier iteration did via
    /// `Arc<Mutex<Option<Child>>>`) deadlocked because the waiter
    /// holds ownership across `child.wait()`. Sending SIGKILL by pid
    /// makes `wait()` return, which lets the waiter exit cleanly and
    /// the master drop below run without blockers.
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            // On macOS / Linux: SIGKILL is fatal and ignores signal
            // handlers. If the child already exited, kill() returns
            // ESRCH — harmless. We swallow the error because Drop
            // shouldn't panic.
            #[cfg(unix)]
            unsafe {
                let _ = libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
            #[cfg(not(unix))]
            {
                // weft is macOS-only for v1, but if this ever compiles
                // elsewhere we at least don't leak silently.
                tracing::warn!(
                    id = %self.id,
                    pid,
                    "TerminalSession::Drop on non-Unix — pid-based kill not wired",
                );
            }
        }
        // Dropping master afterwards signals SIGHUP and closes the fd.
    }
}

fn return_pty_exit(
    session_id: &str,
    handle: Option<&AppHandle>,
    code: Option<i32>,
    success: bool,
) {
    if let Some(handle) = handle {
        let payload = PtyExitEvent {
            session_id: session_id.to_string(),
            code,
            success,
        };
        if let Err(e) = handle.emit(PTY_EXIT_EVENT, payload) {
            tracing::warn!(id = %session_id, error = %e, "emit pty_exit failed");
        }
    }
}

#[cfg(unix)]
fn signal_pid(pid: u32, sig: libc::c_int) {
    unsafe {
        let _ = libc::kill(pid as libc::pid_t, sig);
    }
}

#[cfg(not(unix))]
fn signal_pid(_pid: u32, _sig: libc::c_int) {
    tracing::warn!("signal_pid stubbed on non-unix");
}

/// Poll an atomic flag until set or timeout. 20 ms granularity is fine —
/// shutdown latency is dominated by the child's signal-handler response,
/// not this loop.
fn await_flag(flag: &AtomicBool, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if flag.load(Ordering::Acquire) {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    flag.load(Ordering::Acquire)
}

/// Scrollback persistence path. Matches the reconciler in
/// `services/reconcile.rs::reconcile_scrollback`.
pub fn scrollback_path(tab_id: &str) -> Result<PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| anyhow!("no data_dir"))?
        .join("weft")
        .join("scrollback");
    Ok(base.join(format!("{tab_id}.bin")))
}

fn write_scrollback(tab_id: &str, bytes: &[u8]) -> Result<()> {
    let path = scrollback_path(tab_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    // Truncated write is fine: the waiter is the sole writer for this
    // tab_id (session ids are unique per spawn) and the file is final
    // at this point. No atomic-rename dance needed.
    std::fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn spawn_reader_thread(
    id: String,
    mut reader: Box<dyn Read + Send>,
    buffer: Arc<Mutex<Vec<u8>>>,
    done: Arc<AtomicBool>,
    channel: Arc<Channel<Vec<u8>>>,
    recorder: Arc<StdMutex<Option<ScrollbackRecorder>>>,
) -> Result<()> {
    thread::Builder::new()
        .name(format!("pty-reader-{id}"))
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                reader_loop(&id, &mut reader, &buffer, &channel, &recorder);
            }));
            if result.is_err() {
                tracing::error!(id = %id, "PTY reader thread panicked");
                let _ = channel.send(
                    b"\r\n\x1b[31m[weft: reader thread died]\x1b[0m\r\n".to_vec(),
                );
            }
            // ALWAYS set done, no matter how we exited — otherwise the
            // flusher loops forever. See review S2.
            done.store(true, Ordering::Release);
        })
        .context("spawn reader thread")?;
    Ok(())
}

fn reader_loop(
    id: &str,
    reader: &mut Box<dyn Read + Send>,
    buffer: &Arc<Mutex<Vec<u8>>>,
    channel: &Arc<Channel<Vec<u8>>>,
    recorder: &Arc<StdMutex<Option<ScrollbackRecorder>>>,
) {
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => {
                tracing::debug!(id = %id, "reader EOF");
                break;
            }
            Ok(n) => {
                // Feed the recorder BEFORE taking the flush lock. The
                // recorder has its own lock but contention is negligible
                // vs. the flusher's consumer side.
                if let Ok(mut guard) = recorder.lock() {
                    if let Some(rec) = guard.as_mut() {
                        rec.feed(&chunk[..n]);
                    }
                }
                let mut b = buffer.lock();
                b.extend_from_slice(&chunk[..n]);
                if b.len() >= FLUSH_BYTES {
                    let out = std::mem::take(&mut *b);
                    drop(b);
                    if channel.send(out).is_err() {
                        tracing::debug!(id = %id, "channel closed in reader, exiting");
                        break;
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                tracing::warn!(id = %id, error = %e, "reader error, exiting");
                break;
            }
        }
    }
}

fn spawn_flusher_thread(
    id: String,
    buffer: Arc<Mutex<Vec<u8>>>,
    done: Arc<AtomicBool>,
    channel: Arc<Channel<Vec<u8>>>,
) -> Result<()> {
    thread::Builder::new()
        .name(format!("pty-flusher-{id}"))
        .spawn(move || {
            // Diagnostics: count sends + bytes over a 1s window and log
            // if the rate is high. If the UI hangs around PTY spawn we
            // expect to see a huge spike here in the first second or two
            // after the terminal mounts.
            let mut window_start = std::time::Instant::now();
            let mut window_sends: u32 = 0;
            let mut window_bytes: usize = 0;

            loop {
                thread::sleep(FLUSH_INTERVAL);
                let pending = {
                    let mut b = buffer.lock();
                    if b.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut *b))
                    }
                };
                if let Some(bytes) = pending {
                    window_sends += 1;
                    window_bytes += bytes.len();
                    if channel.send(bytes).is_err() {
                        tracing::debug!(id = %id, "channel closed in flusher, exiting");
                        break;
                    }
                }
                if window_start.elapsed() >= Duration::from_secs(1) {
                    if window_sends > 0 {
                        tracing::debug!(
                            target: "weft::pty",
                            id = %id,
                            sends = window_sends,
                            bytes = window_bytes,
                            "flusher 1s window",
                        );
                        if window_sends >= 40 || window_bytes >= 512 * 1024 {
                            tracing::warn!(
                                target: "weft::pty",
                                id = %id,
                                sends = window_sends,
                                bytes = window_bytes,
                                "FLOOD: PTY sending aggressively — possible UI hang culprit",
                            );
                        }
                    }
                    window_start = std::time::Instant::now();
                    window_sends = 0;
                    window_bytes = 0;
                }
                if done.load(Ordering::Acquire) {
                    let final_pending = {
                        let mut b = buffer.lock();
                        if b.is_empty() {
                            None
                        } else {
                            Some(std::mem::take(&mut *b))
                        }
                    };
                    if let Some(bytes) = final_pending {
                        let _ = channel.send(bytes);
                    }
                    break;
                }
            }
        })
        .context("spawn flusher thread")?;
    Ok(())
}
