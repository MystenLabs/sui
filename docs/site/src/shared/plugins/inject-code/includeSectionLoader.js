/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

// Webpack pre-loader that resolves {@include} directives at build time.
//
// Syntax:
//   {@include ./file.mdx#heading-slug}        — extract full section
//   {@include ./file.mdx#heading-slug@tab-id}  — extract one TabItem's content
//
// The heading line itself is NOT included in the output — the consuming page
// provides its own heading structure.

const fs = require("fs");
const nodePath = require("path");

const INCLUDE_RE = /^\{@include\s+([^#\s]+)#([^@}\s]+)(?:@([^}\s]+))?\s*\}$/gm;

function slugify(text) {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, "")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-")
    .trim();
}

function stripFrontmatter(src) {
  const m = src.match(/^\uFEFF?---\r?\n[\s\S]*?\r?\n---\r?\n?/);
  if (!m || m.index !== 0) return src;
  return src.slice(m[0].length);
}

// Extract the body of a section identified by heading slug.
function extractSection(body, slug) {
  const lines = body.split(/\r?\n/);
  let startIdx = -1;
  let headingLevel = 0;

  let inFence = false;
  let fenceMarker = null;
  let fenceLen = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    const fm = line.match(/^[ \t]*(`{3,}|~{3,})/);
    if (fm) {
      const marker = fm[1][0];
      const len = fm[1].length;
      if (!inFence) {
        inFence = true;
        fenceMarker = marker;
        fenceLen = len;
      } else if (marker === fenceMarker && len >= fenceLen) {
        inFence = false;
        fenceMarker = null;
        fenceLen = 0;
      }
      continue;
    }
    if (inFence) continue;

    const hm = line.match(/^(#{1,6})\s+(.+?)(?:\s+#+\s*)?$/);
    if (!hm) continue;

    const level = hm[1].length;
    const title = hm[2];

    if (startIdx === -1) {
      if (slugify(title) === slug) {
        startIdx = i + 1;
        headingLevel = level;
      }
    } else {
      if (level <= headingLevel) {
        return lines.slice(startIdx, i).join("\n").trim();
      }
    }
  }

  if (startIdx !== -1) {
    return lines.slice(startIdx).join("\n").trim();
  }

  return null;
}

// Extract the inner content of a <TabItem value="tabId"> from a section string.
function extractTabItem(section, tabId) {
  // Match <TabItem value="tabId" ...> allowing label and other attrs in any order
  const openRe = new RegExp(
    `<TabItem\\b[^>]*\\bvalue=["']${tabId}["'][^>]*>`,
    "s"
  );
  const openMatch = section.match(openRe);
  if (!openMatch) return null;

  const startPos = openMatch.index + openMatch[0].length;

  // Find the matching </TabItem> — handle nesting by counting open/close tags.
  let depth = 1;
  let pos = startPos;
  while (depth > 0 && pos < section.length) {
    const nextOpen = section.indexOf("<TabItem", pos);
    const nextClose = section.indexOf("</TabItem>", pos);

    if (nextClose === -1) break;

    if (nextOpen !== -1 && nextOpen < nextClose) {
      depth++;
      pos = nextOpen + 8;
    } else {
      depth--;
      if (depth === 0) {
        return section.slice(startPos, nextClose).trim();
      }
      pos = nextClose + 10;
    }
  }

  return null;
}

module.exports = function includeSectionLoader(source) {
  const callback = this.async();
  const resourceDir = nodePath.dirname(this.resourcePath);

  const hasIncludes = INCLUDE_RE.test(source);
  INCLUDE_RE.lastIndex = 0;

  let result = source;
  const errors = [];

  result = result.replace(INCLUDE_RE, (match, relPath, slug, tabId) => {
    const absPath = nodePath.resolve(resourceDir, relPath);

    if (!fs.existsSync(absPath)) {
      errors.push(`@include: file not found: ${absPath}`);
      return `<!-- @include error: file not found: ${relPath} -->`;
    }

    this.addDependency(absPath);

    const raw = fs.readFileSync(absPath, "utf8");
    const body = stripFrontmatter(raw);
    const section = extractSection(body, slug);

    if (section === null) {
      errors.push(`@include: section "#${slug}" not found in ${relPath}`);
      return `<!-- @include error: section "#${slug}" not found in ${relPath} -->`;
    }

    // If no tab specified, return the full section.
    if (!tabId) {
      return section;
    }

    // Extract content from a specific TabItem.
    const tabContent = extractTabItem(section, tabId);
    if (tabContent === null) {
      errors.push(
        `@include: tab "@${tabId}" not found in section "#${slug}" of ${relPath}`
      );
      return `<!-- @include error: tab "@${tabId}" not found in #${slug} of ${relPath} -->`;
    }

    return tabContent;
  });

  if (errors.length > 0) {
    this.emitWarning(new Error("@include warnings:\n" + errors.join("\n")));
  }

  callback(null, result);
};
