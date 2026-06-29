# car-mechanic — Agent Usage Guide

`car-mechanic` is a CLI tool for diagnosing and fixing CaR build failures.
It encodes accumulated knowledge from years of CI breakages into structured, queryable patterns.

## Companion tools available in a mozilla-central checkout

| Tool | Purpose |
|---|---|
| `car-mechanic search` | Search upstream **Chromium** source (chromium-search, bundled) |
| `searchfox-cli` | Search **Firefox/mozilla-central** source — use for taskcluster configs, build scripts |
| `treeherder-cli` | Fetch CI **failure** logs — pass a Treeherder URL to `car-mechanic diagnose --url` |

`searchfox-cli` and `treeherder-cli` are available in any mozilla-central checkout.
Use `searchfox-cli` when you need to look up Firefox-side code (e.g. `build-custom-car.sh`,
`misc.yml` contents, Taskcluster transforms). Use `car-mechanic search` for upstream Chromium.

## Setup (one time per clone)

```bash
cargo install --git https://github.com/92kns/car-mechanic-cli
git config core.hooksPath .githooks   # enforce rustfmt before every push
```

## Installation

```
cargo install --git https://github.com/92kns/car-mechanic-cli
```

Requires Rust (https://rustup.rs). Puts `car-mechanic` on PATH via `~/.cargo/bin/`.

## Update

```
car-mechanic update
```

## Seven commands

### 1. diagnose — the primary entry point

**Preferred**: pass a Treeherder URL directly — the tool fetches failure logs via `treeherder-cli`:

```
car-mechanic diagnose --url 'https://treeherder.mozilla.org/jobs?repo=mozilla-central&revision=abc&...'
car-mechanic diagnose --url 'https://treeherder.mozilla.org/...' --json
```

`treeherder-cli` only fetches **failure** logs (passing jobs have no useful log to diagnose).
The `--url` flag runs it with `--fetch-logs --filter custom-car --match-filter failure`.

Alternatively, pipe or pass a file:

```
# Pipe manually:
treeherder-cli <revision> --fetch-logs --filter custom-car --match-filter failure | car-mechanic diagnose

# From a file:
car-mechanic diagnose /path/to/build.log

# Filter to one platform:
car-mechanic diagnose --platform linux64 < build.log
```

Output includes: pattern id, cause, ordered fix steps (some with shell commands),
related bug URLs, upstream files to inspect, and suggested `search` queries.

### 2. check — pre-flight config check

Reads `taskcluster/kinds/toolchain/misc.yml` live from the mozilla-central checkout
(walks up from CWD). Reports `max-run-time`, Python version pin, and macOS SDK fetch
for each CaR platform. Falls back to offline reminders if misc.yml is not found.

```
car-mechanic check               # all platforms
car-mechanic check linux64
car-mechanic check macos-x64 --json
```

### 3. explain — full detail on a known pattern

```
car-mechanic explain macos-sdk-version
car-mechanic explain linux-vulkan-crash --json
```

Use `car-mechanic list` to see all pattern ids.

### 4. list — enumerate known patterns

```
car-mechanic list
car-mechanic list --platform android
car-mechanic list --json
```

### 5. search — search Chromium/depot_tools/V8 source

```
# Search Chromium source (delegates to chromium-search on PATH):
car-mechanic search 'mac_sdk_path'
car-mechanic search 'DEPOT_TOOLS_WIN_TOOLCHAIN file:build/'

# Print a specific file:
car-mechanic search --cat build/config/mac/mac_sdk.gni
car-mechanic search --cat build/vs_toolchain.py

# Search depot_tools or V8 commit messages:
car-mechanic search --repo depot_tools cipd
car-mechanic search --repo v8 snapshot

# Pass extra flags to chromium-search:
car-mechanic search 'sdk_inputs' -- --json -C 3
```

`chromium-search` must be on PATH for `--repo chromium` (the default).
Install from: https://github.com/92kns/chromium-search

### 6. triage — structured reasoning when patterns don't match

Use when `diagnose` returns no match, or returns many noisy matches and the relevant
ones aren't obvious. `triage` answers four diagnostic questions and produces a hypothesis:

```
car-mechanic triage --url UWjqf7IgReac7jLj7MvSCQ
car-mechanic triage build.log
cat failure.log | car-mechanic triage
```

The four questions:

1. **Phase** — when in the build did it fail? Sub-60s from start → depot_tools/env setup.
   1–20min → source sync. Later → compile/link. Phase alone collapses the search space.

2. **Scope** — which platforms failed vs passed for this revision? (Requires `--url`; calls
   the Treeherder API for you.) macOS-only failure → worker image or tooling difference,
   not a code regression. All platforms → shared infra or code change.

3. **Ownership** — is the failing path inside `depot_tools/`, `vpython-root`, or
   `third_party/`? → upstream infra; don't look inward, retry or escalate. Path in
   `build-custom-car.sh` or `misc.yml`? → ours.

4. **Last good line** — the last successful operation before the first error. That
   boundary is your entry point for investigation.

Output example:
```
Triage summary
  Phase      : depot_tools / env setup (~6s from start)
  Scope      : failed: macos-arm64; passed: linux64, android — single platform, likely worker image or tooling difference
  Ownership  : upstream infra (path: vpython-root)
  Last good  : "Current depot_tools revision: 99c70721..."

Hypothesis  : Upstream depot_tools or vpython change — retry; if persistent, check recent depot_tools commits.
Suggested   :
              car-mechanic search --repo depot_tools cipd
              car-mechanic risk --since 7
```

### 7. risk — upstream change radar

Queries the GitHub API for recent commits to files known to break CaR builds.

```
car-mechanic risk                          # all platforms
car-mechanic risk --platform win64
car-mechanic risk --since 14 --platform macos-x64
car-mechanic risk --json
```

Tracked files: `build/config/mac/mac_sdk.gni`, `build/vs_toolchain.py`,
`build/config/win/visual_studio_version.gni`, `build/install-build-deps.py`,
`build/config/android/config.gni`, `DEPS`, `.vpython3`, and more.

---

## Recommended diagnostic workflow

1. Pass the task ID or URL to diagnose:
   `car-mechanic diagnose --url '<task-id-or-url>'`
2. If patterns matched: read the fix steps. Run the suggested `search` queries to
   verify current upstream state before applying a fix.
3. If no patterns matched, or if the matches are noisy/off-platform:
   `car-mechanic triage --url '<task-id-or-url>'`
   This answers Phase / Scope / Ownership / Last-good-line and gives a hypothesis.
4. Use the triage output to pick the right next tool:
   - Ownership = upstream infra → `car-mechanic search --repo depot_tools <term>`
   - Ownership = upstream code → `car-mechanic risk`, then `car-mechanic search <term>`
   - Ownership = ours → `searchfox-cli` on build-custom-car.sh / misc.yml
5. Cross-reference with related bugs in `explain` to understand the full history.
6. Run `car-mechanic check <platform>` to verify misc.yml config (Python version,
   SDK fetch, max-run-time, Dockerfile path).

## Known platforms

| Value | Toolchain task in misc.yml | Worker |
|---|---|---|
| `macos-x64` | `macosx-custom-car` | b-osx-arm64 (cross-compiled) |
| `macos-arm64` | `macosx-arm64-custom-car` | b-osx-arm64 |
| `linux64` | `linux64-custom-car` | b-linux-docker-xlarge-amd |
| `win64` | `win64-custom-car` | b-win2022-xxlarge |
| `android` | `android-custom-car` | b-linux-docker-xlarge-amd |

## Key files in the Firefox repo

| File | Purpose |
|---|---|
| `taskcluster/scripts/misc/build-custom-car.sh` | Main build script; all OS-specific logic |
| `taskcluster/kinds/toolchain/misc.yml` | Task definitions: GN args, timeouts, fetches |
| `taskcluster/docker/custom-car-linux/` | Linux/Android Docker image |

## Upstream tracker URLs in logs

When a log contains a `crbug.com`, `bugs.chromium.org`, or `issues.chromium.org` URL,
the upstream tool is embedding its own bug reference. This is a strong signal that the
failure is a known upstream issue -- not a Mozilla code problem.

`car-mechanic triage` and `car-mechanic diagnose` extract these automatically and attempt
to fetch them. Two cases:

**Accessible** (small/public issue IDs): the title and status are shown inline. Read the
most recent comment for a workaround or ETA before attempting any Mozilla-side fix.

**401/403 (Google-internal Buganizer)**: 9-digit IDs in the 400M+ range are almost always
internal. The tool surfaces this explicitly and suggests alternative searches:
```
car-mechanic search --repo depot_tools '<error substring>'
WebSearch: 'chromium "<error substring>" site:bugs.chromium.org OR site:groups.google.com'
```
Note: `crbug.com` URLs that return 401 *themselves* tell you something -- this is an
internal infra issue with no public ETA. Don't look for a Mozilla-side fix; monitor the
upstream tracker or wait for it to resolve.

The vpython "AR INSTALL FAILED" URL is a generic catch-all; always search for the
*specific package name and version* from the log, not the generic error string.

## Adding a new failure pattern

Edit `src/patterns.rs`. Each `Pattern` in the `PATTERNS` array has:
- `id`: kebab-case, unique
- `platforms`: which CaR targets are affected
- `error_patterns`: regex strings — any match in the log fires the pattern
- `cause`: plain-text explanation
- `fix_steps`: ordered list; `command` is optional
- `bugs`: Bugzilla bug numbers (tool generates URLs automatically)
- `upstream_files`: Chromium/depot_tools paths worth checking
- `search_queries`: suggested `car-mechanic search` invocations

After editing: `cargo build` to verify, then `car-mechanic update` to reinstall.

## Adding a tracked file to `risk`

Edit `TRACKED_FILES` in `src/risk.rs`. Set `github_repo` to `Some("owner/repo")`
for files with a GitHub mirror, or `None` for depot_tools (skipped with a note).
