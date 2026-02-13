// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import path from "node:path";

const ROOT = process.argv[2] ?? path.join("docs", "content", "references");

const SKIP_DIRS = new Set([
  "node_modules",
  ".git",
  ".docusaurus",
  "build",
  "dist",
  ".next",
]);

function walk(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory() && SKIP_DIRS.has(entry.name)) continue;

    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(p));
    else out.push(p);
  }
  return out;
}

const rootAbs = path.resolve(ROOT);
if (!fs.existsSync(rootAbs) || !fs.statSync(rootAbs).isDirectory()) {
  console.error(`strip-no-description: root not found or not a directory: ${ROOT}`);
  process.exit(1);
}

const files = walk(rootAbs).filter((f) => f.endsWith(".md") || f.endsWith(".mdx"));

let changed = 0;

for (const file of files) {
  const src = fs.readFileSync(file, "utf8");

  // Fast path: avoid rewriting files that clearly don't contain the placeholder
  if (!src.match(/No description/i)) continue;

  // Remove standalone placeholder lines like:
  // No description
  // **No description**
  //
  // Keep line ending compatibility: handle \n and \r\n
  let next = src.replace(
    // Match a whole line containing only optional bold wrappers + "No description"
    // Capture the newline so we can remove the entire line cleanly.
    /^[ \t]*(\*\*)?No description(\*\*)?[ \t]*(\r?\n|$)/gim,
    ""
  );

  // Collapse excessive blank lines that can result from removals
  next = next.replace(/\r?\n(\s*\r?\n){2,}/g, "\n\n");

  // Ensure file ends with a single newline (nice for POSIX tools)
  next = next.replace(/\s*$/, "\n");

  if (next !== src) {
    fs.writeFileSync(file, next, "utf8");
    changed++;
  }
}

console.log(`strip-no-description: updated ${changed} file(s) under ${ROOT}`);