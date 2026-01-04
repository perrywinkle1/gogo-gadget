//! Self-Extending Capability Module
//!
//! This module enables gogo-gadget to:
//! 1. Recognize when it would benefit from a new capability (MCP, SKILL.md, AGENT.md)
//! 2. Synthesize that capability
//! 3. Configure/register it
//! 4. Utilize it in subsequent task execution

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod context;
pub mod gap;
pub mod loader;
pub mod overseer;
pub mod registry;
pub mod synthesis;
pub mod verify;

// Re-export main types from gap module
pub use gap::{FailureStats, GapDetector, GapPattern, GapType, TaskAttempt};

// Re-export main types from loader module
pub use loader::{HotLoader, LoadResult};

// Re-export main types from registry module
pub use registry::{
    AgentCapability, Capability, CapabilityRegistry, McpCapability, RegistryStats, SkillCapability,
};

// Re-export main types from synthesis module
pub use synthesis::{
    SynthesisEngine, SynthesisResult, SynthesisTemplate, TemplateRegistry, TemplateType,
};

// Re-export main types from verify module
pub use verify::{
    quick, CapabilityVerifier, VerificationConfig, VerificationResult, VerificationStep,
};

// Re-export main types from overseer module
pub use overseer::{
    BrainstormResult, CapabilityIdea, CreativeOverseer, EffortLevel, OverseerConfig,
};

// Re-export main types from context module
pub use context::{SharedContext, TaskPhase, TaskSnapshot};

/// Type of capability that can be synthesized
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityType {
    /// MCP (Model Context Protocol) server
    Mcp,
    /// SKILL.md skill definition
    Skill,
    /// AGENT.md agent definition
    Agent,
}

impl Default for CapabilityType {
    fn default() -> Self {
        Self::Skill
    }
}

impl CapabilityType {
    /// Get string representation of capability type
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::Skill => "skill",
            Self::Agent => "agent",
        }
    }
}

/// Represents a detected capability gap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CapabilityGap {
    /// Need for an MCP server
    Mcp {
        name: String,
        purpose: String,
        api_hint: Option<String>,
    },
    /// Need for a skill definition
    Skill {
        name: String,
        trigger: String,
        purpose: String,
    },
    /// Need for an agent definition
    Agent {
        name: String,
        specialization: String,
    },
}

impl CapabilityGap {
    /// Get the name of the capability
    pub fn name(&self) -> &str {
        match self {
            Self::Mcp { name, .. } => name,
            Self::Skill { name, .. } => name,
            Self::Agent { name, .. } => name,
        }
    }

    /// Get the type of capability
    pub fn capability_type(&self) -> CapabilityType {
        match self {
            Self::Mcp { .. } => CapabilityType::Mcp,
            Self::Skill { .. } => CapabilityType::Skill,
            Self::Agent { .. } => CapabilityType::Agent,
        }
    }
}

/// A synthesized capability ready for registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizedCapability {
    /// Unique identifier for this capability
    pub id: String,
    /// Type of capability
    pub capability_type: CapabilityType,
    /// Name of the capability
    pub name: String,
    /// Path to the capability files
    pub path: PathBuf,
    /// Configuration for the capability (JSON)
    pub config: serde_json::Value,
    /// Whether this was auto-synthesized
    pub synthesized: bool,
    /// Usage statistics
    pub usage_count: u32,
    /// Success rate (0.0-1.0)
    pub success_rate: f32,
    /// Timestamp when capability was created
    pub created_at: u64,
}

impl SynthesizedCapability {
    /// Create a new synthesized capability
    pub fn new(capability_type: CapabilityType, name: impl Into<String>, path: PathBuf) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let name_str = name.into();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Generate a unique ID based on type, name, and timestamp
        let id = format!(
            "{}-{}-{}",
            capability_type.as_str(),
            name_str.to_lowercase().replace(' ', "-"),
            timestamp
        );

        Self {
            id,
            capability_type,
            name: name_str,
            path,
            config: serde_json::Value::Null,
            synthesized: true,
            usage_count: 0,
            success_rate: 0.0,
            created_at: timestamp,
        }
    }

    /// Create with a specific ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Create with configuration
    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config = config;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_type_default() {
        assert_eq!(CapabilityType::default(), CapabilityType::Skill);
    }

    #[test]
    fn test_capability_gap_name() {
        let gap = CapabilityGap::Mcp {
            name: "test-mcp".to_string(),
            purpose: "testing".to_string(),
            api_hint: None,
        };
        assert_eq!(gap.name(), "test-mcp");
    }

    #[test]
    fn test_capability_gap_type() {
        let mcp_gap = CapabilityGap::Mcp {
            name: "test".to_string(),
            purpose: "test".to_string(),
            api_hint: None,
        };
        assert_eq!(mcp_gap.capability_type(), CapabilityType::Mcp);

        let skill_gap = CapabilityGap::Skill {
            name: "test".to_string(),
            trigger: "test".to_string(),
            purpose: "test".to_string(),
        };
        assert_eq!(skill_gap.capability_type(), CapabilityType::Skill);

        let agent_gap = CapabilityGap::Agent {
            name: "test".to_string(),
            specialization: "test".to_string(),
        };
        assert_eq!(agent_gap.capability_type(), CapabilityType::Agent);
    }

    #[test]
    fn test_synthesized_capability_new() {
        let cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "test-skill",
            PathBuf::from("/tmp/skill.md"),
        );
        assert_eq!(cap.name, "test-skill");
        assert!(cap.id.starts_with("skill-test-skill-"));
        assert!(cap.synthesized);
        assert_eq!(cap.usage_count, 0);
    }
}
