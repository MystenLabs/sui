// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Plugin copies files from specified directories into the
// references/framework directory. Formats the nav listing
// and processes files so they still work in the crates/.../docs
// directory on GitHub. Source files are created via cargo docs.

import path from "path";
import fs from "fs";

const BRIDGE_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/bridge",
);
const FRAMEWORK_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/sui",
);
const STDLIB_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/std",
);
// const DEEPBOOK_PATH = path.join(
//   __dirname,
//   "../../../../../crates/sui-framework/docs/deepbook",
// );
const SUISYS_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/sui_system",
);
const DOCS_PATH = path.join(
  __dirname,
  "../../../../content/references/framework",
);

// prefix helper for the first path segment only
const prefixRootDir = (seg) => `sui_${seg}`;

// map of crate dir -> prefixed dir, used to rewrite hrefs in HTML
const CRATE_PREFIX_MAP = {
  bridge: "sui_bridge",
  sui: "sui_sui",
  std: "sui_std",
  sui_system: "sui_sui_system",
};

const CRATE_PACKAGES_PATH = {
  bridge: "sui/crates/sui-framework/packages/bridge",
  sui: "sui/crates/sui-framework/packages/sui",
  std: "sui/crates/sui-framework/packages/std",
  sui_system: "sui/crates/sui-framework/packages/sui_system",
};

const SKIP_INDEX_AT = new Set([DOCS_PATH]);

function hasPreexistingIndex(absDir) {
  try {
    return fs.readdirSync(absDir).some((name) =>
      /^(index|readme)\.(md|mdx)$/i.test(name)
    );
  } catch {
    return false;
  }
}

function shouldSkipIndex(absDir) {
  return SKIP_INDEX_AT.has(absDir) || hasPreexistingIndex(absDir);
}

const pjoin = path.posix.join;

const toLowerTitleText = (s) =>
  s.replace(/^sui_/, "").replace(/[-_]+/g, " ").toLowerCase();

/* ----------------- Validation helpers ---------------------- */

function validateSourcePath(srcPath, label) {
  if (!fs.existsSync(srcPath)) {
    console.warn(
      `[sui-framework-plugin] WARNING: Source path for "${label}" does not exist: ${srcPath}`,
    );
    return false;
  }
  try {
    const stat = fs.statSync(srcPath);
    if (!stat.isDirectory()) {
      console.warn(
        `[sui-framework-plugin] WARNING: Source path for "${label}" is not a directory: ${srcPath}`,
      );
      return false;
    }
  } catch (err) {
    console.warn(
      `[sui-framework-plugin] WARNING: Cannot stat source path for "${label}": ${err.message}`,
    );
    return false;
  }
  return true;
}

function safeWriteFile(filePath, content) {
  try {
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, content, "utf8");
  } catch (err) {
    console.error(
      `[sui-framework-plugin] ERROR: Failed to write ${filePath}: ${err.message}`,
    );
  }
}

function safeReadFile(filePath) {
  try {
    return fs.readFileSync(filePath, "utf8");
  } catch (err) {
    console.error(
      `[sui-framework-plugin] ERROR: Failed to read ${filePath}: ${err.message}`,
    );
    return null;
  }
}

/* ----------------- Content cleanup helpers ----------------- */

/**
 * Remove or replace "no description" placeholders that cargo doc generates
 * for undocumented items. These render as visible text on the page.
 */
function cleanNoDescriptionPlaceholders(md) {
  // Remove standalone "No description" lines (case-insensitive, with optional surrounding whitespace)
  md = md.replace(/^\s*[Nn]o\s+description\.?\s*$/gm, "");

  // Remove "No description" inside table cells: | No description |
  md = md.replace(/\|\s*[Nn]o\s+description\.?\s*\|/g, "| |");

  // Remove "No description" inside HTML table cells: <td>No description</td>
  md = md.replace(
    /<td>\s*[Nn]o\s+description\.?\s*<\/td>/g,
    "<td></td>",
  );

  // Remove "No description" that appears as the only paragraph content
  md = md.replace(
    /<p>\s*[Nn]o\s+description\.?\s*<\/p>/g,
    "",
  );

  return md;
}

/**
 * Remove empty <pre><code></code></pre> blocks that cargo doc generates
 * for items with no code content.
 */
function removeEmptyCodeBlocks(md) {
  md = md.replace(/<pre>\s*<code>\s*<\/code>\s*<\/pre>/g, "");
  return md;
}

/**
 * Clean up empty or visually broken sections that result from missing docs.
 * For example, a section header followed immediately by another header with
 * nothing in between, or sections that contain only whitespace.
 */
function cleanEmptySections(md) {
  // Remove consecutive blank lines (more than 2) to tighten layout
  md = md.replace(/\n{4,}/g, "\n\n\n");

  // Remove HTML headings that are immediately followed by another heading with no content between
  md = md.replace(
    /(<h([2-6])[^>]*>[^<]*<\/h\2>)\s*\n\s*(?=<h[2-6])/g,
    "$1\n\n",
  );

  // Remove empty <details> blocks: <details><summary>...</summary> with only
  // empty <dl></dl> or whitespace inside. These appear for structs/resources
  // that have no documented fields.
  md = md.replace(
    /<details>\s*\n\s*<summary>[^<]*<\/summary>\s*\n\s*(?:<dl>\s*<\/dl>\s*\n?\s*)?<\/details>/g,
    "",
  );

  // Also handle the multi-line spaced variant that cargo doc produces:
  //   <details>
  //   <summary>Fields</summary>
  //
  //
  //   <dl>
  //   </dl>
  //
  //
  //   </details>
  md = md.replace(
    /<details>\s*\n\s*<summary>[^<]*<\/summary>\s*\n[\s\n]*<dl>\s*\n?\s*<\/dl>[\s\n]*<\/details>/g,
    "",
  );

  // Remove <details> blocks where the <dl> contains only empty <dd> entries
  // (all fields undocumented — just type signatures with no descriptions)
  md = md.replace(
    /<details>\s*\n\s*<summary>[^<]*<\/summary>[\s\S]*?<\/details>/g,
    (match) => {
      // Check if all <dd> elements are empty (whitespace only)
      const ddContents = [...match.matchAll(/<dd>([\s\S]*?)<\/dd>/g)];
      if (ddContents.length === 0) return ""; // no <dd> at all → empty
      const allEmpty = ddContents.every(
        (m) => m[1].trim() === "",
      );
      // If all <dd> are empty, keep the block but simplify it
      // (we still want to show field names, just not empty descriptions)
      // So return as-is if there are <dt> entries with content
      const dtContents = [...match.matchAll(/<dt>([\s\S]*?)<\/dt>/g)];
      const hasMeaningfulDt = dtContents.some(
        (m) => m[1].trim() !== "",
      );
      if (!hasMeaningfulDt) return ""; // no meaningful content at all
      return match; // keep it, fields have type info even if no descriptions
    },
  );

  return md;
}

/**
 * Normalize inconsistent whitespace around HTML elements that
 * can cause Docusaurus/MDX to render content as plain text.
 */
function normalizeHtmlWhitespace(md) {
  // Ensure blank lines before and after block-level HTML elements
  // so MDX does not treat them as inline and render surrounding
  // Markdown as plain text.
  const blockTags = ["h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "table", "pre", "div", "hr"];
  for (const tag of blockTags) {
    // Opening tags: ensure blank line before if not at start of string
    md = md.replace(
      new RegExp(`([^\n])\n(<${tag}[\\s>])`, "g"),
      `$1\n\n$2`,
    );
    // Closing tags: ensure blank line after if not at end of string
    md = md.replace(
      new RegExp(`(<\\/${tag}>)\n([^\n])`, "g"),
      `$1\n\n$2`,
    );
  }

  return md;
}

/**
 * Fix common MDX rendering issues:
 * - Curly braces in code text that MDX interprets as JSX expressions
 * - Angle brackets in text that MDX interprets as components
 */
function fixMdxEscaping(md) {
  // Escape lone curly braces outside of code blocks and HTML tags
  // that MDX would try to interpret as JSX expressions.
  // Only target braces in running text, not inside <code>, <pre>, or backticks.
  // This is conservative: only escape { } that appear in plain paragraph text.
  md = md.replace(
    /^([^`<\n]*?)\{([^}]*)\}([^`]*?)$/gm,
    (match, before, inner, after) => {
      // Skip if this looks like an HTML attribute, a heading id, or a code-inline span
      if (/className=/.test(match)) return match;
      if (/class=/.test(match)) return match;
      if (/\{#/.test(match)) return match;
      if (/<code/.test(before) || /<\/code>/.test(after)) return match;
      if (/href=/.test(before)) return match;
      return `${before}\\{${inner}\\}${after}`;
    },
  );

  return md;
}

/**
 * Convert inline <code>...</code> tags to <span class="code-inline">
 * when they appear in paragraph/prose text (NOT inside <pre> blocks).
 *
 * Docusaurus/MDX renders <code> containing nested HTML (like <a> tags)
 * as block-level code elements, which breaks inline display. For example:
 *
 *   <code><a href="...">Permit</a></code>
 *
 * renders as a standalone code block instead of inline code. Converting
 * to <span> avoids this MDX behavior while preserving the visual style
 * (via the "code-inline" CSS class).
 *
 * We must NOT convert <code> inside <pre><code>...</code></pre> blocks,
 * as those are legitimate code blocks.
 */
function convertInlineCodeToSpan(md) {
  // Split content into regions: inside <pre>...</pre> vs. outside.
  // Only transform <code> tags outside of <pre> blocks.
  const parts = md.split(/(<pre[\s>][\s\S]*?<\/pre>)/g);

  for (let i = 0; i < parts.length; i++) {
    // Odd indices are <pre>...</pre> blocks — leave them alone
    if (i % 2 === 1) continue;

    // Even indices are non-pre content — convert inline <code> to <span>
    // Match <code>...</code> that may contain nested tags (like <a>)
    // but is on a single line (not a multi-line code block)
    parts[i] = parts[i].replace(
      /<code>((?:[^<]|<[^>]*>)*?)<\/code>/g,
      (match, inner) => {
        // Don't convert if inner content spans multiple lines (likely a code block)
        if (inner.includes("\n")) return match;
        // Don't convert empty <code></code>
        if (inner.trim() === "") return match;
        // Escape $ to prevent MDX from interpreting $name as a variable expression
        const safeInner = inner.replace(/\$/g, "&#36;");
        return `<span class="code-inline">${safeInner}</span>`;
      },
    );
  }

  return parts.join("");
}

/**
 * Ensure consecutive prose lines (non-blank, non-HTML-block, non-heading)
 * are separated by <br/> so Docusaurus doesn't merge them into one line.
 * This fixes cargo doc descriptions where separate sentences are on
 * separate lines but with no blank line between them.
 */
function insertLineBreaks(md) {
  const lines = md.split("\n");
  const result = [];

  // Matches lines that are block-level HTML or structural elements
  const isBlockLine = /^\s*(<\/?(h[1-6]|pre|code|ul|ol|li|table|tr|td|th|thead|tbody|dl|dt|dd|details|summary|div|hr|blockquote)\b|<!--|#{1,6}\s|---|\*\*\*|___|-\s+\[)/;

  // Matches a line whose content is solely an inline span/code element
  // (i.e. a line that rejoinOrphanedInlineElements will merge — don't <br/> before these)
  const isInlineOnlyLine =
    /^\s*(?:<span(?:\s+class="code-inline"|\s[^>]*)?>[^<\n]*<\/span>|<code>(?:[^<]|<[^>]*>)*?<\/code>|<a\s[^>]*>[^<\n]*<\/a>)\s*[.,;:!?)]*\s*$/;

  // Matches lines that are blank or only whitespace
  const isBlank = /^\s*$/;

  // Matches frontmatter delimiters
  const isFrontmatter = /^---\s*$/;

  // Track if we're inside frontmatter
  let inFrontmatter = false;
  let frontmatterCount = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const nextLine = lines[i + 1];

    // Track frontmatter boundaries
    if (isFrontmatter.test(line)) {
      frontmatterCount++;
      if (frontmatterCount === 1) inFrontmatter = true;
      if (frontmatterCount === 2) inFrontmatter = false;
      result.push(line);
      continue;
    }

    // Don't modify lines inside frontmatter
    if (inFrontmatter) {
      result.push(line);
      continue;
    }

    result.push(line);

    // If current line is non-blank prose and next line is also non-blank prose,
    // and the next line starts a new sentence (capital letter), append <br/>
    // BUT skip if the next line is an inline-only element (rejoin will handle it)
    if (
      nextLine !== undefined &&
      !isBlank.test(line) &&
      !isBlank.test(nextLine) &&
      !isBlockLine.test(line) &&
      !isBlockLine.test(nextLine) &&
      !isInlineOnlyLine.test(nextLine) &&
      /^\s*[A-Z]/.test(nextLine)
    ) {
      // Add period if the line doesn't already end with punctuation
      // (check the visible text, ignoring trailing HTML close tags)
      let lineText = result[result.length - 1].trimEnd();
      const visibleEnd = lineText.replace(/<\/[^>]+>\s*$/g, "").trimEnd();
      if (!/[.!?;:]$/.test(visibleEnd)) {
        lineText += ".";
      }
      result[result.length - 1] = lineText + "<br/>";
    }
  }

  return result.join("\n");
}

/**
 * Rejoin inline elements that cargo doc placed on their own line
 * back into the surrounding sentence.
 *
 * Cargo doc sometimes produces output like:
 *
 *   To write a function guarded by a
 *   <code><a href="...">Permit</a></code>
 *   , require it as an argument.
 *
 * or:
 *   Error from
 *   `from_bytes`
 *   when it is supplied too many or too few bytes.
 *
 * or (after backtick conversion):
 *   Loops applying
 *   <span class="code-inline">f</span>
 *   to each number from
 *   <span class="code-inline">$start</span>
 *   to
 *
 * This function detects lines that contain ONLY an inline element
 * (backticked code, <code>...</code>, <span class="code-inline">...</span>,
 * or nested combinations like <code><a>...</a></code>) with optional trailing
 * punctuation, and joins them to the surrounding prose lines.
 */
function rejoinOrphanedInlineElements(md) {
  // Split into lines and rejoin orphaned inline-only lines with
  // their surrounding prose. This avoids fragile multiline regexes.
  const lines = md.split("\n");
  const result = [];

  // Matches a line whose meaningful content is ONLY an inline element
  // with optional leading/trailing whitespace and punctuation.
  // Covers:
  //   - `backtick`
  //   - <code>..nested tags..</code>
  //   - <span class="code-inline">...</span>  ← produced by backtick conversion
  //   - <span ...>...</span> (other span variants)
  //   - <a ...>...</a>
  const inlineOnlyLine =
    /^\s*(?:`[^`]+`|<code>(?:[^<]|<[^>]*>)*?<\/code>|<span(?:\s+class="code-inline"|\s[^>]*)?>[^<\n]*<\/span>|<a\s[^>]*>[^<\n]*<\/a>)\s*[.,;:!?)]*\s*$/;

  // A line that looks like it ends mid-sentence (ends with a word char, comma,
  // or a closing inline tag — so the next orphaned span can attach)
  const endsMidSentence = /(?:[a-zA-Z,]|<\/span>|<\/code>|<\/a>|`)\s*$/;

  // A line that looks like a sentence continuation (starts with punctuation only,
  // not lowercase letters — to avoid merging separate sentences)
  const startsContinuation = /^\s*[,;:.!?)]/;

  // Lines that are part of structured HTML blocks where we should NOT rejoin
  const insideStructuredHtml = /^\s*<\/?(dl|dt|dd|details|summary|table|tr|td|th|thead|tbody|li|ul|ol)\b/;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    // Skip empty lines — just pass through
    if (trimmed === "") {
      result.push(line);
      continue;
    }

    // Check if this line is an orphaned inline element
    if (inlineOnlyLine.test(trimmed)) {
      // Don't rejoin if inside a structured HTML block
      if (insideStructuredHtml.test(trimmed)) {
        result.push(line);
        continue;
      }

      // Check if previous non-empty line ends mid-sentence
      const prevIdx = result.length - 1;
      if (prevIdx >= 0 && endsMidSentence.test(result[prevIdx]) && !insideStructuredHtml.test(result[prevIdx])) {
        // Join to previous line
        result[prevIdx] = result[prevIdx].trimEnd() + " " + trimmed;
        continue;
      }
    }

    // Check if this line is a continuation that should attach to a previous
    // line ending with an inline element
    if (
      startsContinuation.test(trimmed) &&
      result.length > 0 &&
      !insideStructuredHtml.test(trimmed)
    ) {
      const prev = result[result.length - 1];
      if (
        /(?:<\/code>|<\/span>|<\/a>|`)\s*[.,;:!?)]*\s*$/.test(prev) &&
        !insideStructuredHtml.test(prev)
      ) {
        result[result.length - 1] = prev.trimEnd() + " " + trimmed;
        continue;
      }
    }

    result.push(line);
  }

  let joined = result.join("\n");

  // Fix dangling punctuation: "Permit</code> , require" → "Permit</code>, require"
  joined = joined.replace(
    /(<\/code>|<\/span>|<\/a>|`)\s+([.,;:!?)])/g,
    "$1$2",
  );

  return joined;
}

// Module anchor from either HTML or Markdown module heading
function getModuleAnchor(md) {
  let m = md.match(/<h[1-6][^>]*>\s*Module\s*<code>([^<]+)<\/code>\s*<\/h[1-6]>/m);
  if (!m) m = md.match(/<h[1-6][^>]*>\s*Module\s*<span[^>]*>([^<]+)<\/span>\s*<\/h[1-6]>/m);
  if (!m) m = md.match(/^\s*#{1,6}\s*Module\s+`([^`]+)`/m);
  if (!m) return null;
  return m[1].replace(/::/g, "_");
}

// Add id="..." to an HTML heading if missing
function addIdToHtmlHeading(tagOpen, level, attrs, inner, id) {
  if (/\sid\s*=/.test(attrs)) return `<h${level}${attrs}>${inner}</h${level}>`;
  const space = attrs.trim().length ? " " : "";
  return `<h${level}${space}${attrs} id="${id}">${inner}</h${level}>`;
}

// Convert Markdown heading (## ...) to an HTML heading with id
function mdHeadingToHtml(hashes, innerHtml, id) {
  const level = hashes.length;
  return `<h${level} id="${id}">${innerHtml.trim()}</h${level}>`;
}

// Ensure Struct/Function/Constants headings have the exact IDs
function ensureHeadingIdsHtml(md) {
  const moduleAnchor = getModuleAnchor(md);

  // 1) Merge legacy anchors preceding headings → heading id
  // HTML Struct/Function
  md = md.replace(
    /<a name="([^"]+)"><\/a>\s*\n\s*<h([2-6])([^>]*)>\s*((?:Entry\s+Function|Public\s+Function|Function|Struct))\s*<code>([^<]+)<\/code>\s*<\/h\2>/g,
    (_m, name, lvl, attrs, kind, ident) =>
      addIdToHtmlHeading("h", lvl, attrs, `${kind} <code>${ident}</code>`, name),
  );
  // HTML Constants
  md = md.replace(
    /<a name="(@?Constants_\d+)"><\/a>\s*\n\s*<h([2-6])([^>]*)>\s*Constants\b[^<]*<\/h\2>/g,
    (_m, name, lvl, attrs) =>
      addIdToHtmlHeading("h", lvl, attrs, `Constants`, name),
  );
  // Markdown Struct/Function with legacy anchor
  md = md.replace(
    /<a name="([^"]+)"><\/a>\s*\n\s*(#{2,6})\s*((?:Entry\s+Function|Public\s+Function|Function|Struct))\s+`([^`]+)`/g,
    (_m, name, hashes, kind, ident) =>
      mdHeadingToHtml(hashes, `${kind} <code>${ident}</code>`, name),
  );
  // Markdown Constants with legacy anchor
  md = md.replace(
    /<a name="(@?Constants_\d+)"><\/a>\s*\n\s*(#{2,6})\s*Constants\b.*/g,
    (_m, name, hashes) => mdHeadingToHtml(hashes, `Constants`, name),
  );

  // 2) Promote Markdown Struct/Function/Constants to HTML headings w/ ids (avoids MDX plaintext)
  if (moduleAnchor) {
    md = md.replace(
      /^(\#{2,6})\s*Struct\s+`([^`]+)`(?![^\n]*\{#)/gm,
      (_m, hashes, ident) =>
        mdHeadingToHtml(hashes, `Struct <code>${ident}</code>`, `${moduleAnchor}_${ident}`),
    );
    md = md.replace(
      /^(\#{2,6})\s*(Entry\s+Function|Public\s+Function|Function)\s+`([^`]+)`(?![^\n]*\{#)/gm,
      (_m, hashes, kind, ident) =>
        mdHeadingToHtml(hashes, `${kind} <code>${ident}</code>`, `${moduleAnchor}_${ident}`),
    );
    md = md.replace(
      /^(\#{2,6})\s*Constants\b(?![^\n]*\{#)/gm,
      (_m, hashes) => mdHeadingToHtml(hashes, `Constants`, `@Constants_0`),
    );
  }

  // 3) Add IDs to HTML headings if still missing
  if (moduleAnchor) {
    // HTML Struct
    md = md.replace(
      /<h([2-6])([^>]*)>\s*Struct\s*<code>([^<]+)<\/code>\s*<\/h\1>/g,
      (_m, lvl, attrs, ident) =>
        addIdToHtmlHeading(
          "h",
          lvl,
          attrs,
          `Struct <code>${ident}</code>`,
          `${moduleAnchor}_${ident}`,
        ),
    );
    // HTML Functions
    md = md.replace(
      /<h([2-6])([^>]*)>\s*(Entry\s+Function|Public\s+Function|Function)\s*<code>([^<]+)<\/code>\s*<\/h\1>/g,
      (_m, lvl, attrs, kind, ident) =>
        addIdToHtmlHeading(
          "h",
          lvl,
          attrs,
          `${kind} <code>${ident}</code>`,
          `${moduleAnchor}_${ident}`,
        ),
    );
  }
  // HTML Constants (always same id)
  md = md.replace(
    /<h([2-6])([^>]*)>\s*Constants\b([^<]*)<\/h\1>/g,
    (_m, lvl, attrs, tail) =>
      addIdToHtmlHeading("h", lvl, attrs, `Constants${tail || ""}`, `@Constants_0`),
  );

  // 4) Remove stray empty anchors to avoid MDX HTML-mode swallowing content
  md = md.replace(/^\s*<a\b[^>]*><\/a>\s*$/gm, "");

  return md;
}

// Build HTML TOC list after Module heading (uses ensured ids)
function buildHtmlToc(md) {
  const items = [];
  // Structs
  md.replace(
    /<h[2-6][^>]*\sid="([^"]+)"[^>]*>\s*Struct\s*<code>([^<]+)<\/code>\s*<\/h[2-6]>/g,
    (_m, id, name) => {
      items.push(`<li><a href="#${id}">Struct <code>${name}</code></a></li>`);
      return _m;
    },
  );
  // Constants
  if (/<h[2-6][^>]*\sid="@Constants_0"[^>]*>/.test(md)) {
    items.push(`<li><a href="#@Constants_0">Constants</a></li>`);
  }
  // Functions
  md.replace(
    /<h[2-6][^>]*\sid="([^"]+)"[^>]*>\s*(?:Entry\s+Function|Public\s+Function|Function)\s*<code>([^<]+)<\/code>\s*<\/h[2-6]>/g,
    (_m, id, name) => {
      items.push(`<li><a href="#${id}">Function <code>${name}</code></a></li>`);
      return _m;
    },
  );

  if (!items.length) return null;

  return [
    `<!-- AUTOGENERATED: NAV-ANCHORS -->`,
    `<ul>`,
    items.join("\n"),
    `</ul>`,
    `<!-- /AUTOGENERATED: NAV-ANCHORS -->`,
  ].join("\n");
}

// Insert TOC after Module heading (HTML or Markdown)
function injectToc(md) {
  const toc = buildHtmlToc(md);
  if (!toc) return md;

  // Update if present
  if (
    /<!-- AUTOGENERATED: NAV-ANCHORS -->[\s\S]*?<!-- \/AUTOGENERATED: NAV-ANCHORS -->/.test(
      md,
    )
  ) {
    return md.replace(
      /<!-- AUTOGENERATED: NAV-ANCHORS -->[\s\S]*?<!-- \/AUTOGENERATED: NAV-ANCHORS -->/,
      toc,
    );
  }

  // Insert after HTML Module heading
  if (/<h[1-6][^>]*>\s*Module\s*<code>[^<]+<\/code>\s*<\/h[1-6]>/.test(md)) {
    return md.replace(
      /(<h[1-6][^>]*>\s*Module\s*<code>[^<]+<\/code>\s*<\/h[1-6]>)/,
      `$1\n\n${toc}\n\n`,
    );
  }

  // Also match Module heading with code-inline span (after backtick conversion)
  if (
    /<h[1-6][^>]*>\s*Module\s*<span[^>]*>[^<]+<\/span>\s*<\/h[1-6]>/.test(md)
  ) {
    return md.replace(
      /(<h[1-6][^>]*>\s*Module\s*<span[^>]*>[^<]+<\/span>\s*<\/h[1-6]>)/,
      `$1\n\n${toc}\n\n`,
    );
  }

  // Insert after Markdown Module heading (convert position only)
  if (/^\s*#{1,6}\s*Module\s+`[^`]+`.*$/m.test(md)) {
    return md.replace(
      /^(\s*#{1,6}\s*Module\s+`[^`]+`.*)$/m,
      (_m, line) => `${line}\n\n${toc}\n\n`,
    );
  }

  return md;
}

/* -------------------------------------------------------------------- */

const frameworkPlugin = (_context, _options) => {
  return {
    name: "sui-framework-plugin",

    async loadContent() {
      // Always start fresh — remove existing framework docs
      if (fs.existsSync(DOCS_PATH)) {
        try {
          fs.rmSync(DOCS_PATH, { recursive: true, force: true });
          console.log("[sui-framework-plugin] Removed existing framework docs directory.");
        } catch (err) {
          console.error(
            `[sui-framework-plugin] ERROR: Cannot remove existing directory ${DOCS_PATH}: ${err.message}`,
          );
          return;
        }
      }

      try {
        fs.mkdirSync(DOCS_PATH, { recursive: true });
      } catch (err) {
        console.error(
          `[sui-framework-plugin] ERROR: Cannot create output directory ${DOCS_PATH}: ${err.message}`,
        );
        return;
      }

      const recurseFiles = (dirPath, files = []) => {
        let entries;
        try {
          entries = fs.readdirSync(dirPath, { withFileTypes: true });
        } catch (err) {
          console.warn(
            `[sui-framework-plugin] WARNING: Cannot read directory ${dirPath}: ${err.message}`,
          );
          return files;
        }
        entries.forEach((file) => {
          const fp = path.join(dirPath, file.name);
          if (file.isDirectory()) {
            recurseFiles(fp, files);
          } else if (file.isFile() && path.extname(file.name) === ".md") {
            files.push(fp);
          }
        });
        return files;
      };

      // Validate source paths and collect files, skipping missing crates
      const sourceDirs = [
        { path: BRIDGE_PATH, label: "bridge" },
        { path: FRAMEWORK_PATH, label: "sui" },
        { path: STDLIB_PATH, label: "std" },
        // { path: DEEPBOOK_PATH, label: "deepbook" },
        { path: SUISYS_PATH, label: "sui_system" },
      ];

      const allFiles = [];
      let validSources = 0;
      for (const src of sourceDirs) {
        if (validateSourcePath(src.path, src.label)) {
          const files = recurseFiles(src.path);
          if (files.length > 0) {
            allFiles.push(files);
            validSources++;
          } else {
            console.warn(
              `[sui-framework-plugin] WARNING: No .md files found in "${src.label}" at ${src.path}`,
            );
          }
        }
      }

      if (validSources === 0) {
        console.error(
          "[sui-framework-plugin] ERROR: No valid source directories found. " +
            "Ensure cargo docs have been generated before building.",
        );
        return;
      }

      let processedCount = 0;
      let errorCount = 0;

      allFiles.forEach((theseFiles) => {
        theseFiles.forEach((absFile) => {
          let reMarkdown = safeReadFile(absFile);
          if (reMarkdown === null) {
            errorCount++;
            return; // skip this file
          }

          // Make hrefs work without ".md"
          reMarkdown = reMarkdown.replace(/<a\s+(.*?)\.md(.*?)>/g, `<a $1$2>`);

          // Legacy anchor + heading with backticked name (Markdown form) → HTML heading with id
          reMarkdown = reMarkdown.replace(
            /<a name="([^"]+)"><\/a>\s*\n\s*(#{1,6})\s*([A-Za-z ]+)\s+`([^`]+)`/g,
            (_m, id, hashes, kind, ident) =>
              mdHeadingToHtml(hashes, `${kind} <code>${ident}</code>`, id),
          );

          // Normalize cargo-doc front-matter: keep full title, but sidebar_label is just the last part.
          reMarkdown = reMarkdown.replace(
            /(title:\s*.*)Module\s+`([^`]+)`/g,
            (_m, titleLine, fullMod) => {
              const last = fullMod.split("::").pop();
              return `${titleLine}Module ${fullMod}\nsidebar_label: ${last}`;
            },
          );

          // crate-relative link rewriting
          reMarkdown = reMarkdown
            .replace(
              /href=(["'])(\.\.\/)(bridge|sui|std|sui_system)\/([^"']*)\1/g,
              (_m, q, up, seg, tail) =>
                `href=${q}${up}${CRATE_PREFIX_MAP[seg]}/${tail}${q}`,
            )
            // also handle single quotes just in case
            .replace(
              /href='(\.\.\/)(bridge|sui|std|sui_system)\//g,
              (m, up, seg) =>
                `href='${up}${CRATE_PREFIX_MAP[seg]}/"`.replace(/"$/, "'"),
            );

          // --- Content cleanup ---

          // Remove "no description" placeholders from cargo docs
          reMarkdown = cleanNoDescriptionPlaceholders(reMarkdown);

          // Clean up empty sections left behind after removing placeholders
          reMarkdown = cleanEmptySections(reMarkdown);

          // Remove empty <pre><code></code></pre> blocks
          reMarkdown = removeEmptyCodeBlocks(reMarkdown);

          // Ensure headings have ids (HTML-first), then inject HTML TOC
          reMarkdown = ensureHeadingIdsHtml(reMarkdown);
          reMarkdown = injectToc(reMarkdown);

          // Normalize whitespace around block-level HTML to prevent MDX
          // from rendering adjacent content as plain text
          reMarkdown = normalizeHtmlWhitespace(reMarkdown);

          // Fix MDX escaping issues (curly braces interpreted as JSX)
          reMarkdown = fixMdxEscaping(reMarkdown);

          // Rejoin inline elements (backticks, <code>, <a>, etc.) that cargo doc
          // placed on their own line back into the surrounding sentence.
          // Run BEFORE backtick conversion to catch `from_bytes` style orphans,
          // and also handles <code><a href="...">Permit</a></code> style orphans.
          reMarkdown = rejoinOrphanedInlineElements(reMarkdown);

          // Insert <br/> between consecutive prose lines so Docusaurus
          // doesn't merge separate sentences into one run-on line
          reMarkdown = insertLineBreaks(reMarkdown);

          // FINAL STEP: Convert backticks to inline code AFTER all other processing
          // This prevents <code><a href="...">text</a></code> which Docusaurus converts to blocks
          // Use a function replacer (not a string) so $ in the captured content is not
          // misinterpreted as a replacement pattern reference.
          reMarkdown = reMarkdown.replace(
            /`([^`\n]+)`/g,
            (_m, inner) => `<span class="code-inline">${inner.replace(/\$/g, "&#36;")}</span>`,
          );

          // Convert inline <code>...</code> to <span class="code-inline">
          // ONLY when they appear in paragraph text (not inside <pre> blocks).
          // Docusaurus/MDX renders <code> containing <a> tags as block-level
          // code elements, breaking inline display.
          reMarkdown = convertInlineCodeToSpan(reMarkdown);

          // Run rejoin again after span conversion to catch any new orphans
          // produced by the backtick → <span class="code-inline"> conversion.
          // This handles cases like:
          //   Loops applying `f` to each number from `$start` to `$stop` (inclusive)
          // where cargo doc placed each backtick-wrapped term on its own line.
          reMarkdown = rejoinOrphanedInlineElements(reMarkdown);

          // Write to prefixed path
          const filename = absFile.replace(/.*\/docs\/(.*)$/, `$1`);
          const parts = filename.split("/");
          const [root, ...rest] = parts;

          const targetRel = [prefixRootDir(root), ...rest].join("/");
          const fileWrite = path.join(DOCS_PATH, targetRel);

          // Create directories and category files
          let newDir = DOCS_PATH;
          parts.forEach((part, i) => {
            if (!part.match(/\.md$/)) {
              const onDiskPart = i === 0 ? prefixRootDir(part) : part;
              newDir = path.join(newDir, onDiskPart);

              if (!fs.existsSync(newDir)) {
                fs.mkdirSync(newDir, { recursive: true });

                const catfile = path.join(newDir, "_category_.json");
                const relParts = path
                  .relative(DOCS_PATH, newDir)
                  .split(path.sep)
                  .filter(Boolean);
                const slug = pjoin("/references/framework", ...relParts);
                const indexDocId = pjoin(
                  "references/framework",
                  ...relParts,
                  "index",
                );

                const top = relParts[0] || parts[0] || "";

                // Category label: lowercased dirname without sui_ prefix
                const unprefixed = part.replace(/^sui_/, "");
                const label = unprefixed.toLowerCase();

                const category = {
                  label,
                  link: {
                    type: "doc",
                    id: indexDocId,
                  },
                };

                try {
                  fs.writeFileSync(
                    catfile,
                    JSON.stringify(category, null, 2),
                    "utf8",
                  );
                } catch (err) {
                  console.error(
                    `[sui-framework-plugin] ERROR: Failed to create category file ${catfile}: ${err.message}`,
                  );
                }
              }
            }
          });

          safeWriteFile(fileWrite, reMarkdown);
          processedCount++;
        });
      });

      console.log(
        `[sui-framework-plugin] Processed ${processedCount} files` +
          (errorCount > 0 ? ` (${errorCount} errors)` : ""),
      );

      function buildIndexForDir(absDir) {
        const relParts = path
          .relative(DOCS_PATH, absDir)
          .split(path.sep)
          .filter(Boolean);
        const slug = pjoin("/references/framework", ...relParts);

        const dirName = relParts.length
          ? relParts[relParts.length - 1]
          : "framework";
        const titleText = `sui:${toLowerTitleText(dirName)}`;

        let entries;
        try {
          entries = fs.readdirSync(absDir, { withFileTypes: true });
        } catch (err) {
          console.error(
            `[sui-framework-plugin] ERROR: Cannot read directory for index: ${absDir}: ${err.message}`,
          );
          return;
        }

        const children = [];
        const topDir = relParts[0] || "";
        const frameworkName = topDir.replace(/^sui_/, "");
        const norm = (s) =>
          s
            .replace(/\.mdx?$/i, "")
            .toLowerCase()
            .replace(/-/g, "_");

        for (const ent of entries) {
          if (
            ent.isFile() &&
            /(?:\.mdx?)$/i.test(ent.name) &&
            !/^index\.mdx?$/i.test(ent.name)
          ) {
            const nameNoExt = ent.name.replace(/\.mdx?$/i, "");
            const childSlug = pjoin(
              "/references/framework",
              ...relParts,
              nameNoExt,
            );
            const linkText = `${frameworkName}::${norm(nameNoExt)}`;
            children.push({ href: childSlug, text: linkText });
          } else if (ent.isDirectory()) {
            const childSlug = pjoin(
              "/references/framework",
              ...relParts,
              ent.name,
            );
            const linkText = `${frameworkName}::${norm(ent.name)}`;
            children.push({ href: childSlug, text: linkText });
          }
        }

        children.sort((a, b) =>
          a.text.localeCompare(b.text, undefined, {
            sensitivity: "base",
            numeric: true,
          }),
        );

        const listMd = children.length
          ? children.map((c) => `- [${c.text}](${c.href})`).join("\n")
          : "";

        const topUnprefixed = topDir?.replace(/^sui_/, "") ?? "";
        const crateDescription =
          CRATE_PACKAGES_PATH[topUnprefixed]
            ? `Documentation for the modules in the ${CRATE_PACKAGES_PATH[topUnprefixed]} crate. Select a module from the list to see its details.`
            : `Documentation for the ${titleText} modules.`;

        const fm = [
          "---",
          `title: "${titleText.replace(/"/g, '\\"')}"`,
          `slug: ${slug}`,
          `description: "${crateDescription.replace(/"/g, '\\"')}"`,
          "---",
          "",
        ].join("\n");

        try {
          fs.writeFileSync(
            path.join(absDir, "index.md"),
            fm + listMd + "\n",
            "utf8",
          );
        } catch (err) {
          console.error(
            `[sui-framework-plugin] ERROR: Failed to create index.md in ${absDir}: ${err.message}`,
          );
        }
      }

      function buildAllIndexes() {
        const stack = [DOCS_PATH];
        while (stack.length) {
          const dir = stack.pop();
          let entries;
          try {
            entries = fs.readdirSync(dir, { withFileTypes: true });
          } catch (err) {
            console.error(
              `[sui-framework-plugin] ERROR: Cannot read directory ${dir}: ${err.message}`,
            );
            continue;
          }
          for (const ent of entries) {
            if (ent.isDirectory()) stack.push(path.join(dir, ent.name));
          }
          if (shouldSkipIndex(dir)) continue;
          buildIndexForDir(dir);
        }
      }

      buildAllIndexes();
      return;
    },
  };
};

module.exports = frameworkPlugin;