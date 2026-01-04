//! RLM State Persistence
//!
//! Handles saving and loading daemon state for crash recovery
//! and session continuity.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::debug;

/// Daemon statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DaemonStats {
    /// Total queries processed
    pub queries_processed: u64,

    /// Successful queries
    pub queries_succeeded: u64,

    /// Failed queries
    pub queries_failed: u64,

    /// Total tokens used
    pub total_tokens: u64,

    /// Average query duration (ms)
    pub avg_duration_ms: u64,

    /// Daemon start time
    pub started_at: u64,
}

/// Persistent state for RLM daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmState {
    /// Session identifier
    pub session_id: String,

    /// Context fingerprint for cache validation
    pub context_fingerprint: String,

    /// Daemon statistics
    pub stats: DaemonStats,

    /// Timestamp when saved
    pub saved_at: u64,
}

impl Default for RlmState {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            context_fingerprint: String::new(),
            stats: DaemonStats::default(),
            saved_at: 0,
        }
    }
}

/// Load state from file
pub fn load_state(path: &Path) -> Result<RlmState> {
    if !path.exists() {
        debug!("No state file at {}, using defaults", path.display());
        return Ok(RlmState::default());
    }

    let content = fs::read_to_string(path).context("Failed to read state file")?;

    let state: RlmState = serde_json::from_str(&content).context("Failed to parse state JSON")?;

    debug!(
        "Loaded state from {}: session={}",
        path.display(),
        state.session_id
    );
    Ok(state)
}

/// Save state to file
pub fn save_state(path: &Path, state: &RlmState) -> Result<()> {
    let json = serde_json::to_string_pretty(state).context("Failed to serialize state")?;

    fs::write(path, json).context("Failed to write state file")?;

    debug!("Saved state to {}", path.display());
    Ok(())
}

/// Checkpoint for RLM execution (per-query state)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmCheckpoint {
    /// Query being processed
    pub query: String,

    /// Chunks already explored
    pub explored_chunks: Vec<String>,

    /// Current depth
    pub current_depth: u32,

    /// Partial results
    pub partial_results: Vec<String>,

    /// Timestamp
    pub timestamp: u64,
}

impl RlmCheckpoint {
    /// Create a new checkpoint
    pub fn new(query: &str) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        Self {
            query: query.to_string(),
            explored_chunks: Vec::new(),
            current_depth: 0,
            partial_results: Vec::new(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Save checkpoint
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load checkpoint
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let checkpoint: Self = serde_json::from_str(&content)?;
        Ok(checkpoint)
    }

    /// Check if checkpoint is stale (older than 1 hour)
    pub fn is_stale(&self) -> bool {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now - self.timestamp > 3600 // 1 hour
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_daemon_stats_default() {
        let stats = DaemonStats::default();
        assert_eq!(stats.queries_processed, 0);
        assert_eq!(stats.queries_succeeded, 0);
        assert_eq!(stats.queries_failed, 0);
        assert_eq!(stats.total_tokens, 0);
        assert_eq!(stats.avg_duration_ms, 0);
        assert_eq!(stats.started_at, 0);
    }

    #[test]
    fn test_rlm_state_default() {
        let state = RlmState::default();
        assert!(state.session_id.is_empty());
        assert!(state.context_fingerprint.is_empty());
        assert_eq!(state.saved_at, 0);
    }

    #[test]
    fn test_state_roundtrip() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("state.json");

        let state = RlmState {
            session_id: "test-session".to_string(),
            context_fingerprint: "abc123".to_string(),
            stats: DaemonStats {
                queries_processed: 10,
                queries_succeeded: 8,
                queries_failed: 2,
                total_tokens: 5000,
                avg_duration_ms: 250,
                started_at: 1234567890,
            },
            saved_at: 1234567890,
        };

        save_state(&path, &state).unwrap();
        let loaded = load_state(&path).unwrap();

        assert_eq!(loaded.session_id, "test-session");
        assert_eq!(loaded.context_fingerprint, "abc123");
        assert_eq!(loaded.stats.queries_processed, 10);
        assert_eq!(loaded.stats.queries_succeeded, 8);
        assert_eq!(loaded.stats.queries_failed, 2);
    }

    #[test]
    fn test_checkpoint_new() {
        let checkpoint = RlmCheckpoint::new("test query");
        assert_eq!(checkpoint.query, "test query");
        assert!(checkpoint.explored_chunks.is_empty());
        assert_eq!(checkpoint.current_depth, 0);
        assert!(checkpoint.partial_results.is_empty());
        assert!(checkpoint.timestamp > 0);
    }

    #[test]
    fn test_checkpoint_stale() {
        let mut checkpoint = RlmCheckpoint::new("test query");
        assert!(!checkpoint.is_stale());

        // Make it old
        checkpoint.timestamp = 0;
        assert!(checkpoint.is_stale());
    }

    #[test]
    fn test_checkpoint_roundtrip() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("checkpoint.json");

        let checkpoint = RlmCheckpoint {
            query: "find auth code".to_string(),
            explored_chunks: vec!["chunk-1".to_string(), "chunk-2".to_string()],
            current_depth: 2,
            partial_results: vec!["result-1".to_string()],
            timestamp: 1234567890,
        };

        checkpoint.save(&path).unwrap();
        let loaded = RlmCheckpoint::load(&path).unwrap();

        assert_eq!(loaded.query, "find auth code");
        assert_eq!(loaded.explored_chunks.len(), 2);
        assert_eq!(loaded.current_depth, 2);
    }

    #[test]
    fn test_load_missing_state() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nonexistent.json");

        let state = load_state(&path).unwrap();
        assert!(state.session_id.is_empty());
    }

    #[test]
    fn test_state_serialization() {
        let state = RlmState {
            session_id: "test".to_string(),
            context_fingerprint: "fp".to_string(),
            stats: DaemonStats::default(),
            saved_at: 123,
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("fp"));

        let parsed: RlmState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, "test");
    }
}
