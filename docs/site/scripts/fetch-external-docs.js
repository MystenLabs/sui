// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//
// Fetches documentation from external GitHub repositories at build time.
//
// Usage:
//   node scripts/fetch-external-docs.js [config-name] [--force]
//
// Reads configuration from external-docs.json. If config-name is omitted,
// fetches all configured sources. Pass --force to skip the freshness cache
// (used in CI builds).
//
// On network failure the script logs a warning and exits 0 so local dev
// is not blocked. The Docusaurus build will fail if expected content is
// missing, which is the correct behavior for CI.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

const SITE_ROOT = path.resolve(__dirname, "../");
const REPO_ROOT = path.resolve(SITE_ROOT, "../../");
const CONTENT_ROOT = path.resolve(SITE_ROOT, "../content");
const CONFIG_PATH = path.join(SITE_ROOT, "external-docs.json");
const CACHE_DIR = path.join(REPO_ROOT, ".cache-external-docs");
const FRESHNESS_MINUTES = 10;

const args = process.argv.slice(2);
const force = args.includes("--force");
const requestedName = args.find((a) => !a.startsWith("--"));

function loadConfig() {
  const raw = fs.readFileSync(CONFIG_PATH, "utf8");
  return JSON.parse(raw);
}

function isFresh(dir) {
  if (force) return false;
  try {
    const stat = fs.statSync(dir);
    const ageMs = Date.now() - stat.mtimeMs;
    return ageMs < FRESHNESS_MINUTES * 60 * 1000;
  } catch {
    return false;
  }
}

function fetchRepo(name, config) {
  const { repo, branch, sourcePath } = config;
  const cacheDir = path.join(CACHE_DIR, name);
  const sourceDir = path.join(cacheDir, sourcePath);

  if (isFresh(sourceDir)) {
    console.log(`⏩ ${name}: cache is fresh (< ${FRESHNESS_MINUTES}m), skipping fetch`);
    return sourceDir;
  }

  console.log(`📥 ${name}: fetching ${repo}@${branch}/${sourcePath}`);

  // Clean previous cache for this source
  if (fs.existsSync(cacheDir)) {
    fs.rmSync(cacheDir, { recursive: true });
  }
  fs.mkdirSync(cacheDir, { recursive: true });

  try {
    // Use git sparse-checkout to fetch only the docs directory
    execSync(
      [
        `git clone --depth 1 --filter=blob:none --sparse`,
        `--branch ${branch}`,
        `https://github.com/${repo}.git`,
        `"${cacheDir}"`,
      ].join(" "),
      { stdio: "pipe" },
    );

    execSync(`git -C "${cacheDir}" sparse-checkout set "${sourcePath}"`, {
      stdio: "pipe",
    });

    // Touch the directory to mark freshness
    const now = new Date();
    fs.utimesSync(sourceDir, now, now);

    console.log(`✅ ${name}: fetched successfully`);
    return sourceDir;
  } catch (err) {
    console.warn(`⚠️  ${name}: fetch failed (${err.message}). Skipping.`);
    return null;
  }
}

function main() {
  const config = loadConfig();
  const names = requestedName ? [requestedName] : Object.keys(config);

  for (const name of names) {
    if (!config[name]) {
      console.error(`❌ Unknown config: ${name}`);
      process.exit(1);
    }
    fetchRepo(name, config[name]);
  }
}

main();
