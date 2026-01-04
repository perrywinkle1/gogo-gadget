# claude-cost-tracker

> Track and report Claude API usage: tokens consumed per task, per agent, per RLM depth level, estimated costs, and efficiency trends. Stores data in local SQLite for reporting and analytics.

## Trigger

Use when the user asks about `/claude-cost-tracker` or mentions:
- "/claude-cost-tracker"
- "token usage"
- "API costs"
- "usage analytics"
- "cost report"
- "spending breakdown"
- "efficiency metrics"
- "how much have I spent"
- "token consumption"

## Instructions

### 1. Database Initialization

On first run, create SQLite database at `.jarvis/claude_costs.db`: