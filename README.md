# Jarvis-V2

**Autonomous AI Agent Framework with Self-Extending Capabilities**

Jarvis-V2 is an evolution of jarvis-rs that adds the ability to recognize capability gaps during task execution and automatically synthesize new capabilities (MCPs, Skills, Agents) to fill those gaps.

## Features

- **Autonomous Execution**: Runs until tasks are verified complete - no iteration limits
- **Self-Extending**: Detects when it needs capabilities it doesn't have and synthesizes them
- **Swarm Mode**: Parallel execution with multiple Claude agents for complex tasks
- **Verification Loop**: Built-in verification ensures tasks are actually complete
- **Anti-Laziness Enforcement**: Detects mock data patterns and requires real evidence
- **Shortcut Detection**: Catches 18 types of incomplete work (mock data, TODOs, placeholders, etc.)

## Installation

```bash
# Clone the repository
git clone git@github.com:YOUR_USERNAME/jarvis-rs-v2.git
cd jarvis-rs-v2

# Build
cargo build --release

# The binary will be at ./target/release/jarvis-v2
```

## Quick Start

```bash
# Basic task execution
./target/release/jarvis-v2 "Refactor the authentication module"

# With self-extending capabilities enabled
./target/release/jarvis-v2 --self-extend "Fetch data from the GitHub API and analyze repository statistics"

# Dry run to see task analysis
./target/release/jarvis-v2 --dry-run "Your task here"

# Force swarm mode with 5 parallel agents
./target/release/jarvis-v2 --swarm 5 "Implement a full CRUD API"
```

## Self-Extending Capabilities

The core innovation of Jarvis-V2 is its ability to extend itself. When enabled with `--self-extend`, the system:

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
    jarvis-v2 [OPTIONS] [TASK]

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
│                         Jarvis-V2                                │
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
└── subagent.rs          # Claude Code integration
```

## Capability Types

### MCP Servers
Model Context Protocol servers that provide tool access to Claude.

```bash
# List registered MCPs
./target/release/jarvis-v2 --list-capabilities

# Synthesized MCPs are stored in:
~/.jarvis/synthesized/mcps/
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

Jarvis-V2 looks for configuration in:
1. `.jarvis/config.json` (project-level)
2. `~/.jarvis/config.json` (user-level)

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

Jarvis-V2 uses signal files for agent communication:

| File | Purpose |
|------|---------|
| `.jarvis-satisfied` | Agent signals task completion with summary |
| `.jarvis-continue` | Agent needs more iterations |
| `.jarvis-blocked` | Agent is blocked and cannot proceed |
| `.jarvis-checkpoint` | Checkpoint for resume capability |

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
