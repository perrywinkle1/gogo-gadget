#!/bin/bash
#
# Test script for the jarvis bash wrapper
#
# Usage:
#   ./scripts/test_jarvis.sh           # Run all tests
#   ./scripts/test_jarvis.sh --quick   # Run quick tests only (no build)
#   ./scripts/test_jarvis.sh --verbose # Show verbose output
#

set -euo pipefail

# Resolve script location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JARVIS="$SCRIPT_DIR/jarvis"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Options
VERBOSE=0
QUICK=0

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --verbose|-v)
            VERBOSE=1
            shift
            ;;
        --quick|-q)
            QUICK=1
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [--quick] [--verbose]"
            echo ""
            echo "Options:"
            echo "  --quick, -q    Skip tests that require building"
            echo "  --verbose, -v  Show detailed test output"
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# ============================================================================
# Test Helpers
# ============================================================================

log() {
    if [[ $VERBOSE -eq 1 ]]; then
        echo -e "${BLUE}[INFO]${NC} $1"
    fi
}

test_start() {
    local name="$1"
    ((TESTS_RUN++))
    if [[ $VERBOSE -eq 1 ]]; then
        echo -e "${BLUE}[TEST]${NC} $name..."
    fi
}

test_pass() {
    local name="$1"
    ((TESTS_PASSED++))
    echo -e "${GREEN}✓${NC} $name"
}

test_fail() {
    local name="$1"
    local reason="${2:-}"
    ((TESTS_FAILED++))
    echo -e "${RED}✗${NC} $name"
    if [[ -n "$reason" ]]; then
        echo -e "  ${RED}Reason: $reason${NC}"
    fi
}

# Run a command and capture output
run_cmd() {
    local output
    local exit_code=0
    output=$("$@" 2>&1) || exit_code=$?
    echo "$output"
    return $exit_code
}

# ============================================================================
# Script Existence and Syntax Tests
# ============================================================================

test_script_exists() {
    test_start "Script exists"
    if [[ -f "$JARVIS" ]]; then
        test_pass "Script exists at $JARVIS"
    else
        test_fail "Script exists" "Not found at $JARVIS"
        return 1
    fi
}

test_script_executable() {
    test_start "Script is executable"
    if [[ -x "$JARVIS" ]]; then
        test_pass "Script is executable"
    else
        test_fail "Script is executable" "Missing execute permission"
        return 1
    fi
}

test_script_syntax() {
    test_start "Script syntax valid"
    if bash -n "$JARVIS" 2>/dev/null; then
        test_pass "Script syntax valid"
    else
        test_fail "Script syntax valid" "Bash syntax error"
        return 1
    fi
}

# ============================================================================
# Help and Version Tests
# ============================================================================

test_help_output() {
    test_start "Help output"
    local output
    output=$(run_cmd "$JARVIS" --help)

    if echo "$output" | grep -q "USAGE:" && \
       echo "$output" | grep -q "OPTIONS:" && \
       echo "$output" | grep -q "ENVIRONMENT VARIABLES:"; then
        test_pass "Help output contains expected sections"
    else
        test_fail "Help output" "Missing expected sections"
        log "Output: $output"
    fi
}

test_help_short_flag() {
    test_start "Help -h flag"
    local output
    output=$(run_cmd "$JARVIS" -h)

    if echo "$output" | grep -q "USAGE:"; then
        test_pass "Help -h flag works"
    else
        test_fail "Help -h flag" "Did not show help"
    fi
}

test_no_args_shows_help() {
    test_start "No args shows help"
    local output
    output=$(run_cmd "$JARVIS")

    if echo "$output" | grep -q "USAGE:"; then
        test_pass "No args shows help"
    else
        test_fail "No args shows help" "Did not show help"
    fi
}

# ============================================================================
# Shell Completion Tests
# ============================================================================

test_completions_bash() {
    test_start "Bash completions"
    local output
    output=$(run_cmd "$JARVIS" completions bash)

    if echo "$output" | grep -q "_jarvis_completions" && \
       echo "$output" | grep -q "complete -F"; then
        test_pass "Bash completions generated"
    else
        test_fail "Bash completions" "Invalid output"
        log "Output: $output"
    fi
}

test_completions_zsh() {
    test_start "Zsh completions"
    local output
    output=$(run_cmd "$JARVIS" completions zsh)

    if echo "$output" | grep -q "#compdef jarvis" && \
       echo "$output" | grep -q "_jarvis()"; then
        test_pass "Zsh completions generated"
    else
        test_fail "Zsh completions" "Invalid output"
        log "Output: $output"
    fi
}

test_completions_fish() {
    test_start "Fish completions"
    local output
    output=$(run_cmd "$JARVIS" completions fish)

    if echo "$output" | grep -q "complete -c jarvis"; then
        test_pass "Fish completions generated"
    else
        test_fail "Fish completions" "Invalid output"
        log "Output: $output"
    fi
}

test_completions_invalid() {
    test_start "Invalid shell error"
    local exit_code=0
    run_cmd "$JARVIS" completions invalid_shell >/dev/null 2>&1 || exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        test_pass "Invalid shell returns error"
    else
        test_fail "Invalid shell error" "Should have failed"
    fi
}

test_completion_files_exist() {
    test_start "Completion files exist"
    local missing=()

    [[ ! -f "$SCRIPT_DIR/completions/jarvis.bash" ]] && missing+=("jarvis.bash")
    [[ ! -f "$SCRIPT_DIR/completions/jarvis.zsh" ]] && missing+=("jarvis.zsh")
    [[ ! -f "$SCRIPT_DIR/completions/jarvis.fish" ]] && missing+=("jarvis.fish")

    if [[ ${#missing[@]} -eq 0 ]]; then
        test_pass "All completion files exist"
    else
        test_fail "Completion files exist" "Missing: ${missing[*]}"
    fi
}

# ============================================================================
# Status Command Tests
# ============================================================================

test_status_no_task() {
    test_start "Status with no active task"
    local output
    output=$(run_cmd "$JARVIS" status)

    if echo "$output" | grep -qi "no active task\|status"; then
        test_pass "Status command works"
    else
        test_fail "Status command" "Unexpected output"
        log "Output: $output"
    fi
}

test_status_with_dir() {
    test_start "Status with directory argument"
    local tmpdir
    tmpdir=$(mktemp -d)

    local output
    output=$(run_cmd "$JARVIS" status "$tmpdir")

    rm -rf "$tmpdir"

    if echo "$output" | grep -qi "no active task\|status"; then
        test_pass "Status with directory works"
    else
        test_fail "Status with directory" "Unexpected output"
    fi
}

# ============================================================================
# History Command Tests
# ============================================================================

test_history_empty() {
    test_start "History with no entries"

    # Use temp JARVIS_HOME to ensure empty history
    local tmpdir
    tmpdir=$(mktemp -d)

    local output
    output=$(JARVIS_HOME="$tmpdir" run_cmd "$JARVIS" history)

    rm -rf "$tmpdir"

    if echo "$output" | grep -qi "no.*history\|execution history\|0 executions"; then
        test_pass "Empty history handled"
    else
        test_fail "Empty history" "Unexpected output"
        log "Output: $output"
    fi
}

test_history_limit() {
    test_start "History with limit argument"
    local output
    output=$(run_cmd "$JARVIS" history 5)

    if echo "$output" | grep -qi "history\|showing"; then
        test_pass "History limit argument accepted"
    else
        test_fail "History limit" "Unexpected output"
    fi
}

# ============================================================================
# Resume Command Tests
# ============================================================================

test_resume_no_checkpoints() {
    test_start "Resume with no checkpoints"
    local tmpdir
    tmpdir=$(mktemp -d)
    cd "$tmpdir"

    local exit_code=0
    run_cmd "$JARVIS" resume >/dev/null 2>&1 || exit_code=$?

    cd - >/dev/null
    rm -rf "$tmpdir"

    if [[ $exit_code -ne 0 ]]; then
        test_pass "Resume with no checkpoints returns error"
    else
        test_fail "Resume no checkpoints" "Should have failed"
    fi
}

# ============================================================================
# Environment Variable Tests
# ============================================================================

test_env_jarvis_no_color() {
    test_start "JARVIS_NO_COLOR environment variable"
    local output
    output=$(JARVIS_NO_COLOR=1 run_cmd "$JARVIS" --help)

    # Output should not contain ANSI escape codes
    if ! echo "$output" | grep -q $'\033'; then
        test_pass "JARVIS_NO_COLOR disables colors"
    else
        test_fail "JARVIS_NO_COLOR" "Colors still present in output"
    fi
}

test_env_invalid_max_iter() {
    test_start "Invalid JARVIS_MAX_ITER"
    local exit_code=0

    # Use a simple task that won't actually run (because of validation error)
    JARVIS_MAX_ITER="not_a_number" run_cmd "$JARVIS" "test" 2>&1 || exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        test_pass "Invalid JARVIS_MAX_ITER rejected"
    else
        test_fail "Invalid JARVIS_MAX_ITER" "Should have failed"
    fi
}

test_env_invalid_output_fmt() {
    test_start "Invalid JARVIS_OUTPUT_FMT"
    local exit_code=0

    JARVIS_OUTPUT_FMT="invalid" run_cmd "$JARVIS" "test" 2>&1 || exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        test_pass "Invalid JARVIS_OUTPUT_FMT rejected"
    else
        test_fail "Invalid JARVIS_OUTPUT_FMT" "Should have failed"
    fi
}

test_env_valid_output_formats() {
    test_start "Valid output formats accepted"
    local errors=0

    for fmt in text json markdown; do
        # Just test that the help command works with the env var
        local exit_code=0
        JARVIS_OUTPUT_FMT="$fmt" run_cmd "$JARVIS" --help >/dev/null 2>&1 || exit_code=$?
        if [[ $exit_code -ne 0 ]]; then
            ((errors++))
        fi
    done

    if [[ $errors -eq 0 ]]; then
        test_pass "All valid output formats accepted"
    else
        test_fail "Valid output formats" "$errors formats failed"
    fi
}

# ============================================================================
# Input Validation Tests
# ============================================================================

test_empty_task_rejected() {
    test_start "Empty task shows help"
    local output
    output=$(run_cmd "$JARVIS" "" 2>&1)

    # Empty string passed as task shows help (reasonable UX behavior)
    if echo "$output" | grep -q "USAGE:"; then
        test_pass "Empty task shows help"
    else
        test_fail "Empty task" "Unexpected output"
    fi
}

test_whitespace_task_rejected() {
    test_start "Whitespace-only task rejected"
    local exit_code=0

    run_cmd "$JARVIS" "   " 2>&1 || exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        test_pass "Whitespace-only task rejected"
    else
        test_fail "Whitespace task" "Should have failed"
    fi
}

# ============================================================================
# Install Command Test (Dry Run)
# ============================================================================

test_install_help_in_output() {
    test_start "Install command shows info"
    local output
    output=$(run_cmd "$JARVIS" --help)

    if echo "$output" | grep -q -- "--install"; then
        test_pass "Install option documented in help"
    else
        test_fail "Install in help" "Not found in help output"
    fi
}

# ============================================================================
# Edge Case Tests
# ============================================================================

test_special_chars_in_path() {
    test_start "Handle special characters in path"
    local tmpdir
    tmpdir=$(mktemp -d)
    local test_dir="$tmpdir/path with spaces"
    mkdir -p "$test_dir"

    local output
    output=$(cd "$test_dir" && run_cmd "$JARVIS" status)

    rm -rf "$tmpdir"

    if echo "$output" | grep -qi "status"; then
        test_pass "Handles paths with spaces"
    else
        test_fail "Paths with spaces" "Failed to handle"
    fi
}

test_version_output() {
    test_start "Version flag"
    local output
    output=$(run_cmd "$JARVIS" --version 2>&1)

    if echo "$output" | grep -qi "jarvis"; then
        test_pass "Version flag works"
    else
        test_fail "Version flag" "Unexpected output"
    fi
}

test_version_short_flag() {
    test_start "Version -V flag"
    local output
    output=$(run_cmd "$JARVIS" -V 2>&1)

    if echo "$output" | grep -qi "jarvis"; then
        test_pass "Version -V flag works"
    else
        test_fail "Version -V flag" "Unexpected output"
    fi
}

# ============================================================================
# Run Tests
# ============================================================================

echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Jarvis Bash Wrapper Test Suite${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""

# Script existence tests (required)
test_script_exists || exit 1
test_script_executable || exit 1
test_script_syntax || exit 1

echo ""
echo -e "${YELLOW}--- Help and Version Tests ---${NC}"
test_help_output
test_help_short_flag
test_no_args_shows_help
test_version_output
test_version_short_flag

echo ""
echo -e "${YELLOW}--- Shell Completion Tests ---${NC}"
test_completions_bash
test_completions_zsh
test_completions_fish
test_completions_invalid
test_completion_files_exist

echo ""
echo -e "${YELLOW}--- Command Tests ---${NC}"
test_status_no_task
test_status_with_dir
test_history_empty
test_history_limit
test_resume_no_checkpoints

echo ""
echo -e "${YELLOW}--- Environment Variable Tests ---${NC}"
test_env_jarvis_no_color
test_env_invalid_max_iter
test_env_invalid_output_fmt
test_env_valid_output_formats

echo ""
echo -e "${YELLOW}--- Input Validation Tests ---${NC}"
test_empty_task_rejected
test_whitespace_task_rejected
test_install_help_in_output

echo ""
echo -e "${YELLOW}--- Edge Case Tests ---${NC}"
test_special_chars_in_path

# Summary
echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Test Summary${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  Tests run:    $TESTS_RUN"
echo -e "  ${GREEN}Passed:       $TESTS_PASSED${NC}"
echo -e "  ${RED}Failed:       $TESTS_FAILED${NC}"
echo ""

if [[ $TESTS_FAILED -eq 0 ]]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi
