//! Parallel Exploration Engine
//!
//! Enables concurrent exploration of multiple chunks to speed up
//! RLM processing on multi-core systems.

use crate::rlm::context::RlmContext;
use crate::rlm::types::{Chunk, ChunkResult, Finding, FindingType, RlmConfig};
use anyhow::{Context, Result};
use std::sync::mpsc;
use std::thread;
use tracing::{debug, info, warn};

/// Parallel exploration configuration
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Maximum concurrent explorations
    pub max_concurrent: usize,

    /// Timeout per chunk in milliseconds
    pub chunk_timeout_ms: u64,

    /// Whether to use thread pool (vs spawning)
    pub use_thread_pool: bool,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            chunk_timeout_ms: 30_000, // 30 seconds
            use_thread_pool: true,
        }
    }
}

/// Result from parallel exploration
#[derive(Debug)]
pub struct ParallelResult {
    /// Successful chunk results
    pub results: Vec<ChunkResult>,

    /// Failed chunk IDs with errors
    pub failures: Vec<(String, String)>,

    /// Total duration in milliseconds
    pub duration_ms: u64,
}

/// Parallel exploration engine
pub struct ParallelExplorer {
    config: ParallelConfig,
    rlm_config: RlmConfig,
}

impl ParallelExplorer {
    /// Create a new parallel explorer
    pub fn new(rlm_config: RlmConfig) -> Self {
        Self {
            config: ParallelConfig::default(),
            rlm_config,
        }
    }

    /// Create with custom parallel config
    pub fn with_config(rlm_config: RlmConfig, config: ParallelConfig) -> Self {
        Self { config, rlm_config }
    }

    /// Get current config
    pub fn config(&self) -> &ParallelConfig {
        &self.config
    }

    /// Get RLM config
    pub fn rlm_config(&self) -> &RlmConfig {
        &self.rlm_config
    }

    /// Explore chunks in parallel
    pub fn explore_parallel(
        &self,
        chunks: Vec<Chunk>,
        query: &str,
        _context: &RlmContext,
        working_dir: &std::path::Path,
        current_depth: u32,
    ) -> Result<ParallelResult> {
        let start = std::time::Instant::now();

        if chunks.is_empty() {
            return Ok(ParallelResult {
                results: vec![],
                failures: vec![],
                duration_ms: 0,
            });
        }

        // If only one chunk or parallelism disabled, run sequentially
        if chunks.len() == 1 || self.config.max_concurrent <= 1 {
            return self.explore_sequential(chunks, query, working_dir, current_depth);
        }

        info!(
            "Parallel exploration of {} chunks with {} workers",
            chunks.len(),
            self.config.max_concurrent
        );

        // Channel for results
        let (tx, rx) = mpsc::channel();

        // Shared data
        let query = query.to_string();
        let working_dir = working_dir.to_path_buf();

        // Spawn threads for each chunk (up to max_concurrent)
        let mut handles = Vec::new();
        let chunk_batches = self.batch_chunks(chunks);

        for batch in chunk_batches {
            let tx = tx.clone();
            let query = query.clone();
            let working_dir = working_dir.clone();

            let handle = thread::spawn(move || {
                for chunk in batch {
                    let chunk_id = chunk.id.clone();
                    let result =
                        Self::explore_single_chunk(&chunk, &query, &working_dir, current_depth);

                    let _ = tx.send((chunk_id, result));
                }
            });

            handles.push(handle);
        }

        // Drop sender so receiver knows when done
        drop(tx);

        // Collect results
        let mut results = Vec::new();
        let mut failures = Vec::new();

        for (chunk_id, result) in rx {
            match result {
                Ok(chunk_result) => {
                    debug!("Chunk {} completed successfully", chunk_id);
                    results.push(chunk_result);
                }
                Err(e) => {
                    warn!("Chunk {} failed: {}", chunk_id, e);
                    failures.push((chunk_id, e.to_string()));
                }
            }
        }

        // Wait for all threads
        for handle in handles {
            let _ = handle.join();
        }

        Ok(ParallelResult {
            results,
            failures,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Batch chunks for parallel processing
    fn batch_chunks(&self, chunks: Vec<Chunk>) -> Vec<Vec<Chunk>> {
        let batch_size =
            (chunks.len() + self.config.max_concurrent - 1) / self.config.max_concurrent;

        chunks
            .chunks(batch_size.max(1))
            .map(|c| c.to_vec())
            .collect()
    }

    /// Explore a single chunk (called from thread)
    fn explore_single_chunk(
        chunk: &Chunk,
        query: &str,
        working_dir: &std::path::Path,
        depth: u32,
    ) -> Result<ChunkResult> {
        use std::process::Command;

        // Build prompt for chunk exploration
        let prompt = format!(
            r#"Explore this code chunk to answer the query.

QUERY: {}

CHUNK (from {}):
```
{}
```

Extract relevant findings. Respond with JSON:
{{
  "findings": [
    {{"content": "...", "relevance": 0.9, "type": "Answer|Evidence|Related"}}
  ],
  "needs_decomposition": false,
  "summary": "Brief summary of what was found"
}}"#,
            query,
            chunk
                .source_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            chunk.preview(2000)
        );

        let output = Command::new("claude")
            .args(["--print", "-p", &prompt])
            .current_dir(working_dir)
            .output()
            .context("Failed to execute claude CLI")?;

        if !output.status.success() {
            anyhow::bail!("Claude CLI failed");
        }

        let response = String::from_utf8_lossy(&output.stdout);

        // Parse response
        let json_str = Self::extract_json(&response);
        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .unwrap_or_else(|_| serde_json::json!({"findings": [], "summary": "Parse error"}));

        // Build findings
        let findings: Vec<Finding> = parsed["findings"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| {
                        let finding_type_str = f["type"].as_str().unwrap_or("Related");
                        let finding_type = match finding_type_str.to_lowercase().as_str() {
                            "answer" => FindingType::Answer,
                            "evidence" => FindingType::Evidence,
                            _ => FindingType::Related,
                        };

                        Some(Finding {
                            content: f["content"].as_str()?.to_string(),
                            relevance: f["relevance"].as_f64()? as f32,
                            source_chunk_id: chunk.id.clone(),
                            source_path: chunk.source_path.clone(),
                            line_range: chunk.line_range,
                            finding_type,
                            confidence: f["relevance"].as_f64()? as f32,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ChunkResult {
            chunk_id: chunk.id.clone(),
            success: true,
            findings,
            sub_results: vec![],
            depth,
            tokens_used: (chunk.token_count as f32 * 1.5) as u32, // Estimate
            error: None,
        })
    }

    /// Extract JSON from response
    fn extract_json(response: &str) -> &str {
        if let Some(start) = response.find("```json") {
            let content_start = start + 7;
            if let Some(end) = response[content_start..].find("```") {
                return response[content_start..content_start + end].trim();
            }
        }
        if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                return &response[start..=end];
            }
        }
        response.trim()
    }

    /// Sequential fallback
    fn explore_sequential(
        &self,
        chunks: Vec<Chunk>,
        query: &str,
        working_dir: &std::path::Path,
        depth: u32,
    ) -> Result<ParallelResult> {
        let start = std::time::Instant::now();
        let mut results = Vec::new();
        let mut failures = Vec::new();

        for chunk in chunks {
            let chunk_id = chunk.id.clone();
            match Self::explore_single_chunk(&chunk, query, working_dir, depth) {
                Ok(result) => results.push(result),
                Err(e) => failures.push((chunk_id, e.to_string())),
            }
        }

        Ok(ParallelResult {
            results,
            failures,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_config_default() {
        let config = ParallelConfig::default();
        assert_eq!(config.max_concurrent, 4);
        assert_eq!(config.chunk_timeout_ms, 30_000);
        assert!(config.use_thread_pool);
    }

    #[test]
    fn test_parallel_config_custom() {
        let config = ParallelConfig {
            max_concurrent: 8,
            chunk_timeout_ms: 60_000,
            use_thread_pool: false,
        };
        assert_eq!(config.max_concurrent, 8);
        assert!(!config.use_thread_pool);
    }

    #[test]
    fn test_parallel_explorer_creation() {
        let rlm_config = RlmConfig::default();
        let explorer = ParallelExplorer::new(rlm_config.clone());
        assert_eq!(explorer.config().max_concurrent, 4);
        assert_eq!(
            explorer.rlm_config().max_depth,
            rlm_config.max_depth
        );
    }

    #[test]
    fn test_parallel_explorer_with_config() {
        let rlm_config = RlmConfig::default();
        let parallel_config = ParallelConfig {
            max_concurrent: 8,
            ..Default::default()
        };
        let explorer = ParallelExplorer::with_config(rlm_config, parallel_config);
        assert_eq!(explorer.config().max_concurrent, 8);
    }

    #[test]
    fn test_batch_chunks_even() {
        let config = RlmConfig::default();
        let explorer = ParallelExplorer::new(config);

        let chunks: Vec<Chunk> = (0..8)
            .map(|i| Chunk::new(format!("chunk-{}", i), "content"))
            .collect();

        let batches = explorer.batch_chunks(chunks);
        assert_eq!(batches.len(), 4); // max_concurrent = 4
        assert_eq!(batches[0].len(), 2); // 8 / 4 = 2 per batch
    }

    #[test]
    fn test_batch_chunks_uneven() {
        let config = RlmConfig::default();
        let explorer = ParallelExplorer::new(config);

        let chunks: Vec<Chunk> = (0..10)
            .map(|i| Chunk::new(format!("chunk-{}", i), "content"))
            .collect();

        let batches = explorer.batch_chunks(chunks);
        assert_eq!(batches.len(), 4); // max_concurrent = 4
        // 10 / 4 = 2.5, rounded up = 3 per batch
        assert!(batches[0].len() >= 2);
    }

    #[test]
    fn test_batch_chunks_fewer_than_workers() {
        let config = RlmConfig::default();
        let explorer = ParallelExplorer::new(config);

        let chunks: Vec<Chunk> = (0..2)
            .map(|i| Chunk::new(format!("chunk-{}", i), "content"))
            .collect();

        let batches = explorer.batch_chunks(chunks);
        // With 2 chunks and 4 workers, we get 2 batches of 1 each
        assert!(batches.len() <= 2);
    }

    #[test]
    fn test_batch_chunks_empty() {
        let config = RlmConfig::default();
        let explorer = ParallelExplorer::new(config);

        let chunks: Vec<Chunk> = vec![];
        let batches = explorer.batch_chunks(chunks);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_extract_json_code_block() {
        let response = r#"Here's my analysis:
```json
{"findings": []}
```
"#;
        let json = ParallelExplorer::extract_json(response);
        assert!(json.starts_with('{'));
        assert!(json.contains("findings"));
    }

    #[test]
    fn test_extract_json_raw() {
        let response = r#"{"findings": [], "summary": "nothing"}"#;
        let json = ParallelExplorer::extract_json(response);
        assert_eq!(json, response);
    }

    #[test]
    fn test_extract_json_with_text() {
        let response = r#"I found the following:
{"findings": [{"content": "test", "relevance": 0.9, "type": "Answer"}]}
That's my analysis."#;
        let json = ParallelExplorer::extract_json(response);
        assert!(json.starts_with('{'));
        assert!(json.ends_with('}'));
    }

    #[test]
    fn test_extract_json_no_json() {
        let response = "No JSON content here";
        let json = ParallelExplorer::extract_json(response);
        assert_eq!(json, "No JSON content here");
    }

    #[test]
    fn test_parallel_result_empty() {
        let result = ParallelResult {
            results: vec![],
            failures: vec![],
            duration_ms: 0,
        };
        assert!(result.results.is_empty());
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_parallel_result_with_data() {
        let chunk_result = ChunkResult {
            chunk_id: "chunk-1".to_string(),
            success: true,
            findings: vec![],
            sub_results: vec![],
            depth: 1,
            tokens_used: 100,
            error: None,
        };
        let result = ParallelResult {
            results: vec![chunk_result],
            failures: vec![("chunk-2".to_string(), "timeout".to_string())],
            duration_ms: 1500,
        };
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.duration_ms, 1500);
    }

    #[test]
    fn test_explore_parallel_empty() {
        let config = RlmConfig::default();
        let explorer = ParallelExplorer::new(config.clone());
        let context = RlmContext::new(config);

        let result = explorer
            .explore_parallel(
                vec![],
                "test query",
                &context,
                std::path::Path::new("."),
                0,
            )
            .unwrap();

        assert!(result.results.is_empty());
        assert!(result.failures.is_empty());
        assert_eq!(result.duration_ms, 0);
    }

    #[test]
    fn test_explore_sequential_empty() {
        let config = RlmConfig::default();
        let explorer = ParallelExplorer::new(config);

        let result = explorer
            .explore_sequential(vec![], "test query", std::path::Path::new("."), 0)
            .unwrap();

        assert!(result.results.is_empty());
        assert!(result.failures.is_empty());
    }
}
