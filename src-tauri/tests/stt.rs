//! U7: in-process transcription. The whisper tests need the whisper-base
//! model (`cargo test -- --ignored`), read from `$SCRIBULAR_MODELS` or the
//! app's data directory; the fake runs under `--features fake-engines`.

use std::path::PathBuf;

use scribular_lib::stt::{read_wav, SAMPLE_RATE};

/// The first 12 s of the Electron 2-speakers benchmark audio.
const FIXTURE: &str = "tests/fixtures/audio/2-speakers-12s.wav";

fn fixture() -> Vec<f32> {
    read_wav(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)).unwrap()
}

#[test]
fn fixture_reads_as_16khz_mono_samples() {
    let samples = fixture();
    assert_eq!(samples.len(), 12 * SAMPLE_RATE as usize);
    assert!(samples.iter().all(|s| (-1.0..=1.0).contains(s)));
    assert!(samples.iter().any(|&s| s != 0.0));
}

#[cfg(feature = "fake-engines")]
mod fake {
    use std::sync::atomic::AtomicBool;

    use scribular_lib::engines::fake::FakeTranscriber;
    use scribular_lib::stt::{SttError, Transcriber, TranscriptSegment};

    fn scripted() -> Vec<TranscriptSegment> {
        vec![
            TranscriptSegment {
                start_ms: 0,
                end_ms: 4980,
                text: "Thanks for coming in today.".into(),
            },
            TranscriptSegment {
                start_ms: 4980,
                end_ms: 11920,
                text: "Um, well, I don't know.".into(),
            },
        ]
    }

    #[test]
    fn fake_transcriber_returns_its_scripted_segments() {
        let stt = FakeTranscriber::new(scripted());
        let segments = stt.transcribe(&super::fixture(), &AtomicBool::new(false));
        assert_eq!(segments, Ok(scripted()));
        assert_eq!(stt.calls(), vec![12 * 16_000]);
    }

    #[test]
    fn fake_transcriber_stops_once_cancelled() {
        let stt = FakeTranscriber::new(scripted());
        let segments = stt.transcribe(&super::fixture(), &AtomicBool::new(true));
        assert_eq!(segments, Err(SttError::Cancelled));
    }
}

#[cfg(not(feature = "fake-engines"))]
mod whisper {
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };

    use scribular_lib::models::{self, ModelManager, ModelState, WHISPER_BASE};
    use scribular_lib::stt::whisper::WhisperLoader;
    use scribular_lib::stt::{SttError, TranscriberLoader};

    fn models() -> Arc<ModelManager> {
        let dir = std::env::var_os("SCRIBULAR_MODELS")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var("HOME").unwrap())
                    .join("Library/Application Support/com.app.scribular.tauri/models")
            });
        let models = Arc::new(ModelManager::new(dir.clone(), models::manifest()));
        models.scan();
        assert_eq!(
            models.readiness.get(WHISPER_BASE),
            ModelState::Ready,
            "{WHISPER_BASE} isn't in {}; run `cargo run --bin bench -- <file> --download` first",
            dir.display()
        );
        models
    }

    #[test]
    fn loading_before_the_model_is_ready_is_not_ready() {
        let dir = std::env::temp_dir().join(format!("scribular-stt-empty-{}", std::process::id()));
        let models = Arc::new(ModelManager::new(dir, models::manifest()));
        models.scan();
        assert_eq!(
            WhisperLoader::new(models).load().err(),
            Some(SttError::NotReady)
        );
    }

    #[test]
    #[ignore = "needs the whisper-base model"]
    fn fixture_transcribes_with_ordered_timestamps() {
        let stt = WhisperLoader::new(models()).load().unwrap();
        let segments = stt
            .transcribe(&super::fixture(), &AtomicBool::new(false))
            .unwrap();

        assert!(!segments.is_empty());
        let mut prev_end = 0;
        for s in &segments {
            assert!(s.start_ms >= prev_end, "{s:?} starts before {prev_end}");
            assert!(s.end_ms >= s.start_ms, "{s:?} ends before it starts");
            prev_end = s.end_ms;
        }
        assert!(prev_end <= 12_000, "last segment ends at {prev_end} ms");
        let text = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<String>()
            .to_lowercase();
        assert!(text.contains("coming in today"), "{text}");
    }

    #[test]
    #[ignore = "needs the whisper-base model"]
    fn cancelling_partway_returns_within_a_second() {
        let stt = WhisperLoader::new(models()).load().unwrap();
        // Five minutes of audio, so the run is still going when the token is set.
        let samples = super::fixture().repeat(25);
        let cancel = Arc::new(AtomicBool::new(false));
        let setter = {
            let cancel = Arc::clone(&cancel);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(500));
                cancel.store(true, Ordering::Relaxed);
                Instant::now()
            })
        };

        let result = stt.transcribe(&samples, &cancel);
        let returned = Instant::now();
        let cancelled_at = setter.join().unwrap();

        assert_eq!(result.err(), Some(SttError::Cancelled));
        let latency = returned.saturating_duration_since(cancelled_at);
        assert!(latency < Duration::from_secs(1), "returned {latency:?} after cancel");
    }
}
