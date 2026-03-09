#!/usr/bin/env bash

# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

LOG=$(mktemp)

# Remove generated framework docs if they exist
FRAMEWORK_DIR="$(dirname "$0")/../content/references/framework"
if [ -d "$FRAMEWORK_DIR" ]; then
  echo "Removing existing framework docs at $FRAMEWORK_DIR"
  rm -rf "$FRAMEWORK_DIR"
fi

# Pre-build generation steps
echo "Running pre-build generation..."
node scripts/generate-import-context.js || { echo "❌ generate-import-context failed"; exit 1; }
node scripts/grpc-download.js || { echo "❌ grpc-download failed"; exit 1; }
docusaurus graphql-to-doc:beta && node scripts/remove-no-desc.mjs ../content/references/sui-api/sui-graphql/beta/reference || { echo "❌ graphql-to-doc step failed"; exit 1; }
node scripts/getopenrpcspecs.js || { echo "❌ getopenrpcspecs failed"; exit 1; }
node scripts/massagegraphql.js || { echo "❌ massagegraphql failed"; exit 1; }
echo "✅ Pre-build generation complete"

## Build displayV2 app - only download during build process, do not commit files locally

SITE_DIR="$(pwd)"

TEMP_DIR=$(mktemp -d)
git clone --depth 1 https://github.com/MystenLabs/display-preview.git "$TEMP_DIR/display-preview"
cd "$TEMP_DIR/display-preview"
pnpm install
pnpm build
cp -r dist/ "$SITE_DIR/static/display-preview"
cd "$SITE_DIR"
rm -rf "$TEMP_DIR"

## Begin Docusaurus build

docusaurus build 2>&1 | while IFS= read -r line; do
  echo "$line"
  echo "$line" >> "$LOG"
done

## Generate markdown, llms.txt, and check relative link files 
node scripts/copy-markdown-files.js || { echo "❌ copy-markdown-files failed"; exit 1; }
node src/shared/js/generate-llmstxt.mjs build/markdown/ --output static/llms.txt || { echo "❌ generate-llmstxt failed"; exit 1; }
node src/shared/js/check-links.mjs ../content || { echo "❌ generate-llmstxt failed"; exit 1; }

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
