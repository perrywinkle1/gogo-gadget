# capability-synthesis-validator

> Workflow for validating synthesized capabilities (MCP servers, Skills, Agents) against a quality checklist: correct syntax, dependency availability, integration tests, no hardcoded paths, proper error handling

## Trigger

Use when the user asks to validate a synthesized capability or mentions:
- "/capability-synthesis-validator"
- "validate my skill"
- "check this MCP server"
- "validate agent definition"
- "capability quality check"
- "skill validation"
- "verify synthesis output"

## Instructions

### 1. Identify Capability Type

Determine the type of capability being validated:
- **MCP Server**: Check for `mcp.json`, server entry point, tool definitions
- **Skill**: Check for `SKILL.md` or skill definition in CLAUDE.md
- **Agent**: Check for agent configuration, subagent definitions, task routing

### 2. Execute Validation Checklist

#### A. Syntax Validation