# GoGoGadget

Autonomous AI agent framework that executes tasks to genuine completion using outcome-focused iteration, parallel worker execution (CLI flag: `--swarm`), and self-extending capabilities. Built in Rust with Tokio async runtime, uses Claude CLI (OAuth, no API keys).

## Core Execution Loop

GoGoGadget uses **Ralph-style iteration**: runs until the task is genuinely complete, not for a fixed number of iterations.

### Signal Files
- `.gogo-gadget-satisfied` - Agent declares task complete with evidence
- `.gogo-gadget-continue` - Agent requests another iteration (work remains)
- `.gogo-gadget-blocked` - Agent is blocked and needs help

### Evidence Levels
Set via `--evidence-level`:
- `minimal` - Trust agent's completion claim
- `adequate` (default) - Require build/test pass + LLM verification
- `strong` - Require all checks + comprehensive evidence in satisfaction file

### Iteration Flow
1. Agent executes with evolved prompt (includes prior feedback)
2. System checks for signal files
3. Verifier runs build/test + LLM completion check
4. If incomplete, feedback is added to next iteration prompt
5. Repeat until satisfied or blocked

## Worker Mode (CLI: --swarm)

Enabled with `--swarm N` where N is the number of parallel agents.

### Decomposition
- LLM decomposes task into N subtasks with focus areas
- Each subtask gets unique instructions and scope boundaries
- Subtasks run in parallel via Claude CLI subprocesses
- RLM is auto-used for decomposition only when repeated assignments are detected

### Goal-Anchor Drift Guard
Extracts significant words from original task and checks agent output each iteration. If agents drift from the goal, feedback redirects them.

### Aggregation
After all agents complete, results are merged:
- Conflict detection between agent outputs
- LLM synthesis produces unified result
- Verification runs on aggregated changes

### Continuous Mode
With `--continuous`, worker mode ignores satisfaction signals and only stops on blocked. Useful for ongoing maintenance tasks.

## Self-Extend System

Enabled by default. Disable with `--no-self-extend`.

### Gap Detection
Analyzes task failures to identify missing capabilities:
- MCP servers (external APIs)
- Skills (reusable patterns via SKILL.md)
- Agents (specialized behaviors via AGENT.md)

### Synthesis Pipeline
1. Gap detected from failure patterns
2. Template selected (MCP, Skill, or Agent)
3. LLM generates capability definition
4. Quick verification (syntax, basic tests)
5. Registration in capability registry

### Hot Loading
New capabilities are available immediately for next iteration without restart.

### Registry
Tracks all capabilities in `.gogo-gadget/capabilities.json`:
- Usage statistics
- Success rates
- Auto-pruning of low-performing capabilities

## Creative Overseer

The "Chief of Product" pattern. Runs alongside worker mode to proactively create capabilities before gaps are encountered.

### How It Works
1. Observes task decomposition and agent assignments
2. Brainstorms tools/APIs/patterns that would help
3. Synthesizes high-confidence ideas immediately
4. Suggests new capabilities to agents via shared context

### Activation
- `--overseer-only` - Run overseer without worker mode (standalone ideation)
- Automatically active in worker mode when self-extend is enabled

### Configuration (in OverseerConfig)
- `min_confidence_to_synthesize`: 0.7 (default)
- `max_ideas_per_iteration`: 3
- `brainstorm_model`: opus (default)

## Verification & Anti-Laziness

### Build/Test Verification
Auto-detects project type and runs appropriate commands:
- Rust: `cargo build`, `cargo test`
- Node.js: `npm run build`, `npm test`
- Python: `pytest`

### LLM Completion Check
After build/test, an LLM reviews the output to determine if the task is actually complete. Returns:
- `is_complete`: boolean
- `explanation`: why or why not
- `next_steps`: what remains (if incomplete)

### Mock Detector
Enabled with `--anti-laziness`. Scans for shortcuts:
- TODO/FIXME comments
- Placeholder data (example.com, John Doe, lorem ipsum)
- `todo!()` / `unimplemented!()` macros
- Debug statements in production code
- Hardcoded secrets

Violations block completion until fixed.

## RLM Mode (Opt-In)

Recursive Language Model for processing codebases that exceed context limits. Enable with `--rlm`.

### When to Use
- Codebase > 50 files or > 100K tokens
- Questions requiring cross-file understanding
- NOT for greenfield projects (nothing to explore)

### How It Works
1. **Chunker** splits context into ~4000 token pieces (structural/semantic/fixed strategies)
2. **Navigator** (Haiku) selects most relevant chunks based on query
3. **Explorer** (Opus) reads selected chunks in detail
4. **Aggregator** synthesizes findings with evidence citations

### Modes
- `oneshot` (default) - Single query, return results
- `continuous` - Watch for `.gogo-gadget-rlm-query` signal files

### CLI Flags
- `--rlm-context <PATH>` - File or directory to analyze
- `--rlm-depth <N>` - Max recursion depth (default: 5)
- `--rlm-mode <MODE>` - `oneshot` or `continuous`

## CLI Flags Overview

### Execution Mode
| Flag | Description |
|------|-------------|
| `--swarm <N>` | Run N parallel agents (worker mode) |
| `--rlm` | Enable RLM mode for large codebases |
| `--continuous` | Run until blocked (ignore satisfaction) |
| `--subagent` | Run as a child process of another agent |

### Self-Extension
| Flag | Description |
|------|-------------|
| `--self-extend` | Enable capability synthesis (default: on) |
| `--no-self-extend` | Disable capability synthesis |
| `--overseer-only` | Run Creative Overseer standalone |

### Verification
| Flag | Description |
|------|-------------|
| `--anti-laziness` | Enable mock data detection |
| `--evidence-level <LEVEL>` | minimal / adequate / strong |
| `--skip-build-check` | Skip build verification |
| `--skip-test-check` | Skip test verification |

### Output
| Flag | Description |
|------|-------------|
| `--quiet` | Minimal output |
| `--verbose` | Detailed logging |
| `--json` | Output results as JSON |

## File Map

```
src/
├── main.rs              # CLI entry point, argument parsing
├── task_loop.rs         # Core execution loop, signal file handling
├── swarm/
│   ├── mod.rs           # Worker types and re-exports (implementation)
│   ├── coordinator.rs   # Parallel agent orchestration, drift detection
│   ├── decomposer.rs    # Task decomposition into subtasks
│   └── aggregator.rs    # Result merging and conflict resolution
├── extend/
│   ├── mod.rs           # Self-extend types and re-exports
│   ├── gap.rs           # Gap detection from failures
│   ├── synthesis.rs     # Capability generation
│   ├── registry.rs      # Capability storage and tracking
│   ├── loader.rs        # Hot loading of new capabilities
│   ├── verify.rs        # Quick verification of synthesized code
│   ├── overseer.rs      # Creative Overseer (Chief of Product)
│   └── context.rs       # Shared context for overseer observation
├── verify/
│   ├── mod.rs           # Verification orchestration
│   └── mock_detector.rs # Anti-laziness pattern detection
├── rlm/
│   ├── executor.rs      # RLM main entry point
│   ├── navigator.rs     # LLM-based chunk selection
│   ├── chunker.rs       # Content splitting strategies
│   ├── context.rs       # Explorable environment
│   ├── aggregator.rs    # Result synthesis
│   ├── daemon.rs        # Continuous mode signal file watching
│   ├── cache.rs         # LRU cache for navigation
│   └── types.rs         # RlmConfig, Chunk, RlmResult
└── claude.rs            # Claude CLI wrapper
```

## How to Read This Repo

### Start Here
1. `src/main.rs` - Understand CLI flags and how modes are selected
2. `src/task_loop.rs` - The core execution loop (signal files, iteration, evolved prompts)
3. `src/swarm/coordinator.rs` - How parallel agents are orchestrated (worker mode)

### Deep Dives
- **Worker internals**: `docs/specs/core.md`, then `src/swarm/decomposer.rs`
- **Self-extend system**: `docs/specs/self-extend.md`, then `src/extend/gap.rs` → `synthesis.rs` → `loader.rs`
- **RLM architecture**: `docs/agents/rlm-architecture.md`, then `src/rlm/executor.rs`
- **Verification logic**: `src/verify/mod.rs` and `src/verify/mock_detector.rs`

### Documentation
```
docs/
├── specs/
│   ├── core.md           # Core iteration and completion semantics
│   └── self-extend.md    # Self-extension architecture
├── agents/
│   ├── rlm-architecture.md
│   └── rlm-tuning.md
├── architecture/
│   └── flowchart.md      # Visual system flow
└── bugs/
    └── workers-verification.md
```

## Commands
- Build: `cargo build --release`
- Test: `cargo test`
- Run: `gogo-gadget "<task>"` or `gogo-gadget --swarm 3 "<task>"` (worker mode)
