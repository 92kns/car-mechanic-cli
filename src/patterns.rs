use crate::types::{FixStep, Pattern, Platform};

pub static PATTERNS: &[Pattern] = &[
    // -------------------------------------------------------------------------
    // macOS
    // -------------------------------------------------------------------------
    Pattern {
        id: "macos-rbe-action",
        title: "macOS gn gen fails: sdk_inputs RBE action rejects external SDK path",
        platforms: &[Platform::MacosX64, Platform::MacosArm64],
        error_patterns: &[
            r"sdk_inputs",
            r"outside root_build_dir",
            r"build/config/mac/BUILD\.gn.*error",
        ],
        cause: "build/config/mac/BUILD.gn declares SDK files as outputs for Remote Build \
                Execution so remote workers can access them. GN validates this even when RBE \
                is not used, and rejects the action when mac_sdk_path points outside \
                root_build_dir (which it always does for our fetched SDK).",
        fix_steps: &[
            FixStep {
                description: "Patch the sdk_inputs guard in build/config/mac/BUILD.gn to never fire",
                command: Some(
                    r#"sed -i '' 's/if (use_system_xcode && current_toolchain == default_toolchain)/if (false)/' build/config/mac/BUILD.gn"#,
                ),
            },
            FixStep {
                description: "Verify the patch applied (command must produce no output)",
                command: Some(
                    r#"grep -q 'use_system_xcode && current_toolchain == default_toolchain' build/config/mac/BUILD.gn && echo "PATCH FAILED - upstream changed" && exit 1 || echo "OK""#,
                ),
            },
            FixStep {
                description: "The patch is applied inside build-custom-car.sh; check Bug 2045375 \
                               for the current implementation and update the sed expression if \
                               upstream refactored the guard condition",
                command: None,
            },
        ],
        bugs: &[2045375],
        upstream_files: &["build/config/mac/BUILD.gn"],
        search_queries: &[
            "cat build/config/mac/BUILD.gn",
            "sdk_inputs use_system_xcode",
        ],
    },
    Pattern {
        id: "macos-clang-modules",
        title: "macOS gn gen fails: DarwinFoundation.modulemap not found",
        platforms: &[Platform::MacosX64, Platform::MacosArm64],
        error_patterns: &[
            r"DarwinFoundation\.modulemap",
            r"module 'Darwin[^']*' not found",
            r"modulemap",
        ],
        cause: "Chromium C++ modules require modulemap files that only exist in a full \
                Xcode installation. Our fetched Command Line Tools SDK does not include them. \
                The fix is to disable clang modules entirely; the resulting binary is identical.",
        fix_steps: &[
            FixStep {
                description: "Add use_clang_modules=false to the GN arguments for the affected \
                               platform task in taskcluster/kinds/toolchain/misc.yml",
                command: None,
            },
            FixStep {
                description: "Verify it is already set (should be present for both macos tasks)",
                command: Some(
                    "grep -A 30 'macosx-custom-car:' taskcluster/kinds/toolchain/misc.yml | grep use_clang_modules",
                ),
            },
        ],
        bugs: &[2045375],
        upstream_files: &["build/config/mac/BUILD.gn"],
        search_queries: &["use_clang_modules"],
    },
    Pattern {
        id: "macos-sdk-version",
        title: "macOS build fails: SDK too old, 403, or SDK path not found",
        platforms: &[Platform::MacosX64, Platform::MacosArm64],
        error_patterns: &[
            r"403.*[Ss][Dd][Kk]",
            r"[Ss][Dd][Kk].*403",
            r"MacOSX\d+\.\d+\.sdk.*[Nn]ot [Ff]ound",
            r"No such file.*MacOSX\d+",
            r"mac_sdk_path.*not found",
            r"SDK.*[Uu]navailable",
            r"error: use of undeclared identifier.*(?:NS|kCG|CA)[A-Z]",
            r"error: no known class method for selector",
            r"error: use of undeclared identifier 'kCGImage",
        ],
        cause: "Google periodically bumps the minimum macOS SDK version in \
                build/config/mac/mac_sdk.gni. Our toolchain fetches a pinned SDK; when the \
                required version diverges, the build fails in one of two ways: (1) obvious path \
                errors (403, SDK not found), or (2) compile-time undeclared-identifier errors \
                because the fetched SDK is missing symbols added in newer OS versions (e.g. \
                NSCursorFrameResizePositionRight, kCGImageByteOrder32Host). Both require the \
                same fix: update to a newer SDK toolchain.",
        fix_steps: &[
            FixStep {
                description: "Check what SDK version Chromium now requires upstream",
                command: Some("car-mechanic search --cat build/config/mac/mac_sdk.gni"),
            },
            FixStep {
                description: "Check the current SDK version fetched in misc.yml",
                command: Some(
                    "grep -A 5 'macosx-custom-car:' taskcluster/kinds/toolchain/misc.yml | grep -i sdk",
                ),
            },
            FixStep {
                description: "If the versions differ, check if a newer MacOSX<N>.sdk toolchain \
                               artifact exists in-tree",
                command: Some(
                    "grep -r 'MacOSX.*sdk' taskcluster/kinds/toolchain/ | grep -v custom-car",
                ),
            },
            FixStep {
                description: "If the SDK does not exist yet, file a toolchain request with the \
                               build team (r?#firefox-build-system-reviewers) to add \
                               MacOSX<N>.sdk as a fetched artifact",
                command: None,
            },
            FixStep {
                description: "Update the fetches: block in misc.yml for both macosx-custom-car \
                               and macosx-arm64-custom-car tasks once the new SDK lands",
                command: None,
            },
        ],
        bugs: &[1919962, 1989676, 2006535, 2025209, 2038942],
        upstream_files: &[
            "build/config/mac/mac_sdk.gni",
            "build/mac_toolchain.py",
        ],
        search_queries: &[
            "cat build/config/mac/mac_sdk.gni",
            "mac_sdk_min_build_system_version",
        ],
    },
    // -------------------------------------------------------------------------
    // Windows
    // -------------------------------------------------------------------------
    Pattern {
        id: "windows-msvc-redist",
        title: "Windows build fails: MSVC Redistributable DLL not found or version mismatch",
        platforms: &[Platform::Win64],
        error_patterns: &[
            r"msvcp140.*not found",
            r"vcruntime.*not found",
            r"Microsoft\.VC.*\.CRT",
            r"MSVC.*[Rr]edist",
            r"VC\.Redist",
            r"msvcp\d+\.dll",
        ],
        cause: "Chromium's build scripts expect MSVC Redistributable DLLs at a specific path \
                derived from the redist version number. When upstream bumps the MSVC Redist \
                version our toolchain may not match, causing the DLL move logic in \
                build-custom-car.sh to fail or produce the wrong structure.",
        fix_steps: &[
            FixStep {
                description: "Check if the version detection in build-custom-car.sh is working: \
                               MSVC_REDIST_VERSION is dynamically detected via ls + sort -V",
                command: None,
            },
            FixStep {
                description: "Check upstream vs_toolchain.py for the expected MSVC Redist version",
                command: Some("car-mechanic search --cat build/vs_toolchain.py"),
            },
            FixStep {
                description: "If upstream recently bumped the Redist version and the toolchain \
                               has not caught up, temporarily revert win64-custom-car to the \
                               previous VS toolchain in misc.yml (see Bug 1928841 for precedent)",
                command: None,
            },
            FixStep {
                description: "Check if upstream itself rolled back (sometimes resolves within \
                               1-2 weeks without action on our side)",
                command: Some("car-mechanic risk --since 14 --platform win64"),
            },
        ],
        bugs: &[1925145, 1928841, 1986578, 2014501, 2039270],
        upstream_files: &[
            "build/vs_toolchain.py",
            "build/toolchain/win/setup_toolchain.py",
        ],
        search_queries: &[
            "cat build/vs_toolchain.py",
            "MSVC_REDIST_VERSION",
        ],
    },
    Pattern {
        id: "windows-sdk-version",
        title: "Windows build fails: Windows SDK version mismatch or DLL missing",
        platforms: &[Platform::Win64],
        error_patterns: &[
            r"SDK_VERSION",
            r"Windows Kits.*not found",
            r"winsdkver.*not found",
            r"10\.0\.\d+\.\d+.*not found",
            r"WINDOWSSDKDIR.*invalid",
            r"dxil\.dll.*missing",
            r"\.dll.*missing and no known rule to make it",
            r"ninja: error.*Windows Kits.*\.dll",
        ],
        cause: "The Windows SDK version is hardcoded in build/vs_toolchain.py and \
                build/toolchain/win/setup_toolchain.py. Our fetched VS toolchain pins a \
                specific SDK version; when upstream changes their pinned version, sed patches \
                in build-custom-car.sh override it. If the sed pattern changes upstream, the \
                patch silently fails.",
        fix_steps: &[
            FixStep {
                description: "Verify the SDK_VERSION sed patch in build-custom-car.sh still \
                               matches the current pattern in vs_toolchain.py",
                command: Some("car-mechanic search --cat build/vs_toolchain.py"),
            },
            FixStep {
                description: "Check what SDK_VERSION env var is set to in the CI environment",
                command: Some(
                    "grep -A 5 'win64-custom-car' taskcluster/kinds/toolchain/misc.yml | grep SDK_VERSION",
                ),
            },
            FixStep {
                description: "Update the sed expression in build-custom-car.sh if the regex \
                               pattern for SDK_VERSION changed upstream",
                command: None,
            },
        ],
        bugs: &[1925145, 1986578, 2039270],
        upstream_files: &[
            "build/vs_toolchain.py",
            "build/toolchain/win/setup_toolchain.py",
            "build/config/win/visual_studio_version.gni",
        ],
        search_queries: &[
            "cat build/vs_toolchain.py",
            "SDK_VERSION file:build/vs_toolchain.py",
        ],
    },
    Pattern {
        id: "windows-gyp-env",
        title: "Windows build fails: missing VS/GYP environment variables",
        platforms: &[Platform::Win64],
        error_patterns: &[
            r"GYP_MSVS_OVERRIDE_PATH",
            r"DEPOT_TOOLS_WIN_TOOLCHAIN",
            r"vs2022_install.*not set",
            r"vs2026_install.*not set",
            r"vcvarsall\.bat.*not found",
            r"Could not find.*Visual Studio",
        ],
        cause: "Chromium's Windows build requires a specific set of environment variables \
                pointing to Visual Studio paths. These are set explicitly in the Msys branch \
                of build-custom-car.sh; if a new variable is needed upstream or an existing \
                one is renamed, the build breaks.",
        fix_steps: &[
            FixStep {
                description: "Check the current env var setup in the Msys section of \
                               build-custom-car.sh",
                command: None,
            },
            FixStep {
                description: "Search upstream for any new variables referenced in the Windows toolchain",
                command: Some("car-mechanic search 'DEPOT_TOOLS_WIN_TOOLCHAIN file:build/'"),
            },
        ],
        bugs: &[1925145],
        upstream_files: &["build/vs_toolchain.py"],
        search_queries: &[
            "DEPOT_TOOLS_WIN_TOOLCHAIN file:build/",
            "GYP_MSVS_OVERRIDE_PATH",
        ],
    },
    Pattern {
        id: "windows-lastchange",
        title: "Windows build fails: missing LASTCHANGE file",
        platforms: &[Platform::Win64],
        error_patterns: &[
            r"LASTCHANGE.*not found",
            r"build/util/LASTCHANGE",
            r"lastchange\.py.*error",
        ],
        cause: "Because we fetch without history (--no-history), git-based version info \
                is unavailable. build-custom-car.sh generates a dummy LASTCHANGE via \
                build/util/lastchange.py. If upstream moves or renames this script, the \
                dummy generation breaks.",
        fix_steps: &[
            FixStep {
                description: "Verify the LASTCHANGE generation step in build-custom-car.sh \
                               still points to the correct script path",
                command: Some("car-mechanic search --cat build/util/lastchange.py"),
            },
        ],
        bugs: &[],
        upstream_files: &["build/util/lastchange.py"],
        search_queries: &["cat build/util/lastchange.py"],
    },
    // -------------------------------------------------------------------------
    // Linux
    // -------------------------------------------------------------------------
    Pattern {
        id: "linux-vulkan-crash",
        title: "Linux Chrome crashes on launch: Vulkan/ANGLE/GPU failure on Intel CI workers",
        platforms: &[Platform::Linux64],
        error_patterns: &[
            r"(?i)vulkan.*error",
            r"(?i)EGL_BAD_ACCESS",
            r"(?i)libvulkan.*not found",
            r"(?i)GPU process.*crash",
            r"(?i)VK_ERROR",
            r"(?i)ANGLE.*Vulkan",
            r"(?i)FATAL.*gpu",
            r"(?i)gpu.*FATAL",
            r"(?i)vkCreateInstance.*failed",
            // Browsertime-visible symptoms of Chrome crashing (GPU process killed)
            r"session deleted as the browser has closed the connection",
            r"not connected to DevTools",
            r"Browsertime process exited with code -9",
            r"BrowserError.*custom-car",
        ],
        cause: "Chrome defaults to the Vulkan rendering path on Ubuntu 24.04. CI workers \
                use Intel Skylake/Coffeelake GPUs whose drivers do not support Vulkan reliably \
                in the containerized CI environment. The crash manifests either as GPU process \
                FATAL log lines or, when seen through browsertime, as 'session deleted / not \
                connected to DevTools' with exit code -9.",
        fix_steps: &[
            FixStep {
                description: "Add --use-angle=gl-egl to Chrome launch flags to use ANGLE over \
                               EGL/native OpenGL instead of the Vulkan path",
                command: None,
            },
            FixStep {
                description: "If --use-angle=gl-egl still crashes, try --use-gl=desktop \
                               to force native GLX",
                command: None,
            },
            FixStep {
                description: "Last resort: --disable-gpu (disables hardware acceleration entirely)",
                command: None,
            },
            FixStep {
                description: "Also check for /dev/shm exhaustion; add --disable-dev-shm-usage \
                               if crashes are intermittent",
                command: None,
            },
            FixStep {
                description: "Enable verbose GPU logging to narrow down the failure point: \
                               --enable-logging --v=1 --log-file=/tmp/chrome-gpu.log",
                command: None,
            },
        ],
        bugs: &[2046664],
        upstream_files: &[],
        search_queries: &[
            "file:content/gpu/ VulkanImplementation",
            "use_angle lang:cpp file:content/",
        ],
    },
    Pattern {
        id: "linux-missing-libs",
        title: "Linux Chrome crashes or fails to start: missing shared libraries in Docker",
        platforms: &[Platform::Linux64, Platform::Android],
        error_patterns: &[
            r"cannot open shared object",
            r"error while loading shared libraries",
            r"libxcb.*not found",
            r"libdbus.*not found",
            r"libgtk.*not found",
            r"libgbm.*not found",
            r"libegl.*not found",
            r"libdrm.*not found",
            r"libnspr.*not found",
            r"libnss.*not found",
            r"libxcomposite.*not found",
        ],
        cause: "The custom-car-linux Docker image must explicitly list all runtime libraries \
                Chrome needs. When Chromium adds new library dependencies or we switch Ubuntu \
                base versions, the image needs updating.",
        fix_steps: &[
            FixStep {
                description: "Run ldd on the Chrome binary to identify all missing libraries",
                command: Some("ldd chrome 2>&1 | grep 'not found'"),
            },
            FixStep {
                description: "Add the missing packages to the custom-car-linux Docker image \
                               (taskcluster/docker/custom-car-linux/Dockerfile or equivalent)",
                command: None,
            },
            FixStep {
                description: "Check if install-build-deps.py would have installed the missing \
                               package",
                command: Some("car-mechanic search --cat build/install-build-deps.py"),
            },
        ],
        bugs: &[1989677, 2027893],
        upstream_files: &["build/install-build-deps.py"],
        search_queries: &[
            "cat build/install-build-deps.py",
            "file:build/ apt-get install",
        ],
    },
    Pattern {
        id: "linux-install-build-deps",
        title: "Linux build fails: install-build-deps script changed upstream",
        platforms: &[Platform::Linux64, Platform::Android],
        error_patterns: &[
            r"install-build-deps.*failed",
            r"Failed to install.*deps",
            r"install_build_deps.*error",
            r"apt-get.*install.*failed",
        ],
        cause: "Google migrated from install-build-deps.sh to install-build-deps.py and \
                continues to evolve its dependency list. When upstream changes the script's \
                interface or adds required packages, our Docker-based build environment breaks.",
        fix_steps: &[
            FixStep {
                description: "Check recent changes to install-build-deps.py upstream",
                command: Some("car-mechanic risk --since 30 --platform linux64"),
            },
            FixStep {
                description: "Compare the package list in the script with what the Docker \
                               image provides",
                command: Some("car-mechanic search --cat build/install-build-deps.py"),
            },
            FixStep {
                description: "Add the newly required packages directly to the Docker image \
                               rather than running the script (avoids sudo requirement)",
                command: None,
            },
        ],
        bugs: &[1847210],
        upstream_files: &["build/install-build-deps.py"],
        search_queries: &["cat build/install-build-deps.py"],
    },
    // -------------------------------------------------------------------------
    // Android
    // -------------------------------------------------------------------------
    Pattern {
        id: "android-gclient-sync",
        title: "Android gclient sync fails after cipd or depot_tools change",
        platforms: &[Platform::Android],
        error_patterns: &[
            r"gclient sync.*failed",
            r"(?i)android.*target_os.*error",
            r"Failed to fetch.*android",
            r"cipd.*android",
            r"NDK.*not found",
            r"android.*NDK",
        ],
        cause: "Android CaR builds require a gclient sync pass after adding \
                'target_os = [\"android\"]' to .gclient. Changes to cipd manifests, \
                NDK versions, or depot_tools sync behavior can break this step. \
                Because Android requires a Linux host, Linux-side issues also apply.",
        fix_steps: &[
            FixStep {
                description: "Check cipd_manifest.txt and Android config for recent NDK/SDK bumps",
                command: Some("car-mechanic risk --since 14 --platform android"),
            },
            FixStep {
                description: "Verify build/config/android/config.gni for NDK version changes",
                command: Some("car-mechanic search --cat build/config/android/config.gni"),
            },
            FixStep {
                description: "Also check linux-missing-libs and linux-install-build-deps \
                               patterns — Android builds on a Linux host so those apply too",
                command: None,
            },
        ],
        bugs: &[1847919, 1903568],
        upstream_files: &[
            "build/config/android/config.gni",
            "DEPS",
        ],
        search_queries: &[
            "cat build/config/android/config.gni",
            "android_ndk_version",
        ],
    },
    Pattern {
        id: "android-symbols",
        title: "Android symbols artifact packaging fails",
        platforms: &[Platform::Android],
        error_patterns: &[
            r"lib\.unstripped.*not found",
            r"symbols.*artifact.*failed",
            r"SYM_DIR.*not found",
            r"car_android_symbols",
        ],
        cause: "The Android CaR build packages a separate symbols artifact from \
                src/out/Default/lib.unstripped. If symbol_level is not 2 or the output \
                path changes, the symbols packaging step fails.",
        fix_steps: &[
            FixStep {
                description: "Verify symbol_level=2 is set in android-custom-car in misc.yml",
                command: Some(
                    "grep -A 30 'android-custom-car:' taskcluster/kinds/toolchain/misc.yml | grep symbol_level",
                ),
            },
            FixStep {
                description: "Check if lib.unstripped was produced in src/out/Default/",
                command: None,
            },
        ],
        bugs: &[1999317],
        upstream_files: &[],
        search_queries: &["lib.unstripped file:build/"],
    },
    // -------------------------------------------------------------------------
    // Cross-platform
    // -------------------------------------------------------------------------
    Pattern {
        id: "build-timeout",
        title: "CaR build exceeds max-run-time",
        platforms: &[
            Platform::MacosX64,
            Platform::MacosArm64,
            Platform::Linux64,
            Platform::Win64,
            Platform::Android,
        ],
        error_patterns: &[
            r"(?i)exceeded.*max.run.time",
            r"(?i)task.*timed out",
            r"(?i)build.*timeout",
            r"(?i)autoninja.*killed",
            r"max_run_time",
        ],
        cause: "Chromium's codebase grows continuously. CaR build times trend upward \
                independently of any single commit. This is a maintenance tax, not a code \
                regression. Historical values: linux 25000s, android 30000s, macos 15000s, \
                win64 10000s.",
        fix_steps: &[
            FixStep {
                description: "Find the failing platform's current max-run-time",
                command: Some(
                    "grep -A 5 'custom-car' taskcluster/kinds/toolchain/misc.yml | grep max-run-time",
                ),
            },
            FixStep {
                description: "Bump max-run-time by ~20-30% for the affected platform in misc.yml",
                command: None,
            },
        ],
        bugs: &[1930319, 1939792, 1947911, 1976130],
        upstream_files: &[],
        search_queries: &[],
    },
    Pattern {
        id: "python-version",
        title: "Build script fails: Python version too old (PEP 585 type hints require 3.9+)",
        platforms: &[
            Platform::MacosX64,
            Platform::MacosArm64,
            Platform::Linux64,
            Platform::Win64,
            Platform::Android,
        ],
        error_patterns: &[
            r"SyntaxError.*list\[",
            r"TypeError.*subscript",
            r"requires Python 3\.[89]",
            r"python.*3\.[0-8]\b",
            r"TypeError.*'type' object is not subscriptable",
        ],
        cause: "Chromium build scripts use PEP 585 generic type hints (e.g. list[str]) \
                which require Python 3.9+. If a CI task's Python is not explicitly pinned \
                to 3.11, an older worker Python may be used.",
        fix_steps: &[
            FixStep {
                description: "Verify use-python: \"3.11\" is set for all five CaR tasks in misc.yml",
                command: Some(
                    "grep -B 2 'build-custom-car.sh' taskcluster/kinds/toolchain/misc.yml | grep use-python",
                ),
            },
            FixStep {
                description: "If missing, add use-python: \"3.11\" to the affected task",
                command: None,
            },
        ],
        bugs: &[1955729],
        upstream_files: &[".vpython3"],
        search_queries: &["file:build/ list[str] lang:python"],
    },
    Pattern {
        id: "depot-tools-cipd",
        title: "cipd or depot_tools path/environment setup fails",
        platforms: &[
            Platform::MacosX64,
            Platform::MacosArm64,
            Platform::Linux64,
            Platform::Android,
        ],
        error_patterns: &[
            r"cipd.*failed",
            r"cipd.*error",
            r"depot_tools.*not found",
            r"XDG_CONFIG_HOME",
            r"gclient.*config.*error",
            r"cipd_bin_setup",
            // Bug 1847210: cipd binary missing after depot_tools change
            r"\.cipd_bin.*No such file or directory",
            r"generate_location_tags.*exit status 127",
            // Bug 1901936: depot_tools writes to $HOME/.config which is not writable in CI
            r"PermissionError.*depot_tools",
            r"Permission denied.*\.config/depot_tools",
            // Bug 1847919: macOS workers had git 2.27 which lacks --format flag
            r"git ls-tree.*exit status 12[0-9]",
            r"git.*returned non-zero exit status 129",
        ],
        cause: "depot_tools environment setup can break in several ways: (1) cipd binary \
                missing after upstream depot_tools changes its bootstrap path \
                (Bug 1847210: .cipd_bin/dirmd not found); (2) depot_tools tries to write \
                config to $HOME/.config which CI workers cannot write to — fix is to set \
                XDG_CONFIG_HOME to a writable directory (Bug 1901936); (3) macOS workers \
                running old git (< 2.28) lack the --format flag used by gclient \
                (Bug 1847919 — resolved upstream in depot_tools, just retry).",
        fix_steps: &[
            FixStep {
                description: "Verify build-custom-car.sh sources cipd_bin_setup.sh for Linux/macOS",
                command: None,
            },
            FixStep {
                description: "For Permission denied on .config/depot_tools: verify XDG_CONFIG_HOME \
                               is set to CUSTOM_CAR_DIR in the Linux/Android section of \
                               build-custom-car.sh",
                command: None,
            },
            FixStep {
                description: "For .cipd_bin missing: check recent depot_tools changes for \
                               cipd bootstrap path changes",
                command: Some("car-mechanic search --repo depot_tools cipd"),
            },
            FixStep {
                description: "For git ls-tree exit 129 on macOS: this is a git version issue \
                               fixed upstream in depot_tools — retry the task",
                command: None,
            },
        ],
        bugs: &[1901936, 1847210, 1847919],
        upstream_files: &[],
        search_queries: &[
            "cipd_bin_setup file:third_party/depot_tools/",
            "XDG_CONFIG_HOME",
        ],
    },
    Pattern {
        id: "pgo-profdata-missing",
        title: "PGO profdata file not found for platform",
        platforms: &[
            Platform::MacosX64,
            Platform::MacosArm64,
            Platform::Linux64,
            Platform::Win64,
            Platform::Android,
        ],
        error_patterns: &[
            r"PGO_DATA_PATH.*empty",
            r"pgo_profiles.*missing",
            r"profdata.*not found",
            r"chrome-.*-main.*not found",
        ],
        cause: "PGO profile data is downloaded during gclient runhooks and matched by a \
                platform-specific substring (PGO_SUBSTR). If Chromium renames the profdata \
                file format or the PGO_SUBSTR in build-custom-car.sh no longer matches, \
                PGO_DATA_PATH is never set and gn gen fails.",
        fix_steps: &[
            FixStep {
                description: "List what profdata files were actually downloaded",
                command: Some("ls -la src/chrome/build/pgo_profiles/"),
            },
            FixStep {
                description: "Compare the actual filenames against the PGO_SUBSTR values in \
                               build-custom-car.sh: chrome-linux-main, chrome-mac-main, \
                               chrome-mac-arm-main, chrome-win64-main, android64",
                command: None,
            },
            FixStep {
                description: "Update PGO_SUBSTR for the affected platform if the naming changed",
                command: Some("car-mechanic search 'pgo_profiles file:chrome/build/'"),
            },
        ],
        bugs: &[],
        upstream_files: &["chrome/build/"],
        search_queries: &["pgo_profiles file:chrome/build/"],
    },
    Pattern {
        id: "code-cache-generator",
        title: "Build failure: code_cache_generator exits with status 127 or V8 snapshot error",
        platforms: &[Platform::MacosX64, Platform::MacosArm64, Platform::Linux64],
        error_patterns: &[
            r"code_cache_generator.*failed",
            r"code_cache_generator.*FAILED",
            r"code_cache_generator.*exit.*127",
            r"code_cache_generator.*status 127",
            r"v8_context_snapshot.*failed",
            r"FAILED.*code_cache",
            r"snapshot_blob",
        ],
        cause: "On Linux, exit status 127 from code_cache_generator means a required shared \
                library is missing at runtime (the binary was built but can't load). This is \
                the same root cause as linux-missing-libs — check the Docker image. \
                On all platforms, this can also fail intermittently due to resource constraints \
                or upstream V8 changes. Setting use_v8_context_snapshot=false disables \
                snapshot generation entirely.",
        fix_steps: &[
            FixStep {
                description: "Verify use_v8_context_snapshot=false is set in GN args for \
                               the failing platform in misc.yml",
                command: Some(
                    "grep -A 30 'custom-car' taskcluster/kinds/toolchain/misc.yml | grep v8_context_snapshot",
                ),
            },
            FixStep {
                description: "If exit status 127 on Linux: a shared library is missing at \
                               runtime — run ldd on the chrome binary and follow the \
                               linux-missing-libs pattern",
                command: Some("ldd src/out/Default/code_cache_generator 2>&1 | grep 'not found'"),
            },
            FixStep {
                description: "Check recent V8 upstream changes",
                command: Some("car-mechanic risk --since 7 --platform linux64"),
            },
        ],
        bugs: &[2021140],
        upstream_files: &[],
        search_queries: &[
            "code_cache_generator file:tools/v8_context_snapshot/",
            "use_v8_context_snapshot",
        ],
    },
    Pattern {
        id: "fetch-network-error",
        title: "fetch --no-history fails: network error reaching googlesource.com",
        platforms: &[
            Platform::MacosX64,
            Platform::MacosArm64,
            Platform::Linux64,
            Platform::Win64,
            Platform::Android,
        ],
        error_patterns: &[
            r"fetch.*failed",
            r"googlesource\.com.*error",
            r"Connection.*refused.*googlesource",
            r"HTTP 5\d\d.*googlesource",
            r"sparse.*checkout.*failed",
        ],
        cause: "Intermittent network issues reaching chromium.googlesource.com from CI \
                workers. There is also a known sparse checkout bug (Bug 1539681) that \
                triggers on try and occasionally production branches. Usually self-resolves \
                on retry.",
        fix_steps: &[
            FixStep {
                description: "Retry the task — network errors are almost always transient",
                command: None,
            },
            FixStep {
                description: "If the sparse checkout error recurs frequently, check \
                               https://bugzilla.mozilla.org/show_bug.cgi?id=1539681 for \
                               any available workaround",
                command: None,
            },
        ],
        bugs: &[],
        upstream_files: &[],
        search_queries: &[],
    },
];

pub fn find_by_id(id: &str) -> Option<&'static Pattern> {
    PATTERNS.iter().find(|p| p.id == id)
}

pub fn filter_by_platform(platform: Platform) -> impl Iterator<Item = &'static Pattern> {
    PATTERNS.iter().filter(move |p| p.platforms.contains(&platform))
}
