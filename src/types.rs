use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacosX64,
    MacosArm64,
    Linux64,
    Win64,
    Android,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::MacosX64 => "macos-x64",
            Platform::MacosArm64 => "macos-arm64",
            Platform::Linux64 => "linux64",
            Platform::Win64 => "win64",
            Platform::Android => "android",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "macos-x64" | "macos" | "mac" | "osx" | "macosx" => Some(Platform::MacosX64),
            "macos-arm64" | "macos-arm" | "mac-arm" | "arm64" => Some(Platform::MacosArm64),
            "linux64" | "linux" => Some(Platform::Linux64),
            "win64" | "windows" | "win" => Some(Platform::Win64),
            "android" => Some(Platform::Android),
            _ => None,
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for Platform {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FixStep {
    pub description: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pattern {
    pub id: &'static str,
    pub title: &'static str,
    pub platforms: &'static [Platform],
    /// Regex strings matched against the full log text (any match = pattern fires)
    pub error_patterns: &'static [&'static str],
    pub cause: &'static str,
    pub fix_steps: &'static [FixStep],
    pub bugs: &'static [u32],
    /// Upstream Chromium/depot_tools files to inspect when diagnosing
    pub upstream_files: &'static [&'static str],
    /// Suggested chromium-search queries for deeper investigation
    pub search_queries: &'static [&'static str],
}

#[derive(Debug, Serialize)]
pub struct DiagnoseMatch<'a> {
    pub pattern: &'a Pattern,
    pub matched_on: Vec<String>,
}
