#!/bin/bash
cd "$(git rev-parse --show-toplevel)" || exit 1

git subtree pull --prefix=docs/site/src/shared git@github.com:MystenLabs/ML-Shared-Docusaurus.git master --squash
git subtree pull --prefix=docs/subtree/awesome-sui git@github.com:sui-foundation/awesome-sui.git main --squash
git subtree pull --prefix=docs/subtree/awesome-gaming git@github.com:becky-sui/awesome-sui-gaming.git main --squash

echo "✅ All subtree content updated — commit and push the changes"