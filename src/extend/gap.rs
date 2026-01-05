//! Gap Detection Module
//!
//! Detects when the agent lacks a capability it needs.
//!
//! Signals:
//! - **Explicit**: Claude outputs "I would need an MCP for X" or "This would be easier with a tool for Y"
//! - **Implicit**: Repeated failures (>3) on same category of task
//! - **Pattern**: Known capability gap patterns (e.g., "fetch from API X" without API MCP)

use super::CapabilityGap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pattern for detecting capability gaps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapPattern {
    /// Pattern name
    pub name: String,
    /// Regex pattern to match in output
    pub pattern: String,
    /// Type of gap this pattern indicates
    pub gap_type: GapType,
    /// Capture group index for extracting the capability name (0 = full match)
    pub name_capture_group: usize,
    /// Capture group index for extracting purpose/description
    pub purpose_capture_group: Option<usize>,
}

/// Type of gap pattern
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapType {
    /// Explicit request for capability
    Explicit,
    /// Implicit from failure pattern
    Implicit,
    /// Known pattern match
    Pattern,
}

/// Record of a task attempt for failure pattern analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAttempt {
    /// Task description
    pub task: String,
    /// Whether it succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp (unix epoch seconds)
    pub timestamp: u64,
    /// Category of the task (for similarity grouping)
    pub category: Option<String>,
    /// Keywords extracted from the task
    pub keywords: Vec<String>,
}

impl TaskAttempt {
    /// Create a new task attempt
    pub fn new(task: impl Into<String>, success: bool) -> Self {
        let task_str = task.into();
        let keywords = Self::extract_keywords(&task_str);
        Self {
            task: task_str,
            success,
            error: None,
            timestamp: current_timestamp(),
            category: None,
            keywords,
        }
    }

    /// Create a failed attempt with error
    pub fn failed(task: impl Into<String>, error: impl Into<String>) -> Self {
        let task_str = task.into();
        let keywords = Self::extract_keywords(&task_str);
        Self {
            task: task_str,
            success: false,
            error: Some(error.into()),
            timestamp: current_timestamp(),
            category: None,
            keywords,
        }
    }

    /// Extract keywords from task description
    fn extract_keywords(task: &str) -> Vec<String> {
        // Extract words that might indicate capability needs
        let api_pattern = Regex::new(r"(?i)\b(api|service|endpoint|fetch|request|call|integrate)\b").unwrap();
        let tool_pattern = Regex::new(r"(?i)\b(tool|mcp|skill|capability|feature)\b").unwrap();
        let action_pattern = Regex::new(r"(?i)\b(create|update|delete|read|write|send|receive|process)\b").unwrap();

        let mut keywords = Vec::new();

        for pattern in [&api_pattern, &tool_pattern, &action_pattern] {
            for cap in pattern.find_iter(task) {
                keywords.push(cap.as_str().to_lowercase());
            }
        }

        keywords.sort();
        keywords.dedup();
        keywords
    }

    /// Calculate similarity with another task attempt
    pub fn similarity(&self, other: &TaskAttempt) -> f32 {
        if self.keywords.is_empty() && other.keywords.is_empty() {
            return 0.0;
        }

        let intersection: usize = self.keywords.iter()
            .filter(|k| other.keywords.contains(k))
            .count();

        let union: usize = {
            let mut combined = self.keywords.clone();
            combined.extend(other.keywords.clone());
            combined.sort();
            combined.dedup();
            combined.len()
        };

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }
}

/// Compiled gap pattern for efficient matching
struct CompiledPattern {
    pattern: GapPattern,
    regex: Regex,
}

/// Detects capability gaps from output analysis and failure patterns
pub struct GapDetector {
    /// Compiled patterns to match against
    patterns: Vec<CompiledPattern>,
    /// History of task attempts
    history: Vec<TaskAttempt>,
    /// Maximum history size
    max_history: usize,
    /// Failure threshold for implicit detection
    failure_threshold: usize,
    /// Similarity threshold for grouping failures
    similarity_threshold: f32,
}

impl Default for GapDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl GapDetector {
    /// Create a new gap detector with default patterns
    pub fn new() -> Self {
        let patterns = Self::default_patterns();
        let compiled = patterns.into_iter()
            .filter_map(|p| {
                Regex::new(&p.pattern).ok().map(|regex| CompiledPattern { pattern: p, regex })
            })
            .collect();

        Self {
            patterns: compiled,
            history: Vec::new(),
            max_history: 100,
            failure_threshold: 3,
            similarity_threshold: 0.5,
        }
    }

    /// Create with custom patterns
    pub fn with_patterns(patterns: Vec<GapPattern>) -> Self {
        let compiled = patterns.into_iter()
            .filter_map(|p| {
                Regex::new(&p.pattern).ok().map(|regex| CompiledPattern { pattern: p, regex })
            })
            .collect();

        Self {
            patterns: compiled,
            history: Vec::new(),
            max_history: 100,
            failure_threshold: 3,
            similarity_threshold: 0.5,
        }
    }

    /// Set failure threshold for implicit detection
    pub fn with_failure_threshold(mut self, threshold: usize) -> Self {
        self.failure_threshold = threshold;
        self
    }

    /// Set similarity threshold for grouping failures
    pub fn with_similarity_threshold(mut self, threshold: f32) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    /// Get default gap detection patterns
    fn default_patterns() -> Vec<GapPattern> {
        vec![
            // Explicit MCP requests
            GapPattern {
                name: "explicit_mcp_request".to_string(),
                pattern: r"(?i)I would need (?:an? )?MCP(?: server)? (?:for |to )?(.+?)(?:\.|$)".to_string(),
                gap_type: GapType::Explicit,
                name_capture_group: 1,
                purpose_capture_group: Some(1),
            },
            GapPattern {
                name: "mcp_would_help".to_string(),
                pattern: r"(?i)(?:an? )?MCP(?: server)? (?:for |to )(.+?) would (?:help|be useful|make this easier)".to_string(),
                gap_type: GapType::Explicit,
                name_capture_group: 1,
                purpose_capture_group: Some(1),
            },
            // Explicit tool requests
            GapPattern {
                name: "explicit_tool_request".to_string(),
                pattern: r"(?i)(?:would be easier|could use|need) (?:a )?tool (?:for |to )(.+?)(?:\.|$)".to_string(),
                gap_type: GapType::Explicit,
                name_capture_group: 1,
                purpose_capture_group: Some(1),
            },
            // API access needs
            GapPattern {
                name: "api_access_needed".to_string(),
                pattern: r"(?i)need(?:s)? (?:API )?access to (\w+(?:\s+\w+)*)".to_string(),
                gap_type: GapType::Pattern,
                name_capture_group: 1,
                purpose_capture_group: Some(1),
            },
            GapPattern {
                name: "no_api_available".to_string(),
                pattern: r"(?i)(?:no|don't have|lack) (?:access to |an? )?(\w+) (?:API|service|endpoint)".to_string(),
                gap_type: GapType::Pattern,
                name_capture_group: 1,
                purpose_capture_group: None,
            },
            // Skill requests
            GapPattern {
                name: "skill_request".to_string(),
                pattern: r"(?i)(?:would benefit from|could use|need) (?:a )?skill (?:for |to )(.+?)(?:\.|$)".to_string(),
                gap_type: GapType::Explicit,
                name_capture_group: 1,
                purpose_capture_group: Some(1),
            },
            // Capability gaps
            GapPattern {
                name: "capability_missing".to_string(),
                pattern: r"(?i)(?:don't have|lack|missing) (?:the )?capability (?:to |for )(.+?)(?:\.|$)".to_string(),
                gap_type: GapType::Pattern,
                name_capture_group: 1,
                purpose_capture_group: Some(1),
            },
            // Integration needs
            GapPattern {
                name: "integration_needed".to_string(),
                pattern: r"(?i)(?:need to |would need to |should )integrat(?:e|ion) (?:with )?(\w+(?:\s+\w+)*)".to_string(),
                gap_type: GapType::Pattern,
                name_capture_group: 1,
                purpose_capture_group: None,
            },
            // Cannot access patterns
            GapPattern {
                name: "cannot_access".to_string(),
                pattern: r"(?i)cannot (?:access|reach|connect to) (?:the )?(\w+(?:\s+\w+)*)".to_string(),
                gap_type: GapType::Implicit,
                name_capture_group: 1,
                purpose_capture_group: None,
            },
            // HTTP/fetch failures
            GapPattern {
                name: "fetch_failure".to_string(),
                pattern: r"(?i)(?:failed to |could not |unable to )(?:fetch|retrieve|get) (?:from )?(\w+(?:\.\w+)*)".to_string(),
                gap_type: GapType::Implicit,
                name_capture_group: 1,
                purpose_capture_group: None,
            },
        ]
    }

    /// Analyze output for explicit capability gap signals
    pub fn analyze_output(&self, output: &str) -> Option<CapabilityGap> {
        for compiled in &self.patterns {
            if let Some(captures) = compiled.regex.captures(output) {
                let name = captures.get(compiled.pattern.name_capture_group)
                    .map(|m| sanitize_capability_name(m.as_str()))
                    .unwrap_or_else(|| "detected-capability".to_string());

                let purpose = compiled.pattern.purpose_capture_group
                    .and_then(|idx| captures.get(idx))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| format!("Detected via pattern: {}", compiled.pattern.name));

                // Determine capability type based on pattern
                return Some(match compiled.pattern.gap_type {
                    GapType::Explicit => {
                        if compiled.pattern.name.contains("mcp") {
                            CapabilityGap::Mcp {
                                name,
                                purpose,
                                api_hint: extract_api_hint(output),
                            }
                        } else if compiled.pattern.name.contains("skill") {
                            CapabilityGap::Skill {
                                name: name.clone(),
                                trigger: format!("/{}", name.to_lowercase().replace(' ', "-")),
                                purpose,
                            }
                        } else {
                            // Default to skill for explicit requests
                            CapabilityGap::Skill {
                                name: name.clone(),
                                trigger: format!("/{}", name.to_lowercase().replace(' ', "-")),
                                purpose,
                            }
                        }
                    }
                    GapType::Pattern => {
                        // Pattern matches usually indicate API/MCP needs
                        if compiled.pattern.name.contains("api") || compiled.pattern.name.contains("integration") {
                            CapabilityGap::Mcp {
                                name,
                                purpose,
                                api_hint: extract_api_hint(output),
                            }
                        } else {
                            CapabilityGap::Skill {
                                name: name.clone(),
                                trigger: format!("/{}", name.to_lowercase().replace(' ', "-")),
                                purpose,
                            }
                        }
                    }
                    GapType::Implicit => {
                        // Implicit failures often indicate MCP needs
                        CapabilityGap::Mcp {
                            name,
                            purpose,
                            api_hint: extract_api_hint(output),
                        }
                    }
                });
            }
        }
        None
    }

    /// Analyze failure patterns in history to detect implicit gaps
    pub fn analyze_failure_pattern(&self, history: &[TaskAttempt]) -> Option<CapabilityGap> {
        // Get recent failures
        let failures: Vec<_> = history.iter()
            .filter(|t| !t.success)
            .collect();

        if failures.len() < self.failure_threshold {
            return None;
        }

        // Group failures by similarity
        let mut failure_groups: HashMap<usize, Vec<&TaskAttempt>> = HashMap::new();
        let mut group_id = 0;
        let mut assigned: Vec<bool> = vec![false; failures.len()];

        for (i, attempt) in failures.iter().enumerate() {
            if assigned[i] {
                continue;
            }

            let mut group = vec![*attempt];
            assigned[i] = true;

            for (j, other) in failures.iter().enumerate().skip(i + 1) {
                if !assigned[j] && attempt.similarity(other) >= self.similarity_threshold {
                    group.push(*other);
                    assigned[j] = true;
                }
            }

            if group.len() >= self.failure_threshold {
                failure_groups.insert(group_id, group);
                group_id += 1;
            }
        }

        // Find the largest failure group
        if let Some((_, group)) = failure_groups.into_iter().max_by_key(|(_, g)| g.len()) {
            // Extract common keywords from the failure group
            let mut keyword_counts: HashMap<&str, usize> = HashMap::new();
            for attempt in &group {
                for keyword in &attempt.keywords {
                    *keyword_counts.entry(keyword.as_str()).or_insert(0) += 1;
                }
            }

            // Find the most common keyword
            let common_keyword = keyword_counts.into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(kw, _)| kw)
                .unwrap_or("unknown");

            // Analyze error messages for patterns
            let error_analysis: String = group.iter()
                .filter_map(|a| a.error.as_ref())
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            // Try to detect what kind of capability is needed
            if error_analysis.to_lowercase().contains("api")
                || error_analysis.to_lowercase().contains("http")
                || error_analysis.to_lowercase().contains("fetch") {
                return Some(CapabilityGap::Mcp {
                    name: format!("{}-api", common_keyword),
                    purpose: format!("Detected from {} repeated failures related to {}", group.len(), common_keyword),
                    api_hint: extract_api_hint(&error_analysis),
                });
            } else {
                return Some(CapabilityGap::Skill {
                    name: format!("{}-skill", common_keyword),
                    trigger: format!("/{}", common_keyword),
                    purpose: format!("Detected from {} repeated failures related to {}", group.len(), common_keyword),
                });
            }
        }

        None
    }

    /// Analyze both output and history for capability gaps
    pub fn analyze(&self, output: &str, additional_history: &[TaskAttempt]) -> Option<CapabilityGap> {
        // First check for explicit signals in output
        if let Some(gap) = self.analyze_output(output) {
            return Some(gap);
        }

        // Then check for implicit patterns in history
        let mut combined_history = self.history.clone();
        combined_history.extend(additional_history.iter().cloned());

        self.analyze_failure_pattern(&combined_history)
    }

    /// Record a task attempt
    pub fn record_attempt(&mut self, attempt: TaskAttempt) {
        self.history.push(attempt);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    /// Get the current history
    pub fn history(&self) -> &[TaskAttempt] {
        &self.history
    }

    /// Clear history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Get failure statistics
    pub fn failure_stats(&self) -> FailureStats {
        let total = self.history.len();
        let failures = self.history.iter().filter(|a| !a.success).count();
        let successes = total - failures;

        FailureStats {
            total,
            failures,
            successes,
            failure_rate: if total > 0 { failures as f32 / total as f32 } else { 0.0 },
        }
    }

    /// Detect a capability gap from an error message
    ///
    /// This is the main async entry point for gap detection during task execution.
    /// It analyzes the error message and working directory context to identify
    /// missing capabilities.
    ///
    /// # Arguments
    /// * `error` - The error message or output to analyze
    /// * `_working_dir` - Working directory for context (currently unused but available for future use)
    ///
    /// # Returns
    /// `Ok(Some(gap))` if a gap is detected, `Ok(None)` otherwise
    pub async fn detect_from_error(
        &self,
        error: &str,
        _working_dir: &std::path::Path,
    ) -> anyhow::Result<Option<CapabilityGap>> {
        // First try to detect explicit signals in the error message
        if let Some(gap) = self.analyze_output(error) {
            return Ok(Some(gap));
        }

        // Then check for implicit patterns based on failure history
        if let Some(gap) = self.analyze_failure_pattern(&self.history) {
            return Ok(Some(gap));
        }

        Ok(None)
    }
}

/// Failure statistics
#[derive(Debug, Clone)]
pub struct FailureStats {
    pub total: usize,
    pub failures: usize,
    pub successes: usize,
    pub failure_rate: f32,
}

/// Sanitize a capability name extracted from text
fn sanitize_capability_name(name: &str) -> String {
    // Remove trailing punctuation and whitespace
    let name = name.trim().trim_end_matches(|c: char| c.is_ascii_punctuation());

    // Convert to kebab-case
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.len() > 3 {
        // If too many words, just take the first 3
        words[..3].join("-").to_lowercase()
    } else {
        words.join("-").to_lowercase()
    }
}

/// Extract API hint from text
fn extract_api_hint(text: &str) -> Option<String> {
    // Look for URLs
    let url_pattern = Regex::new(r"https?://[^\s]+").ok()?;
    if let Some(m) = url_pattern.find(text) {
        return Some(m.as_str().to_string());
    }

    // Look for domain names
    let domain_pattern = Regex::new(r"\b([a-zA-Z0-9-]+\.(?:com|org|io|dev|api|net))\b").ok()?;
    if let Some(captures) = domain_pattern.captures(text) {
        if let Some(m) = captures.get(1) {
            return Some(m.as_str().to_string());
        }
    }

    None
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gap_detector_new() {
        let detector = GapDetector::new();
        assert!(!detector.patterns.is_empty());
        assert!(detector.history.is_empty());
    }

    #[test]
    fn test_analyze_explicit_mcp_request() {
        let detector = GapDetector::new();
        let output = "I would need an MCP for accessing the GitHub API to complete this task.";

        let gap = detector.analyze_output(output);
        assert!(gap.is_some());

        if let Some(CapabilityGap::Mcp { name, purpose, .. }) = gap {
            assert!(name.contains("github") || name.contains("accessing"));
            assert!(!purpose.is_empty());
        } else {
            panic!("Expected MCP gap");
        }
    }

    #[test]
    fn test_analyze_api_access_needed() {
        let detector = GapDetector::new();
        let output = "I need API access to Stripe for processing payments.";

        let gap = detector.analyze_output(output);
        assert!(gap.is_some());

        if let Some(CapabilityGap::Mcp { name, .. }) = gap {
            assert!(name.to_lowercase().contains("stripe"));
        }
    }

    #[test]
    fn test_analyze_skill_request() {
        let detector = GapDetector::new();
        let output = "I could use a skill for code review automation.";

        let gap = detector.analyze_output(output);
        assert!(gap.is_some());

        if let Some(CapabilityGap::Skill { trigger, .. }) = gap {
            assert!(trigger.starts_with('/'));
        }
    }

    #[test]
    fn test_record_attempt() {
        let mut detector = GapDetector::new();
        detector.record_attempt(TaskAttempt::new("test task", true));
        assert_eq!(detector.history().len(), 1);
    }

    #[test]
    fn test_clear_history() {
        let mut detector = GapDetector::new();
        detector.record_attempt(TaskAttempt::new("test", true));
        detector.clear_history();
        assert!(detector.history().is_empty());
    }

    #[test]
    fn test_failure_pattern_detection() {
        let detector = GapDetector::new();

        let history = vec![
            TaskAttempt::failed("fetch api data", "HTTP error 401"),
            TaskAttempt::failed("fetch api data from endpoint", "connection refused"),
            TaskAttempt::failed("fetch api data from service", "unauthorized"),
        ];

        let gap = detector.analyze_failure_pattern(&history);
        assert!(gap.is_some());
    }

    #[test]
    fn test_task_attempt_similarity() {
        let a = TaskAttempt::new("fetch data from API", true);
        let b = TaskAttempt::new("fetch user data from API endpoint", true);
        let c = TaskAttempt::new("write file to disk", true);

        let sim_ab = a.similarity(&b);
        let sim_ac = a.similarity(&c);

        assert!(sim_ab > sim_ac, "Similar tasks should have higher similarity");
    }

    #[test]
    fn test_sanitize_capability_name() {
        assert_eq!(sanitize_capability_name("GitHub API"), "github-api");
        assert_eq!(sanitize_capability_name("accessing the stripe payment service."), "accessing-the-stripe");
        assert_eq!(sanitize_capability_name("test"), "test");
    }

    #[test]
    fn test_extract_api_hint() {
        assert_eq!(
            extract_api_hint("Check out https://api.github.com for more info"),
            Some("https://api.github.com".to_string())
        );
        assert_eq!(
            extract_api_hint("The stripe.com API is needed"),
            Some("stripe.com".to_string())
        );
    }

    #[test]
    fn test_failure_stats() {
        let mut detector = GapDetector::new();
        detector.record_attempt(TaskAttempt::new("test1", true));
        detector.record_attempt(TaskAttempt::new("test2", false));
        detector.record_attempt(TaskAttempt::new("test3", true));

        let stats = detector.failure_stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.failures, 1);
        assert_eq!(stats.successes, 2);
    }

    #[test]
    fn test_gap_type_variants() {
        let _types = vec![GapType::Explicit, GapType::Implicit, GapType::Pattern];
    }
}
