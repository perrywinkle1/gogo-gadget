//! Hot Loader Module
//!
//! Registers capabilities without requiring restart.
//!
//! ## Design
//!
//! - Skills can be loaded without restart (written to ~/.claude/skills/)
//! - Agents can be loaded without restart (written to ~/.claude/agents/)
//! - MCPs require Claude settings update and restart
//!
//! ## MCP Restart Strategy
//!
//! Since Claude Code requires a restart to pick up new MCP configurations,
//! the loader:
//! 1. Writes the MCP server code to the appropriate location
//! 2. Updates ~/.claude/settings.json with the MCP configuration
//! 3. Returns LoadResult with `restart_required: true`
//! 4. The caller should inform the user to restart Claude

use super::{CapabilityType, SynthesizedCapability};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info, warn};

/// Result of loading a capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadResult {
    /// Whether loading succeeded
    pub success: bool,
    /// Message describing the result
    pub message: String,
    /// Whether a restart is required
    pub restart_required: bool,
    /// Path where capability was installed
    pub installed_path: Option<PathBuf>,
    /// Configuration that was applied
    pub config_applied: Option<serde_json::Value>,
}

impl LoadResult {
    /// Create a successful load result
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            restart_required: false,
            installed_path: None,
            config_applied: None,
        }
    }

    /// Create a result requiring restart
    pub fn requires_restart(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            restart_required: true,
            installed_path: None,
            config_applied: None,
        }
    }

    /// Create a failed result
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            restart_required: false,
            installed_path: None,
            config_applied: None,
        }
    }

    /// Add installed path to result
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.installed_path = Some(path);
        self
    }

    /// Add config to result
    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config_applied = Some(config);
        self
    }
}

/// MCP server configuration in Claude settings
#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpServerConfig {
    command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    env: HashMap<String, String>,
}

/// Hot loader for registering capabilities at runtime
#[derive(Debug, Clone)]
pub struct HotLoader {
    /// Path to Claude settings file (for MCPs: ~/.claude.json)
    claude_settings_path: PathBuf,
    /// Path to Claude Code settings file (for hooks: ~/.claude/settings.json)
    hooks_settings_path: PathBuf,
    /// Directory for skills
    skills_dir: PathBuf,
    /// Directory for agents
    agents_dir: PathBuf,
    /// Directory for MCP servers
    mcp_dir: PathBuf,
    /// Directory for hook scripts
    hooks_dir: PathBuf,
}

impl Default for HotLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl HotLoader {
    /// Create a new hot loader with default paths
    pub fn new() -> Self {
        let home = dirs_home().unwrap_or_else(|| PathBuf::from("."));
        let claude_dir = home.join(".claude");
        Self {
            // Claude Code reads MCP config from ~/.claude.json (NOT settings.json)
            claude_settings_path: home.join(".claude.json"),
            // Claude Code reads hooks from ~/.claude/settings.json
            hooks_settings_path: claude_dir.join("settings.json"),
            skills_dir: claude_dir.join("skills"),
            agents_dir: claude_dir.join("agents"),
            mcp_dir: claude_dir.join("mcp-servers"),
            hooks_dir: claude_dir.join("hooks"),
        }
    }

    /// Create with custom paths
    pub fn with_paths(
        claude_settings: PathBuf,
        skills_dir: PathBuf,
        agents_dir: PathBuf,
    ) -> Self {
        let home = dirs_home().unwrap_or_else(|| PathBuf::from("."));
        let claude_dir = home.join(".claude");
        Self {
            claude_settings_path: claude_settings,
            hooks_settings_path: claude_dir.join("settings.json"),
            skills_dir,
            agents_dir,
            mcp_dir: claude_dir.join("mcp-servers"),
            hooks_dir: claude_dir.join("hooks"),
        }
    }

    /// Register an MCP capability
    ///
    /// This involves:
    /// 1. Copying MCP server code to ~/.claude/mcp-servers/{name}/
    /// 2. Running npm install in the MCP directory
    /// 3. Updating ~/.claude/settings.json with the MCP configuration
    /// 4. Returning that a restart is required
    pub fn register_mcp(&self, capability: &SynthesizedCapability) -> Result<LoadResult> {
        info!("Registering MCP: {}", capability.name);

        // Ensure MCP directory exists
        let mcp_dest = self.mcp_dir.join(&capability.name);
        fs::create_dir_all(&mcp_dest).context("Failed to create MCP destination directory")?;

        // Copy MCP server files
        if capability.path.is_dir() {
            copy_dir_recursive(&capability.path, &mcp_dest)?;
        } else if capability.path.is_file() {
            let dest_file = mcp_dest.join(capability.path.file_name().unwrap_or_default());
            fs::copy(&capability.path, &dest_file)?;
        }
        debug!("Copied MCP files to {:?}", mcp_dest);

        // Check if package.json exists and run npm install
        let package_json = mcp_dest.join("package.json");
        if package_json.exists() {
            info!("Running npm install for MCP: {}", capability.name);
            let output = Command::new("npm")
                .arg("install")
                .current_dir(&mcp_dest)
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    debug!("npm install succeeded for MCP: {}", capability.name);
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    warn!("npm install failed for MCP {}: {}", capability.name, stderr);
                }
                Err(e) => {
                    warn!("Failed to run npm install for MCP {}: {}", capability.name, e);
                }
            }
        }

        // Update Claude settings
        let mcp_config = self.create_mcp_config(&capability.name, &mcp_dest)?;
        self.update_claude_settings_mcp(&capability.name, &mcp_config)?;

        let config_json = serde_json::to_value(&mcp_config)?;

        Ok(LoadResult::requires_restart(format!(
            "MCP '{}' registered at {:?}. Restart Claude Code to activate.",
            capability.name, mcp_dest
        ))
        .with_path(mcp_dest)
        .with_config(config_json))
    }

    /// Create MCP configuration for Claude settings
    fn create_mcp_config(&self, _name: &str, mcp_path: &PathBuf) -> Result<McpServerConfig> {
        // Determine the entry point
        let index_ts = mcp_path.join("src").join("index.ts");
        let index_js = mcp_path.join("dist").join("index.js");
        let main_js = mcp_path.join("index.js");

        let (command, args) = if index_js.exists() {
            ("node".to_string(), vec![index_js.display().to_string()])
        } else if main_js.exists() {
            ("node".to_string(), vec![main_js.display().to_string()])
        } else if index_ts.exists() {
            (
                "npx".to_string(),
                vec!["tsx".to_string(), index_ts.display().to_string()],
            )
        } else {
            // Default to running via npm start
            ("npm".to_string(), vec!["start".to_string()])
        };

        Ok(McpServerConfig {
            command,
            args,
            env: HashMap::new(),
        })
    }

    /// Update Claude settings with MCP configuration
    fn update_claude_settings_mcp(&self, name: &str, config: &McpServerConfig) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.claude_settings_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Read existing settings or create new
        let mut settings: serde_json::Value = if self.claude_settings_path.exists() {
            let content = fs::read_to_string(&self.claude_settings_path)?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        // Ensure mcpServers object exists
        if !settings.get("mcpServers").is_some() {
            settings["mcpServers"] = serde_json::json!({});
        }

        // Add the MCP server configuration (Claude Code requires type: "stdio")
        settings["mcpServers"][name] = serde_json::json!({
            "type": "stdio",
            "command": config.command,
            "args": config.args,
            "env": config.env
        });

        // Write back
        let content = serde_json::to_string_pretty(&settings)?;
        fs::write(&self.claude_settings_path, content)?;

        info!(
            "Updated Claude settings with MCP '{}' at {:?}",
            name, self.claude_settings_path
        );

        Ok(())
    }

    /// Register a skill capability
    ///
    /// Skills are markdown files that Claude Code picks up automatically.
    /// Claude Code expects: ~/.claude/skills/{name}/SKILL.md
    /// No restart required.
    pub fn register_skill(&self, capability: &SynthesizedCapability) -> Result<LoadResult> {
        info!("Registering skill: {}", capability.name);

        // Claude Code expects skills in subdirectory structure: skills/{name}/SKILL.md
        let skill_name = capability.name.trim_end_matches(".SKILL.md");
        let skill_dir = self.skills_dir.join(skill_name);
        fs::create_dir_all(&skill_dir).context("Failed to create skill directory")?;

        let dest = skill_dir.join("SKILL.md");

        // Copy the skill file
        if capability.path.is_file() {
            fs::copy(&capability.path, &dest).context("Failed to copy skill file")?;
        } else if capability.path.is_dir() {
            // If it's a directory, look for the skill file inside
            let skill_file = capability.path.join(format!("{}.SKILL.md", skill_name));
            let alt_skill_file = capability.path.join("SKILL.md");
            if skill_file.exists() {
                fs::copy(&skill_file, &dest)?;
            } else if alt_skill_file.exists() {
                fs::copy(&alt_skill_file, &dest)?;
            } else {
                return Err(anyhow::anyhow!(
                    "Could not find skill file in directory {:?}",
                    capability.path
                ));
            }
        } else {
            return Err(anyhow::anyhow!(
                "Skill path does not exist: {:?}",
                capability.path
            ));
        }

        debug!("Skill installed at {:?}", dest);

        Ok(LoadResult::success(format!(
            "Skill '{}' registered and active at {:?}",
            skill_name, skill_dir
        ))
        .with_path(skill_dir))
    }

    /// Register an agent capability
    ///
    /// Agents are flat markdown files with YAML frontmatter.
    /// Claude Code expects: ~/.claude/agents/{name}.md (NOT subdirectories!)
    /// No restart required.
    pub fn register_agent(&self, capability: &SynthesizedCapability) -> Result<LoadResult> {
        info!("Registering agent: {}", capability.name);

        // Ensure agents directory exists
        fs::create_dir_all(&self.agents_dir).context("Failed to create agents directory")?;

        // Claude Code expects flat files: agents/{name}.md (NOT subdirectories)
        let agent_name = capability.name.trim_end_matches(".AGENT.md");
        let dest = self.agents_dir.join(format!("{}.md", agent_name));

        // Read the source agent content
        let content = if capability.path.is_file() {
            fs::read_to_string(&capability.path).context("Failed to read agent file")?
        } else if capability.path.is_dir() {
            let agent_file = capability.path.join(format!("{}.AGENT.md", agent_name));
            let alt_agent_file = capability.path.join("AGENT.md");
            if agent_file.exists() {
                fs::read_to_string(&agent_file)?
            } else if alt_agent_file.exists() {
                fs::read_to_string(&alt_agent_file)?
            } else {
                return Err(anyhow::anyhow!(
                    "Could not find agent file in directory {:?}",
                    capability.path
                ));
            }
        } else {
            return Err(anyhow::anyhow!(
                "Agent path does not exist: {:?}",
                capability.path
            ));
        };

        // Ensure content has YAML frontmatter (required by Claude Code)
        let final_content = if content.starts_with("---\n") {
            content
        } else {
            // Extract description from the first blockquote or first paragraph
            let description = extract_agent_description(&content, agent_name);
            format!(
                "---\nname: {}\ndescription: {}\n---\n\n{}",
                agent_name, description, content
            )
        };

        fs::write(&dest, final_content).context("Failed to write agent file")?;
        debug!("Agent installed at {:?}", dest);

        Ok(LoadResult::success(format!(
            "Agent '{}' registered and active at {:?}",
            agent_name, dest
        ))
        .with_path(dest))
    }

    /// Register a hook capability
    ///
    /// Hooks are shell scripts that run before/after tool execution.
    /// Claude Code expects hooks configured in ~/.claude/settings.json
    /// Hook scripts are stored in ~/.claude/hooks/
    /// Restart required for hook changes to take effect.
    pub fn register_hook(&self, capability: &SynthesizedCapability) -> Result<LoadResult> {
        info!("Registering hook: {}", capability.name);

        // Ensure hooks directory exists
        fs::create_dir_all(&self.hooks_dir).context("Failed to create hooks directory")?;

        // Parse hook configuration from capability config
        let hook_config = &capability.config;
        let event = hook_config
            .get("event")
            .and_then(|v| v.as_str())
            .unwrap_or("PreToolUse");
        let matcher = hook_config
            .get("matcher")
            .and_then(|v| v.as_str())
            .unwrap_or("*");
        let timeout = hook_config
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as u32;

        // Copy hook script to hooks directory
        let hook_name = capability.name.trim_end_matches(".sh");
        let script_dest = self.hooks_dir.join(format!("{}.sh", hook_name));

        if capability.path.is_file() {
            fs::copy(&capability.path, &script_dest).context("Failed to copy hook script")?;
        } else if capability.path.is_dir() {
            // Look for the hook script in the directory
            let script_file = capability.path.join(format!("{}.sh", hook_name));
            let alt_script = capability.path.join("hook.sh");
            if script_file.exists() {
                fs::copy(&script_file, &script_dest)?;
            } else if alt_script.exists() {
                fs::copy(&alt_script, &script_dest)?;
            } else {
                return Err(anyhow::anyhow!(
                    "Could not find hook script in directory {:?}",
                    capability.path
                ));
            }
        } else {
            return Err(anyhow::anyhow!(
                "Hook path does not exist: {:?}",
                capability.path
            ));
        }

        // Make script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_dest)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_dest, perms)?;
        }

        // Update Claude settings to register the hook
        self.update_claude_settings_hook(hook_name, event, matcher, &script_dest, timeout)?;

        debug!("Hook installed at {:?}", script_dest);

        Ok(LoadResult::requires_restart(format!(
            "Hook '{}' registered at {:?}. Restart Claude Code to activate.",
            hook_name, script_dest
        ))
        .with_path(script_dest))
    }

    /// Update Claude settings with hook configuration
    fn update_claude_settings_hook(
        &self,
        name: &str,
        event: &str,
        matcher: &str,
        script_path: &PathBuf,
        timeout: u32,
    ) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.hooks_settings_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Read existing settings or create new
        let mut settings: serde_json::Value = if self.hooks_settings_path.exists() {
            let content = fs::read_to_string(&self.hooks_settings_path)?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        // Ensure hooks object exists
        if settings.get("hooks").is_none() {
            settings["hooks"] = serde_json::json!({});
        }

        // Ensure event array exists
        if settings["hooks"].get(event).is_none() {
            settings["hooks"][event] = serde_json::json!([]);
        }

        // Build the command string
        let command = format!("bash {}", script_path.display());

        // Create the new hook entry
        let new_hook_entry = serde_json::json!({
            "matcher": matcher,
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": timeout
            }]
        });

        // Check if a hook with this matcher already exists; update it if so
        let hooks_array = settings["hooks"][event].as_array_mut();
        if let Some(arr) = hooks_array {
            let mut found = false;
            for entry in arr.iter_mut() {
                if entry.get("matcher").and_then(|m| m.as_str()) == Some(matcher) {
                    // Update existing entry
                    *entry = new_hook_entry.clone();
                    found = true;
                    break;
                }
            }
            if !found {
                arr.push(new_hook_entry);
            }
        }

        // Write back
        let content = serde_json::to_string_pretty(&settings)?;
        fs::write(&self.hooks_settings_path, content)?;

        info!(
            "Updated Claude settings with hook '{}' ({}) at {:?}",
            name, event, self.hooks_settings_path
        );

        Ok(())
    }

    /// Register any capability based on its type
    pub fn register(&self, capability: &SynthesizedCapability) -> Result<LoadResult> {
        match capability.capability_type {
            CapabilityType::Mcp => self.register_mcp(capability),
            CapabilityType::Skill => self.register_skill(capability),
            CapabilityType::Agent => self.register_agent(capability),
            CapabilityType::Hook => self.register_hook(capability),
        }
    }

    /// Load a capability (alias for register)
    ///
    /// This is the method called by task_loop and swarm coordinator to install
    /// synthesized capabilities at runtime.
    pub fn load(&self, capability: &SynthesizedCapability) -> Result<LoadResult> {
        self.register(capability)
    }

    /// Unregister an MCP capability
    pub fn unregister_mcp(&self, name: &str) -> Result<LoadResult> {
        // Remove from settings
        if self.claude_settings_path.exists() {
            let content = fs::read_to_string(&self.claude_settings_path)?;
            let mut settings: serde_json::Value = serde_json::from_str(&content)?;

            if let Some(mcp_servers) = settings.get_mut("mcpServers") {
                if let Some(obj) = mcp_servers.as_object_mut() {
                    obj.remove(name);
                }
            }

            let content = serde_json::to_string_pretty(&settings)?;
            fs::write(&self.claude_settings_path, content)?;
        }

        // Remove MCP directory
        let mcp_path = self.mcp_dir.join(name);
        if mcp_path.exists() {
            fs::remove_dir_all(&mcp_path)?;
        }

        Ok(LoadResult::requires_restart(format!(
            "MCP '{}' unregistered. Restart Claude Code to complete removal.",
            name
        )))
    }

    /// Unregister a skill capability
    pub fn unregister_skill(&self, name: &str) -> Result<LoadResult> {
        let skill_name = name.trim_end_matches(".SKILL.md");

        // Try subdirectory structure first (Claude Code standard)
        let dir_path = self.skills_dir.join(skill_name);
        if dir_path.is_dir() {
            fs::remove_dir_all(&dir_path)?;
            return Ok(LoadResult::success(format!("Skill '{}' unregistered", skill_name)));
        }

        // Fallback to legacy flat file structure
        let file_path = self.skills_dir.join(format!("{}.SKILL.md", skill_name));
        if file_path.exists() {
            fs::remove_file(&file_path)?;
            return Ok(LoadResult::success(format!("Skill '{}' unregistered", skill_name)));
        }

        Ok(LoadResult::failure(format!(
            "Skill '{}' not found",
            skill_name
        )))
    }

    /// Unregister an agent capability
    pub fn unregister_agent(&self, name: &str) -> Result<LoadResult> {
        let agent_name = name.trim_end_matches(".AGENT.md").trim_end_matches(".md");

        // Claude Code uses flat files: agents/{name}.md
        let file_path = self.agents_dir.join(format!("{}.md", agent_name));
        if file_path.exists() {
            fs::remove_file(&file_path)?;
            return Ok(LoadResult::success(format!("Agent '{}' unregistered", agent_name)));
        }

        // Fallback: try legacy subdirectory structure
        let dir_path = self.agents_dir.join(agent_name);
        if dir_path.is_dir() {
            fs::remove_dir_all(&dir_path)?;
            return Ok(LoadResult::success(format!("Agent '{}' unregistered (legacy dir)", agent_name)));
        }

        // Fallback: try .AGENT.md extension
        let legacy_path = self.agents_dir.join(format!("{}.AGENT.md", agent_name));
        if legacy_path.exists() {
            fs::remove_file(&legacy_path)?;
            return Ok(LoadResult::success(format!("Agent '{}' unregistered", agent_name)));
        }

        Ok(LoadResult::failure(format!(
            "Agent '{}' not found",
            agent_name
        )))
    }

    /// Unregister a hook capability
    pub fn unregister_hook(&self, name: &str) -> Result<LoadResult> {
        let hook_name = name.trim_end_matches(".sh");

        // Remove hook script
        let script_path = self.hooks_dir.join(format!("{}.sh", hook_name));
        if script_path.exists() {
            fs::remove_file(&script_path)?;
        }

        // Note: We don't remove from settings.json automatically as hooks may have
        // multiple scripts or be manually configured. User should manage this.

        Ok(LoadResult::requires_restart(format!(
            "Hook '{}' script removed. Manually update ~/.claude/settings.json if needed.",
            hook_name
        )))
    }

    /// List all installed skills
    pub fn list_skills(&self) -> Result<Vec<String>> {
        if !self.skills_dir.exists() {
            return Ok(Vec::new());
        }

        let mut skills = Vec::new();
        for entry in fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            // Claude Code uses subdirectory structure: skills/{name}/SKILL.md
            if path.is_dir() {
                let skill_file = path.join("SKILL.md");
                if skill_file.exists() {
                    if let Some(name) = path.file_name() {
                        skills.push(name.to_string_lossy().to_string());
                    }
                }
            }
            // Also support legacy flat file structure
            if path.is_file() {
                if let Some(name) = path.file_name() {
                    let name = name.to_string_lossy();
                    if name.ends_with(".SKILL.md") {
                        skills.push(name.trim_end_matches(".SKILL.md").to_string());
                    }
                }
            }
        }
        Ok(skills)
    }

    /// List all installed agents
    pub fn list_agents(&self) -> Result<Vec<String>> {
        if !self.agents_dir.exists() {
            return Ok(Vec::new());
        }

        let mut agents = Vec::new();
        for entry in fs::read_dir(&self.agents_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Claude Code uses flat files: agents/{name}.md
            if path.is_file() {
                if let Some(name) = path.file_name() {
                    let name = name.to_string_lossy();
                    if name.ends_with(".md") {
                        agents.push(name.trim_end_matches(".md").to_string());
                    } else if name.ends_with(".AGENT.md") {
                        // Legacy format
                        agents.push(name.trim_end_matches(".AGENT.md").to_string());
                    }
                }
            }

            // Also support legacy subdirectory structure
            if path.is_dir() {
                let agent_file = path.join("AGENT.md");
                if agent_file.exists() {
                    if let Some(name) = path.file_name() {
                        agents.push(name.to_string_lossy().to_string());
                    }
                }
            }
        }
        Ok(agents)
    }

    /// List all registered MCPs
    pub fn list_mcps(&self) -> Result<Vec<String>> {
        if !self.claude_settings_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.claude_settings_path)?;
        let settings: serde_json::Value = serde_json::from_str(&content)?;

        let mut mcps = Vec::new();
        if let Some(mcp_servers) = settings.get("mcpServers") {
            if let Some(obj) = mcp_servers.as_object() {
                for key in obj.keys() {
                    mcps.push(key.clone());
                }
            }
        }
        Ok(mcps)
    }

    /// List all installed hooks
    pub fn list_hooks(&self) -> Result<Vec<String>> {
        if !self.hooks_dir.exists() {
            return Ok(Vec::new());
        }

        let mut hooks = Vec::new();
        for entry in fs::read_dir(&self.hooks_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name() {
                    let name = name.to_string_lossy();
                    if name.ends_with(".sh") {
                        hooks.push(name.trim_end_matches(".sh").to_string());
                    }
                }
            }
        }
        Ok(hooks)
    }

    /// Reload Claude config (signal that changes were made)
    ///
    /// Note: This is a no-op for now as Claude Code doesn't support hot reloading.
    /// The user must restart Claude Code manually for MCP changes.
    pub fn reload_claude_config(&self) -> Result<()> {
        info!("Claude config reload requested - manual restart required for MCPs");
        Ok(())
    }

    /// Get claude settings path
    pub fn claude_settings_path(&self) -> &PathBuf {
        &self.claude_settings_path
    }

    /// Get skills directory
    pub fn skills_dir(&self) -> &PathBuf {
        &self.skills_dir
    }

    /// Get agents directory
    pub fn agents_dir(&self) -> &PathBuf {
        &self.agents_dir
    }

    /// Get MCP directory
    pub fn mcp_dir(&self) -> &PathBuf {
        &self.mcp_dir
    }

    /// Get hooks directory
    pub fn hooks_dir(&self) -> &PathBuf {
        &self.hooks_dir
    }

    /// Get hooks settings path
    pub fn hooks_settings_path(&self) -> &PathBuf {
        &self.hooks_settings_path
    }
}

/// Get home directory
fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

/// Extract description from agent markdown content
/// Looks for blockquote (> description) or first paragraph after title
fn extract_agent_description(content: &str, fallback_name: &str) -> String {
    // Try to find a blockquote description (common pattern: > Description here)
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("> ") {
            let desc = trimmed.trim_start_matches("> ").trim();
            if !desc.is_empty() && desc.len() > 10 {
                // Escape any quotes and limit length
                let escaped = desc.replace('"', "'");
                return if escaped.len() > 200 {
                    format!("{}...", &escaped[..197])
                } else {
                    escaped
                };
            }
        }
    }

    // Fallback: generate from name
    format!(
        "Specialized agent for {}",
        fallback_name.replace('-', " ").replace('_', " ")
    )
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            // Skip node_modules and other common excluded directories
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if matches!(
                name_str.as_ref(),
                "node_modules" | ".git" | "target" | "dist" | "build"
            ) {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_result_success() {
        let result = LoadResult::success("test");
        assert!(result.success);
        assert!(!result.restart_required);
    }

    #[test]
    fn test_load_result_requires_restart() {
        let result = LoadResult::requires_restart("test");
        assert!(result.success);
        assert!(result.restart_required);
    }

    #[test]
    fn test_load_result_failure() {
        let result = LoadResult::failure("test");
        assert!(!result.success);
    }

    #[test]
    fn test_load_result_with_path() {
        let result = LoadResult::success("test").with_path(PathBuf::from("/tmp/skill.md"));
        assert_eq!(result.installed_path, Some(PathBuf::from("/tmp/skill.md")));
    }

    #[test]
    fn test_hot_loader_new() {
        let loader = HotLoader::new();
        assert!(loader
            .claude_settings_path
            .to_string_lossy()
            .contains("claude"));
    }

    #[test]
    fn test_hot_loader_with_paths() {
        let loader = HotLoader::with_paths(
            PathBuf::from("/tmp/settings.json"),
            PathBuf::from("/tmp/skills"),
            PathBuf::from("/tmp/agents"),
        );
        assert_eq!(loader.skills_dir, PathBuf::from("/tmp/skills"));
    }

    #[test]
    fn test_register_skill() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let source_dir = temp_dir.path().join("source");
        fs::create_dir_all(&source_dir).unwrap();

        // Create a source skill file
        let skill_content = "# Test Skill\n\n## Instructions\nTest instructions\n";
        let source_skill = source_dir.join("test-skill.SKILL.md");
        fs::write(&source_skill, skill_content).unwrap();

        let loader = HotLoader::with_paths(
            temp_dir.path().join("settings.json"),
            skills_dir.clone(),
            temp_dir.path().join("agents"),
        );

        let capability = SynthesizedCapability::new(
            CapabilityType::Skill,
            "test-skill",
            source_skill,
        );

        let result = loader.register_skill(&capability).unwrap();
        assert!(result.success);
        assert!(!result.restart_required);

        // Verify the skill was copied (subdirectory structure)
        let installed_skill = skills_dir.join("test-skill").join("SKILL.md");
        assert!(installed_skill.exists());
        let installed_content = fs::read_to_string(&installed_skill).unwrap();
        assert_eq!(installed_content, skill_content);
    }

    #[test]
    fn test_register_agent() {
        let temp_dir = TempDir::new().unwrap();
        let agents_dir = temp_dir.path().join("agents");
        let source_dir = temp_dir.path().join("source");
        fs::create_dir_all(&source_dir).unwrap();

        // Create a source agent file with blockquote description
        let agent_content = "# Test Agent\n\n> Specialized agent for testing purposes and validation\n\n## Specialization\nTest spec\n";
        let source_agent = source_dir.join("test-agent.AGENT.md");
        fs::write(&source_agent, agent_content).unwrap();

        let loader = HotLoader::with_paths(
            temp_dir.path().join("settings.json"),
            temp_dir.path().join("skills"),
            agents_dir.clone(),
        );

        let capability = SynthesizedCapability::new(
            CapabilityType::Agent,
            "test-agent",
            source_agent,
        );

        let result = loader.register_agent(&capability).unwrap();
        assert!(result.success);
        assert!(!result.restart_required);

        // Verify the agent was written as flat file with YAML frontmatter
        let installed_agent = agents_dir.join("test-agent.md");
        assert!(installed_agent.exists());
        let content = fs::read_to_string(&installed_agent).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("name: test-agent"));
        assert!(content.contains("description:"));
    }

    #[test]
    fn test_list_skills() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create skills with subdirectory structure
        fs::create_dir_all(skills_dir.join("skill1")).unwrap();
        fs::write(skills_dir.join("skill1").join("SKILL.md"), "# Skill 1").unwrap();
        fs::create_dir_all(skills_dir.join("skill2")).unwrap();
        fs::write(skills_dir.join("skill2").join("SKILL.md"), "# Skill 2").unwrap();

        let loader = HotLoader::with_paths(
            temp_dir.path().join("settings.json"),
            skills_dir,
            temp_dir.path().join("agents"),
        );

        let skills = loader.list_skills().unwrap();
        assert_eq!(skills.len(), 2);
        assert!(skills.contains(&"skill1".to_string()));
        assert!(skills.contains(&"skill2".to_string()));
    }

    #[test]
    fn test_unregister_skill() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create a skill with subdirectory structure
        let skill_dir = skills_dir.join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, "# Test Skill").unwrap();
        assert!(skill_file.exists());

        let loader = HotLoader::with_paths(
            temp_dir.path().join("settings.json"),
            skills_dir,
            temp_dir.path().join("agents"),
        );

        let result = loader.unregister_skill("test-skill").unwrap();
        assert!(result.success);
        assert!(!skill_dir.exists());
    }

    #[test]
    fn test_update_claude_settings_mcp() {
        let temp_dir = TempDir::new().unwrap();
        let settings_path = temp_dir.path().join(".claude").join("settings.json");

        let loader = HotLoader::with_paths(
            settings_path.clone(),
            temp_dir.path().join("skills"),
            temp_dir.path().join("agents"),
        );

        let config = McpServerConfig {
            command: "node".to_string(),
            args: vec!["/path/to/server.js".to_string()],
            env: HashMap::new(),
        };

        loader.update_claude_settings_mcp("test-mcp", &config).unwrap();

        // Verify the settings were written
        assert!(settings_path.exists());
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert!(settings["mcpServers"]["test-mcp"].is_object());
        assert_eq!(settings["mcpServers"]["test-mcp"]["command"], "node");
    }

    #[test]
    fn test_copy_dir_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let src = temp_dir.path().join("src");
        let dst = temp_dir.path().join("dst");

        // Create source structure
        fs::create_dir_all(src.join("subdir")).unwrap();
        fs::write(src.join("file1.js"), "content1").unwrap();
        fs::write(src.join("subdir").join("file2.js"), "content2").unwrap();
        fs::create_dir_all(src.join("node_modules")).unwrap();
        fs::write(src.join("node_modules").join("skip.js"), "skip").unwrap();

        copy_dir_recursive(&src, &dst).unwrap();

        // Verify files were copied
        assert!(dst.join("file1.js").exists());
        assert!(dst.join("subdir").join("file2.js").exists());
        // Verify node_modules was skipped
        assert!(!dst.join("node_modules").exists());
    }
}
