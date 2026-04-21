// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const fs = require('fs');
const path = require('path');
const matter = require('gray-matter');

const contentDir = path.join(__dirname, '../../content');
const outputDir = path.join(__dirname, '../build/markdown');
const repoRoot = path.join(__dirname, '../../../..');
const snippetsDir = path.join(contentDir, 'snippets');

// ── Snippet resolution ──────────────────────────────────────────────────────

function resolveSnippet(source) {
  const candidates = [
    path.join(snippetsDir, source),
    path.join(snippetsDir, source + '.mdx'),
    path.join(snippetsDir, source + '.md'),
  ];

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      const raw = fs.readFileSync(candidate, 'utf8');
      const { content } = matter(raw);
      return content.trim();
    }
  }

  // Try subdirectories (e.g., "console-output/sui-client-help")
  const subPath = path.join(snippetsDir, source);
  const subCandidates = [subPath + '.mdx', subPath + '.md'];
  for (const candidate of subCandidates) {
    if (fs.existsSync(candidate)) {
      const raw = fs.readFileSync(candidate, 'utf8');
      const { content } = matter(raw);
      return content.trim();
    }
  }

  return null;
}

// ── Code file resolution ────────────────────────────────────────────────────

function resolveCodeFile(source) {
  const filePath = path.join(repoRoot, source);
  if (fs.existsSync(filePath)) {
    return fs.readFileSync(filePath, 'utf8');
  }
  return null;
}

function extractLines(content, linesAttr) {
  if (!linesAttr) return content;
  const match = linesAttr.match(/^(\d+)-(\d+)$/);
  if (!match) return content;
  const start = parseInt(match[1], 10) - 1;
  const end = parseInt(match[2], 10);
  return content.split('\n').slice(start, end).join('\n');
}

function extractFunction(content, funcName, lang) {
  const names = funcName.split(',').map((n) => n.trim());
  const results = [];

  for (const name of names) {
    // Try Move-style: (public )?(entry )?fun name
    const moveRe = new RegExp(
      `((?:public\\s+)?(?:entry\\s+)?fun\\s+${name}\\b[\\s\\S]*?\\n\\})`,
      'm',
    );
    const moveMatch = content.match(moveRe);
    if (moveMatch) {
      results.push(moveMatch[1]);
      continue;
    }

    // Try Rust/TS-style: fn/function/const name
    const genericRe = new RegExp(
      `((?:pub\\s+)?(?:async\\s+)?(?:fn|function)\\s+${name}\\b[\\s\\S]*?\\n\\})`,
      'm',
    );
    const genericMatch = content.match(genericRe);
    if (genericMatch) {
      results.push(genericMatch[1]);
      continue;
    }

    // Try arrow function: const name =
    const arrowRe = new RegExp(
      `((?:export\\s+)?const\\s+${name}\\s*=\\s*[\\s\\S]*?\\n\\};?)`,
      'm',
    );
    const arrowMatch = content.match(arrowRe);
    if (arrowMatch) {
      results.push(arrowMatch[1]);
    }
  }

  return results.length ? results.join('\n\n') : null;
}

function extractStruct(content, structName) {
  const re = new RegExp(
    `((?:public\\s+)?struct\\s+${structName}\\b[\\s\\S]*?\\n\\})`,
    'm',
  );
  const match = content.match(re);
  return match ? match[1] : null;
}

function guessLanguage(source) {
  const ext = path.extname(source).toLowerCase();
  const langMap = {
    '.move': 'move',
    '.rs': 'rust',
    '.ts': 'typescript',
    '.tsx': 'typescript',
    '.js': 'javascript',
    '.jsx': 'javascript',
    '.json': 'json',
    '.toml': 'toml',
    '.yaml': 'yaml',
    '.yml': 'yaml',
    '.md': 'markdown',
    '.sh': 'bash',
  };
  return langMap[ext] || '';
}

// ── ImportContent handler ───────────────────────────────────────────────────

function expandImportContent(fullMatch) {
  const attrStr = fullMatch;

  const getAttr = (name) => {
    const re = new RegExp(`${name}=(?:"([^"]*)"|\\{([^}]*)\\})`);
    const m = attrStr.match(re);
    return m ? m[1] || m[2] : null;
  };

  const source = getAttr('source');
  const mode = getAttr('mode');
  const fun = getAttr('fun');
  const struct = getAttr('struct');
  const lines = getAttr('lines');
  const style = getAttr('style');
  const org = getAttr('org');

  if (!source || !mode) return '';

  if (mode === 'snippet') {
    const resolved = resolveSnippet(source);
    return resolved ? `\n${resolved}\n` : '';
  }

  if (mode === 'code') {
    // Skip external GitHub sources — we can't resolve them at build time
    if (org) {
      return `\n<!-- External code reference: ${source} -->\n`;
    }

    let content = resolveCodeFile(source);
    if (!content) return `\n<!-- Code file not found: ${source} -->\n`;

    // Apply extraction filters
    if (lines) {
      content = extractLines(content, lines);
    } else if (fun) {
      const extracted = extractFunction(content, fun, guessLanguage(source));
      if (extracted) content = extracted;
    } else if (struct) {
      const extracted = extractStruct(content, struct);
      if (extracted) content = extracted;
    }

    // If style is markdown, inline directly
    if (style === 'md' || style === 'markdown') {
      return `\n${content.trim()}\n`;
    }

    const lang = guessLanguage(source);
    return `\n\`\`\`${lang}\n${content.trim()}\n\`\`\`\n`;
  }

  return '';
}

// ── DocCardList handler ─────────────────────────────────────────────────────

function expandDocCardList(filePath) {
  const dir = path.dirname(filePath);
  const entries = [];

  if (!fs.existsSync(dir)) return '';

  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);

    if (entry.isDirectory()) {
      // Look for index file in subdirectory
      const indexFile =
        ['.mdx', '.md']
          .map((ext) => path.join(fullPath, `index${ext}`))
          .find((f) => fs.existsSync(f)) || null;

      if (indexFile) {
        const raw = fs.readFileSync(indexFile, 'utf8');
        const { data } = matter(raw);
        const title = data.title || formatDirName(entry.name);
        const desc = data.description ? ` — ${data.description}` : '';
        entries.push(`- [${title}](${entry.name}/)${desc}`);
      } else {
        entries.push(`- [${formatDirName(entry.name)}](${entry.name}/)`);
      }
    } else if (
      (entry.name.endsWith('.md') || entry.name.endsWith('.mdx')) &&
      entry.name !== 'index.md' &&
      entry.name !== 'index.mdx' &&
      fullPath !== filePath
    ) {
      const raw = fs.readFileSync(fullPath, 'utf8');
      const { data } = matter(raw);
      const slug = entry.name.replace(/\.mdx?$/, '');
      const title = data.title || formatDirName(slug);
      const desc = data.description ? ` — ${data.description}` : '';
      entries.push(`- [${title}](${slug})${desc}`);
    }
  }

  if (!entries.length) return '';
  return '\n' + entries.join('\n') + '\n';
}

function formatDirName(name) {
  return name
    .replace(/[-_]/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

// ── Protocol handler ────────────────────────────────────────────────────────

function expandProtocol() {
  const jsonPath = path.join(contentDir, 'documentation.json');
  if (!fs.existsSync(jsonPath)) return '\n<!-- Protocol specification not available -->\n';

  try {
    const spec = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
    const lines = [];
    lines.push('\n## Protocol Reference\n');

    // Extract files, messages, services, enums from the spec
    const files = spec.files || [];
    for (const file of files) {
      if (file.name) {
        lines.push(`### ${file.name}\n`);
      }
      if (file.description) {
        lines.push(file.description + '\n');
      }

      // Messages
      const messages = file.messages || [];
      for (const msg of messages) {
        lines.push(`#### ${msg.longName || msg.name}\n`);
        if (msg.description) lines.push(msg.description + '\n');
        if (msg.fields && msg.fields.length) {
          lines.push('| Field | Type | Description |');
          lines.push('|---|---|---|');
          for (const f of msg.fields) {
            const desc = (f.description || '').replace(/\n/g, ' ').replace(/\|/g, '\\|');
            lines.push(`| ${f.name} | ${f.type || f.fullType || ''} | ${desc} |`);
          }
          lines.push('');
        }
      }

      // Services
      const services = file.services || [];
      for (const svc of services) {
        lines.push(`#### Service: ${svc.name}\n`);
        if (svc.description) lines.push(svc.description + '\n');
        if (svc.methods && svc.methods.length) {
          lines.push('| Method | Request | Response | Description |');
          lines.push('|---|---|---|---|');
          for (const m of svc.methods) {
            const desc = (m.description || '').replace(/\n/g, ' ').replace(/\|/g, '\\|');
            lines.push(`| ${m.name} | ${m.requestType || ''} | ${m.responseType || ''} | ${desc} |`);
          }
          lines.push('');
        }
      }

      // Enums
      const enums = file.enums || [];
      for (const en of enums) {
        lines.push(`#### ${en.longName || en.name}\n`);
        if (en.description) lines.push(en.description + '\n');
        if (en.values && en.values.length) {
          lines.push('| Value | Number | Description |');
          lines.push('|---|---|---|');
          for (const v of en.values) {
            const desc = (v.description || '').replace(/\n/g, ' ').replace(/\|/g, '\\|');
            lines.push(`| ${v.name} | ${v.number} | ${desc} |`);
          }
          lines.push('');
        }
      }
    }

    // Scalar types
    if (spec.scalarValueTypes && spec.scalarValueTypes.length) {
      lines.push('### Scalar Value Types\n');
      lines.push('| Type | Notes |');
      lines.push('|---|---|');
      for (const s of spec.scalarValueTypes) {
        const notes = (s.notes || '').replace(/\n/g, ' ');
        lines.push(`| ${s.protoType || s.name || ''} | ${notes} |`);
      }
      lines.push('');
    }

    return lines.join('\n');
  } catch {
    return '\n<!-- Failed to parse protocol specification -->\n';
  }
}

// ── Main processing ─────────────────────────────────────────────────────────

function stripFrontmatter(content, outputPath, filePath) {
  const { content: markdownContent, data } = matter(content);
  const cleaned = cleanMdxComponents(markdownContent, filePath);

  const meta = {};
  if (data.title) meta.title = data.title;
  if (data.description) meta.description = data.description;
  if (Object.keys(meta).length) {
    const metaPath = outputPath.replace(/\.md$/, '.meta.json');
    fs.writeFileSync(metaPath, JSON.stringify(meta, null, 2), 'utf8');
  }

  return cleaned;
}

function cleanMdxComponents(content, filePath) {
  let cleaned = content;

  // Remove import/export lines only.
  cleaned = cleaned.replace(/^\s*import\s+.*?from\s+['"].*?['"];?\s*$/gm, '');
  cleaned = cleaned.replace(/^\s*export\s+(default\s+)?.*$/gm, '');

  // ── Expand ImportContent (must run before generic tag stripping) ─────────
  // Self-closing: <ImportContent ... />
  cleaned = cleaned.replace(
    /<ImportContent\b[^>]*\/>/g,
    (match) => expandImportContent(match),
  );
  // Paired tags (rare but possible): <ImportContent ...>...</ImportContent>
  cleaned = cleaned.replace(
    /<ImportContent\b[^>]*>[\s\S]*?<\/ImportContent>/g,
    (match) => expandImportContent(match),
  );

  // ── Expand DocCardList ──────────────────────────────────────────────────
  cleaned = cleaned.replace(
    /<DocCardList\b[^>]*\/?>/g,
    () => (filePath ? expandDocCardList(filePath) : ''),
  );

  // ── Expand Protocol component ──────────────────────────────────────────
  cleaned = cleaned.replace(/<Protocol\b[^>]*\/?>/g, () => expandProtocol());

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

    const cleanContent = stripFrontmatter(content, outputPath, filePath);
    fs.writeFileSync(outputPath, cleanContent, 'utf8');
  }
}

fs.mkdirSync(outputDir, { recursive: true });
copyMarkdownFiles(contentDir);

console.log('\n✅ Markdown files exported successfully');
