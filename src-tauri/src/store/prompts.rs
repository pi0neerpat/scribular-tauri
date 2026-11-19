//! Summary prompts. The defaults are the Electron app's, copied word for word
//! into `default_prompts.json` by `scripts/gen-default-prompts.mjs`.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::coach::{compile, StyleSpec};

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
    pub show_in_style_menu: bool,
    #[serde(default)]
    pub style_spec: Option<StyleSpec>,
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

pub(super) fn insert_initial(conn: &Connection, p: &Prompt) -> Result<()> {
    conn.execute(
        "INSERT INTO prompts (id, name, description, text, is_default, starred)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            p.id,
            p.name,
            p.description,
            p.text,
            p.is_default,
            p.show_in_style_menu
        ],
    )?;
    Ok(())
}

const COLUMNS: &str = "id, name, description, text, is_default, starred, style_spec";

fn from_row(row: &Row) -> rusqlite::Result<Prompt> {
    Ok(Prompt {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        text: row.get(3)?,
        is_default: row.get(4)?,
        show_in_style_menu: row.get(5)?,
        style_spec: row
            .get::<_, Option<String>>(6)?
            .map(|json| {
                serde_json::from_str(&json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
            })
            .transpose()?,
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

    /// A custom style (R3). The editor only calls this on first
    /// Save, so `text` is checked like an update.
    pub fn create_prompt(&self, name: &str, text: &str) -> Result<Prompt> {
        self.create_prompt_with_visibility(name, text, true)
    }

    pub fn create_prompt_with_visibility(
        &self,
        name: &str,
        text: &str,
        show_in_style_menu: bool,
    ) -> Result<Prompt> {
        let text = checked_text(text)?;
        self.with(|c| {
            let p = Prompt {
                id: new_id(c)?,
                name: clean_name(name),
                description: String::new(),
                text: text.into(),
                is_default: false,
                show_in_style_menu,
                style_spec: None,
            };
            insert_initial(c, &p)?;
            Ok(p)
        })
    }

    /// Copies any style as a custom `"<name> (Copy)"`.
    pub fn duplicate_prompt(&self, id: &str) -> Result<Prompt> {
        let src = self.get_prompt(id)?.ok_or_else(|| not_found(id))?;
        self.with(|c| {
            let p = Prompt {
                id: new_id(c)?,
                name: format!("{} (Copy)", src.name),
                is_default: false,
                show_in_style_menu: true,
                style_spec: None,
                ..src
            };
            insert_initial(c, &p)?;
            Ok(p)
        })
    }

    pub fn update_prompt(&self, id: &str, name: &str, text: &str) -> Result<()> {
        self.edit_prompt(id, name, text, None, false)
    }

    pub fn edit_prompt(
        &self,
        id: &str,
        name: &str,
        text: &str,
        show_in_style_menu: Option<bool>,
        detach_coach: bool,
    ) -> Result<()> {
        let text = checked_text(text)?;
        self.with(|c| {
            custom(c, id)?;
            let changed = c.execute(
                "UPDATE prompts SET name = ?2, text = ?3, starred = coalesce(?4, starred),
                     style_spec = CASE WHEN text = ?3 THEN style_spec ELSE NULL END
                 WHERE id = ?1 AND (style_spec IS NULL OR text = ?3 OR ?5)",
                params![id, clean_name(name), text, show_in_style_menu, detach_coach],
            )?;
            if changed == 0 {
                return Err(StoreError::DetachCoachStyle);
            }
            Ok(())
        })
    }

    pub(crate) fn save_coach_style(
        &self,
        original: Option<&Prompt>,
        name: &str,
        spec: &StyleSpec,
        use_next: bool,
    ) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
            return Err(StoreError::CoachStyle(
                "Enter a style name of 1 to 100 characters.",
            ));
        }
        let text = compile(spec).map_err(StoreError::CoachStyle)?;
        let json = serde_json::to_string(spec)?;
        self.with(|c| {
            let tx = c.transaction()?;
            let id = if let Some(original) = original {
                let previous = original.style_spec.as_ref().ok_or(
                    StoreError::CoachStyle("This style is no longer available to adjust."),
                )?;
                let current = tx.query_row(
                    &format!("SELECT {COLUMNS} FROM prompts WHERE id = ?1"),
                    [&original.id],
                    from_row,
                ).optional()?;
                if current.is_none_or(|p| p.is_default || p.name != original.name
                        || p.style_spec.as_ref() != Some(previous) || p.text != original.text) {
                    return Err(StoreError::CoachStyle("This style changed. Reopen it before adjusting."));
                }
                tx.execute(
                    "UPDATE prompts SET name = ?2, text = ?3, style_spec = ?4 WHERE id = ?1",
                    params![original.id, name, text, json],
                )?;
                original.id.clone()
            } else {
                let id = new_id(&tx)?;
                tx.execute(
                    "INSERT INTO prompts (id, name, description, text, is_default, starred, style_spec)
                     VALUES (?1, ?2, '', ?3, 0, 1, ?4)",
                    params![id, name, text, json],
                )?;
                id
            };
            if use_next {
                set_active(&tx, &id)?;
            }
            tx.commit()?;
            Ok(id)
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

    pub fn set_prompt_visibility(&self, id: &str, show_in_style_menu: bool) -> Result<()> {
        self.with(|c| {
            let changed = c.execute(
                "UPDATE prompts SET starred = ?2 WHERE id = ?1",
                params![id, show_in_style_menu],
            )?;
            if changed == 0 {
                return Err(not_found(id));
            }
            Ok(())
        })
    }

    /// Makes an existing style active for new recordings.
    pub fn set_active_prompt(&self, id: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            set_active(&tx, id)?;
            tx.commit()?;
            Ok(())
        })
    }

    /// R9: brings the built-in rows in line with the embedded JSON. Missing
    /// ids are inserted hidden; existing ones keep their visibility. Custom rows
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
    Ok(
        c.query_row("SELECT 'custom-' || lower(hex(randomblob(16)))", [], |r| {
            r.get(0)
        })?,
    )
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

pub(super) fn set_active(c: &Connection, id: &str) -> Result<()> {
    c.query_row("SELECT 1 FROM prompts WHERE id = ?1", [id], |_| Ok(()))
        .optional()?
        .ok_or_else(|| not_found(id))?;
    settings::insert(c, ACTIVE_PROMPT_ID, id)
}

fn repair_active(c: &Connection) -> Result<()> {
    let active_ok = match settings::select(c, ACTIVE_PROMPT_ID)? {
        Some(id) => c
            .query_row("SELECT 1 FROM prompts WHERE id = ?1", [&id], |_| Ok(()))
            .optional()?
            .is_some(),
        None => false,
    };
    if active_ok {
        return Ok(());
    }
    let next: Option<String> = c
        .query_row(
            "SELECT id FROM prompts ORDER BY starred DESC, is_default, rowid LIMIT 1",
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

    #[test]
    fn coach_save_and_detach_are_atomic_and_leave_saved_notes_alone() {
        let path =
            std::env::temp_dir().join(format!("scribular-coach-style-{}.db", std::process::id()));
        let keys = MemoryKeyStore::with_key([8; 32]);
        let store = Store::open(&path, &keys).unwrap();
        let existing_active = store.get_active_prompt_id().unwrap().unwrap();
        let session = store
            .create_session(&crate::store::NewSession {
                title: "Existing note".into(),
                input_mode: crate::store::InputMode::Mic,
                prompt_id: Some(existing_active.clone()),
                model_id: None,
            })
            .unwrap();
        store
            .finish_summary(
                &session.id,
                "Existing summary.",
                &existing_active,
                "test",
                &Default::default(),
            )
            .unwrap();
        let mut spec = crate::coach::test_style();
        let id = store
            .save_coach_style(None, "Made here", &spec, false)
            .unwrap();
        assert_eq!(
            store.get_active_prompt_id().unwrap(),
            Some(existing_active.clone())
        );
        let created = store.get_prompt(&id).unwrap().unwrap();
        assert!(created.show_in_style_menu);
        assert_eq!(created.style_spec.as_ref(), Some(&spec));

        store
            .edit_prompt(&id, "Renamed", &created.text, Some(false), false)
            .unwrap();
        let edited = store.get_prompt(&id).unwrap().unwrap();
        assert_eq!(edited.style_spec, created.style_spec);
        assert!(!edited.show_in_style_menu);
        assert!(matches!(
            store.edit_prompt(&id, "Renamed", "detached", Some(true), false),
            Err(StoreError::DetachCoachStyle)
        ));
        assert_eq!(store.get_prompt(&id).unwrap(), Some(edited.clone()));

        spec.compiler_version += 1;
        assert!(store
            .save_coach_style(Some(&edited), "Invalid", &spec, true)
            .is_err());
        assert_eq!(store.get_active_prompt_id().unwrap(), Some(existing_active));
        assert_eq!(store.get_prompt(&id).unwrap(), Some(edited.clone()));
        spec.compiler_version -= 1;
        spec.sentence_max = 3;
        assert_eq!(
            store
                .save_coach_style(Some(&edited), "Adjusted", &spec, true)
                .unwrap(),
            id
        );
        assert_eq!(
            store.get_active_prompt_id().unwrap().as_deref(),
            Some(id.as_str())
        );
        let adjusted = store.get_prompt(&id).unwrap().unwrap();
        assert_eq!(adjusted.style_spec.as_ref(), Some(&spec));
        assert!(!adjusted.show_in_style_menu);
        assert_ne!(adjusted.text, created.text);
        assert!(store
            .save_coach_style(Some(&edited), "Stale", &spec, true)
            .is_err());
        assert_eq!(
            store.get_session(&session.id).unwrap().unwrap().summary,
            "Existing summary."
        );

        store
            .edit_prompt(&id, "Adjusted", "detached", None, true)
            .unwrap();
        drop(store);
        let reopened = Store::open(&path, &keys).unwrap();
        assert!(reopened
            .get_prompt(&id)
            .unwrap()
            .unwrap()
            .style_spec
            .is_none());
        drop(reopened);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    /// AS7: edited built-in text is restored with its visibility kept, and a
    /// built-in id missing from the DB is inserted hidden.
    #[test]
    fn sync_restores_built_in_text_and_inserts_missing_ids() {
        let path = std::env::temp_dir().join(format!("scribular-sync-{}.db", std::process::id()));
        let s = Store::open(&path, &MemoryKeyStore::with_key([7; 32])).unwrap();
        let shipped = defaults().unwrap().prompts;
        s.set_prompt_visibility(&shipped[1].id, true).unwrap();
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
        assert!(restored.show_in_style_menu);
        let reinserted = s.get_prompt(&shipped[2].id).unwrap().unwrap();
        assert_eq!(reinserted.text, shipped[2].text);
        assert!(!reinserted.show_in_style_menu && reinserted.is_default);
        drop(s);
        for suffix in ["", "-wal", "-shm"] {
            let mut p = path.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
    }
}
