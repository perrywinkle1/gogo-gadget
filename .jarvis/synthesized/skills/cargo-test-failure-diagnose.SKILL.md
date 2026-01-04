# cargo-test-failure-diagnose

> Workflow that runs `cargo test`, captures failures, and automatically generates diagnostic suggestions including test isolation issues, fixture problems, and race conditions

## Trigger

Use when the user invokes `/cargo-test-failure-diagnose` or mentions:
- "diagnose cargo test failures"
- "why are my rust tests failing"
- "debug test failures"
- "flaky rust tests"
- "cargo test race condition"
- "test isolation issues"

## Instructions

### Phase 1: Run Tests and Capture Output

1. **Execute cargo test with full output capture**