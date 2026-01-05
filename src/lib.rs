//! GoGoGadget Core Library
//!
//! A Rust implementation of the Ralph-Wiggum iterative task execution pattern.
//! This library provides:
//!
//! - **Task Analysis**: Analyze tasks for complexity and execution strategy
//! - **Task Loop**: Ralph-style iteration until verified completion
//! - **Verification**: Detect incomplete work and mock data
//! - **Checkpointing**: Save and restore execution state between iterations
//! - **Parallel Execution**: Support for multi-file parallel task execution
//! - **Signal Handling**: Graceful shutdown on SIGINT/SIGTERM
//! - **Progress Callbacks**: Real-time progress reporting

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub mod brain;
pub mod extend;
pub mod rlm;
pub mod subagent;
pub mod swarm;
pub mod task_loop;
pub mod verify;

/// Evidence level required for completion verification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceLevel {
    /// Minimal evidence - just need files_modified to be non-empty
    Minimal,
    /// Adequate evidence - need files + summary (default)
    #[default]
    Adequate,
    /// Strong evidence - need files + substantial summary + confidence >= 0.9
    Strong,
}

impl std::str::FromStr for EvidenceLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "minimal" => Ok(EvidenceLevel::Minimal),
            "adequate" => Ok(EvidenceLevel::Adequate),
            "strong" => Ok(EvidenceLevel::Strong),
            _ => Err(format!("Invalid evidence level: {}. Use minimal, adequate, or strong.", s)),
        }
    }
}

/// Anti-laziness enforcement configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiLazinessConfig {
    /// Master switch for anti-laziness enforcement
    pub enabled: bool,

    /// Detect mock data patterns in output
    pub mock_detection_enabled: bool,

    /// Require evidence in completion claims
    pub evidence_required: bool,

    /// Minimum confidence threshold for completion (0.0-1.0)
    pub min_confidence: f32,

    /// Evidence level required (Minimal, Adequate, Strong)
    pub evidence_level: EvidenceLevel,
}

impl Default for AntiLazinessConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Off by default for backwards compat
            mock_detection_enabled: true,
            evidence_required: true,
            min_confidence: 0.8,
            evidence_level: EvidenceLevel::Adequate,
        }
    }
}

/// Configuration for task execution
/// Note: No iteration limits - tasks run until genuinely complete
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Working directory for task execution
    pub working_dir: PathBuf,

    /// Optional completion promise string to detect in output
    pub completion_promise: Option<String>,

    /// Optional model override for Claude
    pub model: Option<String>,

    /// Dry run mode - analyze without executing
    pub dry_run: bool,

    /// Anti-laziness enforcement configuration
    #[serde(default)]
    pub anti_laziness: AntiLazinessConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            completion_promise: None,
            model: None,
            dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
        }
    }
}

/// Result of a task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Whether the task completed successfully
    pub success: bool,

    /// Number of iterations executed
    pub iterations: u32,

    /// Summary of what was accomplished
    pub summary: Option<String>,

    /// Error message if failed
    pub error: Option<String>,

    /// Detailed verification results
    pub verification: Option<verify::VerificationResult>,
}

/// Execution mode determined by task analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Single agent execution
    Single,
    /// Swarm of parallel agents
    Swarm { agent_count: u32 },
}

/// Task difficulty level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Trivial,
    Easy,
    Medium,
    Hard,
    Expert,
}

impl From<u32> for Difficulty {
    fn from(score: u32) -> Self {
        match score {
            0..=2 => Difficulty::Trivial,
            3..=4 => Difficulty::Easy,
            5..=6 => Difficulty::Medium,
            7..=8 => Difficulty::Hard,
            _ => Difficulty::Expert,
        }
    }
}

/// Task state for tracking execution progress
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskState {
    /// Task is pending execution
    #[default]
    Pending,
    /// Task is currently running
    Running,
    /// Task is blocked and cannot proceed
    Blocked,
    /// Task has completed (successfully or not)
    Complete,
}

/// Checkpoint for saving execution state between iterations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique identifier for this checkpoint
    pub id: String,

    /// Current iteration number
    pub iteration: u32,

    /// Current task state
    pub state: TaskState,

    /// The original task prompt
    pub task: String,

    /// Accumulated feedback from previous iterations
    pub accumulated_feedback: String,

    /// Output from the last iteration
    pub last_output: String,

    /// History of previous iteration attempts
    pub iteration_history: Vec<IterationAttempt>,

    /// Timestamp when checkpoint was created (as unix epoch seconds)
    pub created_at: u64,

    /// Custom metadata for extensibility
    pub metadata: HashMap<String, String>,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(task: &str) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        Self {
            id: uuid_v4(),
            iteration: 0,
            state: TaskState::Pending,
            task: task.to_string(),
            accumulated_feedback: String::new(),
            last_output: String::new(),
            iteration_history: Vec::new(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            metadata: HashMap::new(),
        }
    }

    /// Save checkpoint to a file
    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load checkpoint from a file
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let checkpoint: Self = serde_json::from_str(&json)?;
        Ok(checkpoint)
    }
}

/// Record of a single iteration attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationAttempt {
    /// Iteration number
    pub iteration: u32,

    /// The prompt used for this iteration
    pub prompt: String,

    /// Output from Claude
    pub output: String,

    /// Whether this iteration succeeded
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,

    /// Duration of this iteration in milliseconds
    pub duration_ms: u64,

    /// Feedback generated from this iteration
    pub feedback: Option<String>,
}

/// Execution metrics for timing and performance tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionMetrics {
    /// Total execution time in milliseconds
    pub total_duration_ms: u64,

    /// Time spent in each iteration (keyed by iteration number)
    pub iteration_times_ms: HashMap<u32, u64>,

    /// Number of retries performed
    pub retry_count: u32,

    /// Number of timeouts encountered
    pub timeout_count: u32,

    /// Number of successful iterations
    pub successful_iterations: u32,

    /// Number of failed iterations
    pub failed_iterations: u32,

    /// Peak memory usage (if available)
    pub peak_memory_bytes: Option<u64>,

    /// Average iteration time in milliseconds
    pub avg_iteration_ms: Option<u64>,
}

impl ExecutionMetrics {
    /// Create a new metrics tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an iteration time
    pub fn record_iteration(&mut self, iteration: u32, duration_ms: u64, success: bool) {
        self.iteration_times_ms.insert(iteration, duration_ms);
        if success {
            self.successful_iterations += 1;
        } else {
            self.failed_iterations += 1;
        }
        self.update_averages();
    }

    /// Record a retry
    pub fn record_retry(&mut self) {
        self.retry_count += 1;
    }

    /// Record a timeout
    pub fn record_timeout(&mut self) {
        self.timeout_count += 1;
    }

    /// Finalize metrics with total duration
    pub fn finalize(&mut self, total_duration_ms: u64) {
        self.total_duration_ms = total_duration_ms;
        self.update_averages();
    }

    /// Update computed averages
    fn update_averages(&mut self) {
        if !self.iteration_times_ms.is_empty() {
            let total: u64 = self.iteration_times_ms.values().sum();
            self.avg_iteration_ms = Some(total / self.iteration_times_ms.len() as u64);
        }
    }
}

/// Structured output from Claude for JSON parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeStructuredOutput {
    /// Status of the task
    pub status: ClaudeOutputStatus,

    /// Summary of what was done
    pub summary: Option<String>,

    /// Files that were modified
    pub files_modified: Vec<String>,

    /// Next steps if not complete
    pub next_steps: Vec<String>,

    /// Any errors encountered
    pub errors: Vec<String>,

    /// Confidence level (0.0 - 1.0)
    pub confidence: f32,

    /// Raw output text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<String>,
}

/// Status reported by Claude in structured output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeOutputStatus {
    /// Task is complete
    Complete,
    /// Task is in progress, needs more iterations
    InProgress,
    /// Task is blocked
    Blocked,
    /// Task failed
    Failed,
    /// Unknown status (parsing failed)
    Unknown,
}

impl Default for ClaudeStructuredOutput {
    fn default() -> Self {
        Self {
            status: ClaudeOutputStatus::Unknown,
            summary: None,
            files_modified: Vec::new(),
            next_steps: Vec::new(),
            errors: Vec::new(),
            confidence: 0.0,
            raw_output: None,
        }
    }
}

/// Configuration for backoff strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffConfig {
    /// Initial delay in milliseconds
    pub initial_delay_ms: u64,

    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,

    /// Multiplier for each retry
    pub multiplier: f64,

    /// Maximum number of retries
    pub max_retries: u32,

    /// Add randomness to delays (jitter)
    pub jitter: bool,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            multiplier: 2.0,
            max_retries: 5,
            jitter: true,
        }
    }
}

impl BackoffConfig {
    /// Calculate delay for a given retry attempt
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(0);
        }

        let delay = self.initial_delay_ms as f64 * self.multiplier.powi(attempt as i32 - 1);
        let delay = delay.min(self.max_delay_ms as f64);

        let delay = if self.jitter {
            // Add up to 25% jitter
            let jitter = delay * 0.25 * rand_f64();
            delay + jitter
        } else {
            delay
        };

        Duration::from_millis(delay as u64)
    }
}

/// Configuration for parallel execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelConfig {
    /// Maximum number of concurrent tasks
    pub max_concurrent: usize,

    /// Files to process in parallel
    pub files: Vec<PathBuf>,

    /// Whether to share context between parallel tasks
    pub share_context: bool,

    /// Timeout per parallel task in seconds
    pub task_timeout_secs: u64,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            files: Vec::new(),
            share_context: false,
            task_timeout_secs: 300,
        }
    }
}

/// Result of parallel execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelResult {
    /// Results for each file/task
    pub results: HashMap<PathBuf, TaskResult>,

    /// Overall success (all tasks succeeded)
    pub all_succeeded: bool,

    /// Total execution time
    pub total_duration_ms: u64,

    /// Number of successful tasks
    pub succeeded_count: usize,

    /// Number of failed tasks
    pub failed_count: usize,
}

/// Simple UUID v4 generation (no external dependency)
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let random = (time ^ (time >> 64)) as u64;

    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (random >> 32) as u32,
        ((random >> 16) & 0xFFFF) as u16,
        (random & 0x0FFF) as u16,
        ((random >> 48) & 0x3FFF) | 0x8000,
        random & 0xFFFFFFFFFFFF
    )
}

/// Simple random f64 between 0 and 1
fn rand_f64() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    (nanos as f64) / (u32::MAX as f64)
}

/// Signal handler for graceful shutdown
#[derive(Clone)]
pub struct ShutdownSignal {
    /// Flag indicating shutdown was requested
    shutdown: Arc<AtomicBool>,
    /// Flag indicating interrupt was requested (Ctrl+C)
    interrupted: Arc<AtomicBool>,
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownSignal {
    /// Create a new shutdown signal handler
    pub fn new() -> Self {
        Self {
            shutdown: Arc::new(AtomicBool::new(false)),
            interrupted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if shutdown was requested
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Check if interrupt (Ctrl+C) was received
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    /// Request shutdown
    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Mark as interrupted
    pub fn mark_interrupted(&self) {
        self.interrupted.store(true, Ordering::SeqCst);
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Get arc clone for sharing across threads
    pub fn clone_shutdown_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Install signal handlers (call once at startup)
    pub fn install_handlers(&self) -> anyhow::Result<()> {
        let shutdown = Arc::clone(&self.shutdown);
        let interrupted = Arc::clone(&self.interrupted);

        ctrlc::set_handler(move || {
            // On first interrupt, set flags
            if !shutdown.load(Ordering::SeqCst) {
                eprintln!(
                    "\n⚠️  Interrupt received - saving checkpoint and shutting down gracefully..."
                );
                interrupted.store(true, Ordering::SeqCst);
                shutdown.store(true, Ordering::SeqCst);
            } else {
                // On second interrupt, force exit
                eprintln!("\n⛔ Force exit requested!");
                std::process::exit(130);
            }
        })?;

        Ok(())
    }
}

/// Progress event for callbacks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// Event type
    pub event_type: ProgressEventType,
    /// Current iteration (no maximum - runs until complete)
    pub iteration: u32,
    /// Timestamp (unix epoch millis)
    pub timestamp_ms: u64,
    /// Optional message
    pub message: Option<String>,
    /// Optional metrics snapshot
    pub metrics: Option<ExecutionMetrics>,
}

/// Types of progress events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressEventType {
    /// Task execution started
    Started,
    /// New iteration beginning
    IterationStart,
    /// Iteration completed successfully
    IterationComplete,
    /// Iteration failed
    IterationFailed,
    /// Iteration timed out
    IterationTimeout,
    /// Task completed successfully
    TaskComplete,
    /// Task failed
    TaskFailed,
    /// Task was interrupted
    TaskInterrupted,
    /// Checkpoint saved
    CheckpointSaved,
    /// Checkpoint loaded
    CheckpointLoaded,
    /// Promise detected in output
    PromiseDetected,
    /// Shortcut detected in output
    ShortcutDetected,
    /// Capability was synthesized and extended
    CapabilityExtended,
    /// Capability gap was detected
    CapabilityGapDetected,
}

impl ProgressEvent {
    /// Create a new progress event
    /// Note: max_iterations parameter kept for API compatibility but ignored (always 0)
    pub fn new(event_type: ProgressEventType, iteration: u32, _max_iterations: u32) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        Self {
            event_type,
            iteration,
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            message: None,
            metrics: None,
        }
    }

    /// Add message to event
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Add metrics snapshot to event
    pub fn with_metrics(mut self, metrics: ExecutionMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }
}

/// Trait for progress callbacks
pub trait ProgressCallback: Send + Sync {
    /// Called when a progress event occurs
    fn on_progress(&self, event: &ProgressEvent);
}

/// Console progress reporter with colored output
pub struct ConsoleProgressReporter {
    /// Whether to use colors
    use_colors: bool,
    /// Whether to show verbose output
    verbose: bool,
}

impl ConsoleProgressReporter {
    /// Create a new console progress reporter
    pub fn new(use_colors: bool, verbose: bool) -> Self {
        Self {
            use_colors,
            verbose,
        }
    }
}

impl ProgressCallback for ConsoleProgressReporter {
    fn on_progress(&self, event: &ProgressEvent) {
        use colored::Colorize;

        let msg = match event.event_type {
            ProgressEventType::Started => {
                "🚀 Starting task execution (runs until complete)".to_string()
            }
            ProgressEventType::IterationStart => {
                format!(
                    "📍 Starting iteration {} (running until complete)",
                    event.iteration
                )
            }
            ProgressEventType::IterationComplete => {
                let elapsed = if let Some(ref metrics) = event.metrics {
                    metrics
                        .iteration_times_ms
                        .get(&event.iteration)
                        .map(|ms| format!(" ({:.1}s)", *ms as f64 / 1000.0))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                format!("✅ Iteration {} completed{}", event.iteration, elapsed)
            }
            ProgressEventType::IterationFailed => {
                format!(
                    "❌ Iteration {} failed: {}",
                    event.iteration,
                    event.message.as_deref().unwrap_or("unknown error")
                )
            }
            ProgressEventType::IterationTimeout => {
                format!("⏱️  Iteration {} timed out", event.iteration)
            }
            ProgressEventType::TaskComplete => {
                format!(
                    "🎉 Task completed successfully after {} iterations!",
                    event.iteration
                )
            }
            ProgressEventType::TaskFailed => {
                format!(
                    "💔 Task failed after {} iterations: {}",
                    event.iteration,
                    event.message.as_deref().unwrap_or("unknown error")
                )
            }
            ProgressEventType::TaskInterrupted => {
                format!("⚠️  Task interrupted at iteration {}", event.iteration)
            }
            ProgressEventType::CheckpointSaved => {
                format!("💾 Checkpoint saved at iteration {}", event.iteration)
            }
            ProgressEventType::CheckpointLoaded => {
                format!(
                    "📂 Resuming from checkpoint at iteration {}",
                    event.iteration
                )
            }
            ProgressEventType::PromiseDetected => {
                format!(
                    "🎯 Completion promise detected: {}",
                    event.message.as_deref().unwrap_or("")
                )
            }
            ProgressEventType::ShortcutDetected => {
                format!(
                    "⚠️  Shortcut detected: {}",
                    event.message.as_deref().unwrap_or("")
                )
            }
            ProgressEventType::CapabilityExtended => {
                format!(
                    "🔧 Capability extended: {}",
                    event.message.as_deref().unwrap_or("")
                )
            }
            ProgressEventType::CapabilityGapDetected => {
                format!(
                    "🔍 Capability gap detected: {}",
                    event.message.as_deref().unwrap_or("")
                )
            }
        };

        if self.use_colors {
            let colored_msg = match event.event_type {
                ProgressEventType::Started | ProgressEventType::TaskComplete => msg.green().bold(),
                ProgressEventType::IterationStart | ProgressEventType::CheckpointLoaded => {
                    msg.cyan()
                }
                ProgressEventType::IterationComplete | ProgressEventType::PromiseDetected => {
                    msg.green()
                }
                ProgressEventType::IterationFailed | ProgressEventType::TaskFailed => {
                    msg.red().bold()
                }
                ProgressEventType::IterationTimeout
                | ProgressEventType::ShortcutDetected
                | ProgressEventType::TaskInterrupted => msg.yellow(),
                ProgressEventType::CheckpointSaved => msg.blue(),
                ProgressEventType::CapabilityExtended => msg.magenta().bold(),
                ProgressEventType::CapabilityGapDetected => msg.magenta(),
            };
            println!("{}", colored_msg);
        } else {
            println!("{}", msg);
        }

        // Verbose metrics output
        if self.verbose {
            if let Some(ref metrics) = event.metrics {
                if let Some(avg) = metrics.avg_iteration_ms {
                    println!("  📊 Avg iteration time: {:.1}s", avg as f64 / 1000.0);
                }
                if metrics.retry_count > 0 {
                    println!("  🔄 Retries: {}", metrics.retry_count);
                }
                if metrics.timeout_count > 0 {
                    println!("  ⏱️  Timeouts: {}", metrics.timeout_count);
                }
            }
        }
    }
}

/// JSON progress reporter for machine consumption
pub struct JsonProgressReporter;

impl ProgressCallback for JsonProgressReporter {
    fn on_progress(&self, event: &ProgressEvent) {
        if let Ok(json) = serde_json::to_string(event) {
            println!("{}", json);
        }
    }
}

// ============================================================================
// Gas Town Context Compression Types
// ============================================================================

/// Mode of operation for gogo-gadget
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OperationMode {
    /// Standalone execution (normal CLI usage)
    #[default]
    Standalone,
    /// Running as a Claude Code subagent
    Subagent,
}

/// Configuration for subagent mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    /// The task assigned by the parent
    pub task: String,

    /// Parent's session ID for correlation
    pub parent_session_id: Option<String>,

    /// Maximum depth of recursive subagent spawning (safety limit)
    pub max_depth: u32,

    /// Current depth in the subagent tree
    pub current_depth: u32,

    /// Context policy for compression
    pub context_policy: ContextPolicy,

    /// Output format for the compressed result
    pub output_format: SubagentOutputFormat,
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            task: String::new(),
            parent_session_id: None,
            max_depth: 5, // Safety limit: no more than 5 levels deep
            current_depth: 0,
            context_policy: ContextPolicy::default(),
            output_format: SubagentOutputFormat::default(),
        }
    }
}

/// Policy for context compression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPolicy {
    /// Hide failed iterations from parent
    pub hide_failures: bool,

    /// Hide intermediate states
    pub hide_intermediates: bool,

    /// Include commit information if git is available
    pub include_commit: bool,

    /// Maximum summary length in characters
    pub max_summary_length: usize,

    /// Include file diffs in output
    pub include_diffs: bool,
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self {
            hide_failures: true,
            hide_intermediates: true,
            include_commit: true,
            max_summary_length: 500,
            include_diffs: false,
        }
    }
}

/// Output format for subagent results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SubagentOutputFormat {
    /// JSON output (default for programmatic consumption)
    #[default]
    Json,
    /// Minimal text output
    Text,
    /// Structured markdown
    Markdown,
}

/// Compressed result returned by a subagent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedResult {
    /// Whether the task succeeded
    pub success: bool,

    /// Git commit hash (if applicable)
    pub commit_hash: Option<String>,

    /// Files that were changed
    pub files_changed: Vec<PathBuf>,

    /// Brief summary of what was accomplished
    pub summary: String,

    /// Duration of execution in milliseconds
    pub duration_ms: u64,

    /// Number of iterations (without details)
    pub iterations: u32,

    /// Error message if failed (only if success=false)
    pub error: Option<String>,

    /// Verification status
    pub verified: bool,

    /// Subagent metadata for debugging
    pub metadata: CompressedResultMetadata,
}

/// Metadata for compressed results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedResultMetadata {
    /// GoGoGadget version
    pub version: String,

    /// Subagent depth level
    pub depth: u32,

    /// Parent session ID if applicable
    pub parent_session_id: Option<String>,

    /// Timestamp of completion
    pub completed_at: u64,

    /// Model used for execution
    pub model: Option<String>,
}

/// Subagent registration for Claude Code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentRegistration {
    /// Name of the subagent
    pub name: String,

    /// Description for Claude Code
    pub description: String,

    /// Command to invoke the subagent
    pub command: String,

    /// Trigger patterns (when to suggest this subagent)
    pub triggers: Vec<String>,

    /// Capabilities this subagent provides
    pub capabilities: Vec<String>,

    /// Context policy advertisement
    pub context_policy: ContextPolicy,

    /// Version of the subagent
    pub version: String,
}

impl Default for SubagentRegistration {
    fn default() -> Self {
        Self {
            name: "gogo-gadget".to_string(),
            description: "Autonomous AI agent with verified completion and context compression. \
                          Iterates until tasks are genuinely complete, then returns compressed results."
                .to_string(),
            command: "gogo-gadget --subagent-mode".to_string(),
            triggers: vec![
                "iterate until done".to_string(),
                "run until complete".to_string(),
                "verify completion".to_string(),
                "autonomous task".to_string(),
                "ralph wiggum".to_string(),
                "gogo-gadget".to_string(),
            ],
            capabilities: vec![
                "iterative_execution".to_string(),
                "verified_completion".to_string(),
                "context_compression".to_string(),
                "anti_laziness".to_string(),
                "recursive_subagents".to_string(),
            ],
            context_policy: ContextPolicy::default(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

// Re-export subagent types
pub use subagent::{
    CapabilityRegistrationResult, DetectedGap, RegistrationScope, SubagentExecutor,
};

// Re-export extend module types for self-extension capability
pub use extend::{
    CapabilityRegistry, CapabilityType, CapabilityVerifier, CreativeOverseer, HotLoader, LoadResult,
    OverseerConfig, SynthesizedCapability, SynthesisEngine, VerificationResult as ExtendVerificationResult,
};

/// Enhanced checkpoint with more metadata
impl Checkpoint {
    /// Create checkpoint with working directory context
    pub fn new_with_context(task: &str, working_dir: &Path) -> Self {
        let mut checkpoint = Self::new(task);
        checkpoint
            .metadata
            .insert("working_dir".to_string(), working_dir.display().to_string());
        checkpoint
    }

    /// Add a key-value pair to metadata
    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }

    /// Get a metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// Update checkpoint from current iteration state
    pub fn update_from_iteration(
        &mut self,
        iteration: u32,
        output: &str,
        feedback: &str,
        attempt: IterationAttempt,
    ) {
        self.iteration = iteration;
        self.last_output = output.to_string();
        self.accumulated_feedback = feedback.to_string();
        self.iteration_history.push(attempt);
    }

    /// Calculate progress percentage
    pub fn progress_percent(&self, max_iterations: u32) -> f32 {
        if max_iterations == 0 {
            return 0.0;
        }
        (self.iteration as f32 / max_iterations as f32) * 100.0
    }

    /// Get last successful iteration
    pub fn last_successful_iteration(&self) -> Option<&IterationAttempt> {
        self.iteration_history.iter().rev().find(|a| a.success)
    }

    /// Get failure count
    pub fn failure_count(&self) -> usize {
        self.iteration_history.iter().filter(|a| !a.success).count()
    }

    /// Check if checkpoint is stale (older than given duration)
    pub fn is_stale(&self, max_age: Duration) -> bool {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now.saturating_sub(self.created_at) > max_age.as_secs()
    }

    /// Create a recovery point with summary
    pub fn create_recovery_summary(&self) -> String {
        let success_count = self.iteration_history.iter().filter(|a| a.success).count();
        let fail_count = self.failure_count();

        format!(
            "Checkpoint Recovery Summary:\n\
             - Task: {}\n\
             - Iteration: {}\n\
             - State: {:?}\n\
             - Successful iterations: {}\n\
             - Failed iterations: {}\n\
             - Created: {} seconds ago",
            self.task,
            self.iteration,
            self.state,
            success_count,
            fail_count,
            {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                now.saturating_sub(self.created_at)
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_default() {
        assert_eq!(TaskState::default(), TaskState::Pending);
    }

    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = Checkpoint::new("Test task");
        assert_eq!(checkpoint.task, "Test task");
        assert_eq!(checkpoint.iteration, 0);
        assert_eq!(checkpoint.state, TaskState::Pending);
        assert!(!checkpoint.id.is_empty());
    }

    #[test]
    fn test_execution_metrics() {
        let mut metrics = ExecutionMetrics::new();
        metrics.record_iteration(1, 1000, true);
        metrics.record_iteration(2, 2000, false);
        metrics.record_retry();

        assert_eq!(metrics.successful_iterations, 1);
        assert_eq!(metrics.failed_iterations, 1);
        assert_eq!(metrics.retry_count, 1);
        assert_eq!(metrics.avg_iteration_ms, Some(1500));
    }

    #[test]
    fn test_backoff_config() {
        let config = BackoffConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 10000,
            multiplier: 2.0,
            max_retries: 5,
            jitter: false,
        };

        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(0));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(1000));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(2000));
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(4000));
        assert_eq!(config.delay_for_attempt(4), Duration::from_millis(8000));
        assert_eq!(config.delay_for_attempt(5), Duration::from_millis(10000)); // Capped at max
    }

    #[test]
    fn test_claude_structured_output_parsing() {
        let json = r#"{
            "status": "complete",
            "summary": "Task done",
            "files_modified": ["src/main.rs"],
            "next_steps": [],
            "errors": [],
            "confidence": 0.95
        }"#;

        let output: ClaudeStructuredOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.status, ClaudeOutputStatus::Complete);
        assert_eq!(output.summary, Some("Task done".to_string()));
        assert_eq!(output.confidence, 0.95);
    }
}
