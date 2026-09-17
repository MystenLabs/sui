// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//
// Generates site/src/data/skills.json from the MystenLabs/skills repository.
//
// This runs at build time (wired into the `prebuild` and `prestart` scripts in
// package.json). When a skill is added, removed, or edited in the skills repo,
// the next docs build regenerates the /skills page automatically. No manual
// edits to the page are needed.
//
// The script reads every `SKILL.md` in the repository. Each skill is the
// directory that contains a `SKILL.md`. It parses the YAML frontmatter for
// `name`, `title`, and `description`, and derives a category tag from the
// top-level folder.
//
// On any failure (network error, API rate limit, or a private repo with no
// token) the script logs a warning and exits 0 without touching the committed
// src/data/skills.json fallback, so the docs build never breaks. To read a
// private repo, set GITHUB_TOKEN (or GH_TOKEN) in the build environment.

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import matter from "gray-matter";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const REPO_OWNER = "MystenLabs";
const REPO_NAME = "skills";
const REPO_BRANCH = "main";
const OUT_PATH = path.join(__dirname, "../src/data/skills.json");

const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
const headers = {
  Accept: "application/vnd.github+json",
  "User-Agent": "sui-docs-skills-generator",
  ...(token ? { Authorization: `Bearer ${token}` } : {}),
};

const MAX_CARD_CHARS = 200;

// SKILL.md descriptions are written to route an agent, not to fill a card: a
// summary sentence, then "Use when ..." trigger guidance, then cross-references
// to sibling skills. The card wants the summary, so keep the opening paragraph
// up to the point the trigger guidance starts.
function cardDescription(raw) {
  if (!raw) return "";
  const firstParagraph = String(raw).trim().split(/\n\s*\n/)[0];
  const text = firstParagraph.replace(/\s+/g, " ").trim();
  const trigger = text.search(/\b(?:Use|Also use)\b[^.]*?\bwhen\b/i);
  let summary = (trigger > 0 ? text.slice(0, trigger) : text)
    .trim()
    .replace(/[\s\u2014-]+$/, "");
  // Every card in the grid is the same width, so a description that runs on
  // would stretch its whole row. Keep whole sentences where one fits.
  if (summary.length > MAX_CARD_CHARS) {
    const lastStop = summary.lastIndexOf(". ", MAX_CARD_CHARS);
    summary =
      lastStop > 0
        ? summary.slice(0, lastStop + 1)
        : summary.slice(0, MAX_CARD_CHARS).trimEnd() + "\u2026";
  }
  return summary;
}

// Turn a kebab-case or snake_case slug into a display title.
function titleCase(slug) {
  return slug
    .split(/[-_]/)
    .filter(Boolean)
    .map((word) => word[0].toUpperCase() + word.slice(1))
    .join(" ");
}


async function getJson(url) {
  const res = await fetch(url, { headers });
  if (!res.ok) {
    throw new Error(`${res.status} ${res.statusText} for ${url}`);
  }
  return res.json();
}

async function main() {
  // 1. Load the existing committed skills.json as the base.
  let existing = [];
  try {
    existing = JSON.parse(fs.readFileSync(OUT_PATH, "utf-8"));
  } catch {
    // No existing file — start fresh.
  }
  const existingSlugs = new Set(existing.map((s) => s.slug));

  // 2. List the full repository tree.
  const tree = await getJson(
    `https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/git/trees/${REPO_BRANCH}?recursive=1`,
  );
  const skillFiles = (tree.tree || []).filter(
    (entry) => entry.type === "blob" && /(^|\/)SKILL\.md$/i.test(entry.path),
  );
  if (skillFiles.length === 0) {
    throw new Error("no SKILL.md files found in the repository tree");
  }

  // 3. Only fetch and add skills that aren't already in the committed file.
  let added = 0;
  for (const file of skillFiles) {
    const dir = path.posix.dirname(file.path);
    const parts = dir.split("/");
    // Peek at the slug before fetching — skip if already known or is the template.
    const dirSlug = parts[parts.length - 1];
    if (dirSlug === "template" || dirSlug === "sui-dev-skills") continue;
    if (existingSlugs.has(dirSlug)) continue;

    const res = await fetch(
      `https://raw.githubusercontent.com/${REPO_OWNER}/${REPO_NAME}/${REPO_BRANCH}/${file.path}`,
      { headers },
    );
    if (!res.ok) {
      console.warn(`  skipping ${file.path}: ${res.status}`);
      continue;
    }
    const { data } = matter(await res.text());
    const slug = data.name || dirSlug;
    if (existingSlugs.has(slug)) continue;

    existing.push({
      slug,
      title: data.title || titleCase(slug),
      description: cardDescription(data.description),
      category: "General",
      path: dir,
    });
    existingSlugs.add(slug);
    added++;
  }

  existing.sort((a, b) => a.title.localeCompare(b.title));

  // 4. Write the data file consumed by src/pages/skills.js.
  fs.writeFileSync(OUT_PATH, JSON.stringify(existing, null, 2) + "\n");
  if (added > 0) {
    console.log(`✅ Added ${added} new skill(s) to src/data/skills.json`);
  } else {
    console.log(`✅ skills.json is up to date (${existing.length} skills)`);
  }
}

main().catch((err) => {
  console.warn(`⚠️  Skill generation skipped: ${err.message}`);
  console.warn("   Keeping the committed src/data/skills.json fallback.");
  process.exit(0); // Never break the build.
});
