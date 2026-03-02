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

const markdownDir  = path.resolve(positional[0] ?? ".");
const baseUrl      = flags["base-url"]    ?? "";
const outputFile   = flags["output"]      ?? "llms.txt";
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
    .replace(/&amp;/g, "&");

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

    if (clean.length > 0) description = clean.slice(0, 100);
  }

  return { title, description };
}

function fileToUrlPath(filePath, rootDir) {
  let rel = path.relative(rootDir, filePath).replace(/\\/g, "/");
  // Strip .md and .mdx extensions — afdocs will append .md itself
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
  const { title, description } = parseMarkdown(file, content);
  const urlPath = fileToUrlPath(file, markdownDir);
  const url = joinUrl(resolvedBaseUrl, urlPath);

  // Derive title from filename if no heading found
  const filename = path.basename(file, path.extname(file));
  const derivedTitle = title || filename
    .replace(/[-_]/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());

  const segments = urlPath.replace(/^\//, "").split("/");
  const section = segments.length > 1
    ? toSectionTitle(segments[0])
    : "General";

  pages.push({ title: derivedTitle, url, description, section });
}

// ── Build llms.txt ────────────────────────────────────────────────────────────

const TARGET_CHARS = 49_000;

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
if (siteDesc) allLines.push(`> ${siteDesc}`, "");
for (const section of sectionOrder) {
  allLines.push(`## ${section}`, "");
  for (const { title, url, description } of grouped[section]) {
    allLines.push(`- [${description || title}](${url})`);
  }
  allLines.push("");
}
let output = allLines.join("\n");

// Second pass: fall back to title only
if (output.length > TARGET_CHARS) {
  const trimmedLines = [`# ${resolvedName}`, ""];
  if (siteDesc) trimmedLines.push(`> ${siteDesc}`, "");
  for (const section of sectionOrder) {
    trimmedLines.push(`## ${section}`, "");
    for (const { title, url } of grouped[section]) {
      trimmedLines.push(`- [${title}](${url})`);
    }
    trimmedLines.push("");
  }
  output = trimmedLines.join("\n");
}

// Third pass: drop pages proportionally per section
if (output.length > TARGET_CHARS) {
  const ratio = TARGET_CHARS / output.length;
  const finalLines = [`# ${resolvedName}`, ""];
  if (siteDesc) finalLines.push(`> ${siteDesc}`, "");
  for (const section of sectionOrder) {
    const sectionPages = grouped[section];
    const keep = Math.max(1, Math.floor(sectionPages.length * ratio));
    finalLines.push(`## ${section}`, "");
    for (const { title, url } of sectionPages.slice(0, keep)) {
      finalLines.push(`- [${title}](${url})`);
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