---
title: Enforcing the dependency publish-date cutoff on the whole lockfile
date: 2026-10-19
category: workflow-issues
module: dependency policy (Cargo.lock, package-lock.json)
problem_type: workflow_issue
component: tooling
severity: medium
applies_when:
  - "Cargo.lock or package-lock.json changes, including a fresh `cargo generate-lockfile` or `npm install`"
  - A unit adds or bumps any Rust crate or npm package
  - The dependency review moves the publish-date cutoff
symptoms:
  - Exact `=x.y.z` pins on direct dependencies, yet the lockfile holds transitive packages published after the cutoff
  - "`cargo update --precise` refuses a downgrade because another locked crate still requires the newer version"
root_cause: missing_tooling
resolution_type: tooling_addition
tags: [lockfile, supply-chain, cargo, npm, dependency-cutoff, ktd14, crates-io]
---

# Enforcing the dependency publish-date cutoff on the whole lockfile

## Context

KTD14 requires every dependency to be published on or before the cutoff in
`plans/DEPENDENCY_REVIEW_M0.md` (2026-10-05 for U1). Exact pins on direct
dependencies do not meet that rule. `tauri = "=2.12.1"` asks for its own
internals with `~` ranges, and Vite asks for rolldown the same way, so a fresh
resolve pulls in releases from after the cutoff. In U1, 35 of 409 crates and 19 of 98
npm packages were too new. The review doc records which packages moved. This
doc records how to repeat the check, because the scripts that did it existed
only in a session scratchpad.

## Guidance

Check the publish date of every *locked* registry package, not only the direct
ones, and downgrade each late one to the newest compatible release published
before the cutoff.

**Cargo: list late crates.** Read `Cargo.lock`, keep `registry+` packages, and
ask crates.io for each version's `created_at`. crates.io needs a `User-Agent` and
expects about one request a second. Count a failed lookup as late, so it gets
looked at and is never passed silently.

```python
# python3 lockdates.py src-tauri/Cargo.lock > late.out
import sys, json, time, tomllib, urllib.request
CUTOFF = "2026-10-05T23:59:59"
H = {"User-Agent": "scribular-lockfile-date-check"}
lock = tomllib.load(open(sys.argv[1], "rb"))
pkgs = [(p["name"], p["version"]) for p in lock["package"] if p.get("source", "").startswith("registry+")]
late = 0
for n, v in pkgs:
    try:
        d = json.load(urllib.request.urlopen(urllib.request.Request(
            f"https://crates.io/api/v1/crates/{n}/{v}", headers=H), timeout=20))["version"]["created_at"]
    except Exception as e:
        d = f"ERR {e}"
    if d.startswith("ERR") or d[:19] > CUTOFF:
        late += 1; print("LATE", n, v, d, flush=True)
    time.sleep(1)
print(f"DONE checked={len(pkgs)} late={late}")
```

**Cargo: downgrade each late crate.** For each `LATE` line, pick the newest
version that is not yanked, not a prerelease, published before the cutoff, and
semver-compatible with the locked one. Compatibility is judged on the first
nonzero component and everything before it, so `0.24.x` stays `0.24` and `1.x`
stays `1`. Then run `cargo update -p <name>@<locked> --precise <target>`.

```python
# python3 downgrade.py late.out src-tauri
import sys, json, time, subprocess, urllib.request
C = "2026-10-05T23:59:59"
H = {"User-Agent": "scribular-lockfile-date-check"}
def compat(v):
    out = []
    for p in v.split("+")[0].split("-")[0].split("."):
        out.append(p)
        if p != "0": break
    return tuple(out)
for line in open(sys.argv[1]):
    if not line.startswith("LATE"): continue
    _, n, v, _ = line.split(maxsplit=3)
    vs = json.load(urllib.request.urlopen(urllib.request.Request(
        f"https://crates.io/api/v1/crates/{n}/versions", headers=H)))["versions"]
    time.sleep(1)
    ok = [x for x in vs if not x["yanked"] and x["created_at"][:19] <= C
          and compat(x["num"]) == compat(v) and "-" not in x["num"].split("+")[0]]
    if not ok: print("NOFIT", n, v, flush=True); continue
    t = max(ok, key=lambda x: x["created_at"])["num"]
    r = subprocess.run(["cargo", "update", "-p", f"{n}@{v}", "--precise", t],
                       cwd=sys.argv[2], capture_output=True, text=True)
    print("OK" if r.returncode == 0 else "FAIL", n, v, "->", t, flush=True)
```

**Repeat check and downgrade until a check prints `late=0`.** One pass is not
enough. Cargo refuses a downgrade while another locked crate still requires the
newer version, and the order of the late list does not follow the dependency
graph. In U1, `swift-rs 1.0.9 -> 1.0.8` failed in pass 2: Cargo's error named the locked
`tauri = "=2.12.1"` requirement as the blocker. `tauri-utils` and `tauri-codegen`
were downgraded later in that same pass, and pass 3 then moved `swift-rs`
cleanly. Pass 4 found nothing late. A `FAIL` is therefore usually "retry next
pass", not a dead end. A `NOFIT`, where no compatible release exists before the
cutoff, is a real decision for the dependency review.

**npm.** Read `package-lock.json` and look up `time[<version>]` from
`https://registry.npmjs.org/<name>` (scoped names keep their `@`). Fix late
packages with exact versions in `package.json` `overrides`, not by editing the
lockfile, so `npm install` keeps them. Then reinstall, recheck, and run
`npm audit`.

## Why This Matters

Without this check the cutoff only covers the handful of crates named in
`Cargo.toml`. Most of the tree, including build scripts that run at compile time
(`cc`, `swift-rs`, `embed-resource`), arrives through ranges that resolve to
whatever is newest. That newest code is exactly what the cutoff exists to keep
out. Each later unit adds dependencies (U2 brings the storage crates), so the
gap reopens every time the lockfile changes.

## When to Apply

- Every unit that touches `Cargo.lock` or `package-lock.json`, before committing.
- After any `cargo update` or `npm install` without `--precise` or `ci`, since
  these re-resolve to the newest versions.
- When the review moves the cutoff: change `CUTOFF` in both scripts.

## Examples

U1 results (details in `plans/DEPENDENCY_REVIEW_M0.md`, "Lockfile check"):

- Cargo: 35 of 409 were late. It took three downgrade passes and a final clean
  check. Afterwards the tree has 419 registry crates, all before the cutoff, and
  includes `tauri-utils 2.10.1` and `swift-rs 1.0.8`.
- npm: 19 of 98 were late, all under three roots. They are pinned in
  `package.json` `overrides` (`rolldown` 1.2.12, `postcss` 8.5.29, `tinybench`
  6.2.0).

## Related

- `plans/DEPENDENCY_REVIEW_M0.md`: the policy, the cutoff, and the U1 lockfile record.
- `plans/2026-10-19-2001-feat-tauri-rewrite-evidence-prototype-plan.md`: KTD14.
