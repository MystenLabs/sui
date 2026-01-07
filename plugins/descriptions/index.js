
// This plugin gets the descriptions from yaml header and
// adds them to global data as
// { title: doc title, id: docID, description: YAML header, section: the section of llms.txt the file should be listed in }

import path from "path";
import fs from "fs";
import matter from "gray-matter";
import TurndownService from "turndown";

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
    if (/^import\s+/.test(s)) continue;      // skip import lines
    if (/^#{1,6}\s/.test(s)) continue;       // skip headings
    if (/^{\s*@\w+:\s*.+}\s*$/.test(s)) continue; // skip directives
    if (!/^[a-zA-Z]{1}/.test(s)) continue;   // must start with a letter (keeps your old intent)
    buf.push(s);
  }
  let paragraph = buf.join(" ").trim();
  if (!paragraph) return "";
  // strip simple md/HTML
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
  // collapse foo/index -> /foo
  if (noExt.endsWith("/index")) return `/${noExt.slice(0, -"/index".length)}`;
  return `/${noExt}`;
}

// ---------- plugin ----------
const descriptionPlugin = (context, options) => {
  return {
    name: "sui-description-plugin",

    async loadContent() {
      // Find classic preset options robustly
      const presetTuple = (context.siteConfig.presets || []).find(
        (p) =>
          Array.isArray(p) &&
          typeof p[0] === "string" &&
          (p[0] === "classic" ||
            p[0] === "@docusaurus/preset-classic" ||
            p[0].endsWith("preset-classic"))
      );

      const docsPathConfig = presetTuple?.[1]?.docs?.path ?? "docs";
      // Make absolute against siteDir
      const docsRootAbs = path.resolve(context.siteDir, docsPathConfig);

      // Collect .md/.mdx, skipping known heavy/irrelevant trees
      const EXCLUDES = [
        "/sui-api/sui-graphql/",
        "/content/snippets/",
        "/references/framework/",
        "/standards/deepbook-ref/",
        "/submodules/",
        "/app-examples/ts-sdk-ref/",
      ].map((s) => s.replace(/\\/g, "/"));

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
          continue; // unreadable file
        }

        let data = {};
        let content = "";
        try {
          const parsed = matter(markdown);
          data = parsed.data || {};
          content = parsed.content || "";
        } catch {
          // not valid front-matter; treat whole file as content
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

    async postBuild({ content, siteConfig, routesPaths = [], outDir }) {
      const safeContent = content || { descriptions: [] };
      const items = Array.isArray(safeContent.descriptions)
        ? safeContent.descriptions
        : [];

      // -------- llms.txt (grouped by section)
      let llms = [`# ${siteConfig.title}\n`, `${siteConfig.tagline}`];
      const grouped = items.reduce((acc, item) => {
        const key = item.llmSection || "";
        (acc[key] ||= []).push(item);
        return acc;
      }, {});
      Object.keys(grouped)
        .sort()
        .forEach((section) => {
          llms.push(`\n## ${section}\n`);
          grouped[section].forEach((item) => {
            const tail =
              item.description && String(item.description).trim() !== ""
                ? `: ${item.description}`
                : "";
            llms.push(`- [${item.title}](${item.id})${tail}`);
          });
        });
      try {
        fs.writeFileSync(path.join(outDir, "llms.txt"), llms.join("\n"));
      } catch {
        // ignore write errors to keep build alive
      }

      // -------- llms-full.txt (raw site content converted to markdown)
      const skips = new Set(["/404.html", "/search", "/sui-api-ref", "/"]);
      const td = new TurndownService({
        headingStyle: "atx",
        preformattedCode: true,
      });
      td.keep(["table"]);

      let llmsFull = [`# ${siteConfig.title}\n`, `${siteConfig.tagline}`];
      for (const route of routesPaths) {
        if (skips.has(route)) continue;

        const htmlPath = path.join(outDir, route, "index.html");
        let raw = "";
        try {
          raw = fs.readFileSync(htmlPath, "utf-8");
        } catch {
          continue;
        }

        // Try to isolate the main content region, but fall back safely
        const startMatch =
          raw.match(/<div class="theme-doc-markdown markdown">/) ||
          raw.match(/<main[^>]*>/i) ||
          raw.match(/<div[^>]*class="main-wrapper[^"]*"[^>]*>/i);
        const endMatch =
          raw.match(/<footer class=/) ||
          raw.match(/<\/main>/i) ||
          raw.match(/<\/body>/i);

        let slice;
        if (startMatch && endMatch && endMatch.index > startMatch.index) {
          slice = raw.substring(startMatch.index, endMatch.index);
        } else {
          // last resort: whole document
          slice = raw;
        }

        try {
          llmsFull.push(td.turndown(`<html>${slice}</html>`));
        } catch {
          // if turndown chokes on something odd, skip this route
          continue;
        }
      }

      try {
        fs.writeFileSync(path.join(outDir, "llms-full.txt"), llmsFull.join("\n\n"));
      } catch {
        // ignore write errors
      }
    },
  };
};

module.exports = descriptionPlugin;
