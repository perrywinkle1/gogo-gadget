//! RLM Module - Recursive Language Model Processing
//!
//! This module enables processing of inputs far exceeding context windows
//! through recursive decomposition. Based on the RLM paper (arxiv 2512.24601).
//!
//! ## Core Concepts
//!
//! - **RlmContext**: Treats large inputs as explorable environments
//! - **Navigator**: LLM-based chunk selection with embedding pre-scoring
//! - **Chunker**: Intelligent content splitting (structural, semantic, fixed)
//! - **RlmExecutor**: Main execution engine for recursive exploration
//! - **Aggregator**: Result synthesis with deduplication and conflict resolution
//! - **RlmDaemon**: Continuous mode daemon for long-running operations
//! - **Persistence**: State persistence for crash recovery
//! - **ParallelExplorer**: Concurrent chunk exploration for performance
//! - **RlmCache**: LRU caching for navigation decisions and results
//!
//! ## Usage
//!
//! ```rust,ignore
//! use gogo_gadget::rlm::{RlmConfig, RlmExecutor};
//!
//! let config = RlmConfig::default();
//! let mut executor = RlmExecutor::new(config, working_dir);
//! let result = executor.execute("Find auth code", context_path)?;
//! ```

pub mod aggregator;
pub mod cache;
pub mod chunker;
pub mod context;
pub mod daemon;
pub mod embeddings;
pub mod executor;
pub mod navigator;
pub mod parallel;
pub mod persistence;
pub mod types;

// Re-exports
pub use aggregator::{AggregationStrategy, Aggregator};
pub use cache::{CacheConfig, CacheStats, RlmCache};
pub use chunker::{ChunkStrategy, ContextChunker, DefaultChunker};
pub use context::RlmContext;
pub use daemon::{RlmDaemon, RlmQuery, RlmResponse};
pub use embeddings::{get_default_embedder, EmbeddingProvider};
pub use executor::RlmExecutor;
pub use navigator::{NavigationContext, NavigationDecision, NavigationEngine, Navigator};
pub use parallel::{ParallelConfig, ParallelExplorer, ParallelResult};
pub use persistence::{load_state, save_state, DaemonStats, RlmCheckpoint, RlmState};
pub use types::*;
