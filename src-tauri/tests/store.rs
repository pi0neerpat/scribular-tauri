//! U2: the SQLite store survives reopening, cascades deletes, seeds the
//! Electron prompts, migrates idempotently, and recovers crashed sessions.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

use scribular_lib::store::{
    AudioSource, InputMode, NewSession, RecordingSettings, SessionPatch, SessionStatus, Store,
    Timings, ACTIVE_PROMPT_ID,
};
use scribular_lib::text::Segment;

/// A fresh database path, removed (with its WAL files) on drop.
struct TempDb(PathBuf);

impl TempDb {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "scribular-store-{}-{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        );
        let db = Self(std::env::temp_dir().join(name));
        db.remove();
        db
    }

    fn open(&self) -> Store {
        Store::open(&self.0).unwrap()
    }

    fn remove(&self) {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = self.0.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        self.remove();
    }
}

fn new_session(title: &str) -> NewSession {
    NewSession {
        title: title.into(),
        input_mode: InputMode::MicAndSystem,
        prompt_id: Some("default-therapy-dora".into()),
        model_id: Some("gemma-2-2b-it-q4km".into()),
    }
}

fn segment(start_ms: i64, text: &str) -> Segment {
    Segment {
        start_ms,
        end_ms: start_ms + 1000,
        text: Some(text.into()),
    }
}

#[test]
fn sessions_persist_across_reopen() {
    let db = TempDb::new();
    let id = {
        let store = db.open();
        let s = store.create_session(&new_session("Monday")).unwrap();
        store.save_transcript(&s.id, "hello there", 61_000).unwrap();
        store.set_status(&s.id, SessionStatus::Done, None).unwrap();
        s.id
    };

    let store = db.open();
    let s = store.get_session(&id).unwrap().unwrap();
    assert_eq!(s.title, "Monday");
    assert_eq!(s.transcript, "hello there");
    assert_eq!(s.duration_ms, Some(61_000));
    assert_eq!(s.status, SessionStatus::Done);
    assert_eq!(s.input_mode, InputMode::MicAndSystem);
    assert_eq!(s.prompt_id.as_deref(), Some("default-therapy-dora"));
    assert_eq!(store.list_sessions().unwrap().len(), 1);
}

#[test]
fn list_is_newest_first() {
    let db = TempDb::new();
    let store = db.open();
    let a = store.create_session(&new_session("a")).unwrap();
    let b = store.create_session(&new_session("b")).unwrap();
    let ids: Vec<_> = store
        .list_sessions()
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, [b.id, a.id]);
}

#[test]
fn update_applies_only_present_fields() {
    let db = TempDb::new();
    let store = db.open();
    let s = store.create_session(&new_session("old")).unwrap();
    store.save_transcript(&s.id, "words", 1000).unwrap();

    let updated = store
        .update_session(
            &s.id,
            &SessionPatch {
                title: Some("new".into()),
                summary: Some("sum".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(updated.title, "new");
    assert_eq!(updated.summary, "sum");
    assert_eq!(updated.transcript, "words");
    assert_eq!(store.get_session(&s.id).unwrap().unwrap(), updated);

    let empty: SessionPatch = serde_json::from_str("{}").unwrap();
    assert_eq!(store.update_session(&s.id, &empty).unwrap(), updated);
    assert!(store.update_session("missing", &empty).is_err());
}

#[test]
fn timings_round_trip_as_json() {
    let db = TempDb::new();
    let store = db.open();
    let s = store.create_session(&new_session("t")).unwrap();
    assert_eq!(s.timings, None);
    let timings = Timings {
        tail_transcription_ms: Some(800),
        model_load_ms: Some(2100),
        chunk_ms: vec![9000, 8700],
        combine_ms: Some(4000),
        save_ms: Some(3),
    };
    store.set_summary(&s.id, "summary", &timings).unwrap();
    let s = store.get_session(&s.id).unwrap().unwrap();
    assert_eq!(s.summary, "summary");
    assert_eq!(s.timings, Some(timings));
}

#[test]
fn delete_cascades_to_segments_and_audio() {
    let db = TempDb::new();
    let store = db.open();
    let keep = store.create_session(&new_session("keep")).unwrap();
    let gone = store.create_session(&new_session("gone")).unwrap();
    for s in [&keep, &gone] {
        store
            .add_segment(&s.id, AudioSource::Mic, &segment(0, "hi"))
            .unwrap();
        store
            .append_audio_chunk(&s.id, AudioSource::Mic, 0, &[1, -2, 3])
            .unwrap();
    }

    assert!(store.delete_session(&gone.id).unwrap());
    assert!(!store.delete_session(&gone.id).unwrap());
    assert!(store.get_session(&gone.id).unwrap().is_none());
    assert!(store
        .segments(&gone.id, AudioSource::Mic)
        .unwrap()
        .is_empty());
    assert_eq!(store.audio_chunk_count(&gone.id).unwrap(), 0);

    assert_eq!(store.segments(&keep.id, AudioSource::Mic).unwrap().len(), 1);
    assert_eq!(store.audio_chunk_count(&keep.id).unwrap(), 1);
}

#[test]
fn audio_round_trips_in_chunk_order_per_source() {
    let db = TempDb::new();
    let store = db.open();
    let s = store.create_session(&new_session("audio")).unwrap();
    store
        .append_audio_chunk(&s.id, AudioSource::Mic, 1, &[3, i16::MAX])
        .unwrap();
    store
        .append_audio_chunk(&s.id, AudioSource::Mic, 0, &[i16::MIN, -1])
        .unwrap();
    store
        .append_audio_chunk(&s.id, AudioSource::System, 0, &[42])
        .unwrap();
    assert_eq!(
        store.audio_samples(&s.id, AudioSource::Mic).unwrap(),
        [i16::MIN, -1, 3, i16::MAX]
    );
    assert_eq!(
        store.audio_samples(&s.id, AudioSource::System).unwrap(),
        [42]
    );
    // A repeated (source, seq) is a writer bug, not something to overwrite.
    assert!(store
        .append_audio_chunk(&s.id, AudioSource::Mic, 0, &[0])
        .is_err());
}

#[test]
fn saving_the_transcript_deletes_audio() {
    let db = TempDb::new();
    let store = db.open();
    let s = store.create_session(&new_session("ktd2")).unwrap();
    store
        .append_audio_chunk(&s.id, AudioSource::Mic, 0, &[1; 16])
        .unwrap();
    store
        .append_audio_chunk(&s.id, AudioSource::System, 0, &[2; 16])
        .unwrap();
    store.save_transcript(&s.id, "done", 1).unwrap();
    assert_eq!(store.audio_chunk_count(&s.id).unwrap(), 0);
    assert!(store.save_transcript("missing", "x", 1).is_err());
}

#[test]
fn segments_come_back_in_time_order() {
    let db = TempDb::new();
    let store = db.open();
    let s = store.create_session(&new_session("seg")).unwrap();
    store
        .add_segment(&s.id, AudioSource::Mic, &segment(5000, "later"))
        .unwrap();
    store
        .add_segment(&s.id, AudioSource::Mic, &segment(0, "first"))
        .unwrap();
    store
        .add_segment(
            &s.id,
            AudioSource::Mic,
            &Segment {
                start_ms: 9000,
                end_ms: 9500,
                text: None,
            },
        )
        .unwrap();
    store
        .add_segment(&s.id, AudioSource::System, &segment(1000, "other"))
        .unwrap();
    let texts: Vec<_> = store
        .segments(&s.id, AudioSource::Mic)
        .unwrap()
        .into_iter()
        .map(|s| s.text)
        .collect();
    assert_eq!(texts, [Some("first".into()), Some("later".into()), None]);
}

#[test]
fn seeds_the_electron_default_prompts() {
    let db = TempDb::new();
    let store = db.open();
    let prompts = store.list_prompts().unwrap();

    let json: serde_json::Value =
        serde_json::from_str(include_str!("../src/store/default_prompts.json")).unwrap();
    let expected = json["prompts"].as_array().unwrap();
    assert_eq!(prompts.len(), 7);
    assert_eq!(prompts.len(), expected.len());
    for (p, e) in prompts.iter().zip(expected) {
        assert_eq!(serde_json::to_value(p).unwrap(), *e);
    }
    assert!(prompts.iter().all(|p| p.is_default));

    let active = store.get_setting(ACTIVE_PROMPT_ID).unwrap().unwrap();
    assert_eq!(active, json["activePromptId"].as_str().unwrap());
    assert!(store.get_prompt(&active).unwrap().is_some());
}

#[test]
fn migrations_are_idempotent() {
    let db = TempDb::new();
    let version = db.open().schema_version().unwrap();
    assert!(version >= 1);
    for _ in 0..3 {
        let store = db.open();
        assert_eq!(store.schema_version().unwrap(), version);
        assert_eq!(store.list_prompts().unwrap().len(), 7);
    }
}

#[test]
fn settings_upsert() {
    let db = TempDb::new();
    let store = db.open();
    assert_eq!(store.get_setting("k").unwrap(), None);
    store.set_setting("k", "1").unwrap();
    store.set_setting("k", "2").unwrap();
    assert_eq!(store.get_setting("k").unwrap().as_deref(), Some("2"));
    drop(store);
    assert_eq!(db.open().get_setting("k").unwrap().as_deref(), Some("2"));
}

#[test]
fn recording_settings_round_trip_and_clear() {
    let db = TempDb::new();
    let store = db.open();
    let seeded = store.recording_settings().unwrap();
    assert_eq!(seeded.input_mode, InputMode::Mic);
    assert_eq!(seeded.target_app, None);
    assert!(seeded.prompt_id.is_some());

    let chosen = RecordingSettings {
        input_mode: InputMode::System,
        target_app: Some("Zoom".into()),
        prompt_id: Some("default-therapy-dora".into()),
    };
    store.set_recording_settings(&chosen).unwrap();
    drop(store);
    let store = db.open();
    assert_eq!(store.recording_settings().unwrap(), chosen);

    let cleared = RecordingSettings {
        input_mode: InputMode::MicAndSystem,
        target_app: None,
        prompt_id: None,
    };
    store.set_recording_settings(&cleared).unwrap();
    assert_eq!(store.recording_settings().unwrap(), cleared);
    assert_eq!(store.get_setting(ACTIVE_PROMPT_ID).unwrap(), None);
}

#[test]
fn reopening_fails_in_flight_sessions_and_keeps_their_audio() {
    let db = TempDb::new();
    let (recording, finalizing, summarizing, done) = {
        let store = db.open();
        let make = |status| {
            let s = store.create_session(&new_session("x")).unwrap();
            store.set_status(&s.id, status, None).unwrap();
            store
                .append_audio_chunk(&s.id, AudioSource::Mic, 0, &[7; 8])
                .unwrap();
            s.id
        };
        (
            make(SessionStatus::Recording),
            make(SessionStatus::Finalizing),
            make(SessionStatus::Summarizing),
            make(SessionStatus::Done),
        )
    };

    let store = db.open();
    for id in [&recording, &finalizing] {
        let s = store.get_session(id).unwrap().unwrap();
        assert_eq!(s.status, SessionStatus::Failed);
        assert_eq!(s.error.as_deref(), Some("interrupted"));
        assert_eq!(store.audio_samples(id, AudioSource::Mic).unwrap(), [7; 8]);
    }
    // Summarizing jobs are re-queued by U10, not failed here.
    let s = store.get_session(&summarizing).unwrap().unwrap();
    assert_eq!(s.status, SessionStatus::Summarizing);
    let s = store.get_session(&done).unwrap().unwrap();
    assert_eq!(s.status, SessionStatus::Done);
    assert_eq!(s.error, None);
}

#[test]
fn ipc_types_serialize_camel_case() {
    let db = TempDb::new();
    let store = db.open();
    let s = store.create_session(&new_session("ipc")).unwrap();
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["status"], "recording");
    assert_eq!(json["inputMode"], "micAndSystem");
    assert!(json.get("createdAt").is_some());
    let item = serde_json::to_value(&store.list_sessions().unwrap()[0]).unwrap();
    assert!(item.get("durationMs").is_some());
}
