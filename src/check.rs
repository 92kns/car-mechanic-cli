use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;

use crate::types::Platform;

struct PlatformConfig {
    platform: Platform,
    task_name: &'static str,
}

static PLATFORM_CONFIGS: &[PlatformConfig] = &[
    PlatformConfig {
        platform: Platform::MacosX64,
        task_name: "macosx-custom-car",
    },
    PlatformConfig {
        platform: Platform::MacosArm64,
        task_name: "macosx-arm64-custom-car",
    },
    PlatformConfig {
        platform: Platform::Linux64,
        task_name: "linux64-custom-car",
    },
    PlatformConfig {
        platform: Platform::Win64,
        task_name: "win64-custom-car",
    },
    PlatformConfig {
        platform: Platform::Android,
        task_name: "android-custom-car",
    },
];

#[derive(Serialize)]
pub struct CheckResult {
    platform: String,
    task_name: &'static str,
    checks: Vec<CheckItem>,
}

#[derive(Serialize)]
pub struct CheckItem {
    name: String,
    status: &'static str,
    detail: String,
}

pub fn run(platform: Option<&str>, json: bool) -> Result<()> {
    let platform_filter = platform.and_then(Platform::from_str);

    let configs: Vec<&PlatformConfig> = PLATFORM_CONFIGS
        .iter()
        .filter(|c| {
            if let Some(pf) = platform_filter {
                c.platform == pf
            } else {
                true
            }
        })
        .collect();

    let misc_yml = find_misc_yml();
    let misc_content = misc_yml
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok());

    if misc_yml.is_none() {
        eprintln!(
            "note: misc.yml not found by walking up from CWD — \
             run from inside a mozilla-central checkout for live checks."
        );
    }

    let results: Vec<CheckResult> = configs
        .iter()
        .map(|c| {
            let checks = if let Some(ref content) = misc_content {
                check_task(c, content)
            } else {
                offline_checks(c)
            };
            CheckResult {
                platform: c.platform.as_str().to_string(),
                task_name: c.task_name,
                checks,
            }
        })
        .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    let source = misc_yml
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "offline (misc.yml not found)".to_string());

    println!("CaR config check [{}]\n", source);

    for r in &results {
        println!("[{}]", r.platform);
        for c in &r.checks {
            let icon = match c.status {
                "ok" => "✓",
                "warn" => "⚠",
                "info" => "·",
                _ => "?",
            };
            println!("  {} {} — {}", icon, c.name, c.detail);
        }
        println!();
    }

    Ok(())
}

fn check_task(c: &PlatformConfig, misc_content: &str) -> Vec<CheckItem> {
    let mut items = Vec::new();
    let task_section = extract_task_section(misc_content, c.task_name);

    // max-run-time
    if let Some(run_time) = task_section
        .as_deref()
        .and_then(|s| extract_value(s, "max-run-time"))
        .and_then(|v| v.parse::<u32>().ok())
    {
        items.push(CheckItem {
            name: "max-run-time".to_string(),
            status: "info",
            detail: format!("{}s", run_time),
        });
    } else {
        items.push(CheckItem {
            name: "max-run-time".to_string(),
            status: "info",
            detail: "not found in task section".to_string(),
        });
    }

    // use-python
    let python_ver = task_section
        .as_deref()
        .and_then(|s| extract_value(s, "use-python"));
    match python_ver.as_deref() {
        Some("\"3.11\"") | Some("3.11") => {
            items.push(CheckItem {
                name: "use-python".to_string(),
                status: "ok",
                detail: "3.11".to_string(),
            });
        }
        Some(v) => {
            items.push(CheckItem {
                name: "use-python".to_string(),
                status: "warn",
                detail: format!("{} — should be 3.11 (see Bug 1955729)", v),
            });
        }
        None => {
            items.push(CheckItem {
                name: "use-python".to_string(),
                status: "warn",
                detail: "not set — should be \"3.11\"".to_string(),
            });
        }
    }

    // macOS SDK fetch
    if matches!(c.platform, Platform::MacosX64 | Platform::MacosArm64) {
        let sdk = task_section.as_deref().and_then(|s| extract_sdk_fetch(s));
        match sdk {
            Some(ref name) => {
                items.push(CheckItem {
                    name: "sdk-fetch".to_string(),
                    status: "info",
                    detail: format!(
                        "{} — run `car-mechanic search --cat build/config/mac/mac_sdk.gni` \
                         to verify this matches upstream",
                        name
                    ),
                });
            }
            None => {
                items.push(CheckItem {
                    name: "sdk-fetch".to_string(),
                    status: "info",
                    detail: "not detected in fetches block".to_string(),
                });
            }
        }
    }

    // Android symbol_level
    if c.platform == Platform::Android {
        let has_symbol_level_2 = task_section
            .as_deref()
            .map(|s| s.contains("symbol_level=2"))
            .unwrap_or(false);
        items.push(CheckItem {
            name: "gn:symbol_level".to_string(),
            status: if has_symbol_level_2 { "ok" } else { "warn" },
            detail: if has_symbol_level_2 {
                "symbol_level=2 (symbols artifact enabled)".to_string()
            } else {
                "symbol_level=2 not found — symbols artifact may not be packaged".to_string()
            },
        });
    }

    items
}

fn offline_checks(_c: &PlatformConfig) -> Vec<CheckItem> {
    vec![
        CheckItem {
            name: "max-run-time".to_string(),
            status: "info",
            detail: "run from mozilla-central checkout for live value".to_string(),
        },
        CheckItem {
            name: "use-python".to_string(),
            status: "info",
            detail: "run from mozilla-central checkout to verify".to_string(),
        },
    ]
}

/// Walk up from CWD to find taskcluster/kinds/toolchain/misc.yml
fn find_misc_yml() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("taskcluster/kinds/toolchain/misc.yml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Extract the YAML section for a given task name (from its key to the next top-level key)
fn extract_task_section(content: &str, task_name: &str) -> Option<String> {
    let marker = format!("{}:", task_name);
    let start = content.find(&marker)?;
    let rest = &content[start..];

    // Find the next top-level key (line starting with a non-whitespace char that isn't the marker)
    let mut end = rest.len();
    for (i, line) in rest.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let trimmed = line.trim_start();
        if !trimmed.is_empty()
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !line.starts_with('#')
        {
            end = rest
                .lines()
                .take(i)
                .map(|l| l.len() + 1)
                .sum::<usize>()
                .min(rest.len());
            break;
        }
    }

    Some(rest[..end].to_string())
}

/// Extract a scalar value for a given key within a section
fn extract_value<'a>(section: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{}:", key);
    let line = section
        .lines()
        .find(|l| l.trim_start().starts_with(&needle))?;
    let value = line.splitn(2, ':').nth(1)?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Extract a MacOSX SDK name from the fetches block
fn extract_sdk_fetch(section: &str) -> Option<String> {
    section
        .lines()
        .map(|l| l.trim())
        .find(|l| l.starts_with("- MacOSX") || l.starts_with("MacOSX"))
        .map(|l| l.trim_start_matches("- ").to_string())
}
