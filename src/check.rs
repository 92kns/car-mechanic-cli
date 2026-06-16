use anyhow::Result;
use serde::Serialize;

use crate::types::Platform;

struct PlatformConfig {
    platform: Platform,
    task_name: &'static str,
    max_run_time: u32,
    /// Warn if build time exceeds this fraction of max_run_time
    warn_threshold: f32,
}

static PLATFORM_CONFIGS: &[PlatformConfig] = &[
    PlatformConfig {
        platform: Platform::MacosX64,
        task_name: "macosx-custom-car",
        max_run_time: 15000,
        warn_threshold: 0.80,
    },
    PlatformConfig {
        platform: Platform::MacosArm64,
        task_name: "macosx-arm64-custom-car",
        max_run_time: 15000,
        warn_threshold: 0.80,
    },
    PlatformConfig {
        platform: Platform::Linux64,
        task_name: "linux64-custom-car",
        max_run_time: 25000,
        warn_threshold: 0.80,
    },
    PlatformConfig {
        platform: Platform::Win64,
        task_name: "win64-custom-car",
        max_run_time: 10000,
        warn_threshold: 0.80,
    },
    PlatformConfig {
        platform: Platform::Android,
        task_name: "android-custom-car",
        max_run_time: 30000,
        warn_threshold: 0.80,
    },
];

#[derive(Serialize)]
struct CheckResult {
    platform: String,
    task_name: &'static str,
    checks: Vec<CheckItem>,
}

#[derive(Serialize)]
struct CheckItem {
    name: &'static str,
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

    let results: Vec<CheckResult> = configs.iter().map(|c| {
        let mut checks = Vec::new();

        // Timeout margin check
        let warn_secs = (c.max_run_time as f32 * c.warn_threshold) as u32;
        checks.push(CheckItem {
            name: "timeout-margin",
            status: "info",
            detail: format!(
                "max-run-time: {}s — warn threshold at {}s ({}%). \
                 If recent builds are approaching this, bump max-run-time in misc.yml.",
                c.max_run_time, warn_secs, (c.warn_threshold * 100.0) as u32
            ),
        });

        // Remind about python version
        checks.push(CheckItem {
            name: "python-version",
            status: "info",
            detail: format!(
                "Verify `use-python: \"3.11\"` is set for {} in \
                 taskcluster/kinds/toolchain/misc.yml",
                c.task_name
            ),
        });

        // Platform-specific checks
        match c.platform {
            Platform::MacosX64 | Platform::MacosArm64 => {
                checks.push(CheckItem {
                    name: "sdk-version",
                    status: "info",
                    detail: format!(
                        "Run `car-mechanic search --cat build/config/mac/mac_sdk.gni` \
                         and compare with the SDK fetched in {} in misc.yml. \
                         Also run `car-mechanic risk --platform {}` to catch recent upstream changes.",
                        c.task_name, c.platform
                    ),
                });
                checks.push(CheckItem {
                    name: "gn-args",
                    status: "info",
                    detail: "Verify use_clang_modules=false and use_v8_context_snapshot=false \
                             are set in misc.yml GN args.".to_string(),
                });
            }
            Platform::Win64 => {
                checks.push(CheckItem {
                    name: "sdk-version",
                    status: "info",
                    detail: "Run `car-mechanic risk --platform win64` to check for recent \
                             VS toolchain or Windows SDK version changes upstream."
                        .to_string(),
                });
            }
            Platform::Linux64 => {
                checks.push(CheckItem {
                    name: "docker-libs",
                    status: "info",
                    detail: "If Chrome crashes at runtime, run `ldd chrome 2>&1 | grep 'not found'` \
                             to identify missing libraries in the Docker image."
                        .to_string(),
                });
            }
            Platform::Android => {
                checks.push(CheckItem {
                    name: "symbol-level",
                    status: "info",
                    detail: "Verify symbol_level=2 is set in android-custom-car GN args \
                             for symbols artifact packaging.".to_string(),
                });
            }
        }

        CheckResult {
            platform: c.platform.as_str().to_string(),
            task_name: c.task_name,
            checks,
        }
    }).collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    for r in &results {
        println!("[{}] {}", r.platform, r.task_name);
        for c in &r.checks {
            println!("  {} {}", c.name, c.detail);
        }
        println!();
    }

    println!("Run `car-mechanic risk [--platform P]` for live upstream change data.");

    Ok(())
}
