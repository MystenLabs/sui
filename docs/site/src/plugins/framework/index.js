// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Plugin copies files from specified directories into the
// references/framework directory. Formats the nav listing
// and processes files so they still work in the crates/.../docs
// directory on github. Source files are created via cargo docs.

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
/*
** Deprecated **
const DEEPBOOK_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/deepbook",
);*/
const SUISYS_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/sui_system",
);
const DOCS_PATH = path.join(
  __dirname,
  "../../../../content/references/framework",
);

// Insert a slug into front-matter (create front-matter if missing)
function ensureFrontmatterSlug(markdown, slugValue) {
  const slugLine = `slug: /${slugValue}`;
  // Already has slug?
  if (/^---\n[\s\S]*?\nslug:\s*/m.test(markdown)) return markdown;

  // Has front-matter: inject slug after opening ---
  if (markdown.startsWith("---\n")) {
    const end = markdown.indexOf("\n---", 4);
    if (end !== -1) {
      const head = markdown.slice(0, end + 1); // includes the first closing newline
      const tail = markdown.slice(end + 1);
      return head.replace(/^---\n/, `---\n${slugLine}\n`) + tail;
    }
  }
  // No front-matter: create minimal one
  return `---\n${slugLine}\n---\n` + markdown;
}

// Create/modify _category_.json in a dir
function upsertCategoryFile(dir, { label, link } = {}) {
  const catfile = path.join(dir, "_category_.json");
  let data = {};
  if (fs.existsSync(catfile)) {
    try {
      data = JSON.parse(fs.readFileSync(catfile, "utf8"));
    } catch {
      data = {};
    }
  }
  if (label) data.label = label;
  if (link) data.link = link;
  fs.writeFileSync(catfile, JSON.stringify(data, null, 2), "utf8");
}

// Ensure a minimal stub doc exists (no cards) and return nothing.
// We do NOT set an `id:` here; Docusaurus infers id from file path.
// We DO set a `slug:` that equals the category root URL.
function ensureStubIndexDoc(dirAbs, slugValue, title) {
  const stubPath = path.join(dirAbs, "_index.md");
  if (!fs.existsSync(stubPath)) {
    const contents = `---\ntitle: ${title}\nslug: /${slugValue}\n---\n`;
    fs.writeFileSync(stubPath, contents, "utf8");
  }
}

// Title-case a segment for labels
function humanizeSegment(part) {
  return part
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
    .join(" ");
}

const frameworkPlugin = (context, options) => {
  return {
    name: "sui-framework-plugin",

    async loadContent() {
      // framework folder is added to gitignore, so should only exist locally.
      // Clearing the folder programmatically messes up the watch dev build,
      // so only do it when the directory is missing. Should never exist on vercel.
      if (fs.existsSync(DOCS_PATH)) {
        console.log(
          "\n******\nSkipping framework doc build. If you want to rebuild, delete the framework folder before restarting the server.\n******",
        );
        return;
      } else {
        fs.mkdirSync(DOCS_PATH, { recursive: true });
      }

      const recurseFiles = (dirPath, files = []) => {
        const f = fs.readdirSync(dirPath, { withFileTypes: true });
        // Copy md files from provided directory.
        f.forEach((file) => {
          const fp = path.join(dirPath, file.name);
          if (file.isDirectory()) {
            recurseFiles(fp, files);
          } else if (file.isFile() && path.extname(file.name) === ".md") {
            files.push(fp);
          }
        });

        return files;
      };

      const bridgeFiles = recurseFiles(BRIDGE_PATH);
      const frameworkFiles = recurseFiles(FRAMEWORK_PATH);
      const stdlibFiles = recurseFiles(STDLIB_PATH);
      // const deepbookFiles = recurseFiles(DEEPBOOK_PATH);
      const suisysFiles = recurseFiles(SUISYS_PATH);
      const allFiles = [
        bridgeFiles,
        frameworkFiles,
        stdlibFiles,
        // deepbookFiles,
        suisysFiles,
      ];

      allFiles.forEach((theseFiles) => {
        theseFiles.forEach((file) => {
          const markdown = fs.readFileSync(file, "utf8");
          // Normalize generated markdown
          let reMarkdown = markdown
            // .md extension in links messes up routing.
            .replace(/<a\s+(.*?)\.md(.*?)>/g, `<a $1$2>`)
            // Remove backticks around module name in title and set sidebar label
            .replace(
              /(title: .*)Module `(.*::)(.*)`/g,
              `$1 Module $2$3\nsidebar_label: $3`,
            )
            // Remove inline <code> not inside <pre>
            .replace(/(?<!<pre>)<code>(.*?)<\/code>/gs, `$1`)
            // Remove empty code blocks
            .replace(/<pre><code><\/code><\/pre>/g, "")
            // Convert named anchors on headings to IDs
            .replace(
              /<a name="([^"]+)"><\/a>\n\n(#+) (.+) `([^`]+)`/g,
              `$2 $3 \`$4\` {#$1}`,
            )
            .replace(/<a name=/g, "<a style='scroll-margin-top:80px' id=");

          // Compute output path and parts
          const filename = file.replace(/.*\/docs\/(.*)$/, `$1`);
          const parts = filename.split("/");
          const fileWrite = path.join(DOCS_PATH, filename);
          let newDir = DOCS_PATH;

          // Walk path parts to create dirs & category files
          parts.forEach((part, i) => {
            if (part.match(/\.md$/)) {
              // File
              const base = part.replace(/\.md$/, "");
              const parent = parts[i - 1];
              const isCategoryCollision = base === parent || base === "index";

              if (isCategoryCollision) {
                // 1) Force unique slug on the colliding doc so it won't be used as index
                const slug = fileWrite.replace(
                  /^.*?\/content\/(.*)\.md$/,
                  `$1`,
                );
                reMarkdown = ensureFrontmatterSlug(reMarkdown, slug);

                // 2) Create a minimal stub page for the category and link the category to it
                const parentRel = parts.slice(0, i).join("/"); // e.g. "bridge"
                const parentDirAbs = path.join(DOCS_PATH, parentRel);

                const label =
                  humanizeSegment(parentRel.split("/").pop() || "Docs");

                // Doc id to reference in category link (path without extension)
                // Docusaurus will infer this id from the file path.
                const stubDocId = path.posix.join(
                  "references",
                  "framework",
                  parentRel,
                  "_index",
                );

                // Slug for the stub so it renders at the category root URL
                const stubSlug = path.posix.join(
                  "references",
                  "framework",
                  parentRel,
                  "",
                );

                fs.mkdirSync(parentDirAbs, { recursive: true });
                ensureStubIndexDoc(parentDirAbs, stubSlug, label);

                // Point the category to the stub doc (super minimal page)
                upsertCategoryFile(parentDirAbs, {
                  label,
                  link: { type: "doc", id: stubDocId },
                });
              }
            } else {
              // Directory
              const styledPart = humanizeSegment(part);
              newDir = path.join(newDir, part);

              if (!fs.existsSync(newDir)) {
                fs.mkdirSync(newDir, { recursive: true });
                // Create/refresh category label (no link yet)
                upsertCategoryFile(newDir, { label: styledPart });
              }
            }
          });

          // Ensure directories exist then write the processed file
          fs.mkdirSync(path.dirname(fileWrite), { recursive: true });
          fs.writeFileSync(fileWrite, reMarkdown, "utf8");
        });
      });

      return;
    },
  };
};

module.exports = frameworkPlugin;
