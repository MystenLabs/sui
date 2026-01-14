#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -e

# Inputs from environment
TITLE="$PR_TITLE"
PR_NUM="$PR_NUMBER"
BODY="$PR_BODY"

# Build commit title
COMMIT_TITLE="${TITLE} (#${PR_NUM})"

# Function to process a section
process_section() {
    local section_name="$1"
    local section_content="$2"
    local section_header="$3"
    
    section_content=$(echo "$section_content" | xargs)
    
    case "$section_name" in
        "Description")
            # Skip if it's template text or empty
            if [ -n "$section_content" ] && \
                [ "$section_content" != "Describe the changes or additions included in this PR." ]; then
                echo "$section_content"
            fi
            ;;
        "Test plan")
            # Skip if it's CI, template text, or empty
            local lower_content=$(echo "$section_content" | tr '[:upper:]' '[:lower:]')
            local lower_template=$(echo "How did you test the new or updated feature?" | tr '[:upper:]' '[:lower:]')
            if [ -n "$section_content" ] && \
                [ "$lower_content" != "ci" ] && \
                [ "$lower_content" != "$lower_template" ]; then
                echo "## Test plan"
                echo ""
                echo "$section_content"
            fi
            ;;
        *)
            # Keep other sections as-is if they have content
            if [ -n "$section_content" ]; then
                echo "$section_header"
                echo ""
                echo "$section_content"
            fi
            ;;
    esac
}

# Process body line by line
PROCESSED_BODY=""
CURRENT_SECTION=""
CURRENT_CONTENT=""
CURRENT_HEADER=""

while IFS= read -r line; do
    # Stop at Release notes or ---
    if echo "$line" | grep -q "^## Release notes" || [ "$line" = "---" ]; then
        # Process final section if any
        if [ -n "$CURRENT_SECTION" ]; then
            result=$(process_section "$CURRENT_SECTION" "$CURRENT_CONTENT" "$CURRENT_HEADER")
            if [ -n "$result" ]; then
                [ -n "$PROCESSED_BODY" ] && PROCESSED_BODY+=$'\n\n'
                PROCESSED_BODY+="$result"
            fi
        fi
        break
    fi
    
    # Check if entering new section
    if echo "$line" | grep -q "^##"; then
        # Process previous section if any
        if [ -n "$CURRENT_SECTION" ]; then
            result=$(process_section "$CURRENT_SECTION" "$CURRENT_CONTENT" "$CURRENT_HEADER")
            if [ -n "$result" ]; then
                [ -n "$PROCESSED_BODY" ] && PROCESSED_BODY+=$'\n\n'
                PROCESSED_BODY+="$result"
            fi
        fi
        
        # Start new section
        CURRENT_HEADER="$line"
        if echo "$line" | grep -qi "^## Description"; then
            CURRENT_SECTION="Description"
        elif echo "$line" | grep -qi "^## Test plan"; then
            CURRENT_SECTION="Test plan"
        else
            CURRENT_SECTION="Other"
        fi
        CURRENT_CONTENT=""
    else
        # Accumulate content
        if [ -n "$CURRENT_SECTION" ]; then
            [ -n "$CURRENT_CONTENT" ] && CURRENT_CONTENT+=$'\n'
            CURRENT_CONTENT+="$line"
        else
            # Content before any section
            [ -n "$PROCESSED_BODY" ] && PROCESSED_BODY+=$'\n'
            PROCESSED_BODY+="$line"
        fi
    fi
done <<< "$BODY"

# Handle final section if we didn't hit Release notes/---
if [ -n "$CURRENT_SECTION" ]; then
    result=$(process_section "$CURRENT_SECTION" "$CURRENT_CONTENT" "$CURRENT_HEADER")
    if [ -n "$result" ]; then
        [ -n "$PROCESSED_BODY" ] && PROCESSED_BODY+=$'\n\n'
        PROCESSED_BODY+="$result"
    fi
fi

# Extract checked release notes
CHECKED_NOTES=""
if echo "$BODY" | grep -q "^## Release notes"; then
    CHECKED_NOTES=$(echo "$BODY" | awk '
        /^## Release notes/ { in_notes=1; next }
        in_notes && /^- \[[xX]\]/ { print }
    ')
fi

# Build final message
FULL_MSG="${COMMIT_TITLE}"

# Add body and release notes if they exist
PROCESSED_BODY=$(echo "$PROCESSED_BODY" | sed '/^[[:space:]]*$/d')
[ -n "$PROCESSED_BODY" ] && FULL_MSG+=$'\n\n'"$PROCESSED_BODY"
[ -n "$CHECKED_NOTES" ] && FULL_MSG+=$'\n\n## Release notes\n'"$CHECKED_NOTES"

# Save to file to avoid issues with special characters
echo "$FULL_MSG" > /tmp/commit_message.txt
