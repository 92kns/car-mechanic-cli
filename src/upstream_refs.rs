use regex::Regex;

pub struct IssueInfo {
    pub access: IssueAccess,
}

pub enum IssueAccess {
    Accessible { title: String },
    Internal,
    Unreachable(String),
}

/// Extract upstream tracker URLs from the error window of a log (+-5 lines around each error).
pub fn extract_tracker_refs(log: &str) -> Vec<String> {
    let url_re = Regex::new(
        r#"https?://(?:crbug\.com|bugs\.chromium\.org|issues\.chromium\.org)/[^\s'"<>]+"#,
    )
    .unwrap();

    let lines: Vec<&str> = log.lines().collect();
    let mut scan_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for (i, line) in lines.iter().enumerate() {
        if is_error_line(line) {
            for j in i.saturating_sub(5)..=(i + 5).min(lines.len().saturating_sub(1)) {
                scan_indices.insert(j);
            }
        }
    }

    // Fallback: scan the whole log if no error markers found
    let indices: Vec<usize> = if scan_indices.is_empty() {
        (0..lines.len()).collect()
    } else {
        let mut v: Vec<usize> = scan_indices.into_iter().collect();
        v.sort_unstable();
        v
    };

    let mut seen = std::collections::HashSet::new();
    let mut refs = Vec::new();
    for i in indices {
        for m in url_re.find_iter(lines[i]) {
            let url = m
                .as_str()
                .trim_end_matches(|c| c == '.' || c == ',' || c == ')')
                .to_string();
            if seen.insert(url.clone()) {
                refs.push(url);
            }
        }
    }
    refs
}

/// Try to fetch a crbug/chromium issue URL and characterize the result.
pub fn fetch_issue(url: &str) -> IssueInfo {
    match ureq::get(url).set("User-Agent", "car-mechanic-cli").call() {
        Ok(resp) => {
            let body = resp.into_string().unwrap_or_default();
            let title = extract_html_title(&body).unwrap_or_else(|| "(no title)".to_string());
            IssueInfo {
                access: IssueAccess::Accessible { title },
            }
        }
        Err(ureq::Error::Status(code, _)) if code == 401 || code == 403 => IssueInfo {
            access: IssueAccess::Internal,
        },
        Err(e) => IssueInfo {
            access: IssueAccess::Unreachable(e.to_string()),
        },
    }
}

/// Print tracker refs to stdout, fetching each one.
/// `error_snippet` is used in search suggestions when an issue is internal.
pub fn print_tracker_refs(refs: &[String], error_snippet: Option<&str>) {
    if refs.is_empty() {
        return;
    }
    println!();
    println!("Upstream tracker refs found in log:");
    for url in refs {
        let info = fetch_issue(url);
        match info.access {
            IssueAccess::Accessible { ref title } => {
                println!("  {} -- \"{}\"", url, title);
            }
            IssueAccess::Internal => {
                println!(
                    "  {} -- Google-internal issue (Buganizer, requires login)",
                    url
                );
                if let Some(snippet) = error_snippet {
                    let term = sanitize_for_search(snippet);
                    println!("    -> Search instead:");
                    println!("      car-mechanic search --repo depot_tools '{}'", term);
                    println!(
                        "      WebSearch: 'chromium \"{}\" site:bugs.chromium.org OR site:groups.google.com'",
                        term
                    );
                }
            }
            IssueAccess::Unreachable(ref e) => {
                println!("  {} -- could not fetch ({})", url, e);
            }
        }
    }
}

/// Extract the first error line from the log for use as a search term.
pub fn extract_error_snippet(log: &str) -> Option<String> {
    for line in log.lines() {
        if is_error_line(line) {
            let t = line.trim();
            let stripped = t
                .trim_start_matches(|c: char| c == '[' || c.is_ascii_digit() || c == ':')
                .trim_start_matches(|c: char| c == ']' || c == ' ')
                .trim_start_matches("ERROR:")
                .trim_start_matches("error:")
                .trim();
            if !stripped.is_empty() {
                return Some(stripped.chars().take(80).collect());
            }
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

fn extract_html_title(html: &str) -> Option<String> {
    let re = Regex::new(r"(?i)<title[^>]*>([^<]+)</title>").unwrap();
    re.captures(html)
        .map(|cap| cap[1].trim().to_string())
        .filter(|s| !s.is_empty())
}

fn sanitize_for_search(snippet: &str) -> String {
    let path_re = Regex::new(r"/[^\s]+").unwrap();
    let cleaned = path_re.replace_all(snippet, "");
    let ws_re = Regex::new(r"\s+").unwrap();
    ws_re
        .replace_all(cleaned.trim(), " ")
        .chars()
        .take(60)
        .collect::<String>()
        .trim()
        .to_string()
}
