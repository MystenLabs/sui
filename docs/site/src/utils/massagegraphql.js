// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const fs = require("fs");
const path = require("path");

const filePath = path.resolve(
  __dirname,
  "../../../content/references/sui-api/sui-graphql/beta/reference/types/objects/checkpoint.mdx",
);

let content = fs.readFileSync(filePath, "utf8");

content = content.replace(
  /\[(<code[^>]*><b>Query<\/b><\/code>)\]\([^)]*query\.mdx\)/,
  "$1",
);

fs.writeFileSync(filePath, content, "utf8");

console.log("✅ Patched query link in checkpoint.mdx");
