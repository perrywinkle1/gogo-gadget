# Bug Report: Swarm Verification Does Not Catch Build Errors

## Summary
Swarm mode completed "successfully" (5/5 agents, 0 conflicts) but left the application in a broken state with TypeScript compilation errors and corrupted webpack cache.

## Severity
**High** - Users see a broken application despite jarvis-rs reporting success.

## Environment
- jarvis-rs v0.1.0
- Task: Transform Your Harness to cypherpunk theme
- 5 agents spawned across 23 files

## Expected Behavior
The orchestrator should verify that the application actually builds and runs correctly after agents complete their work.

## Actual Behavior
1. Swarm reported success: "5/5 agents succeeded", "0 conflicts", "verified after 1 iteration"
2. Application showed Next.js error indicator (purple logo) instead of content
3. Multiple TypeScript compilation errors existed:
   - 15+ unused import declarations
   - Property access errors (e.g., `h.userSatisfactionScore` vs `h.metrics.userSatisfactionScore`)
   - Type mismatches between data structure and type definitions
4. `.next` cache was corrupted with missing webpack chunks

## Root Causes

### 1. No Build Verification
The orchestrator does not run `npm run build` or `npx tsc --noEmit` to verify TypeScript compilation succeeds.

```rust
// Current: Orchestrator only checks if agents report success
// Missing: Actual build/type verification step
```

### 2. Agents Add Unused Imports
Agents frequently import utilities they don't end up using:
```typescript
// Agent added these imports but never used them:
import { cn } from '@/lib/utils/cn';
import { Card, Badge, Rating, Button } from '@/components/ui';
import type { PricingModel, Harness } from '@/types';
```

### 3. Agents Don't Verify Type Compatibility
Agents access properties based on component patterns without verifying against type definitions:
```typescript
// Agent wrote:
harness.userSatisfactionScore

// Type definition says:
harness.metrics.userSatisfactionScore
```

### 4. No Cache Clearing
The `.next` directory can become corrupted during concurrent agent file modifications. Orchestrator should clear it before final verification.

## Proposed Fixes

### Fix 1: Add Build Verification to Orchestrator
```rust
// In swarm verification phase:
async fn verify_build(&self) -> Result<bool> {
    // Clear build cache
    let _ = tokio::fs::remove_dir_all(".next").await;

    // Run TypeScript compilation check
    let output = Command::new("npx")
        .args(["tsc", "--noEmit"])
        .current_dir(&self.working_dir)
        .output()
        .await?;

    if !output.status.success() {
        log::error!("TypeScript errors found:\n{}",
            String::from_utf8_lossy(&output.stderr));
        return Ok(false);
    }

    Ok(true)
}
```

### Fix 2: Add Lint/Unused Detection to Agent Prompts
Include in agent system prompt:
```
After making changes, verify:
1. All imports are used (no unused declarations)
2. Property access matches type definitions
3. Run `npx tsc --noEmit` if TypeScript project
```

### Fix 3: Clear Build Cache Before Verification
```rust
// Before running dev server or verification
async fn clear_build_cache(&self) {
    let cache_paths = [".next", "dist", "build", ".turbo"];
    for path in cache_paths {
        let _ = tokio::fs::remove_dir_all(path).await;
    }
}
```

### Fix 4: Add Compilation Step to Agent Completion
Each agent should verify their changes compile before reporting success:
```rust
// In agent completion logic
let compile_check = self.run_compile_check().await?;
if !compile_check.success {
    return AgentResult::Failed {
        reason: "TypeScript compilation errors",
        errors: compile_check.errors,
    };
}
```

## Errors Found (Partial List)

| File | Error |
|------|-------|
| BrowseContent.tsx | Unused imports: `PricingModel`, `Harness`, `cn` |
| BrowseContent.tsx | `h.userSatisfactionScore` should be `h.metrics.userSatisfactionScore` |
| CartContent.tsx | Unused import: `cn` |
| HarnessDetailContent.tsx | `harness.compatiblePlatforms` doesn't exist on type |
| OptimizerWizard.tsx | Unused imports: `Input`, `Rating`, unused variable `index` |
| HarnessCard.tsx | Unused imports: `Card`, `Badge`, `Rating`, `Button` |
| CategorySection.tsx | Unused import: `Card` |
| + many more... | |

## Impact
- User sees broken purple logo instead of cypherpunk-themed site
- All jarvis-rs swarm success metrics are misleading
- Manual intervention required to fix agent mistakes
- Trust in autonomous operation compromised

## Priority
This should be fixed before promoting swarm mode as production-ready.

## Related Files
- `apps/jarvis-rs/src/swarm/orchestrator.rs` - needs verification logic
- `apps/jarvis-rs/src/swarm/executor.rs` - agent completion validation
- `apps/jarvis-rs/src/swarm/coordinator.rs` - swarm-level verification
