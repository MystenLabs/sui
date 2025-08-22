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
          const markdown = fs.readFileSync(absFile, "utf8");

          // Remove .md in <a href="..."> links so routing works on GitHub and site
          // Fix front-matter title/sidebar_label
          // Normalize code blocks
          // Transform legacy named anchors to id'ed headings
          // Rewrite internal crate links to the new prefixed dirs
          let reMarkdown = markdown
            .replace(/<a\s+(.*?)\.md(.*?)>/g, `<a $1$2>`)
            .replace(
              /(title: .*)Module `(.*::)(.*)`/g,
              `$1 Module $2$3\nsidebar_label: $3`,
            )
            .replace(/(?<!<pre>)<code>(.*?)<\/code>/gs, `$1`)
            .replace(/<pre><code><\/code><\/pre>/g, "")
            .replace(
              /<a name="([^"]+)"><\/a>\n\n(#+) (.+) `([^`]+)`/g,
              `$2 $3 \`$4\` {#$1}`,
            )
            .replace(/<a name=/g, "<a style='scroll-margin-top:80px' id=")
            // *** NEW: rewrite crate-relative links to the prefixed dirs ***
            .replace(
              /href=(["'])(\.\.\/)(bridge|sui|std|sui_system)\/([^"']*)\1/g,
              (_m, q, up, seg, tail) => `href=${q}${up}${CRATE_PREFIX_MAP[seg]}/${tail}${q}`,
            )
            // also handle single quotes just in case
            .replace(
              /href='(\.\.\/)(bridge|sui|std|sui_system)\//g,
              (m, up, seg) => `href='${up}${CRATE_PREFIX_MAP[seg]}/"`.replace(/"$/, "'"),
            );

          // relative filename under crates/*/docs
          const filename = absFile.replace(/.*\/docs\/(.*)$/, `$1`);
          const parts = filename.split("/"); // e.g. ["bridge","foo.md"]
          const [root, ...rest] = parts;

          // write to /content/references/framework/sui_<root>/...rest
          const targetRel = [prefixRootDir(root), ...rest].join("/");
          const fileWrite = path.join(DOCS_PATH, targetRel);

          // Create directories along the way; prefix the FIRST segment only
          let newDir = DOCS_PATH;
          parts.forEach((part, i) => {
            if (part.match(/\.md$/)) {
              // No longer needed: the parent dir won't equal the filename after prefix.
              // But keep slug injection in case future nested dirs mirror filenames.
              if (part.replace(/\.md$/, "") === parts[i - 1]) {
                const slug = fileWrite.replace(/^.*?\/content\/(.*)\.md$/, `$1`);
                reMarkdown = reMarkdown.replace(
                  /sidebar_label/,
                  `slug: /${slug}\nsidebar_label`,
                );
              }
            } else {
              const onDiskPart = i === 0 ? prefixRootDir(part) : part;

              // Human label from the original segment (without prefix)
              const styledPart = part
                .split("-")
                .map(
                  (word) =>
                    word.charAt(0).toUpperCase() + word.slice(1).toLowerCase(),
                )
                .join(" ");

              newDir = path.join(newDir, onDiskPart);

              if (!fs.existsSync(newDir)) {
                fs.mkdirSync(newDir, { recursive: true });

                // Category file label shows the original name, but slug uses the prefixed path
                const catfile = path.join(newDir, "_category_.json");
                const slug = path.join("/references/framework", i === 0 ? onDiskPart : onDiskPart);
                fs.writeFile(
                  catfile,
                  JSON.stringify({
                    label: styledPart},
                    ),
                  "utf8",
                  (err) => {
                    if (err) {
                      console.error(
                        "An error occurred creating category file:",
                        err,
                      );
                    }
                  },
                );
              }
            }
          });

          // Ensure parent dir exists before writing file
          fs.mkdirSync(path.dirname(fileWrite), { recursive: true });

          fs.writeFileSync(fileWrite, reMarkdown, "utf8", (err) => {
            if (err) {
              console.error("An error occurred creating framework file:", err);
              return;
            }
          });
        });
      });

      return;
    },
  };
};

module.exports = frameworkPlugin;
