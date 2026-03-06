// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import fs from "fs";
import path from "path";

// ── CLI args ─────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const flags = {};
const positional = [];

for (let i = 0; i < args.length; i++) {
  if (args[i].startsWith("--")) {
    flags[args[i].slice(2)] = args[i + 1];
    i++;
  } else {
    positional.push(args[i]);
  }
}

const scriptDir = path.dirname(new URL(import.meta.url).pathname);
const markdownDir = path.resolve(positional[0] ?? path.join(scriptDir, "../../static/markdown"));
const baseUrl      = flags["base-url"]    ?? "";
const outputFile = flags["output"] ?? path.join(scriptDir, "../../static/llms.txt");
const siteDesc     = flags["description"] ?? "";

// ── Auto-detect docusaurus config ────────────────────────────────────────────
let resolvedName = flags["name"] ?? null;
let resolvedBaseUrl = baseUrl;

function findDocusaurusConfig(startDir) {
  let dir = startDir;
  for (let i = 0; i < 6; i++) {
    for (const cfg of ["docusaurus.config.js", "docusaurus.config.ts"]) {
      const p = path.join(dir, cfg);
      if (fs.existsSync(p)) return fs.readFileSync(p, "utf8");
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

const configText = findDocusaurusConfig(markdownDir);
if (configText) {
  if (!resolvedName) {
    const m = configText.match(/\btitle:\s*['"](.+?)['"]/);
    if (m) resolvedName = m[1];
  }
  if (!resolvedBaseUrl) {
    const m = configText.match(/\burl:\s*['"](.+?)['"]/);
    if (m) resolvedBaseUrl = m[1];
  }
}
resolvedName ??= "Documentation";

// ── Resolve site description for blockquote ──────────────────────────────────
// Priority: --description flag > Docusaurus tagline
let siteDescription = siteDesc;
if (!siteDescription && configText) {
  const m = configText.match(/\btagline:\s*['"](.+?)['"]/);
  if (m) siteDescription = m[1];
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function walk(dir, results = []) {
  if (!fs.existsSync(dir)) return results;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walk(full, results);
    } else if (entry.name.endsWith(".md") || entry.name.endsWith(".mdx")) {
      results.push(full);
    }
  }
  return results;
}

function parseMarkdown(filePath, content) {
  let title = "";
  let description = "";

  // Check for metadata sidecar written by export script
  const metaPath = filePath.replace(/\.md$/, ".meta.json");
  if (fs.existsSync(metaPath)) {
    try {
      const meta = JSON.parse(fs.readFileSync(metaPath, "utf8"));
      if (meta.title) title = meta.title;
      if (meta.description) description = meta.description;
    } catch {}
  }

  // Strip unwanted HTML before any processing
  let body = content
    .replace(/<a\b[^>]*>[\s\S]*?<\/a>/gi, "")
    .replace(/<span\s+class="code-inline"[^>]*>[\s\S]*?<\/span>/gi, "")
    .replace(/&nbsp;●&nbsp;/g, "")
    .replace(/&nbsp;/g, " ")
    .replace(/&gt;/g, ">")
    .replace(/&lt;/g, "<")
    .replace(/&amp;/g, "&")
    // Strip linear.app issue links: [text](https://linear.app/...) → just text
    .replace(/\[([^\]]*)\]\(https?:\/\/linear\.app\/[^)]*\)/gi, "$1")
    // Strip bare linear.app URLs
    .replace(/https?:\/\/linear\.app\/\S+/gi, "")
    // Strip linear issue references and {/ /} markers
    .replace(/\{[^}]*linear\.app[^}]*\}/gi, "")
    .replace(/\{\/\s*/g, "")
    .replace(/\s*\/\}/g, "");

  // Fallback: first H1
  if (!title) {
    const h1 = body.match(/^#\s+(.+)$/m);
    if (h1) title = h1[1].trim();
  }

  // Fallback description: clean entire body, take first 100 chars of real text
  if (!description) {
    const clean = body
      .replace(/^#+\s+.+$/gm, "")              // remove headings
      .replace(/```[\s\S]*?```/g, "")           // remove code blocks
      .replace(/`[^`]+`/g, "")                 // remove inline code
      .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1") // links → text
      .replace(/[*_]/g, "")                    // remove emphasis
      .replace(/<[^>]+>/g, "")                 // strip remaining HTML
      .replace(/^\s*\d+\.\s+/gm, "")           // remove ordered list markers
      .replace(/^\s*[-*]\s+/gm, "")            // remove unordered list markers
      .replace(/\n+/g, " ")                    // collapse newlines
      .replace(/\s+/g, " ")                    // collapse whitespace
      .trim();

    if (clean.length > 0) {
      const chunk = clean.slice(0, 300);
      // Find the last sentence-ending punctuation within the chunk
      const lastEnd = Math.max(chunk.lastIndexOf(". "), chunk.lastIndexOf("! "), chunk.lastIndexOf("? "));
      if (lastEnd > 0) {
        description = chunk.slice(0, lastEnd + 1).trim();
      } else if (clean.length <= 300) {
        // Entire text fits, use it as-is
        description = clean.trim();
      } else {
        // No sentence boundary found, truncate at last word boundary
        description = chunk.replace(/\s+\S*$/, "").trim();
      }
    }
  }

  // Discard redirect-page descriptions
  if (/redirecting/i.test(description)) description = "";

  return { title, description };
}

function fileToUrlPath(filePath, rootDir) {
  let rel = path.relative(rootDir, filePath).replace(/\\/g, "/");
  rel = rel.replace(/\.mdx?$/, "");
  if (rel === "index" || rel.endsWith("/index")) {
    rel = rel.replace(/\/?index$/, "") || "/";
  }
  return rel || "/";
}

function joinUrl(base, p) {
  if (!base) return "/" + p.replace(/^\//, "");
  return base.replace(/\/$/, "") + "/" + p.replace(/^\//, "");
}

function toSectionTitle(seg) {
  return seg.replace(/[-_]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function isLinearUrl(url) {
  return /linear\.app/i.test(url);
}

// ── Collect pages ─────────────────────────────────────────────────────────────

if (!fs.existsSync(markdownDir)) {
  console.error(`Directory not found: ${markdownDir}`);
  process.exit(1);
}

const files = walk(markdownDir)
  .filter((f) => {
    const rel = path.relative(markdownDir, f).replace(/\\/g, "/");
    return !rel.startsWith("snippets/") && !f.endsWith(".meta.json");
  })
  .sort();

if (!files.length) {
  console.error(`No .md/.mdx files found in: ${markdownDir}`);
  process.exit(1);
}

const pages = [];

for (const file of files) {
  const content = fs.readFileSync(file, "utf8");
  if (!content.trim()) continue; // Skip empty files (e.g., drafts)
  const { title, description } = parseMarkdown(file, content);
  const urlPath = fileToUrlPath(file, markdownDir);

  // Skip /design and /dev-guide sections
  if (/^\/?(design|dev-guide)(\/)/.test(urlPath) || urlPath === "/design" || urlPath === "/dev-guide") continue;

  // Ensure URL path starts with /docs
  const docUrlPath = urlPath.startsWith("/docs") ? urlPath : "/docs" + (urlPath.startsWith("/") ? urlPath : "/" + urlPath);
  const url = joinUrl(resolvedBaseUrl, docUrlPath) + ".md";

  // Skip linear.app URLs
  if (isLinearUrl(url)) continue;

  // Derive title from filename if no heading found
  const filename = path.basename(file, path.extname(file));
  const derivedTitle = title || filename
    .replace(/[-_]/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());

  const segments = docUrlPath.replace(/^\//, "").split("/");
  // segments[0] is "docs", so use segments[1] for category grouping
  const section = segments.length > 2
    ? toSectionTitle(segments[1])
    : segments.length > 1 && segments[1]
      ? toSectionTitle(segments[1])
      : "General";

  pages.push({ title: derivedTitle, url, description, section });
}

// Wrap a line to max 100 chars, continuing indented lines at the same indent level
function wrapLine(line, indentSpaces = 0) {
  if (line.length <= 100) return [line];
  const indent = " ".repeat(indentSpaces);
  const words = line.trimStart().split(" ");
  const lines = [];
  let current = indent;
  for (const word of words) {
    if (current.length + word.length + 1 > 100 && current.trim().length > 0) {
      lines.push(current.trimEnd());
      current = indent + "    " + word + " ";
    } else {
      current += word + " ";
    }
  }
  if (current.trim()) lines.push(current.trimEnd());
  return lines;
}

// ── Build llms.txt ────────────────────────────────────────────────────────────

const TARGET_CHARS = 120_000;

const sectionOrder = [];
const grouped = {};
for (const page of pages) {
  if (!grouped[page.section]) {
    sectionOrder.push(page.section);
    grouped[page.section] = [];
  }
  grouped[page.section].push(page);
}

// First pass: description as link label
const allLines = [`# ${resolvedName}`, ""];
if (siteDescription) allLines.push(`> ${siteDescription}`, "");
for (const section of sectionOrder) {
  allLines.push(`## ${section}`, "");
  for (const { title, url, description } of grouped[section]) {
    const descLine = description ? `    Description: ${description}` : null;
    allLines.push(...wrapLine(`- [${title}](${url})`, 0));
    if (descLine) allLines.push(...wrapLine(descLine, 4));
  }
  allLines.push("");
}
let output = allLines.join("\n");

// Second pass: fall back to title only
if (output.length > TARGET_CHARS) {
  const trimmedLines = [`# ${resolvedName}`, ""];
  if (siteDescription) trimmedLines.push(`> ${siteDescription}`, "");
  for (const section of sectionOrder) {
    trimmedLines.push(`## ${section}`, "");
    for (const { title, url, description } of grouped[section]) {
      trimmedLines.push(...wrapLine(`- [${title}](${url})`, 0));
      if (description) trimmedLines.push(...wrapLine(`    Description: ${description}`, 4));
    }
    trimmedLines.push("");
  }
  output = trimmedLines.join("\n");
}

// Third pass: drop pages proportionally per section
if (output.length > TARGET_CHARS) {
  const ratio = TARGET_CHARS / output.length;
  const finalLines = [`# ${resolvedName}`, ""];
  if (siteDescription) finalLines.push(`> ${siteDescription}`, "");
  for (const section of sectionOrder) {
    const sectionPages = grouped[section];
    const keep = Math.max(1, Math.floor(sectionPages.length * ratio));
    finalLines.push(`## ${section}`, "");
    for (const { title, url, description } of sectionPages.slice(0, keep)) {
      finalLines.push(...wrapLine(`- [${title}](${url})`, 0));
      if (description) finalLines.push(...wrapLine(`    Description: ${description}`, 4));
    }
    finalLines.push("");
  }
  output = finalLines.join("\n");
}

// Ensure output directory exists
const outDir = path.dirname(path.resolve(outputFile));
fs.mkdirSync(outDir, { recursive: true });

fs.writeFileSync(outputFile, output, "utf8");
console.log(`✓ Generated ${outputFile} with ${pages.length} pages across ${sectionOrder.length} sections (${output.length.toLocaleString()} chars)`);