// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Plugin copies files from specified directories into the
// references/framework directory. Formats the nav listing
// and processes files so they still work in the crates/.../docs
// directory on github. Source files are created via cargo docs.

import path from "path";
import fs from "fs";

const FRAMEWORK_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/sui-framework",
);
const STDLIB_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/move-stdlib",
);
const DEEPBOOK_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/deepbook",
);
const SUISYS_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-framework/docs/sui-system",
);
const DOCS_PATH = path.join(
  __dirname,
  "../../../../content/references/framework",
);


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

      const frameworkFiles = recurseFiles(FRAMEWORK_PATH);
      const stdlibFiles = recurseFiles(STDLIB_PATH);
      const deepbookFiles = recurseFiles(DEEPBOOK_PATH);
      const suisysFiles = recurseFiles(SUISYS_PATH);
      const allFiles = [
        frameworkFiles,
        stdlibFiles,
        deepbookFiles,
        suisysFiles,
      ];
      allFiles.forEach((theseFiles) => {
        theseFiles.forEach((file) => {
          const markdown = fs.readFileSync(file, "utf8");
          // .md extension in links messes up routing.
          // Removing here so linking still works in github crates/docs.
          // Remove the backticks from title.
          // Remove code blocks without pre's. Render automatically adds
          // pre element that messes up formatting.
          // Remove empty code blocks because it looks lame.
          const reMarkdown = markdown
            .replace(/<a\s+(.*?)\.md(.*?)>/g, `<a $1$2>`)
            .replace(
              /(title: .*)Module `(0x[1-9a-f]{1,4}::)(.*)`/g,
              `$1 Module $2$3\nsidebar_label: $3`,
            )
            .replace(/(?<!<pre>)<code>(.*?)<\/code>/gs, `$1`)
            .replace(/<pre><code><\/code><\/pre>/g, "");
          const filename = file.replace(/.*\/docs\/(.*)$/, `$1`);
          const parts = filename.split("/");
          const fileWrite = path.join(DOCS_PATH, filename);
          let newDir = DOCS_PATH;

          // Should work for nested docs, but is currently flat tree.
          parts.forEach((part) => {
            if (!part.match(/\.md$/)) {
              // Capitalize lib name for nav.
              let styledPart = part
                .split("-")
                .map(
                  (word) =>
                    word.charAt(0).toUpperCase() + word.slice(1).toLowerCase(),
                )
                .join(" ");

              newDir = path.join(newDir, part);

              if (!fs.existsSync(newDir)) {
                fs.mkdirSync(newDir);
                // Create file that handles nav label. Only run once at dir create.
                const catfile = path.join(newDir, "_category_.json");
                fs.writeFile(
                  catfile,
                  JSON.stringify({
                    label: styledPart,
                    link: {
                      type: "generated-index",
                      slug: path.join("/references/framework", part),
                      description: `Documentation for the modules in the sui/crates/sui-framework/packages/${part} crate. Select a module from the list to see its details.`,
                    },
                  }),
                  "utf8",
                  (err) => {
                    if (err) {
                      console.error(
                        "An error occurred creating category file:",
                        err,
                      );
                      return;
                    }
                  },
                );
              }
            }
          });
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
