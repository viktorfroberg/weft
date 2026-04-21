//! Diagnostics helpers for the dev-loop freeze hunt.
//!
//! Two pieces:
//! - A panic hook that writes to stderr AND to a breadcrumb file at
//!   `~/Library/Application Support/weft/crash.log`. Survives process
//!   death so a post-mortem is possible even when the webview has
//!   frozen the window and we have to force-quit.
//! - A `timed!` helper that wraps hot Tauri commands with
//!   `tracing::info!` at entry + exit with duration. Surfaces to the
//!   terminal running `bun run tauri dev` because the default env
//!   filter was bumped to `weft=debug,warn`.

use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::io::Write;
use std::panic;
use std::path::PathBuf;
use std::time::Instant;

/// Path where the crash breadcrumb log lives. Mirrors `db::data_dir()`
/// but doesn't depend on it, so we can set the panic hook before the
/// DB layer is wired up.
fn crash_log_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("weft").join("crash.log"))
}

/// Install a global panic hook. Idempotent — installing twice leaks a
/// small amount of memory but doesn't crash.
pub fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        // Keep the default behavior (stderr print) — visible in the dev
        // terminal.
        default_hook(info);

        // Add our own breadcrumb with stack trace. Best-effort: if
        // writing fails we can't do much about it, but the stderr copy
        // already ran.
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>");
        let bt = Backtrace::force_capture();
        let payload: &str = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| {
                info.payload()
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
            })
            .unwrap_or("<non-string panic payload>");
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".into());
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = format!(
            "\n--- PANIC ({ts}) ---\n\
             thread: {thread_name}\n\
             location: {location}\n\
             message: {payload}\n\
             backtrace:\n{bt}\n",
        );

        if let Some(path) = crash_log_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut f) = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)
            {
                let _ = f.write_all(entry.as_bytes());
            }
        }
    }));
}

/// Guard type that logs elapsed time on drop. Use via `timed!("name")`
/// in a function body:
///
/// ```ignore
/// pub fn tasks_list(...) -> ... {
///     let _t = crate::debug::timed("tasks_list");
///     ...  // duration logged when _t drops
/// }
/// ```
pub struct Timed {
    name: &'static str,
    started_at: Instant,
}

impl Timed {
    pub fn new(name: &'static str) -> Self {
        tracing::debug!(target: "weft::cmd", "▶ {name}");
        Self {
            name,
            started_at: Instant::now(),
        }
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        let elapsed = self.started_at.elapsed();
        // Flag anything over 100ms as `info` so it stands out; fast
        // commands stay at `debug` to not spam.
        if elapsed.as_millis() >= 100 {
            tracing::info!(
                target: "weft::cmd",
                "◀ {} — {:?} (SLOW)",
                self.name,
                elapsed,
            );
        } else {
            tracing::debug!(
                target: "weft::cmd",
                "◀ {} — {:?}",
                self.name,
                elapsed,
            );
        }
    }
}

#[macro_export]
macro_rules! timed {
    ($name:expr) => {
        let _timed_guard = $crate::debug::Timed::new($name);
    };
}
