# Jarvis fish completion
# Save to ~/.config/fish/completions/jarvis.fish
# Or: cp jarvis.fish ~/.config/fish/completions/

# Disable file completion by default
complete -c jarvis -f

# Commands
complete -c jarvis -n "__fish_use_subcommand" -a "status" -d "Show current task status"
complete -c jarvis -n "__fish_use_subcommand" -a "resume" -d "Resume from checkpoint"
complete -c jarvis -n "__fish_use_subcommand" -a "history" -d "Show execution history"
complete -c jarvis -n "__fish_use_subcommand" -a "completions" -d "Generate shell completions"

# Options
complete -c jarvis -l max-iterations -s i -d "Maximum iterations" -x -a "5 10 15 20 30 50"
complete -c jarvis -l promise -s p -d "Completion promise string" -x
complete -c jarvis -l dir -s d -d "Working directory" -r -a "(__fish_complete_directories)"
complete -c jarvis -l model -s m -d "Claude model" -x -a "claude-opus-4-5-20251101 claude-sonnet-4-20250514"
complete -c jarvis -l verbose -s v -d "Enable verbose output"
complete -c jarvis -l quiet -s q -d "Suppress non-essential output"
complete -c jarvis -l dry-run -d "Analyze without executing"
complete -c jarvis -l output-format -s o -d "Output format" -x -a "text json markdown"
complete -c jarvis -l checkpoint -s c -d "Checkpoint file" -r
complete -c jarvis -l no-color -d "Disable colored output"
complete -c jarvis -l install -d "Install to directory" -r -a "(__fish_complete_directories)"
complete -c jarvis -l help -d "Show help"
complete -c jarvis -l version -d "Show version"

# Resume subcommand completions
function __jarvis_resume_completions
    echo "latest	Most recent checkpoint"
    if test -d ".jarvis-checkpoints"
        for f in .jarvis-checkpoints/*.json
            if test -f "$f"
                basename "$f"
            end
        end
    end
end
complete -c jarvis -n "__fish_seen_subcommand_from resume" -a "(__jarvis_resume_completions)"

# Completions subcommand
complete -c jarvis -n "__fish_seen_subcommand_from completions" -a "bash" -d "Bash completion script"
complete -c jarvis -n "__fish_seen_subcommand_from completions" -a "zsh" -d "Zsh completion script"
complete -c jarvis -n "__fish_seen_subcommand_from completions" -a "fish" -d "Fish completion script"

# History subcommand - suggest common limits
complete -c jarvis -n "__fish_seen_subcommand_from history" -a "5 10 20 50 100" -d "Number of entries"
