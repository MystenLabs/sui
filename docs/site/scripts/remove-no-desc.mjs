// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import path from "node:path";

const ROOT = process.argv[2] ?? "docs"; // pass your generated folder if different

function walk(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(p));
    else out.push(p);
  }
  return out;
}

const files = walk(ROOT).filter((f) => f.endsWith(".md") || f.endsWith(".mdx"));

let changed = 0;

for (const file of files) {
  const src = fs.readFileSync(file, "utf8");

  // Remove standalone placeholder lines like:
  // "No description"
  // or markdown paragraphs containing only that
  const next = src
    // remove a line that is just "No description" (optionally bolded)
    .replace(/^\s*(\*\*)?No description(\*\*)?\s*$/gim, "")
    // collapse multiple blank lines created by removals
    .replace(/\n{3,}/g, "\n\n")
    .trimEnd() + "\n";

  if (next !== src) {
    fs.writeFileSync(file, next, "utf8");
    changed++;
  }
}

console.log(`strip-no-description: updated ${changed} file(s) under ${ROOT}`);