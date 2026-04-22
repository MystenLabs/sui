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
const outputFile = flags["output"] ?? path.join(scriptDir, "../../../static/llms.txt");
const baseUrl = flags["base-url"] ?? "https://docs.sui.io";

// ── Constants ────────────────────────────────────────────────────────────────
const TARGET_CHARS = 80_000;
const PINNED_SECTIONS = ["Move", "Top Level Navigation", "Sui Developer Skills"];

// ── Helpers ──────────────────────────────────────────────────────────────────

const IGNORE_DIRS = new Set([
  "snippets",
]);

const IGNORE_PATHS = new Set([
  "guides/developer/digital-assets",
  "guides/developer/wallets",
]);

const IGNORE_FILES = new Set([
  "guides/operator/observability.md",
  "references/ts-asset-tokenization.md",
  "guides/developer/getting-started/sui-wallets.md",
  "guides/developer/coin/stablecoins.md",
  "guides/developer/app-examples/recaptcha.md",
  "guides/developer/accessing-data/index.md",
  "references/framework/sui_bridge/message_types.md",
  "references/framework/sui_std/address.md",
  "references/framework/sui_std/bool.md",
  "references/framework/sui_sui/hex.md",
  "references/framework/sui_sui/prover.md",
  "references/framework/sui_sui_system/validator_wrapper.md",
  "references/sui-api/sui-graphql/beta/reference/types/enums/multisig-member-signature-scheme.md",
  "references/sui-api/sui-graphql/beta/reference/types/objects/multisig-member-signature.md",
  "references/sui-framework-reference.md",
  "/references/release-notes.md",
  "/references/awesome-sui.md",
]);

function walk(dir, results = []) {
  if (!fs.existsSync(dir)) return results;

  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    const rel = path.relative(markdownDir, full).replace(/\\/g, "/");

    if (entry.isDirectory()) {
      // Ignore by directory name
      if (IGNORE_DIRS.has(entry.name)) continue;

      // Ignore by full relative path (subtrees)
      if (IGNORE_PATHS.has(rel)) continue;

      walk(full, results);
    } else if (entry.name.endsWith(".md") || entry.name.endsWith(".mdx")) {
      // Ignore specific files
      if (IGNORE_FILES.has(rel)) continue;

      results.push(full);
    }
  }

  return results;
}

function isDraft(filePath) {
  const content = fs.readFileSync(filePath, "utf8");
  // Only check within the first 1KB (safely within frontmatter range)
  const head = content.slice(0, 1024);
  // Must start with frontmatter delimiter
  if (!head.startsWith("---")) return false;
  // Find the closing delimiter
  const end = head.indexOf("\n---", 3);
  if (end === -1) return false;
  const frontmatter = head.slice(0, end);
  return /draft:\s*true/i.test(frontmatter);
}

function joinUrl(base, p) {
  if (!base) return "/" + p.replace(/^\//, "");
  return base.replace(/\/$/, "") + "/" + p.replace(/^\//, "");
}

function formatTitle(str) {
  return str
    .replace(/[-_]/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function wrapLine(line, indent = 0) {
  if (line.length <= 100) return [line];
  const pad = " ".repeat(indent);
  const words = line.split(" ");
  const out = [];
  let cur = pad;

  for (const w of words) {
    if (cur.length + w.length + 1 > 100) {
      out.push(cur.trimEnd());
      cur = pad + "    " + w + " ";
    } else {
      cur += w + " ";
    }
  }
  if (cur.trim()) out.push(cur.trimEnd());
  return out;
}

// ── Hierarchy logic ──────────────────────────────────────────────────────────

function getHierarchy(relPath) {
  const parts = relPath.replace(/\.mdx?$/, "").split("/");

  // Detect index files before popping
  const isIndex = parts[parts.length - 1] === "index";
  if (isIndex) parts.pop();

  // Capitalize the section heading (e.g. "concepts" → "Concepts")
  const section = formatTitle(parts[0] || "General");

  // Use the full parent directory path as the subsection so each directory
  // gets its own group. For index files (already popped), parts IS the
  // directory path. For regular files, drop the filename to get the directory.
  const dirParts = isIndex ? parts : parts.slice(0, -1);
  const subsection = dirParts.length >= 2 ? dirParts.join("/") : null;

  return { section, subsection, isIndex, parts };
}

// ── Skills loader ────────────────────────────────────────────────────────────

function collectSkills() {
  const base = path.join(scriptDir, "../../static");
  const dirs = ["sui-move", "sui-frontend", "sui-app"];
  const out = [];

  for (const d of dirs) {
    const full = path.join(base, d);
    if (!fs.existsSync(full)) continue;

    const files = walk(full);

    for (const file of files) {
      const rel = path.relative(base, file).replace(/\\/g, "/");
      const title = formatTitle(path.basename(file, path.extname(file)));

      out.push({
        section: "Sui Developer Skills",
        subsection: d,
        title,
        url: joinUrl(baseUrl, rel.replace(/\.mdx?$/, "") + ".md")
      });
    }
  }

  return out;
}

// ── Collect pages ────────────────────────────────────────────────────────────

const files = walk(markdownDir);
const grouped = {};

// ── Move (pinned) ────────────────────────────────────────────────────────────
grouped["Move"] = [
  {
    title: "Move Language Reference",
    url: "https://move-book.com/llms.txt",
    description:
      "Complete reference for the Move programming language as used on Sui. " +
      "Covers syntax, types, functions, structs, abilities (copy, drop, store, key), " +
      "generics, ownership and the Sui object model, entry functions, public functions, " +
      "module structure, error handling, events, and testing with the Move test framework. " +
      "Includes best practices for safe and efficient contracts, object creation and transfer, " +
      "capability patterns, witness patterns, and hot potato patterns. " +
      "Essential reference for all Move smart contract development on Sui."
  }
];

// ── Skills (pinned) ──────────────────────────────────────────────────────────
const skills = collectSkills();
if (skills.length) grouped["Sui Developer Skills"] = skills;

// ── Markdown pages ───────────────────────────────────────────────────────────

for (const file of files) {
  if (isDraft(file)) continue;

  const rel = path.relative(markdownDir, file).replace(/\\/g, "/");

  const { section, subsection, isIndex, parts } = getHierarchy(rel);

  // Fix index titles: derive from the parent folder of the index file
  let title;
  if (isIndex) {
    // parts has already had "index" popped — last element is the containing folder
    // e.g. concepts/cryptography/index.md → parts = ["concepts","cryptography"] → "Cryptography Index"
    // e.g. concepts/index.md             → parts = ["concepts"]               → "Concepts Index"
    const parent = parts[parts.length - 1] || section;
    title = `${formatTitle(parent)} Index`;
  } else {
    title = formatTitle(path.basename(file, path.extname(file)));
  }

  // Index files: preserve /index in the URL so they resolve correctly
  // e.g. guides/developer/digital-assets/index.md → .../digital-assets/index.md
  // Non-index files: strip extension and re-add .md
  // e.g. guides/developer/coin/currency.md → .../coin/currency.md
  const cleanPath = rel.replace(/\.mdx?$/, "").replace(/\/index$/, "");
  const url = isIndex
    ? joinUrl(baseUrl, cleanPath + "/index") + ".md"
    : joinUrl(baseUrl, cleanPath) + ".md";

  if (!grouped[section]) grouped[section] = [];

  grouped[section].push({
    title,
    url,
    subsection
  });
}

// ── Merge single-entry sections into Top Level Navigation ────────────────────
// Also collapse single-entry subsections (lone ### headings) within each section

const topLevel = [];

for (const section of Object.keys(grouped)) {
  if (PINNED_SECTIONS.includes(section)) continue;

  const pages = grouped[section];

  // Entire section has only one page → hoist to Top Level Navigation
  if (pages.length === 1) {
    topLevel.push({
      ...pages[0],
      title: `${formatTitle(section)} — ${pages[0].title}`,
      subsection: null   // no subsection grouping in top-level
    });
    delete grouped[section];
    continue;
  }

  // Count pages per subsection; subsections with only one page get subsection cleared
  // so they render inline without a lone ### heading
  const subCounts = {};
  for (const page of pages) {
    if (page.subsection) {
      subCounts[page.subsection] = (subCounts[page.subsection] ?? 0) + 1;
    }
  }

  for (const page of pages) {
    if (page.subsection && subCounts[page.subsection] === 1) {
      page.subsection = null;
    }
  }
}

if (topLevel.length) {
  grouped["Top Level Navigation"] = topLevel;
}

// ── Sorting ──────────────────────────────────────────────────────────────────

function sortSections(sections) {
  return [
    ...PINNED_SECTIONS.filter((s) => sections.includes(s)),
    ...sections
      .filter((s) => !PINNED_SECTIONS.includes(s))
      .sort()
  ];
}

function sortPages(pages) {
  return pages.sort((a, b) => {
    if (a.subsection && b.subsection && a.subsection !== b.subsection) {
      return a.subsection.localeCompare(b.subsection);
    }
    return a.url.localeCompare(b.url);
  });
}

// ── Build output ─────────────────────────────────────────────────────────────

function build(ratio = 1) {
  const lines = [];

  // Static header (LLM optimized)
  lines.push("# Sui Documentation for LLMs", "");
  lines.push(
    "> Comprehensive reference for Sui blockchain development, including Move smart contract programming, " +
    "Sui framework concepts, frontend integration, and fullstack application architecture. " +
    "Designed for efficient retrieval and grounding by large language models.",
    ""
  );

  const sections = sortSections(Object.keys(grouped));

  for (const section of sections) {
    lines.push(`## ${section}`);

    const pages = sortPages(grouped[section]);

    const keep = PINNED_SECTIONS.includes(section)
      ? pages.length
      : Math.max(1, Math.floor(pages.length * ratio));

    let currentSub = null;
    let firstPage = true;

    for (const page of pages.slice(0, keep)) {
      if (page.subsection && page.subsection !== currentSub) {
        currentSub = page.subsection;
        // Blank line before ### to separate from preceding content,
        // but also serves as the blank line after ## on first entry
        lines.push("", `### ${formatTitle(currentSub)}`);
      } else if (firstPage) {
        // No subsection on first page — blank line after ## heading
        lines.push("");
      }
      firstPage = false;

      lines.push(...wrapLine(`- [${page.title}](${page.url})`, 0));
      if (page.description) {
        lines.push(...wrapLine(`  ${page.description}`, 2));
      }
    }

    lines.push("");
  }

  return lines.join("\n");
}

// ── Trim passes ──────────────────────────────────────────────────────────────

let output = build(1);

if (output.length > TARGET_CHARS) {
  const ratio = TARGET_CHARS / output.length;
  output = build(ratio);
}

if (output.length > TARGET_CHARS) {
  output = output.slice(0, TARGET_CHARS);
}

// ── Write file ───────────────────────────────────────────────────────────────

fs.mkdirSync(path.dirname(outputFile), { recursive: true });
fs.writeFileSync(outputFile, output, "utf8");

console.log(`✓ Generated ${outputFile} (${output.length.toLocaleString()} chars)`);
