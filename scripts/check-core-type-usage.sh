#!/bin/bash
#
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Script to check if new core data types introduced since a previous release are already being used
# in the current codebase. This ensures downstream applications have time to adopt new types before
# they're actively used in production.
#
# This script extracts the relevant data and outputs a structured request that should be analyzed
# by Claude Code (with tool access) or another analysis tool.
#
# Usage: ./check-core-type-usage.sh [OLD_BRANCH]
#   OLD_BRANCH: Optional. Previous release branch to compare against (default: latest release)

set -euo pipefail

STAGED_FILE="crates/sui-core/tests/staged/sui.yaml"
PROTOCOL_CONFIG_FILE="crates/sui-protocol-config/src/lib.rs"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

# Function to display usage
usage() {
    echo "Usage: $0 [OLD_BRANCH]"
    echo ""
    echo "Compare core data types between OLD_BRANCH and current branch."
    echo "Outputs analysis request that can be provided to Claude Code for checking."
    echo ""
    echo "Arguments:"
    echo "  OLD_BRANCH    Previous release branch (default: latest major release)"
    echo ""
    echo "Examples:"
    echo "  $0                                    # Compare with latest release"
    echo "  $0 releases/sui-v1.56.0-release       # Compare with specific release"
    echo ""
    echo "Output can be piped to Claude Code or used in CI/automation."
    exit 1
}

# Function to get the previous major release branch
get_previous_release() {
    git fetch origin --quiet 2>/dev/null || true
    git branch -r | grep "origin/releases/sui-v" | grep -E "v[0-9]+\.[0-9]+\.0-release$" | sort -V | tail -1 | sed 's/^[[:space:]]*origin\///'
}

# Parse arguments
if [ "$#" -gt 1 ]; then
    usage
fi

OLD_BRANCH="${1:-}"

if [ -z "$OLD_BRANCH" ]; then
    OLD_BRANCH=$(get_previous_release)
    if [ -z "$OLD_BRANCH" ]; then
        echo "Error: Could not determine previous release branch"
        exit 1
    fi
fi

# Verify old branch exists
if ! git rev-parse --verify "$OLD_BRANCH" >/dev/null 2>&1; then
    if ! git rev-parse --verify "origin/$OLD_BRANCH" >/dev/null 2>&1; then
        echo "Error: Branch '$OLD_BRANCH' not found"
        exit 1
    fi
    OLD_BRANCH="origin/$OLD_BRANCH"
fi

# Get current branch name for display
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)

# Create temp directory
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

OLD_YAML="$TEMP_DIR/old_sui.yaml"
CURRENT_YAML="$TEMP_DIR/current_sui.yaml"

# Extract the yaml files
git show "$OLD_BRANCH:$STAGED_FILE" > "$OLD_YAML" 2>/dev/null || {
    echo "Error: Could not fetch $STAGED_FILE from $OLD_BRANCH"
    exit 1
}

cp "$STAGED_FILE" "$CURRENT_YAML" 2>/dev/null || {
    echo "Error: Could not read $STAGED_FILE from current branch"
    exit 1
}

# Execute Claude with the prompt
echo "Comparing $OLD_BRANCH -> $CURRENT_BRANCH (current)" >&2
echo "Analyzing core type changes and checking for premature usage..." >&2
echo "" >&2

# First, generate a diff to identify what's new
DIFF_FILE="$TEMP_DIR/diff.txt"
diff -u "$OLD_YAML" "$CURRENT_YAML" > "$DIFF_FILE" || true

# Extract added lines (new types/variants)
NEW_ITEMS=$(grep "^+" "$DIFF_FILE" | grep -v "^+++" | grep -E "^\\+[A-Za-z]" | sed 's/^+//' | cut -d: -f1 | sort -u || true)

# Create the prompt in a temp file
PROMPT_FILE="$TEMP_DIR/prompt.txt"
cat > "$PROMPT_FILE" <<'PROMPT_END'
You are analyzing changes to Sui blockchain core data types to ensure new types are not prematurely used in production before downstream applications can adopt them.

## Task

1. **Identify NEW additions** from the DIFF below (ignore renames):
   - Look at added lines (lines starting with +)
   - If an old type was removed (lines starting with -) and a new one replaces it with equivalent functionality, that's a RENAME - skip it
   - Focus on genuinely NEW enum variants, types, and fields

2. **Check current codebase usage** for each new addition:
   - You MUST use Grep tool to search for each new type name
   - Search patterns:
     * For enum variants like "CoinRegistryCreate": search for "CoinRegistryCreate" in *.rs files
     * For types like "FundsWithdrawalArg": search for "FundsWithdrawalArg" in *.rs files
   - Look in crates/ and sui-execution/ directories
   - Exclude test files (files containing "/test", "test_", or "#[test]")
   - For each match found, examine the context to determine if it's construction/usage
   - Check if usage is gated behind protocol config feature flags

3. **Evaluate feature flags**:
   - Check if the flag is enabled on BOTH mainnet AND testnet
   - Look for conditions like `if chain != Chain::Mainnet` or `if chain != Chain::Testnet`
   - Flags enabled ONLY for devnet do NOT count as production usage
   - Ungated usage OR usage behind flags enabled on mainnet/testnet = PRODUCTION usage = FAIL

## Output Format

**CRITICAL: Your response MUST start IMMEDIATELY with one of these exact strings (no analysis, no thinking, no preamble):**
```
CHECK PASSED
```
or
```
CHECK FAILED
```

Do NOT write ANY text before this. NO thinking process, NO "Perfect!", NO "Let me analyze", NO "Summary of findings".
The VERY FIRST CHARACTERS of your response must be either "CHECK PASSED" or "CHECK FAILED".

**If CHECK PASSED:**
```
No new core data types are being used prematurely in production (mainnet/testnet).
```

If only renames exist, add: `No new additions found - only renames/refactorings.` and stop.

If safe new additions exist, list them:
```
## New additions that are NOT yet in production (OK):

### [Type/Variant/Field Name]
- **Location**: [where added in schema]
- **Usage**: [description of usage or "not used yet"]
- **Feature flag**: [flag name and enablement status, or "not gated"]
- **Status**: Safe - [reason why it's not in production]
```

**If CHECK FAILED:**
```
## New additions found that are already in production use:

### [Type/Variant/Field Name]
- **Location**: [where added in schema]
- **Usage locations**: [file:line references]
- **Feature flag**: [flag name, or "not gated"]
- **Reason for concern**: [why this is premature - not gated OR flag enabled on mainnet/testnet]
```

## Rules

- Do NOT mention renames anywhere in your output - completely omit them
- First line MUST be "CHECK PASSED" or "CHECK FAILED" with no text before it
- Only report genuinely NEW types/variants/fields (not renamed ones)
- Only flag as FAILED if used in production on mainnet OR testnet
- Search the CURRENT codebase (your working directory) for usage

=== DIFF SUMMARY ===
Only showing added/changed lines to keep prompt focused:
PROMPT_END

# Append just the diff, not full YAML files
cat "$DIFF_FILE" >> "$PROMPT_FILE"

# Add note about protocol config
cat >> "$PROMPT_FILE" <<'PROTOCOL_NOTE'

=== PROTOCOL CONFIG LOCATION ===
Feature flags are defined in: crates/sui-protocol-config/src/lib.rs
You MUST use Read tool to examine this file when checking feature flags.
Look for where flags are set to `true` in protocol version blocks (e.g., "96 => {").
PROTOCOL_NOTE

# Add final instructions
cat >> "$PROMPT_FILE" <<'FINAL_END'

=== END OF FILES ===

Perform the analysis with these MANDATORY steps:

1. **Identify new additions**: Diff the two sui.yaml files
   - List every new type, enum variant, or field
   - Ignore renames (where old was removed and new replaces it)

2. **For EACH new addition, you MUST**:
   - Use Grep tool to search: `grep -r "TypeName" crates/ sui-execution/ --include="*.rs"`
   - Exclude test files from results
   - If matches found, read those files to understand the usage context
   - Check if usage is behind a feature flag

3. **Evaluate feature flags** - BE VERY CAREFUL HERE:
   - The PROTOCOL CONFIG sections above show feature flag definitions and configurations
   - For each flag, find where it's set to `true` in the PROTOCOL VERSION CONFIGURATIONS section
   - A flag is ONLY safe if the line setting it to `true` is INSIDE an `if chain != Chain::Mainnet && chain != Chain::Testnet` block
   - If a flag is set to `true` WITHOUT any chain condition, it's enabled on ALL chains (FAIL)
   - If a flag is set with `if chain == Chain::Unknown`, it's devnet-only (SAFE)
   - **DO NOT make up flag names** - only use flags that actually appear in the PROTOCOL CONFIG section

4. **Report**: CHECK PASSED or CHECK FAILED based on findings

CRITICAL: Your response MUST begin with "CHECK PASSED" or "CHECK FAILED" as the absolute first text.
Do NOT include thinking, analysis, or any other text before that line.

YOU MUST USE THE GREP TOOL TO SEARCH - do not just assume types are not used!
FINAL_END

# Run Claude CLI with tool access
cd "$(git rev-parse --show-toplevel)"

if ! command -v claude &> /dev/null; then
    echo "Error: 'claude' command not found. Please install the Claude CLI." >&2
    echo "Visit: https://github.com/anthropics/claude-cli" >&2
    exit 1
fi

# Run claude with full tool access (pass prompt via stdin to avoid arg length limits)
RESULT=$(cat "$PROMPT_FILE" | claude -p --allowed-tools "Grep Read Glob" 2>&1)
echo "$RESULT"

# Check the output for PASS/FAIL
if echo "$RESULT" | grep -q "CHECK PASSED"; then
    echo "" >&2
    echo -e "${GREEN}✓ Core type usage check passed${NC}" >&2
    exit 0
elif echo "$RESULT" | grep -q "CHECK FAILED"; then
    echo "" >&2
    echo -e "${RED}✗ Core type usage check failed${NC}" >&2
    exit 1
else
    echo "" >&2
    echo -e "${RED}✗ Unexpected output format from Claude${NC}" >&2
    exit 1
fi
