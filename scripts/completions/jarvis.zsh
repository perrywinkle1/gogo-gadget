#compdef jarvis
# Jarvis zsh completion
# Add to ~/.zshrc: source /path/to/jarvis.zsh
# Or: eval "$(jarvis completions zsh)"
# Or: fpath+=(/path/to/completions) && autoload -Uz compinit && compinit

_jarvis() {
    local -a commands options

    commands=(
        'status:Show current task status'
        'resume:Resume from checkpoint'
        'history:Show execution history'
        'completions:Generate shell completions'
    )

    options=(
        '(-i --max-iterations)'{-i,--max-iterations}'[Maximum iterations]:count:(5 10 15 20 30 50)'
        '(-p --promise)'{-p,--promise}'[Completion promise string]:string:'
        '(-d --dir)'{-d,--dir}'[Working directory]:directory:_files -/'
        '(-m --model)'{-m,--model}'[Claude model]:model:(claude-opus-4-5-20251101 claude-sonnet-4-20250514)'
        '(-v --verbose)'{-v,--verbose}'[Enable verbose output]'
        '(-q --quiet)'{-q,--quiet}'[Suppress non-essential output]'
        '--dry-run[Analyze without executing]'
        '(-o --output-format)'{-o,--output-format}'[Output format]:format:(text json markdown)'
        '(-c --checkpoint)'{-c,--checkpoint}'[Checkpoint file]:file:_files'
        '--no-color[Disable colored output]'
        '--install[Install to /usr/local/bin]:directory:_files -/'
        '(-h --help)'{-h,--help}'[Show help]'
        '(-V --version)'{-V,--version}'[Show version]'
    )

    _arguments -C \
        '1:command:->command' \
        '*::arg:->args' \
        $options

    case "$state" in
        command)
            _describe -t commands 'jarvis commands' commands
            ;;
        args)
            case "${words[1]}" in
                resume)
                    if [[ -d ".jarvis-checkpoints" ]]; then
                        local -a checkpoints
                        checkpoints=(${(f)"$(ls .jarvis-checkpoints/*.json 2>/dev/null | xargs -I{} basename {})"})
                        checkpoints+=('latest:Most recent checkpoint')
                        _describe -t checkpoints 'checkpoints' checkpoints
                    else
                        _values 'checkpoints' 'latest[Most recent checkpoint]'
                    fi
                    ;;
                completions)
                    _values 'shells' 'bash[Bash completion script]' 'zsh[Zsh completion script]' 'fish[Fish completion script]'
                    ;;
                history)
                    _values 'limit' '5' '10' '20' '50' '100'
                    ;;
                status)
                    _files -/
                    ;;
            esac
            ;;
    esac
}

compdef _jarvis jarvis
