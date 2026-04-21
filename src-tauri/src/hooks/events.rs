use serde::{Deserialize, Serialize};

/// Agent-neutral event payload. Per-agent adapters map whatever they emit
/// into this shape before POSTing to `/v1/events`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Identifier for the agent that produced the event. Informational only;
    /// weft doesn't branch on this today.
    pub source: String,

    /// Globally-unique task id. Agents are launched with `WEFT_TASK_ID`
    /// injected into env so they can include it here.
    pub task_id: String,

    /// What kind of thing happened. Drives state transitions in
    /// `StatusStore::apply`.
    pub kind: EventKind,

    /// Free-form adapter-specific payload (tool name, prompt text, etc.).
    /// Stored for debugging/auditing in later phases; not interpreted here.
    #[serde(default)]
    pub detail: serde_json::Value,

    /// Unix seconds when the adapter observed the event.
    pub timestamp: i64,
}

/// Coarse event kinds. Extend cautiously: new kinds require a matching arm
/// in `StatusStore::apply` or they'll be silently ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Agent started a new action (tool use, reasoning, etc.).
    Active,
    /// Agent is blocked on a prompt/permission the user must answer.
    WaitingInput,
    /// Agent surfaced an error condition.
    Error,
    /// Session ended (agent exited, user closed terminal, etc.).
    SessionEnd,
}
