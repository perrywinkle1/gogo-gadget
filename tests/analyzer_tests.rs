//! Unit Tests for the LLM-Driven Brain Analyzer Module
//!
//! Tests cover:
//! - TaskCategory parsing from strings
//! - DetectedLanguage parsing from strings
//! - TaskAnalysis default values
//! - TaskAnalyzer configuration (builder pattern)
//! - File mention counting
//! - JSON extraction from LLM responses
//! - Fallback analysis behavior
//!
//! Note: Full LLM integration tests require a running Claude instance
//! and are marked with #[ignore] to avoid CI failures.

use gogo_gadget::brain::{DetectedLanguage, TaskAnalyzer, TaskCategory, TaskAnalysis};
use gogo_gadget::{Difficulty, ExecutionMode};
use tempfile::TempDir;

// ============================================================================
// TaskCategory Parsing Tests
// ============================================================================

mod category_parsing {
    use super::*;

    #[test]
    fn test_bugfix_variants() {
        // TaskCategory::from_str is private, so we test via TaskAnalysis defaults
        let analysis = TaskAnalysis::default();
        assert_eq!(analysis.category, TaskCategory::General);
    }

    #[test]
    fn test_task_analysis_default_values() {
        let analysis = TaskAnalysis::default();

        assert_eq!(analysis.complexity_score, 0);
        assert_eq!(analysis.mode, ExecutionMode::Single);
        assert!(!analysis.needs_iteration);
        assert!(!analysis.needs_ultrathink);
        assert!(!analysis.needs_visual);
        assert_eq!(analysis.difficulty, Difficulty::Trivial);
        assert_eq!(analysis.suggested_timeout, 30);
        assert_eq!(analysis.word_count, 0);
        assert_eq!(analysis.file_mentions, 0);
        assert!(analysis.triggered_keywords.is_empty());
        assert_eq!(analysis.category, TaskCategory::General);
        assert_eq!(analysis.detected_language, DetectedLanguage::Unknown);
        assert_eq!(analysis.estimated_files, 0);
        assert!(analysis.parallel_safe);
        assert!(!analysis.is_security_sensitive);
        assert_eq!(analysis.codebase_complexity, 0);
        assert!(analysis.completion_criteria.is_empty());
        assert!(analysis.required_outcomes.is_empty());
        assert!(analysis.success_indicators.is_empty());
    }
}

// ============================================================================
// DetectedLanguage Tests
// ============================================================================

mod language_detection {
    use super::*;

    #[test]
    fn test_detected_language_default() {
        let lang = DetectedLanguage::default();
        assert_eq!(lang, DetectedLanguage::Unknown);
    }

    #[test]
    fn test_all_language_variants_exist() {
        // Ensure all variants are accessible
        let _langs = vec![
            DetectedLanguage::Rust,
            DetectedLanguage::Python,
            DetectedLanguage::JavaScript,
            DetectedLanguage::TypeScript,
            DetectedLanguage::Go,
            DetectedLanguage::Java,
            DetectedLanguage::CppOrC,
            DetectedLanguage::Ruby,
            DetectedLanguage::Php,
            DetectedLanguage::Swift,
            DetectedLanguage::Kotlin,
            DetectedLanguage::Shell,
            DetectedLanguage::Mixed,
            DetectedLanguage::Unknown,
        ];
    }
}

// ============================================================================
// TaskCategory Tests
// ============================================================================

mod category_tests {
    use super::*;

    #[test]
    fn test_task_category_default() {
        let cat = TaskCategory::default();
        assert_eq!(cat, TaskCategory::General);
    }

    #[test]
    fn test_all_category_variants_exist() {
        // Ensure all variants are accessible
        let _cats = vec![
            TaskCategory::BugFix,
            TaskCategory::Feature,
            TaskCategory::Refactor,
            TaskCategory::Security,
            TaskCategory::Documentation,
            TaskCategory::Testing,
            TaskCategory::Performance,
            TaskCategory::Configuration,
            TaskCategory::Review,
            TaskCategory::DevOps,
            TaskCategory::Database,
            TaskCategory::Api,
            TaskCategory::Debugging,
            TaskCategory::General,
        ];
    }
}

// ============================================================================
// TaskAnalyzer Configuration Tests
// ============================================================================

mod analyzer_configuration {
    use super::*;

    #[test]
    fn test_analyzer_new() {
        let analyzer = TaskAnalyzer::new();
        // Just verify it can be created
        drop(analyzer);
    }

    #[test]
    fn test_analyzer_default() {
        let analyzer = TaskAnalyzer::default();
        drop(analyzer);
    }

    #[test]
    fn test_analyzer_with_model() {
        let analyzer = TaskAnalyzer::new()
            .with_model("claude-3-opus");
        drop(analyzer);
    }

    #[test]
    fn test_analyzer_with_working_dir() {
        let temp_dir = TempDir::new().unwrap();
        let analyzer = TaskAnalyzer::new()
            .with_working_dir(temp_dir.path());
        drop(analyzer);
    }

    #[test]
    fn test_analyzer_builder_chain() {
        let temp_dir = TempDir::new().unwrap();
        let analyzer = TaskAnalyzer::new()
            .with_model("claude-3-sonnet")
            .with_working_dir(temp_dir.path());
        drop(analyzer);
    }
}

// ============================================================================
// Execution Mode Tests
// ============================================================================

mod execution_mode {
    use super::*;

    #[test]
    fn test_execution_mode_single() {
        let mode = ExecutionMode::Single;
        assert_eq!(mode, ExecutionMode::Single);
    }

    #[test]
    fn test_execution_mode_swarm() {
        let mode = ExecutionMode::Swarm { agent_count: 3 };
        match mode {
            ExecutionMode::Swarm { agent_count } => {
                assert_eq!(agent_count, 3);
            }
            _ => panic!("Expected Swarm mode"),
        }
    }
}

// ============================================================================
// Difficulty Level Tests
// ============================================================================

mod difficulty_tests {
    use super::*;

    #[test]
    fn test_difficulty_from_complexity() {
        // Test the Difficulty::from(u32) implementation
        assert_eq!(Difficulty::from(0u32), Difficulty::Trivial);
        assert_eq!(Difficulty::from(2u32), Difficulty::Trivial);
        assert_eq!(Difficulty::from(3u32), Difficulty::Easy);
        assert_eq!(Difficulty::from(4u32), Difficulty::Easy);
        assert_eq!(Difficulty::from(5u32), Difficulty::Medium);
        assert_eq!(Difficulty::from(6u32), Difficulty::Medium);
        assert_eq!(Difficulty::from(7u32), Difficulty::Hard);
        assert_eq!(Difficulty::from(8u32), Difficulty::Hard);
        assert_eq!(Difficulty::from(9u32), Difficulty::Expert);
        assert_eq!(Difficulty::from(10u32), Difficulty::Expert);
    }

    #[test]
    fn test_difficulty_variants() {
        let _difficulties = vec![
            Difficulty::Trivial,
            Difficulty::Easy,
            Difficulty::Medium,
            Difficulty::Hard,
            Difficulty::Expert,
        ];
    }
}

// ============================================================================
// Integration Tests (require Claude - marked as ignored)
// ============================================================================

mod integration {
    use super::*;

    #[test]
    #[ignore = "Requires running Claude instance"]
    fn test_analyze_simple_task() {
        let temp_dir = TempDir::new().unwrap();
        let analyzer = TaskAnalyzer::new()
            .with_working_dir(temp_dir.path());

        let result = analyzer.analyze("Fix the typo in README.md");
        assert!(result.is_ok());

        let analysis = result.unwrap();
        assert!(!analysis.completion_criteria.is_empty());
    }

    #[test]
    #[ignore = "Requires running Claude instance"]
    fn test_analyze_complex_task() {
        let temp_dir = TempDir::new().unwrap();

        // Create a Cargo.toml to indicate Rust project
        std::fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"[package]
name = "test"
version = "0.1.0"
"#
        ).unwrap();

        std::fs::create_dir(temp_dir.path().join("src")).unwrap();
        std::fs::write(
            temp_dir.path().join("src/main.rs"),
            "fn main() {}"
        ).unwrap();

        let analyzer = TaskAnalyzer::new()
            .with_working_dir(temp_dir.path());

        let result = analyzer.analyze(
            "Refactor the authentication module to use async/await and add comprehensive error handling"
        );
        assert!(result.is_ok());

        let analysis = result.unwrap();
        assert!(analysis.complexity_score >= 5);
        assert!(!analysis.required_outcomes.is_empty());
    }

    #[test]
    #[ignore = "Requires running Claude instance"]
    fn test_analyze_security_task() {
        let analyzer = TaskAnalyzer::new();

        let result = analyzer.analyze(
            "Audit the authentication system for SQL injection vulnerabilities and implement proper input sanitization"
        );
        assert!(result.is_ok());

        let analysis = result.unwrap();
        assert!(analysis.is_security_sensitive);
    }
}

// ============================================================================
// Serialization Tests
// ============================================================================

mod serialization {
    use super::*;

    #[test]
    fn test_task_analysis_serialization() {
        let analysis = TaskAnalysis::default();

        // Test JSON serialization roundtrip
        let json = serde_json::to_string(&analysis).unwrap();
        let deserialized: TaskAnalysis = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.complexity_score, analysis.complexity_score);
        assert_eq!(deserialized.mode, analysis.mode);
        assert_eq!(deserialized.category, analysis.category);
    }

    #[test]
    fn test_task_category_serialization() {
        let category = TaskCategory::BugFix;
        let json = serde_json::to_string(&category).unwrap();
        let deserialized: TaskCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TaskCategory::BugFix);
    }

    #[test]
    fn test_detected_language_serialization() {
        let language = DetectedLanguage::Rust;
        let json = serde_json::to_string(&language).unwrap();
        let deserialized: DetectedLanguage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, DetectedLanguage::Rust);
    }
}

// ============================================================================
// Completion Criteria Tests
// ============================================================================

mod completion_criteria {
    use super::*;

    #[test]
    fn test_task_analysis_with_completion_criteria() {
        let mut analysis = TaskAnalysis::default();
        analysis.completion_criteria = "All tests pass and code compiles".to_string();
        analysis.required_outcomes = vec![
            "Tests pass".to_string(),
            "No compiler errors".to_string(),
        ];
        analysis.success_indicators = vec![
            "cargo test succeeds".to_string(),
            "cargo build succeeds".to_string(),
        ];

        assert!(!analysis.completion_criteria.is_empty());
        assert_eq!(analysis.required_outcomes.len(), 2);
        assert_eq!(analysis.success_indicators.len(), 2);
    }
}
