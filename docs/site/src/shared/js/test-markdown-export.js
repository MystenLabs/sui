/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

const fs = require('fs');
const path = require('path');

/**
 * Test script to verify markdown export works correctly
 */

const buildDir = path.join(__dirname, '../build');
const markdownDir = path.join(buildDir, 'markdown');

console.log('🧪 Testing markdown export...\n');

// Check if build directory exists
if (!fs.existsSync(buildDir)) {
  console.error('❌ Build directory does not exist. Run `pnpm build` first.');
  process.exit(1);
}

// Check if markdown directory exists
if (!fs.existsSync(markdownDir)) {
  console.error('❌ Markdown directory does not exist. Build may have failed.');
  process.exit(1);
}

console.log('✅ Build directory exists');
console.log('✅ Markdown directory exists\n');

// Count markdown files
function countMarkdownFiles(dir) {
  let count = 0;
  const files = fs.readdirSync(dir);

  files.forEach(file => {
    const filePath = path.join(dir, file);
    const stat = fs.statSync(filePath);

    if (stat.isDirectory()) {
      count += countMarkdownFiles(filePath);
    } else if (file.endsWith('.md')) {
      count++;
    }
  });

  return count;
}

const markdownCount = countMarkdownFiles(markdownDir);
console.log(`📊 Total markdown files exported: ${markdownCount}\n`);

// Test a few sample files
const samplePaths = [
  'getting-started/index.md',
  'walrus-sites/commands.md',
  'system-overview/quilt.md'
];

console.log('📝 Checking sample files:\n');

samplePaths.forEach(samplePath => {
  const fullPath = path.join(markdownDir, samplePath);
  if (fs.existsSync(fullPath)) {
    const content = fs.readFileSync(fullPath, 'utf8');
    const hasContent = content.length > 0;
    const hasFrontmatter = content.trim().startsWith('---');

    console.log(`  ✓ ${samplePath}`);
    console.log(`    - Size: ${content.length} bytes`);
    console.log(`    - Frontmatter stripped: ${!hasFrontmatter ? '✓' : '✗'}`);
  } else {
    console.log(`  ⚠ ${samplePath} (not found)`);
  }
});

console.log('\n✅ Markdown export test completed');
console.log('\n📖 To test locally:');
console.log('   1. Run: pnpm serve');
console.log('   2. Visit: http://localhost:3000/docs/data-security.md');
console.log('   3. You should see raw markdown content');
