/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

// One place that answers "where is the content" and "where is the source".
//
// Eight build scripts used to compute these independently as `../../` from the
// site directory, which quietly assumed three things are always true at once:
// that the docs site, the docs content, and the Rust and Move source all sit in
// the same checkout. That holds today and is the default here.
//
// It stops holding the moment docs/content is served from somewhere else, and
// the two roots are not the same root:
//
//   CONTENT_ROOT  the pages. Where docs/content lives.
//   SOURCE_ROOT   the code those pages quote. Where crates/ and examples/ live.
//
// They differ because `<ImportContent source="crates/..." />` with no org or
// repo resolves against the checkout at build time. There are 382 such tags.
// Pointing SOURCE_ROOT at a fetched checkout of MystenLabs/sui keeps every one
// of them working without editing a single page.
//
// Override either with an environment variable. Both default to the layout
// this repository has today, so setting neither changes nothing.

const fs = require("fs");
const path = require("path");

// lib/ -> scripts/ -> site/
const SITE_ROOT = path.resolve(__dirname, "../../");
const DOCS_ROOT = path.resolve(SITE_ROOT, "../");
const MONOREPO_ROOT = path.resolve(DOCS_ROOT, "../");

const fromEnv = (name, fallback) => {
  const raw = process.env[name];
  if (!raw) return fallback;
  const resolved = path.resolve(raw);
  if (!fs.existsSync(resolved)) {
    throw new Error(`${name}=${raw} resolves to ${resolved}, which does not exist`);
  }
  return resolved;
};

/** The pages. Default: docs/content in this checkout. */
const CONTENT_ROOT = fromEnv("DOCS_CONTENT_ROOT", path.join(DOCS_ROOT, "content"));

/**
 * The code the pages quote: crates/, examples/, and anything else an
 * ImportContent path is relative to. Default: this monorepo.
 */
const SOURCE_ROOT = fromEnv("DOCS_SOURCE_ROOT", MONOREPO_ROOT);

/** Resolve a repo-relative ImportContent path against the source checkout. */
const sourceFile = (relPath) =>
  path.join(SOURCE_ROOT, String(relPath).replace(/^\.?\//, ""));

/** Resolve a path inside the content tree. */
const contentFile = (relPath) =>
  path.join(CONTENT_ROOT, String(relPath).replace(/^\.?\//, ""));

/**
 * True when content and source are the same checkout, which is the layout
 * today. A script that can only work in that case should say so rather than
 * failing obscurely later.
 */
const isSingleCheckout = () => path.join(SOURCE_ROOT, "docs/content") === CONTENT_ROOT;

function describe() {
  return [
    `  SITE_ROOT     ${SITE_ROOT}`,
    `  CONTENT_ROOT  ${CONTENT_ROOT}${process.env.DOCS_CONTENT_ROOT ? "  (from DOCS_CONTENT_ROOT)" : ""}`,
    `  SOURCE_ROOT   ${SOURCE_ROOT}${process.env.DOCS_SOURCE_ROOT ? "  (from DOCS_SOURCE_ROOT)" : ""}`,
    `  single checkout: ${isSingleCheckout()}`,
  ].join("\n");
}

module.exports = {
  SITE_ROOT,
  DOCS_ROOT,
  MONOREPO_ROOT,
  CONTENT_ROOT,
  SOURCE_ROOT,
  sourceFile,
  contentFile,
  isSingleCheckout,
  describe,
};
