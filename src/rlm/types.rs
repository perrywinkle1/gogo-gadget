//! Core types for the RLM module

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for RLM execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmConfig {
    /// Maximum recursion depth (default: 5)
    pub max_depth: u32,

    /// Maximum chunks to consider per level (default: 10)
    pub max_chunks_per_level: usize,

    /// Maximum total tokens to process (default: 100_000)
    pub max_total_tokens: u32,

    /// Minimum relevance score to explore a chunk (default: 0.3)
    pub min_relevance: f32,

    /// Target chunk size in approximate tokens (default: 4000)
    pub target_chunk_size: usize,

    /// Overlap between chunks in characters (default: 200)
    pub chunk_overlap: usize,

    /// Enable parallel chunk exploration (default: true)
    pub parallel_exploration: bool,

    /// Model for navigation decisions
    pub navigator_model: Option<String>,

    /// Model for chunk exploration
    pub explorer_model: Option<String>,

    /// Enable embedding-based pre-scoring
    pub use_embeddings: bool,

    /// Query rate limit per minute for continuous mode
    pub rate_limit_per_minute: u32,
}

impl Default for RlmConfig {
    fn default() -> Self {
        Self {
            max_depth: 5,
            max_chunks_per_level: 10,
            max_total_tokens: 100_000,
            min_relevance: 0.3,
            target_chunk_size: 4000,
            chunk_overlap: 200,
            parallel_exploration: true,
            navigator_model: Some("claude-haiku-4-5-20251101".to_string()),
            explorer_model: Some("claude-opus-4-5-20251101".to_string()),
            use_embeddings: false, // Use heuristic scoring, no OpenAI needed
            rate_limit_per_minute: 10,
        }
    }
}

/// A chunk of content from the context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Unique identifier for this chunk
    pub id: String,

    /// The actual content
    pub content: String,

    /// Source file path (if applicable)
    pub source_path: Option<PathBuf>,

    /// Byte range in the source file
    pub byte_range: Option<(usize, usize)>,

    /// Line range in the source file
    pub line_range: Option<(usize, usize)>,

    /// Approximate token count
    pub token_count: u32,

    /// Relevance score (0.0 to 1.0, set by navigator)
    pub relevance_score: f32,

    /// Semantic hints about the chunk content
    pub hints: Vec<String>,

    /// Parent chunk ID (if this is a sub-chunk)
    pub parent_id: Option<String>,

    /// Child chunk IDs (if decomposed)
    pub children: Vec<String>,

    /// Metadata
    pub metadata: ChunkMetadata,
}

impl Chunk {
    /// Create a new chunk with the given content
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let token_count = estimate_tokens(&content);
        Self {
            id: id.into(),
            content,
            source_path: None,
            byte_range: None,
            line_range: None,
            token_count,
            relevance_score: 0.0,
            hints: Vec::new(),
            parent_id: None,
            children: Vec::new(),
            metadata: ChunkMetadata::default(),
        }
    }

    /// Set the source path
    pub fn with_source(mut self, path: PathBuf) -> Self {
        self.source_path = Some(path);
        self
    }

    /// Set byte range
    pub fn with_byte_range(mut self, start: usize, end: usize) -> Self {
        self.byte_range = Some((start, end));
        self
    }

    /// Set line range
    pub fn with_line_range(mut self, start: usize, end: usize) -> Self {
        self.line_range = Some((start, end));
        self
    }

    /// Add hints
    pub fn with_hints(mut self, hints: Vec<String>) -> Self {
        self.hints = hints;
        self
    }

    /// Get a preview of the content (first N chars)
    pub fn preview(&self, max_len: usize) -> &str {
        if self.content.len() <= max_len {
            &self.content
        } else {
            // Find a good break point
            let mut end = max_len;
            while end > 0 && !self.content.is_char_boundary(end) {
                end -= 1;
            }
            &self.content[..end]
        }
    }
}

/// Metadata for a chunk
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// Detected programming language
    pub language: Option<String>,

    /// Whether this chunk contains code
    pub is_code: bool,

    /// Whether this is a structural boundary (function, class, etc.)
    pub is_structural_boundary: bool,

    /// Structural type if applicable
    pub structural_type: Option<StructuralType>,

    /// Creation timestamp
    pub created_at: u64,
}

/// Types of structural boundaries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructuralType {
    Function,
    Class,
    Module,
    Struct,
    Enum,
    Trait,
    Impl,
    Interface,
    Method,
    File,
    Section,
    Paragraph,
}

/// Result from exploring a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkResult {
    /// The chunk that was explored
    pub chunk_id: String,

    /// Whether exploration succeeded
    pub success: bool,

    /// Findings from this chunk
    pub findings: Vec<Finding>,

    /// Sub-results if chunk was decomposed
    pub sub_results: Vec<ChunkResult>,

    /// Depth at which this chunk was explored
    pub depth: u32,

    /// Tokens used for this exploration
    pub tokens_used: u32,

    /// Error if exploration failed
    pub error: Option<String>,
}

/// A finding from chunk exploration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// The finding content
    pub content: String,

    /// Relevance to the query (0.0 to 1.0)
    pub relevance: f32,

    /// Source chunk ID
    pub source_chunk_id: String,

    /// Source file path
    pub source_path: Option<PathBuf>,

    /// Line range if applicable
    pub line_range: Option<(usize, usize)>,

    /// Type of finding
    pub finding_type: FindingType,

    /// Confidence in this finding (0.0 to 1.0)
    pub confidence: f32,
}

/// Types of findings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingType {
    /// Direct answer to the query
    Answer,
    /// Supporting evidence
    Evidence,
    /// Related but not direct
    Related,
    /// Contradiction or conflict
    Conflict,
    /// Additional context
    Context,
}

/// Final result from RLM processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmResult {
    /// Whether processing succeeded
    pub success: bool,

    /// The synthesized answer
    pub answer: String,

    /// Key insights extracted
    pub key_insights: Vec<String>,

    /// Evidence supporting the answer
    pub evidence: Vec<Evidence>,

    /// Number of chunks explored
    pub chunks_explored: usize,

    /// Maximum depth reached
    pub max_depth_reached: u32,

    /// Total tokens processed
    pub total_tokens: u32,

    /// Confidence in the result (0.0 to 1.0)
    pub confidence: f32,

    /// Processing duration in milliseconds
    pub duration_ms: u64,

    /// Error if processing failed
    pub error: Option<String>,
}

/// Evidence citation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// The evidence content
    pub content: String,

    /// Source file path
    pub source_path: PathBuf,

    /// Line range
    pub line_range: Option<(usize, usize)>,

    /// Relevance to the answer
    pub relevance: f32,
}

/// Estimate token count from text (approximate: chars / 4)
pub fn estimate_tokens(text: &str) -> u32 {
    // Approximate: 1 token ≈ 4 characters for English
    // This is faster than tiktoken and good enough for budgeting
    (text.len() / 4) as u32
}

// ============================================================================
// PreToolUse Hook System
// ============================================================================

/// Event types for PreToolUse hooks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RlmToolEvent {
    /// Navigation decision being made
    NavigationStart {
        query: String,
        chunk_count: usize,
        depth: u32,
    },
    /// Chunk exploration starting
    ChunkExploreStart { chunk_id: String, query: String },
    /// Cache lookup
    CacheLookup {
        cache_type: String,
        key: String,
        hit: bool,
    },
    /// Aggregation starting
    AggregationStart {
        finding_count: usize,
        strategy: String,
    },
    /// LLM call being made
    LlmCall {
        model: String,
        purpose: String,
        token_estimate: u32,
    },
    /// Daemon query received
    DaemonQueryReceived { query_id: String, query: String },
    /// Creative pipeline stage execution
    CreativePipelineStage {
        stage: PipelineStage,
        agent_name: String,
        domain: CreativeDomain,
    },
}

/// Result of a hook invocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// Whether to proceed with the tool use
    pub proceed: bool,
    /// Optional message from the hook
    pub message: Option<String>,
    /// Modified parameters (if hook transformed input)
    pub modified_params: Option<serde_json::Value>,
}

impl Default for HookResult {
    fn default() -> Self {
        Self {
            proceed: true,
            message: None,
            modified_params: None,
        }
    }
}

/// Trait for PreToolUse hooks
pub trait PreToolUseHook: Send + Sync {
    /// Called before a tool is used
    fn on_pre_tool_use(&self, event: &RlmToolEvent) -> HookResult;

    /// Hook name for logging
    fn name(&self) -> &str;
}

/// A logging hook that records all tool events
pub struct LoggingHook {
    name: String,
    log_level: LogLevel,
}

/// Log level for the logging hook
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LogLevel {
    /// Debug level (most verbose)
    Debug,
    /// Info level (default)
    #[default]
    Info,
    /// Warn level
    Warn,
    /// Error level (least verbose)
    Error,
}

impl LoggingHook {
    /// Create a new logging hook
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            log_level: LogLevel::Info,
        }
    }

    /// Set the log level
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.log_level = level;
        self
    }
}

impl PreToolUseHook for LoggingHook {
    fn on_pre_tool_use(&self, event: &RlmToolEvent) -> HookResult {
        match self.log_level {
            LogLevel::Debug => {
                tracing::debug!(hook = %self.name, event = ?event, "PreToolUse hook triggered");
            }
            LogLevel::Info => {
                tracing::info!(hook = %self.name, event = ?event, "PreToolUse hook triggered");
            }
            LogLevel::Warn => {
                tracing::warn!(hook = %self.name, event = ?event, "PreToolUse hook triggered");
            }
            LogLevel::Error => {
                tracing::error!(hook = %self.name, event = ?event, "PreToolUse hook triggered");
            }
        }
        HookResult::default()
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Registry for managing PreToolUse hooks
#[derive(Default)]
pub struct HookRegistry {
    hooks: Vec<Arc<dyn PreToolUseHook>>,
}

impl HookRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Register a hook
    pub fn register(&mut self, hook: Arc<dyn PreToolUseHook>) {
        tracing::debug!(hook_name = %hook.name(), "Registering PreToolUse hook");
        self.hooks.push(hook);
    }

    /// Invoke all hooks for an event
    pub fn invoke(&self, event: &RlmToolEvent) -> HookResult {
        for hook in &self.hooks {
            let result = hook.on_pre_tool_use(event);
            if !result.proceed {
                tracing::warn!(
                    hook_name = %hook.name(),
                    message = ?result.message,
                    "Hook blocked tool use"
                );
                return result;
            }
        }
        HookResult::default()
    }

    /// Check if any hooks are registered
    pub fn has_hooks(&self) -> bool {
        !self.hooks.is_empty()
    }

    /// Get the number of registered hooks
    pub fn hook_count(&self) -> usize {
        self.hooks.len()
    }

    /// Clear all hooks
    pub fn clear(&mut self) {
        self.hooks.clear();
    }
}

/// Configuration for hook behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Whether hooks are enabled
    pub enabled: bool,
    /// Log level for built-in logging hook
    pub log_level: LogLevel,
    /// Whether to include timing information
    pub include_timing: bool,
    /// Whether to log cache hits/misses
    pub log_cache_events: bool,
    /// Whether to log LLM calls
    pub log_llm_calls: bool,
    /// Whether to log navigation decisions
    pub log_navigation: bool,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            log_level: LogLevel::Info,
            include_timing: true,
            log_cache_events: true,
            log_llm_calls: true,
            log_navigation: true,
        }
    }
}

// ============================================================================
// Capability Integration Types
// ============================================================================

/// Types of capabilities that can be integrated with RLM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RlmCapabilityType {
    /// MCP server for external API integration
    McpServer {
        name: String,
        api_type: String,
        description: String,
    },
    /// Skill for reusable patterns (e.g., migrations)
    Skill {
        name: String,
        trigger: String,
        purpose: String,
    },
    /// Agent for specialized behaviors (e.g., security review)
    Agent {
        name: String,
        specialization: String,
    },
    /// Creative pipeline agent for content generation workflows
    CreativePipelineAgent {
        name: String,
        /// The creative domain (video, image, audio, text, 3d)
        domain: CreativeDomain,
        /// Pipeline stage this agent handles
        stage: PipelineStage,
        /// Required tools/MCPs for this agent
        required_tools: Vec<String>,
        /// Description of capabilities
        description: String,
    },
}

/// Creative domains supported by pipeline agents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreativeDomain {
    /// Video generation and editing (Veo, ffmpeg)
    Video,
    /// Image generation and manipulation (Gemini, DALL-E)
    Image,
    /// Audio generation and processing
    Audio,
    /// Text and document generation
    Text,
    /// 3D model and scene generation (Three.js)
    ThreeD,
    /// Multi-modal combining multiple domains
    MultiModal,
}

/// Stages in a creative pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineStage {
    /// Initial ideation and concept generation
    Ideation,
    /// Prompt engineering and refinement
    PromptEngineering,
    /// Content generation (calling AI APIs)
    Generation,
    /// Post-processing and enhancement
    PostProcessing,
    /// Quality review and iteration
    QualityReview,
    /// Final assembly and export
    Assembly,
    /// Distribution and publishing
    Distribution,
}

impl RlmCapabilityType {
    /// Create a new MCP server capability
    pub fn mcp_server(
        name: impl Into<String>,
        api_type: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        RlmCapabilityType::McpServer {
            name: name.into(),
            api_type: api_type.into(),
            description: description.into(),
        }
    }

    /// Create a new Skill capability
    pub fn skill(
        name: impl Into<String>,
        trigger: impl Into<String>,
        purpose: impl Into<String>,
    ) -> Self {
        RlmCapabilityType::Skill {
            name: name.into(),
            trigger: trigger.into(),
            purpose: purpose.into(),
        }
    }

    /// Create a new Agent capability
    pub fn agent(name: impl Into<String>, specialization: impl Into<String>) -> Self {
        RlmCapabilityType::Agent {
            name: name.into(),
            specialization: specialization.into(),
        }
    }

    /// Create a new Creative Pipeline Agent capability
    pub fn creative_agent(
        name: impl Into<String>,
        domain: CreativeDomain,
        stage: PipelineStage,
        required_tools: Vec<String>,
        description: impl Into<String>,
    ) -> Self {
        RlmCapabilityType::CreativePipelineAgent {
            name: name.into(),
            domain,
            stage,
            required_tools,
            description: description.into(),
        }
    }

    /// Get the capability name
    pub fn name(&self) -> &str {
        match self {
            RlmCapabilityType::McpServer { name, .. } => name,
            RlmCapabilityType::Skill { name, .. } => name,
            RlmCapabilityType::Agent { name, .. } => name,
            RlmCapabilityType::CreativePipelineAgent { name, .. } => name,
        }
    }

    /// Check if this is a creative pipeline capability
    pub fn is_creative_pipeline(&self) -> bool {
        matches!(self, RlmCapabilityType::CreativePipelineAgent { .. })
    }

    /// Get the creative domain if this is a creative pipeline agent
    pub fn creative_domain(&self) -> Option<CreativeDomain> {
        match self {
            RlmCapabilityType::CreativePipelineAgent { domain, .. } => Some(*domain),
            _ => None,
        }
    }

    /// Get the pipeline stage if this is a creative pipeline agent
    pub fn pipeline_stage(&self) -> Option<PipelineStage> {
        match self {
            RlmCapabilityType::CreativePipelineAgent { stage, .. } => Some(*stage),
            _ => None,
        }
    }
}

impl CreativeDomain {
    /// Get the display name for this domain
    pub fn display_name(&self) -> &'static str {
        match self {
            CreativeDomain::Video => "Video",
            CreativeDomain::Image => "Image",
            CreativeDomain::Audio => "Audio",
            CreativeDomain::Text => "Text",
            CreativeDomain::ThreeD => "3D",
            CreativeDomain::MultiModal => "Multi-Modal",
        }
    }

    /// Get recommended MCP tools for this domain
    pub fn recommended_tools(&self) -> Vec<&'static str> {
        match self {
            CreativeDomain::Video => vec!["veo", "ffmpeg", "gemini"],
            CreativeDomain::Image => vec!["gemini", "dall-e", "stable-diffusion"],
            CreativeDomain::Audio => vec!["elevenlabs", "speechify", "ffmpeg"],
            CreativeDomain::Text => vec!["anthropic", "gemini", "openai"],
            CreativeDomain::ThreeD => vec!["threejs-dev", "blender-mcp"],
            CreativeDomain::MultiModal => vec!["gemini", "veo", "anthropic"],
        }
    }
}

impl PipelineStage {
    /// Get the display name for this stage
    pub fn display_name(&self) -> &'static str {
        match self {
            PipelineStage::Ideation => "Ideation",
            PipelineStage::PromptEngineering => "Prompt Engineering",
            PipelineStage::Generation => "Generation",
            PipelineStage::PostProcessing => "Post-Processing",
            PipelineStage::QualityReview => "Quality Review",
            PipelineStage::Assembly => "Assembly",
            PipelineStage::Distribution => "Distribution",
        }
    }

    /// Get the typical next stage in the pipeline
    pub fn next_stage(&self) -> Option<PipelineStage> {
        match self {
            PipelineStage::Ideation => Some(PipelineStage::PromptEngineering),
            PipelineStage::PromptEngineering => Some(PipelineStage::Generation),
            PipelineStage::Generation => Some(PipelineStage::PostProcessing),
            PipelineStage::PostProcessing => Some(PipelineStage::QualityReview),
            PipelineStage::QualityReview => Some(PipelineStage::Assembly),
            PipelineStage::Assembly => Some(PipelineStage::Distribution),
            PipelineStage::Distribution => None,
        }
    }

    /// Check if this is a terminal stage
    pub fn is_terminal(&self) -> bool {
        matches!(self, PipelineStage::Distribution)
    }
}

/// Registry of capabilities available to RLM
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RlmCapabilityRegistry {
    /// Registered capabilities
    pub capabilities: Vec<RlmCapabilityType>,
}

impl RlmCapabilityRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            capabilities: Vec::new(),
        }
    }

    /// Register a capability
    pub fn register(&mut self, capability: RlmCapabilityType) {
        tracing::info!(capability_name = %capability.name(), "Registering RLM capability");
        self.capabilities.push(capability);
    }

    /// Get all MCP servers
    pub fn mcp_servers(&self) -> impl Iterator<Item = &RlmCapabilityType> {
        self.capabilities
            .iter()
            .filter(|c| matches!(c, RlmCapabilityType::McpServer { .. }))
    }

    /// Get all skills
    pub fn skills(&self) -> impl Iterator<Item = &RlmCapabilityType> {
        self.capabilities
            .iter()
            .filter(|c| matches!(c, RlmCapabilityType::Skill { .. }))
    }

    /// Get all agents
    pub fn agents(&self) -> impl Iterator<Item = &RlmCapabilityType> {
        self.capabilities
            .iter()
            .filter(|c| matches!(c, RlmCapabilityType::Agent { .. }))
    }

    /// Get all creative pipeline agents
    pub fn creative_pipeline_agents(&self) -> impl Iterator<Item = &RlmCapabilityType> {
        self.capabilities
            .iter()
            .filter(|c| matches!(c, RlmCapabilityType::CreativePipelineAgent { .. }))
    }

    /// Get creative pipeline agents by domain
    pub fn agents_for_domain(&self, domain: CreativeDomain) -> Vec<&RlmCapabilityType> {
        self.capabilities
            .iter()
            .filter(|c| c.creative_domain() == Some(domain))
            .collect()
    }

    /// Get creative pipeline agents by stage
    pub fn agents_for_stage(&self, stage: PipelineStage) -> Vec<&RlmCapabilityType> {
        self.capabilities
            .iter()
            .filter(|c| c.pipeline_stage() == Some(stage))
            .collect()
    }

    /// Check if a capability exists by name
    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.iter().any(|c| c.name() == name)
    }

    /// Create a registry with common capabilities pre-registered
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Register common MCP servers
        registry.register(RlmCapabilityType::mcp_server(
            "external-api-mcp",
            "rest",
            "MCP server for external REST API integration",
        ));

        // Register common skills
        registry.register(RlmCapabilityType::skill(
            "schema-migration",
            "migrate",
            "Skill for database schema migrations",
        ));

        // Register common agents
        registry.register(RlmCapabilityType::agent(
            "security-reviewer",
            "Security code review and vulnerability analysis",
        ));

        registry
    }

    /// Create a registry with creative pipeline agents pre-registered
    pub fn with_creative_pipeline_defaults() -> Self {
        let mut registry = Self::with_defaults();

        // Video generation agents
        registry.register(RlmCapabilityType::creative_agent(
            "video-ideation-agent",
            CreativeDomain::Video,
            PipelineStage::Ideation,
            vec!["anthropic".to_string(), "gemini".to_string()],
            "Generates video concepts, storyboards, and creative briefs",
        ));
        registry.register(RlmCapabilityType::creative_agent(
            "video-prompt-engineer",
            CreativeDomain::Video,
            PipelineStage::PromptEngineering,
            vec!["anthropic".to_string()],
            "Crafts optimized prompts for Veo and video generation models",
        ));
        registry.register(RlmCapabilityType::creative_agent(
            "video-generator",
            CreativeDomain::Video,
            PipelineStage::Generation,
            vec!["veo".to_string()],
            "Generates videos using Veo 3.1 API",
        ));
        registry.register(RlmCapabilityType::creative_agent(
            "video-post-processor",
            CreativeDomain::Video,
            PipelineStage::PostProcessing,
            vec!["ffmpeg".to_string()],
            "Handles video encoding, trimming, effects, and format conversion",
        ));

        // Image generation agents
        registry.register(RlmCapabilityType::creative_agent(
            "image-ideation-agent",
            CreativeDomain::Image,
            PipelineStage::Ideation,
            vec!["anthropic".to_string()],
            "Generates image concepts and visual direction",
        ));
        registry.register(RlmCapabilityType::creative_agent(
            "image-generator",
            CreativeDomain::Image,
            PipelineStage::Generation,
            vec!["gemini".to_string()],
            "Generates images using Gemini or DALL-E",
        ));

        // 3D generation agents
        registry.register(RlmCapabilityType::creative_agent(
            "threejs-scene-agent",
            CreativeDomain::ThreeD,
            PipelineStage::Generation,
            vec!["threejs-dev".to_string()],
            "Creates Three.js 3D scenes, models, and animations",
        ));

        // Multi-modal orchestration agent
        registry.register(RlmCapabilityType::creative_agent(
            "creative-orchestrator",
            CreativeDomain::MultiModal,
            PipelineStage::Assembly,
            vec!["anthropic".to_string(), "veo".to_string(), "gemini".to_string()],
            "Orchestrates multi-modal creative workflows across domains",
        ));

        // Quality review agent
        registry.register(RlmCapabilityType::creative_agent(
            "creative-qa-agent",
            CreativeDomain::MultiModal,
            PipelineStage::QualityReview,
            vec!["anthropic".to_string(), "gemini".to_string()],
            "Reviews generated content for quality, coherence, and brand alignment",
        ));

        registry
    }

    /// Get a pipeline of agents for a specific domain (ordered by stage)
    pub fn get_pipeline_for_domain(&self, domain: CreativeDomain) -> Vec<&RlmCapabilityType> {
        let stages = [
            PipelineStage::Ideation,
            PipelineStage::PromptEngineering,
            PipelineStage::Generation,
            PipelineStage::PostProcessing,
            PipelineStage::QualityReview,
            PipelineStage::Assembly,
            PipelineStage::Distribution,
        ];

        let mut pipeline = Vec::new();
        for stage in stages {
            let agents: Vec<_> = self.capabilities
                .iter()
                .filter(|c| c.creative_domain() == Some(domain) && c.pipeline_stage() == Some(stage))
                .collect();
            pipeline.extend(agents);
        }
        pipeline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== RlmConfig Tests ====================

    #[test]
    fn test_rlm_config_default() {
        let config = RlmConfig::default();
        assert_eq!(config.max_depth, 5);
        assert_eq!(config.max_chunks_per_level, 10);
        assert_eq!(config.max_total_tokens, 100_000);
        assert!((config.min_relevance - 0.3).abs() < f32::EPSILON);
        assert_eq!(config.target_chunk_size, 4000);
        assert_eq!(config.chunk_overlap, 200);
        assert!(config.parallel_exploration);
        assert_eq!(
            config.navigator_model,
            Some("claude-haiku-4-5-20251101".to_string())
        );
        assert_eq!(
            config.explorer_model,
            Some("claude-opus-4-5-20251101".to_string())
        );
        assert!(!config.use_embeddings);
        assert_eq!(config.rate_limit_per_minute, 10);
    }

    #[test]
    fn test_rlm_config_serialization() {
        let config = RlmConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.max_depth, deserialized.max_depth);
        assert_eq!(
            config.max_chunks_per_level,
            deserialized.max_chunks_per_level
        );
    }

    #[test]
    fn test_rlm_config_custom() {
        let config = RlmConfig {
            max_depth: 10,
            max_chunks_per_level: 20,
            max_total_tokens: 200_000,
            min_relevance: 0.5,
            target_chunk_size: 8000,
            chunk_overlap: 400,
            parallel_exploration: false,
            navigator_model: Some("custom-model".to_string()),
            explorer_model: Some("explorer-model".to_string()),
            use_embeddings: false,
            rate_limit_per_minute: 20,
        };
        assert_eq!(config.max_depth, 10);
        assert!(!config.parallel_exploration);
        assert!(!config.use_embeddings);
    }

    // ==================== Chunk Tests ====================

    #[test]
    fn test_chunk_creation() {
        let chunk = Chunk::new("chunk-1", "fn main() { println!(\"hello\"); }");
        assert_eq!(chunk.id, "chunk-1");
        assert!(chunk.token_count > 0);
        assert_eq!(chunk.relevance_score, 0.0);
        assert!(chunk.hints.is_empty());
        assert!(chunk.parent_id.is_none());
        assert!(chunk.children.is_empty());
    }

    #[test]
    fn test_chunk_with_source() {
        let chunk = Chunk::new("test", "content")
            .with_source(PathBuf::from("src/main.rs"))
            .with_line_range(10, 20);
        assert_eq!(chunk.source_path, Some(PathBuf::from("src/main.rs")));
        assert_eq!(chunk.line_range, Some((10, 20)));
    }

    #[test]
    fn test_chunk_with_byte_range() {
        let chunk = Chunk::new("test", "content").with_byte_range(100, 200);
        assert_eq!(chunk.byte_range, Some((100, 200)));
    }

    #[test]
    fn test_chunk_with_hints() {
        let hints = vec!["function".to_string(), "authentication".to_string()];
        let chunk = Chunk::new("test", "content").with_hints(hints.clone());
        assert_eq!(chunk.hints, hints);
    }

    #[test]
    fn test_chunk_preview() {
        let content = "This is a long piece of content that should be truncated";
        let chunk = Chunk::new("test", content);
        let preview = chunk.preview(20);
        assert!(preview.len() <= 20);
        assert_eq!(preview, "This is a long piece");
    }

    #[test]
    fn test_chunk_preview_short_content() {
        let content = "Short";
        let chunk = Chunk::new("test", content);
        let preview = chunk.preview(100);
        assert_eq!(preview, content);
    }

    #[test]
    fn test_chunk_preview_unicode() {
        let content = "Unicode: 日本語テスト with more text";
        let chunk = Chunk::new("test", content);
        // Should not panic on unicode boundary
        let preview = chunk.preview(15);
        assert!(preview.len() <= 15);
    }

    #[test]
    fn test_chunk_builder_chain() {
        let chunk = Chunk::new("id-1", "function code here")
            .with_source(PathBuf::from("lib.rs"))
            .with_byte_range(0, 100)
            .with_line_range(1, 10)
            .with_hints(vec!["utility".to_string()]);

        assert_eq!(chunk.id, "id-1");
        assert_eq!(chunk.source_path, Some(PathBuf::from("lib.rs")));
        assert_eq!(chunk.byte_range, Some((0, 100)));
        assert_eq!(chunk.line_range, Some((1, 10)));
        assert_eq!(chunk.hints.len(), 1);
    }

    #[test]
    fn test_chunk_serialization() {
        let chunk = Chunk::new("test-chunk", "some content").with_source(PathBuf::from("test.rs"));
        let json = serde_json::to_string(&chunk).unwrap();
        let deserialized: Chunk = serde_json::from_str(&json).unwrap();
        assert_eq!(chunk.id, deserialized.id);
        assert_eq!(chunk.content, deserialized.content);
        assert_eq!(chunk.source_path, deserialized.source_path);
    }

    // ==================== ChunkMetadata Tests ====================

    #[test]
    fn test_chunk_metadata_default() {
        let metadata = ChunkMetadata::default();
        assert!(metadata.language.is_none());
        assert!(!metadata.is_code);
        assert!(!metadata.is_structural_boundary);
        assert!(metadata.structural_type.is_none());
        assert_eq!(metadata.created_at, 0);
    }

    #[test]
    fn test_chunk_metadata_with_values() {
        let metadata = ChunkMetadata {
            language: Some("rust".to_string()),
            is_code: true,
            is_structural_boundary: true,
            structural_type: Some(StructuralType::Function),
            created_at: 1234567890,
        };
        assert_eq!(metadata.language, Some("rust".to_string()));
        assert!(metadata.is_code);
        assert_eq!(metadata.structural_type, Some(StructuralType::Function));
    }

    // ==================== StructuralType Tests ====================

    #[test]
    fn test_structural_type_variants() {
        let types = vec![
            StructuralType::Function,
            StructuralType::Class,
            StructuralType::Module,
            StructuralType::Struct,
            StructuralType::Enum,
            StructuralType::Trait,
            StructuralType::Impl,
            StructuralType::Interface,
            StructuralType::Method,
            StructuralType::File,
            StructuralType::Section,
            StructuralType::Paragraph,
        ];
        assert_eq!(types.len(), 12);
    }

    #[test]
    fn test_structural_type_equality() {
        assert_eq!(StructuralType::Function, StructuralType::Function);
        assert_ne!(StructuralType::Function, StructuralType::Class);
    }

    #[test]
    fn test_structural_type_serialization() {
        let struct_type = StructuralType::Function;
        let json = serde_json::to_string(&struct_type).unwrap();
        let deserialized: StructuralType = serde_json::from_str(&json).unwrap();
        assert_eq!(struct_type, deserialized);
    }

    // ==================== ChunkResult Tests ====================

    #[test]
    fn test_chunk_result_success() {
        let result = ChunkResult {
            chunk_id: "chunk-1".to_string(),
            success: true,
            findings: vec![],
            sub_results: vec![],
            depth: 2,
            tokens_used: 500,
            error: None,
        };
        assert!(result.success);
        assert_eq!(result.depth, 2);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_chunk_result_with_error() {
        let result = ChunkResult {
            chunk_id: "chunk-2".to_string(),
            success: false,
            findings: vec![],
            sub_results: vec![],
            depth: 1,
            tokens_used: 100,
            error: Some("Rate limit exceeded".to_string()),
        };
        assert!(!result.success);
        assert_eq!(result.error, Some("Rate limit exceeded".to_string()));
    }

    #[test]
    fn test_chunk_result_with_findings() {
        let finding = Finding {
            content: "Found authentication logic".to_string(),
            relevance: 0.9,
            source_chunk_id: "chunk-1".to_string(),
            source_path: Some(PathBuf::from("src/auth.rs")),
            line_range: Some((10, 50)),
            finding_type: FindingType::Answer,
            confidence: 0.85,
        };
        let result = ChunkResult {
            chunk_id: "chunk-1".to_string(),
            success: true,
            findings: vec![finding],
            sub_results: vec![],
            depth: 1,
            tokens_used: 1000,
            error: None,
        };
        assert_eq!(result.findings.len(), 1);
        assert!((result.findings[0].relevance - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_chunk_result_nested() {
        let sub_result = ChunkResult {
            chunk_id: "sub-1".to_string(),
            success: true,
            findings: vec![],
            sub_results: vec![],
            depth: 2,
            tokens_used: 200,
            error: None,
        };
        let result = ChunkResult {
            chunk_id: "parent".to_string(),
            success: true,
            findings: vec![],
            sub_results: vec![sub_result],
            depth: 1,
            tokens_used: 500,
            error: None,
        };
        assert_eq!(result.sub_results.len(), 1);
        assert_eq!(result.sub_results[0].chunk_id, "sub-1");
    }

    // ==================== Finding Tests ====================

    #[test]
    fn test_finding_creation() {
        let finding = Finding {
            content: "Token validation function".to_string(),
            relevance: 0.75,
            source_chunk_id: "chunk-5".to_string(),
            source_path: Some(PathBuf::from("src/token.rs")),
            line_range: Some((100, 150)),
            finding_type: FindingType::Evidence,
            confidence: 0.8,
        };
        assert_eq!(finding.finding_type, FindingType::Evidence);
        assert!((finding.confidence - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_finding_without_source() {
        let finding = Finding {
            content: "Related information".to_string(),
            relevance: 0.5,
            source_chunk_id: "chunk-3".to_string(),
            source_path: None,
            line_range: None,
            finding_type: FindingType::Related,
            confidence: 0.6,
        };
        assert!(finding.source_path.is_none());
        assert!(finding.line_range.is_none());
    }

    #[test]
    fn test_finding_serialization() {
        let finding = Finding {
            content: "Test finding".to_string(),
            relevance: 0.7,
            source_chunk_id: "chunk-1".to_string(),
            source_path: Some(PathBuf::from("test.rs")),
            line_range: Some((1, 10)),
            finding_type: FindingType::Answer,
            confidence: 0.9,
        };
        let json = serde_json::to_string(&finding).unwrap();
        let deserialized: Finding = serde_json::from_str(&json).unwrap();
        assert_eq!(finding.content, deserialized.content);
        assert_eq!(finding.finding_type, deserialized.finding_type);
    }

    // ==================== FindingType Tests ====================

    #[test]
    fn test_finding_type_variants() {
        let types = vec![
            FindingType::Answer,
            FindingType::Evidence,
            FindingType::Related,
            FindingType::Conflict,
            FindingType::Context,
        ];
        assert_eq!(types.len(), 5);
    }

    #[test]
    fn test_finding_type_equality() {
        assert_eq!(FindingType::Answer, FindingType::Answer);
        assert_ne!(FindingType::Answer, FindingType::Evidence);
    }

    // ==================== RlmResult Tests ====================

    #[test]
    fn test_rlm_result_success() {
        let result = RlmResult {
            success: true,
            answer: "The authentication is handled in auth.rs".to_string(),
            key_insights: vec![
                "Uses JWT tokens".to_string(),
                "Validates on each request".to_string(),
            ],
            evidence: vec![],
            chunks_explored: 15,
            max_depth_reached: 3,
            total_tokens: 50000,
            confidence: 0.92,
            duration_ms: 2500,
            error: None,
        };
        assert!(result.success);
        assert_eq!(result.key_insights.len(), 2);
        assert_eq!(result.chunks_explored, 15);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_rlm_result_failure() {
        let result = RlmResult {
            success: false,
            answer: String::new(),
            key_insights: vec![],
            evidence: vec![],
            chunks_explored: 5,
            max_depth_reached: 1,
            total_tokens: 10000,
            confidence: 0.0,
            duration_ms: 500,
            error: Some("Context too large to process".to_string()),
        };
        assert!(!result.success);
        assert!(result.answer.is_empty());
        assert!(result.error.is_some());
    }

    #[test]
    fn test_rlm_result_with_evidence() {
        let evidence = Evidence {
            content: "fn validate_token(token: &str) -> bool".to_string(),
            source_path: PathBuf::from("src/auth/validator.rs"),
            line_range: Some((42, 55)),
            relevance: 0.95,
        };
        let result = RlmResult {
            success: true,
            answer: "Token validation uses the validate_token function".to_string(),
            key_insights: vec!["Function returns bool".to_string()],
            evidence: vec![evidence],
            chunks_explored: 8,
            max_depth_reached: 2,
            total_tokens: 25000,
            confidence: 0.88,
            duration_ms: 1500,
            error: None,
        };
        assert_eq!(result.evidence.len(), 1);
        assert!((result.evidence[0].relevance - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rlm_result_serialization() {
        let result = RlmResult {
            success: true,
            answer: "Test answer".to_string(),
            key_insights: vec!["Insight 1".to_string()],
            evidence: vec![],
            chunks_explored: 10,
            max_depth_reached: 3,
            total_tokens: 30000,
            confidence: 0.85,
            duration_ms: 2000,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: RlmResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result.success, deserialized.success);
        assert_eq!(result.answer, deserialized.answer);
        assert_eq!(result.chunks_explored, deserialized.chunks_explored);
    }

    // ==================== Evidence Tests ====================

    #[test]
    fn test_evidence_creation() {
        let evidence = Evidence {
            content: "pub struct User { id: u64, name: String }".to_string(),
            source_path: PathBuf::from("src/models/user.rs"),
            line_range: Some((15, 18)),
            relevance: 0.8,
        };
        assert_eq!(evidence.source_path, PathBuf::from("src/models/user.rs"));
        assert_eq!(evidence.line_range, Some((15, 18)));
    }

    #[test]
    fn test_evidence_without_line_range() {
        let evidence = Evidence {
            content: "Configuration settings".to_string(),
            source_path: PathBuf::from("config.toml"),
            line_range: None,
            relevance: 0.6,
        };
        assert!(evidence.line_range.is_none());
    }

    #[test]
    fn test_evidence_serialization() {
        let evidence = Evidence {
            content: "Test content".to_string(),
            source_path: PathBuf::from("test.rs"),
            line_range: Some((1, 5)),
            relevance: 0.75,
        };
        let json = serde_json::to_string(&evidence).unwrap();
        let deserialized: Evidence = serde_json::from_str(&json).unwrap();
        assert_eq!(evidence.content, deserialized.content);
        assert_eq!(evidence.source_path, deserialized.source_path);
    }

    // ==================== estimate_tokens Tests ====================

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_exact_boundary() {
        assert_eq!(estimate_tokens("test"), 1); // 4 chars = 1 token
        assert_eq!(estimate_tokens("testtest"), 2); // 8 chars = 2 tokens
    }

    #[test]
    fn test_estimate_tokens_partial() {
        assert_eq!(estimate_tokens("this is a longer string"), 5); // 23 chars ≈ 5 tokens
        assert_eq!(estimate_tokens("abc"), 0); // 3 chars = 0 tokens (rounds down)
    }

    #[test]
    fn test_estimate_tokens_unicode() {
        // Unicode characters take more bytes
        let unicode_text = "日本語テスト";
        let tokens = estimate_tokens(unicode_text);
        // Should handle unicode without panicking
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_large_text() {
        let large_text = "a".repeat(40000);
        let tokens = estimate_tokens(&large_text);
        assert_eq!(tokens, 10000); // 40000 / 4 = 10000
    }

    #[test]
    fn test_estimate_tokens_code_sample() {
        let code = r#"
fn main() {
    let message = "Hello, world!";
    println!("{}", message);
}
"#;
        let tokens = estimate_tokens(code);
        // ~80 chars / 4 = ~20 tokens
        assert!(tokens > 10);
        assert!(tokens < 30);
    }

    #[test]
    fn test_estimate_tokens_mixed_content() {
        let content = "Code: fn foo() {} and text with numbers 12345";
        let tokens = estimate_tokens(content);
        assert!(tokens > 0);
    }

    // ==================== RlmCapabilityType Tests ====================

    #[test]
    fn test_capability_type_mcp_server() {
        let cap = RlmCapabilityType::mcp_server("test-mcp", "rest", "Test MCP server");
        assert_eq!(cap.name(), "test-mcp");
        match cap {
            RlmCapabilityType::McpServer {
                api_type,
                description,
                ..
            } => {
                assert_eq!(api_type, "rest");
                assert_eq!(description, "Test MCP server");
            }
            _ => panic!("Expected McpServer variant"),
        }
    }

    #[test]
    fn test_capability_type_skill() {
        let cap = RlmCapabilityType::skill("migration-skill", "migrate", "Database migrations");
        assert_eq!(cap.name(), "migration-skill");
        match cap {
            RlmCapabilityType::Skill {
                trigger, purpose, ..
            } => {
                assert_eq!(trigger, "migrate");
                assert_eq!(purpose, "Database migrations");
            }
            _ => panic!("Expected Skill variant"),
        }
    }

    #[test]
    fn test_capability_type_agent() {
        let cap = RlmCapabilityType::agent("security-agent", "Security review");
        assert_eq!(cap.name(), "security-agent");
        match cap {
            RlmCapabilityType::Agent { specialization, .. } => {
                assert_eq!(specialization, "Security review");
            }
            _ => panic!("Expected Agent variant"),
        }
    }

    #[test]
    fn test_capability_type_serialization() {
        let cap = RlmCapabilityType::mcp_server("test", "graphql", "GraphQL API");
        let json = serde_json::to_string(&cap).unwrap();
        let deserialized: RlmCapabilityType = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, deserialized);
    }

    // ==================== RlmCapabilityRegistry Tests ====================

    #[test]
    fn test_capability_registry_new() {
        let registry = RlmCapabilityRegistry::new();
        assert!(registry.capabilities.is_empty());
    }

    #[test]
    fn test_capability_registry_register() {
        let mut registry = RlmCapabilityRegistry::new();
        registry.register(RlmCapabilityType::mcp_server("api-mcp", "rest", "REST API"));
        assert_eq!(registry.capabilities.len(), 1);
        assert!(registry.has_capability("api-mcp"));
    }

    #[test]
    fn test_capability_registry_filters() {
        let mut registry = RlmCapabilityRegistry::new();
        registry.register(RlmCapabilityType::mcp_server("mcp1", "rest", "MCP 1"));
        registry.register(RlmCapabilityType::skill("skill1", "trigger1", "Skill 1"));
        registry.register(RlmCapabilityType::agent("agent1", "Agent 1"));
        registry.register(RlmCapabilityType::mcp_server("mcp2", "graphql", "MCP 2"));

        assert_eq!(registry.mcp_servers().count(), 2);
        assert_eq!(registry.skills().count(), 1);
        assert_eq!(registry.agents().count(), 1);
    }

    #[test]
    fn test_capability_registry_has_capability() {
        let mut registry = RlmCapabilityRegistry::new();
        registry.register(RlmCapabilityType::agent("test-agent", "Testing"));

        assert!(registry.has_capability("test-agent"));
        assert!(!registry.has_capability("nonexistent"));
    }

    #[test]
    fn test_capability_registry_with_defaults() {
        let registry = RlmCapabilityRegistry::with_defaults();

        // Should have default capabilities
        assert!(registry.has_capability("external-api-mcp"));
        assert!(registry.has_capability("schema-migration"));
        assert!(registry.has_capability("security-reviewer"));

        // Check counts
        assert!(registry.mcp_servers().count() >= 1);
        assert!(registry.skills().count() >= 1);
        assert!(registry.agents().count() >= 1);
    }

    #[test]
    fn test_capability_registry_serialization() {
        let registry = RlmCapabilityRegistry::with_defaults();
        let json = serde_json::to_string(&registry).unwrap();
        let deserialized: RlmCapabilityRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(registry.capabilities.len(), deserialized.capabilities.len());
    }

    // ==================== Creative Pipeline Tests ====================

    #[test]
    fn test_creative_domain_display_names() {
        assert_eq!(CreativeDomain::Video.display_name(), "Video");
        assert_eq!(CreativeDomain::Image.display_name(), "Image");
        assert_eq!(CreativeDomain::Audio.display_name(), "Audio");
        assert_eq!(CreativeDomain::Text.display_name(), "Text");
        assert_eq!(CreativeDomain::ThreeD.display_name(), "3D");
        assert_eq!(CreativeDomain::MultiModal.display_name(), "Multi-Modal");
    }

    #[test]
    fn test_creative_domain_recommended_tools() {
        let video_tools = CreativeDomain::Video.recommended_tools();
        assert!(video_tools.contains(&"veo"));
        assert!(video_tools.contains(&"ffmpeg"));

        let image_tools = CreativeDomain::Image.recommended_tools();
        assert!(image_tools.contains(&"gemini"));

        let threed_tools = CreativeDomain::ThreeD.recommended_tools();
        assert!(threed_tools.contains(&"threejs-dev"));
    }

    #[test]
    fn test_pipeline_stage_display_names() {
        assert_eq!(PipelineStage::Ideation.display_name(), "Ideation");
        assert_eq!(PipelineStage::PromptEngineering.display_name(), "Prompt Engineering");
        assert_eq!(PipelineStage::Generation.display_name(), "Generation");
        assert_eq!(PipelineStage::PostProcessing.display_name(), "Post-Processing");
        assert_eq!(PipelineStage::QualityReview.display_name(), "Quality Review");
        assert_eq!(PipelineStage::Assembly.display_name(), "Assembly");
        assert_eq!(PipelineStage::Distribution.display_name(), "Distribution");
    }

    #[test]
    fn test_pipeline_stage_next_stage() {
        assert_eq!(PipelineStage::Ideation.next_stage(), Some(PipelineStage::PromptEngineering));
        assert_eq!(PipelineStage::PromptEngineering.next_stage(), Some(PipelineStage::Generation));
        assert_eq!(PipelineStage::Generation.next_stage(), Some(PipelineStage::PostProcessing));
        assert_eq!(PipelineStage::PostProcessing.next_stage(), Some(PipelineStage::QualityReview));
        assert_eq!(PipelineStage::QualityReview.next_stage(), Some(PipelineStage::Assembly));
        assert_eq!(PipelineStage::Assembly.next_stage(), Some(PipelineStage::Distribution));
        assert_eq!(PipelineStage::Distribution.next_stage(), None);
    }

    #[test]
    fn test_pipeline_stage_is_terminal() {
        assert!(!PipelineStage::Ideation.is_terminal());
        assert!(!PipelineStage::Generation.is_terminal());
        assert!(PipelineStage::Distribution.is_terminal());
    }

    #[test]
    fn test_creative_pipeline_agent_creation() {
        let agent = RlmCapabilityType::creative_agent(
            "test-video-agent",
            CreativeDomain::Video,
            PipelineStage::Generation,
            vec!["veo".to_string()],
            "Generates test videos",
        );

        assert_eq!(agent.name(), "test-video-agent");
        assert!(agent.is_creative_pipeline());
        assert_eq!(agent.creative_domain(), Some(CreativeDomain::Video));
        assert_eq!(agent.pipeline_stage(), Some(PipelineStage::Generation));
    }

    #[test]
    fn test_non_creative_agent_methods() {
        let regular_agent = RlmCapabilityType::agent("regular", "Regular agent");
        assert!(!regular_agent.is_creative_pipeline());
        assert_eq!(regular_agent.creative_domain(), None);
        assert_eq!(regular_agent.pipeline_stage(), None);
    }

    #[test]
    fn test_capability_registry_with_creative_pipeline_defaults() {
        let registry = RlmCapabilityRegistry::with_creative_pipeline_defaults();

        // Should have creative pipeline agents
        assert!(registry.has_capability("video-ideation-agent"));
        assert!(registry.has_capability("video-generator"));
        assert!(registry.has_capability("image-generator"));
        assert!(registry.has_capability("threejs-scene-agent"));
        assert!(registry.has_capability("creative-orchestrator"));
        assert!(registry.has_capability("creative-qa-agent"));

        // Should still have regular capabilities
        assert!(registry.has_capability("external-api-mcp"));
        assert!(registry.has_capability("security-reviewer"));
    }

    #[test]
    fn test_capability_registry_creative_pipeline_agents_filter() {
        let registry = RlmCapabilityRegistry::with_creative_pipeline_defaults();
        let creative_agents: Vec<_> = registry.creative_pipeline_agents().collect();

        // Should have multiple creative pipeline agents
        assert!(creative_agents.len() >= 5);

        // All should be creative pipeline agents
        for agent in creative_agents {
            assert!(agent.is_creative_pipeline());
        }
    }

    #[test]
    fn test_capability_registry_agents_for_domain() {
        let registry = RlmCapabilityRegistry::with_creative_pipeline_defaults();

        let video_agents = registry.agents_for_domain(CreativeDomain::Video);
        assert!(video_agents.len() >= 3); // ideation, prompt eng, generator, post-processor

        let image_agents = registry.agents_for_domain(CreativeDomain::Image);
        assert!(image_agents.len() >= 1);

        let threed_agents = registry.agents_for_domain(CreativeDomain::ThreeD);
        assert!(threed_agents.len() >= 1);
    }

    #[test]
    fn test_capability_registry_agents_for_stage() {
        let registry = RlmCapabilityRegistry::with_creative_pipeline_defaults();

        let ideation_agents = registry.agents_for_stage(PipelineStage::Ideation);
        assert!(ideation_agents.len() >= 1);

        let generation_agents = registry.agents_for_stage(PipelineStage::Generation);
        assert!(generation_agents.len() >= 2); // video-generator, image-generator, threejs

        let qa_agents = registry.agents_for_stage(PipelineStage::QualityReview);
        assert!(qa_agents.len() >= 1);
    }

    #[test]
    fn test_get_pipeline_for_domain() {
        let registry = RlmCapabilityRegistry::with_creative_pipeline_defaults();

        let video_pipeline = registry.get_pipeline_for_domain(CreativeDomain::Video);
        assert!(!video_pipeline.is_empty());

        // Pipeline should be ordered by stage
        let mut prev_stage: Option<PipelineStage> = None;
        for agent in video_pipeline {
            if let Some(stage) = agent.pipeline_stage() {
                if let Some(prev) = prev_stage {
                    // Current stage should be >= previous stage in order
                    assert!(stage as u8 >= prev as u8, "Pipeline should be ordered by stage");
                }
                prev_stage = Some(stage);
            }
        }
    }

    #[test]
    fn test_creative_domain_serialization() {
        let domain = CreativeDomain::Video;
        let json = serde_json::to_string(&domain).unwrap();
        let deserialized: CreativeDomain = serde_json::from_str(&json).unwrap();
        assert_eq!(domain, deserialized);
    }

    #[test]
    fn test_pipeline_stage_serialization() {
        let stage = PipelineStage::Generation;
        let json = serde_json::to_string(&stage).unwrap();
        let deserialized: PipelineStage = serde_json::from_str(&json).unwrap();
        assert_eq!(stage, deserialized);
    }

    #[test]
    fn test_creative_pipeline_agent_serialization() {
        let agent = RlmCapabilityType::creative_agent(
            "test-agent",
            CreativeDomain::MultiModal,
            PipelineStage::Assembly,
            vec!["tool1".to_string(), "tool2".to_string()],
            "Test multi-modal agent",
        );

        let json = serde_json::to_string(&agent).unwrap();
        let deserialized: RlmCapabilityType = serde_json::from_str(&json).unwrap();

        assert_eq!(agent.name(), deserialized.name());
        assert_eq!(agent.creative_domain(), deserialized.creative_domain());
        assert_eq!(agent.pipeline_stage(), deserialized.pipeline_stage());
    }
}
