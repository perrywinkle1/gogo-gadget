//! RLM Aggregator - Combines recursive exploration results
//!
//! Handles deduplication, conflict resolution, and synthesis of findings.

use crate::rlm::types::{ChunkResult, Evidence, Finding, FindingType, RlmConfig, RlmResult};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info, warn};

/// Aggregation strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregationStrategy {
    /// Simple concatenation of findings
    Concatenate,
    /// LLM-based synthesis
    Synthesize,
    /// Hierarchical bottom-up aggregation
    Hierarchical,
}

impl Default for AggregationStrategy {
    fn default() -> Self {
        AggregationStrategy::Synthesize
    }
}

/// Result aggregator
pub struct Aggregator {
    /// Model for synthesis (strong model)
    #[allow(dead_code)]
    model: Option<String>,

    /// Working directory
    working_dir: PathBuf,

    /// Similarity threshold for deduplication
    #[allow(dead_code)]
    dedup_threshold: f32,

    /// Configuration
    #[allow(dead_code)]
    config: RlmConfig,
}

impl Aggregator {
    /// Create a new aggregator
    pub fn new(working_dir: PathBuf, config: RlmConfig) -> Self {
        Self {
            model: config.explorer_model.clone(),
            working_dir,
            dedup_threshold: 0.85,
            config,
        }
    }

    /// Aggregate results into final answer
    pub fn aggregate(
        &self,
        results: &[ChunkResult],
        query: &str,
        strategy: AggregationStrategy,
        duration_ms: u64,
    ) -> Result<RlmResult> {
        // Collect all findings
        let mut all_findings: Vec<Finding> = results
            .iter()
            .flat_map(|r| self.collect_findings(r))
            .collect();

        if all_findings.is_empty() {
            return Ok(self.empty_result(results, duration_ms));
        }

        // Deduplicate
        all_findings = self.deduplicate(all_findings);

        // Resolve conflicts
        all_findings = self.resolve_conflicts(all_findings);

        // Sort by relevance
        all_findings.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Generate final answer based on strategy
        let answer = match strategy {
            AggregationStrategy::Concatenate => self.concatenate_findings(&all_findings),
            AggregationStrategy::Synthesize => self.synthesize_findings(&all_findings, query)?,
            AggregationStrategy::Hierarchical => {
                self.hierarchical_aggregate(&all_findings, query)?
            }
        };

        // Build evidence list
        let evidence = self.build_evidence(&all_findings);

        // Calculate metrics
        let chunks_explored = results.len();
        let max_depth = results.iter().map(|r| r.depth).max().unwrap_or(0);
        let total_tokens: u32 = results.iter().map(|r| r.tokens_used).sum();
        let avg_confidence = if all_findings.is_empty() {
            0.0
        } else {
            all_findings.iter().map(|f| f.confidence).sum::<f32>() / all_findings.len() as f32
        };

        Ok(RlmResult {
            success: true,
            answer,
            key_insights: self.extract_insights(&all_findings),
            evidence,
            chunks_explored,
            max_depth_reached: max_depth,
            total_tokens,
            confidence: avg_confidence,
            duration_ms,
            error: None,
        })
    }

    /// Collect findings recursively from chunk results
    pub fn collect_findings(&self, result: &ChunkResult) -> Vec<Finding> {
        let mut findings = result.findings.clone();

        for sub in &result.sub_results {
            findings.extend(self.collect_findings(sub));
        }

        findings
    }

    /// Deduplicate similar findings
    pub fn deduplicate(&self, findings: Vec<Finding>) -> Vec<Finding> {
        let original_len = findings.len();
        let mut unique: Vec<Finding> = Vec::new();
        let mut seen_content: HashSet<String> = HashSet::new();

        for finding in findings {
            // Simple content-based dedup
            let normalized = finding
                .content
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric() || c.is_whitespace())
                .collect::<String>();

            // Check first 100 chars for similarity
            let key = normalized.chars().take(100).collect::<String>();

            if !seen_content.contains(&key) {
                seen_content.insert(key);
                unique.push(finding);
            } else {
                debug!(
                    "Deduped finding: {}",
                    finding.content.chars().take(50).collect::<String>()
                );
            }
        }

        info!("Deduplicated {} findings to {}", original_len, unique.len());
        unique
    }

    /// Resolve conflicting findings
    pub fn resolve_conflicts(&self, findings: Vec<Finding>) -> Vec<Finding> {
        // Group findings by type
        let mut answers: Vec<Finding> = Vec::new();
        let mut evidence: Vec<Finding> = Vec::new();
        let mut conflicts: Vec<Finding> = Vec::new();
        let mut other: Vec<Finding> = Vec::new();

        for finding in findings {
            match finding.finding_type {
                FindingType::Answer => answers.push(finding),
                FindingType::Evidence => evidence.push(finding),
                FindingType::Conflict => conflicts.push(finding),
                _ => other.push(finding),
            }
        }

        // If there are conflicts, keep them for synthesis
        if !conflicts.is_empty() {
            warn!("Found {} conflicting findings", conflicts.len());
        }

        // Weight answers by confidence and depth (deeper = more specific)
        answers.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Combine all
        let mut result = answers;
        result.extend(evidence);
        result.extend(conflicts);
        result.extend(other);

        result
    }

    /// Simple concatenation of findings
    pub fn concatenate_findings(&self, findings: &[Finding]) -> String {
        findings
            .iter()
            .filter(|f| f.finding_type == FindingType::Answer)
            .take(5)
            .map(|f| f.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// LLM-based synthesis of findings
    pub fn synthesize_findings(&self, findings: &[Finding], query: &str) -> Result<String> {
        if findings.is_empty() {
            return Ok("No relevant findings to synthesize.".to_string());
        }

        // Prepare findings summary for synthesis
        let findings_text = findings
            .iter()
            .take(10)
            .enumerate()
            .map(|(i, f)| {
                format!(
                    "{}. [{}] (confidence: {:.2})\n{}",
                    i + 1,
                    format!("{:?}", f.finding_type),
                    f.confidence,
                    f.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            r#"Synthesize these findings into a coherent answer to the query.

QUERY: {}

FINDINGS:
{}

Provide a clear, direct answer that:
1. Answers the query directly
2. Integrates information from multiple findings
3. Notes any conflicts or uncertainties
4. Is concise but complete

Answer:"#,
            query, findings_text
        );

        let output = Command::new("claude")
            .args(["--print", "-p", &prompt])
            .current_dir(&self.working_dir)
            .output()
            .context("Failed to execute claude CLI")?;

        if !output.status.success() {
            // Fall back to concatenation
            return Ok(self.concatenate_findings(findings));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Hierarchical bottom-up aggregation
    pub fn hierarchical_aggregate(&self, findings: &[Finding], query: &str) -> Result<String> {
        // Group by source file
        let mut by_file: std::collections::HashMap<Option<PathBuf>, Vec<&Finding>> =
            std::collections::HashMap::new();

        for finding in findings {
            by_file
                .entry(finding.source_path.clone())
                .or_default()
                .push(finding);
        }

        // Summarize each file's findings
        let mut summaries: Vec<String> = Vec::new();
        for (path, file_findings) in by_file {
            let path_str = path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let summary = file_findings
                .iter()
                .take(3)
                .map(|f| f.content.chars().take(100).collect::<String>())
                .collect::<Vec<_>>()
                .join("; ");

            summaries.push(format!("{}: {}", path_str, summary));
        }

        // Final synthesis
        if summaries.len() <= 3 {
            return self.synthesize_findings(findings, query);
        }

        // Otherwise, summarize the summaries
        let meta_prompt = format!(
            r#"Synthesize these per-file summaries into a final answer.

QUERY: {}

FILE SUMMARIES:
{}

Provide a concise, integrated answer:"#,
            query,
            summaries.join("\n")
        );

        let output = Command::new("claude")
            .args(["--print", "-p", &meta_prompt])
            .current_dir(&self.working_dir)
            .output()
            .context("Failed to execute claude CLI")?;

        if !output.status.success() {
            return Ok(summaries.join("\n"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Extract key insights from findings
    pub fn extract_insights(&self, findings: &[Finding]) -> Vec<String> {
        findings
            .iter()
            .filter(|f| f.confidence >= 0.7)
            .take(5)
            .map(|f| f.content.chars().take(150).collect())
            .collect()
    }

    /// Build evidence list from findings
    pub fn build_evidence(&self, findings: &[Finding]) -> Vec<Evidence> {
        findings
            .iter()
            .filter(|f| f.source_path.is_some())
            .take(10)
            .map(|f| Evidence {
                content: f.content.chars().take(200).collect(),
                source_path: f.source_path.clone().unwrap(),
                line_range: f.line_range,
                relevance: f.relevance,
            })
            .collect()
    }

    /// Create empty result
    pub fn empty_result(&self, results: &[ChunkResult], duration_ms: u64) -> RlmResult {
        RlmResult {
            success: false,
            answer: "No relevant information found in the provided context.".to_string(),
            key_insights: vec![],
            evidence: vec![],
            chunks_explored: results.len(),
            max_depth_reached: results.iter().map(|r| r.depth).max().unwrap_or(0),
            total_tokens: results.iter().map(|r| r.tokens_used).sum(),
            confidence: 0.0,
            duration_ms,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_finding(content: &str, confidence: f32, finding_type: FindingType) -> Finding {
        Finding {
            content: content.to_string(),
            relevance: confidence,
            source_chunk_id: "test".to_string(),
            source_path: Some(PathBuf::from("test.rs")),
            line_range: Some((1, 10)),
            finding_type,
            confidence,
        }
    }

    fn make_chunk_result(findings: Vec<Finding>, depth: u32) -> ChunkResult {
        ChunkResult {
            chunk_id: format!("chunk-{}", depth),
            success: true,
            findings,
            sub_results: vec![],
            depth,
            tokens_used: 100,
            error: None,
        }
    }

    // ==================== Aggregator Creation Tests ====================

    #[test]
    fn test_aggregator_new() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);
        assert_eq!(aggregator.working_dir, PathBuf::from("."));
        assert!((aggregator.dedup_threshold - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn test_aggregator_with_custom_config() {
        let mut config = RlmConfig::default();
        config.explorer_model = Some("custom-model".to_string());
        let aggregator = Aggregator::new(PathBuf::from("/tmp"), config);
        assert_eq!(aggregator.model, Some("custom-model".to_string()));
        assert_eq!(aggregator.working_dir, PathBuf::from("/tmp"));
    }

    // ==================== Deduplication Tests ====================

    #[test]
    fn test_deduplicate_identical() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("This is a finding about auth", 0.9, FindingType::Answer),
            make_finding("This is a finding about auth", 0.8, FindingType::Answer),
            make_finding("Different finding", 0.7, FindingType::Evidence),
        ];

        let deduped = aggregator.deduplicate(findings);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_deduplicate_case_insensitive() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("AUTH HANDLER function", 0.9, FindingType::Answer),
            make_finding("auth handler function", 0.8, FindingType::Answer),
        ];

        let deduped = aggregator.deduplicate(findings);
        assert_eq!(deduped.len(), 1);
    }

    #[test]
    fn test_deduplicate_empty() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings: Vec<Finding> = vec![];
        let deduped = aggregator.deduplicate(findings);
        assert!(deduped.is_empty());
    }

    #[test]
    fn test_deduplicate_all_unique() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("First unique finding", 0.9, FindingType::Answer),
            make_finding("Second unique finding", 0.8, FindingType::Evidence),
            make_finding("Third unique finding", 0.7, FindingType::Related),
        ];

        let deduped = aggregator.deduplicate(findings);
        assert_eq!(deduped.len(), 3);
    }

    #[test]
    fn test_deduplicate_ignores_punctuation() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("Finding, with: punctuation!", 0.9, FindingType::Answer),
            make_finding("Finding with punctuation", 0.8, FindingType::Answer),
        ];

        let deduped = aggregator.deduplicate(findings);
        assert_eq!(deduped.len(), 1);
    }

    // ==================== Conflict Resolution Tests ====================

    #[test]
    fn test_resolve_conflicts_ordering() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("Answer 1", 0.9, FindingType::Answer),
            make_finding("Evidence 1", 0.8, FindingType::Evidence),
            make_finding("Conflict 1", 0.7, FindingType::Conflict),
        ];

        let resolved = aggregator.resolve_conflicts(findings);
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].finding_type, FindingType::Answer);
        assert_eq!(resolved[1].finding_type, FindingType::Evidence);
        assert_eq!(resolved[2].finding_type, FindingType::Conflict);
    }

    #[test]
    fn test_resolve_conflicts_sorts_answers_by_confidence() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("Low confidence answer", 0.5, FindingType::Answer),
            make_finding("High confidence answer", 0.9, FindingType::Answer),
            make_finding("Medium confidence answer", 0.7, FindingType::Answer),
        ];

        let resolved = aggregator.resolve_conflicts(findings);
        assert_eq!(resolved.len(), 3);
        assert!((resolved[0].confidence - 0.9).abs() < f32::EPSILON);
        assert!((resolved[1].confidence - 0.7).abs() < f32::EPSILON);
        assert!((resolved[2].confidence - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_resolve_conflicts_handles_related_and_context() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("Related info", 0.6, FindingType::Related),
            make_finding("Context info", 0.5, FindingType::Context),
            make_finding("Answer", 0.9, FindingType::Answer),
        ];

        let resolved = aggregator.resolve_conflicts(findings);
        assert_eq!(resolved.len(), 3);
        // Answer should be first
        assert_eq!(resolved[0].finding_type, FindingType::Answer);
    }

    #[test]
    fn test_resolve_conflicts_empty() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings: Vec<Finding> = vec![];
        let resolved = aggregator.resolve_conflicts(findings);
        assert!(resolved.is_empty());
    }

    // ==================== Concatenate Findings Tests ====================

    #[test]
    fn test_concatenate_findings() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("First answer", 0.9, FindingType::Answer),
            make_finding("Second answer", 0.8, FindingType::Answer),
        ];

        let result = aggregator.concatenate_findings(&findings);
        assert!(result.contains("First answer"));
        assert!(result.contains("Second answer"));
    }

    #[test]
    fn test_concatenate_findings_filters_non_answers() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("Answer text", 0.9, FindingType::Answer),
            make_finding("Evidence text", 0.8, FindingType::Evidence),
            make_finding("Related text", 0.7, FindingType::Related),
        ];

        let result = aggregator.concatenate_findings(&findings);
        assert!(result.contains("Answer text"));
        assert!(!result.contains("Evidence text"));
        assert!(!result.contains("Related text"));
    }

    #[test]
    fn test_concatenate_findings_limits_to_five() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings: Vec<Finding> = (0..10)
            .map(|i| make_finding(&format!("Answer {}", i), 0.9, FindingType::Answer))
            .collect();

        let result = aggregator.concatenate_findings(&findings);
        // Count occurrences of "Answer"
        let count = result.matches("Answer").count();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_concatenate_findings_empty() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings: Vec<Finding> = vec![];
        let result = aggregator.concatenate_findings(&findings);
        assert!(result.is_empty());
    }

    // ==================== Extract Insights Tests ====================

    #[test]
    fn test_extract_insights() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("High confidence insight", 0.9, FindingType::Answer),
            make_finding("Low confidence", 0.3, FindingType::Evidence),
        ];

        let insights = aggregator.extract_insights(&findings);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0], "High confidence insight");
    }

    #[test]
    fn test_extract_insights_threshold() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("Above threshold", 0.7, FindingType::Answer),
            make_finding("Below threshold", 0.69, FindingType::Answer),
        ];

        let insights = aggregator.extract_insights(&findings);
        assert_eq!(insights.len(), 1);
    }

    #[test]
    fn test_extract_insights_limits_to_five() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings: Vec<Finding> = (0..10)
            .map(|i| make_finding(&format!("Insight {}", i), 0.9, FindingType::Answer))
            .collect();

        let insights = aggregator.extract_insights(&findings);
        assert_eq!(insights.len(), 5);
    }

    #[test]
    fn test_extract_insights_truncates_long_content() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let long_content = "a".repeat(200);
        let findings = vec![make_finding(&long_content, 0.9, FindingType::Answer)];

        let insights = aggregator.extract_insights(&findings);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].len(), 150);
    }

    // ==================== Build Evidence Tests ====================

    #[test]
    fn test_build_evidence() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("Evidence content", 0.8, FindingType::Evidence),
            make_finding("Another finding", 0.7, FindingType::Answer),
        ];

        let evidence = aggregator.build_evidence(&findings);
        assert_eq!(evidence.len(), 2);
        assert_eq!(evidence[0].source_path, PathBuf::from("test.rs"));
    }

    #[test]
    fn test_build_evidence_filters_without_path() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let finding_with_path = make_finding("Has path", 0.8, FindingType::Evidence);
        let mut finding_without_path = make_finding("No path", 0.8, FindingType::Evidence);
        finding_without_path.source_path = None;

        let findings = vec![finding_with_path, finding_without_path];

        let evidence = aggregator.build_evidence(&findings);
        assert_eq!(evidence.len(), 1);
    }

    #[test]
    fn test_build_evidence_limits_to_ten() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings: Vec<Finding> = (0..15)
            .map(|i| make_finding(&format!("Finding {}", i), 0.8, FindingType::Evidence))
            .collect();

        let evidence = aggregator.build_evidence(&findings);
        assert_eq!(evidence.len(), 10);
    }

    #[test]
    fn test_build_evidence_truncates_content() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let long_content = "x".repeat(300);
        let findings = vec![make_finding(&long_content, 0.8, FindingType::Evidence)];

        let evidence = aggregator.build_evidence(&findings);
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].content.len(), 200);
    }

    // ==================== Collect Findings Tests ====================

    #[test]
    fn test_collect_findings_flat() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("Finding 1", 0.9, FindingType::Answer),
            make_finding("Finding 2", 0.8, FindingType::Evidence),
        ];
        let result = make_chunk_result(findings, 1);

        let collected = aggregator.collect_findings(&result);
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_collect_findings_nested() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let sub_findings = vec![make_finding("Sub finding", 0.7, FindingType::Related)];
        let sub_result = make_chunk_result(sub_findings, 2);

        let parent_findings = vec![make_finding("Parent finding", 0.9, FindingType::Answer)];
        let mut parent_result = make_chunk_result(parent_findings, 1);
        parent_result.sub_results = vec![sub_result];

        let collected = aggregator.collect_findings(&parent_result);
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_collect_findings_deeply_nested() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        // Create 3 levels deep
        let level3 = make_chunk_result(
            vec![make_finding("Level 3", 0.5, FindingType::Context)],
            3,
        );
        let mut level2 = make_chunk_result(
            vec![make_finding("Level 2", 0.6, FindingType::Related)],
            2,
        );
        level2.sub_results = vec![level3];

        let mut level1 = make_chunk_result(
            vec![make_finding("Level 1", 0.9, FindingType::Answer)],
            1,
        );
        level1.sub_results = vec![level2];

        let collected = aggregator.collect_findings(&level1);
        assert_eq!(collected.len(), 3);
    }

    // ==================== Empty Result Tests ====================

    #[test]
    fn test_empty_result() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let results = vec![
            make_chunk_result(vec![], 1),
            make_chunk_result(vec![], 2),
        ];

        let result = aggregator.empty_result(&results, 1000);
        assert!(!result.success);
        assert_eq!(result.chunks_explored, 2);
        assert_eq!(result.max_depth_reached, 2);
        assert!((result.confidence - 0.0).abs() < f32::EPSILON);
        assert_eq!(result.duration_ms, 1000);
    }

    #[test]
    fn test_empty_result_no_chunks() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let results: Vec<ChunkResult> = vec![];
        let result = aggregator.empty_result(&results, 500);
        assert_eq!(result.chunks_explored, 0);
        assert_eq!(result.max_depth_reached, 0);
    }

    // ==================== Aggregate Tests ====================

    #[test]
    fn test_aggregate_empty_findings() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let results = vec![make_chunk_result(vec![], 1)];
        let result = aggregator
            .aggregate(&results, "test query", AggregationStrategy::Concatenate, 1000)
            .unwrap();

        assert!(!result.success);
        assert!(result.answer.contains("No relevant information"));
    }

    #[test]
    fn test_aggregate_concatenate_strategy() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("First answer", 0.9, FindingType::Answer),
            make_finding("Second answer", 0.8, FindingType::Answer),
        ];
        let results = vec![make_chunk_result(findings, 1)];

        let result = aggregator
            .aggregate(&results, "test query", AggregationStrategy::Concatenate, 1000)
            .unwrap();

        assert!(result.success);
        assert!(result.answer.contains("First answer"));
        assert!(result.answer.contains("Second answer"));
    }

    #[test]
    fn test_aggregate_calculates_metrics() {
        let config = RlmConfig::default();
        let aggregator = Aggregator::new(PathBuf::from("."), config);

        let findings = vec![
            make_finding("Finding 1", 0.9, FindingType::Answer),
            make_finding("Finding 2", 0.7, FindingType::Answer),
        ];
        let results = vec![
            ChunkResult {
                chunk_id: "chunk-1".to_string(),
                success: true,
                findings: findings.clone(),
                sub_results: vec![],
                depth: 1,
                tokens_used: 100,
                error: None,
            },
            ChunkResult {
                chunk_id: "chunk-2".to_string(),
                success: true,
                findings: vec![],
                sub_results: vec![],
                depth: 3,
                tokens_used: 200,
                error: None,
            },
        ];

        let result = aggregator
            .aggregate(&results, "test", AggregationStrategy::Concatenate, 2000)
            .unwrap();

        assert_eq!(result.chunks_explored, 2);
        assert_eq!(result.max_depth_reached, 3);
        assert_eq!(result.total_tokens, 300);
        assert_eq!(result.duration_ms, 2000);
    }

    // ==================== AggregationStrategy Tests ====================

    #[test]
    fn test_aggregation_strategy_default() {
        let strategy = AggregationStrategy::default();
        assert_eq!(strategy, AggregationStrategy::Synthesize);
    }

    #[test]
    fn test_aggregation_strategy_equality() {
        assert_eq!(
            AggregationStrategy::Concatenate,
            AggregationStrategy::Concatenate
        );
        assert_ne!(
            AggregationStrategy::Concatenate,
            AggregationStrategy::Synthesize
        );
        assert_ne!(
            AggregationStrategy::Synthesize,
            AggregationStrategy::Hierarchical
        );
    }

    #[test]
    fn test_aggregation_strategy_clone() {
        let strategy = AggregationStrategy::Hierarchical;
        let cloned = strategy.clone();
        assert_eq!(strategy, cloned);
    }

    #[test]
    fn test_aggregation_strategy_debug() {
        let strategy = AggregationStrategy::Synthesize;
        let debug_str = format!("{:?}", strategy);
        assert!(debug_str.contains("Synthesize"));
    }
}
