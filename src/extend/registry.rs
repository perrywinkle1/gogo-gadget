//! Capability Registry Module
//!
//! Tracks available and synthesized capabilities.
//! Storage: ~/.gogo-gadget/capabilities.json

use super::{CapabilityGap, CapabilityType, SynthesizedCapability};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// MCP capability metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapability {
    /// Name of the MCP
    pub name: String,
    /// Path to MCP server
    pub path: PathBuf,
    /// MCP configuration
    pub config: serde_json::Value,
    /// Whether this was auto-synthesized
    pub synthesized: bool,
    /// Usage count
    pub usage_count: u32,
    /// Last used timestamp (unix epoch seconds)
    pub last_used: u64,
    /// Success rate (0.0-1.0)
    pub success_rate: f32,
    /// Created timestamp
    pub created_at: u64,
    /// Description of the MCP
    pub description: Option<String>,
    /// Successful invocations
    pub success_count: u32,
    /// Failed invocations
    pub failure_count: u32,
}

impl McpCapability {
    /// Create a new MCP capability
    pub fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
            config: serde_json::Value::Null,
            synthesized: true,
            usage_count: 0,
            last_used: 0,
            success_rate: 0.0,
            created_at: current_timestamp(),
            description: None,
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Record a usage
    pub fn record_usage(&mut self, success: bool) {
        self.usage_count += 1;
        self.last_used = current_timestamp();

        if success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }

        let total = self.success_count + self.failure_count;
        self.success_rate = if total > 0 {
            self.success_count as f32 / total as f32
        } else {
            0.0
        };
    }

    /// Check if capability is stale (not used in N days)
    pub fn is_stale(&self, max_age_days: u32) -> bool {
        let threshold = current_timestamp() - (max_age_days as u64 * 24 * 60 * 60);
        self.last_used < threshold && self.synthesized
    }
}

/// Skill capability metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCapability {
    /// Name of the skill
    pub name: String,
    /// Path to SKILL.md file
    pub path: PathBuf,
    /// Trigger patterns
    pub triggers: Vec<String>,
    /// Whether this was auto-synthesized
    pub synthesized: bool,
    /// Usage count
    pub usage_count: u32,
    /// Success rate (0.0-1.0)
    pub success_rate: f32,
    /// Last used timestamp
    pub last_used: u64,
    /// Created timestamp
    pub created_at: u64,
    /// Description of the skill
    pub description: Option<String>,
    /// Successful invocations
    pub success_count: u32,
    /// Failed invocations
    pub failure_count: u32,
}

impl SkillCapability {
    /// Create a new skill capability
    pub fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
            triggers: Vec::new(),
            synthesized: true,
            usage_count: 0,
            success_rate: 0.0,
            last_used: 0,
            created_at: current_timestamp(),
            description: None,
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Add a trigger pattern
    pub fn add_trigger(&mut self, trigger: impl Into<String>) {
        self.triggers.push(trigger.into());
    }

    /// Record a usage
    pub fn record_usage(&mut self, success: bool) {
        self.usage_count += 1;
        self.last_used = current_timestamp();

        if success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }

        let total = self.success_count + self.failure_count;
        self.success_rate = if total > 0 {
            self.success_count as f32 / total as f32
        } else {
            0.0
        };
    }

    /// Check if capability is stale
    pub fn is_stale(&self, max_age_days: u32) -> bool {
        let threshold = current_timestamp() - (max_age_days as u64 * 24 * 60 * 60);
        self.last_used < threshold && self.synthesized
    }
}

/// Agent capability metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapability {
    /// Name of the agent
    pub name: String,
    /// Path to AGENT.md file
    pub path: PathBuf,
    /// Specialization description
    pub specialization: String,
    /// Whether this was auto-synthesized
    pub synthesized: bool,
    /// Usage count
    pub usage_count: u32,
    /// Success rate (0.0-1.0)
    pub success_rate: f32,
    /// Last used timestamp
    pub last_used: u64,
    /// Created timestamp
    pub created_at: u64,
    /// Successful invocations
    pub success_count: u32,
    /// Failed invocations
    pub failure_count: u32,
}

impl AgentCapability {
    /// Create a new agent capability
    pub fn new(name: impl Into<String>, path: PathBuf, specialization: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path,
            specialization: specialization.into(),
            synthesized: true,
            usage_count: 0,
            success_rate: 0.0,
            last_used: 0,
            created_at: current_timestamp(),
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Record a usage
    pub fn record_usage(&mut self, success: bool) {
        self.usage_count += 1;
        self.last_used = current_timestamp();

        if success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }

        let total = self.success_count + self.failure_count;
        self.success_rate = if total > 0 {
            self.success_count as f32 / total as f32
        } else {
            0.0
        };
    }

    /// Check if capability is stale
    pub fn is_stale(&self, max_age_days: u32) -> bool {
        let threshold = current_timestamp() - (max_age_days as u64 * 24 * 60 * 60);
        self.last_used < threshold && self.synthesized
    }
}

/// Hook capability metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCapability {
    /// Name of the hook
    pub name: String,
    /// Path to hook script
    pub path: PathBuf,
    /// Hook event type (PreToolUse, PostToolUse, etc.)
    pub event: String,
    /// Tool matcher pattern
    pub matcher: String,
    /// Whether this was auto-synthesized
    pub synthesized: bool,
    /// Usage count
    pub usage_count: u32,
    /// Success rate (0.0-1.0)
    pub success_rate: f32,
    /// Last used timestamp
    pub last_used: u64,
    /// Created timestamp
    pub created_at: u64,
    /// Successful invocations
    pub success_count: u32,
    /// Failed invocations
    pub failure_count: u32,
}

impl HookCapability {
    /// Create a new hook capability
    pub fn new(name: impl Into<String>, path: PathBuf, event: impl Into<String>, matcher: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path,
            event: event.into(),
            matcher: matcher.into(),
            synthesized: true,
            usage_count: 0,
            success_rate: 0.0,
            last_used: 0,
            created_at: current_timestamp(),
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Record a usage
    pub fn record_usage(&mut self, success: bool) {
        self.usage_count += 1;
        self.last_used = current_timestamp();

        if success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }

        let total = self.success_count + self.failure_count;
        self.success_rate = if total > 0 {
            self.success_count as f32 / total as f32
        } else {
            0.0
        };
    }

    /// Check if capability is stale
    pub fn is_stale(&self, max_age_days: u32) -> bool {
        let threshold = current_timestamp() - (max_age_days as u64 * 24 * 60 * 60);
        self.last_used < threshold && self.synthesized
    }
}

/// Wrapper enum for any capability type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Capability {
    Mcp(McpCapability),
    Skill(SkillCapability),
    Agent(AgentCapability),
    Hook(HookCapability),
}

impl Capability {
    /// Get the name of the capability
    pub fn name(&self) -> &str {
        match self {
            Capability::Mcp(c) => &c.name,
            Capability::Skill(c) => &c.name,
            Capability::Agent(c) => &c.name,
            Capability::Hook(c) => &c.name,
        }
    }

    /// Get the capability type
    pub fn capability_type(&self) -> CapabilityType {
        match self {
            Capability::Mcp(_) => CapabilityType::Mcp,
            Capability::Skill(_) => CapabilityType::Skill,
            Capability::Agent(_) => CapabilityType::Agent,
            Capability::Hook(_) => CapabilityType::Hook,
        }
    }

    /// Check if synthesized
    pub fn is_synthesized(&self) -> bool {
        match self {
            Capability::Mcp(c) => c.synthesized,
            Capability::Skill(c) => c.synthesized,
            Capability::Agent(c) => c.synthesized,
            Capability::Hook(c) => c.synthesized,
        }
    }
}

/// Registry of all available capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityRegistry {
    /// Registered MCPs
    pub mcps: Vec<McpCapability>,
    /// Registered skills
    pub skills: Vec<SkillCapability>,
    /// Registered agents
    pub agents: Vec<AgentCapability>,
    /// Registered hooks
    #[serde(default)]
    pub hooks: Vec<HookCapability>,
    /// Last modified timestamp
    pub last_modified: u64,
    /// Version for schema migration
    pub version: u32,
    /// Path to registry file
    #[serde(skip)]
    pub registry_path: Option<PathBuf>,
}

impl CapabilityRegistry {
    /// Current schema version
    const CURRENT_VERSION: u32 = 1;

    /// Create a new empty registry with optional path
    pub fn new(registry_path: &PathBuf) -> Self {
        Self {
            mcps: Vec::new(),
            skills: Vec::new(),
            agents: Vec::new(),
            hooks: Vec::new(),
            last_modified: current_timestamp(),
            version: Self::CURRENT_VERSION,
            registry_path: Some(registry_path.clone()),
        }
    }

    /// Create a new empty registry with default path
    pub fn new_default() -> Self {
        let path = Self::default_path().unwrap_or_else(|_| PathBuf::from(".gogo-gadget/capabilities.json"));
        Self {
            mcps: Vec::new(),
            skills: Vec::new(),
            agents: Vec::new(),
            hooks: Vec::new(),
            last_modified: current_timestamp(),
            version: Self::CURRENT_VERSION,
            registry_path: Some(path),
        }
    }

    /// Load registry from default path (~/.gogo-gadget/capabilities.json)
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::default_path()?;
        Self::load_from(&path)
    }

    /// Load registry or create new if not exists
    pub fn load_or_create() -> anyhow::Result<Self> {
        let path = Self::default_path()?;
        if path.exists() {
            Self::load_from(&path)
        } else {
            let mut registry = Self::new(&path);
            registry.registry_path = Some(path);
            Ok(registry)
        }
    }

    /// Load registry from a specific path
    pub fn load_from(path: &PathBuf) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let mut registry: Self = serde_json::from_str(&content)?;
            registry.registry_path = Some(path.clone());

            // Migrate if needed
            if registry.version < Self::CURRENT_VERSION {
                registry.migrate()?;
            }

            Ok(registry)
        } else {
            let mut registry = Self::new(path);
            registry.registry_path = Some(path.clone());
            Ok(registry)
        }
    }

    /// Get default registry path
    pub fn default_path() -> anyhow::Result<PathBuf> {
        let home = dirs_home().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        Ok(home.join(".gogo-gadget").join("capabilities.json"))
    }

    /// Migrate registry to current version
    fn migrate(&mut self) -> anyhow::Result<()> {
        // Currently only version 1, no migrations needed
        self.version = Self::CURRENT_VERSION;
        Ok(())
    }

    /// Save registry to file
    pub fn save(&mut self) -> anyhow::Result<()> {
        self.last_modified = current_timestamp();

        if let Some(ref path) = self.registry_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(self)?;
            std::fs::write(path, content)?;
        }
        Ok(())
    }

    /// Check if we have a capability matching the gap
    pub fn has_capability_for_gap(&self, gap: &CapabilityGap) -> bool {
        match gap {
            CapabilityGap::Mcp { name, .. } => {
                self.mcps.iter().any(|m| m.name.eq_ignore_ascii_case(name))
            }
            CapabilityGap::Skill { name, .. } => {
                self.skills.iter().any(|s| s.name.eq_ignore_ascii_case(name))
            }
            CapabilityGap::Agent { name, .. } => {
                self.agents.iter().any(|a| a.name.eq_ignore_ascii_case(name))
            }
            CapabilityGap::Hook { name, .. } => {
                self.hooks.iter().any(|h| h.name.eq_ignore_ascii_case(name))
            }
        }
    }

    /// Check if we have a capability with the given name
    pub fn has_capability(&self, name: &str) -> bool {
        self.mcps.iter().any(|m| m.name.eq_ignore_ascii_case(name))
            || self.skills.iter().any(|s| s.name.eq_ignore_ascii_case(name))
            || self.agents.iter().any(|a| a.name.eq_ignore_ascii_case(name))
            || self.hooks.iter().any(|h| h.name.eq_ignore_ascii_case(name))
    }

    /// Find a capability by name
    pub fn find_by_name(&self, name: &str) -> Option<Capability> {
        // Check MCPs
        if let Some(mcp) = self.mcps.iter().find(|m| m.name.eq_ignore_ascii_case(name)) {
            return Some(Capability::Mcp(mcp.clone()));
        }

        // Check skills
        if let Some(skill) = self.skills.iter().find(|s| s.name.eq_ignore_ascii_case(name)) {
            return Some(Capability::Skill(skill.clone()));
        }

        // Check agents
        if let Some(agent) = self.agents.iter().find(|a| a.name.eq_ignore_ascii_case(name)) {
            return Some(Capability::Agent(agent.clone()));
        }

        None
    }

    /// Find a synthesized capability by ID
    pub fn find_by_id(&self, id: &str) -> Option<SynthesizedCapability> {
        // Search through all capability types
        for mcp in &self.mcps {
            let expected_id = format!(
                "mcp-{}-{}",
                mcp.name.to_lowercase().replace(' ', "-"),
                mcp.created_at
            );
            if expected_id == id || mcp.name == id {
                return Some(SynthesizedCapability {
                    id: expected_id,
                    capability_type: CapabilityType::Mcp,
                    name: mcp.name.clone(),
                    path: mcp.path.clone(),
                    config: mcp.config.clone(),
                    synthesized: mcp.synthesized,
                    usage_count: mcp.usage_count,
                    success_rate: mcp.success_rate,
                    created_at: mcp.created_at,
                });
            }
        }

        for skill in &self.skills {
            let expected_id = format!(
                "skill-{}-{}",
                skill.name.to_lowercase().replace(' ', "-"),
                skill.created_at
            );
            if expected_id == id || skill.name == id {
                return Some(SynthesizedCapability {
                    id: expected_id,
                    capability_type: CapabilityType::Skill,
                    name: skill.name.clone(),
                    path: skill.path.clone(),
                    config: serde_json::Value::Null,
                    synthesized: skill.synthesized,
                    usage_count: skill.usage_count,
                    success_rate: skill.success_rate,
                    created_at: skill.created_at,
                });
            }
        }

        for agent in &self.agents {
            let expected_id = format!(
                "agent-{}-{}",
                agent.name.to_lowercase().replace(' ', "-"),
                agent.created_at
            );
            if expected_id == id || agent.name == id {
                return Some(SynthesizedCapability {
                    id: expected_id,
                    capability_type: CapabilityType::Agent,
                    name: agent.name.clone(),
                    path: agent.path.clone(),
                    config: serde_json::Value::Null,
                    synthesized: agent.synthesized,
                    usage_count: agent.usage_count,
                    success_rate: agent.success_rate,
                    created_at: agent.created_at,
                });
            }
        }

        None
    }

    /// Find capabilities by type
    pub fn find_by_type(&self, _capability_type: CapabilityType) -> Vec<&SynthesizedCapability> {
        // This returns references to SynthesizedCapability, but our storage uses different types
        // We'll return an empty vec for now and rely on the alternative methods
        Vec::new()
    }

    /// List all capabilities as SynthesizedCapability
    pub fn list_all(&self) -> Vec<&SynthesizedCapability> {
        // Same issue - storage format differs from SynthesizedCapability
        Vec::new()
    }

    /// Get all capabilities as a vector (owned values)
    pub fn all_capabilities(&self) -> Vec<SynthesizedCapability> {
        let mut result = Vec::new();

        for mcp in &self.mcps {
            result.push(SynthesizedCapability {
                id: format!(
                    "mcp-{}-{}",
                    mcp.name.to_lowercase().replace(' ', "-"),
                    mcp.created_at
                ),
                capability_type: CapabilityType::Mcp,
                name: mcp.name.clone(),
                path: mcp.path.clone(),
                config: mcp.config.clone(),
                synthesized: mcp.synthesized,
                usage_count: mcp.usage_count,
                success_rate: mcp.success_rate,
                created_at: mcp.created_at,
            });
        }

        for skill in &self.skills {
            result.push(SynthesizedCapability {
                id: format!(
                    "skill-{}-{}",
                    skill.name.to_lowercase().replace(' ', "-"),
                    skill.created_at
                ),
                capability_type: CapabilityType::Skill,
                name: skill.name.clone(),
                path: skill.path.clone(),
                config: serde_json::Value::Null,
                synthesized: skill.synthesized,
                usage_count: skill.usage_count,
                success_rate: skill.success_rate,
                created_at: skill.created_at,
            });
        }

        for agent in &self.agents {
            result.push(SynthesizedCapability {
                id: format!(
                    "agent-{}-{}",
                    agent.name.to_lowercase().replace(' ', "-"),
                    agent.created_at
                ),
                capability_type: CapabilityType::Agent,
                name: agent.name.clone(),
                path: agent.path.clone(),
                config: serde_json::Value::Null,
                synthesized: agent.synthesized,
                usage_count: agent.usage_count,
                success_rate: agent.success_rate,
                created_at: agent.created_at,
            });
        }

        result
    }

    /// Get capabilities of a specific type (owned values)
    pub fn capabilities_of_type(&self, capability_type: CapabilityType) -> Vec<SynthesizedCapability> {
        self.all_capabilities()
            .into_iter()
            .filter(|c| c.capability_type == capability_type)
            .collect()
    }

    /// Unregister a capability by ID
    pub fn unregister(&mut self, id: &str) -> anyhow::Result<()> {
        // Try to find and remove by ID or name
        if let Some(cap) = self.find_by_id(id) {
            self.remove(&cap.name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Capability not found: {}", id))
        }
    }

    /// Register a new capability
    pub fn register(&mut self, capability: SynthesizedCapability) -> anyhow::Result<()> {
        match capability.capability_type {
            CapabilityType::Mcp => {
                // Check for duplicate
                if self.mcps.iter().any(|m| m.name == capability.name) {
                    return Err(anyhow::anyhow!("MCP '{}' already registered", capability.name));
                }

                self.mcps.push(McpCapability {
                    name: capability.name,
                    path: capability.path,
                    config: capability.config,
                    synthesized: capability.synthesized,
                    usage_count: 0,
                    last_used: 0,
                    success_rate: 0.0,
                    created_at: current_timestamp(),
                    description: None,
                    success_count: 0,
                    failure_count: 0,
                });
            }
            CapabilityType::Skill => {
                // Check for duplicate
                if self.skills.iter().any(|s| s.name == capability.name) {
                    return Err(anyhow::anyhow!("Skill '{}' already registered", capability.name));
                }

                self.skills.push(SkillCapability {
                    name: capability.name,
                    path: capability.path,
                    triggers: Vec::new(),
                    synthesized: capability.synthesized,
                    usage_count: 0,
                    success_rate: 0.0,
                    last_used: 0,
                    created_at: current_timestamp(),
                    description: None,
                    success_count: 0,
                    failure_count: 0,
                });
            }
            CapabilityType::Agent => {
                // Check for duplicate
                if self.agents.iter().any(|a| a.name == capability.name) {
                    return Err(anyhow::anyhow!("Agent '{}' already registered", capability.name));
                }

                self.agents.push(AgentCapability {
                    name: capability.name,
                    path: capability.path,
                    specialization: String::new(),
                    synthesized: capability.synthesized,
                    usage_count: 0,
                    success_rate: 0.0,
                    last_used: 0,
                    created_at: current_timestamp(),
                    success_count: 0,
                    failure_count: 0,
                });
            }
            CapabilityType::Hook => {
                // Check for duplicate
                if self.hooks.iter().any(|h| h.name == capability.name) {
                    return Err(anyhow::anyhow!("Hook '{}' already registered", capability.name));
                }

                // Extract event and matcher from config
                let event = capability.config
                    .get("event")
                    .and_then(|v| v.as_str())
                    .unwrap_or("PreToolUse")
                    .to_string();
                let matcher = capability.config
                    .get("matcher")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*")
                    .to_string();

                self.hooks.push(HookCapability {
                    name: capability.name,
                    path: capability.path,
                    event,
                    matcher,
                    synthesized: capability.synthesized,
                    usage_count: 0,
                    success_rate: 0.0,
                    last_used: 0,
                    created_at: current_timestamp(),
                    success_count: 0,
                    failure_count: 0,
                });
            }
        }

        self.last_modified = current_timestamp();
        Ok(())
    }

    /// Register or update a capability
    pub fn upsert(&mut self, capability: SynthesizedCapability) -> anyhow::Result<()> {
        match capability.capability_type {
            CapabilityType::Mcp => {
                if let Some(mcp) = self.mcps.iter_mut().find(|m| m.name == capability.name) {
                    mcp.path = capability.path;
                    mcp.config = capability.config;
                } else {
                    return self.register(capability);
                }
            }
            CapabilityType::Skill => {
                if let Some(skill) = self.skills.iter_mut().find(|s| s.name == capability.name) {
                    skill.path = capability.path;
                } else {
                    return self.register(capability);
                }
            }
            CapabilityType::Agent => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.name == capability.name) {
                    agent.path = capability.path;
                } else {
                    return self.register(capability);
                }
            }
            CapabilityType::Hook => {
                if let Some(hook) = self.hooks.iter_mut().find(|h| h.name == capability.name) {
                    hook.path = capability.path;
                    // Update event and matcher from config
                    if let Some(event) = capability.config.get("event").and_then(|v| v.as_str()) {
                        hook.event = event.to_string();
                    }
                    if let Some(matcher) = capability.config.get("matcher").and_then(|v| v.as_str()) {
                        hook.matcher = matcher.to_string();
                    }
                } else {
                    return self.register(capability);
                }
            }
        }

        self.last_modified = current_timestamp();
        Ok(())
    }

    /// Record usage of a capability
    pub fn record_usage(&mut self, name: &str, success: bool) {
        // Check MCPs
        if let Some(mcp) = self.mcps.iter_mut().find(|m| m.name == name) {
            mcp.record_usage(success);
            self.last_modified = current_timestamp();
            return;
        }

        // Check skills
        if let Some(skill) = self.skills.iter_mut().find(|s| s.name == name) {
            skill.record_usage(success);
            self.last_modified = current_timestamp();
            return;
        }

        // Check agents
        if let Some(agent) = self.agents.iter_mut().find(|a| a.name == name) {
            agent.record_usage(success);
            self.last_modified = current_timestamp();
        }
    }

    /// Prune unused capabilities older than max_age_days
    pub fn prune_unused(&mut self, max_age_days: u32) -> Vec<SynthesizedCapability> {
        let mut pruned = Vec::new();

        // Prune MCPs
        let (keep, remove): (Vec<_>, Vec<_>) = self
            .mcps
            .drain(..)
            .partition(|m| !m.is_stale(max_age_days));
        self.mcps = keep;
        for mcp in remove {
            pruned.push(SynthesizedCapability {
                id: format!(
                    "mcp-{}-{}",
                    mcp.name.to_lowercase().replace(' ', "-"),
                    mcp.created_at
                ),
                capability_type: CapabilityType::Mcp,
                name: mcp.name,
                path: mcp.path,
                config: mcp.config,
                synthesized: mcp.synthesized,
                usage_count: mcp.usage_count,
                success_rate: mcp.success_rate,
                created_at: mcp.created_at,
            });
        }

        // Prune skills
        let (keep, remove): (Vec<_>, Vec<_>) = self
            .skills
            .drain(..)
            .partition(|s| !s.is_stale(max_age_days));
        self.skills = keep;
        for skill in remove {
            pruned.push(SynthesizedCapability {
                id: format!(
                    "skill-{}-{}",
                    skill.name.to_lowercase().replace(' ', "-"),
                    skill.created_at
                ),
                capability_type: CapabilityType::Skill,
                name: skill.name,
                path: skill.path,
                config: serde_json::Value::Null,
                synthesized: skill.synthesized,
                usage_count: skill.usage_count,
                success_rate: skill.success_rate,
                created_at: skill.created_at,
            });
        }

        // Prune agents
        let (keep, remove): (Vec<_>, Vec<_>) = self
            .agents
            .drain(..)
            .partition(|a| !a.is_stale(max_age_days));
        self.agents = keep;
        for agent in remove {
            pruned.push(SynthesizedCapability {
                id: format!(
                    "agent-{}-{}",
                    agent.name.to_lowercase().replace(' ', "-"),
                    agent.created_at
                ),
                capability_type: CapabilityType::Agent,
                name: agent.name,
                path: agent.path,
                config: serde_json::Value::Null,
                synthesized: agent.synthesized,
                usage_count: agent.usage_count,
                success_rate: agent.success_rate,
                created_at: agent.created_at,
            });
        }

        if !pruned.is_empty() {
            self.last_modified = current_timestamp();
        }

        pruned
    }

    /// Remove a capability by name
    pub fn remove(&mut self, name: &str) -> Option<Capability> {
        // Check MCPs
        if let Some(idx) = self.mcps.iter().position(|m| m.name == name) {
            let removed = self.mcps.remove(idx);
            self.last_modified = current_timestamp();
            return Some(Capability::Mcp(removed));
        }

        // Check skills
        if let Some(idx) = self.skills.iter().position(|s| s.name == name) {
            let removed = self.skills.remove(idx);
            self.last_modified = current_timestamp();
            return Some(Capability::Skill(removed));
        }

        // Check agents
        if let Some(idx) = self.agents.iter().position(|a| a.name == name) {
            let removed = self.agents.remove(idx);
            self.last_modified = current_timestamp();
            return Some(Capability::Agent(removed));
        }

        None
    }

    /// Get total capability count
    pub fn total_count(&self) -> usize {
        self.mcps.len() + self.skills.len() + self.agents.len()
    }

    /// Get synthesized capability count
    pub fn synthesized_count(&self) -> usize {
        self.mcps.iter().filter(|m| m.synthesized).count()
            + self.skills.iter().filter(|s| s.synthesized).count()
            + self.agents.iter().filter(|a| a.synthesized).count()
    }

    /// Get all capabilities sorted by usage
    pub fn by_usage(&self) -> Vec<Capability> {
        let mut all: Vec<Capability> = Vec::new();

        for mcp in &self.mcps {
            all.push(Capability::Mcp(mcp.clone()));
        }
        for skill in &self.skills {
            all.push(Capability::Skill(skill.clone()));
        }
        for agent in &self.agents {
            all.push(Capability::Agent(agent.clone()));
        }
        for hook in &self.hooks {
            all.push(Capability::Hook(hook.clone()));
        }

        all.sort_by(|a, b| {
            let usage_a = match a {
                Capability::Mcp(c) => c.usage_count,
                Capability::Skill(c) => c.usage_count,
                Capability::Agent(c) => c.usage_count,
                Capability::Hook(c) => c.usage_count,
            };
            let usage_b = match b {
                Capability::Mcp(c) => c.usage_count,
                Capability::Skill(c) => c.usage_count,
                Capability::Agent(c) => c.usage_count,
                Capability::Hook(c) => c.usage_count,
            };
            usage_b.cmp(&usage_a)
        });

        all
    }

    /// Get capabilities with low success rate
    pub fn low_performers(&self, threshold: f32) -> Vec<Capability> {
        let mut result = Vec::new();

        for mcp in &self.mcps {
            if mcp.usage_count > 0 && mcp.success_rate < threshold {
                result.push(Capability::Mcp(mcp.clone()));
            }
        }
        for skill in &self.skills {
            if skill.usage_count > 0 && skill.success_rate < threshold {
                result.push(Capability::Skill(skill.clone()));
            }
        }
        for agent in &self.agents {
            if agent.usage_count > 0 && agent.success_rate < threshold {
                result.push(Capability::Agent(agent.clone()));
            }
        }

        result
    }

    /// Get registry statistics
    pub fn stats(&self) -> RegistryStats {
        RegistryStats {
            total_mcps: self.mcps.len(),
            total_skills: self.skills.len(),
            total_agents: self.agents.len(),
            synthesized_mcps: self.mcps.iter().filter(|m| m.synthesized).count(),
            synthesized_skills: self.skills.iter().filter(|s| s.synthesized).count(),
            synthesized_agents: self.agents.iter().filter(|a| a.synthesized).count(),
            total_usages: self.mcps.iter().map(|m| m.usage_count as u64).sum::<u64>()
                + self.skills.iter().map(|s| s.usage_count as u64).sum::<u64>()
                + self.agents.iter().map(|a| a.usage_count as u64).sum::<u64>(),
        }
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_mcps: usize,
    pub total_skills: usize,
    pub total_agents: usize,
    pub synthesized_mcps: usize,
    pub synthesized_skills: usize,
    pub synthesized_agents: usize,
    pub total_usages: u64,
}

/// Get home directory (platform-independent)
fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_registry_new() {
        let registry = CapabilityRegistry::new_default();
        assert!(registry.mcps.is_empty());
        assert!(registry.skills.is_empty());
        assert!(registry.agents.is_empty());
    }

    #[test]
    fn test_registry_register() {
        let mut registry = CapabilityRegistry::new_default();
        let cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "test-skill",
            PathBuf::from("/tmp/test.md"),
        );
        registry.register(cap).unwrap();
        assert_eq!(registry.skills.len(), 1);
    }

    #[test]
    fn test_registry_duplicate_register() {
        let mut registry = CapabilityRegistry::new_default();
        let cap1 = SynthesizedCapability::new(
            CapabilityType::Skill,
            "test-skill",
            PathBuf::from("/tmp/test.md"),
        );
        let cap2 = SynthesizedCapability::new(
            CapabilityType::Skill,
            "test-skill",
            PathBuf::from("/tmp/test2.md"),
        );
        registry.register(cap1).unwrap();
        assert!(registry.register(cap2).is_err());
    }

    #[test]
    fn test_has_capability() {
        let mut registry = CapabilityRegistry::new_default();
        let cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "my-skill",
            PathBuf::from("/tmp/skill.md"),
        );
        registry.register(cap).unwrap();

        // Test has_capability with name
        assert!(registry.has_capability("my-skill"));
        assert!(registry.has_capability("MY-SKILL")); // Case insensitive
        assert!(!registry.has_capability("missing-skill"));

        // Test has_capability_for_gap
        let gap = CapabilityGap::Skill {
            name: "my-skill".to_string(),
            trigger: "test".to_string(),
            purpose: "test".to_string(),
        };
        assert!(registry.has_capability_for_gap(&gap));

        let missing_gap = CapabilityGap::Skill {
            name: "missing-skill".to_string(),
            trigger: "test".to_string(),
            purpose: "test".to_string(),
        };
        assert!(!registry.has_capability_for_gap(&missing_gap));
    }

    #[test]
    fn test_find_by_name() {
        let mut registry = CapabilityRegistry::new_default();
        let cap = SynthesizedCapability::new(
            CapabilityType::Mcp,
            "github-api",
            PathBuf::from("/tmp/mcp"),
        );
        registry.register(cap).unwrap();

        assert!(registry.find_by_name("github-api").is_some());
        assert!(registry.find_by_name("GitHub-API").is_some()); // Case insensitive
        assert!(registry.find_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_record_usage() {
        let mut registry = CapabilityRegistry::new_default();
        let cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "test-skill",
            PathBuf::from("/tmp/skill.md"),
        );
        registry.register(cap).unwrap();

        registry.record_usage("test-skill", true);
        registry.record_usage("test-skill", true);
        registry.record_usage("test-skill", false);

        let skill = &registry.skills[0];
        assert_eq!(skill.usage_count, 3);
        assert_eq!(skill.success_count, 2);
        assert_eq!(skill.failure_count, 1);
        assert!((skill.success_rate - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_remove() {
        let mut registry = CapabilityRegistry::new_default();
        let cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "to-remove",
            PathBuf::from("/tmp/skill.md"),
        );
        registry.register(cap).unwrap();
        assert_eq!(registry.skills.len(), 1);

        let removed = registry.remove("to-remove");
        assert!(removed.is_some());
        assert_eq!(registry.skills.len(), 0);
    }

    #[test]
    fn test_total_count() {
        let mut registry = CapabilityRegistry::new_default();
        assert_eq!(registry.total_count(), 0);

        registry
            .register(SynthesizedCapability::new(
                CapabilityType::Skill,
                "s1",
                PathBuf::from("/tmp/s1.md"),
            ))
            .unwrap();
        registry
            .register(SynthesizedCapability::new(
                CapabilityType::Mcp,
                "m1",
                PathBuf::from("/tmp/m1"),
            ))
            .unwrap();

        assert_eq!(registry.total_count(), 2);
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test_registry.json");

        let mut registry = CapabilityRegistry::new_default();
        registry.registry_path = Some(path.clone());
        registry
            .register(SynthesizedCapability::new(
                CapabilityType::Skill,
                "test-skill",
                PathBuf::from("/tmp/skill.md"),
            ))
            .unwrap();
        registry.save().unwrap();

        let loaded = CapabilityRegistry::load_from(&path).unwrap();
        assert_eq!(loaded.skills.len(), 1);
        assert_eq!(loaded.skills[0].name, "test-skill");
    }

    #[test]
    fn test_upsert() {
        let mut registry = CapabilityRegistry::new_default();

        // First insert
        registry.upsert(SynthesizedCapability::new(
            CapabilityType::Skill,
            "my-skill",
            PathBuf::from("/tmp/v1.md"),
        )).unwrap();
        assert_eq!(registry.skills.len(), 1);
        assert_eq!(registry.skills[0].path, PathBuf::from("/tmp/v1.md"));

        // Update
        registry.upsert(SynthesizedCapability::new(
            CapabilityType::Skill,
            "my-skill",
            PathBuf::from("/tmp/v2.md"),
        )).unwrap();
        assert_eq!(registry.skills.len(), 1);
        assert_eq!(registry.skills[0].path, PathBuf::from("/tmp/v2.md"));
    }

    #[test]
    fn test_stats() {
        let mut registry = CapabilityRegistry::new_default();
        registry
            .register(SynthesizedCapability::new(
                CapabilityType::Skill,
                "s1",
                PathBuf::from("/tmp/s1.md"),
            ))
            .unwrap();
        registry
            .register(SynthesizedCapability::new(
                CapabilityType::Mcp,
                "m1",
                PathBuf::from("/tmp/m1"),
            ))
            .unwrap();

        let stats = registry.stats();
        assert_eq!(stats.total_skills, 1);
        assert_eq!(stats.total_mcps, 1);
        assert_eq!(stats.synthesized_skills, 1);
        assert_eq!(stats.synthesized_mcps, 1);
    }

    #[test]
    fn test_capability_enum() {
        let mcp = McpCapability::new("test", PathBuf::from("/tmp"));
        let cap = Capability::Mcp(mcp);

        assert_eq!(cap.name(), "test");
        assert_eq!(cap.capability_type(), CapabilityType::Mcp);
        assert!(cap.is_synthesized());
    }
}
