// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const fs = require('fs');
const path = require('path');
const matter = require('gray-matter');

const contentDir = path.join(__dirname, '../../content');
const outputDir = path.join(__dirname, '../build/markdown');
const repoRoot = path.join(__dirname, '../../..');
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
  const basename = path.basename(filePath, path.extname(filePath));
  const isIndex = basename === 'index';

  // Determine which directory to list:
  // - index files list their sibling pages (same directory)
  // - non-index files list the matching subdirectory (e.g. getting-started.mdx → getting-started/)
  let targetDir;
  if (isIndex) {
    targetDir = dir;
  } else {
    targetDir = path.join(dir, basename);
    if (!fs.existsSync(targetDir) || !fs.statSync(targetDir).isDirectory()) {
      targetDir = dir; // fallback to parent
    }
  }

  const entries = [];
  if (!fs.existsSync(targetDir)) return '';

  for (const entry of fs.readdirSync(targetDir, { withFileTypes: true })) {
    const fullPath = path.join(targetDir, entry.name);

    // Skip snippets directory
    if (entry.name === 'snippets') continue;

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

  // Prepend the title as an h1 so the markdown export matches the HTML page.
  const titlePrefix = data.title ? `# ${data.title}\n\n` : '';
  const cleaned = cleanMdxComponents(titlePrefix + markdownContent, filePath);

  const meta = {};
  if (data.title) meta.title = data.title;
  if (data.description) meta.description = data.description;
  if (Object.keys(meta).length) {
    const metaPath = outputPath.replace(/\.md$/, '.meta.json');
    fs.writeFileSync(metaPath, JSON.stringify(meta, null, 2), 'utf8');
  }

  return cleaned;
}

function stripMultilineExports(content) {
  const lines = content.split('\n');
  const result = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    if (/^\s*export\s+(const|let|var|function|class)\b/.test(line)) {
      // Track brace/paren depth to find the end of the block
      let depth = 0;
      let foundOpen = false;
      while (i < lines.length) {
        const current = lines[i];
        for (const ch of current) {
          if (ch === '{' || ch === '(') { depth++; foundOpen = true; }
          if (ch === '}' || ch === ')') { depth--; }
        }
        i++;
        if (foundOpen && depth <= 0) break;
        // Single-line export with no braces
        if (!foundOpen && (current.endsWith(';') || i >= lines.length)) break;
      }
      continue;
    }
    result.push(line);
    i++;
  }

  return result.join('\n');
}

function cleanMdxComponents(content, filePath) {
  let cleaned = content;

  // Remove import lines.
  cleaned = cleaned.replace(/^\s*import\s+.*?from\s+['"].*?['"];?\s*$/gm, '');

  // Remove multi-line export blocks (e.g. export const Component = () => { ... };)
  cleaned = stripMultilineExports(cleaned);

  // Remove any remaining single-line export statements.
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

  // Convert ToolCard components to markdown list items with descriptions.
  cleaned = cleaned.replace(
    /<ToolCard\b([^>]*?)\/>/gs,
    (_, attrs) => {
      const name = attrs.match(/\bname="([^"]*)"/)?.[1] || '';
      const desc = attrs.match(/\bdescription="([^"]*)"/)?.[1] || '';
      const docs = attrs.match(/\bdocs="([^"]*)"/)?.[1];
      const website = attrs.match(/\bwebsite="([^"]*)"/)?.[1];
      const github = attrs.match(/\bgithub="([^"]*)"/)?.[1];
      const link = docs || website || github || '';
      const linkText = link ? `[${name}](${link})` : `**${name}**`;
      return desc ? `\n- ${linkText}: ${desc}\n` : `\n- ${linkText}\n`;
    },
  );

  // Remove ToolGrid wrapper tags but keep children.
  cleaned = cleaned.replace(/<\/?ToolGrid\b[^>]*>/g, '');

  // Convert Badge to inline text.
  cleaned = cleaned.replace(
    /<Badge\b[^>]*\btext="([^"]*)"[^>]*\/>/g,
    '`$1`',
  );

  // Remove Bullet spacer components.
  cleaned = cleaned.replace(/<Bullet\s*\/>/g, ' ');

  // Convert ImportContent to a placeholder noting external content.
  cleaned = cleaned.replace(
    /<ImportContent\b([^>]*?)\/>/g,
    (_, attrs) => {
      const source = attrs.match(/\bsource="([^"]*)"/)?.[1] || '';
      const mode = attrs.match(/\bmode="([^"]*)"/)?.[1] || '';
      if (mode === 'code' && source) {
        return `\n\`\`\`\n// Source: ${source}\n\`\`\`\n`;
      }
      return '';
    },
  );

  // Remove inline <style> blocks (JSX CSS).
  cleaned = cleaned.replace(/<style>\{`[\s\S]*?`\}<\/style>/g, '');
  cleaned = cleaned.replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '');

  // Convert inline JSX <code> with style attributes to backtick code.
  // Handle nested HTML (e.g. <b>) inside <code> by stripping inner tags.
  cleaned = cleaned.replace(
    /<code\b[^>]*>([\s\S]*?)<\/code>/g,
    (_, inner) => `\`${inner.replace(/<\/?[a-z][^>]*>/gi, '')}\``,
  );

  // Remove common purely decorative/self-closing components.
  cleaned = cleaned.replace(/<\s*(Spacer|Br|Break|Icon|Diagram|IconButton)\b[^>]*\/>/g, '');

  // Remove a few known wrapper components but keep their content.
  const unwrapTags = [
    'BrowserOnly',
    'Center',
    'Columns',
    'Column',
    'ToolGrid',
    'SearchProvider',
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

  // ── HTML-to-markdown cleanup (for framework-generated pages) ────────────

  // Convert <span class="code-inline">X</span> to `X`
  cleaned = cleaned.replace(
    /<span\s+class="code-inline">([^<]*)<\/span>/g,
    '`$1`',
  );

  // Convert <pre><code>...</code></pre> to fenced code blocks.
  cleaned = cleaned.replace(
    /<pre><code>([\s\S]*?)<\/code><\/pre>/g,
    (_, inner) => {
      // Strip HTML tags inside code blocks for readability
      const plain = inner.replace(/<\/?[a-z][^>]*>/gi, '');
      return `\n\`\`\`\n${plain.trim()}\n\`\`\`\n`;
    },
  );

  // Convert <h2 id="...">text</h2> etc. to markdown headings
  cleaned = cleaned.replace(/<h([1-6])\b[^>]*>([\s\S]*?)<\/h\1>/g, (_, level, text) => {
    const plain = text.replace(/<\/?[a-z][^>]*>/gi, '').trim();
    return '\n' + '#'.repeat(parseInt(level)) + ' ' + plain + '\n';
  });

  // Convert <dl>/<dt>/<dd> definition lists to bold+text
  cleaned = cleaned.replace(/<dl>([\s\S]*?)<\/dl>/g, (_, inner) => {
    let result = inner;
    result = result.replace(/<dt>([\s\S]*?)<\/dt>/g, (__, dt) => {
      const plain = dt.replace(/<\/?[a-z][^>]*>/gi, '').trim();
      return `\n**${plain}**`;
    });
    result = result.replace(/<dd>([\s\S]*?)<\/dd>/g, (__, dd) => {
      return `\n${dd.replace(/<\/?[a-z][^>]*>/gi, '').trim()}\n`;
    });
    return result;
  });

  // Convert <a href="...">text</a> to [text](href)
  cleaned = cleaned.replace(
    /<a\s+href="([^"]*)"[^>]*>([\s\S]*?)<\/a>/g,
    (_, href, text) => `[${text.replace(/<\/?[a-z][^>]*>/gi, '')}](${href})`,
  );

  // Convert <b>text</b> and <strong>text</strong> to **text**
  cleaned = cleaned.replace(/<(?:b|strong)>([\s\S]*?)<\/(?:b|strong)>/g, '**$1**');

  // Convert <em>text</em> and <i>text</i> to *text*
  cleaned = cleaned.replace(/<(?:em|i)>([\s\S]*?)<\/(?:em|i)>/g, '*$1*');

  // Convert <code>text</code> (inline) to `text`
  cleaned = cleaned.replace(/<code>([\s\S]*?)<\/code>/g, '`$1`');

  // Convert <br/> and <br> to newlines
  cleaned = cleaned.replace(/<br\s*\/?>/g, '\n');

  // Remove remaining HTML block-level tags but keep content
  cleaned = cleaned.replace(/<\/?(?:div|p|ul|ol|li|table|thead|tbody|tr|td|th|nav|header|footer|article|aside|figure|figcaption)\b[^>]*>/g, '');

  // Remove any remaining self-closing HTML tags
  cleaned = cleaned.replace(/<[a-z][^>]*\/>/gi, '');

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
