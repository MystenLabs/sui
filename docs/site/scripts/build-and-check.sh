#!/usr/bin/env bash

LOG=$(mktemp)

# Remove generated framework docs if they exist
FRAMEWORK_DIR="$(dirname "$0")/../content/references/framework"
if [ -d "$FRAMEWORK_DIR" ]; then
  echo "Removing existing framework docs at $FRAMEWORK_DIR"
  rm -rf "$FRAMEWORK_DIR"
fi

pnpm docusaurus build 2>&1 | while IFS= read -r line; do
  echo "$line"
  echo "$line" >> "$LOG"
done

BUILD_EXIT=${PIPESTATUS[0]}

ERRORS=$(grep -iE '\[ERROR\]|fatal|Can'\''t resolve|MDX compilation failed|Missing file for ImportContent|Missing or invalid snippet' "$LOG" || true)

if [ $BUILD_EXIT -ne 0 ] || [ -n "$ERRORS" ]; then
  echo ""
  echo "❌ Build failed or contained errors:"
  echo ""
  echo "$ERRORS"
  echo ""
  grep -iE 'Cause:|"reason":|"message":' "$LOG" || true
  rm -f "$LOG"
  exit 1
fi

rm -f "$LOG"
echo "✅ Build succeeded"