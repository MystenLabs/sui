/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

// One place that answers "where are the pages" and "where is the code".
//
// Six scripts and docusaurus.config.js each used to climb out of docs/site with
// their own `../..` and arrive at a value called REPO_ROOT, which was then used
// for two unrelated things:
//
//   the pages        docs/content
//   the code quoted  crates/, examples/, external-crates/, release-notes/
//                    by the pages
//
// Conflating them is what ties this build to living inside the monorepo. 382
// <ImportContent> tags name a path like `crates/sui-framework/...` with no org
// or repo and resolve against whatever checkout is on disk, so the code root has
// to stay pointed at a sui checkout even if the pages move. Naming the two roots
// separately is what makes that possible later; nothing moves today.
//
// Defaults reproduce the previous values exactly. Override either with
// DOCS_CONTENT_ROOT or DOCS_SOURCE_ROOT.

const fs = require("fs");
const path = require("path");

// lib/ -> scripts/ -> site/ -> docs/ -> monorepo
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

/** The pages this site renders. */
const CONTENT_ROOT = fromEnv("DOCS_CONTENT_ROOT", path.join(DOCS_ROOT, "content"));

/** A sui checkout, for the code the pages quote. */
const SOURCE_ROOT = fromEnv("DOCS_SOURCE_ROOT", MONOREPO_ROOT);

const sourceFile = (relPath) =>
  path.join(SOURCE_ROOT, String(relPath).replace(/^\.?\//, ""));

const contentFile = (relPath) =>
  path.join(CONTENT_ROOT, String(relPath).replace(/^\.?\//, ""));

function describe() {
  return [
    `  SITE_ROOT     ${SITE_ROOT}`,
    `  CONTENT_ROOT  ${CONTENT_ROOT}${process.env.DOCS_CONTENT_ROOT ? "  (from DOCS_CONTENT_ROOT)" : ""}`,
    `  SOURCE_ROOT   ${SOURCE_ROOT}${process.env.DOCS_SOURCE_ROOT ? "  (from DOCS_SOURCE_ROOT)" : ""}`,
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
  describe,
};
