# GoGoGadget with RLM

Autonomous AI agent with Recursive Language Model for processing codebases exceeding context limits.

## Tech Stack
- Rust with Tokio async runtime
- Claude API via CLI (OAuth, no API keys needed)
- Haiku 4.5 (navigation) + Opus 4.5 (exploration/synthesis)

## RLM Architecture
```
src/rlm/
├── executor.rs    # Main entry: RlmExecutor.execute()
├── navigator.rs   # LLM-based chunk selection
├── chunker.rs     # Content splitting (structural/semantic/fixed)
├── context.rs     # RlmContext - explorable environment
├── aggregator.rs  # Result synthesis + deduplication
├── daemon.rs      # Continuous mode (.gogo-gadget-rlm-query signal files)
├── cache.rs       # LRU cache for navigation decisions
└── types.rs       # RlmConfig, Chunk, RlmResult
```

## Commands
- Build: `cargo build --release`
- Test: `cargo test rlm`
- Run: `gogo-gadget --rlm --rlm-context <path> "<query>"`

## RLM Use Cases

| Use Case | Command |
|----------|---------|
| **Codebase Exploration** | `gogo-gadget --rlm --rlm-context ./src "How does authentication work?"` |
| **Security Audit** | `gogo-gadget --rlm --rlm-context ./src "Find SQL injection vulnerabilities"` |
| **Architecture Understanding** | `gogo-gadget --rlm --rlm-context . "Map the data flow from API to database"` |
| **Debugging** | `gogo-gadget --rlm --rlm-context ./src "Why might the checkout fail silently?"` |
| **Refactoring Analysis** | `gogo-gadget --rlm --rlm-context ./src "What would break if I rename UserService?"` |
| **Documentation Gen** | `gogo-gadget --rlm --rlm-context ./src "Document all public APIs"` |
| **Dependency Analysis** | `gogo-gadget --rlm --rlm-context . "What external services does this app call?"` |
| **Test Coverage** | `gogo-gadget --rlm --rlm-context ./tests "What scenarios are not covered?"` |

## CLI Flags
- `--rlm` - Enable RLM mode
- `--rlm-context <PATH>` - File or directory to analyze
- `--rlm-depth <N>` - Max recursion depth (default: 5)
- `--rlm-mode <MODE>` - `oneshot` (default) or `continuous`

## Flow
1. Chunker splits context into ~4000 token pieces
2. Navigator (Haiku) selects most relevant chunks
3. Explorer (Opus) reads selected chunks in detail
4. Aggregator synthesizes findings with evidence citations

## When to Use RLM
- Codebase > 50 files or > 100K tokens
- Questions requiring cross-file understanding
- NOT for greenfield projects (nothing to explore)

## Additional Docs
- `agent_docs/rlm_architecture.md` - Detailed component design
- `agent_docs/rlm_tuning.md` - Performance optimization
