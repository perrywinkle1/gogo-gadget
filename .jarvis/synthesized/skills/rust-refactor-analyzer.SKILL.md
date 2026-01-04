# rust-refactor-analyzer

> Analyzes Rust code for refactoring opportunities: dead code, unused dependencies, overly complex functions, missing error handling patterns, and API inconsistencies. Outputs actionable improvement suggestions with file:line references.

## Trigger

Use when the user asks to analyze Rust code for refactoring or mentions:
- "/rust-refactor-analyzer"
- "analyze rust code"
- "find dead code"
- "unused dependencies"
- "complex functions"
- "refactoring opportunities"
- "code quality rust"
- "rust code review"
- "technical debt rust"

## Instructions

### Phase 1: Gather Context

1. **Identify Target Scope**
   - If user specifies a path, use that
   - If no path specified, use `./src` as default
   - Check for `Cargo.toml` to confirm Rust project

2. **Run Compiler Diagnostics**