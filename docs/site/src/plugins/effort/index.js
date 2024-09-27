// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Plugin processes example sizes in the frontmatter to
// place a admonition box explaining the rating.

function effortRemarkPlugin() {
  return (tree, file) => {
    if (file.data.frontMatter && file.data.frontMatter.effort) {
      const effortValue = file.data.frontMatter.effort;
      // Create a new node that represents the custom component
      const customComponentNode = {
        type: "mdxJsxFlowElement",
        name: "EffortBox",
        attributes: [
          {
            type: "mdxJsxAttribute",
            name: "effort",
            value: effortValue,
          },
        ],
        children: [],
      };
      tree.children.unshift(customComponentNode);
    }
  };
}

module.exports = effortRemarkPlugin;
