//! RLM Executor - Main execution engine for recursive exploration
//!
//! Orchestrates chunking, navigation, and result aggregation.

use crate::rlm::chunker::ChunkStrategy;
use crate::rlm::context::RlmContext;
use crate::rlm::navigator::{ChunkSummary, ExploredSummary, NavigationContext, NavigationEngine, Navigator};
use crate::rlm::types::{Chunk, ChunkResult, Evidence, Finding, FindingType, RlmConfig, RlmResult};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Main RLM executor
pub struct RlmExecutor {
    /// Configuration
    config: RlmConfig,

    /// Navigator for chunk selection
    navigator: Box<dyn NavigationEngine>,

    /// Working directory
    working_dir: PathBuf,

    /// Explored chunk IDs (to prevent revisiting)
    explored: HashSet<String>,

    /// All chunk results collected during exploration
    results: Vec<ChunkResult>,
}

impl RlmExecutor {
    /// Create a new executor
    pub fn new(config: RlmConfig, working_dir: PathBuf) -> Self {
        let navigator = Box::new(Navigator::new(working_dir.clone(), &config));

        Self {
            config,
            navigator,
            working_dir,
            explored: HashSet::new(),
            results: Vec::new(),
        }
    }

    /// Create executor with a custom navigator (for testing)
    pub fn with_navigator(
        config: RlmConfig,
        working_dir: PathBuf,
        navigator: Box<dyn NavigationEngine>,
    ) -> Self {
        Self {
            config,
            navigator,
            working_dir,
            explored: HashSet::new(),
            results: Vec::new(),
        }
    }

    /// Execute an RLM query on a file or directory
    pub fn execute(&mut self, query: &str, context_path: &Path) -> Result<RlmResult> {
        let start = Instant::now();

        // Create context from path
        let mut context = if context_path.is_dir() {
            RlmContext::from_directory(context_path, self.config.clone())?
        } else {
            RlmContext::from_file(context_path, self.config.clone())?
        };

        info!(
            "Starting RLM query: {} on {} chunks",
            query,
            context.chunk_count()
        );

        // Pre-score chunks by relevance
        let mut chunks: Vec<Chunk> = context.all_chunks().cloned().collect();
        self.navigator.prescore_chunks(query, &mut chunks)?;

        // Update context with scores
        for chunk in &chunks {
            context.update_relevance(&chunk.id, chunk.relevance_score);
        }

        // Start recursive exploration
        self.explored.clear();
        self.results.clear();

        self.explore_recursive(query, &mut context, 0)?;

        // Aggregate results
        let result = self.aggregate_results(query, start.elapsed().as_millis() as u64)?;

        Ok(result)
    }

    /// Execute an RLM query on an existing RlmContext (for daemon mode)
    pub fn execute_on_context(&mut self, query: &str, context: &RlmContext) -> Result<RlmResult> {
        let start = Instant::now();

        // Clone the context so we can modify it
        let mut ctx = RlmContext::new(context.config().clone());
        for chunk in context.all_chunks() {
            ctx.add_chunk(chunk.clone());
        }

        info!(
            "Starting RLM query on context: {} on {} chunks",
            query,
            ctx.chunk_count()
        );

        // Pre-score chunks by relevance
        let mut chunks: Vec<Chunk> = ctx.all_chunks().cloned().collect();
        self.navigator.prescore_chunks(query, &mut chunks)?;

        // Update context with scores
        for chunk in &chunks {
            ctx.update_relevance(&chunk.id, chunk.relevance_score);
        }

        // Start recursive exploration
        self.explored.clear();
        self.results.clear();

        self.explore_recursive(query, &mut ctx, 0)?;

        // Aggregate results
        let result = self.aggregate_results(query, start.elapsed().as_millis() as u64)?;

        Ok(result)
    }

    /// Execute an RLM query on raw text content
    pub fn execute_on_text(&mut self, query: &str, content: String) -> Result<RlmResult> {
        let start = Instant::now();

        let mut context = RlmContext::from_text(content, self.config.clone())?;

        info!(
            "Starting RLM query on text: {} on {} chunks",
            query,
            context.chunk_count()
        );

        // Pre-score chunks by relevance
        let mut chunks: Vec<Chunk> = context.all_chunks().cloned().collect();
        self.navigator.prescore_chunks(query, &mut chunks)?;

        // Update context with scores
        for chunk in &chunks {
            context.update_relevance(&chunk.id, chunk.relevance_score);
        }

        // Start recursive exploration
        self.explored.clear();
        self.results.clear();

        self.explore_recursive(query, &mut context, 0)?;

        // Aggregate results
        let result = self.aggregate_results(query, start.elapsed().as_millis() as u64)?;

        Ok(result)
    }

    /// Recursive exploration of context
    fn explore_recursive(
        &mut self,
        query: &str,
        context: &mut RlmContext,
        depth: u32,
    ) -> Result<()> {
        // Check depth limit
        if depth >= self.config.max_depth {
            debug!("Max depth {} reached", depth);
            return Ok(());
        }

        // Get available chunks (not yet explored)
        let explored_ids: Vec<String> = self.explored.iter().cloned().collect();
        let available: Vec<ChunkSummary> = context
            .unexplored_chunks(&explored_ids)
            .into_iter()
            .filter(|c| c.relevance_score >= self.config.min_relevance)
            .take(self.config.max_chunks_per_level)
            .map(ChunkSummary::from)
            .collect();

        if available.is_empty() {
            debug!("No more chunks to explore at depth {}", depth);
            return Ok(());
        }

        // Get explored summaries for context
        let explored_summaries: Vec<ExploredSummary> = self
            .results
            .iter()
            .map(|r| ExploredSummary {
                chunk_id: r.chunk_id.clone(),
                summary: r
                    .findings
                    .first()
                    .map(|f| f.content.chars().take(100).collect())
                    .unwrap_or_default(),
                findings_count: r.findings.len(),
                was_useful: !r.findings.is_empty(),
            })
            .collect();

        // Build navigation context
        let nav_ctx = NavigationContext {
            query: query.to_string(),
            depth,
            max_depth: self.config.max_depth,
            available_chunks: available,
            explored: explored_summaries,
            budget_remaining: self
                .config
                .max_total_tokens
                .saturating_sub(self.results.iter().map(|r| r.tokens_used).sum()),
        };

        // Get navigation decision
        let decision = self.navigator.decide_next(&nav_ctx)?;

        if decision.exploration_complete {
            info!(
                "Exploration complete at depth {}: {}",
                depth, decision.reasoning
            );
            return Ok(());
        }

        // Explore selected chunks
        for chunk_id in &decision.selected_chunks {
            if self.explored.contains(chunk_id) {
                continue;
            }

            self.explored.insert(chunk_id.clone());

            if let Some(chunk) = context.get_chunk(chunk_id) {
                let chunk_clone = chunk.clone();
                let result = self.explore_chunk(query, &chunk_clone, context, depth)?;
                self.results.push(result);
            }
        }

        // Recurse if we found useful results
        if !decision.exploration_complete && depth + 1 < self.config.max_depth {
            self.explore_recursive(query, context, depth + 1)?;
        }

        Ok(())
    }

    /// Explore a single chunk
    fn explore_chunk(
        &mut self,
        query: &str,
        chunk: &Chunk,
        context: &mut RlmContext,
        depth: u32,
    ) -> Result<ChunkResult> {
        debug!("Exploring chunk {} at depth {}", chunk.id, depth);

        // If chunk is small enough, query it directly
        if chunk.token_count <= 4000 {
            return self.query_chunk(query, chunk, depth);
        }

        // Otherwise, decompose and recurse
        let sub_ids = context.decompose(
            &chunk.id,
            ChunkStrategy::FixedSize {
                size: 2000,
                overlap: 200,
            },
        )?;

        let mut sub_results = Vec::new();
        for sub_id in sub_ids {
            if let Some(sub_chunk) = context.get_chunk(&sub_id) {
                let sub_chunk_clone = sub_chunk.clone();
                let result = self.explore_chunk(query, &sub_chunk_clone, context, depth + 1)?;
                sub_results.push(result);
            }
        }

        // Combine sub-results
        let findings: Vec<Finding> = sub_results
            .iter()
            .flat_map(|r| r.findings.clone())
            .collect();

        let tokens_used: u32 = sub_results.iter().map(|r| r.tokens_used).sum();

        Ok(ChunkResult {
            chunk_id: chunk.id.clone(),
            success: true,
            findings,
            sub_results,
            depth,
            tokens_used,
            error: None,
        })
    }

    /// Query a chunk directly using Claude
    fn query_chunk(&self, query: &str, chunk: &Chunk, depth: u32) -> Result<ChunkResult> {
        let source_display = chunk
            .source_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "inline content".to_string());

        let prompt = format!(
            r#"Answer this query based on the following content:

QUERY: {}

CONTENT (from {}):
{}

Respond with JSON only:
{{
  "relevant": true/false,
  "findings": [
    {{"content": "...", "type": "answer|evidence|related", "confidence": 0.0-1.0}}
  ]
}}

If not relevant, set "relevant": false and "findings": []."#,
            query, source_display, chunk.content
        );

        let output = Command::new("claude")
            .args(["--print", "-p", &prompt])
            .current_dir(&self.working_dir)
            .output()
            .context("Failed to execute claude CLI")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Claude CLI failed for chunk {}: {}", chunk.id, stderr);
            return Ok(ChunkResult {
                chunk_id: chunk.id.clone(),
                success: false,
                findings: vec![],
                sub_results: vec![],
                depth,
                tokens_used: chunk.token_count,
                error: Some(stderr.to_string()),
            });
        }

        let response = String::from_utf8_lossy(&output.stdout);
        let findings = self.parse_findings(&response, chunk)?;

        Ok(ChunkResult {
            chunk_id: chunk.id.clone(),
            success: true,
            findings,
            sub_results: vec![],
            depth,
            tokens_used: chunk.token_count,
            error: None,
        })
    }

    /// Parse findings from Claude response
    fn parse_findings(&self, response: &str, chunk: &Chunk) -> Result<Vec<Finding>> {
        // Extract JSON from response (handle markdown code blocks)
        let json_str = Self::extract_json(response);

        if json_str.is_empty() {
            return Ok(vec![]);
        }

        #[derive(serde::Deserialize)]
        struct Response {
            relevant: bool,
            findings: Vec<RawFinding>,
        }

        #[derive(serde::Deserialize)]
        struct RawFinding {
            content: String,
            #[serde(rename = "type")]
            finding_type: String,
            confidence: f32,
        }

        let parsed: Response = match serde_json::from_str(json_str) {
            Ok(p) => p,
            Err(e) => {
                debug!("Failed to parse response JSON: {}", e);
                return Ok(vec![]);
            }
        };

        if !parsed.relevant {
            return Ok(vec![]);
        }

        Ok(parsed
            .findings
            .into_iter()
            .map(|f| Finding {
                content: f.content,
                relevance: f.confidence,
                source_chunk_id: chunk.id.clone(),
                source_path: chunk.source_path.clone(),
                line_range: chunk.line_range,
                finding_type: match f.finding_type.as_str() {
                    "answer" => FindingType::Answer,
                    "evidence" => FindingType::Evidence,
                    _ => FindingType::Related,
                },
                confidence: f.confidence,
            })
            .collect())
    }

    /// Extract JSON from Claude's response (handles markdown code blocks)
    fn extract_json(response: &str) -> &str {
        // Try to find JSON in ```json blocks first
        if let Some(start) = response.find("```json") {
            let content_start = start + 7;
            if let Some(end) = response[content_start..].find("```") {
                return response[content_start..content_start + end].trim();
            }
        }

        // Try plain code blocks
        if let Some(start) = response.find("```") {
            let content_start = start + 3;
            // Skip language identifier if present
            let content_start = if let Some(newline) = response[content_start..].find('\n') {
                content_start + newline + 1
            } else {
                content_start
            };
            if let Some(end) = response[content_start..].find("```") {
                return response[content_start..content_start + end].trim();
            }
        }

        // Try to find raw JSON
        if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                return &response[start..=end];
            }
        }

        ""
    }

    /// Aggregate all results into final answer
    fn aggregate_results(&self, query: &str, duration_ms: u64) -> Result<RlmResult> {
        let all_findings: Vec<&Finding> = self
            .results
            .iter()
            .flat_map(|r| r.findings.iter())
            .collect();

        if all_findings.is_empty() {
            return Ok(RlmResult {
                success: false,
                answer: format!("No relevant information found for query: {}", query),
                key_insights: vec![],
                evidence: vec![],
                chunks_explored: self.explored.len(),
                max_depth_reached: self.results.iter().map(|r| r.depth).max().unwrap_or(0),
                total_tokens: self.results.iter().map(|r| r.tokens_used).sum(),
                confidence: 0.0,
                duration_ms,
                error: None,
            });
        }

        // Sort by confidence
        let mut sorted: Vec<_> = all_findings.clone();
        sorted.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Build answer from top findings
        let answers: Vec<_> = sorted
            .iter()
            .filter(|f| f.finding_type == FindingType::Answer)
            .take(3)
            .collect();

        let answer = if answers.is_empty() {
            sorted.first().map(|f| f.content.clone()).unwrap_or_default()
        } else {
            answers
                .iter()
                .map(|f| f.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        };

        let evidence: Vec<Evidence> = sorted
            .iter()
            .filter(|f| f.source_path.is_some())
            .take(5)
            .map(|f| Evidence {
                content: f.content.chars().take(200).collect(),
                source_path: f.source_path.clone().unwrap(),
                line_range: f.line_range,
                relevance: f.relevance,
            })
            .collect();

        let avg_confidence =
            all_findings.iter().map(|f| f.confidence).sum::<f32>() / all_findings.len() as f32;

        let key_insights: Vec<String> = sorted
            .iter()
            .take(5)
            .map(|f| f.content.chars().take(100).collect())
            .collect();

        Ok(RlmResult {
            success: true,
            answer,
            key_insights,
            evidence,
            chunks_explored: self.explored.len(),
            max_depth_reached: self.results.iter().map(|r| r.depth).max().unwrap_or(0),
            total_tokens: self.results.iter().map(|r| r.tokens_used).sum(),
            confidence: avg_confidence,
            duration_ms,
            error: None,
        })
    }

    /// Get the number of chunks explored so far
    pub fn chunks_explored(&self) -> usize {
        self.explored.len()
    }

    /// Get the collected results
    pub fn results(&self) -> &[ChunkResult] {
        &self.results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_executor_creation() {
        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, PathBuf::from("."));

        assert_eq!(executor.chunks_explored(), 0);
        assert!(executor.results().is_empty());
    }

    #[test]
    fn test_extract_json_code_block() {
        let response = r#"Here's the result:
```json
{"relevant": true, "findings": []}
```
"#;
        let json = RlmExecutor::extract_json(response);
        assert!(json.starts_with('{'));
        assert!(json.contains("relevant"));
    }

    #[test]
    fn test_extract_json_plain_code_block() {
        let response = r#"```
{"relevant": false, "findings": []}
```"#;
        let json = RlmExecutor::extract_json(response);
        assert!(json.starts_with('{'));
    }

    #[test]
    fn test_extract_json_raw() {
        let response = r#"{"relevant": true, "findings": []}"#;
        let json = RlmExecutor::extract_json(response);
        assert_eq!(json, response);
    }

    #[test]
    fn test_extract_json_empty() {
        let response = "No JSON here";
        let json = RlmExecutor::extract_json(response);
        assert!(json.is_empty());
    }

    #[test]
    fn test_parse_findings_empty() {
        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, PathBuf::from("."));

        let chunk = Chunk::new("test", "content");
        let response = r#"{"relevant": false, "findings": []}"#;

        let findings = executor.parse_findings(response, &chunk).unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_findings_with_results() {
        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, PathBuf::from("."));

        let chunk = Chunk::new("test", "content");
        let response =
            r#"{"relevant": true, "findings": [{"content": "Found it", "type": "answer", "confidence": 0.9}]}"#;

        let findings = executor.parse_findings(response, &chunk).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].finding_type, FindingType::Answer);
        assert!((findings[0].confidence - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_findings_evidence_type() {
        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, PathBuf::from("."));

        let chunk = Chunk::new("test", "content");
        let response = r#"{"relevant": true, "findings": [{"content": "Support", "type": "evidence", "confidence": 0.7}]}"#;

        let findings = executor.parse_findings(response, &chunk).unwrap();
        assert_eq!(findings[0].finding_type, FindingType::Evidence);
    }

    #[test]
    fn test_parse_findings_related_type() {
        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, PathBuf::from("."));

        let chunk = Chunk::new("test", "content");
        let response = r#"{"relevant": true, "findings": [{"content": "Related info", "type": "related", "confidence": 0.5}]}"#;

        let findings = executor.parse_findings(response, &chunk).unwrap();
        assert_eq!(findings[0].finding_type, FindingType::Related);
    }

    #[test]
    fn test_parse_findings_unknown_type() {
        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, PathBuf::from("."));

        let chunk = Chunk::new("test", "content");
        let response = r#"{"relevant": true, "findings": [{"content": "Info", "type": "unknown", "confidence": 0.5}]}"#;

        let findings = executor.parse_findings(response, &chunk).unwrap();
        assert_eq!(findings[0].finding_type, FindingType::Related);
    }

    #[test]
    fn test_parse_findings_invalid_json() {
        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, PathBuf::from("."));

        let chunk = Chunk::new("test", "content");
        let response = "not json at all";

        let findings = executor.parse_findings(response, &chunk).unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_findings_with_code_block() {
        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, PathBuf::from("."));

        let chunk = Chunk::new("test", "content");
        let response = r#"Here's my analysis:
```json
{"relevant": true, "findings": [{"content": "Found", "type": "answer", "confidence": 0.8}]}
```"#;

        let findings = executor.parse_findings(response, &chunk).unwrap();
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn test_aggregate_results_empty() {
        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, PathBuf::from("."));

        let result = executor
            .aggregate_results("test query", 1000)
            .unwrap();

        assert!(!result.success);
        assert!(result.answer.contains("No relevant information"));
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_aggregate_results_combines_findings() {
        let config = RlmConfig::default();
        let mut executor = RlmExecutor::new(config, PathBuf::from("."));

        // Simulate some results
        executor.results.push(ChunkResult {
            chunk_id: "chunk-1".to_string(),
            success: true,
            findings: vec![Finding {
                content: "First answer".to_string(),
                relevance: 0.9,
                source_chunk_id: "chunk-1".to_string(),
                source_path: Some(PathBuf::from("test.rs")),
                line_range: Some((1, 10)),
                finding_type: FindingType::Answer,
                confidence: 0.9,
            }],
            sub_results: vec![],
            depth: 0,
            tokens_used: 100,
            error: None,
        });

        executor.explored.insert("chunk-1".to_string());

        let result = executor.aggregate_results("test", 500).unwrap();

        assert!(result.success);
        assert!(result.answer.contains("First answer"));
        assert_eq!(result.chunks_explored, 1);
        assert!(!result.evidence.is_empty());
    }

    #[test]
    fn test_aggregate_multiple_answers() {
        let config = RlmConfig::default();
        let mut executor = RlmExecutor::new(config, PathBuf::from("."));

        executor.results.push(ChunkResult {
            chunk_id: "chunk-1".to_string(),
            success: true,
            findings: vec![
                Finding {
                    content: "Answer one".to_string(),
                    relevance: 0.9,
                    source_chunk_id: "chunk-1".to_string(),
                    source_path: Some(PathBuf::from("a.rs")),
                    line_range: None,
                    finding_type: FindingType::Answer,
                    confidence: 0.9,
                },
                Finding {
                    content: "Answer two".to_string(),
                    relevance: 0.8,
                    source_chunk_id: "chunk-1".to_string(),
                    source_path: Some(PathBuf::from("b.rs")),
                    line_range: None,
                    finding_type: FindingType::Answer,
                    confidence: 0.8,
                },
            ],
            sub_results: vec![],
            depth: 0,
            tokens_used: 200,
            error: None,
        });

        executor.explored.insert("chunk-1".to_string());

        let result = executor.aggregate_results("test", 300).unwrap();

        assert!(result.answer.contains("Answer one"));
        assert!(result.answer.contains("Answer two"));
    }

    #[test]
    fn test_executor_with_temp_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.rs");
        fs::write(&file, "fn main() { println!(\"hello\"); }").unwrap();

        let config = RlmConfig::default();
        let executor = RlmExecutor::new(config, temp.path().to_path_buf());

        // We can't actually execute without Claude CLI, but we can verify setup
        assert_eq!(executor.chunks_explored(), 0);
    }
}
