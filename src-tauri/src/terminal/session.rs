use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter};

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

#[derive(Debug, Clone)]
pub struct SpawnOptions {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub rows: u16,
    pub cols: u16,
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
}

impl TerminalSession {
    pub fn spawn(
        id: String,
        opts: SpawnOptions,
        output_channel: Channel<Vec<u8>>,
        app_handle: Option<AppHandle>,
    ) -> Result<Self> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: opts.rows,
                cols: opts.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open pty")?;

        let mut cmd = CommandBuilder::new(&opts.command);
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
            .context("spawn command in pty")?;
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

        spawn_reader_thread(
            id.clone(),
            reader,
            Arc::clone(&buffer),
            Arc::clone(&done),
            Arc::clone(&channel),
        )?;
        spawn_flusher_thread(
            id.clone(),
            buffer,
            Arc::clone(&done),
            channel,
        )?;

        // Waiter thread: reap the child and flip `done` so the flusher
        // exits. Takes the Child by value — NO mutex around it. This is
        // the critical fix for the freeze: the previous code held
        // `Arc<Mutex<Option<Child>>>::lock()` across `child.wait()`,
        // which blocked indefinitely and deadlocked `Drop` on the same
        // mutex when `terminal_kill` tried to teardown.
        let done_for_wait = Arc::clone(&done);
        let id_w = id.clone();
        let handle_for_wait = app_handle.clone();
        thread::Builder::new()
            .name(format!("pty-waiter-{id_w}"))
            .spawn(move || {
                let mut child = child;
                let status = child.wait().ok();
                tracing::info!(id = %id_w, ?status, "child exited");
                done_for_wait.store(true, Ordering::Release);

                // Fire pty_exit so the frontend can flip agent tab
                // badges + toast the exit code.
                if let Some(handle) = handle_for_wait {
                    let code = status.as_ref().and_then(|s| {
                        // portable_pty returns an ExitStatus with a `.exit_code()`
                        // method on platforms we care about. Extract as i32.
                        Some(s.exit_code() as i32)
                    });
                    let success = status.as_ref().map(|s| s.success()).unwrap_or(false);
                    let payload = PtyExitEvent {
                        session_id: id_w.clone(),
                        code,
                        success,
                    };
                    if let Err(e) = handle.emit(PTY_EXIT_EVENT, payload) {
                        tracing::warn!(id = %id_w, error = %e, "emit pty_exit failed");
                    }
                }
            })
            .context("spawn waiter thread")?;

        Ok(Self {
            id,
            master,
            writer,
            pid,
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

fn spawn_reader_thread(
    id: String,
    mut reader: Box<dyn Read + Send>,
    buffer: Arc<Mutex<Vec<u8>>>,
    done: Arc<AtomicBool>,
    channel: Arc<Channel<Vec<u8>>>,
) -> Result<()> {
    thread::Builder::new()
        .name(format!("pty-reader-{id}"))
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                reader_loop(&id, &mut reader, &buffer, &channel);
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
) {
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => {
                tracing::debug!(id = %id, "reader EOF");
                break;
            }
            Ok(n) => {
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
