# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

#!/bin/bash
# scripts/update-subtrees.sh
# Updates all git subtrees in the repository

set -e

# Define your subtrees here: "prefix|remote_url|branch"
SUBTREES=(
  "docs/site/src/shared|https://github.com/jessiemongeon1/ML-Shared-Docusaurus.git|master"
  "docs/subtree/awesome-sui|https://github.com/sui-foundation/awesome-sui.git|main"

  # Add more subtrees as needed:
  # "path/to/subtree|https://github.com/org/repo.git|main"
)

echo "Updating all subtrees..."

for subtree in "${SUBTREES[@]}"; do
  IFS='|' read -r prefix remote branch <<< "$subtree"
  echo ""
  echo "=== Updating $prefix from $remote ($branch) ==="
  git subtree pull --prefix="$prefix" "$remote" "$branch" --squash
done

echo ""
echo "✓ All subtrees updated!"
