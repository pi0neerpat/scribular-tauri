//! U5 against the real gemma-2-2b model (`cargo test -- --ignored`). The
//! model is read from `$SCRIBULAR_MODELS`, or the app's data directory once
//! the app (or `bench --download`) has fetched it.
#![cfg(not(feature = "fake-engines"))]

use std::{path::PathBuf, sync::Arc, time::Duration};

use scribular_lib::jobs::{JobEvents, JobQueue, SessionProgress, SummaryDelta};
use scribular_lib::llm::llama::{LlamaLoader, LlamaSummarizer};
use scribular_lib::llm::prompt::{chunk_budget, chunk_prompt};
use scribular_lib::llm::Summarizer;
use scribular_lib::models::{self, ModelManager, ModelState, GEMMA_2_2B};
use scribular_lib::store::{InputMode, NewSession, SessionStatus, Store};
use scribular_lib::text::{chunk_by_tokens, clean_transcript, merge_diarized_segments, Segment};

const BENCH_50_MIN: &str = "tests/fixtures/transcripts/2-speakers-50min.json";
const DORA: &str = "default-therapy-dora";

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
        models.readiness.get(GEMMA_2_2B),
        ModelState::Ready,
        "{GEMMA_2_2B} isn't in {}; run `cargo run --bin bench -- <file> --download` first",
        dir.display()
    );
    models
}

/// The Electron benchmark segments up to `until_ms`, through the U3 pipeline.
fn transcript(until_ms: i64) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(BENCH_50_MIN);
    let values: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let segments: Vec<Segment> = values
        .iter()
        .map(|v| Segment {
            start_ms: v["start_ms"].as_i64().unwrap(),
            end_ms: v["end_ms"].as_i64().unwrap(),
            text: v["text"].as_str().map(str::to_owned),
        })
        .filter(|s| s.start_ms < until_ms)
        .collect();
    clean_transcript(&merge_diarized_segments(&segments, &[]))
}

struct Quiet;

impl JobEvents for Quiet {
    fn progress(&self, _: SessionProgress) {}
    fn summary_delta(&self, _: SummaryDelta) {}
}

/// KTD3 estimates "about 3 chunks instead of about 14"; gemma's tokenizer
/// gives 2 for this transcript, so the test holds the ceiling and the budget.
#[test]
#[ignore = "needs the gemma-2-2b model"]
fn fifty_minute_benchmark_splits_into_at_most_three_chunks() {
    let models = models();
    let llm = LlamaSummarizer::load(&models.path(GEMMA_2_2B).unwrap()).unwrap();
    let store = Store::open(
        &std::env::temp_dir().join(format!("scribular-llama-chunks-{}.db", std::process::id())),
    )
    .unwrap();
    let base = store.get_prompt(DORA).unwrap().unwrap().text;

    let budget = chunk_budget(
        llm.context_tokens(),
        llm.prompt_tokens(&chunk_prompt(&base, "")).unwrap(),
    )
    .unwrap();
    let chunks = chunk_by_tokens(&transcript(i64::MAX), budget, |t| {
        llm.count_tokens(t).unwrap()
    })
    .unwrap();
    let sizes: Vec<usize> = chunks
        .iter()
        .map(|c| llm.count_tokens(c).unwrap())
        .collect();
    assert!(
        (2..=3).contains(&chunks.len()),
        "{} chunks of {sizes:?} tokens, budget {budget}",
        chunks.len()
    );
    assert!(
        sizes.iter().all(|&n| n <= budget),
        "{sizes:?} over budget {budget}"
    );
}

#[test]
#[ignore = "needs the gemma-2-2b model"]
fn every_prompt_summarizes_thirty_seconds() {
    let models = models();
    let db =
        std::env::temp_dir().join(format!("scribular-llama-prompts-{}.db", std::process::id()));
    let store = Arc::new(Store::open(&db).unwrap());
    let queue = JobQueue::new(
        Arc::clone(&store),
        Arc::new(LlamaLoader::new(models)),
        Arc::new(Quiet),
    );
    let text = transcript(30_000);
    let prompts = store.list_prompts().unwrap();
    assert_eq!(prompts.len(), 7);

    for prompt in prompts {
        let session = store
            .create_session(&NewSession {
                title: prompt.id.clone(),
                input_mode: InputMode::Mic,
                prompt_id: None,
                model_id: None,
            })
            .unwrap();
        store.save_transcript(&session.id, &text, 30_000).unwrap();
        store
            .set_status(&session.id, SessionStatus::Done, None)
            .unwrap();
        queue.regenerate(&session.id, &prompt.id).unwrap();
        assert!(queue.wait_idle(Duration::from_secs(600)));

        let done = store.get_session(&session.id).unwrap().unwrap();
        assert_eq!(
            done.status,
            SessionStatus::Done,
            "{}: {:?}",
            prompt.id,
            done.error
        );
        assert!(
            !done.summary.trim().is_empty(),
            "{} gave no summary",
            prompt.id
        );
        assert_eq!(done.model_id.as_deref(), Some(GEMMA_2_2B));
    }
}
