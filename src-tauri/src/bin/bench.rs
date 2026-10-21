//! Headless summarization bench (U5, KTD16): runs a transcript through the
//! same job queue and llama engine as the app and prints one JSON line of
//! stage timings per run. The first run loads the model (cold); later runs
//! reuse it, as the app's worker does (warm).
//!
//! ```text
//! bench <transcript.json|.txt> [prompt-id] [--runs N] [--models DIR] [--download]
//! ```
//!
//! A `.json` transcript is an array of `{start_ms, end_ms, text}` segments
//! (Electron's `benchmark/results/batch/*/transcription-output.json`) and goes
//! through the U3 pipeline; any other file is used as the transcript as is.

#[cfg(feature = "fake-engines")]
fn main() {
    eprintln!("bench needs the real engines; build without --features fake-engines");
    std::process::exit(2);
}

#[cfg(not(feature = "fake-engines"))]
fn main() {
    if let Err(e) = real::main() {
        eprintln!("bench: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "fake-engines"))]
mod real {
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
        time::{Duration, Instant},
    };

    use scribular_lib::jobs::{JobEvents, JobQueue, SessionProgress, SessionSegments, SummaryDelta};
    use scribular_lib::llm::llama::LlamaLoader;
    use scribular_lib::models::{self, ModelManager, ModelState, GEMMA_2_2B};
    use scribular_lib::store::{InputMode, NewSession, SessionStatus, Store, Timings};
    use scribular_lib::text::{clean_transcript, merge_diarized_segments, Segment};

    type Error = Box<dyn std::error::Error>;

    const DEFAULT_PROMPT: &str = "default-therapy-dora";
    /// The app's data directory (`identifier` in tauri.conf.json).
    const APP_DIR: &str = "Library/Application Support/com.app.scribular.tauri";
    const RUN_TIMEOUT: Duration = Duration::from_secs(60 * 60);

    struct Args {
        transcript: PathBuf,
        prompt_id: String,
        runs: usize,
        models: PathBuf,
        download: bool,
    }

    fn args() -> Result<Args, Error> {
        let mut it = std::env::args().skip(1);
        let mut positional = Vec::new();
        let mut runs = 1;
        let mut models = None;
        let mut download = false;
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--runs" => runs = it.next().ok_or("--runs needs a number")?.parse()?,
                "--models" => {
                    models = Some(PathBuf::from(it.next().ok_or("--models needs a dir")?))
                }
                "--download" => download = true,
                _ => positional.push(arg),
            }
        }
        let mut positional = positional.into_iter();
        let transcript = positional.next().ok_or(
            "usage: bench <transcript.json|.txt> [prompt-id] [--runs N] [--models DIR] [--download]",
        )?;
        let models = match models {
            Some(dir) => dir,
            None => PathBuf::from(std::env::var("HOME")?)
                .join(APP_DIR)
                .join("models"),
        };
        Ok(Args {
            transcript: transcript.into(),
            prompt_id: positional.next().unwrap_or_else(|| DEFAULT_PROMPT.into()),
            runs: runs.max(1),
            models,
            download,
        })
    }

    /// A segment array goes through merge and clean as one source would.
    fn load_transcript(path: &Path) -> Result<String, Error> {
        let raw = std::fs::read_to_string(path)?;
        if path.extension().is_some_and(|e| e == "json") {
            let values: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
            let segments = values
                .iter()
                .map(|v| Segment {
                    start_ms: v["start_ms"].as_i64().unwrap_or(0),
                    end_ms: v["end_ms"].as_i64().unwrap_or(0),
                    text: v["text"].as_str().map(str::to_owned),
                })
                .collect::<Vec<_>>();
            Ok(clean_transcript(&merge_diarized_segments(&segments, &[])))
        } else {
            Ok(raw)
        }
    }

    /// Progress goes to stderr so stdout stays JSON.
    struct Log;

    impl JobEvents for Log {
        fn progress(&self, p: SessionProgress) {
            eprintln!(
                "[{}/{}] {:?}: {}",
                p.step_index, p.step_count, p.state, p.message
            );
        }

        fn summary_delta(&self, _: SummaryDelta) {}

        fn segments(&self, _: SessionSegments) {}
    }

    /// Peak resident memory of this process so far, in bytes.
    fn peak_rss() -> u64 {
        #[repr(C)]
        struct Rusage {
            utime: [i64; 2],
            stime: [i64; 2],
            maxrss: i64,
            rest: [i64; 13],
        }
        extern "C" {
            fn getrusage(who: i32, usage: *mut Rusage) -> i32;
        }
        let mut usage = std::mem::MaybeUninit::<Rusage>::zeroed();
        // SAFETY: RUSAGE_SELF (0) with a buffer laid out as Darwin's
        // `struct rusage` (two timevals, then fourteen longs).
        if unsafe { getrusage(0, usage.as_mut_ptr()) } != 0 {
            return 0;
        }
        // SAFETY: zeroed, then filled in by getrusage.
        let maxrss = unsafe { usage.assume_init() }.maxrss;
        // Darwin reports bytes; Linux reports kilobytes.
        let scale = if cfg!(target_os = "macos") { 1 } else { 1024 };
        u64::try_from(maxrss).unwrap_or(0) * scale
    }

    pub fn main() -> Result<(), Error> {
        let args = args()?;
        let transcript = load_transcript(&args.transcript)?;

        let models = Arc::new(ModelManager::new(args.models.clone(), models::manifest()));
        models.scan();
        if models.readiness.get(GEMMA_2_2B) != ModelState::Ready {
            if !args.download {
                return Err(format!(
                    "{GEMMA_2_2B} isn't in {}; open the app once or pass --download",
                    args.models.display()
                )
                .into());
            }
            eprintln!("downloading models to {}", args.models.display());
            tauri::async_runtime::block_on(models.ensure_all(&|_| {}));
            if models.readiness.get(GEMMA_2_2B) != ModelState::Ready {
                return Err(format!("{GEMMA_2_2B} download failed").into());
            }
        }

        let db = std::env::temp_dir().join(format!("scribular-bench-{}.db", std::process::id()));
        let store = Arc::new(Store::open(&db)?);
        if store.get_prompt(&args.prompt_id)?.is_none() {
            return Err(format!("unknown prompt {}", args.prompt_id).into());
        }
        let queue = JobQueue::new(
            Arc::clone(&store),
            Arc::new(LlamaLoader::new(Arc::clone(&models))),
            Arc::new(Log),
        );

        let words = transcript.split_whitespace().count();
        let mut result = Ok(());
        for run in 0..args.runs {
            let session = store.create_session(&NewSession {
                title: format!("bench {run}"),
                input_mode: InputMode::Mic,
                prompt_id: Some(args.prompt_id.clone()),
                model_id: None,
            })?;
            store.save_transcript(&session.id, &transcript, 0)?;
            store.set_status(&session.id, SessionStatus::Done, None)?;

            let started = Instant::now();
            queue.regenerate(&session.id, &args.prompt_id)?;
            if !queue.wait_idle(RUN_TIMEOUT) {
                result = Err("run timed out".into());
                break;
            }
            let wall_ms = started.elapsed().as_millis();
            let done = store.get_session(&session.id)?.ok_or("session vanished")?;
            if done.status != SessionStatus::Done {
                result = Err(format!("run {run} ended {:?}: {:?}", done.status, done.error).into());
                break;
            }
            let timings = done.timings.unwrap_or_else(Timings::default);
            println!(
                "{}",
                serde_json::json!({
                    "run": run,
                    "kind": if run == 0 { "cold" } else { "warm" },
                    "promptId": args.prompt_id,
                    "transcriptWords": words,
                    "wallMs": wall_ms,
                    "peakRssBytes": peak_rss(),
                    "timings": timings,
                    "summaryChars": done.summary.chars().count(),
                })
            );
            if run == args.runs - 1 {
                eprintln!("\n--- summary (run {run}) ---\n{}", done.summary);
            }
        }

        drop(queue);
        drop(store);
        for suffix in ["", "-wal", "-shm"] {
            let mut p = db.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
        result
    }
}
