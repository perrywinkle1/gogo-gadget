//! Comprehensive Tests for Verification Module
//!
//! This test file provides coverage of the LLM-based verification module,
//! focusing on:
//! - Build/test verification
//! - LLM completion checking
//! - File modification tracking
//! - Verification history

use jarvis_v2::verify::{
    BuildResult, DiffVerification, FileDiff, LlmCompletionCheck, TestResult,
    VerificationHistory, VerificationResult, Verifier,
    create_llm_completion_prompt, parse_llm_completion_response,
};
use std::path::PathBuf;
use tempfile::TempDir;

// =============================================================================
// Basic Verification Tests
// =============================================================================

mod basic_verification_tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_verify_with_promise() {
        let verifier = Verifier::new(Some("TASK_COMPLETE".to_string()));
        let temp_dir = TempDir::new().unwrap();

        let output = "Successfully implemented the feature. TASK_COMPLETE";
        let result = verifier.verify(output, temp_dir.path());

        assert!(result.promise_found);
        assert!(result.is_complete);
    }

    #[test]
    fn test_verify_without_promise() {
        let verifier = Verifier::new(Some("TASK_COMPLETE".to_string()));
        let temp_dir = TempDir::new().unwrap();

        let output = "Still working on it...";
        let result = verifier.verify(output, temp_dir.path());

        assert!(!result.promise_found);
        assert!(!result.is_complete);
    }

    #[test]
    fn test_verify_no_promise_required() {
        let verifier = Verifier::new(None);
        let temp_dir = TempDir::new().unwrap();

        let output = "Task completed successfully";
        let result = verifier.verify(output, temp_dir.path());

        // With no promise required and no build/test, it should not be complete
        // (requires either promise or LLM check)
        assert!(!result.promise_found);
        assert!(!result.is_complete);
    }

    #[test]
    fn test_file_modification_verification() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.rs");

        // Create a file
        fs::write(&test_file, "initial content").unwrap();

        let mut verifier = Verifier::new(None);
        verifier.expect_files(vec![PathBuf::from("test.rs")]);
        verifier.capture_baseline(temp_dir.path());

        // Modify the file
        fs::write(&test_file, "modified content").unwrap();

        let result = verifier.verify("Task complete", temp_dir.path());

        assert!(!result.file_modifications.is_empty());
        assert!(result.file_modifications[0].exists);
    }

    #[test]
    fn test_file_missing_verification() {
        let temp_dir = TempDir::new().unwrap();

        let mut verifier = Verifier::new(None);
        verifier.expect_files(vec![PathBuf::from("nonexistent.rs")]);

        let result = verifier.verify("Task complete", temp_dir.path());

        assert!(!result.file_modifications.is_empty());
        assert!(!result.file_modifications[0].exists);
        assert!(result.warnings.iter().any(|w| w.contains("missing")));
    }
}

// =============================================================================
// Build and Test Result Tests
// =============================================================================

mod build_test_result_tests {
    use super::*;

    #[test]
    fn test_build_result_default() {
        let result = BuildResult::default();
        assert!(!result.executed);
        assert!(!result.success);
        assert!(result.output.is_empty());
        assert!(result.error.is_none());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_test_result_default() {
        let result = TestResult::default();
        assert!(!result.executed);
        assert!(!result.passed);
        assert_eq!(result.passed_count, 0);
        assert_eq!(result.failed_count, 0);
        assert_eq!(result.skipped_count, 0);
    }

    #[test]
    fn test_verification_result_has_blocking_errors_build_failure() {
        let mut result = VerificationResult::default();
        assert!(!result.has_blocking_errors());

        result.build_result = Some(BuildResult {
            executed: true,
            success: false,
            output: "Build error".to_string(),
            error: Some("Exit code: 1".to_string()),
            warnings: vec![],
        });
        assert!(result.has_blocking_errors());
    }

    #[test]
    fn test_verification_result_has_blocking_errors_test_failure() {
        let mut result = VerificationResult::default();

        result.build_result = Some(BuildResult {
            executed: true,
            success: true,
            ..Default::default()
        });
        result.test_result = Some(TestResult {
            executed: true,
            passed: false,
            failed_count: 2,
            ..Default::default()
        });
        assert!(result.has_blocking_errors());
    }

    #[test]
    fn test_verification_result_no_blocking_errors() {
        let mut result = VerificationResult::default();

        result.build_result = Some(BuildResult {
            executed: true,
            success: true,
            ..Default::default()
        });
        result.test_result = Some(TestResult {
            executed: true,
            passed: true,
            passed_count: 10,
            ..Default::default()
        });
        assert!(!result.has_blocking_errors());
    }
}

// =============================================================================
// LLM Completion Check Tests
// =============================================================================

mod llm_completion_tests {
    use super::*;

    #[test]
    fn test_llm_completion_check_default() {
        let check = LlmCompletionCheck::default();
        assert!(!check.is_complete);
        assert!(check.explanation.is_empty());
        assert!(check.next_steps.is_empty());
    }

    #[test]
    fn test_create_llm_completion_prompt_basic() {
        let prompt = create_llm_completion_prompt(
            "Implement a hello world function",
            "I created the function that prints Hello World",
            None,
            None,
        );

        assert!(prompt.contains("Implement a hello world function"));
        assert!(prompt.contains("prints Hello World"));
        assert!(prompt.contains("is_complete"));
        assert!(prompt.contains("explanation"));
        assert!(prompt.contains("next_steps"));
    }

    #[test]
    fn test_create_llm_completion_prompt_with_build() {
        let build = BuildResult {
            executed: true,
            success: true,
            output: "Build completed successfully".to_string(),
            ..Default::default()
        };

        let prompt = create_llm_completion_prompt(
            "Fix the bug",
            "Bug has been fixed",
            Some(&build),
            None,
        );

        assert!(prompt.contains("Build Result"));
        assert!(prompt.contains("SUCCESS"));
    }

    #[test]
    fn test_create_llm_completion_prompt_with_tests() {
        let test = TestResult {
            executed: true,
            passed: true,
            passed_count: 5,
            failed_count: 0,
            skipped_count: 1,
            ..Default::default()
        };

        let prompt = create_llm_completion_prompt(
            "Add unit tests",
            "Tests added",
            None,
            Some(&test),
        );

        assert!(prompt.contains("Test Result"));
        assert!(prompt.contains("PASSED"));
        assert!(prompt.contains("5"));
    }

    #[test]
    fn test_parse_llm_response_complete() {
        let response = r#"
            Based on my analysis:
            ```json
            {
                "is_complete": true,
                "explanation": "The task has been completed successfully",
                "next_steps": []
            }
            ```
        "#;

        let check = parse_llm_completion_response(response).unwrap();
        assert!(check.is_complete);
        assert_eq!(check.explanation, "The task has been completed successfully");
        assert!(check.next_steps.is_empty());
    }

    #[test]
    fn test_parse_llm_response_incomplete() {
        let response = r#"{
            "is_complete": false,
            "explanation": "Missing error handling",
            "next_steps": ["Add error handling", "Add tests"]
        }"#;

        let check = parse_llm_completion_response(response).unwrap();
        assert!(!check.is_complete);
        assert_eq!(check.explanation, "Missing error handling");
        assert_eq!(check.next_steps.len(), 2);
        assert_eq!(check.next_steps[0], "Add error handling");
    }

    #[test]
    fn test_parse_llm_response_no_json() {
        let response = "This response has no JSON in it.";
        let result = parse_llm_completion_response(response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_llm_response_malformed_json() {
        let response = r#"{ "is_complete": true, broken }"#;
        let result = parse_llm_completion_response(response);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_llm_check() {
        let mut result = VerificationResult::default();
        result.build_result = Some(BuildResult {
            executed: true,
            success: true,
            ..Default::default()
        });

        let llm_check = LlmCompletionCheck {
            is_complete: true,
            explanation: "All requirements met".to_string(),
            next_steps: vec![],
            shortcut_risk: 0.0,
        };

        Verifier::apply_llm_check(&mut result, llm_check);

        assert!(result.is_complete);
        assert!(result.llm_check.is_some());
        assert!(result.summary.as_ref().unwrap().contains("complete"));
    }

    #[test]
    fn test_apply_llm_check_incomplete() {
        let mut result = VerificationResult::default();
        result.build_result = Some(BuildResult {
            executed: true,
            success: true,
            ..Default::default()
        });

        let llm_check = LlmCompletionCheck {
            is_complete: false,
            explanation: "Missing tests".to_string(),
            next_steps: vec!["Add tests".to_string()],
            shortcut_risk: 0.0,
        };

        Verifier::apply_llm_check(&mut result, llm_check);

        assert!(!result.is_complete);
        assert!(result.summary.as_ref().unwrap().contains("incomplete"));
    }

    #[test]
    fn test_llm_check_does_not_override_build_failure() {
        let mut result = VerificationResult::default();
        result.build_result = Some(BuildResult {
            executed: true,
            success: false,
            ..Default::default()
        });

        let llm_check = LlmCompletionCheck {
            is_complete: true,
            explanation: "Looks good".to_string(),
            next_steps: vec![],
            shortcut_risk: 0.0,
        };

        Verifier::apply_llm_check(&mut result, llm_check);

        // Even though LLM says complete, build failed so it should not be complete
        assert!(!result.is_complete);
    }
}

// =============================================================================
// Verification History Tests
// =============================================================================

mod verification_history_tests {
    use super::*;

    #[test]
    fn test_history_tracking() {
        let mut history = VerificationHistory::new(10);

        let result1 = VerificationResult {
            is_complete: false,
            ..Default::default()
        };
        history.add(result1, "iteration_1", None);

        let result2 = VerificationResult {
            is_complete: true,
            ..Default::default()
        };
        history.add(result2, "iteration_2", None);

        assert_eq!(history.iteration_count(), 2);
        assert!(history.latest().unwrap().is_complete);
    }

    #[test]
    fn test_history_stuck_detection() {
        let mut history = VerificationHistory::new(10);

        // Add 3 results with same completion status
        for _ in 0..3 {
            let result = VerificationResult {
                is_complete: false,
                ..Default::default()
            };
            history.add(result, "iter", None);
        }

        assert!(history.is_stuck(3));
    }

    #[test]
    fn test_history_not_stuck() {
        let mut history = VerificationHistory::new(10);

        history.add(
            VerificationResult {
                is_complete: false,
                ..Default::default()
            },
            "iter1",
            None,
        );
        history.add(
            VerificationResult {
                is_complete: true,
                ..Default::default()
            },
            "iter2",
            None,
        );
        history.add(
            VerificationResult {
                is_complete: false,
                ..Default::default()
            },
            "iter3",
            None,
        );

        assert!(!history.is_stuck(3));
    }

    #[test]
    fn test_history_completion_rate() {
        let mut history = VerificationHistory::new(10);

        history.add(
            VerificationResult {
                is_complete: true,
                ..Default::default()
            },
            "iter1",
            None,
        );
        history.add(
            VerificationResult {
                is_complete: false,
                ..Default::default()
            },
            "iter2",
            None,
        );
        history.add(
            VerificationResult {
                is_complete: true,
                ..Default::default()
            },
            "iter3",
            None,
        );
        history.add(
            VerificationResult {
                is_complete: true,
                ..Default::default()
            },
            "iter4",
            None,
        );

        let rate = history.completion_rate();
        assert!((rate - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_history_max_entries() {
        let mut history = VerificationHistory::new(3);

        for i in 0..5 {
            history.add(
                VerificationResult {
                    is_complete: i >= 3,
                    ..Default::default()
                },
                &format!("iter_{}", i),
                None,
            );
        }

        assert_eq!(history.iteration_count(), 3);
        // Should keep the last 3 entries (indices 2, 3, 4 which are complete for 3, 4)
        assert!(history.latest().unwrap().is_complete);
    }

    #[test]
    fn test_history_is_making_progress() {
        let mut history = VerificationHistory::new(10);

        // Not enough data
        assert!(history.is_making_progress().is_none());

        history.add(
            VerificationResult {
                is_complete: false,
                ..Default::default()
            },
            "iter1",
            None,
        );

        // Still not enough data
        assert!(history.is_making_progress().is_none());

        history.add(
            VerificationResult {
                is_complete: true,
                ..Default::default()
            },
            "iter2",
            None,
        );

        // Now we have enough, and there's a complete one
        assert_eq!(history.is_making_progress(), Some(true));
    }
}

// =============================================================================
// DiffVerification Tests
// =============================================================================

mod diff_verification_tests {
    use super::*;

    #[test]
    fn test_diff_verification_summary() {
        let verification = DiffVerification {
            files: vec![],
            total_lines_added: 100,
            total_lines_removed: 50,
            files_created: 2,
            files_deleted: 1,
            files_modified: 5,
        };

        let summary = verification.summary();
        assert!(summary.contains("2 created"));
        assert!(summary.contains("5 modified"));
        assert!(summary.contains("1 deleted"));
        assert!(summary.contains("+100"));
        assert!(summary.contains("-50"));
        assert!(verification.has_changes());
    }

    #[test]
    fn test_diff_verification_no_changes() {
        let verification = DiffVerification::default();
        assert_eq!(verification.summary(), "No changes detected");
        assert!(!verification.has_changes());
    }

    #[test]
    fn test_file_diff_new_file() {
        let new_file = FileDiff {
            path: PathBuf::from("new.rs"),
            is_new: true,
            is_deleted: false,
            is_modified: false,
            lines_added: 50,
            lines_removed: 0,
            size_delta: 1000,
            diff_text: None,
            change_summary: None,
        };

        assert!(new_file.is_meaningful());
        assert_eq!(new_file.change_type(), "created");
        assert!(new_file.brief_summary().contains("created"));
        assert!(new_file.brief_summary().contains("+50"));
    }

    #[test]
    fn test_file_diff_deleted_file() {
        let deleted_file = FileDiff {
            path: PathBuf::from("old.rs"),
            is_new: false,
            is_deleted: true,
            is_modified: false,
            lines_added: 0,
            lines_removed: 100,
            size_delta: -2000,
            diff_text: None,
            change_summary: None,
        };

        assert!(deleted_file.is_meaningful());
        assert_eq!(deleted_file.change_type(), "deleted");
        assert!(deleted_file.brief_summary().contains("deleted"));
        assert!(deleted_file.brief_summary().contains("-100"));
    }

    #[test]
    fn test_file_diff_modified_file() {
        let modified_file = FileDiff {
            path: PathBuf::from("changed.rs"),
            is_new: false,
            is_deleted: false,
            is_modified: true,
            lines_added: 20,
            lines_removed: 10,
            size_delta: 200,
            diff_text: None,
            change_summary: None,
        };

        assert!(modified_file.is_meaningful());
        assert_eq!(modified_file.change_type(), "modified");
        assert!(modified_file.brief_summary().contains("+20"));
        assert!(modified_file.brief_summary().contains("-10"));
    }

    #[test]
    fn test_file_diff_unchanged_file() {
        let unchanged_file = FileDiff {
            path: PathBuf::from("stable.rs"),
            is_new: false,
            is_deleted: false,
            is_modified: false,
            lines_added: 0,
            lines_removed: 0,
            size_delta: 0,
            diff_text: None,
            change_summary: None,
        };

        assert!(!unchanged_file.is_meaningful());
        assert_eq!(unchanged_file.change_type(), "unchanged");
        assert_eq!(unchanged_file.brief_summary(), "unchanged");
    }
}

// =============================================================================
// Verifier Builder Pattern Tests
// =============================================================================

mod verifier_builder_tests {
    use super::*;

    #[test]
    fn test_verifier_with_test_command() {
        let verifier = Verifier::new(None)
            .with_test_command("cargo test");

        // Can't directly test private fields, but we can test behavior
        let temp_dir = TempDir::new().unwrap();
        let result = verifier.verify("output", temp_dir.path());

        // Test command was set, but execution will fail gracefully
        // since we're not in a cargo project
        assert!(result.test_result.is_some());
    }

    #[test]
    fn test_verifier_with_build_command() {
        let verifier = Verifier::new(None)
            .with_build_command("cargo build");

        let temp_dir = TempDir::new().unwrap();
        let result = verifier.verify("output", temp_dir.path());

        assert!(result.build_result.is_some());
    }

    #[test]
    fn test_verifier_full_verification() {
        let verifier = Verifier::new(Some("DONE".to_string()))
            .with_test_command("echo 'tests passed'")
            .with_build_command("echo 'build passed'");

        let temp_dir = TempDir::new().unwrap();

        // Test with build only
        let result = verifier.verify_full("DONE", temp_dir.path(), false, true);
        assert!(result.build_result.is_some());
        assert!(result.test_result.is_none());

        // Test with tests only
        let result = verifier.verify_full("DONE", temp_dir.path(), true, false);
        assert!(result.test_result.is_some());
        assert!(result.build_result.is_none());

        // Test with both
        let result = verifier.verify_full("DONE", temp_dir.path(), true, true);
        assert!(result.test_result.is_some());
        assert!(result.build_result.is_some());
    }
}

// =============================================================================
// Formatted Summary Tests
// =============================================================================

mod formatted_summary_tests {
    use super::*;

    #[test]
    fn test_formatted_summary_no_results() {
        let result = VerificationResult::default();
        assert_eq!(result.formatted_summary(), "No verification performed");
    }

    #[test]
    fn test_formatted_summary_build_only() {
        let mut result = VerificationResult::default();
        result.build_result = Some(BuildResult {
            executed: true,
            success: true,
            ..Default::default()
        });

        let summary = result.formatted_summary();
        assert!(summary.contains("Build: passed"));
    }

    #[test]
    fn test_formatted_summary_build_failed() {
        let mut result = VerificationResult::default();
        result.build_result = Some(BuildResult {
            executed: true,
            success: false,
            ..Default::default()
        });

        let summary = result.formatted_summary();
        assert!(summary.contains("Build: FAILED"));
    }

    #[test]
    fn test_formatted_summary_tests_only() {
        let mut result = VerificationResult::default();
        result.test_result = Some(TestResult {
            executed: true,
            passed: true,
            passed_count: 10,
            failed_count: 0,
            ..Default::default()
        });

        let summary = result.formatted_summary();
        assert!(summary.contains("Tests: passed"));
        assert!(summary.contains("10 passed"));
    }

    #[test]
    fn test_formatted_summary_full() {
        let mut result = VerificationResult::default();
        result.build_result = Some(BuildResult {
            executed: true,
            success: true,
            ..Default::default()
        });
        result.test_result = Some(TestResult {
            executed: true,
            passed: true,
            passed_count: 5,
            failed_count: 0,
            ..Default::default()
        });
        result.llm_check = Some(LlmCompletionCheck {
            is_complete: true,
            explanation: "Done".to_string(),
            next_steps: vec![],
            shortcut_risk: 0.0,
        });

        let summary = result.formatted_summary();
        assert!(summary.contains("Build: passed"));
        assert!(summary.contains("Tests: passed"));
        assert!(summary.contains("LLM Check: complete"));
    }
}

// =============================================================================
// Edge Case Tests
// =============================================================================

mod edge_case_tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_empty_output() {
        let verifier = Verifier::new(Some("COMPLETE".to_string()));
        let temp_dir = TempDir::new().unwrap();

        let result = verifier.verify("", temp_dir.path());
        assert!(!result.promise_found);
        assert!(!result.is_complete);
    }

    #[test]
    fn test_multiple_expected_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create some files
        fs::write(temp_dir.path().join("file1.rs"), "content1").unwrap();
        fs::write(temp_dir.path().join("file2.rs"), "content2").unwrap();

        let mut verifier = Verifier::new(None);
        verifier.expect_files(vec![
            PathBuf::from("file1.rs"),
            PathBuf::from("file2.rs"),
            PathBuf::from("file3.rs"), // This one doesn't exist
        ]);

        let result = verifier.verify("output", temp_dir.path());

        assert_eq!(result.file_modifications.len(), 3);
        assert!(result.warnings.iter().any(|w| w.contains("1 of 3")));
    }

    #[test]
    fn test_promise_case_sensitive() {
        let verifier = Verifier::new(Some("COMPLETE".to_string()));
        let temp_dir = TempDir::new().unwrap();

        // Lowercase should not match
        let result = verifier.verify("complete", temp_dir.path());
        assert!(!result.promise_found);

        // Uppercase should match
        let result = verifier.verify("COMPLETE", temp_dir.path());
        assert!(result.promise_found);
    }

    #[test]
    fn test_verifier_without_any_config() {
        let verifier = Verifier::new(None);
        let temp_dir = TempDir::new().unwrap();

        let result = verifier.verify("some output", temp_dir.path());

        assert!(!result.is_complete);
        assert!(result.build_result.is_none());
        assert!(result.test_result.is_none());
        assert!(result.file_modifications.is_empty());
    }

    #[test]
    fn test_parse_llm_response_with_extra_text() {
        let response = r#"
            Here's my analysis of the task:

            The implementation looks good overall.

            ```json
            {
                "is_complete": true,
                "explanation": "All requirements have been met",
                "next_steps": []
            }
            ```

            Let me know if you need anything else!
        "#;

        let check = parse_llm_completion_response(response).unwrap();
        assert!(check.is_complete);
        assert_eq!(check.explanation, "All requirements have been met");
    }

    #[test]
    fn test_parse_llm_response_minimal() {
        let response = r#"{"is_complete": false}"#;

        let check = parse_llm_completion_response(response).unwrap();
        assert!(!check.is_complete);
        assert_eq!(check.explanation, "No explanation provided");
        assert!(check.next_steps.is_empty());
    }
}

// =============================================================================
// Capability Verification Tests (Self-Extend Module)
// =============================================================================

mod capability_verification_tests {
    use jarvis_v2::extend::{
        CapabilityType, CapabilityVerifier, SynthesizedCapability, VerificationConfig,
    };
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_synthesized_capability_creation() {
        let cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "test-skill",
            PathBuf::from("/tmp/test-skill.md"),
        );

        assert_eq!(cap.name, "test-skill");
        assert_eq!(cap.capability_type, CapabilityType::Skill);
        assert!(cap.id.starts_with("skill-test-skill-"));
        assert!(cap.synthesized);
        assert_eq!(cap.usage_count, 0);
        assert_eq!(cap.success_rate, 0.0);
    }

    #[test]
    fn test_capability_type_as_str() {
        assert_eq!(CapabilityType::Mcp.as_str(), "mcp");
        assert_eq!(CapabilityType::Skill.as_str(), "skill");
        assert_eq!(CapabilityType::Agent.as_str(), "agent");
    }

    #[test]
    fn test_capability_type_default() {
        assert_eq!(CapabilityType::default(), CapabilityType::Skill);
    }

    #[test]
    fn test_synthesized_capability_with_id() {
        let cap = SynthesizedCapability::new(
            CapabilityType::Mcp,
            "my-mcp",
            PathBuf::from("/tmp/mcp"),
        )
        .with_id("custom-id-123");

        assert_eq!(cap.id, "custom-id-123");
    }

    #[test]
    fn test_synthesized_capability_with_config() {
        let config = serde_json::json!({
            "api_url": "https://api.example.com",
            "timeout": 30
        });

        let cap = SynthesizedCapability::new(
            CapabilityType::Mcp,
            "api-mcp",
            PathBuf::from("/tmp/mcp"),
        )
        .with_config(config.clone());

        assert_eq!(cap.config, config);
    }

    #[test]
    fn test_verification_config_default() {
        let config = VerificationConfig::default();
        assert!(config.run_npm_install);
        assert!(config.test_mcp_server);
        assert!(config.validate_triggers);
        assert!(!config.required_skill_sections.is_empty());
    }

    #[tokio::test]
    async fn test_verifier_skill_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let skill_path = temp_dir.path().join("nonexistent-skill.md");

        let cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "missing-skill",
            skill_path,
        );

        let verifier = CapabilityVerifier::new();
        let verification = verifier.verify(&cap).await;

        assert!(!verification.success);
        assert!(!verification.errors.is_empty());
    }

    #[tokio::test]
    async fn test_verifier_skill_valid_file() {
        let temp_dir = TempDir::new().unwrap();
        let skill_path = temp_dir.path().join("valid-skill.md");

        // Create a valid SKILL.md file
        let skill_content = r#"# Valid Skill

## Description
A test skill for validation.

## Triggers
- /test
- test something

## Instructions
Run the test command.
"#;
        fs::write(&skill_path, skill_content).unwrap();

        let cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "valid-skill",
            skill_path,
        );

        let verifier = CapabilityVerifier::new();
        let verification = verifier.verify(&cap).await;

        // May have warnings but should have steps
        assert!(!verification.steps_completed.is_empty());
    }

    #[tokio::test]
    async fn test_verifier_agent_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let agent_path = temp_dir.path().join("nonexistent-agent.md");

        let cap = SynthesizedCapability::new(
            CapabilityType::Agent,
            "missing-agent",
            agent_path,
        );

        let verifier = CapabilityVerifier::new();
        let verification = verifier.verify(&cap).await;

        assert!(!verification.success);
    }

    #[tokio::test]
    async fn test_verifier_mcp_missing_directory() {
        let temp_dir = TempDir::new().unwrap();
        let mcp_path = temp_dir.path().join("nonexistent-mcp");

        let cap = SynthesizedCapability::new(
            CapabilityType::Mcp,
            "missing-mcp",
            mcp_path,
        );

        let verifier = CapabilityVerifier::new();
        let verification = verifier.verify(&cap).await;

        assert!(!verification.success);
    }

    #[test]
    fn test_quick_file_readable() {
        use jarvis_v2::extend::verify::quick;

        let temp_dir = TempDir::new().unwrap();
        let skill_path = temp_dir.path().join("quick-skill.md");

        // Create a readable file
        fs::write(&skill_path, "test content").unwrap();

        assert!(quick::file_readable(&skill_path));

        // Test non-existent file
        let missing_path = temp_dir.path().join("missing.md");
        assert!(!quick::file_readable(&missing_path));
    }

    #[test]
    fn test_quick_markdown_has_sections() {
        use jarvis_v2::extend::verify::quick;

        let content = r#"# Test
## Trigger
Some trigger

## Instructions
Do something
"#;

        // Check that required sections are present
        let missing = quick::markdown_has_sections(content, &["Trigger", "Instructions"]);
        assert!(missing.is_empty());

        // Check for missing sections
        let missing = quick::markdown_has_sections(content, &["Trigger", "NonExistent"]);
        assert_eq!(missing, vec!["NonExistent"]);
    }
}
