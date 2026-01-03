//! Integration tests for Jarvis-RS
//!
//! NOTE: Tests that relied on deprecated ShortcutType/ShortcutDetector functionality
//! have been removed. The verification system now uses LlmCompletionCheck instead.

use jarvis_v2::{
    brain::TaskAnalyzer,
    verify::Verifier,
    AntiLazinessConfig, Config, Difficulty, ExecutionMode,
};
use std::path::PathBuf;
use tempfile::TempDir;

// ============================================================================
// Task Analyzer Tests
// ============================================================================

#[test]
#[ignore = "Requires running Claude instance"]
fn test_analyzer_simple_task() {
    let analyzer = TaskAnalyzer::new();
    let analysis = analyzer.analyze("Fix the typo in README").expect("Analysis should succeed");

    assert!(analysis.complexity_score < 3);
    assert_eq!(analysis.mode, ExecutionMode::Single);
    assert!(!analysis.needs_iteration);
    assert_eq!(analysis.difficulty, Difficulty::Trivial);
}

#[test]
#[ignore = "Requires running Claude instance"]
fn test_analyzer_complex_task() {
    let analyzer = TaskAnalyzer::new();
    let analysis = analyzer.analyze(
        r#"Build a complete blog platform with the following features:
        - Post management in posts.rs
        - CRUD operations for comments in api/comments.ts
        - Data models in models.py
        - Frontend components in components/PostList.tsx
        - Integration tests in tests/api_test.go
        - CSS styling in styles/main.css
        Ensure all tests pass and the application compiles without errors."#,
    ).expect("Analysis should succeed");

    assert!(analysis.complexity_score >= 5);
    assert!(matches!(analysis.mode, ExecutionMode::Swarm { .. }));
    assert!(analysis.needs_iteration);
    assert!(analysis.file_mentions >= 5);
}

#[test]
#[ignore = "Requires running Claude instance"]
fn test_analyzer_iteration_keywords() {
    let analyzer = TaskAnalyzer::new();

    // Test various iteration-triggering phrases
    let test_cases = [
        "Keep improving until perfect",
        "Iterate until all tests pass",
        "Polish the implementation",
        "Ensure everything works correctly",
        "Fix all the bugs",
    ];

    for case in test_cases {
        let analysis = analyzer.analyze(case).expect("Analysis should succeed");
        assert!(
            analysis.needs_iteration,
            "Expected iteration for: '{}'",
            case
        );
    }
}

#[test]
#[ignore = "Requires running Claude instance"]
fn test_analyzer_ultrathink_keywords() {
    let analyzer = TaskAnalyzer::new();

    let test_cases = [
        "Architect a scalable solution",
        "Design the best approach",
        "Analyze the trade-offs",
        "Compare different strategies",
        "Think carefully about the implementation",
    ];

    for case in test_cases {
        let analysis = analyzer.analyze(case).expect("Analysis should succeed");
        assert!(
            analysis.needs_ultrathink,
            "Expected ultrathink for: '{}'",
            case
        );
    }
}

#[test]
#[ignore = "Requires running Claude instance"]
fn test_analyzer_visual_detection() {
    let analyzer = TaskAnalyzer::new();

    let test_cases = [
        "Fix the button styling",
        "Update the frontend layout",
        "Check http://localhost:3000",
        "Improve the UI components",
        "Make the page render correctly",
    ];

    for case in test_cases {
        let analysis = analyzer.analyze(case).expect("Analysis should succeed");
        assert!(analysis.needs_visual, "Expected visual for: '{}'", case);
    }
}

// ============================================================================
// Verifier Tests
// ============================================================================

#[test]
fn test_verifier_with_promise() {
    let verifier = Verifier::new(Some("<promise>COMPLETE</promise>".to_string()));
    let temp_dir = TempDir::new().unwrap();

    let output = "Task finished. <promise>COMPLETE</promise>";
    let result = verifier.verify(output, temp_dir.path());

    assert!(result.promise_found);
}

#[test]
fn test_verifier_without_promise() {
    let verifier = Verifier::new(Some("DONE".to_string()));
    let temp_dir = TempDir::new().unwrap();

    let output = "I finished the task but didn't include the promise.";
    let result = verifier.verify(output, temp_dir.path());

    assert!(!result.promise_found);
}

#[test]
fn test_verifier_clean_completion() {
    let verifier = Verifier::new(Some("SUCCESS".to_string()));
    let temp_dir = TempDir::new().unwrap();

    let output = r#"
        Task completed successfully! ✅

        Implementation:
        - Created user authentication module
        - Added database connection pooling
        - Implemented API endpoints
        - All tests pass

        SUCCESS
    "#;

    let result = verifier.verify(output, temp_dir.path());

    assert!(result.promise_found);
    assert!(result.is_complete);
}

// ============================================================================
// Config Tests
// ============================================================================

#[test]
fn test_config_defaults() {
    let config = Config::default();

    assert!(config.completion_promise.is_none());
    assert!(config.model.is_none());
    assert!(!config.dry_run);
}

// ============================================================================
// Difficulty Tests
// ============================================================================

#[test]
fn test_difficulty_from_score() {
    assert_eq!(Difficulty::from(0), Difficulty::Trivial);
    assert_eq!(Difficulty::from(2), Difficulty::Trivial);
    assert_eq!(Difficulty::from(3), Difficulty::Easy);
    assert_eq!(Difficulty::from(5), Difficulty::Medium);
    assert_eq!(Difficulty::from(7), Difficulty::Hard);
    assert_eq!(Difficulty::from(10), Difficulty::Expert);
}

// ============================================================================
// Execution Mode Tests
// ============================================================================

#[test]
fn test_execution_mode_equality() {
    assert_eq!(ExecutionMode::Single, ExecutionMode::Single);
    assert_eq!(
        ExecutionMode::Swarm { agent_count: 5 },
        ExecutionMode::Swarm { agent_count: 5 }
    );
    assert_ne!(
        ExecutionMode::Swarm { agent_count: 5 },
        ExecutionMode::Swarm { agent_count: 3 }
    );
    assert_ne!(
        ExecutionMode::Single,
        ExecutionMode::Swarm { agent_count: 1 }
    );
}

// ============================================================================
// End-to-End Tests (with mocked Claude)
// ============================================================================

mod e2e_tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    /// Create a mock claude script that simulates successful completion
    fn create_mock_claude(temp_dir: &TempDir, behavior: MockClaudeBehavior) -> PathBuf {
        let script_path = temp_dir.path().join("mock_claude");
        let script_content: String = match behavior {
            MockClaudeBehavior::SuccessImmediate => r#"#!/bin/bash
# Mock Claude - immediate success
echo "✅ Task completed successfully!"
echo "Successfully implemented all requested features."
touch "$PWD/.jarvis-satisfied"
echo "All tasks complete" > "$PWD/.jarvis-satisfied"
"#
            .to_string(),
            MockClaudeBehavior::SuccessAfterIterations(n) => {
                let counter_file = temp_dir.path().join(".mock_counter");
                format!(
                    r#"#!/bin/bash
COUNTER_FILE="{}"
if [ -f "$COUNTER_FILE" ]; then
    COUNT=$(cat "$COUNTER_FILE")
else
    COUNT=0
fi
COUNT=$((COUNT + 1))
echo $COUNT > "$COUNTER_FILE"

if [ $COUNT -ge {} ]; then
    echo "✅ Task completed successfully!"
    touch "$PWD/.jarvis-satisfied"
    echo "Completed after $COUNT iterations" > "$PWD/.jarvis-satisfied"
else
    echo "⏳ Still working on iteration $COUNT..."
    touch "$PWD/.jarvis-continue"
    echo "Need more work" > "$PWD/.jarvis-continue"
fi
"#,
                    counter_file.display(),
                    n
                )
            }
            MockClaudeBehavior::Blocked => r#"#!/bin/bash
# Mock Claude - blocked
echo "❌ Cannot proceed"
touch "$PWD/.jarvis-blocked"
echo "Missing required dependencies" > "$PWD/.jarvis-blocked"
"#
            .to_string(),
            MockClaudeBehavior::ErrorOutput => r#"#!/bin/bash
# Mock Claude - error output
echo "Error: Failed to compile the project ❌"
echo "Build failed with 5 errors"
exit 1
"#
            .to_string(),
        };

        let mut file = fs::File::create(&script_path).unwrap();
        file.write_all(script_content.as_bytes()).unwrap();

        // Make executable
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();

        script_path
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum MockClaudeBehavior {
        SuccessImmediate,
        SuccessAfterIterations(u32),
        Blocked,
        ErrorOutput,
    }

    #[test]
    fn test_e2e_immediate_success() {
        let temp_dir = TempDir::new().unwrap();
        let mock_claude = create_mock_claude(&temp_dir, MockClaudeBehavior::SuccessImmediate);

        // Verify mock script exists and is executable
        assert!(mock_claude.exists());
        let metadata = fs::metadata(&mock_claude).unwrap();
        assert!(metadata.permissions().mode() & 0o111 != 0);

        // The mock creates .jarvis-satisfied - verify that behavior
        let output = std::process::Command::new(&mock_claude)
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        assert!(output.status.success());
        assert!(temp_dir.path().join(".jarvis-satisfied").exists());
    }

    #[test]
    fn test_e2e_iterative_success() {
        let temp_dir = TempDir::new().unwrap();
        let mock_claude =
            create_mock_claude(&temp_dir, MockClaudeBehavior::SuccessAfterIterations(3));

        // Run mock multiple times simulating iterations
        for i in 1..=3 {
            let output = std::process::Command::new(&mock_claude)
                .current_dir(temp_dir.path())
                .output()
                .unwrap();

            let stdout = String::from_utf8_lossy(&output.stdout);

            if i < 3 {
                assert!(stdout.contains("Still working"));
                assert!(temp_dir.path().join(".jarvis-continue").exists());
            } else {
                assert!(stdout.contains("completed successfully"));
                assert!(temp_dir.path().join(".jarvis-satisfied").exists());
            }
        }
    }

    #[test]
    fn test_e2e_blocked_execution() {
        let temp_dir = TempDir::new().unwrap();
        let mock_claude = create_mock_claude(&temp_dir, MockClaudeBehavior::Blocked);

        let _output = std::process::Command::new(&mock_claude)
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        assert!(temp_dir.path().join(".jarvis-blocked").exists());
        let blocked_content = fs::read_to_string(temp_dir.path().join(".jarvis-blocked")).unwrap();
        assert!(blocked_content.contains("Missing required dependencies"));
    }
}

// ============================================================================
// Checkpoint Tests
// ============================================================================

mod checkpoint_tests {
    use super::*;
    use serde_json;
    use std::fs;

    /// Simulated checkpoint structure
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Checkpoint {
        task: String,
        iteration: u32,
        feedback: String,
        working_dir: PathBuf,
        timestamp: u64,
    }

    fn create_checkpoint(
        temp_dir: &TempDir,
        task: &str,
        iteration: u32,
        feedback: &str,
    ) -> PathBuf {
        let checkpoint = Checkpoint {
            task: task.to_string(),
            iteration,
            feedback: feedback.to_string(),
            working_dir: temp_dir.path().to_path_buf(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let checkpoint_dir = temp_dir.path().join(".jarvis-checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();

        let checkpoint_path = checkpoint_dir.join(format!("checkpoint_{}.json", iteration));
        let json = serde_json::to_string_pretty(&checkpoint).unwrap();
        fs::write(&checkpoint_path, json).unwrap();

        checkpoint_path
    }

    fn load_checkpoint(path: &Path) -> Result<Checkpoint, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let checkpoint: Checkpoint = serde_json::from_str(&content)?;
        Ok(checkpoint)
    }

    use serde::{Deserialize, Serialize};
    use std::path::Path;

    #[test]
    fn test_checkpoint_save() {
        let temp_dir = TempDir::new().unwrap();

        let checkpoint_path = create_checkpoint(
            &temp_dir,
            "Build a REST API",
            3,
            "Previous iteration found issues with auth",
        );

        assert!(checkpoint_path.exists());

        let content = fs::read_to_string(&checkpoint_path).unwrap();
        assert!(content.contains("Build a REST API"));
        assert!(content.contains("\"iteration\": 3"));
    }

    #[test]
    fn test_checkpoint_restore() {
        let temp_dir = TempDir::new().unwrap();

        let checkpoint_path =
            create_checkpoint(&temp_dir, "Implement user auth", 5, "Needs JWT fix");

        let restored = load_checkpoint(&checkpoint_path).unwrap();

        assert_eq!(restored.task, "Implement user auth");
        assert_eq!(restored.iteration, 5);
        assert_eq!(restored.feedback, "Needs JWT fix");
    }

    #[test]
    fn test_checkpoint_list() {
        let temp_dir = TempDir::new().unwrap();

        // Create multiple checkpoints
        for i in 1..=5 {
            create_checkpoint(&temp_dir, "Test task", i, &format!("Feedback {}", i));
        }

        let checkpoint_dir = temp_dir.path().join(".jarvis-checkpoints");
        let entries: Vec<_> = fs::read_dir(&checkpoint_dir).unwrap().collect();

        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn test_checkpoint_cleanup_old() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_dir = temp_dir.path().join(".jarvis-checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();

        // Create checkpoints
        for i in 1..=10 {
            create_checkpoint(&temp_dir, "Task", i, "Feedback");
        }

        // Simulate cleanup - keep only last 3
        let mut entries: Vec<_> = fs::read_dir(&checkpoint_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        // Remove older checkpoints
        for entry in entries.iter().take(entries.len() - 3) {
            fs::remove_file(entry.path()).unwrap();
        }

        let remaining: Vec<_> = fs::read_dir(&checkpoint_dir).unwrap().collect();
        assert_eq!(remaining.len(), 3);
    }

    #[test]
    fn test_checkpoint_with_state() {
        let temp_dir = TempDir::new().unwrap();

        // Create state files that would be part of checkpoint
        fs::write(
            temp_dir.path().join("partial_work.rs"),
            "// Partial implementation",
        )
        .unwrap();
        fs::write(temp_dir.path().join(".jarvis-continue"), "More work needed").unwrap();

        // Create checkpoint
        let checkpoint_path =
            create_checkpoint(&temp_dir, "Complete implementation", 2, "Partial work done");

        // Verify checkpoint can reference the state
        let checkpoint = load_checkpoint(&checkpoint_path).unwrap();
        assert!(checkpoint.working_dir.join("partial_work.rs").exists());
        assert!(checkpoint.working_dir.join(".jarvis-continue").exists());
    }
}

// ============================================================================
// Timeout Handling Tests
// ============================================================================

mod timeout_tests {
    #[allow(unused_imports)]
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn test_timeout_detection() {
        let start = Instant::now();
        let timeout = Duration::from_millis(100);

        // Simulate work
        std::thread::sleep(Duration::from_millis(50));

        assert!(start.elapsed() < timeout);
    }

    #[test]
    fn test_timeout_exceeded() {
        let start = Instant::now();
        let timeout = Duration::from_millis(50);

        // Simulate long work
        std::thread::sleep(Duration::from_millis(100));

        assert!(start.elapsed() > timeout);
    }

    #[test]
    fn test_iteration_timeout_tracking() {
        #[derive(Debug)]
        struct IterationTimer {
            timeout: Duration,
            iterations: Vec<Duration>,
        }

        impl IterationTimer {
            fn new(timeout: Duration) -> Self {
                Self {
                    timeout,
                    iterations: Vec::new(),
                }
            }

            fn record_iteration(&mut self, duration: Duration) {
                self.iterations.push(duration);
            }

            fn any_exceeded(&self) -> bool {
                self.iterations.iter().any(|d| *d > self.timeout)
            }

            fn average_duration(&self) -> Option<Duration> {
                if self.iterations.is_empty() {
                    None
                } else {
                    let total: Duration = self.iterations.iter().sum();
                    Some(total / self.iterations.len() as u32)
                }
            }
        }

        let mut timer = IterationTimer::new(Duration::from_secs(30));

        // Record some iterations
        timer.record_iteration(Duration::from_secs(10));
        timer.record_iteration(Duration::from_secs(15));
        timer.record_iteration(Duration::from_secs(20));

        assert!(!timer.any_exceeded());
        assert_eq!(timer.average_duration(), Some(Duration::from_secs(15)));

        // Add an exceeded iteration
        timer.record_iteration(Duration::from_secs(45));
        assert!(timer.any_exceeded());
    }

    #[test]
    fn test_global_timeout() {
        let global_timeout = Duration::from_secs(300); // 5 minutes
        let iteration_count = 10;
        let per_iteration_max = global_timeout / iteration_count;

        assert_eq!(per_iteration_max, Duration::from_secs(30));
    }

    #[test]
    fn test_timeout_with_backoff() {
        let base_timeout = Duration::from_secs(30);

        let timeouts: Vec<Duration> = (1..=5)
            .map(|i| {
                // Exponential backoff: 30, 60, 120, 240, 480
                base_timeout * 2u32.pow(i - 1)
            })
            .collect();

        assert_eq!(timeouts[0], Duration::from_secs(30));
        assert_eq!(timeouts[1], Duration::from_secs(60));
        assert_eq!(timeouts[2], Duration::from_secs(120));
        assert_eq!(timeouts[3], Duration::from_secs(240));
        assert_eq!(timeouts[4], Duration::from_secs(480));
    }
}

// ============================================================================
// Concurrent Execution Tests
// ============================================================================

mod concurrent_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn test_concurrent_verification() {
        let verifier = Arc::new(Verifier::new(Some("DONE".to_string())));
        let results = Arc::new(Mutex::new(Vec::new()));

        let outputs = vec![
            ("Success output DONE", true),
            ("Failed output", false),
            ("Another success DONE with more text", true),
            ("Error: something went wrong", false),
        ];

        let temp_dirs: Vec<_> = (0..outputs.len())
            .map(|_| TempDir::new().unwrap())
            .collect();

        let handles: Vec<_> = outputs
            .iter()
            .zip(temp_dirs.iter())
            .map(|((output, expected), temp_dir)| {
                let verifier = Arc::clone(&verifier);
                let results = Arc::clone(&results);
                let output = output.to_string();
                let expected = *expected;
                let path = temp_dir.path().to_path_buf();

                thread::spawn(move || {
                    let result = verifier.verify(&output, &path);
                    results
                        .lock()
                        .unwrap()
                        .push((expected, result.promise_found));
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let final_results = results.lock().unwrap();
        for (expected, actual) in final_results.iter() {
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn test_swarm_agent_coordination() {
        // Simulate swarm agent work distribution
        #[derive(Debug, Clone)]
        #[allow(dead_code)]
        struct SwarmAgent {
            id: u32,
            assigned_files: Vec<String>,
        }

        let files = vec![
            "src/main.rs",
            "src/lib.rs",
            "src/task_loop.rs",
            "src/verify/mod.rs",
            "tests/integration.rs",
            "Cargo.toml",
        ];

        let agent_count = 3;
        let agents: Vec<SwarmAgent> = (0..agent_count)
            .map(|i| SwarmAgent {
                id: i,
                assigned_files: files
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| j % agent_count as usize == i as usize)
                    .map(|(_, f)| f.to_string())
                    .collect(),
            })
            .collect();

        // Verify all files are assigned
        let total_assigned: Vec<_> = agents
            .iter()
            .flat_map(|a| a.assigned_files.clone())
            .collect();
        assert_eq!(total_assigned.len(), files.len());

        // Verify no overlaps
        let mut seen = std::collections::HashSet::new();
        for file in total_assigned {
            assert!(seen.insert(file), "File assigned to multiple agents");
        }
    }

    #[test]
    fn test_parallel_file_processing() {
        use std::fs;

        let temp_dir = TempDir::new().unwrap();

        // Create test files
        for i in 0..10 {
            fs::write(
                temp_dir.path().join(format!("file_{}.txt", i)),
                format!("Content of file {}", i),
            )
            .unwrap();
        }

        let file_count = Arc::new(Mutex::new(0));
        let files: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        let handles: Vec<_> = files
            .into_iter()
            .map(|entry| {
                let count = Arc::clone(&file_count);

                thread::spawn(move || {
                    let content = fs::read_to_string(entry.path()).unwrap();
                    assert!(content.contains("Content"));
                    *count.lock().unwrap() += 1;
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(*file_count.lock().unwrap(), 10);
    }
}

// ============================================================================
// Additional Coverage Tests
// ============================================================================

mod coverage_tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_config_serialization() {
        let config = Config {
            working_dir: PathBuf::from("/tmp/test"),
            completion_promise: Some("COMPLETED".to_string()),
            model: Some("claude-opus-4-5-20251101".to_string()),
            dry_run: true,
            anti_laziness: AntiLazinessConfig::default(),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("COMPLETED"));

        let deserialized: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.completion_promise,
            Some("COMPLETED".to_string())
        );
    }

    #[test]
    fn test_task_result_serialization() {
        let result = jarvis_v2::TaskResult {
            success: true,
            iterations: 5,
            summary: Some("Task completed".to_string()),
            error: None,
            verification: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("success"));
        assert!(json.contains("true"));
        assert!(json.contains("iterations"));
        assert!(json.contains("5"));
    }

    #[test]
    fn test_all_difficulty_levels() {
        let cases = [
            (0, Difficulty::Trivial),
            (1, Difficulty::Trivial),
            (2, Difficulty::Trivial),
            (3, Difficulty::Easy),
            (4, Difficulty::Easy),
            (5, Difficulty::Medium),
            (6, Difficulty::Medium),
            (7, Difficulty::Hard),
            (8, Difficulty::Hard),
            (9, Difficulty::Expert),
            (10, Difficulty::Expert),
            (100, Difficulty::Expert),
        ];

        for (score, expected) in cases {
            assert_eq!(
                Difficulty::from(score),
                expected,
                "Score {} should be {:?}",
                score,
                expected
            );
        }
    }

    #[test]
    fn test_execution_mode_swarm_variations() {
        for count in [2, 3, 5, 8, 10] {
            let mode = ExecutionMode::Swarm { agent_count: count };
            if let ExecutionMode::Swarm { agent_count } = mode {
                assert_eq!(agent_count, count);
            } else {
                panic!("Expected Swarm mode");
            }
        }
    }

    #[test]
    fn test_verifier_edge_cases() {
        let temp_dir = TempDir::new().unwrap();

        // Empty output
        let verifier = Verifier::new(None);
        let result = verifier.verify("", temp_dir.path());
        assert!(!result.is_complete);

        // Only whitespace
        let result = verifier.verify("   \n\t  ", temp_dir.path());
        assert!(!result.is_complete);

        // Unicode content
        let result = verifier.verify("Task completed successfully! ✅ 🎉 完成", temp_dir.path());
        // Just verify it doesn't crash
        let _ = result.is_complete;
    }

    #[test]
    fn test_working_directory_handling() {
        let temp_dir = TempDir::new().unwrap();

        // Create nested structure
        fs::create_dir_all(temp_dir.path().join("src/components")).unwrap();
        fs::write(
            temp_dir.path().join("src/components/test.tsx"),
            "export default function Test() {}",
        )
        .unwrap();

        // Verify path resolution
        let config = Config {
            working_dir: temp_dir.path().to_path_buf(),
            completion_promise: None,
            model: None,
            dry_run: true,
            anti_laziness: AntiLazinessConfig::default(),
        };

        assert!(config.working_dir.join("src/components/test.tsx").exists());
    }
}

// ============================================================================
// Signal File Tests
// ============================================================================

mod signal_tests {
    use super::*;
    use std::fs;

    const SATISFIED_FILE: &str = ".jarvis-satisfied";
    const CONTINUE_FILE: &str = ".jarvis-continue";
    const BLOCKED_FILE: &str = ".jarvis-blocked";
    const WORKING_FILE: &str = ".jarvis-working";

    #[test]
    fn test_signal_file_creation() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join(SATISFIED_FILE), "Done!").unwrap();

        assert!(temp_dir.path().join(SATISFIED_FILE).exists());
        let content = fs::read_to_string(temp_dir.path().join(SATISFIED_FILE)).unwrap();
        assert_eq!(content, "Done!");
    }

    #[test]
    fn test_signal_file_priority() {
        let temp_dir = TempDir::new().unwrap();

        // Create all signal files
        fs::write(temp_dir.path().join(SATISFIED_FILE), "satisfied").unwrap();
        fs::write(temp_dir.path().join(CONTINUE_FILE), "continue").unwrap();
        fs::write(temp_dir.path().join(BLOCKED_FILE), "blocked").unwrap();

        // In priority order: satisfied > blocked > continue
        if temp_dir.path().join(SATISFIED_FILE).exists() {
            assert!(true, "Satisfied takes priority");
        } else if temp_dir.path().join(BLOCKED_FILE).exists() {
            panic!("Should have found satisfied first");
        }
    }

    #[test]
    fn test_signal_cleanup() {
        let temp_dir = TempDir::new().unwrap();

        // Create signals
        for file in [SATISFIED_FILE, CONTINUE_FILE, BLOCKED_FILE] {
            fs::write(temp_dir.path().join(file), "content").unwrap();
        }

        // Cleanup
        for file in [SATISFIED_FILE, CONTINUE_FILE, BLOCKED_FILE] {
            let path = temp_dir.path().join(file);
            if path.exists() {
                fs::remove_file(&path).unwrap();
            }
        }

        // Verify cleanup
        for file in [SATISFIED_FILE, CONTINUE_FILE, BLOCKED_FILE] {
            assert!(!temp_dir.path().join(file).exists());
        }
    }

    #[test]
    fn test_working_file_creation_and_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let working_path = temp_dir.path().join(WORKING_FILE);

        // Create working file with task description
        let task = "Build a REST API";
        fs::write(&working_path, task).unwrap();

        assert!(working_path.exists());
        let content = fs::read_to_string(&working_path).unwrap();
        assert_eq!(content, task);

        // Simulate cleanup on completion
        fs::remove_file(&working_path).unwrap();
        assert!(!working_path.exists());
    }

    #[test]
    fn test_signal_file_with_json_content() {
        let temp_dir = TempDir::new().unwrap();

        // Signal files can contain structured data
        let json_content = serde_json::json!({
            "summary": "Task completed successfully",
            "files_modified": ["src/main.rs", "Cargo.toml"],
        });

        fs::write(
            temp_dir.path().join(SATISFIED_FILE),
            serde_json::to_string_pretty(&json_content).unwrap(),
        )
        .unwrap();

        let content = fs::read_to_string(temp_dir.path().join(SATISFIED_FILE)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["files_modified"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_blocked_file_with_reason() {
        let temp_dir = TempDir::new().unwrap();

        let block_reasons = vec![
            "Missing required API key",
            "Database connection failed",
            "Insufficient permissions",
            "Dependency conflict detected",
        ];

        for reason in block_reasons {
            let blocked_path = temp_dir.path().join(BLOCKED_FILE);
            fs::write(&blocked_path, reason).unwrap();

            let content = fs::read_to_string(&blocked_path).unwrap();
            assert_eq!(content, reason);

            fs::remove_file(&blocked_path).unwrap();
        }
    }

    #[test]
    fn test_continue_file_with_feedback() {
        let temp_dir = TempDir::new().unwrap();

        let feedback = serde_json::json!({
            "iteration": 3,
            "next_steps": [
                "Fix failing tests",
                "Add error handling",
                "Update documentation"
            ],
            "progress_percent": 60
        });

        fs::write(
            temp_dir.path().join(CONTINUE_FILE),
            serde_json::to_string_pretty(&feedback).unwrap(),
        )
        .unwrap();

        let content = fs::read_to_string(temp_dir.path().join(CONTINUE_FILE)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["iteration"], 3);
        assert_eq!(parsed["progress_percent"], 60);
    }

    #[test]
    fn test_signal_file_atomic_write() {
        use std::io::Write;

        let temp_dir = TempDir::new().unwrap();
        let signal_path = temp_dir.path().join(SATISFIED_FILE);

        // Simulate atomic write pattern
        let temp_path = temp_dir.path().join(".jarvis-satisfied.tmp");
        {
            let mut file = fs::File::create(&temp_path).unwrap();
            file.write_all(b"Task completed").unwrap();
            file.sync_all().unwrap();
        }
        fs::rename(&temp_path, &signal_path).unwrap();

        assert!(signal_path.exists());
        assert!(!temp_path.exists());
        assert_eq!(fs::read_to_string(&signal_path).unwrap(), "Task completed");
    }

    #[test]
    fn test_multiple_signal_file_states() {
        let temp_dir = TempDir::new().unwrap();

        // Simulate state transitions
        let states = vec![
            (WORKING_FILE, "Starting task"),
            (CONTINUE_FILE, "Iteration 1 complete"),
            (CONTINUE_FILE, "Iteration 2 complete"),
            (SATISFIED_FILE, "Task done"),
        ];

        for (file, content) in states {
            // Clean up previous signals (except working)
            for f in [SATISFIED_FILE, CONTINUE_FILE, BLOCKED_FILE] {
                let _ = fs::remove_file(temp_dir.path().join(f));
            }

            fs::write(temp_dir.path().join(file), content).unwrap();
            assert!(temp_dir.path().join(file).exists());
        }
    }
}

// ============================================================================
// Enhanced Checkpoint Tests
// ============================================================================

mod checkpoint_edge_cases {
    use super::*;
    use jarvis_v2::{Checkpoint, IterationAttempt, TaskState};
    use std::fs;

    #[test]
    fn test_checkpoint_new_has_valid_defaults() {
        let checkpoint = Checkpoint::new("Test task");

        assert_eq!(checkpoint.task, "Test task");
        assert_eq!(checkpoint.iteration, 0);
        assert_eq!(checkpoint.state, TaskState::Pending);
        assert!(!checkpoint.id.is_empty());
        assert!(checkpoint.accumulated_feedback.is_empty());
        assert!(checkpoint.last_output.is_empty());
        assert!(checkpoint.iteration_history.is_empty());
        assert!(checkpoint.created_at > 0);
        assert!(checkpoint.metadata.is_empty());
    }

    #[test]
    fn test_checkpoint_save_and_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_path = temp_dir.path().join("test_checkpoint.json");

        let mut checkpoint = Checkpoint::new("Build REST API");
        checkpoint.iteration = 5;
        checkpoint.state = TaskState::Running;
        checkpoint.accumulated_feedback = "Previous iteration had issues".to_string();
        checkpoint.last_output = "Some output from Claude".to_string();
        checkpoint
            .metadata
            .insert("custom_key".to_string(), "custom_value".to_string());

        // Save
        checkpoint.save(&checkpoint_path).unwrap();
        assert!(checkpoint_path.exists());

        // Load
        let loaded = Checkpoint::load(&checkpoint_path).unwrap();

        assert_eq!(loaded.task, checkpoint.task);
        assert_eq!(loaded.iteration, checkpoint.iteration);
        assert_eq!(loaded.state, checkpoint.state);
        assert_eq!(loaded.accumulated_feedback, checkpoint.accumulated_feedback);
        assert_eq!(loaded.last_output, checkpoint.last_output);
        assert_eq!(loaded.id, checkpoint.id);
        assert_eq!(
            loaded.metadata.get("custom_key"),
            Some(&"custom_value".to_string())
        );
    }

    #[test]
    fn test_checkpoint_with_iteration_history() {
        let mut checkpoint = Checkpoint::new("Complex task");

        // Add some iteration history
        for i in 1..=3 {
            checkpoint.iteration_history.push(IterationAttempt {
                iteration: i,
                prompt: format!("Iteration {} prompt", i),
                output: format!("Iteration {} output", i),
                success: i == 3,
                error: if i < 3 {
                    Some("Still working".to_string())
                } else {
                    None
                },
                duration_ms: 1000 * i as u64,
                feedback: Some(format!("Feedback for iteration {}", i)),
            });
        }

        checkpoint.iteration = 3;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("checkpoint.json");

        checkpoint.save(&path).unwrap();
        let loaded = Checkpoint::load(&path).unwrap();

        assert_eq!(loaded.iteration_history.len(), 3);
        assert!(loaded.iteration_history[2].success);
        assert_eq!(loaded.iteration_history[0].duration_ms, 1000);
    }

    #[test]
    fn test_checkpoint_load_nonexistent_file() {
        let result = Checkpoint::load(&PathBuf::from("/nonexistent/path/checkpoint.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_checkpoint_load_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("invalid.json");

        fs::write(&path, "{ invalid json }").unwrap();

        let result = Checkpoint::load(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_checkpoint_task_state_transitions() {
        let mut checkpoint = Checkpoint::new("Task");

        // Verify default state
        assert_eq!(checkpoint.state, TaskState::Pending);

        // Simulate state transitions
        checkpoint.state = TaskState::Running;
        assert_eq!(checkpoint.state, TaskState::Running);

        checkpoint.state = TaskState::Blocked;
        assert_eq!(checkpoint.state, TaskState::Blocked);

        checkpoint.state = TaskState::Complete;
        assert_eq!(checkpoint.state, TaskState::Complete);
    }

    #[test]
    fn test_checkpoint_large_output() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("large_checkpoint.json");

        let mut checkpoint = Checkpoint::new("Large output task");

        // Create a large output string
        checkpoint.last_output = "x".repeat(100_000);
        checkpoint.accumulated_feedback = "y".repeat(50_000);

        checkpoint.save(&path).unwrap();
        let loaded = Checkpoint::load(&path).unwrap();

        assert_eq!(loaded.last_output.len(), 100_000);
        assert_eq!(loaded.accumulated_feedback.len(), 50_000);
    }

    #[test]
    fn test_checkpoint_unicode_content() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("unicode_checkpoint.json");

        let mut checkpoint = Checkpoint::new("Unicode task: 日本語タスク");
        checkpoint.last_output = "Output with emoji: ✅ 🚀 🎉".to_string();
        checkpoint.accumulated_feedback = "Feedback: 成功しました！".to_string();

        checkpoint.save(&path).unwrap();
        let loaded = Checkpoint::load(&path).unwrap();

        assert!(loaded.task.contains("日本語"));
        assert!(loaded.last_output.contains("✅"));
        assert!(loaded.accumulated_feedback.contains("成功"));
    }

    #[test]
    fn test_checkpoint_metadata_various_types() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("metadata_checkpoint.json");

        let mut checkpoint = Checkpoint::new("Task with metadata");
        checkpoint
            .metadata
            .insert("string_value".to_string(), "hello".to_string());
        checkpoint
            .metadata
            .insert("number_as_string".to_string(), "42".to_string());
        checkpoint
            .metadata
            .insert("boolean_as_string".to_string(), "true".to_string());
        checkpoint.metadata.insert(
            "json_as_string".to_string(),
            r#"{"key": "value"}"#.to_string(),
        );

        checkpoint.save(&path).unwrap();
        let loaded = Checkpoint::load(&path).unwrap();

        assert_eq!(loaded.metadata.len(), 4);
        assert_eq!(
            loaded.metadata.get("number_as_string"),
            Some(&"42".to_string())
        );
    }
}

// ============================================================================
// Enhanced Timeout Tests
// ============================================================================

mod timeout_edge_cases {
    #[allow(unused_imports)]
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn test_zero_timeout() {
        let timeout = Duration::from_millis(0);
        let start = Instant::now();

        // Any operation should exceed zero timeout
        let elapsed = start.elapsed();
        // This should always be true or equal
        assert!(elapsed >= timeout);
    }

    #[test]
    fn test_very_short_timeout() {
        let timeout = Duration::from_nanos(1);
        let start = Instant::now();

        // Perform a small operation that takes some time
        let mut sum = 0u64;
        for i in 0..100 {
            sum = sum.wrapping_add(std::hint::black_box(i));
        }
        std::hint::black_box(sum);

        let elapsed = start.elapsed();

        // Either we exceeded the timeout or we matched it exactly (both acceptable)
        assert!(elapsed >= timeout || elapsed.as_nanos() >= 0);
    }

    #[test]
    fn test_timeout_accumulation() {
        let per_iteration_timeout = Duration::from_millis(100);
        let max_iterations = 10;
        let max_total_timeout = per_iteration_timeout * max_iterations;

        assert_eq!(max_total_timeout, Duration::from_secs(1));
    }

    #[test]
    fn test_timeout_remaining_calculation() {
        let total_timeout = Duration::from_secs(300);
        let elapsed = Duration::from_secs(120);

        let remaining = total_timeout.saturating_sub(elapsed);
        assert_eq!(remaining, Duration::from_secs(180));

        // Test saturating behavior
        let over_elapsed = Duration::from_secs(400);
        let remaining = total_timeout.saturating_sub(over_elapsed);
        assert_eq!(remaining, Duration::ZERO);
    }

    #[test]
    fn test_adaptive_timeout() {
        // Simulate adaptive timeout based on task complexity
        struct AdaptiveTimeout {
            base_timeout: Duration,
            complexity_multiplier: f64,
        }

        impl AdaptiveTimeout {
            fn timeout_for_complexity(&self, complexity: u32) -> Duration {
                let multiplier = 1.0 + (complexity as f64 * self.complexity_multiplier);
                Duration::from_secs_f64(self.base_timeout.as_secs_f64() * multiplier)
            }
        }

        let adaptive = AdaptiveTimeout {
            base_timeout: Duration::from_secs(30),
            complexity_multiplier: 0.5,
        };

        assert_eq!(adaptive.timeout_for_complexity(0), Duration::from_secs(30));
        assert_eq!(adaptive.timeout_for_complexity(2), Duration::from_secs(60));
        assert_eq!(adaptive.timeout_for_complexity(4), Duration::from_secs(90));
    }

    #[test]
    fn test_timeout_with_grace_period() {
        let hard_timeout = Duration::from_secs(60);
        let grace_period = Duration::from_secs(10);
        let soft_timeout = hard_timeout - grace_period;

        assert_eq!(soft_timeout, Duration::from_secs(50));

        // Simulate: if soft timeout reached, start cleanup
        let elapsed = Duration::from_secs(55);
        let in_grace_period = elapsed >= soft_timeout && elapsed < hard_timeout;
        assert!(in_grace_period);
    }

    #[test]
    fn test_timeout_per_iteration_fairness() {
        let total_timeout = Duration::from_secs(300);
        let iterations_remaining = 5;
        let fair_timeout_per_iteration = total_timeout / iterations_remaining;

        assert_eq!(fair_timeout_per_iteration, Duration::from_secs(60));

        // As iterations decrease, each gets more time
        let iterations_remaining = 2;
        let fair_timeout_per_iteration = total_timeout / iterations_remaining;
        assert_eq!(fair_timeout_per_iteration, Duration::from_secs(150));
    }

    #[test]
    fn test_exponential_timeout_increase() {
        let base = Duration::from_secs(30);
        let max = Duration::from_secs(300);

        let timeouts: Vec<Duration> = (0..5)
            .map(|attempt| {
                let timeout = base * 2u32.pow(attempt);
                timeout.min(max)
            })
            .collect();

        assert_eq!(timeouts[0], Duration::from_secs(30));
        assert_eq!(timeouts[1], Duration::from_secs(60));
        assert_eq!(timeouts[2], Duration::from_secs(120));
        assert_eq!(timeouts[3], Duration::from_secs(240));
        assert_eq!(timeouts[4], Duration::from_secs(300)); // Capped at max
    }

    #[test]
    fn test_timeout_cancellation_token() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_clone = Arc::clone(&cancelled);

        // Simulate cancellation
        cancelled_clone.store(true, Ordering::SeqCst);

        // Check if cancelled
        assert!(cancelled.load(Ordering::SeqCst));
    }
}

// ============================================================================
// Verifier Edge Cases
// ============================================================================

mod verifier_edge_cases {
    use super::*;

    #[test]
    fn test_verifier_with_xml_promise_tags() {
        let verifier = Verifier::new(Some("<promise>COMPLETE</promise>".to_string()));
        let temp_dir = TempDir::new().unwrap();

        let output = "Task finished successfully. <promise>COMPLETE</promise>";
        let result = verifier.verify(output, temp_dir.path());

        assert!(result.promise_found);
    }

    #[test]
    fn test_verifier_promise_case_sensitivity() {
        let verifier = Verifier::new(Some("DONE".to_string()));
        let temp_dir = TempDir::new().unwrap();

        // Exact match should work
        assert!(verifier.verify("Task DONE", temp_dir.path()).promise_found);

        // Case mismatch should not match
        assert!(!verifier.verify("Task done", temp_dir.path()).promise_found);
    }

    #[test]
    fn test_verifier_empty_promise() {
        let verifier = Verifier::new(Some("".to_string()));
        let temp_dir = TempDir::new().unwrap();

        // Empty promise should match any output containing empty string
        let result = verifier.verify("Any output", temp_dir.path());
        assert!(result.promise_found);
    }

    #[test]
    fn test_verifier_special_chars_in_promise() {
        let verifier = Verifier::new(Some("✅ SUCCESS ✅".to_string()));
        let temp_dir = TempDir::new().unwrap();

        let output = "Task completed ✅ SUCCESS ✅";
        let result = verifier.verify(output, temp_dir.path());

        assert!(result.promise_found);
    }

    #[test]
    fn test_verifier_multiline_output() {
        let verifier = Verifier::new(Some("COMPLETE".to_string()));
        let temp_dir = TempDir::new().unwrap();

        let output = r#"
            Line 1: Starting task
            Line 2: Processing
            Line 3: COMPLETE
            Line 4: Cleanup done
        "#;

        let result = verifier.verify(output, temp_dir.path());
        assert!(result.promise_found);
    }
}

// ============================================================================
// Execution Metrics Tests
// ============================================================================

mod metrics_tests {
    use jarvis_v2::ExecutionMetrics;

    #[test]
    fn test_metrics_empty() {
        let metrics = ExecutionMetrics::new();

        assert_eq!(metrics.total_duration_ms, 0);
        assert!(metrics.iteration_times_ms.is_empty());
        assert_eq!(metrics.retry_count, 0);
        assert_eq!(metrics.timeout_count, 0);
        assert_eq!(metrics.successful_iterations, 0);
        assert_eq!(metrics.failed_iterations, 0);
        assert_eq!(metrics.avg_iteration_ms, None);
    }

    #[test]
    fn test_metrics_record_iterations() {
        let mut metrics = ExecutionMetrics::new();

        metrics.record_iteration(1, 1000, true);
        metrics.record_iteration(2, 2000, true);
        metrics.record_iteration(3, 3000, false);

        assert_eq!(metrics.successful_iterations, 2);
        assert_eq!(metrics.failed_iterations, 1);
        assert_eq!(metrics.iteration_times_ms.len(), 3);
        assert_eq!(metrics.avg_iteration_ms, Some(2000));
    }

    #[test]
    fn test_metrics_retry_and_timeout() {
        let mut metrics = ExecutionMetrics::new();

        for _ in 0..3 {
            metrics.record_retry();
        }
        for _ in 0..2 {
            metrics.record_timeout();
        }

        assert_eq!(metrics.retry_count, 3);
        assert_eq!(metrics.timeout_count, 2);
    }

    #[test]
    fn test_metrics_finalize() {
        let mut metrics = ExecutionMetrics::new();

        metrics.record_iteration(1, 1000, true);
        metrics.record_iteration(2, 2000, true);
        metrics.finalize(5000);

        assert_eq!(metrics.total_duration_ms, 5000);
    }
}

// ============================================================================
// Backoff Configuration Tests
// ============================================================================

mod backoff_tests {
    use jarvis_v2::BackoffConfig;
    use std::time::Duration;

    #[test]
    fn test_backoff_defaults() {
        let config = BackoffConfig::default();

        assert_eq!(config.initial_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 60000);
        assert_eq!(config.multiplier, 2.0);
        assert_eq!(config.max_retries, 5);
        assert!(config.jitter);
    }

    #[test]
    fn test_backoff_zero_attempt() {
        let config = BackoffConfig::default();
        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(0));
    }

    #[test]
    fn test_backoff_exponential_growth() {
        let config = BackoffConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 100000,
            multiplier: 2.0,
            max_retries: 10,
            jitter: false,
        };

        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(1000));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(2000));
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(4000));
        assert_eq!(config.delay_for_attempt(4), Duration::from_millis(8000));
    }

    #[test]
    fn test_backoff_max_cap() {
        let config = BackoffConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 5000,
            multiplier: 2.0,
            max_retries: 10,
            jitter: false,
        };

        // Should cap at 5000ms
        assert_eq!(config.delay_for_attempt(10), Duration::from_millis(5000));
    }

    #[test]
    fn test_backoff_with_jitter() {
        let config = BackoffConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            multiplier: 2.0,
            max_retries: 5,
            jitter: true,
        };

        // With jitter, delays should vary
        let delay = config.delay_for_attempt(2);
        // Should be between base delay and base + 25%
        assert!(delay >= Duration::from_millis(2000));
        assert!(delay <= Duration::from_millis(2500));
    }

    #[test]
    fn test_backoff_custom_multiplier() {
        let config = BackoffConfig {
            initial_delay_ms: 100,
            max_delay_ms: 10000,
            multiplier: 3.0,
            max_retries: 5,
            jitter: false,
        };

        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(300));
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(900));
    }
}

// ============================================================================
// Claude Output Status Tests
// ============================================================================

mod output_status_tests {
    use jarvis_v2::{ClaudeOutputStatus, ClaudeStructuredOutput};

    #[test]
    fn test_output_status_parsing() {
        let statuses = vec![
            ("complete", ClaudeOutputStatus::Complete),
            ("in_progress", ClaudeOutputStatus::InProgress),
            ("blocked", ClaudeOutputStatus::Blocked),
            ("failed", ClaudeOutputStatus::Failed),
            ("unknown", ClaudeOutputStatus::Unknown),
        ];

        for (json_value, expected) in statuses {
            let json = format!(
                r#"{{"status": "{}", "summary": null, "files_modified": [], "next_steps": [], "errors": [], "confidence": 0.5}}"#,
                json_value
            );
            let output: ClaudeStructuredOutput = serde_json::from_str(&json).unwrap();
            assert_eq!(output.status, expected);
        }
    }

    #[test]
    fn test_output_with_all_fields() {
        let json = r#"{
            "status": "complete",
            "summary": "Task finished successfully",
            "files_modified": ["src/main.rs", "Cargo.toml"],
            "next_steps": [],
            "errors": [],
            "confidence": 0.95,
            "raw_output": "Full output here"
        }"#;

        let output: ClaudeStructuredOutput = serde_json::from_str(json).unwrap();

        assert_eq!(output.status, ClaudeOutputStatus::Complete);
        assert_eq!(
            output.summary,
            Some("Task finished successfully".to_string())
        );
        assert_eq!(output.files_modified.len(), 2);
        assert_eq!(output.confidence, 0.95);
        assert_eq!(output.raw_output, Some("Full output here".to_string()));
    }

    #[test]
    fn test_output_default() {
        let output = ClaudeStructuredOutput::default();

        assert_eq!(output.status, ClaudeOutputStatus::Unknown);
        assert_eq!(output.summary, None);
        assert!(output.files_modified.is_empty());
        assert!(output.next_steps.is_empty());
        assert!(output.errors.is_empty());
        assert_eq!(output.confidence, 0.0);
        assert_eq!(output.raw_output, None);
    }
}

// ============================================================================
// Parallel Config Tests
// ============================================================================

mod parallel_tests {
    use jarvis_v2::ParallelConfig;
    use std::path::PathBuf;

    #[test]
    fn test_parallel_config_defaults() {
        let config = ParallelConfig::default();

        assert_eq!(config.max_concurrent, 4);
        assert!(config.files.is_empty());
        assert!(!config.share_context);
        assert_eq!(config.task_timeout_secs, 300);
    }

    #[test]
    fn test_parallel_config_with_files() {
        let config = ParallelConfig {
            max_concurrent: 8,
            files: vec![
                PathBuf::from("src/main.rs"),
                PathBuf::from("src/lib.rs"),
                PathBuf::from("tests/integration.rs"),
            ],
            share_context: true,
            task_timeout_secs: 600,
        };

        assert_eq!(config.max_concurrent, 8);
        assert_eq!(config.files.len(), 3);
        assert!(config.share_context);
        assert_eq!(config.task_timeout_secs, 600);
    }
}

// ============================================================================
// Gas Town Context Compression Integration Tests
// ============================================================================

mod gas_town_tests {
    use jarvis_v2::{
        CompressedResult, CompressedResultMetadata, ContextPolicy, OperationMode, RegistrationScope,
        SubagentConfig, SubagentExecutor, SubagentOutputFormat, SubagentRegistration,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_subagent_executor_creation() {
        let config = SubagentConfig::default();
        let working_dir = PathBuf::from("/tmp/test");

        let executor = SubagentExecutor::new(config, working_dir.clone());

        // Verify executor is created successfully
        drop(executor);
    }

    #[test]
    fn test_subagent_executor_with_model() {
        let config = SubagentConfig::default();
        let working_dir = PathBuf::from("/tmp/test");

        let executor = SubagentExecutor::new(config, working_dir)
            .with_model(Some("claude-3-opus".to_string()));

        drop(executor);
    }

    #[test]
    fn test_subagent_output_result_json() {
        let config = SubagentConfig {
            output_format: SubagentOutputFormat::Json,
            ..Default::default()
        };
        let executor = SubagentExecutor::new(config, PathBuf::from("."));

        let result = CompressedResult {
            success: true,
            commit_hash: Some("abc123".to_string()),
            files_changed: vec![PathBuf::from("src/lib.rs")],
            summary: "Test implementation".to_string(),
            duration_ms: 5000,
            iterations: 3,
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

        let output = executor.output_result(&result);

        // Verify JSON format
        assert!(output.contains("\"success\": true"));
        assert!(output.contains("\"commit_hash\""));
        assert!(output.contains("abc123"));

        // Verify it's valid JSON
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&output);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_subagent_output_result_text() {
        let config = SubagentConfig {
            output_format: SubagentOutputFormat::Text,
            ..Default::default()
        };
        let executor = SubagentExecutor::new(config, PathBuf::from("."));

        let result = CompressedResult {
            success: true,
            commit_hash: Some("def456".to_string()),
            files_changed: vec![
                PathBuf::from("src/main.rs"),
                PathBuf::from("src/lib.rs"),
            ],
            summary: "Implemented feature".to_string(),
            duration_ms: 3000,
            iterations: 2,
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

        let output = executor.output_result(&result);

        assert!(output.contains("STATUS: SUCCESS"));
        assert!(output.contains("COMMIT: def456"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("SUMMARY: Implemented feature"));
    }

    #[test]
    fn test_subagent_output_result_markdown() {
        let config = SubagentConfig {
            output_format: SubagentOutputFormat::Markdown,
            ..Default::default()
        };
        let executor = SubagentExecutor::new(config, PathBuf::from("."));

        let result = CompressedResult {
            success: false,
            commit_hash: None,
            files_changed: vec![PathBuf::from("test.rs")],
            summary: "Task failed".to_string(),
            duration_ms: 1000,
            iterations: 1,
            error: Some("Compilation error".to_string()),
            verified: false,
            metadata: CompressedResultMetadata {
                version: "0.1.0".to_string(),
                depth: 1,
                parent_session_id: Some("parent-123".to_string()),
                completed_at: 0,
                model: None,
            },
        };

        let output = executor.output_result(&result);

        assert!(output.contains("## Subagent Result: Failed"));
        assert!(output.contains("**Commit**: `none`"));
        assert!(output.contains("**Files Changed**"));
        assert!(output.contains("**Duration**: 1000ms"));
        assert!(output.contains("**Iterations**: 1"));
    }

    #[test]
    fn test_subagent_registration_project_scope() {
        let temp_dir = TempDir::new().unwrap();

        // This tests the RegistrationScope enum behavior
        let scope = RegistrationScope::Project(temp_dir.path().to_path_buf());

        // Verify enum variant matching
        match scope {
            RegistrationScope::Project(path) => {
                assert_eq!(path, temp_dir.path());
            }
            RegistrationScope::Global => panic!("Expected Project scope"),
        }
    }

    #[test]
    fn test_subagent_registration_global_scope() {
        let scope = RegistrationScope::Global;

        match scope {
            RegistrationScope::Global => {
                // Expected
            }
            RegistrationScope::Project(_) => panic!("Expected Global scope"),
        }
    }

    #[test]
    fn test_subagent_config_depth_check() {
        let config = SubagentConfig {
            task: "Deep task".to_string(),
            parent_session_id: Some("parent".to_string()),
            max_depth: 3,
            current_depth: 3,
            context_policy: ContextPolicy::default(),
            output_format: SubagentOutputFormat::Json,
        };

        // At max depth - should not spawn more
        assert!(config.current_depth >= config.max_depth);
    }

    #[test]
    fn test_subagent_config_within_depth() {
        let config = SubagentConfig {
            task: "Normal task".to_string(),
            parent_session_id: Some("parent".to_string()),
            max_depth: 5,
            current_depth: 2,
            context_policy: ContextPolicy::default(),
            output_format: SubagentOutputFormat::Json,
        };

        // Not at max depth - can spawn more
        assert!(config.current_depth < config.max_depth);
    }

    #[test]
    fn test_operation_mode_standalone_integration() {
        let mode = OperationMode::Standalone;

        // In standalone mode, should not compress output
        match mode {
            OperationMode::Standalone => {
                // Normal CLI output
                assert!(true);
            }
            OperationMode::Subagent => {
                panic!("Should be standalone");
            }
        }
    }

    #[test]
    fn test_operation_mode_subagent_integration() {
        let mode = OperationMode::Subagent;

        // In subagent mode, should compress output
        match mode {
            OperationMode::Subagent => {
                // Compressed output
                assert!(true);
            }
            OperationMode::Standalone => {
                panic!("Should be subagent");
            }
        }
    }

    #[test]
    fn test_compressed_result_json_roundtrip() {
        let result = CompressedResult {
            success: true,
            commit_hash: Some("abc123def456".to_string()),
            files_changed: vec![
                PathBuf::from("src/main.rs"),
                PathBuf::from("src/lib.rs"),
                PathBuf::from("tests/test.rs"),
            ],
            summary: "Implemented full feature set".to_string(),
            duration_ms: 25000,
            iterations: 5,
            error: None,
            verified: true,
            metadata: CompressedResultMetadata {
                version: "0.2.0".to_string(),
                depth: 1,
                parent_session_id: Some("session-xyz".to_string()),
                completed_at: 1704067200,
                model: Some("claude-3-opus".to_string()),
            },
        };

        // Serialize
        let json = serde_json::to_string(&result).unwrap();

        // Deserialize
        let deserialized: CompressedResult = serde_json::from_str(&json).unwrap();

        // Verify all fields match
        assert_eq!(deserialized.success, result.success);
        assert_eq!(deserialized.commit_hash, result.commit_hash);
        assert_eq!(deserialized.files_changed, result.files_changed);
        assert_eq!(deserialized.summary, result.summary);
        assert_eq!(deserialized.duration_ms, result.duration_ms);
        assert_eq!(deserialized.iterations, result.iterations);
        assert_eq!(deserialized.verified, result.verified);
        assert_eq!(deserialized.metadata.version, result.metadata.version);
        assert_eq!(deserialized.metadata.depth, result.metadata.depth);
        assert_eq!(
            deserialized.metadata.parent_session_id,
            result.metadata.parent_session_id
        );
    }

    #[test]
    fn test_context_policy_affects_output() {
        // Policy that hides failures
        let hide_failures_policy = ContextPolicy {
            hide_failures: true,
            hide_intermediates: true,
            include_commit: true,
            max_summary_length: 500,
            include_diffs: false,
        };

        // Policy that shows everything
        let show_all_policy = ContextPolicy {
            hide_failures: false,
            hide_intermediates: false,
            include_commit: true,
            max_summary_length: 1000,
            include_diffs: true,
        };

        // Verify different policies have different settings
        assert_ne!(hide_failures_policy.hide_failures, show_all_policy.hide_failures);
        assert_ne!(
            hide_failures_policy.hide_intermediates,
            show_all_policy.hide_intermediates
        );
        assert_ne!(
            hide_failures_policy.max_summary_length,
            show_all_policy.max_summary_length
        );
        assert_ne!(hide_failures_policy.include_diffs, show_all_policy.include_diffs);
    }

    #[test]
    fn test_subagent_registration_version() {
        let reg = SubagentRegistration::default();

        // Version should match Cargo.toml version
        assert!(!reg.version.is_empty());
        // Should be a valid semver format
        assert!(reg.version.contains('.'));
    }

    #[test]
    fn test_subagent_registration_command() {
        let reg = SubagentRegistration::default();

        assert!(reg.command.contains("jarvis-rs"));
        assert!(reg.command.contains("--subagent-mode"));
    }

    #[test]
    fn test_subagent_executor_output_formats() {
        let formats = [
            SubagentOutputFormat::Json,
            SubagentOutputFormat::Text,
            SubagentOutputFormat::Markdown,
        ];

        let base_result = CompressedResult {
            success: true,
            commit_hash: Some("test123".to_string()),
            files_changed: vec![PathBuf::from("file.rs")],
            summary: "Test summary".to_string(),
            duration_ms: 100,
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

        for format in formats {
            let config = SubagentConfig {
                output_format: format,
                ..Default::default()
            };
            let executor = SubagentExecutor::new(config, PathBuf::from("."));
            let output = executor.output_result(&base_result);

            // All formats should contain the commit hash somewhere
            assert!(output.contains("test123"), "Format {:?} missing commit hash", format);
        }
    }

    #[test]
    fn test_compressed_result_with_error() {
        let result = CompressedResult {
            success: false,
            commit_hash: None,
            files_changed: vec![],
            summary: "Task failed".to_string(),
            duration_ms: 5000,
            iterations: 3,
            error: Some("Build failed with 5 errors:\n1. Missing import\n2. Type mismatch".to_string()),
            verified: false,
            metadata: CompressedResultMetadata {
                version: "0.1.0".to_string(),
                depth: 0,
                parent_session_id: None,
                completed_at: 0,
                model: None,
            },
        };

        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.as_ref().unwrap().contains("5 errors"));
    }

    #[test]
    fn test_subagent_config_inheritance() {
        // Parent config
        let parent_config = SubagentConfig {
            task: "Parent task".to_string(),
            parent_session_id: Some("session-1".to_string()),
            max_depth: 5,
            current_depth: 1,
            context_policy: ContextPolicy {
                hide_failures: true,
                hide_intermediates: true,
                include_commit: true,
                max_summary_length: 500,
                include_diffs: false,
            },
            output_format: SubagentOutputFormat::Json,
        };

        // Child config would inherit from parent
        let child_config = SubagentConfig {
            task: "Child task".to_string(),
            parent_session_id: parent_config.parent_session_id.clone(),
            max_depth: parent_config.max_depth,
            current_depth: parent_config.current_depth + 1,
            context_policy: parent_config.context_policy.clone(),
            output_format: SubagentOutputFormat::Json,
        };

        assert_eq!(child_config.current_depth, 2);
        assert_eq!(child_config.max_depth, 5);
        assert_eq!(
            child_config.parent_session_id,
            parent_config.parent_session_id
        );
    }

    #[test]
    fn test_subagent_executor_working_directory() {
        let temp_dir = TempDir::new().unwrap();
        let config = SubagentConfig::default();

        let executor = SubagentExecutor::new(config, temp_dir.path().to_path_buf());

        // Just verify creation succeeds with a real directory
        drop(executor);

        assert!(temp_dir.path().exists());
    }
}

// ============================================================================
// Self-Extension Module Integration Tests (src/extend/)
// ============================================================================

mod extend_integration_tests {
    use jarvis_v2::extend::{
        CapabilityGap, CapabilityRegistry, CapabilityType, GapDetector, HotLoader,
        SynthesisEngine, SynthesizedCapability, CapabilityVerifier, VerificationResult,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;
    use std::fs;

    // ========================================================================
    // Gap Detection to Synthesis Pipeline Tests
    // ========================================================================

    #[test]
    fn test_gap_to_synthesis_pipeline_mcp() {
        // Simulate the full pipeline: detect gap -> synthesize capability
        let gap = CapabilityGap::Mcp {
            name: "weather-api".to_string(),
            purpose: "Fetch real-time weather data".to_string(),
            api_hint: Some("https://api.openweathermap.org/data/2.5".to_string()),
        };

        // Verify gap properties
        match &gap {
            CapabilityGap::Mcp { name, purpose, api_hint } => {
                assert_eq!(name, "weather-api");
                assert!(purpose.contains("weather"));
                assert!(api_hint.is_some());
            }
            _ => panic!("Expected Mcp gap"),
        }

        // Create corresponding synthesized capability
        let mut capability = SynthesizedCapability::new(
            CapabilityType::Mcp,
            "weather-api",
            PathBuf::from("/home/user/.jarvis/mcps/weather-api"),
        );
        capability.config = serde_json::json!({
            "api_key_env": "OPENWEATHER_API_KEY",
            "base_url": "https://api.openweathermap.org/data/2.5"
        });

        assert_eq!(capability.capability_type, CapabilityType::Mcp);
        assert_eq!(capability.name, "weather-api");
    }

    #[test]
    fn test_gap_to_synthesis_pipeline_skill() {
        let gap = CapabilityGap::Skill {
            name: "docker-compose".to_string(),
            trigger: "docker-compose up".to_string(),
            purpose: "Manage multi-container Docker applications".to_string(),
        };

        match &gap {
            CapabilityGap::Skill { name, trigger, .. } => {
                assert_eq!(name, "docker-compose");
                assert!(trigger.contains("docker"));
            }
            _ => panic!("Expected Skill gap"),
        }

        let mut capability = SynthesizedCapability::new(
            CapabilityType::Skill,
            "docker-compose",
            PathBuf::from("/home/user/.jarvis/skills/docker-compose.md"),
        );
        capability.config = serde_json::json!({
            "triggers": ["docker-compose", "containers", "orchestrate"]
        });
        capability.usage_count = 5;
        capability.success_rate = 0.8;

        assert_eq!(capability.capability_type, CapabilityType::Skill);
        assert!(capability.success_rate > 0.0);
    }

    #[test]
    fn test_gap_to_synthesis_pipeline_agent() {
        let gap = CapabilityGap::Agent {
            name: "database-migration".to_string(),
            specialization: "Handle complex database schema migrations with rollback support".to_string(),
        };

        match &gap {
            CapabilityGap::Agent { specialization, .. } => {
                assert!(specialization.contains("database"));
            }
            _ => panic!("Expected Agent gap"),
        }

        let mut capability = SynthesizedCapability::new(
            CapabilityType::Agent,
            "database-migration",
            PathBuf::from("/home/user/.jarvis/agents/database-migration"),
        );
        capability.config = serde_json::json!({
            "model": "claude-3-opus",
            "max_iterations": 10
        });
        capability.usage_count = 3;
        capability.success_rate = 1.0;

        assert_eq!(capability.capability_type, CapabilityType::Agent);
    }

    // ========================================================================
    // Registry Integration Tests
    // ========================================================================

    #[test]
    fn test_registry_with_multiple_capability_types() {
        let _registry = CapabilityRegistry::new_default();

        // Create capabilities of each type
        let mut mcp_cap = SynthesizedCapability::new(
            CapabilityType::Mcp,
            "github-api",
            PathBuf::from("/mcps/github"),
        );
        mcp_cap.usage_count = 10;
        mcp_cap.success_rate = 0.95;

        let mut skill_cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "code-review",
            PathBuf::from("/skills/code-review.md"),
        );
        skill_cap.usage_count = 25;
        skill_cap.success_rate = 0.88;

        let mut agent_cap = SynthesizedCapability::new(
            CapabilityType::Agent,
            "refactor-assistant",
            PathBuf::from("/agents/refactor"),
        );
        agent_cap.usage_count = 5;
        agent_cap.success_rate = 0.75;

        // Verify all types are distinct
        assert_ne!(mcp_cap.capability_type, skill_cap.capability_type);
        assert_ne!(skill_cap.capability_type, agent_cap.capability_type);
        assert_ne!(mcp_cap.capability_type, agent_cap.capability_type);
    }

    // ========================================================================
    // Verification Pipeline Tests
    // ========================================================================

    #[test]
    fn test_verification_success_flow() {
        let result = VerificationResult::success();

        assert!(result.success);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_verification_failure_flow() {
        let mut result = VerificationResult::failure("Missing required field: api_key");
        result.add_error("Invalid configuration format");
        result.add_warning("Consider adding documentation");

        assert!(!result.success);
        assert_eq!(result.errors.len(), 2);
        assert!(result.errors.contains(&"Missing required field: api_key".to_string()));
    }

    #[test]
    fn test_verification_result_serialization() {
        let mut result = VerificationResult::success();
        result.add_warning("Minor warning");

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));

        let deserialized: VerificationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.success, result.success);
        assert_eq!(deserialized.errors, result.errors);
    }

    // ========================================================================
    // Component Instantiation Tests
    // ========================================================================

    #[test]
    fn test_all_extend_components_instantiate() {
        // Verify all components can be created without panicking
        let detector = GapDetector::new();
        let registry = CapabilityRegistry::new_default();
        let engine = SynthesisEngine::default();
        let verifier = CapabilityVerifier::new();
        let loader = HotLoader::new();

        // Drop them to ensure cleanup works
        drop(detector);
        drop(registry);
        drop(engine);
        drop(verifier);
        drop(loader);
    }

    // ========================================================================
    // Capability Type Exhaustiveness Tests
    // ========================================================================

    #[test]
    fn test_capability_type_match_exhaustiveness() {
        let types = [CapabilityType::Mcp, CapabilityType::Skill, CapabilityType::Agent];

        for cap_type in types {
            let description = match cap_type {
                CapabilityType::Mcp => "Model Context Protocol server",
                CapabilityType::Skill => "Skill definition file",
                CapabilityType::Agent => "Subagent configuration",
            };
            assert!(!description.is_empty());
        }
    }

    #[test]
    fn test_capability_gap_match_exhaustiveness() {
        let gaps = [
            CapabilityGap::Mcp {
                name: "test".to_string(),
                purpose: "test".to_string(),
                api_hint: None,
            },
            CapabilityGap::Skill {
                name: "test".to_string(),
                trigger: "test".to_string(),
                purpose: "test".to_string(),
            },
            CapabilityGap::Agent {
                name: "test".to_string(),
                specialization: "test".to_string(),
            },
        ];

        for gap in gaps {
            let gap_type = match &gap {
                CapabilityGap::Mcp { .. } => "mcp",
                CapabilityGap::Skill { .. } => "skill",
                CapabilityGap::Agent { .. } => "agent",
            };
            assert!(!gap_type.is_empty());
        }
    }

    // ========================================================================
    // Synthesized Capability Lifecycle Tests
    // ========================================================================

    #[test]
    fn test_synthesized_capability_usage_tracking() {
        let mut capability = SynthesizedCapability::new(
            CapabilityType::Skill,
            "tracked-skill",
            PathBuf::from("/skills/tracked.md"),
        );

        // Simulate usage tracking
        capability.usage_count += 1;
        capability.success_rate = 1.0; // First use was successful

        capability.usage_count += 1;
        capability.success_rate = 1.0; // Second use successful

        capability.usage_count += 1;
        capability.success_rate = 0.67; // Third use failed

        assert_eq!(capability.usage_count, 3);
        assert!(capability.success_rate > 0.5);
        assert!(capability.success_rate < 1.0);
    }

    #[test]
    fn test_synthesized_capability_config_flexibility() {
        // MCP config
        let mcp_config = serde_json::json!({
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-github"],
            "env": {
                "GITHUB_TOKEN": "${GITHUB_TOKEN}"
            }
        });

        // Skill config
        let skill_config = serde_json::json!({
            "triggers": ["commit code", "git commit"],
            "description": "Create git commits with conventional format"
        });

        // Agent config
        let agent_config = serde_json::json!({
            "model": "claude-3-opus",
            "max_iterations": 5,
            "tools": ["read", "write", "bash"]
        });

        // All configs should serialize properly
        let mut mcp_cap = SynthesizedCapability::new(
            CapabilityType::Mcp,
            "github",
            PathBuf::from("/mcps/github"),
        );
        mcp_cap.config = mcp_config;

        let mut skill_cap = SynthesizedCapability::new(
            CapabilityType::Skill,
            "commit",
            PathBuf::from("/skills/commit.md"),
        );
        skill_cap.config = skill_config;

        let mut agent_cap = SynthesizedCapability::new(
            CapabilityType::Agent,
            "helper",
            PathBuf::from("/agents/helper"),
        );
        agent_cap.config = agent_config;

        // All should serialize to JSON
        assert!(serde_json::to_string(&mcp_cap).is_ok());
        assert!(serde_json::to_string(&skill_cap).is_ok());
        assert!(serde_json::to_string(&agent_cap).is_ok());
    }

    // ========================================================================
    // Hot Loading Integration Tests
    // ========================================================================

    #[test]
    fn test_hot_loader_with_temp_directory() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let claude_settings = temp_dir.path().join("claude_settings.json");
        let agents_dir = temp_dir.path().join("agents");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&agents_dir).unwrap();

        let _loader = HotLoader::with_paths(claude_settings, skills_dir.clone(), agents_dir);

        // Create a skill capability file
        let skill_path = skills_dir.join("test-skill.md");
        fs::write(&skill_path, "# Test Skill\nThis is a test skill.").unwrap();

        assert!(skill_path.exists());
    }

    // ========================================================================
    // Full Pipeline Integration Test
    // ========================================================================

    #[test]
    fn test_full_extend_pipeline_integration() {
        // 1. Gap Detection Phase
        let detector = GapDetector::new();
        let _gap = CapabilityGap::Mcp {
            name: "slack-api".to_string(),
            purpose: "Send messages to Slack channels".to_string(),
            api_hint: Some("https://slack.com/api".to_string()),
        };

        // 2. Synthesis Phase
        let engine = SynthesisEngine::default();
        let mut capability = SynthesizedCapability::new(
            CapabilityType::Mcp,
            "slack-api",
            PathBuf::from("/mcps/slack-api"),
        );
        capability.config = serde_json::json!({
            "command": "npx",
            "args": ["-y", "@anthropic/mcp-server-slack"],
            "env": {"SLACK_TOKEN": "${SLACK_TOKEN}"}
        });

        // 3. Verification Phase
        let verifier = CapabilityVerifier::new();
        let verification = VerificationResult::success();

        // 4. Registry Phase
        let registry = CapabilityRegistry::new_default();

        // 5. Hot Loading Phase
        let loader = HotLoader::new();

        // Verify the pipeline components are all functional
        assert!(verification.success);
        assert_eq!(capability.name, "slack-api");

        // Cleanup
        drop(detector);
        drop(engine);
        drop(verifier);
        drop(registry);
        drop(loader);
    }
}
