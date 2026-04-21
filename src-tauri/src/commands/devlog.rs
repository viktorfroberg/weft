//! Fire-and-forget logging channel for frontend diagnostics.
//!
//! Tauri v2's webview doesn't pipe `console.log` / `console.warn` to
//! the dev terminal reliably, so when the UI locks up there's no way
//! to see what the frontend was doing in its last moments. This
//! command takes a structured payload from JS and routes it through
//! the Rust `tracing` subscriber — same stream as the command
//! boundaries, timestamped, visible to anyone tailing `bun run tauri
//! dev`.
//!
//! Safe to call very frequently; the command is sync + allocates
//! minimally and the tracing subscriber handles its own throughput.

use serde::Deserialize;

#[derive(Deserialize)]
pub struct DevLogInput {
    /// Short scope tag, e.g. "render", "mount", "rate", "event".
    pub scope: String,
    /// Primary human-readable message.
    pub msg: String,
    /// Optional extra payload (counts, ids, elapsed ms, etc).
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
}

#[tauri::command]
pub fn dev_log(input: DevLogInput) {
    if let Some(meta) = input.meta {
        tracing::warn!(
            target: "weft::fe",
            scope = %input.scope,
            meta = %meta,
            "{}",
            input.msg,
        );
    } else {
        tracing::warn!(target: "weft::fe", scope = %input.scope, "{}", input.msg);
    }
}
