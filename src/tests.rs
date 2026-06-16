use regex::Regex;

use crate::patterns::{find_by_id, filter_by_platform, PATTERNS};
use crate::types::Platform;

// ---------------------------------------------------------------------------
// Pattern registry integrity
// ---------------------------------------------------------------------------

#[test]
fn all_pattern_ids_are_unique() {
    let mut ids: Vec<&str> = PATTERNS.iter().map(|p| p.id).collect();
    ids.sort_unstable();
    let original_len = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), original_len, "duplicate pattern ids found");
}

#[test]
fn all_patterns_have_required_fields() {
    for p in PATTERNS {
        assert!(!p.id.is_empty(), "pattern has empty id");
        assert!(!p.title.is_empty(), "pattern '{}' has empty title", p.id);
        assert!(!p.cause.is_empty(), "pattern '{}' has empty cause", p.id);
        assert!(!p.platforms.is_empty(), "pattern '{}' has no platforms", p.id);
        assert!(
            !p.fix_steps.is_empty(),
            "pattern '{}' has no fix steps",
            p.id
        );
        for step in p.fix_steps {
            assert!(
                !step.description.is_empty(),
                "pattern '{}' has a fix step with empty description",
                p.id
            );
        }
    }
}

#[test]
fn all_error_patterns_are_valid_regex() {
    for p in PATTERNS {
        for &pat in p.error_patterns {
            assert!(
                Regex::new(pat).is_ok(),
                "pattern '{}' has invalid regex: {}",
                p.id,
                pat
            );
        }
    }
}

#[test]
fn find_by_id_returns_correct_pattern() {
    for p in PATTERNS {
        let found = find_by_id(p.id).expect("find_by_id should find every pattern by its own id");
        assert_eq!(found.id, p.id);
    }
}

#[test]
fn find_by_id_returns_none_for_unknown() {
    assert!(find_by_id("does-not-exist").is_none());
    assert!(find_by_id("").is_none());
}

#[test]
fn filter_by_platform_returns_subset() {
    for &platform in &[
        Platform::MacosX64,
        Platform::MacosArm64,
        Platform::Linux64,
        Platform::Win64,
        Platform::Android,
    ] {
        let filtered: Vec<_> = filter_by_platform(platform).collect();
        assert!(
            !filtered.is_empty(),
            "no patterns for platform {}",
            platform
        );
        for p in &filtered {
            assert!(
                p.platforms.contains(&platform),
                "pattern '{}' returned for {} but doesn't list it",
                p.id,
                platform
            );
        }
    }
}

// ---------------------------------------------------------------------------
// diagnose: regex matching against log text
// ---------------------------------------------------------------------------

fn matching_ids(log: &str) -> Vec<&'static str> {
    PATTERNS
        .iter()
        .filter(|p| {
            p.error_patterns
                .iter()
                .any(|&pat| Regex::new(pat).unwrap().is_match(log))
        })
        .map(|p| p.id)
        .collect()
}

#[test]
fn diagnose_macos_rbe_action() {
    let log = "ERROR: sdk_inputs action output is outside root_build_dir";
    let ids = matching_ids(log);
    assert!(ids.contains(&"macos-rbe-action"), "got: {:?}", ids);
}

#[test]
fn diagnose_macos_clang_modules() {
    let log = "error: module 'DarwinFoundation' not found";
    let ids = matching_ids(log);
    assert!(ids.contains(&"macos-clang-modules"), "got: {:?}", ids);
}

#[test]
fn diagnose_macos_sdk_403() {
    let log = "Error: 403 Forbidden fetching MacOSX14.0.sdk";
    let ids = matching_ids(log);
    assert!(ids.contains(&"macos-sdk-version"), "got: {:?}", ids);
}

#[test]
fn diagnose_linux_missing_lib() {
    let log = "chrome: error while loading shared libraries: libxcb.so.1: cannot open shared object file";
    let ids = matching_ids(log);
    assert!(ids.contains(&"linux-missing-libs"), "got: {:?}", ids);
    // Should NOT fire code-cache-generator for a plain missing-lib message
    assert!(!ids.contains(&"code-cache-generator"), "got: {:?}", ids);
}

#[test]
fn diagnose_code_cache_generator_status_127() {
    let log = "FAILED: gen/v8_context_snapshot.bin\n./code_cache_generator exit status 127";
    let ids = matching_ids(log);
    assert!(ids.contains(&"code-cache-generator"), "got: {:?}", ids);
}

#[test]
fn diagnose_windows_msvc_redist() {
    let log = "msvcp140.dll: not found in search path";
    let ids = matching_ids(log);
    assert!(ids.contains(&"windows-msvc-redist"), "got: {:?}", ids);
}

#[test]
fn diagnose_build_timeout() {
    let log = "Task exceeded max-run-time of 25000 seconds";
    let ids = matching_ids(log);
    assert!(ids.contains(&"build-timeout"), "got: {:?}", ids);
}

#[test]
fn diagnose_python_version() {
    let log = "TypeError: 'type' object is not subscriptable";
    let ids = matching_ids(log);
    assert!(ids.contains(&"python-version"), "got: {:?}", ids);
}

#[test]
fn diagnose_android_gclient() {
    let log = "gclient sync failed: android NDK not found";
    let ids = matching_ids(log);
    assert!(ids.contains(&"android-gclient-sync"), "got: {:?}", ids);
}

// Backtest: real error snippets from historical bugs

#[test]
fn diagnose_depot_tools_cipd_bin_missing() {
    // Bug 1847210
    let log = "/builds/worker/custom_car/depot_tools/.cipd_bin/dirmd: No such file or directory\ngenerate_location_tags.py returned non-zero exit status 127";
    let ids = matching_ids(log);
    assert!(ids.contains(&"depot-tools-cipd"), "got: {:?}", ids);
}

#[test]
fn diagnose_depot_tools_permission_denied() {
    // Bug 1901936
    let log = "PermissionError: [Errno 13] Permission denied: '/builds/worker/.config/depot_tools'";
    let ids = matching_ids(log);
    assert!(ids.contains(&"depot-tools-cipd"), "got: {:?}", ids);
}

#[test]
fn diagnose_macos_sdk_undeclared_selector() {
    // Bug 1919962
    let log = "error: no known class method for selector 'frameResizeCursorFromPosition:inDirections:'";
    let ids = matching_ids(log);
    assert!(ids.contains(&"macos-sdk-version"), "got: {:?}", ids);
}

#[test]
fn diagnose_windows_dxil_dll_missing() {
    // Bug 1925145, 1928841
    let log = "ninja: error: 'D:/task/fetches/VS/Windows Kits/10/bin/10.0.26100.0/x64/dxil.dll', needed by 'dxil.dll', missing and no known rule to make it";
    let ids = matching_ids(log);
    assert!(ids.contains(&"windows-sdk-version"), "got: {:?}", ids);
}

#[test]
fn diagnose_macos_sdk_undeclared_cg_identifier() {
    // Bug 1989676
    let log = "error: use of undeclared identifier 'kCGImageByteOrder32Host'; did you mean 'kCGImageByteOrder32Big'?";
    let ids = matching_ids(log);
    assert!(ids.contains(&"macos-sdk-version"), "got: {:?}", ids);
}

#[test]
fn diagnose_linux_vulkan_crash_browsertime() {
    // Bug 2046664 - GPU crash as seen through browsertime
    let log = "BrowserError: invalid session id: session deleted as the browser has closed the connection\nBrowsertime process exited with code -9";
    let ids = matching_ids(log);
    assert!(ids.contains(&"linux-vulkan-crash"), "got: {:?}", ids);
}

#[test]
fn diagnose_git_lstree_format_error() {
    // Bug 1847919 - macOS git too old for --format flag
    let log = "Error: Command 'git ls-tree -r HEAD --format %(objectmode) %(objectname) %(path)' returned non-zero exit status 129";
    let ids = matching_ids(log);
    assert!(ids.contains(&"depot-tools-cipd"), "got: {:?}", ids);
}

#[test]
fn diagnose_no_match_for_unrelated_log() {
    let log = "Build succeeded. All tests passed.";
    let ids = matching_ids(log);
    assert!(ids.is_empty(), "unexpected matches: {:?}", ids);
}
