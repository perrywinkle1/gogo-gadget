# swarm-health-monitor

> Health checks and coordinated execution tracking for swarm agents: track per-agent iteration count, failure rates, divergence detection (when agents take conflicting actions), and automatic fallback to single-agent mode on coordinator failure.

## Trigger

Use when the user asks to:
- "check swarm health"
- "swarm status"
- "monitor swarm agents"
- "detect agent divergence"
- "swarm diagnostics"
- "agent failure rates"
- "swarm coordination status"
- "fallback to single agent"

## Instructions

### 1. Gather Swarm State

First, collect the current swarm state from available sources: