# GoGoGadget fish completion
# Save to ~/.config/fish/completions/gogo-gadget.fish
# Or: cp gogo-gadget.fish ~/.config/fish/completions/

# Disable file completion by default
complete -c gogo-gadget -f

# Commands
complete -c gogo-gadget -n "__fish_use_subcommand" -a "status" -d "Show current task status"
complete -c gogo-gadget -n "__fish_use_subcommand" -a "resume" -d "Resume from checkpoint"
complete -c gogo-gadget -n "__fish_use_subcommand" -a "history" -d "Show execution history"
complete -c gogo-gadget -n "__fish_use_subcommand" -a "completions" -d "Generate shell completions"

# Options
complete -c gogo-gadget -l max-iterations -s i -d "Maximum iterations" -x -a "5 10 15 20 30 50"
complete -c gogo-gadget -l promise -s p -d "Completion promise string" -x
complete -c gogo-gadget -l dir -s d -d "Working directory" -r -a "(__fish_complete_directories)"
complete -c gogo-gadget -l model -s m -d "Claude model" -x -a "claude-opus-4-5-20251101 claude-sonnet-4-20250514"
complete -c gogo-gadget -l verbose -s v -d "Enable verbose output"
complete -c gogo-gadget -l quiet -s q -d "Suppress non-essential output"
complete -c gogo-gadget -l dry-run -d "Analyze without executing"
complete -c gogo-gadget -l output-format -s o -d "Output format" -x -a "text json markdown"
complete -c gogo-gadget -l checkpoint -s c -d "Checkpoint file" -r
complete -c gogo-gadget -l no-color -d "Disable colored output"
complete -c gogo-gadget -l install -d "Install to directory" -r -a "(__fish_complete_directories)"
complete -c gogo-gadget -l help -d "Show help"
complete -c gogo-gadget -l version -d "Show version"

# Resume subcommand completions
function __gogo_gadget_resume_completions
    echo "latest	Most recent checkpoint"
    if test -d ".gogo-gadget-checkpoints"
        for f in .gogo-gadget-checkpoints/*.json
            if test -f "$f"
                basename "$f"
            end
        end
    end
end
complete -c gogo-gadget -n "__fish_seen_subcommand_from resume" -a "(__gogo_gadget_resume_completions)"

# Completions subcommand
complete -c gogo-gadget -n "__fish_seen_subcommand_from completions" -a "bash" -d "Bash completion script"
complete -c gogo-gadget -n "__fish_seen_subcommand_from completions" -a "zsh" -d "Zsh completion script"
complete -c gogo-gadget -n "__fish_seen_subcommand_from completions" -a "fish" -d "Fish completion script"

# History subcommand - suggest common limits
complete -c gogo-gadget -n "__fish_seen_subcommand_from history" -a "5 10 20 50 100" -d "Number of entries"
