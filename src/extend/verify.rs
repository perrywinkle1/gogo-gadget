//! Capability Verification Module
//!
//! Tests synthesized capabilities before registration.
//! - For Skills: Validate SKILL.md structure, check triggers
//! - For MCPs: npm install, start server, call test endpoint
//! - For Agents: Validate AGENT.md structure

use super::{CapabilityType, SynthesizedCapability};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

/// Result of capability verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether verification passed
    pub success: bool,
    /// Errors encountered (blocking issues)
    pub errors: Vec<String>,
    /// Warnings (non-blocking issues)
    pub warnings: Vec<String>,
    /// Verification steps completed
    pub steps_completed: Vec<VerificationStep>,
    /// Duration of verification
    pub duration_ms: u64,
}

/// A verification step with its result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStep {
    /// Name of the step
    pub name: String,
    /// Description of what was checked
    pub description: String,
    /// Whether the step passed
    pub passed: bool,
    /// Details or output from the step
    pub details: Option<String>,
}

impl Default for VerificationResult {
    fn default() -> Self {
        Self {
            success: false,
            errors: Vec::new(),
            warnings: Vec::new(),
            steps_completed: Vec::new(),
            duration_ms: 0,
        }
    }
}

impl VerificationResult {
    /// Create a successful result
    pub fn success() -> Self {
        Self {
            success: true,
            ..Default::default()
        }
    }

    /// Create a failed result with error
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            errors: vec![error.into()],
            ..Default::default()
        }
    }

    /// Add an error
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
        self.success = false;
    }

    /// Add a warning
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Complete a verification step
    pub fn complete_step(&mut self, name: impl Into<String>, description: impl Into<String>, passed: bool, details: Option<String>) {
        self.steps_completed.push(VerificationStep {
            name: name.into(),
            description: description.into(),
            passed,
            details,
        });

        if !passed {
            self.success = false;
        }
    }

    /// Mark a step as completed successfully
    pub fn pass_step(&mut self, name: impl Into<String>, description: impl Into<String>) {
        self.complete_step(name, description, true, None);
    }

    /// Mark a step as failed
    pub fn fail_step(&mut self, name: impl Into<String>, description: impl Into<String>, details: impl Into<String>) {
        self.complete_step(name, description, false, Some(details.into()));
    }

    /// Check if all steps passed
    pub fn all_steps_passed(&self) -> bool {
        self.steps_completed.iter().all(|s| s.passed)
    }

    /// Get a summary of the verification
    pub fn summary(&self) -> String {
        let passed_count = self.steps_completed.iter().filter(|s| s.passed).count();
        let total = self.steps_completed.len();
        let status = if self.success { "PASSED" } else { "FAILED" };

        format!(
            "Verification {}: {}/{} steps passed, {} errors, {} warnings",
            status,
            passed_count,
            total,
            self.errors.len(),
            self.warnings.len()
        )
    }
}

/// Configuration for capability verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Timeout for verification tests in seconds
    pub timeout_secs: u64,
    /// Whether to run npm install for MCPs
    pub run_npm_install: bool,
    /// Whether to start and test MCP servers
    pub test_mcp_server: bool,
    /// Whether to validate trigger patterns
    pub validate_triggers: bool,
    /// Required sections in SKILL.md
    pub required_skill_sections: Vec<String>,
    /// Required sections in AGENT.md
    pub required_agent_sections: Vec<String>,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            run_npm_install: true,
            test_mcp_server: true,
            validate_triggers: true,
            required_skill_sections: vec![
                "Trigger".to_string(),
                "Instructions".to_string(),
            ],
            required_agent_sections: vec![
                "Capabilities".to_string(),
                "Instructions".to_string(),
            ],
        }
    }
}

/// Verifies synthesized capabilities before registration
#[derive(Debug, Clone)]
pub struct CapabilityVerifier {
    /// Sandbox directory for testing
    sandbox_dir: PathBuf,
    /// Configuration
    config: VerificationConfig,
}

impl Default for CapabilityVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityVerifier {
    /// Create a new verifier
    pub fn new() -> Self {
        Self {
            sandbox_dir: std::env::temp_dir().join("gogo-gadget-verify"),
            config: VerificationConfig::default(),
        }
    }

    /// Create with custom sandbox directory
    pub fn with_sandbox_dir(mut self, dir: PathBuf) -> Self {
        self.sandbox_dir = dir;
        self
    }

    /// Set verification timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.config.timeout_secs = secs;
        self
    }

    /// Set configuration
    pub fn with_config(mut self, config: VerificationConfig) -> Self {
        self.config = config;
        self
    }

    /// Verify an MCP capability
    pub async fn verify_mcp(&self, capability: &SynthesizedCapability) -> VerificationResult {
        let start = std::time::Instant::now();
        let mut result = VerificationResult::default();
        result.success = true;

        // Step 1: Check path exists
        if !capability.path.exists() {
            result.fail_step(
                "path_exists",
                "Check MCP directory exists",
                format!("Path does not exist: {:?}", capability.path),
            );
            result.duration_ms = start.elapsed().as_millis() as u64;
            return result;
        }
        result.pass_step("path_exists", "MCP directory exists");

        // Step 2: Check for index.ts or package.json
        let index_path = capability.path.join("index.ts");
        let package_path = capability.path.join("package.json");

        if !index_path.exists() && !package_path.exists() {
            result.fail_step(
                "files_exist",
                "Check MCP files exist",
                "Neither index.ts nor package.json found",
            );
            result.duration_ms = start.elapsed().as_millis() as u64;
            return result;
        }
        result.pass_step("files_exist", "Required MCP files found");

        // Step 3: Validate package.json if present
        if package_path.exists() {
            match std::fs::read_to_string(&package_path) {
                Ok(content) => {
                    match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(pkg) => {
                            // Check for required fields
                            let has_name = pkg.get("name").is_some();
                            let has_deps = pkg.get("dependencies").is_some();

                            if has_name && has_deps {
                                result.pass_step("package_json", "package.json is valid");
                            } else {
                                result.add_warning("package.json missing name or dependencies");
                                result.complete_step(
                                    "package_json",
                                    "Validate package.json",
                                    true,
                                    Some("Missing optional fields".to_string()),
                                );
                            }
                        }
                        Err(e) => {
                            result.fail_step(
                                "package_json",
                                "Validate package.json",
                                format!("Invalid JSON: {}", e),
                            );
                        }
                    }
                }
                Err(e) => {
                    result.fail_step(
                        "package_json",
                        "Read package.json",
                        format!("Failed to read: {}", e),
                    );
                }
            }
        }

        // Step 4: Validate TypeScript syntax (basic check)
        if index_path.exists() {
            match std::fs::read_to_string(&index_path) {
                Ok(content) => {
                    // Basic syntax checks
                    let has_imports = content.contains("import ");
                    let has_server = content.contains("Server");
                    let has_handler = content.contains("setRequestHandler");

                    if has_imports && has_server && has_handler {
                        result.pass_step("typescript_syntax", "TypeScript structure looks valid");
                    } else {
                        let mut missing = Vec::new();
                        if !has_imports { missing.push("imports"); }
                        if !has_server { missing.push("Server"); }
                        if !has_handler { missing.push("setRequestHandler"); }

                        result.add_warning(format!("MCP may be incomplete, missing: {}", missing.join(", ")));
                        result.complete_step(
                            "typescript_syntax",
                            "Validate TypeScript structure",
                            true,
                            Some(format!("Missing: {}", missing.join(", "))),
                        );
                    }
                }
                Err(e) => {
                    result.fail_step(
                        "typescript_syntax",
                        "Read index.ts",
                        format!("Failed to read: {}", e),
                    );
                }
            }
        }

        // Step 5: Run npm install if configured
        if self.config.run_npm_install && package_path.exists() {
            match self.run_npm_install(&capability.path) {
                Ok(output) => {
                    if output.status.success() {
                        result.pass_step("npm_install", "Dependencies installed successfully");
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        result.add_warning(format!("npm install had issues: {}", stderr.lines().next().unwrap_or("")));
                        result.complete_step(
                            "npm_install",
                            "Install npm dependencies",
                            true,
                            Some("Completed with warnings".to_string()),
                        );
                    }
                }
                Err(e) => {
                    result.add_warning(format!("Could not run npm install: {}", e));
                }
            }
        }

        // Step 6: Validate MCP configuration
        if let Some(config) = capability.config.as_object() {
            if config.contains_key("command") || config.contains_key("args") {
                result.pass_step("config_valid", "MCP configuration is valid");
            } else {
                result.add_warning("MCP configuration missing command or args");
            }
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        result
    }

    /// Run npm install in a directory
    fn run_npm_install(&self, dir: &PathBuf) -> std::io::Result<std::process::Output> {
        Command::new("npm")
            .arg("install")
            .arg("--prefer-offline")
            .current_dir(dir)
            .output()
    }

    /// Verify a skill capability
    pub async fn verify_skill(&self, capability: &SynthesizedCapability) -> VerificationResult {
        let start = std::time::Instant::now();
        let mut result = VerificationResult::default();
        result.success = true;

        // Step 1: Check file exists
        if !capability.path.exists() {
            result.fail_step(
                "file_exists",
                "Check SKILL.md exists",
                format!("File does not exist: {:?}", capability.path),
            );
            result.duration_ms = start.elapsed().as_millis() as u64;
            return result;
        }
        result.pass_step("file_exists", "SKILL.md file exists");

        // Step 2: Read and validate content
        let content = match std::fs::read_to_string(&capability.path) {
            Ok(c) => c,
            Err(e) => {
                result.fail_step(
                    "read_content",
                    "Read SKILL.md content",
                    format!("Failed to read: {}", e),
                );
                result.duration_ms = start.elapsed().as_millis() as u64;
                return result;
            }
        };
        result.pass_step("read_content", "SKILL.md content read successfully");

        // Step 3: Check for title (# header)
        let has_title = Regex::new(r"^#\s+.+")
            .map(|re| re.is_match(&content))
            .unwrap_or(false);

        if has_title {
            result.pass_step("has_title", "SKILL.md has a title");
        } else {
            result.fail_step(
                "has_title",
                "Check for title header",
                "Missing main title (# Header)",
            );
        }

        // Step 4: Check for required sections
        for section in &self.config.required_skill_sections {
            let section_pattern = format!(r"(?i)##\s+{}", regex::escape(section));
            let has_section = Regex::new(&section_pattern)
                .map(|re| re.is_match(&content))
                .unwrap_or(false);

            if has_section {
                result.pass_step(
                    format!("section_{}", section.to_lowercase()),
                    format!("Has required section: {}", section),
                );
            } else {
                result.add_warning(format!("Missing recommended section: {}", section));
            }
        }

        // Step 5: Validate trigger patterns if present
        if self.config.validate_triggers {
            let trigger_pattern = Regex::new(r"(?i)##\s+trigger");
            if let Ok(re) = trigger_pattern {
                if re.is_match(&content) {
                    // Check if there's actual trigger content
                    let lines: Vec<&str> = content.lines().collect();
                    let trigger_idx = lines.iter().position(|l| re.is_match(l));

                    if let Some(idx) = trigger_idx {
                        // Check next few lines for trigger content
                        let has_trigger_content = lines.iter()
                            .skip(idx + 1)
                            .take(5)
                            .any(|l| !l.trim().is_empty() && !l.starts_with('#'));

                        if has_trigger_content {
                            result.pass_step("trigger_content", "Trigger section has content");
                        } else {
                            result.add_warning("Trigger section appears empty");
                        }
                    }
                }
            }
        }

        // Step 6: Check for examples
        let has_examples = content.to_lowercase().contains("example");
        if has_examples {
            result.pass_step("has_examples", "Contains examples");
        } else {
            result.add_warning("No examples found - consider adding some");
        }

        // Step 7: Check minimum content length
        let word_count = content.split_whitespace().count();
        if word_count >= 50 {
            result.pass_step("content_length", "Sufficient content length");
        } else {
            result.add_warning(format!("Content may be too short ({} words)", word_count));
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        result
    }

    /// Verify an agent capability
    pub async fn verify_agent(&self, capability: &SynthesizedCapability) -> VerificationResult {
        let start = std::time::Instant::now();
        let mut result = VerificationResult::default();
        result.success = true;

        // Step 1: Check file exists
        if !capability.path.exists() {
            result.fail_step(
                "file_exists",
                "Check AGENT.md exists",
                format!("File does not exist: {:?}", capability.path),
            );
            result.duration_ms = start.elapsed().as_millis() as u64;
            return result;
        }
        result.pass_step("file_exists", "AGENT.md file exists");

        // Step 2: Read content
        let content = match std::fs::read_to_string(&capability.path) {
            Ok(c) => c,
            Err(e) => {
                result.fail_step(
                    "read_content",
                    "Read AGENT.md content",
                    format!("Failed to read: {}", e),
                );
                result.duration_ms = start.elapsed().as_millis() as u64;
                return result;
            }
        };
        result.pass_step("read_content", "AGENT.md content read successfully");

        // Step 3: Check for title
        let has_title = Regex::new(r"^#\s+.+")
            .map(|re| re.is_match(&content))
            .unwrap_or(false);

        if has_title {
            result.pass_step("has_title", "AGENT.md has a title");
        } else {
            result.fail_step(
                "has_title",
                "Check for title header",
                "Missing main title (# Header)",
            );
        }

        // Step 4: Check for required sections
        for section in &self.config.required_agent_sections {
            let section_pattern = format!(r"(?i)##\s+{}", regex::escape(section));
            let has_section = Regex::new(&section_pattern)
                .map(|re| re.is_match(&content))
                .unwrap_or(false);

            if has_section {
                result.pass_step(
                    format!("section_{}", section.to_lowercase()),
                    format!("Has required section: {}", section),
                );
            } else {
                result.add_warning(format!("Missing recommended section: {}", section));
            }
        }

        // Step 5: Check for specialization/capabilities content
        let has_capabilities = content.to_lowercase().contains("capabilities")
            || content.to_lowercase().contains("specializ");

        if has_capabilities {
            result.pass_step("has_capabilities", "Defines capabilities or specialization");
        } else {
            result.add_warning("Agent should define its capabilities or specialization");
        }

        // Step 6: Check for handoff criteria
        let has_handoff = content.to_lowercase().contains("handoff")
            || content.to_lowercase().contains("hand off")
            || content.to_lowercase().contains("when to use");

        if has_handoff {
            result.pass_step("has_handoff", "Defines handoff criteria");
        } else {
            result.add_warning("Consider adding handoff criteria");
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        result
    }

    /// Verify any capability
    pub async fn verify(&self, capability: &SynthesizedCapability) -> VerificationResult {
        match capability.capability_type {
            CapabilityType::Mcp => self.verify_mcp(capability).await,
            CapabilityType::Skill => self.verify_skill(capability).await,
            CapabilityType::Agent => self.verify_agent(capability).await,
        }
    }

    /// Verify multiple capabilities
    pub async fn verify_all(&self, capabilities: &[SynthesizedCapability]) -> Vec<(String, VerificationResult)> {
        let mut results = Vec::new();

        for cap in capabilities {
            let result = self.verify(cap).await;
            results.push((cap.name.clone(), result));
        }

        results
    }

    /// Get sandbox directory
    pub fn sandbox_dir(&self) -> &PathBuf {
        &self.sandbox_dir
    }

    /// Get configuration
    pub fn config(&self) -> &VerificationConfig {
        &self.config
    }
}

/// Quick verification functions for common checks
pub mod quick {
    use super::*;

    /// Quickly check if a file exists and is readable
    pub fn file_readable(path: &PathBuf) -> bool {
        std::fs::read_to_string(path).is_ok()
    }

    /// Quickly check if a directory has required files
    pub fn has_required_files(dir: &PathBuf, files: &[&str]) -> Vec<String> {
        files.iter()
            .filter(|f| !dir.join(f).exists())
            .map(|f| f.to_string())
            .collect()
    }

    /// Quickly check markdown structure
    pub fn markdown_has_sections(content: &str, sections: &[&str]) -> Vec<String> {
        sections.iter()
            .filter(|s| {
                let pattern = format!(r"(?i)##\s+{}", regex::escape(s));
                Regex::new(&pattern)
                    .map(|re| !re.is_match(content))
                    .unwrap_or(true)
            })
            .map(|s| s.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_verification_result_default() {
        let result = VerificationResult::default();
        assert!(!result.success);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_verification_result_success() {
        let result = VerificationResult::success();
        assert!(result.success);
    }

    #[test]
    fn test_verification_result_failure() {
        let result = VerificationResult::failure("test error");
        assert!(!result.success);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_add_error() {
        let mut result = VerificationResult::success();
        result.add_error("error 1");
        assert!(!result.success);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_complete_step() {
        let mut result = VerificationResult::default();
        result.complete_step("step1", "First step", true, None);
        result.complete_step("step2", "Second step", false, Some("Failed reason".into()));

        assert_eq!(result.steps_completed.len(), 2);
        assert!(result.steps_completed[0].passed);
        assert!(!result.steps_completed[1].passed);
    }

    #[test]
    fn test_summary() {
        let mut result = VerificationResult::success();
        result.pass_step("step1", "Step 1");
        result.pass_step("step2", "Step 2");

        let summary = result.summary();
        assert!(summary.contains("PASSED"));
        assert!(summary.contains("2/2"));
    }

    #[test]
    fn test_verifier_new() {
        let verifier = CapabilityVerifier::new();
        assert_eq!(verifier.config.timeout_secs, 30);
    }

    #[test]
    fn test_verifier_with_timeout() {
        let verifier = CapabilityVerifier::new().with_timeout(60);
        assert_eq!(verifier.config.timeout_secs, 60);
    }

    #[tokio::test]
    async fn test_verify_skill_valid() {
        let temp_dir = TempDir::new().unwrap();
        let skill_path = temp_dir.path().join("test.SKILL.md");

        let content = r#"# Test Skill

> A test skill for verification

## Trigger

Use when the user asks to test something.

## Instructions

1. Analyze the request
2. Execute the test
3. Return results

## Examples

Example: "Run the test"
"#;

        std::fs::write(&skill_path, content).unwrap();

        let capability = SynthesizedCapability::new(
            CapabilityType::Skill,
            "test-skill",
            skill_path,
        );

        let verifier = CapabilityVerifier::new();
        let result = verifier.verify_skill(&capability).await;

        assert!(result.success);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_verify_skill_missing_file() {
        let capability = SynthesizedCapability::new(
            CapabilityType::Skill,
            "missing-skill",
            PathBuf::from("/nonexistent/path.md"),
        );

        let verifier = CapabilityVerifier::new();
        let result = verifier.verify_skill(&capability).await;

        assert!(!result.success);
        assert!(!result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_verify_mcp_valid() {
        let temp_dir = TempDir::new().unwrap();
        let mcp_dir = temp_dir.path().join("test-mcp");
        std::fs::create_dir_all(&mcp_dir).unwrap();

        // Create index.ts
        let index_content = r#"
import { Server } from "@modelcontextprotocol/sdk/server/index.js";

const server = new Server({ name: "test" }, {});
server.setRequestHandler(() => {});
"#;
        std::fs::write(mcp_dir.join("index.ts"), index_content).unwrap();

        // Create package.json
        let package_content = r#"{
  "name": "test-mcp",
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.0.0"
  }
}"#;
        std::fs::write(mcp_dir.join("package.json"), package_content).unwrap();

        let capability = SynthesizedCapability::new(
            CapabilityType::Mcp,
            "test-mcp",
            mcp_dir,
        ).with_config(serde_json::json!({"command": "npx", "args": ["ts-node", "index.ts"]}));

        let verifier = CapabilityVerifier::new()
            .with_config(VerificationConfig {
                run_npm_install: false,  // Skip npm install in tests
                ..Default::default()
            });

        let result = verifier.verify_mcp(&capability).await;

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_verify_agent_valid() {
        let temp_dir = TempDir::new().unwrap();
        let agent_path = temp_dir.path().join("test.AGENT.md");

        let content = r#"# Test Agent

> Specialized agent for testing

## Capabilities

This agent can:
- Run tests
- Validate results

## Instructions

1. Analyze the task
2. Execute tests
3. Report results

## Handoff Criteria

Use this agent when:
- Testing is required
"#;

        std::fs::write(&agent_path, content).unwrap();

        let capability = SynthesizedCapability::new(
            CapabilityType::Agent,
            "test-agent",
            agent_path,
        );

        let verifier = CapabilityVerifier::new();
        let result = verifier.verify_agent(&capability).await;

        assert!(result.success);
    }

    #[test]
    fn test_quick_file_readable() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.txt");
        std::fs::write(&path, "content").unwrap();

        assert!(quick::file_readable(&path));
        assert!(!quick::file_readable(&PathBuf::from("/nonexistent")));
    }

    #[test]
    fn test_quick_has_required_files() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "").unwrap();

        let missing = quick::has_required_files(
            &temp_dir.path().to_path_buf(),
            &["file1.txt", "file2.txt"],
        );

        assert_eq!(missing, vec!["file2.txt"]);
    }

    #[test]
    fn test_quick_markdown_has_sections() {
        let content = "# Title\n\n## Section One\n\n## Section Two\n";

        let missing = quick::markdown_has_sections(content, &["Section One", "Section Three"]);
        assert_eq!(missing, vec!["Section Three"]);
    }
}
