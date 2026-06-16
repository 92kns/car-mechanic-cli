mod diagnose;
mod explain;
mod list;
mod patterns;
mod risk;
mod search;
mod types;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

const REPO_URL: &str = "https://github.com/92kns/car-mechanic-cli";

#[derive(Parser)]
#[command(
    name = "car-mechanic",
    about = "Diagnose and fix Mozilla custom Chromium-as-Release (CaR) build failures",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output results as JSON
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Pattern-match a build log against known CaR failure signatures
    ///
    /// Reads from a file or stdin. Pipe a CI log directly:
    ///   treeherder-cli log <task-id> | car-mechanic diagnose
    Diagnose {
        /// Path to log file (reads stdin if omitted)
        file: Option<PathBuf>,

        /// Restrict to patterns for a specific platform
        /// (macos-x64, macos-arm64, linux64, win64, android)
        #[arg(short, long)]
        platform: Option<String>,
    },

    /// Show full details for a known failure pattern by id
    ///
    /// Use `car-mechanic list` to see all available ids.
    Explain {
        /// Pattern id (e.g. macos-sdk-version)
        id: String,
    },

    /// List all known failure patterns
    List {
        /// Filter by platform (macos-x64, macos-arm64, linux64, win64, android)
        #[arg(short, long)]
        platform: Option<String>,
    },

    /// Search Chromium, depot_tools, or V8 source code
    ///
    /// For --repo chromium (default), delegates to chromium-search on PATH.
    /// For --repo depot_tools or --repo v8, searches recent commit messages.
    ///
    /// Examples:
    ///   car-mechanic search 'mac_sdk_path'
    ///   car-mechanic search --cat build/config/mac/mac_sdk.gni
    ///   car-mechanic search --repo depot_tools cipd
    Search {
        /// Search query (passed to chromium-search for --repo chromium)
        #[arg(required_unless_present = "cat")]
        query: Option<String>,

        /// Repository to search (chromium, depot_tools, v8)
        #[arg(long, default_value = "chromium")]
        repo: String,

        /// Maximum results (chromium only)
        #[arg(short = 'L', long, default_value = "30")]
        limit: usize,

        /// Print file contents instead of searching
        #[arg(long, value_name = "FILE_PATH")]
        cat: Option<String>,

        /// Git ref for --cat (e.g. refs/tags/130.0.6723.58)
        #[arg(long)]
        git_ref: Option<String>,

        /// Extra flags passed through to chromium-search (chromium only)
        #[arg(last = true)]
        extra: Vec<String>,
    },

    /// Show recent upstream changes to files known to break CaR builds
    ///
    /// Queries the GitHub API for the most recent commits to tracked high-risk
    /// files in Chromium and V8.
    Risk {
        /// Informational label for how many days back you care about;
        /// the command always returns the 5 most recent commits per file
        #[arg(long, default_value = "7")]
        since: u32,

        /// Filter to files relevant to a specific platform
        #[arg(short, long)]
        platform: Option<String>,
    },

    /// Update car-mechanic to the latest version from GitHub
    ///
    /// Equivalent to: cargo install --force --git https://github.com/92kns/car-mechanic-cli
    Update,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Diagnose { file, platform } => {
            diagnose::run(file, platform.as_deref(), cli.json)
        }
        Commands::Explain { id } => explain::run(&id, cli.json),
        Commands::List { platform } => list::run(platform.as_deref(), cli.json),
        Commands::Search {
            query,
            repo,
            limit,
            cat,
            git_ref,
            extra,
        } => {
            if let Some(file_path) = cat {
                search::run_cat(&file_path, &repo, git_ref.as_deref())
            } else if let Some(q) = query {
                search::run(&q, &repo, limit, &extra, cli.json)
            } else {
                unreachable!("clap ensures query or --cat is present")
            }
        }
        Commands::Risk { since, platform } => risk::run(since, platform.as_deref(), cli.json),
        Commands::Update => run_update(),
    }
}

fn run_update() -> Result<()> {
    println!("Updating car-mechanic from {}...", REPO_URL);
    let status = std::process::Command::new("cargo")
        .args(["install", "--force", "--git", REPO_URL])
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("Updated successfully.");
            Ok(())
        }
        Ok(s) => anyhow::bail!("cargo install exited with status {}", s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "cargo not found on PATH.\nInstall Rust from https://rustup.rs then retry."
            )
        }
        Err(e) => Err(e).context("failed to run cargo install"),
    }
}
