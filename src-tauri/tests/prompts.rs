//! Parity spec 03: note-style CRUD rules (AS1–AS7) against a real store.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

use scribular_lib::store::key::MemoryKeyStore;
use scribular_lib::store::{InputMode, NewSession, Store, StoreError};

const DORA: &str = "default-therapy-dora";
const NARRATIVE: &str = "default-narrative-therapy";

struct TempDb(PathBuf);

impl TempDb {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        Self(std::env::temp_dir().join(format!(
            "scribular-prompts-{}-{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        )))
    }

    fn open(&self) -> Store {
        Store::open(&self.0, &MemoryKeyStore::with_key([7; 32])).unwrap()
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = self.0.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
    }
}

fn active(s: &Store) -> String {
    s.get_active_prompt_id().unwrap().unwrap()
}

#[test]
fn as1_create_adds_a_starred_custom_style_last() {
    let db = TempDb::new();
    let s = db.open();
    let p = s.create_prompt("Mine", "text").unwrap();
    assert!(!p.is_default);
    assert!(p.starred);
    let all = s.list_prompts().unwrap();
    assert_eq!(all.len(), 8);
    assert_eq!(all.last().unwrap(), &p);
}

#[test]
fn as2_built_ins_cannot_be_edited_or_deleted() {
    let db = TempDb::new();
    let s = db.open();
    let before = s.get_prompt(DORA).unwrap();
    assert!(matches!(
        s.update_prompt(DORA, "x", "y"),
        Err(StoreError::BuiltInPrompt)
    ));
    assert!(matches!(s.delete_prompt(DORA), Err(StoreError::BuiltInPrompt)));
    assert_eq!(s.get_prompt(DORA).unwrap(), before);
}

#[test]
fn as3_blank_text_is_refused_and_a_blank_name_is_untitled() {
    let db = TempDb::new();
    let s = db.open();
    let p = s.create_prompt("Mine", "text").unwrap();
    assert!(matches!(
        s.update_prompt(&p.id, "Mine", " \n\t "),
        Err(StoreError::EmptyPrompt)
    ));
    s.update_prompt(&p.id, "  ", "new text").unwrap();
    let p = s.get_prompt(&p.id).unwrap().unwrap();
    assert_eq!(p.name, "Untitled Style");
    assert_eq!(p.text, "new text");
}

#[test]
fn as4_the_last_favourite_cannot_be_unstarred() {
    let db = TempDb::new();
    let s = db.open();
    assert!(matches!(
        s.set_prompt_starred(DORA, false),
        Err(StoreError::LastFavourite)
    ));
    assert!(s.get_prompt(DORA).unwrap().unwrap().starred);
    assert_eq!(active(&s), DORA);
}

#[test]
fn as5_unstarring_the_active_style_moves_active_to_a_favourite() {
    let db = TempDb::new();
    let s = db.open();
    let x = s.create_prompt("X", "text").unwrap();
    s.set_active_prompt(&x.id).unwrap();
    s.set_prompt_starred(&x.id, false).unwrap();
    // DORA is the remaining (first) favourite.
    assert_eq!(active(&s), DORA);
    assert!(s.get_prompt(DORA).unwrap().unwrap().starred);
}

#[test]
fn as6_deleting_the_active_style_repairs_active_and_keeps_sessions() {
    let db = TempDb::new();
    let s = db.open();
    let x = s.create_prompt("X", "text").unwrap();
    s.set_active_prompt(&x.id).unwrap();
    s.set_prompt_starred(DORA, false).unwrap();
    let session = s
        .create_session(&NewSession {
            title: "t".into(),
            input_mode: InputMode::Mic,
            prompt_id: Some(x.id.clone()),
            model_id: None,
        })
        .unwrap();

    s.delete_prompt(&x.id).unwrap();

    let prompts = s.list_prompts().unwrap();
    let active_id = active(&s);
    // No favourite was left, so the first style becomes active and starred.
    assert_eq!(active_id, DORA);
    assert!(prompts.iter().any(|p| p.id == active_id && p.starred));
    assert_eq!(
        s.get_session(&session.id).unwrap().unwrap().prompt_id,
        Some(x.id)
    );
}

#[test]
fn set_active_stars_the_style() {
    let db = TempDb::new();
    let s = db.open();
    s.set_active_prompt(NARRATIVE).unwrap();
    assert_eq!(active(&s), NARRATIVE);
    assert!(s.get_prompt(NARRATIVE).unwrap().unwrap().starred);
}

#[test]
fn duplicate_copies_a_built_in_as_a_custom_style() {
    let db = TempDb::new();
    let s = db.open();
    let src = s.get_prompt(NARRATIVE).unwrap().unwrap();
    let copy = s.duplicate_prompt(NARRATIVE).unwrap();
    assert_eq!(copy.name, format!("{} (Copy)", src.name));
    assert_eq!(copy.text, src.text);
    assert!(!copy.is_default && copy.starred);
    s.update_prompt(&copy.id, "Edited", "mine").unwrap();
}

#[test]
fn as7_sync_keeps_stars_and_custom_rows_and_is_idempotent() {
    let db = TempDb::new();
    let s = db.open();
    let custom = s.create_prompt("Mine", "text").unwrap();
    s.set_prompt_starred(NARRATIVE, true).unwrap();
    s.set_active_prompt(&custom.id).unwrap();
    let before = s.list_prompts().unwrap();

    s.sync_builtins().unwrap();
    s.sync_builtins().unwrap();

    assert_eq!(s.list_prompts().unwrap(), before);
    assert_eq!(active(&s), custom.id);
}
