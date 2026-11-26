# Verify sessions use the host llama-server (plan)

Date: 2026-11-26. Status: plan only, nothing implemented.

## Problem

cwf verify sessions run in the Tart VM. They build and launch Scribular with the commands in
`CLAUDE.md:3-4`, which have no `--features remote-llm` and no `SCRIBULAR_LLM_URL`. The optional
note at `CLAUDE.md:12` is not followed: the feat-recording-p3 build has no `SCRIBULAR_LLM_URL` string
and ran Gemma on the VM CPU (1.8 GB footprint, slow summaries).

## Where the commands come from

- Build and run commands: project `CLAUDE.md:3-4`. The uitest doctor fix string at
  `tools/uitest/Sources/uitest/main.swift:234` repeats the build command.
- The coding-task skill (`~/Projects/clauffice/workflows/coding/skills/coding-task/SKILL.md:124-171`)
  says "use uitest" and "find the built .app". It has no Scribular build flags. cwf has no per-task env
  injection. `watcher-config.json` has only `preflight` (`lib/cwf/preflight.py:13`), which runs a shell
  command in tmux before verify starts.
- uitest forwards `SCRIBULAR_LLM_URL` from its own environment to the app at
  `tools/uitest/Sources/uitest/AppLifecycle.swift:60-63`.

Conclusion: the fix belongs in the repo, in the debug overlay and in uitest. Every verify session
already uses both, on every branch that merges main. Prompts and per-task config stay unchanged.

## Changes

### 1. Feature flag through the debug overlay (no command change)

`tools/uitest/tauri-debug-overlay.json` (whole file, 4 lines): add `"features": ["remote-llm"]` under
`build`. Tauri v2 passes `build.features` to cargo (`node_modules/@tauri-apps/cli/config.schema.json`,
BuildConfig.features).

```json
{
  "build": {
    "beforeBuildCommand": "NODE_ENV=development npm run build -- --mode development",
    "features": ["remote-llm"]
  },
  "bundle": { "createUpdaterArtifacts": false }
}
```

This is safe on the host and in shipping builds:
- The overlay is used only for uitest debug builds. Release builds never load it.
- With `SCRIBULAR_LLM_URL` unset, the feature does nothing: the app uses `LlamaLoader`
  (`src-tauri/src/lib.rs:64-71`).

### 2. uitest sets the URL itself, only in a VM

`tools/uitest/Sources/uitest/AppLifecycle.swift:60-63`: replace the forward-only block with
`resolveLLMURL()`, a pure function added to `UITestCore`, for example
`Sources/UITestCore/HostLLM.swift`:

- `SCRIBULAR_LLM_URL` is present in the environment, even when empty: use it as given. Empty means
  on-device. This is the explicit override, and it needs no new flag.
- Otherwise, if `sysctl kern.hv_vmm_present` is `1` (VM; it is `0` on the host): GET
  `http://192.168.64.1:5801/health` with a 2 s timeout.
  - `200` with `"ok"`: use `http://192.168.64.1:5801` and print
    `info host llama-server: http://192.168.64.1:5801`.
  - Otherwise: print
    `WARN host llama-server unreachable at 192.168.64.1:5801; summaries run on the VM CPU (see README "Host llama-server (VM)")`
    and fall back to on-device.
- Not a VM: on-device. Host runs behave exactly as they do today.

Inject the probes (`isVM`, `healthOK`) so `Tests/UITestCoreTests/HostLLMTests.swift` covers the four
cases without a network: env set, env empty, VM up, VM down.

shortcut: the host address and port are hard-coded to the Softnet bridge `192.168.64.1:5801`. Read
them from the env if a second VM network is ever used.

### 3. Assert the log line, so a missing feature flag fails the run

`tools/uitest/Sources/uitest/main.swift:183` copies `scribular.log` to `<runDir>/app.log` after the
run. After that copy:
- If `resolveLLMURL()` returned a URL and `app.log` lacks `SCRIBULAR_LLM_URL set: summaries use`, fail
  the run (exit 1) with
  `app ignored SCRIBULAR_LLM_URL: build lacks --features remote-llm (tools/uitest/tauri-debug-overlay.json)`.

This check catches a regression in the overlay, in `lib.rs:64-69`, or in the forwarding. It runs on
every remote-URL run, so no scenario needs to change. The log line is written at setup
(`lib.rs:411`), so it is in the log after any launch.

### 4. Doctor and preflight: report, don't block

`tools/uitest/Sources/UITestCore/Doctor.swift:86-88`: in a VM, add an info line, not a check, with
the result of `resolveLLMURL()`: `info host llama-server: ok <url>` or
`info host llama-server: unreachable (summaries on VM CPU)`.

It is an info line so that a host server that is down does not hold verify. The preflight output in
`logs/preflight.log` and the uitest WARN line show the state. Add a `DoctorTests` case.

### 5. Docs

- `CLAUDE.md:12`: replace it with: "In the Tart VM, uitest uses the host llama-server automatically
  (debug overlay enables `remote-llm`; uitest probes `192.168.64.1:5801`). Set `SCRIBULAR_LLM_URL=`
  (empty) to force on-device."
- `tools/uitest/README.md:58-69` ("Host llama-server (VM)"): steps 3-4 become automatic for debug
  overlay builds. Keep the manual release-style build line. Document the WARN, the empty override and
  the log assertion.

## Rollout

1. Implement it as its own cwf task on a branch from main (uitest change, Cubic PR), as `CLAUDE.md`
   requires for `tools/uitest` side-quests.
2. After it merges, merge main into feat-recording-p3 and any other open Scribular branches. Their
   next verify build picks up the feature and the URL.
3. Stopgap for the current feat-recording-p3 verify, before the PR merges: hand off with
   `cwf handoff --next verify --note "rebuild with --features remote-llm added to the CLAUDE.md build command; run uitest with SCRIBULAR_LLM_URL=http://192.168.64.1:5801; confirm app.log has 'SCRIBULAR_LLM_URL set'"`.

## Verification (in the VM)

1. `curl -s http://192.168.64.1:5801/health` prints `{"status":"ok"}`.
2. Build with the unchanged `CLAUDE.md` command. Then
   `strings <target>/debug/bundle/macos/Scribular.app/Contents/MacOS/scribular-tauri | grep -c SCRIBULAR_LLM_URL`
   is at least 1. Confirm the binary name with `ls Contents/MacOS`.
3. `SCRIBULAR_SKIP_LICENSE=1 swift run --package-path tools/uitest uitest tools/uitest/scenarios/system-audio-smoke.uitest --app <app>`:
   - uitest prints `info host llama-server: http://192.168.64.1:5801`.
   - `<runDir>/app.log` contains `SCRIBULAR_LLM_URL set: summaries use http://192.168.64.1:5801`.
   - The run passes. The summary takes well under 400 s, and the host llama-server log shows a
     `/completion` request.
   - The app's footprint stays well below 1.8 GB (Activity Monitor or
     `footprint -p <pid>`).
4. Negative: build without the overlay feature (remove it temporarily) and rerun. uitest fails with
   "app ignored SCRIBULAR_LLM_URL".
5. `swift test --package-path tools/uitest` passes (new `HostLLMTests` and `DoctorTests`).
6. On the host (not a VM), the same run prints no host llama-server line and uses on-device, as
   before.

## Fallback when the host server is down

- uitest prints the WARN line and runs on-device. Verify still completes, slowly (allow
  `expect "Summary·" timeout 400s`). The log assertion is skipped because no URL was set.
- Doctor reports `unreachable` in `logs/preflight.log`, so the cause is visible.
- To restore it, start the server on the host, after the VM is running:
  `llama-server -m gemma-4-E2B-it-GGUF-Q4_K_M.gguf --host 192.168.64.1 --port 5801 -np 4 -c 32768`.
- To force on-device on purpose: `SCRIBULAR_LLM_URL= swift run ... uitest ...`.

## Security

The server has no auth and receives transcript text over plain HTTP. Keep it bound to
`192.168.64.1` (the Softnet bridge exists only while a VM runs). Never bind `0.0.0.0`. uitest only
ever probes that fixed bridge address, and only inside a VM, so host runs never send transcripts off
the device. Test fixtures are synthetic speech; no real user audio goes to the server.
