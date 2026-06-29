use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Serialize)]
pub struct TriageResult {
    pub phase: String,
    pub scope: String,
    pub ownership: String,
    pub last_good_line: Option<String>,
    pub hypothesis: String,
    pub suggested: Vec<String>,
}

pub fn run(file: Option<PathBuf>, url: Option<&str>, json: bool) -> Result<()> {
    let (log_text, treeherder_url) = match url {
        Some(u) => {
            let (th_url, _platform) = crate::diagnose::normalize_to_treeherder_url(u)?;
            eprintln!("Fetching CaR failure logs via treeherder-cli...");
            let output = std::process::Command::new("treeherder-cli")
                .args([
                    th_url.as_str(),
                    "--fetch-logs",
                    "--filter",
                    "custom-car",
                    "--match-filter",
                    "failure",
                ])
                .output()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        anyhow::anyhow!(
                            "treeherder-cli not found on PATH.\n\
                             It ships with the Firefox repo."
                        )
                    } else {
                        anyhow::anyhow!("running treeherder-cli: {}", e)
                    }
                })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("treeherder-cli failed:\n{}", stderr);
            }
            let log = String::from_utf8_lossy(&output.stdout).into_owned();
            (log, Some(th_url))
        }
        None => (read_input(file)?, None),
    };

    let phase = detect_phase(&log_text);
    let scope = treeherder_url
        .as_deref()
        .map(|u| fetch_platform_scope(u).unwrap_or_else(|e| format!("unknown ({})", e)))
        .unwrap_or_else(|| "unknown (use --url to compare platforms across the push)".to_string());
    let ownership = detect_ownership(&log_text);
    let last_good = find_last_good_line(&log_text);
    let tracker_refs = crate::upstream_refs::extract_tracker_refs(&log_text);
    let hypothesis = generate_hypothesis(&phase, &scope, &ownership, &tracker_refs);
    let suggested = suggest_commands(&phase, &ownership, &log_text);
    let docker_diff = docker_package_diff();

    if json {
        let result = TriageResult {
            phase,
            scope,
            ownership,
            last_good_line: last_good,
            hypothesis,
            suggested,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("Triage summary");
    println!("  Phase      : {}", phase);
    println!("  Scope      : {}", scope);
    println!("  Ownership  : {}", ownership);
    if let Some(ref line) = last_good {
        let truncated = &line[..line.len().min(120)];
        println!("  Last good  : \"{}\"", truncated.trim());
    }
    println!();
    println!("Hypothesis  : {}", hypothesis);
    if !suggested.is_empty() {
        println!("Suggested   :");
        for cmd in &suggested {
            println!("              {}", cmd);
        }
    }

    if let Some(ref diff) = docker_diff {
        println!();
        println!("Docker image diff (linux vs android):");
        println!("{}", diff);
    }

    let snippet = crate::upstream_refs::extract_error_snippet(&log_text);
    crate::upstream_refs::print_tracker_refs(&tracker_refs, snippet.as_deref());

    Ok(())
}

// ---------------------------------------------------------------------------
// Q1 — Phase: when in the build did it fail?
// ---------------------------------------------------------------------------

fn detect_phase(log: &str) -> String {
    let error_pos = find_first_error_pos(log);
    let preamble = error_pos.map(|i| &log[..i]).unwrap_or(log);

    // Timing-based if elapsed markers are present
    if let Some(secs) = extract_elapsed_at_error(log) {
        let phase = if secs < 120 {
            "depot_tools / env setup"
        } else if secs < 1200 {
            "source sync"
        } else {
            "compile / link"
        };
        return format!("{} (~{}s from start)", phase, secs);
    }

    // Content-based fallback — look at what ran before the first error
    if preamble.contains("gclient config")
        || preamble.contains("cipd_bin_setup")
        || preamble.contains("vpython")
        || preamble.contains("depot_tools")
    {
        return "depot_tools / env setup".to_string();
    }
    if preamble.contains("gclient sync")
        || preamble.contains("fetch chromium")
        || preamble.contains("Syncing with")
    {
        return "source sync".to_string();
    }
    if preamble.contains("gn gen") || preamble.contains("Running GN") {
        return "gn gen / configure".to_string();
    }
    if preamble.contains("autoninja")
        || preamble.contains("ninja -C")
        || log.contains("ninja: FAILED")
    {
        return "compile / link".to_string();
    }

    "unknown".to_string()
}

fn find_first_error_pos(log: &str) -> Option<usize> {
    for line in log.lines() {
        if is_error_line(line) {
            return log.find(line);
        }
    }
    None
}

fn is_error_line(line: &str) -> bool {
    let markers = [
        "ERROR",
        "FAILED",
        "error:",
        "Fatal:",
        "fatal:",
        "failed with exit",
        "exit status 1",
        "non-zero exit",
        "error while loading shared libraries",
        "cannot open shared object",
        "No such file or directory",
    ];
    markers.iter().any(|m| line.contains(m))
}

fn extract_elapsed_at_error(log: &str) -> Option<u32> {
    use regex::Regex;
    // Match [H:MM:SS] or [MM:SS] at the start of the first error line
    let ts_re = Regex::new(r"^\[(\d+):(\d{2}):(\d{2})").unwrap();
    let ts_short = Regex::new(r"^\[(\d+):(\d{2})\]").unwrap();

    for line in log.lines() {
        if !is_error_line(line) {
            continue;
        }
        if let Some(cap) = ts_re.captures(line) {
            let h: u32 = cap[1].parse().ok()?;
            let m: u32 = cap[2].parse().ok()?;
            let s: u32 = cap[3].parse().ok()?;
            return Some(h * 3600 + m * 60 + s);
        }
        if let Some(cap) = ts_short.captures(line) {
            let m: u32 = cap[1].parse().ok()?;
            let s: u32 = cap[2].parse().ok()?;
            return Some(m * 60 + s);
        }
        break; // only look at first error line
    }
    None
}

// ---------------------------------------------------------------------------
// Q2 — Scope: which platforms failed vs passed?
// ---------------------------------------------------------------------------

fn fetch_platform_scope(treeherder_url: &str) -> Result<String> {
    let repo = parse_url_param(treeherder_url, "repo")
        .ok_or_else(|| anyhow::anyhow!("no repo in Treeherder URL"))?;
    let revision = parse_url_param(treeherder_url, "revision")
        .ok_or_else(|| anyhow::anyhow!("no revision in Treeherder URL"))?;

    eprintln!("Fetching platform scope from Treeherder...");

    // Get push_id (reuse same pattern as diagnose logviewer resolution)
    #[derive(serde::Deserialize)]
    struct PushResult {
        id: u64,
    }
    #[derive(serde::Deserialize)]
    struct PushResponse {
        results: Vec<PushResult>,
    }
    let push_api = format!(
        "https://treeherder.mozilla.org/api/push/?repo={}&revision={}",
        repo, revision
    );
    let push_body = ureq::get(&push_api)
        .set("User-Agent", "car-mechanic-cli")
        .call()
        .context("fetching Treeherder push")?
        .into_string()
        .context("reading push response")?;
    let push: PushResponse =
        serde_json::from_str(&push_body).context("parsing Treeherder push response")?;
    let push_id = push
        .results
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no push found for revision {}", revision))?
        .id;

    // Get all jobs for this push, filter client-side for custom-car
    let jobs_api = format!(
        "https://treeherder.mozilla.org/api/jobs/?repo={}&push_id={}&count=200",
        repo, push_id
    );
    let jobs_body = ureq::get(&jobs_api)
        .set("User-Agent", "car-mechanic-cli")
        .call()
        .context("fetching Treeherder jobs")?
        .into_string()
        .context("reading jobs response")?;

    let jobs_val: serde_json::Value =
        serde_json::from_str(&jobs_body).context("parsing Treeherder jobs response")?;

    // Treeherder returns a compact positional format:
    //   job_property_names: ["id", "result_set_id", "platform", ..., "result", "job_type_name", ...]
    //   results: [[val, val, ...], ...]
    let prop_names: Vec<String> = jobs_val["job_property_names"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let results = jobs_val["results"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("unexpected Treeherder jobs format"))?;

    let mut failed: Vec<String> = Vec::new();
    let mut passed: Vec<String> = Vec::new();

    for job in results {
        let (type_name, result, platform) = if let Some(arr) = job.as_array() {
            if prop_names.is_empty() {
                continue;
            }
            let get = |name: &str| -> String {
                prop_names
                    .iter()
                    .position(|p| p == name)
                    .and_then(|i| arr.get(i))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            (get("job_type_name"), get("result"), get("platform"))
        } else if job.is_object() {
            (
                job["job_type_name"].as_str().unwrap_or("").to_string(),
                job["result"].as_str().unwrap_or("").to_string(),
                job["platform"].as_str().unwrap_or("").to_string(),
            )
        } else {
            continue;
        };

        if !type_name.contains("custom-car") {
            continue;
        }

        let label = if platform.is_empty() {
            type_name.clone()
        } else {
            platform.clone()
        };

        match result.as_str() {
            "success" => passed.push(label),
            "testfailed" | "busted" | "exception" => failed.push(label),
            _ => {} // pending / running / retry — skip
        }
    }

    if failed.is_empty() && passed.is_empty() {
        return Ok("no completed custom-car jobs found (may still be running)".to_string());
    }

    let mut parts = Vec::new();
    if !failed.is_empty() {
        parts.push(format!("failed: {}", failed.join(", ")));
    }
    if !passed.is_empty() {
        parts.push(format!("passed: {}", passed.join(", ")));
    }

    let note = if failed.len() == 1 && !passed.is_empty() {
        " — single platform, likely worker image or tooling difference"
    } else if passed.is_empty() {
        " — all platforms failing, likely shared code or infra"
    } else {
        ""
    };

    Ok(format!("{}{}", parts.join("; "), note))
}

fn parse_url_param(url: &str, key: &str) -> Option<String> {
    let query = url.find('?').map(|i| &url[i + 1..])?;
    for pair in query.split('&') {
        if let Some(val) = pair.strip_prefix(&format!("{}=", key)) {
            return Some(val.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Q3 — Ownership: upstream infra, upstream code, or ours?
// ---------------------------------------------------------------------------

fn detect_ownership(log: &str) -> String {
    let ctx = extract_error_context(log);

    if ctx.contains("crbug.com") || ctx.contains("bugs.chromium.org") {
        return "upstream (error references chromium bug tracker)".to_string();
    }

    let upstream_infra = [
        ("vpython-root", "vpython-root"),
        ("depot_tools/", "depot_tools"),
        (".cipd_bin/", ".cipd_bin"),
        ("vpython3/", "vpython3"),
        ("third_party/", "third_party"),
    ];
    for (pat, label) in &upstream_infra {
        if ctx.contains(pat) {
            return format!("upstream infra (path: {})", label);
        }
    }

    let ours = [
        ("build-custom-car.sh", "build-custom-car.sh"),
        ("misc.yml", "misc.yml"),
        ("custom-car-linux", "custom-car-linux Dockerfile"),
        ("custom-car-android", "custom-car-android Dockerfile"),
    ];
    for (pat, label) in &ours {
        if ctx.contains(pat) {
            return format!("ours ({})", label);
        }
    }

    "unknown".to_string()
}

/// Return a window of lines starting at the first error, for ownership heuristics.
fn extract_error_context(log: &str) -> String {
    let mut found = false;
    let mut lines = Vec::new();
    for line in log.lines() {
        if !found && is_error_line(line) {
            found = true;
        }
        if found {
            lines.push(line);
            if lines.len() >= 20 {
                break;
            }
        }
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Q4 — Last good line: what ran successfully immediately before the error?
// ---------------------------------------------------------------------------

fn find_last_good_line(log: &str) -> Option<String> {
    let mut last_good: Option<&str> = None;
    for line in log.lines() {
        if is_error_line(line) {
            return last_good.map(|s| s.trim().to_string());
        }
        let t = line.trim();
        if !t.is_empty() && !t.starts_with('#') {
            last_good = Some(line);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Synthesis — hypothesis + suggested commands
// ---------------------------------------------------------------------------

fn generate_hypothesis(
    phase: &str,
    scope: &str,
    ownership: &str,
    tracker_refs: &[String],
) -> String {
    if !tracker_refs.is_empty() {
        return "Upstream tool embedded a bug tracker reference -- this is a known upstream issue. See tracker refs below before investigating further.".to_string();
    }
    let depot_phase = phase.contains("depot_tools") || phase.contains("env setup");
    let infra = ownership.contains("upstream infra");
    let upstream = ownership.contains("upstream");
    let ours = ownership.contains("ours");
    let single = scope.contains("single platform");
    let all_fail = scope.contains("all platforms failing");

    if depot_phase && infra {
        return "Upstream depot_tools or vpython change — retry; if persistent, check recent depot_tools commits.".to_string();
    }
    if infra && single {
        return "Platform-specific upstream infra issue — worker image tooling difference, not a code regression.".to_string();
    }
    if infra {
        return "Upstream infra failure — check depot_tools and vpython for recent changes."
            .to_string();
    }
    if upstream && single {
        return "Upstream code change broke one platform — check recent Chromium commits to the failing path.".to_string();
    }
    if upstream {
        return "Upstream Chromium code change — check recent commits to the file in the error."
            .to_string();
    }
    if ours && single {
        return "Platform-specific issue in our configuration — check misc.yml, build script, or Dockerfile for the failing platform.".to_string();
    }
    if ours {
        return "Issue in our CaR configuration or build script.".to_string();
    }
    if all_fail {
        return "All platforms failing — likely a shared upstream regression or infra outage."
            .to_string();
    }
    if single {
        return "Single platform failure — narrow to that platform's worker image, tool version, or our platform-specific config.".to_string();
    }

    "Ownership and scope unclear — examine the last good line and error context directly."
        .to_string()
}

fn suggest_commands(phase: &str, ownership: &str, log: &str) -> Vec<String> {
    let mut cmds: Vec<String> = Vec::new();

    if phase.contains("depot_tools") || phase.contains("env setup") {
        cmds.push("car-mechanic search --repo depot_tools cipd".to_string());
        cmds.push("car-mechanic risk --since 7".to_string());
    } else if phase.contains("compile") || phase.contains("link") {
        if let Some(path) = extract_failing_source_path(log) {
            cmds.push(format!("car-mechanic search --cat {}", path));
        }
        cmds.push("car-mechanic risk --since 7".to_string());
    } else if phase.contains("gn gen") {
        cmds.push("car-mechanic risk --since 7".to_string());
    } else {
        cmds.push("car-mechanic risk --since 7".to_string());
    }

    if ownership.contains("upstream infra") {
        cmds.retain(|c| c.contains("risk") || c.contains("depot_tools"));
    }

    if cmds.is_empty() {
        cmds.push("car-mechanic risk --since 7".to_string());
    }

    cmds
}

/// Try to extract a source file path from the first error line (for search suggestions).
fn extract_failing_source_path(log: &str) -> Option<String> {
    use regex::Regex;
    let re = Regex::new(r"(build/[^\s:]+|chrome/[^\s:]+|third_party/[^\s:]+)").unwrap();
    for line in log.lines() {
        if is_error_line(line) {
            if let Some(cap) = re.captures(line) {
                return Some(cap[1].to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Docker image diff
// ---------------------------------------------------------------------------

/// Diff apt packages between the linux and android Docker images.
/// Returns None if either Dockerfile isn't found (not in a checkout).
fn docker_package_diff() -> Option<String> {
    let linux = find_dockerfile_path("taskcluster/docker/custom-car-linux/Dockerfile")?;
    let android = find_dockerfile_path("taskcluster/docker/custom-car-android/Dockerfile")?;

    let linux_content = std::fs::read_to_string(linux).ok()?;
    let android_content = std::fs::read_to_string(android).ok()?;

    let linux_pkgs = extract_apt_packages(&linux_content);
    let android_pkgs = extract_apt_packages(&android_content);

    let mut only_linux: Vec<&String> = linux_pkgs.difference(&android_pkgs).collect();
    let mut only_android: Vec<&String> = android_pkgs.difference(&linux_pkgs).collect();
    only_linux.sort();
    only_android.sort();

    if only_linux.is_empty() && only_android.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    if !only_linux.is_empty() {
        parts.push(format!(
            "  linux only : {}",
            only_linux
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !only_android.is_empty() {
        parts.push(format!(
            "  android only: {}",
            only_android
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Some(parts.join("\n"))
}

fn find_dockerfile_path(rel: &str) -> Option<std::path::PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(rel);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn extract_apt_packages(content: &str) -> std::collections::HashSet<String> {
    let skip: std::collections::HashSet<&str> = [
        "RUN",
        "apt-get",
        "apt",
        "install",
        "update",
        "upgrade",
        "autoremove",
        "rm",
        "-y",
        "-q",
        "-rf",
        "&&",
        "true",
        "false",
        "--no-install-recommends",
        "--no-install-suggests",
    ]
    .iter()
    .cloned()
    .collect();

    let mut in_install = false;
    let mut packages = std::collections::HashSet::new();

    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        if t.contains("apt-get install") || (t.contains("apt ") && t.contains(" install")) {
            in_install = true;
        }
        if in_install {
            for tok in t.split_whitespace() {
                let tok = tok.trim_end_matches('\\').trim();
                if tok.is_empty()
                    || tok.starts_with('-')
                    || tok.starts_with('$')
                    || tok.starts_with('/')
                {
                    continue;
                }
                if skip.contains(tok) {
                    continue;
                }
                if tok
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_alphabetic())
                    .unwrap_or(false)
                    && tok
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '+')
                {
                    packages.insert(tok.to_string());
                }
            }
            if !t.ends_with('\\') {
                in_install = false;
            }
        }
    }
    packages
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn read_input(file: Option<PathBuf>) -> Result<String> {
    match file {
        Some(path) => {
            std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading stdin")?;
            Ok(buf)
        }
    }
}
