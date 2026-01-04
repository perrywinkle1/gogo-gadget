# GoGoGadget Redesign Spec

## Problem

The current gogo-gadget is over-engineered around the wrong abstractions:

1. **Dumb regex-based "brain"** - 700 lines of pattern matching that fails basic cases (e.g., "build an app from scratch" scores 2/10 "trivial")

2. **Iteration-focused instead of outcome-focused** - Shows "iteration 1 of 10" instead of just doing the work until done

3. **Arbitrary limits** - Max iterations, complexity scores, timeouts - none of which matter to the user

4. **Noisy UX** - Exposes implementation details instead of just producing results

## Design Principles

1. **Outcome over process** - The only goal is "production-grade working software"
2. **Smart over rigid** - Use LLM for decisions, not regex
3. **Silent until meaningful** - Don't narrate every step
4. **No arbitrary limits** - Run until genuinely done, not until a counter expires

## Changes Required

### 1. Replace Regex Brain with LLM Brain

**Delete:** `src/brain/analyzer.rs` (700 lines of regex)

**Replace with:** Single LLM call to analyze task

```rust
// src/brain/mod.rs
pub async fn analyze_task(task: &str) -> TaskStrategy {
    let prompt = format!(r#"
Analyze this task and respond with JSON only:
{{
  "agent_count": <1-10, how many parallel agents would help>,
  "approach": "<one sentence strategy>"
}}

Task: {}
"#, task);

    // Call Claude haiku - fast and cheap
    let response = call_claude_haiku(&prompt).await;
    parse_json(response)
}
```

No complexity scores. No keyword matching. Just ask a smart model what to do.

### 2. Remove Iteration Limits

**Delete:** `--max-iterations` flag, iteration counters, "iteration X of Y" output

**Replace with:** Run until completion signal

```rust
// src/task_loop.rs
pub async fn run_until_done(task: &str) -> Result<()> {
    loop {
        let result = execute_claude(task).await?;

        if is_genuinely_complete(&result) {
            return Ok(());
        }

        // Feed issues back and continue
        task = format!("{}\n\nPrevious attempt had issues:\n{}", task, result.issues);
    }
}
```

### 3. Simplify Completion Detection

**Delete:** 18 shortcut types, severity scores, confidence calculations

**Replace with:** Ask Claude if it's done

```rust
async fn is_genuinely_complete(output: &str, task: &str) -> bool {
    let prompt = format!(r#"
Was this task completed to production quality? Answer YES or NO with one sentence reason.

Task: {}
Output: {}
"#, task, output);

    let response = call_claude_haiku(&prompt).await;
    response.starts_with("YES")
}
```

### 4. Silent UX

**Delete:** Progress bars, iteration announcements, complexity scores, emoji spam

**Keep:**
- Errors (when something actually fails)
- Final result summary
- Ctrl+C handling

```rust
// Only output when meaningful
println!("Building screen recorder...");
// ... silence while working ...
println!("Done. Project created at /path/to/screen-recorder");
```

### 5. Simplified CLI

**Before:**
```
gogo-gadget "task" --max-iterations 10 --format json --verbose --complexity-threshold 5
```

**After:**
```
gogo-gadget "task"
gogo-gadget "task" --dir /path
```

That's it. No flags for internal behavior.

## Files to Modify

| File | Action |
|------|--------|
| `src/brain/analyzer.rs` | Delete entirely |
| `src/brain/mod.rs` | Replace with LLM-based analysis |
| `src/task_loop.rs` | Remove iteration limits, simplify loop |
| `src/verify/mod.rs` | Replace with LLM-based completion check |
| `src/verify/shortcuts.rs` | Delete entirely |
| `src/main.rs` | Simplify CLI, remove noise |
| `src/lib.rs` | Remove unused types (Difficulty, complexity scores) |

## Success Criteria

1. `gogo-gadget "build a screen recorder"` produces a working Xcode project
2. No "trivial" or "complexity 2/10" nonsense
3. No "iteration 1 of 10" output
4. Runs until the software actually works
5. Total output is < 5 lines for a successful run

## Anti-Goals

- Don't add more regex patterns
- Don't add more configuration options
- Don't add more shortcut detection types
- Don't optimize for "fast analysis" at the cost of accuracy
