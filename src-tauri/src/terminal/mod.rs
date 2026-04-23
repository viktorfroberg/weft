//! PTY-backed terminal sessions.
//!
//! Design notes (see plan.md "Known risks" → IPC bandwidth):
//! - PTY output is streamed to the frontend via Tauri v2 `Channel<Vec<u8>>`,
//!   NOT `emit`. `emit` JSON-serializes + base64-encodes bytes which stalls
//!   on fast output (`npm install`, `cat large-file`).
//! - Reads are coalesced in the Rust reader thread: flush on a 64KB buffer
//!   OR every 8ms, whichever first. Never byte-by-byte.
//! - Stdin is low-volume; we use `invoke` for `terminal_write` — fine.

pub mod command_resolve;
pub mod manager;
pub mod recorder;
pub mod session;

pub use manager::TerminalManager;
pub use session::{scrollback_path, ExitMode, SpawnOptions, TerminalSession, DEFAULT_SHUTDOWN_MS};
