use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// How a preset delivers its `bootstrap_prompt_template` when a second
/// agent joins a task (i.e. when no fresh user prompt is available).
/// `Argv` shoves the template into the positional `{prompt}` slot —
/// portable across any CLI agent. `AppendSystemPrompt` uses the
/// `--append-system-prompt` flag pair (Claude-only), which keeps the
/// orientation out of the visible transcript so Claude doesn't "reply"
/// to a bureaucratic first turn.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapDelivery {
    Argv,
    AppendSystemPrompt,
}

impl BootstrapDelivery {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "argv" => Some(Self::Argv),
            "append_system_prompt" => Some(Self::AppendSystemPrompt),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPreset {
    pub id: String,
    pub name: String,
    pub command: String,
    /// JSON-serialized `Vec<String>` of arg templates.
    pub args_json: String,
    /// JSON-serialized `HashMap<String,String>` of env templates.
    pub env_json: String,
    pub is_default: bool,
    pub sort_order: i64,
    pub created_at: i64,
    /// Orientation text for second-agent / reload launches when the
    /// task's `initial_prompt` is consumed. Null = drop the `{prompt}`
    /// (and `{bootstrap}`) tokens silently, same as pre-0009 behavior.
    pub bootstrap_prompt_template: Option<String>,
    /// Controls where `bootstrap_prompt_template` lands in argv. Null
    /// is treated as `Argv` by the launch pipeline — the portable
    /// default.
    pub bootstrap_delivery: Option<BootstrapDelivery>,
}

pub struct PresetRepo<'a> {
    conn: &'a Connection,
}

const SELECT_COLS: &str = "id, name, command, args_json, env_json, is_default, sort_order, created_at, bootstrap_prompt_template, bootstrap_delivery";

impl<'a> PresetRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<AgentPreset>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SELECT_COLS} FROM agent_presets ORDER BY sort_order, created_at"
        ))?;
        let rows = stmt.query_map([], row_to_preset)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: &str) -> Result<Option<AgentPreset>> {
        let p = self
            .conn
            .query_row(
                &format!("SELECT {SELECT_COLS} FROM agent_presets WHERE id = ?1"),
                [id],
                row_to_preset,
            )
            .optional()?;
        Ok(p)
    }

    /// Returns the preset flagged `is_default = 1`. Falls back to the
    /// lowest-sort-order preset if the default flag was cleared.
    pub fn get_default(&self) -> Result<Option<AgentPreset>> {
        let row = self
            .conn
            .query_row(
                &format!(
                    "SELECT {SELECT_COLS} FROM agent_presets
                     WHERE is_default = 1
                     ORDER BY sort_order LIMIT 1"
                ),
                [],
                row_to_preset,
            )
            .optional()?;
        if row.is_some() {
            return Ok(row);
        }
        let fallback = self
            .conn
            .query_row(
                &format!(
                    "SELECT {SELECT_COLS} FROM agent_presets ORDER BY sort_order LIMIT 1"
                ),
                [],
                row_to_preset,
            )
            .optional()?;
        Ok(fallback)
    }
}

fn row_to_preset(row: &rusqlite::Row) -> rusqlite::Result<AgentPreset> {
    let delivery_raw: Option<String> = row.get(9)?;
    Ok(AgentPreset {
        id: row.get(0)?,
        name: row.get(1)?,
        command: row.get(2)?,
        args_json: row.get(3)?,
        env_json: row.get(4)?,
        is_default: row.get::<_, i64>(5)? != 0,
        sort_order: row.get(6)?,
        created_at: row.get(7)?,
        bootstrap_prompt_template: row.get(8)?,
        bootstrap_delivery: delivery_raw.as_deref().and_then(BootstrapDelivery::parse),
    })
}
