//! Jarvis-V2: Ralph-style autonomous AI agent framework
//!
//! An iterative task execution system inspired by the Ralph-Wiggum pattern.
//! Executes tasks in a loop until verified complete, detecting shortcuts
//! and incomplete work.
//!
//! Features:
//! - Iterative execution until verified complete
//! - Completion promise detection (Ralph-style)
//! - Signal file-based completion (.jarvis-satisfied, .jarvis-blocked)
//! - Checkpoint save/restore for resumable execution
//! - Graceful signal handling (Ctrl+C saves checkpoint)
//! - Progress callbacks with colored output

use anyhow::Result;
use clap::{Parser, ValueEnum};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use jarvis_v2::{
    brain::TaskAnalyzer,
    extend::{CapabilityRegistry, GapDetector, HotLoader, SynthesisEngine},
    subagent, swarm::SwarmCoordinator, task_loop::TaskLoop, verify::Verifier,
    AntiLazinessConfig, Config, ConsoleProgressReporter, EvidenceLevel, ExecutionMode,
    JsonProgressReporter, ProgressCallback, ShutdownSignal, SubagentConfig,
};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{warn, Level};
use tracing_subscriber::FmtSubscriber;

/// Output format for results
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    /// Human-readable text output (default)
    #[default]
    Text,
    /// JSON output for programmatic consumption
    Json,
    /// Markdown formatted output
    Markdown,
}

/// Checkpoint state for saving/resuming execution
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct Checkpoint {
    /// The task being executed
    pub task: String,
    /// Current iteration number
    pub iteration: u32,
    /// Maximum iterations allowed
    pub max_iterations: u32,
    /// Working directory path
    pub working_dir: PathBuf,
    /// Completion promise if any
    pub completion_promise: Option<String>,
    /// Model being used
    pub model: Option<String>,
    /// Whether task has completed
    pub completed: bool,
    /// Last error if any
    pub last_error: Option<String>,
}

/// Result output structure for JSON/structured output
#[derive(Debug, Serialize)]
struct TaskOutput {
    success: bool,
    iterations: u32,
    summary: Option<String>,
    error: Option<String>,
}

/// Jarvis-V2: Autonomous AI agent with verified completion
///
/// Jarvis-V2 is an LLM-driven autonomous agent framework.
/// Agents run until their task is complete - no arbitrary iteration limits.
///
/// # Examples
///
/// Basic usage:
///   jarvis-v2 "Write a hello world program in Rust"
///
/// With model selection:
///   jarvis-v2 "Refactor the auth module" -m claude-sonnet-4-20250514
///
/// Swarm mode with parallel agents:
///   jarvis-v2 "Build a full feature" -s 3
#[derive(Parser, Debug)]
#[command(name = "jarvis-v2")]
#[command(author = "Jarvis AI Framework")]
#[command(version = "0.1.0")]
#[command(about = "LLM-driven autonomous task execution")]
#[command(
    long_about = "Jarvis-V2 is an autonomous AI agent framework that executes tasks \
until verified complete. Agents run until done - no iteration limits. \
Uses Claude as the underlying LLM with built-in verification."
)]
struct Args {
    /// The task to execute (required unless resuming from checkpoint or using registration/capability/rlm/overseer commands)
    #[arg(required_unless_present_any = ["checkpoint", "register_subagent", "unregister_subagent", "list_capabilities", "prune_capabilities", "rlm", "overseer_only"])]
    task: Option<String>,

    /// Working directory for task execution
    ///
    /// All file operations will be relative to this directory.
    /// Defaults to the current working directory.
    #[arg(short = 'd', long, value_name = "PATH")]
    dir: Option<PathBuf>,

    /// Completion promise string to detect success
    ///
    /// A regex pattern that when matched in the output indicates task completion.
    #[arg(short = 'p', long, value_name = "PATTERN")]
    promise: Option<String>,

    /// Dry run - analyze task without executing
    #[arg(long)]
    dry_run: bool,

    /// Model to use for Claude
    ///
    /// Specify a Claude model ID (e.g., claude-opus-4-5-20251101, claude-sonnet-4-20250514).
    #[arg(short = 'm', long, value_name = "MODEL_ID")]
    model: Option<String>,

    /// Output format for results (text, json, or markdown)
    #[arg(short = 'o', long, value_enum, default_value_t = OutputFormat::Text, value_name = "FORMAT")]
    output_format: OutputFormat,

    /// Path to checkpoint file for saving or resuming state
    #[arg(short = 'c', long, value_name = "FILE")]
    checkpoint: Option<PathBuf>,

    /// Disable colored output
    #[arg(long)]
    no_color: bool,

    /// Force swarm mode with N parallel agents
    #[arg(short = 's', long, value_name = "N")]
    swarm: Option<u32>,

    /// Enable anti-laziness enforcement
    ///
    /// Detects mock data patterns, requires evidence in completion claims,
    /// and injects quality standards into prompts.
    #[arg(long)]
    anti_laziness: bool,

    /// Disable anti-laziness enforcement (overrides config)
    #[arg(long, conflicts_with = "anti_laziness")]
    no_anti_laziness: bool,

    /// Required evidence level: minimal, adequate, strong
    ///
    /// Controls how much evidence is required in completion claims.
    /// - minimal: Just need files_modified to be non-empty
    /// - adequate: Need files + summary (default)
    /// - strong: Need files + substantial summary + confidence >= 0.9
    #[arg(long, value_name = "LEVEL", default_value = "adequate")]
    evidence_level: String,

    /// Run as a Claude Code subagent
    ///
    /// When enabled, jarvis-v2 runs in subagent mode with context compression.
    /// Output is compressed to hide iteration details from the parent agent.
    #[arg(long)]
    subagent_mode: bool,

    /// Subagent configuration (JSON) - internal use
    #[arg(long, value_name = "JSON", hide = true)]
    subagent_config: Option<String>,

    /// Register jarvis-v2 as a Claude Code subagent
    ///
    /// Adds jarvis-v2 to the Claude Code settings file so it can be
    /// invoked as a specialized subagent.
    #[arg(long)]
    register_subagent: bool,

    /// Unregister jarvis-v2 from Claude Code
    ///
    /// Removes jarvis-v2 from the Claude Code settings file.
    #[arg(long)]
    unregister_subagent: bool,

    /// Registration scope: "global" or "project"
    ///
    /// - global: Register in ~/.claude/settings.json
    /// - project: Register in .claude/settings.json in working directory
    #[arg(long, value_name = "SCOPE", default_value = "project")]
    registration_scope: String,

    /// Enable self-extending capabilities (DEFAULT: enabled)
    ///
    /// Jarvis-v2 detects capability gaps during execution and synthesizes
    /// missing capabilities (MCPs, Skills, Agents). This is enabled by default.
    /// Use --no-self-extend to disable.
    #[arg(long)]
    self_extend: bool,

    /// Disable self-extending capabilities
    #[arg(long)]
    no_self_extend: bool,

    /// Show detected capability gaps without synthesizing
    ///
    /// Useful for understanding what capabilities would be beneficial
    /// without actually creating them.
    #[arg(long)]
    detect_gaps_only: bool,

    /// List registered capabilities from the capability registry
    #[arg(long)]
    list_capabilities: bool,

    /// Prune unused synthesized capabilities older than N days
    #[arg(long, value_name = "DAYS")]
    prune_capabilities: Option<u32>,

    /// Disable the Creative Overseer (proactive capability brainstorming)
    ///
    /// By default, the Creative Overseer runs in parallel with swarm execution
    /// to proactively brainstorm and synthesize new capabilities before gaps
    /// are encountered. Use this flag to disable this behavior.
    #[arg(long)]
    no_overseer: bool,

    /// Run Creative Overseer standalone (capability synthesis only)
    ///
    /// Runs the Creative Overseer in a loop to continuously brainstorm and
    /// synthesize new capabilities (Skills, MCPs, Agents) WITHOUT executing
    /// any tasks or modifying source code. Use this to build up capabilities.
    #[arg(long)]
    overseer_only: bool,

    /// Model to use for Creative Overseer brainstorming
    ///
    /// Default is "opus" for high-quality capability ideation. Options: haiku, sonnet, opus
    #[arg(long, value_name = "MODEL", default_value = "opus")]
    overseer_model: String,

    /// Enable RLM (Recursive Language Model) mode
    ///
    /// RLM enables processing inputs far exceeding context windows through
    /// recursive decomposition. Treats large contexts as explorable environments.
    #[arg(long)]
    rlm: bool,

    /// Context path for RLM (file or directory)
    ///
    /// The path to analyze. Can be a single file or directory.
    /// Defaults to ./src if not specified.
    #[arg(long, value_name = "PATH")]
    rlm_context: Option<PathBuf>,

    /// Maximum recursion depth for RLM
    ///
    /// Controls how deep the recursive exploration can go.
    /// Higher values explore more deeply but use more tokens.
    #[arg(long, default_value = "5")]
    rlm_depth: u32,

    /// RLM mode: oneshot or continuous
    ///
    /// - oneshot: Run a single query against the context
    /// - continuous: Run as a daemon, watching for query files
    #[arg(long, default_value = "oneshot")]
    rlm_mode: String,
}

fn create_spinner() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

fn print_header(no_color: bool) {
    let version = env!("CARGO_PKG_VERSION");
    let header = format!("Jarvis-V2 v{}", version);

    if no_color {
        println!("{}", header);
        println!("{}", "=".repeat(header.len()));
    } else {
        println!("{}", header.bright_cyan().bold());
        println!("{}", "═".repeat(header.len()).cyan());
    }
}

fn print_analysis(
    analysis: &jarvis_v2::brain::TaskAnalysis,
    format: OutputFormat,
    no_color: bool,
    swarm_override: Option<u32>,
) -> Result<()> {
    // Determine effective execution mode (CLI flag overrides analysis)
    let effective_mode = if let Some(agent_count) = swarm_override {
        ExecutionMode::Swarm { agent_count }
    } else {
        analysis.mode
    };

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&analysis)?);
        }
        OutputFormat::Markdown => {
            println!("## Task Analysis\n");
            println!("| Metric | Value |");
            println!("|--------|-------|");
            println!("| Complexity | {}/10 |", analysis.complexity_score);
            println!("| Needs Iteration | {} |", analysis.needs_iteration);
            println!("| Needs Deep Reasoning | {} |", analysis.needs_ultrathink);
            println!("| Difficulty | {:?} |", analysis.difficulty);
        }
        OutputFormat::Text => {
            if no_color {
                println!("\nTask Analysis:");
                println!("  Complexity: {}/10", analysis.complexity_score);
                println!("  Needs iteration: {}", analysis.needs_iteration);
                println!("  Needs deep reasoning: {}", analysis.needs_ultrathink);
                println!("  Estimated difficulty: {:?}", analysis.difficulty);
                println!(
                    "  Execution mode: {}",
                    match effective_mode {
                        ExecutionMode::Single => "Single agent".to_string(),
                        ExecutionMode::Swarm { agent_count } =>
                            format!("Swarm ({} agents)", agent_count),
                    }
                );
            } else {
                println!("\n{}", "Task Analysis:".yellow().bold());
                println!(
                    "  {} {}/10",
                    "Complexity:".white(),
                    format!("{}", analysis.complexity_score).bright_white()
                );
                println!(
                    "  {} {}",
                    "Needs iteration:".white(),
                    if analysis.needs_iteration {
                        "yes".green()
                    } else {
                        "no".white()
                    }
                );
                println!(
                    "  {} {}",
                    "Needs deep reasoning:".white(),
                    if analysis.needs_ultrathink {
                        "yes".green()
                    } else {
                        "no".white()
                    }
                );
                println!(
                    "  {} {:?}",
                    "Estimated difficulty:".white(),
                    analysis.difficulty
                );
                println!(
                    "  {} {}",
                    "Execution mode:".white(),
                    match effective_mode {
                        ExecutionMode::Single => "Single agent".white(),
                        ExecutionMode::Swarm { agent_count } =>
                            format!("🐝 Swarm ({} agents)", agent_count).cyan().bold(),
                    }
                );
            }
        }
    }
    Ok(())
}

fn print_result(result: &jarvis_v2::TaskResult, format: OutputFormat, no_color: bool) {
    let output = TaskOutput {
        success: result.success,
        iterations: result.iterations,
        summary: result.summary.clone(),
        error: result.error.clone(),
    };

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Markdown => {
            if result.success {
                println!("\n## Result: Success\n");
                println!("Completed after **{}** iterations.\n", result.iterations);
                if let Some(ref summary) = result.summary {
                    println!("### Summary\n\n{}", summary);
                }
            } else {
                println!("\n## Result: Failed\n");
                println!("Failed after **{}** iterations.\n", result.iterations);
                if let Some(ref error) = result.error {
                    println!("### Error\n\n```\n{}\n```", error);
                }
            }
        }
        OutputFormat::Text => {
            if result.success {
                let msg = format!(
                    "\n✅ Task completed successfully after {} iterations",
                    result.iterations
                );
                if no_color {
                    println!("{}", msg);
                } else {
                    println!("{}", msg.green().bold());
                }
                if let Some(ref summary) = result.summary {
                    if no_color {
                        println!("\nSummary:\n{}", summary);
                    } else {
                        println!("\n{}\n{}", "Summary:".yellow().bold(), summary);
                    }
                }
            } else {
                let msg = format!("\n❌ Task failed after {} iterations", result.iterations);
                if no_color {
                    println!("{}", msg);
                } else {
                    println!("{}", msg.red().bold());
                }
                if let Some(ref error) = result.error {
                    if no_color {
                        println!("\nError: {}", error);
                    } else {
                        println!("\n{} {}", "Error:".red().bold(), error);
                    }
                }
            }
        }
    }
}

fn save_checkpoint(checkpoint: &Checkpoint, path: &PathBuf) -> Result<()> {
    let json = serde_json::to_string_pretty(checkpoint)?;
    fs::write(path, json)?;
    Ok(())
}

fn load_checkpoint(path: &PathBuf) -> Result<Checkpoint> {
    let json = fs::read_to_string(path)?;
    let checkpoint: Checkpoint = serde_json::from_str(&json)?;
    Ok(checkpoint)
}

/// Run RLM mode (oneshot or continuous)
fn run_rlm(
    args: &Args,
    working_dir: PathBuf,
    shutdown_signal: ShutdownSignal,
) -> Result<()> {
    use jarvis_v2::rlm::{RlmConfig, RlmContext, RlmDaemon, RlmExecutor};
    use tracing::info;

    let context_path = args
        .rlm_context
        .clone()
        .unwrap_or_else(|| working_dir.join("src"));

    let config = RlmConfig {
        max_depth: args.rlm_depth,
        ..Default::default()
    };

    match args.rlm_mode.as_str() {
        "continuous" => {
            info!("Starting RLM in continuous mode");
            let mut daemon = RlmDaemon::new(config, working_dir, context_path, shutdown_signal);
            daemon.run()?;
        }
        "oneshot" | _ => {
            // Get query from remaining args or prompt
            let query = args.task.clone().unwrap_or_else(|| {
                eprintln!("Usage: jarvis-v2 --rlm --rlm-context ./src \"your query\"");
                std::process::exit(1);
            });

            info!("Running RLM one-shot query: {}", query);

            let context = if context_path.is_dir() {
                RlmContext::from_directory(&context_path, config.clone())?
            } else {
                RlmContext::from_file(&context_path, config.clone())?
            };

            let mut executor = RlmExecutor::new(config, working_dir);
            let result = executor.execute_on_context(&query, &context)?;

            // Output result
            println!("\n=== RLM Result ===");
            println!("Answer: {}", result.answer);
            println!("\nKey Insights:");
            for insight in &result.key_insights {
                println!("  - {}", insight);
            }
            println!("\nEvidence ({} sources):", result.evidence.len());
            for ev in result.evidence.iter().take(5) {
                println!("  - {}:{:?}", ev.source_path.display(), ev.line_range);
            }
            println!(
                "\nStats: {} chunks, depth {}, {} tokens, {:.0}% confidence",
                result.chunks_explored,
                result.max_depth_reached,
                result.total_tokens,
                result.confidence * 100.0
            );

            if !result.success {
                if let Some(err) = result.error {
                    eprintln!("\nError: {}", err);
                }
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle color output settings
    if args.no_color {
        colored::control::set_override(false);
    }

    // Initialize logging - quiet by default, only show warnings
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::WARN)
        .with_target(false)
        .compact()
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    // Set up signal handling for graceful shutdown
    let shutdown_signal = ShutdownSignal::new();
    if let Err(e) = shutdown_signal.install_handlers() {
        warn!("Failed to install signal handlers: {}", e);
    }

    // Determine working directory early for registration commands
    let working_dir = args
        .dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Handle registration commands
    if args.register_subagent {
        let scope = match args.registration_scope.as_str() {
            "global" => subagent::RegistrationScope::Global,
            _ => subagent::RegistrationScope::Project(working_dir.clone()),
        };
        subagent::register_subagent(scope)?;
        println!("jarvis-v2 registered as Claude Code subagent");
        return Ok(());
    }

    if args.unregister_subagent {
        let scope = match args.registration_scope.as_str() {
            "global" => subagent::RegistrationScope::Global,
            _ => subagent::RegistrationScope::Project(working_dir.clone()),
        };
        subagent::unregister_subagent(scope)?;
        println!("jarvis-v2 unregistered from Claude Code");
        return Ok(());
    }

    // Handle capability registry commands
    if args.list_capabilities {
        let registry = CapabilityRegistry::load().unwrap_or_default();
        println!("Registered Capabilities:");
        println!("========================");
        println!("\nMCPs ({}):", registry.mcps.len());
        for mcp in &registry.mcps {
            let synth = if mcp.synthesized { " [synthesized]" } else { "" };
            println!("  - {} (uses: {}, success: {:.0}%){}",
                mcp.name, mcp.usage_count, mcp.success_rate * 100.0, synth);
        }
        println!("\nSkills ({}):", registry.skills.len());
        for skill in &registry.skills {
            let synth = if skill.synthesized { " [synthesized]" } else { "" };
            println!("  - {} (uses: {}, success: {:.0}%){}",
                skill.name, skill.usage_count, skill.success_rate * 100.0, synth);
        }
        println!("\nAgents ({}):", registry.agents.len());
        for agent in &registry.agents {
            let synth = if agent.synthesized { " [synthesized]" } else { "" };
            println!("  - {} (uses: {}, success: {:.0}%){}",
                agent.name, agent.usage_count, agent.success_rate * 100.0, synth);
        }
        println!("\nTotal: {} capabilities ({} synthesized)",
            registry.total_count(), registry.synthesized_count());
        return Ok(());
    }

    if let Some(days) = args.prune_capabilities {
        let mut registry = CapabilityRegistry::load().unwrap_or_default();
        let pruned = registry.prune_unused(days);
        if pruned.is_empty() {
            println!("No capabilities to prune (none older than {} days)", days);
        } else {
            println!("Pruned {} unused capabilities:", pruned.len());
            for cap in &pruned {
                println!("  - {} ({:?})", cap.name, cap.capability_type);
            }
            registry.save()?;
            println!("Registry updated.");
        }
        return Ok(());
    }

    // Handle RLM mode
    if args.rlm {
        return run_rlm(&args, working_dir, shutdown_signal);
    }

    // Handle overseer-only mode (standalone capability synthesis)
    if args.overseer_only {
        return run_overseer_only(&args, working_dir, shutdown_signal).await;
    }

    // Handle subagent mode
    if args.subagent_mode {
        let task = args
            .task
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Task is required in subagent mode"))?;

        let subagent_config: SubagentConfig = if let Some(ref config_json) = args.subagent_config {
            serde_json::from_str(config_json)?
        } else {
            SubagentConfig {
                task: task.clone(),
                parent_session_id: None,
                current_depth: 0,
                ..Default::default()
            }
        };

        let executor =
            subagent::SubagentExecutor::new(subagent_config.clone(), working_dir.clone())
                .with_model(args.model.clone());

        let result = executor.execute(&task).await?;

        // Output compressed result
        let output = executor.output_result(&result);
        println!("{}", output);

        if result.success {
            return Ok(());
        } else {
            std::process::exit(1);
        }
    }

    // Check for checkpoint resume
    let (task, starting_iteration, checkpoint_path) = if let Some(ref cp_path) = args.checkpoint {
        if cp_path.exists() {
            let checkpoint = load_checkpoint(cp_path)?;
            if checkpoint.completed {
                println!(
                    "{}",
                    "Checkpoint indicates task already completed.".yellow()
                );
                return Ok(());
            }
            let msg = format!(
                "📂 Resuming from checkpoint at iteration {}",
                checkpoint.iteration
            );
            if args.no_color {
                println!("{}", msg);
            } else {
                println!("{}", msg.cyan());
            }
            (checkpoint.task, checkpoint.iteration, Some(cp_path.clone()))
        } else {
            let task = args.task.clone().ok_or_else(|| {
                anyhow::anyhow!("Task is required when creating a new checkpoint")
            })?;
            (task, 0, Some(cp_path.clone()))
        }
    } else {
        let task = args
            .task
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Task is required"))?;
        (task, 0, None)
    };

    // Only print header for non-JSON output
    if !matches!(args.output_format, OutputFormat::Json) {
        print_header(args.no_color);
    }

    // Parse evidence level
    let evidence_level = args.evidence_level.parse::<EvidenceLevel>().unwrap_or_else(|e| {
        eprintln!("Warning: {}", e);
        EvidenceLevel::Adequate
    });

    // Configure anti-laziness enforcement
    let anti_laziness = AntiLazinessConfig {
        enabled: args.anti_laziness && !args.no_anti_laziness,
        mock_detection_enabled: true,
        evidence_required: true,
        min_confidence: 0.8,
        evidence_level,
    };

    // Build configuration - agents run until done
    let config = Config {
        working_dir: working_dir.clone(),
        completion_promise: args.promise.clone(),
        model: args.model.clone(),
        dry_run: args.dry_run,
        anti_laziness,
    };

    // Save initial checkpoint if path provided
    if let Some(ref cp_path) = checkpoint_path {
        let checkpoint = Checkpoint {
            task: task.clone(),
            iteration: starting_iteration,
            max_iterations: u32::MAX,
            working_dir: working_dir.clone(),
            completion_promise: args.promise.clone(),
            model: args.model.clone(),
            completed: false,
            last_error: None,
        };
        save_checkpoint(&checkpoint, cp_path)?;
    }

    // Analyze the task
    let spinner = create_spinner();
    spinner.set_message("Analyzing task...");

    let analyzer = TaskAnalyzer::new();
    let analysis = analyzer.analyze(&task)?;

    spinner.finish_and_clear();

    print_analysis(&analysis, args.output_format, args.no_color, args.swarm)?;

    if args.dry_run {
        match args.output_format {
            OutputFormat::Json => {
                // Already printed analysis as JSON
            }
            OutputFormat::Markdown => {
                println!("\n---\n*Dry run - no changes made*");
            }
            OutputFormat::Text => {
                if args.no_color {
                    println!("\n=== DRY RUN - Analysis Only ===");
                } else {
                    println!("\n{}", "=== DRY RUN - Analysis Only ===".yellow().bold());
                }
            }
        }
        return Ok(());
    }

    // Create progress callback based on output format
    let progress_callback: Arc<dyn ProgressCallback> = match args.output_format {
        OutputFormat::Json => Arc::new(JsonProgressReporter),
        _ => Arc::new(ConsoleProgressReporter::new(!args.no_color, false)),
    };

    // Determine execution mode (CLI flag overrides analysis)
    let execution_mode = if let Some(agent_count) = args.swarm {
        ExecutionMode::Swarm { agent_count }
    } else {
        analysis.mode
    };

    // Check for detect-gaps-only mode
    if args.detect_gaps_only {
        let analyzer = TaskAnalyzer::new().with_working_dir(&working_dir);
        let gaps = analyzer.detect_capability_gaps(&task);

        if gaps.is_empty() {
            println!("No capability gaps detected in the task description.");
        } else {
            println!("Detected Capability Gaps:");
            println!("==========================");
            for (i, gap) in gaps.iter().enumerate() {
                println!("\n{}. {} ({:?})", i + 1, gap.name(), gap.capability_type());
                match gap {
                    jarvis_v2::extend::CapabilityGap::Mcp { purpose, api_hint, .. } => {
                        println!("   Purpose: {}", purpose);
                        if let Some(hint) = api_hint {
                            println!("   API Hint: {}", hint);
                        }
                    }
                    jarvis_v2::extend::CapabilityGap::Skill { trigger, purpose, .. } => {
                        println!("   Trigger: {}", trigger);
                        println!("   Purpose: {}", purpose);
                    }
                    jarvis_v2::extend::CapabilityGap::Agent { specialization, .. } => {
                        println!("   Specialization: {}", specialization);
                    }
                }
            }
        }
        return Ok(());
    }

    // Self-extend is enabled by default, use --no-self-extend to disable
    let self_extend_enabled = !args.no_self_extend;

    // Execute based on mode
    let result = match execution_mode {
        ExecutionMode::Single => {
            // Single agent execution
            let verifier = Verifier::new(config.completion_promise.clone());
            let mut task_loop = TaskLoop::new(config.clone(), verifier)
                .with_shutdown_signal(shutdown_signal.clone())
                .with_progress_callback(progress_callback);

            // Enable self-extending capabilities if requested
            if self_extend_enabled {
                task_loop = task_loop.with_extension_enabled();
                task_loop.execute_with_extension(&task).await?
            } else {
                task_loop.execute(&task).await?
            }
        }
        ExecutionMode::Swarm { agent_count } => {
            // Swarm execution with multiple parallel agents
            if !matches!(args.output_format, OutputFormat::Json) {
                if args.no_color {
                    println!("Starting swarm execution with {} agents", agent_count);
                } else {
                    println!(
                        "{}",
                        format!("🐝 Starting swarm execution with {} agents", agent_count)
                            .cyan()
                            .bold()
                    );
                }
            }

            // Configure overseer if not disabled
            let mut coordinator = SwarmCoordinator::from_config(&config, agent_count)
                .with_shutdown_signal(shutdown_signal.clone());

            // TODO: Integrate overseer into swarm coordinator
            // For now, use --overseer-only mode for standalone capability synthesis
            let _ = (&args.no_overseer, &args.overseer_model); // suppress unused warnings

            let swarm_result = coordinator.execute(&task).await?;

            // Convert SwarmResult to TaskResult
            jarvis_v2::TaskResult::from(swarm_result)
        }
    };

    // Update checkpoint with completion
    if let Some(ref cp_path) = checkpoint_path {
        let checkpoint = Checkpoint {
            task: task.clone(),
            iteration: result.iterations,
            max_iterations: u32::MAX,
            working_dir,
            completion_promise: args.promise,
            model: args.model,
            completed: result.success,
            last_error: result.error.clone(),
        };
        save_checkpoint(&checkpoint, cp_path)?;
    }

    // Report results (skip if JSON format since progress callback handles it)
    if !matches!(args.output_format, OutputFormat::Json) {
        print_result(&result, args.output_format, args.no_color);
    }

    if result.success {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

/// Run the Creative Overseer in standalone mode for capability synthesis
async fn run_overseer_only(
    args: &Args,
    working_dir: PathBuf,
    shutdown_signal: ShutdownSignal,
) -> Result<()> {
    use jarvis_v2::extend::{CapabilityRegistry, CapabilityType, CreativeOverseer, HotLoader, OverseerConfig, SynthesizedCapability};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    println!("{}", "Creative Overseer - Standalone Mode".bright_cyan().bold());
    println!("{}", "═".repeat(36).cyan());
    println!();
    println!("Continuously brainstorming and synthesizing capabilities...");
    println!("  Skills  → ~/.claude/skills/");
    println!("  MCPs    → ~/.claude/settings.json (mcpServers)");
    println!("  Agents  → ~/.claude/agents/");
    println!("  Hooks   → ~/.claude/settings.json (hooks)");
    println!("Press Ctrl+C to stop.\n");

    let registry = Arc::new(Mutex::new(CapabilityRegistry::load().unwrap_or_default()));
    let hot_loader = HotLoader::new(); // Registers capabilities into Claude Code

    let overseer_config = OverseerConfig {
        enabled: true,
        brainstorm_model: args.overseer_model.clone(),
        working_dir: working_dir.clone(),
        max_ideas_per_iteration: 5,
        min_confidence_to_synthesize: 0.6,
        immediate_synthesis: true,
    };

    let mut overseer = CreativeOverseer::new(overseer_config, registry.clone());

    // Get existing capabilities for context
    let existing_caps: Vec<String> = {
        let reg = registry.lock().unwrap();
        reg.mcps.iter().map(|c| format!("MCP: {}", c.name))
            .chain(reg.skills.iter().map(|c| format!("Skill: {}", c.name)))
            .chain(reg.agents.iter().map(|c| format!("Agent: {}", c.name)))
            .collect()
    };

    let mut iteration = 0;
    let mut total_synthesized = 0;

    while !shutdown_signal.is_shutdown() {
        iteration += 1;
        println!("{}  Iteration {} - Brainstorming...", "🧠".bright_yellow(), iteration);

        // Build a context-aware brainstorming task
        let task = format!(
            "Analyze this Rust/software project and suggest NEW capabilities. \
             Focus on tools, integrations, and workflows that would be genuinely useful. \
             Consider: testing, debugging, deployment, monitoring, documentation, code quality. \
             This is iteration {} - try to suggest different things than before.",
            iteration
        );

        // Run brainstorming
        match overseer.brainstorm(&task, &[], &existing_caps).await {
            Ok(result) => {
                if result.ideas.is_empty() {
                    println!("   No new capability ideas this iteration.");
                } else {
                    let mut installed_count = 0;
                    for idea in &result.ideas {
                        let mut status = match idea.synthesis_success {
                            Some(true) => "✅ synthesized".green(),
                            Some(false) => "❌ failed".red(),
                            None => "⏳ pending".yellow(),
                        };

                        // If synthesized, install into Claude Code
                        if idea.synthesis_success == Some(true) {
                            let synth_dir = working_dir.join(".jarvis").join("synthesized");
                            let cap_path = match idea.capability_type {
                                CapabilityType::Skill => synth_dir.join("skills").join(format!("{}.SKILL.md", idea.name)),
                                CapabilityType::Mcp => synth_dir.join("mcps").join(&idea.name),
                                CapabilityType::Agent => synth_dir.join("agents").join(format!("{}.AGENT.md", idea.name)),
                            };

                            if cap_path.exists() {
                                let capability = SynthesizedCapability::new(
                                    idea.capability_type.clone(),
                                    idea.name.clone(),
                                    cap_path.clone(),
                                );

                                match hot_loader.register(&capability) {
                                    Ok(load_result) => {
                                        status = format!("✅ installed → {}",
                                            load_result.installed_path.as_ref()
                                                .map(|p| p.display().to_string())
                                                .unwrap_or_else(|| "~/.claude/".to_string())
                                        ).green();
                                        installed_count += 1;
                                    }
                                    Err(e) => {
                                        status = format!("⚠️ install failed: {}", e).yellow();
                                    }
                                }
                            } else {
                                status = format!("⚠️ file not found: {}", cap_path.display()).yellow();
                            }
                        } else if idea.synthesis_success == Some(false) {
                            // Already marked as failed
                        } else {
                            status = format!("⏳ synthesis_success={:?}", idea.synthesis_success).dimmed();
                        }

                        println!(
                            "   {:?}: {} - {} [confidence: {:.0}%] {}",
                            idea.capability_type,
                            idea.name.bright_white(),
                            idea.description.dimmed(),
                            idea.confidence * 100.0,
                            status
                        );
                    }
                    total_synthesized += result.synthesized_count;
                    if installed_count > 0 {
                        println!("   {} {} capabilities installed into Claude Code", "→".green(), installed_count);
                    }
                }
                println!(
                    "   Duration: {}ms | Synthesized: {} | Total: {}\n",
                    result.duration_ms, result.synthesized_count, total_synthesized
                );
            }
            Err(e) => {
                eprintln!("   Error during brainstorming: {}", e);
            }
        }

        // Wait before next iteration (give time for Claude rate limits)
        println!("   Waiting 30s before next iteration...");
        for _ in 0..30 {
            if shutdown_signal.is_shutdown() {
                println!("\nShutdown signal received, stopping...");
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    // Summary
    println!("\n{}", "═".repeat(40).cyan());
    println!("{}  Session complete!", "✨".bright_green());
    println!("   Iterations: {}", iteration);
    println!("   Capabilities synthesized: {}", total_synthesized);

    // Show what was created
    let synth_dir = working_dir.join(".jarvis").join("synthesized");
    if synth_dir.exists() {
        println!("\n   Created files:");
        for entry in walkdir::WalkDir::new(&synth_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
        {
            println!("   - {}", entry.path().strip_prefix(&working_dir).unwrap_or(entry.path()).display());
        }
    }

    Ok(())
}
