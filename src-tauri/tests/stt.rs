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

/// A RIFF/WAVE file holding `chunks` as (id, body) pairs, padded per spec.
fn riff(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut body = b"WAVE".to_vec();
    for (id, data) in chunks {
        body.extend_from_slice(*id);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&(body.len() as u32).to_le_bytes());
    file.extend(body);
    file
}

fn fmt(channels: u16, rate: u32, bits: u16) -> Vec<u8> {
    let block = channels * bits / 8;
    [
        &1u16.to_le_bytes()[..],
        &channels.to_le_bytes(),
        &rate.to_le_bytes(),
        &(rate * u32::from(block)).to_le_bytes(),
        &block.to_le_bytes(),
        &bits.to_le_bytes(),
    ]
    .concat()
}

fn read_bytes(name: &str, bytes: &[u8]) -> std::io::Result<Vec<f32>> {
    let path =
        std::env::temp_dir().join(format!("scribular-wav-{}-{name}.wav", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let result = read_wav(&path);
    std::fs::remove_file(&path).unwrap();
    result
}

#[test]
fn wav_skips_odd_sized_chunks_before_data() {
    let samples = [0i16, 16384, -32768];
    let data = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let bytes = riff(&[
        (b"fmt ", fmt(1, SAMPLE_RATE, 16)),
        (b"LIST", vec![1, 2, 3]),
        (b"data", data),
    ]);
    assert_eq!(read_bytes("padded", &bytes).unwrap(), vec![0.0, 0.5, -1.0]);
}

#[test]
fn wav_rejects_unsupported_or_malformed_files() {
    let data = (b"data", vec![0u8; 4]);
    let mut truncated = riff(&[(b"fmt ", fmt(1, SAMPLE_RATE, 16)), data.clone()]);
    truncated.truncate(truncated.len() - 2);
    let cases = [
        ("not-riff", b"not a wav file at all".to_vec()),
        ("8khz", riff(&[(b"fmt ", fmt(1, 8_000, 16)), data.clone()])),
        (
            "stereo",
            riff(&[(b"fmt ", fmt(2, SAMPLE_RATE, 16)), data.clone()]),
        ),
        (
            "8bit",
            riff(&[(b"fmt ", fmt(1, SAMPLE_RATE, 8)), data.clone()]),
        ),
        (
            "data-first",
            riff(&[data.clone(), (b"fmt ", fmt(1, SAMPLE_RATE, 16))]),
        ),
        ("no-data", riff(&[(b"fmt ", fmt(1, SAMPLE_RATE, 16))])),
        ("truncated", truncated),
    ];
    for (name, bytes) in cases {
        let err = read_bytes(name, &bytes).expect_err(name);
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{name}: {err}");
    }
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
        assert!(
            latency < Duration::from_secs(1),
            "returned {latency:?} after cancel"
        );
    }
}
