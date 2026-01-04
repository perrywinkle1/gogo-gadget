# rlm-performance-profiler

> Instrument RLM components to measure: chunk scoring time, navigator decision latency, parallel exploration overhead, and aggregation synthesis time. Generates flame graphs and latency histograms to identify optimization targets.

## Trigger

Use when the user asks to profile RLM performance or mentions:
- "/rlm-performance-profiler"
- "RLM is slow"
- "profile RLM"
- "RLM latency"
- "RLM bottleneck"
- "optimize RLM performance"
- "RLM timing breakdown"
- "why is RLM taking so long"

## Instructions

### 1. Instrument the RLM Execution

Add timing instrumentation to each RLM component: