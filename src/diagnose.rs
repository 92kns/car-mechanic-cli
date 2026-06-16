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
    eprintln!("Fetching CaR failure logs via treeherder-cli...");
    let output = std::process::Command::new("treeherder-cli")
        .args([
            url,
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
        bail!("treeherder-cli failed:\n{}", stderr);
    }

    let log_text = String::from_utf8_lossy(&output.stdout).into_owned();
    if log_text.trim().is_empty() {
        eprintln!(
            "treeherder-cli returned no output. The task may not have failed yet, \
                   or no custom-car jobs matched."
        );
        return Ok(());
    }

    run_on_text(&log_text, platform, json)
}

fn run_on_text(log_text: &str, platform: Option<&str>, json: bool) -> Result<()> {
    let platform_filter = platform.and_then(Platform::from_str);

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

    matches.sort_by(|a, b| b.matched_on.len().cmp(&a.matched_on.len()));

    if json {
        println!("{}", serde_json::to_string_pretty(&matches)?);
        return Ok(());
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

        println!("{}. [{}] {}", i + 1, p.id, p.title);
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
            println!("   Related bugs:");
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
