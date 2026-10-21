# U9 M5b Check: Live Transcription (R16)

- **Result:** Pending. This needs a real call on the developer's Mac and was not run in the build sandbox.
- **Build:** the U9 commit, `feat: stream live transcript while recording (U9)`.

## What the automated tests cover

- `src-tauri/tests/stream_dedup.rs`:
  - 20 s windows cut every 15 s cover all the audio, including the tail.
  - Overlap segments at or before the last kept end are dropped.
  - A segment cut off at a window's end is taken whole from the next window, so its words are not lost.
  - Silent windows are skipped.
  - Catching up means more than 2 windows are queued.
- `src-tauri/tests/session_job.rs`, with the fake engines:
  - A slow transcriber shows "Catching up...", and no audio is dropped.
  - Stop drains the queue while the summary model loads.
  - Cancel during Finalizing stops a window that is still transcribing.
  - Segments are saved and emitted in recording time.
  - Whisper loads once at the start of a recording and is unloaded after the transcript when `unload_whisper` is set.
- `src/lib/transcript.test.ts`: the live view merges `Client:`/`You:` turns like the final transcript, and drops tags like `[BLANK_AUDIO]` from both views.

## Smoke check (2026-10-20)

On a recording longer than 20 s, the live transcript appeared while recording, and Stop gave the full transcript. Live text starts only once the first window, cut at 15 s, has been transcribed, so how short a recording can be and show none depends on transcription speed. Recordings under 20 s showed none here. The timed criteria below have not been measured yet.

## Manual check (to run)

1. Run `npm run tauri dev`. In Settings, choose Mic + app audio and FaceTime.
2. Record a 5-minute FaceTime call with both sides talking. Watch Little Snitch or `nettop -p scribular` for the whole run.
3. While the call runs, note how far the live transcript lags the speech.
4. At Stop, time until the first "Generating summary..." and until "Session ready.".
5. Record the same call audio as a 16 kHz mono WAV, run it through `transcribe_file`, and compare the word count with the live transcript's.

| Criterion | Target | Result |
|---|---|---|
| Live transcript lag | ≤ ~20 s | |
| Summary starts after Stop | ≤ 10 s | |
| Word count against batch | within 5% | |
| Network traffic | none | |
| Machine | | |

## Known limits

- A summary that runs right after a preload records `model_load_ms` as 0, because the load happened during Finalizing.
- Retrying a failed session's transcription would add its saved segments again. No retry path exists yet.
- Live segments from before a window reload are not shown again; the final transcript has them.
