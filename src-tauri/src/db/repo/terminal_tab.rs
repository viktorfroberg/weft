use super::now;
use crate::db::events::{DbEvent, Entity};
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TabKind {
    Shell,
    Agent,
}

impl TabKind {
    fn as_str(&self) -> &'static str {
        match self {
            TabKind::Shell => "shell",
            TabKind::Agent => "agent",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "shell" => Some(TabKind::Shell),
            "agent" => Some(TabKind::Agent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TabState {
    Live,
    Dormant,
}

impl TabState {
    fn as_str(&self) -> &'static str {
        match self {
            TabState::Live => "live",
            TabState::Dormant => "dormant",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "live" => Some(TabState::Live),
            "dormant" => Some(TabState::Dormant),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalTabRow {
    pub id: String,
    pub task_id: String,
    pub kind: TabKind,
    pub label: String,
    pub preset_id: Option<String>,
    pub sort_order: i64,
    pub state: TabState,
    pub closed_at: Option<i64>,
    pub last_exit_code: Option<i32>,
    pub cwd: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewTerminalTab {
    pub id: String,
    pub task_id: String,
    pub kind: TabKind,
    pub label: String,
    pub preset_id: Option<String>,
    pub cwd: Option<String>,
}

pub struct TerminalTabRepo<'a> {
    conn: &'a Connection,
}

const SELECT_COLS: &str = "id, task_id, kind, label, preset_id, sort_order, state, closed_at, last_exit_code, cwd, created_at";

impl<'a> TerminalTabRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_for_task(&self, task_id: &str) -> Result<Vec<TerminalTabRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SELECT_COLS} FROM terminal_tabs WHERE task_id = ?1 ORDER BY sort_order, created_at"
        ))?;
        let rows = stmt.query_map([task_id], row_to_tab)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: &str) -> Result<Option<TerminalTabRow>> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {SELECT_COLS} FROM terminal_tabs WHERE id = ?1"),
                [id],
                row_to_tab,
            )
            .optional()?;
        Ok(row)
    }

    pub fn list_all_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT id FROM terminal_tabs")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn insert(&self, input: NewTerminalTab) -> Result<(TerminalTabRow, DbEvent)> {
        let sort_order = next_sort_order(self.conn, &input.task_id);
        let ts = now();
        self.conn.execute(
            "INSERT INTO terminal_tabs
               (id, task_id, kind, label, preset_id, sort_order, state, closed_at, last_exit_code, cwd, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'live', NULL, NULL, ?7, ?8)",
            params![
                input.id,
                input.task_id,
                input.kind.as_str(),
                input.label,
                input.preset_id,
                sort_order,
                input.cwd,
                ts,
            ],
        )?;
        let row = self
            .get(&input.id)?
            .ok_or_else(|| anyhow!("terminal_tab {} vanished after insert", input.id))?;
        let ev = DbEvent::insert(Entity::TerminalTab, row.id.clone());
        Ok((row, ev))
    }

    pub fn update_label(&self, id: &str, label: &str) -> Result<DbEvent> {
        let n = self
            .conn
            .execute("UPDATE terminal_tabs SET label = ?2 WHERE id = ?1", params![id, label])?;
        if n == 0 {
            return Err(anyhow!("terminal_tab {id} not found"));
        }
        Ok(DbEvent::update(Entity::TerminalTab, id.to_string()))
    }

    /// Flip live → dormant with a closing timestamp + optional exit code.
    /// Returns `Ok(true)` if a row was updated, `Ok(false)` if the row is
    /// gone (deleted out from under us). Waiters rely on the false path
    /// to skip orphan scrollback writes.
    pub fn mark_dormant(
        &self,
        id: &str,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        let ts = now();
        let n = self.conn.execute(
            "UPDATE terminal_tabs
               SET state = 'dormant', closed_at = ?2, last_exit_code = ?3
             WHERE id = ?1 AND state = 'live'",
            params![id, ts, exit_code],
        )?;
        Ok(n > 0)
    }

    /// Flip dormant → live (used on resume). Does not touch scrollback.
    pub fn mark_live(&self, id: &str) -> Result<DbEvent> {
        let n = self.conn.execute(
            "UPDATE terminal_tabs
               SET state = 'live', closed_at = NULL, last_exit_code = NULL
             WHERE id = ?1",
            params![id],
        )?;
        if n == 0 {
            return Err(anyhow!("terminal_tab {id} not found"));
        }
        Ok(DbEvent::update(Entity::TerminalTab, id.to_string()))
    }

    pub fn delete(&self, id: &str) -> Result<DbEvent> {
        let n = self.conn.execute("DELETE FROM terminal_tabs WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(anyhow!("terminal_tab {id} not found"));
        }
        Ok(DbEvent::delete(Entity::TerminalTab, id.to_string()))
    }
}

fn next_sort_order(conn: &Connection, task_id: &str) -> i64 {
    conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM terminal_tabs WHERE task_id = ?1",
        [task_id],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
}

fn row_to_tab(row: &rusqlite::Row) -> rusqlite::Result<TerminalTabRow> {
    let kind_raw: String = row.get(2)?;
    let state_raw: String = row.get(6)?;
    Ok(TerminalTabRow {
        id: row.get(0)?,
        task_id: row.get(1)?,
        kind: TabKind::parse(&kind_raw).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown tab kind {kind_raw:?}"),
                )),
            )
        })?,
        label: row.get(3)?,
        preset_id: row.get(4)?,
        sort_order: row.get(5)?,
        state: TabState::parse(&state_raw).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown tab state {state_raw:?}"),
                )),
            )
        })?,
        closed_at: row.get(7)?,
        last_exit_code: row.get(8)?,
        cwd: row.get(9)?,
        created_at: row.get(10)?,
    })
}
