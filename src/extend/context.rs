//! Shared Task Context
//!
//! Provides a shared view of the current task state that the Creative Overseer
//! can observe to proactively create capabilities.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────┐     writes      ┌─────────────────────┐
//! │  Swarm Orchestrator │ ───────────────▶│   SharedContext     │
//! │       (Brain)       │                 │                     │
//! └─────────────────────┘                 │  • current_task     │
//!                                         │  • subtasks         │
//!                                         │  • iteration        │
//!                             reads       │  • capabilities     │
//! ┌─────────────────────┐ ◀───────────────│    _suggested       │
//! │  Creative Overseer  │                 └─────────────────────┘
//! │  (Chief of Product) │
//! └─────────────────────┘
//! ```

use crate::swarm::Subtask;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// The current phase of task execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPhase {
    /// Task is being analyzed
    Analyzing,
    /// Task is being decomposed into subtasks
    Decomposing,
    /// Agents are executing subtasks
    Executing,
    /// Results are being aggregated
    Aggregating,
    /// Verifying the work
    Verifying,
    /// Task is complete
    Complete,
    /// Task failed
    Failed,
}

/// Snapshot of the current task state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    /// The original task description
    pub task: String,

    /// Current phase of execution
    pub phase: TaskPhase,

    /// Current iteration number
    pub iteration: u32,

    /// Subtasks assigned to agents
    pub subtasks: Vec<Subtask>,

    /// Keywords/domains detected in the task
    pub domains: Vec<String>,

    /// Technologies mentioned or detected
    pub technologies: Vec<String>,

    /// Capabilities that have been suggested by the overseer
    pub suggested_capabilities: HashSet<String>,

    /// Capabilities that have been used this session
    pub used_capabilities: HashSet<String>,

    /// Timestamp when task started
    #[serde(skip)]
    pub started_at: Option<Instant>,
}

impl Default for TaskSnapshot {
    fn default() -> Self {
        Self {
            task: String::new(),
            phase: TaskPhase::Analyzing,
            iteration: 0,
            subtasks: Vec::new(),
            domains: Vec::new(),
            technologies: Vec::new(),
            suggested_capabilities: HashSet::new(),
            used_capabilities: HashSet::new(),
            started_at: None,
        }
    }
}

impl TaskSnapshot {
    /// Create a new snapshot for a task
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            task: task.into(),
            started_at: Some(Instant::now()),
            ..Default::default()
        }
    }

    /// Extract domains and technologies from the task description
    pub fn analyze_task(&mut self) {
        let task_lower = self.task.to_lowercase();

        // Detect common domains
        let domain_keywords = [
            ("auth", "authentication"),
            ("login", "authentication"),
            ("jwt", "authentication"),
            ("oauth", "authentication"),
            ("api", "api-development"),
            ("rest", "api-development"),
            ("graphql", "api-development"),
            ("database", "database"),
            ("sql", "database"),
            ("postgres", "database"),
            ("mysql", "database"),
            ("mongo", "database"),
            ("test", "testing"),
            ("spec", "testing"),
            ("deploy", "deployment"),
            ("docker", "containerization"),
            ("kubernetes", "orchestration"),
            ("k8s", "orchestration"),
            ("ci", "ci-cd"),
            ("cd", "ci-cd"),
            ("pipeline", "ci-cd"),
            ("monitor", "observability"),
            ("log", "observability"),
            ("metric", "observability"),
            ("cache", "caching"),
            ("redis", "caching"),
            ("queue", "messaging"),
            ("kafka", "messaging"),
            ("rabbit", "messaging"),
            ("websocket", "realtime"),
            ("socket", "realtime"),
            ("file", "file-handling"),
            ("upload", "file-handling"),
            ("s3", "cloud-storage"),
            ("email", "email"),
            ("smtp", "email"),
            ("pdf", "document-processing"),
            ("csv", "data-processing"),
            ("json", "data-processing"),
            ("xml", "data-processing"),
        ];

        for (keyword, domain) in domain_keywords {
            if task_lower.contains(keyword) && !self.domains.contains(&domain.to_string()) {
                self.domains.push(domain.to_string());
            }
        }

        // Detect technologies
        let tech_keywords = [
            "rust", "python", "javascript", "typescript", "go", "java", "ruby",
            "react", "vue", "angular", "svelte", "next", "nuxt",
            "node", "deno", "bun",
            "express", "fastify", "actix", "axum", "rocket",
            "django", "flask", "fastapi",
            "spring", "rails",
            "postgres", "mysql", "sqlite", "mongodb", "redis",
            "docker", "kubernetes", "terraform", "ansible",
            "aws", "gcp", "azure",
            "git", "github", "gitlab",
        ];

        for tech in tech_keywords {
            if task_lower.contains(tech) && !self.technologies.contains(&tech.to_string()) {
                self.technologies.push(tech.to_string());
            }
        }
    }
}

/// Shared context that both brain and overseer can access
#[derive(Clone)]
pub struct SharedContext {
    inner: Arc<RwLock<TaskSnapshot>>,
}

impl Default for SharedContext {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedContext {
    /// Create a new shared context
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(TaskSnapshot::default())),
        }
    }

    /// Start a new task
    pub fn start_task(&self, task: impl Into<String>) {
        if let Ok(mut snapshot) = self.inner.write() {
            *snapshot = TaskSnapshot::new(task);
            snapshot.analyze_task();
        }
    }

    /// Update the current phase
    pub fn set_phase(&self, phase: TaskPhase) {
        if let Ok(mut snapshot) = self.inner.write() {
            snapshot.phase = phase;
        }
    }

    /// Update the current iteration
    pub fn set_iteration(&self, iteration: u32) {
        if let Ok(mut snapshot) = self.inner.write() {
            snapshot.iteration = iteration;
        }
    }

    /// Set the subtasks
    pub fn set_subtasks(&self, subtasks: Vec<Subtask>) {
        if let Ok(mut snapshot) = self.inner.write() {
            snapshot.subtasks = subtasks;
        }
    }

    /// Record that a capability was used
    pub fn mark_capability_used(&self, name: &str) {
        if let Ok(mut snapshot) = self.inner.write() {
            snapshot.used_capabilities.insert(name.to_string());
        }
    }

    /// Suggest a capability (called by overseer)
    pub fn suggest_capability(&self, name: &str) {
        if let Ok(mut snapshot) = self.inner.write() {
            snapshot.suggested_capabilities.insert(name.to_string());
        }
    }

    /// Get a read-only snapshot of the current state
    pub fn snapshot(&self) -> Option<TaskSnapshot> {
        self.inner.read().ok().map(|s| s.clone())
    }

    /// Get the current task description
    pub fn current_task(&self) -> Option<String> {
        self.inner.read().ok().map(|s| s.task.clone())
    }

    /// Get detected domains
    pub fn domains(&self) -> Vec<String> {
        self.inner
            .read()
            .ok()
            .map(|s| s.domains.clone())
            .unwrap_or_default()
    }

    /// Get detected technologies
    pub fn technologies(&self) -> Vec<String> {
        self.inner
            .read()
            .ok()
            .map(|s| s.technologies.clone())
            .unwrap_or_default()
    }

    /// Get suggested capabilities that haven't been used yet
    pub fn pending_suggestions(&self) -> Vec<String> {
        self.inner
            .read()
            .ok()
            .map(|s| {
                s.suggested_capabilities
                    .difference(&s.used_capabilities)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if a capability has been suggested
    pub fn is_suggested(&self, name: &str) -> bool {
        self.inner
            .read()
            .ok()
            .map(|s| s.suggested_capabilities.contains(name))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_snapshot_analyze() {
        let mut snapshot = TaskSnapshot::new("Add JWT authentication to the REST API");
        snapshot.analyze_task();

        assert!(snapshot.domains.contains(&"authentication".to_string()));
        assert!(snapshot.domains.contains(&"api-development".to_string()));
    }

    #[test]
    fn test_shared_context_lifecycle() {
        let ctx = SharedContext::new();

        ctx.start_task("Deploy Docker containers to Kubernetes");
        ctx.set_phase(TaskPhase::Executing);
        ctx.set_iteration(2);

        let snapshot = ctx.snapshot().unwrap();
        assert_eq!(snapshot.phase, TaskPhase::Executing);
        assert_eq!(snapshot.iteration, 2);
        assert!(snapshot.domains.contains(&"containerization".to_string()));
        assert!(snapshot.domains.contains(&"orchestration".to_string()));
    }

    #[test]
    fn test_capability_suggestions() {
        let ctx = SharedContext::new();
        ctx.start_task("Test task");

        ctx.suggest_capability("auth-helper");
        ctx.suggest_capability("jwt-validator");
        ctx.mark_capability_used("auth-helper");

        let pending = ctx.pending_suggestions();
        assert_eq!(pending.len(), 1);
        assert!(pending.contains(&"jwt-validator".to_string()));
    }
}
