// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//
// Replaces the @docusaurus-plugin-includes feature, which is not compatible with Docusaurus 3.8
// Expands {@include: path} and {@inject: path[#selector] [options...]} BEFORE the default remark plugins,
// so admonitions/tables/etc. still parse.
//
// - {@include: ...} injects raw markdown (parsed with MDX+GFM+Directives).
// - {@inject: ...} injects formatted fenced code blocks, using simple language mapping.
//

const fs = require("fs");
const path = require("path");
const https = require("https");

// Visit with ancestor stack so we can hoist block replacements correctly
const { visitParents } = require("unist-util-visit-parents");

// Core MD parsing + MDX/GFM adapters
const { fromMarkdown } = require("mdast-util-from-markdown");
const { mdxFromMarkdown } = require("mdast-util-mdx");
const { mdxjs } = require("micromark-extension-mdxjs");
const { gfm } = require("micromark-extension-gfm");
const { gfmFromMarkdown } = require("mdast-util-gfm");

// Directives so :::tip/:::info become containerDirective nodes
const { directive } = require("micromark-extension-directive");
const { directiveFromMarkdown } = require("mdast-util-directive");

// Redirect-aware HTTPS fetch that concatenates chunks
function fetchHttps(url) {
  const maxRedirects = 5;
  return new Promise((resolve, reject) => {
    const go = (target, redirects = 0) => {
      https
        .get(target, (res) => {
          if (
            [301, 302, 303, 307, 308].includes(res.statusCode || 0) &&
            res.headers.location
          ) {
            if (redirects >= maxRedirects) {
              return reject(new Error("Too many redirects: " + target));
            }
            return go(res.headers.location, redirects + 1);
          }
          if (res.statusCode !== 200) {
            console.error(
              `[remark-includes] Failed to fetch ${target}: ${res.statusCode}`,
            );
            return resolve(`Error loading content (status ${res.statusCode})`);
          }
          res.setEncoding("utf8");
          let data = "";
          res.on("data", (chunk) => (data += chunk));
          res.on("end", () => resolve(data));
        })
        .on("error", (err) => reject(err));
    };
    go(url);
  });
}

// Build local path
function buildPath(spec, filePath, docsDir) {
  if (/^https?:\/\//i.test(spec)) return spec;

  const parts = spec.split("/");

  if (spec.startsWith("./") || spec.startsWith("../")) {
    return path.resolve(path.dirname(filePath), spec);
  }
  if (spec.startsWith("/")) {
    return path.resolve(docsDir, "." + spec);
  }
  return path.resolve(docsDir, spec);
}

function languageFromExt(file) {
  const ext = (file.split(".").pop() || "").toLowerCase();
  switch (ext) {
    case "lock":
      return "toml";
    case "sh":
      return "shell";
    case "mdx":
      return "markdown";
    case "tsx":
      return "ts";
    case "rs":
      return "rust";
    case "move":
      return "move";
    case "prisma":
      return "ts";
    default:
      return ext || "text";
  }
}

// Simple option passthrough hook (no-ops; keep for parity with loader)
function processOptions(text, options) {
  // Check for 'noComments' option to strip comments
  if (options && options.includes('noComments')) {
    // Remove single-line comments for Move language
    text = text.replace(/^\s*\/\/.*$/gm, '');
    // Remove multi-line comments
    text = text.replace(/\/\*[\s\S]*?\*\//g, '');
    // Remove extra blank lines
    text = text.replace(/\n\s*\n\s*\n/g, '\n\n');
  }
  return text;
}

function formatAsFence(language, title, content) {
  if (!language) language = "text";
  const safe = (content || "").replace(/\t/g, "  ");
  return `\`\`\`${language} title="${title}"\n${safe}\n\`\`\``;
}

// Parse markdown into AST nodes so includes are re-parsed by downstream plugins
// Add mdast adapters for: MDX, GFM (tables, task-lists, autolinks), and Directives (:::admonition)
function parseMarkdownToNodes(markdownText) {
  const tree = fromMarkdown(markdownText, {
    extensions: [mdxjs(), gfm(), directive()],
    mdastExtensions: [mdxFromMarkdown(), gfmFromMarkdown(), directiveFromMarkdown()],
  });
  return tree.children || [];
}

// Synchronous read-or-fetch helper
async function readSpec(fullPath) {
  if (/^https?:\/\//.test(fullPath)) {
    return await fetchHttps(fullPath);
  }
  if (!fs.existsSync(fullPath)) {
    return `Error loading content (missing file): ${fullPath}`;
  }
  return fs.readFileSync(fullPath, "utf8").replaceAll("\t", "  ");
}

// --- helper: escape for building safe regexes
function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// Extract content based on selector
function extractContent(content, selector) {
  if (!selector) return content;

  const lines = content.split('\n');

  // Handle explicit line-range selector like #L1-L10 or single line #L5
  if (/^#L\d+(?:-L?\d+)?$/.test(selector)) {
    return extractLineRange(lines, selector);
  }

  // Handle function selector like #fun=set_value
  if (selector.startsWith('#fun=')) {
    const functionName = selector.slice(5); // Remove '#fun='
    return extractFunction(lines, functionName);
  }

  // Handle struct selector like #struct=Counter
  if (selector.startsWith('#struct=')) {
    const structName = selector.slice(8); // Remove '#struct='
    return extractStruct(lines, structName);
  }

  // Handle variable selectors like #variable=myCounter or #var=myCounter
  if (selector.startsWith('#variable=')) {
    const variableName = selector.slice(10); // Remove '#variable='
    return extractVariable(lines, variableName);
  }
  if (selector.startsWith('#var=')) {
    const variableName = selector.slice(5); // Remove '#var='
    return extractVariable(lines, variableName);
  }

  // Handle doc-tagged sections like:
  // //docs::/#test
  // #[test]
  // Usage in markdown: {@inject: path#test}
  if (selector.startsWith('#')) {
    const tagName = selector.slice(1);
    return extractTaggedSection(lines, tagName);
  }

  return content;
}

// Extract a specific function from Move code
function extractFunction(lines, functionName) {
  const result = [];
  let inFunction = false;
  let braceLevel = 0;
  let foundFunction = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Look for function definition (public/entry variants too)
    if (
      !inFunction &&
      (line.includes(`fun ${functionName}`) ||
       line.includes(`public fun ${functionName}`) ||
       line.includes(`entry fun ${functionName}`))
    ) {
      inFunction = true;
      foundFunction = true;
      result.push(line);

      // Count braces on the same line
      for (const char of line) {
        if (char === '{') braceLevel++;
        if (char === '}') braceLevel--;
      }
      continue;
    }

    if (inFunction) {
      result.push(line);

      // Count braces to know when function ends
      for (const char of line) {
        if (char === '{') braceLevel++;
        if (char === '}') braceLevel--;
      }

      // If we've closed all braces, the function is complete
      if (braceLevel <= 0) {
        break;
      }
    }
  }

  if (!foundFunction) {
    return `// Function '${functionName}' not found`;
  }

  return result.join('\n');
}

// Extract a specific struct from Move code
function extractStruct(lines, structName) {
  const result = [];
  let inStruct = false;
  let braceLevel = 0;
  let foundStruct = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Look for struct definition
    if (!inStruct && (line.includes(`struct ${structName}`) || line.includes(`public struct ${structName}`))) {
      inStruct = true;
      foundStruct = true;
      result.push(line);

      // Count opening braces on the same line
      for (const char of line) {
        if (char === '{') braceLevel++;
        if (char === '}') braceLevel--;
      }
      continue;
    }

    if (inStruct) {
      result.push(line);

      // Count braces to know when struct ends
      for (const char of line) {
        if (char === '{') braceLevel++;
        if (char === '}') braceLevel--;
      }

      // If we've closed all braces, the struct is complete
      if (braceLevel <= 0) {
        break;
      }
    }
  }

  if (!foundStruct) {
    return `// Struct '${structName}' not found`;
  }

  return result.join('\n');
}

// Extract a specific variable/const declaration from Move code
function extractVariable(lines, variableName) {
  const nameRe = new RegExp(`\\b${escapeRegex(variableName)}\\b`);
  const declStartRe = /\b(let|const)\b/;

  for (let i = 0; i < lines.length; i++) {
    // Quickly skip lines that don't look like a declaration start
    if (!declStartRe.test(lines[i])) continue;

    // Collect the full statement up to the terminating ';'
    let stmt = lines[i];
    while (!stmt.includes(';') && i + 1 < lines.length) {
      i++;
      stmt += '\n' + lines[i];
    }

    // Only consider statements that actually *start* with let/const after trimming
    const trimmed = stmt.replace(/^\s+/, '');
    if (!/^(let|const)\b/.test(trimmed)) continue;

    // Left-hand side of the binding (before '=' or ';')
    const lhs = trimmed.split('=')[0].split(';')[0];

    // Remove the leading 'let' + optional 'mut' to inspect the pattern/binder(s)
    const lhsBinder = lhs.replace(/^let\s+mut\s+|^let\s+/i, '').trim();

    // For const, the binder comes right after 'const'
    const lhsConstBinder = lhs.replace(/^const\s+/i, '').trim();

    // If this is a let-binding, check the binder part for the name (supports tuples and types)
    const matchesLet = /^let\b/i.test(trimmed) && nameRe.test(lhsBinder);
    // If this is a const, check the const name
    const matchesConst = /^const\b/i.test(trimmed) && nameRe.test(lhsConstBinder);

    if (matchesLet || matchesConst) {
      return stmt.trimEnd();
    }
  }

  return `// Variable '${variableName}' not found`;
}

// Extract a doc-tagged section that starts after a marker comment:
//
// //docs::/#tagName
// <section lines to include>
// [stops before the next //docs::/#...] or EOF
//
function extractTaggedSection(lines, tagName) {
  const startPattern = new RegExp(`^\\s*//\\s*docs::/\\s*#${escapeRegex(tagName)}\\s*$`);
  const anyTagPattern = /^\s*\/\/\s*docs::\/\s*#.+$/;

  let startIdx = -1;

  for (let i = 0; i < lines.length; i++) {
    if (startPattern.test(lines[i])) {
      startIdx = i + 1; // start after the marker line
      // skip leading empty lines after the marker
      while (startIdx < lines.length && /^\s*$/.test(lines[startIdx])) {
        startIdx++;
      }
      // collect until next tag or EOF
      const out = [];
      for (let j = startIdx; j < lines.length; j++) {
        if (anyTagPattern.test(lines[j])) break;
        out.push(lines[j]);
      }
      const result = out.join('\n').trimEnd();
      return result.length ? result : `// Section '#${tagName}' is empty`;
    }
  }

  return `// Section '#${tagName}' not found`;
}

// Extract a line range specified as #Lstart-Lend or a single line #Lnum (1-based)
function extractLineRange(lines, selector) {
  // Matches #L5 or #L1-L10 or #L1-L10 (with optional 'L' before the end number)
  const m = selector.match(/^#L(\d+)(?:-L?(\d+))?$/);
  if (!m) return `// Invalid line selector '${selector}'`;

  const start = parseInt(m[1], 10);
  const end = m[2] ? parseInt(m[2], 10) : start;

  if (isNaN(start) || isNaN(end) || start <= 0 || end <= 0) {
    return `// Invalid line numbers in selector '${selector}'`;
  }

  const total = lines.length;
  const s = Math.min(start, end);
  const e = Math.max(start, end);

  if (s > total) return `// Line start ${s} exceeds file length (${total})`;

  const slice = lines.slice(s - 1, Math.min(e, total));
  return slice.join('\n');
}

// Extract "#marker" (now with actual implementation)
function splitPathMarker(spec) {
  const hash = spec.indexOf("#");
  if (hash < 0) return { file: spec, marker: null };
  return { file: spec.slice(0, hash), marker: spec.slice(hash) };
}

// Helper: choose correct container/index to replace a whole block when the directive
// appears as the sole child of a paragraph. This hoists block nodes (admonitions, tables, etc.)
function pickBlockContainer(node, ancestors) {
  const parent = ancestors[ancestors.length - 1];
  const grand = ancestors[ancestors.length - 2];

  // Default: replace the node itself inside its parent
  let container = parent?.children || [];
  let index = container.indexOf(node);

  // If the directive is the only child of a paragraph, replace the entire paragraph
  if (
    parent &&
    parent.type === "paragraph" &&
    Array.isArray(parent.children) &&
    parent.children.length === 1 &&
    grand &&
    Array.isArray(grand.children)
  ) {
    container = grand.children;
    index = container.indexOf(parent);
  }

  return { container, index };
}

// The remark plugin
module.exports = function remarkIncludes(options) {
  const docsDir = options?.docsDir || process.cwd();

  // Match an entire line containing a directive (skip inline code by design)
  const RE_INCLUDE_TXT = /^\s*\{@include:\s*([^\s}]+)\s*\}\s*$/;
  const RE_INCLUDE_HTML =
    /^\s*<!--\s*\{@include:\s*([^\s}]+)\s*\}\s*-->\s*$/;

  const RE_INJECT_TXT =
    /^\s*\{@inject:\s*([^\s}]+)(?:\s+([^}]*?))?\s*\}\s*$/;
  const RE_INJECT_HTML =
    /^\s*<!--\s*\{@inject:\s*([^\s}]+)(?:\s+([^}]*?))?\s*\}\s*-->\s*$/;

  return async (tree, file) => {
    const replacements = [];

    visitParents(tree, (node, ancestors) => {
      const parent = ancestors[ancestors.length - 1];
      if (!parent || (node.type !== "text" && node.type !== "html")) return;

      const value = node.value || "";
      let m;

      if ((m = value.match(RE_INCLUDE_TXT)) || (m = value.match(RE_INCLUDE_HTML))) {
        const spec = m[1].trim();
        const { container, index } = pickBlockContainer(node, ancestors);
        if (index >= 0) {
          replacements.push({
            container,
            index,
            kind: "include",
            spec,
          });
        }
        return;
      }

      if ((m = value.match(RE_INJECT_TXT)) || (m = value.match(RE_INJECT_HTML))) {
        const spec = m[1].trim();
        const rest = (m[2] || "").trim();
        const { container, index } = pickBlockContainer(node, ancestors);
        if (index >= 0) {
          replacements.push({
            container,
            index,
            kind: "inject",
            spec,
            rest,
          });
        }
        return;
      }
    });

    // Perform async replacements in reverse order to keep indices valid
    for (const r of replacements.reverse()) {
      const { container, index, kind, spec } = r;
      const { file: fileSpec, marker } = splitPathMarker(spec);
      const fullPath = buildPath(fileSpec, file.history?.[0] || file.path, docsDir);
      const raw = await readSpec(fullPath);

      // Apply selector if marker exists
      const content = extractContent(raw, marker);

      if (kind === "include") {
        // Parse with directive adapters so :::tip comes through as containerDirective
        const nodes = parseMarkdownToNodes(content);
        container.splice(index, 1, ...nodes);
      } else {
        // inject: create a proper fenced code block by parsing the markdown
        const language = languageFromExt(fileSpec);
        const processedContent = processOptions(content, r.rest?.split(/\s+/) || []);
        const fenced = formatAsFence(language, fileSpec, processedContent);

        // Parse the fenced code block as markdown to get proper AST nodes
        const nodes = parseMarkdownToNodes(fenced);
        container.splice(index, 1, ...nodes);
      }
    }
  };
};
