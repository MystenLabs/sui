// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Creates duplicate .md files for directory index pages so that Vercel rewrites
// can resolve both /markdown/foo.md and /markdown/foo/index.md.
// The Accept: text/markdown rewrite maps /foo → /markdown/foo.md, but index
// pages live at /markdown/foo/index.md. Copying index.md up one level as
// foo.md ensures both paths work.
//
// IMPORTANT: This script must run AFTER generate-llmstxt.mjs so the llms.txt
// index does not include these duplicate entries.

const fs = require('fs');
const path = require('path');

const markdownDir = path.join(__dirname, '../build/markdown');

function createIndexDuplicates(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const indexFile = path.join(fullPath, 'index.md');
      const duplicate = fullPath + '.md';
      if (fs.existsSync(indexFile) && !fs.existsSync(duplicate)) {
        fs.copyFileSync(indexFile, duplicate);
      }
      createIndexDuplicates(fullPath);
    }
  }
}

if (fs.existsSync(markdownDir)) {
  createIndexDuplicates(markdownDir);

  // Also handle top-level section pages that have matching directories.
  // e.g. develop.md exists AND develop/ exists — ensure develop/index.md exists.
  for (const entry of fs.readdirSync(markdownDir, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith('.md')) continue;
    const name = entry.name.replace(/\.md$/, '');
    const subdir = path.join(markdownDir, name);
    if (fs.existsSync(subdir) && fs.statSync(subdir).isDirectory()) {
      const indexPath = path.join(subdir, 'index.md');
      if (!fs.existsSync(indexPath)) {
        fs.copyFileSync(path.join(markdownDir, entry.name), indexPath);
      }
    }
  }

  console.log('✅ Markdown index duplicates created');
} else {
  console.log('⚠ Markdown directory not found, skipping index duplicates');
}
