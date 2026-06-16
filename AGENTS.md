# car-mechanic — Agent Usage Guide

`car-mechanic` is a CLI tool for diagnosing and fixing Mozilla custom Chromium-as-Release
(CaR) build failures. It encodes tribal knowledge from ~40 bugs since CaR inception (~2022).

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

## Six commands

### 1. diagnose — the primary entry point

**Preferred**: pass a Treeherder URL directly — the tool fetches the log via `treeherder-cli`:

```
car-mechanic diagnose --url 'https://treeherder.mozilla.org/jobs?repo=mozilla-central&revision=abc&...'
car-mechanic diagnose --url 'https://treeherder.mozilla.org/...' --json
```

`treeherder-cli` is available in any mozilla-central checkout. The `--url` flag automatically
runs it with `--fetch-logs --filter custom-car --match-filter failure`.

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

### 6. risk — upstream change radar

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

1. Pass the Treeherder job URL to diagnose:
   `car-mechanic diagnose --url '<treeherder-url>'`
2. If patterns matched: read the fix steps. Run the suggested `search` queries to
   verify current upstream state before applying a fix.
3. If no patterns matched: run `car-mechanic risk` to check for recent upstream
   changes. Then search for the error string:
   `car-mechanic search '<error substring>'`
4. Cross-reference with related bugs in `explain` to understand the full history
   of that failure class.
5. Run `car-mechanic check <platform>` to verify the current misc.yml config looks
   sane (Python version, SDK fetch, max-run-time).

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
