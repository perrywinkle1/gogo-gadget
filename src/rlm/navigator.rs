//! Navigation engine for RLM chunk selection
//!
//! Uses LLM to decide which chunks to explore next based on
//! the query and already-explored results.

use crate::rlm::embeddings::{get_default_embedder, EmbeddingProvider};
use crate::rlm::types::{Chunk, RlmConfig};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use tracing::debug;

/// Navigation decision from the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationDecision {
    /// IDs of chunks to explore next
    pub selected_chunks: Vec<String>,

    /// Reasoning for the selection
    pub reasoning: String,

    /// Whether to explore in parallel
    pub parallel: bool,

    /// Whether exploration is complete (answer found)
    pub exploration_complete: bool,

    /// Hints for chunk decomposition
    #[serde(default)]
    pub decomposition_hints: Vec<DecompositionHint>,
}

/// Hint for how to decompose a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompositionHint {
    pub chunk_id: String,
    pub strategy: String,
    pub reason: String,
}

/// Context for making navigation decisions
#[derive(Debug, Clone)]
pub struct NavigationContext {
    /// The original query
    pub query: String,

    /// Current recursion depth
    pub depth: u32,

    /// Maximum allowed depth
    pub max_depth: u32,

    /// Available chunks to choose from
    pub available_chunks: Vec<ChunkSummary>,

    /// Already explored chunks with their results
    pub explored: Vec<ExploredSummary>,

    /// Budget remaining (approximate tokens)
    pub budget_remaining: u32,
}

/// Summary of a chunk for navigation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSummary {
    pub id: String,
    pub preview: String,
    pub hints: Vec<String>,
    pub token_count: u32,
    pub relevance_score: f32,
    pub source_path: Option<String>,
}

impl From<&Chunk> for ChunkSummary {
    fn from(chunk: &Chunk) -> Self {
        Self {
            id: chunk.id.clone(),
            preview: chunk.preview(200).to_string(),
            hints: chunk.hints.clone(),
            token_count: chunk.token_count,
            relevance_score: chunk.relevance_score,
            source_path: chunk.source_path.as_ref().map(|p| p.display().to_string()),
        }
    }
}

/// Summary of an explored chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploredSummary {
    pub chunk_id: String,
    pub summary: String,
    pub findings_count: usize,
    pub was_useful: bool,
}

/// Trait for navigation engines
pub trait NavigationEngine: Send + Sync {
    /// Decide which chunks to explore next
    fn decide_next(&self, ctx: &NavigationContext) -> Result<NavigationDecision>;

    /// Pre-score chunks by relevance (using embeddings if available)
    fn prescore_chunks(&self, query: &str, chunks: &mut [Chunk]) -> Result<()>;
}

/// Default navigator using Claude CLI
pub struct Navigator {
    /// Model for navigation decisions (fast model)
    fast_model: String,

    /// Working directory for Claude CLI
    working_dir: PathBuf,

    /// Embedding provider for pre-scoring
    embedder: Option<Box<dyn EmbeddingProvider>>,

    /// Maximum chunks to show in prompt
    #[allow(dead_code)]
    max_chunks_in_prompt: usize,
}

impl Navigator {
    /// Create a new navigator
    pub fn new(working_dir: PathBuf, config: &RlmConfig) -> Self {
        let embedder = if config.use_embeddings {
            get_default_embedder().ok()
        } else {
            None
        };

        Self {
            fast_model: config
                .navigator_model
                .clone()
                .unwrap_or_else(|| "claude-3-haiku-20240307".to_string()),
            working_dir,
            embedder,
            max_chunks_in_prompt: config.max_chunks_per_level,
        }
    }

    /// Build the navigation prompt
    fn build_prompt(&self, ctx: &NavigationContext) -> String {
        let chunks_json =
            serde_json::to_string_pretty(&ctx.available_chunks).unwrap_or_else(|_| "[]".to_string());

        let explored_json =
            serde_json::to_string_pretty(&ctx.explored).unwrap_or_else(|_| "[]".to_string());

        format!(
            r#"You are navigating a large context to answer this query:

QUERY: {query}

CURRENT DEPTH: {depth}/{max_depth}
BUDGET REMAINING: {budget} tokens

AVAILABLE CHUNKS (sorted by relevance):
{chunks}

ALREADY EXPLORED:
{explored}

Based on the query and what we've already found, decide:
1. Which chunks to explore next (by ID)
2. Whether to explore them in parallel
3. Whether we have enough information to answer the query

Respond with ONLY valid JSON:
{{
  "selected_chunks": ["chunk-id-1", "chunk-id-2"],
  "reasoning": "Brief explanation",
  "parallel": true,
  "exploration_complete": false,
  "decomposition_hints": []
}}

If exploration_complete is true, we will stop and aggregate results.
Select up to 3 chunks maximum. Prioritize by relevance_score."#,
            query = ctx.query,
            depth = ctx.depth,
            max_depth = ctx.max_depth,
            budget = ctx.budget_remaining,
            chunks = chunks_json,
            explored = explored_json,
        )
    }

    /// Call Claude CLI for navigation decision
    fn call_claude(&self, prompt: &str) -> Result<String> {
        let output = Command::new("claude")
            .args(["--print", "--model", &self.fast_model, "-p", prompt])
            .current_dir(&self.working_dir)
            .output()
            .context("Failed to execute claude CLI")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Claude CLI failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim().to_string())
    }

    /// Extract JSON from Claude's response (handles markdown code blocks)
    fn extract_json(response: &str) -> &str {
        // Try to find JSON in code blocks first
        if let Some(start) = response.find("```json") {
            let content_start = start + 7;
            if let Some(end) = response[content_start..].find("```") {
                return response[content_start..content_start + end].trim();
            }
        }

        // Try plain code blocks
        if let Some(start) = response.find("```") {
            let content_start = start + 3;
            if let Some(end) = response[content_start..].find("```") {
                return response[content_start..content_start + end].trim();
            }
        }

        // Return as-is if no code blocks
        response.trim()
    }

    /// Heuristic scoring when embeddings aren't available
    fn heuristic_score(&self, query: &str, chunk: &Chunk) -> f32 {
        let query_lower = query.to_lowercase();
        let content_lower = chunk.content.to_lowercase();

        let mut score = 0.0f32;

        // Word overlap
        let query_words: std::collections::HashSet<_> = query_lower
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .collect();

        for word in &query_words {
            if content_lower.contains(*word) {
                score += 0.2;
            }
        }

        // Hint matching
        for hint in &chunk.hints {
            if query_lower.contains(&hint.to_lowercase()) {
                score += 0.3;
            }
        }

        // Structural bonus
        if chunk.metadata.is_structural_boundary {
            score += 0.1;
        }

        score.min(1.0)
    }
}

impl NavigationEngine for Navigator {
    fn decide_next(&self, ctx: &NavigationContext) -> Result<NavigationDecision> {
        // If no chunks available, we're done
        if ctx.available_chunks.is_empty() {
            return Ok(NavigationDecision {
                selected_chunks: vec![],
                reasoning: "No more chunks to explore".to_string(),
                parallel: false,
                exploration_complete: true,
                decomposition_hints: vec![],
            });
        }

        // If at max depth, stop
        if ctx.depth >= ctx.max_depth {
            return Ok(NavigationDecision {
                selected_chunks: vec![],
                reasoning: "Maximum depth reached".to_string(),
                parallel: false,
                exploration_complete: true,
                decomposition_hints: vec![],
            });
        }

        // If only one or two high-relevance chunks, just return them
        if ctx.available_chunks.len() <= 2 {
            let chunks: Vec<_> = ctx.available_chunks.iter().map(|c| c.id.clone()).collect();
            return Ok(NavigationDecision {
                selected_chunks: chunks,
                reasoning: "Few chunks remaining, exploring all".to_string(),
                parallel: true,
                exploration_complete: false,
                decomposition_hints: vec![],
            });
        }

        // Build and send prompt
        let prompt = self.build_prompt(ctx);
        let response = self.call_claude(&prompt)?;

        // Parse response
        let json_str = Self::extract_json(&response);
        let decision: NavigationDecision =
            serde_json::from_str(json_str).context("Failed to parse navigation decision")?;

        debug!("Navigation decision: {:?}", decision);
        Ok(decision)
    }

    fn prescore_chunks(&self, query: &str, chunks: &mut [Chunk]) -> Result<()> {
        let embedder = match &self.embedder {
            Some(e) => e,
            None => {
                // No embedder, use heuristic scoring
                for chunk in chunks.iter_mut() {
                    chunk.relevance_score = self.heuristic_score(query, chunk);
                }
                chunks.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
                return Ok(());
            }
        };

        // Get query embedding
        let query_texts: Vec<&str> = vec![query];
        let query_embeddings = embedder.embed(&query_texts)?;
        let query_embedding = &query_embeddings[0];

        // Get chunk embeddings (batch for efficiency)
        let chunk_previews: Vec<&str> = chunks.iter().map(|c| c.preview(500)).collect();
        let chunk_embeddings = embedder.embed(&chunk_previews)?;

        // Compute cosine similarity
        for (i, chunk) in chunks.iter_mut().enumerate() {
            chunk.relevance_score = cosine_similarity(query_embedding, &chunk_embeddings[i]);
        }

        // Sort by relevance
        chunks.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());

        Ok(())
    }
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation_decision_parsing() {
        let json = r#"{
            "selected_chunks": ["chunk-1", "chunk-2"],
            "reasoning": "These look relevant",
            "parallel": true,
            "exploration_complete": false,
            "decomposition_hints": []
        }"#;

        let decision: NavigationDecision = serde_json::from_str(json).unwrap();
        assert_eq!(decision.selected_chunks.len(), 2);
        assert!(decision.parallel);
        assert!(!decision.exploration_complete);
    }

    #[test]
    fn test_navigation_decision_minimal() {
        let json = r#"{
            "selected_chunks": [],
            "reasoning": "done",
            "parallel": false,
            "exploration_complete": true
        }"#;

        let decision: NavigationDecision = serde_json::from_str(json).unwrap();
        assert!(decision.selected_chunks.is_empty());
        assert!(decision.exploration_complete);
        assert!(decision.decomposition_hints.is_empty());
    }

    #[test]
    fn test_navigation_decision_with_hints() {
        let json = r#"{
            "selected_chunks": ["chunk-1"],
            "reasoning": "needs decomposition",
            "parallel": false,
            "exploration_complete": false,
            "decomposition_hints": [
                {"chunk_id": "chunk-1", "strategy": "structural", "reason": "too large"}
            ]
        }"#;

        let decision: NavigationDecision = serde_json::from_str(json).unwrap();
        assert_eq!(decision.decomposition_hints.len(), 1);
        assert_eq!(decision.decomposition_hints[0].strategy, "structural");
    }

    #[test]
    fn test_extract_json_code_block() {
        let response = r#"Here's my decision:
```json
{"selected_chunks": ["a"], "reasoning": "test", "parallel": false, "exploration_complete": true}
```
"#;
        let json = Navigator::extract_json(response);
        assert!(json.starts_with('{'));
        assert!(json.contains("selected_chunks"));
    }

    #[test]
    fn test_extract_json_plain_code_block() {
        let response = r#"```
{"selected_chunks": ["b"], "reasoning": "test", "parallel": true, "exploration_complete": false}
```"#;
        let json = Navigator::extract_json(response);
        assert!(json.starts_with('{'));
    }

    #[test]
    fn test_extract_json_no_block() {
        let response = r#"{"selected_chunks": ["c"], "reasoning": "test", "parallel": false, "exploration_complete": true}"#;
        let json = Navigator::extract_json(response);
        assert_eq!(json, response);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_heuristic_score() {
        let nav = Navigator {
            fast_model: "test".to_string(),
            working_dir: PathBuf::from("."),
            embedder: None,
            max_chunks_in_prompt: 10,
        };

        let mut chunk = Chunk::new("test", "This is a function that handles authentication");
        chunk.hints = vec!["auth".to_string()];

        let score = nav.heuristic_score("find authentication code", &chunk);
        assert!(score > 0.0);
    }

    #[test]
    fn test_heuristic_score_no_match() {
        let nav = Navigator {
            fast_model: "test".to_string(),
            working_dir: PathBuf::from("."),
            embedder: None,
            max_chunks_in_prompt: 10,
        };

        let chunk = Chunk::new("test", "unrelated content about databases");
        let score = nav.heuristic_score("find authentication code", &chunk);
        assert!(score < 0.5);
    }

    #[test]
    fn test_heuristic_score_structural_bonus() {
        let nav = Navigator {
            fast_model: "test".to_string(),
            working_dir: PathBuf::from("."),
            embedder: None,
            max_chunks_in_prompt: 10,
        };

        let mut chunk = Chunk::new("test", "some content");
        chunk.metadata.is_structural_boundary = true;

        let score = nav.heuristic_score("query", &chunk);
        assert!(score >= 0.1);
    }

    #[test]
    fn test_chunk_summary_from_chunk() {
        let chunk = Chunk::new("chunk-1", "This is test content for the chunk")
            .with_source(PathBuf::from("src/main.rs"))
            .with_hints(vec!["main".to_string()]);

        let summary = ChunkSummary::from(&chunk);
        assert_eq!(summary.id, "chunk-1");
        assert_eq!(summary.hints, vec!["main".to_string()]);
        assert!(summary.source_path.is_some());
    }

    #[test]
    fn test_navigation_context_empty_chunks() {
        let nav = Navigator {
            fast_model: "test".to_string(),
            working_dir: PathBuf::from("."),
            embedder: None,
            max_chunks_in_prompt: 10,
        };

        let ctx = NavigationContext {
            query: "test query".to_string(),
            depth: 0,
            max_depth: 5,
            available_chunks: vec![],
            explored: vec![],
            budget_remaining: 10000,
        };

        let decision = nav.decide_next(&ctx).unwrap();
        assert!(decision.exploration_complete);
        assert!(decision.selected_chunks.is_empty());
    }

    #[test]
    fn test_navigation_context_max_depth() {
        let nav = Navigator {
            fast_model: "test".to_string(),
            working_dir: PathBuf::from("."),
            embedder: None,
            max_chunks_in_prompt: 10,
        };

        let ctx = NavigationContext {
            query: "test query".to_string(),
            depth: 5,
            max_depth: 5,
            available_chunks: vec![ChunkSummary {
                id: "chunk-1".to_string(),
                preview: "test".to_string(),
                hints: vec![],
                token_count: 100,
                relevance_score: 0.8,
                source_path: None,
            }],
            explored: vec![],
            budget_remaining: 10000,
        };

        let decision = nav.decide_next(&ctx).unwrap();
        assert!(decision.exploration_complete);
        assert_eq!(decision.reasoning, "Maximum depth reached");
    }

    #[test]
    fn test_navigation_context_few_chunks() {
        let nav = Navigator {
            fast_model: "test".to_string(),
            working_dir: PathBuf::from("."),
            embedder: None,
            max_chunks_in_prompt: 10,
        };

        let ctx = NavigationContext {
            query: "test query".to_string(),
            depth: 0,
            max_depth: 5,
            available_chunks: vec![
                ChunkSummary {
                    id: "chunk-1".to_string(),
                    preview: "test".to_string(),
                    hints: vec![],
                    token_count: 100,
                    relevance_score: 0.8,
                    source_path: None,
                },
                ChunkSummary {
                    id: "chunk-2".to_string(),
                    preview: "test2".to_string(),
                    hints: vec![],
                    token_count: 100,
                    relevance_score: 0.7,
                    source_path: None,
                },
            ],
            explored: vec![],
            budget_remaining: 10000,
        };

        let decision = nav.decide_next(&ctx).unwrap();
        assert!(!decision.exploration_complete);
        assert_eq!(decision.selected_chunks.len(), 2);
        assert!(decision.parallel);
    }

    #[test]
    fn test_decomposition_hint_serialization() {
        let hint = DecompositionHint {
            chunk_id: "chunk-1".to_string(),
            strategy: "structural".to_string(),
            reason: "too large".to_string(),
        };

        let json = serde_json::to_string(&hint).unwrap();
        let deserialized: DecompositionHint = serde_json::from_str(&json).unwrap();
        assert_eq!(hint.chunk_id, deserialized.chunk_id);
        assert_eq!(hint.strategy, deserialized.strategy);
    }

    #[test]
    fn test_explored_summary_serialization() {
        let summary = ExploredSummary {
            chunk_id: "chunk-1".to_string(),
            summary: "Found relevant code".to_string(),
            findings_count: 3,
            was_useful: true,
        };

        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: ExploredSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(summary.chunk_id, deserialized.chunk_id);
        assert!(deserialized.was_useful);
    }

    #[test]
    fn test_prescore_chunks_heuristic() {
        let nav = Navigator {
            fast_model: "test".to_string(),
            working_dir: PathBuf::from("."),
            embedder: None,
            max_chunks_in_prompt: 10,
        };

        let mut chunks = vec![
            Chunk::new("chunk-1", "irrelevant content"),
            Chunk::new("chunk-2", "authentication and login code"),
        ];

        nav.prescore_chunks("find authentication", &mut chunks)
            .unwrap();

        // chunk-2 should be first (higher relevance)
        assert_eq!(chunks[0].id, "chunk-2");
        assert!(chunks[0].relevance_score > chunks[1].relevance_score);
    }
}
