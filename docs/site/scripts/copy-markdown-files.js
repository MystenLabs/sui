// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const fs = require('fs');
const path = require('path');
const matter = require('gray-matter');

const contentDir = path.join(__dirname, '../../content');
const outputDir = path.join(__dirname, '../build/markdown');

/**
 * Strips frontmatter and writes a metadata sidecar for llms.txt
 */
function stripFrontmatter(content, outputPath) {
  const { content: markdownContent, data } = matter(content);
  const cleaned = cleanMdxComponents(markdownContent);

  // Write metadata sidecar for llms.txt
  const meta = {};
  if (data.title) meta.title = data.title;
  if (data.description) meta.description = data.description;
  if (Object.keys(meta).length) {
    const metaPath = outputPath.replace(/\.md$/, '.meta.json');
    fs.writeFileSync(metaPath, JSON.stringify(meta), 'utf8');
  }

  return cleaned;
}

/**
 * Removes or simplifies MDX/JSX components for cleaner markdown
 */
function cleanMdxComponents(content) {
  let cleaned = content;

  // Remove import statements
  cleaned = cleaned.replace(/^import\s+.*?from\s+['"].*?['"];?\s*$/gm, '');

  // Convert Card components to markdown links
  cleaned = cleaned.replace(/<Card[^>]*title="([^"]*)"[^>]*href="([^"]*)"[^>]*\/>/g, '- [$1]($2)');

  // Remove Cards wrapper
  cleaned = cleaned.replace(/<Cards[^>]*>/g, '');
  cleaned = cleaned.replace(/<\/Cards>/g, '');

  // Remove other common JSX components but keep their content
  cleaned = cleaned.replace(/<(\w+)[^>]*>(.*?)<\/\1>/gs, '$2');

  // Remove self-closing JSX tags
  cleaned = cleaned.replace(/<\w+[^>]*\/>/g, '');

  // Replace .mdx with .md in markdown links to ensure internal links work
  cleaned = cleaned.replace(/\[([^\]]+)\]\(([^)]+)\.mdx((?:#[^)]*)?)\)/g, '[$1]($2.md$3)');

  // Clean up excessive newlines
  cleaned = cleaned.replace(/\n{3,}/g, '\n\n');

  return cleaned.trim();
}

/**
 * Recursively copies markdown files from content dir to build output
 */
function copyMarkdownFiles(dir, baseDir = dir) {
  const files = fs.readdirSync(dir);

  files.forEach(file => {
    const filePath = path.join(dir, file);
    const stat = fs.statSync(filePath);

    if (stat.isDirectory()) {
      copyMarkdownFiles(filePath, baseDir);
    } else if (file.endsWith('.md') || file.endsWith('.mdx')) {
      const content = fs.readFileSync(filePath, 'utf8');

      // Preserve directory structure
      const relativePath = path.relative(baseDir, filePath);
      // Normalize all files to .md extension
      const outputPath = path.join(outputDir, relativePath.replace(/\.mdx?$/, '.md'));

      // Create directory structure if it doesn't exist
      fs.mkdirSync(path.dirname(outputPath), { recursive: true });

      // Strip frontmatter, write sidecar, write cleaned content
      const cleanContent = stripFrontmatter(content, outputPath);
      fs.writeFileSync(outputPath, cleanContent, 'utf8');
    }
  });
}

// Create output directory
fs.mkdirSync(outputDir, { recursive: true });

// Copy all markdown files
copyMarkdownFiles(contentDir);

console.log('\n✅ Markdown files exported successfully');