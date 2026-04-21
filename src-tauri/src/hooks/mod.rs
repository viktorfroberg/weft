//! Generic event-ingest HTTP server for agent status updates.
//!
//! Agents (Claude Code, Codex, Gemini, etc.) POST events to `/v1/events`.
//! weft translates events into a coarse `TaskStatus` per task slug. The
//! event schema is intentionally agent-neutral; per-agent adapters live in
//! the agent configs (e.g. Claude Code's `--settings` hook points at this
//! server).
//!
//! Port selection: prefer `DEFAULT_PORT` (17293). If bind fails (another
//! weft instance, or something else), fall back to an OS-assigned port.
//! The chosen port is written to `<data_dir>/hooks.port` so adapters can
//! discover it.

pub mod events;
pub mod install_lock;
pub mod server;
pub mod status;

pub use install_lock::InstallLockStore;
pub use server::{start_server, HookServerHandle};
pub use status::{StatusStore, TransitionResult};
