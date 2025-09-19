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
  return fs.readdirSync(absDir).some((name) =>
    /^(index|readme)\.(md|mdx)$/i.test(name)
  );
}

function shouldSkipIndex(absDir) {
  return SKIP_INDEX_AT.has(absDir) || hasPreexistingIndex(absDir);
}

const pjoin = path.posix.join;

const toLowerTitleText = (s) =>
  s.replace(/^sui_/, "").replace(/[-_]+/g, " ").toLowerCase();

/* ----------------- HTML-safe anchor helpers ----------------- */

// Module anchor from either HTML or Markdown module heading
function getModuleAnchor(md) {
  let m = md.match(/<h[1-6][^>]*>\s*Module\s*<code>([^<]+)<\/code>\s*<\/h[1-6]>/m);
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

  // 1) Merge legacy anchors preceding headings → heading id
  // HTML Struct/Function
  md = md.replace(
    /<a name="([^"]+)"><\/a>\s*\n\s*<h([2-6])([^>]*)>\s*((?:Entry\s+Function|Public\s+Function|Function|Struct))\s*<code>([^<]+)<\/code>\s*<\/h\2>/g,
    (_m, name, lvl, attrs, kind, ident) =>
      addIdToHtmlHeading("h", lvl, attrs, `${kind} <code>${ident}</code>`, name)
  );
  // HTML Constants
  md = md.replace(
    /<a name="(@?Constants_\d+)"><\/a>\s*\n\s*<h([2-6])([^>]*)>\s*Constants\b[^<]*<\/h\2>/g,
    (_m, name, lvl, attrs) =>
      addIdToHtmlHeading("h", lvl, attrs, `Constants`, name)
  );
  // Markdown Struct/Function with legacy anchor
  md = md.replace(
    /<a name="([^"]+)"><\/a>\s*\n\s*(#{2,6})\s*((?:Entry\s+Function|Public\s+Function|Function|Struct))\s+`([^`]+)`/g,
    (_m, name, hashes, kind, ident) => mdHeadingToHtml(hashes, `${kind} <code>${ident}</code>`, name)
  );
  // Markdown Constants with legacy anchor
  md = md.replace(
    /<a name="(@?Constants_\d+)"><\/a>\s*\n\s*(#{2,6})\s*Constants\b.*/g,
    (_m, name, hashes) => mdHeadingToHtml(hashes, `Constants`, name)
  );

  // 2) Promote Markdown Struct/Function/Constants to HTML headings w/ ids (avoids MDX plaintext)
  if (moduleAnchor) {
    md = md.replace(
      /^(\#{2,6})\s*Struct\s+`([^`]+)`(?![^\n]*\{#)/gm,
      (_m, hashes, ident) => mdHeadingToHtml(hashes, `Struct <code>${ident}</code>`, `${moduleAnchor}_${ident}`)
    );
    md = md.replace(
      /^(\#{2,6})\s*(Entry\s+Function|Public\s+Function|Function)\s+`([^`]+)`(?![^\n]*\{#)/gm,
      (_m, hashes, kind, ident) => mdHeadingToHtml(hashes, `${kind} <code>${ident}</code>`, `${moduleAnchor}_${ident}`)
    );
    md = md.replace(
      /^(\#{2,6})\s*Constants\b(?![^\n]*\{#)/gm,
      (_m, hashes) => mdHeadingToHtml(hashes, `Constants`, `@Constants_0`)
    );
  }

  // 3) Add IDs to HTML headings if still missing
  if (moduleAnchor) {
    // HTML Struct
    md = md.replace(
      /<h([2-6])([^>]*)>\s*Struct\s*<code>([^<]+)<\/code>\s*<\/h\1>/g,
      (_m, lvl, attrs, ident) =>
        addIdToHtmlHeading("h", lvl, attrs, `Struct <code>${ident}</code>`, `${moduleAnchor}_${ident}`)
    );
    // HTML Functions
    md = md.replace(
      /<h([2-6])([^>]*)>\s*(Entry\s+Function|Public\s+Function|Function)\s*<code>([^<]+)<\/code>\s*<\/h\1>/g,
      (_m, lvl, attrs, kind, ident) =>
        addIdToHtmlHeading("h", lvl, attrs, `${kind} <code>${ident}</code>`, `${moduleAnchor}_${ident}`)
    );
  }
  // HTML Constants (always same id)
  md = md.replace(
    /<h([2-6])([^>]*)>\s*Constants\b([^<]*)<\/h\1>/g,
    (_m, lvl, attrs, tail) =>
      addIdToHtmlHeading("h", lvl, attrs, `Constants${tail || ""}`, `@Constants_0`)
  );

  // 4) Remove stray empty anchors to avoid MDX HTML-mode swallowing content
  md = md.replace(/^\s*<a\b[^>]*><\/a>\s*$/gm, "");

  return md;
}

// Build HTML TOC list after Module heading (uses ensured ids)
function buildHtmlToc(md) {
  const items = [];
  // Structs
  md.replace(
    /<h[2-6][^>]*\sid="([^"]+)"[^>]*>\s*Struct\s*<code>([^<]+)<\/code>\s*<\/h[2-6]>/g,
    (_m, id, name) => { items.push(`<li><a href="#${id}">Struct <code>${name}</code></a></li>`); return _m; }
  );
  // Constants
  if (/<h[2-6][^>]*\sid="@Constants_0"[^>]*>/.test(md)) {
    items.push(`<li><a href="#@Constants_0">Constants</a></li>`);
  }
  // Functions
  md.replace(
    /<h[2-6][^>]*\sid="([^"]+)"[^>]*>\s*(?:Entry\s+Function|Public\s+Function|Function)\s*<code>([^<]+)<\/code>\s*<\/h[2-6]>/g,
    (_m, id, name) => { items.push(`<li><a href="#${id}">Function <code>${name}</code></a></li>`); return _m; }
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

// Insert TOC after Module heading (HTML or Markdown)
function injectToc(md) {
  const toc = buildHtmlToc(md);
  if (!toc) return md;

  // Update if present
  if (/<!-- AUTOGENERATED: NAV-ANCHORS -->[\s\S]*?<!-- \/AUTOGENERATED: NAV-ANCHORS -->/.test(md)) {
    return md.replace(
      /<!-- AUTOGENERATED: NAV-ANCHORS -->[\s\S]*?<!-- \/AUTOGENERATED: NAV-ANCHORS -->/,
      toc
    );
  }

  // Insert after HTML Module heading
  if (/<h[1-6][^>]*>\s*Module\s*<code>[^<]+<\/code>\s*<\/h[1-6]>/.test(md)) {
    return md.replace(
      /(<h[1-6][^>]*>\s*Module\s*<code>[^<]+<\/code>\s*<\/h[1-6]>)/,
      `$1\n\n${toc}\n\n`
    );
  }

  // Insert after Markdown Module heading (convert position only)
  if (/^\s*#{1,6}\s*Module\s+`[^`]+`.*$/m.test(md)) {
    return md.replace(
      /^(\s*#{1,6}\s*Module\s+`[^`]+`.*)$/m,
      (_m, line) => `${line}\n\n${toc}\n\n`
    );
  }

  return md;
}

/* -------------------------------------------------------------------- */

const frameworkPlugin = (_context, _options) => {
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
        fs.mkdirSync(DOCS_PATH);
      }

      const recurseFiles = (dirPath, files = []) => {
        const f = fs.readdirSync(dirPath, { withFileTypes: true });
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
        theseFiles.forEach((absFile) => {
          let reMarkdown = fs.readFileSync(absFile, "utf8");

          // Make hrefs work without ".md"
          reMarkdown = reMarkdown.replace(/<a\s+(.*?)\.md(.*?)>/g, `<a $1$2>`);

          // Legacy anchor + heading with backticked name (Markdown form) → HTML heading with id
          reMarkdown = reMarkdown.replace(
            /<a name="([^"]+)"><\/a>\s*\n\s*(#{1,6})\s*([A-Za-z ]+)\s+`([^`]+)`/g,
            (_m, id, hashes, kind, ident) => mdHeadingToHtml(hashes, `${kind} <code>${ident}</code>`, id)
          );

          // Normalize cargo-doc front-matter: keep full title, but sidebar_label is just the last part.
          reMarkdown = reMarkdown.replace(
            /(title:\s*.*)Module\s+`([^`]+)`/g,
            (_m, titleLine, fullMod) => {
              const last = fullMod.split("::").pop();   // e.g., "chain_ids"
              return `${titleLine}Module ${fullMod}\nsidebar_label: ${last}`;
            }
          );

           // Do NOT strip <p> or convert other <a name=...> to <a id=...>; avoid MDX HTML-mode pitfalls

          // crate-relative link rewriting
          reMarkdown = reMarkdown
            .replace(
              /href=(["'])(\.\.\/)(bridge|sui|std|sui_system)\/([^"']*)\1/g,
              (_m, q, up, seg, tail) => `href=${q}${up}${CRATE_PREFIX_MAP[seg]}/${tail}${q}`,
            )
            // also handle single quotes just in case
            .replace(
              /href='(\.\.\/)(bridge|sui|std|sui_system)\//g,
              (m, up, seg) => `href='${up}${CRATE_PREFIX_MAP[seg]}/"`.replace(/"$/, "'"),
            );

          // Ensure headings have ids (HTML-first), then inject HTML TOC
          reMarkdown = ensureHeadingIdsHtml(reMarkdown);
          reMarkdown = injectToc(reMarkdown);

          // FINAL STEP: Convert backticks to inline code AFTER all other processing
          // This prevents <code><a href="...">text</a></code> which Docusaurus converts to blocks
          reMarkdown = reMarkdown.replace(/`([^`\n]+)`/g, '<span className="code-inline">$1</span>');

          // Write to prefixed path
          const filename = absFile.replace(/.*\/docs\/(.*)$/, `$1`);
          const parts = filename.split("/");
          const [root, ...rest] = parts;

          const targetRel = [prefixRootDir(root), ...rest].join("/");
          const fileWrite = path.join(DOCS_PATH, targetRel);

          // Create directories and category files
          let newDir = DOCS_PATH;
          parts.forEach((part, i) => {
            if (!part.match(/\.md$/)) {
              const onDiskPart = i === 0 ? prefixRootDir(part) : part;
              newDir = path.join(newDir, onDiskPart);

              if (!fs.existsSync(newDir)) {
                fs.mkdirSync(newDir, { recursive: true });

                const catfile = path.join(newDir, "_category_.json");
                const relParts = path.relative(DOCS_PATH, newDir)
                  .split(path.sep).filter(Boolean);
                const slug = pjoin("/references/framework", ...relParts);
                const indexDocId = pjoin("references/framework", ...relParts, "index");

                const top = relParts[0] || parts[0] || "";
                const topUnpref = top.replace(/^sui_/, "");

                // Category label: lowercased dirname without sui_ prefix
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
                  fs.writeFileSync(catfile, JSON.stringify(category, null, 2), "utf8");
                } catch (err) {
                  console.error("An error occurred creating category file:", err);
                }
              }
            }
          });

          fs.mkdirSync(path.dirname(fileWrite), { recursive: true });
          fs.writeFileSync(fileWrite, reMarkdown, "utf8", (err) => {
            if (err) console.error("An error occurred creating framework file:", err);
          });
        });
      });

      function buildIndexForDir(absDir) {
        const relParts = path.relative(DOCS_PATH, absDir).split(path.sep).filter(Boolean);
        const slug = pjoin("/references/framework", ...relParts);

        const dirName = relParts.length ? relParts[relParts.length - 1] : "framework";
        const titleText = `sui:${toLowerTitleText(dirName)}`;

        const entries = fs.readdirSync(absDir, { withFileTypes: true });
        const children = [];
        const topDir = relParts[0] || "";
        const frameworkName = topDir.replace(/^sui_/, "");
        const norm = (s) => s.replace(/\.mdx?$/i, "").toLowerCase().replace(/-/g, "_");

        for (const ent of entries) {
          if (ent.isFile() && /(?:\.mdx?)$/i.test(ent.name) && !/^index\.mdx?$/i.test(ent.name)) {
            const nameNoExt = ent.name.replace(/\.mdx?$/i, "");
            const childSlug = pjoin("/references/framework", ...relParts, nameNoExt);
            const linkText = `${frameworkName}::${norm(nameNoExt)}`;
            children.push({ href: childSlug, text: linkText });
          } else if (ent.isDirectory()) {
            const childSlug = pjoin("/references/framework", ...relParts, ent.name);
            const linkText = `${frameworkName}::${norm(ent.name)}`;
            children.push({ href: childSlug, text: linkText });
          }
        }

        children.sort((a, b) =>
          a.text.localeCompare(b.text, undefined, { sensitivity: "base", numeric: true })
        );

        const listMd = children.length
          ? children.map((c) => `- [${c.text}](${c.href})`).join("\n")
          : "_No pages yet._";

        const fm = [
          "---",
          `title: "${titleText.replace(/"/g, '\\"')}"`,
          `slug: ${slug}`,
          `description: "${(`Documentation for the modules in the ${CRATE_PACKAGES_PATH[topDir?.replace(/^sui_/, "")] ?? ""} crate. Select a module from the list to see its details.`).replace(/"/g, '\\"')}"`,
          "---",
          "",
        ].join("\n");

        try {
          fs.writeFileSync(path.join(absDir, "index.md"), fm + listMd + "\n", "utf8");
        } catch (err) {
          console.error("An error occurred creating index.md:", err);
        }
      }

      function buildAllIndexes() {
        const stack = [DOCS_PATH];
        while (stack.length) {
          const dir = stack.pop();
          const entries = fs.readdirSync(dir, { withFileTypes: true });
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