use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const DEPOT_TOOLS_LOG_URL: &str =
    "https://chromium.googlesource.com/chromium/tools/depot_tools/+log/main";
const V8_LOG_URL: &str = "https://chromium.googlesource.com/v8/v8/+log/main";

#[derive(Deserialize, Serialize)]
struct GitilesCommit {
    commit: String,
    message: String,
    author: GitilesAuthor,
    committer: GitilesAuthor,
}

#[derive(Deserialize, Serialize)]
struct GitilesAuthor {
    name: String,
    email: String,
    time: String,
}

#[derive(Deserialize)]
struct GitilesLog {
    log: Vec<GitilesCommit>,
}

pub fn run(query: &str, repo: &str, limit: usize, extra_args: &[String], json: bool) -> Result<()> {
    match repo {
        "chromium" => run_chromium_search(query, limit, extra_args),
        "depot_tools" => run_gitiles_log_search(query, DEPOT_TOOLS_LOG_URL, limit, json),
        "v8" => run_gitiles_log_search(query, V8_LOG_URL, limit, json),
        other => bail!(
            "unknown repo '{}'. Valid options: chromium, depot_tools, v8",
            other
        ),
    }
}

pub fn run_cat(path: &str, repo: &str, git_ref: Option<&str>) -> Result<()> {
    match repo {
        "chromium" => {
            let mut args = vec!["cat".to_string(), path.to_string()];
            if let Some(r) = git_ref {
                args.extend_from_slice(&["--ref".to_string(), r.to_string()]);
            }
            run_chromium_search_raw(&args)
        }
        other => bail!(
            "cat is only supported for --repo chromium (got '{}')",
            other
        ),
    }
}

fn run_chromium_search(query: &str, limit: usize, extra_args: &[String]) -> Result<()> {
    let mut args = vec![query.to_string(), "-L".to_string(), limit.to_string()];
    args.extend_from_slice(extra_args);
    run_chromium_search_raw(&args)
}

fn run_chromium_search_raw(args: &[String]) -> Result<()> {
    let status = std::process::Command::new("chromium-search")
        .args(args)
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => bail!("chromium-search exited with status {}", s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
            "chromium-search not found on PATH.\n\
             Install from: https://github.com/92kns/chromium-search\n\
             Or copy from ~/perf/chromium-search/chromium-search and put it on PATH."
        ),
        Err(e) => Err(e).context("running chromium-search"),
    }
}

fn run_gitiles_log_search(query: &str, base_url: &str, limit: usize, json: bool) -> Result<()> {
    let url = format!("{}?format=JSON&n={}", base_url, limit.min(200));
    let raw = fetch_text(&url)?;
    let stripped = raw.trim_start_matches(")]}'").trim_start_matches('\n');
    let log: GitilesLog =
        serde_json::from_str(stripped).context("parsing gitiles log response")?;

    let q_lower = query.to_lowercase();
    let matches: Vec<&GitilesCommit> = log
        .log
        .iter()
        .filter(|c| c.message.to_lowercase().contains(&q_lower))
        .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&matches)?);
        return Ok(());
    }

    if matches.is_empty() {
        println!("No commits matching '{}' in the last {} entries.", query, limit);
        return Ok(());
    }

    println!("{} commit(s) matching '{}':\n", matches.len(), query);
    for c in matches {
        let short = &c.commit[..12];
        let first_line = c.message.lines().next().unwrap_or("").trim();
        println!("  {} {} [{}]", short, first_line, c.author.time);
    }

    Ok(())
}

fn fetch_text(url: &str) -> Result<String> {
    ureq::get(url)
        .set("User-Agent", "car-mechanic-cli")
        .call()
        .with_context(|| format!("GET {}", url))?
        .into_string()
        .context("reading response body")
}
