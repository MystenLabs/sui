// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Plugin processes beta frontmatter.

function effortRemarkPlugin() {
  return (tree, file) => {
    if (file.data.frontMatter && file.data.frontMatter.beta) {
      const betaValue = file.data.frontMatter.beta;
      // Create a new node that represents the custom component
      const customComponentNode = {
        type: "mdxJsxFlowElement",
        name: "BetaTag",
        attributes: [
          {
            type: "mdxJsxAttribute",
            name: "beta",
            value: betaValue,
          },
        ],
        children: [],
      };
      tree.children.unshift(customComponentNode);
    }
  };
}

module.exports = effortRemarkPlugin;
