// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { readdir, readFile, stat } from "node:fs/promises";
import { resolve, dirname, join, extname } from "node:path";

// ── Config ────────────────────────────────────────────────────────────────────

const DOCS_DIR = resolve(process.argv[2] ?? "./docs");
const EXTENSIONS = new Set([".mdx", ".md"]);

// Files to skip entirely (relative to DOCS_DIR, forward slashes).
const EXCLUDED_FILES = new Set([
  "references/release-notes.mdx",
]);

// File extensions to try when resolving a link target.
// Order matters — first match wins.
const RESOLUTION_EXTENSIONS = [
  "",         // exact match (already has extension)
  ".mdx",
  ".md",
  "/index.mdx",
  "/index.md",
];

// ── File discovery ────────────────────────────────────────────────────────────

async function findFiles(dir, exts) {
  const results = [];
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      // Skip hidden dirs, node_modules, build output
      if (entry.name.startsWith(".") || entry.name === "node_modules" || entry.name === "build") {
        continue;
      }
      results.push(...(await findFiles(full, exts)));
    } else if (exts.has(extname(entry.name).toLowerCase())) {
      results.push(full);
    }
  }
  return results;
}

// ── Link extraction ───────────────────────────────────────────────────────────

/**
 * Extracts relative links from markdown/MDX content.
 *
 * Captures:
 *  - Standard markdown links: [text](./path) [text](../path) [text](path)
 *  - Reference-style definitions: [id]: ./path
 *  - JSX/HTML href attributes: href="./path" or to="./path"
 *
 * Ignores:
 *  - Absolute URLs (http://, https://, //)
 *  - Root-absolute paths (starting with /)
 *  - Fragment-only links (#section)
 *  - mailto:, tel:, javascript:
 *  - Import/require paths (handled by bundler, not file links)
 *  - Links inside code blocks (fenced ``` or indented)
 */
function extractRelativeLinks(content, filePath) {
  const links = [];

  // Strip fenced code blocks so we don't extract links from code examples
  const stripped = content.replace(/```[\s\S]*?```/g, (match) =>
    "\n".repeat((match.match(/\n/g) ?? []).length)
  );

  // Also strip inline code
  const clean = stripped.replace(/`[^`\n]+`/g, "");

  const lines = clean.split("\n");

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const lineNum = i + 1;

    // Pattern 1: Markdown links [text](url)
    // Handles nested brackets in text via non-greedy match
    const mdLinkRegex = /\[(?:[^\]]*)\]\(([^)]+)\)/g;
    let match;
    while ((match = mdLinkRegex.exec(line)) !== null) {
      const raw = match[1].trim().split(/\s+/)[0]; // strip title after space
      if (isRelativeLink(raw)) {
        links.push({ href: raw, line: lineNum, source: match[0] });
      }
    }

    // Pattern 2: Reference-style link definitions [id]: url
    const refRegex = /^\[([^\]]+)\]:\s*(\S+)/;
    const refMatch = line.match(refRegex);
    if (refMatch) {
      const raw = refMatch[2];
      if (isRelativeLink(raw)) {
        links.push({ href: raw, line: lineNum, source: refMatch[0] });
      }
    }

    // Pattern 3: JSX href="..." or to="..."
    const jsxRegex = /(?:href|to)=["']([^"']+)["']/g;
    while ((match = jsxRegex.exec(line)) !== null) {
      const raw = match[1];
      if (isRelativeLink(raw)) {
        links.push({ href: raw, line: lineNum, source: match[0] });
      }
    }
  }

  return links;
}

function isRelativeLink(href) {
  if (!href) return false;
  // Skip absolute URLs
  if (/^(https?:\/\/|\/\/)/i.test(href)) return false;
  // Skip root-absolute paths — these depend on routing config, not file layout
  if (href.startsWith("/")) return false;
  // Skip special protocols
  if (/^(#|mailto:|tel:|javascript:|data:)/i.test(href)) return false;
  // Skip import-style paths (typically in MDX import statements)
  if (/^@/.test(href)) return false;
  return true;
}

// ── Link resolution ───────────────────────────────────────────────────────────

async function fileExists(p) {
  try {
    const s = await stat(p);
    return s.isFile() || s.isDirectory();
  } catch {
    return false;
  }
}

/**
 * Tries to resolve a relative link to an actual file on disk.
 * Returns the resolved path if found, or null if broken.
 */
async function resolveLink(href, sourceFile) {
  // Strip fragment
  const pathPart = href.split("#")[0];
  if (!pathPart) return href; // fragment-only after stripping is fine

  const sourceDir = dirname(sourceFile);
  const target = resolve(sourceDir, pathPart);

  // Try each resolution strategy
  for (const ext of RESOLUTION_EXTENSIONS) {
    const candidate = target + ext;
    if (await fileExists(candidate)) {
      return candidate;
    }
  }

  // Also try stripping any existing extension and re-adding
  const withoutExt = target.replace(/\.(mdx?|MDX?)$/, "");
  if (withoutExt !== target) {
    for (const ext of RESOLUTION_EXTENSIONS) {
      const candidate = withoutExt + ext;
      if (await fileExists(candidate)) {
        return candidate;
      }
    }
  }

  return null;
}

// ── Main ──────────────────────────────────────────────────────────────────────

async function main() {
  console.log(`\nScanning for .md/.mdx files in: ${DOCS_DIR}\n`);

  let files;
  try {
    files = await findFiles(DOCS_DIR, EXTENSIONS);
  } catch (err) {
    console.error(`Error: Could not read directory "${DOCS_DIR}"`);
    console.error(`  ${err.message}`);
    process.exit(1);
  }

  console.log(`Found ${files.length} files to check.\n`);

  let totalLinks = 0;
  let brokenLinks = 0;
  const brokenByFile = new Map();

  for (const file of files) {
    // Skip excluded files
    const relPath = file.replace(DOCS_DIR + "/", "").replace(/\\/g, "/");
    if (EXCLUDED_FILES.has(relPath)) continue;

    const content = await readFile(file, "utf-8");
    const links = extractRelativeLinks(content, file);

    for (const link of links) {
      totalLinks++;
      const resolved = await resolveLink(link.href, file);

      if (resolved === null) {
        brokenLinks++;
        const relFile = file.replace(DOCS_DIR + "/", "");
        if (!brokenByFile.has(relFile)) {
          brokenByFile.set(relFile, []);
        }
        brokenByFile.get(relFile).push(link);
      }
    }
  }

  // ── Report ──

  if (brokenByFile.size === 0) {
    console.log(`✅ All ${totalLinks} relative links are valid.\n`);
    process.exit(0);
  }

  console.log(`❌ Found ${brokenLinks} broken link${brokenLinks === 1 ? "" : "s"} across ${brokenByFile.size} file${brokenByFile.size === 1 ? "" : "s"}:\n`);

  for (const [file, links] of brokenByFile) {
    console.log(`  ${file}`);
    for (const link of links) {
      console.log(`    Line ${link.line}: ${link.href}`);
    }
    console.log();
  }

  console.log(`Summary: ${brokenLinks} broken / ${totalLinks} total relative links.\n`);
  process.exit(1);
}

main();