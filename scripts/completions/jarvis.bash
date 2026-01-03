# Jarvis bash completion
# Add to ~/.bashrc: source /path/to/jarvis.bash
# Or: eval "$(jarvis completions bash)"

_jarvis_completions() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Commands
    local commands="status resume history completions --help --install --version"

    # Options
    local options="--max-iterations --promise --dir --model --verbose --quiet --dry-run --output-format --checkpoint --no-color"

    case "$prev" in
        jarvis)
            COMPREPLY=($(compgen -W "$commands $options" -- "$cur"))
            ;;
        resume)
            # Complete checkpoint files
            if [[ -d ".jarvis-checkpoints" ]]; then
                local checkpoints=$(ls .jarvis-checkpoints/*.json 2>/dev/null | xargs -I{} basename {})
                COMPREPLY=($(compgen -W "$checkpoints latest" -- "$cur"))
            else
                COMPREPLY=($(compgen -W "latest" -- "$cur"))
            fi
            ;;
        completions)
            COMPREPLY=($(compgen -W "bash zsh fish" -- "$cur"))
            ;;
        history)
            # Suggest common limits
            COMPREPLY=($(compgen -W "5 10 20 50 100" -- "$cur"))
            ;;
        --dir|-d)
            COMPREPLY=($(compgen -d -- "$cur"))
            ;;
        --model|-m)
            COMPREPLY=($(compgen -W "claude-opus-4-5-20251101 claude-sonnet-4-20250514" -- "$cur"))
            ;;
        --max-iterations|-i)
            COMPREPLY=($(compgen -W "5 10 15 20 30 50" -- "$cur"))
            ;;
        --output-format|-o)
            COMPREPLY=($(compgen -W "text json markdown" -- "$cur"))
            ;;
        --checkpoint|-c)
            COMPREPLY=($(compgen -f -- "$cur"))
            ;;
        --install)
            COMPREPLY=($(compgen -d -- "$cur"))
            ;;
        *)
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "$options" -- "$cur"))
            fi
            ;;
    esac
}

complete -F _jarvis_completions jarvis
