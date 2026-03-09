// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import path from "path";
import fs from "fs";
import matter from "gray-matter";

// ---------- helpers ----------
function walk(dir, filter) {
  const out = [];
  try {
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    for (const e of entries) {
      const abs = path.join(dir, e.name);
      let st;
      try {
        st = fs.statSync(abs);
      } catch {
        continue;
      }
      if (st.isDirectory()) out.push(...walk(abs, filter));
      else if (st.isFile() && filter(abs)) out.push(abs);
    }
  } catch {
    // ignore unreadable dirs
  }
  return out;
}

function createSection(routePath) {
  const parts = routePath.replace(/^\//, "").split("/");
  if (parts.length === 0) return "";
  if (parts.length === 1)
    return (parts[0][0].toUpperCase() + parts[0].slice(1)).replaceAll("-", " ");
  const p = parts[parts.length - 2];
  return (p[0].toUpperCase() + p.slice(1)).replaceAll("-", " ");
}

function firstParagraphFrom(body) {
  const lines = body.split(/\r?\n/);
  const buf = [];
  for (const raw of lines) {
    const s = raw.trim();
    if (!s) {
      if (buf.length) break;
      continue;
    }
    if (/^import\s+/.test(s)) continue;
    if (/^#{1,6}\s/.test(s)) continue;
    if (/^{\s*@\w+:\s*.+}\s*$/.test(s)) continue;
    if (!/^[a-zA-Z]{1}/.test(s)) continue;
    buf.push(s);
  }
  let paragraph = buf.join(" ").trim();
  if (!paragraph) return "";
  paragraph = paragraph
    .replace(/\[([^\]]+)]\([^)]+\)/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/<[^>]+>/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return paragraph;
}

function computeRouteFromFile(docsRootAbs, fileAbs) {
  const rel = path.relative(docsRootAbs, fileAbs).replace(/\\/g, "/");
  const noExt = rel.replace(/\.(md|mdx|markdown)$/i, "");
  if (noExt.endsWith("/index")) return `/${noExt.slice(0, -"/index".length)}`;
  return `/${noExt}`;
}

// ---------- plugin ----------
const descriptionPlugin = (context, options) => {
  return {
    name: "sui-description-plugin",

    async loadContent() {
      const presetTuple = (context.siteConfig.presets || []).find(
        (p) =>
          Array.isArray(p) &&
          typeof p[0] === "string" &&
          (p[0] === "classic" ||
            p[0] === "@docusaurus/preset-classic" ||
            p[0].endsWith("preset-classic"))
      );

      const docsPathConfig = presetTuple?.[1]?.docs?.path ?? "docs";
      const docsRootAbs = path.resolve(context.siteDir, docsPathConfig);

      const EXCLUDES = [].map((s) => s.replace(/\\/g, "/"));

      const mdFiles = walk(docsRootAbs, (abs) => {
        const norm = abs.replace(/\\/g, "/");
        if (!/\.(md|mdx|markdown)$/i.test(norm)) return false;
        if (EXCLUDES.some((seg) => norm.includes(seg))) return false;
        return true;
      });

      const descriptions = [];

      for (const file of mdFiles) {
        let markdown = "";
        try {
          markdown = fs.readFileSync(file, "utf8");
        } catch {
          continue;
        }

        let data = {};
        let content = "";
        try {
          const parsed = matter(markdown);
          data = parsed.data || {};
          content = parsed.content || "";
        } catch {
          content = markdown;
        }

        if (data.draft) continue;

        const id = computeRouteFromFile(docsRootAbs, file);
        const title = data.title || "No title";
        const llmSection = data.section || createSection(id);

        let description = "";
        if (typeof data.description !== "undefined" && data.description !== null) {
          description = String(data.description).trim();
        } else {
          description = firstParagraphFrom(content);
        }

        descriptions.push({ llmSection, title, id, description });
      }

      return { descriptions, docsRootAbs };
    },

    async contentLoaded({ content, actions }) {
      const { setGlobalData } = actions;
      setGlobalData(content || { descriptions: [] });
    },
  };
};

module.exports = descriptionPlugin;