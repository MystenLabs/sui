#!/bin/bash

# Test script to verify bash and python versions produce the same dry-run output

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BASH_SCRIPT="$SCRIPT_DIR/run-antithesis-tests"
PYTHON_SCRIPT="$SCRIPT_DIR/run-antithesis-tests.py"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

FAILED=0
PASSED=0

compare_output() {
    local test_name="$1"
    shift
    local args=("$@")

    # Run both scripts with dry-run mode
    bash_output=$("$BASH_SCRIPT" -D "${args[@]}" 2>&1 | grep "^Running:")
    python_output=$("$PYTHON_SCRIPT" -n "${args[@]}" 2>&1 | grep "^Running:")

    if [ "$bash_output" = "$python_output" ]; then
        echo -e "${GREEN}PASS${NC}: $test_name"
        ((PASSED++))
    else
        echo -e "${RED}FAIL${NC}: $test_name"
        echo "  Bash:   $bash_output"
        echo "  Python: $python_output"
        ((FAILED++))
    fi
}

compare_output_different_args() {
    local test_name="$1"
    local bash_args="$2"
    local python_args="$3"

    # Run both scripts with dry-run mode but different arguments
    bash_output=$("$BASH_SCRIPT" -D $bash_args 2>&1 | grep "^Running:")
    python_output=$("$PYTHON_SCRIPT" -n $python_args 2>&1 | grep "^Running:")

    if [ "$bash_output" = "$python_output" ]; then
        echo -e "${GREEN}PASS${NC}: $test_name"
        ((PASSED++))
    else
        echo -e "${RED}FAIL${NC}: $test_name"
        echo "  Bash:   $bash_output"
        echo "  Python: $python_output"
        ((FAILED++))
    fi
}

echo "Testing antithesis scripts for output parity..."
echo "================================================"
echo

# Use a fixed commit hash for consistent testing
TEST_COMMIT="abc123def456"

# Test 1: Default arguments (with explicit commit to avoid git operations)
compare_output "Default args with commit" -C "$TEST_COMMIT"

# Test 2: Custom test duration
compare_output "Custom test duration" -C "$TEST_COMMIT" -t 1.5

# Test 3: Split version mode
compare_output "Split version mode" -C "$TEST_COMMIT" -s -a "def456abc123"

# Test 4: Upgrade test type
compare_output "Upgrade test type" -C "$TEST_COMMIT" -u

# Test 5: Description (single word)
compare_output "Description (single word)" -C "$TEST_COMMIT" -d "testing"

# Test 6: Log level
compare_output "Log level" -C "$TEST_COMMIT" -l "info"

# Test 7: Tidehunter commit
compare_output "Tidehunter commit" -C "$TEST_COMMIT" -T "tide123"

# Test 8: Config commit
compare_output "Config commit" -C "$TEST_COMMIT" -c "config456"

# Test 9: Stress commit
compare_output "Stress commit" -C "$TEST_COMMIT" -S "stress789"

# Test 10: Protocol override
compare_output "Protocol override" -C "$TEST_COMMIT" -p "testnet"

# Test 11: Test name (Python uses --test-name, bash uses -n)
compare_output_different_args "Test name" "-C $TEST_COMMIT -n my-test" "-C $TEST_COMMIT --test-name my-test"

# Test 12: Workflow ref
compare_output "Workflow ref" -C "$TEST_COMMIT" -r "feature-branch"

# Test 13: Multiple options combined
compare_output_different_args "Multiple options" "-C $TEST_COMMIT -t 2.0 -u -l debug -n combined-test" "-C $TEST_COMMIT -t 2.0 -u -l debug --test-name combined-test"

# Test 14: All options
compare_output_different_args "All options" "-C $TEST_COMMIT -t 3.0 -d fulltest -u -a alt123 -l warn -T tide456 -c cfg789 -S stress012 -p mainnet -n alltest -r main" "-C $TEST_COMMIT -t 3.0 -d fulltest -u -a alt123 -l warn -T tide456 -c cfg789 -S stress012 -p mainnet --test-name alltest -r main"

# Test 15: Split version with alt commit
compare_output "Split with alt" -C "$TEST_COMMIT" -s -a "custom_alt_sha"

echo
echo "Testing git-dependent cases..."
echo "================================================"
echo

# These tests use actual git commands to resolve commits
# Both scripts should produce identical output

# Test 16: No args - uses HEAD
compare_output "No args (uses HEAD)"

# Test 17: Split version without alt - uses merge-base and HEAD
compare_output "Split version only (-s)" -s

# Test 18: Split version with upgrade - uses merge-base and HEAD
compare_output "Split version + upgrade (-s -u)" -s -u

# Test 19: Split version with other options but no alt commit
compare_output_different_args "Split version + options" "-s -t 1.0 -n split-test" "-s -t 1.0 --test-name split-test"

echo
echo "Testing Python long argument versions..."
echo "================================================"
echo

# Test long args - compare python long form to python short form
compare_python_long_short() {
    local test_name="$1"
    local short_args="$2"
    local long_args="$3"

    python_short=$("$PYTHON_SCRIPT" --dry-run $short_args 2>&1 | grep "^Running:")
    python_long=$("$PYTHON_SCRIPT" --dry-run $long_args 2>&1 | grep "^Running:")

    if [ "$python_short" = "$python_long" ]; then
        echo -e "${GREEN}PASS${NC}: $test_name"
        ((PASSED++))
    else
        echo -e "${RED}FAIL${NC}: $test_name"
        echo "  Short: $python_short"
        echo "  Long:  $python_long"
        ((FAILED++))
    fi
}

compare_python_long_short "Long: test-duration" "-C $TEST_COMMIT -t 2.0" "--sui-commit $TEST_COMMIT --test-duration 2.0"
compare_python_long_short "Long: split-version" "-C $TEST_COMMIT -s -a alt123" "--sui-commit $TEST_COMMIT --split-version --alt-commit alt123"
compare_python_long_short "Long: description" "-C $TEST_COMMIT -d desc" "--sui-commit $TEST_COMMIT --description desc"
compare_python_long_short "Long: upgrade" "-C $TEST_COMMIT -u" "--sui-commit $TEST_COMMIT --upgrade"
compare_python_long_short "Long: log-level" "-C $TEST_COMMIT -l warn" "--sui-commit $TEST_COMMIT --log-level warn"
compare_python_long_short "Long: tidehunter-commit" "-C $TEST_COMMIT -T tide" "--sui-commit $TEST_COMMIT --tidehunter-commit tide"
compare_python_long_short "Long: config-commit" "-C $TEST_COMMIT -c cfg" "--sui-commit $TEST_COMMIT --config-commit cfg"
compare_python_long_short "Long: stress-commit" "-C $TEST_COMMIT -S stress" "--sui-commit $TEST_COMMIT --stress-commit stress"
compare_python_long_short "Long: protocol-override" "-C $TEST_COMMIT -p mainnet" "--sui-commit $TEST_COMMIT --protocol-override mainnet"
# Note: -n is now dry-run in Python, so test-name only has --test-name
compare_python_long_short "Long: test-name" "-C $TEST_COMMIT --test-name mytest" "--sui-commit $TEST_COMMIT --test-name mytest"
compare_python_long_short "Long: workflow-ref" "-C $TEST_COMMIT -r main" "--sui-commit $TEST_COMMIT --workflow-ref main"
compare_python_long_short "Long: mixed short and long" "-C $TEST_COMMIT -t 1.5 --upgrade -l info" "--sui-commit $TEST_COMMIT --test-duration 1.5 -u --log-level info"

echo
echo "================================================"
echo "Results: $PASSED passed, $FAILED failed"

if [ $FAILED -gt 0 ]; then
    exit 1
fi
