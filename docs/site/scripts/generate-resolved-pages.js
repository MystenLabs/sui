/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * generate-resolved-pages.js
 *
 * Produces AI-readable versions of MDX documentation files by resolving every
 * <ImportContent> tag inline.  Output goes to site/.resolved/ mirroring the
 * content/ directory structure.  Only files that contain at least one
 * <ImportContent> tag are written — all other files are already self-contained.
 *
 * Snippet mode  – reads the raw .mdx snippet from content/snippets/ and inlines
 *                 it (with import lines stripped).
 * Code mode     – reads the source file from the monorepo, applies the same
 *                 extraction filters (tag, fun, struct, lines …) that the React
 *                 component uses at runtime, and wraps the result in a fenced
 *                 code block.
 *
 * Run:  node scripts/generate-resolved-pages.js
 */

const fs = require("fs");
const path = require("path");
const glob = require("glob");

// ── Paths ────────────────────────────────────────────────────────────────────
const SITE_ROOT = path.resolve(__dirname, "../");
const REPO_ROOT = path.resolve(SITE_ROOT, "../../");
const CONTENT_DIR = path.join(REPO_ROOT, "docs/content");
const SNIPPETS_DIR = path.join(CONTENT_DIR, "snippets");
const OUT_DIR = path.join(SITE_ROOT, ".resolved");

// ── Code extraction utilities (shared with the ImportContent component) ──────
const utils = require(path.join(
  SITE_ROOT,
  "src/shared/components/ImportContent/utils",
));

// ── Helpers ──────────────────────────────────────────────────────────────────
const readText = (p) =>
  fs.existsSync(p) ? fs.readFileSync(p, "utf8").replace(/\r\n?/g, "\n") : null;

function stripFencedCode(md) {
  return md.replace(/```[\s\S]*?```/g, "");
}

/** Parse JSX-style attributes from the inner portion of an <ImportContent …/> tag. */
function parseAttributes(attrStr) {
  const attrs = {};

  // key="value", key='value', key={value}
  const kvRe = /(\w+)\s*=\s*(?:"([^"]*)"|'([^']*)'|\{([^}]*)\})/g;
  let m;
  while ((m = kvRe.exec(attrStr))) {
    const val = m[2] ?? m[3] ?? m[4];
    attrs[m[1]] = val === "true" ? true : val === "false" ? false : val;
  }

  // Bare boolean props (e.g. noComments without =)
  const boolRe = /\b(noComments|noTests|noTitle|signatureOnly)\b(?!\s*=)/g;
  while ((m = boolRe.exec(attrStr))) {
    if (!(m[1] in attrs)) attrs[m[1]] = true;
  }
  return attrs;
}

function guessLanguage(filePath) {
  const ext = path.extname(filePath).slice(1);
  const map = {
    lock: "toml",
    sh: "shell",
    mdx: "markdown",
    tsx: "tsx",
    ts: "ts",
    rs: "rust",
    move: "move",
    prisma: "ts",
    json: "json",
    toml: "toml",
    yaml: "yaml",
    yml: "yaml",
  };
  return map[ext] || ext || "text";
}

const MAX_DEPTH = 5; // guard against circular snippet references

// ── Snippet resolution ───────────────────────────────────────────────────────

function resolveSnippet(source, depth) {
  const normalized = source.replace(/^\.\//, "");
  const candidates = [
    path.join(SNIPPETS_DIR, normalized),
    path.join(SNIPPETS_DIR, normalized + ".mdx"),
    path.join(SNIPPETS_DIR, normalized + ".md"),
  ];
  for (const p of candidates) {
    const text = readText(p);
    if (text != null) {
      let resolved = text
        .replace(/^\s*import\s+.*?from\s+['"].*?['"];?\s*$/gm, "")
        .replace(/^\s*\n/, "")
        .trim();
      // Recursively resolve nested <ImportContent> tags within snippets
      if (depth < MAX_DEPTH && /<ImportContent\b/i.test(stripFencedCode(resolved))) {
        resolved = resolveImportContent(resolved, depth + 1);
      }
      return resolved;
    }
  }
  return `<!-- [unresolved snippet: ${source}] -->`;
}

// ── Code resolution ──────────────────────────────────────────────────────────

function resolveCode(attrs) {
  const { source, org, repo } = attrs;

  // Skip GitHub-fetched content (requires network)
  if (org && repo) {
    return `<!-- [external GitHub content: ${org}/${repo}/${source} — fetch at runtime] -->`;
  }

  const cleaned = (source || "").replace(/^\/+/, "").replace(/^\.\//, "");
  const abs = path.join(REPO_ROOT, cleaned);
  let content = readText(abs);
  if (content == null) {
    return `<!-- [unresolved code: ${cleaned}] -->`;
  }

  // Strip license headers and local Sui dependency lines
  content = content
    .replace(
      /^\/\/\s*Copyright.*Mysten Labs.*\n\/\/\s*SPDX-License.*?\n?$/gim,
      "",
    )
    .replace(
      /\[dependencies\]\nsui\s?=\s?\{\s?local\s?=.*sui-framework.*\n/i,
      "[dependencies]",
    );

  let out = content;

  // ── Apply the same filter chain as the React component ──
  if (attrs.lines) {
    const parts = attrs.lines.split("-").map((n) => parseInt(n, 10));
    const start = parts[0];
    const end = parts[1] ?? parts[0];
    if (!isNaN(start) && !isNaN(end)) {
      out = out
        .split("\n")
        .slice(start - 1, end)
        .join("\n");
    }
  }

  const lang = attrs.language || guessLanguage(cleaned);

  if (attrs.tag) out = utils.returnTag(out, attrs.tag);
  if (attrs.module) out = utils.returnModules(out, attrs.module);
  if (attrs.fun)
    out = utils.returnFunctions(out, attrs.fun, lang, attrs.signatureOnly);
  if (attrs.variable) out = utils.returnVariables(out, attrs.variable, lang);
  if (attrs.struct) out = utils.returnStructs(out, attrs.struct, lang);
  if (attrs.type) out = utils.returnTypes(out, attrs.type);
  if (attrs.impl) out = utils.returnImplementations(out, attrs.impl);
  if (attrs.trait) out = utils.returnTraits(out, attrs.trait);
  if (attrs.enumeration) out = utils.returnEnums(out, attrs.enumeration);
  if (attrs.dep) out = utils.returnDeps(out, attrs.dep);
  if (attrs.test) out = utils.returnTests(out, attrs.test);

  // Strip docs:: marker comments
  out = out.replace(/^\s*\/\/\s*docs::\/?.*\r?$\n?/gm, "");

  if (attrs.noTests) out = utils.returnNotests(out);
  if (attrs.noComments) out = out.replace(/^ *\/\/.*\n/gm, "");

  // Remove leading blank line
  out = out.replace(/^\s*\n/, "");

  // If style is markdown, return raw (no code fence)
  if (/^m(?:d|arkdown)$/i.test(attrs.style || "")) {
    return out.trimEnd();
  }

  return "```" + lang + ' title="' + cleaned + '"\n' + out.trimEnd() + "\n```";
}

// ── Main resolver ────────────────────────────────────────────────────────────

/**
 * Split content into fenced-code and non-fenced regions, then resolve
 * <ImportContent> tags only in non-fenced regions.
 */
function resolveImportContent(mdxContent, depth = 0) {
  if (depth > MAX_DEPTH) return mdxContent;

  const regions = [];
  const fenceRe = /```[\s\S]*?```/g;
  let lastIndex = 0;
  let m;

  while ((m = fenceRe.exec(mdxContent))) {
    if (m.index > lastIndex) {
      regions.push({ text: mdxContent.slice(lastIndex, m.index), fenced: false });
    }
    regions.push({ text: m[0], fenced: true });
    lastIndex = m.index + m[0].length;
  }
  if (lastIndex < mdxContent.length) {
    regions.push({ text: mdxContent.slice(lastIndex), fenced: false });
  }

  const tagRe = /<ImportContent\b([^>]*?)\/?>(?!\s*<\/ImportContent>)/gi;

  return regions
    .map(({ text, fenced }) => {
      if (fenced) return text;
      return text.replace(tagRe, (_fullMatch, attrStr) => {
        const attrs = parseAttributes(attrStr);
        if (!attrs.source) return _fullMatch;

        // Infer mode when empty or missing: paths with file extensions → code,
        // otherwise → snippet
        let mode = attrs.mode;
        if (!mode) {
          mode = /\.\w+$/.test(attrs.source) ? "code" : "snippet";
        }

        if (mode === "snippet") {
          return resolveSnippet(attrs.source, depth);
        } else if (mode === "code") {
          return resolveCode(attrs);
        }
        return _fullMatch;
      });
    })
    .join("");
}

// ── Entry point ──────────────────────────────────────────────────────────────

const mdxFiles = glob
  .sync(path.join(CONTENT_DIR, "**/*.{md,mdx}"), { nodir: true })
  .filter((f) => !f.includes("/snippets/")); // Don't resolve snippet files

let resolved = 0;

for (const mdxPath of mdxFiles) {
  const text = readText(mdxPath);

  // Only process files that actually contain <ImportContent> outside fenced code
  const stripped = stripFencedCode(text);
  if (!/<ImportContent\b/i.test(stripped)) continue;

  const output = resolveImportContent(text);

  const relPath = path.relative(CONTENT_DIR, mdxPath);
  const outPath = path.join(OUT_DIR, relPath);

  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, output, "utf8");
  resolved++;
}

console.log(
  `Wrote ${resolved} resolved file(s) to ${path.relative(process.cwd(), OUT_DIR)}`,
);
