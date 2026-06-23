# car-mechanic

CLI tool for diagnosing and fixing CaR build failures.

Encodes accumulated knowledge from years of CI breakages into structured, queryable patterns. Designed for use by both engineers and AI agents.

> For AI agents: see [AGENTS.md](AGENTS.md) for the full diagnostic workflow.

## Install

Requires [Rust](https://rustup.rs).

```
cargo install --git https://github.com/92kns/car-mechanic-cli
```

## Update

```
car-mechanic update
```

## Dependencies

| Tool | Required | Notes |
|---|---|---|
| `chromium-search` | Bundled | Vendored inside the binary — no install needed. PATH version takes priority if present. |
| `treeherder-cli` | When using `diagnose --url` | Available in any mozilla-central checkout. Fetches CaR failure logs. |
| `searchfox-cli` | Never called directly | Available in mozilla-central checkouts. Use it alongside car-mechanic to search Firefox-side code (taskcluster configs, build scripts). |

## Commands

### `diagnose` — match a build log against known failures

The recommended path is `--url` — car-mechanic resolves the URL to a Treeherder jobs URL and fetches failure logs via `treeherder-cli` automatically. Four input formats are accepted:

```
# Treeherder jobs URL
car-mechanic diagnose --url 'https://treeherder.mozilla.org/jobs?repo=mozilla-central&revision=<hash>'

# Treeherder logviewer URL
car-mechanic diagnose --url 'https://treeherder.mozilla.org/logviewer?job_id=<id>&repo=mozilla-central'

# Taskcluster task URL
car-mechanic diagnose --url 'https://firefox-ci-tc.services.mozilla.com/tasks/<task-id>'

# Bare task ID
car-mechanic diagnose --url UWjqf7IgReac7jLj7MvSCQ
```

Or pipe / pass a file manually:

```
# Pipe from treeherder-cli
treeherder-cli <revision> --fetch-logs --filter custom-car --match-filter failure | car-mechanic diagnose

# From a file
car-mechanic diagnose build.log

# Filter to one platform
car-mechanic diagnose --platform linux64 < build.log

# JSON output (for AI/scripting)
car-mechanic diagnose --json --url 'https://treeherder.mozilla.org/...'
```

Returns: matched pattern(s), cause, ordered fix steps, related Bugzilla bugs, upstream files to check, and suggested search queries.

Platforms: `macos-x64`, `macos-arm64`, `linux64`, `win64`, `android`

### `check` — pre-flight config check

Reads `taskcluster/kinds/toolchain/misc.yml` from your mozilla-central checkout and reports the live config for each CaR platform: `max-run-time`, Python version, macOS SDK being fetched, and (for linux64) the path to the Docker image so you can cross-check runtime lib packages against `install-build-deps.py`.

```
car-mechanic check               # all platforms
car-mechanic check linux64
car-mechanic check macos-x64 --json
```

Run from inside a mozilla-central checkout for live values; falls back to offline mode otherwise.

### `explain` — full detail on a known pattern

```
car-mechanic explain macos-sdk-version
car-mechanic explain linux-vulkan-crash --json
```

### `list` — enumerate all known patterns

```
car-mechanic list
car-mechanic list --platform android
```

### `search` — search Chromium, depot_tools, or V8

```
# Search Chromium source (chromium-search bundled — no separate install needed)
car-mechanic search 'mac_sdk_path'
car-mechanic search --cat build/config/mac/mac_sdk.gni

# Search depot_tools or V8 commit messages
car-mechanic search --repo depot_tools cipd
car-mechanic search --repo v8 snapshot
```

### `risk` — upstream change radar

Queries GitHub for recent commits to files known to break CaR builds (`DEPS`, `mac_sdk.gni`, `visual_studio_version.gni`, `install-build-deps.py`, etc.). Set `GITHUB_TOKEN` to avoid rate limits.

```
car-mechanic risk
car-mechanic risk --platform macos-x64
car-mechanic risk --since 14 --platform win64
```

## Key files in the Firefox repo

| File | Purpose |
|---|---|
| `taskcluster/scripts/misc/build-custom-car.sh` | Main build script; all OS-specific logic |
| `taskcluster/kinds/toolchain/misc.yml` | Task definitions: GN args, timeouts, SDK fetches |
| `taskcluster/docker/custom-car-linux/` | Linux/Android Docker image |

## Adding a new failure pattern

Edit `src/patterns.rs` — add a `Pattern` struct to the `PATTERNS` array:

```rust
Pattern {
    id: "my-new-pattern",
    title: "Short description of the failure",
    platforms: &[Platform::Linux64],
    error_patterns: &[
        r"some regex that matches the log",
    ],
    cause: "Why this happens.",
    fix_steps: &[
        FixStep { description: "Do this first", command: Some("some shell command") },
        FixStep { description: "Then do this", command: None },
    ],
    bugs: &[1234567],
    upstream_files: &["build/some/file.py"],
    search_queries: &["cat build/some/file.py"],
},
```

Then `cargo build` to verify, push, and `car-mechanic update` to reinstall.

## Adding a tracked upstream file to `risk`

Edit `TRACKED_FILES` in `src/risk.rs`:

```rust
TrackedFile {
    path: "build/some/new/file.gni",
    github_repo: Some("chromium/chromium"),
    platforms: &[Platform::Linux64],
    risk_description: "What breaks if this changes",
},
```
