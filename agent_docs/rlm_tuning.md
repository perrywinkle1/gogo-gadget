# RLM Performance Tuning

## Depth Configuration

| `--rlm-depth` | Use Case | Token Cost |
|---------------|----------|------------|
| 2 | Quick surface-level scan | Low |
| 3 | Standard exploration | Medium |
| 5 (default) | Deep analysis | High |
| 7+ | Exhaustive search | Very High |

**Rule of thumb**: Start with depth 3, increase if answer is incomplete.

## Chunk Size Tuning

In `RlmConfig`:
- `target_chunk_size: 4000` - Default, good balance
- `target_chunk_size: 2000` - More granular, better for dense code
- `target_chunk_size: 8000` - Fewer chunks, faster navigation

## Chunking Strategy Selection

```rust
// For source code
ChunkStrategy::Structural  // Respects function/class boundaries

// For mixed content
ChunkStrategy::FixedSize { size: 4000, overlap: 200 }

// For directory exploration
ChunkStrategy::ByFile  // One chunk per file
```

## Caching

RlmCache stores:
- Navigation decisions (query + chunk IDs → selected chunks)
- Exploration results (chunk ID → findings)

Configure in `CacheConfig`:
```rust
CacheConfig {
    max_entries: 1000,
    ttl_seconds: 3600,  // 1 hour
}
```

## Parallel Exploration

Enable with `parallel_exploration: true` (default).

Configure concurrency:
```rust
ParallelConfig {
    max_concurrent: 4,  // Parallel LLM calls
    timeout_ms: 30000,  // Per-chunk timeout
}
```

## Memory Management

For large codebases (1000+ files):
1. Use `ChunkStrategy::ByFile` to avoid loading all content
2. Set `max_chunks_per_level: 5` to reduce navigation payload
3. Enable caching to avoid re-exploration

## Continuous Mode Optimization

For `--rlm-mode continuous`:
- Context is loaded once and kept in memory
- Queries reuse pre-computed chunk structure
- Set `rate_limit_per_minute: 10` to prevent overload

## Cost Estimation

| Codebase Size | Typical Query Cost |
|---------------|-------------------|
| 10 files | ~$0.10 |
| 50 files | ~$0.30 |
| 200 files | ~$0.80 |
| 1000 files | ~$2.00 |

*Costs based on Opus 4.5 exploration + Haiku 4.5 navigation*

## Debugging

Enable tracing:
```bash
RUST_LOG=jarvis_v2::rlm=debug jarvis-v2 --rlm ...
```

Key log points:
- `navigator.rs:decide_next` - See selected chunks
- `executor.rs:explore_recursive` - Track depth/progress
- `aggregator.rs:aggregate` - See deduplication stats
