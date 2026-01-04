//! Core types for the RLM module

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
            navigator_model: Some("claude-opus-4-5-20251101".to_string()),
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
        assert_eq!(config.navigator_model, Some("claude-3-haiku-20240307".to_string()));
        assert_eq!(config.explorer_model, None);
        assert!(config.use_embeddings);
        assert_eq!(config.rate_limit_per_minute, 10);
    }

    #[test]
    fn test_rlm_config_serialization() {
        let config = RlmConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.max_depth, deserialized.max_depth);
        assert_eq!(config.max_chunks_per_level, deserialized.max_chunks_per_level);
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
        let chunk = Chunk::new("test", "content")
            .with_byte_range(100, 200);
        assert_eq!(chunk.byte_range, Some((100, 200)));
    }

    #[test]
    fn test_chunk_with_hints() {
        let hints = vec!["function".to_string(), "authentication".to_string()];
        let chunk = Chunk::new("test", "content")
            .with_hints(hints.clone());
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
        let chunk = Chunk::new("test-chunk", "some content")
            .with_source(PathBuf::from("test.rs"));
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
}
