use anyhow::{bail, Result};

use crate::patterns::find_by_id;

pub fn run(id: &str, json: bool) -> Result<()> {
    let p = match find_by_id(id) {
        Some(p) => p,
        None => bail!(
            "unknown pattern id '{}'\nRun `car-mechanic list` to see all pattern ids.",
            id
        ),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(p)?);
        return Ok(());
    }

    let platforms: Vec<&str> = p.platforms.iter().map(|pl| pl.as_str()).collect();

    println!("Pattern  : {}", p.id);
    println!("Title    : {}", p.title);
    println!("Platforms: {}", platforms.join(", "));
    println!();
    println!("Cause");
    println!("-----");
    println!("{}", p.cause);
    println!();

    println!("Fix Steps");
    println!("---------");
    for (i, step) in p.fix_steps.iter().enumerate() {
        println!("{}. {}", i + 1, step.description);
        if let Some(cmd) = step.command {
            println!("   $ {}", cmd);
        }
    }

    if !p.bugs.is_empty() {
        println!();
        println!("Related Bugs");
        println!("------------");
        for b in p.bugs {
            println!(
                "  Bug {} - https://bugzilla.mozilla.org/show_bug.cgi?id={}",
                b, b
            );
        }
    }

    if !p.upstream_files.is_empty() {
        println!();
        println!("Upstream Files");
        println!("--------------");
        for f in p.upstream_files {
            println!("  {}", f);
        }
    }

    if !p.search_queries.is_empty() {
        println!();
        println!("Suggested Search Queries");
        println!("------------------------");
        for q in p.search_queries {
            println!("  car-mechanic search '{}'", q);
        }
    }

    if !p.error_patterns.is_empty() {
        println!();
        println!("Error Patterns (regex)");
        println!("----------------------");
        for pat in p.error_patterns {
            println!("  {}", pat);
        }
    }

    Ok(())
}
