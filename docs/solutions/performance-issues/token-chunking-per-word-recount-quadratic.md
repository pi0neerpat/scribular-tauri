---
title: Token chunking re-tokenized the growing chunk per word and cost more than the model
date: 2026-10-20
category: performance-issues
module: transcript chunking (src-tauri/src/text/chunk.rs, jobs/summarize.rs)
problem_type: performance_issue
component: service_layer
symptoms:
  - "Warm bench wallMs of about 82.8 s on a 50-minute transcript, against about 36 s of model time in `timings`"
  - The gap sat outside every timed stage, so the model looked slow when it was not
  - Unit and parity tests passed quickly because they use cheap fake counters
root_cause: logic_error
resolution_type: code_fix
severity: high
related_components:
  - testing_framework
tags: [chunking, tokenizer, quadratic, llama-cpp, gemma, binary-search, bench, summarize]
---

# Token chunking re-tokenized the growing chunk per word and cost more than the model

## Problem

`chunk_by_tokens` packed words into chunks by appending one word and counting
the whole candidate chunk again with the real tokenizer. Each count tokenizes
a string as long as the chunk so far, so a transcript costs one count per word
and quadratic tokenizer work overall. On the 50-minute benchmark (about 8,100
words) that was 8,123 counts and 48.4 s, more than the model's own time.

## Symptoms

- The U5 bench (`src-tauri/src/bin/bench.rs`) reported warm `wallMs` of about
  82.8 s while the measured stages added up to about 36 s.
- Nothing in `timings` showed the missing time. Chunking
  (`src-tauri/src/jobs/summarize.rs:189`) runs after the model-load timer
  stops and before any chunk-generation timer starts, so it falls outside
  every timed stage.
- Tests never showed it. The parity fixtures and the fake engine count tokens
  with cheap functions, so the per-word loop stayed fast in tests.

## What Didn't Work

- **Reading the bench at face value.** The unexplained wall time looked like a
  slow model on the target hardware. A throwaway test that timed only
  chunking with the real Gemma tokenizer found the 48.4 s (session history).
  One tokenize of the whole transcript takes about 21 ms, so the count calls
  were the problem, not the tokenizer.
- **Tokenize once and map token offsets back to word boundaries.** This was the
  first fix proposed, and it was dropped. Gemma's SentencePiece vocabulary
  folds the leading space into tokens and falls back to raw byte tokens for
  some characters, so token boundaries do not line up with words or even with
  UTF-8 characters. It would also have replaced the `count_tokens(&str)`
  closure that the tests and the call site depend on (session history).
- **Balanced chunks (plan U2).** Splitting into equal-sized chunks instead of
  greedy packing was measured on the host and was slower: warm median 63.0 s
  and largest `chunkMs` 54.4 s, against 40.4 s and 35.1 s for greedy. It is
  not in the code.
- **Summing `chunkMs` for the gate.** Each entry is the finish time measured
  from the start of one batched generation (`summarize.rs:223`), so entries
  overlap. Summing them overstates model time 2-3x. The batch time is the
  largest entry (session history).

## Solution

The old loop, one count per word:

```rust
for word in words(transcript)? {
    let candidate = if has_words { format!("{current} {word}") } else { word.to_owned() };
    if count_tokens(&candidate) <= budget { current = candidate; has_words = true; continue; }
    // ... start a new chunk with `word`
}
```

The fix (`src-tauri/src/text/chunk.rs:80-122`) keeps the signature and the
call site, and changes only how a chunk boundary is found:

1. Count the whole transcript once. If it fits, return it as one chunk. The
   benchmark's common case then costs one count.
2. Otherwise, for each chunk, check the first word alone (else
   `WordExceedsBudget`). Then double the word count until the joined text
   overflows, and binary-search between the largest count that fit and the
   first that overflowed.
3. Push only a slice that was actually counted and fit.

That is O(log w) counts per chunk instead of one per word. After the fix the
host bench ran warm at 40.4 / 34.5 / 64.3 s (median 40.4 s), 2 chunks per run,
and wall time matched the timed stages within the gate below. The 64.3 s run
is model-time variance, not chunking.

## Why This Works

The search counts real text and never infers a count. Whatever the tokenizer
does at word joins, an accepted chunk is a string the caller's counter said
fits. That is why it stays correct with a counter that is not additive, and
not monotonic either: such a counter can make a chunk shorter than the
largest that fits, never over budget. Chunks are still word slices, so
joining them with `" "` restores the input byte for byte, including empty
words from doubled spaces.

## Prevention

- Bound the counter calls, not just the output. In
  `src-tauri/tests/text_parity.rs:168-186`, a test counts calls and asserts
  `calls <= 1 + chunks * (2 * (log2(words) + 1) + 1)` and `calls < words / 4`.
  The old loop fails it. Output-only parity tests could not have caught this.
- Keep the non-monotonic counter test (`text_parity.rs:188-205`) and the greedy
  test (`text_parity.rs:207-217`), so a faster search cannot buy speed by going
  over budget or by cutting chunks short.
- Treat a wall-time gap in the bench as a bug to find. The gate used here:
  warm `wallMs` within 3 s of `modelLoadMs + max(chunkMs) + combineMs`.
- When a callback wraps a real tokenizer or model, ask how often it is called
  and on how much text. A loop that re-processes a growing prefix is quadratic
  even when each call is fast.

## Related Issues

- Bench binary and llama.cpp linking:
  `docs/solutions/runtime-errors/llama-dylib-rpath-works-under-cargo-only.md`
  (run the bench binary directly).
- The fix is on local `main` and had not been pushed or opened as a PR when
  this was written.
