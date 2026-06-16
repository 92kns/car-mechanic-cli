use anyhow::Result;
use serde::Serialize;

use crate::patterns::{filter_by_platform, PATTERNS};
use crate::types::Platform;

#[derive(Serialize)]
struct PatternSummary {
    id: &'static str,
    title: &'static str,
    platforms: Vec<&'static str>,
    bug_count: usize,
}

pub fn run(platform: Option<&str>, json: bool) -> Result<()> {
    let platform_filter = platform.and_then(Platform::from_str);

    let summaries: Vec<PatternSummary> = if let Some(pf) = platform_filter {
        filter_by_platform(pf).collect::<Vec<_>>()
    } else {
        PATTERNS.iter().collect()
    }
    .into_iter()
    .map(|p| PatternSummary {
        id: p.id,
        title: p.title,
        platforms: p.platforms.iter().map(|pl| pl.as_str()).collect(),
        bug_count: p.bugs.len(),
    })
    .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&summaries)?);
        return Ok(());
    }

    let filter_label = platform.unwrap_or("all");
    println!("{} known pattern(s) [platform: {}]\n", summaries.len(), filter_label);

    for s in &summaries {
        println!(
            "  {:35}  {}  ({})",
            s.id,
            s.platforms.join(", "),
            s.title,
        );
    }

    println!();
    println!("Run `car-mechanic explain <id>` for full details.");

    Ok(())
}
