# capability-gap-detector

> Compares the extend/gap.rs gap analysis output against the extend/registry.rs capabilities to identify what's missing, what's stale, and what synthesis opportunities exist. Produces prioritized capability suggestions.

## Trigger

Use when the user asks to run `/capability-gap-detector` or mentions:
- "capability gap"
- "missing capabilities"
- "stale capabilities"
- "capability audit"
- "what capabilities are missing"
- "synthesis opportunities"
- "extend gap analysis"
- "registry health check"

## Instructions

### 1. Load Gap Analysis Data

Read and parse the gap analysis output from `src/extend/gap.rs`: