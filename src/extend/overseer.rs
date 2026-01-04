//! Creative Overseer Module
//!
//! The Creative Overseer runs in parallel with swarm execution to proactively
//! brainstorm and synthesize new capabilities (MCPs, Skills, Agents) before
//! gaps are encountered.
//!
//! Unlike reactive gap detection (which waits for failures), the overseer:
//! 1. Analyzes the task decomposition upfront
//! 2. Brainstorms what tools/capabilities would help
//! 3. Synthesizes high-confidence ideas proactively
//! 4. Learns from execution patterns to improve future suggestions

use crate::extend::{
    CapabilityGap, CapabilityRegistry, CapabilityType, SynthesisEngine, SynthesizedCapability,
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Configuration for the Creative Overseer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverseerConfig {
    /// Whether the overseer is enabled
    pub enabled: bool,

    /// Maximum ideas to generate per iteration
    pub max_ideas_per_iteration: u32,

    /// Minimum confidence threshold to trigger synthesis (0.0-1.0)
    pub min_confidence_to_synthesize: f32,

    /// Model to use for brainstorming (e.g., "haiku", "sonnet")
    pub brainstorm_model: String,

    /// Working directory for synthesis
    pub working_dir: PathBuf,

    /// Whether to synthesize immediately or queue for next iteration
    pub immediate_synthesis: bool,
}

impl Default for OverseerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_ideas_per_iteration: 3,
            min_confidence_to_synthesize: 0.7,
            brainstorm_model: "opus".to_string(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            immediate_synthesis: true,
        }
    }
}

/// Effort level estimate for implementing a capability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffortLevel {
    /// Simple capability, quick to implement
    Low,
    /// Moderate complexity
    Medium,
    /// Complex capability, significant work
    High,
}

impl Default for EffortLevel {
    fn default() -> Self {
        Self::Medium
    }
}

/// A brainstormed capability idea
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityIdea {
    /// Type of capability (MCP, Skill, Agent)
    pub capability_type: CapabilityType,

    /// Short identifier name
    pub name: String,

    /// What the capability does
    pub description: String,

    /// Why it would help the current task
    pub rationale: String,

    /// Confidence score (0.0-1.0)
    pub confidence: f32,

    /// Estimated effort to implement
    pub effort_estimate: EffortLevel,

    /// Whether this idea was synthesized (internal state, not from LLM)
    #[serde(default)]
    pub synthesized: bool,

    /// Whether synthesis was successful (internal state, not from LLM)
    #[serde(default)]
    pub synthesis_success: Option<bool>,
}

impl CapabilityIdea {
    /// Convert to a CapabilityGap for synthesis
    pub fn to_gap(&self) -> CapabilityGap {
        match self.capability_type {
            CapabilityType::Mcp => CapabilityGap::Mcp {
                name: self.name.clone(),
                purpose: self.description.clone(),
                api_hint: Some(self.rationale.clone()),
            },
            CapabilityType::Skill => CapabilityGap::Skill {
                name: self.name.clone(),
                trigger: format!("/{}", self.name.to_lowercase().replace(' ', "-")),
                purpose: self.description.clone(),
            },
            CapabilityType::Agent => CapabilityGap::Agent {
                name: self.name.clone(),
                specialization: self.description.clone(),
            },
        }
    }
}

/// Result of an overseer brainstorming session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainstormResult {
    /// Ideas generated in this session
    pub ideas: Vec<CapabilityIdea>,

    /// Ideas that were synthesized
    pub synthesized_count: u32,

    /// Ideas queued for later
    pub queued_count: u32,

    /// Time taken for brainstorming (ms)
    pub duration_ms: u64,
}

/// The Creative Overseer agent
pub struct CreativeOverseer {
    config: OverseerConfig,
    registry: Arc<Mutex<CapabilityRegistry>>,
    synthesis_engine: SynthesisEngine,
    ideas_generated: Vec<CapabilityIdea>,
    ideas_queued: Vec<CapabilityIdea>,
}

impl CreativeOverseer {
    /// Create a new Creative Overseer
    pub fn new(config: OverseerConfig, registry: Arc<Mutex<CapabilityRegistry>>) -> Self {
        let output_dir = config.working_dir.join(".jarvis").join("synthesized");
        let synthesis_engine = SynthesisEngine::new(&output_dir);

        Self {
            config,
            registry,
            synthesis_engine,
            ideas_generated: Vec::new(),
            ideas_queued: Vec::new(),
        }
    }

    /// Create with default configuration
    pub fn with_defaults(registry: Arc<Mutex<CapabilityRegistry>>) -> Self {
        Self::new(OverseerConfig::default(), registry)
    }

    /// Brainstorm capabilities for a task
    pub async fn brainstorm(
        &mut self,
        task: &str,
        agent_assignments: &[String],
        existing_capabilities: &[String],
    ) -> Result<BrainstormResult> {
        let start = std::time::Instant::now();

        if !self.config.enabled {
            return Ok(BrainstormResult {
                ideas: vec![],
                synthesized_count: 0,
                queued_count: 0,
                duration_ms: 0,
            });
        }

        info!("Creative Overseer brainstorming capabilities for task...");

        // Build the brainstorming prompt
        let prompt = self.build_brainstorm_prompt(task, agent_assignments, existing_capabilities);

        // Call Claude for brainstorming
        let response = self.call_claude(&prompt)?;

        // Parse ideas from response
        let ideas = self.parse_ideas(&response)?;

        debug!("Generated {} capability ideas", ideas.len());

        // Process ideas
        let mut synthesized_count = 0;
        let mut queued_count = 0;

        for mut idea in ideas {
            if idea.confidence >= self.config.min_confidence_to_synthesize {
                if self.config.immediate_synthesis {
                    // Try to synthesize immediately
                    match self.synthesize_idea(&idea).await {
                        Ok(_) => {
                            idea.synthesized = true;
                            idea.synthesis_success = Some(true);
                            synthesized_count += 1;
                            info!("Synthesized capability: {} ({})", idea.name, idea.capability_type.as_str());
                        }
                        Err(e) => {
                            idea.synthesis_success = Some(false);
                            warn!("Failed to synthesize {}: {}", idea.name, e);
                        }
                    }
                } else {
                    // Queue for later
                    self.ideas_queued.push(idea.clone());
                    queued_count += 1;
                }
            }
            self.ideas_generated.push(idea);
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(BrainstormResult {
            ideas: self.ideas_generated.clone(),
            synthesized_count,
            queued_count,
            duration_ms,
        })
    }

    /// Process queued ideas (call between iterations)
    pub async fn process_queue(&mut self) -> Result<u32> {
        let mut synthesized = 0;

        while let Some(mut idea) = self.ideas_queued.pop() {
            match self.synthesize_idea(&idea).await {
                Ok(_) => {
                    idea.synthesized = true;
                    idea.synthesis_success = Some(true);
                    synthesized += 1;
                    info!("Synthesized queued capability: {}", idea.name);
                }
                Err(e) => {
                    idea.synthesis_success = Some(false);
                    warn!("Failed to synthesize queued {}: {}", idea.name, e);
                }
            }
        }

        Ok(synthesized)
    }

    /// Learn from execution results
    pub fn learn_from_results(&mut self, agent_results: &[crate::swarm::AgentResult]) {
        // Analyze which synthesized capabilities were actually used
        // This would update confidence scores for future brainstorming

        for result in agent_results {
            // Check if any synthesized capabilities were mentioned in the result
            for idea in &mut self.ideas_generated {
                if idea.synthesized {
                    // Simple heuristic: if the capability name appears in the result summary
                    if let Some(summary) = result.result.summary.as_ref() {
                        if summary.to_lowercase().contains(&idea.name.to_lowercase()) {
                            debug!("Capability {} appears to have been used", idea.name);
                            // Could update registry usage stats here
                        }
                    }
                }
            }
        }
    }

    /// Get all generated ideas
    pub fn get_ideas(&self) -> &[CapabilityIdea] {
        &self.ideas_generated
    }

    /// Get queued ideas count
    pub fn queued_count(&self) -> usize {
        self.ideas_queued.len()
    }

    /// Build the brainstorming prompt
    fn build_brainstorm_prompt(
        &self,
        task: &str,
        agent_assignments: &[String],
        existing_capabilities: &[String],
    ) -> String {
        let agents_str = if agent_assignments.is_empty() {
            "No specific agent assignments yet".to_string()
        } else {
            agent_assignments
                .iter()
                .enumerate()
                .map(|(i, a)| format!("Agent {}: {}", i + 1, a))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let capabilities_str = if existing_capabilities.is_empty() {
            "None registered".to_string()
        } else {
            existing_capabilities.join(", ")
        };

        format!(
            r#"You are a Creative Overseer analyzing a task to proactively identify helpful capabilities.

## Task
{task}

## Agent Assignments
{agents_str}

## Existing Capabilities
{capabilities_str}

## Your Job
Brainstorm what tools, APIs, or capabilities would help complete this task MORE EFFECTIVELY.

Think about:
- What external APIs or services would be useful? (→ MCP)
- What reusable patterns or workflows would help? (→ Skill)
- What specialized agent behaviors would be beneficial? (→ Agent)

## Response Format
Respond with a JSON array of capability ideas (max {max}):

```json
[
  {{
    "capability_type": "Mcp" | "Skill" | "Agent",
    "name": "short-identifier",
    "description": "What it does",
    "rationale": "Why it helps THIS specific task",
    "confidence": 0.0-1.0,
    "effort_estimate": "Low" | "Medium" | "High"
  }}
]
```

Only suggest capabilities with confidence >= 0.5. Be specific about WHY each would help.
If no new capabilities would significantly help, return an empty array [].
"#,
            task = task,
            agents_str = agents_str,
            capabilities_str = capabilities_str,
            max = self.config.max_ideas_per_iteration
        )
    }

    /// Parse ideas from Claude's response
    fn parse_ideas(&self, response: &str) -> Result<Vec<CapabilityIdea>> {
        // Extract JSON from response (may be wrapped in markdown code blocks)
        let json_str = if let Some(start) = response.find('[') {
            if let Some(end) = response.rfind(']') {
                &response[start..=end]
            } else {
                return Ok(vec![]);
            }
        } else {
            return Ok(vec![]);
        };

        // Parse JSON
        let ideas: Vec<CapabilityIdea> = serde_json::from_str(json_str).map_err(|e| {
            debug!("Failed to parse ideas JSON: {}", e);
            anyhow!("Failed to parse capability ideas: {}", e)
        })?;

        // Filter and limit
        let filtered: Vec<CapabilityIdea> = ideas
            .into_iter()
            .filter(|i| i.confidence >= 0.5)
            .take(self.config.max_ideas_per_iteration as usize)
            .collect();

        Ok(filtered)
    }

    /// Call Claude for brainstorming
    fn call_claude(&self, prompt: &str) -> Result<String> {
        let model = match self.config.brainstorm_model.as_str() {
            "haiku" => "haiku",
            "sonnet" => "sonnet",
            "opus" => "opus",
            other => other,
        };

        let output = Command::new("claude")
            .arg("--print")
            .arg("--model")
            .arg(model)
            .arg("-p")
            .arg(prompt)
            .current_dir(&self.config.working_dir)
            .output()
            .map_err(|e| anyhow!("Failed to run claude CLI: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Claude CLI failed: {}", stderr));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Synthesize a capability idea
    async fn synthesize_idea(&self, idea: &CapabilityIdea) -> Result<SynthesizedCapability> {
        let gap = idea.to_gap();

        // Use the synthesis engine
        let result = self.synthesis_engine.synthesize(&gap).await?;

        // Register the capability
        {
            let mut registry = self.registry.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
            registry.register(result.capability.clone())?;
        }

        Ok(result.capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overseer_config_default() {
        let config = OverseerConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_ideas_per_iteration, 3);
        assert_eq!(config.min_confidence_to_synthesize, 0.7);
        assert_eq!(config.brainstorm_model, "opus");
    }

    #[test]
    fn test_capability_idea_to_gap() {
        let idea = CapabilityIdea {
            capability_type: CapabilityType::Mcp,
            name: "stripe-payments".to_string(),
            description: "Stripe payment processing API".to_string(),
            rationale: "Handle payment flows".to_string(),
            confidence: 0.9,
            effort_estimate: EffortLevel::Medium,
            synthesized: false,
            synthesis_success: None,
        };

        let gap = idea.to_gap();
        assert_eq!(gap.name(), "stripe-payments");
        assert_eq!(gap.capability_type(), CapabilityType::Mcp);
    }

    #[test]
    fn test_parse_ideas_json() {
        let overseer = CreativeOverseer::new(
            OverseerConfig::default(),
            Arc::new(Mutex::new(CapabilityRegistry::new_default())),
        );

        let response = r#"
Here are some ideas:
```json
[
  {
    "capability_type": "Mcp",
    "name": "github-api",
    "description": "GitHub API access",
    "rationale": "Fetch repo data",
    "confidence": 0.8,
    "effort_estimate": "Low",
    "synthesized": false,
    "synthesis_success": null
  }
]
```
"#;

        let ideas = overseer.parse_ideas(response).unwrap();
        assert_eq!(ideas.len(), 1);
        assert_eq!(ideas[0].name, "github-api");
        assert_eq!(ideas[0].confidence, 0.8);
    }

    #[test]
    fn test_effort_level_default() {
        assert_eq!(EffortLevel::default(), EffortLevel::Medium);
    }
}
