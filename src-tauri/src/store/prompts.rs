//! Summary prompts. The defaults are the Electron app's, copied word for word
//! into `default_prompts.json` by `scripts/gen-default-prompts.mjs`.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Result, Store};

const DEFAULT_PROMPTS_JSON: &str = include_str!("default_prompts.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub id: String,
    pub name: String,
    pub description: String,
    pub text: String,
    pub is_default: bool,
    pub starred: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Defaults {
    pub active_prompt_id: String,
    pub prompts: Vec<Prompt>,
}

pub(super) fn defaults() -> Result<Defaults> {
    Ok(serde_json::from_str(DEFAULT_PROMPTS_JSON)?)
}

pub(super) fn insert(conn: &Connection, p: &Prompt) -> Result<()> {
    conn.execute(
        "INSERT INTO prompts (id, name, description, text, is_default, starred)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![p.id, p.name, p.description, p.text, p.is_default, p.starred],
    )?;
    Ok(())
}

const COLUMNS: &str = "id, name, description, text, is_default, starred";

fn from_row(row: &Row) -> rusqlite::Result<Prompt> {
    Ok(Prompt {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        text: row.get(3)?,
        is_default: row.get(4)?,
        starred: row.get(5)?,
    })
}

impl Store {
    /// All prompts in insertion order, so the defaults keep Electron's order.
    pub fn list_prompts(&self) -> Result<Vec<Prompt>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM prompts ORDER BY rowid"))?;
            let rows = stmt.query_map([], from_row)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn get_prompt(&self, id: &str) -> Result<Option<Prompt>> {
        self.with(|c| {
            Ok(c.query_row(
                &format!("SELECT {COLUMNS} FROM prompts WHERE id = ?1"),
                [id],
                from_row,
            )
            .optional()?)
        })
    }
}
