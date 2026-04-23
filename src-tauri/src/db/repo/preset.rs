use super::{new_id, now};
use crate::db::events::{DbEvent, Entity};
use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// Whether this agent's CLI supports resuming a prior session via
    /// a captured external `session_id`. Today: Claude Code only
    /// (via `claude --resume <id>`). Drives the dormant-tab reopen
    /// path — see `services/agent_launch::resolve_launch_resume`.
    pub supports_resume: bool,
}

/// Shape for creating a new preset via the Settings UI. `is_default` is
/// not settable here — that goes through `set_default` so the app-layer
/// invariant (exactly-one default, or zero with a deterministic
/// fallback) stays enforceable in one place.
#[derive(Debug, Clone, Deserialize)]
pub struct NewAgentPreset {
    pub name: String,
    pub command: String,
    pub args_json: String,
    pub env_json: String,
    pub sort_order: Option<i64>,
    pub bootstrap_prompt_template: Option<String>,
    /// Raw string — repo validates against `'argv'` / `'append_system_prompt'`.
    pub bootstrap_delivery: Option<String>,
}

/// Full replacement patch for an existing preset. The UI dialog always
/// loads the full preset, edits it, and posts all fields back — avoids
/// partial-update footguns on merge.
#[derive(Debug, Clone, Deserialize)]
pub struct PresetPatch {
    pub name: String,
    pub command: String,
    pub args_json: String,
    pub env_json: String,
    pub sort_order: i64,
    pub bootstrap_prompt_template: Option<String>,
    pub bootstrap_delivery: Option<String>,
}

pub struct PresetRepo<'a> {
    conn: &'a Connection,
}

const SELECT_COLS: &str = "id, name, command, args_json, env_json, is_default, sort_order, created_at, bootstrap_prompt_template, bootstrap_delivery, supports_resume";

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

    pub fn insert(&self, input: NewAgentPreset) -> Result<(AgentPreset, DbEvent)> {
        validate_args_json(&input.args_json)?;
        validate_env_json(&input.env_json)?;
        validate_bootstrap_delivery(input.bootstrap_delivery.as_deref())?;

        let id = new_id();
        let ts = now();
        let sort_order = input.sort_order.unwrap_or_else(|| next_sort_order(self.conn));

        self.conn.execute(
            "INSERT INTO agent_presets
               (id, name, command, args_json, env_json, is_default, sort_order, created_at,
                bootstrap_prompt_template, bootstrap_delivery)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9)",
            params![
                id,
                input.name,
                input.command,
                input.args_json,
                input.env_json,
                sort_order,
                ts,
                input.bootstrap_prompt_template,
                input.bootstrap_delivery,
            ],
        )?;

        let preset = self
            .get(&id)?
            .ok_or_else(|| anyhow!("preset {id} vanished after insert"))?;
        Ok((preset, DbEvent::insert(Entity::Preset, id)))
    }

    pub fn update(&self, id: &str, patch: PresetPatch) -> Result<(AgentPreset, DbEvent)> {
        validate_args_json(&patch.args_json)?;
        validate_env_json(&patch.env_json)?;
        validate_bootstrap_delivery(patch.bootstrap_delivery.as_deref())?;

        let affected = self.conn.execute(
            "UPDATE agent_presets SET
               name = ?2,
               command = ?3,
               args_json = ?4,
               env_json = ?5,
               sort_order = ?6,
               bootstrap_prompt_template = ?7,
               bootstrap_delivery = ?8
             WHERE id = ?1",
            params![
                id,
                patch.name,
                patch.command,
                patch.args_json,
                patch.env_json,
                patch.sort_order,
                patch.bootstrap_prompt_template,
                patch.bootstrap_delivery,
            ],
        )?;
        if affected == 0 {
            bail!("preset not found");
        }
        let preset = self
            .get(id)?
            .ok_or_else(|| anyhow!("preset {id} vanished after update"))?;
        Ok((preset, DbEvent::update(Entity::Preset, id.to_string())))
    }

    /// Delete a preset. Rules:
    ///   - Reject if this is the only row (the app needs at least one
    ///     preset to launch an agent).
    ///   - If the row is the current default, promote the next row
    ///     (lowest `sort_order`, then oldest `created_at`) to default in
    ///     the same transaction — keeps `get_default()` deterministic.
    pub fn delete(&self, id: &str) -> Result<DbEvent> {
        let tx = self.conn.unchecked_transaction()?;

        let total: i64 = tx
            .query_row("SELECT COUNT(*) FROM agent_presets", [], |r| r.get(0))?;
        if total <= 1 {
            bail!("cannot delete the only agent preset");
        }

        let (was_default, ) : (i64,) = tx.query_row(
            "SELECT is_default FROM agent_presets WHERE id = ?1",
            [id],
            |r| Ok((r.get::<_, i64>(0)?,)),
        ).optional()?.ok_or_else(|| anyhow!("preset not found"))?;

        tx.execute("DELETE FROM agent_presets WHERE id = ?1", [id])?;

        if was_default != 0 {
            // Promote the next candidate.
            let next_id: Option<String> = tx
                .query_row(
                    "SELECT id FROM agent_presets
                     ORDER BY sort_order, created_at LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(next) = next_id {
                tx.execute(
                    "UPDATE agent_presets SET is_default = 1 WHERE id = ?1",
                    [next],
                )?;
            }
        }

        tx.commit()?;
        Ok(DbEvent::delete(Entity::Preset, id.to_string()))
    }

    /// Set a single preset as the default, atomically. Pre-checks
    /// existence inside the transaction so a bogus id can't wipe every
    /// flag and leave the DB with zero defaults.
    pub fn set_default(&self, id: &str) -> Result<DbEvent> {
        let tx = self.conn.unchecked_transaction()?;

        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM agent_presets WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_none() {
            bail!("preset not found");
        }

        tx.execute(
            "UPDATE agent_presets SET is_default = CASE WHEN id = ?1 THEN 1 ELSE 0 END",
            [id],
        )?;
        tx.commit()?;
        Ok(DbEvent::update(Entity::Preset, id.to_string()))
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

fn validate_args_json(s: &str) -> Result<()> {
    serde_json::from_str::<Vec<String>>(s)
        .map_err(|e| anyhow!("args must be a JSON array of strings: {e}"))?;
    Ok(())
}

fn validate_env_json(s: &str) -> Result<()> {
    serde_json::from_str::<BTreeMap<String, String>>(s)
        .map_err(|e| anyhow!("env must be a JSON object of string→string: {e}"))?;
    Ok(())
}

fn validate_bootstrap_delivery(s: Option<&str>) -> Result<()> {
    match s {
        None => Ok(()),
        Some(v) if BootstrapDelivery::parse(v).is_some() => Ok(()),
        Some(v) => bail!("bootstrap_delivery must be 'argv' or 'append_system_prompt' (got {v:?})"),
    }
}

fn next_sort_order(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM agent_presets",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
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
        supports_resume: row.get::<_, i64>(10)? != 0,
    })
}
