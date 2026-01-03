//! Subagent Module - Gas Town Context Compression
//!
//! This module implements the "Gas Town" pattern for context compression
//! at subagent boundaries. When jarvis-rs runs as a Claude Code subagent,
//! it compresses successful results to hide iteration details from the parent.
//!
//! ## Self-Extension Support
//!
//! This module integrates with the capability extension system to allow
//! subagents to synthesize and register new capabilities (MCPs, Skills, Agents)
//! on-the-fly when capability gaps are detected.

use crate::{
    extend::{CapabilityRegistry, CapabilityType, HotLoader, SynthesizedCapability},
    task_loop::TaskLoop,
    verify::Verifier,
    AntiLazinessConfig, CompressedResult, CompressedResultMetadata, Config, SubagentConfig,
    SubagentOutputFormat, SubagentRegistration, TaskResult,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

/// Claude Code settings file structure (subset)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ClaudeSettings {
    #[serde(default)]
    permissions: ClaudePermissions,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    subagents: Vec<SubagentRegistration>,

    #[serde(flatten)]
    other: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ClaudePermissions {
    #[serde(default)]
    allow: Vec<String>,
}

/// Git state snapshot
#[derive(Debug, Clone)]
struct GitState {
    head_commit: Option<String>,
    dirty_files: Vec<PathBuf>,
}

/// Subagent executor for compressed execution
pub struct SubagentExecutor {
    config: SubagentConfig,
    working_dir: PathBuf,
    model: Option<String>,
    /// Optional capability registry for self-extension
    capability_registry: Option<CapabilityRegistry>,
    /// Optional hot loader for registering capabilities
    hot_loader: Option<HotLoader>,
    /// Track capabilities synthesized during execution
    synthesized_capabilities: Vec<SynthesizedCapability>,
}

impl SubagentExecutor {
    /// Create a new subagent executor
    pub fn new(config: SubagentConfig, working_dir: PathBuf) -> Self {
        Self {
            config,
            working_dir,
            model: None,
            capability_registry: None,
            hot_loader: None,
            synthesized_capabilities: Vec::new(),
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }

    /// Enable self-extension capability with a registry and hot loader
    pub fn with_self_extension(mut self, registry: CapabilityRegistry, loader: HotLoader) -> Self {
        self.capability_registry = Some(registry);
        self.hot_loader = Some(loader);
        self
    }

    /// Enable self-extension with default registry and loader
    pub fn with_default_self_extension(mut self) -> Result<Self> {
        let registry = CapabilityRegistry::load_or_create()?;
        let loader = HotLoader::new();
        self.capability_registry = Some(registry);
        self.hot_loader = Some(loader);
        Ok(self)
    }

    /// Get list of capabilities synthesized during execution
    pub fn synthesized_capabilities(&self) -> &[SynthesizedCapability] {
        &self.synthesized_capabilities
    }

    /// Check if self-extension is enabled
    pub fn is_self_extension_enabled(&self) -> bool {
        self.capability_registry.is_some() && self.hot_loader.is_some()
    }

    /// Execute the task and return compressed result
    pub async fn execute(&self, task: &str) -> Result<CompressedResult> {
        let start_time = std::time::Instant::now();

        // Check depth limit
        if self.config.current_depth >= self.config.max_depth {
            return Err(anyhow::anyhow!(
                "Maximum subagent depth ({}) exceeded",
                self.config.max_depth
            ));
        }

        // Capture git state before execution
        let git_before = self.capture_git_state()?;

        // Run the task using existing TaskLoop
        let result = self.run_task(task).await?;

        // Capture git state after execution
        let git_after = self.capture_git_state()?;

        // Compress the result
        let compressed = self.compress_result(
            result,
            git_before,
            git_after,
            start_time.elapsed().as_millis() as u64,
        )?;

        Ok(compressed)
    }

    /// Run the actual task using TaskLoop
    async fn run_task(&self, task: &str) -> Result<TaskResult> {
        let config = Config {
            working_dir: self.working_dir.clone(),
            completion_promise: None,
            model: self.model.clone(),
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };

        let verifier = Verifier::new(None);
        let mut task_loop = TaskLoop::new(config, verifier);

        task_loop.execute(task).await
    }

    /// Capture current git state
    fn capture_git_state(&self) -> Result<GitState> {
        let head_commit = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.working_dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let dirty_files: Vec<PathBuf> = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.working_dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| {
                        let trimmed = l.trim();
                        if trimmed.len() > 3 {
                            Some(PathBuf::from(&trimmed[3..]))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(GitState {
            head_commit,
            dirty_files,
        })
    }

    /// Compress the result according to context policy
    fn compress_result(
        &self,
        result: TaskResult,
        git_before: GitState,
        git_after: GitState,
        duration_ms: u64,
    ) -> Result<CompressedResult> {
        // Determine files changed
        let files_changed = self.compute_files_changed(&git_before, &git_after);

        // Get commit hash if a new commit was made
        let commit_hash = if git_before.head_commit != git_after.head_commit {
            git_after.head_commit.clone()
        } else {
            None
        };

        // Compress summary
        let summary = self.compress_summary(&result);

        Ok(CompressedResult {
            success: result.success,
            commit_hash,
            files_changed,
            summary,
            duration_ms,
            iterations: result.iterations,
            error: if result.success { None } else { result.error },
            verified: result.verification.map(|v| v.is_complete).unwrap_or(false),
            metadata: CompressedResultMetadata {
                version: env!("CARGO_PKG_VERSION").to_string(),
                depth: self.config.current_depth,
                parent_session_id: self.config.parent_session_id.clone(),
                completed_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                model: self.model.clone(),
            },
        })
    }

    /// Compute files changed between two git states
    fn compute_files_changed(&self, before: &GitState, after: &GitState) -> Vec<PathBuf> {
        let mut files = after.dirty_files.clone();

        // If commits differ, get files from git diff
        if before.head_commit != after.head_commit {
            if let (Some(old), Some(new)) = (&before.head_commit, &after.head_commit) {
                if let Ok(output) = Command::new("git")
                    .args(["diff", "--name-only", old, new])
                    .current_dir(&self.working_dir)
                    .output()
                {
                    if output.status.success() {
                        let diff_files: Vec<PathBuf> = String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(PathBuf::from)
                            .collect();
                        files.extend(diff_files);
                    }
                }
            }
        }

        // Deduplicate
        files.sort();
        files.dedup();
        files
    }

    /// Compress summary according to policy
    fn compress_summary(&self, result: &TaskResult) -> String {
        let raw = result.summary.clone().unwrap_or_else(|| {
            if result.success {
                "Task completed successfully".to_string()
            } else {
                "Task failed".to_string()
            }
        });

        // Truncate to max length
        if raw.len() > self.config.context_policy.max_summary_length {
            format!(
                "{}...",
                &raw[..self.config.context_policy.max_summary_length - 3]
            )
        } else {
            raw
        }
    }

    /// Output the compressed result in the configured format
    pub fn output_result(&self, result: &CompressedResult) -> String {
        match self.config.output_format {
            SubagentOutputFormat::Json => {
                serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".to_string())
            }
            SubagentOutputFormat::Text => {
                let status = if result.success { "SUCCESS" } else { "FAILED" };
                let commit = result.commit_hash.as_deref().unwrap_or("none");
                let files = result
                    .files_changed
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                format!(
                    "STATUS: {}\nCOMMIT: {}\nFILES: {}\nSUMMARY: {}",
                    status, commit, files, result.summary
                )
            }
            SubagentOutputFormat::Markdown => {
                let status = if result.success { "Success" } else { "Failed" };
                let commit = result.commit_hash.as_deref().unwrap_or("none");

                let mut md = format!("## Subagent Result: {}\n\n", status);
                md.push_str(&format!("**Commit**: `{}`\n\n", commit));
                md.push_str("**Files Changed**:\n");
                for file in &result.files_changed {
                    md.push_str(&format!("- `{}`\n", file.display()));
                }
                md.push_str(&format!("\n**Summary**: {}\n", result.summary));
                md.push_str(&format!(
                    "\n**Duration**: {}ms | **Iterations**: {}\n",
                    result.duration_ms, result.iterations
                ));
                md
            }
        }
    }

    /// Detect capability gaps from task execution output
    ///
    /// Analyzes task output to identify missing capabilities that could
    /// be synthesized and registered.
    pub fn detect_capability_gaps(&self, output: &str) -> Vec<DetectedGap> {
        let mut gaps = Vec::new();

        // Patterns that suggest MCP capability gaps
        let mcp_patterns = [
            ("No MCP server", "database", CapabilityType::Mcp),
            ("MCP not available", "generic", CapabilityType::Mcp),
            ("cannot connect to", "external_service", CapabilityType::Mcp),
            ("API not configured", "api_integration", CapabilityType::Mcp),
            ("tool not found", "tool", CapabilityType::Mcp),
        ];

        // Patterns that suggest Skill capability gaps
        let skill_patterns = [
            ("don't know how to", "knowledge", CapabilityType::Skill),
            ("no skill for", "skill", CapabilityType::Skill),
            ("unfamiliar with", "domain", CapabilityType::Skill),
            ("need expertise in", "expertise", CapabilityType::Skill),
        ];

        // Patterns that suggest Agent capability gaps
        let agent_patterns = [
            ("need specialized agent", "agent", CapabilityType::Agent),
            ("requires autonomous", "autonomous", CapabilityType::Agent),
            ("delegate to", "delegation", CapabilityType::Agent),
        ];

        let output_lower = output.to_lowercase();

        // Check MCP patterns
        for (pattern, category, cap_type) in mcp_patterns {
            if output_lower.contains(&pattern.to_lowercase()) {
                gaps.push(DetectedGap {
                    capability_type: cap_type,
                    category: category.to_string(),
                    description: format!("Missing capability detected: {}", pattern),
                    suggested_name: None,
                    context: self.extract_context(output, pattern),
                });
            }
        }

        // Check Skill patterns
        for (pattern, category, cap_type) in skill_patterns {
            if output_lower.contains(&pattern.to_lowercase()) {
                gaps.push(DetectedGap {
                    capability_type: cap_type,
                    category: category.to_string(),
                    description: format!("Missing capability detected: {}", pattern),
                    suggested_name: None,
                    context: self.extract_context(output, pattern),
                });
            }
        }

        // Check Agent patterns
        for (pattern, category, cap_type) in agent_patterns {
            if output_lower.contains(&pattern.to_lowercase()) {
                gaps.push(DetectedGap {
                    capability_type: cap_type,
                    category: category.to_string(),
                    description: format!("Missing capability detected: {}", pattern),
                    suggested_name: None,
                    context: self.extract_context(output, pattern),
                });
            }
        }

        gaps
    }

    /// Extract surrounding context for a pattern match
    fn extract_context(&self, output: &str, pattern: &str) -> String {
        let output_lower = output.to_lowercase();
        let pattern_lower = pattern.to_lowercase();

        if let Some(pos) = output_lower.find(&pattern_lower) {
            let start = pos.saturating_sub(100);
            let end = (pos + pattern.len() + 100).min(output.len());
            output[start..end].to_string()
        } else {
            String::new()
        }
    }

    /// Register a synthesized capability via the hot loader
    pub async fn register_capability(
        &mut self,
        capability: &SynthesizedCapability,
    ) -> Result<CapabilityRegistrationResult> {
        let loader = self
            .hot_loader
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Self-extension not enabled"))?;

        let registry = self
            .capability_registry
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Self-extension not enabled"))?;

        // Register via hot loader (sync operations)
        let load_result = match capability.capability_type {
            CapabilityType::Mcp => loader.register_mcp(capability),
            CapabilityType::Skill => loader.register_skill(capability),
            CapabilityType::Agent => loader.register_agent(capability),
        }?;

        if load_result.success {
            // Add to registry
            registry.register(capability.clone())?;
            registry.save()?;

            // Track synthesized capability
            self.synthesized_capabilities.push(capability.clone());

            info!(
                "Registered capability: {} ({})",
                capability.name,
                capability.capability_type.as_str()
            );

            Ok(CapabilityRegistrationResult {
                success: true,
                message: load_result.message,
                restart_required: load_result.restart_required,
                capability_id: Some(capability.id.clone()),
            })
        } else {
            warn!(
                "Failed to register capability {}: {}",
                capability.name, load_result.message
            );

            Ok(CapabilityRegistrationResult {
                success: false,
                message: load_result.message,
                restart_required: false,
                capability_id: None,
            })
        }
    }

    /// Unregister a capability by ID
    pub async fn unregister_capability(&mut self, capability_id: &str) -> Result<bool> {
        let loader = self
            .hot_loader
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Self-extension not enabled"))?;

        let registry = self
            .capability_registry
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Self-extension not enabled"))?;

        // Find capability in registry
        let capability = registry
            .find_by_id(capability_id)
            .ok_or_else(|| anyhow::anyhow!("Capability not found: {}", capability_id))?
            .clone();

        // Unregister via hot loader (sync operations, return LoadResult)
        let load_result = match capability.capability_type {
            CapabilityType::Mcp => loader.unregister_mcp(&capability.name)?,
            CapabilityType::Skill => loader.unregister_skill(&capability.name)?,
            CapabilityType::Agent => loader.unregister_agent(&capability.name)?,
        };

        if load_result.success {
            registry.unregister(capability_id)?;
            registry.save()?;
            info!("Unregistered capability: {}", capability.name);
        }

        Ok(load_result.success)
    }

    /// List all registered capabilities
    pub fn list_capabilities(&self) -> Vec<&SynthesizedCapability> {
        match &self.capability_registry {
            Some(registry) => registry.list_all(),
            None => Vec::new(),
        }
    }

    /// Find capabilities by type
    pub fn find_capabilities_by_type(
        &self,
        capability_type: CapabilityType,
    ) -> Vec<&SynthesizedCapability> {
        match &self.capability_registry {
            Some(registry) => registry.find_by_type(capability_type),
            None => Vec::new(),
        }
    }
}

/// Represents a detected capability gap from task output analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedGap {
    /// Type of capability that's missing
    pub capability_type: CapabilityType,
    /// Category of the gap (e.g., "database", "api", "skill")
    pub category: String,
    /// Human-readable description
    pub description: String,
    /// Suggested name for the capability
    pub suggested_name: Option<String>,
    /// Surrounding context from the output
    pub context: String,
}

/// Result of capability registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRegistrationResult {
    /// Whether registration succeeded
    pub success: bool,
    /// Status message
    pub message: String,
    /// Whether a restart is required (for MCPs)
    pub restart_required: bool,
    /// ID of the registered capability (if successful)
    pub capability_id: Option<String>,
}

/// Registration scope
pub enum RegistrationScope {
    /// Register in ~/.claude/settings.json
    Global,
    /// Register in .claude/settings.json within the given path
    Project(PathBuf),
}

/// Register jarvis-rs as a Claude Code subagent
pub fn register_subagent(scope: RegistrationScope) -> Result<()> {
    let registration = SubagentRegistration::default();

    match scope {
        RegistrationScope::Global => {
            register_to_global_settings(&registration)?;
        }
        RegistrationScope::Project(path) => {
            register_to_project_settings(&registration, &path)?;
        }
    }

    Ok(())
}

/// Register to global Claude settings
fn register_to_global_settings(registration: &SubagentRegistration) -> Result<()> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let settings_path = PathBuf::from(home).join(".claude").join("settings.json");

    update_settings_file(&settings_path, registration)
}

/// Register to project-level Claude settings
fn register_to_project_settings(
    registration: &SubagentRegistration,
    project_path: &Path,
) -> Result<()> {
    let settings_path = project_path.join(".claude").join("settings.json");

    // Create .claude directory if it doesn't exist
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }

    update_settings_file(&settings_path, registration)
}

/// Update a settings file with the subagent registration
fn update_settings_file(path: &Path, registration: &SubagentRegistration) -> Result<()> {
    let mut settings: ClaudeSettings = if path.exists() {
        let content = fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        ClaudeSettings::default()
    };

    // Remove existing jarvis-rs registration if present
    settings.subagents.retain(|s| s.name != "jarvis-rs");

    // Add new registration
    settings.subagents.push(registration.clone());

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write back
    let content = serde_json::to_string_pretty(&settings)?;
    fs::write(path, content)?;

    info!(
        "Registered jarvis-rs as Claude Code subagent at {:?}",
        path
    );
    Ok(())
}

/// Unregister jarvis-rs from Claude Code
pub fn unregister_subagent(scope: RegistrationScope) -> Result<()> {
    let path = match scope {
        RegistrationScope::Global => {
            let home = std::env::var("HOME").context("HOME not set")?;
            PathBuf::from(home).join(".claude").join("settings.json")
        }
        RegistrationScope::Project(p) => p.join(".claude").join("settings.json"),
    };

    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&path)?;
    let mut settings: ClaudeSettings = serde_json::from_str(&content).unwrap_or_default();

    settings.subagents.retain(|s| s.name != "jarvis-rs");

    let content = serde_json::to_string_pretty(&settings)?;
    fs::write(&path, content)?;

    info!("Unregistered jarvis-rs from {:?}", path);
    Ok(())
}

/// Spawn a child jarvis-rs subagent (for true recursion)
pub async fn spawn_subagent(
    task: &str,
    working_dir: &Path,
    parent_config: &SubagentConfig,
    model: Option<&str>,
) -> Result<CompressedResult> {
    // Check depth limit
    let new_depth = parent_config.current_depth + 1;
    if new_depth >= parent_config.max_depth {
        return Err(anyhow::anyhow!(
            "Maximum subagent depth ({}) exceeded",
            parent_config.max_depth
        ));
    }

    let child_config = SubagentConfig {
        task: task.to_string(),
        parent_session_id: parent_config.parent_session_id.clone(),
        max_depth: parent_config.max_depth,
        current_depth: new_depth,
        context_policy: parent_config.context_policy.clone(),
        output_format: SubagentOutputFormat::Json,
    };

    let config_json = serde_json::to_string(&child_config)?;

    // Build command
    let mut cmd = Command::new("jarvis-rs");
    cmd.arg("--subagent-mode");
    cmd.arg("--subagent-config");
    cmd.arg(&config_json);

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    cmd.arg("--");
    cmd.arg(task);
    cmd.current_dir(working_dir);

    let output = cmd.output().context("Failed to spawn child subagent")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Child subagent failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: CompressedResult =
        serde_json::from_str(&stdout).context("Failed to parse child subagent output")?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContextPolicy;

    #[test]
    fn test_subagent_registration_default() {
        let reg = SubagentRegistration::default();
        assert_eq!(reg.name, "jarvis-rs");
        assert!(reg.triggers.contains(&"jarvis".to_string()));
        assert!(reg.capabilities.contains(&"context_compression".to_string()));
    }

    #[test]
    fn test_context_policy_default() {
        let policy = ContextPolicy::default();
        assert!(policy.hide_failures);
        assert!(policy.hide_intermediates);
        assert!(policy.include_commit);
    }

    #[test]
    fn test_compressed_result_serialization() {
        let result = CompressedResult {
            success: true,
            commit_hash: Some("abc1234".to_string()),
            files_changed: vec![PathBuf::from("src/lib.rs")],
            summary: "Implemented feature".to_string(),
            duration_ms: 5000,
            iterations: 3,
            error: None,
            verified: true,
            metadata: CompressedResultMetadata {
                version: "0.1.0".to_string(),
                depth: 0,
                parent_session_id: None,
                completed_at: 0,
                model: None,
            },
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("abc1234"));
        assert!(json.contains("src/lib.rs"));
    }

    #[test]
    fn test_subagent_output_text_format() {
        let config = SubagentConfig {
            output_format: SubagentOutputFormat::Text,
            ..Default::default()
        };
        let executor = SubagentExecutor::new(config, PathBuf::from("."));

        let result = CompressedResult {
            success: true,
            commit_hash: Some("abc1234".to_string()),
            files_changed: vec![PathBuf::from("src/lib.rs")],
            summary: "Test summary".to_string(),
            duration_ms: 1000,
            iterations: 1,
            error: None,
            verified: true,
            metadata: CompressedResultMetadata {
                version: "0.1.0".to_string(),
                depth: 0,
                parent_session_id: None,
                completed_at: 0,
                model: None,
            },
        };

        let output = executor.output_result(&result);
        assert!(output.contains("STATUS: SUCCESS"));
        assert!(output.contains("COMMIT: abc1234"));
        assert!(output.contains("src/lib.rs"));
    }

    #[test]
    fn test_subagent_output_markdown_format() {
        let config = SubagentConfig {
            output_format: SubagentOutputFormat::Markdown,
            ..Default::default()
        };
        let executor = SubagentExecutor::new(config, PathBuf::from("."));

        let result = CompressedResult {
            success: false,
            commit_hash: None,
            files_changed: vec![],
            summary: "Failed to complete".to_string(),
            duration_ms: 500,
            iterations: 2,
            error: Some("Test error".to_string()),
            verified: false,
            metadata: CompressedResultMetadata {
                version: "0.1.0".to_string(),
                depth: 1,
                parent_session_id: Some("parent-123".to_string()),
                completed_at: 0,
                model: None,
            },
        };

        let output = executor.output_result(&result);
        assert!(output.contains("## Subagent Result: Failed"));
        assert!(output.contains("**Commit**: `none`"));
    }
}
