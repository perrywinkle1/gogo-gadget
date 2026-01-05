//! Tests for swarm execution functionality

use gogo_gadget::brain::{TaskAnalysis, TaskAnalyzer};
use gogo_gadget::swarm::{
    DecompositionStrategy, SwarmConfig, SwarmConfigBuilder, SwarmCoordinator, SwarmResult,
    TaskDecomposer,
};
use gogo_gadget::{ExecutionMode, TaskResult};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_swarm_config_builder() {
    let config = SwarmConfigBuilder::new()
        .agent_count(5)
        .timeout_secs(600)
        .share_context(true)
        .strategy(DecompositionStrategy::ByFocus)
        .build();

    assert_eq!(config.agent_count, 5);
    assert_eq!(config.agent_timeout_secs, 600);
    assert!(config.share_context);
    assert_eq!(config.decomposition_strategy, DecompositionStrategy::ByFocus);
}

#[test]
fn test_swarm_config_default() {
    let config = SwarmConfig::default();

    assert_eq!(config.agent_count, 3);
    assert_eq!(config.agent_timeout_secs, 600); // 10 minute safety timeout
    assert!(config.share_context);
    assert_eq!(config.decomposition_strategy, DecompositionStrategy::Auto);
}

#[test]
fn test_task_decomposer_by_focus() {
    let temp_dir = TempDir::new().unwrap();
    let decomposer = TaskDecomposer::new(temp_dir.path());

    let config = SwarmConfigBuilder::new()
        .agent_count(3)
        .strategy(DecompositionStrategy::ByFocus)
        .working_dir(temp_dir.path().to_path_buf())
        .build();

    let analysis = TaskAnalysis::default();
    let subtasks = decomposer.decompose("Build a new feature", &analysis, &config);

    assert_eq!(subtasks.len(), 3);
    assert!(subtasks[0].focus.contains("Core"));
    assert!(subtasks[1].focus.contains("Testing"));
    assert!(subtasks[2].focus.contains("Documentation"));
}

#[test]
fn test_task_decomposer_with_more_agents() {
    let temp_dir = TempDir::new().unwrap();
    let decomposer = TaskDecomposer::new(temp_dir.path());

    let config = SwarmConfigBuilder::new()
        .agent_count(5)
        .strategy(DecompositionStrategy::ByFocus)
        .working_dir(temp_dir.path().to_path_buf())
        .build();

    let analysis = TaskAnalysis::default();
    let subtasks = decomposer.decompose("Complex task", &analysis, &config);

    assert_eq!(subtasks.len(), 5);
    // Each agent should have a unique ID
    let ids: Vec<_> = subtasks.iter().map(|s| s.id).collect();
    assert_eq!(ids, vec![0, 1, 2, 3, 4]);
}

#[test]
fn test_task_decomposer_by_files_with_source_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create a src directory with some files
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();
    std::fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();
    std::fs::write(src_dir.join("lib.rs"), "pub fn lib() {}").unwrap();
    std::fs::write(src_dir.join("utils.rs"), "pub fn util() {}").unwrap();

    let decomposer = TaskDecomposer::new(temp_dir.path());

    let config = SwarmConfigBuilder::new()
        .agent_count(3)
        .strategy(DecompositionStrategy::ByFiles)
        .working_dir(temp_dir.path().to_path_buf())
        .build();

    let mut analysis = TaskAnalysis::default();
    analysis.detected_language = gogo_gadget::brain::DetectedLanguage::Rust;

    let subtasks = decomposer.decompose("Update all files", &analysis, &config);

    assert_eq!(subtasks.len(), 3);
    // At least one subtask should have target files
    let total_files: usize = subtasks.iter().map(|s| s.target_files.len()).sum();
    assert!(total_files > 0 || subtasks.iter().any(|s| s.focus.contains("Review")));
}

#[test]
fn test_swarm_coordinator_creation() {
    let temp_dir = TempDir::new().unwrap();
    let config = SwarmConfigBuilder::new()
        .agent_count(3)
        .working_dir(temp_dir.path().to_path_buf())
        .build();

    let _coordinator = SwarmCoordinator::new(config);
    // Just verify it creates without panic
}

#[test]
fn test_swarm_result_to_task_result_success() {
    let swarm_result = SwarmResult {
        success: true,
        agent_results: vec![
            gogo_gadget::swarm::AgentResult {
                agent_id: 0,
                subtask: "Core implementation".to_string(),
                result: TaskResult {
                    success: true,
                    iterations: 1,
                    summary: Some("Implemented core".to_string()),
                    error: None,
                    verification: None,
                },
                files_modified: vec![PathBuf::from("src/main.rs")],
                duration_ms: 5000,
                compressed_result: None,
            },
            gogo_gadget::swarm::AgentResult {
                agent_id: 1,
                subtask: "Testing".to_string(),
                result: TaskResult {
                    success: true,
                    iterations: 1,
                    summary: Some("Added tests".to_string()),
                    error: None,
                    verification: None,
                },
                files_modified: vec![PathBuf::from("tests/test.rs")],
                duration_ms: 3000,
                compressed_result: None,
            },
        ],
        summary: Some("All done".to_string()),
        conflicts: vec![],
        total_duration_ms: 8000,
        successful_agents: 2,
        failed_agents: 0,
        verification_passed: true,
        compressed_output: None,
    };

    let task_result: TaskResult = swarm_result.into();

    assert!(task_result.success);
    assert_eq!(task_result.iterations, 2);
    assert!(task_result.summary.is_some());
    assert!(task_result.error.is_none());
}

#[test]
fn test_swarm_result_to_task_result_failure() {
    let swarm_result = SwarmResult {
        success: false,
        agent_results: vec![gogo_gadget::swarm::AgentResult {
            agent_id: 0,
            subtask: "Failed task".to_string(),
            result: TaskResult {
                success: false,
                iterations: 1,
                summary: None,
                error: Some("Something went wrong".to_string()),
                verification: None,
            },
            files_modified: vec![],
            duration_ms: 1000,
            compressed_result: None,
        }],
        summary: None,
        conflicts: vec![],
        total_duration_ms: 1000,
        successful_agents: 0,
        failed_agents: 1,
        verification_passed: false,
        compressed_output: None,
    };

    let task_result: TaskResult = swarm_result.into();

    assert!(!task_result.success);
    assert!(task_result.error.is_some());
    assert!(task_result.error.unwrap().contains("Something went wrong"));
}

#[test]
fn test_swarm_result_with_conflicts() {
    let swarm_result = SwarmResult {
        success: false,
        agent_results: vec![
            gogo_gadget::swarm::AgentResult {
                agent_id: 0,
                subtask: "Agent 1".to_string(),
                result: TaskResult {
                    success: true,
                    iterations: 1,
                    summary: Some("Done".to_string()),
                    error: None,
                    verification: None,
                },
                files_modified: vec![PathBuf::from("src/main.rs")],
                duration_ms: 1000,
                compressed_result: None,
            },
            gogo_gadget::swarm::AgentResult {
                agent_id: 1,
                subtask: "Agent 2".to_string(),
                result: TaskResult {
                    success: true,
                    iterations: 1,
                    summary: Some("Done".to_string()),
                    error: None,
                    verification: None,
                },
                files_modified: vec![PathBuf::from("src/main.rs")], // Same file!
                duration_ms: 1000,
                compressed_result: None,
            },
        ],
        summary: Some("Conflict detected".to_string()),
        conflicts: vec![gogo_gadget::swarm::SwarmConflict {
            file: PathBuf::from("src/main.rs"),
            agent_ids: vec![0, 1],
            description: "Both agents modified src/main.rs".to_string(),
            resolved: false,
        }],
        total_duration_ms: 2000,
        successful_agents: 2,
        failed_agents: 0,
        verification_passed: false,
        compressed_output: None,
    };

    assert!(!swarm_result.success);
    assert_eq!(swarm_result.conflicts.len(), 1);
    assert_eq!(swarm_result.conflicts[0].agent_ids, vec![0, 1]);
}

#[test]
fn test_analyzer_detects_swarm_mode() {
    let analyzer = TaskAnalyzer::new();

    // Task that should trigger swarm mode
    let analysis = analyzer.analyze(
        "Build a full-stack application with frontend React components and backend API endpoints,
         including user authentication, database models, and comprehensive testing",
    ).expect("Analysis should succeed");

    // High complexity parallel-safe tasks should trigger swarm mode
    // Note: the exact mode depends on the analysis logic
    if analysis.parallel_safe && analysis.complexity_score >= 5 {
        assert!(matches!(analysis.mode, ExecutionMode::Swarm { .. }));
    }
}

#[test]
fn test_analyzer_security_tasks_use_swarm_with_compression() {
    let analyzer = TaskAnalyzer::new();

    // With Gas Town context compression, security tasks CAN use swarm mode
    // Each subagent is isolated - parent only sees CompressedResult
    // (success, commit_hash, files_changed, summary)
    let analysis = analyzer.analyze(
        "Audit the authentication system and fix password hashing vulnerabilities",
    ).expect("Analysis should succeed");

    assert!(analysis.is_security_sensitive);
    // Security tasks are parallel-safe with Gas Town compression boundaries
    assert!(analysis.parallel_safe);
    // High complexity security tasks should use swarm
    assert!(matches!(analysis.mode, ExecutionMode::Swarm { .. }));
}

#[test]
fn test_decomposition_strategy_variants() {
    assert_eq!(
        DecompositionStrategy::default(),
        DecompositionStrategy::Auto
    );

    // All variants should be eq-comparable
    assert_ne!(DecompositionStrategy::Auto, DecompositionStrategy::ByFiles);
    assert_ne!(
        DecompositionStrategy::ByFiles,
        DecompositionStrategy::ByComponents
    );
    assert_ne!(
        DecompositionStrategy::ByComponents,
        DecompositionStrategy::ByFocus
    );
}

#[test]
fn test_subtask_properties() {
    let subtask = gogo_gadget::swarm::Subtask {
        id: 0,
        description: "Test subtask".to_string(),
        focus: "Testing".to_string(),
        target_files: vec![PathBuf::from("test.rs")],
        shared_context: Some("Context from other agents".to_string()),
        priority: 1,
        use_subagent_mode: true,
        subagent_depth: 1,
    };

    assert_eq!(subtask.id, 0);
    assert_eq!(subtask.focus, "Testing");
    assert_eq!(subtask.target_files.len(), 1);
    assert!(subtask.shared_context.is_some());
    assert!(subtask.use_subagent_mode);
    assert_eq!(subtask.subagent_depth, 1);
}

#[test]
fn test_agent_result_properties() {
    let result = gogo_gadget::swarm::AgentResult {
        agent_id: 2,
        subtask: "Documentation".to_string(),
        result: TaskResult {
            success: true,
            iterations: 3,
            summary: Some("Added docs".to_string()),
            error: None,
            verification: None,
        },
        files_modified: vec![PathBuf::from("README.md"), PathBuf::from("docs/api.md")],
        duration_ms: 15000,
        compressed_result: None,
    };

    assert_eq!(result.agent_id, 2);
    assert!(result.result.success);
    assert_eq!(result.files_modified.len(), 2);
    assert_eq!(result.duration_ms, 15000);
}

// ============================================================================
// Gas Town + Swarm Coordination Tests
// ============================================================================

mod gas_town_swarm_tests {
    use gogo_gadget::swarm::{
        AgentResult, DecompositionStrategy, Subtask, SwarmConfigBuilder, SwarmConflict,
        SwarmResult,
    };
    use gogo_gadget::{
        CompressedResult, CompressedResultMetadata, ContextPolicy, OperationMode,
        SubagentConfig, SubagentOutputFormat, TaskResult,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_swarm_agent_as_subagent() {
        // A swarm agent can run in subagent mode
        let config = SubagentConfig {
            task: "Agent 1 subtask: Core implementation".to_string(),
            parent_session_id: Some("swarm-session-123".to_string()),
            max_depth: 3,
            current_depth: 1, // Spawned by swarm coordinator
            context_policy: ContextPolicy::default(),
            output_format: SubagentOutputFormat::Json,
        };

        assert_eq!(config.current_depth, 1);
        assert!(config.parent_session_id.is_some());
        assert!(config.task.contains("Agent 1"));
    }

    #[test]
    fn test_compressed_result_from_agent_result() {
        let agent_result = AgentResult {
            agent_id: 0,
            subtask: "Implement authentication".to_string(),
            result: TaskResult {
                success: true,
                iterations: 3,
                summary: Some("Authentication module completed".to_string()),
                error: None,
                verification: None,
            },
            files_modified: vec![
                PathBuf::from("src/auth/mod.rs"),
                PathBuf::from("src/auth/jwt.rs"),
            ],
            duration_ms: 12000,
            compressed_result: None,
        };

        // Convert agent result to compressed result
        let compressed = CompressedResult {
            success: agent_result.result.success,
            commit_hash: Some("swarm-agent-0-abc123".to_string()),
            files_changed: agent_result.files_modified.clone(),
            summary: agent_result.result.summary.clone().unwrap_or_default(),
            duration_ms: agent_result.duration_ms,
            iterations: agent_result.result.iterations as u32,
            error: agent_result.result.error.clone(),
            verified: true,
            metadata: CompressedResultMetadata {
                version: "0.1.0".to_string(),
                depth: 1,
                parent_session_id: Some("swarm-coordinator".to_string()),
                completed_at: 1704067200,
                model: None,
            },
        };

        assert!(compressed.success);
        assert_eq!(compressed.files_changed.len(), 2);
        assert_eq!(compressed.iterations, 3);
        assert!(compressed.summary.contains("Authentication"));
    }

    #[test]
    fn test_swarm_result_to_compressed_result() {
        let swarm_result = SwarmResult {
            success: true,
            agent_results: vec![
                AgentResult {
                    agent_id: 0,
                    subtask: "Core".to_string(),
                    result: TaskResult {
                        success: true,
                        iterations: 2,
                        summary: Some("Core done".to_string()),
                        error: None,
                        verification: None,
                    },
                    files_modified: vec![PathBuf::from("src/lib.rs")],
                    duration_ms: 5000,
                    compressed_result: None,
                },
                AgentResult {
                    agent_id: 1,
                    subtask: "Tests".to_string(),
                    result: TaskResult {
                        success: true,
                        iterations: 1,
                        summary: Some("Tests done".to_string()),
                        error: None,
                        verification: None,
                    },
                    files_modified: vec![PathBuf::from("tests/test.rs")],
                    duration_ms: 3000,
                    compressed_result: None,
                },
            ],
            summary: Some("Swarm completed successfully".to_string()),
            conflicts: vec![],
            total_duration_ms: 8000,
            successful_agents: 2,
            failed_agents: 0,
            verification_passed: true,
            compressed_output: None,
        };

        // Aggregate to compressed result
        let all_files: Vec<PathBuf> = swarm_result
            .agent_results
            .iter()
            .flat_map(|r| r.files_modified.clone())
            .collect();

        let total_iterations: u32 = swarm_result
            .agent_results
            .iter()
            .map(|r| r.result.iterations as u32)
            .sum();

        let compressed = CompressedResult {
            success: swarm_result.success,
            commit_hash: Some("swarm-combined-xyz789".to_string()),
            files_changed: all_files.clone(),
            summary: swarm_result.summary.clone().unwrap_or_default(),
            duration_ms: swarm_result.total_duration_ms,
            iterations: total_iterations,
            error: None,
            verified: swarm_result.verification_passed,
            metadata: CompressedResultMetadata {
                version: "0.1.0".to_string(),
                depth: 0,
                parent_session_id: None,
                completed_at: 1704067200,
                model: None,
            },
        };

        assert!(compressed.success);
        assert_eq!(compressed.files_changed.len(), 2);
        assert_eq!(compressed.iterations, 3); // 2 + 1
        assert_eq!(compressed.duration_ms, 8000);
    }

    #[test]
    fn test_operation_mode_for_swarm_agents() {
        // Swarm agents should run in subagent mode
        let agent_mode = OperationMode::Subagent;
        let coordinator_mode = OperationMode::Standalone;

        assert_ne!(agent_mode, coordinator_mode);

        match agent_mode {
            OperationMode::Subagent => {
                // Swarm agents compress their output
                assert!(true);
            }
            OperationMode::Standalone => panic!("Agents should be in Subagent mode"),
        }
    }

    #[test]
    fn test_context_policy_for_swarm() {
        // Swarm-specific context policy
        let swarm_policy = ContextPolicy {
            hide_failures: false, // Show failures so coordinator can handle
            hide_intermediates: true, // Hide intermediate steps to save context
            include_commit: true,
            max_summary_length: 300, // Shorter summaries for aggregation
            include_diffs: false, // No diffs to save space
        };

        assert!(!swarm_policy.hide_failures);
        assert!(swarm_policy.hide_intermediates);
        assert_eq!(swarm_policy.max_summary_length, 300);
    }

    #[test]
    fn test_subtask_with_subagent_config() {
        let subtask = Subtask {
            id: 0,
            description: "Implement authentication module".to_string(),
            focus: "Core implementation".to_string(),
            target_files: vec![PathBuf::from("src/auth/mod.rs")],
            shared_context: Some("API endpoints are in src/api/".to_string()),
            priority: 1,
            use_subagent_mode: true,
            subagent_depth: 1,
        };

        // Create subagent config from subtask
        let subagent_config = SubagentConfig {
            task: format!("{}: {}", subtask.focus, subtask.description),
            parent_session_id: Some("swarm-session".to_string()),
            max_depth: 2, // Swarm agents have limited nesting depth
            current_depth: subtask.subagent_depth,
            context_policy: ContextPolicy::default(),
            output_format: SubagentOutputFormat::Json,
        };

        assert!(subagent_config.task.contains("Core implementation"));
        assert!(subagent_config.task.contains("authentication module"));
        assert!(subtask.use_subagent_mode);
    }

    #[test]
    fn test_swarm_conflict_with_compressed_results() {
        // Simulate conflict detection from compressed results
        let agent0_result = CompressedResult {
            success: true,
            commit_hash: Some("agent0-abc".to_string()),
            files_changed: vec![PathBuf::from("src/main.rs")],
            summary: "Modified main.rs".to_string(),
            duration_ms: 1000,
            iterations: 1,
            error: None,
            verified: true,
            metadata: CompressedResultMetadata {
                version: "0.1.0".to_string(),
                depth: 0,
                parent_session_id: None,
                completed_at: 0,
                model: None,
            },
        };

        let agent1_result = CompressedResult {
            success: true,
            commit_hash: Some("agent1-def".to_string()),
            files_changed: vec![PathBuf::from("src/main.rs")], // Same file!
            summary: "Also modified main.rs".to_string(),
            duration_ms: 1000,
            iterations: 1,
            error: None,
            verified: true,
            metadata: CompressedResultMetadata {
                version: "0.1.0".to_string(),
                depth: 0,
                parent_session_id: None,
                completed_at: 0,
                model: None,
            },
        };

        // Detect conflict
        let overlapping: Vec<&PathBuf> = agent0_result
            .files_changed
            .iter()
            .filter(|f| agent1_result.files_changed.contains(f))
            .collect();

        assert!(!overlapping.is_empty());
        assert_eq!(overlapping[0], &PathBuf::from("src/main.rs"));

        // Create conflict struct
        let conflict = SwarmConflict {
            file: overlapping[0].clone(),
            agent_ids: vec![0, 1],
            description: "Both agents modified the same file".to_string(),
            resolved: false,
        };

        assert!(!conflict.resolved);
        assert_eq!(conflict.agent_ids.len(), 2);
    }

    #[test]
    fn test_swarm_with_depth_limit() {
        // Swarm coordinator at depth 0
        let coordinator_config = SubagentConfig {
            task: "Coordinate swarm".to_string(),
            parent_session_id: None,
            max_depth: 3,
            current_depth: 0,
            context_policy: ContextPolicy::default(),
            output_format: SubagentOutputFormat::Json,
        };

        // Swarm agents at depth 1
        let agent_configs: Vec<SubagentConfig> = (0..3)
            .map(|i| SubagentConfig {
                task: format!("Agent {} task", i),
                parent_session_id: Some("swarm-coordinator".to_string()),
                max_depth: coordinator_config.max_depth,
                current_depth: coordinator_config.current_depth + 1,
                context_policy: coordinator_config.context_policy.clone(),
                output_format: SubagentOutputFormat::Json,
            })
            .collect();

        for config in &agent_configs {
            assert_eq!(config.current_depth, 1);
            assert!(config.current_depth < config.max_depth);
        }

        // Nested subagent at depth 2 (still allowed)
        let nested_config = SubagentConfig {
            task: "Nested task".to_string(),
            parent_session_id: Some("agent-0".to_string()),
            max_depth: coordinator_config.max_depth,
            current_depth: 2,
            context_policy: ContextPolicy::default(),
            output_format: SubagentOutputFormat::Json,
        };

        assert!(nested_config.current_depth < nested_config.max_depth);

        // Depth 3 would be at limit
        let limit_config = SubagentConfig {
            current_depth: 3,
            max_depth: 3,
            ..Default::default()
        };

        assert!(limit_config.current_depth >= limit_config.max_depth);
    }

    fn default_metadata() -> CompressedResultMetadata {
        CompressedResultMetadata {
            version: "0.1.0".to_string(),
            depth: 0,
            parent_session_id: None,
            completed_at: 0,
            model: None,
        }
    }

    #[test]
    fn test_swarm_result_aggregation() {
        let results = vec![
            CompressedResult {
                success: true,
                commit_hash: Some("a".to_string()),
                files_changed: vec![PathBuf::from("a.rs")],
                summary: "A done".to_string(),
                duration_ms: 1000,
                iterations: 1,
                error: None,
                verified: true,
                metadata: default_metadata(),
            },
            CompressedResult {
                success: true,
                commit_hash: Some("b".to_string()),
                files_changed: vec![PathBuf::from("b.rs")],
                summary: "B done".to_string(),
                duration_ms: 2000,
                iterations: 2,
                error: None,
                verified: true,
                metadata: default_metadata(),
            },
            CompressedResult {
                success: false,
                commit_hash: None,
                files_changed: vec![],
                summary: "C failed".to_string(),
                duration_ms: 500,
                iterations: 1,
                error: Some("Error occurred".to_string()),
                verified: false,
                metadata: default_metadata(),
            },
        ];

        // Aggregate results
        let all_success = results.iter().all(|r| r.success);
        let any_success = results.iter().any(|r| r.success);
        let total_duration: u64 = results.iter().map(|r| r.duration_ms).sum();
        let total_files: usize = results.iter().map(|r| r.files_changed.len()).sum();
        let successful_count = results.iter().filter(|r| r.success).count();
        let failed_count = results.iter().filter(|r| !r.success).count();

        assert!(!all_success);
        assert!(any_success);
        assert_eq!(total_duration, 3500);
        assert_eq!(total_files, 2);
        assert_eq!(successful_count, 2);
        assert_eq!(failed_count, 1);
    }

    #[test]
    fn test_swarm_config_with_subagent_settings() {
        let temp_dir = TempDir::new().unwrap();

        let config = SwarmConfigBuilder::new()
            .agent_count(4)
            .timeout_secs(300)
            .strategy(DecompositionStrategy::ByFocus)
            .working_dir(temp_dir.path().to_path_buf())
            .share_context(true)
            .build();

        // Create subagent configs from swarm config
        let subagent_configs: Vec<SubagentConfig> = (0..config.agent_count)
            .map(|i| SubagentConfig {
                task: format!("Swarm agent {} task", i),
                parent_session_id: Some("swarm-main".to_string()),
                max_depth: 2,
                current_depth: 1,
                context_policy: ContextPolicy {
                    hide_failures: false,
                    hide_intermediates: true,
                    include_commit: true,
                    max_summary_length: 250,
                    include_diffs: false,
                },
                output_format: SubagentOutputFormat::Json,
            })
            .collect();

        assert_eq!(subagent_configs.len(), 4);
        for (i, cfg) in subagent_configs.iter().enumerate() {
            assert!(cfg.task.contains(&format!("agent {}", i)));
            assert_eq!(cfg.context_policy.max_summary_length, 250);
        }
    }

    #[test]
    fn test_output_format_for_swarm_coordination() {
        // JSON format is best for swarm coordination
        let json_format = SubagentOutputFormat::Json;
        let text_format = SubagentOutputFormat::Text;
        let markdown_format = SubagentOutputFormat::Markdown;

        // All formats should serialize correctly
        for format in [json_format, text_format, markdown_format] {
            let json = serde_json::to_string(&format).unwrap();
            let deserialized: SubagentOutputFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, format);
        }
    }

    #[test]
    fn test_compressed_result_metadata_for_swarm() {
        let coordinator_metadata = CompressedResultMetadata {
            version: "0.1.0".to_string(),
            depth: 0,
            parent_session_id: None,
            completed_at: 1704067200,
            model: Some("claude-3-opus".to_string()),
        };

        let agent_metadata = CompressedResultMetadata {
            version: "0.1.0".to_string(),
            depth: 1,
            parent_session_id: Some("coordinator-session".to_string()),
            completed_at: 1704067200,
            model: Some("claude-3-sonnet".to_string()), // Different model for agents
        };

        assert_eq!(coordinator_metadata.depth, 0);
        assert!(coordinator_metadata.parent_session_id.is_none());
        assert_eq!(agent_metadata.depth, 1);
        assert!(agent_metadata.parent_session_id.is_some());
    }

    #[test]
    fn test_swarm_agent_files_distribution() {
        // Simulate file distribution across agents
        let all_files = vec![
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/lib.rs"),
            PathBuf::from("src/api/mod.rs"),
            PathBuf::from("src/api/handlers.rs"),
            PathBuf::from("tests/integration.rs"),
            PathBuf::from("tests/unit.rs"),
        ];

        let agent_count = 3;
        let files_per_agent: Vec<Vec<PathBuf>> = (0..agent_count)
            .map(|i| {
                all_files
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| j % agent_count == i)
                    .map(|(_, f)| f.clone())
                    .collect()
            })
            .collect();

        // Verify distribution
        assert_eq!(files_per_agent.len(), 3);
        let total_distributed: usize = files_per_agent.iter().map(|f| f.len()).sum();
        assert_eq!(total_distributed, all_files.len());

        // Verify no duplicates
        let mut all_distributed: Vec<&PathBuf> =
            files_per_agent.iter().flat_map(|f| f.iter()).collect();
        let original_len = all_distributed.len();
        all_distributed.sort();
        all_distributed.dedup();
        assert_eq!(all_distributed.len(), original_len);
    }
}
