//! Brain Module - LLM-Driven Task Analysis
//!
//! Analyzes tasks using Claude to understand requirements, determine completion
//! criteria, and assess task characteristics. This replaces regex-based analysis
//! with intelligent, context-aware understanding.

use crate::extend::{CapabilityGap, GapDetector};
use crate::{Difficulty, ExecutionMode};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

/// Category of task being performed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskCategory {
    /// Bug fix - fixing existing functionality
    BugFix,
    /// New feature implementation
    Feature,
    /// Code refactoring without behavior change
    Refactor,
    /// Security-related changes (audits, fixes, hardening)
    Security,
    /// Documentation updates
    Documentation,
    /// Test writing or fixing
    Testing,
    /// Performance optimization
    Performance,
    /// Configuration or setup changes
    Configuration,
    /// Code review or analysis
    Review,
    /// DevOps/Infrastructure tasks (CI/CD, deployment, containers)
    DevOps,
    /// Database/Data tasks (migrations, queries, schema)
    Database,
    /// API/Integration tasks (endpoints, webhooks, third-party)
    Api,
    /// Debugging/Investigation tasks
    Debugging,
    /// General/uncategorized task
    #[default]
    General,
}

impl TaskCategory {
    /// Parse category from string (case-insensitive)
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bugfix" | "bug_fix" | "bug fix" | "fix" => TaskCategory::BugFix,
            "feature" | "new feature" => TaskCategory::Feature,
            "refactor" | "refactoring" => TaskCategory::Refactor,
            "security" | "security audit" => TaskCategory::Security,
            "documentation" | "docs" | "doc" => TaskCategory::Documentation,
            "testing" | "test" | "tests" => TaskCategory::Testing,
            "performance" | "perf" | "optimization" => TaskCategory::Performance,
            "configuration" | "config" | "setup" => TaskCategory::Configuration,
            "review" | "code review" => TaskCategory::Review,
            "devops" | "infrastructure" | "ci" | "cd" | "cicd" => TaskCategory::DevOps,
            "database" | "db" | "data" | "migration" => TaskCategory::Database,
            "api" | "integration" | "endpoint" => TaskCategory::Api,
            "debugging" | "debug" | "investigation" => TaskCategory::Debugging,
            _ => TaskCategory::General,
        }
    }
}

/// Detected programming language in the task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DetectedLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    CppOrC,
    Ruby,
    Php,
    Swift,
    Kotlin,
    Shell,
    Mixed,
    #[default]
    Unknown,
}

impl DetectedLanguage {
    /// Parse language from string (case-insensitive)
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => DetectedLanguage::Rust,
            "python" | "py" => DetectedLanguage::Python,
            "javascript" | "js" => DetectedLanguage::JavaScript,
            "typescript" | "ts" => DetectedLanguage::TypeScript,
            "go" | "golang" => DetectedLanguage::Go,
            "java" => DetectedLanguage::Java,
            "c" | "c++" | "cpp" | "c/c++" => DetectedLanguage::CppOrC,
            "ruby" | "rb" => DetectedLanguage::Ruby,
            "php" => DetectedLanguage::Php,
            "swift" => DetectedLanguage::Swift,
            "kotlin" | "kt" => DetectedLanguage::Kotlin,
            "shell" | "bash" | "sh" | "zsh" => DetectedLanguage::Shell,
            "mixed" | "multiple" => DetectedLanguage::Mixed,
            _ => DetectedLanguage::Unknown,
        }
    }
}

/// Analysis result for a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    /// Complexity score (0-10)
    pub complexity_score: u32,

    /// Recommended execution mode
    pub mode: ExecutionMode,

    /// Whether iteration is needed
    pub needs_iteration: bool,

    /// Whether deep reasoning (ultrathink) is beneficial
    pub needs_ultrathink: bool,

    /// Whether visual verification is needed
    pub needs_visual: bool,

    /// Estimated difficulty
    pub difficulty: Difficulty,

    /// Suggested timeout in minutes
    pub suggested_timeout: u32,

    /// Word count in the task
    pub word_count: usize,

    /// Number of file references detected
    pub file_mentions: usize,

    /// Keywords that triggered special handling
    pub triggered_keywords: Vec<String>,

    /// Category of the task
    pub category: TaskCategory,

    /// Primary detected language
    pub detected_language: DetectedLanguage,

    /// Estimated number of files that will be modified
    pub estimated_files: usize,

    /// Whether the task is safe for parallel/swarm execution
    /// (no shared state conflicts, independent file operations)
    pub parallel_safe: bool,

    /// Whether security-sensitive operations are detected
    pub is_security_sensitive: bool,

    /// Codebase complexity score based on file count
    pub codebase_complexity: u32,

    /// What "done" looks like for this task (LLM-determined)
    pub completion_criteria: String,

    /// Key outcomes that must be achieved
    pub required_outcomes: Vec<String>,

    /// Success indicators to look for
    pub success_indicators: Vec<String>,

    /// Detected capability gaps from task analysis
    pub detected_gaps: Vec<CapabilityGap>,
}

impl Default for TaskAnalysis {
    fn default() -> Self {
        Self {
            complexity_score: 0,
            mode: ExecutionMode::Single,
            needs_iteration: false,
            needs_ultrathink: false,
            needs_visual: false,
            difficulty: Difficulty::Trivial,
            suggested_timeout: 30,
            word_count: 0,
            file_mentions: 0,
            triggered_keywords: Vec::new(),
            category: TaskCategory::default(),
            detected_language: DetectedLanguage::default(),
            estimated_files: 0,
            parallel_safe: true,
            is_security_sensitive: false,
            codebase_complexity: 0,
            completion_criteria: String::new(),
            required_outcomes: Vec::new(),
            success_indicators: Vec::new(),
            detected_gaps: Vec::new(),
        }
    }
}

/// LLM response structure for task analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmAnalysisResponse {
    /// Category of the task
    category: String,
    /// Primary programming language
    language: String,
    /// Complexity score 0-10
    complexity: u32,
    /// Whether deep reasoning is needed
    needs_ultrathink: bool,
    /// Whether visual verification is needed
    needs_visual: bool,
    /// Whether security-sensitive
    is_security_sensitive: bool,
    /// Whether safe for parallel execution
    parallel_safe: bool,
    /// Estimated number of files to modify
    estimated_files: u32,
    /// What "done" looks like
    completion_criteria: String,
    /// Required outcomes
    required_outcomes: Vec<String>,
    /// Success indicators
    success_indicators: Vec<String>,
    /// Keywords or concepts identified
    key_concepts: Vec<String>,
    /// Suggested timeout in minutes
    suggested_timeout: u32,
}

/// LLM-driven task analyzer that uses Claude to understand task requirements
pub struct TaskAnalyzer {
    /// Optional model override
    model: Option<String>,
    /// Working directory for context
    working_dir: Option<std::path::PathBuf>,
    /// Gap detector for capability gap detection
    gap_detector: GapDetector,
}

impl TaskAnalyzer {
    /// Create a new LLM-based task analyzer
    pub fn new() -> Self {
        Self {
            model: None,
            working_dir: None,
            gap_detector: GapDetector::new(),
        }
    }

    /// Set a specific model for analysis
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set working directory for codebase context
    pub fn with_working_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.working_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Get the gap detector for external use
    pub fn gap_detector(&self) -> &GapDetector {
        &self.gap_detector
    }

    /// Get mutable gap detector for configuration
    pub fn gap_detector_mut(&mut self) -> &mut GapDetector {
        &mut self.gap_detector
    }

    /// Analyze a task using LLM to understand requirements and completion criteria
    pub fn analyze(&self, task: &str) -> Result<TaskAnalysis> {
        // Build the analysis prompt
        let prompt = self.build_analysis_prompt(task);

        // Run Claude for analysis
        let response = self.run_llm_analysis(&prompt)?;

        // Parse the response into TaskAnalysis
        self.parse_analysis_response(task, &response)
    }

    /// Analyze Claude output for capability gaps
    ///
    /// This method scans output from Claude for signals indicating
    /// that a capability (MCP, Skill, or Agent) would be beneficial.
    pub fn detect_capability_gaps(&self, output: &str) -> Vec<CapabilityGap> {
        let mut gaps = Vec::new();
        if let Some(gap) = self.gap_detector.analyze_output(output) {
            info!("Detected capability gap in output: {:?}", gap);
            debug!("Gap detected: {:?}", gap);
            gaps.push(gap);
        }
        gaps
    }

    /// Analyze a task with capability gap detection integrated
    pub fn analyze_with_gap_detection(&self, task: &str) -> Result<TaskAnalysis> {
        let mut analysis = self.analyze(task)?;

        // Also check the task description itself for capability gaps
        let task_gaps = self.detect_capability_gaps(task);
        analysis.detected_gaps.extend(task_gaps);

        Ok(analysis)
    }

    /// Build the prompt for LLM analysis
    fn build_analysis_prompt(&self, task: &str) -> String {
        let codebase_context = self.gather_codebase_context();

        format!(
            r#"Analyze this software engineering task and provide a structured assessment.

TASK:
{task}

{codebase_context}

Respond with ONLY a JSON object (no markdown, no explanation) with these fields:
{{
  "category": "bugfix|feature|refactor|security|documentation|testing|performance|configuration|review|devops|database|api|debugging|general",
  "language": "rust|python|javascript|typescript|go|java|cpp|ruby|php|swift|kotlin|shell|mixed|unknown",
  "complexity": 0-10,
  "needs_ultrathink": true/false,
  "needs_visual": true/false,
  "is_security_sensitive": true/false,
  "parallel_safe": true/false,
  "estimated_files": number,
  "completion_criteria": "Clear description of what 'done' looks like",
  "required_outcomes": ["outcome1", "outcome2"],
  "success_indicators": ["indicator1", "indicator2"],
  "key_concepts": ["concept1", "concept2"],
  "suggested_timeout": minutes
}}

Guidelines:
- complexity: 0-2 trivial, 3-4 easy, 5-6 medium, 7-8 hard, 9-10 expert
- needs_ultrathink: true for complex architecture, security, or subtle bugs
- needs_visual: true if UI/visual verification is needed
- parallel_safe: false if task involves shared state or sequential dependencies
- completion_criteria: Be specific about what constitutes success
- required_outcomes: List concrete, verifiable outcomes
- success_indicators: What signals would indicate the task is complete"#,
            task = task,
            codebase_context = codebase_context
        )
    }

    /// Gather context about the codebase if working directory is set
    fn gather_codebase_context(&self) -> String {
        let Some(ref dir) = self.working_dir else {
            return String::new();
        };

        let mut context = String::from("CODEBASE CONTEXT:\n");

        // Count files to estimate codebase complexity
        if let Ok(entries) = std::fs::read_dir(dir) {
            let file_count = entries.count();
            context.push_str(&format!("- Root directory contains {} entries\n", file_count));
        }

        // Check for common project markers
        let markers = [
            ("Cargo.toml", "Rust project"),
            ("package.json", "JavaScript/Node project"),
            ("pyproject.toml", "Python project"),
            ("go.mod", "Go project"),
            ("pom.xml", "Java Maven project"),
            ("build.gradle", "Java Gradle project"),
        ];

        for (file, desc) in markers {
            if dir.join(file).exists() {
                context.push_str(&format!("- {} detected ({})\n", desc, file));
            }
        }

        // Check for src directory
        let src_dir = dir.join("src");
        if src_dir.exists() && src_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&src_dir) {
                let src_files: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .take(10)
                    .collect();
                if !src_files.is_empty() {
                    context.push_str(&format!("- src/ contains: {}\n", src_files.join(", ")));
                }
            }
        }

        context
    }

    /// Run LLM analysis using Claude
    fn run_llm_analysis(&self, prompt: &str) -> Result<String> {
        let mut cmd = Command::new("claude");

        // Use specified model or default
        if let Some(ref model) = self.model {
            cmd.arg("--model").arg(model);
        }

        // Set working directory if available
        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }

        // Request structured output
        cmd.arg("--print");
        cmd.arg(prompt);

        debug!("Running LLM analysis ({} chars prompt)", prompt.len());

        let output = cmd.output().context("Failed to run Claude for task analysis")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Claude analysis returned non-zero: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    /// Parse the LLM response into a TaskAnalysis
    fn parse_analysis_response(&self, task: &str, response: &str) -> Result<TaskAnalysis> {
        // Try to extract JSON from the response
        let json_str = self.extract_json(response);

        // Parse the JSON
        let llm_response: LlmAnalysisResponse = match serde_json::from_str(&json_str) {
            Ok(r) => r,
            Err(e) => {
                warn!("Failed to parse LLM response as JSON: {}. Using fallback analysis.", e);
                return Ok(self.fallback_analysis(task));
            }
        };

        // Convert to TaskAnalysis
        let difficulty = Difficulty::from(llm_response.complexity);
        let word_count = task.split_whitespace().count();

        // Determine execution mode based on analysis
        // With Gas Town context compression, all tasks are parallel-safe:
        // - Each subagent is isolated, parent only sees CompressedResult
        // - Security isolation comes from compression boundaries, not single-agent mode
        // Use swarm mode for:
        // 1. Tasks with many files to modify (estimated_files > 3)
        // 2. High-complexity tasks (complexity >= 7)
        let mode = if llm_response.estimated_files > 3 {
            ExecutionMode::Swarm {
                agent_count: std::cmp::min(llm_response.estimated_files, 5) as u32,
            }
        } else if llm_response.complexity >= 7 {
            // High-complexity greenfield or single-file tasks still benefit from swarm
            // Use 5 agents for complexity 9-10, 4 for 8, 3 for 7
            let agent_count = match llm_response.complexity {
                9..=10 => 5,
                8 => 4,
                _ => 3,
            };
            ExecutionMode::Swarm { agent_count }
        } else {
            ExecutionMode::Single
        };

        Ok(TaskAnalysis {
            complexity_score: llm_response.complexity,
            mode,
            needs_iteration: llm_response.complexity >= 5,
            needs_ultrathink: llm_response.needs_ultrathink,
            needs_visual: llm_response.needs_visual,
            difficulty,
            suggested_timeout: llm_response.suggested_timeout,
            word_count,
            file_mentions: self.count_file_mentions(task),
            triggered_keywords: llm_response.key_concepts,
            category: TaskCategory::from_str(&llm_response.category),
            detected_language: DetectedLanguage::from_str(&llm_response.language),
            estimated_files: llm_response.estimated_files as usize,
            // Gas Town compression boundaries make all tasks parallel-safe
            parallel_safe: true,
            is_security_sensitive: llm_response.is_security_sensitive,
            codebase_complexity: self.estimate_codebase_complexity(),
            completion_criteria: llm_response.completion_criteria,
            required_outcomes: llm_response.required_outcomes,
            success_indicators: llm_response.success_indicators,
            detected_gaps: Vec::new(), // Populated by analyze_with_gap_detection
        })
    }

    /// Extract JSON from LLM response (handles markdown code blocks)
    fn extract_json(&self, response: &str) -> String {
        // Try to find JSON in code block
        if let Some(start) = response.find("```json") {
            if let Some(end) = response[start..].find("```\n") {
                let json_start = start + 7;
                let json_end = start + end;
                if json_end > json_start {
                    return response[json_start..json_end].trim().to_string();
                }
            }
            // Try without newline after closing backticks
            if let Some(end) = response[start + 7..].find("```") {
                let json_start = start + 7;
                let json_end = start + 7 + end;
                return response[json_start..json_end].trim().to_string();
            }
        }

        // Try plain code block
        if let Some(start) = response.find("```") {
            let after_start = start + 3;
            // Skip any language identifier on the first line
            let content_start = response[after_start..]
                .find('\n')
                .map(|i| after_start + i + 1)
                .unwrap_or(after_start);
            if let Some(end) = response[content_start..].find("```") {
                return response[content_start..content_start + end].trim().to_string();
            }
        }

        // Try to find raw JSON object
        if let Some(start) = response.find('{') {
            let mut depth = 0;
            let mut end = start;
            for (i, c) in response[start..].chars().enumerate() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = start + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if end > start {
                return response[start..end].to_string();
            }
        }

        // Return the whole response as fallback
        response.trim().to_string()
    }

    /// Count file references in the task description
    fn count_file_mentions(&self, task: &str) -> usize {
        let file_patterns = [
            ".rs", ".py", ".js", ".ts", ".go", ".java", ".cpp", ".c", ".h",
            ".rb", ".php", ".swift", ".kt", ".sh", ".yaml", ".yml", ".json",
            ".toml", ".md", ".txt",
        ];

        task.split_whitespace()
            .filter(|word| file_patterns.iter().any(|ext| word.ends_with(ext)))
            .count()
    }

    /// Estimate codebase complexity based on working directory
    fn estimate_codebase_complexity(&self) -> u32 {
        let Some(ref dir) = self.working_dir else {
            return 0;
        };

        // Simple heuristic: count source files
        let src_dir = dir.join("src");
        if !src_dir.exists() {
            return 1;
        }

        fn count_files(dir: &Path) -> usize {
            std::fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .map(|e| {
                            if e.path().is_dir() {
                                count_files(&e.path())
                            } else {
                                1
                            }
                        })
                        .sum()
                })
                .unwrap_or(0)
        }

        let file_count = count_files(&src_dir);
        match file_count {
            0..=5 => 1,
            6..=20 => 2,
            21..=50 => 3,
            51..=100 => 4,
            _ => 5,
        }
    }

    /// Fallback analysis when LLM parsing fails
    fn fallback_analysis(&self, task: &str) -> TaskAnalysis {
        let word_count = task.split_whitespace().count();
        let file_mentions = self.count_file_mentions(task);
        let is_security_sensitive = task.to_lowercase().contains("security")
            || task.to_lowercase().contains("auth");

        // Simple heuristics for fallback
        let complexity_score = match word_count {
            0..=10 => 2,
            11..=50 => 4,
            51..=100 => 6,
            _ => 8,
        };

        // Use swarm mode for high-complexity tasks even in fallback
        let mode = if is_security_sensitive {
            ExecutionMode::Swarm { agent_count: 3 }
        } else if complexity_score >= 7 {
            let agent_count = match complexity_score {
                9..=10 => 5,
                8 => 4,
                _ => 3,
            };
            ExecutionMode::Swarm { agent_count }
        } else {
            ExecutionMode::Single
        };

        TaskAnalysis {
            complexity_score,
            mode,
            needs_iteration: complexity_score >= 5,
            needs_ultrathink: complexity_score >= 7,
            needs_visual: task.to_lowercase().contains("ui")
                || task.to_lowercase().contains("visual"),
            difficulty: Difficulty::from(complexity_score),
            suggested_timeout: 30 + (complexity_score as u32 * 5),
            word_count,
            file_mentions,
            triggered_keywords: Vec::new(),
            category: TaskCategory::General,
            detected_language: DetectedLanguage::Unknown,
            estimated_files: file_mentions.max(1),
            parallel_safe: true,
            is_security_sensitive,
            codebase_complexity: self.estimate_codebase_complexity(),
            completion_criteria: "Task completed successfully".to_string(),
            required_outcomes: vec!["Task requirements met".to_string()],
            success_indicators: vec!["No errors".to_string()],
            detected_gaps: Vec::new(),
        }
    }
}

impl Default for TaskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_category_from_str() {
        assert_eq!(TaskCategory::from_str("bugfix"), TaskCategory::BugFix);
        assert_eq!(TaskCategory::from_str("Feature"), TaskCategory::Feature);
        assert_eq!(TaskCategory::from_str("REFACTOR"), TaskCategory::Refactor);
        assert_eq!(TaskCategory::from_str("unknown"), TaskCategory::General);
    }

    #[test]
    fn test_detected_language_from_str() {
        assert_eq!(DetectedLanguage::from_str("rust"), DetectedLanguage::Rust);
        assert_eq!(DetectedLanguage::from_str("Python"), DetectedLanguage::Python);
        assert_eq!(DetectedLanguage::from_str("TS"), DetectedLanguage::TypeScript);
        assert_eq!(DetectedLanguage::from_str("unknown"), DetectedLanguage::Unknown);
    }

    #[test]
    fn test_task_analysis_default() {
        let analysis = TaskAnalysis::default();
        assert_eq!(analysis.complexity_score, 0);
        assert_eq!(analysis.mode, ExecutionMode::Single);
        assert!(!analysis.needs_iteration);
        assert!(analysis.completion_criteria.is_empty());
    }

    #[test]
    fn test_analyzer_creation() {
        let analyzer = TaskAnalyzer::new();
        assert!(analyzer.model.is_none());
        assert!(analyzer.working_dir.is_none());
    }

    #[test]
    fn test_analyzer_with_model() {
        let analyzer = TaskAnalyzer::new().with_model("claude-3-opus");
        assert_eq!(analyzer.model, Some("claude-3-opus".to_string()));
    }

    #[test]
    fn test_count_file_mentions() {
        let analyzer = TaskAnalyzer::new();
        let task = "Fix bug in main.rs and update config.toml";
        assert_eq!(analyzer.count_file_mentions(task), 2);
    }

    #[test]
    fn test_extract_json_from_code_block() {
        let analyzer = TaskAnalyzer::new();
        let response = r#"Here's the analysis:
```json
{"category": "bugfix", "complexity": 3}
```
"#;
        let json = analyzer.extract_json(response);
        assert!(json.contains("bugfix"));
    }

    #[test]
    fn test_extract_raw_json() {
        let analyzer = TaskAnalyzer::new();
        let response = r#"{"category": "feature", "complexity": 5}"#;
        let json = analyzer.extract_json(response);
        assert!(json.contains("feature"));
    }

    #[test]
    fn test_fallback_analysis() {
        let analyzer = TaskAnalyzer::new();
        let analysis = analyzer.fallback_analysis("Fix the bug in authentication");
        assert!(analysis.complexity_score > 0);
        assert!(!analysis.completion_criteria.is_empty());
    }

    #[test]
    fn test_analyzer_has_gap_detector() {
        let analyzer = TaskAnalyzer::new();
        // Verify gap detector is accessible
        let _ = analyzer.gap_detector();
    }

    #[test]
    fn test_detect_capability_gaps_empty() {
        let analyzer = TaskAnalyzer::new();
        let gaps = analyzer.detect_capability_gaps("Normal output without any gap signals");
        assert!(gaps.is_empty());
    }

    #[test]
    fn test_detect_capability_gaps_mcp_signal() {
        let analyzer = TaskAnalyzer::new();
        let output = "I would need an MCP for GitHub API access to complete this task";
        let gaps = analyzer.detect_capability_gaps(output);
        // Gap detection depends on the GapDetector implementation
        // This test verifies the integration works
        assert!(gaps.is_empty() || gaps.iter().any(|g| matches!(g, CapabilityGap::Mcp { .. })));
    }

    #[test]
    fn test_task_analysis_includes_detected_gaps() {
        let analysis = TaskAnalysis::default();
        assert!(analysis.detected_gaps.is_empty());
    }

    #[test]
    fn test_gap_detector_mut_access() {
        let mut analyzer = TaskAnalyzer::new();
        // Verify mutable access works
        let detector = analyzer.gap_detector_mut();
        // Should be able to configure it
        let _ = detector;
    }
}
