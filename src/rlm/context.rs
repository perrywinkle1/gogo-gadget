//! RLM Context - Explorable environment for recursive queries
//!
//! Treats large inputs as environments that can be navigated recursively.

use crate::rlm::chunker::{ChunkStrategy, ContextChunker, DefaultChunker};
use crate::rlm::types::{Chunk, RlmConfig};
use anyhow::Result;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// An explorable context environment
pub struct RlmContext {
    /// All chunks indexed by ID
    chunks: HashMap<String, Chunk>,

    /// Root-level chunk IDs
    root_chunks: Vec<String>,

    /// Total tokens in the context
    total_tokens: u32,

    /// Source path (file or directory)
    source_path: Option<PathBuf>,

    /// Chunker for decomposition
    chunker: Box<dyn ContextChunker>,

    /// Configuration
    config: RlmConfig,
}

impl fmt::Debug for RlmContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RlmContext")
            .field("chunks", &self.chunks.len())
            .field("root_chunks", &self.root_chunks)
            .field("total_tokens", &self.total_tokens)
            .field("source_path", &self.source_path)
            .field("chunker", &"<ContextChunker>")
            .field("config", &self.config)
            .finish()
    }
}

impl RlmContext {
    /// Create a new empty context
    pub fn new(config: RlmConfig) -> Self {
        Self {
            chunks: HashMap::new(),
            root_chunks: Vec::new(),
            total_tokens: 0,
            source_path: None,
            chunker: Box::new(DefaultChunker::default()),
            config,
        }
    }

    /// Create context from a file
    pub fn from_file(path: &Path, config: RlmConfig) -> Result<Self> {
        let chunker = DefaultChunker::default();
        let chunks = chunker.chunk_file(path, ChunkStrategy::Structural)?;

        let mut context = Self::new(config);
        context.source_path = Some(path.to_path_buf());

        for chunk in chunks {
            context.total_tokens += chunk.token_count;
            context.root_chunks.push(chunk.id.clone());
            context.chunks.insert(chunk.id.clone(), chunk);
        }

        info!(
            "Created context from file {} with {} chunks, {} tokens",
            path.display(),
            context.chunks.len(),
            context.total_tokens
        );

        Ok(context)
    }

    /// Create context from a directory
    pub fn from_directory(path: &Path, config: RlmConfig) -> Result<Self> {
        let chunker = DefaultChunker::default();
        let chunks = chunker.chunk_directory(path, ChunkStrategy::ByFile)?;

        let mut context = Self::new(config);
        context.source_path = Some(path.to_path_buf());

        for chunk in chunks {
            context.total_tokens += chunk.token_count;
            context.root_chunks.push(chunk.id.clone());
            context.chunks.insert(chunk.id.clone(), chunk);
        }

        info!(
            "Created context from directory {} with {} chunks, {} tokens",
            path.display(),
            context.chunks.len(),
            context.total_tokens
        );

        Ok(context)
    }

    /// Create context from raw text
    pub fn from_text(content: String, config: RlmConfig) -> Result<Self> {
        let chunker = DefaultChunker::default();
        let chunks = chunker.chunk(&content, ChunkStrategy::default())?;

        let mut context = Self::new(config);

        for chunk in chunks {
            context.total_tokens += chunk.token_count;
            context.root_chunks.push(chunk.id.clone());
            context.chunks.insert(chunk.id.clone(), chunk);
        }

        info!(
            "Created context from text with {} chunks, {} tokens",
            context.chunks.len(),
            context.total_tokens
        );

        Ok(context)
    }

    /// Get a chunk by ID
    pub fn get_chunk(&self, id: &str) -> Option<&Chunk> {
        self.chunks.get(id)
    }

    /// Add a chunk to the context
    pub fn add_chunk(&mut self, chunk: Chunk) {
        self.total_tokens += chunk.token_count;
        if chunk.parent_id.is_none() {
            self.root_chunks.push(chunk.id.clone());
        }
        self.chunks.insert(chunk.id.clone(), chunk);
    }

    /// Get all root chunks
    pub fn root_chunks(&self) -> Vec<&Chunk> {
        self.root_chunks
            .iter()
            .filter_map(|id| self.chunks.get(id))
            .collect()
    }

    /// Get all chunks
    pub fn all_chunks(&self) -> impl Iterator<Item = &Chunk> {
        self.chunks.values()
    }

    /// Get chunk count
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Get total tokens
    pub fn total_tokens(&self) -> u32 {
        self.total_tokens
    }

    /// Decompose a chunk into sub-chunks
    pub fn decompose(&mut self, chunk_id: &str, strategy: ChunkStrategy) -> Result<Vec<String>> {
        let chunk = self
            .chunks
            .get(chunk_id)
            .ok_or_else(|| anyhow::anyhow!("Chunk not found: {}", chunk_id))?
            .clone();

        let sub_chunks = self.chunker.chunk(&chunk.content, strategy)?;

        let mut new_ids = Vec::new();
        for mut sub in sub_chunks {
            sub.id = format!("{}-{}", chunk_id, sub.id);
            sub.parent_id = Some(chunk_id.to_string());
            sub.source_path = chunk.source_path.clone();

            new_ids.push(sub.id.clone());
            self.chunks.insert(sub.id.clone(), sub);
        }

        // Update parent's children
        if let Some(parent) = self.chunks.get_mut(chunk_id) {
            parent.children = new_ids.clone();
        }

        debug!(
            "Decomposed chunk {} into {} sub-chunks",
            chunk_id,
            new_ids.len()
        );
        Ok(new_ids)
    }

    /// Create a sub-context from a single chunk
    pub fn create_sub_context(&self, chunk: &Chunk) -> RlmContext {
        let mut sub = RlmContext::new(self.config.clone());
        sub.source_path = chunk.source_path.clone();
        sub.total_tokens = chunk.token_count;
        sub.root_chunks.push(chunk.id.clone());
        sub.chunks.insert(chunk.id.clone(), chunk.clone());
        sub
    }

    /// Check if a chunk fits within a single context window
    pub fn chunk_fits_context(&self, chunk_id: &str, context_size: u32) -> bool {
        self.chunks
            .get(chunk_id)
            .map(|c| c.token_count <= context_size)
            .unwrap_or(false)
    }

    /// Get chunks that haven't been explored yet
    pub fn unexplored_chunks(&self, explored: &[String]) -> Vec<&Chunk> {
        self.chunks
            .values()
            .filter(|c| !explored.contains(&c.id))
            .collect()
    }

    /// Update chunk relevance scores
    pub fn update_relevance(&mut self, chunk_id: &str, score: f32) {
        if let Some(chunk) = self.chunks.get_mut(chunk_id) {
            chunk.relevance_score = score;
        }
    }

    /// Get the source path
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    /// Get configuration
    pub fn config(&self) -> &RlmConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_context_new() {
        let config = RlmConfig::default();
        let context = RlmContext::new(config);

        assert_eq!(context.chunk_count(), 0);
        assert_eq!(context.total_tokens(), 0);
        assert!(context.source_path().is_none());
    }

    #[test]
    fn test_context_from_text() {
        let config = RlmConfig::default();
        let context =
            RlmContext::from_text("Hello world. This is a test.".to_string(), config).unwrap();

        assert!(context.chunk_count() > 0);
        assert!(context.total_tokens() > 0);
    }

    #[test]
    fn test_context_from_text_empty() {
        let config = RlmConfig::default();
        let context = RlmContext::from_text(String::new(), config).unwrap();

        assert_eq!(context.chunk_count(), 0);
        assert_eq!(context.total_tokens(), 0);
    }

    #[test]
    fn test_context_from_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.rs");
        fs::write(&file, "fn main() { println!(\"hello\"); }").unwrap();

        let config = RlmConfig::default();
        let context = RlmContext::from_file(&file, config).unwrap();

        assert!(context.chunk_count() > 0);
        assert!(context.source_path().is_some());
    }

    #[test]
    fn test_context_from_directory() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn foo() {}").unwrap();
        fs::write(src.join("main.rs"), "fn main() {}").unwrap();

        let config = RlmConfig::default();
        let context = RlmContext::from_directory(temp.path(), config).unwrap();

        assert!(context.chunk_count() > 0);
    }

    #[test]
    fn test_get_chunk() {
        let config = RlmConfig::default();
        let context = RlmContext::from_text("Test content".to_string(), config).unwrap();

        let root_ids: Vec<_> = context.root_chunks.clone();
        assert!(!root_ids.is_empty());

        let chunk = context.get_chunk(&root_ids[0]);
        assert!(chunk.is_some());
    }

    #[test]
    fn test_get_chunk_not_found() {
        let config = RlmConfig::default();
        let context = RlmContext::from_text("Test content".to_string(), config).unwrap();

        let chunk = context.get_chunk("nonexistent-id");
        assert!(chunk.is_none());
    }

    #[test]
    fn test_root_chunks() {
        let config = RlmConfig::default();
        let context = RlmContext::from_text("Test content".to_string(), config).unwrap();

        let root = context.root_chunks();
        assert!(!root.is_empty());
    }

    #[test]
    fn test_all_chunks() {
        let config = RlmConfig::default();
        let context = RlmContext::from_text("Test content".to_string(), config).unwrap();

        let all: Vec<_> = context.all_chunks().collect();
        assert!(!all.is_empty());
        assert_eq!(all.len(), context.chunk_count());
    }

    #[test]
    fn test_decompose_chunk() {
        let config = RlmConfig::default();
        let long_content = "a".repeat(20000);
        let mut context = RlmContext::from_text(long_content, config).unwrap();

        let root_id = context.root_chunks[0].clone();
        let sub_ids = context
            .decompose(
                &root_id,
                ChunkStrategy::FixedSize {
                    size: 1000,
                    overlap: 100,
                },
            )
            .unwrap();

        assert!(!sub_ids.is_empty());

        // Verify parent has children
        let parent = context.get_chunk(&root_id).unwrap();
        assert!(!parent.children.is_empty());

        // Verify children have parent ID
        for sub_id in &sub_ids {
            let sub = context.get_chunk(sub_id).unwrap();
            assert_eq!(sub.parent_id, Some(root_id.clone()));
        }
    }

    #[test]
    fn test_decompose_nonexistent() {
        let config = RlmConfig::default();
        let mut context = RlmContext::from_text("Test".to_string(), config).unwrap();

        let result = context.decompose("nonexistent", ChunkStrategy::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_sub_context() {
        let config = RlmConfig::default();
        let context = RlmContext::from_text("Test content".to_string(), config).unwrap();

        let chunk = context.root_chunks()[0];
        let sub = context.create_sub_context(chunk);

        assert_eq!(sub.chunk_count(), 1);
        assert_eq!(sub.total_tokens(), chunk.token_count);
    }

    #[test]
    fn test_chunk_fits_context() {
        let config = RlmConfig::default();
        let context = RlmContext::from_text("Small content".to_string(), config).unwrap();

        let root_id = &context.root_chunks[0];
        assert!(context.chunk_fits_context(root_id, 10000));
        assert!(!context.chunk_fits_context(root_id, 0));
    }

    #[test]
    fn test_chunk_fits_context_nonexistent() {
        let config = RlmConfig::default();
        let context = RlmContext::from_text("Test".to_string(), config).unwrap();

        assert!(!context.chunk_fits_context("nonexistent", 10000));
    }

    #[test]
    fn test_unexplored_chunks() {
        let config = RlmConfig::default();
        let context = RlmContext::from_text("Test content here".to_string(), config).unwrap();

        let unexplored = context.unexplored_chunks(&[]);
        assert_eq!(unexplored.len(), context.chunk_count());

        let root_id = context.root_chunks[0].clone();
        let unexplored_after = context.unexplored_chunks(&[root_id]);
        assert!(unexplored_after.len() < unexplored.len());
    }

    #[test]
    fn test_update_relevance() {
        let config = RlmConfig::default();
        let mut context = RlmContext::from_text("Test content".to_string(), config).unwrap();

        let root_id = context.root_chunks[0].clone();
        context.update_relevance(&root_id, 0.9);

        let chunk = context.get_chunk(&root_id).unwrap();
        assert!((chunk.relevance_score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_update_relevance_nonexistent() {
        let config = RlmConfig::default();
        let mut context = RlmContext::from_text("Test".to_string(), config).unwrap();

        // Should not panic
        context.update_relevance("nonexistent", 0.5);
    }

    #[test]
    fn test_config_accessor() {
        let config = RlmConfig {
            max_depth: 10,
            ..RlmConfig::default()
        };
        let context = RlmContext::new(config);

        assert_eq!(context.config().max_depth, 10);
    }
}
