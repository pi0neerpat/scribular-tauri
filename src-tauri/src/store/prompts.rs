//! Summary prompts. The defaults are the Electron app's, copied word for word
//! into `default_prompts.json` by `scripts/gen-default-prompts.mjs`.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    settings::{self, ACTIVE_PROMPT_ID},
    Result, Store, StoreError,
};

/// Spec 03 R4: the longest built-in is about 2,000 characters; summarize
/// rejects prompts too long for the context anyway.
pub const MAX_PROMPT_CHARS: usize = 8_000;
const UNTITLED: &str = "Untitled Style";

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

    pub fn get_active_prompt_id(&self) -> Result<Option<String>> {
        self.get_setting(ACTIVE_PROMPT_ID)
    }

    /// A custom, starred style (R3). The editor only calls this on first
    /// Save, so `text` is checked like an update.
    pub fn create_prompt(&self, name: &str, text: &str) -> Result<Prompt> {
        let text = checked_text(text)?;
        self.with(|c| {
            let p = Prompt {
                id: new_id(c)?,
                name: clean_name(name),
                description: String::new(),
                text: text.into(),
                is_default: false,
                starred: true,
            };
            insert(c, &p)?;
            Ok(p)
        })
    }

    /// Copies any style as a custom, starred `"<name> (Copy)"`.
    pub fn duplicate_prompt(&self, id: &str) -> Result<Prompt> {
        let src = self.get_prompt(id)?.ok_or_else(|| not_found(id))?;
        self.with(|c| {
            let p = Prompt {
                id: new_id(c)?,
                name: format!("{} (Copy)", src.name),
                is_default: false,
                starred: true,
                ..src
            };
            insert(c, &p)?;
            Ok(p)
        })
    }

    pub fn update_prompt(&self, id: &str, name: &str, text: &str) -> Result<()> {
        let text = checked_text(text)?;
        self.with(|c| {
            custom(c, id)?;
            c.execute(
                "UPDATE prompts SET name = ?2, text = ?3 WHERE id = ?1",
                params![id, clean_name(name), text],
            )?;
            Ok(())
        })
    }

    /// Sessions keep their `prompt_id` (R8); the active style moves on (R6).
    pub fn delete_prompt(&self, id: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            custom(&tx, id)?;
            tx.execute("DELETE FROM prompts WHERE id = ?1", [id])?;
            repair_active(&tx)?;
            tx.commit()?;
            Ok(())
        })
    }

    /// At least one style stays starred (R5); unstarring the active style
    /// moves active to the first remaining favourite (R6).
    pub fn set_prompt_starred(&self, id: &str, starred: bool) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            let changed = tx.execute(
                "UPDATE prompts SET starred = ?2 WHERE id = ?1",
                params![id, starred],
            )?;
            if changed == 0 {
                return Err(not_found(id));
            }
            if starred_count(&tx)? == 0 {
                return Err(StoreError::LastFavourite);
            }
            repair_active(&tx)?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Makes `id` the style new recordings use, starring it if needed (R7).
    pub fn set_active_prompt(&self, id: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            set_active(&tx, id)?;
            tx.commit()?;
            Ok(())
        })
    }

    /// R9: brings the built-in rows in line with the embedded JSON. Missing
    /// ids are inserted unstarred; existing ones keep `starred`. Custom rows
    /// are never touched.
    pub fn sync_builtins(&self) -> Result<()> {
        let defaults = defaults()?;
        self.with(|c| {
            let tx = c.transaction()?;
            for p in &defaults.prompts {
                tx.execute(
                    "INSERT INTO prompts (id, name, description, text, is_default, starred)
                     VALUES (?1, ?2, ?3, ?4, 1, 0)
                     ON CONFLICT(id) DO UPDATE SET name = excluded.name,
                         description = excluded.description, text = excluded.text
                     WHERE is_default = 1",
                    params![p.id, p.name, p.description, p.text],
                )?;
            }
            repair_active(&tx)?;
            tx.commit()?;
            Ok(())
        })
    }
}

fn not_found(id: &str) -> StoreError {
    StoreError::NotFound(format!("style {id}"))
}

fn clean_name(name: &str) -> String {
    match name.trim() {
        "" => UNTITLED.into(),
        n => n.into(),
    }
}

fn checked_text(text: &str) -> Result<&str> {
    if text.trim().is_empty() {
        Err(StoreError::EmptyPrompt)
    } else if text.chars().count() > MAX_PROMPT_CHARS {
        Err(StoreError::PromptTooLong)
    } else {
        Ok(text)
    }
}

/// `custom-<32 hex>` from SQLite's CSPRNG, so no id crate is needed.
fn new_id(c: &Connection) -> Result<String> {
    Ok(c.query_row("SELECT 'custom-' || lower(hex(randomblob(16)))", [], |r| {
        r.get(0)
    })?)
}

/// Fails unless `id` names a custom style.
fn custom(c: &Connection, id: &str) -> Result<()> {
    let is_default: Option<bool> = c
        .query_row("SELECT is_default FROM prompts WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    match is_default {
        None => Err(not_found(id)),
        Some(true) => Err(StoreError::BuiltInPrompt),
        Some(false) => Ok(()),
    }
}

fn starred_count(c: &Connection) -> Result<i64> {
    Ok(c.query_row("SELECT count(*) FROM prompts WHERE starred", [], |r| {
        r.get(0)
    })?)
}

pub(super) fn set_active(c: &Connection, id: &str) -> Result<()> {
    if c.execute("UPDATE prompts SET starred = 1 WHERE id = ?1", [id])? == 0 {
        return Err(not_found(id));
    }
    settings::insert(c, ACTIVE_PROMPT_ID, id)
}

/// R6: the active id names an existing, starred style. Otherwise it moves to
/// the first favourite in list order, or, with none left, the first style,
/// which is starred.
fn repair_active(c: &Connection) -> Result<()> {
    let active_ok = settings::select(c, ACTIVE_PROMPT_ID)?.is_some_and(|id| {
        c.query_row(
            "SELECT 1 FROM prompts WHERE id = ?1 AND starred",
            [&id],
            |_| Ok(()),
        )
        .is_ok()
    });
    if active_ok {
        return Ok(());
    }
    let next: Option<String> = c
        .query_row(
            "SELECT id FROM prompts ORDER BY starred DESC, rowid LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match next {
        Some(id) => set_active(c, &id),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::key::MemoryKeyStore;

    /// AS7: edited built-in text is restored with its star kept, and a
    /// built-in id missing from the DB is inserted unstarred.
    #[test]
    fn sync_restores_built_in_text_and_inserts_missing_ids() {
        let path = std::env::temp_dir().join(format!("scribular-sync-{}.db", std::process::id()));
        let s = Store::open(&path, &MemoryKeyStore::with_key([7; 32])).unwrap();
        let shipped = defaults().unwrap().prompts;
        s.set_prompt_starred(&shipped[1].id, true).unwrap();
        s.with(|c| {
            c.execute(
                "UPDATE prompts SET text = 'tampered' WHERE id = ?1",
                [&shipped[1].id],
            )?;
            c.execute("DELETE FROM prompts WHERE id = ?1", [&shipped[2].id])?;
            Ok(())
        })
        .unwrap();

        s.sync_builtins().unwrap();

        let restored = s.get_prompt(&shipped[1].id).unwrap().unwrap();
        assert_eq!(restored.text, shipped[1].text);
        assert!(restored.starred);
        let reinserted = s.get_prompt(&shipped[2].id).unwrap().unwrap();
        assert_eq!(reinserted.text, shipped[2].text);
        assert!(!reinserted.starred && reinserted.is_default);
        drop(s);
        for suffix in ["", "-wal", "-shm"] {
            let mut p = path.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
    }
}
