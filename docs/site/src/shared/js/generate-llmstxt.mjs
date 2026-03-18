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
const outputFile   = flags["output"]      ?? path.join(scriptDir, "../../static/llms.txt");
const siteDesc     = flags["description"] ?? "";
// --sitemap: local file path (recommended) or URL to sitemap.xml
// For Docusaurus: point at build/sitemap.xml after `npm run build`
const sitemapSource = flags["sitemap"]    ?? "";

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

/**
 * Parse YAML frontmatter from markdown content.
 * Returns an object with any key-value pairs found.
 */
function parseFrontmatter(content) {
  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!match) return {};
  const fm = {};
  for (const line of match[1].split("\n")) {
    const kv = line.match(/^(\w[\w-]*):\s*['"]?(.*?)['"]?\s*$/);
    if (kv) fm[kv[1]] = kv[2];
  }
  return fm;
}

function parseMarkdown(filePath, content) {
  let title = "";
  let description = "";

  // Check for metadata sidecar written by export script
  const metaPath = filePath.replace(/\.mdx?$/, ".meta.json");
  if (fs.existsSync(metaPath)) {
    try {
      const meta = JSON.parse(fs.readFileSync(metaPath, "utf8"));
      if (meta.title) title = meta.title;
      if (meta.description) description = meta.description;
    } catch {}
  }

  // Parse frontmatter for title/description/slug
  const fm = parseFrontmatter(content);
  if (!title && fm.title) title = fm.title;
  if (!description && fm.description) description = fm.description;

  // Strip unwanted HTML before any processing
  let body = content
    .replace(/^---[\s\S]*?---\n?/, "")           // strip frontmatter
    .replace(/<a\b[^>]*>[\s\S]*?<\/a>/gi, "")
    .replace(/<span\s+class="code-inline"[^>]*>[\s\S]*?<\/span>/gi, "")
    .replace(/&nbsp;●&nbsp;/g, "")
    .replace(/&nbsp;/g, " ")
    .replace(/&gt;/g, ">")
    .replace(/&lt;/g, "<")
    .replace(/&amp;/g, "&")
    .replace(/\[([^\]]*)\]\(https?:\/\/linear\.app\/[^)]*\)/gi, "$1")
    .replace(/https?:\/\/linear\.app\/\S+/gi, "")
    .replace(/\{[^}]*linear\.app[^}]*\}/gi, "")
    .replace(/\{\/\s*/g, "")
    .replace(/\s*\/\}/g, "");

  // Fallback: first H1
  if (!title) {
    const h1 = body.match(/^#\s+(.+)$/m);
    if (h1) title = h1[1].trim();
  }

  // Fallback description
  if (!description) {
    const clean = body
      .replace(/^#+\s+.+$/gm, "")
      .replace(/```[\s\S]*?```/g, "")
      .replace(/`[^`]+`/g, "")
      .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
      .replace(/[*_]/g, "")
      .replace(/<[^>]+>/g, "")
      .replace(/^\s*\d+\.\s+/gm, "")
      .replace(/^\s*[-*]\s+/gm, "")
      .replace(/\n+/g, " ")
      .replace(/\s+/g, " ")
      .trim();

    if (clean.length > 0) {
      const chunk = clean.slice(0, 300);
      const lastEnd = Math.max(chunk.lastIndexOf(". "), chunk.lastIndexOf("! "), chunk.lastIndexOf("? "));
      if (lastEnd > 0) {
        description = chunk.slice(0, lastEnd + 1).trim();
      } else if (clean.length <= 300) {
        description = clean.trim();
      } else {
        description = chunk.replace(/\s+\S*$/, "").trim();
      }
    }
  }

  if (/redirecting/i.test(description)) description = "";

  return { title, description, slug: fm.slug || "" };
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

// Normalise URL for dedup: strip trailing slashes
const norm = (u) => u.replace(/\/+$/, "");

// ── Sitemap loading ──────────────────────────────────────────────────────────

async function loadSitemapUrls(source) {
  if (!source) return [];

  let xml;
  if (source.startsWith("http://") || source.startsWith("https://")) {
    try {
      const resp = await fetch(source);
      if (!resp.ok) {
        console.error(`✗ ERROR: Could not fetch sitemap from ${source}: ${resp.status} ${resp.statusText}`);
        console.error(`  Hint: Use a local file path instead (e.g., --sitemap build/sitemap.xml)`);
        process.exit(1);
      }
      xml = await resp.text();
    } catch (err) {
      console.error(`✗ ERROR: Failed to fetch sitemap from ${source}: ${err.message}`);
      console.error(`  Hint: Node < 18 does not have global fetch(). Use a local file path instead.`);
      process.exit(1);
    }
  } else {
    const resolved = path.resolve(source);
    if (!fs.existsSync(resolved)) {
      console.error(`✗ ERROR: Sitemap file not found: ${resolved}`);
      process.exit(1);
    }
    xml = fs.readFileSync(resolved, "utf8");
    console.log(`  Loaded sitemap from ${resolved} (${xml.length.toLocaleString()} bytes)`);
  }

  // Handle sitemap index
  const sitemapRefs = [...xml.matchAll(/<sitemap>\s*<loc>\s*(.*?)\s*<\/loc>/gi)].map(m => m[1]);
  if (sitemapRefs.length > 0) {
    console.log(`  Sitemap index with ${sitemapRefs.length} child sitemaps`);
    const nested = [];
    for (const ref of sitemapRefs) {
      let childSource = ref;
      if (!source.startsWith("http") && !ref.startsWith("http")) {
        childSource = path.resolve(path.dirname(source), ref);
      }
      const urls = await loadSitemapUrls(childSource);
      nested.push(...urls);
    }
    return nested;
  }

  const urls = [...xml.matchAll(/<url>\s*<loc>\s*(.*?)\s*<\/loc>/gi)].map(m => m[1]);
  if (urls.length === 0) {
    // Fallback: try simpler regex in case of different formatting
    const simpleLocs = [...xml.matchAll(/<loc>\s*(.*?)\s*<\/loc>/gi)].map(m => m[1]);
    if (simpleLocs.length > 0) {
      console.log(`  Parsed ${simpleLocs.length} URLs from sitemap (simple loc extraction)`);
      return simpleLocs;
    }
    console.error(`✗ WARNING: Sitemap loaded but no <loc> URLs found. Check the file format.`);
  } else {
    console.log(`  Parsed ${urls.length} URLs from sitemap`);
  }
  return urls;
}

// ── Collect pages from markdown ──────────────────────────────────────────────

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
const seenUrls = new Set();

for (const file of files) {
  const content = fs.readFileSync(file, "utf8");
  if (!content.trim()) continue;
  const { title, description, slug } = parseMarkdown(file, content);
  const urlPath = fileToUrlPath(file, markdownDir);

  // Skip /design and /dev-guide sections
  if (/^\/?(design|dev-guide)(\/)/.test(urlPath) || urlPath === "/design" || urlPath === "/dev-guide") continue;

  // Build the canonical URL:
  // 1) Use frontmatter slug if present (handles Docusaurus slug overrides)
  // 2) Otherwise derive from filesystem path
  // No .md suffix — matches sitemap clean URLs
  let url;
  if (slug) {
    if (slug.startsWith("/")) {
      url = resolvedBaseUrl ? resolvedBaseUrl.replace(/\/$/, "") + slug : slug;
    } else {
      const parent = urlPath.replace(/\/[^/]*$/, "");
      url = joinUrl(resolvedBaseUrl, parent ? parent + "/" + slug : slug);
    }
  } else {
    url = joinUrl(resolvedBaseUrl, urlPath);
  }

  if (isLinearUrl(url)) continue;

  const filename = path.basename(file, path.extname(file));
  const derivedTitle = title || filename
    .replace(/[-_]/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());

  // Derive section from the resolved URL path
  const resolvedPath = slug?.startsWith("/") ? slug : "/" + urlPath;
  const segments = resolvedPath.replace(/^\//, "").split("/");
  const section = segments.length > 1
    ? toSectionTitle(segments[0])
    : "General";

  seenUrls.add(norm(url));
  // Append .md so LLMs fetch raw markdown instead of rendered HTML
  const displayUrl = url.endsWith("/") || url === resolvedBaseUrl ? url : url + ".md";
  pages.push({ title: derivedTitle, url: displayUrl, description, section, source: "markdown" });
}

console.log(`  Found ${pages.length} pages from markdown files`);

// ── Fill gaps from sitemap ───────────────────────────────────────────────────

const sitemapUrls = await loadSitemapUrls(sitemapSource);
let sitemapAdded = 0;
let sitemapSkippedSeen = 0;
let sitemapSkippedFilter = 0;

if (sitemapUrls.length > 0) {
  // Non-content pages to skip
  const skipPatterns = [/\/search$/, /\/sui-api-ref$/];
  const baseNorm = norm(resolvedBaseUrl || "");

  for (const rawUrl of sitemapUrls) {
    const url = norm(rawUrl);

    if (seenUrls.has(url)) {
      sitemapSkippedSeen++;
      continue;
    }

    if (skipPatterns.some((re) => re.test(url))) {
      sitemapSkippedFilter++;
      continue;
    }

    // Must be under our base URL
    if (baseNorm && !url.startsWith(baseNorm)) continue;

    const rel = url.replace(baseNorm, "").replace(/^\//, "");
    const segments = rel.split("/").filter(Boolean);
    const section = segments.length > 1
      ? toSectionTitle(segments[0])
      : "General";

    const lastSeg = segments[segments.length - 1] || "index";
    const derivedTitle = lastSeg
      .replace(/[-_]/g, " ")
      .replace(/\b\w/g, (c) => c.toUpperCase());

    seenUrls.add(url);
    const displayUrl = url.endsWith("/") ? url : url + ".md";
    pages.push({ title: derivedTitle, url: displayUrl, description: "", section, source: "sitemap" });
    sitemapAdded++;
  }

  console.log(`  Sitemap backfill: +${sitemapAdded} pages, ${sitemapSkippedSeen} already covered, ${sitemapSkippedFilter} filtered`);
} else if (sitemapSource) {
  console.error(`✗ WARNING: --sitemap was provided but yielded 0 URLs`);
}

// ── Build llms.txt ────────────────────────────────────────────────────────────

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

const TARGET_CHARS = 100_000;

const sectionOrder = [];
const grouped = {};
for (const page of pages) {
  if (!grouped[page.section]) {
    sectionOrder.push(page.section);
    grouped[page.section] = [];
  }
  grouped[page.section].push(page);
}

// First pass: with descriptions
const allLines = [`# ${resolvedName}`, ""];
if (siteDescription) allLines.push(`> ${siteDescription}`, "");
for (const section of sectionOrder) {
  allLines.push(`## ${section}`, "");
  for (const { title, url, description } of grouped[section]) {
    allLines.push(...wrapLine(`- [${title}](${url})`, 0));
    if (description) allLines.push(...wrapLine(`    Description: ${description}`, 4));
  }
  allLines.push("");
}
let output = allLines.join("\n");

// Second pass: drop descriptions if over limit
if (output.length > TARGET_CHARS) {
  const trimmedLines = [`# ${resolvedName}`, ""];
  if (siteDescription) trimmedLines.push(`> ${siteDescription}`, "");
  for (const section of sectionOrder) {
    trimmedLines.push(`## ${section}`, "");
    for (const { title, url } of grouped[section]) {
      trimmedLines.push(...wrapLine(`- [${title}](${url})`, 0));
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

// Hard cap
if (output.length > TARGET_CHARS) {
  const truncated = output.slice(0, TARGET_CHARS);
  const lastNewline = truncated.lastIndexOf("\n- ");
  if (lastNewline > 0) {
    const cutPoint = truncated.lastIndexOf("\n", lastNewline - 1);
    output = (cutPoint > 0 ? truncated.slice(0, cutPoint) : truncated.slice(0, lastNewline)) + "\n";
  } else {
    output = truncated + "\n";
  }
}

const outDir = path.dirname(path.resolve(outputFile));
fs.mkdirSync(outDir, { recursive: true });

fs.writeFileSync(outputFile, output, "utf8");

const mdCount = pages.filter(p => p.source === "markdown").length;
const smCount = pages.filter(p => p.source === "sitemap").length;
const parts = [`${pages.length} total pages (${mdCount} markdown + ${smCount} sitemap)`];
parts.push(`${sectionOrder.length} sections`);
parts.push(`${output.length.toLocaleString()} chars`);
console.log(`✓ Generated ${outputFile}: ${parts.join(", ")}`);