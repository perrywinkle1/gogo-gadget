# RLM Architecture

## Overview

RLM (Recursive Language Model) enables processing of contexts exceeding LLM context windows through intelligent recursive decomposition. Instead of truncating or summarizing, RLM treats large codebases as explorable environments.

## Core Components

### 1. RlmContext (`context.rs`)
Represents the explorable environment:
- Holds all chunks in a HashMap by ID
- Tracks chunk hierarchy (parent/child relationships)
- Maintains relevance scores per chunk
- Provides iteration methods for exploration

### 2. Chunker (`chunker.rs`)
Splits content into processable pieces:

| Strategy | Description | Best For |
|----------|-------------|----------|
| `Structural` | Split at code boundaries (fn, class, struct) | Source code |
| `Semantic` | Split at topic/meaning boundaries | Documentation |
| `FixedSize` | Fixed size with overlap | Generic text |
| `ByFile` | One chunk per file | Directory exploration |

### 3. Navigator (`navigator.rs`)
LLM-based chunk selection using Haiku 4.5:
- Receives query + chunk previews
- Returns `NavigationDecision` with selected chunk IDs
- Supports parallel vs sequential exploration hints
- Falls back to heuristic scoring without embeddings

### 4. RlmExecutor (`executor.rs`)
Main orchestration engine:
```
execute(query, path)
  → create RlmContext
  → prescore_chunks()
  → explore_recursive(depth=0)
      → navigator.decide_next()
      → for each selected chunk:
          → if small enough: explore directly
          → else: decompose and recurse
  → aggregate_results()
  → return RlmResult
```

### 5. Aggregator (`aggregator.rs`)
Synthesizes findings from multiple chunks:
- Deduplication (semantic similarity > 0.85)
- Conflict resolution (weighted by depth, relevance)
- Evidence citation with file:line references
- Confidence scoring

### 6. RlmDaemon (`daemon.rs`)
Continuous mode operation:
- Watches for `.jarvis-rlm-query` signal files
- Maintains RlmContext in memory
- Writes responses to `.jarvis-rlm-response`
- Respects `.jarvis-blocked` shutdown signal

## Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                         RLM Pipeline                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Input: Query + Context Path                                    │
│     │                                                           │
│     ▼                                                           │
│  ┌─────────┐    ┌───────────┐    ┌───────────┐                 │
│  │ Chunker │───▶│  Context  │───▶│ Pre-score │                 │
│  │(split)  │    │ (HashMap) │    │(relevance)│                 │
│  └─────────┘    └───────────┘    └───────────┘                 │
│                                        │                        │
│                                        ▼                        │
│                               ┌─────────────────┐               │
│                               │    Navigator    │               │
│                               │   (Haiku 4.5)   │               │
│                               └────────┬────────┘               │
│                                        │                        │
│                        ┌───────────────┼───────────────┐        │
│                        ▼               ▼               ▼        │
│                   ┌─────────┐    ┌─────────┐    ┌─────────┐    │
│                   │ Chunk A │    │ Chunk B │    │ Chunk C │    │
│                   │(explore)│    │(explore)│    │(explore)│    │
│                   └────┬────┘    └────┬────┘    └────┬────┘    │
│                        │              │              │          │
│                        └──────────────┼──────────────┘          │
│                                       ▼                         │
│                              ┌─────────────────┐                │
│                              │   Aggregator    │                │
│                              │  (Opus 4.5)     │                │
│                              └────────┬────────┘                │
│                                       │                         │
│                                       ▼                         │
│  Output: RlmResult { answer, insights, evidence, confidence }   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Key Types

### RlmConfig
```rust
RlmConfig {
    max_depth: 5,              // Recursion limit
    max_chunks_per_level: 10,  // Chunks shown to navigator
    target_chunk_size: 4000,   // Tokens per chunk
    chunk_overlap: 200,        // Overlap for context preservation
    navigator_model: "haiku",
    explorer_model: "opus",
    use_embeddings: false,     // Heuristic scoring by default
}
```

### RlmResult
```rust
RlmResult {
    success: bool,
    answer: String,
    key_insights: Vec<String>,
    evidence: Vec<Evidence>,   // file:line citations
    chunks_explored: usize,
    max_depth_reached: u32,
    total_tokens: u32,
    confidence: f32,           // 0.0 - 1.0
}
```

## Performance Considerations

| Factor | Impact | Mitigation |
|--------|--------|------------|
| Large directories | Slow chunking | Use ByFile strategy, skip binary |
| Deep recursion | Token cost | Set appropriate max_depth |
| Many chunks | Navigator overwhelm | Pre-score and limit to top N |
| Repeated queries | Redundant work | RlmCache (LRU) |
