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
  md = md.replace(/^\s*[Nn]o\s+description\.?\s*$/gm, "");
  md = md.replace(/\|\s*[Nn]o\s+description\.?\s*\|/g, "| |");
  md = md.replace(
    /<td>\s*[Nn]o\s+description\.?\s*<\/td>/g,
    "<td></td>",
  );
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
 */
function cleanEmptySections(md) {
  md = md.replace(/\n{4,}/g, "\n\n\n");
  md = md.replace(
    /(<h([2-6])[^>]*>[^<]*<\/h\2>)\s*\n\s*(?=<h[2-6])/g,
    "$1\n\n",
  );
  md = md.replace(
    /<details>\s*\n\s*<summary>[^<]*<\/summary>\s*\n\s*(?:<dl>\s*<\/dl>\s*\n?\s*)?<\/details>/g,
    "",
  );
  md = md.replace(
    /<details>\s*\n\s*<summary>[^<]*<\/summary>\s*\n[\s\n]*<dl>\s*\n?\s*<\/dl>[\s\n]*<\/details>/g,
    "",
  );
  md = md.replace(
    /<details>\s*\n\s*<summary>[^<]*<\/summary>[\s\S]*?<\/details>/g,
    (match) => {
      const ddContents = [...match.matchAll(/<dd>([\s\S]*?)<\/dd>/g)];
      if (ddContents.length === 0) return "";
      const allEmpty = ddContents.every(
        (m) => m[1].trim() === "",
      );
      const dtContents = [...match.matchAll(/<dt>([\s\S]*?)<\/dt>/g)];
      const hasMeaningfulDt = dtContents.some(
        (m) => m[1].trim() !== "",
      );
      if (!hasMeaningfulDt) return "";
      return match;
    },
  );
  return md;
}

/**
 * Normalize inconsistent whitespace around HTML elements.
 */
function normalizeHtmlWhitespace(md) {
  const blockTags = ["h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "table", "pre", "div", "hr"];
  for (const tag of blockTags) {
    md = md.replace(
      new RegExp(`([^\n])\n(<${tag}[\\s>])`, "g"),
      `$1\n\n$2`,
    );
    md = md.replace(
      new RegExp(`(<\\/${tag}>)\n([^\n])`, "g"),
      `$1\n\n$2`,
    );
  }
  return md;
}

/**
 * Fix common MDX rendering issues.
 */
function fixMdxEscaping(md) {
  md = md.replace(
    /^([^`<\n]*?)\{([^}]*)\}([^`]*?)$/gm,
    (match, before, inner, after) => {
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
 */
function convertInlineCodeToSpan(md) {
  const parts = md.split(/(<pre[\s>][\s\S]*?<\/pre>)/g);
  for (let i = 0; i < parts.length; i++) {
    if (i % 2 === 1) continue;
    parts[i] = parts[i].replace(
      /<code>((?:[^<]|<[^>]*>)*?)<\/code>/g,
      (match, inner) => {
        if (inner.includes("\n")) return match;
        if (inner.trim() === "") return match;
        const safeInner = inner.replace(/\$/g, "&#36;");
        return `<span class="code-inline">${safeInner}</span>`;
      },
    );
  }
  return parts.join("");
}

/**
 * Ensure consecutive prose lines are separated by <br/>.
 */
function insertLineBreaks(md) {
  const lines = md.split("\n");
  const result = [];
  const isBlockLine = /^\s*(<\/?(h[1-6]|pre|code|ul|ol|li|table|tr|td|th|thead|tbody|dl|dt|dd|details|summary|div|hr|blockquote)\b|<!--|#{1,6}\s|---|\*\*\*|___|-\s+\[)/;
  const isInlineOnlyLine =
    /^\s*(?:<span(?:\s+class="code-inline"|\s[^>]*)?>[^<\n]*<\/span>|<code>(?:[^<]|<[^>]*>)*?<\/code>|<a\s[^>]*>[^<\n]*<\/a>)\s*[.,;:!?)]*\s*$/;
  const isBlank = /^\s*$/;
  const isFrontmatter = /^---\s*$/;
  let inFrontmatter = false;
  let frontmatterCount = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const nextLine = lines[i + 1];
    if (isFrontmatter.test(line)) {
      frontmatterCount++;
      if (frontmatterCount === 1) inFrontmatter = true;
      if (frontmatterCount === 2) inFrontmatter = false;
      result.push(line);
      continue;
    }
    if (inFrontmatter) {
      result.push(line);
      continue;
    }
    result.push(line);
    if (
      nextLine !== undefined &&
      !isBlank.test(line) &&
      !isBlank.test(nextLine) &&
      !isBlockLine.test(line) &&
      !isBlockLine.test(nextLine) &&
      !isInlineOnlyLine.test(nextLine) &&
      /^\s*[A-Z]/.test(nextLine)
    ) {
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
 * Rejoin inline elements that cargo doc placed on their own line.
 */
function rejoinOrphanedInlineElements(md) {
  const lines = md.split("\n");
  const result = [];
  const inlineOnlyLine =
    /^\s*(?:`[^`]+`|<code>(?:[^<]|<[^>]*>)*?<\/code>|<span(?:\s+class="code-inline"|\s[^>]*)?>[^<\n]*<\/span>|<a\s[^>]*>[^<\n]*<\/a>)\s*[.,;:!?)]*\s*$/;
  const endsMidSentence = /(?:[a-zA-Z,]|<\/span>|<\/code>|<\/a>|`)\s*$/;
  const startsContinuation = /^\s*[,;:.!?)]/;
  const insideStructuredHtml = /^\s*<\/?(dl|dt|dd|details|summary|table|tr|td|th|thead|tbody|li|ul|ol)\b/;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();
    if (trimmed === "") {
      result.push(line);
      continue;
    }
    if (inlineOnlyLine.test(trimmed)) {
      if (insideStructuredHtml.test(trimmed)) {
        result.push(line);
        continue;
      }
      const prevIdx = result.length - 1;
      if (prevIdx >= 0 && endsMidSentence.test(result[prevIdx]) && !insideStructuredHtml.test(result[prevIdx])) {
        result[prevIdx] = result[prevIdx].trimEnd() + " " + trimmed;
        continue;
      }
    }
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

  md = md.replace(
    /<a name="([^"]+)"><\/a>\s*\n\s*<h([2-6])([^>]*)>\s*((?:Entry\s+Function|Public\s+Function|Function|Struct))\s*<code>([^<]+)<\/code>\s*<\/h\2>/g,
    (_m, name, lvl, attrs, kind, ident) =>
      addIdToHtmlHeading("h", lvl, attrs, `${kind} <code>${ident}</code>`, name),
  );
  md = md.replace(
    /<a name="(@?Constants_\d+)"><\/a>\s*\n\s*<h([2-6])([^>]*)>\s*Constants\b[^<]*<\/h\2>/g,
    (_m, name, lvl, attrs) =>
      addIdToHtmlHeading("h", lvl, attrs, `Constants`, name),
  );
  md = md.replace(
    /<a name="([^"]+)"><\/a>\s*\n\s*(#{2,6})\s*((?:Entry\s+Function|Public\s+Function|Function|Struct))\s+`([^`]+)`/g,
    (_m, name, hashes, kind, ident) =>
      mdHeadingToHtml(hashes, `${kind} <code>${ident}</code>`, name),
  );
  md = md.replace(
    /<a name="(@?Constants_\d+)"><\/a>\s*\n\s*(#{2,6})\s*Constants\b.*/g,
    (_m, name, hashes) => mdHeadingToHtml(hashes, `Constants`, name),
  );

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

  if (moduleAnchor) {
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
  md = md.replace(
    /<h([2-6])([^>]*)>\s*Constants\b([^<]*)<\/h\1>/g,
    (_m, lvl, attrs, tail) =>
      addIdToHtmlHeading("h", lvl, attrs, `Constants${tail || ""}`, `@Constants_0`),
  );

  md = md.replace(/^\s*<a\b[^>]*><\/a>\s*$/gm, "");

  return md;
}

// Build HTML TOC list after Module heading (uses ensured ids)
function buildHtmlToc(md) {
  const items = [];
  md.replace(
    /<h[2-6][^>]*\sid="([^"]+)"[^>]*>\s*Struct\s*<code>([^<]+)<\/code>\s*<\/h[2-6]>/g,
    (_m, id, name) => {
      items.push(`<li><a href="#${id}">Struct <code>${name}</code></a></li>`);
      return _m;
    },
  );
  if (/<h[2-6][^>]*\sid="@Constants_0"[^>]*>/.test(md)) {
    items.push(`<li><a href="#@Constants_0">Constants</a></li>`);
  }
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

// ── CHANGED: Insert TOC AFTER the first description paragraph, not before it ──
// This ensures real content (the module description) appears first in the page,
// fixing content-start-position issues where the TOC pushed content past 15%.
function injectToc(md) {
  const toc = buildHtmlToc(md);
  if (!toc) return md;

  // Update existing TOC if present
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

  // Find the Module heading, then look for the first blank line AFTER it
  // (which marks the end of the description paragraph). Insert TOC there.
  // This way: Heading → Description → TOC → Struct/Function sections

  // HTML Module heading
  const htmlModuleRe = /(<h[1-6][^>]*>\s*Module\s*(?:<code>[^<]+<\/code>|<span[^>]*>[^<]+<\/span>)\s*<\/h[1-6]>)/;
  // Markdown Module heading
  const mdModuleRe = /^(\s*#{1,6}\s*Module\s+`[^`]+`.*)$/m;

  const moduleMatch = md.match(htmlModuleRe) || md.match(mdModuleRe);
  if (!moduleMatch) return md;

  const headingEnd = md.indexOf(moduleMatch[0]) + moduleMatch[0].length;
  const afterHeading = md.slice(headingEnd);

  // Find the first "content block" after the heading — this is the description.
  // A content block ends at the next blank line, next heading, or next HTML block element.
  // We want to insert the TOC AFTER this first block.
  const firstBreak = afterHeading.search(/\n\s*\n\s*(?=<h[2-6]|#{2,6}\s|\n)/);

  if (firstBreak > 0) {
    // There IS content between the heading and the next section — insert TOC after it
    const insertPos = headingEnd + firstBreak;
    return md.slice(0, insertPos) + "\n\n" + toc + "\n" + md.slice(insertPos);
  }

  // Fallback: no description found, insert right after heading (original behavior)
  return md.slice(0, headingEnd) + "\n\n" + toc + "\n\n" + md.slice(headingEnd);
}

/* -------------------------------------------------------------------- */

// ── CHANGED: Extract plain-text summary from module content ──────────────
// Used to build a description for the frontmatter of generated pages.
// Extracts the first meaningful paragraph after the Module heading,
// stripping HTML/markdown formatting to produce clean text.
function extractModuleDescription(md) {
  // Find content after Module heading
  const headingRe = /(?:<h[1-6][^>]*>\s*Module\s*(?:<code>[^<]+<\/code>|<span[^>]*>[^<]+<\/span>)\s*<\/h[1-6]>|^\s*#{1,6}\s*Module\s+`[^`]+`.*$)/m;
  const match = md.match(headingRe);
  if (!match) return "";

  const afterHeading = md.slice(md.indexOf(match[0]) + match[0].length);

  // Skip blank lines, then grab text up to the next heading or HTML block
  const trimmed = afterHeading.replace(/^\s*\n/, "");
  const endOfParagraph = trimmed.search(/\n\s*(?:\n|<h[2-6]|#{2,6}\s|<!-- )/);
  const paragraph = endOfParagraph > 0 ? trimmed.slice(0, endOfParagraph) : trimmed.slice(0, 300);

  // Strip HTML tags and markdown formatting to get plain text
  return paragraph
    .replace(/<[^>]+>/g, "")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/[*_]/g, "")
    .replace(/<br\s*\/?>/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 300);
}

const frameworkPlugin = (_context, _options) => {
  return {
    name: "sui-framework-plugin",

    async loadContent() {
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

      // ── CHANGED: Collect per-module descriptions for index page generation ──
      // Maps relative dir path → array of {name, description} for child modules
      const moduleDescriptions = new Map();

      allFiles.forEach((theseFiles) => {
        theseFiles.forEach((absFile) => {
          let reMarkdown = safeReadFile(absFile);
          if (reMarkdown === null) {
            errorCount++;
            return;
          }

          // Make hrefs work without ".md"
          reMarkdown = reMarkdown.replace(/<a\s+(.*?)\.md(.*?)>/g, `<a $1$2>`);

          // Legacy anchor + heading with backticked name → HTML heading with id
          reMarkdown = reMarkdown.replace(
            /<a name="([^"]+)"><\/a>\s*\n\s*(#{1,6})\s*([A-Za-z ]+)\s+`([^`]+)`/g,
            (_m, id, hashes, kind, ident) =>
              mdHeadingToHtml(hashes, `${kind} <code>${ident}</code>`, id),
          );

          // Normalize cargo-doc front-matter
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
            .replace(
              /href='(\.\.\/)(bridge|sui|std|sui_system)\//g,
              (m, up, seg) =>
                `href='${up}${CRATE_PREFIX_MAP[seg]}/"`.replace(/"$/, "'"),
            );

          // --- Content cleanup ---
          reMarkdown = cleanNoDescriptionPlaceholders(reMarkdown);
          reMarkdown = cleanEmptySections(reMarkdown);
          reMarkdown = removeEmptyCodeBlocks(reMarkdown);
          reMarkdown = ensureHeadingIdsHtml(reMarkdown);
          reMarkdown = normalizeHtmlWhitespace(reMarkdown);
          reMarkdown = fixMdxEscaping(reMarkdown);
          reMarkdown = rejoinOrphanedInlineElements(reMarkdown);
          reMarkdown = insertLineBreaks(reMarkdown);

          // FINAL STEP: Convert backticks to inline code
          reMarkdown = reMarkdown.replace(
            /`([^`\n]+)`/g,
            (_m, inner) => `<span class="code-inline">${inner.replace(/\$/g, "&#36;")}</span>`,
          );

          reMarkdown = convertInlineCodeToSpan(reMarkdown);
          reMarkdown = rejoinOrphanedInlineElements(reMarkdown);

          // Write to prefixed path
          const filename = absFile.replace(/.*\/docs\/(.*)$/, `$1`);
          const parts = filename.split("/");
          const [root, ...rest] = parts;

          const targetRel = [prefixRootDir(root), ...rest].join("/");
          const fileWrite = path.join(DOCS_PATH, targetRel);

          // ── CHANGED: Extract description for use in index pages ──
          const moduleDesc = extractModuleDescription(reMarkdown);
          if (moduleDesc) {
            const dirKey = path.dirname(targetRel);
            if (!moduleDescriptions.has(dirKey)) moduleDescriptions.set(dirKey, []);
            const nameNoExt = rest[rest.length - 1]?.replace(/\.mdx?$/i, "") || "";
            moduleDescriptions.get(dirKey).push({ name: nameNoExt, description: moduleDesc });
          }

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

      // ── CHANGED: Rewritten buildIndexForDir to include prose description ──
      // Fixes content-start-position: index pages now have a real description
      // paragraph before the module list, so content starts in the top 5%.
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
            children.push({ href: childSlug, text: linkText, name: nameNoExt });
          } else if (ent.isDirectory()) {
            const childSlug = pjoin(
              "/references/framework",
              ...relParts,
              ent.name,
            );
            const linkText = `${frameworkName}::${norm(ent.name)}`;
            children.push({ href: childSlug, text: linkText, name: ent.name });
          }
        }

        children.sort((a, b) =>
          a.text.localeCompare(b.text, undefined, {
            sensitivity: "base",
            numeric: true,
          }),
        );

        const topUnprefixed = topDir?.replace(/^sui_/, "") ?? "";
        const cratePath = CRATE_PACKAGES_PATH[topUnprefixed];
        const crateDescription = cratePath
          ? `Documentation for the modules in the ${cratePath} crate. Select a module from the list to see its details.`
          : `Documentation for the ${titleText} modules.`;

        // ── CHANGED: Build a richer module list with descriptions ──
        // Look up per-module descriptions we extracted during processing
        const dirKey = relParts.join("/");
        const descMap = new Map();
        const descs = moduleDescriptions.get(dirKey) || [];
        for (const d of descs) descMap.set(d.name, d.description);

        const listLines = [];
        for (const child of children) {
          const desc = descMap.get(child.name);
          if (desc) {
            // Truncate description to first sentence or 160 chars for index readability
            let short = desc;
            const sentenceEnd = short.search(/[.!?]\s/);
            if (sentenceEnd > 0 && sentenceEnd < 160) {
              short = short.slice(0, sentenceEnd + 1);
            } else if (short.length > 160) {
              short = short.slice(0, 160).replace(/\s+\S*$/, "") + "…";
            }
            listLines.push(`- [${child.text}](${child.href}): ${short}`);
          } else {
            listLines.push(`- [${child.text}](${child.href})`);
          }
        }
        const listMd = listLines.join("\n");

        const fm = [
          "---",
          `title: "${titleText.replace(/"/g, '\\"')}"`,
          `slug: ${slug}`,
          `description: "${crateDescription.replace(/"/g, '\\"')}"`,
          "---",
          "",
          // ── CHANGED: Add prose paragraph before the module list ──
          // This ensures content-start-position is near 0% instead of 100%
          crateDescription,
          "",
          listMd,
          "",
        ].join("\n");

        try {
          fs.writeFileSync(
            path.join(absDir, "index.md"),
            fm,
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