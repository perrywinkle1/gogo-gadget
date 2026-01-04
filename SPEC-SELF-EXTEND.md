# Self-Extending Capability Specification

## Vision

gogo-gadget should be able to:
1. Recognize when it would benefit from a new capability (MCP, SKILL.md, AGENT.md)
2. Synthesize that capability
3. Configure/register it
4. Utilize it in subsequent task execution

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         GADGET-V2                                │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                     META LOOP                             │   │
│  │                                                           │   │
│  │   ┌─────────────┐    ┌─────────────────────────────┐     │   │
│  │   │ Task Loop   │───>│ Gap Detection               │     │   │
│  │   │ (existing)  │    │                             │     │   │
│  │   └─────────────┘    │ - Explicit: "I need X"      │     │   │
│  │         │            │ - Implicit: repeated fail   │     │   │
│  │         │            │ - Pattern: category stuck   │     │   │
│  │         │            └──────────────┬──────────────┘     │   │
│  │         │                           │                     │   │
│  │         │            ┌──────────────▼──────────────┐     │   │
│  │         │            │ Capability Registry         │     │   │
│  │         │            │                             │     │   │
│  │         │            │ - Available capabilities    │     │   │
│  │         │            │ - Usage statistics          │     │   │
│  │         │            │ - Cost/benefit tracking     │     │   │
│  │         │            └──────────────┬──────────────┘     │   │
│  │         │                           │                     │   │
│  │         │            ┌──────────────▼──────────────┐     │   │
│  │         │            │ Synthesis Engine            │     │   │
│  │         │            │                             │     │   │
│  │         │            │ - MCP generator             │     │   │
│  │         │            │ - SKILL.md generator        │     │   │
│  │         │            │ - AGENT.md generator        │     │   │
│  │         │            └──────────────┬──────────────┘     │   │
│  │         │                           │                     │   │
│  │         │            ┌──────────────▼──────────────┐     │   │
│  │         │            │ Verification Loop           │     │   │
│  │         │            │                             │     │   │
│  │         │            │ - Syntax validation         │     │   │
│  │         │            │ - Functional testing        │     │   │
│  │         │            │ - Integration check         │     │   │
│  │         │            └──────────────┬──────────────┘     │   │
│  │         │                           │                     │   │
│  │         │            ┌──────────────▼──────────────┐     │   │
│  │         │            │ Hot Loader                  │     │   │
│  │         │            │                             │     │   │
│  │         │            │ - Register MCP              │     │   │
│  │         │            │ - Register Skill            │     │   │
│  │         │            │ - Update config             │     │   │
│  │         │            └──────────────┬──────────────┘     │   │
│  │         │                           │                     │   │
│  │         └───────────────────────────┘                     │   │
│  │              (retry with new capability)                  │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Components

### 1. Gap Detection (`src/extend/gap.rs`)

Detects when the agent lacks a capability it needs.

**Signals:**
- **Explicit**: Claude outputs "I would need an MCP for X" or "This would be easier with a tool for Y"
- **Implicit**: Repeated failures (>3) on same category of task
- **Pattern**: Known capability gap patterns (e.g., "fetch from API X" without API MCP)

**Interface:**
```rust
pub struct GapDetector {
    patterns: Vec<GapPattern>,
    history: Vec<TaskAttempt>,
}

pub enum CapabilityGap {
    MCP { name: String, purpose: String, api_hint: Option<String> },
    Skill { name: String, trigger: String, purpose: String },
    Agent { name: String, specialization: String },
}

impl GapDetector {
    fn analyze_output(&self, output: &str) -> Option<CapabilityGap>;
    fn analyze_failure_pattern(&self, history: &[TaskAttempt]) -> Option<CapabilityGap>;
}
```

### 2. Capability Registry (`src/extend/registry.rs`)

Tracks available and synthesized capabilities.

**Storage:** `~/.gogo-gadget/capabilities.json`

```rust
pub struct CapabilityRegistry {
    mcps: Vec<MCPCapability>,
    skills: Vec<SkillCapability>,
    agents: Vec<AgentCapability>,
}

pub struct MCPCapability {
    name: String,
    path: PathBuf,           // Path to MCP server
    config: serde_json::Value, // MCP config
    synthesized: bool,       // true if we created it
    usage_count: u32,
    last_used: DateTime<Utc>,
    success_rate: f32,
}

impl CapabilityRegistry {
    fn has_capability(&self, gap: &CapabilityGap) -> bool;
    fn register(&mut self, capability: Capability) -> Result<()>;
    fn prune_unused(&mut self, max_age_days: u32) -> Vec<Capability>;
}
```

### 3. Synthesis Engine (`src/extend/synthesis.rs`)

Generates new capabilities from gap descriptions.

**Approach:** Use Claude to generate the capability, with templates/scaffolding.

```rust
pub struct SynthesisEngine {
    templates: TemplateRegistry,
    working_dir: PathBuf,
}

impl SynthesisEngine {
    async fn synthesize_mcp(&self, gap: &CapabilityGap) -> Result<MCPCapability>;
    async fn synthesize_skill(&self, gap: &CapabilityGap) -> Result<SkillCapability>;
    async fn synthesize_agent(&self, gap: &CapabilityGap) -> Result<AgentCapability>;
}
```

**MCP Synthesis Flow:**
1. Identify API/service needed
2. Select template (HTTP API, file-based, database, etc.)
3. Generate MCP server code (TypeScript or Python)
4. Generate configuration
5. Return for verification

**Skill Synthesis Flow:**
1. Parse trigger patterns
2. Generate SKILL.md with appropriate structure
3. Include execution instructions
4. Return for verification

### 4. Verification Loop (`src/extend/verify.rs`)

Tests synthesized capabilities before registration.

```rust
pub struct CapabilityVerifier {
    sandbox_dir: PathBuf,
}

impl CapabilityVerifier {
    async fn verify_mcp(&self, mcp: &MCPCapability) -> VerificationResult;
    async fn verify_skill(&self, skill: &SkillCapability) -> VerificationResult;

    // For MCPs: start server, call a test method, verify response
    // For Skills: parse SKILL.md, validate structure, dry-run trigger
}

pub struct VerificationResult {
    success: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
}
```

### 5. Hot Loader (`src/extend/loader.rs`)

Registers capabilities without requiring restart.

```rust
pub struct HotLoader {
    claude_settings_path: PathBuf,
    skills_dir: PathBuf,
}

impl HotLoader {
    fn register_mcp(&self, mcp: &MCPCapability) -> Result<()>;
    fn register_skill(&self, skill: &SkillCapability) -> Result<()>;
    fn reload_claude_config(&self) -> Result<()>;
}
```

**Challenge:** Claude Code MCP config requires restart.

**Solutions:**
1. **Meta-MCP Proxy**: A single MCP that proxies to dynamically loaded MCPs
2. **File-watching**: Synthesize capability, signal user to restart Claude
3. **Skill-first**: Skills don't require restart, prefer them when possible

## Integration with Task Loop

Modify `task_loop.rs` to include meta-loop:

```rust
impl TaskLoop {
    pub async fn execute_with_extension(&mut self, task: &str) -> Result<TaskResult> {
        let gap_detector = GapDetector::new();
        let registry = CapabilityRegistry::load()?;
        let synthesis = SynthesisEngine::new();

        loop {
            // Attempt task
            let result = self.execute(task).await;

            // Check for capability gap
            if !result.success {
                if let Some(gap) = gap_detector.analyze(&result) {
                    if !registry.has_capability(&gap) {
                        // Synthesize new capability
                        let capability = synthesis.synthesize(&gap).await?;

                        // Verify it works
                        let verification = verify(&capability).await?;
                        if verification.success {
                            // Register and retry
                            registry.register(capability)?;
                            hot_loader.load(&capability)?;
                            continue; // Retry with new capability
                        }
                    }
                }
            }

            return result;
        }
    }
}
```

## File Structure

```
src/
├── extend/
│   ├── mod.rs           # Module exports
│   ├── gap.rs           # Gap detection
│   ├── registry.rs      # Capability registry
│   ├── synthesis.rs     # Capability generation
│   ├── verify.rs        # Verification loop
│   ├── loader.rs        # Hot loading
│   └── templates/       # Synthesis templates
│       ├── mcp_http.rs
│       ├── mcp_file.rs
│       ├── skill.rs
│       └── agent.rs
```

## Configuration

```toml
# ~/.gogo-gadget/config.toml

[self_extend]
enabled = true
max_synthesized_capabilities = 20
auto_prune_days = 30
require_verification = true

[self_extend.synthesis]
prefer_skills_over_mcps = true  # Skills don't need restart
mcp_language = "typescript"      # or "python"
```

## Open Questions

1. **Scope creep**: How to prevent creating too many capabilities?
   - Usage-based pruning
   - Cost/benefit analysis before synthesis
   - User approval for synthesis (optional)

2. **Quality**: How to ensure synthesized capabilities work well?
   - Verification loop with real tests
   - Rollback if capability causes failures
   - Track success rate per capability

3. **Hot reload**: Claude Code requires restart for MCP changes
   - Use meta-MCP proxy pattern?
   - Prefer skills (no restart needed)?
   - Accept restart requirement?

4. **Security**: Synthesized code could be malicious
   - Sandbox synthesis and verification
   - Review before registration (optional)
   - Limit capabilities (no network for file MCPs, etc.)

## Implementation Order

1. **Gap Detection** - Detect explicit "I need X" signals
2. **Skill Synthesis** - Skills are simpler, no restart needed
3. **Capability Registry** - Track what we have
4. **Verification Loop** - Ensure quality
5. **MCP Synthesis** - More complex, requires restart handling
6. **Hot Loader** - Meta-MCP proxy for dynamic loading
