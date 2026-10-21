//! Callback backpressure preserves the recording timeline with silence.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Arc,
    },
};

use scribular_lib::audio::{chunk_writer, resample::to_i16, Format};
use scribular_lib::store::{AudioSource, InputMode, NewSession, Store};

struct TempDb(PathBuf);

impl TempDb {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "scribular-chunk-writer-{}-{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        );
        let db = Self(std::env::temp_dir().join(name));
        db.remove();
        db
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

const FORMAT: Format = Format {
    sample_rate: 16_000,
    channels: 1,
};

fn session(store: &Store) -> String {
    store
        .create_session(&NewSession {
            title: "Ring buffer".into(),
            input_mode: InputMode::Mic,
            prompt_id: None,
            model_id: None,
        })
        .unwrap()
        .id
}

#[test]
fn a_writer_that_falls_behind_pads_the_dropped_audio_with_silence() {
    let db = TempDb::new();
    let store = Arc::new(Store::open(&db.0).unwrap());
    let id = session(&store);
    let (mut sink, consumer) = chunk_writer::channel_with_capacity(1024);
    assert!(sink.try_push(&[0.5; 1000]));
    assert!(!sink.try_push(&[0.5; 1000]));
    sink.push(&[0.5; 1000]);
    let (live, _audio) = mpsc::channel();
    let writer = chunk_writer::spawn(
        store.clone(),
        id.clone(),
        AudioSource::Mic,
        FORMAT,
        consumer,
        live,
    )
    .unwrap();
    drop(sink);
    assert_eq!(writer.join().unwrap().unwrap(), 2000);
    let samples = store.audio_samples(&id, AudioSource::Mic).unwrap();
    assert_eq!(samples.len(), 2000);
    assert!(samples[..1000].iter().all(|&s| s == to_i16(0.5)));
    assert!(samples[1000..].iter().all(|&s| s == 0));
}

#[test]
fn a_writer_that_keeps_up_loses_nothing() {
    let db = TempDb::new();
    let store = Arc::new(Store::open(&db.0).unwrap());
    let id = session(&store);
    let (mut sink, consumer) = chunk_writer::channel_with_capacity(1024);
    let (live, _audio) = mpsc::channel();
    let writer = chunk_writer::spawn(
        store.clone(),
        id.clone(),
        AudioSource::Mic,
        FORMAT,
        consumer,
        live,
    )
    .unwrap();
    for _ in 0..20 {
        while !sink.try_push(&[0.5; 700]) {
            std::thread::yield_now();
        }
    }
    drop(sink);
    assert_eq!(writer.join().unwrap().unwrap(), 14_000);
    let samples = store.audio_samples(&id, AudioSource::Mic).unwrap();
    assert_eq!(samples.len(), 14_000);
    assert!(samples.iter().all(|&s| s == to_i16(0.5)));
}
