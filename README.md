# GoGoGadget

**Autonomous AI Agent Framework with Self-Extending Capabilities**

GoGoGadget is an autonomous AI agent framework that recognizes capability gaps during task execution and automatically synthesizes new capabilities (MCPs, Skills, Agents) to fill those gaps.

## Features

- **Autonomous Execution**: Runs until tasks are verified complete - no iteration limits
- **Self-Extending**: Detects when it needs capabilities it doesn't have and synthesizes them
- **Swarm Mode**: Parallel execution with multiple Claude agents for complex tasks
- **Verification Loop**: Built-in verification ensures tasks are actually complete
- **Anti-Laziness Enforcement**: Detects mock data patterns and requires real evidence
- **Shortcut Detection**: Catches 18 types of incomplete work (mock data, TODOs, placeholders, etc.)
- **RLM Mode**: Recursive Language Model for processing codebases exceeding context limits

## Installation

```bash
# Clone the repository
git clone git@github.com:YOUR_USERNAME/gogo-gadget.git
cd gogo-gadget

# Build
cargo build --release

# The binary will be at ./target/release/gogo-gadget
```

## Quick Start

```bash
# Basic task execution
./target/release/gogo-gadget "Refactor the authentication module"

# With self-extending capabilities enabled
./target/release/gogo-gadget --self-extend "Fetch data from the GitHub API and analyze repository statistics"

# Dry run to see task analysis
./target/release/gogo-gadget --dry-run "Your task here"

# Force swarm mode with 5 parallel agents
./target/release/gogo-gadget --swarm 5 "Implement a full CRUD API"
```

## Self-Extending Capabilities

The core innovation of GoGoGadget is its ability to extend itself. When enabled with `--self-extend`, the system:

1. **Detects Gaps**: Monitors Claude's output for signals like:
   - "I would need an MCP for..."
   - "This would require a skill that..."
   - Repeated failures on similar tasks

2. **Synthesizes Capabilities**: Automatically creates:
   - **MCP Servers**: For API integrations (TypeScript/Python)
   - **SKILL.md Files**: For reusable task patterns
   - **AGENT.md Files**: For specialized agent behaviors

3. **Verifies & Loads**: Tests synthesized capabilities before registering them

4. **Hot Reloads**: Skills load immediately; MCPs signal for restart

### Example Flow

```
Task: "Fetch weather data for San Francisco"

1. Claude attempts task, outputs: "I would need an MCP for OpenWeatherMap API"
2. Gap Detector recognizes MCP need
3. Synthesis Engine creates openweathermap-mcp/ with:
   - index.ts (MCP server implementation)
   - package.json
4. Verifier checks syntax, runs npm install
5. Loader registers MCP in capability registry
6. Next iteration: Claude has weather API access
```

## CLI Reference

```
USAGE:
    gogo-gadget [OPTIONS] [TASK]

ARGUMENTS:
    [TASK]    The task to execute

OPTIONS:
    -d, --dir <PATH>              Working directory for task execution
    -p, --promise <PATTERN>       Completion promise regex pattern
    -m, --model <MODEL_ID>        Claude model to use
    -o, --output-format <FORMAT>  Output format (text, json, markdown)
    -c, --checkpoint <FILE>       Checkpoint file for save/resume
    -s, --swarm <N>               Force swarm mode with N agents

    --dry-run                     Analyze task without executing
    --self-extend                 Enable self-extending capabilities
    --no-self-extend              Disable self-extending (default)
    --detect-gaps-only            Show gaps without synthesizing
    --list-capabilities           List registered capabilities
    --prune-capabilities <DAYS>   Remove unused capabilities older than N days

    --anti-laziness               Enable anti-laziness enforcement
    --no-anti-laziness            Disable anti-laziness enforcement
    --evidence-level <LEVEL>      Required evidence: minimal, adequate, strong

    --subagent-mode               Run as Claude Code subagent
    --register-subagent           Register as Claude Code subagent
    --unregister-subagent         Remove from Claude Code

    -h, --help                    Print help
    -V, --version                 Print version
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         GoGoGadget                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │  Task Loop   │───▶│  Verifier    │───▶│  Completion  │       │
│  └──────────────┘    └──────────────┘    └──────────────┘       │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────┐       │
│  │                   Meta Loop                           │       │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐      │       │
│  │  │    Gap     │  │ Synthesis  │  │    Hot     │      │       │
│  │  │  Detector  │─▶│   Engine   │─▶│   Loader   │      │       │
│  │  └────────────┘  └────────────┘  └────────────┘      │       │
│  │         │              │               │              │       │
│  │         ▼              ▼               ▼              │       │
│  │  ┌────────────────────────────────────────────┐      │       │
│  │  │           Capability Registry              │      │       │
│  │  │  MCPs │ Skills │ Agents │ Usage Stats      │      │       │
│  │  └────────────────────────────────────────────┘      │       │
│  └──────────────────────────────────────────────────────┘       │
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐                           │
│  │    Swarm     │    │    Brain     │                           │
│  │ Coordinator  │    │  (Analysis)  │                           │
│  └──────────────┘    └──────────────┘                           │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## RLM Mode (Recursive Language Model)

RLM enables processing of codebases exceeding LLM context windows through intelligent recursive decomposition. Instead of truncating or summarizing, RLM treats large codebases as explorable environments.

### RLM Data Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          RLM Pipeline                                    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  INPUT: Query + Context Path                                             │
│     │                                                                    │
│     ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                        CHUNKING PHASE                            │    │
│  │  ┌─────────┐    ┌───────────┐    ┌───────────┐                  │    │
│  │  │ Chunker │───▶│  Context  │───▶│ Pre-score │                  │    │
│  │  │(split)  │    │ (HashMap) │    │(relevance)│                  │    │
│  │  └─────────┘    └───────────┘    └───────────┘                  │    │
│  │                                                                  │    │
│  │  Strategies: Structural | Semantic | FixedSize | ByFile         │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                        │                                 │
│                                        ▼                                 │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                       NAVIGATION PHASE                           │    │
│  │                                                                  │    │
│  │      Query: "How does authentication work?"                      │    │
│  │                         │                                        │    │
│  │                         ▼                                        │    │
│  │               ┌─────────────────┐                                │    │
│  │               │    Navigator    │ ◄── Haiku 4.5 (fast)           │    │
│  │               │  "Select most   │                                │    │
│  │               │   relevant      │                                │    │
│  │               │   chunks"       │                                │    │
│  │               └────────┬────────┘                                │    │
│  │                        │                                         │    │
│  │         ┌──────────────┼──────────────┐                          │    │
│  │         ▼              ▼              ▼                          │    │
│  │    [auth.rs]     [login.rs]    [session.rs]                      │    │
│  │    score: 0.92   score: 0.87   score: 0.81                       │    │
│  │                                                                  │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                        │                                 │
│                                        ▼                                 │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                      EXPLORATION PHASE                           │    │
│  │                                                                  │    │
│  │   ┌─────────┐    ┌─────────┐    ┌─────────┐                     │    │
│  │   │ auth.rs │    │login.rs │    │session.rs│  ◄── Opus 4.5      │    │
│  │   │(explore)│    │(explore)│    │(explore) │      (deep)        │    │
│  │   └────┬────┘    └────┬────┘    └────┬─────┘                     │    │
│  │        │              │              │                           │    │
│  │        ▼              ▼              ▼                           │    │
│  │   [Findings]     [Findings]     [Findings]                       │    │
│  │   - JWT tokens   - Password     - Redis store                    │    │
│  │   - OAuth flow   - 2FA check    - TTL config                     │    │
│  │                                                                  │    │
│  │   If chunk too large → DECOMPOSE & RECURSE (depth+1)             │    │
│  │                                                                  │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                        │                                 │
│                                        ▼                                 │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                      AGGREGATION PHASE                           │    │
│  │                                                                  │    │
│  │   ┌────────────────────────────────────────────┐                 │    │
│  │   │              Aggregator (Opus 4.5)          │                 │    │
│  │   │                                            │                 │    │
│  │   │  1. Deduplicate (similarity > 0.85)        │                 │    │
│  │   │  2. Resolve conflicts                      │                 │    │
│  │   │  3. Synthesize answer                      │                 │    │
│  │   │  4. Cite evidence (file:line)              │                 │    │
│  │   │                                            │                 │    │
│  │   └────────────────────────────────────────────┘                 │    │
│  │                                                                  │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                        │                                 │
│                                        ▼                                 │
│  OUTPUT: RlmResult                                                       │
│    ├── answer: "Auth uses JWT with OAuth2 flow..."                       │
│    ├── key_insights: ["JWT in auth.rs:45", "Redis sessions"]             │
│    ├── evidence: [{ file: "src/auth.rs", lines: 42-67 }, ...]           │
│    ├── confidence: 0.94                                                  │
│    └── chunks_explored: 12                                               │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### RLM Use Cases

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

### RLM CLI Flags

```bash
# One-shot RLM query
gogo-gadget --rlm "Find security issues" --rlm-context ./src

# Continuous mode (keeps context in memory)
gogo-gadget --rlm --rlm-mode continuous --rlm-context ./src

# With custom depth (default: 5)
gogo-gadget --rlm --rlm-depth 3 "How does auth work?"
```

| Flag | Description |
|------|-------------|
| `--rlm` | Enable RLM mode |
| `--rlm-context <PATH>` | File or directory to analyze |
| `--rlm-depth <N>` | Max recursion depth (default: 5) |
| `--rlm-mode <MODE>` | `oneshot` (default) or `continuous` |

### When to Use RLM

**Use RLM for:**
- Codebases > 50 files or > 100K tokens
- Questions requiring cross-file understanding
- Security audits of existing code
- Architecture documentation

**Don't use RLM for:**
- Greenfield projects (nothing to explore)
- Single-file questions (use regular mode)
- Simple grep-able queries

### RLM Module Structure

```
src/rlm/
├── executor.rs    # Main entry: RlmExecutor.execute()
├── navigator.rs   # LLM-based chunk selection (Haiku 4.5)
├── chunker.rs     # Content splitting (structural/semantic/fixed)
├── context.rs     # RlmContext - explorable environment
├── aggregator.rs  # Result synthesis + deduplication (Opus 4.5)
├── daemon.rs      # Continuous mode (.gogo-gadget-rlm-query signal files)
├── cache.rs       # LRU cache for navigation decisions
└── types.rs       # RlmConfig, Chunk, RlmResult
```

### RLM Cost Estimation

| Codebase Size | Typical Query Cost |
|---------------|-------------------|
| 10 files | ~$0.10 |
| 50 files | ~$0.30 |
| 200 files | ~$0.80 |
| 1000 files | ~$2.00 |

*Costs based on Opus 4.5 exploration + Haiku 4.5 navigation*

---

## Module Structure

```
src/
├── main.rs              # CLI entry point
├── lib.rs               # Core types (Config, TaskResult, Checkpoint)
├── task_loop.rs         # Iteration loop with verification + meta-loop
├── brain/
│   ├── mod.rs           # Task analysis and complexity scoring
│   └── analyzer.rs      # Language detection, swarm decisions
├── verify/
│   ├── mod.rs           # Verification engine
│   ├── shortcuts.rs     # Shortcut detection (18 types)
│   └── mock_detector.rs # Mock data pattern detection
├── extend/              # Self-extending capability system
│   ├── mod.rs           # Core types (CapabilityGap, SynthesizedCapability)
│   ├── gap.rs           # Gap detection from Claude output
│   ├── registry.rs      # Capability registry with persistence
│   ├── synthesis.rs     # Capability synthesis engine
│   ├── verify.rs        # Capability verification
│   └── loader.rs        # Hot loading of capabilities
├── swarm/
│   ├── mod.rs           # Swarm coordination
│   ├── coordinator.rs   # Multi-agent orchestration
│   └── executor.rs      # Parallel execution
├── rlm/                 # Recursive Language Model
│   ├── mod.rs           # Module exports
│   ├── types.rs         # RlmConfig, Chunk, RlmResult
│   ├── executor.rs      # Main RLM orchestration
│   ├── navigator.rs     # LLM-based chunk selection
│   ├── chunker.rs       # Content splitting strategies
│   ├── context.rs       # RlmContext explorable environment
│   ├── aggregator.rs    # Result synthesis
│   ├── daemon.rs        # Continuous mode runner
│   └── cache.rs         # LRU navigation cache
└── subagent.rs          # Claude Code integration
```

## Capability Types

### MCP Servers
Model Context Protocol servers that provide tool access to Claude.

```bash
# List registered MCPs
./target/release/gogo-gadget --list-capabilities

# Synthesized MCPs are stored in:
~/.gogo-gadget/synthesized/mcps/
```

### Skills (SKILL.md)
Reusable task patterns with triggers and instructions.

```markdown
# Example synthesized skill
---
name: api-integration
trigger: /api-integrate
---

## Instructions
1. Analyze the target API documentation
2. Generate type definitions
3. Create client wrapper with error handling
```

### Agents (AGENT.md)
Specialized agent behaviors for specific domains.

```markdown
# Example synthesized agent
---
name: security-auditor
specialization: Security vulnerability analysis
---

## Capabilities
- OWASP Top 10 detection
- Dependency vulnerability scanning
- Code pattern analysis
```

## Configuration

GoGoGadget looks for configuration in:
1. `.gogo-gadget/config.json` (project-level)
2. `~/.gogo-gadget/config.json` (user-level)

```json
{
  "self_extend": {
    "enabled": true,
    "auto_synthesize": true,
    "require_verification": true,
    "mcp_language": "typescript"
  },
  "anti_laziness": {
    "enabled": true,
    "evidence_level": "adequate"
  },
  "swarm": {
    "max_agents": 10,
    "strategy": "auto"
  }
}
```

## Signal Files

GoGoGadget uses signal files for agent communication:

| File | Purpose |
|------|---------|
| `.gogo-gadget-satisfied` | Agent signals task completion with summary |
| `.gogo-gadget-continue` | Agent needs more iterations |
| `.gogo-gadget-blocked` | Agent is blocked and cannot proceed |
| `.gogo-gadget-checkpoint` | Checkpoint for resume capability |

## Shortcut Types Detected

- Mock/placeholder data ("John Doe", "example@test.com")
- TODO/FIXME comments
- `unimplemented!()`, `todo!()`, `panic!()`
- Debug statements (console.log, print, dbg!)
- Hardcoded secrets and credentials
- Empty implementations
- Commented-out code
- Lorem ipsum text
- Incomplete error handling
- Missing tests

## Development

```bash
# Run tests
cargo test

# Run with debug output
RUST_LOG=debug cargo run -- "your task"

# Build documentation
cargo doc --open
```

## License

MIT
