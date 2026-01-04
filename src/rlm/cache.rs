//! RLM Caching System
//!
//! LRU caching for navigation decisions, embeddings, and chunk results
//! to avoid redundant LLM calls.

use crate::rlm::navigator::NavigationDecision;
use crate::rlm::types::ChunkResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Cache entry with expiration
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    value: T,
    created_at: Instant,
    ttl: Duration,
}

impl<T> CacheEntry<T> {
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            created_at: Instant::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum navigation decisions to cache
    pub max_navigation_entries: usize,

    /// Maximum embeddings to cache
    pub max_embedding_entries: usize,

    /// Maximum chunk results to cache
    pub max_result_entries: usize,

    /// TTL for navigation decisions (default: 15 minutes)
    pub navigation_ttl: Duration,

    /// TTL for embeddings (default: 1 hour)
    pub embedding_ttl: Duration,

    /// TTL for chunk results (default: 5 minutes)
    pub result_ttl: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_navigation_entries: 100,
            max_embedding_entries: 500,
            max_result_entries: 200,
            navigation_ttl: Duration::from_secs(15 * 60),
            embedding_ttl: Duration::from_secs(60 * 60),
            result_ttl: Duration::from_secs(5 * 60),
        }
    }
}

/// Simple LRU cache with capacity limit
struct LruCache<K, V> {
    map: HashMap<K, (V, usize)>,
    order: Vec<K>,
    capacity: usize,
    counter: usize,
}

impl<K: std::hash::Hash + Eq + Clone, V> LruCache<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::with_capacity(capacity),
            capacity,
            counter: 0,
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        if let Some((value, order)) = self.map.get_mut(key) {
            self.counter += 1;
            *order = self.counter;
            Some(value)
        } else {
            None
        }
    }

    fn put(&mut self, key: K, value: V) {
        self.counter += 1;

        if self.map.contains_key(&key) {
            self.map.insert(key, (value, self.counter));
            return;
        }

        // Evict if at capacity
        if self.map.len() >= self.capacity {
            self.evict_lru();
        }

        self.order.push(key.clone());
        self.map.insert(key, (value, self.counter));
    }

    fn evict_lru(&mut self) {
        if self.map.is_empty() {
            return;
        }

        // Find the key with the lowest order number
        let mut min_order = usize::MAX;
        let mut min_key: Option<K> = None;

        for (k, (_, order)) in &self.map {
            if *order < min_order {
                min_order = *order;
                min_key = Some(k.clone());
            }
        }

        if let Some(key) = min_key {
            self.map.remove(&key);
            self.order.retain(|k| k != &key);
        }
    }

    #[allow(dead_code)]
    fn pop(&mut self, key: &K) {
        self.map.remove(key);
        self.order.retain(|k| k != key);
    }

    fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.counter = 0;
    }

    fn len(&self) -> usize {
        self.map.len()
    }

    fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter().map(|(k, (v, _))| (k, v))
    }
}

/// RLM Cache
pub struct RlmCache {
    /// Navigation decision cache
    navigation: RwLock<LruCache<String, CacheEntry<NavigationDecision>>>,

    /// Embedding cache (chunk_id -> embedding)
    embeddings: RwLock<LruCache<String, CacheEntry<Vec<f32>>>>,

    /// Chunk result cache
    results: RwLock<LruCache<String, CacheEntry<ChunkResult>>>,

    /// Configuration
    config: CacheConfig,

    /// Statistics
    stats: RwLock<CacheStats>,
}

/// Cache statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub navigation_hits: u64,
    pub navigation_misses: u64,
    pub embedding_hits: u64,
    pub embedding_misses: u64,
    pub result_hits: u64,
    pub result_misses: u64,
}

impl CacheStats {
    pub fn hit_rate(&self, cache_type: &str) -> f64 {
        let (hits, misses) = match cache_type {
            "navigation" => (self.navigation_hits, self.navigation_misses),
            "embedding" => (self.embedding_hits, self.embedding_misses),
            "result" => (self.result_hits, self.result_misses),
            _ => return 0.0,
        };
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    pub fn total_hits(&self) -> u64 {
        self.navigation_hits + self.embedding_hits + self.result_hits
    }

    pub fn total_misses(&self) -> u64 {
        self.navigation_misses + self.embedding_misses + self.result_misses
    }

    pub fn overall_hit_rate(&self) -> f64 {
        let total = self.total_hits() + self.total_misses();
        if total == 0 {
            0.0
        } else {
            self.total_hits() as f64 / total as f64
        }
    }
}

impl RlmCache {
    /// Create a new cache with default config
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            navigation: RwLock::new(LruCache::new(config.max_navigation_entries)),
            embeddings: RwLock::new(LruCache::new(config.max_embedding_entries)),
            results: RwLock::new(LruCache::new(config.max_result_entries)),
            config,
            stats: RwLock::new(CacheStats::default()),
        }
    }

    /// Get cache config
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    /// Generate cache key for navigation
    pub fn navigation_key(query: &str, chunk_ids: &[String], depth: u32) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        query.hash(&mut hasher);
        for id in chunk_ids {
            id.hash(&mut hasher);
        }
        depth.hash(&mut hasher);
        format!("nav:{:x}", hasher.finish())
    }

    /// Get cached navigation decision
    pub fn get_navigation(&self, key: &str) -> Option<NavigationDecision> {
        let mut cache = self.navigation.write().ok()?;
        let mut stats = self.stats.write().ok()?;

        if let Some(entry) = cache.get(&key.to_string()) {
            if !entry.is_expired() {
                stats.navigation_hits += 1;
                debug!("Navigation cache hit: {}", key);
                return Some(entry.value.clone());
            }
        }

        stats.navigation_misses += 1;
        None
    }

    /// Cache navigation decision
    pub fn put_navigation(&self, key: String, decision: NavigationDecision) {
        if let Ok(mut cache) = self.navigation.write() {
            let entry = CacheEntry::new(decision, self.config.navigation_ttl);
            cache.put(key, entry);
        }
    }

    /// Get cached embedding
    pub fn get_embedding(&self, chunk_id: &str) -> Option<Vec<f32>> {
        let mut cache = self.embeddings.write().ok()?;
        let mut stats = self.stats.write().ok()?;

        if let Some(entry) = cache.get(&chunk_id.to_string()) {
            if !entry.is_expired() {
                stats.embedding_hits += 1;
                return Some(entry.value.clone());
            }
        }

        stats.embedding_misses += 1;
        None
    }

    /// Cache embedding
    pub fn put_embedding(&self, chunk_id: String, embedding: Vec<f32>) {
        if let Ok(mut cache) = self.embeddings.write() {
            let entry = CacheEntry::new(embedding, self.config.embedding_ttl);
            cache.put(chunk_id, entry);
        }
    }

    /// Get cached chunk result
    pub fn get_result(&self, query: &str, chunk_id: &str) -> Option<ChunkResult> {
        let key = format!("{}:{}", query, chunk_id);
        let mut cache = self.results.write().ok()?;
        let mut stats = self.stats.write().ok()?;

        if let Some(entry) = cache.get(&key) {
            if !entry.is_expired() {
                stats.result_hits += 1;
                return Some(entry.value.clone());
            }
        }

        stats.result_misses += 1;
        None
    }

    /// Cache chunk result
    pub fn put_result(&self, query: &str, chunk_id: &str, result: ChunkResult) {
        let key = format!("{}:{}", query, chunk_id);
        if let Ok(mut cache) = self.results.write() {
            let entry = CacheEntry::new(result, self.config.result_ttl);
            cache.put(key, entry);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.stats.read().map(|s| s.clone()).unwrap_or_default()
    }

    /// Get cache sizes
    pub fn sizes(&self) -> (usize, usize, usize) {
        let nav_size = self.navigation.read().map(|c| c.len()).unwrap_or(0);
        let emb_size = self.embeddings.read().map(|c| c.len()).unwrap_or(0);
        let res_size = self.results.read().map(|c| c.len()).unwrap_or(0);
        (nav_size, emb_size, res_size)
    }

    /// Clear all caches
    pub fn clear(&self) {
        if let Ok(mut cache) = self.navigation.write() {
            cache.clear();
        }
        if let Ok(mut cache) = self.embeddings.write() {
            cache.clear();
        }
        if let Ok(mut cache) = self.results.write() {
            cache.clear();
        }
        if let Ok(mut stats) = self.stats.write() {
            *stats = CacheStats::default();
        }
        info!("Cache cleared");
    }

    /// Evict expired entries
    pub fn evict_expired(&self) -> usize {
        let mut evicted = 0;

        if let Ok(mut cache) = self.navigation.write() {
            let expired: Vec<_> = cache
                .iter()
                .filter(|(_, v)| v.is_expired())
                .map(|(k, _)| k.clone())
                .collect();
            for key in expired {
                cache.pop(&key);
                evicted += 1;
            }
        }

        if let Ok(mut cache) = self.embeddings.write() {
            let expired: Vec<_> = cache
                .iter()
                .filter(|(_, v)| v.is_expired())
                .map(|(k, _)| k.clone())
                .collect();
            for key in expired {
                cache.pop(&key);
                evicted += 1;
            }
        }

        if let Ok(mut cache) = self.results.write() {
            let expired: Vec<_> = cache
                .iter()
                .filter(|(_, v)| v.is_expired())
                .map(|(k, _)| k.clone())
                .collect();
            for key in expired {
                cache.pop(&key);
                evicted += 1;
            }
        }

        if evicted > 0 {
            debug!("Evicted {} expired cache entries", evicted);
        }

        evicted
    }
}

impl Default for RlmCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.max_navigation_entries, 100);
        assert_eq!(config.max_embedding_entries, 500);
        assert_eq!(config.max_result_entries, 200);
        assert_eq!(config.navigation_ttl, Duration::from_secs(15 * 60));
        assert_eq!(config.embedding_ttl, Duration::from_secs(60 * 60));
        assert_eq!(config.result_ttl, Duration::from_secs(5 * 60));
    }

    #[test]
    fn test_cache_config_custom() {
        let config = CacheConfig {
            max_navigation_entries: 50,
            max_embedding_entries: 250,
            max_result_entries: 100,
            navigation_ttl: Duration::from_secs(60),
            embedding_ttl: Duration::from_secs(120),
            result_ttl: Duration::from_secs(30),
        };
        assert_eq!(config.max_navigation_entries, 50);
        assert_eq!(config.result_ttl, Duration::from_secs(30));
    }

    #[test]
    fn test_navigation_key_generation() {
        let key1 = RlmCache::navigation_key("query", &["chunk-1".to_string()], 1);
        let key2 = RlmCache::navigation_key("query", &["chunk-1".to_string()], 1);
        let key3 = RlmCache::navigation_key("different", &["chunk-1".to_string()], 1);

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert!(key1.starts_with("nav:"));
    }

    #[test]
    fn test_navigation_key_depth_matters() {
        let key1 = RlmCache::navigation_key("query", &["chunk-1".to_string()], 1);
        let key2 = RlmCache::navigation_key("query", &["chunk-1".to_string()], 2);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_navigation_key_chunks_matter() {
        let key1 = RlmCache::navigation_key("query", &["chunk-1".to_string()], 1);
        let key2 =
            RlmCache::navigation_key("query", &["chunk-1".to_string(), "chunk-2".to_string()], 1);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_navigation_cache() {
        let cache = RlmCache::new();
        let key = RlmCache::navigation_key("test query", &["chunk-1".to_string()], 1);

        // Miss
        assert!(cache.get_navigation(&key).is_none());

        // Put
        let decision = NavigationDecision {
            selected_chunks: vec!["chunk-1".to_string()],
            reasoning: "test".to_string(),
            parallel: false,
            exploration_complete: false,
            decomposition_hints: vec![],
        };
        cache.put_navigation(key.clone(), decision.clone());

        // Hit
        let cached = cache.get_navigation(&key).unwrap();
        assert_eq!(cached.selected_chunks, decision.selected_chunks);
        assert_eq!(cached.reasoning, "test");
    }

    #[test]
    fn test_embedding_cache() {
        let cache = RlmCache::new();

        // Miss
        assert!(cache.get_embedding("chunk-1").is_none());

        // Put
        let embedding = vec![0.1, 0.2, 0.3];
        cache.put_embedding("chunk-1".to_string(), embedding.clone());

        // Hit
        let cached = cache.get_embedding("chunk-1").unwrap();
        assert_eq!(cached, embedding);
    }

    #[test]
    fn test_result_cache() {
        let cache = RlmCache::new();

        // Miss
        assert!(cache.get_result("query", "chunk-1").is_none());

        // Put
        let result = ChunkResult {
            chunk_id: "chunk-1".to_string(),
            success: true,
            findings: vec![],
            sub_results: vec![],
            depth: 1,
            tokens_used: 100,
            error: None,
        };
        cache.put_result("query", "chunk-1", result.clone());

        // Hit
        let cached = cache.get_result("query", "chunk-1").unwrap();
        assert_eq!(cached.chunk_id, "chunk-1");
        assert!(cached.success);
    }

    #[test]
    fn test_cache_stats() {
        let cache = RlmCache::new();

        // Generate some hits and misses
        cache.get_navigation("miss1");
        cache.get_navigation("miss2");

        let decision = NavigationDecision {
            selected_chunks: vec![],
            reasoning: "test".to_string(),
            parallel: false,
            exploration_complete: false,
            decomposition_hints: vec![],
        };
        cache.put_navigation("hit".to_string(), decision);
        cache.get_navigation("hit");

        let stats = cache.stats();
        assert_eq!(stats.navigation_misses, 2);
        assert_eq!(stats.navigation_hits, 1);
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let mut stats = CacheStats::default();
        stats.navigation_hits = 3;
        stats.navigation_misses = 7;

        let rate = stats.hit_rate("navigation");
        assert!((rate - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_hit_rate_zero() {
        let stats = CacheStats::default();
        assert_eq!(stats.hit_rate("navigation"), 0.0);
        assert_eq!(stats.hit_rate("embedding"), 0.0);
        assert_eq!(stats.hit_rate("result"), 0.0);
        assert_eq!(stats.hit_rate("unknown"), 0.0);
    }

    #[test]
    fn test_cache_stats_totals() {
        let mut stats = CacheStats::default();
        stats.navigation_hits = 10;
        stats.embedding_hits = 20;
        stats.result_hits = 30;
        stats.navigation_misses = 5;
        stats.embedding_misses = 10;
        stats.result_misses = 15;

        assert_eq!(stats.total_hits(), 60);
        assert_eq!(stats.total_misses(), 30);
        assert!((stats.overall_hit_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_cache_expiration() {
        let config = CacheConfig {
            navigation_ttl: Duration::from_millis(10),
            ..Default::default()
        };
        let cache = RlmCache::with_config(config);

        let decision = NavigationDecision {
            selected_chunks: vec![],
            reasoning: "test".to_string(),
            parallel: false,
            exploration_complete: false,
            decomposition_hints: vec![],
        };
        cache.put_navigation("key".to_string(), decision);

        // Should hit immediately
        assert!(cache.get_navigation("key").is_some());

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(20));

        // Should miss after expiration
        assert!(cache.get_navigation("key").is_none());
    }

    #[test]
    fn test_cache_clear() {
        let cache = RlmCache::new();

        // Add some data
        let decision = NavigationDecision {
            selected_chunks: vec![],
            reasoning: "test".to_string(),
            parallel: false,
            exploration_complete: false,
            decomposition_hints: vec![],
        };
        cache.put_navigation("key".to_string(), decision);
        cache.put_embedding("chunk-1".to_string(), vec![0.1, 0.2]);

        // Verify data exists
        assert!(cache.get_navigation("key").is_some());

        // Clear
        cache.clear();

        // Verify empty
        // Note: get_navigation after clear will miss, but we can check sizes
        let (nav, emb, res) = cache.sizes();
        assert_eq!(nav, 0);
        assert_eq!(emb, 0);
        assert_eq!(res, 0);
    }

    #[test]
    fn test_cache_sizes() {
        let cache = RlmCache::new();

        let (nav, emb, res) = cache.sizes();
        assert_eq!(nav, 0);
        assert_eq!(emb, 0);
        assert_eq!(res, 0);

        let decision = NavigationDecision {
            selected_chunks: vec![],
            reasoning: "test".to_string(),
            parallel: false,
            exploration_complete: false,
            decomposition_hints: vec![],
        };
        cache.put_navigation("key".to_string(), decision);

        let (nav, emb, res) = cache.sizes();
        assert_eq!(nav, 1);
        assert_eq!(emb, 0);
        assert_eq!(res, 0);
    }

    #[test]
    fn test_evict_expired() {
        let config = CacheConfig {
            navigation_ttl: Duration::from_millis(10),
            embedding_ttl: Duration::from_millis(10),
            result_ttl: Duration::from_millis(10),
            ..Default::default()
        };
        let cache = RlmCache::with_config(config);

        // Add entries
        let decision = NavigationDecision {
            selected_chunks: vec![],
            reasoning: "test".to_string(),
            parallel: false,
            exploration_complete: false,
            decomposition_hints: vec![],
        };
        cache.put_navigation("key1".to_string(), decision);
        cache.put_embedding("chunk-1".to_string(), vec![0.1]);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(20));

        // Evict
        let evicted = cache.evict_expired();
        assert!(evicted >= 2);
    }

    #[test]
    fn test_lru_eviction() {
        let config = CacheConfig {
            max_navigation_entries: 2,
            ..Default::default()
        };
        let cache = RlmCache::with_config(config);

        // Add 3 entries (exceeds capacity of 2)
        for i in 0..3 {
            let decision = NavigationDecision {
                selected_chunks: vec![format!("chunk-{}", i)],
                reasoning: format!("reason-{}", i),
                parallel: false,
                exploration_complete: false,
                decomposition_hints: vec![],
            };
            cache.put_navigation(format!("key-{}", i), decision);
        }

        // Only 2 should remain
        let (nav, _, _) = cache.sizes();
        assert_eq!(nav, 2);
    }

    #[test]
    fn test_cache_default() {
        let cache = RlmCache::default();
        assert_eq!(cache.config().max_navigation_entries, 100);
    }

    #[test]
    fn test_cache_entry_expiration() {
        let entry = CacheEntry::new("value", Duration::from_millis(10));
        assert!(!entry.is_expired());

        std::thread::sleep(Duration::from_millis(20));
        assert!(entry.is_expired());
    }

    #[test]
    fn test_cache_entry_no_expiration() {
        let entry = CacheEntry::new("value", Duration::from_secs(3600));
        assert!(!entry.is_expired());
    }
}
