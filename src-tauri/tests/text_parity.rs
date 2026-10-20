//! Byte-for-byte parity between `scribular_lib::text` and the Electron
//! JavaScript it ports. Fixtures are produced from the JS itself by
//! `scripts/gen-text-fixtures.mjs`; regenerate them rather than editing.

use std::path::Path;

use scribular_lib::text::{
    chunk_by_chars, chunk_by_tokens, clean_transcript, merge_diarized_segments, ChunkError,
    Segment, CHUNK_SIZE,
};
use serde::Deserialize;

fn fixture<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/text")
        .join(name);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[derive(Deserialize)]
struct Cases<T> {
    cases: Vec<T>,
}

#[derive(Deserialize)]
struct CleanCase {
    name: String,
    input: String,
    output: String,
}

#[derive(Deserialize)]
struct ChunkFixture {
    #[serde(rename = "chunkSize")]
    chunk_size: usize,
    cases: Vec<ChunkCase>,
}

#[derive(Deserialize)]
struct ChunkCase {
    name: String,
    input: String,
    output: Option<Vec<String>>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct MergeCase {
    name: String,
    client: Vec<Segment>,
    you: Vec<Segment>,
    output: String,
}

#[test]
fn text_parity_clean_transcript() {
    let fixture: Cases<CleanCase> = fixture("clean.json");
    assert!(fixture.cases.len() > 400, "fixture lost its fuzz cases");
    let failures: Vec<_> = fixture
        .cases
        .iter()
        .filter(|c| clean_transcript(&c.input) != c.output)
        .map(|c| {
            format!(
                "{}: {:?} -> {:?}, want {:?}",
                c.name,
                c.input,
                clean_transcript(&c.input),
                c.output
            )
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn text_parity_merge_diarized_segments() {
    let fixture: Cases<MergeCase> = fixture("merge.json");
    for case in &fixture.cases {
        assert_eq!(
            merge_diarized_segments(&case.client, &case.you),
            case.output,
            "{}",
            case.name
        );
    }
}

#[test]
fn text_parity_single_source_merge_has_no_empty_turns() {
    let fixture: Cases<MergeCase> = fixture("merge.json");
    for case in fixture
        .cases
        .iter()
        .filter(|c| c.name.starts_with("single source"))
    {
        let merged = merge_diarized_segments(&case.client, &case.you);
        assert_eq!(merged.lines().count(), 1, "{}", case.name);
        assert!(merged.lines().all(|l| !l.ends_with(": ")), "{}", case.name);
    }
}

#[test]
fn text_parity_chunk_by_chars() {
    let fixture: ChunkFixture = fixture("chunk.json");
    assert_eq!(fixture.chunk_size, CHUNK_SIZE);
    for case in &fixture.cases {
        match (chunk_by_chars(&case.input), &case.output, &case.error) {
            (Ok(chunks), Some(want), None) => assert_eq!(&chunks, want, "{}", case.name),
            (Err(err), None, Some(want)) => assert_eq!(&err.to_string(), want, "{}", case.name),
            (got, _, _) => panic!(
                "{}: got {got:?}, fixture {:?} / {:?}",
                case.name, case.output, case.error
            ),
        }
    }
}

/// Stand-in tokenizer: one token per 4 UTF-8 bytes of each word, plus one per
/// word boundary, so it is non-additive like a real BPE count.
fn fake_count(s: &str) -> usize {
    s.split(' ')
        .map(|w| w.len().div_ceil(4).max(1))
        .sum::<usize>()
        + s.matches(' ').count() / 2
}

#[test]
fn text_parity_token_chunks_respect_budget_and_words() {
    let fixture: ChunkFixture = fixture("chunk.json");
    let input = &fixture
        .cases
        .iter()
        .find(|c| c.name == "benchmark transcript cleaned")
        .unwrap()
        .input;
    for budget in [64, 256, 1024] {
        let chunks = chunk_by_tokens(input, budget, fake_count).unwrap();
        assert!(
            chunks.len() > 1,
            "budget {budget} should split the benchmark transcript"
        );
        for chunk in &chunks {
            assert!(fake_count(chunk) <= budget, "chunk over budget {budget}");
            assert!(!chunk.is_empty());
        }
        // Rejoining restores the input, so no word was split or dropped.
        assert_eq!(&chunks.join(" "), input);
    }
}

fn benchmark_transcript() -> String {
    let fixture: ChunkFixture = fixture("chunk.json");
    fixture
        .cases
        .into_iter()
        .find(|c| c.name == "benchmark transcript cleaned")
        .unwrap()
        .input
}

#[test]
fn text_parity_token_chunks_count_logarithmically_per_chunk() {
    let input = benchmark_transcript();
    let words = input.split(' ').count();
    let mut calls = 0;
    let chunks = chunk_by_tokens(&input, 256, |s| {
        calls += 1;
        fake_count(s)
    })
    .unwrap();
    // One whole-transcript count, then a doubling and a binary search per chunk.
    let per_chunk = 2 * (words.ilog2() as usize + 1) + 1;
    assert!(
        calls <= 1 + chunks.len() * per_chunk,
        "{calls} counts for {} chunks of {words} words",
        chunks.len()
    );
    assert!(calls < words / 4, "{calls} counts for {words} words");
}

#[test]
fn text_parity_token_chunks_stay_in_budget_with_non_monotonic_counter() {
    let input = benchmark_transcript();
    let budget = 256;
    // Text of 3n words overflows, so adding a word can lower the count.
    let penalized = std::cell::Cell::new(false);
    let count = |s: &str| {
        let hit = s.split(' ').count().is_multiple_of(3);
        penalized.set(penalized.get() || hit);
        fake_count(s) + if hit { budget } else { 0 }
    };
    let chunks = chunk_by_tokens(&input, budget, count).unwrap();
    assert!(penalized.get(), "the search never counted a penalized text");
    for chunk in &chunks {
        assert!(count(chunk) <= budget, "chunk over budget {budget}");
    }
    assert_eq!(chunks.join(" "), input);
}

#[test]
fn text_parity_token_chunks_count_once_when_transcript_fits() {
    let input = "a transcript of twelve short words that fits one chunk easily";
    let mut calls = 0;
    let chunks = chunk_by_tokens(input, 1024, |s| {
        calls += 1;
        fake_count(s)
    })
    .unwrap();
    assert_eq!(chunks, [input]);
    assert_eq!(calls, 1);
}

#[test]
fn text_parity_token_chunks_restore_doubled_spaces() {
    let input = "a  b c  d e f  g h i j k l  m n o";
    let chunks = chunk_by_tokens(input, 4, fake_count).unwrap();
    assert!(chunks.len() > 1);
    for chunk in &chunks {
        assert!(fake_count(chunk) <= 4, "chunk over budget 4");
    }
    assert_eq!(chunks.join(" "), input);
}

#[test]
fn text_parity_token_chunks_reject_short_input_like_js() {
    let mut calls = 0;
    let err = chunk_by_tokens(
        "only nine words in this short little transcript here",
        512,
        |s| {
            calls += 1;
            fake_count(s)
        },
    );
    assert_eq!(calls, 0);
    assert_eq!(err, Err(ChunkError::InsufficientAudio));
    assert_eq!(
        err.unwrap_err().to_string(),
        chunk_by_chars("only nine words").unwrap_err().to_string()
    );
}

#[test]
fn text_parity_token_chunks_reject_oversized_word() {
    let input = format!("{} a b c d e f g h i", "x".repeat(400));
    assert_eq!(
        chunk_by_tokens(&input, 50, fake_count),
        Err(ChunkError::WordExceedsBudget {
            tokens: 100,
            budget: 50
        })
    );
}

#[test]
fn text_parity_token_chunks_reject_oversized_middle_word() {
    let input = format!("a b c d e {} f g h i", "x".repeat(400));
    assert_eq!(
        chunk_by_tokens(&input, 50, fake_count),
        Err(ChunkError::WordExceedsBudget {
            tokens: 100,
            budget: 50
        })
    );
}
