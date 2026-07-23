// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Remark plugin that injects description and keywords into auto-generated
// GraphQL reference pages. These pages are created by @graphql-markdown/docusaurus
// and identified by the `isGraphQlBeta` frontmatter flag.

function graphqlFrontmatterPlugin() {
  return (_tree, file) => {
    const fm = file.data.frontMatter;
    if (!fm || !fm.isGraphQlBeta) {
      return;
    }

    const title = fm.title || fm.id || "";

    // Derive the GraphQL category from the file path.
    // Paths look like: references/sui-api/sui-graphql/beta/reference/operations/queries/address.mdx
    //                   references/sui-api/sui-graphql/beta/reference/types/objects/gas-coin.mdx
    const filePath = file.history?.[0] || "";
    let category = "type";
    if (filePath.includes("/operations/queries/")) {
      category = "query";
    } else if (filePath.includes("/operations/mutations/")) {
      category = "mutation";
    } else if (filePath.includes("/operations/directives/") || filePath.includes("/types/directives/")) {
      category = "directive";
    } else if (filePath.includes("/types/enums/")) {
      category = "enum";
    } else if (filePath.includes("/types/inputs/")) {
      category = "input type";
    } else if (filePath.includes("/types/interfaces/")) {
      category = "interface";
    } else if (filePath.includes("/types/objects/")) {
      category = "object type";
    } else if (filePath.includes("/types/scalars/")) {
      category = "scalar";
    } else if (filePath.includes("/types/unions/")) {
      category = "union type";
    }

    if (!fm.description) {
      fm.description = `Reference documentation for the ${title} ${category} in the Sui GraphQL API.`;
    }

    if (!fm.keywords) {
      fm.keywords = [
        "sui graphql",
        "graphql api",
        title,
        category,
        "sui graphql reference",
      ];
    }
  };
}

module.exports = graphqlFrontmatterPlugin;
