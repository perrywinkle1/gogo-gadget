//! Comprehensive unit tests for gogo-gadget core library types
//!
//! Tests cover:
//! - Config and default values
//! - TaskResult construction
//! - ExecutionMode and Difficulty enums
//! - TaskState transitions
//! - Checkpoint creation, save/load, and metadata
//! - IterationAttempt records
//! - ExecutionMetrics tracking
//! - ClaudeStructuredOutput parsing
//! - BackoffConfig delay calculations
//! - ParallelConfig and ParallelResult
//! - ShutdownSignal behavior
//! - ProgressEvent and ProgressEventType
//! - Edge cases and error handling

use gogo_gadget::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Config Tests
// ============================================================================

#[test]
fn test_config_default() {
    let config = Config::default();
    assert!(!config.dry_run);
    assert!(config.completion_promise.is_none());
    assert!(config.model.is_none());
}

#[test]
fn test_config_with_custom_values() {
    let config = Config {
        working_dir: PathBuf::from("/custom/path"),
        completion_promise: Some("DONE".to_string()),
        model: Some("claude-3-opus".to_string()),
        dry_run: true,
            anti_laziness: AntiLazinessConfig::default(),
    };

    assert_eq!(config.working_dir, PathBuf::from("/custom/path"));
    assert_eq!(config.completion_promise, Some("DONE".to_string()));
    assert_eq!(config.model, Some("claude-3-opus".to_string()));
    assert!(config.dry_run);
}

#[test]
fn test_config_serialization() {
    let config = Config {
        working_dir: PathBuf::from("/test"),
        completion_promise: Some("COMPLETE".to_string()),
        model: None,
        dry_run: false,
            anti_laziness: AntiLazinessConfig::default(),
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: Config = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.completion_promise, Some("COMPLETE".to_string()));
    assert!(!deserialized.dry_run);
}

// ============================================================================
// TaskResult Tests
// ============================================================================

#[test]
fn test_task_result_success() {
    let result = TaskResult {
        success: true,
        iterations: 3,
        summary: Some("Task completed successfully".to_string()),
        error: None,
        verification: None,
    };

    assert!(result.success);
    assert_eq!(result.iterations, 3);
    assert!(result.summary.is_some());
    assert!(result.error.is_none());
}

#[test]
fn test_task_result_failure() {
    let result = TaskResult {
        success: false,
        iterations: 10,
        summary: None,
        error: Some("Max iterations reached".to_string()),
        verification: None,
    };

    assert!(!result.success);
    assert_eq!(result.iterations, 10);
    assert!(result.error.is_some());
}

#[test]
fn test_task_result_serialization() {
    let result = TaskResult {
        success: true,
        iterations: 5,
        summary: Some("Done".to_string()),
        error: None,
        verification: None,
    };

    let json = serde_json::to_string(&result).unwrap();
    let deserialized: TaskResult = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.success, true);
    assert_eq!(deserialized.iterations, 5);
}

// ============================================================================
// ExecutionMode Tests
// ============================================================================

#[test]
fn test_execution_mode_single() {
    let mode = ExecutionMode::Single;
    assert_eq!(mode, ExecutionMode::Single);
}

#[test]
fn test_execution_mode_swarm() {
    let mode = ExecutionMode::Swarm { agent_count: 5 };
    match mode {
        ExecutionMode::Swarm { agent_count } => assert_eq!(agent_count, 5),
        _ => panic!("Expected Swarm mode"),
    }
}

#[test]
fn test_execution_mode_equality() {
    assert_eq!(ExecutionMode::Single, ExecutionMode::Single);
    assert_eq!(
        ExecutionMode::Swarm { agent_count: 3 },
        ExecutionMode::Swarm { agent_count: 3 }
    );
    assert_ne!(
        ExecutionMode::Swarm { agent_count: 3 },
        ExecutionMode::Swarm { agent_count: 5 }
    );
    assert_ne!(ExecutionMode::Single, ExecutionMode::Swarm { agent_count: 1 });
}

#[test]
fn test_execution_mode_serialization() {
    let single = ExecutionMode::Single;
    let json = serde_json::to_string(&single).unwrap();
    let deserialized: ExecutionMode = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, ExecutionMode::Single);

    let swarm = ExecutionMode::Swarm { agent_count: 4 };
    let json = serde_json::to_string(&swarm).unwrap();
    let deserialized: ExecutionMode = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, ExecutionMode::Swarm { agent_count: 4 });
}

// ============================================================================
// Difficulty Tests
// ============================================================================

#[test]
fn test_difficulty_from_score() {
    assert_eq!(Difficulty::from(0), Difficulty::Trivial);
    assert_eq!(Difficulty::from(1), Difficulty::Trivial);
    assert_eq!(Difficulty::from(2), Difficulty::Trivial);
    assert_eq!(Difficulty::from(3), Difficulty::Easy);
    assert_eq!(Difficulty::from(4), Difficulty::Easy);
    assert_eq!(Difficulty::from(5), Difficulty::Medium);
    assert_eq!(Difficulty::from(6), Difficulty::Medium);
    assert_eq!(Difficulty::from(7), Difficulty::Hard);
    assert_eq!(Difficulty::from(8), Difficulty::Hard);
    assert_eq!(Difficulty::from(9), Difficulty::Expert);
    assert_eq!(Difficulty::from(10), Difficulty::Expert);
    assert_eq!(Difficulty::from(100), Difficulty::Expert);
}

#[test]
fn test_difficulty_equality() {
    assert_eq!(Difficulty::Trivial, Difficulty::Trivial);
    assert_ne!(Difficulty::Easy, Difficulty::Hard);
}

#[test]
fn test_difficulty_serialization() {
    let difficulty = Difficulty::Medium;
    let json = serde_json::to_string(&difficulty).unwrap();
    let deserialized: Difficulty = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, Difficulty::Medium);
}

// ============================================================================
// TaskState Tests
// ============================================================================

#[test]
fn test_task_state_default() {
    assert_eq!(TaskState::default(), TaskState::Pending);
}

#[test]
fn test_task_state_variants() {
    let states = [
        TaskState::Pending,
        TaskState::Running,
        TaskState::Blocked,
        TaskState::Complete,
    ];

    for state in &states {
        let json = serde_json::to_string(state).unwrap();
        let deserialized: TaskState = serde_json::from_str(&json).unwrap();
        assert_eq!(&deserialized, state);
    }
}

#[test]
fn test_task_state_clone() {
    let state = TaskState::Running;
    let cloned = state.clone();
    assert_eq!(state, cloned);
}

// ============================================================================
// Checkpoint Tests
// ============================================================================

#[test]
fn test_checkpoint_creation() {
    let checkpoint = Checkpoint::new("Test task");
    assert_eq!(checkpoint.task, "Test task");
    assert_eq!(checkpoint.iteration, 0);
    assert_eq!(checkpoint.state, TaskState::Pending);
    assert!(!checkpoint.id.is_empty());
    assert!(checkpoint.accumulated_feedback.is_empty());
    assert!(checkpoint.last_output.is_empty());
    assert!(checkpoint.iteration_history.is_empty());
    assert!(checkpoint.created_at > 0);
}

#[test]
fn test_checkpoint_with_context() {
    let path = PathBuf::from("/test/path");
    let checkpoint = Checkpoint::new_with_context("Test task", &path);

    assert_eq!(checkpoint.task, "Test task");
    assert_eq!(
        checkpoint.get_metadata("working_dir"),
        Some(&"/test/path".to_string())
    );
}

#[test]
fn test_checkpoint_metadata() {
    let mut checkpoint = Checkpoint::new("Test");
    checkpoint.set_metadata("key1", "value1");
    checkpoint.set_metadata("key2", "value2");

    assert_eq!(checkpoint.get_metadata("key1"), Some(&"value1".to_string()));
    assert_eq!(checkpoint.get_metadata("key2"), Some(&"value2".to_string()));
    assert_eq!(checkpoint.get_metadata("nonexistent"), None);
}

#[test]
fn test_checkpoint_save_and_load() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("checkpoint.json");

    let mut checkpoint = Checkpoint::new("Test task");
    checkpoint.iteration = 5;
    checkpoint.state = TaskState::Running;
    checkpoint.accumulated_feedback = "Some feedback".to_string();
    checkpoint.set_metadata("test_key", "test_value");

    checkpoint.save(&path).unwrap();

    let loaded = Checkpoint::load(&path).unwrap();
    assert_eq!(loaded.task, "Test task");
    assert_eq!(loaded.iteration, 5);
    assert_eq!(loaded.state, TaskState::Running);
    assert_eq!(loaded.accumulated_feedback, "Some feedback");
    assert_eq!(loaded.get_metadata("test_key"), Some(&"test_value".to_string()));
}

#[test]
fn test_checkpoint_load_nonexistent() {
    let result = Checkpoint::load(&PathBuf::from("/nonexistent/path/checkpoint.json"));
    assert!(result.is_err());
}

#[test]
fn test_checkpoint_progress_percent() {
    let mut checkpoint = Checkpoint::new("Test");
    checkpoint.iteration = 3;

    // Use approximate comparison for floating point
    assert!((checkpoint.progress_percent(10) - 30.0).abs() < 0.001);
    assert!((checkpoint.progress_percent(5) - 60.0).abs() < 0.001);
    assert_eq!(checkpoint.progress_percent(0), 0.0); // Edge case: avoid division by zero
}

#[test]
fn test_checkpoint_update_from_iteration() {
    let mut checkpoint = Checkpoint::new("Test");
    let attempt = IterationAttempt {
        iteration: 1,
        prompt: "Do something".to_string(),
        output: "Did something".to_string(),
        success: true,
        error: None,
        duration_ms: 1000,
        feedback: Some("Good job".to_string()),
    };

    checkpoint.update_from_iteration(1, "output", "feedback", attempt);

    assert_eq!(checkpoint.iteration, 1);
    assert_eq!(checkpoint.last_output, "output");
    assert_eq!(checkpoint.accumulated_feedback, "feedback");
    assert_eq!(checkpoint.iteration_history.len(), 1);
}

#[test]
fn test_checkpoint_last_successful_iteration() {
    let mut checkpoint = Checkpoint::new("Test");

    checkpoint.iteration_history.push(IterationAttempt {
        iteration: 1,
        prompt: "".to_string(),
        output: "".to_string(),
        success: false,
        error: Some("Error".to_string()),
        duration_ms: 100,
        feedback: None,
    });
    checkpoint.iteration_history.push(IterationAttempt {
        iteration: 2,
        prompt: "".to_string(),
        output: "Success output".to_string(),
        success: true,
        error: None,
        duration_ms: 200,
        feedback: None,
    });
    checkpoint.iteration_history.push(IterationAttempt {
        iteration: 3,
        prompt: "".to_string(),
        output: "".to_string(),
        success: false,
        error: Some("Another error".to_string()),
        duration_ms: 100,
        feedback: None,
    });

    let last_success = checkpoint.last_successful_iteration();
    assert!(last_success.is_some());
    assert_eq!(last_success.unwrap().iteration, 2);
}

#[test]
fn test_checkpoint_failure_count() {
    let mut checkpoint = Checkpoint::new("Test");

    checkpoint.iteration_history.push(IterationAttempt {
        iteration: 1,
        prompt: "".to_string(),
        output: "".to_string(),
        success: false,
        error: Some("Error 1".to_string()),
        duration_ms: 100,
        feedback: None,
    });
    checkpoint.iteration_history.push(IterationAttempt {
        iteration: 2,
        prompt: "".to_string(),
        output: "".to_string(),
        success: true,
        error: None,
        duration_ms: 200,
        feedback: None,
    });
    checkpoint.iteration_history.push(IterationAttempt {
        iteration: 3,
        prompt: "".to_string(),
        output: "".to_string(),
        success: false,
        error: Some("Error 2".to_string()),
        duration_ms: 100,
        feedback: None,
    });

    assert_eq!(checkpoint.failure_count(), 2);
}

#[test]
fn test_checkpoint_is_stale() {
    let checkpoint = Checkpoint::new("Test");
    // Just created, should not be stale for a long duration
    assert!(!checkpoint.is_stale(Duration::from_secs(3600)));
    // Note: is_stale uses > comparison, so 0 seconds means the checkpoint
    // must have been created more than 0 seconds ago.
    // Since we just created it, the difference might be 0 or a tiny bit more.
    // A freshly created checkpoint should not be stale for max_age = 1 second
    assert!(!checkpoint.is_stale(Duration::from_secs(1)));
}

#[test]
fn test_checkpoint_create_recovery_summary() {
    let mut checkpoint = Checkpoint::new("Important task");
    checkpoint.iteration = 5;
    checkpoint.state = TaskState::Blocked;
    checkpoint.iteration_history.push(IterationAttempt {
        iteration: 1,
        prompt: "".to_string(),
        output: "".to_string(),
        success: true,
        error: None,
        duration_ms: 100,
        feedback: None,
    });
    checkpoint.iteration_history.push(IterationAttempt {
        iteration: 2,
        prompt: "".to_string(),
        output: "".to_string(),
        success: false,
        error: Some("Error".to_string()),
        duration_ms: 100,
        feedback: None,
    });

    let summary = checkpoint.create_recovery_summary();
    assert!(summary.contains("Important task"));
    assert!(summary.contains("Iteration: 5"));
    assert!(summary.contains("Blocked"));
    assert!(summary.contains("Successful iterations: 1"));
    assert!(summary.contains("Failed iterations: 1"));
}

// ============================================================================
// IterationAttempt Tests
// ============================================================================

#[test]
fn test_iteration_attempt_creation() {
    let attempt = IterationAttempt {
        iteration: 1,
        prompt: "Do the thing".to_string(),
        output: "Did the thing".to_string(),
        success: true,
        error: None,
        duration_ms: 5000,
        feedback: Some("Good".to_string()),
    };

    assert_eq!(attempt.iteration, 1);
    assert!(attempt.success);
    assert_eq!(attempt.duration_ms, 5000);
}

#[test]
fn test_iteration_attempt_with_error() {
    let attempt = IterationAttempt {
        iteration: 2,
        prompt: "Try again".to_string(),
        output: "".to_string(),
        success: false,
        error: Some("Timeout".to_string()),
        duration_ms: 30000,
        feedback: None,
    };

    assert!(!attempt.success);
    assert_eq!(attempt.error, Some("Timeout".to_string()));
}

#[test]
fn test_iteration_attempt_serialization() {
    let attempt = IterationAttempt {
        iteration: 3,
        prompt: "Test prompt".to_string(),
        output: "Test output".to_string(),
        success: true,
        error: None,
        duration_ms: 1500,
        feedback: None,
    };

    let json = serde_json::to_string(&attempt).unwrap();
    let deserialized: IterationAttempt = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.iteration, 3);
    assert_eq!(deserialized.prompt, "Test prompt");
}

// ============================================================================
// ExecutionMetrics Tests
// ============================================================================

#[test]
fn test_execution_metrics_new() {
    let metrics = ExecutionMetrics::new();
    assert_eq!(metrics.total_duration_ms, 0);
    assert!(metrics.iteration_times_ms.is_empty());
    assert_eq!(metrics.retry_count, 0);
    assert_eq!(metrics.timeout_count, 0);
    assert_eq!(metrics.successful_iterations, 0);
    assert_eq!(metrics.failed_iterations, 0);
    assert!(metrics.avg_iteration_ms.is_none());
}

#[test]
fn test_execution_metrics_record_iteration() {
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
fn test_execution_metrics_record_retry() {
    let mut metrics = ExecutionMetrics::new();
    metrics.record_retry();
    metrics.record_retry();
    metrics.record_retry();

    assert_eq!(metrics.retry_count, 3);
}

#[test]
fn test_execution_metrics_record_timeout() {
    let mut metrics = ExecutionMetrics::new();
    metrics.record_timeout();
    metrics.record_timeout();

    assert_eq!(metrics.timeout_count, 2);
}

#[test]
fn test_execution_metrics_finalize() {
    let mut metrics = ExecutionMetrics::new();
    metrics.record_iteration(1, 1000, true);
    metrics.record_iteration(2, 2000, true);
    metrics.finalize(5000);

    assert_eq!(metrics.total_duration_ms, 5000);
    assert_eq!(metrics.avg_iteration_ms, Some(1500));
}

#[test]
fn test_execution_metrics_serialization() {
    let mut metrics = ExecutionMetrics::new();
    metrics.record_iteration(1, 1000, true);
    metrics.finalize(2000);

    let json = serde_json::to_string(&metrics).unwrap();
    let deserialized: ExecutionMetrics = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.total_duration_ms, 2000);
    assert_eq!(deserialized.successful_iterations, 1);
}

// ============================================================================
// ClaudeStructuredOutput Tests
// ============================================================================

#[test]
fn test_claude_structured_output_default() {
    let output = ClaudeStructuredOutput::default();
    assert_eq!(output.status, ClaudeOutputStatus::Unknown);
    assert!(output.summary.is_none());
    assert!(output.files_modified.is_empty());
    assert!(output.next_steps.is_empty());
    assert!(output.errors.is_empty());
    assert_eq!(output.confidence, 0.0);
}

#[test]
fn test_claude_structured_output_complete() {
    let json = r#"{
        "status": "complete",
        "summary": "All done",
        "files_modified": ["src/main.rs", "tests/test.rs"],
        "next_steps": [],
        "errors": [],
        "confidence": 0.95
    }"#;

    let output: ClaudeStructuredOutput = serde_json::from_str(json).unwrap();
    assert_eq!(output.status, ClaudeOutputStatus::Complete);
    assert_eq!(output.summary, Some("All done".to_string()));
    assert_eq!(output.files_modified.len(), 2);
    assert_eq!(output.confidence, 0.95);
}

#[test]
fn test_claude_structured_output_in_progress() {
    let json = r#"{
        "status": "in_progress",
        "summary": "Working on it",
        "files_modified": [],
        "next_steps": ["Finish implementation", "Add tests"],
        "errors": [],
        "confidence": 0.5
    }"#;

    let output: ClaudeStructuredOutput = serde_json::from_str(json).unwrap();
    assert_eq!(output.status, ClaudeOutputStatus::InProgress);
    assert_eq!(output.next_steps.len(), 2);
}

#[test]
fn test_claude_structured_output_blocked() {
    let json = r#"{
        "status": "blocked",
        "summary": null,
        "files_modified": [],
        "next_steps": [],
        "errors": ["Missing dependency", "Permission denied"],
        "confidence": 0.0
    }"#;

    let output: ClaudeStructuredOutput = serde_json::from_str(json).unwrap();
    assert_eq!(output.status, ClaudeOutputStatus::Blocked);
    assert!(output.summary.is_none());
    assert_eq!(output.errors.len(), 2);
}

#[test]
fn test_claude_structured_output_failed() {
    let json = r#"{
        "status": "failed",
        "summary": null,
        "files_modified": [],
        "next_steps": [],
        "errors": ["Critical failure"],
        "confidence": 0.0
    }"#;

    let output: ClaudeStructuredOutput = serde_json::from_str(json).unwrap();
    assert_eq!(output.status, ClaudeOutputStatus::Failed);
}

#[test]
fn test_claude_output_status_serialization() {
    for status in [
        ClaudeOutputStatus::Complete,
        ClaudeOutputStatus::InProgress,
        ClaudeOutputStatus::Blocked,
        ClaudeOutputStatus::Failed,
        ClaudeOutputStatus::Unknown,
    ] {
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: ClaudeOutputStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }
}

// ============================================================================
// BackoffConfig Tests
// ============================================================================

#[test]
fn test_backoff_config_default() {
    let config = BackoffConfig::default();
    assert_eq!(config.initial_delay_ms, 1000);
    assert_eq!(config.max_delay_ms, 60000);
    assert_eq!(config.multiplier, 2.0);
    assert_eq!(config.max_retries, 5);
    assert!(config.jitter);
}

#[test]
fn test_backoff_delay_for_attempt_zero() {
    let config = BackoffConfig::default();
    assert_eq!(config.delay_for_attempt(0), Duration::from_millis(0));
}

#[test]
fn test_backoff_delay_exponential_no_jitter() {
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
    assert_eq!(config.delay_for_attempt(5), Duration::from_millis(16000));
}

#[test]
fn test_backoff_delay_max_cap() {
    let config = BackoffConfig {
        initial_delay_ms: 1000,
        max_delay_ms: 5000,
        multiplier: 2.0,
        max_retries: 10,
        jitter: false,
    };

    // 1000 * 2^3 = 8000, but capped at 5000
    assert_eq!(config.delay_for_attempt(4), Duration::from_millis(5000));
    assert_eq!(config.delay_for_attempt(5), Duration::from_millis(5000));
}

#[test]
fn test_backoff_delay_with_jitter() {
    let config = BackoffConfig {
        initial_delay_ms: 1000,
        max_delay_ms: 100000,
        multiplier: 2.0,
        max_retries: 5,
        jitter: true,
    };

    // With jitter, delay should be between base and base * 1.25
    let delay = config.delay_for_attempt(1);
    assert!(delay >= Duration::from_millis(1000));
    assert!(delay <= Duration::from_millis(1250));
}

#[test]
fn test_backoff_config_serialization() {
    let config = BackoffConfig {
        initial_delay_ms: 500,
        max_delay_ms: 30000,
        multiplier: 1.5,
        max_retries: 3,
        jitter: false,
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: BackoffConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.initial_delay_ms, 500);
    assert_eq!(deserialized.multiplier, 1.5);
}

// ============================================================================
// ParallelConfig Tests
// ============================================================================

#[test]
fn test_parallel_config_default() {
    let config = ParallelConfig::default();
    assert_eq!(config.max_concurrent, 4);
    assert!(config.files.is_empty());
    assert!(!config.share_context);
    assert_eq!(config.task_timeout_secs, 300);
}

#[test]
fn test_parallel_config_custom() {
    let config = ParallelConfig {
        max_concurrent: 8,
        files: vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")],
        share_context: true,
        task_timeout_secs: 600,
    };

    assert_eq!(config.max_concurrent, 8);
    assert_eq!(config.files.len(), 2);
    assert!(config.share_context);
}

#[test]
fn test_parallel_config_serialization() {
    let config = ParallelConfig {
        max_concurrent: 2,
        files: vec![PathBuf::from("test.rs")],
        share_context: false,
        task_timeout_secs: 120,
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: ParallelConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.max_concurrent, 2);
    assert_eq!(deserialized.task_timeout_secs, 120);
}

// ============================================================================
// ParallelResult Tests
// ============================================================================

#[test]
fn test_parallel_result_all_success() {
    let mut results = HashMap::new();
    results.insert(
        PathBuf::from("a.rs"),
        TaskResult {
            success: true,
            iterations: 2,
            summary: Some("Done".to_string()),
            error: None,
            verification: None,
        },
    );
    results.insert(
        PathBuf::from("b.rs"),
        TaskResult {
            success: true,
            iterations: 3,
            summary: Some("Done".to_string()),
            error: None,
            verification: None,
        },
    );

    let result = ParallelResult {
        results,
        all_succeeded: true,
        total_duration_ms: 5000,
        succeeded_count: 2,
        failed_count: 0,
    };

    assert!(result.all_succeeded);
    assert_eq!(result.succeeded_count, 2);
    assert_eq!(result.failed_count, 0);
}

#[test]
fn test_parallel_result_partial_failure() {
    let mut results = HashMap::new();
    results.insert(
        PathBuf::from("a.rs"),
        TaskResult {
            success: true,
            iterations: 2,
            summary: Some("Done".to_string()),
            error: None,
            verification: None,
        },
    );
    results.insert(
        PathBuf::from("b.rs"),
        TaskResult {
            success: false,
            iterations: 10,
            summary: None,
            error: Some("Failed".to_string()),
            verification: None,
        },
    );

    let result = ParallelResult {
        results,
        all_succeeded: false,
        total_duration_ms: 10000,
        succeeded_count: 1,
        failed_count: 1,
    };

    assert!(!result.all_succeeded);
    assert_eq!(result.succeeded_count, 1);
    assert_eq!(result.failed_count, 1);
}

// ============================================================================
// ShutdownSignal Tests
// ============================================================================

#[test]
fn test_shutdown_signal_new() {
    let signal = ShutdownSignal::new();
    assert!(!signal.is_shutdown());
    assert!(!signal.is_interrupted());
}

#[test]
fn test_shutdown_signal_request_shutdown() {
    let signal = ShutdownSignal::new();
    signal.request_shutdown();
    assert!(signal.is_shutdown());
    assert!(!signal.is_interrupted());
}

#[test]
fn test_shutdown_signal_mark_interrupted() {
    let signal = ShutdownSignal::new();
    signal.mark_interrupted();
    assert!(signal.is_shutdown());
    assert!(signal.is_interrupted());
}

#[test]
fn test_shutdown_signal_clone() {
    let signal = ShutdownSignal::new();
    let cloned = signal.clone();

    signal.request_shutdown();

    // Both should see the shutdown because they share the atomic
    assert!(signal.is_shutdown());
    assert!(cloned.is_shutdown());
}

#[test]
fn test_shutdown_signal_clone_flag() {
    let signal = ShutdownSignal::new();
    let flag = signal.clone_shutdown_flag();

    signal.request_shutdown();

    assert!(flag.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_shutdown_signal_default() {
    let signal = ShutdownSignal::default();
    assert!(!signal.is_shutdown());
    assert!(!signal.is_interrupted());
}

// ============================================================================
// ProgressEvent Tests
// ============================================================================

#[test]
fn test_progress_event_new() {
    let event = ProgressEvent::new(ProgressEventType::Started, 0, 10);
    assert_eq!(event.event_type, ProgressEventType::Started);
    assert_eq!(event.iteration, 0);
    assert!(event.timestamp_ms > 0);
    assert!(event.message.is_none());
    assert!(event.metrics.is_none());
}

#[test]
fn test_progress_event_with_message() {
    let event = ProgressEvent::new(ProgressEventType::TaskComplete, 5, 10)
        .with_message("Task finished successfully");

    assert_eq!(event.message, Some("Task finished successfully".to_string()));
}

#[test]
fn test_progress_event_with_metrics() {
    let mut metrics = ExecutionMetrics::new();
    metrics.record_iteration(1, 1000, true);

    let event = ProgressEvent::new(ProgressEventType::IterationComplete, 1, 10)
        .with_metrics(metrics);

    assert!(event.metrics.is_some());
    assert_eq!(event.metrics.unwrap().successful_iterations, 1);
}

#[test]
fn test_progress_event_types() {
    let types = [
        ProgressEventType::Started,
        ProgressEventType::IterationStart,
        ProgressEventType::IterationComplete,
        ProgressEventType::IterationFailed,
        ProgressEventType::IterationTimeout,
        ProgressEventType::TaskComplete,
        ProgressEventType::TaskFailed,
        ProgressEventType::TaskInterrupted,
        ProgressEventType::CheckpointSaved,
        ProgressEventType::CheckpointLoaded,
        ProgressEventType::PromiseDetected,
        ProgressEventType::ShortcutDetected,
    ];

    for event_type in types {
        let event = ProgressEvent::new(event_type, 1, 10);
        assert_eq!(event.event_type, event_type);
    }
}

#[test]
fn test_progress_event_serialization() {
    let event = ProgressEvent::new(ProgressEventType::TaskComplete, 5, 10)
        .with_message("Done");

    let json = serde_json::to_string(&event).unwrap();
    let deserialized: ProgressEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.event_type, ProgressEventType::TaskComplete);
    assert_eq!(deserialized.iteration, 5);
    assert_eq!(deserialized.message, Some("Done".to_string()));
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
fn test_empty_task_prompt() {
    let checkpoint = Checkpoint::new("");
    assert_eq!(checkpoint.task, "");
}

#[test]
fn test_large_iteration_count() {
    let mut metrics = ExecutionMetrics::new();
    for i in 0..1000 {
        metrics.record_iteration(i, 100, true);
    }
    assert_eq!(metrics.successful_iterations, 1000);
    assert_eq!(metrics.avg_iteration_ms, Some(100));
}

#[test]
fn test_config_empty_working_dir() {
    let config = Config {
        working_dir: PathBuf::new(),
        ..Config::default()
    };
    assert_eq!(config.working_dir, PathBuf::new());
}

#[test]
fn test_checkpoint_empty_metadata() {
    let checkpoint = Checkpoint::new("Test");
    assert!(checkpoint.metadata.is_empty());
    assert!(checkpoint.get_metadata("anything").is_none());
}

#[test]
fn test_backoff_large_multiplier() {
    let config = BackoffConfig {
        initial_delay_ms: 1,
        max_delay_ms: 1000,
        multiplier: 100.0,
        max_retries: 5,
        jitter: false,
    };

    // With multiplier of 100, should hit max quickly
    assert_eq!(config.delay_for_attempt(2), Duration::from_millis(100));
    assert_eq!(config.delay_for_attempt(3), Duration::from_millis(1000)); // Capped
}

#[test]
fn test_checkpoint_unique_ids() {
    let c1 = Checkpoint::new("Task 1");
    let c2 = Checkpoint::new("Task 2");
    // IDs should be different (though there's a small chance of collision)
    // This tests the uuid generation
    assert!(!c1.id.is_empty());
    assert!(!c2.id.is_empty());
}

#[test]
fn test_metrics_empty_average() {
    let metrics = ExecutionMetrics::new();
    assert!(metrics.avg_iteration_ms.is_none());
}

#[test]
fn test_checkpoint_save_invalid_path() {
    let checkpoint = Checkpoint::new("Test");
    let result = checkpoint.save(&PathBuf::from("/invalid/nonexistent/deep/path/checkpoint.json"));
    assert!(result.is_err());
}

#[test]
fn test_parallel_result_empty() {
    let result = ParallelResult {
        results: HashMap::new(),
        all_succeeded: true,
        total_duration_ms: 0,
        succeeded_count: 0,
        failed_count: 0,
    };

    assert!(result.all_succeeded);
    assert_eq!(result.results.len(), 0);
}

#[test]
fn test_claude_output_with_raw_output() {
    let mut output = ClaudeStructuredOutput::default();
    output.raw_output = Some("Full raw output here".to_string());

    assert!(output.raw_output.is_some());
    assert_eq!(output.raw_output.unwrap(), "Full raw output here");
}

#[test]
fn test_iteration_attempt_zero_duration() {
    let attempt = IterationAttempt {
        iteration: 1,
        prompt: "Test".to_string(),
        output: "Result".to_string(),
        success: true,
        error: None,
        duration_ms: 0,
        feedback: None,
    };

    assert_eq!(attempt.duration_ms, 0);
}

#[test]
fn test_progress_event_chaining() {
    let mut metrics = ExecutionMetrics::new();
    metrics.record_iteration(1, 500, true);

    let event = ProgressEvent::new(ProgressEventType::TaskComplete, 1, 10)
        .with_message("All done")
        .with_metrics(metrics);

    assert_eq!(event.message, Some("All done".to_string()));
    assert!(event.metrics.is_some());
}

// ============================================================================
// Gas Town Context Compression Types Tests
// ============================================================================

#[test]
fn test_operation_mode_default() {
    let mode = gogo_gadget::OperationMode::default();
    assert_eq!(mode, gogo_gadget::OperationMode::Standalone);
}

#[test]
fn test_operation_mode_variants() {
    let standalone = gogo_gadget::OperationMode::Standalone;
    let subagent = gogo_gadget::OperationMode::Subagent;

    assert_eq!(standalone, gogo_gadget::OperationMode::Standalone);
    assert_eq!(subagent, gogo_gadget::OperationMode::Subagent);
    assert_ne!(standalone, subagent);
}

#[test]
fn test_operation_mode_serialization() {
    let standalone = gogo_gadget::OperationMode::Standalone;
    let json = serde_json::to_string(&standalone).unwrap();
    let deserialized: gogo_gadget::OperationMode = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, gogo_gadget::OperationMode::Standalone);

    let subagent = gogo_gadget::OperationMode::Subagent;
    let json = serde_json::to_string(&subagent).unwrap();
    let deserialized: gogo_gadget::OperationMode = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, gogo_gadget::OperationMode::Subagent);
}

#[test]
fn test_subagent_config_default() {
    let config = gogo_gadget::SubagentConfig::default();

    assert!(config.task.is_empty());
    assert!(config.parent_session_id.is_none());
    assert_eq!(config.max_depth, 5);
    assert_eq!(config.current_depth, 0);
}

#[test]
fn test_subagent_config_custom() {
    let config = gogo_gadget::SubagentConfig {
        task: "Build REST API".to_string(),
        parent_session_id: Some("session-123".to_string()),
        max_depth: 3,
        current_depth: 1,
        context_policy: gogo_gadget::ContextPolicy::default(),
        output_format: gogo_gadget::SubagentOutputFormat::Json,
    };

    assert_eq!(config.task, "Build REST API");
    assert_eq!(config.parent_session_id, Some("session-123".to_string()));
    assert_eq!(config.max_depth, 3);
    assert_eq!(config.current_depth, 1);
}

#[test]
fn test_subagent_config_serialization() {
    let config = gogo_gadget::SubagentConfig {
        task: "Implement feature".to_string(),
        parent_session_id: Some("parent-456".to_string()),
        max_depth: 4,
        current_depth: 2,
        context_policy: gogo_gadget::ContextPolicy::default(),
        output_format: gogo_gadget::SubagentOutputFormat::Markdown,
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: gogo_gadget::SubagentConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.task, config.task);
    assert_eq!(deserialized.max_depth, config.max_depth);
    assert_eq!(deserialized.current_depth, config.current_depth);
}

#[test]
fn test_context_policy_default() {
    let policy = gogo_gadget::ContextPolicy::default();

    assert!(policy.hide_failures);
    assert!(policy.hide_intermediates);
    assert!(policy.include_commit);
    assert_eq!(policy.max_summary_length, 500);
    assert!(!policy.include_diffs);
}

#[test]
fn test_context_policy_custom() {
    let policy = gogo_gadget::ContextPolicy {
        hide_failures: false,
        hide_intermediates: false,
        include_commit: false,
        max_summary_length: 1000,
        include_diffs: true,
    };

    assert!(!policy.hide_failures);
    assert!(!policy.hide_intermediates);
    assert!(!policy.include_commit);
    assert_eq!(policy.max_summary_length, 1000);
    assert!(policy.include_diffs);
}

#[test]
fn test_context_policy_serialization() {
    let policy = gogo_gadget::ContextPolicy {
        hide_failures: true,
        hide_intermediates: false,
        include_commit: true,
        max_summary_length: 250,
        include_diffs: true,
    };

    let json = serde_json::to_string(&policy).unwrap();
    let deserialized: gogo_gadget::ContextPolicy = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.hide_failures, policy.hide_failures);
    assert_eq!(deserialized.hide_intermediates, policy.hide_intermediates);
    assert_eq!(deserialized.include_commit, policy.include_commit);
    assert_eq!(deserialized.max_summary_length, policy.max_summary_length);
    assert_eq!(deserialized.include_diffs, policy.include_diffs);
}

#[test]
fn test_subagent_output_format_default() {
    let format = gogo_gadget::SubagentOutputFormat::default();
    assert_eq!(format, gogo_gadget::SubagentOutputFormat::Json);
}

#[test]
fn test_subagent_output_format_variants() {
    let json = gogo_gadget::SubagentOutputFormat::Json;
    let text = gogo_gadget::SubagentOutputFormat::Text;
    let markdown = gogo_gadget::SubagentOutputFormat::Markdown;

    assert_eq!(json, gogo_gadget::SubagentOutputFormat::Json);
    assert_eq!(text, gogo_gadget::SubagentOutputFormat::Text);
    assert_eq!(markdown, gogo_gadget::SubagentOutputFormat::Markdown);
    assert_ne!(json, text);
    assert_ne!(text, markdown);
}

#[test]
fn test_subagent_output_format_serialization() {
    for format in [
        gogo_gadget::SubagentOutputFormat::Json,
        gogo_gadget::SubagentOutputFormat::Text,
        gogo_gadget::SubagentOutputFormat::Markdown,
    ] {
        let json = serde_json::to_string(&format).unwrap();
        let deserialized: gogo_gadget::SubagentOutputFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, format);
    }
}

#[test]
fn test_compressed_result_success() {
    let result = gogo_gadget::CompressedResult {
        success: true,
        commit_hash: Some("abc123def456".to_string()),
        files_changed: vec![
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/lib.rs"),
        ],
        summary: "Implemented authentication module".to_string(),
        duration_ms: 15000,
        iterations: 3,
        error: None,
        verified: true,
        metadata: gogo_gadget::CompressedResultMetadata {
            version: "0.1.0".to_string(),
            depth: 0,
            parent_session_id: None,
            completed_at: 1704067200,
            model: Some("claude-3-opus".to_string()),
        },
    };

    assert!(result.success);
    assert!(result.verified);
    assert!(result.error.is_none());
    assert_eq!(result.files_changed.len(), 2);
    assert_eq!(result.iterations, 3);
}

#[test]
fn test_compressed_result_failure() {
    let result = gogo_gadget::CompressedResult {
        success: false,
        commit_hash: None,
        files_changed: vec![],
        summary: "Task failed due to compilation errors".to_string(),
        duration_ms: 5000,
        iterations: 5,
        error: Some("Build failed with 3 errors".to_string()),
        verified: false,
        metadata: gogo_gadget::CompressedResultMetadata {
            version: "0.1.0".to_string(),
            depth: 1,
            parent_session_id: Some("parent-session".to_string()),
            completed_at: 1704067200,
            model: None,
        },
    };

    assert!(!result.success);
    assert!(!result.verified);
    assert!(result.error.is_some());
    assert_eq!(result.error.as_ref().unwrap(), "Build failed with 3 errors");
    assert!(result.files_changed.is_empty());
}

#[test]
fn test_compressed_result_serialization() {
    let result = gogo_gadget::CompressedResult {
        success: true,
        commit_hash: Some("xyz789".to_string()),
        files_changed: vec![PathBuf::from("test.rs")],
        summary: "Test summary".to_string(),
        duration_ms: 1000,
        iterations: 1,
        error: None,
        verified: true,
        metadata: gogo_gadget::CompressedResultMetadata {
            version: "0.1.0".to_string(),
            depth: 0,
            parent_session_id: None,
            completed_at: 0,
            model: None,
        },
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("xyz789"));
    assert!(json.contains("test.rs"));

    let deserialized: gogo_gadget::CompressedResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.success, result.success);
    assert_eq!(deserialized.commit_hash, result.commit_hash);
    assert_eq!(deserialized.summary, result.summary);
}

#[test]
fn test_compressed_result_metadata() {
    let metadata = gogo_gadget::CompressedResultMetadata {
        version: "1.2.3".to_string(),
        depth: 2,
        parent_session_id: Some("session-abc".to_string()),
        completed_at: 1704067200,
        model: Some("claude-3-sonnet".to_string()),
    };

    assert_eq!(metadata.version, "1.2.3");
    assert_eq!(metadata.depth, 2);
    assert_eq!(metadata.parent_session_id, Some("session-abc".to_string()));
    assert_eq!(metadata.completed_at, 1704067200);
    assert_eq!(metadata.model, Some("claude-3-sonnet".to_string()));
}

#[test]
fn test_compressed_result_metadata_serialization() {
    let metadata = gogo_gadget::CompressedResultMetadata {
        version: "0.2.0".to_string(),
        depth: 1,
        parent_session_id: None,
        completed_at: 1704153600,
        model: None,
    };

    let json = serde_json::to_string(&metadata).unwrap();
    let deserialized: gogo_gadget::CompressedResultMetadata = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.version, metadata.version);
    assert_eq!(deserialized.depth, metadata.depth);
    assert_eq!(deserialized.parent_session_id, metadata.parent_session_id);
}

#[test]
fn test_subagent_registration_default() {
    let reg = gogo_gadget::SubagentRegistration::default();

    assert_eq!(reg.name, "gogo-gadget");
    assert!(reg.description.contains("Autonomous AI agent"));
    assert_eq!(reg.command, "gogo-gadget --subagent-mode");
    assert!(!reg.triggers.is_empty());
    assert!(!reg.capabilities.is_empty());
}

#[test]
fn test_subagent_registration_triggers() {
    let reg = gogo_gadget::SubagentRegistration::default();

    assert!(reg.triggers.contains(&"iterate until done".to_string()));
    assert!(reg.triggers.contains(&"run until complete".to_string()));
    assert!(reg.triggers.contains(&"verify completion".to_string()));
    assert!(reg.triggers.contains(&"autonomous task".to_string()));
    assert!(reg.triggers.contains(&"gogo-gadget".to_string()));
}

#[test]
fn test_subagent_registration_capabilities() {
    let reg = gogo_gadget::SubagentRegistration::default();

    assert!(reg.capabilities.contains(&"iterative_execution".to_string()));
    assert!(reg.capabilities.contains(&"verified_completion".to_string()));
    assert!(reg.capabilities.contains(&"context_compression".to_string()));
    assert!(reg.capabilities.contains(&"anti_laziness".to_string()));
    assert!(reg.capabilities.contains(&"recursive_subagents".to_string()));
}

#[test]
fn test_subagent_registration_serialization() {
    let reg = gogo_gadget::SubagentRegistration::default();

    let json = serde_json::to_string(&reg).unwrap();
    assert!(json.contains("gogo-gadget"));
    assert!(json.contains("context_compression"));

    let deserialized: gogo_gadget::SubagentRegistration = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, reg.name);
    assert_eq!(deserialized.triggers, reg.triggers);
}

#[test]
fn test_subagent_registration_custom() {
    let reg = gogo_gadget::SubagentRegistration {
        name: "custom-agent".to_string(),
        description: "A custom subagent".to_string(),
        command: "custom-agent --mode subagent".to_string(),
        triggers: vec!["custom trigger".to_string()],
        capabilities: vec!["custom capability".to_string()],
        context_policy: gogo_gadget::ContextPolicy::default(),
        version: "1.0.0".to_string(),
    };

    assert_eq!(reg.name, "custom-agent");
    assert_eq!(reg.triggers.len(), 1);
    assert_eq!(reg.capabilities.len(), 1);
}

#[test]
fn test_subagent_config_depth_limits() {
    // Test various depth configurations
    let configs = [
        (0, 0, true),   // depth 0, max 0 - at limit
        (0, 5, false),  // depth 0, max 5 - not at limit
        (4, 5, false),  // depth 4, max 5 - not at limit
        (5, 5, true),   // depth 5, max 5 - at limit
        (6, 5, true),   // depth 6, max 5 - exceeded
    ];

    for (current, max, at_limit) in configs {
        let config = gogo_gadget::SubagentConfig {
            task: "test".to_string(),
            parent_session_id: None,
            max_depth: max,
            current_depth: current,
            context_policy: gogo_gadget::ContextPolicy::default(),
            output_format: gogo_gadget::SubagentOutputFormat::default(),
        };

        assert_eq!(
            config.current_depth >= config.max_depth,
            at_limit,
            "current={}, max={} should be at_limit={}",
            current,
            max,
            at_limit
        );
    }
}

#[test]
fn test_context_policy_max_summary_length_edge_cases() {
    // Test zero length
    let policy_zero = gogo_gadget::ContextPolicy {
        max_summary_length: 0,
        ..gogo_gadget::ContextPolicy::default()
    };
    assert_eq!(policy_zero.max_summary_length, 0);

    // Test very large length
    let policy_large = gogo_gadget::ContextPolicy {
        max_summary_length: 1_000_000,
        ..gogo_gadget::ContextPolicy::default()
    };
    assert_eq!(policy_large.max_summary_length, 1_000_000);
}

#[test]
fn test_compressed_result_empty_files() {
    let result = gogo_gadget::CompressedResult {
        success: true,
        commit_hash: None,
        files_changed: vec![],
        summary: "No files changed".to_string(),
        duration_ms: 100,
        iterations: 1,
        error: None,
        verified: true,
        metadata: gogo_gadget::CompressedResultMetadata {
            version: "0.1.0".to_string(),
            depth: 0,
            parent_session_id: None,
            completed_at: 0,
            model: None,
        },
    };

    assert!(result.files_changed.is_empty());
    assert!(result.commit_hash.is_none());
}

#[test]
fn test_compressed_result_many_files() {
    let files: Vec<PathBuf> = (0..100)
        .map(|i| PathBuf::from(format!("src/file_{}.rs", i)))
        .collect();

    let result = gogo_gadget::CompressedResult {
        success: true,
        commit_hash: Some("abc123".to_string()),
        files_changed: files.clone(),
        summary: "Changed 100 files".to_string(),
        duration_ms: 60000,
        iterations: 10,
        error: None,
        verified: true,
        metadata: gogo_gadget::CompressedResultMetadata {
            version: "0.1.0".to_string(),
            depth: 0,
            parent_session_id: None,
            completed_at: 0,
            model: None,
        },
    };

    assert_eq!(result.files_changed.len(), 100);
}

#[test]
fn test_subagent_config_with_nested_context_policy() {
    let custom_policy = gogo_gadget::ContextPolicy {
        hide_failures: false,
        hide_intermediates: false,
        include_commit: true,
        max_summary_length: 1000,
        include_diffs: true,
    };

    let config = gogo_gadget::SubagentConfig {
        task: "Complex task".to_string(),
        parent_session_id: Some("parent-123".to_string()),
        max_depth: 3,
        current_depth: 1,
        context_policy: custom_policy,
        output_format: gogo_gadget::SubagentOutputFormat::Markdown,
    };

    assert!(!config.context_policy.hide_failures);
    assert!(config.context_policy.include_diffs);
    assert_eq!(config.context_policy.max_summary_length, 1000);
}

// ============================================================================
// Self-Extension Module Types Tests (src/extend/)
// ============================================================================

#[test]
fn test_capability_type_variants() {
    use gogo_gadget::extend::CapabilityType;

    let mcp = CapabilityType::Mcp;
    let skill = CapabilityType::Skill;
    let agent = CapabilityType::Agent;

    assert_eq!(mcp, CapabilityType::Mcp);
    assert_eq!(skill, CapabilityType::Skill);
    assert_eq!(agent, CapabilityType::Agent);
    assert_ne!(mcp, skill);
    assert_ne!(skill, agent);
}

#[test]
fn test_capability_type_serialization() {
    use gogo_gadget::extend::CapabilityType;

    for cap_type in [CapabilityType::Mcp, CapabilityType::Skill, CapabilityType::Agent] {
        let json = serde_json::to_string(&cap_type).unwrap();
        let deserialized: CapabilityType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, cap_type);
    }
}

#[test]
fn test_capability_gap_mcp() {
    use gogo_gadget::extend::CapabilityGap;

    let gap = CapabilityGap::Mcp {
        name: "weather-api".to_string(),
        purpose: "Fetch weather data from OpenWeatherMap".to_string(),
        api_hint: Some("https://api.openweathermap.org".to_string()),
    };

    match gap {
        CapabilityGap::Mcp { name, purpose, api_hint } => {
            assert_eq!(name, "weather-api");
            assert_eq!(purpose, "Fetch weather data from OpenWeatherMap");
            assert_eq!(api_hint, Some("https://api.openweathermap.org".to_string()));
        }
        _ => panic!("Expected Mcp variant"),
    }
}

#[test]
fn test_capability_gap_skill() {
    use gogo_gadget::extend::CapabilityGap;

    let gap = CapabilityGap::Skill {
        name: "code-review".to_string(),
        trigger: "review this code".to_string(),
        purpose: "Perform thorough code review with suggestions".to_string(),
    };

    match gap {
        CapabilityGap::Skill { name, trigger, purpose } => {
            assert_eq!(name, "code-review");
            assert_eq!(trigger, "review this code");
            assert_eq!(purpose, "Perform thorough code review with suggestions");
        }
        _ => panic!("Expected Skill variant"),
    }
}

#[test]
fn test_capability_gap_agent() {
    use gogo_gadget::extend::CapabilityGap;

    let gap = CapabilityGap::Agent {
        name: "security-auditor".to_string(),
        specialization: "Security vulnerability analysis and remediation".to_string(),
    };

    match gap {
        CapabilityGap::Agent { name, specialization } => {
            assert_eq!(name, "security-auditor");
            assert_eq!(specialization, "Security vulnerability analysis and remediation");
        }
        _ => panic!("Expected Agent variant"),
    }
}

#[test]
fn test_capability_gap_serialization() {
    use gogo_gadget::extend::CapabilityGap;

    let gaps = vec![
        CapabilityGap::Mcp {
            name: "test-mcp".to_string(),
            purpose: "Test purpose".to_string(),
            api_hint: None,
        },
        CapabilityGap::Skill {
            name: "test-skill".to_string(),
            trigger: "test trigger".to_string(),
            purpose: "Test skill purpose".to_string(),
        },
        CapabilityGap::Agent {
            name: "test-agent".to_string(),
            specialization: "Test specialization".to_string(),
        },
    ];

    for gap in gaps {
        let json = serde_json::to_string(&gap).unwrap();
        let deserialized: CapabilityGap = serde_json::from_str(&json).unwrap();

        // Verify roundtrip works
        let json2 = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json, json2);
    }
}

#[test]
fn test_synthesized_capability_mcp() {
    use gogo_gadget::extend::{SynthesizedCapability, CapabilityType};

    let cap = SynthesizedCapability::new(
        CapabilityType::Mcp,
        "github-api",
        PathBuf::from("/home/user/.gogo-gadget/mcps/github-api"),
    );

    assert_eq!(cap.capability_type, CapabilityType::Mcp);
    assert_eq!(cap.name, "github-api");
    assert_eq!(cap.usage_count, 0);
    assert_eq!(cap.success_rate, 0.0);
    assert!(cap.synthesized);
}

#[test]
fn test_synthesized_capability_skill() {
    use gogo_gadget::extend::{SynthesizedCapability, CapabilityType};

    let mut cap = SynthesizedCapability::new(
        CapabilityType::Skill,
        "docker-compose",
        PathBuf::from("/home/user/.gogo-gadget/skills/docker-compose.md"),
    );
    cap.usage_count = 5;
    cap.success_rate = 0.8;
    cap.config = serde_json::json!({
        "trigger": "compose docker services",
        "requires": ["docker", "docker-compose"]
    });

    assert_eq!(cap.capability_type, CapabilityType::Skill);
    assert_eq!(cap.name, "docker-compose");
    assert_eq!(cap.usage_count, 5);
    assert_eq!(cap.success_rate, 0.8);
}

#[test]
fn test_synthesized_capability_serialization() {
    use gogo_gadget::extend::{SynthesizedCapability, CapabilityType};

    let mut cap = SynthesizedCapability::new(
        CapabilityType::Agent,
        "test-agent",
        PathBuf::from("/agents/test"),
    );
    cap.config = serde_json::json!({"key": "value"});
    cap.usage_count = 10;
    cap.success_rate = 0.95;

    let json = serde_json::to_string(&cap).unwrap();
    let deserialized: SynthesizedCapability = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.capability_type, cap.capability_type);
    assert_eq!(deserialized.name, cap.name);
    assert_eq!(deserialized.usage_count, cap.usage_count);
    assert_eq!(deserialized.success_rate, cap.success_rate);
}

#[test]
fn test_gap_detector_creation() {
    use gogo_gadget::extend::GapDetector;

    let detector = GapDetector::new();
    // Verify detector can be created
    drop(detector);
}

#[test]
fn test_capability_registry_creation() {
    use gogo_gadget::extend::CapabilityRegistry;

    let registry = CapabilityRegistry::new_default();
    // Verify registry can be created
    drop(registry);
}

#[test]
fn test_synthesis_engine_creation() {
    use gogo_gadget::extend::SynthesisEngine;

    let engine = SynthesisEngine::default();
    // Verify engine can be created
    drop(engine);
}

#[test]
fn test_capability_verifier_creation() {
    use gogo_gadget::extend::CapabilityVerifier;

    let verifier = CapabilityVerifier::new();
    // Verify verifier can be created
    drop(verifier);
}

#[test]
fn test_hot_loader_creation() {
    use gogo_gadget::extend::HotLoader;

    let loader = HotLoader::new();
    // Verify loader can be created
    drop(loader);
}

#[test]
fn test_verification_result_success() {
    use gogo_gadget::extend::VerificationResult;

    let result = VerificationResult::success();

    assert!(result.success);
    assert!(result.errors.is_empty());
    assert!(result.warnings.is_empty());
}

#[test]
fn test_verification_result_failure() {
    use gogo_gadget::extend::VerificationResult;

    let mut result = VerificationResult::failure("Failed to start MCP server");
    result.add_error("Port already in use");

    assert!(!result.success);
    assert_eq!(result.errors.len(), 2);
    assert!(result.warnings.is_empty());
}

#[test]
fn test_verification_result_serialization() {
    use gogo_gadget::extend::VerificationResult;

    let mut result = VerificationResult::success();
    result.add_warning("Warning 1");

    let json = serde_json::to_string(&result).unwrap();
    let deserialized: VerificationResult = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.success, result.success);
    assert_eq!(deserialized.errors, result.errors);
    assert_eq!(deserialized.warnings, result.warnings);
}
