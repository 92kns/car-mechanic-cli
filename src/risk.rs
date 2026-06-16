use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::types::Platform;

struct TrackedFile {
    path: &'static str,
    github_repo: Option<&'static str>,
    platforms: &'static [Platform],
    risk_description: &'static str,
}

static TRACKED_FILES: &[TrackedFile] = &[
    TrackedFile {
        path: "build/config/mac/mac_sdk.gni",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::MacosX64, Platform::MacosArm64],
        risk_description: "macOS SDK version change → likely needs SDK toolchain update (pattern: macos-sdk-version)",
    },
    TrackedFile {
        path: "build/config/mac/BUILD.gn",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::MacosX64, Platform::MacosArm64],
        risk_description: "RBE action guards → sed patch in build-custom-car.sh may need updating (pattern: macos-rbe-action)",
    },
    TrackedFile {
        path: "build/mac_toolchain.py",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::MacosX64, Platform::MacosArm64],
        risk_description: "macOS toolchain setup changes → cipd or SDK path logic may break",
    },
    TrackedFile {
        path: "build/config/win/visual_studio_version.gni",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::Win64],
        risk_description: "VS version bump → MSVC Redist version likely changed too (patterns: windows-msvc-redist, windows-sdk-version)",
    },
    TrackedFile {
        path: "build/vs_toolchain.py",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::Win64],
        risk_description: "Windows VS/SDK setup changes → sed patches in build-custom-car.sh may need updating",
    },
    TrackedFile {
        path: "build/toolchain/win/setup_toolchain.py",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::Win64],
        risk_description: "Windows toolchain setup → SDK_VERSION logic or DLL paths may have changed",
    },
    TrackedFile {
        path: "build/install-build-deps.py",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::Linux64, Platform::Android],
        risk_description: "Linux dependency list change → Docker image may be missing new packages (pattern: linux-install-build-deps)",
    },
    TrackedFile {
        path: "build/config/android/config.gni",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::Android],
        risk_description: "Android NDK/SDK version change → gclient sync may fail (pattern: android-gclient-sync)",
    },
    TrackedFile {
        path: "DEPS",
        github_repo: Some("chromium/chromium"),
        platforms: &[
            Platform::MacosX64,
            Platform::MacosArm64,
            Platform::Linux64,
            Platform::Win64,
            Platform::Android,
        ],
        risk_description: "Dependency version changes (clang, gn, buildtools) → may affect all platforms",
    },
    TrackedFile {
        path: ".vpython3",
        github_repo: Some("chromium/chromium"),
        platforms: &[
            Platform::MacosX64,
            Platform::MacosArm64,
            Platform::Linux64,
            Platform::Win64,
            Platform::Android,
        ],
        risk_description: "Python environment change → may affect build scripts (pattern: python-version)",
    },
    TrackedFile {
        path: "build/config/c++/modules.gni",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::MacosX64, Platform::MacosArm64],
        risk_description: "C++ modules config change → may break gn gen with SDK path errors (pattern: macos-clang-modules, Bug 2045375)",
    },
    TrackedFile {
        path: "build/util/lastchange.py",
        github_repo: Some("chromium/chromium"),
        platforms: &[Platform::Win64],
        risk_description: "LASTCHANGE script moved or changed interface → Windows dummy LASTCHANGE generation breaks (pattern: windows-lastchange)",
    },
    // depot_tools has no GitHub mirror; googlesource.com now requires sign-in for log queries.
    // Tracked here for documentation — skipped at runtime with a manual-check note.
    TrackedFile {
        path: "depot_tools/gclient.py",
        github_repo: None,
        platforms: &[
            Platform::MacosX64,
            Platform::MacosArm64,
            Platform::Linux64,
            Platform::Android,
        ],
        risk_description: "depot_tools gclient changes → sync or hook behavior may break (pattern: depot-tools-cipd) [manual check required: no queryable mirror]",
    },
];

#[derive(Deserialize)]
struct GithubCommit {
    sha: String,
    commit: GithubCommitDetail,
    html_url: String,
}

#[derive(Deserialize)]
struct GithubCommitDetail {
    author: GithubAuthor,
    message: String,
}

#[derive(Deserialize)]
struct GithubAuthor {
    name: String,
    date: String,
}

#[derive(Serialize)]
struct RiskEntry {
    file: &'static str,
    platforms: Vec<&'static str>,
    risk_description: &'static str,
    recent_commits: Vec<CommitSummary>,
}

#[derive(Serialize)]
struct CommitSummary {
    hash: String,
    message_first_line: String,
    author: String,
    date: String,
    url: String,
}

pub fn run(since_days: u32, platform: Option<&str>, json: bool) -> Result<()> {
    let platform_filter = platform.and_then(Platform::from_str);

    let tracked: Vec<&TrackedFile> = TRACKED_FILES
        .iter()
        .filter(|f| {
            if let Some(pf) = platform_filter {
                f.platforms.contains(&pf)
            } else {
                true
            }
        })
        .collect();

    let mut results: Vec<RiskEntry> = Vec::new();
    let mut skipped: Vec<&'static str> = Vec::new();

    for file in &tracked {
        let Some(repo) = file.github_repo else {
            skipped.push(file.path);
            continue;
        };

        match fetch_github_commits(repo, file.path, 5) {
            Ok(commits) if !commits.is_empty() => {
                let platforms: Vec<&str> = file.platforms.iter().map(|p| p.as_str()).collect();
                let summaries = commits
                    .into_iter()
                    .map(|c| CommitSummary {
                        hash: c.sha[..12].to_string(),
                        message_first_line: c
                            .commit
                            .message
                            .lines()
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                        author: c.commit.author.name,
                        date: c.commit.author.date,
                        url: c.html_url,
                    })
                    .collect();
                results.push(RiskEntry {
                    file: file.path,
                    platforms,
                    risk_description: file.risk_description,
                    recent_commits: summaries,
                });
            }
            Ok(_) => {}
            Err(e) => eprintln!("warn: could not fetch commits for {}: {}", file.path, e),
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    let filter_label = platform.unwrap_or("all platforms");
    println!(
        "Recent upstream changes to tracked high-risk files [last ~{} days, {}]\n",
        since_days, filter_label
    );

    if results.is_empty() && skipped.is_empty() {
        println!("No recent changes found in tracked files.");
        return Ok(());
    }

    for entry in &results {
        println!("  {} [{}]", entry.file, entry.platforms.join(", "));
        println!("  Risk: {}", entry.risk_description);
        for c in &entry.recent_commits {
            println!("    {} {}  ({})", c.hash, c.message_first_line, c.date);
        }
        println!();
    }

    if !skipped.is_empty() {
        println!("Skipped (no queryable mirror — check manually):");
        for path in skipped {
            let short = path.strip_prefix("depot_tools/").unwrap_or(path);
            println!(
                "  https://chromium.googlesource.com/chromium/tools/depot_tools/+log/main/{}",
                short
            );
        }
        println!();
    }

    println!("Note: shows the 5 most recent commits per file regardless of --since.");

    Ok(())
}

fn fetch_github_commits(repo: &str, path: &str, n: usize) -> Result<Vec<GithubCommit>> {
    let url = format!(
        "https://api.github.com/repos/{}/commits?path={}&per_page={}",
        repo, path, n
    );

    let mut req = ureq::get(&url)
        .set("Accept", "application/vnd.github.v3+json")
        .set("User-Agent", "car-mechanic-cli");

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        req = req.set("Authorization", &format!("Bearer {}", token));
    }

    let resp = req.call();

    match resp {
        Err(ureq::Error::Status(403, _)) | Err(ureq::Error::Status(429, _)) => {
            anyhow::bail!(
                "GitHub API rate limit hit for {}.\n\
                 Set GITHUB_TOKEN env var to raise the limit (60 → 5000 req/hour):\n\
                 export GITHUB_TOKEN=<your-token>",
                path
            )
        }
        Err(e) => return Err(e).with_context(|| format!("GET {}", url)),
        Ok(response) => {
            let body = response.into_string().context("reading response body")?;
            let commits: Vec<GithubCommit> = serde_json::from_str(&body)
                .with_context(|| format!("parsing commits for {}", path))?;
            Ok(commits)
        }
    }
}
