//! Task Loop - Outcome-Focused Iterative Execution
//!
//! Implements the core execution loop that runs tasks repeatedly
//! until genuinely complete. No arbitrary iteration limits.
//!
//! Features:
//! - Checkpointing: save/restore state between iterations
//! - Timeout handling: per-iteration timeouts with configurable limits
//! - Exponential backoff: intelligent retry with backoff on failures
//! - Parallel execution: support for multi-file concurrent tasks
//! - Iteration history: memory of previous attempts for learning
//! - Smart prompt evolution: adaptive prompts based on feedback
//! - Structured JSON output: parse Claude's output as structured data
//! - Signal handling: graceful shutdown on SIGINT/SIGTERM
//! - Progress callbacks: real-time progress reporting
//! - LLM-based completion: task runs until LLM determines it's done
//! - Anti-laziness enforcement: optional mock data detection and evidence requirements
//! - Self-extending capabilities: automatic capability gap detection and synthesis

use crate::extend::{
    CapabilityGap, CapabilityRegistry, GapDetector, HotLoader, SynthesisEngine,
    CapabilityVerifier, SynthesizedCapability, VerificationResult,
};
use crate::verify::{MockDataResult, Severity, Verifier};
use crate::{
    BackoffConfig, Checkpoint, ClaudeOutputStatus, ClaudeStructuredOutput, Config,
    ExecutionMetrics, IterationAttempt, ParallelConfig, ParallelResult, ProgressCallback,
    ProgressEvent, ProgressEventType, ShutdownSignal, TaskResult, TaskState,
};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// Completion signal files
const SATISFIED_FILE: &str = ".gogo-gadget-satisfied";
const CONTINUE_FILE: &str = ".gogo-gadget-continue";
const BLOCKED_FILE: &str = ".gogo-gadget-blocked";
const CHECKPOINT_FILE: &str = ".gogo-gadget-checkpoint";

/// Default timeout for a single iteration (5 minutes)
const DEFAULT_ITERATION_TIMEOUT_SECS: u64 = 300;

/// Maximum consecutive failures before giving up (safety valve)
const MAX_CONSECUTIVE_FAILURES: u32 = 10;

/// Maximum self-extension attempts per task (prevent infinite loops)
const MAX_EXTENSION_ATTEMPTS: u32 = 3;

/// Configuration for self-extending capability system
#[derive(Debug, Clone)]
pub struct ExtensionConfig {
    /// Enable self-extending capabilities
    pub enabled: bool,
    /// Maximum extension attempts per task
    pub max_attempts: u32,
    /// Registry path for capabilities
    pub registry_path: PathBuf,
    /// Skills output directory
    pub skills_dir: PathBuf,
}

impl Default for ExtensionConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            enabled: false,
            max_attempts: MAX_EXTENSION_ATTEMPTS,
            registry_path: PathBuf::from(&home).join(".gogo-gadget/capabilities.json"),
            skills_dir: PathBuf::from(&home).join(".claude/skills"),
        }
    }
}

/// Result of a self-extension attempt
#[derive(Debug, Clone)]
pub struct ExtensionResult {
    /// Whether extension was successful
    pub success: bool,
    /// The capability that was synthesized (if any)
    pub capability: Option<SynthesizedCapability>,
    /// The gap that was detected
    pub gap: Option<CapabilityGap>,
    /// Error message if extension failed
    pub error: Option<String>,
}

/// Outcome-focused task execution loop
/// Runs until the task is genuinely complete - no arbitrary iteration limits
pub struct TaskLoop {
    config: Config,
    verifier: Verifier,
    backoff: BackoffConfig,
    iteration_timeout: Duration,
    checkpoint: Option<Checkpoint>,
    metrics: ExecutionMetrics,
    /// Signal handler for graceful shutdown
    shutdown_signal: Option<ShutdownSignal>,
    /// Progress callback for reporting
    progress_callback: Option<Arc<dyn ProgressCallback>>,
    /// Auto-save checkpoint on each iteration
    auto_checkpoint: bool,
    /// Self-extending capability configuration
    extension_config: ExtensionConfig,
    /// Capability registry for managing synthesized capabilities
    capability_registry: Option<CapabilityRegistry>,
    /// Gap detector for identifying missing capabilities
    gap_detector: Option<GapDetector>,
    /// Synthesis engine for creating new capabilities
    synthesis_engine: Option<SynthesisEngine>,
    /// Extension attempts counter for current task
    extension_attempts: u32,
}

impl TaskLoop {
    /// Create a new task loop
    pub fn new(config: Config, verifier: Verifier) -> Self {
        Self {
            config,
            verifier,
            backoff: BackoffConfig::default(),
            iteration_timeout: Duration::from_secs(DEFAULT_ITERATION_TIMEOUT_SECS),
            checkpoint: None,
            metrics: ExecutionMetrics::new(),
            shutdown_signal: None,
            progress_callback: None,
            auto_checkpoint: true,
            extension_config: ExtensionConfig::default(),
            capability_registry: None,
            gap_detector: None,
            synthesis_engine: None,
            extension_attempts: 0,
        }
    }

    /// Enable self-extending capabilities with custom configuration
    pub fn with_extension_config(mut self, config: ExtensionConfig) -> Self {
        self.extension_config = config.clone();
        if config.enabled {
            // Initialize extension components
            self.capability_registry = Some(CapabilityRegistry::new(&config.registry_path));
            self.gap_detector = Some(GapDetector::new());
            self.synthesis_engine = Some(SynthesisEngine::new(&config.skills_dir));
        }
        self
    }

    /// Enable self-extending capabilities with default configuration
    pub fn with_extension_enabled(mut self) -> Self {
        let mut config = ExtensionConfig::default();
        config.enabled = true;
        self.with_extension_config(config)
    }

    /// Create task loop with custom backoff configuration
    pub fn with_backoff(mut self, backoff: BackoffConfig) -> Self {
        self.backoff = backoff;
        self
    }

    /// Set custom iteration timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.iteration_timeout = timeout;
        self
    }

    /// Resume from a checkpoint
    pub fn with_checkpoint(mut self, checkpoint: Checkpoint) -> Self {
        self.checkpoint = Some(checkpoint);
        self
    }

    /// Set shutdown signal handler for graceful termination
    pub fn with_shutdown_signal(mut self, signal: ShutdownSignal) -> Self {
        self.shutdown_signal = Some(signal);
        self
    }

    /// Set progress callback for real-time reporting
    pub fn with_progress_callback(mut self, callback: Arc<dyn ProgressCallback>) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Enable or disable auto-checkpointing
    pub fn with_auto_checkpoint(mut self, enabled: bool) -> Self {
        self.auto_checkpoint = enabled;
        self
    }

    /// Get execution metrics
    pub fn metrics(&self) -> &ExecutionMetrics {
        &self.metrics
    }

    /// Emit a progress event
    fn emit_progress(&self, event: ProgressEvent) {
        if let Some(ref callback) = self.progress_callback {
            callback.on_progress(&event);
        }
    }

    /// Check if shutdown was requested
    fn is_shutdown_requested(&self) -> bool {
        self.shutdown_signal
            .as_ref()
            .map(|s| s.is_shutdown())
            .unwrap_or(false)
    }

    /// Execute a task until genuinely complete
    /// No iteration limits - runs until done or blocked
    pub async fn execute(&mut self, task: &str) -> Result<TaskResult> {
        let start_time = Instant::now();

        // Initialize or restore checkpoint
        let mut checkpoint = self
            .checkpoint
            .take()
            .unwrap_or_else(|| Checkpoint::new_with_context(task, &self.config.working_dir));
        checkpoint.state = TaskState::Running;

        let mut iteration = checkpoint.iteration;
        let mut accumulated_feedback = checkpoint.accumulated_feedback.clone();
        let mut last_output = checkpoint.last_output.clone();
        let mut consecutive_failures = 0u32;

        // Emit started event (no max_iterations - runs until done)
        self.emit_progress(
            ProgressEvent::new(ProgressEventType::Started, 0, 0)
                .with_message(task),
        );

        info!(
            "Starting outcome-focused task loop (no iteration limit, timeout {:?}/iteration)",
            self.iteration_timeout
        );

        // Run until complete - no arbitrary iteration cap
        loop {
            // Check for shutdown signal before each iteration
            if self.is_shutdown_requested() {
                warn!("Shutdown requested, saving checkpoint and exiting");
                checkpoint.state = TaskState::Blocked;
                checkpoint.set_metadata("shutdown_reason", "user_interrupt");
                self.save_checkpoint(&checkpoint)?;

                self.emit_progress(
                    ProgressEvent::new(
                        ProgressEventType::TaskInterrupted,
                        iteration,
                        0,
                    )
                    .with_metrics(self.metrics.clone()),
                );

                self.metrics
                    .finalize(start_time.elapsed().as_millis() as u64);
                return Ok(TaskResult {
                    success: false,
                    iterations: iteration,
                    summary: Some("Task interrupted by user".to_string()),
                    error: Some("Shutdown signal received".to_string()),
                    verification: None,
                });
            }

            // Safety valve: if we've had too many consecutive failures, stop
            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                error!(
                    "Too many consecutive failures ({}), giving up",
                    consecutive_failures
                );
                checkpoint.state = TaskState::Blocked;
                checkpoint.set_metadata("completion_status", "max_failures_reached");
                self.save_checkpoint(&checkpoint)?;

                self.emit_progress(
                    ProgressEvent::new(
                        ProgressEventType::TaskFailed,
                        iteration,
                        0,
                    )
                    .with_message(format!(
                        "Too many consecutive failures ({})",
                        consecutive_failures
                    ))
                    .with_metrics(self.metrics.clone()),
                );

                self.metrics
                    .finalize(start_time.elapsed().as_millis() as u64);

                return Ok(TaskResult {
                    success: false,
                    iterations: iteration,
                    summary: Some(checkpoint.create_recovery_summary()),
                    error: Some(format!(
                        "Stopped after {} consecutive failures",
                        consecutive_failures
                    )),
                    verification: Some(self.verifier.verify(&last_output, &self.config.working_dir)),
                });
            }

            iteration += 1;

            // Emit iteration start event
            self.emit_progress(ProgressEvent::new(
                ProgressEventType::IterationStart,
                iteration,
                0, // No max - runs until done
            ));

            println!();
            println!("═══════════════════════════════════════════════════════════════");
            println!("  📍 Iteration {} (running until complete)", iteration);
            println!("═══════════════════════════════════════════════════════════════");
            println!();

            // Clean up previous signal files
            self.cleanup_signals()?;

            // Build the evolved prompt based on history
            let iter_prompt = self.build_evolved_prompt(
                task,
                iteration,
                &accumulated_feedback,
                &checkpoint.iteration_history,
            );

            let iter_start = Instant::now();

            // Execute Claude with timeout
            let result = timeout(self.iteration_timeout, self.run_claude(&iter_prompt)).await;

            let iter_duration = iter_start.elapsed();
            let duration_ms = iter_duration.as_millis() as u64;

            match result {
                Ok(Ok(output)) => {
                    last_output = output.clone();
                    consecutive_failures = 0;

                    // Record successful iteration
                    self.metrics.record_iteration(iteration, duration_ms, true);

                    // Emit iteration complete event
                    self.emit_progress(
                        ProgressEvent::new(
                            ProgressEventType::IterationComplete,
                            iteration,
                            0,
                        )
                        .with_metrics(self.metrics.clone()),
                    );

                    // Try to parse structured output
                    let structured = self.parse_structured_output(&output);

                    // Record iteration attempt in history
                    let attempt = IterationAttempt {
                        iteration,
                        prompt: iter_prompt.clone(),
                        output: output.clone(),
                        success: true,
                        error: None,
                        duration_ms,
                        feedback: None,
                    };
                    checkpoint.iteration_history.push(attempt);

                    // Check for completion signals (file-based pattern)
                    if let Some(result) = self.check_completion_signals()? {
                        checkpoint.state = TaskState::Complete;
                        self.save_checkpoint(&checkpoint)?;

                        self.emit_progress(
                            ProgressEvent::new(
                                ProgressEventType::TaskComplete,
                                iteration,
                                0,
                            )
                            .with_message(result.summary.clone().unwrap_or_default())
                            .with_metrics(self.metrics.clone()),
                        );

                        self.metrics
                            .finalize(start_time.elapsed().as_millis() as u64);
                        return Ok(result.with_iterations(iteration));
                    }

                    // Check for completion promise in output
                    if let Some(ref promise) = self.config.completion_promise {
                        if output.contains(promise) {
                            info!("Completion promise '{}' detected in output", promise);

                            self.emit_progress(
                                ProgressEvent::new(
                                    ProgressEventType::PromiseDetected,
                                    iteration,
                                    0,
                                )
                                .with_message(promise.clone()),
                            );

                            checkpoint.state = TaskState::Complete;
                            self.save_checkpoint(&checkpoint)?;

                            self.emit_progress(
                                ProgressEvent::new(
                                    ProgressEventType::TaskComplete,
                                    iteration,
                                    0,
                                )
                                .with_message(format!("Completion promise '{}' detected", promise))
                                .with_metrics(self.metrics.clone()),
                            );

                            self.metrics
                                .finalize(start_time.elapsed().as_millis() as u64);
                            return Ok(TaskResult {
                                success: true,
                                iterations: iteration,
                                summary: Some(format!(
                                    "Task completed - promise '{}' detected",
                                    promise
                                )),
                                error: None,
                                verification: None,
                            });
                        }
                    }

                    // Check structured output status
                    if let Some(ref structured) = structured {
                        match structured.status {
                            ClaudeOutputStatus::Complete => {
                                if structured.confidence >= 0.8 {
                                    info!(
                                        "Task complete via structured output (confidence: {:.0}%)",
                                        structured.confidence * 100.0
                                    );
                                    checkpoint.state = TaskState::Complete;
                                    self.save_checkpoint(&checkpoint)?;

                                    self.emit_progress(
                                        ProgressEvent::new(
                                            ProgressEventType::TaskComplete,
                                            iteration,
                                            0,
                                        )
                                        .with_message(format!(
                                            "Confidence: {:.0}%",
                                            structured.confidence * 100.0
                                        ))
                                        .with_metrics(self.metrics.clone()),
                                    );

                                    self.metrics
                                        .finalize(start_time.elapsed().as_millis() as u64);
                                    return Ok(TaskResult {
                                        success: true,
                                        iterations: iteration,
                                        summary: structured.summary.clone(),
                                        error: None,
                                        verification: None,
                                    });
                                }
                            }
                            ClaudeOutputStatus::Blocked => {
                                warn!("Task blocked via structured output");
                                checkpoint.state = TaskState::Blocked;
                                self.save_checkpoint(&checkpoint)?;

                                self.emit_progress(
                                    ProgressEvent::new(
                                        ProgressEventType::TaskFailed,
                                        iteration,
                                        0,
                                    )
                                    .with_message("Task blocked")
                                    .with_metrics(self.metrics.clone()),
                                );

                                self.metrics
                                    .finalize(start_time.elapsed().as_millis() as u64);
                                return Ok(TaskResult {
                                    success: false,
                                    iterations: iteration,
                                    summary: None,
                                    error: Some("Task blocked".to_string()),
                                    verification: None,
                                });
                            }
                            _ => {}
                        }
                    }

                    // Verify the output
                    let verification = self.verifier.verify(&output, &self.config.working_dir);

                    // Emit shortcut detection events
                    for shortcut in &verification.shortcuts {
                        self.emit_progress(
                            ProgressEvent::new(
                                ProgressEventType::ShortcutDetected,
                                iteration,
                                0,
                            )
                            .with_message(format!("{:?}", shortcut)),
                        );
                    }

                    if verification.is_complete {
                        info!("Task verified as complete");
                        checkpoint.state = TaskState::Complete;
                        self.save_checkpoint(&checkpoint)?;

                        self.emit_progress(
                            ProgressEvent::new(
                                ProgressEventType::TaskComplete,
                                iteration,
                                0,
                            )
                            .with_message(verification.summary.clone().unwrap_or_default())
                            .with_metrics(self.metrics.clone()),
                        );

                        self.metrics
                            .finalize(start_time.elapsed().as_millis() as u64);
                        return Ok(TaskResult {
                            success: true,
                            iterations: iteration,
                            summary: verification.summary.clone(),
                            error: None,
                            verification: Some(verification),
                        });
                    }

                    // Evolve feedback based on verification results
                    let feedback = self.generate_evolved_feedback(
                        iteration,
                        &verification.shortcuts,
                        &structured,
                        &checkpoint.iteration_history,
                    );
                    accumulated_feedback.push_str(&feedback);

                    // Check for continue feedback file
                    if let Ok(file_feedback) =
                        fs::read_to_string(self.config.working_dir.join(CONTINUE_FILE))
                    {
                        info!("Continue feedback found");
                        accumulated_feedback.push_str(&format!(
                            "\n\n---\n**External feedback from iteration {}**:\n{}\n",
                            iteration,
                            file_feedback.trim()
                        ));
                    }

                    // Update checkpoint with iteration state
                    checkpoint.update_from_iteration(
                        iteration,
                        &last_output,
                        &accumulated_feedback,
                        IterationAttempt {
                            iteration,
                            prompt: iter_prompt.clone(),
                            output: output.clone(),
                            success: true,
                            error: None,
                            duration_ms,
                            feedback: Some(feedback.clone()),
                        },
                    );

                    // Auto-save checkpoint
                    if self.auto_checkpoint {
                        self.save_checkpoint(&checkpoint)?;
                        self.emit_progress(ProgressEvent::new(
                            ProgressEventType::CheckpointSaved,
                            iteration,
                            0,
                        ));
                    }
                }
                Ok(Err(e)) => {
                    consecutive_failures += 1;
                    self.metrics.record_iteration(iteration, duration_ms, false);
                    self.metrics.record_retry();

                    warn!(
                        "Claude execution error (failure {}/{}): {}",
                        consecutive_failures, MAX_CONSECUTIVE_FAILURES, e
                    );

                    // Emit iteration failed event
                    self.emit_progress(
                        ProgressEvent::new(
                            ProgressEventType::IterationFailed,
                            iteration,
                            0,
                        )
                        .with_message(e.to_string())
                        .with_metrics(self.metrics.clone()),
                    );

                    // Record failed attempt
                    let attempt = IterationAttempt {
                        iteration,
                        prompt: iter_prompt.clone(),
                        output: String::new(),
                        success: false,
                        error: Some(e.to_string()),
                        duration_ms,
                        feedback: None,
                    };
                    checkpoint.iteration_history.push(attempt);

                    // Auto-save checkpoint on failure
                    if self.auto_checkpoint {
                        checkpoint.iteration = iteration;
                        checkpoint.set_metadata("last_error", e.to_string());
                        self.save_checkpoint(&checkpoint)?;
                    }

                    // Apply exponential backoff
                    if consecutive_failures <= self.backoff.max_retries {
                        let delay = self.backoff.delay_for_attempt(consecutive_failures);
                        warn!("Applying backoff delay: {:?}", delay);
                        tokio::time::sleep(delay).await;
                    }

                    accumulated_feedback.push_str(&format!(
                        "\n\n---\n**Error in iteration {}**:\n{}\nPlease fix and continue.\n",
                        iteration, e
                    ));
                }
                Err(_) => {
                    // Timeout occurred
                    self.metrics.record_timeout();
                    self.metrics.record_iteration(iteration, duration_ms, false);
                    consecutive_failures += 1;

                    warn!(
                        "Iteration {} timed out after {:?}",
                        iteration, self.iteration_timeout
                    );

                    // Emit timeout event
                    self.emit_progress(
                        ProgressEvent::new(
                            ProgressEventType::IterationTimeout,
                            iteration,
                            0,
                        )
                        .with_message(format!("Timed out after {:?}", self.iteration_timeout))
                        .with_metrics(self.metrics.clone()),
                    );

                    // Record timeout attempt
                    let attempt = IterationAttempt {
                        iteration,
                        prompt: iter_prompt.clone(),
                        output: String::new(),
                        success: false,
                        error: Some(format!("Timeout after {:?}", self.iteration_timeout)),
                        duration_ms,
                        feedback: None,
                    };
                    checkpoint.iteration_history.push(attempt);

                    // Auto-save checkpoint on timeout
                    if self.auto_checkpoint {
                        checkpoint.iteration = iteration;
                        checkpoint.set_metadata("last_error", "timeout");
                        self.save_checkpoint(&checkpoint)?;
                    }

                    // Apply backoff for timeout
                    if consecutive_failures <= self.backoff.max_retries {
                        let delay = self.backoff.delay_for_attempt(consecutive_failures);
                        warn!("Applying backoff delay after timeout: {:?}", delay);
                        tokio::time::sleep(delay).await;
                    }

                    accumulated_feedback.push_str(&format!(
                        "\n\n---\n**Timeout in iteration {}**:\nThe previous attempt timed out after {:?}. Please work more efficiently and break down the task if needed.\n",
                        iteration, self.iteration_timeout
                    ));
                }
            }
        }
        // Note: This loop only exits via return statements above
        // - Completion signals (satisfied, blocked)
        // - Completion promise detected
        // - Structured output indicates complete/blocked
        // - Verification indicates complete
        // - Shutdown signal
        // - Too many consecutive failures
    }

    /// Execute a task with self-extending capability support
    ///
    /// This method wraps the standard execute() with capability gap detection
    /// and synthesis. On task failure, it:
    /// 1. Checks for capability gaps in the error output
    /// 2. If a gap is found and capability doesn't exist, synthesizes it
    /// 3. Verifies the synthesized capability
    /// 4. Registers and loads it
    /// 5. Retries the task with the new capability
    ///
    /// Extension attempts are limited to prevent infinite loops.
    pub async fn execute_with_extension(&mut self, task: &str) -> Result<TaskResult> {
        // Reset extension counter for this task
        self.extension_attempts = 0;

        loop {
            // Execute the task normally
            let result = self.execute(task).await?;

            // If successful or extension not enabled, return result
            if result.success || !self.extension_config.enabled {
                return Ok(result);
            }

            // Check if we've exceeded extension attempts
            if self.extension_attempts >= self.extension_config.max_attempts {
                warn!(
                    "Maximum extension attempts ({}) reached, returning failed result",
                    self.extension_config.max_attempts
                );
                return Ok(result);
            }

            // Try to detect a capability gap from the failure
            let gap = self.detect_capability_gap(&result).await;

            match gap {
                Some(detected_gap) => {
                    info!(
                        "Capability gap detected: {} (type: {:?})",
                        detected_gap.name(),
                        detected_gap.capability_type()
                    );

                    // Check if capability already exists
                    if let Some(ref registry) = self.capability_registry {
                        if registry.has_capability(detected_gap.name()) {
                            warn!(
                                "Capability '{}' already exists but task still failed",
                                detected_gap.name()
                            );
                            return Ok(result);
                        }
                    }

                    // Try to synthesize the capability
                    let extension_result = self.synthesize_and_register_capability(&detected_gap).await;

                    match extension_result {
                        Ok(ext_result) if ext_result.success => {
                            info!(
                                "Successfully synthesized capability: {}",
                                detected_gap.name()
                            );
                            self.extension_attempts += 1;

                            // Emit progress event for extension
                            self.emit_progress(
                                ProgressEvent::new(
                                    ProgressEventType::CapabilityExtended,
                                    self.extension_attempts,
                                    self.extension_config.max_attempts,
                                )
                                .with_message(format!(
                                    "Synthesized capability: {}",
                                    detected_gap.name()
                                )),
                            );

                            // Continue loop to retry with new capability
                            continue;
                        }
                        Ok(ext_result) => {
                            warn!(
                                "Failed to synthesize capability: {:?}",
                                ext_result.error
                            );
                            return Ok(result);
                        }
                        Err(e) => {
                            error!("Error during capability synthesis: {}", e);
                            return Ok(result);
                        }
                    }
                }
                None => {
                    // No capability gap detected, return the original result
                    debug!("No capability gap detected in failure");
                    return Ok(result);
                }
            }
        }
    }

    /// Detect a capability gap from a failed task result
    async fn detect_capability_gap(&self, result: &TaskResult) -> Option<CapabilityGap> {
        let gap_detector = self.gap_detector.as_ref()?;

        // Combine error and summary for analysis
        let context = format!(
            "Error: {}\nSummary: {}",
            result.error.as_deref().unwrap_or("Unknown error"),
            result.summary.as_deref().unwrap_or("No summary")
        );

        // Use the gap detector to analyze the failure
        gap_detector.detect_from_error(&context, &self.config.working_dir).await.ok().flatten()
    }

    /// Synthesize, verify, and register a capability for a detected gap
    async fn synthesize_and_register_capability(
        &mut self,
        gap: &CapabilityGap,
    ) -> Result<ExtensionResult> {
        let synthesis_engine = match &self.synthesis_engine {
            Some(engine) => engine,
            None => {
                return Ok(ExtensionResult {
                    success: false,
                    capability: None,
                    gap: Some(gap.clone()),
                    error: Some("Synthesis engine not initialized".to_string()),
                });
            }
        };

        // Step 1: Synthesize the capability
        info!("Synthesizing capability for gap: {}", gap.name());
        let synthesis_result = match synthesis_engine.synthesize(gap).await {
            Ok(result) => result,
            Err(e) => {
                return Ok(ExtensionResult {
                    success: false,
                    capability: None,
                    gap: Some(gap.clone()),
                    error: Some(format!("Synthesis failed: {}", e)),
                });
            }
        };

        // Extract the capability from the synthesis result
        let capability = synthesis_result.capability;

        // Step 2: Verify the capability
        info!("Verifying synthesized capability: {}", capability.name);
        let verifier = CapabilityVerifier::new();
        let verification = verifier.verify(&capability).await;

        if verification.success {
            info!("Capability verification passed: {}", capability.name);
        } else {
            return Ok(ExtensionResult {
                success: false,
                capability: Some(capability),
                gap: Some(gap.clone()),
                error: Some(format!("Verification failed: {:?}", verification.errors)),
            });
        }

        // Step 3: Load the capability (hot load)
        info!("Hot loading capability: {}", capability.name);
        let loader = HotLoader::new();
        if let Err(e) = loader.load(&capability) {
            warn!("Hot loading failed (non-fatal): {}", e);
        }

        // Step 4: Register the capability
        if let Some(ref mut registry) = self.capability_registry {
            if let Err(e) = registry.register(capability.clone()) {
                warn!("Registration failed (non-fatal): {}", e);
            }
        }

        Ok(ExtensionResult {
            success: true,
            capability: Some(capability),
            gap: Some(gap.clone()),
            error: None,
        })
    }

    /// Execute tasks in parallel across multiple files
    pub async fn execute_parallel(
        &mut self,
        task_template: &str,
        parallel_config: ParallelConfig,
    ) -> Result<ParallelResult> {
        use tokio::sync::Semaphore;

        let start_time = Instant::now();
        let semaphore = Arc::new(Semaphore::new(parallel_config.max_concurrent));
        let results: Arc<Mutex<HashMap<PathBuf, TaskResult>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let mut handles = Vec::new();

        for file in &parallel_config.files {
            let permit = semaphore.clone().acquire_owned().await?;
            let file = file.clone();
            let results = results.clone();
            let task = task_template.replace("{file}", &file.display().to_string());
            let config = self.config.clone();
            let verifier_promise = self.verifier.completion_promise().map(|s| s.to_string());
            let timeout_secs = parallel_config.task_timeout_secs;

            let handle = tokio::spawn(async move {
                let verifier = Verifier::new(verifier_promise);
                let mut task_loop =
                    TaskLoop::new(config, verifier).with_timeout(Duration::from_secs(timeout_secs));

                let result =
                    timeout(Duration::from_secs(timeout_secs), task_loop.execute(&task)).await;

                let task_result = match result {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => TaskResult {
                        success: false,
                        iterations: 0,
                        summary: None,
                        error: Some(e.to_string()),
                        verification: None,
                    },
                    Err(_) => TaskResult {
                        success: false,
                        iterations: 0,
                        summary: None,
                        error: Some(format!("Task timed out after {} seconds", timeout_secs)),
                        verification: None,
                    },
                };

                results.lock().await.insert(file, task_result);
                drop(permit);
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await?;
        }

        let results = Arc::try_unwrap(results)
            .expect("All handles completed")
            .into_inner();

        let succeeded_count = results.values().filter(|r| r.success).count();
        let failed_count = results.len() - succeeded_count;
        let all_succeeded = failed_count == 0;

        Ok(ParallelResult {
            results,
            all_succeeded,
            total_duration_ms: start_time.elapsed().as_millis() as u64,
            succeeded_count,
            failed_count,
        })
    }

    /// Get anti-laziness preamble for prompts when enabled
    fn get_anti_laziness_preamble(&self) -> String {
        format!(
            r#"## COMPLETION STANDARDS (Non-Negotiable)

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

5. **BUILD + TEST**
   cargo test --all must pass before completion.

The system will verify these claims. Do not waste iterations."#,
            self.config.anti_laziness.evidence_level
        )
    }

    /// Build an evolved prompt based on iteration history
    fn build_evolved_prompt(
        &self,
        task: &str,
        iteration: u32,
        feedback: &str,
        history: &[IterationAttempt],
    ) -> String {
        let mut prompt = String::new();

        // NEW: Anti-laziness preamble (opt-in)
        if self.config.anti_laziness.enabled {
            prompt.push_str(&self.get_anti_laziness_preamble());
            prompt.push_str("\n\n---\n\n");
        }

        prompt.push_str(task);

        // Add feedback from accumulated iterations
        if !feedback.is_empty() {
            prompt.push_str(feedback);
        }

        // Add learning from history if we have multiple attempts
        if history.len() >= 2 {
            let recent_failures: Vec<_> = history
                .iter()
                .rev()
                .take(3)
                .filter(|a| !a.success)
                .collect();

            if !recent_failures.is_empty() {
                prompt.push_str("\n\n---\n**⚠️ Learning from recent failures**:\n");
                for attempt in recent_failures {
                    if let Some(ref err) = attempt.error {
                        prompt.push_str(&format!("- Iteration {}: {}\n", attempt.iteration, err));
                    }
                }
                prompt.push_str("Please avoid repeating these mistakes.\n");
            }

            // Calculate success rate
            let success_count = history.iter().filter(|a| a.success).count();
            let total = history.len();
            let success_rate = (success_count as f32 / total as f32) * 100.0;

            if success_rate < 50.0 {
                prompt.push_str(&format!(
                    "\n**Note**: Success rate is low ({:.0}%). Consider a different approach.\n",
                    success_rate
                ));
            }
        }

        // Add iteration context - no max iterations, runs until done
        prompt.push_str(&format!(
            r#"

---
**Iteration**: {} (running until genuinely complete)

**Completion signals**:
- Create '{}' when genuinely satisfied with the result (include summary)
- Create '{}' with notes if more work is needed
- Create '{}' if blocked and cannot continue

**Structured Output** (optional but helpful):
You can include a JSON block in your output for better parsing:
```json
{{
  "status": "complete|in_progress|blocked|failed",
  "summary": "Brief summary of what was done",
  "files_modified": ["file1.rs", "file2.rs"],
  "next_steps": ["step1", "step2"],
  "errors": [],
  "confidence": 0.0-1.0
}}
```

**Self-Assessment**:
After completing the task, assess your own work:
- Did you fully accomplish the goal?
- Are there any remaining issues or improvements?
- Would you be genuinely satisfied with this result?
- Is this task actually complete, or just "good enough"?

**Important**: This task will run until you signal completion.
There is no iteration limit. Focus on actual outcomes, not progress.
"#,
            iteration, SATISFIED_FILE, CONTINUE_FILE, BLOCKED_FILE
        ));

        // Add completion promise if configured
        if let Some(ref promise) = self.config.completion_promise {
            prompt.push_str(&format!(
                "\n**Completion Promise**: Output `{}` when the task is complete.\n",
                promise
            ));
        }

        prompt
    }

    /// Generate evolved feedback based on verification and history
    fn generate_evolved_feedback(
        &self,
        iteration: u32,
        shortcuts: &[crate::verify::ShortcutType],
        structured: &Option<ClaudeStructuredOutput>,
        history: &[IterationAttempt],
    ) -> String {
        let mut feedback = String::new();

        // Add shortcut warnings
        if !shortcuts.is_empty() {
            warn!("Shortcuts detected: {:?}", shortcuts);
            feedback.push_str(&format!(
                "\n\n---\n**⚠️ Iteration {} Feedback - Shortcuts Detected**:\n",
                iteration
            ));
            for shortcut in shortcuts {
                feedback.push_str(&format!("- {:?}\n", shortcut));
            }
            feedback.push_str("Please address these issues before continuing.\n");
        }

        // Add structured output feedback
        if let Some(ref structured) = structured {
            if !structured.next_steps.is_empty() {
                feedback.push_str(&format!(
                    "\n**Your stated next steps from iteration {}**:\n",
                    iteration
                ));
                for step in &structured.next_steps {
                    feedback.push_str(&format!("- {}\n", step));
                }
            }

            if !structured.errors.is_empty() {
                feedback.push_str(&format!(
                    "\n**Errors you reported in iteration {}**:\n",
                    iteration
                ));
                for err in &structured.errors {
                    feedback.push_str(&format!("- {}\n", err));
                }
            }
        }

        // Add progress pattern analysis
        if history.len() >= 3 {
            let recent: Vec<_> = history.iter().rev().take(3).collect();
            let all_similar = recent.windows(2).all(|w| {
                // Check if outputs are very similar (could be stuck in a loop)
                let sim = string_similarity(&w[0].output, &w[1].output);
                sim > 0.9
            });

            if all_similar {
                feedback.push_str("\n\n**⚠️ Pattern detected**: Recent iterations produced very similar outputs. Consider:\n");
                feedback.push_str("- Taking a different approach\n");
                feedback.push_str("- Breaking down the task further\n");
                feedback.push_str("- Addressing the root cause of the issue\n");
            }
        }

        feedback
    }

    /// Generate anti-laziness feedback when mock data is detected
    fn generate_anti_laziness_feedback(
        &self,
        mock_result: Option<&MockDataResult>,
        evidence_insufficient: bool,
    ) -> String {
        let mut feedback = String::new();

        if !self.config.anti_laziness.enabled {
            return feedback;
        }

        // Add mock data feedback
        if let Some(mock) = mock_result {
            if !mock.is_clean {
                feedback.push_str("\n\n## ANTI-LAZINESS CHECK FAILED\n\n");
                feedback.push_str("The following mock data patterns were detected:\n\n");

                for pattern in &mock.patterns_found {
                    if pattern.severity == Severity::Error {
                        feedback.push_str(&format!("**{:?}**\n", pattern.pattern_type));
                        feedback.push_str("Files affected:\n");
                        for file in &pattern.files_affected {
                            feedback.push_str(&format!("- {}\n", file.display()));
                        }
                        feedback.push_str("Examples:\n");
                        for example in &pattern.examples {
                            feedback.push_str(&format!("  `{}`\n", example));
                        }
                        feedback.push('\n');
                    }
                }

                feedback.push_str("You MUST fix these issues before claiming completion.\n");
            }
        }

        // Add evidence insufficient feedback
        if evidence_insufficient {
            feedback.push_str("\n\n## INSUFFICIENT EVIDENCE\n\n");
            feedback.push_str(&format!(
                "Evidence level {:?} is required. Your completion claim lacks:\n",
                self.config.anti_laziness.evidence_level
            ));
            feedback.push_str("- Specific files modified with line counts\n");
            feedback.push_str("- Verification steps performed\n");
            feedback.push_str("- Test output if applicable\n");
        }

        feedback
    }

    /// Parse structured JSON output from Claude's response
    fn parse_structured_output(&self, output: &str) -> Option<ClaudeStructuredOutput> {
        // Look for JSON block in the output
        let json_regex = Regex::new(r"```json\s*(\{[\s\S]*?\})\s*```").ok()?;

        if let Some(captures) = json_regex.captures(output) {
            if let Some(json_match) = captures.get(1) {
                let json_str = json_match.as_str();
                match serde_json::from_str::<ClaudeStructuredOutput>(json_str) {
                    Ok(mut structured) => {
                        structured.raw_output = Some(output.to_string());
                        debug!("Parsed structured output: {:?}", structured.status);
                        return Some(structured);
                    }
                    Err(e) => {
                        debug!("Failed to parse structured output: {}", e);
                    }
                }
            }
        }

        // Try to find inline JSON (not in code block)
        let inline_regex = Regex::new(r#"\{[^{}]*"status"\s*:\s*"[^"]+""#).ok()?;
        if inline_regex.is_match(output) {
            // Try to extract and parse the full JSON object
            if let Some(start) = output.find('{') {
                let mut depth = 0;
                let mut end = start;
                for (i, c) in output[start..].chars().enumerate() {
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
                    let json_str = &output[start..end];
                    if let Ok(mut structured) =
                        serde_json::from_str::<ClaudeStructuredOutput>(json_str)
                    {
                        structured.raw_output = Some(output.to_string());
                        return Some(structured);
                    }
                }
            }
        }

        None
    }

    /// Run Claude with the given prompt
    async fn run_claude(&self, prompt: &str) -> Result<String> {
        debug!("Running Claude with prompt ({} chars)", prompt.len());

        let mut cmd = Command::new("claude");

        // Add model flag if specified
        if let Some(ref model) = self.config.model {
            cmd.arg("--model").arg(model);
        }

        // Skip permissions for autonomous operation
        cmd.arg("--dangerously-skip-permissions");

        // Set working directory
        cmd.current_dir(&self.config.working_dir);

        // Add the prompt
        cmd.arg(prompt);

        let output = cmd.output().context("Failed to execute Claude")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            warn!("Claude exited with status: {}", output.status);
            if !stderr.is_empty() {
                debug!("stderr: {}", stderr);
            }
        }

        // Combine stdout and stderr for full output
        let full_output = if stderr.is_empty() {
            stdout
        } else {
            format!("{}\n\n--- STDERR ---\n{}", stdout, stderr)
        };

        Ok(full_output)
    }

    /// Check for completion signal files
    fn check_completion_signals(&self) -> Result<Option<TaskResult>> {
        let working_dir = &self.config.working_dir;

        // Check for satisfied signal
        let satisfied_path = working_dir.join(SATISFIED_FILE);
        if satisfied_path.exists() {
            let summary = fs::read_to_string(&satisfied_path)
                .ok()
                .filter(|s| !s.trim().is_empty());
            info!("Found satisfaction signal");
            return Ok(Some(TaskResult {
                success: true,
                iterations: 0, // Will be set by caller
                summary,
                error: None,
                verification: None,
            }));
        }

        // Check for blocked signal
        let blocked_path = working_dir.join(BLOCKED_FILE);
        if blocked_path.exists() {
            let reason = fs::read_to_string(&blocked_path).ok();
            warn!("Found blocked signal");
            return Ok(Some(TaskResult {
                success: false,
                iterations: 0,
                summary: None,
                error: reason.or_else(|| Some("Blocked without reason".to_string())),
                verification: None,
            }));
        }

        Ok(None)
    }

    /// Clean up signal files from previous iterations
    fn cleanup_signals(&self) -> Result<()> {
        let working_dir = &self.config.working_dir;

        for file in [SATISFIED_FILE, CONTINUE_FILE, BLOCKED_FILE] {
            let path = working_dir.join(file);
            if path.exists() {
                fs::remove_file(&path).ok();
            }
        }

        Ok(())
    }

    /// Save checkpoint to working directory
    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        let path = self.config.working_dir.join(CHECKPOINT_FILE);
        checkpoint.save(&path)?;
        debug!("Saved checkpoint to {:?}", path);
        Ok(())
    }

    /// Load checkpoint from working directory if exists
    pub fn load_checkpoint(working_dir: &Path) -> Option<Checkpoint> {
        let path = working_dir.join(CHECKPOINT_FILE);
        if path.exists() {
            match Checkpoint::load(&path) {
                Ok(checkpoint) => {
                    info!("Loaded checkpoint from {:?}", path);
                    Some(checkpoint)
                }
                Err(e) => {
                    warn!("Failed to load checkpoint: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }
}

/// Simple string similarity metric (Jaccard index on word tokens)
fn string_similarity(a: &str, b: &str) -> f32 {
    let words_a: std::collections::HashSet<_> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<_> = b.split_whitespace().collect();

    if words_a.is_empty() && words_b.is_empty() {
        return 1.0;
    }

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    if union == 0 {
        return 0.0;
    }

    intersection as f32 / union as f32
}

// Extension trait for Verifier to get completion promise
trait VerifierExt {
    fn completion_promise(&self) -> Option<&str>;
}

impl VerifierExt for Verifier {
    fn completion_promise(&self) -> Option<&str> {
        // This would need to be exposed by the Verifier
        // For now, return None
        None
    }
}

impl TaskResult {
    /// Update iterations count
    fn with_iterations(mut self, iterations: u32) -> Self {
        self.iterations = iterations;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::Verifier;
    use crate::AntiLazinessConfig;
    use tempfile::TempDir;

    #[test]
    fn test_build_evolved_prompt() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: Some("DONE".to_string()),
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };
        let verifier = Verifier::new(Some("DONE".to_string()));
        let task_loop = TaskLoop::new(config, verifier);

        let prompt = task_loop.build_evolved_prompt("Build a thing", 2, "", &[]);

        // Should mention iteration number but not max iterations
        assert!(prompt.contains("Iteration"));
        assert!(prompt.contains("running until genuinely complete"));
        assert!(prompt.contains("DONE"));
        assert!(prompt.contains(".gogo-gadget-satisfied"));
        assert!(prompt.contains("Structured Output"));
        // Should NOT have "of X" pattern for max iterations
        assert!(!prompt.contains(" of 5"));
        assert!(!prompt.contains(" of 10"));
    }

    #[test]
    fn test_build_prompt_with_history() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: None,
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };
        let verifier = Verifier::new(None);
        let task_loop = TaskLoop::new(config, verifier);

        let history = vec![
            IterationAttempt {
                iteration: 1,
                prompt: "test".to_string(),
                output: "output1".to_string(),
                success: false,
                error: Some("First error".to_string()),
                duration_ms: 1000,
                feedback: None,
            },
            IterationAttempt {
                iteration: 2,
                prompt: "test".to_string(),
                output: "output2".to_string(),
                success: false,
                error: Some("Second error".to_string()),
                duration_ms: 1000,
                feedback: None,
            },
        ];

        let prompt = task_loop.build_evolved_prompt("Build a thing", 3, "", &history);

        assert!(prompt.contains("Learning from recent failures"));
        assert!(prompt.contains("First error"));
        assert!(prompt.contains("Second error"));
    }

    #[test]
    fn test_check_satisfied_signal() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: None,
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };
        let verifier = Verifier::new(None);
        let task_loop = TaskLoop::new(config, verifier);

        // Create satisfied file
        fs::write(
            temp_dir.path().join(SATISFIED_FILE),
            "Task completed successfully!",
        )
        .unwrap();

        let result = task_loop.check_completion_signals().unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().success);
    }

    #[test]
    fn test_check_blocked_signal() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: None,
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };
        let verifier = Verifier::new(None);
        let task_loop = TaskLoop::new(config, verifier);

        // Create blocked file
        fs::write(
            temp_dir.path().join(BLOCKED_FILE),
            "Cannot proceed: missing dependencies",
        )
        .unwrap();

        let result = task_loop.check_completion_signals().unwrap();
        assert!(result.is_some());
        assert!(!result.unwrap().success);
    }

    #[test]
    fn test_parse_structured_output() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: None,
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };
        let verifier = Verifier::new(None);
        let task_loop = TaskLoop::new(config, verifier);

        let output = r#"
Here's what I did:
```json
{
  "status": "complete",
  "summary": "Implemented the feature",
  "files_modified": ["src/main.rs"],
  "next_steps": [],
  "errors": [],
  "confidence": 0.95
}
```
All done!
"#;

        let structured = task_loop.parse_structured_output(output);
        assert!(structured.is_some());
        let s = structured.unwrap();
        assert_eq!(s.status, ClaudeOutputStatus::Complete);
        assert_eq!(s.confidence, 0.95);
        assert_eq!(s.summary, Some("Implemented the feature".to_string()));
    }

    #[test]
    fn test_parse_structured_output_in_progress() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: None,
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };
        let verifier = Verifier::new(None);
        let task_loop = TaskLoop::new(config, verifier);

        let output = r#"
Working on it:
```json
{
  "status": "in_progress",
  "summary": "Started implementation",
  "files_modified": [],
  "next_steps": ["Add tests", "Fix edge cases"],
  "errors": [],
  "confidence": 0.4
}
```
"#;

        let structured = task_loop.parse_structured_output(output);
        assert!(structured.is_some());
        let s = structured.unwrap();
        assert_eq!(s.status, ClaudeOutputStatus::InProgress);
        assert_eq!(s.next_steps, vec!["Add tests", "Fix edge cases"]);
    }

    #[test]
    fn test_string_similarity() {
        assert_eq!(string_similarity("hello world", "hello world"), 1.0);
        assert!(string_similarity("hello world", "hello there") > 0.0);
        assert!(string_similarity("hello world", "hello there") < 1.0);
        assert_eq!(
            string_similarity("completely different", "no overlap here"),
            0.0
        );
    }

    #[test]
    fn test_task_loop_with_backoff() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: None,
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };
        let verifier = Verifier::new(None);
        let backoff = BackoffConfig {
            initial_delay_ms: 100,
            max_delay_ms: 1000,
            multiplier: 2.0,
            max_retries: 3,
            jitter: false,
        };

        let task_loop = TaskLoop::new(config, verifier).with_backoff(backoff);
        assert_eq!(task_loop.backoff.initial_delay_ms, 100);
        assert_eq!(task_loop.backoff.max_retries, 3);
    }

    #[test]
    fn test_task_loop_with_timeout() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: None,
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };
        let verifier = Verifier::new(None);

        let task_loop = TaskLoop::new(config, verifier).with_timeout(Duration::from_secs(120));
        assert_eq!(task_loop.iteration_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_checkpoint_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint = Checkpoint::new("Test task");

        let path = temp_dir.path().join("test-checkpoint.json");
        checkpoint.save(&path).unwrap();

        let loaded = Checkpoint::load(&path).unwrap();
        assert_eq!(loaded.task, "Test task");
        assert_eq!(loaded.id, checkpoint.id);
    }

    #[test]
    fn test_metrics_tracking() {
        let mut metrics = ExecutionMetrics::new();

        metrics.record_iteration(1, 1000, true);
        metrics.record_iteration(2, 2000, true);
        metrics.record_iteration(3, 1500, false);
        metrics.record_retry();
        metrics.record_timeout();

        assert_eq!(metrics.successful_iterations, 2);
        assert_eq!(metrics.failed_iterations, 1);
        assert_eq!(metrics.retry_count, 1);
        assert_eq!(metrics.timeout_count, 1);
        assert_eq!(metrics.avg_iteration_ms, Some(1500));

        metrics.finalize(5000);
        assert_eq!(metrics.total_duration_ms, 5000);
    }

    #[test]
    fn test_generate_evolved_feedback_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: None,
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        };
        let verifier = Verifier::new(None);
        let task_loop = TaskLoop::new(config, verifier);

        // Empty shortcuts since shortcut detection is deprecated
        let shortcuts = vec![];

        // With no shortcuts, no structured output, and no history, feedback is empty
        let feedback = task_loop.generate_evolved_feedback(1, &shortcuts, &None, &[]);
        assert!(feedback.is_empty());

        // With structured output containing next steps, we should get feedback
        let structured = Some(crate::ClaudeStructuredOutput {
            status: crate::ClaudeOutputStatus::InProgress,
            summary: Some("Test".to_string()),
            next_steps: vec!["Step 1".to_string()],
            errors: vec![],
            files_modified: vec![],
            confidence: 0.5,
            raw_output: None,
        });
        let feedback = task_loop.generate_evolved_feedback(1, &shortcuts, &structured, &[]);
        assert!(feedback.contains("next steps"));
        assert!(feedback.contains("1"));
    }
}
