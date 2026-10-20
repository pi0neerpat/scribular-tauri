# U6 Kill Gate: Summarization Bench

- **Result:** Pass. Warm median 40.4 s against the 50 s gate (KTD12).
- **Acknowledged by the developer:** 2026-10-20.
- **Hardware:** M2, 24 GB. This replaces the 8 GB M2/M3 the plan first required. See the amended Key Decision in `plans/2026-10-19-2001-feat-tauri-rewrite-evidence-prototype-plan.md`.
- **Build:** `cargo build --release --bin bench` at commit `430d4cf` (fix: find token chunk boundaries in O(log w) counts).
- **Input:** `2-speakers-50min.json`, the transcription output for the KTD12 recording.
- **Command:** `target/release/bench <2-speakers-50min.json> --runs 4`

## Runs

| Run | wallMs | Chunks |
|---|---|---|
| Cold | not recorded | 2 |
| Warm 1 | 40.4 s | 2 |
| Warm 2 | 34.5 s | 2 |
| Warm 3 | 64.3 s | 2 |
| **Warm median** | **40.4 s** | |

## Time per stage (median warm run)

| Stage | Time |
|---|---|
| Load | ~0 s |
| Chunk summaries, max(chunkMs) | 35.1 s |
| Combine | 5.1 s |
| Sum | 40.15 s against a 40.41 s wall |

Chunking and model loading together take about 0.25 s. Before the chunking fix, the U5 bench's warm median was 82.8 s.

## Caveats

- Warm 3 (64.3 s) went over the gate on its own. The gate is on the median, so it passes, but the spread is wide and its cause was not investigated.
- Peak memory was not recorded. With 24 GB it says nothing about 8 GB Macs, and KTD5's Whisper unload was not exercised.
- The side-by-side read of the DORA summary against Electron's output has not been done yet.
