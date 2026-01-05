//! Verification Module
//!
//! This module provides LLM-based verification for task completion.
//! It uses actual execution (build/test) plus LLM judgment to determine
//! if a task is complete.
//!
//! ## Design Philosophy
//!
//! This module has moved away from regex-based shortcut detection to LLM-based
//! completion checking. The reasoning is:
//! - LLM judgment is more nuanced and context-aware than pattern matching
//! - Actual verification (build/test) provides ground truth
//! - Binary completion status is clearer than confidence scores
//!
//! ## Anti-Laziness Enforcement
//!
//! When enabled, the mock_detector module provides additional verification
//! to detect common shortcuts and incomplete implementations:
//! - TODO/FIXME comments in production code
//! - Placeholder data (example.com, John Doe, etc.)
//! - Unimplemented macros (todo!(), unimplemented!())
//! - Debug statements in production code
//! - Hardcoded secrets

pub mod mock_detector;
pub use mock_detector::{
    detect_mock_data, detect_mock_data_with_config, format_mock_results,
    CustomPattern, MockDataResult, MockDetectionSummary, MockDetectorConfig, MockPattern,
    MockPatternType, Severity,
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;
use tracing::debug;

/// Result of running build command
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildResult {
    /// Whether the build was executed
    pub executed: bool,
    /// Whether the build succeeded
    pub success: bool,
    /// Build output (stdout + stderr)
    pub output: String,
    /// Error message if build failed to run
    pub error: Option<String>,
    /// Warning messages from the build
    pub warnings: Vec<String>,
}

/// Result of running tests
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestResult {
    /// Whether tests were executed
    pub executed: bool,
    /// Whether all tests passed
    pub passed: bool,
    /// Number of tests that passed
    pub passed_count: usize,
    /// Number of tests that failed
    pub failed_count: usize,
    /// Number of tests that were skipped
    pub skipped_count: usize,
    /// Test output (stdout + stderr)
    pub output: String,
    /// Error message if tests failed to run
    pub error: Option<String>,
}

/// LLM completion check result
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmCompletionCheck {
    /// Whether the LLM determined the task is complete
    pub is_complete: bool,
    /// Explanation from the LLM
    pub explanation: String,
    /// Suggested next steps if not complete
    pub next_steps: Vec<String>,
    /// Shortcut risk assessment (0.0-1.0) - higher means more likely shortcuts taken
    #[serde(default)]
    pub shortcut_risk: f32,
}

/// Information about expected file modifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileModificationInfo {
    /// Path to the file (relative to working directory)
    pub path: PathBuf,
    /// Whether the file exists
    pub exists: bool,
    /// File size in bytes (if exists)
    pub size: Option<u64>,
    /// Last modification time (if exists)
    pub modified_time: Option<u64>,
}

/// Result of verifying a task's completion
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the task is considered complete
    pub is_complete: bool,
    /// Whether the completion promise was found in output
    pub promise_found: bool,
    /// Build result (if build was run)
    pub build_result: Option<BuildResult>,
    /// Test result (if tests were run)
    pub test_result: Option<TestResult>,
    /// LLM completion check result
    pub llm_check: Option<LlmCompletionCheck>,
    /// Information about expected file modifications
    pub file_modifications: Vec<FileModificationInfo>,
    /// Warning messages
    pub warnings: Vec<String>,
    /// Error messages
    pub errors: Vec<String>,
    /// Summary of the verification
    pub summary: Option<String>,
    /// Mock data patterns found (if anti-laziness detection enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock_data_found: Option<MockDataResult>,
    /// Whether evidence was insufficient for completion claim
    #[serde(default)]
    pub evidence_insufficient: bool,
}

impl VerificationResult {
    /// Check if there are any blocking errors
    pub fn has_blocking_errors(&self) -> bool {
        // Build failure is blocking
        if let Some(ref build) = self.build_result {
            if build.executed && !build.success {
                return true;
            }
        }
        // Test failure is blocking
        if let Some(ref test) = self.test_result {
            if test.executed && !test.passed {
                return true;
            }
        }
        false
    }

    /// Get a formatted summary of the verification
    pub fn formatted_summary(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref build) = self.build_result {
            if build.executed {
                parts.push(format!(
                    "Build: {}",
                    if build.success { "passed" } else { "FAILED" }
                ));
            }
        }

        if let Some(ref test) = self.test_result {
            if test.executed {
                parts.push(format!(
                    "Tests: {} ({} passed, {} failed)",
                    if test.passed { "passed" } else { "FAILED" },
                    test.passed_count,
                    test.failed_count
                ));
            }
        }

        if let Some(ref llm) = self.llm_check {
            parts.push(format!(
                "LLM Check: {}",
                if llm.is_complete {
                    "complete"
                } else {
                    "incomplete"
                }
            ));
        }

        if parts.is_empty() {
            "No verification performed".to_string()
        } else {
            parts.join(" | ")
        }
    }
}

/// Verification history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationHistoryEntry {
    /// The verification result
    pub result: VerificationResult,
    /// Identifier for this entry
    pub identifier: String,
    /// Optional notes
    pub notes: Option<String>,
    /// Timestamp (unix epoch millis)
    pub timestamp_ms: u64,
}

/// History of verification attempts
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerificationHistory {
    /// Maximum number of entries to keep
    max_entries: usize,
    /// History entries
    entries: Vec<VerificationHistoryEntry>,
}

impl VerificationHistory {
    /// Create a new verification history with a maximum number of entries
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: Vec::new(),
        }
    }

    /// Add a verification result to history
    pub fn add(&mut self, result: VerificationResult, identifier: &str, notes: Option<String>) {
        use std::time::{SystemTime, UNIX_EPOCH};

        let entry = VerificationHistoryEntry {
            result,
            identifier: identifier.to_string(),
            notes,
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        self.entries.push(entry);

        // Trim to max entries
        while self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    /// Get the number of iterations
    pub fn iteration_count(&self) -> usize {
        self.entries.len()
    }

    /// Get the latest result
    pub fn latest(&self) -> Option<&VerificationResult> {
        self.entries.last().map(|e| &e.result)
    }

    /// Check if the verification is stuck (no progress in recent attempts)
    pub fn is_stuck(&self, window: usize) -> bool {
        if self.entries.len() < window {
            return false;
        }

        let recent: Vec<_> = self
            .entries
            .iter()
            .rev()
            .take(window)
            .map(|e| e.result.is_complete)
            .collect();

        // Stuck if all recent attempts have the same result
        recent.iter().all(|&x| x == recent[0])
    }

    /// Get average completion rate
    pub fn completion_rate(&self) -> f32 {
        if self.entries.is_empty() {
            return 0.0;
        }

        let complete_count = self.entries.iter().filter(|e| e.result.is_complete).count();
        complete_count as f32 / self.entries.len() as f32
    }

    /// Check if progress is being made (at least one successful completion recently)
    pub fn is_making_progress(&self) -> Option<bool> {
        if self.entries.len() < 2 {
            return None;
        }

        let recent_complete = self.entries.iter().rev().take(3).any(|e| e.result.is_complete);

        Some(recent_complete)
    }
}

/// Diff verification result for tracking file changes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffVerification {
    /// Files that were changed
    pub files: Vec<FileDiff>,
    /// Total lines added
    pub total_lines_added: usize,
    /// Total lines removed
    pub total_lines_removed: usize,
    /// Number of files created
    pub files_created: usize,
    /// Number of files deleted
    pub files_deleted: usize,
    /// Number of files modified
    pub files_modified: usize,
}

impl DiffVerification {
    /// Get a summary of the changes
    pub fn summary(&self) -> String {
        if !self.has_changes() {
            return "No changes detected".to_string();
        }

        format!(
            "{} created, {} modified, {} deleted (+{} -{} lines)",
            self.files_created,
            self.files_modified,
            self.files_deleted,
            self.total_lines_added,
            self.total_lines_removed
        )
    }

    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        self.files_created > 0
            || self.files_deleted > 0
            || self.files_modified > 0
            || self.total_lines_added > 0
            || self.total_lines_removed > 0
    }
}

/// Information about a file diff
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileDiff {
    /// Path to the file
    pub path: PathBuf,
    /// Whether the file is new
    pub is_new: bool,
    /// Whether the file was deleted
    pub is_deleted: bool,
    /// Whether the file was modified
    pub is_modified: bool,
    /// Lines added
    pub lines_added: usize,
    /// Lines removed
    pub lines_removed: usize,
    /// Size change in bytes
    pub size_delta: i64,
    /// Diff text (if available)
    pub diff_text: Option<String>,
    /// Summary of changes
    pub change_summary: Option<String>,
}

impl FileDiff {
    /// Check if the diff represents meaningful changes
    pub fn is_meaningful(&self) -> bool {
        self.is_new || self.is_deleted || self.is_modified || self.lines_added > 0 || self.lines_removed > 0
    }

    /// Get the change type as a string
    pub fn change_type(&self) -> &'static str {
        if self.is_new {
            "created"
        } else if self.is_deleted {
            "deleted"
        } else if self.is_modified {
            "modified"
        } else {
            "unchanged"
        }
    }

    /// Get a brief summary of the change
    pub fn brief_summary(&self) -> String {
        if self.is_new {
            format!("created (+{} lines)", self.lines_added)
        } else if self.is_deleted {
            format!("deleted (-{} lines)", self.lines_removed)
        } else if self.is_modified {
            format!("+{} -{}", self.lines_added, self.lines_removed)
        } else {
            "unchanged".to_string()
        }
    }
}

/// Main verifier for task completion
#[derive(Debug, Clone)]
pub struct Verifier {
    /// Optional completion promise string to look for
    completion_promise: Option<String>,
    /// Test command to run
    test_command: Option<String>,
    /// Build command to run
    build_command: Option<String>,
    /// Expected files to be modified
    expected_files: Vec<PathBuf>,
    /// Baseline file info captured before task execution
    baseline_files: HashMap<PathBuf, FileModificationInfo>,
}

impl Verifier {
    /// Create a new verifier
    pub fn new(completion_promise: Option<String>) -> Self {
        Self {
            completion_promise,
            test_command: None,
            build_command: None,
            expected_files: Vec::new(),
            baseline_files: HashMap::new(),
        }
    }

    /// Set the test command
    pub fn with_test_command(mut self, command: impl Into<String>) -> Self {
        self.test_command = Some(command.into());
        self
    }

    /// Set the build command
    pub fn with_build_command(mut self, command: impl Into<String>) -> Self {
        self.build_command = Some(command.into());
        self
    }

    /// Set expected files to be modified
    pub fn expect_files(&mut self, files: Vec<PathBuf>) {
        self.expected_files = files;
    }

    /// Capture baseline file information before task execution
    pub fn capture_baseline(&mut self, working_dir: &Path) {
        self.baseline_files.clear();
        for file in &self.expected_files {
            let full_path = working_dir.join(file);
            let info = Self::get_file_info(&full_path, file);
            self.baseline_files.insert(file.clone(), info);
        }
    }

    /// Get file information
    fn get_file_info(full_path: &Path, relative_path: &Path) -> FileModificationInfo {
        let metadata = fs::metadata(full_path).ok();

        FileModificationInfo {
            path: relative_path.to_path_buf(),
            exists: metadata.is_some(),
            size: metadata.as_ref().map(|m| m.len()),
            modified_time: metadata
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        }
    }

    /// Verify file modifications
    fn verify_file_modifications(&self, working_dir: &Path) -> Vec<FileModificationInfo> {
        self.expected_files
            .iter()
            .map(|file| {
                let full_path = working_dir.join(file);
                Self::get_file_info(&full_path, file)
            })
            .collect()
    }

    /// Run tests and capture result
    fn run_tests(&self, working_dir: &Path) -> TestResult {
        let Some(ref command) = self.test_command else {
            return TestResult::default();
        };

        debug!("Running test command: {}", command);

        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);

                let (passed, failed, skipped) = Self::parse_test_output(&combined);

                TestResult {
                    executed: true,
                    passed: output.status.success() && failed == 0,
                    passed_count: passed,
                    failed_count: failed,
                    skipped_count: skipped,
                    output: Self::truncate_output(&combined, 5000),
                    error: if output.status.success() {
                        None
                    } else {
                        Some(format!("Exit code: {:?}", output.status.code()))
                    },
                }
            }
            Err(e) => TestResult {
                executed: false,
                passed: false,
                error: Some(format!("Failed to run tests: {}", e)),
                ..Default::default()
            },
        }
    }

    /// Parse test output for pass/fail counts across different test frameworks
    fn parse_test_output(output: &str) -> (usize, usize, usize) {
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;

        // ===== Rust cargo test =====
        // "test result: ok. 10 passed; 0 failed; 0 ignored;"
        if let Some(caps) = Regex::new(r"test result:.*?(\d+)\s+passed")
            .ok()
            .and_then(|r| r.captures(output))
        {
            passed = caps[1].parse().unwrap_or(passed);
        }
        if let Some(caps) = Regex::new(r"test result:.*?(\d+)\s+failed")
            .ok()
            .and_then(|r| r.captures(output))
        {
            failed = caps[1].parse().unwrap_or(failed);
        }
        if let Some(caps) = Regex::new(r"test result:.*?(\d+)\s+ignored")
            .ok()
            .and_then(|r| r.captures(output))
        {
            skipped = caps[1].parse().unwrap_or(skipped);
        }

        // ===== Jest/Vitest =====
        // "Tests:       2 failed, 10 passed, 12 total"
        if let Some(caps) = Regex::new(r"Tests:.*?(\d+)\s+passed")
            .ok()
            .and_then(|r| r.captures(output))
        {
            passed = caps[1].parse().unwrap_or(passed);
        }
        if let Some(caps) = Regex::new(r"Tests:.*?(\d+)\s+failed")
            .ok()
            .and_then(|r| r.captures(output))
        {
            failed = caps[1].parse().unwrap_or(failed);
        }
        if let Some(caps) = Regex::new(r"Tests:.*?(\d+)\s+skipped")
            .ok()
            .and_then(|r| r.captures(output))
        {
            skipped = caps[1].parse().unwrap_or(skipped);
        }

        // ===== Mocha =====
        // "10 passing (500ms)"
        // "2 failing"
        // "1 pending"
        if let Some(caps) = Regex::new(r"(\d+)\s+passing")
            .ok()
            .and_then(|r| r.captures(output))
        {
            passed = caps[1].parse().unwrap_or(passed);
        }
        if let Some(caps) = Regex::new(r"(\d+)\s+failing")
            .ok()
            .and_then(|r| r.captures(output))
        {
            failed = caps[1].parse().unwrap_or(failed);
        }
        if let Some(caps) = Regex::new(r"(\d+)\s+pending")
            .ok()
            .and_then(|r| r.captures(output))
        {
            skipped = caps[1].parse().unwrap_or(skipped);
        }

        // ===== Python pytest =====
        // "===== 10 passed, 2 failed, 1 skipped in 0.5s ====="
        if let Some(caps) = Regex::new(r"=+\s*(\d+)\s+passed")
            .ok()
            .and_then(|r| r.captures(output))
        {
            passed = caps[1].parse().unwrap_or(passed);
        }
        if let Some(caps) = Regex::new(r"=+.*?(\d+)\s+failed")
            .ok()
            .and_then(|r| r.captures(output))
        {
            failed = caps[1].parse().unwrap_or(failed);
        }
        if let Some(caps) = Regex::new(r"=+.*?(\d+)\s+skipped")
            .ok()
            .and_then(|r| r.captures(output))
        {
            skipped = caps[1].parse().unwrap_or(skipped);
        }

        // ===== Go test =====
        let go_pass_count = Regex::new(r"---\s+PASS:")
            .ok()
            .map(|r| r.find_iter(output).count())
            .unwrap_or(0);
        let go_fail_count = Regex::new(r"---\s+FAIL:")
            .ok()
            .map(|r| r.find_iter(output).count())
            .unwrap_or(0);
        let go_skip_count = Regex::new(r"---\s+SKIP:")
            .ok()
            .map(|r| r.find_iter(output).count())
            .unwrap_or(0);

        if go_pass_count > 0 || go_fail_count > 0 {
            passed = passed.max(go_pass_count);
            failed = failed.max(go_fail_count);
            skipped = skipped.max(go_skip_count);
        }

        // ===== Generic fallback =====
        if passed == 0 && failed == 0 {
            if let Some(caps) = Regex::new(r"(\d+)\s+(?:tests?\s+)?passed")
                .ok()
                .and_then(|r| r.captures(output))
            {
                passed = caps[1].parse().unwrap_or(0);
            }
            if let Some(caps) = Regex::new(r"(\d+)\s+(?:tests?\s+)?failed")
                .ok()
                .and_then(|r| r.captures(output))
            {
                failed = caps[1].parse().unwrap_or(0);
            }
        }

        (passed, failed, skipped)
    }

    /// Run build and capture result
    fn run_build(&self, working_dir: &Path) -> BuildResult {
        let Some(ref command) = self.build_command else {
            return BuildResult::default();
        };

        debug!("Running build command: {}", command);

        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);

                // Extract warnings
                let warnings: Vec<String> = combined
                    .lines()
                    .filter(|l| l.to_lowercase().contains("warning"))
                    .map(|l| l.to_string())
                    .take(10)
                    .collect();

                BuildResult {
                    executed: true,
                    success: output.status.success(),
                    output: Self::truncate_output(&combined, 5000),
                    error: if output.status.success() {
                        None
                    } else {
                        Some(format!("Exit code: {:?}", output.status.code()))
                    },
                    warnings,
                }
            }
            Err(e) => BuildResult {
                executed: false,
                success: false,
                error: Some(format!("Failed to run build: {}", e)),
                ..Default::default()
            },
        }
    }

    /// Truncate output to a maximum length
    fn truncate_output(output: &str, max_len: usize) -> String {
        if output.len() <= max_len {
            output.to_string()
        } else {
            format!("{}...[truncated]", &output[..max_len])
        }
    }

    /// Verify task output and working directory
    ///
    /// This performs actual verification via build/test commands.
    /// The LLM completion check should be performed separately by the caller.
    pub fn verify(&self, output: &str, working_dir: &Path) -> VerificationResult {
        let mut result = VerificationResult::default();

        // Check for completion promise
        if let Some(ref promise) = self.completion_promise {
            if output.contains(promise) {
                result.promise_found = true;
                debug!("Completion promise found: {}", promise);
            }
        }

        // Verify file modifications
        if !self.expected_files.is_empty() {
            result.file_modifications = self.verify_file_modifications(working_dir);

            let total_files = self.expected_files.len();
            let existing_count = result
                .file_modifications
                .iter()
                .filter(|f| f.exists)
                .count();

            if existing_count < total_files {
                let missing = total_files - existing_count;
                result.warnings.push(format!(
                    "{} of {} expected files are missing",
                    missing, total_files
                ));
            }
        }

        // Run tests if configured
        if self.test_command.is_some() {
            let test_result = self.run_tests(working_dir);
            if test_result.executed && !test_result.passed {
                result.errors.push(format!(
                    "Tests failed: {} passed, {} failed",
                    test_result.passed_count, test_result.failed_count
                ));
            }
            result.test_result = Some(test_result);
        }

        // Run build if configured
        if self.build_command.is_some() {
            let build_result = self.run_build(working_dir);
            if build_result.executed && !build_result.success {
                result.errors.push("Build failed".to_string());
            }
            result.build_result = Some(build_result);
        }

        // Determine completion based on actual results
        let tests_passed = result
            .test_result
            .as_ref()
            .map(|t| !t.executed || t.passed)
            .unwrap_or(true);

        let build_passed = result
            .build_result
            .as_ref()
            .map(|b| !b.executed || b.success)
            .unwrap_or(true);

        // Task is complete if:
        // 1. Build passes (or wasn't run)
        // 2. Tests pass (or weren't run)
        // 3. Either promise was found OR LLM check passes (set by caller)
        result.is_complete = build_passed && tests_passed && result.promise_found;

        // Generate summary
        result.summary = Some(if result.is_complete {
            "Task complete: build and tests passed".to_string()
        } else {
            let mut reasons = Vec::new();
            if !tests_passed {
                reasons.push("tests failed");
            }
            if !build_passed {
                reasons.push("build failed");
            }
            if !result.promise_found && self.completion_promise.is_some() {
                reasons.push("completion promise not found");
            }
            if reasons.is_empty() {
                "Task incomplete: awaiting LLM verification".to_string()
            } else {
                format!("Task incomplete: {}", reasons.join(", "))
            }
        });

        result
    }

    /// Apply LLM completion check to the result
    pub fn apply_llm_check(result: &mut VerificationResult, llm_check: LlmCompletionCheck) {
        // If build and tests pass, defer to LLM judgment
        let tests_passed = result
            .test_result
            .as_ref()
            .map(|t| !t.executed || t.passed)
            .unwrap_or(true);

        let build_passed = result
            .build_result
            .as_ref()
            .map(|b| !b.executed || b.success)
            .unwrap_or(true);

        // Update completion status based on LLM check
        if build_passed && tests_passed {
            result.is_complete = llm_check.is_complete;
        }

        // Update summary
        result.summary = Some(if result.is_complete {
            format!("Task complete: {}", llm_check.explanation)
        } else {
            format!("Task incomplete: {}", llm_check.explanation)
        });

        result.llm_check = Some(llm_check);
    }

    /// Full verification with optional test and build execution
    pub fn verify_full(
        &self,
        output: &str,
        working_dir: &Path,
        run_tests: bool,
        run_build: bool,
    ) -> VerificationResult {
        // Create a mutable clone to optionally add commands
        let mut verifier = Verifier::new(self.completion_promise.clone());
        verifier.expected_files = self.expected_files.clone();
        verifier.baseline_files = self.baseline_files.clone();

        if run_tests {
            verifier.test_command = self.test_command.clone();
        }
        if run_build {
            verifier.build_command = self.build_command.clone();
        }

        verifier.verify(output, working_dir)
    }
}

/// Generate an LLM prompt for completion checking
///
/// This function creates a prompt that can be sent to an LLM to determine
/// if a task is complete. The LLM should respond with a JSON object containing:
/// - is_complete: boolean
/// - explanation: string
/// - next_steps: array of strings (if not complete)
pub fn create_llm_completion_prompt(
    task_description: &str,
    output: &str,
    build_result: Option<&BuildResult>,
    test_result: Option<&TestResult>,
) -> String {
    let mut context = String::new();

    context.push_str("You are reviewing whether a task has been completed.\n\n");
    context.push_str("## Original Task\n");
    context.push_str(task_description);
    context.push_str("\n\n");

    context.push_str("## Agent Output\n");
    context.push_str(output);
    context.push_str("\n\n");

    if let Some(build) = build_result {
        if build.executed {
            context.push_str("## Build Result\n");
            context.push_str(&format!("Status: {}\n", if build.success { "SUCCESS" } else { "FAILED" }));
            if !build.output.is_empty() {
                context.push_str(&format!("Output:\n```\n{}\n```\n", build.output));
            }
            context.push_str("\n");
        }
    }

    if let Some(test) = test_result {
        if test.executed {
            context.push_str("## Test Result\n");
            context.push_str(&format!("Status: {}\n", if test.passed { "PASSED" } else { "FAILED" }));
            context.push_str(&format!("Passed: {}, Failed: {}, Skipped: {}\n",
                test.passed_count, test.failed_count, test.skipped_count));
            if !test.output.is_empty() {
                context.push_str(&format!("Output:\n```\n{}\n```\n", test.output));
            }
            context.push_str("\n");
        }
    }

    context.push_str("## Your Task\n");
    context.push_str("Determine if the task has been completed successfully.\n\n");
    context.push_str("Consider:\n");
    context.push_str("1. Does the code work? (Build and tests pass)\n");
    context.push_str("2. Does it solve the original problem?\n");
    context.push_str("3. Is it production-ready? (No obvious placeholders, mock data, or incomplete implementations)\n\n");
    context.push_str("Respond with JSON only:\n");
    context.push_str("```json\n");
    context.push_str("{\n");
    context.push_str("  \"is_complete\": true/false,\n");
    context.push_str("  \"explanation\": \"Brief explanation of your determination\",\n");
    context.push_str("  \"next_steps\": [\"step 1\", \"step 2\"] // if not complete\n");
    context.push_str("}\n");
    context.push_str("```\n");

    context
}

/// Parse LLM response into an LlmCompletionCheck
pub fn parse_llm_completion_response(response: &str) -> Result<LlmCompletionCheck, String> {
    // Try to extract JSON from the response
    let json_start = response.find('{');
    let json_end = response.rfind('}');

    let json_str = match (json_start, json_end) {
        (Some(start), Some(end)) if start < end => &response[start..=end],
        _ => return Err("No JSON object found in response".to_string()),
    };

    // Parse the JSON
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    let is_complete = value
        .get("is_complete")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let explanation = value
        .get("explanation")
        .and_then(|v| v.as_str())
        .unwrap_or("No explanation provided")
        .to_string();

    let next_steps = value
        .get("next_steps")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let shortcut_risk = value
        .get("shortcut_risk")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(0.0);

    Ok(LlmCompletionCheck {
        is_complete,
        explanation,
        next_steps,
        shortcut_risk,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_verify_with_promise() {
        let verifier = Verifier::new(Some("TASK_COMPLETE".to_string()));
        let temp_dir = TempDir::new().unwrap();

        let output = "Successfully implemented the feature. TASK_COMPLETE";
        let result = verifier.verify(output, temp_dir.path());

        assert!(result.promise_found);
        assert!(result.is_complete);
    }

    #[test]
    fn test_verify_without_promise() {
        let verifier = Verifier::new(Some("TASK_COMPLETE".to_string()));
        let temp_dir = TempDir::new().unwrap();

        let output = "Still working on it...";
        let result = verifier.verify(output, temp_dir.path());

        assert!(!result.promise_found);
        assert!(!result.is_complete);
    }

    #[test]
    fn test_file_modification_verification() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.rs");

        // Create a file
        fs::write(&test_file, "initial content").unwrap();

        let mut verifier = Verifier::new(None);
        verifier.expect_files(vec![PathBuf::from("test.rs")]);
        verifier.capture_baseline(temp_dir.path());

        // Modify the file
        fs::write(&test_file, "modified content").unwrap();

        let result = verifier.verify("Task complete", temp_dir.path());

        assert!(!result.file_modifications.is_empty());
        assert!(result.file_modifications[0].exists);
    }

    #[test]
    fn test_file_missing_verification() {
        let temp_dir = TempDir::new().unwrap();

        let mut verifier = Verifier::new(None);
        verifier.expect_files(vec![PathBuf::from("nonexistent.rs")]);

        let result = verifier.verify("Task complete", temp_dir.path());

        assert!(!result.file_modifications.is_empty());
        assert!(!result.file_modifications[0].exists);
        assert!(result.warnings.iter().any(|w| w.contains("missing")));
    }

    #[test]
    fn test_verification_history() {
        let mut history = VerificationHistory::new(10);

        let result1 = VerificationResult {
            is_complete: false,
            ..Default::default()
        };
        history.add(result1, "initial", None);

        let result2 = VerificationResult {
            is_complete: true,
            ..Default::default()
        };
        history.add(result2, "improved", None);

        assert_eq!(history.iteration_count(), 2);
        assert!(history.latest().unwrap().is_complete);
    }

    #[test]
    fn test_history_stuck_detection() {
        let mut history = VerificationHistory::new(10);

        // Add 3 results with same completion status
        for _ in 0..3 {
            let result = VerificationResult {
                is_complete: false,
                ..Default::default()
            };
            history.add(result, "iter", None);
        }

        assert!(history.is_stuck(3));
    }

    #[test]
    fn test_llm_completion_prompt() {
        let prompt = create_llm_completion_prompt(
            "Implement a hello world function",
            "I created the function that prints Hello World",
            None,
            None,
        );

        assert!(prompt.contains("Implement a hello world function"));
        assert!(prompt.contains("prints Hello World"));
        assert!(prompt.contains("is_complete"));
    }

    #[test]
    fn test_parse_llm_response() {
        let response = r#"
            Based on the output, here's my assessment:
            ```json
            {
                "is_complete": true,
                "explanation": "The task has been completed successfully",
                "next_steps": []
            }
            ```
        "#;

        let check = parse_llm_completion_response(response).unwrap();
        assert!(check.is_complete);
        assert_eq!(check.explanation, "The task has been completed successfully");
    }

    #[test]
    fn test_parse_llm_response_incomplete() {
        let response = r#"{
            "is_complete": false,
            "explanation": "Missing error handling",
            "next_steps": ["Add error handling", "Add tests"]
        }"#;

        let check = parse_llm_completion_response(response).unwrap();
        assert!(!check.is_complete);
        assert_eq!(check.next_steps.len(), 2);
    }

    #[test]
    fn test_diff_verification_summary() {
        let verification = DiffVerification {
            files: vec![],
            total_lines_added: 100,
            total_lines_removed: 50,
            files_created: 2,
            files_deleted: 1,
            files_modified: 5,
        };

        let summary = verification.summary();
        assert!(summary.contains("2 created"));
        assert!(summary.contains("5 modified"));
        assert!(summary.contains("1 deleted"));
        assert!(verification.has_changes());
    }

    #[test]
    fn test_diff_verification_no_changes() {
        let verification = DiffVerification::default();
        assert_eq!(verification.summary(), "No changes detected");
        assert!(!verification.has_changes());
    }

    #[test]
    fn test_file_diff_methods() {
        let new_file = FileDiff {
            path: PathBuf::from("new.rs"),
            is_new: true,
            lines_added: 50,
            ..Default::default()
        };

        assert!(new_file.is_meaningful());
        assert_eq!(new_file.change_type(), "created");
        assert!(new_file.brief_summary().contains("created"));

        let deleted_file = FileDiff {
            path: PathBuf::from("old.rs"),
            is_deleted: true,
            lines_removed: 100,
            ..Default::default()
        };

        assert!(deleted_file.is_meaningful());
        assert_eq!(deleted_file.change_type(), "deleted");
    }

    #[test]
    fn test_verification_result_has_blocking_errors() {
        let mut result = VerificationResult::default();
        assert!(!result.has_blocking_errors());

        result.build_result = Some(BuildResult {
            executed: true,
            success: false,
            ..Default::default()
        });
        assert!(result.has_blocking_errors());

        result.build_result = Some(BuildResult {
            executed: true,
            success: true,
            ..Default::default()
        });
        result.test_result = Some(TestResult {
            executed: true,
            passed: false,
            ..Default::default()
        });
        assert!(result.has_blocking_errors());
    }

    #[test]
    fn test_apply_llm_check() {
        let mut result = VerificationResult::default();
        result.build_result = Some(BuildResult {
            executed: true,
            success: true,
            ..Default::default()
        });

        let llm_check = LlmCompletionCheck {
            is_complete: true,
            explanation: "All requirements met".to_string(),
            next_steps: vec![],
            shortcut_risk: 0.0,
        };

        Verifier::apply_llm_check(&mut result, llm_check);

        assert!(result.is_complete);
        assert!(result.llm_check.is_some());
        assert!(result.summary.unwrap().contains("complete"));
    }
}
