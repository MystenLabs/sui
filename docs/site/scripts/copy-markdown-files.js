// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const fs = require('fs');
const path = require('path');
const matter = require('gray-matter');

const contentDir = path.join(__dirname, '../../content');
const outputDir = path.join(__dirname, '../build/markdown');

function stripFrontmatter(content, outputPath) {
  const { content: markdownContent, data } = matter(content);
  const cleaned = cleanMdxComponents(markdownContent);

  const meta = {};
  if (data.title) meta.title = data.title;
  if (data.description) meta.description = data.description;
  if (Object.keys(meta).length) {
    const metaPath = outputPath.replace(/\.md$/, '.meta.json');
    fs.writeFileSync(metaPath, JSON.stringify(meta, null, 2), 'utf8');
  }

  return cleaned;
}

function cleanMdxComponents(content) {
  let cleaned = content;

  // Remove import/export lines only.
  cleaned = cleaned.replace(/^\s*import\s+.*?from\s+['"].*?['"];?\s*$/gm, '');
  cleaned = cleaned.replace(/^\s*export\s+(default\s+)?.*$/gm, '');

  // Convert common self-closing cards to markdown links.
  cleaned = cleaned.replace(
    /<Card\b[^>]*\btitle="([^"]*)"[^>]*\bhref="([^"]*)"[^>]*\/>/g,
    '\n- [$1]($2)\n',
  );

  // Remove Cards container tags but keep children.
  cleaned = cleaned.replace(/<\/?Cards\b[^>]*>/g, '');

  // Convert admonitions like :::note when authors used JSX-style wrappers.
  cleaned = cleaned.replace(
    /<Admonition\b[^>]*type="([^"]+)"[^>]*>([\s\S]*?)<\/Admonition>/g,
    (_, type, inner) => `\n:::${type}\n${inner.trim()}\n:::\n`,
  );

  // Convert simple details/accordion blocks.
  cleaned = cleaned.replace(
    /<details\b[^>]*>\s*<summary>([\s\S]*?)<\/summary>([\s\S]*?)<\/details>/gi,
    (_, summary, inner) => `\n**${summary.trim()}**\n\n${inner.trim()}\n`,
  );

  // Convert TabItem blocks into labeled sections so content is not lost.
  cleaned = cleaned.replace(
    /<TabItem\b[^>]*label="([^"]*)"[^>]*>([\s\S]*?)<\/TabItem>/g,
    (_, label, inner) => `\n## ${label.trim()}\n\n${inner.trim()}\n`,
  );

  // Remove Tabs wrapper tags but keep tab contents.
  cleaned = cleaned.replace(/<\/?Tabs\b[^>]*>/g, '');

  // Remove common purely decorative/self-closing components.
  cleaned = cleaned.replace(/<\s*(Spacer|Br|Break|Icon|Diagram)\b[^>]*\/>/g, '');

  // Remove a few known wrapper components but keep their content.
  const unwrapTags = [
    'BrowserOnly',
    'Center',
    'Columns',
    'Column',
    'div',
    'span',
    'section',
  ];
  for (const tag of unwrapTags) {
    const re = new RegExp(`<${tag}\\b[^>]*>([\\s\\S]*?)<\\/${tag}>`, 'g');
    cleaned = cleaned.replace(re, '$1');
  }

  // Remove JSX comments.
  cleaned = cleaned.replace(/\{\/\*[\s\S]*?\*\/\}/g, '');

  // Remove bare expression blocks that often break markdown export.
  // Keep this conservative so we do not nuke prose accidentally.
  cleaned = cleaned.replace(/^\s*\{[A-Z][A-Za-z0-9_.]*\}\s*$/gm, '');

  // Normalize internal links.
  cleaned = cleaned.replace(
    /\[([^\]]+)\]\(([^)]+)\.mdx((?:#[^)]*)?)\)/g,
    '[$1]($2.md$3)',
  );
  cleaned = cleaned.replace(
    /\[([^\]]+)\]\(([^)]+)\/index\.md((?:#[^)]*)?)\)/g,
    '[$1]($2/$3)',
  );

  // Clean up excessive blank lines.
  cleaned = cleaned.replace(/[ \t]+\n/g, '\n');
  cleaned = cleaned.replace(/\n{3,}/g, '\n\n');

  return cleaned.trim() + '\n';
}

function copyMarkdownFiles(dir, baseDir = dir) {
  const files = fs.readdirSync(dir);

  for (const file of files) {
    const filePath = path.join(dir, file);
    const stat = fs.statSync(filePath);

    if (stat.isDirectory()) {
      copyMarkdownFiles(filePath, baseDir);
      continue;
    }

    if (!file.endsWith('.md') && !file.endsWith('.mdx')) {
      continue;
    }

    const content = fs.readFileSync(filePath, 'utf8');
    const relativePath = path.relative(baseDir, filePath);
    const outputPath = path.join(outputDir, relativePath.replace(/\.mdx?$/, '.md'));

    fs.mkdirSync(path.dirname(outputPath), { recursive: true });

    const cleanContent = stripFrontmatter(content, outputPath);
    fs.writeFileSync(outputPath, cleanContent, 'utf8');
  }
}

fs.mkdirSync(outputDir, { recursive: true });
copyMarkdownFiles(contentDir);

console.log('\n✅ Markdown files exported successfully');