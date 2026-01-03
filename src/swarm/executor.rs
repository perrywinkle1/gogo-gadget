//! Swarm Executor
//!
//! Spawns and manages multiple Claude agent processes in parallel.

use super::{AgentResult, Subtask, SwarmConfig, SwarmConflict, SwarmResult};
use crate::subagent::spawn_subagent;
use crate::{ProgressCallback, ShutdownSignal, SubagentConfig, TaskResult};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, error, info, warn};

/// Executes multiple Claude agents in parallel
pub struct SwarmExecutor {
    /// Swarm configuration
    config: SwarmConfig,
    /// Shutdown signal for graceful termination
    shutdown_signal: Option<ShutdownSignal>,
    /// Progress callback for reporting
    progress_callback: Option<Arc<dyn ProgressCallback>>,
}

impl SwarmExecutor {
    /// Create a new swarm executor
    pub fn new(config: SwarmConfig) -> Self {
        Self {
            config,
            shutdown_signal: None,
            progress_callback: None,
        }
    }

    /// Set shutdown signal handler
    pub fn with_shutdown_signal(mut self, signal: ShutdownSignal) -> Self {
        self.shutdown_signal = Some(signal);
        self
    }

    /// Set progress callback
    pub fn with_progress_callback(mut self, callback: Arc<dyn ProgressCallback>) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Check if shutdown was requested
    fn is_shutdown_requested(&self) -> bool {
        self.shutdown_signal
            .as_ref()
            .map(|s| s.is_shutdown())
            .unwrap_or(false)
    }

    /// Execute all subtasks in parallel
    pub async fn execute(&self, subtasks: Vec<Subtask>) -> Result<SwarmResult> {
        let start_time = Instant::now();

        info!(
            "Starting swarm execution with {} agents",
            self.config.agent_count
        );

        println!();
        println!("═══════════════════════════════════════════════════════════════");
        println!(
            "  🐝 SWARM MODE: Spawning {} parallel Claude agents",
            self.config.agent_count
        );
        println!("═══════════════════════════════════════════════════════════════");
        println!();

        // Semaphore to limit concurrent agents
        let semaphore = Arc::new(Semaphore::new(self.config.agent_count as usize));

        // Shared state for results
        let results: Arc<Mutex<Vec<AgentResult>>> = Arc::new(Mutex::new(Vec::new()));
        let files_modified: Arc<Mutex<HashMap<PathBuf, Vec<u32>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Spawn all agents
        let mut handles = Vec::new();

        for subtask in subtasks {
            // Check for shutdown before spawning
            if self.is_shutdown_requested() {
                warn!("Shutdown requested, not spawning more agents");
                break;
            }

            let permit = semaphore.clone().acquire_owned().await?;
            let results = results.clone();
            let files_modified = files_modified.clone();
            let config = self.config.clone();
            let shutdown_signal = self.shutdown_signal.clone();

            let handle = tokio::spawn(async move {
                let agent_result =
                    Self::run_agent(subtask, &config, shutdown_signal.as_ref()).await;

                // Record result
                let mut results_guard = results.lock().await;
                if let Ok(ref result) = agent_result {
                    // Track files modified by this agent
                    let mut files_guard = files_modified.lock().await;
                    for file in &result.files_modified {
                        files_guard
                            .entry(file.clone())
                            .or_insert_with(Vec::new)
                            .push(result.agent_id);
                    }
                    results_guard.push(result.clone());
                }

                drop(permit);
                agent_result
            });

            handles.push(handle);
        }

        // Wait for all agents to complete
        let mut agent_errors = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    error!("Agent error: {}", e);
                    agent_errors.push(e.to_string());
                }
                Err(e) => {
                    error!("Agent task panicked: {}", e);
                    agent_errors.push(format!("Task panicked: {}", e));
                }
            }
        }

        // Collect results
        let agent_results = Arc::try_unwrap(results)
            .expect("All handles completed")
            .into_inner();

        // Detect conflicts
        let files_map = Arc::try_unwrap(files_modified)
            .expect("All handles completed")
            .into_inner();
        let conflicts = Self::detect_conflicts(&files_map);

        // Calculate statistics
        let successful_agents = agent_results.iter().filter(|r| r.result.success).count() as u32;
        let failed_agents = agent_results.len() as u32 - successful_agents;
        let total_duration_ms = start_time.elapsed().as_millis() as u64;

        // Generate combined summary
        let summary = Self::generate_summary(&agent_results, &conflicts);

        // Determine overall success
        let success = successful_agents > 0
            && failed_agents == 0
            && conflicts.iter().all(|c| c.resolved)
            && agent_errors.is_empty();

        println!();
        println!("═══════════════════════════════════════════════════════════════");
        println!(
            "  🐝 SWARM COMPLETE: {}/{} agents succeeded ({:.1}s)",
            successful_agents,
            agent_results.len(),
            total_duration_ms as f64 / 1000.0
        );
        if !conflicts.is_empty() {
            println!("  ⚠️  {} conflicts detected", conflicts.len());
        }
        println!("═══════════════════════════════════════════════════════════════");
        println!();

        Ok(SwarmResult {
            success,
            agent_results,
            summary: Some(summary),
            conflicts,
            total_duration_ms,
            successful_agents,
            failed_agents,
            verification_passed: success, // Will be updated by coordinator
            compressed_output: None,
        })
    }

    /// Run a single agent with a subtask
    async fn run_agent(
        subtask: Subtask,
        config: &SwarmConfig,
        shutdown_signal: Option<&ShutdownSignal>,
    ) -> Result<AgentResult> {
        let start_time = Instant::now();
        let agent_id = subtask.id;

        info!("Agent {} starting: {}", agent_id, subtask.focus);
        println!(
            "  🤖 Agent {} started: {}",
            agent_id + 1,
            subtask.focus
        );

        // Check for shutdown
        if shutdown_signal.map(|s| s.is_shutdown()).unwrap_or(false) {
            return Ok(AgentResult {
                agent_id,
                subtask: subtask.description.clone(),
                result: TaskResult {
                    success: false,
                    iterations: 0,
                    summary: Some("Cancelled due to shutdown".to_string()),
                    error: Some("Shutdown requested".to_string()),
                    verification: None,
                },
                files_modified: vec![],
                duration_ms: 0,
                compressed_result: None,
            });
        }

        // Check if we should use subagent mode (Gas Town pattern)
        if subtask.use_subagent_mode && config.subagent_mode.enabled {
            return Self::run_agent_with_subagent(subtask, config, start_time).await;
        }

        // Build the enhanced prompt for this agent
        let prompt = Self::build_agent_prompt(&subtask, config);

        // Run Claude
        let result = Self::run_claude(&prompt, config).await;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                let (task_result, files_modified) = Self::parse_agent_output(&output, agent_id);

                let success_indicator = if task_result.success { "✅" } else { "❌" };
                println!(
                    "  {} Agent {} completed in {:.1}s",
                    success_indicator,
                    agent_id + 1,
                    duration_ms as f64 / 1000.0
                );

                info!(
                    "Agent {} completed: success={}, files={}",
                    agent_id,
                    task_result.success,
                    files_modified.len()
                );

                Ok(AgentResult {
                    agent_id,
                    subtask: subtask.description,
                    result: task_result,
                    files_modified,
                    duration_ms,
                    compressed_result: None,
                })
            }
            Err(e) => {
                error!("Agent {} failed: {}", agent_id, e);
                println!("  ❌ Agent {} failed: {}", agent_id + 1, e);

                Ok(AgentResult {
                    agent_id,
                    subtask: subtask.description,
                    result: TaskResult {
                        success: false,
                        iterations: 0,
                        summary: None,
                        error: Some(e.to_string()),
                        verification: None,
                    },
                    files_modified: vec![],
                    duration_ms,
                    compressed_result: None,
                })
            }
        }
    }

    /// Run an agent using subagent mode (Gas Town context compression)
    async fn run_agent_with_subagent(
        subtask: Subtask,
        config: &SwarmConfig,
        start_time: Instant,
    ) -> Result<AgentResult> {
        let agent_id = subtask.id;

        info!("Agent {} using subagent mode for context compression", agent_id);
        println!(
            "  🔄 Agent {} using subagent mode (Gas Town compression)",
            agent_id + 1
        );

        // Create subagent configuration
        let subagent_config = SubagentConfig {
            task: subtask.description.clone(),
            parent_session_id: config.subagent_mode.parent_session_id.clone(),
            max_depth: config.subagent_mode.max_depth,
            current_depth: subtask.subagent_depth,
            context_policy: config.subagent_mode.context_policy.clone(),
            output_format: config.subagent_mode.output_format,
        };

        // Spawn the subagent
        let compressed_result = spawn_subagent(
            &subtask.description,
            &config.working_dir,
            &subagent_config,
            config.model.as_deref(),
        )
        .await;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        match compressed_result {
            Ok(compressed) => {
                let success = compressed.success;
                let files_modified = compressed.files_changed.clone();
                let summary = Some(compressed.summary.clone());

                let success_indicator = if success { "✅" } else { "❌" };
                println!(
                    "  {} Agent {} (subagent) completed in {:.1}s",
                    success_indicator,
                    agent_id + 1,
                    duration_ms as f64 / 1000.0
                );

                info!(
                    "Agent {} subagent completed: success={}, files={}, iterations={}",
                    agent_id,
                    success,
                    files_modified.len(),
                    compressed.iterations
                );

                Ok(AgentResult {
                    agent_id,
                    subtask: subtask.description,
                    result: TaskResult {
                        success,
                        iterations: compressed.iterations,
                        summary,
                        error: compressed.error.clone(),
                        verification: None,
                    },
                    files_modified,
                    duration_ms,
                    compressed_result: Some(compressed),
                })
            }
            Err(e) => {
                error!("Agent {} subagent failed: {}", agent_id, e);
                println!("  ❌ Agent {} (subagent) failed: {}", agent_id + 1, e);

                Ok(AgentResult {
                    agent_id,
                    subtask: subtask.description,
                    result: TaskResult {
                        success: false,
                        iterations: 0,
                        summary: None,
                        error: Some(e.to_string()),
                        verification: None,
                    },
                    files_modified: vec![],
                    duration_ms,
                    compressed_result: None,
                })
            }
        }
    }

    /// Build the prompt for an agent
    fn build_agent_prompt(subtask: &Subtask, config: &SwarmConfig) -> String {
        let mut prompt = String::new();

        // Add anti-laziness preamble if enabled
        if config.anti_laziness.enabled {
            prompt.push_str(&format!(
                r#"## Agent {} of {} - Quality Standards

You are evaluated on EVIDENCE of real work:
- Mock data shortcuts will be detected and rejected
- Each agent must show concrete progress
- Confidence claims must be backed by evidence
- Minimum confidence for completion: {:.0}%

## COMPLETION STANDARDS (Non-Negotiable)

You MUST satisfy ALL before claiming completion:

1. **NO MOCK DATA**
   - No "TODO:", "FIXME:", "XXX:" comments
   - No placeholder values: "John Doe", "test@example.com"
   - No unimplemented stubs: unimplemented!(), todo!()

2. **NO DEBUG CODE**
   - No println!(), dbg!() in production code
   - Debug only in test modules

3. **NO SHORTCUTS**
   - No hardcoded secrets
   - No commented-out code
   - Proper error handling throughout

4. **EVIDENCE REQUIRED** (Level: {:?})
   Your completion claim must include:
   - Files modified with line counts
   - Verification steps performed
   - Test results if applicable

---

"#,
                subtask.id + 1,
                config.agent_count,
                config.anti_laziness.min_confidence * 100.0,
                config.anti_laziness.evidence_level
            ));
        }

        prompt.push_str(&subtask.description);

        // Add context about being part of a swarm
        prompt.push_str(&format!(
            "\n\n---\n\
             **SWARM EXECUTION MODE**\n\
             You are Agent {} of {} working in parallel on this task.\n\
             Focus on your specific area and avoid conflicts with other agents.\n",
            subtask.id + 1,
            config.agent_count
        ));

        if !subtask.target_files.is_empty() {
            let files: Vec<_> = subtask
                .target_files
                .iter()
                .map(|f| f.display().to_string())
                .collect();
            prompt.push_str(&format!(
                "\n**Your assigned files**: {}\n\
                 Only modify these files to avoid conflicts.\n",
                files.join(", ")
            ));
        }

        if let Some(ref context) = subtask.shared_context {
            prompt.push_str(&format!(
                "\n**Context from other agents**:\n{}\n",
                context
            ));
        }

        // Add completion instructions
        // NOTE: Agents report individual completion via JSON - orchestrator controls overall swarm completion
        prompt.push_str(
            "\n\n**Completion Instructions**:\n\
             - Do NOT create .jarvis-satisfied or any signal files - the orchestrator handles swarm completion\n\
             - Be thorough but focused on your specific area\n\
             - Document what you changed for coordination with other agents\n\
             - When you finish your part, include a JSON summary block:\n\
             ```json\n\
             {\n\
               \"status\": \"complete\",\n\
               \"summary\": \"What you accomplished\",\n\
               \"files_modified\": [\"file1.rs\", \"file2.rs\"],\n\
               \"confidence\": 0.9\n\
             }\n\
             ```\n\
             - Set confidence to 0.9+ if your part is truly complete and working\n\
             - Set confidence lower if you encountered issues or work remains\n",
        );

        prompt
    }

    /// Run Claude with the given prompt
    async fn run_claude(prompt: &str, config: &SwarmConfig) -> Result<String> {
        debug!("Running Claude agent with prompt ({} chars)", prompt.len());

        let mut cmd = Command::new("claude");

        // Add model flag if specified
        if let Some(ref model) = config.model {
            cmd.arg("--model").arg(model);
        }

        // Skip permissions for autonomous operation
        cmd.arg("--dangerously-skip-permissions");

        // Use print mode to capture output properly (not interactive)
        cmd.arg("--print");

        // Set working directory
        cmd.current_dir(&config.working_dir);

        // Set max turns for bounded execution
        cmd.arg("--max-turns").arg("50");

        // Add the prompt as positional argument
        cmd.arg("--");
        cmd.arg(prompt);

        let output = cmd.output().context("Failed to execute Claude agent")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            warn!("Claude agent exited with status: {}", output.status);
        }

        // Combine stdout and stderr
        let full_output = if stderr.is_empty() {
            stdout
        } else {
            format!("{}\n\n--- STDERR ---\n{}", stdout, stderr)
        };

        Ok(full_output)
    }

    /// Parse agent output to extract result and modified files
    fn parse_agent_output(output: &str, agent_id: u32) -> (TaskResult, Vec<PathBuf>) {
        let mut files_modified = Vec::new();
        let mut success = false;
        let mut summary = None;
        let mut confidence = 0.0f32;

        // Try to parse JSON block from output
        // Look for ```json block and extract the JSON content
        if let Some(json_start) = output.find("```json") {
            let content_start = json_start + 7; // Skip "```json"
            let remaining = &output[content_start..];
            // Find closing ``` - could be followed by newline, space, or end of string
            let json_end = remaining.find("\n```")
                .or_else(|| remaining.find("```"))
                .unwrap_or(remaining.len());
            let json_str = remaining[..json_end].trim();
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(status) = parsed.get("status").and_then(|s| s.as_str()) {
                    success = status == "complete";
                }
                if let Some(s) = parsed.get("summary").and_then(|s| s.as_str()) {
                    summary = Some(s.to_string());
                }
                if let Some(c) = parsed.get("confidence").and_then(|c| c.as_f64()) {
                    confidence = c as f32;
                }
                if let Some(files) = parsed.get("files_modified").and_then(|f| f.as_array()) {
                    for file in files {
                        if let Some(f) = file.as_str() {
                            files_modified.push(PathBuf::from(f));
                        }
                    }
                }
            }
        }

        // Agent success is determined by confidence score in their JSON output
        // NOTE: Agents should NOT create .jarvis-satisfied - only the orchestrator decides overall completion
        // Use confidence to adjust success
        if confidence >= 0.8 {
            success = true;
        }

        let task_result = TaskResult {
            success,
            iterations: 1,
            summary,
            error: if success {
                None
            } else {
                Some(format!("Agent {} did not complete successfully", agent_id))
            },
            verification: None,
        };

        (task_result, files_modified)
    }

    /// Detect conflicts between agent outputs
    fn detect_conflicts(files_map: &HashMap<PathBuf, Vec<u32>>) -> Vec<SwarmConflict> {
        let mut conflicts = Vec::new();

        for (file, agents) in files_map {
            if agents.len() > 1 {
                conflicts.push(SwarmConflict {
                    file: file.clone(),
                    agent_ids: agents.clone(),
                    description: format!(
                        "File {} was modified by agents: {:?}",
                        file.display(),
                        agents
                    ),
                    resolved: false, // Will be resolved by coordinator
                });
            }
        }

        conflicts
    }

    /// Generate a combined summary from all agent results
    fn generate_summary(results: &[AgentResult], conflicts: &[SwarmConflict]) -> String {
        let mut summary = String::new();

        summary.push_str("## Swarm Execution Summary\n\n");

        // Agent summaries
        summary.push_str("### Agent Results\n\n");
        for result in results {
            let status = if result.result.success { "✅" } else { "❌" };
            summary.push_str(&format!(
                "- **Agent {}** {}: {}\n",
                result.agent_id + 1,
                status,
                result
                    .result
                    .summary
                    .as_deref()
                    .unwrap_or("No summary provided")
            ));

            if !result.files_modified.is_empty() {
                summary.push_str("  - Files: ");
                summary.push_str(
                    &result
                        .files_modified
                        .iter()
                        .map(|f| f.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                summary.push('\n');
            }
        }

        // Conflicts
        if !conflicts.is_empty() {
            summary.push_str("\n### Conflicts Detected\n\n");
            for conflict in conflicts {
                let status = if conflict.resolved {
                    "✅ Resolved"
                } else {
                    "⚠️ Unresolved"
                };
                summary.push_str(&format!(
                    "- {} - {}: Agents {:?}\n",
                    conflict.file.display(),
                    status,
                    conflict.agent_ids
                ));
            }
        }

        // Statistics
        let successful = results.iter().filter(|r| r.result.success).count();
        let total = results.len();
        let total_files: HashSet<_> = results
            .iter()
            .flat_map(|r| r.files_modified.iter())
            .collect();

        summary.push_str(&format!(
            "\n### Statistics\n\n\
             - Agents: {}/{} successful\n\
             - Total files modified: {}\n\
             - Conflicts: {}\n",
            successful,
            total,
            total_files.len(),
            conflicts.len()
        ));

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_agent_output_with_json() {
        let output = r#"
I completed the task.

```json
{
  "status": "complete",
  "summary": "Added new feature",
  "files_modified": ["src/main.rs", "src/lib.rs"],
  "confidence": 0.95
}
```

All done!
"#;

        let (result, files) = SwarmExecutor::parse_agent_output(output, 0);
        assert!(result.success);
        assert_eq!(result.summary, Some("Added new feature".to_string()));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_parse_agent_output_with_satisfaction() {
        // Test that an agent with high confidence JSON block marks success
        let output = r#"Completed the task.

```json
{
  "status": "complete",
  "summary": "Task completed successfully",
  "files_modified": [],
  "confidence": 0.95
}
```"#;

        let (result, _) = SwarmExecutor::parse_agent_output(output, 0);
        assert!(result.success);
    }

    #[test]
    fn test_detect_conflicts() {
        let mut files_map = HashMap::new();
        files_map.insert(PathBuf::from("src/main.rs"), vec![0, 1]);
        files_map.insert(PathBuf::from("src/lib.rs"), vec![1]);

        let conflicts = SwarmExecutor::detect_conflicts(&files_map);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].file, PathBuf::from("src/main.rs"));
    }
}
