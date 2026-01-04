//! Your Harness - Agentic Harness Marketplace
//!
//! The swarm module now powers "Your Harness" - a marketplace for agentic harnesses.
//! Users can shop for the best agentic harness based on:
//! - Budget constraints
//! - Task requirements
//! - Performance metrics
//!
//! Harness creators can monetize their systems through the platform.
//! Enterprise solutions available for scaled deployments.

mod coordinator;
mod decomposer;
mod executor;

pub use coordinator::SwarmCoordinator;
pub use decomposer::TaskDecomposer;
pub use executor::SwarmExecutor;

use crate::{AntiLazinessConfig, CompressedResult, ContextPolicy, SubagentOutputFormat, TaskResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// YOUR HARNESS MARKETPLACE TYPES
// ============================================================================

/// A harness available in the marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Harness {
    /// Unique identifier for the harness
    pub id: String,

    /// Display name of the harness
    pub name: String,

    /// Harness description and capabilities
    pub description: String,

    /// Creator information
    pub creator: HarnessCreator,

    /// Pricing information
    pub pricing: HarnessPricing,

    /// Supported task categories
    pub task_categories: Vec<TaskCategory>,

    /// Performance metrics and benchmarks
    pub metrics: HarnessMetrics,

    /// Configuration schema for the harness
    pub config_schema: Option<String>,

    /// Version of the harness
    pub version: String,

    /// Whether this harness supports enterprise features
    pub enterprise_ready: bool,
}

/// Creator information for a harness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessCreator {
    /// Creator's unique ID
    pub id: String,

    /// Display name
    pub name: String,

    /// Verification status
    pub verified: bool,

    /// Total harnesses published
    pub harness_count: u32,

    /// Average rating across all harnesses
    pub avg_rating: f32,

    /// Revenue share percentage (creator keeps this %)
    pub revenue_share_percent: f32,
}

/// Pricing model for a harness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessPricing {
    /// Pricing model type
    pub model: PricingModel,

    /// Base price in cents (USD)
    pub base_price_cents: u64,

    /// Price per execution (if usage-based)
    pub per_execution_cents: Option<u64>,

    /// Monthly subscription price (if subscription)
    pub monthly_cents: Option<u64>,

    /// Free tier executions per month
    pub free_tier_executions: u32,

    /// Enterprise custom pricing available
    pub enterprise_custom: bool,
}

/// Pricing model types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PricingModel {
    /// Free to use
    Free,
    /// Pay per execution
    PayPerUse,
    /// Monthly subscription
    Subscription,
    /// One-time purchase
    OneTime,
    /// Enterprise custom pricing
    Enterprise,
}

/// Task categories a harness can handle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum TaskCategory {
    /// Code generation and development
    CodeGeneration,
    /// Data analysis and processing
    DataAnalysis,
    /// Content creation and writing
    ContentCreation,
    /// Research and information gathering
    Research,
    /// Automation and workflow
    Automation,
    /// Testing and QA
    Testing,
    /// DevOps and infrastructure
    DevOps,
    /// Design and UI/UX
    Design,
    /// Security and compliance
    Security,
    /// Custom/Other
    Custom,
}

/// Performance metrics for a harness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessMetrics {
    /// Average execution time in seconds
    pub avg_execution_secs: f64,

    /// Success rate (0.0 - 1.0)
    pub success_rate: f32,

    /// User rating (1-5 stars)
    pub rating: f32,

    /// Number of reviews
    pub review_count: u32,

    /// Total executions
    pub total_executions: u64,

    /// Cost efficiency score (output quality / cost)
    pub cost_efficiency: f32,

    /// Benchmark scores for different task types
    pub benchmarks: std::collections::HashMap<TaskCategory, f32>,
}

/// Budget constraints for harness selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConstraint {
    /// Maximum budget in cents (USD)
    pub max_budget_cents: u64,

    /// Maximum cost per execution
    pub max_per_execution_cents: Option<u64>,

    /// Preferred pricing model
    pub preferred_model: Option<PricingModel>,

    /// Whether to consider enterprise pricing
    pub consider_enterprise: bool,
}

impl Default for BudgetConstraint {
    fn default() -> Self {
        Self {
            max_budget_cents: 10000, // $100 default
            max_per_execution_cents: None,
            preferred_model: None,
            consider_enterprise: false,
        }
    }
}

/// Request to find optimal harness for a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessSearchRequest {
    /// The task to accomplish
    pub task_description: String,

    /// Required task categories
    pub required_categories: Vec<TaskCategory>,

    /// Budget constraints
    pub budget: BudgetConstraint,

    /// Minimum required success rate
    pub min_success_rate: Option<f32>,

    /// Minimum required rating
    pub min_rating: Option<f32>,

    /// Whether enterprise features are required
    pub enterprise_required: bool,

    /// Maximum acceptable execution time
    pub max_execution_secs: Option<f64>,
}

/// Result of harness search/recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessRecommendation {
    /// Recommended harness
    pub harness: Harness,

    /// Match score (0.0 - 1.0)
    pub match_score: f32,

    /// Estimated cost for the task
    pub estimated_cost_cents: u64,

    /// Explanation of why this harness was chosen
    pub reasoning: String,

    /// Alternative harnesses considered
    pub alternatives: Vec<HarnessAlternative>,
}

/// Alternative harness option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessAlternative {
    /// The harness
    pub harness: Harness,

    /// Match score
    pub match_score: f32,

    /// Why this wasn't the top choice
    pub tradeoff: String,
}

/// Harness execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessExecutionContext {
    /// The harness being executed
    pub harness_id: String,

    /// User's budget for this execution
    pub budget: BudgetConstraint,

    /// Task to execute
    pub task: String,

    /// Custom configuration overrides
    pub config_overrides: std::collections::HashMap<String, String>,

    /// Execution tracking ID
    pub tracking_id: String,

    /// Start timestamp
    pub started_at: u64,
}

/// Result of harness execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessExecutionResult {
    /// Execution tracking ID
    pub tracking_id: String,

    /// The harness that was executed
    pub harness_id: String,

    /// Whether execution succeeded
    pub success: bool,

    /// Actual cost in cents
    pub actual_cost_cents: u64,

    /// Execution duration in seconds
    pub duration_secs: f64,

    /// Task result
    pub result: TaskResult,

    /// Usage breakdown
    pub usage: HarnessUsage,
}

/// Usage breakdown for billing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessUsage {
    /// Number of agent executions
    pub agent_executions: u32,

    /// Total tokens used
    pub tokens_used: u64,

    /// API calls made
    pub api_calls: u32,

    /// Compute time in seconds
    pub compute_secs: f64,

    /// Creator revenue share in cents
    pub creator_revenue_cents: u64,

    /// Platform fee in cents
    pub platform_fee_cents: u64,
}

/// Enterprise solution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseSolution {
    /// Organization ID
    pub org_id: String,

    /// Custom harnesses available
    pub custom_harnesses: Vec<String>,

    /// Volume pricing tier
    pub pricing_tier: EnterpriseTier,

    /// Monthly execution allowance
    pub monthly_allowance: u64,

    /// Priority support enabled
    pub priority_support: bool,

    /// SLA guarantees
    pub sla: Option<EnterpriseSLA>,

    /// Private deployment option
    pub private_deployment: bool,
}

/// Enterprise pricing tiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnterpriseTier {
    /// Starter enterprise
    Starter,
    /// Professional
    Professional,
    /// Business
    Business,
    /// Enterprise custom
    Custom,
}

/// SLA configuration for enterprise
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseSLA {
    /// Uptime guarantee percentage
    pub uptime_percent: f32,

    /// Max response time in seconds
    pub max_response_secs: u32,

    /// Support response time in hours
    pub support_response_hours: u32,
}

// ============================================================================
// EXISTING SWARM TYPES (Updated for Your Harness)
// ============================================================================

/// Configuration for swarm execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmConfig {
    /// Number of parallel agents to spawn
    pub agent_count: u32,

    /// Timeout per agent in seconds (safety measure only)
    pub agent_timeout_secs: u64,

    /// Whether to share context between agents
    pub share_context: bool,

    /// Working directory
    pub working_dir: PathBuf,

    /// Model to use for agents
    pub model: Option<String>,

    /// Strategy for task decomposition
    pub decomposition_strategy: DecompositionStrategy,

    /// Anti-laziness enforcement configuration
    #[serde(default)]
    pub anti_laziness: AntiLazinessConfig,

    /// Subagent mode configuration for Gas Town context compression
    #[serde(default)]
    pub subagent_mode: SubagentModeConfig,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            agent_count: 3,
            agent_timeout_secs: 600, // 10 minute safety timeout
            share_context: true,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            model: None,
            decomposition_strategy: DecompositionStrategy::Auto,
            anti_laziness: AntiLazinessConfig::default(),
            subagent_mode: SubagentModeConfig::default(),
        }
    }
}

/// Configuration for subagent mode in swarm execution (Gas Town pattern)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentModeConfig {
    /// Whether agents should use subagent mode for spawning gogo-gadget
    pub enabled: bool,

    /// Maximum subagent recursion depth (safety limit)
    pub max_depth: u32,

    /// Context compression policy
    pub context_policy: ContextPolicy,

    /// Output format for compressed results
    pub output_format: SubagentOutputFormat,

    /// Parent session ID for correlation
    pub parent_session_id: Option<String>,
}

impl Default for SubagentModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_depth: 5,
            context_policy: ContextPolicy::default(),
            output_format: SubagentOutputFormat::Json,
            parent_session_id: None,
        }
    }
}

/// Strategy for decomposing tasks into subtasks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DecompositionStrategy {
    /// Automatically determine best strategy based on task analysis
    #[default]
    Auto,
    /// Decompose by files (each agent handles different files)
    ByFiles,
    /// Decompose by components (frontend, backend, tests, etc.)
    ByComponents,
    /// Run same task with different focus areas
    ByFocus,
    /// Custom decomposition provided by user
    Custom,
}

/// Result of a single agent's work in the swarm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    /// Agent identifier
    pub agent_id: u32,

    /// The subtask this agent worked on
    pub subtask: String,

    /// The result of the agent's work
    pub result: TaskResult,

    /// Files modified by this agent
    pub files_modified: Vec<PathBuf>,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Compressed result from subagent execution (if using Gas Town pattern)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_result: Option<CompressedResult>,
}

/// Overall result of swarm execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmResult {
    /// Overall success (all agents succeeded)
    pub success: bool,

    /// Results from each agent
    pub agent_results: Vec<AgentResult>,

    /// Combined summary of all work done
    pub summary: Option<String>,

    /// Any conflicts detected between agents
    pub conflicts: Vec<SwarmConflict>,

    /// Total execution time in milliseconds
    pub total_duration_ms: u64,

    /// Number of successful agents
    pub successful_agents: u32,

    /// Number of failed agents
    pub failed_agents: u32,

    /// Final verification result
    pub verification_passed: bool,

    /// Compressed result for parent context (Gas Town pattern)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_output: Option<CompressedResult>,
}

/// A conflict detected between agent outputs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmConflict {
    /// File with conflict
    pub file: PathBuf,

    /// Agents that modified the same file
    pub agent_ids: Vec<u32>,

    /// Description of the conflict
    pub description: String,

    /// Whether the conflict was resolved
    pub resolved: bool,
}

/// A subtask assigned to an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    /// Unique identifier for the subtask
    pub id: u32,

    /// The task description for this agent
    pub description: String,

    /// Focus area for this agent
    pub focus: String,

    /// Files this agent should primarily work on (if any)
    pub target_files: Vec<PathBuf>,

    /// Context from other agents to consider
    pub shared_context: Option<String>,

    /// Priority of this subtask (lower = higher priority)
    pub priority: u32,

    /// Whether this subtask should use subagent mode
    #[serde(default)]
    pub use_subagent_mode: bool,

    /// Subagent depth for this subtask (for Gas Town pattern)
    #[serde(default)]
    pub subagent_depth: u32,
}

/// Progress event specific to swarm execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmProgressEvent {
    /// Event type
    pub event_type: SwarmProgressType,

    /// Agent ID (if applicable)
    pub agent_id: Option<u32>,

    /// Total agents
    pub total_agents: u32,

    /// Completed agents
    pub completed_agents: u32,

    /// Optional message
    pub message: Option<String>,
}

/// Types of swarm progress events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwarmProgressType {
    /// Swarm execution started
    SwarmStarted,
    /// Task decomposition complete
    DecompositionComplete,
    /// Agent started
    AgentStarted,
    /// Agent completed
    AgentCompleted,
    /// Agent failed
    AgentFailed,
    /// Conflict detected
    ConflictDetected,
    /// Conflict resolved
    ConflictResolved,
    /// Aggregation started
    AggregationStarted,
    /// Verification started
    VerificationStarted,
    /// Swarm completed successfully
    SwarmComplete,
    /// Swarm failed
    SwarmFailed,
    /// Capability gap detected during swarm execution
    CapabilityGapDetected,
    /// Capability extension in progress
    CapabilityExtending,
    /// Capability successfully extended
    CapabilityExtended,
    /// Capability extension failed
    CapabilityExtensionFailed,
}

/// Result of capability extension attempt in swarm context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmExtensionResult {
    /// Whether the extension was successful
    pub success: bool,
    /// Name of the capability that was extended
    pub capability_name: Option<String>,
    /// Type of capability (Skill, MCP, Agent)
    pub capability_type: Option<String>,
    /// Error message if extension failed
    pub error: Option<String>,
    /// Number of extension attempts made
    pub attempts: u32,
}

/// Builder for SwarmConfig
pub struct SwarmConfigBuilder {
    config: SwarmConfig,
}

impl SwarmConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: SwarmConfig::default(),
        }
    }

    pub fn agent_count(mut self, count: u32) -> Self {
        self.config.agent_count = count;
        self
    }

    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.config.agent_timeout_secs = secs;
        self
    }

    pub fn share_context(mut self, share: bool) -> Self {
        self.config.share_context = share;
        self
    }

    pub fn working_dir(mut self, dir: PathBuf) -> Self {
        self.config.working_dir = dir;
        self
    }

    pub fn model(mut self, model: Option<String>) -> Self {
        self.config.model = model;
        self
    }

    pub fn strategy(mut self, strategy: DecompositionStrategy) -> Self {
        self.config.decomposition_strategy = strategy;
        self
    }

    pub fn anti_laziness(mut self, config: AntiLazinessConfig) -> Self {
        self.config.anti_laziness = config;
        self
    }

    pub fn subagent_mode(mut self, config: SubagentModeConfig) -> Self {
        self.config.subagent_mode = config;
        self
    }

    pub fn enable_subagent_mode(mut self, enabled: bool) -> Self {
        self.config.subagent_mode.enabled = enabled;
        self
    }

    pub fn subagent_max_depth(mut self, depth: u32) -> Self {
        self.config.subagent_mode.max_depth = depth;
        self
    }

    pub fn context_policy(mut self, policy: ContextPolicy) -> Self {
        self.config.subagent_mode.context_policy = policy;
        self
    }

    pub fn build(self) -> SwarmConfig {
        self.config
    }
}

impl Default for SwarmConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
