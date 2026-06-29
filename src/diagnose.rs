use std::io::Read;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use regex::Regex;

use crate::patterns::PATTERNS;
use crate::types::{DiagnoseMatch, Platform};

pub fn run(file: Option<PathBuf>, platform: Option<&str>, json: bool) -> Result<()> {
    let log_text = read_input(file)?;
    run_on_text(&log_text, platform, json)
}

pub fn run_from_url(url: &str, platform: Option<&str>, json: bool) -> Result<()> {
    let (treeherder_url, url_platform) = normalize_to_treeherder_url(url)?;
    if treeherder_url != url {
        eprintln!("Resolved to Treeherder URL: {}", treeherder_url);
    }
    // Platform from --platform flag wins; fall back to what we detected during URL resolution
    let effective_platform: Option<String> = platform
        .map(|s| s.to_string())
        .or_else(|| url_platform.map(|p| p.as_str().to_string()));

    eprintln!("Fetching CaR failure logs via treeherder-cli...");
    let output = std::process::Command::new("treeherder-cli")
        .args([
            treeherder_url.as_str(),
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
                     It ships with the Firefox repo — make sure ~/firefox is set up \
                     and treeherder-cli is on your PATH."
                )
            } else {
                anyhow::anyhow!("running treeherder-cli: {}", e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut msg = format!("treeherder-cli failed:\n{}", stderr);
        if stderr.contains("No revision found") || stderr.contains("revision") {
            msg.push_str(
                "\nExpected Treeherder jobs URL format:\n  \
                 https://treeherder.mozilla.org/jobs?repo=<repo>&revision=<40-char-hash>\n\
                 \nYou can also pass a TC task URL or bare task ID — car-mechanic will resolve it.",
            );
        }
        bail!("{}", msg);
    }

    let log_text = String::from_utf8_lossy(&output.stdout).into_owned();
    if log_text.trim().is_empty() {
        eprintln!(
            "treeherder-cli returned no output. The task may not have failed yet, \
                   or no custom-car jobs matched."
        );
        return Ok(());
    }

    run_on_text(&log_text, effective_platform.as_deref(), json)
}

/// Normalize any supported URL type or bare task ID to a Treeherder jobs URL.
/// Also returns the detected platform when the input is a TC task URL/ID.
pub(crate) fn normalize_to_treeherder_url(url: &str) -> Result<(String, Option<Platform>)> {
    // Already a Treeherder jobs URL with revision
    if (url.contains("treeherder.mozilla.org/jobs")
        || url.contains("treeherder.mozilla.org/#/jobs"))
        && url.contains("revision=")
    {
        return Ok((url.to_string(), None));
    }

    // Treeherder logviewer URL
    if url.contains("treeherder.mozilla.org") && url.contains("logviewer") {
        let th_url = resolve_from_logviewer_url(url)?;
        return Ok((th_url, None));
    }

    // Taskcluster task URL (API endpoint or UI)
    if url.contains("firefox-ci-tc.services.mozilla.com")
        || url.contains("taskcluster-ui.mozilla-releng.net")
    {
        let task_id = extract_tc_task_id_from_url(url)?;
        return resolve_from_tc_task_id(&task_id);
    }

    // Bare task ID: 22 chars of [A-Za-z0-9_-]
    if is_tc_task_id(url) {
        return resolve_from_tc_task_id(url);
    }

    bail!(
        "Unrecognized URL or task ID format.\n\
         Accepted inputs:\n  \
         Treeherder jobs URL:      https://treeherder.mozilla.org/jobs?repo=<repo>&revision=<hash>\n  \
         Treeherder logviewer URL: https://treeherder.mozilla.org/logviewer?job_id=<id>&repo=<repo>\n  \
         Taskcluster task URL:     https://firefox-ci-tc.services.mozilla.com/tasks/<task-id>\n  \
         Bare Taskcluster task ID: <22-char id e.g. UWjqf7IgReac7jLj7MvSCQ>"
    )
}

pub(crate) fn is_tc_task_id(s: &str) -> bool {
    s.len() == 22
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub(crate) fn extract_tc_task_id_from_url(url: &str) -> Result<String> {
    // /tasks/<id>, /tasks/<id>/..., /api/queue/v1/task/<id>
    let re = Regex::new(r"/tasks?/([A-Za-z0-9_-]{22})(?:[/?#]|$)").unwrap();
    if let Some(cap) = re.captures(url) {
        return Ok(cap[1].to_string());
    }
    bail!(
        "Could not extract task ID from Taskcluster URL: {}\n\
         Expected: https://firefox-ci-tc.services.mozilla.com/tasks/<22-char-id>",
        url
    )
}

/// Detect a CaR platform from any string containing a task name.
/// Used both for TC task metadata.name and for log text scanning.
fn detect_platform_from_str(s: &str) -> Option<Platform> {
    // Check arm64 before x64 — macosx-custom-car is a substring of macosx-arm64-custom-car
    if s.contains("macosx-arm64-custom-car") || s.contains("macos-arm64-custom-car") {
        Some(Platform::MacosArm64)
    } else if s.contains("macosx-custom-car") || s.contains("macos-x64-custom-car") {
        Some(Platform::MacosX64)
    } else if s.contains("android-custom-car") {
        Some(Platform::Android)
    } else if s.contains("linux64-custom-car") {
        Some(Platform::Linux64)
    } else if s.contains("win64-custom-car") {
        Some(Platform::Win64)
    } else {
        None
    }
}

fn resolve_from_logviewer_url(url: &str) -> Result<String> {
    // Handle both:
    //   https://treeherder.mozilla.org/logviewer?job_id=<id>&repo=<repo>
    //   https://treeherder.mozilla.org/#/logviewer?job_id=<id>&repo=<repo>
    let query_str: &str = if let Some(hash_pos) = url.find('#') {
        let fragment = &url[hash_pos + 1..];
        fragment.find('?').map(|q| &fragment[q + 1..]).unwrap_or("")
    } else {
        url.find('?').map(|q| &url[q + 1..]).unwrap_or("")
    };

    let mut job_id: Option<String> = None;
    let mut repo: Option<String> = None;
    for pair in query_str.split('&') {
        if let Some(v) = pair.strip_prefix("job_id=") {
            job_id = Some(v.to_string());
        } else if let Some(v) = pair.strip_prefix("repo=") {
            repo = Some(v.to_string());
        }
    }

    let job_id = job_id.ok_or_else(|| {
        anyhow::anyhow!(
            "No job_id found in logviewer URL.\n\
             Expected: https://treeherder.mozilla.org/logviewer?job_id=<id>&repo=<repo>"
        )
    })?;
    let repo = repo.ok_or_else(|| {
        anyhow::anyhow!(
            "No repo found in logviewer URL.\n\
             Expected: https://treeherder.mozilla.org/logviewer?job_id=<id>&repo=<repo>"
        )
    })?;

    eprintln!("Resolving Treeherder job {} to revision...", job_id);

    #[derive(serde::Deserialize)]
    struct Job {
        push_id: u64,
    }

    let job_api = format!("https://treeherder.mozilla.org/api/jobs/{}/", job_id);
    let job_body = ureq::get(&job_api)
        .set("User-Agent", "car-mechanic-cli")
        .call()
        .with_context(|| format!("fetching Treeherder job {}", job_id))?
        .into_string()
        .context("reading Treeherder job response")?;
    let job: Job = serde_json::from_str(&job_body).with_context(|| {
        format!(
            "parsing Treeherder job response: {}",
            &job_body[..job_body.len().min(300)]
        )
    })?;

    #[derive(serde::Deserialize)]
    struct PushResult {
        revision: String,
    }
    #[derive(serde::Deserialize)]
    struct PushResponse {
        results: Vec<PushResult>,
    }

    let push_api = format!(
        "https://treeherder.mozilla.org/api/push/?id={}&format=json",
        job.push_id
    );
    let push_body = ureq::get(&push_api)
        .set("User-Agent", "car-mechanic-cli")
        .call()
        .with_context(|| format!("fetching Treeherder push {}", job.push_id))?
        .into_string()
        .context("reading Treeherder push response")?;
    let push: PushResponse = serde_json::from_str(&push_body).with_context(|| {
        format!(
            "parsing Treeherder push response: {}",
            &push_body[..push_body.len().min(300)]
        )
    })?;

    let revision = push
        .results
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No push found for push_id {}", job.push_id))?
        .revision;

    Ok(format!(
        "https://treeherder.mozilla.org/jobs?repo={}&revision={}",
        repo, revision
    ))
}

fn resolve_from_tc_task_id(task_id: &str) -> Result<(String, Option<Platform>)> {
    eprintln!("Resolving TC task {} to Treeherder URL...", task_id);

    #[derive(serde::Deserialize)]
    struct Metadata {
        name: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Env {
        #[serde(rename = "GECKO_HEAD_REV")]
        gecko_head_rev: Option<String>,
        #[serde(rename = "GECKO_HEAD_REPOSITORY")]
        gecko_head_repository: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Payload {
        env: Option<Env>,
    }
    #[derive(serde::Deserialize)]
    struct Task {
        metadata: Option<Metadata>,
        payload: Option<Payload>,
    }

    let task_api = format!(
        "https://firefox-ci-tc.services.mozilla.com/api/queue/v1/task/{}",
        task_id
    );
    let task_body = ureq::get(&task_api)
        .set("User-Agent", "car-mechanic-cli")
        .call()
        .with_context(|| format!("fetching TC task {}", task_id))?
        .into_string()
        .context("reading TC task response")?;
    let task: Task = serde_json::from_str(&task_body).with_context(|| {
        format!(
            "parsing TC task response: {}",
            &task_body[..task_body.len().min(300)]
        )
    })?;

    // Detect platform from task name (e.g. "toolchain-android-custom-car")
    let platform = task
        .metadata
        .as_ref()
        .and_then(|m| m.name.as_deref())
        .and_then(detect_platform_from_str);

    let env = task.payload.and_then(|p| p.env).ok_or_else(|| {
        anyhow::anyhow!(
            "TC task {} has no payload.env — is this a Gecko CI task?",
            task_id
        )
    })?;

    let revision = env.gecko_head_rev.ok_or_else(|| {
        anyhow::anyhow!("TC task {} has no GECKO_HEAD_REV in payload.env", task_id)
    })?;

    let repo_url = env.gecko_head_repository.ok_or_else(|| {
        anyhow::anyhow!(
            "TC task {} has no GECKO_HEAD_REPOSITORY in payload.env",
            task_id
        )
    })?;

    // https://hg.mozilla.org/mozilla-central → mozilla-central
    // https://hg.mozilla.org/integration/autoland → autoland
    let repo = repo_url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("mozilla-central")
        .to_string();

    let treeherder_url = format!(
        "https://treeherder.mozilla.org/jobs?repo={}&revision={}",
        repo, revision
    );
    Ok((treeherder_url, platform))
}

fn run_on_text(log_text: &str, platform: Option<&str>, json: bool) -> Result<()> {
    // Use explicit --platform, or try to detect from log text as a last resort
    let platform_filter = if let Some(p) = platform.and_then(Platform::from_str) {
        Some(p)
    } else if let Some(p) = detect_platform_from_str(log_text) {
        if !json {
            eprintln!(
                "note: auto-detected platform {} from log — use --platform to override",
                p.as_str()
            );
        }
        Some(p)
    } else {
        None
    };

    let mut matches: Vec<DiagnoseMatch> = PATTERNS
        .iter()
        .filter(|p| {
            if let Some(pf) = platform_filter {
                p.platforms.contains(&pf)
            } else {
                true
            }
        })
        .filter_map(|pattern| {
            let mut matched_on: Vec<String> = Vec::new();
            for &pat in pattern.error_patterns {
                match Regex::new(pat) {
                    Ok(re) if re.is_match(log_text) => matched_on.push(pat.to_string()),
                    Ok(_) => {}
                    Err(e) => eprintln!("warn: invalid regex '{}': {}", pat, e),
                }
            }
            if matched_on.is_empty() {
                None
            } else {
                Some(DiagnoseMatch {
                    pattern,
                    matched_on,
                })
            }
        })
        .collect();

    // Primary sort: platform-specific patterns before cross-platform ones.
    // Secondary: more regex hits within the same specificity bucket.
    matches.sort_by(|a, b| {
        a.pattern
            .platforms
            .len()
            .cmp(&b.pattern.platforms.len())
            .then(b.matched_on.len().cmp(&a.matched_on.len()))
    });

    if json {
        println!("{}", serde_json::to_string_pretty(&matches)?);
        return Ok(());
    }

    if platform_filter.is_none() && matches.len() > 3 && !json {
        eprintln!(
            "note: platform not detected — {} patterns matched. \
             Use --platform (linux64, android, macos-x64, macos-arm64, win64) to filter.",
            matches.len()
        );
    }

    if matches.is_empty() {
        println!("No known patterns matched.");
        println!();
        println!("Next steps:");
        println!("  car-mechanic risk              # check recent upstream changes");
        println!("  car-mechanic search <error>    # search chromium source for the error");
        return Ok(());
    }

    println!("Found {} matching pattern(s):\n", matches.len());

    for (i, m) in matches.iter().enumerate() {
        let p = m.pattern;
        let platforms: Vec<&str> = p.platforms.iter().map(|pl| pl.as_str()).collect();

        let retry_tag = if p.retry_first() {
            " [retry first]"
        } else {
            ""
        };
        println!("{}. [{}] {}{}", i + 1, p.id, p.title, retry_tag);
        println!("   Platforms : {}", platforms.join(", "));
        println!("   Matched on: {}", m.matched_on.join(", "));
        println!();
        println!("   Cause: {}", p.cause);
        println!();

        println!("   Fix steps:");
        for (j, step) in p.fix_steps.iter().enumerate() {
            println!("   {}. {}", j + 1, step.description);
            if let Some(cmd) = step.command {
                println!("      $ {}", cmd);
            }
        }

        if !p.bugs.is_empty() {
            println!();
            println!("   Related bugs ({}):", p.bugs.len());
            for b in p.bugs {
                println!("     https://bugzilla.mozilla.org/show_bug.cgi?id={}", b);
            }
        }

        if !p.upstream_files.is_empty() {
            println!();
            println!("   Upstream files to check:");
            for f in p.upstream_files {
                println!("     {}", f);
            }
        }

        if !p.search_queries.is_empty() {
            println!();
            println!("   Suggested chromium-search queries:");
            for q in p.search_queries {
                println!("     car-mechanic search '{}'", q);
            }
        }

        if i < matches.len() - 1 {
            println!("\n{}\n", "-".repeat(72));
        }
    }

    // Surface any upstream tracker URLs found in the log
    let tracker_refs = crate::upstream_refs::extract_tracker_refs(log_text);
    if !tracker_refs.is_empty() {
        let snippet = crate::upstream_refs::extract_error_snippet(log_text);
        crate::upstream_refs::print_tracker_refs(&tracker_refs, snippet.as_deref());
    }

    Ok(())
}

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
