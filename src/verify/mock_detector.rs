//! Mock data and shortcut detection for GoGoGadget
//!
//! Detects common anti-patterns that indicate incomplete or lazy implementations:
//! - TODO/FIXME comments left in production code
//! - Placeholder data like "John Doe", "example.com"
//! - Unimplemented macros (unimplemented!(), todo!())
//! - Debug statements in non-test code
//! - Hardcoded secrets and credentials
//! - Empty implementations
//! - Mock/stub/fake patterns
//! - Banned patterns from CLAUDE.md anti-laziness config
//!
//! ## Integration with Verification
//!
//! The mock detector integrates with the verification module to provide
//! anti-laziness enforcement. When enabled in Config, the verification
//! process will fail if error-severity patterns are detected.

use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Result of mock data detection scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockDataResult {
    /// Patterns found during scan
    pub patterns_found: Vec<MockPattern>,
    /// Whether the codebase is clean (no error-severity patterns)
    pub is_clean: bool,
    /// Total files scanned
    pub files_scanned: usize,
    /// Files with issues
    pub files_with_issues: usize,
    /// Summary of detection by severity
    pub summary: MockDetectionSummary,
}

/// Summary of detection results by severity
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MockDetectionSummary {
    /// Number of error-level patterns found
    pub error_count: usize,
    /// Number of warning-level patterns found
    pub warning_count: usize,
    /// Breakdown by pattern type
    pub by_type: std::collections::HashMap<String, usize>,
}

/// A single mock pattern detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockPattern {
    /// Type of pattern detected
    pub pattern_type: MockPatternType,
    /// Files where this pattern was found
    pub files_affected: Vec<PathBuf>,
    /// Example matches (limited to first few)
    pub examples: Vec<String>,
    /// Severity of this pattern
    pub severity: Severity,
    /// Line numbers where found (if available)
    pub line_numbers: Vec<usize>,
    /// Suggested fix
    pub suggested_fix: Option<String>,
}

impl MockPattern {
    /// Create a new mock pattern
    pub fn new(pattern_type: MockPatternType, severity: Severity) -> Self {
        Self {
            pattern_type,
            files_affected: Vec::new(),
            examples: Vec::new(),
            severity,
            line_numbers: Vec::new(),
            suggested_fix: None,
        }
    }

    /// Add a file to the affected files list
    pub fn add_file(&mut self, path: PathBuf) {
        if !self.files_affected.contains(&path) {
            self.files_affected.push(path);
        }
    }

    /// Add an example match
    pub fn add_example(&mut self, example: impl Into<String>, max_examples: usize) {
        if self.examples.len() < max_examples {
            self.examples.push(example.into());
        }
    }

    /// Add a line number
    pub fn add_line_number(&mut self, line_num: usize) {
        self.line_numbers.push(line_num);
    }

    /// Set suggested fix
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggested_fix = Some(suggestion.into());
        self
    }
}

/// Types of mock data patterns detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MockPatternType {
    /// TODO, FIXME, XXX comments
    TodoComments,
    /// Placeholder data like "John Doe", "example.com"
    PlaceholderData,
    /// Rust unimplemented!() or todo!() macros
    UnimplementedMacros,
    /// Debug statements (println!, dbg!) in production code
    DebugStatements,
    /// Hardcoded secrets or credentials
    HardcodedSecrets,
    /// Empty function implementations
    EmptyImplementations,
    /// Mock/stub/fake patterns
    MockStubFake,
    /// Lorem ipsum or placeholder text
    LoremIpsum,
    /// Hardcoded test values (foo, bar, baz, qux)
    HardcodedTestValues,
    /// Console.log in production TypeScript/JavaScript
    ConsoleStatements,
    /// Commented-out code blocks
    CommentedCode,
    /// Custom pattern from configuration
    Custom(String),
}

impl std::fmt::Display for MockPatternType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TodoComments => write!(f, "TODO/FIXME comments"),
            Self::PlaceholderData => write!(f, "Placeholder data"),
            Self::UnimplementedMacros => write!(f, "Unimplemented macros"),
            Self::DebugStatements => write!(f, "Debug statements"),
            Self::HardcodedSecrets => write!(f, "Hardcoded secrets"),
            Self::EmptyImplementations => write!(f, "Empty implementations"),
            Self::MockStubFake => write!(f, "Mock/stub/fake patterns"),
            Self::LoremIpsum => write!(f, "Lorem ipsum text"),
            Self::HardcodedTestValues => write!(f, "Hardcoded test values"),
            Self::ConsoleStatements => write!(f, "Console statements"),
            Self::CommentedCode => write!(f, "Commented-out code"),
            Self::Custom(name) => write!(f, "Custom: {}", name),
        }
    }
}

/// Severity level for pattern detection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    /// Must be fixed before completion
    Error,
    /// Should be reviewed but may be acceptable
    Warning,
    /// Informational only
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "ERROR"),
            Self::Warning => write!(f, "WARNING"),
            Self::Info => write!(f, "INFO"),
        }
    }
}

/// Configuration for mock detection
#[derive(Debug, Clone)]
pub struct MockDetectorConfig {
    /// Additional custom patterns to detect
    pub custom_patterns: Vec<CustomPattern>,
    /// Patterns to ignore
    pub ignore_patterns: HashSet<String>,
    /// Maximum examples to collect per pattern
    pub max_examples: usize,
    /// Whether to check debug statements
    pub check_debug_statements: bool,
    /// Whether to check console.log statements
    pub check_console_statements: bool,
    /// File extensions to scan
    pub extensions: HashSet<String>,
    /// Directories to exclude
    pub excluded_dirs: HashSet<String>,
}

impl Default for MockDetectorConfig {
    fn default() -> Self {
        let mut extensions = HashSet::new();
        for ext in ["rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "c", "cpp", "h", "hpp"] {
            extensions.insert(ext.to_string());
        }

        let mut excluded_dirs = HashSet::new();
        for dir in ["target", "node_modules", "dist", "build", "vendor", "__pycache__", ".git"] {
            excluded_dirs.insert(dir.to_string());
        }

        Self {
            custom_patterns: Vec::new(),
            ignore_patterns: HashSet::new(),
            max_examples: 5,
            check_debug_statements: true,
            check_console_statements: true,
            extensions,
            excluded_dirs,
        }
    }
}

/// Custom pattern definition
#[derive(Debug, Clone)]
pub struct CustomPattern {
    /// Pattern name
    pub name: String,
    /// Regex pattern
    pub regex: Regex,
    /// Severity level
    pub severity: Severity,
    /// Suggested fix
    pub suggestion: Option<String>,
}

lazy_static! {
    static ref TODO_REGEX: Regex =
        Regex::new(r"(?i)(TODO|FIXME|XXX|HACK)\s*:?").expect("Invalid TODO regex");

    static ref PLACEHOLDER_REGEX: Regex = Regex::new(
        r"(?i)(john\s+doe|jane\s+doe|test@example\.com|example\.com|placeholder|user@example|test@test\.com|foo@bar\.com)"
    )
    .expect("Invalid placeholder regex");

    static ref UNIMPLEMENTED_REGEX: Regex =
        Regex::new(r"(unimplemented!\s*\(\)|todo!\s*\(\)|panic!\s*\([^)]*not\s+implemented)").expect("Invalid unimplemented regex");

    static ref DEBUG_REGEX: Regex =
        Regex::new(r"(println!\s*\([^)]*debug|dbg!\s*\()").expect("Invalid debug regex");

    static ref SECRET_REGEX: Regex = Regex::new(
        r#"(?i)(password|api_key|secret|token|credential|auth_token|private_key)\s*[:=]\s*["'][^"']{8,}["']"#
    )
    .expect("Invalid secret regex");

    static ref EMPTY_IMPL_REGEX: Regex =
        Regex::new(r"fn\s+\w+[^{]*\{\s*(//[^\n]*)?\s*\}").expect("Invalid empty impl regex");

    static ref MOCK_STUB_REGEX: Regex =
        Regex::new(r#"(?i)["']?(mock|stub|fake|dummy)["']?\s*[:=]|MockData|StubResponse|FakeService"#).expect("Invalid mock/stub regex");

    static ref LOREM_REGEX: Regex =
        Regex::new(r"(?i)lorem\s+ipsum|dolor\s+sit\s+amet").expect("Invalid lorem regex");

    static ref TEST_VALUES_REGEX: Regex =
        Regex::new(r#"(?i)["'](foo|bar|baz|qux|test123|asdf|1234567|password123)["']"#).expect("Invalid test values regex");

    static ref CONSOLE_REGEX: Regex =
        Regex::new(r"console\.(log|debug|info|warn|error)\s*\(").expect("Invalid console regex");

    static ref COMMENTED_CODE_REGEX: Regex =
        Regex::new(r"//\s*(fn |let |const |var |if |for |while |return |import |export |class |struct )").expect("Invalid commented code regex");
}

/// Detect mock data patterns in the given working directory
pub fn detect_mock_data(working_dir: &Path) -> Result<MockDataResult> {
    detect_mock_data_with_config(working_dir, &MockDetectorConfig::default())
}

/// Detect mock data patterns with custom configuration
pub fn detect_mock_data_with_config(working_dir: &Path, config: &MockDetectorConfig) -> Result<MockDataResult> {
    let mut patterns: Vec<MockPattern> = Vec::new();
    let mut files_scanned = 0;
    let mut files_with_issues_set: HashSet<PathBuf> = HashSet::new();

    let source_files = find_source_files(working_dir, config)?;

    for file_path in source_files {
        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue, // Skip files that can't be read
        };

        files_scanned += 1;
        let is_test_file = is_test_file(&file_path);
        let file_ext = file_path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // Skip test files for most checks - tests are allowed to have TODOs etc.
        if !is_test_file {
            // Check 1: TODO/FIXME comments
            if TODO_REGEX.is_match(&content) && !config.ignore_patterns.contains("todo") {
                let mut pattern = MockPattern::new(MockPatternType::TodoComments, Severity::Error)
                    .with_suggestion("Remove or address all TODO/FIXME comments before production");
                pattern.add_file(file_path.clone());
                add_pattern_examples(&content, &TODO_REGEX, &mut pattern, config.max_examples);
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 2: Placeholder data
            if PLACEHOLDER_REGEX.is_match(&content) && !config.ignore_patterns.contains("placeholder") {
                let mut pattern = MockPattern::new(MockPatternType::PlaceholderData, Severity::Error)
                    .with_suggestion("Replace placeholder data with real values or configuration");
                pattern.add_file(file_path.clone());
                add_pattern_examples(&content, &PLACEHOLDER_REGEX, &mut pattern, config.max_examples);
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 3: Unimplemented macros (Rust only)
            if file_ext == "rs" && UNIMPLEMENTED_REGEX.is_match(&content)
                && !config.ignore_patterns.contains("unimplemented") {
                let mut pattern = MockPattern::new(MockPatternType::UnimplementedMacros, Severity::Error)
                    .with_suggestion("Implement the function or remove the placeholder");
                pattern.add_file(file_path.clone());
                add_pattern_examples(&content, &UNIMPLEMENTED_REGEX, &mut pattern, config.max_examples);
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 4: Debug statements (warning only, Rust)
            if file_ext == "rs" && config.check_debug_statements && DEBUG_REGEX.is_match(&content)
                && !config.ignore_patterns.contains("debug") {
                let mut pattern = MockPattern::new(MockPatternType::DebugStatements, Severity::Warning)
                    .with_suggestion("Remove debug statements before production");
                pattern.add_file(file_path.clone());
                add_pattern_examples(&content, &DEBUG_REGEX, &mut pattern, config.max_examples);
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 5: Console statements (TypeScript/JavaScript)
            if matches!(file_ext.as_str(), "ts" | "tsx" | "js" | "jsx")
                && config.check_console_statements
                && CONSOLE_REGEX.is_match(&content)
                && !config.ignore_patterns.contains("console") {
                let mut pattern = MockPattern::new(MockPatternType::ConsoleStatements, Severity::Warning)
                    .with_suggestion("Remove console.log statements before production");
                pattern.add_file(file_path.clone());
                add_pattern_examples(&content, &CONSOLE_REGEX, &mut pattern, config.max_examples);
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 6: Hardcoded secrets
            if SECRET_REGEX.is_match(&content) && !config.ignore_patterns.contains("secrets") {
                let mut pattern = MockPattern::new(MockPatternType::HardcodedSecrets, Severity::Error)
                    .with_suggestion("Use environment variables or a secrets manager");
                pattern.add_file(file_path.clone());
                // Don't expose secrets in examples
                pattern.examples = vec!["[REDACTED - potential secret found]".to_string()];
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 7: Empty implementations (Rust only, warning level)
            if file_ext == "rs" && EMPTY_IMPL_REGEX.is_match(&content)
                && !config.ignore_patterns.contains("empty") {
                let mut pattern = MockPattern::new(MockPatternType::EmptyImplementations, Severity::Warning)
                    .with_suggestion("Implement the function body or mark as unimplemented if intentional");
                pattern.add_file(file_path.clone());
                add_pattern_examples(&content, &EMPTY_IMPL_REGEX, &mut pattern, config.max_examples);
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 8: Mock/stub/fake patterns
            if MOCK_STUB_REGEX.is_match(&content) && !config.ignore_patterns.contains("mock") {
                let mut pattern = MockPattern::new(MockPatternType::MockStubFake, Severity::Error)
                    .with_suggestion("Replace mock/stub/fake implementations with real code");
                pattern.add_file(file_path.clone());
                add_pattern_examples(&content, &MOCK_STUB_REGEX, &mut pattern, config.max_examples);
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 9: Lorem ipsum text
            if LOREM_REGEX.is_match(&content) && !config.ignore_patterns.contains("lorem") {
                let mut pattern = MockPattern::new(MockPatternType::LoremIpsum, Severity::Error)
                    .with_suggestion("Replace placeholder text with actual content");
                pattern.add_file(file_path.clone());
                add_pattern_examples(&content, &LOREM_REGEX, &mut pattern, config.max_examples);
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 10: Hardcoded test values
            if TEST_VALUES_REGEX.is_match(&content) && !config.ignore_patterns.contains("testvalues") {
                let mut pattern = MockPattern::new(MockPatternType::HardcodedTestValues, Severity::Warning)
                    .with_suggestion("Replace test values with meaningful data or configuration");
                pattern.add_file(file_path.clone());
                add_pattern_examples(&content, &TEST_VALUES_REGEX, &mut pattern, config.max_examples);
                patterns.push(pattern);
                files_with_issues_set.insert(file_path.clone());
            }

            // Check 11: Commented-out code
            if COMMENTED_CODE_REGEX.is_match(&content) && !config.ignore_patterns.contains("commented") {
                let matches_count = COMMENTED_CODE_REGEX.find_iter(&content).count();
                // Only flag if there are multiple commented-out code blocks
                if matches_count >= 3 {
                    let mut pattern = MockPattern::new(MockPatternType::CommentedCode, Severity::Warning)
                        .with_suggestion("Remove commented-out code blocks");
                    pattern.add_file(file_path.clone());
                    add_pattern_examples(&content, &COMMENTED_CODE_REGEX, &mut pattern, config.max_examples);
                    patterns.push(pattern);
                    files_with_issues_set.insert(file_path.clone());
                }
            }

            // Check custom patterns
            for custom in &config.custom_patterns {
                if custom.regex.is_match(&content) {
                    let mut pattern = MockPattern::new(
                        MockPatternType::Custom(custom.name.clone()),
                        custom.severity.clone(),
                    );
                    if let Some(ref suggestion) = custom.suggestion {
                        pattern.suggested_fix = Some(suggestion.clone());
                    }
                    pattern.add_file(file_path.clone());
                    add_pattern_examples(&content, &custom.regex, &mut pattern, config.max_examples);
                    patterns.push(pattern);
                    files_with_issues_set.insert(file_path.clone());
                }
            }
        }
    }

    // Build summary
    let error_count = patterns.iter().filter(|p| p.severity == Severity::Error).count();
    let warning_count = patterns.iter().filter(|p| p.severity == Severity::Warning).count();

    let mut by_type = std::collections::HashMap::new();
    for p in &patterns {
        let key = format!("{}", p.pattern_type);
        *by_type.entry(key).or_insert(0) += 1;
    }

    let summary = MockDetectionSummary {
        error_count,
        warning_count,
        by_type,
    };

    Ok(MockDataResult {
        patterns_found: patterns,
        is_clean: error_count == 0,
        files_scanned,
        files_with_issues: files_with_issues_set.len(),
        summary,
    })
}

/// Add example matches from content to a pattern
fn add_pattern_examples(content: &str, regex: &Regex, pattern: &mut MockPattern, max_examples: usize) {
    for (line_num, line) in content.lines().enumerate() {
        if regex.is_match(line) {
            pattern.add_example(line.trim().chars().take(100).collect::<String>(), max_examples);
            pattern.add_line_number(line_num + 1);
            if pattern.examples.len() >= max_examples {
                break;
            }
        }
    }
}

/// Find all source files in the directory
fn find_source_files(working_dir: &Path, config: &MockDetectorConfig) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(working_dir)
        .into_iter()
        .filter_entry(|e| !is_hidden(e) && !is_excluded_dir_with_config(e, config))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if config.extensions.contains(&ext.to_string_lossy().to_lowercase()) {
                    files.push(path.to_path_buf());
                }
            }
        }
    }

    Ok(files)
}

/// Check if a directory entry is hidden (starts with .)
/// Note: We only check files, not the root directory, to avoid
/// filtering out temp directories that may start with "."
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    // Don't filter the root directory being scanned
    if entry.depth() == 0 {
        return false;
    }
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// Check if a directory should be excluded (target, node_modules, etc.)
#[allow(dead_code)]
fn is_excluded_dir(entry: &walkdir::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    matches!(
        name.as_ref(),
        "target" | "node_modules" | "dist" | "build" | "vendor" | "__pycache__"
    )
}

/// Check if a directory should be excluded with custom config
fn is_excluded_dir_with_config(entry: &walkdir::DirEntry, config: &MockDetectorConfig) -> bool {
    let name = entry.file_name().to_string_lossy();
    config.excluded_dirs.contains(name.as_ref())
}

/// Check if a file is a test file
fn is_test_file(path: &Path) -> bool {
    // Only check the file name and immediate parent, not the full path
    // This avoids false positives from temp directories containing "test"
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // File name checks
    file_name.contains("test")
        || file_name.contains("spec")
        || file_name.ends_with("_tests.rs")
        || file_name.ends_with(".test.ts")
        || file_name.ends_with(".test.tsx")
        || file_name.ends_with(".test.js")
        || file_name.ends_with(".spec.ts")
        || file_name.ends_with(".spec.tsx")
        || file_name.ends_with(".spec.js")
        // Parent directory checks
        || parent_name == "tests"
        || parent_name == "test"
        || parent_name == "spec"
        || parent_name == "__tests__"
}

/// Format mock detection results for display
pub fn format_mock_results(result: &MockDataResult) -> String {
    let mut output = String::new();

    if result.is_clean {
        output.push_str("✅ No mock data or shortcuts detected!\n");
        output.push_str(&format!("   Files scanned: {}\n", result.files_scanned));
        return output;
    }

    output.push_str("⚠️ Mock Data Detection Results\n");
    output.push_str("================================\n\n");
    output.push_str(&format!("Files scanned: {}\n", result.files_scanned));
    output.push_str(&format!("Files with issues: {}\n", result.files_with_issues));
    output.push_str(&format!(
        "Errors: {} | Warnings: {}\n\n",
        result.summary.error_count, result.summary.warning_count
    ));

    for pattern in &result.patterns_found {
        let severity_marker = match pattern.severity {
            Severity::Error => "❌",
            Severity::Warning => "⚠️",
            Severity::Info => "ℹ️",
        };

        output.push_str(&format!(
            "{} {} ({:?})\n",
            severity_marker, pattern.pattern_type, pattern.severity
        ));

        for file in &pattern.files_affected {
            output.push_str(&format!("   File: {}\n", file.display()));
        }

        for example in &pattern.examples {
            output.push_str(&format!("   > {}\n", example));
        }

        if let Some(ref suggestion) = pattern.suggested_fix {
            output.push_str(&format!("   💡 {}\n", suggestion));
        }

        output.push('\n');
    }

    output
}

/// Extract matching lines from content (deprecated, use add_pattern_examples instead)
#[allow(dead_code)]
fn extract_matches(content: &str, regex: &Regex, limit: usize) -> Vec<String> {
    content
        .lines()
        .filter(|line| regex.is_match(line))
        .take(limit)
        .map(|s| s.trim().chars().take(100).collect()) // Limit line length
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_todo_comments() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("main.rs");
        fs::write(&file, "fn main() { // TODO: implement this }").unwrap();

        let result = detect_mock_data(dir.path()).unwrap();
        assert!(!result.is_clean);
        assert!(result
            .patterns_found
            .iter()
            .any(|p| p.pattern_type == MockPatternType::TodoComments));
    }

    #[test]
    fn test_detect_placeholder_data() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("main.rs");
        fs::write(&file, r#"let email = "test@example.com";"#).unwrap();

        let result = detect_mock_data(dir.path()).unwrap();
        assert!(!result.is_clean);
        assert!(result
            .patterns_found
            .iter()
            .any(|p| p.pattern_type == MockPatternType::PlaceholderData));
    }

    #[test]
    fn test_detect_unimplemented_macros() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("lib.rs");
        fs::write(&file, "fn process() { unimplemented!() }").unwrap();

        let result = detect_mock_data(dir.path()).unwrap();
        assert!(!result.is_clean);
        assert!(result
            .patterns_found
            .iter()
            .any(|p| p.pattern_type == MockPatternType::UnimplementedMacros));
    }

    #[test]
    fn test_ignores_test_files() {
        let dir = TempDir::new().unwrap();
        let test_dir = dir.path().join("tests");
        fs::create_dir(&test_dir).unwrap();
        let file = test_dir.join("test_main.rs");
        fs::write(&file, "// TODO: add more tests\nfn test_it() { todo!() }").unwrap();

        let result = detect_mock_data(dir.path()).unwrap();
        assert!(result.is_clean); // TODOs in test files are OK
    }

    #[test]
    fn test_clean_code() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("main.rs");
        fs::write(
            &file,
            r#"
fn main() {
    let config = Config::new();
    config.run().expect("Failed to run");
}
"#,
        )
        .unwrap();

        let result = detect_mock_data(dir.path()).unwrap();
        assert!(result.is_clean);
    }

    #[test]
    fn test_debug_statements_are_warnings() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("main.rs");
        fs::write(&file, r#"fn main() { dbg!("test"); }"#).unwrap();

        let result = detect_mock_data(dir.path()).unwrap();
        // Debug statements are warnings, not errors
        assert!(result.is_clean); // is_clean only considers errors
        assert!(result
            .patterns_found
            .iter()
            .any(|p| p.pattern_type == MockPatternType::DebugStatements
                && p.severity == Severity::Warning));
    }

    #[test]
    fn test_detect_mock_stub_patterns() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("service.rs");
        fs::write(&file, r#"let response = MockData::new();"#).unwrap();

        let result = detect_mock_data(dir.path()).unwrap();
        assert!(!result.is_clean);
        assert!(result
            .patterns_found
            .iter()
            .any(|p| p.pattern_type == MockPatternType::MockStubFake));
    }

    #[test]
    fn test_detect_lorem_ipsum() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("content.rs");
        fs::write(&file, r#"let text = "Lorem ipsum dolor sit amet";"#).unwrap();

        let result = detect_mock_data(dir.path()).unwrap();
        assert!(!result.is_clean);
        assert!(result
            .patterns_found
            .iter()
            .any(|p| p.pattern_type == MockPatternType::LoremIpsum));
    }

    #[test]
    fn test_detect_console_log_in_typescript() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("app.ts");
        fs::write(&file, r#"console.log("debugging");"#).unwrap();

        let result = detect_mock_data(dir.path()).unwrap();
        assert!(result.is_clean); // console.log is a warning, not error
        assert!(result
            .patterns_found
            .iter()
            .any(|p| p.pattern_type == MockPatternType::ConsoleStatements));
    }

    #[test]
    fn test_custom_config_ignore_patterns() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("main.rs");
        fs::write(&file, "fn main() { // TODO: implement this }").unwrap();

        let mut config = MockDetectorConfig::default();
        config.ignore_patterns.insert("todo".to_string());

        let result = detect_mock_data_with_config(dir.path(), &config).unwrap();
        assert!(result.is_clean); // TODO patterns are ignored
    }

    #[test]
    fn test_format_mock_results() {
        let result = MockDataResult {
            patterns_found: vec![MockPattern {
                pattern_type: MockPatternType::TodoComments,
                files_affected: vec![PathBuf::from("src/main.rs")],
                examples: vec!["// TODO: fix this".to_string()],
                severity: Severity::Error,
                line_numbers: vec![10],
                suggested_fix: Some("Remove TODO comments".to_string()),
            }],
            is_clean: false,
            files_scanned: 10,
            files_with_issues: 1,
            summary: MockDetectionSummary {
                error_count: 1,
                warning_count: 0,
                by_type: std::collections::HashMap::new(),
            },
        };

        let output = format_mock_results(&result);
        assert!(output.contains("Mock Data Detection Results"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("TODO"));
    }

    #[test]
    fn test_mock_pattern_methods() {
        let mut pattern = MockPattern::new(MockPatternType::TodoComments, Severity::Error);
        pattern.add_file(PathBuf::from("test.rs"));
        pattern.add_file(PathBuf::from("test.rs")); // Duplicate should be ignored
        pattern.add_example("example 1", 3);
        pattern.add_line_number(10);

        assert_eq!(pattern.files_affected.len(), 1);
        assert_eq!(pattern.examples.len(), 1);
        assert_eq!(pattern.line_numbers, vec![10]);
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(format!("{}", Severity::Error), "ERROR");
        assert_eq!(format!("{}", Severity::Warning), "WARNING");
        assert_eq!(format!("{}", Severity::Info), "INFO");
    }

    #[test]
    fn test_pattern_type_display() {
        assert_eq!(
            format!("{}", MockPatternType::TodoComments),
            "TODO/FIXME comments"
        );
        assert_eq!(
            format!("{}", MockPatternType::Custom("my-pattern".to_string())),
            "Custom: my-pattern"
        );
    }
}
