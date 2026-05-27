// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//
// Transforms fetched external documentation into Docusaurus-compatible MDX.
//
// Usage:
//   node scripts/transform-external-docs.js [config-name]
//
// Reads configuration from external-docs.json. Applies configured transforms:
//   - frontmatter: Extract title from # heading, generate description/keywords
//   - admonitions: Convert GFM > [!NOTE] and > **Info:** to :::note
//   - nav-header: Remove manual navigation header blocks
//   - toc: Remove manual ## Contents / ## Table of Contents sections
//   - links: Rewrite internal relative links to absolute Docusaurus paths
//
// Output is written to docs/content/{targetPath}/ as .mdx files with
// kebab-case filenames.

const fs = require("fs");
const path = require("path");

const SITE_ROOT = path.resolve(__dirname, "../");
const CONTENT_ROOT = path.resolve(SITE_ROOT, "../content");
const CONFIG_PATH = path.join(SITE_ROOT, "external-docs.json");
const CACHE_DIR = path.join(SITE_ROOT, ".cache-external-docs");

function loadConfig() {
  return JSON.parse(fs.readFileSync(CONFIG_PATH, "utf8"));
}

// --- Filename helpers ---

function toKebabCase(filename) {
  // APIRef -> api-ref, ArchiveRecovery -> archive-recovery, etc.
  return filename
    .replace(/\.mdx?$/, "")
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1-$2")
    .toLowerCase();
}

function buildFileMap(config, sourceDir) {
  // Build a map from source filenames to target slugs
  const map = {};

  if (config.fileMap) {
    for (const [src, target] of Object.entries(config.fileMap)) {
      const slug = target.replace(/\.mdx?$/, "");
      map[src] = slug;
    }
  }

  // Also discover any .md files in the source that aren't in the explicit map
  if (fs.existsSync(sourceDir)) {
    const files = fs.readdirSync(sourceDir).filter((f) => f.endsWith(".md"));
    for (const f of files) {
      if (!map[f]) {
        map[f] = toKebabCase(f);
      }
    }
  }

  return map;
}

// --- Transform functions ---

function extractTitle(content) {
  const match = content.match(/^#\s+(.+)$/m);
  return match ? match[1].trim() : null;
}

function extractDescription(content) {
  // Find the first real paragraph after the title (skip blank lines, headers, admonitions)
  const lines = content.split("\n");
  let pastTitle = false;
  const paraLines = [];

  for (const line of lines) {
    if (!pastTitle) {
      if (/^#\s+/.test(line)) {
        pastTitle = true;
      }
      continue;
    }

    const trimmed = line.trim();

    // Skip blank lines before first paragraph
    if (paraLines.length === 0 && trimmed === "") continue;

    // Skip admonitions, navigation headers, headers, horizontal rules, code fences
    if (/^>/.test(trimmed)) continue;
    if (/^\*\*Documentation:\*\*/.test(trimmed)) continue;
    if (/^#/.test(trimmed) && paraLines.length === 0) break;
    if (/^---$/.test(trimmed)) continue;
    if (/^```/.test(trimmed)) break;

    // Found a real paragraph line
    if (trimmed === "" && paraLines.length > 0) break;
    paraLines.push(trimmed);
  }

  let desc = paraLines.join(" ");
  // Strip markdown links but keep text
  desc = desc.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1");
  // Strip bold/italic
  desc = desc.replace(/\*\*([^*]+)\*\*/g, "$1");
  desc = desc.replace(/\*([^*]+)\*/g, "$1");
  // Truncate
  if (desc.length > 160) {
    desc = desc.substring(0, 157) + "...";
  }
  // Fallback: if no description found, try the first ## heading text
  if (!desc) {
    const h2Match = content.match(/^##\s+(.+)$/m);
    if (h2Match) {
      desc = h2Match[1].replace(/`([^`]+)`/g, "$1").trim();
    }
  }
  return desc || "";
}

function generateKeywords(slug, title) {
  const base = ["messaging", "sui-stack-messaging"];
  // Add slug-derived keywords
  const slugWords = slug
    .split("-")
    .filter((w) => w.length > 2 && !base.includes(w));
  return [...new Set([...base, ...slugWords])];
}

function addFrontmatter(content, slug) {
  // Don't add if already has frontmatter
  if (/^---\s*\n/.test(content)) return content;

  const title = extractTitle(content) || slug;
  const description = extractDescription(content);
  const keywords = generateKeywords(slug, title);

  const fm = [
    "---",
    `title: "${title.replace(/"/g, '\\"')}"`,
    `description: "${description.replace(/"/g, '\\"')}"`,
    `keywords: [${keywords.map((k) => `"${k}"`).join(", ")}]`,
    "---",
    "",
  ].join("\n");

  // Remove the # title line since frontmatter provides it
  let withoutTitle = content.replace(/^#\s+.+\n*/m, "");
  // Remove leading --- horizontal rules left after nav header removal
  withoutTitle = withoutTitle.replace(/^\s*---\s*\n+/, "");
  return fm + withoutTitle;
}

function convertAdmonitions(content) {
  const lines = content.split("\n");
  const result = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Pattern 1: GFM > [!NOTE], > [!WARNING], > [!TIP], > [!IMPORTANT], > [!CAUTION]
    const gfmMatch = line.match(/^>\s*\[!(NOTE|WARNING|TIP|IMPORTANT|CAUTION)\]\s*$/i);
    if (gfmMatch) {
      const typeMap = {
        NOTE: "note",
        WARNING: "warning",
        TIP: "tip",
        IMPORTANT: "info",
        CAUTION: "caution",
      };
      const type = typeMap[gfmMatch[1].toUpperCase()] || "note";
      result.push(`:::${type}`);
      i++;

      // Collect subsequent > lines
      while (i < lines.length && /^>\s?/.test(lines[i])) {
        result.push(lines[i].replace(/^>\s?/, ""));
        i++;
      }
      result.push(":::");
      continue;
    }

    // Pattern 2: > **Note:** text, > **Info:** text, > **Caveat:** text, etc.
    const boldMatch = line.match(
      /^>\s*\*\*(Note|Warning|Caveat|Info|Tip|Recommendation|Important|Terminology note):\*\*\s*(.*)/i,
    );
    if (boldMatch) {
      const typeMap = {
        note: "note",
        warning: "warning",
        caveat: "caution",
        info: "info",
        tip: "tip",
        recommendation: "tip",
        important: "info",
        "terminology note": "info",
      };
      const type = typeMap[boldMatch[1].toLowerCase()] || "note";
      const firstLine = boldMatch[2];
      result.push(`:::${type}`);
      if (firstLine) result.push(firstLine);
      i++;

      // Collect continuation lines
      while (i < lines.length && /^>\s?/.test(lines[i])) {
        result.push(lines[i].replace(/^>\s?/, ""));
        i++;
      }
      result.push(":::");
      continue;
    }

    result.push(line);
    i++;
  }

  return result.join("\n");
}

function removeNavHeader(content) {
  // Remove the documentation navigation header that appears at the top of each file.
  // Pattern: **Documentation:** [Home](...) | [Installation](...) | ...
  // May span multiple lines if wrapped. Also remove the --- separator that follows.
  let result = content.replace(
    /\*\*Documentation:\*\*\s*(\[.*?\]\(.*?\)\s*\|\s*)*\[.*?\]\(.*?\)\s*\n*/g,
    "",
  );
  // Remove leading --- horizontal rules (often left after removing nav header)
  result = result.replace(/^(#[^\n]+\n)\s*---\s*\n/m, "$1\n");
  // Also remove standalone --- at the start of content (after title)
  result = result.replace(/\n---\s*\n(\n)?/g, (match, trailing) => {
    return trailing ? "\n\n" : "\n";
  });
  return result;
}

function removeToc(content) {
  // Remove ## Contents or ## Table of Contents sections (manual TOC with anchor links)
  // These sections consist of a heading followed by lines starting with - [ or * [
  const lines = content.split("\n");
  const result = [];
  let inToc = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (/^##\s+(Contents|Table of Contents)\s*$/i.test(line)) {
      inToc = true;
      continue;
    }

    if (inToc) {
      // TOC lines are typically: - [Link text](#anchor) or  - [Sub link](#anchor)
      if (/^\s*[-*]\s*\[/.test(line) || line.trim() === "") {
        continue;
      }
      // Hit a non-TOC line — we're past the TOC
      inToc = false;
    }

    result.push(line);
  }

  // Also remove "Back to table of contents" links
  return result
    .join("\n")
    .replace(/\[Back to table of contents\]\(#table-of-contents\)\s*/gi, "");
}

function rewriteLinks(content, fileMap, linkPrefix) {
  // Build a lookup from source filenames to target URL paths
  const linkLookup = {};
  for (const [srcFile, slug] of Object.entries(fileMap)) {
    // Handle both ./File.md and File.md references
    linkLookup[`./${srcFile}`] = `${linkPrefix}/${slug}`;
    linkLookup[srcFile] = `${linkPrefix}/${slug}`;
  }

  // Rewrite markdown links: [text](./File.md#anchor) -> [text](/prefix/slug#anchor)
  return content.replace(
    /\[([^\]]*)\]\(([^)]+)\)/g,
    (match, text, href) => {
      // Don't touch external URLs
      if (/^https?:\/\//.test(href)) return match;
      // Don't touch anchor-only links
      if (href.startsWith("#")) return match;

      // Split href into path and fragment
      const [filePath, fragment] = href.split("#");
      const anchor = fragment ? `#${fragment}` : "";

      // Handle ../../README.md -> GitHub repo link
      if (filePath.includes("README.md") && filePath.startsWith("..")) {
        // Extract repo from config (we'll handle this via the repo URL)
        return match;
      }

      // Try to resolve the file path
      const normalized = filePath.replace(/^\.\//, "");
      const lookup = linkLookup[normalized] || linkLookup[`./${normalized}`];

      if (lookup) {
        return `[${text}](${lookup}${anchor})`;
      }

      // Handle technical_design_docs/ references
      if (normalized.startsWith("technical_design_docs/")) {
        const tdSlug = normalized
          .replace("technical_design_docs/", "")
          .replace(/^\d+_/, "")
          .replace(/\.md$/, "")
          .replace(/_/g, "-");
        // Link to GitHub since we're not including technical design docs
        return match;
      }

      // Unresolved — leave as-is and let the build checker catch it
      return match;
    },
  );
}

function rewriteRepoRelativeLinks(content, repo, branch) {
  // Rewrite ../../README.md links to the GitHub repo
  let result = content.replace(
    /\[([^\]]*)\]\([^)]*README\.md[^)]*\)/g,
    `[$1](https://github.com/${repo})`,
  );

  // Rewrite ../../<path> links (pointing to source code in the repo) to GitHub URLs
  // These are relative links from docs/sui-stack-messaging/ going up to the repo root
  result = result.replace(
    /\[([^\]]*)\]\((?:\.\.\/)+([^)]+)\)/g,
    (match, text, relPath) => {
      // Skip if already rewritten or if it's a sibling doc link
      if (relPath.startsWith("http")) return match;
      // Clean up the path (remove leading ../)
      const cleanPath = relPath.replace(/^(\.\.\/)+/, "");
      // Skip if this looks like a doc reference (already handled by rewriteLinks)
      if (cleanPath.endsWith(".md") && !cleanPath.includes("/")) return match;
      return `[${text}](https://github.com/${repo}/tree/${branch}/${cleanPath})`;
    },
  );

  return result;
}

// --- Main pipeline ---

function transformFile(content, slug, config, fileMap) {
  const transforms = config.transforms || [];
  let result = content;

  if (transforms.includes("nav-header")) {
    result = removeNavHeader(result);
  }

  if (transforms.includes("toc")) {
    result = removeToc(result);
  }

  if (transforms.includes("admonitions")) {
    result = convertAdmonitions(result);
  }

  if (transforms.includes("links")) {
    result = rewriteLinks(result, fileMap, config.linkPrefix);
    result = rewriteRepoRelativeLinks(result, config.repo, config.branch || "main");
  }

  if (transforms.includes("frontmatter")) {
    result = addFrontmatter(result, slug);
  }

  return result;
}

function processSource(name, config) {
  const sourceDir = path.join(CACHE_DIR, name, config.sourcePath);
  const targetDir = path.join(CONTENT_ROOT, config.targetPath);

  if (!fs.existsSync(sourceDir)) {
    console.warn(
      `⚠️  ${name}: source directory not found at ${sourceDir}. Was fetch-external-docs.js run first?`,
    );
    return;
  }

  // Build the file map
  const fileMap = buildFileMap(config, sourceDir);

  // Ensure target directory exists
  // Clean previous output first (except index.mdx which is manually maintained)
  if (fs.existsSync(targetDir)) {
    const existing = fs.readdirSync(targetDir);
    for (const f of existing) {
      if (f === "index.mdx" || f === "chat-app.mdx") continue;
      const fullPath = path.join(targetDir, f);
      if (fs.statSync(fullPath).isFile()) {
        fs.unlinkSync(fullPath);
      }
    }
  } else {
    fs.mkdirSync(targetDir, { recursive: true });
  }

  const exclude = new Set(config.exclude || []);
  let count = 0;

  for (const [srcFile, slug] of Object.entries(fileMap)) {
    if (exclude.has(srcFile)) continue;

    const srcPath = path.join(sourceDir, srcFile);
    if (!fs.existsSync(srcPath)) {
      console.warn(`⚠️  ${name}: source file not found: ${srcFile}`);
      continue;
    }

    const content = fs.readFileSync(srcPath, "utf8");
    const transformed = transformFile(content, slug, config, fileMap);
    const targetFile = slug.endsWith(".mdx") ? slug : `${slug}.mdx`;
    const targetPath = path.join(targetDir, targetFile);

    fs.writeFileSync(targetPath, transformed);
    count++;
  }

  console.log(`✅ ${name}: transformed ${count} files → ${config.targetPath}/`);

  // Validate: fail if the source repo contains .md files not in fileMap or exclude
  const allSourceFiles = fs.readdirSync(sourceDir).filter((f) => f.endsWith(".md"));
  const mappedFiles = new Set(Object.keys(config.fileMap || {}));
  const excludedFiles = new Set(config.exclude || []);
  const unmapped = allSourceFiles.filter(
    (f) => !mappedFiles.has(f) && !excludedFiles.has(f),
  );

  if (unmapped.length > 0) {
    console.error(
      `\n❌ ${name}: found ${unmapped.length} unmapped .md file(s) in ${config.sourcePath}/:\n` +
        unmapped.map((f) => `   - ${f}`).join("\n") +
        `\n\nEvery .md file in the source must have an entry in external-docs.json ` +
        `fileMap (to include it) or exclude (to skip it), and a corresponding ` +
        `sidebar entry in sidebars.js.\n`,
    );
    process.exit(1);
  }
}

function main() {
  const allConfig = loadConfig();
  const requestedName = process.argv[2];
  const names = requestedName ? [requestedName] : Object.keys(allConfig);

  for (const name of names) {
    if (!allConfig[name]) {
      console.error(`❌ Unknown config: ${name}`);
      process.exit(1);
    }
    processSource(name, allConfig[name]);
  }
}

main();
