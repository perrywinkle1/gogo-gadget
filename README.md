# GoGoGadget

Autonomous AI agent framework that detects capability gaps and synthesizes new tools (skills, MCPs, agents) to close them.

## What it does

- Executes tasks end-to-end with verification loops
- Spawns parallel agents for complex work (swarm mode)
- Synthesizes new capabilities when it hits a gap
- Hot-loads skills and registers MCP servers
- Enforces anti-shortcut checks to avoid fake or incomplete outputs

## Quick start

```bash
# Clone the repository
git clone git@github.com:YOUR_USERNAME/gogo-gadget.git
cd gogo-gadget

# Build
cargo build --release

# Basic task execution
./target/release/gogo-gadget "Refactor the authentication module"

# Self-extending mode
./target/release/gogo-gadget --self-extend "Fetch GitHub repo stats and summarize"

# Swarm mode with 5 agents
./target/release/gogo-gadget --swarm 5 "Implement a CRUD API"

# Dry-run to see the plan without executing
./target/release/gogo-gadget --dry-run "Your task here"
```

## How it works

1. **Analyze** the task and decide between single-agent or swarm execution.
2. **Execute** with a verification loop that checks actual results, not just agent claims.
3. **Detect gaps** in capabilities from the model's own outputs.
4. **Synthesize** new skills/MCPs/agents when needed.
5. **Register and hot-load** capabilities for immediate reuse.

## Documentation

- Architecture diagrams: `docs/architecture/flowchart.md`, `docs/architecture/diagram-ascii.txt`
- Core spec: `docs/specs/core.md`
- Self-extend spec: `docs/specs/self-extend.md`
- RLM deep-dive: `docs/agents/rlm-architecture.md`
- RLM tuning: `docs/agents/rlm-tuning.md`
- Known issue report: `docs/bugs/swarm-verification.md`

## Repository layout

```
docs/            Architecture diagrams, specs, and agent notes
scripts/         CLI wrappers and completions
src/             Core engine and execution loop
tests/           Unit + integration tests
```

## Notes

This repo intentionally avoids committing runtime artifacts. Generated capabilities and run state live in user home directories (e.g., `~/.gogo-gadget`).
