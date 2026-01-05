//! Swarm Coordinator
//!
//! Coordinates the overall swarm execution, including decomposition,
//! execution, conflict resolution, and verification.
//!
//! ## Iteration Loop
//!
//! The swarm coordinator runs in a loop until the task is genuinely complete:
//! 1. Decompose task and run parallel agents
//! 2. Check for completion signals (.gogo-gadget-satisfied, .gogo-gadget-blocked)
//! 3. If not satisfied, gather feedback and run another iteration
//! 4. Continue until satisfied or blocked (no arbitrary iteration limit)
//!
//! ## Self-Extending Capabilities
//!
//! The swarm coordinator supports automatic capability extension:
//! - When agents fail due to missing capabilities, the coordinator can detect gaps
//! - It synthesizes new capabilities (Skills, MCPs) using Claude
//! - Verifies and registers the new capabilities
//! - Retries the failed agent work with the new capabilities

use super::{
    SwarmConfig, SwarmConfigBuilder, SwarmConflict, SwarmExecutor, SwarmResult, TaskDecomposer,
};
use crate::brain::TaskAnalyzer;
use crate::extend::{
    CapabilityGap, CapabilityRegistry, GapDetector, HotLoader, SynthesisEngine,
    CapabilityVerifier,
};
use crate::rlm::{RlmConfig, RlmExecutor, RlmResult};
use crate::task_loop::ExtensionConfig;
use crate::{Config, ProgressCallback, ShutdownSignal, TaskResult};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

const SATISFIED_FILE: &str = ".gogo-gadget-satisfied";
const BLOCKED_FILE: &str = ".gogo-gadget-blocked";
const CONTINUE_FILE: &str = ".gogo-gadget-continue";

/// Maximum number of iteration feedbacks to keep (context pruning)
const MAX_FEEDBACK_HISTORY: usize = 3;

/// Common English stopwords to exclude from goal anchors
const STOPWORDS: &[&str] = &[
    // Articles and determiners
    "the", "a", "an", "this", "that", "these", "those",
    // Pronouns
    "i", "you", "he", "she", "it", "we", "they", "who", "what", "which",
    "my", "your", "his", "her", "its", "our", "their",
    // Prepositions
    "in", "on", "at", "to", "for", "of", "with", "by", "from", "into",
    "about", "after", "before", "between", "under", "over", "through",
    // Conjunctions
    "and", "or", "but", "so", "yet", "nor", "both", "either", "neither",
    // Common verbs (auxiliary and modal)
    "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "having",
    "do", "does", "did", "doing",
    "will", "would", "shall", "should", "may", "might", "must", "can", "could",
    // Other common words
    "not", "no", "yes", "all", "any", "some", "each", "every",
    "there", "here", "when", "where", "why", "how",
    "if", "then", "than", "because", "while", "although",
    "just", "only", "also", "very", "more", "most", "less", "least",
    "such", "same", "other", "another", "many", "much", "few", "little",
    // Task-related common words that don't indicate specific goals
    "task", "work", "working", "complete", "completed", "done", "make", "made",
    "use", "using", "used", "need", "needs", "needed", "want", "wants", "wanted",
    "get", "gets", "getting", "got", "set", "sets", "setting",
    "please", "ensure", "check", "create", "update", "implement", "add", "remove",
    "should", "could", "would", "must", "following", "based", "continue",
];

/// Manages iteration feedback with automatic pruning and goal-anchor drift detection
#[derive(Debug, Default)]
struct FeedbackHistory {
    /// Circular buffer of recent feedbacks
    entries: Vec<(u32, String)>, // (iteration, feedback)
    /// Goal anchors extracted from original task (significant words)
    goal_anchors: HashSet<String>,
}

impl FeedbackHistory {
    fn new(original_task: &str) -> Self {
        Self {
            entries: Vec::with_capacity(MAX_FEEDBACK_HISTORY),
            goal_anchors: Self::extract_goal_anchors(original_task),
        }
    }

    /// Extract goal anchors from task text
    /// Returns significant words (>4 chars, excluding stopwords) that indicate task goals
    fn extract_goal_anchors(task: &str) -> HashSet<String> {
        let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();

        task.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| {
                word.len() > 4 && !stopwords.contains(word)
            })
            .map(|s| s.to_string())
            .collect()
    }

    /// Add feedback, pruning old entries beyond MAX_FEEDBACK_HISTORY
    fn add(&mut self, iteration: u32, feedback: String) {
        self.entries.push((iteration, feedback));
        // Keep only last MAX_FEEDBACK_HISTORY entries
        if self.entries.len() > MAX_FEEDBACK_HISTORY {
            self.entries.remove(0);
        }
    }

    /// Check if agents are off-track based on their work summaries
    /// Returns true (and resets) only if agent work has ZERO overlap with goal anchors
    fn is_off_track(&mut self, agent_summaries: &[String]) -> bool {
        if self.goal_anchors.is_empty() || agent_summaries.is_empty() {
            return false;
        }

        // Extract words from all agent summaries
        let summary_words: HashSet<String> = agent_summaries
            .iter()
            .flat_map(|summary| {
                summary
                    .to_lowercase()
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|word| word.len() > 4)
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .collect();

        // Check for ANY overlap with goal anchors
        let overlap_count = self.goal_anchors.intersection(&summary_words).count();

        if overlap_count == 0 {
            tracing::warn!(
                "Task drift detected: agent work has zero overlap with goal anchors. \
                 Goal anchors: {:?}, Agent words: {:?}",
                self.goal_anchors.iter().take(5).collect::<Vec<_>>(),
                summary_words.iter().take(5).collect::<Vec<_>>()
            );
            self.entries.clear();
            true
        } else {
            tracing::debug!(
                "Goal alignment check passed: {} anchor words found in agent work",
                overlap_count
            );
            false
        }
    }

    /// Build accumulated feedback string from recent entries
    fn build_feedback(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        self.entries
            .iter()
            .map(|(iter, fb)| format!("**Iteration {} feedback:**\n{}", iter, fb))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// Tracks assignment history to avoid repeating subtasks across iterations
#[derive(Debug, Default)]
struct AssignmentHistory {
    last_fingerprints: Vec<String>,
    seen_fingerprints: HashSet<String>,
    last_focuses: Vec<String>,
    files_modified: HashSet<PathBuf>,
    last_agent_summaries: Vec<String>,
}

impl AssignmentHistory {
    fn fingerprint(subtask: &super::Subtask) -> String {
        let mut files = subtask
            .target_files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        files.sort();
        if files.is_empty() {
            subtask.focus.trim().to_string()
        } else {
            format!("{}|{}", subtask.focus.trim(), files.join(","))
        }
    }

    fn fingerprints_for(subtasks: &[super::Subtask]) -> Vec<String> {
        let mut fingerprints = subtasks
            .iter()
            .map(Self::fingerprint)
            .collect::<Vec<_>>();
        fingerprints.sort();
        fingerprints
    }

    fn is_repeat(&self, subtasks: &[super::Subtask]) -> bool {
        let fingerprints = Self::fingerprints_for(subtasks);
        if fingerprints.is_empty() {
            return false;
        }

        if fingerprints == self.last_fingerprints {
            return true;
        }

        fingerprints
            .iter()
            .all(|fingerprint| self.seen_fingerprints.contains(fingerprint))
    }

    fn update(&mut self, subtasks: &[super::Subtask], result: &SwarmResult) {
        let fingerprints = Self::fingerprints_for(subtasks);
        self.last_fingerprints = fingerprints.clone();
        for fingerprint in fingerprints {
            self.seen_fingerprints.insert(fingerprint);
        }

        let mut focus_seen = HashSet::new();
        self.last_focuses = subtasks
            .iter()
            .filter_map(|subtask| {
                if focus_seen.insert(subtask.focus.clone()) {
                    Some(subtask.focus.clone())
                } else {
                    None
                }
            })
            .collect();

        for agent_result in &result.agent_results {
            for file in &agent_result.files_modified {
                self.files_modified.insert(file.clone());
            }
        }

        self.last_agent_summaries = result
            .agent_results
            .iter()
            .filter_map(|r| r.result.summary.clone())
            .filter(|summary| !summary.trim().is_empty())
            .collect();
    }

    fn format_context(&self) -> Option<String> {
        let mut lines = Vec::new();

        if !self.last_focuses.is_empty() {
            let focus_list = format_limited_list(&self.last_focuses, 6);
            lines.push(format!(
                "Previously assigned focus areas: {}.",
                focus_list
            ));
        }

        if !self.files_modified.is_empty() {
            let mut files = self
                .files_modified
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            files.sort();
            let file_list = format_limited_list(&files, 8);
            lines.push(format!("Files already touched: {}.", file_list));
        }

        if !self.last_agent_summaries.is_empty() {
            lines.push("Recent summaries:".to_string());
            for summary in self.last_agent_summaries.iter().take(3) {
                lines.push(format!("- {}", summary.trim()));
            }
        }

        if lines.is_empty() {
            return None;
        }

        lines.push("Avoid repeating the exact same assignments.".to_string());
        Some(lines.join("\n"))
    }
}

#[derive(Debug)]
struct RlmSuggestion {
    focus: String,
    files: Vec<PathBuf>,
    file_labels: Vec<String>,
}

/// Completion signal types
enum CompletionSignal {
    /// Task is complete with optional summary
    Satisfied(String),
    /// Task is blocked with reason
    Blocked(String),
    /// Continue with feedback
    Continue(String),
}

/// Maximum extension attempts per swarm execution
const MAX_SWARM_EXTENSION_ATTEMPTS: u32 = 3;

/// Coordinates swarm execution from start to finish
pub struct SwarmCoordinator {
    /// Swarm configuration
    config: SwarmConfig,
    /// Task analyzer
    analyzer: TaskAnalyzer,
    /// Shutdown signal for graceful termination
    shutdown_signal: Option<ShutdownSignal>,
    /// Progress callback for reporting
    progress_callback: Option<Arc<dyn ProgressCallback>>,
    /// Self-extending capability configuration
    extension_config: Option<ExtensionConfig>,
    /// Capability registry for managing synthesized capabilities
    capability_registry: Option<CapabilityRegistry>,
    /// Gap detector for identifying missing capabilities
    gap_detector: Option<GapDetector>,
    /// Synthesis engine for creating new capabilities
    synthesis_engine: Option<SynthesisEngine>,
    /// Extension attempts counter
    extension_attempts: u32,
    /// Shared context for overseer observation (Chief of Product pattern)
    shared_context: Option<crate::extend::SharedContext>,
}

impl SwarmCoordinator {
    /// Create a new swarm coordinator
    pub fn new(config: SwarmConfig) -> Self {
        Self {
            config,
            analyzer: TaskAnalyzer::new(),
            shutdown_signal: None,
            progress_callback: None,
            extension_config: None,
            capability_registry: None,
            gap_detector: None,
            synthesis_engine: None,
            extension_attempts: 0,
            shared_context: None,
        }
    }

    /// Create from a basic Config with agent count
    pub fn from_config(config: &Config, agent_count: u32) -> Self {
        let swarm_config = SwarmConfigBuilder::new()
            .agent_count(agent_count)
            .working_dir(config.working_dir.clone())
            .model(config.model.clone())
            .anti_laziness(config.anti_laziness.clone())
            .build();

        Self::new(swarm_config)
    }

    /// Enable self-extending capabilities with custom configuration
    pub fn with_extension_config(mut self, config: ExtensionConfig) -> Self {
        if config.enabled {
            let registry = match CapabilityRegistry::load_from(&config.registry_path) {
                Ok(registry) => registry,
                Err(err) => {
                    warn!(
                        "Failed to load capability registry from {:?}: {}",
                        config.registry_path, err
                    );
                    CapabilityRegistry::new(&config.registry_path)
                }
            };
            self.capability_registry = Some(registry);
            self.gap_detector = Some(GapDetector::new());
            self.synthesis_engine = Some(SynthesisEngine::new(&config.skills_dir));
        }
        self.extension_config = Some(config);
        self
    }

    /// Enable self-extending capabilities with default configuration
    pub fn with_extension_enabled(self) -> Self {
        let mut config = ExtensionConfig::default();
        config.enabled = true;
        self.with_extension_config(config)
    }

    /// Set shared context for Creative Overseer observation
    ///
    /// The shared context allows the Creative Overseer (Chief of Product) to
    /// observe what the swarm is working on and proactively create capabilities.
    pub fn with_shared_context(mut self, context: crate::extend::SharedContext) -> Self {
        self.shared_context = Some(context);
        self
    }

    /// Get the shared context (for spawning overseer)
    pub fn shared_context(&self) -> Option<&crate::extend::SharedContext> {
        self.shared_context.as_ref()
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

    /// Execute a task using swarm mode with iteration until satisfied
    pub async fn execute(&self, task: &str) -> Result<SwarmResult> {
        let start_time = Instant::now();
        let mut iteration = 1u32;
        let mut feedback_history = FeedbackHistory::new(task);
        let mut assignment_history = AssignmentHistory::default();
        let mut rlm_attempts = 0u32;
        let mut final_result: Option<SwarmResult> = None;

        info!(
            "Swarm coordinator starting with {} agents (iterating until satisfied)",
            self.config.agent_count
        );

        // Update shared context for Creative Overseer observation
        if let Some(ref ctx) = self.shared_context {
            ctx.start_task(task);
            ctx.set_phase(crate::extend::TaskPhase::Analyzing);
        }

        // Clean up any previous signal files
        self.cleanup_signals();

        // Iteration loop - runs until satisfied or blocked
        loop {
            // Check for shutdown
            if self.is_shutdown_requested() {
                warn!("Shutdown requested, stopping swarm iteration");
                break;
            }

            println!();
            println!("════════════════════════════════════════════════════════════════");
            println!("  🔄 SWARM ITERATION {} (running until satisfied)", iteration);
            println!("════════════════════════════════════════════════════════════════");

            // Build the evolved task with feedback from previous iterations (max last 3)
            let accumulated_feedback = feedback_history.build_feedback();
            let evolved_task = if accumulated_feedback.is_empty() {
                task.to_string()
            } else {
                format!(
                    "{}\n\n---\n**Feedback from last {} iterations:**\n{}\n\n\
                     Continue improving based on this feedback. \
                     Report your progress with high confidence (0.9+) when your part is genuinely complete.",
                    task, feedback_history.entries.len(), accumulated_feedback
                )
            };

            // Phase 1: Analyze the task
            let analysis = self.analyzer.analyze(&evolved_task)?;
            info!("Task analysis complete: complexity={}", analysis.complexity_score);

            // Phase 2: Decompose into subtasks
            let decomposer = TaskDecomposer::new(&self.config.working_dir);
            let mut subtasks = decomposer.decompose(&evolved_task, &analysis, &self.config);
            info!("Task decomposed into {} subtasks", subtasks.len());

            let repeat_detected = assignment_history.is_repeat(&subtasks);
            if repeat_detected && rlm_attempts < 2 {
                rlm_attempts += 1;
                info!(
                    "Repeat assignments detected, attempting RLM-guided decomposition (attempt {})",
                    rlm_attempts
                );
                let rlm_subtasks = self.generate_rlm_guided_subtasks(&evolved_task, &assignment_history);
                if !rlm_subtasks.is_empty() {
                    subtasks = rlm_subtasks;
                    info!("RLM-guided decomposition produced {} subtasks", subtasks.len());
                } else {
                    warn!("RLM-guided decomposition did not produce usable subtasks");
                }
            }

            // Update shared context with subtasks for overseer observation
            if let Some(ref ctx) = self.shared_context {
                ctx.set_phase(crate::extend::TaskPhase::Decomposing);
                ctx.set_subtasks(subtasks.clone());
                ctx.set_iteration(iteration);
            }

            // Print subtask info
            println!();
            println!("📋 Task Decomposition:");
            for subtask in &subtasks {
                println!("   Agent {}: {}", subtask.id + 1, subtask.focus);
            }
            println!();

            // Inject assignment history warning and capability context (in order)
            if let Some(history_block) = assignment_history.format_context() {
                inject_context_block(&mut subtasks, &history_block);
            }

            if let Some(capability_block) = self.refresh_capability_context() {
                inject_context_block(&mut subtasks, &capability_block);
            }

            // Phase 3: Execute subtasks in parallel
            // Update shared context for Creative Overseer observation
            if let Some(ref ctx) = self.shared_context {
                ctx.set_phase(crate::extend::TaskPhase::Executing);
            }

            let executor = if let Some(ref signal) = self.shutdown_signal {
                SwarmExecutor::new(self.config.clone()).with_shutdown_signal(signal.clone())
            } else {
                SwarmExecutor::new(self.config.clone())
            };

            let subtasks_for_history = subtasks.clone();
            let mut result = executor.execute(subtasks).await?;

            // Update context: Aggregating phase (collecting and resolving results)
            if let Some(ref ctx) = self.shared_context {
                ctx.set_phase(crate::extend::TaskPhase::Aggregating);
            }

            // Check for goal drift based on agent work summaries
            let agent_summaries: Vec<String> = result
                .agent_results
                .iter()
                .filter_map(|r| r.result.summary.clone())
                .collect();
            if feedback_history.is_off_track(&agent_summaries) {
                info!("Feedback history reset due to goal drift");
            }

            // Phase 4: Resolve conflicts
            if !result.conflicts.is_empty() {
                info!("Resolving {} conflicts", result.conflicts.len());
                self.resolve_conflicts(&mut result).await?;
            }

            // Phase 5: Check for completion signals
            // CONTINUOUS MODE: Ignore satisfaction signals, only stop on blocked
            if let Some(completion) = self.check_completion_signals()? {
                match completion {
                    CompletionSignal::Satisfied(summary) => {
                        // CONTINUOUS MODE: Don't stop on satisfied - delete signal and continue
                        println!();
                        println!("════════════════════════════════════════════════════════════════");
                        println!("  ✅ Iteration {} complete - agent claimed satisfied", iteration);
                        println!("  🔄 CONTINUOUS MODE: Finding next improvements...");
                        println!("════════════════════════════════════════════════════════════════");
                        println!();
                        if !summary.is_empty() {
                            println!("Summary: {}", summary);
                        }
                        // Delete the satisfaction signal so we don't see it again
                        let _ = fs::remove_file(self.config.working_dir.join(SATISFIED_FILE));

                        // Find next improvements instead of stopping
                        let next_tasks = self.find_next_improvements(task, &result).await?;
                        if !next_tasks.is_empty() {
                            feedback_history.add(
                                iteration,
                                format!(
                                    "Previous work done. Now focus on these improvements:\n{}",
                                    next_tasks
                                ),
                            );
                        } else {
                            feedback_history.add(
                                iteration,
                                "Previous work verified. Look deeper: \
                                 What edge cases are missing? What tests should be added? \
                                 What refactoring would improve code quality?".to_string(),
                            );
                        }
                    }
                    CompletionSignal::Blocked(reason) => {
                        // Blocked is the ONLY way to stop - something is genuinely stuck
                        println!();
                        println!("════════════════════════════════════════════════════════════════");
                        println!("  ⛔ SWARM BLOCKED after {} iterations", iteration);
                        println!("════════════════════════════════════════════════════════════════");
                        println!();
                        println!("Reason: {}", reason);

                        // Update context for Creative Overseer observation
                        if let Some(ref ctx) = self.shared_context {
                            ctx.set_phase(crate::extend::TaskPhase::Failed);
                        }

                        result.success = false;
                        result.total_duration_ms = start_time.elapsed().as_millis() as u64;
                        return Ok(result);
                    }
                    CompletionSignal::Continue(feedback) => {
                        println!("  📝 Continue signal received, iterating...");
                        feedback_history.add(iteration, feedback);
                    }
                }
            } else {
                // No signal - run verification to check actual build status
                // Update context for Creative Overseer observation
                if let Some(ref ctx) = self.shared_context {
                    ctx.set_phase(crate::extend::TaskPhase::Verifying);
                }

                let verification_passed = self.verify_result(&result, task).await?;
                result.verification_passed = verification_passed;

                if verification_passed {
                    println!();
                    println!("════════════════════════════════════════════════════════════════");
                    println!("  ✅ SWARM VERIFIED - iteration {} complete", iteration);
                    println!("  🔄 Finding next improvements (continuous mode)...");
                    println!("════════════════════════════════════════════════════════════════");
                    println!();

                    // CONTINUOUS MODE: Don't stop on success - find more improvements
                    let next_tasks = self.find_next_improvements(task, &result).await?;
                    if !next_tasks.is_empty() {
                        feedback_history.add(
                            iteration,
                            format!(
                                "Previous work verified. Now focus on these improvements:\n{}",
                                next_tasks
                            ),
                        );
                    } else {
                        // No improvements found - still continue, ask for deeper analysis
                        feedback_history.add(
                            iteration,
                            "Previous work verified. Look deeper: \
                             What edge cases are missing? What tests should be added? \
                             What refactoring would improve code quality? \
                             What documentation is needed?".to_string(),
                        );
                    }
                } else {
                    // Not verified - add feedback and continue
                    println!("  ⚠️ Verification failed, continuing to iterate...");
                    feedback_history.add(
                        iteration,
                        format!(
                            "Verification failed. The work is not yet complete. \
                             Review what was done and continue improving.\n\
                             Summary so far: {}",
                            result.summary.as_deref().unwrap_or("No summary")
                        ),
                    );
                }
            }

            assignment_history.update(&subtasks_for_history, &result);
            final_result = Some(result);
            iteration += 1;

            // Brief pause between iterations
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        // Return last result if we broke out of the loop
        let mut result = final_result.unwrap_or_else(|| SwarmResult {
            success: false,
            agent_results: vec![],
            summary: Some("Swarm interrupted".to_string()),
            conflicts: vec![],
            total_duration_ms: start_time.elapsed().as_millis() as u64,
            successful_agents: 0,
            failed_agents: 0,
            verification_passed: false,
            compressed_output: None,
        });
        result.total_duration_ms = start_time.elapsed().as_millis() as u64;
        Ok(result)
    }

    fn refresh_capability_context(&self) -> Option<String> {
        let config = self.extension_config.as_ref()?;
        if !config.enabled {
            return None;
        }

        let registry = match CapabilityRegistry::load_from(&config.registry_path) {
            Ok(registry) => registry,
            Err(err) => {
                warn!(
                    "Failed to load capability registry from {:?}: {}",
                    config.registry_path, err
                );
                return None;
            }
        };

        let mut entries = Vec::new();
        for skill in registry.skills {
            entries.push(("Skill", skill.name));
        }
        for mcp in registry.mcps {
            entries.push(("MCP", mcp.name));
        }
        for agent in registry.agents {
            entries.push(("Agent", agent.name));
        }

        if entries.is_empty() {
            return None;
        }

        entries.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(&b.1)));
        let list = entries
            .into_iter()
            .take(10)
            .map(|(kind, name)| format!("- {}: {}", kind, name))
            .collect::<Vec<_>>()
            .join("\n");

        Some(format!(
            "Available capabilities (use when helpful):\n{}",
            list
        ))
    }

    fn generate_rlm_guided_subtasks(
        &self,
        task: &str,
        history: &AssignmentHistory,
    ) -> Vec<super::Subtask> {
        let focus_list = if history.last_focuses.is_empty() {
            "none".to_string()
        } else {
            format_limited_list(&history.last_focuses, 6)
        };

        let files_list = if history.files_modified.is_empty() {
            "none".to_string()
        } else {
            let mut files = history
                .files_modified
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            files.sort();
            format_limited_list(&files, 8)
        };

        let query = format!(
            "You are helping decompose this task into parallel worker assignments.\n\
             Task: \"{}\"\n\
             Avoid repeating these focus areas: {}.\n\
             Files already touched: {}.\n\
             Suggest 3-5 new focus areas and relevant files.\n\
             Return each suggestion on its own line in the format:\n\
             <Focus> | Files: <comma-separated paths>",
            task, focus_list, files_list
        );

        let config = RlmConfig::default();
        let mut executor = RlmExecutor::new(config, self.config.working_dir.clone());
        let result = match executor.execute(&query, &self.config.working_dir) {
            Ok(result) => result,
            Err(err) => {
                warn!("RLM-guided decomposition failed: {}", err);
                return Vec::new();
            }
        };

        let parse_limit = std::cmp::min(self.config.agent_count as usize, 5);
        let mut suggestions =
            parse_rlm_suggestions(&result, &self.config.working_dir, parse_limit);
        if suggestions.is_empty() {
            return Vec::new();
        }

        let agent_count = self.config.agent_count as usize;
        let mut subtasks = Vec::new();
        for (i, suggestion) in suggestions.drain(..).take(agent_count).enumerate() {
            let files_line = if suggestion.file_labels.is_empty() {
                "No specific files suggested.".to_string()
            } else {
                format!("Relevant files: {}", suggestion.file_labels.join(", "))
            };

            let description = format!(
                "{}\n\n**Agent {} Focus**: {}\n{}",
                task,
                i + 1,
                suggestion.focus,
                files_line
            );

            subtasks.push(super::Subtask {
                id: i as u32,
                description,
                focus: suggestion.focus,
                target_files: suggestion.files,
                shared_context: None,
                priority: i as u32,
                use_subagent_mode: false,
                subagent_depth: 0,
            });
        }

        while subtasks.len() < agent_count {
            let id = subtasks.len() as u32;
            subtasks.push(self.create_review_subtask(task, id));
        }

        subtasks
    }

    fn create_review_subtask(&self, task: &str, id: u32) -> super::Subtask {
        super::Subtask {
            id,
            description: format!(
                "{}\n\n**Agent {} Focus**: Review and Verification\n\
                 Review the work done by other agents. Run tests, check for issues, \
                 verify the implementation is complete and correct.",
                task,
                id + 1
            ),
            focus: "Review and Verification".to_string(),
            target_files: vec![],
            shared_context: None,
            priority: id + 100,
            use_subagent_mode: false,
            subagent_depth: 0,
        }
    }

    /// Check if shutdown was requested
    fn is_shutdown_requested(&self) -> bool {
        self.shutdown_signal
            .as_ref()
            .map(|s| s.is_shutdown())
            .unwrap_or(false)
    }

    /// Clean up signal files from previous runs
    fn cleanup_signals(&self) {
        let working_dir = &self.config.working_dir;
        let _ = fs::remove_file(working_dir.join(SATISFIED_FILE));
        let _ = fs::remove_file(working_dir.join(BLOCKED_FILE));
        let _ = fs::remove_file(working_dir.join(CONTINUE_FILE));
    }

    /// Check for completion signals
    fn check_completion_signals(&self) -> Result<Option<CompletionSignal>> {
        let working_dir = &self.config.working_dir;

        // Check for satisfied signal
        let satisfied_path = working_dir.join(SATISFIED_FILE);
        if satisfied_path.exists() {
            let summary = fs::read_to_string(&satisfied_path).unwrap_or_default();
            let _ = fs::remove_file(&satisfied_path);
            return Ok(Some(CompletionSignal::Satisfied(summary)));
        }

        // Check for blocked signal
        let blocked_path = working_dir.join(BLOCKED_FILE);
        if blocked_path.exists() {
            let reason = fs::read_to_string(&blocked_path).unwrap_or_default();
            let _ = fs::remove_file(&blocked_path);
            return Ok(Some(CompletionSignal::Blocked(reason)));
        }

        // Check for continue signal
        let continue_path = working_dir.join(CONTINUE_FILE);
        if continue_path.exists() {
            let feedback = fs::read_to_string(&continue_path).unwrap_or_default();
            let _ = fs::remove_file(&continue_path);
            return Ok(Some(CompletionSignal::Continue(feedback)));
        }

        Ok(None)
    }

    /// Resolve conflicts between agent outputs
    async fn resolve_conflicts(&self, result: &mut SwarmResult) -> Result<()> {
        for conflict in &mut result.conflicts {
            info!(
                "Attempting to resolve conflict in {}",
                conflict.file.display()
            );

            // Strategy: Run a conflict resolution pass with Claude
            let resolution = self.run_conflict_resolution(conflict).await;

            match resolution {
                Ok(resolved) => {
                    conflict.resolved = resolved;
                    if resolved {
                        info!("Conflict resolved: {}", conflict.file.display());
                    } else {
                        warn!("Could not resolve conflict: {}", conflict.file.display());
                    }
                }
                Err(e) => {
                    error!(
                        "Error resolving conflict in {}: {}",
                        conflict.file.display(),
                        e
                    );
                    conflict.resolved = false;
                }
            }
        }

        Ok(())
    }

    /// Run conflict resolution for a single file
    async fn run_conflict_resolution(&self, conflict: &SwarmConflict) -> Result<bool> {
        let prompt = format!(
            "Multiple agents modified the file '{}'. \
             Review the changes and merge them intelligently, \
             keeping the best parts from each modification. \
             \n\nAgents involved: {:?}\n\
             \nInstructions:\n\
             1. Read the current state of the file\n\
             2. Identify any conflicting changes\n\
             3. Merge the changes intelligently\n\
             4. Ensure the code compiles and makes sense\n\
             5. Do NOT create any .gogo-gadget-* signal files\n\
             6. Report success with: {{\"resolved\": true, \"summary\": \"...\"}}\n",
            conflict.file.display(),
            conflict.agent_ids
        );

        let mut cmd = Command::new("claude");

        if let Some(ref model) = self.config.model {
            cmd.arg("--model").arg(model);
        }

        cmd.arg("--dangerously-skip-permissions");
        cmd.current_dir(&self.config.working_dir);
        cmd.arg("--max-turns").arg("5");
        cmd.arg(&prompt);

        let output = cmd.output().context("Failed to run conflict resolution")?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check if resolution succeeded via JSON response or exit status
        let resolved = stdout.contains("\"resolved\": true")
            || stdout.contains("\"resolved\":true")
            || output.status.success();

        Ok(resolved)
    }

    /// Verify the combined result of all agents
    async fn verify_result(&self, result: &SwarmResult, original_task: &str) -> Result<bool> {
        info!("Running final verification");

        // Check basic success criteria
        if result.failed_agents > 0 {
            warn!(
                "Verification failed: {} agents failed",
                result.failed_agents
            );
            return Ok(false);
        }

        if result.conflicts.iter().any(|c| !c.resolved) {
            warn!("Verification failed: unresolved conflicts");
            return Ok(false);
        }

        // Run a verification pass with Claude
        // NOTE: Verifier does NOT create signal files - orchestrator handles that
        let verification_prompt = format!(
            "Verify that the following task was completed correctly:\n\n{}\n\n\
             Multiple agents worked on this task. Review their work:\n\n{}\n\n\
             Instructions:\n\
             1. Check that all parts of the task are complete\n\
             2. Run any tests if applicable\n\
             3. Verify the code compiles (if code was written)\n\
             4. Check for any obvious issues or bugs\n\
             5. Do NOT create any .gogo-gadget-* signal files\n\n\
             Respond with a JSON block:\n\
             ```json\n\
             {{\n\
               \"passed\": true/false,\n\
               \"summary\": \"verification summary\",\n\
               \"issues\": [\"list of issues if any\"],\n\
               \"next_steps\": [\"what still needs to be done\"]\n\
             }}\n\
             ```",
            original_task,
            result.summary.as_deref().unwrap_or("No summary available")
        );

        let mut cmd = Command::new("claude");

        if let Some(ref model) = self.config.model {
            cmd.arg("--model").arg(model);
        }

        cmd.arg("--dangerously-skip-permissions");
        cmd.current_dir(&self.config.working_dir);
        cmd.arg("--max-turns").arg("5");
        cmd.arg(&verification_prompt);

        let output = cmd.output().context("Failed to run verification")?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse verification result from JSON response
        // Look for "passed": true in the output
        let passed = if stdout.contains("\"passed\": true") || stdout.contains("\"passed\":true") {
            true
        } else if stdout.contains("\"passed\": false") || stdout.contains("\"passed\":false") {
            false
        } else {
            // Fallback: check if output looks positive and command succeeded
            output.status.success() && !stdout.to_lowercase().contains("fail")
        };

        if passed {
            info!("Verification passed - continuing to find improvements (continuous mode)");
            // NOTE: In continuous mode, we do NOT create satisfaction signal
            // The swarm runs forever until explicitly stopped
        } else {
            warn!("Verification failed - will iterate to fix issues");
        }

        Ok(passed)
    }

    /// Find next improvements to work on (continuous improvement mode)
    /// Called after verification passes to find more work
    async fn find_next_improvements(&self, original_task: &str, result: &SwarmResult) -> Result<String> {
        info!("Finding next improvements for continuous iteration");

        // First, run the actual build/test command to find concrete issues
        let build_output = self.run_build_check().await?;

        if !build_output.is_empty() {
            // There are actual build/test errors - prioritize these
            return Ok(format!(
                "Build/test issues found that need fixing:\n{}",
                build_output
            ));
        }

        // No build errors - ask Claude for improvement suggestions
        let improvement_prompt = format!(
            "The following task has been completed and verified:\n\n{}\n\n\
             Summary of work done:\n{}\n\n\
             The build passes and tests pass. Now identify the NEXT improvements to make:\n\n\
             1. What edge cases are not handled?\n\
             2. What additional tests should be written?\n\
             3. What code could be refactored for clarity?\n\
             4. What documentation is missing?\n\
             5. What performance optimizations could be made?\n\
             6. What security considerations are missing?\n\n\
             Respond with a prioritized list of specific, actionable improvements.\n\
             Be concrete - name specific files, functions, or features.\n\
             If truly nothing can be improved, respond with just: NOTHING_TO_IMPROVE",
            original_task,
            result.summary.as_deref().unwrap_or("No summary")
        );

        let mut cmd = Command::new("claude");

        if let Some(ref model) = self.config.model {
            cmd.arg("--model").arg(model);
        }

        cmd.arg("--dangerously-skip-permissions");
        cmd.current_dir(&self.config.working_dir);
        cmd.arg("--max-turns").arg("3");
        cmd.arg("--print");
        cmd.arg(&improvement_prompt);

        let output = cmd.output().context("Failed to get improvement suggestions")?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        // Even if Claude says nothing to improve, we return empty and the caller
        // will still push for deeper analysis
        if stdout.contains("NOTHING_TO_IMPROVE") {
            Ok(String::new())
        } else {
            Ok(stdout.trim().to_string())
        }
    }

    /// Run build/test check and return any errors found
    async fn run_build_check(&self) -> Result<String> {
        info!("Running build check");

        // Try to detect project type and run appropriate build command
        let working_dir = &self.config.working_dir;

        let (cmd_name, args): (&str, Vec<&str>) = if working_dir.join("package.json").exists() {
            // Node.js project
            ("npm", vec!["run", "build"])
        } else if working_dir.join("Cargo.toml").exists() {
            // Rust project
            ("cargo", vec!["build"])
        } else if working_dir.join("pyproject.toml").exists() || working_dir.join("setup.py").exists() {
            // Python project
            ("python", vec!["-m", "py_compile", "."])
        } else {
            // Unknown project type - skip build check
            return Ok(String::new());
        };

        let mut cmd = Command::new(cmd_name);
        cmd.args(&args);
        cmd.current_dir(working_dir);

        let output = cmd.output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    Ok(String::new()) // Build passed
                } else {
                    // Build failed - return errors
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Ok(format!("Build errors:\n{}\n{}", stdout, stderr))
                }
            }
            Err(e) => {
                warn!("Failed to run build command: {}", e);
                Ok(String::new()) // Couldn't run build, continue anyway
            }
        }
    }

    /// Check for capability gaps in failed agent results and attempt to extend
    ///
    /// This method analyzes failed agent results to detect capability gaps,
    /// synthesizes new capabilities as needed, and returns whether extension
    /// was successful (indicating the task should be retried).
    pub async fn try_extend_capabilities(&mut self, result: &SwarmResult) -> Result<bool> {
        // Check if extension is enabled
        let ext_config = match &self.extension_config {
            Some(config) if config.enabled => config.clone(),
            _ => return Ok(false),
        };

        // Check extension attempt limit
        if self.extension_attempts >= MAX_SWARM_EXTENSION_ATTEMPTS {
            warn!(
                "Maximum swarm extension attempts ({}) reached",
                MAX_SWARM_EXTENSION_ATTEMPTS
            );
            return Ok(false);
        }

        // Collect errors from failed agents
        let failed_errors: Vec<String> = result
            .agent_results
            .iter()
            .filter(|r| !r.result.success)
            .filter_map(|r| r.result.error.clone())
            .collect();

        if failed_errors.is_empty() {
            debug!("No failed agent errors to analyze for capability gaps");
            return Ok(false);
        }

        // Collect detected gaps first to avoid borrow conflicts
        let mut detected_gaps = Vec::new();
        {
            let gap_detector = match &self.gap_detector {
                Some(detector) => detector,
                None => return Ok(false),
            };

            for error in &failed_errors {
                if let Ok(Some(gap)) = gap_detector
                    .detect_from_error(error, &self.config.working_dir)
                    .await
                {
                    detected_gaps.push(gap);
                }
            }
        }

        // Now process gaps with mutable self
        for detected_gap in detected_gaps {
            info!(
                "Capability gap detected in swarm: {} (type: {:?})",
                detected_gap.name(),
                detected_gap.capability_type()
            );

            // Check if capability already exists
            if let Some(ref registry) = self.capability_registry {
                if registry.has_capability(detected_gap.name()) {
                    warn!(
                        "Capability '{}' already exists but agent still failed",
                        detected_gap.name()
                    );
                    continue;
                }
            }

            // Try to synthesize the capability
            if self.synthesize_capability(&detected_gap, &ext_config).await? {
                self.extension_attempts += 1;
                info!(
                    "Successfully synthesized capability for swarm: {}",
                    detected_gap.name()
                );
                return Ok(true); // Extension successful, retry the swarm
            }
        }

        Ok(false)
    }

    /// Synthesize and register a capability for a detected gap
    async fn synthesize_capability(
        &mut self,
        gap: &CapabilityGap,
        _ext_config: &ExtensionConfig,
    ) -> Result<bool> {
        let synthesis_engine = match &self.synthesis_engine {
            Some(engine) => engine,
            None => {
                warn!("Synthesis engine not available");
                return Ok(false);
            }
        };

        // Step 1: Synthesize the capability
        info!("Synthesizing capability for gap: {}", gap.name());
        let synthesized = match synthesis_engine.synthesize(gap).await {
            Ok(result) => result,
            Err(e) => {
                warn!("Failed to synthesize capability: {}", e);
                return Ok(false);
            }
        };

        // Step 2: Verify the capability
        info!("Verifying synthesized capability: {}", synthesized.capability.name);
        let verifier = CapabilityVerifier::new();
        let verification = verifier.verify(&synthesized.capability).await;

        if verification.success {
            info!("Capability verification passed: {}", synthesized.capability.name);
        } else {
            warn!("Capability verification failed: {:?}", verification.errors);
            return Ok(false);
        }

        // Step 3: Hot load the capability
        info!("Hot loading capability: {}", synthesized.capability.name);
        let loader = HotLoader::new();
        if let Err(e) = loader.load(&synthesized.capability) {
            warn!("Hot loading failed (non-fatal): {}", e);
        }

        // Step 4: Register the capability
        if let Some(ref mut registry) = self.capability_registry {
            if let Err(e) = registry.register(synthesized.capability.clone()) {
                warn!("Registration failed (non-fatal): {}", e);
            } else if let Err(e) = registry.save() {
                warn!("Failed to save capability registry: {}", e);
            }
        }

        Ok(true)
    }

    /// Execute with automatic capability extension on failures
    ///
    /// This method wraps the standard execute() method with capability gap
    /// detection and synthesis. When agents fail due to missing capabilities,
    /// it will attempt to synthesize new capabilities and retry.
    pub async fn execute_with_extension(&mut self, task: &str) -> Result<SwarmResult> {
        // Reset extension counter
        self.extension_attempts = 0;

        loop {
            let result = self.execute(task).await?;

            // If successful or all agents succeeded, return
            if result.success || result.failed_agents == 0 {
                return Ok(result);
            }

            // Try to extend capabilities based on failures
            let extended = self.try_extend_capabilities(&result).await?;

            if extended {
                info!("Capabilities extended, retrying swarm execution");
                // Continue loop to retry with new capabilities
                continue;
            }

            // No extension possible, return the result
            return Ok(result);
        }
    }

    /// Execute a task with the Creative Overseer running alongside
    ///
    /// This implements the "Chief of Product" pattern where the overseer
    /// proactively observes what the swarm is working on and creates
    /// capabilities that would help, without waiting for failures.
    pub async fn execute_with_overseer(
        mut self,
        task: &str,
        registry: std::sync::Arc<std::sync::Mutex<CapabilityRegistry>>,
    ) -> Result<SwarmResult> {
        use crate::extend::{CreativeOverseer, OverseerConfig, SharedContext};
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::time::{interval, Duration};

        // Create shared context for brain-overseer communication
        let context = SharedContext::new();

        // Attach context to coordinator
        self.shared_context = Some(context.clone());

        // Create the overseer
        let mut overseer_config = OverseerConfig::default();
        overseer_config.working_dir = self.config.working_dir.clone();
        let mut overseer = CreativeOverseer::new(overseer_config, registry);

        // Shutdown flag for overseer
        let overseer_running = std::sync::Arc::new(AtomicBool::new(true));
        let overseer_running_clone = overseer_running.clone();
        let context_clone = context.clone();

        // Spawn the overseer as a background task
        let overseer_handle = tokio::spawn(async move {
            info!("Creative Overseer started - observing swarm execution");
            let mut check_interval = interval(Duration::from_secs(10));

            while overseer_running_clone.load(Ordering::Relaxed) {
                check_interval.tick().await;

                // Check if there's a task to observe
                if let Some(snapshot) = context_clone.snapshot() {
                    if !snapshot.task.is_empty() {
                        debug!(
                            "Overseer observing: phase={:?}, iteration={}",
                            snapshot.phase, snapshot.iteration
                        );

                        // Try to observe and create capabilities
                        match overseer.observe_and_create(&context_clone).await {
                            Ok(result) => {
                                if result.synthesized_count > 0 {
                                    info!(
                                        "Overseer created {} capabilities for swarm",
                                        result.synthesized_count
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("Overseer observation failed: {}", e);
                            }
                        }
                    }
                }
            }

            info!("Creative Overseer shutting down");
        });

        // Run the main swarm execution
        let result = self.execute(task).await;

        // Stop the overseer
        overseer_running.store(false, Ordering::Relaxed);
        let _ = overseer_handle.await;

        result
    }
}

fn inject_context_block(subtasks: &mut [super::Subtask], context: &str) {
    for subtask in subtasks {
        subtask.description.push_str("\n\n---\n");
        subtask.description.push_str(context);
    }
}

fn format_limited_list(items: &[String], limit: usize) -> String {
    let mut list = items.iter().take(limit).cloned().collect::<Vec<_>>();
    if items.len() > limit {
        list.push("...".to_string());
    }
    list.join(", ")
}

fn parse_rlm_suggestions(
    result: &RlmResult,
    working_dir: &Path,
    limit: usize,
) -> Vec<RlmSuggestion> {
    let mut lines = Vec::new();
    if !result.key_insights.is_empty() {
        lines.extend(result.key_insights.iter().cloned());
    }
    if lines.is_empty() && !result.answer.trim().is_empty() {
        lines.extend(result.answer.lines().map(|line| line.to_string()));
    }

    let mut suggestions = Vec::new();
    for line in lines {
        if suggestions.len() >= limit {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cleaned = trimmed.trim_start_matches(|c: char| {
            c.is_ascii_digit() || c == '.' || c == '-' || c == '*' || c == ')' || c == ' '
        });
        if cleaned.is_empty() {
            continue;
        }

        let (focus, mut file_labels) = parse_focus_and_files(cleaned);
        if file_labels.is_empty() {
            file_labels = extract_file_tokens(cleaned);
        }

        if focus.is_empty() {
            continue;
        }

        let files = resolve_files(&file_labels, working_dir);

        suggestions.push(RlmSuggestion {
            focus,
            files,
            file_labels,
        });
    }

    suggestions
}

fn parse_focus_and_files(line: &str) -> (String, Vec<String>) {
    let mut focus = String::new();
    let mut files = Vec::new();

    let parts = line.split('|').collect::<Vec<_>>();
    if parts.len() > 1 {
        for part in parts {
            let part_trim = part.trim();
            let lower = part_trim.to_lowercase();
            if lower.contains("file") {
                files.extend(
                    part_trim
                        .splitn(2, ':')
                        .nth(1)
                        .unwrap_or("")
                        .split(',')
                        .map(|f| f.trim().trim_matches(|c: char| c == '"' || c == '\''))
                        .filter(|f| !f.is_empty())
                        .map(|f| f.to_string()),
                );
            } else if focus.is_empty() {
                focus = part_trim
                    .trim_start_matches("Focus:")
                    .trim_start_matches("focus:")
                    .trim()
                    .to_string();
            }
        }
    } else if let Some((left, right)) = split_on_files(line) {
        focus = left.trim().trim_end_matches(':').trim().to_string();
        files.extend(
            right
                .split(',')
                .map(|f| f.trim().trim_matches(|c: char| c == '"' || c == '\''))
                .filter(|f| !f.is_empty())
                .map(|f| f.to_string()),
        );
    } else {
        focus = line.trim().to_string();
    }

    (focus, files)
}

fn split_on_files(line: &str) -> Option<(&str, &str)> {
    let lower = line.to_lowercase();
    if let Some(index) = lower.find("files:") {
        let (left, right) = line.split_at(index);
        let right = &right["files:".len()..];
        return Some((left, right));
    }
    None
}

fn extract_file_tokens(line: &str) -> Vec<String> {
    let extensions = [
        ".rs", ".toml", ".md", ".json", ".yaml", ".yml", ".js", ".jsx", ".ts", ".tsx", ".py",
        ".go", ".java", ".rb", ".php", ".cpp", ".cc", ".c", ".h", ".hpp", ".sh", ".bash",
    ];
    let mut files = Vec::new();
    for token in line.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| {
            c == ',' || c == '.' || c == ';' || c == ':' || c == '(' || c == ')' || c == '['
                || c == ']' || c == '{' || c == '}' || c == '"' || c == '\''
        });
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();
        let is_path = trimmed.contains('/') || trimmed.contains('\\');
        let has_ext = extensions.iter().any(|ext| lower.ends_with(ext));
        if is_path || has_ext {
            files.push(trimmed.to_string());
        }
    }
    files
}

fn resolve_files(labels: &[String], working_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for label in labels {
        let candidate = PathBuf::from(label);
        let path = if candidate.is_absolute() {
            candidate
        } else {
            working_dir.join(candidate)
        };
        if path.exists() && seen.insert(path.clone()) {
            files.push(path);
        }
    }
    files
}

/// Convert SwarmResult to TaskResult for compatibility
impl From<SwarmResult> for TaskResult {
    fn from(swarm: SwarmResult) -> Self {
        let iterations = swarm.agent_results.len() as u32;

        let summary = if swarm.success {
            Some(format!(
                "Swarm execution completed with {}/{} agents successful. {}",
                swarm.successful_agents,
                swarm.agent_results.len(),
                swarm.summary.as_deref().unwrap_or("")
            ))
        } else {
            swarm.summary
        };

        let error = if !swarm.success {
            let errors: Vec<String> = swarm
                .agent_results
                .iter()
                .filter_map(|r| r.result.error.clone())
                .collect();

            if errors.is_empty() {
                if !swarm.verification_passed {
                    Some("Final verification failed".to_string())
                } else {
                    Some("Swarm execution failed".to_string())
                }
            } else {
                Some(errors.join("; "))
            }
        } else {
            None
        };

        TaskResult {
            success: swarm.success,
            iterations,
            summary,
            error,
            verification: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm::AgentResult;
    use tempfile::TempDir;

    #[test]
    fn test_swarm_coordinator_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = SwarmConfigBuilder::new()
            .agent_count(3)
            .working_dir(temp_dir.path().to_path_buf())
            .build();

        let coordinator = SwarmCoordinator::new(config);
        assert_eq!(coordinator.config.agent_count, 3);
    }

    #[test]
    fn test_swarm_result_to_task_result() {
        let swarm_result = SwarmResult {
            success: true,
            agent_results: vec![
                AgentResult {
                    agent_id: 0,
                    subtask: "Task 1".to_string(),
                    result: TaskResult {
                        success: true,
                        iterations: 1,
                        summary: Some("Done".to_string()),
                        error: None,
                        verification: None,
                    },
                    files_modified: vec![],
                    duration_ms: 1000,
                    compressed_result: None,
                },
            ],
            summary: Some("All done".to_string()),
            conflicts: vec![],
            total_duration_ms: 1000,
            successful_agents: 1,
            failed_agents: 0,
            verification_passed: true,
            compressed_output: None,
        };

        let task_result: TaskResult = swarm_result.into();
        assert!(task_result.success);
        assert!(task_result.summary.is_some());
    }

    #[test]
    fn test_extract_goal_anchors() {
        let task = "Implement the authentication system with JWT tokens and password hashing";
        let anchors = FeedbackHistory::extract_goal_anchors(task);

        // Should contain significant words
        assert!(anchors.contains("authentication"));
        assert!(anchors.contains("system"));
        assert!(anchors.contains("tokens"));
        assert!(anchors.contains("password"));
        assert!(anchors.contains("hashing"));

        // Should NOT contain stopwords or short words
        assert!(!anchors.contains("the"));
        assert!(!anchors.contains("with"));
        assert!(!anchors.contains("and"));
        assert!(!anchors.contains("implement")); // in stopwords
    }

    #[test]
    fn test_extract_goal_anchors_filters_stopwords() {
        let task = "Please create and update the code now";
        let anchors = FeedbackHistory::extract_goal_anchors(task);

        // All these words are either stopwords or too short (<=4 chars)
        // "please" (6) - stopword, "create" (6) - stopword, "update" (6) - stopword
        // "the" (3) - short, "code" (4) - short, "now" (3) - short, "and" (3) - short
        assert!(anchors.is_empty());
    }

    #[test]
    fn test_is_off_track_with_matching_work() {
        let task = "Build a REST API with authentication and database integration";
        let mut history = FeedbackHistory::new(task);

        // Agent work mentions database - should NOT be off track
        let summaries = vec!["Implemented database connection pooling".to_string()];
        assert!(!history.is_off_track(&summaries));
    }

    #[test]
    fn test_is_off_track_with_unrelated_work() {
        let task = "Build a REST API with authentication and database integration";
        let mut history = FeedbackHistory::new(task);

        // Agent work is completely unrelated
        let summaries = vec!["Fixed the CSS styling and button colors".to_string()];
        assert!(history.is_off_track(&summaries));
    }

    #[test]
    fn test_is_off_track_resets_feedback() {
        let task = "Build authentication system";
        let mut history = FeedbackHistory::new(task);

        // Add some feedback
        history.add(1, "First iteration feedback".to_string());
        history.add(2, "Second iteration feedback".to_string());
        assert_eq!(history.entries.len(), 2);

        // Drift detected - should clear feedback
        let summaries = vec!["Working on unrelated CSS styling".to_string()];
        assert!(history.is_off_track(&summaries));
        assert!(history.entries.is_empty());
    }

    #[test]
    fn test_is_off_track_with_empty_inputs() {
        let task = "Build authentication";
        let mut history = FeedbackHistory::new(task);

        // Empty summaries - should not be off track
        assert!(!history.is_off_track(&[]));

        // Empty anchors case
        let mut empty_history = FeedbackHistory::new("a b c"); // all short words
        assert!(!empty_history.is_off_track(&["anything".to_string()]));
    }

    #[test]
    fn test_feedback_history_pruning() {
        let mut history = FeedbackHistory::new("test task");

        history.add(1, "feedback 1".to_string());
        history.add(2, "feedback 2".to_string());
        history.add(3, "feedback 3".to_string());
        assert_eq!(history.entries.len(), 3);

        // Adding 4th should prune oldest
        history.add(4, "feedback 4".to_string());
        assert_eq!(history.entries.len(), 3);
        assert_eq!(history.entries[0].0, 2); // oldest is now iteration 2
    }
}
