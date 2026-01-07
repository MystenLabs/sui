/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

// Rewrites custom headings like:
//   ##step Title
//   ###substep Another Title
// (also accepts colon after the keyword, and optional space: "## step: Title")
// into numbered headings:
//   ## Step 1: Title
//   ### Step 1.1: Another Title
//
// Safe with:
// - YAML frontmatter at top (--- ... ---), including `mdx: format md`
// - Fenced code blocks (``` / ~~~), even when indented or with info strings
// - Optional ATX closing hashes at line end (## ... ##)

const STEP_RE = /^(?<indent>[ \t]*)(?<hashes>#{1,6})step\b[ \t]*:?[ \t]*(?<title>.*?)(?<trail>[ \t]#+[ \t]*)?$/i;
const SUBSTEP_RE = /^(?<indent>[ \t]*)(?<hashes>#{1,6})substep\b[ \t]*:?[ \t]*(?<title>.*?)(?<trail>[ \t]#+[ \t]*)?$/i;

// Extract a leading YAML frontmatter block if present.
// Supports optional BOM and \n / \r\n newlines.
function splitFrontmatter(src) {
  const m = src.match(/^\uFEFF?---\r?\n[\s\S]*?\r?\n---\r?\n?/);
  if (!m || m.index !== 0) return { frontmatter: "", body: src };
  return { frontmatter: m[0], body: src.slice(m[0].length) };
}

function rewriteBody(body) {
  const eol = body.includes("\r\n") ? "\r\n" : "\n";
  const lines = body.split(/\r?\n/);

  let stepCounter = 0;
  let substepCounter = 0;

  // Track fenced code blocks (``` or ~~~). Allow leading spaces and info strings.
  let inFence = false;
  let fenceMarker = null; // '`' or '~'
  let fenceLen = 0;

  function toggleFence(line) {
    const m = line.match(/^[ \t]*(`{3,}|~{3,})(.*)$/);
    if (!m) return false;
    const marks = m[1];
    const marker = marks[0];
    const len = marks.length;

    if (!inFence) {
      inFence = true;
      fenceMarker = marker;
      fenceLen = len;
      return true;
    }
    if (marker === fenceMarker && len >= fenceLen) {
      inFence = false;
      fenceMarker = null;
      fenceLen = 0;
      return true;
    }
    return false;
  }

  const out = lines.map((line) => {
    if (toggleFence(line)) return line;
    if (inFence) return line;

    const s = STEP_RE.exec(line);
    if (s) {
      stepCounter += 1;
      substepCounter = 0;
      const { indent, hashes, title, trail = "" } = s.groups;
      return `${indent}${hashes} Step ${stepCounter}: ${title}${trail}`;
    }

    const ss = SUBSTEP_RE.exec(line);
    if (ss) {
      if (stepCounter === 0) stepCounter = 1; // allow substep before any step
      substepCounter += 1;
      const { indent, hashes, title, trail = "" } = ss.groups;
      return `${indent}${hashes} Step ${stepCounter}.${substepCounter}: ${title}${trail}`;
    }

    return line;
  });

  return out.join(eol);
}

const rewrite = async (content) => {
  const { frontmatter, body } = splitFrontmatter(content);
  const rewritten = rewriteBody(body);
  // Reattach frontmatter exactly as-is (no extra blank lines added).
  return frontmatter ? frontmatter + rewritten : rewritten;
};

module.exports = function (source) {
  const callback = this.async(); // mark the loader as async

  rewrite(source)
    .then((result) => callback(null, result))
    .catch((err) => callback(err));
};
