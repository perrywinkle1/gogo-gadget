//! Task Decomposer
//!
//! Decomposes complex tasks into subtasks for parallel execution.
//! Part of the Your Harness platform - optimizing task distribution
//! across multiple agents based on task type and budget constraints.
//!
//! ## Design Philosophy
//!
//! This decomposer follows the Your Harness approach:
//! - Smart decomposition using LLM-based analysis (not rigid patterns)
//! - Budget-aware agent allocation
//! - Harness-specific focus areas for different task types

use super::{DecompositionStrategy, Subtask, SwarmConfig};
use crate::brain::{TaskAnalysis, TaskCategory};
use std::path::{Path, PathBuf};
use tracing::info;

/// Decomposes tasks into subtasks for parallel agent execution.
///
/// The TaskDecomposer is a core component of the Your Harness platform,
/// responsible for breaking down complex tasks into parallelizable subtasks
/// that can be distributed across multiple agents (the "harness").
///
/// ## Budget Optimization
///
/// The decomposer considers budget constraints when allocating agents:
/// - More agents = faster completion but higher cost
/// - Fewer agents = slower but more economical
/// - Smart decomposition minimizes redundant work
pub struct TaskDecomposer {
    /// Working directory for file discovery
    working_dir: PathBuf,
}

impl TaskDecomposer {
    /// Create a new task decomposer
    pub fn new(working_dir: &Path) -> Self {
        Self {
            working_dir: working_dir.to_path_buf(),
        }
    }

    /// Decompose a task into subtasks based on strategy
    pub fn decompose(
        &self,
        task: &str,
        analysis: &TaskAnalysis,
        config: &SwarmConfig,
    ) -> Vec<Subtask> {
        let strategy = self.determine_strategy(config.decomposition_strategy, analysis);

        info!(
            "Decomposing task using {:?} strategy for {} agents",
            strategy, config.agent_count
        );

        match strategy {
            DecompositionStrategy::ByFiles => self.decompose_by_files(task, analysis, config),
            DecompositionStrategy::ByComponents => {
                self.decompose_by_components(task, analysis, config)
            }
            DecompositionStrategy::ByFocus => self.decompose_by_focus(task, analysis, config),
            DecompositionStrategy::Custom => self.decompose_by_focus(task, analysis, config),
            DecompositionStrategy::Auto => self.decompose_by_focus(task, analysis, config),
        }
    }

    /// Determine the best strategy based on analysis
    fn determine_strategy(
        &self,
        requested: DecompositionStrategy,
        analysis: &TaskAnalysis,
    ) -> DecompositionStrategy {
        if requested != DecompositionStrategy::Auto {
            return requested;
        }

        // Auto-select strategy based on task analysis
        if analysis.estimated_files >= 5 {
            DecompositionStrategy::ByFiles
        } else if matches!(
            analysis.category,
            TaskCategory::Feature | TaskCategory::Refactor
        ) {
            DecompositionStrategy::ByComponents
        } else {
            DecompositionStrategy::ByFocus
        }
    }

    /// Decompose by files - each agent handles different files
    fn decompose_by_files(
        &self,
        task: &str,
        analysis: &TaskAnalysis,
        config: &SwarmConfig,
    ) -> Vec<Subtask> {
        let files = self.discover_relevant_files(analysis);
        let agent_count = config.agent_count as usize;

        // If no files found (empty/new project), fall back to ByFocus strategy
        if files.is_empty() {
            info!("No existing files found, using ByFocus strategy for greenfield project");
            return self.decompose_by_focus(task, analysis, config);
        }

        // Distribute files among agents
        let mut subtasks = Vec::new();
        let files_per_agent = (files.len() + agent_count - 1) / agent_count;

        for (i, chunk) in files.chunks(files_per_agent.max(1)).enumerate() {
            if i >= agent_count {
                break;
            }

            let file_list = chunk
                .iter()
                .map(|f| f.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");

            let description = format!(
                "{}\n\n**Agent {} Focus**: Work on these files: {}\n\
                 Only modify these files. Coordinate with other agents working on other files.",
                task,
                i + 1,
                file_list
            );

            subtasks.push(Subtask {
                id: i as u32,
                description,
                focus: format!("Files: {}", file_list),
                target_files: chunk.to_vec(),
                shared_context: None,
                priority: i as u32,
                use_subagent_mode: false,
                subagent_depth: 0,
            });
        }

        // If we have fewer file groups than agents, add general subtasks
        while subtasks.len() < agent_count {
            let i = subtasks.len();
            subtasks.push(self.create_review_subtask(task, i as u32));
        }

        subtasks
    }

    /// Decompose by components (frontend, backend, tests, etc.)
    fn decompose_by_components(
        &self,
        task: &str,
        _analysis: &TaskAnalysis,
        config: &SwarmConfig,
    ) -> Vec<Subtask> {
        let components = self.detect_project_components();
        let agent_count = config.agent_count as usize;

        let mut subtasks = Vec::new();

        // Assign components to agents
        for (i, component) in components.iter().enumerate().take(agent_count) {
            let description = format!(
                "{}\n\n**Agent {} Focus**: {}\n\
                 Focus only on {} related code. Other agents handle other components.",
                task,
                i + 1,
                component.description,
                component.name
            );

            subtasks.push(Subtask {
                id: i as u32,
                description,
                focus: component.name.clone(),
                target_files: component.files.clone(),
                shared_context: None,
                priority: i as u32,
                use_subagent_mode: false,
                subagent_depth: 0,
            });
        }

        // Fill remaining agents with review/verification tasks
        while subtasks.len() < agent_count {
            let i = subtasks.len();
            subtasks.push(self.create_review_subtask(task, i as u32));
        }

        subtasks
    }

    /// Decompose by focus areas - same task with different perspectives
    fn decompose_by_focus(
        &self,
        task: &str,
        analysis: &TaskAnalysis,
        config: &SwarmConfig,
    ) -> Vec<Subtask> {
        let agent_count = config.agent_count as usize;
        let focus_areas = self.generate_focus_areas(analysis, agent_count);

        focus_areas
            .into_iter()
            .enumerate()
            .map(|(i, focus)| {
                let description = format!(
                    "{}\n\n**Agent {} Focus**: {}\n{}\n\
                     Be thorough in your specific focus area while being aware of the overall task.",
                    task,
                    i + 1,
                    focus.name,
                    focus.instructions
                );

                Subtask {
                    id: i as u32,
                    description,
                    focus: focus.name,
                    target_files: vec![],
                    shared_context: None,
                    priority: i as u32,
                    use_subagent_mode: false,
                    subagent_depth: 0,
                }
            })
            .collect()
    }

    /// Generate focus areas based on task analysis
    fn generate_focus_areas(&self, analysis: &TaskAnalysis, count: usize) -> Vec<FocusArea> {
        let mut areas = Vec::new();

        // Primary implementation focus
        areas.push(FocusArea {
            name: "Core Implementation".to_string(),
            instructions: "Focus on implementing the main functionality. \
                          Write clean, working code that accomplishes the primary goal."
                .to_string(),
        });

        // Testing focus
        if count > 1 {
            areas.push(FocusArea {
                name: "Testing & Verification".to_string(),
                instructions: "Focus on writing tests and verifying the implementation works. \
                              Add unit tests, integration tests, and verify edge cases."
                    .to_string(),
            });
        }

        // Documentation/cleanup focus
        if count > 2 {
            areas.push(FocusArea {
                name: "Documentation & Polish".to_string(),
                instructions: "Focus on documentation, error messages, and code quality. \
                              Add comments where needed, improve error handling, ensure code is readable."
                    .to_string(),
            });
        }

        // Architecture/design focus
        if count > 3 {
            areas.push(FocusArea {
                name: "Architecture & Integration".to_string(),
                instructions: "Focus on how this fits into the larger codebase. \
                              Ensure proper integration, check for conflicts with existing code, \
                              suggest architectural improvements."
                    .to_string(),
            });
        }

        // Performance focus
        if count > 4 {
            areas.push(FocusArea {
                name: "Performance & Optimization".to_string(),
                instructions: "Focus on performance aspects. \
                              Identify potential bottlenecks, optimize hot paths, \
                              ensure efficient resource usage."
                    .to_string(),
            });
        }

        // Security focus for security-sensitive tasks
        if analysis.is_security_sensitive && count > 5 {
            areas.push(FocusArea {
                name: "Security Review".to_string(),
                instructions: "Focus on security aspects. \
                              Check for vulnerabilities, ensure proper input validation, \
                              review authentication and authorization logic."
                    .to_string(),
            });
        }

        // Add generic review areas if we need more
        while areas.len() < count {
            let i = areas.len();
            areas.push(FocusArea {
                name: format!("Additional Review #{}", i - 4),
                instructions: format!(
                    "Review iteration {}. Check the work done by other agents, \
                     look for issues, suggest improvements, and fill any gaps.",
                    i - 4
                ),
            });
        }

        areas.truncate(count);
        areas
    }

    /// Discover relevant files in the working directory
    fn discover_relevant_files(&self, analysis: &TaskAnalysis) -> Vec<PathBuf> {
        let extensions = self.get_relevant_extensions(analysis);
        let mut files = Vec::new();

        self.collect_files(&self.working_dir, &extensions, &mut files, 0, 4);

        // Sort by relevance (files in src/ first, then others)
        files.sort_by(|a, b| {
            let a_in_src = a.starts_with(self.working_dir.join("src"));
            let b_in_src = b.starts_with(self.working_dir.join("src"));
            b_in_src.cmp(&a_in_src)
        });

        files
    }

    /// Get relevant file extensions based on detected language
    fn get_relevant_extensions(&self, analysis: &TaskAnalysis) -> Vec<&'static str> {
        use crate::brain::DetectedLanguage;

        match analysis.detected_language {
            DetectedLanguage::Rust => vec!["rs", "toml"],
            DetectedLanguage::Python => vec!["py", "pyi"],
            DetectedLanguage::JavaScript => vec!["js", "jsx", "json"],
            DetectedLanguage::TypeScript => vec!["ts", "tsx", "json"],
            DetectedLanguage::Go => vec!["go"],
            DetectedLanguage::Java => vec!["java"],
            DetectedLanguage::CppOrC => vec!["c", "cpp", "cc", "h", "hpp"],
            DetectedLanguage::Ruby => vec!["rb"],
            DetectedLanguage::Php => vec!["php"],
            DetectedLanguage::Swift => vec!["swift"],
            DetectedLanguage::Kotlin => vec!["kt", "kts"],
            DetectedLanguage::Shell => vec!["sh", "bash"],
            DetectedLanguage::Mixed | DetectedLanguage::Unknown => {
                vec![
                    "rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "rb", "php",
                ]
            }
        }
    }

    /// Recursively collect files with matching extensions
    fn collect_files(
        &self,
        dir: &Path,
        extensions: &[&str],
        files: &mut Vec<PathBuf>,
        depth: usize,
        max_depth: usize,
    ) {
        if depth > max_depth {
            return;
        }

        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden files and common non-source directories
            if file_name.starts_with('.')
                || file_name == "node_modules"
                || file_name == "target"
                || file_name == "__pycache__"
                || file_name == "venv"
                || file_name == ".git"
                || file_name == "dist"
                || file_name == "build"
            {
                continue;
            }

            if path.is_dir() {
                self.collect_files(&path, extensions, files, depth + 1, max_depth);
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext) {
                    files.push(path);
                }
            }
        }
    }

    /// Detect project components (frontend, backend, tests, etc.)
    fn detect_project_components(&self) -> Vec<ProjectComponent> {
        let mut components = Vec::new();

        // Check for common project structures
        let check_dirs = [
            ("src", "Source Code", "Main source code"),
            ("lib", "Library", "Library code"),
            ("tests", "Tests", "Test files"),
            ("test", "Tests", "Test files"),
            ("frontend", "Frontend", "Frontend/UI code"),
            ("client", "Client", "Client-side code"),
            ("backend", "Backend", "Backend/server code"),
            ("server", "Server", "Server-side code"),
            ("api", "API", "API endpoints"),
            ("docs", "Documentation", "Documentation files"),
            ("scripts", "Scripts", "Build and utility scripts"),
            ("config", "Configuration", "Configuration files"),
        ];

        for (dir, name, desc) in check_dirs {
            let path = self.working_dir.join(dir);
            if path.exists() && path.is_dir() {
                let files = self.collect_component_files(&path);
                if !files.is_empty() {
                    components.push(ProjectComponent {
                        name: name.to_string(),
                        description: desc.to_string(),
                        files,
                    });
                }
            }
        }

        // If no specific structure found, create generic components
        if components.is_empty() {
            components.push(ProjectComponent {
                name: "Source".to_string(),
                description: "All source files".to_string(),
                files: self.discover_relevant_files(&crate::brain::TaskAnalysis::default()),
            });
        }

        components
    }

    /// Collect files in a component directory
    fn collect_component_files(&self, dir: &Path) -> Vec<PathBuf> {
        let extensions = vec![
            "rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "rb", "php", "c", "cpp", "h",
        ];
        let mut files = Vec::new();
        self.collect_files(dir, &extensions, &mut files, 0, 3);
        files
    }

    /// Create a review/verification subtask
    fn create_review_subtask(&self, task: &str, id: u32) -> Subtask {
        Subtask {
            id,
            description: format!(
                "{}\n\n**Agent {} Focus**: Review and Verification\n\
                 Review the work done by other agents. Run tests, check for issues, \
                 verify the implementation is complete and correct.",
                task,
                id + 1
            ),
            focus: "Review and Verification".to_string(),
            target_files: vec![],
            shared_context: None,
            priority: id + 100, // Lower priority, runs after main tasks
            use_subagent_mode: false,
            subagent_depth: 0,
        }
    }
}

/// A focus area for an agent
struct FocusArea {
    name: String,
    instructions: String,
}

/// A detected project component
struct ProjectComponent {
    name: String,
    description: String,
    files: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::TaskAnalysis;
    use tempfile::TempDir;

    #[test]
    fn test_decompose_by_focus() {
        let temp_dir = TempDir::new().unwrap();
        let decomposer = TaskDecomposer::new(temp_dir.path());

        let config = SwarmConfig {
            agent_count: 3,
            decomposition_strategy: DecompositionStrategy::ByFocus,
            ..Default::default()
        };

        let analysis = TaskAnalysis::default();
        let subtasks = decomposer.decompose("Build a feature", &analysis, &config);

        assert_eq!(subtasks.len(), 3);
        assert!(subtasks[0].focus.contains("Core"));
        assert!(subtasks[1].focus.contains("Testing"));
        assert!(subtasks[2].focus.contains("Documentation"));
    }

    #[test]
    fn test_generate_focus_areas() {
        let temp_dir = TempDir::new().unwrap();
        let decomposer = TaskDecomposer::new(temp_dir.path());
        let analysis = TaskAnalysis::default();

        let areas = decomposer.generate_focus_areas(&analysis, 5);
        assert_eq!(areas.len(), 5);
        assert!(areas[0].name.contains("Core"));
        assert!(areas[1].name.contains("Testing"));
    }
}
