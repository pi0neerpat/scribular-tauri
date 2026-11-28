# Spike plan: acoustic echo cancellation for "On a call" mode

Date: 2026-11-27. Type: test spike. No product code is merged, nothing is committed from the spike branch.

## Problem

Without headphones the mic ("You") picks up the selected app's audio ("Client") from the Mac speakers. Each
source is transcribed on its own, so the transcript shows near-duplicate lines at the same timestamp (for
example 1:55 "You" and 1:55 "Client"). The saved mixed WAV does not sound doubled, so the damage is in the
transcript, not the recording.

Question: can we remove speaker bleed from the mic stream before transcription, reliably, in real time, on
macOS, without new risk to capture?

## Recommendation (up front)

1. Spike a software AEC fed by the app-audio stream as the far-end reference. First candidate:
   `sonora` 0.3.0 (BSD-3-Clause, pure-Rust WebRTC AudioProcessing port, AEC3 + NS + AGC). Second:
   `aec3` 0.4.0 (MIT OR BSD-3-Clause, AEC3 port). Reason: we already hold both streams, both are already at
   16 kHz mono, and there is no extra clock domain to manage beyond the offset we already compute.
2. Ship the transcript-level dedup (option C) as the cheap safety net regardless. It needs no audio-path change.
3. Voice Processing I/O (option A) is the fallback only if AEC3 fails. It replaces the cpal mic path, which
   is the riskiest change to capture, so it is not first choice.
4. Apply AEC to the transcription feed only (the `live` channel), not to the stored chunks, at first. Stored
   audio and the mixed WAV stay untouched, which keeps capture risk at zero.

## What the codebase does today (cited)

- Mic is cpal at the device default format, any rate/channels: `src-tauri/src/audio/mic.rs:106-110`
  (`default_input_config`), opened by `CpalMicrophone` `mic.rs:19-60`.
- App audio is the bundled `AudioCapCli` sidecar (CoreAudio process tap) at the app's native rate, e.g. 44.1/48 kHz:
  `audio/system.rs:156` (`open`), `system.rs:163` (`sample_rate: found.sample_rate`).
- Both go through a bounded SPSC ring (`audio/ring.rs:1-5`, callback never blocks, overflow drops whole blocks
  and pads silence) into one writer thread per source: `audio/chunk_writer.rs:59-68` (`chunks-mic`, `chunks-system`).
- The writer resamples to 16 kHz mono (`Resampler`, `audio/resample.rs:34`; `SAMPLE_RATE = 16_000`,
  `stt/mod.rs:19`), stores 5 s i16 chunks (`chunk_writer.rs:22`), and sends `(start_sample, Vec<f32>)` on the
  `live` channel (`chunk_writer.rs:134`). The resample point is `chunk_writer.rs:111`.
- Alignment: the app stream is padded with silence so its first sample lands on the mic's sample clock:
  `Input.align` (`chunk_writer.rs:53-54, 96-108`); the offset is computed in `jobs/session/capture.rs:385-396`
  from `first_block = (mic_samples_received, app_samples)` recorded at the first app block
  (`ring.rs:72-79`). This is a one-time offset with sample-count clocks, not wall-clock; it does not track
  drift and ignores speaker + room latency (tens to ~150 ms of acoustic path plus output buffer).
- Streaming STT is per source: 20 s windows every 15 s with 5 s overlap, `stt/stream.rs:1-17`; the per-source
  `Streamer` is `capture.rs:30`; `take_streamers` (`capture.rs:333`) hands them out. No cross-source logic
  exists, so nothing today compares "You" and "Client" text.
- Mixed WAV is a plain sum of mic and system i16: `audio/wav.rs:9`, called from `commands.rs:1274`.
- No AEC, WebRTC, speex or VPIO code or dependency exists (`Cargo.toml`: cpal `=0.18.2`, objc2 `=0.6.5`).

## Options

| | A. Voice Processing I/O | B. Software AEC (AEC3) | C. Transcript dedup + gating |
|---|---|---|---|
| Quality | Good for system output on the same device; unverified for another app's audio via a process tap | Good with a correct reference; AEC3 handles delay up to a few hundred ms and adapts | Removes duplicate lines, not the echo; misses partial overlaps |
| Latency | ~10-20 ms | 10 ms frames, ~20-40 ms plus convergence (1-3 s) | None on audio; text decided after each window (15 s hop) |
| CPU | Low (system DSP) | Low-moderate, one core a fraction at 16 kHz | Negligible |
| Clock drift | OS handles it internally | Mic and tap use separate clocks; AEC3 tolerates small drift, we must feed the reference in step with mic frames | N/A (uses text timestamps) |
| Integration | New mic backend replacing cpal (`mic.rs`), via AVAudioEngine or raw AudioUnit; `objc2` is in use but no AVFAudio feature is enabled | A cross-thread reference queue plus a processor between `chunk_writer.rs:111` and the `live` send (`:134`) for the mic only | New step after both sources' segments exist; the segment merge point is not yet located (open item) |
| Risks | Changes the input device format and can duck other audio (macOS 14+ has a ducking configuration); may not cancel output from other processes; mic may change channel count or rate; cpal cannot select it | Wrong or drifting delay yields residual echo or clipped near-end speech; double-talk suppression may eat the therapist; C++ build (`webrtc-audio-processing`) adds toolchain risk | Drops real "You" lines that resemble Client lines (a therapist echoing a client); two-sided Whisper errors defeat similarity matching |

### Crates (verified on crates.io, 2026-11-27)

- `sonora` 0.3.0, updated 2026-11-07, BSD-3-Clause, MSRV 1.91, pure Rust. Check our toolchain against 1.91.
- `aec3` 0.4.0, updated 2026-09-16, MIT OR BSD-3-Clause, pure Rust, smaller and less proven.
- `webrtc-audio-processing` 2.1.0 (tonarino), updated 2026-05-13, license listed as "non-standard" (BSD-style
  WebRTC licence; read it before adopting), wraps the C++ library and needs a native build.
- `speexdsp` 0.1.2, MIT, last release 2022-04, bindings to a C lib; Speex AEC is older and weaker than AEC3. Skip unless the rest fail.
- Not checked: whether `sonora` and `aec3` accept 16 kHz mono 10 ms frames and expose a stream-delay setting. Step 2 checks this.

## Spike steps and time-box (total 10 h, hard stop 12 h)

1. (1.5 h) Build the fixture and baseline, no code. See next section. Output: baseline duplicate rate and residual dB.
2. (1 h) Read `sonora` and `aec3` sources and examples: frame size, delay API, reference/render call order, MSRV, build time.
3. (3 h) Offline harness, a throwaway Rust test or bin under `src-tauri/examples/` (not merged): read the recorded mic
   and app WAVs (16 kHz), feed 160-sample frames to the AEC (render = app stream shifted by the measured offset,
   capture = mic), write the cleaned mic WAV. Sweep stream delay (0, 50, 100, 150, 250 ms) and compare. Offline first
   removes real-time and threading as variables.
4. (1.5 h) Run cleaned mic WAV through the existing whisper path (`stt/whisper.rs`) and count duplicate lines.
5. (2 h) Only if steps 3-4 pass: prototype in the live path on a scratch branch. Reference queue from
   `chunks-system` thread to `chunks-mic` thread, AEC on the mic `live` feed only. Measure CPU and xrun/dropped-block counts
   (`ring.rs` drop counters) with and without AEC. Stored chunks unchanged.
6. (1 h) Write the decision note (see below). If step 3 fails on both crates, spend at most 1 h on a VPIO probe
   (AVAudioEngine `inputNode.setVoiceProcessingEnabled(true)` in a tiny Swift script, play `afplay` through speakers,
   record the mic) to check whether it cancels another app's output.

## Fixture and metrics

- Use `tools/uitest/README.md:77-83` and `system-audio-smoke.uitest` as the template. Speakers on, no headphones, laptop
  speakers at a normal volume and a second run at high volume. Quiet room.
- Reference WAV: a fixture with a known transcript (existing uitest fixture, or a 2 min two-voice clip). Play with
  `play "fixture.wav"`, App audio set to `afplay`, mode "On a call". Per CLAUDE.md, assert transcript phrases, not summaries.
- Capture three signals from the same run so they are time-aligned: the mic stream, the app stream, and the transcript. For the
  offline harness, dump both 16 kHz streams from the stored chunks in the session DB (they are aligned by `Input.align`).
- Control run A: fixture playing, therapist silent (pure bleed). Control run B: therapist reading a script aloud over the
  fixture (double-talk) to check near-end speech is kept. Run each three times.

Success criteria (all on the same fixture, speakers at normal volume):

1. Duplicate-line rate: lines in "You" whose text is >= 0.8 similar to a "Client" line within +/- 3 s, divided by "Client" lines.
   Baseline measured in step 1. Target: reduced by >= 90% and <= 5% absolute, in control run A.
2. Residual echo level (ERLE): 10 log10(P_mic_before / P_mic_after) over spans where only the app plays. Target: >= 20 dB
   after a 3 s convergence, >= 12 dB averaged from t = 0.
3. Near-end preservation (run B): word error rate of the "You" script on cleaned mic no more than 3 points worse than on raw mic
   recorded with headphones.
4. Real time: AEC cost < 5% of one core; zero added dropped blocks; added latency to the live feed < 100 ms.
5. No change to stored chunks, the mixed WAV, or any capture path in the control build.

## Decision the spike produces

- Pass (1-4 met): approve a build task for software AEC on the transcription feed behind a setting (default on for
  "On a call", off when headphones are detected is optional, not required), plus the dedup net from option C. Name the crate.
- Partial (duplicates fall but ERLE < 12 dB or double-talk loses words): ship option C only, with mic gating while the app RMS is
  above `SILENCE_RMS`-scale, and record the measured gaps.
- Fail: stop AEC, ship option C, and open a separate spike for VPIO with the cpal replacement cost spelled out.

## Top risks

- Delay model: the existing offset (`capture.rs:385-396`) counts samples, not acoustic path. If speaker-to-mic delay exceeds the AEC's
  search range, or the two clocks drift over a 60 min session, echo returns. Mitigation: the sweep in step 3 plus a 30 min soak run.
- Double-talk: the therapist and client speaking together can cause AEC3 to attenuate the therapist. Measured by run B.
- Reference quality: the tap captures the app's pre-speaker signal, but the OS may apply volume, EQ or Bluetooth latency before the
  speaker, so the reference is not the signal the mic hears. Test with built-in speakers and one Bluetooth speaker.
- Toolchain: `sonora` MSRV 1.91 or a C++ build could break CI. Check before step 3.

## Out of scope

Product code, UI settings, headphone detection, re-transcription of past sessions, noise suppression/AGC, and any change to the
stored audio format.

## Open items to resolve during the spike

- Where per-source segments are merged for display (not located in this pass); needed for option C.
- Whether stored mic chunks are ever re-transcribed; if so, cleaning only the live feed would leave the raw bleed there.
