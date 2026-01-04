//! RLM Continuous Mode Daemon
//!
//! Long-running daemon that watches for queries via signal files
//! and maintains persistent state across restarts.

use crate::rlm::context::RlmContext;
use crate::rlm::executor::RlmExecutor;
use crate::rlm::persistence::{load_state, save_state, DaemonStats, RlmState};
use crate::rlm::types::{RlmConfig, RlmResult};
use crate::ShutdownSignal;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// Query submitted via signal file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmQuery {
    /// The query text
    pub query: String,

    /// Optional context path override
    pub context_path: Option<PathBuf>,

    /// Optional depth override
    pub max_depth: Option<u32>,

    /// Query ID for tracking
    pub id: String,

    /// Submission timestamp
    pub submitted_at: u64,
}

/// Response written to signal file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmResponse {
    /// Query ID this responds to
    pub query_id: String,

    /// The result
    pub result: RlmResult,

    /// Processing duration in milliseconds
    pub duration_ms: u64,

    /// Completion timestamp
    pub completed_at: u64,
}

/// Query queue for rate limiting
struct QueryQueue {
    queries: VecDeque<Instant>,
    limit_per_minute: u32,
}

impl QueryQueue {
    fn new(limit_per_minute: u32) -> Self {
        Self {
            queries: VecDeque::new(),
            limit_per_minute,
        }
    }

    /// Check if a new query is allowed
    fn can_query(&mut self) -> bool {
        let now = Instant::now();
        let one_minute_ago = now - Duration::from_secs(60);

        // Remove old entries
        while let Some(&front) = self.queries.front() {
            if front < one_minute_ago {
                self.queries.pop_front();
            } else {
                break;
            }
        }

        self.queries.len() < self.limit_per_minute as usize
    }

    /// Record a query
    fn record_query(&mut self) {
        self.queries.push_back(Instant::now());
    }
}

/// RLM Continuous Mode Daemon
pub struct RlmDaemon {
    /// Configuration
    config: RlmConfig,

    /// Working directory
    working_dir: PathBuf,

    /// Default context path
    context_path: PathBuf,

    /// Signal file for queries
    query_file: PathBuf,

    /// Signal file for responses
    response_file: PathBuf,

    /// State file
    state_file: PathBuf,

    /// Shutdown signal
    shutdown_signal: ShutdownSignal,

    /// Query rate limiter
    rate_limiter: QueryQueue,

    /// Statistics
    stats: DaemonStats,

    /// Cached context (to avoid re-loading)
    cached_context: Option<RlmContext>,

    /// Context fingerprint for cache invalidation
    context_fingerprint: Option<String>,
}

impl RlmDaemon {
    /// Create a new daemon
    pub fn new(
        config: RlmConfig,
        working_dir: PathBuf,
        context_path: PathBuf,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        let query_file = working_dir.join(".gogo-gadget-rlm-query");
        let response_file = working_dir.join(".gogo-gadget-rlm-response");
        let state_file = working_dir.join(".gogo-gadget-rlm-state");

        Self {
            rate_limiter: QueryQueue::new(config.rate_limit_per_minute),
            config,
            working_dir,
            context_path,
            query_file,
            response_file,
            state_file,
            shutdown_signal,
            stats: DaemonStats::default(),
            cached_context: None,
            context_fingerprint: None,
        }
    }

    /// Run the daemon loop
    pub fn run(&mut self) -> Result<()> {
        info!("Starting RLM daemon");
        info!("Query file: {}", self.query_file.display());
        info!("Response file: {}", self.response_file.display());
        info!("Context path: {}", self.context_path.display());

        // Load existing state
        if let Ok(state) = load_state(&self.state_file) {
            self.stats = state.stats;
            info!(
                "Loaded state: {} queries processed",
                self.stats.queries_processed
            );
        }

        self.stats.started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Pre-load context
        self.reload_context()?;

        // Main loop
        loop {
            // Check shutdown
            if self.shutdown_signal.is_shutdown() {
                info!("Shutdown signal received");
                break;
            }

            // Check for blocked file
            if self.working_dir.join(".gogo-gadget-blocked").exists() {
                info!("Blocked file detected, exiting");
                break;
            }

            // Check for query
            if self.query_file.exists() {
                match self.process_query_file() {
                    Ok(()) => {}
                    Err(e) => {
                        error!("Failed to process query: {}", e);
                        self.stats.queries_failed += 1;
                    }
                }
            }

            // Save state periodically
            if self.stats.queries_processed % 10 == 0 && self.stats.queries_processed > 0 {
                if let Err(e) = self.save_current_state() {
                    warn!("Failed to save state: {}", e);
                }
            }

            // Sleep before next poll
            std::thread::sleep(Duration::from_millis(500));
        }

        // Final state save
        self.save_current_state()?;
        info!(
            "RLM daemon stopped. Total queries: {}",
            self.stats.queries_processed
        );

        Ok(())
    }

    /// Process the query file
    fn process_query_file(&mut self) -> Result<()> {
        // Read and remove query file
        let content = fs::read_to_string(&self.query_file).context("Failed to read query file")?;
        fs::remove_file(&self.query_file)?;

        // Parse query
        let query: RlmQuery =
            serde_json::from_str(&content).context("Failed to parse query JSON")?;

        info!("Processing query: {} (id: {})", query.query, query.id);

        // Rate limit check
        if !self.rate_limiter.can_query() {
            warn!("Rate limit exceeded, rejecting query {}", query.id);
            let response = RlmResponse {
                query_id: query.id,
                result: RlmResult {
                    success: false,
                    answer: "Rate limit exceeded. Try again later.".to_string(),
                    key_insights: vec![],
                    evidence: vec![],
                    chunks_explored: 0,
                    max_depth_reached: 0,
                    total_tokens: 0,
                    confidence: 0.0,
                    duration_ms: 0,
                    error: Some("Rate limit exceeded".to_string()),
                },
                duration_ms: 0,
                completed_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };
            self.write_response(&response)?;
            return Ok(());
        }

        self.rate_limiter.record_query();

        // Execute query
        let start = Instant::now();
        let result = self.execute_query(&query);
        let duration_ms = start.elapsed().as_millis() as u64;

        // Update stats
        self.stats.queries_processed += 1;
        match &result {
            Ok(r) if r.success => {
                self.stats.queries_succeeded += 1;
                self.stats.total_tokens += r.total_tokens as u64;
            }
            _ => {
                self.stats.queries_failed += 1;
            }
        }

        // Calculate running average
        let total = self.stats.queries_processed;
        if total > 0 {
            self.stats.avg_duration_ms =
                (self.stats.avg_duration_ms * (total - 1) + duration_ms) / total;
        }

        // Write response
        let response = RlmResponse {
            query_id: query.id,
            result: result.unwrap_or_else(|e| RlmResult {
                success: false,
                answer: format!("Error: {}", e),
                key_insights: vec![],
                evidence: vec![],
                chunks_explored: 0,
                max_depth_reached: 0,
                total_tokens: 0,
                confidence: 0.0,
                duration_ms,
                error: Some(e.to_string()),
            }),
            duration_ms,
            completed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        self.write_response(&response)?;
        Ok(())
    }

    /// Execute a single query
    fn execute_query(&mut self, query: &RlmQuery) -> Result<RlmResult> {
        // Use query-specific context path if provided, clone to avoid borrow issues
        let context_path = query
            .context_path
            .clone()
            .unwrap_or_else(|| self.context_path.clone());

        // Check if context needs reloading
        let fingerprint = self.compute_context_fingerprint(&context_path)?;
        if self.context_fingerprint.as_ref() != Some(&fingerprint) {
            info!("Context changed, reloading");
            self.reload_context_from(&context_path)?;
            self.context_fingerprint = Some(fingerprint);
        }

        let context = self
            .cached_context
            .as_ref()
            .context("No context loaded")?;

        // Build config with overrides
        let mut config = self.config.clone();
        if let Some(depth) = query.max_depth {
            config.max_depth = depth;
        }

        // Execute
        let mut executor = RlmExecutor::new(config, self.working_dir.clone());
        executor.execute_on_context(&query.query, context)
    }

    /// Reload context from default path
    fn reload_context(&mut self) -> Result<()> {
        self.reload_context_from(&self.context_path.clone())
    }

    /// Reload context from specific path
    fn reload_context_from(&mut self, path: &Path) -> Result<()> {
        info!("Loading context from: {}", path.display());

        let context = if path.is_dir() {
            RlmContext::from_directory(path, self.config.clone())?
        } else {
            RlmContext::from_file(path, self.config.clone())?
        };

        self.cached_context = Some(context);
        self.context_fingerprint = Some(self.compute_context_fingerprint(path)?);

        Ok(())
    }

    /// Compute a fingerprint for cache invalidation
    fn compute_context_fingerprint(&self, path: &Path) -> Result<String> {
        let mut hasher = DefaultHasher::new();

        if path.is_file() {
            let meta = fs::metadata(path)?;
            meta.len().hash(&mut hasher);
            if let Ok(modified) = meta.modified() {
                modified
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .hash(&mut hasher);
            }
        } else if path.is_dir() {
            // Hash directory structure (simplified)
            let mut count = 0u64;
            let mut total_size = 0u64;
            for entry in walkdir::WalkDir::new(path)
                .max_depth(4)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    count += 1;
                    if let Ok(meta) = entry.metadata() {
                        total_size += meta.len();
                    }
                }
            }
            count.hash(&mut hasher);
            total_size.hash(&mut hasher);
        }

        Ok(format!("{:x}", hasher.finish()))
    }

    /// Write response to signal file
    fn write_response(&self, response: &RlmResponse) -> Result<()> {
        let json = serde_json::to_string_pretty(response)?;
        fs::write(&self.response_file, json)?;
        debug!("Wrote response for query {}", response.query_id);
        Ok(())
    }

    /// Save current state
    fn save_current_state(&self) -> Result<()> {
        let state = RlmState {
            session_id: format!("daemon-{}", self.stats.started_at),
            context_fingerprint: self.context_fingerprint.clone().unwrap_or_default(),
            stats: self.stats.clone(),
            saved_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        save_state(&self.state_file, &state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_queue_rate_limit() {
        let mut queue = QueryQueue::new(5);

        // Should allow first 5 queries
        for _ in 0..5 {
            assert!(queue.can_query());
            queue.record_query();
        }

        // 6th should be blocked
        assert!(!queue.can_query());
    }

    #[test]
    fn test_query_queue_empty() {
        let mut queue = QueryQueue::new(10);
        assert!(queue.can_query());
    }

    #[test]
    fn test_query_parsing() {
        let json = r#"{
            "query": "find auth code",
            "id": "q-123",
            "submitted_at": 1234567890
        }"#;

        let query: RlmQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.query, "find auth code");
        assert_eq!(query.id, "q-123");
        assert!(query.context_path.is_none());
        assert!(query.max_depth.is_none());
    }

    #[test]
    fn test_query_with_overrides() {
        let json = r#"{
            "query": "search code",
            "id": "q-456",
            "submitted_at": 1234567890,
            "context_path": "/tmp/src",
            "max_depth": 3
        }"#;

        let query: RlmQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.context_path, Some(PathBuf::from("/tmp/src")));
        assert_eq!(query.max_depth, Some(3));
    }

    #[test]
    fn test_response_serialization() {
        let response = RlmResponse {
            query_id: "q-123".to_string(),
            result: RlmResult {
                success: true,
                answer: "Found auth code".to_string(),
                key_insights: vec!["Uses JWT".to_string()],
                evidence: vec![],
                chunks_explored: 5,
                max_depth_reached: 2,
                total_tokens: 1000,
                confidence: 0.9,
                duration_ms: 500,
                error: None,
            },
            duration_ms: 500,
            completed_at: 1234567890,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("q-123"));
        assert!(json.contains("Found auth code"));
        assert!(json.contains("Uses JWT"));
    }

    #[test]
    fn test_response_with_error() {
        let response = RlmResponse {
            query_id: "q-err".to_string(),
            result: RlmResult {
                success: false,
                answer: String::new(),
                key_insights: vec![],
                evidence: vec![],
                chunks_explored: 0,
                max_depth_reached: 0,
                total_tokens: 0,
                confidence: 0.0,
                duration_ms: 100,
                error: Some("Rate limit exceeded".to_string()),
            },
            duration_ms: 100,
            completed_at: 1234567890,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Rate limit exceeded"));
        assert!(json.contains("\"success\":false"));
    }

    #[test]
    fn test_rlm_query_default_values() {
        let query = RlmQuery {
            query: "test".to_string(),
            context_path: None,
            max_depth: None,
            id: "test-id".to_string(),
            submitted_at: 0,
        };
        assert!(query.context_path.is_none());
        assert!(query.max_depth.is_none());
    }
}
