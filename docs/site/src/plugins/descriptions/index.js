// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This plugin gets the descriptions from yaml header and
// adds them to global data as
// { id: docID, description: YAML header }

import path from "path";
import fs from "fs";
import matter from "gray-matter";

const descriptionPlugin = (context, options) => {
  return {
    name: "sui-description-plugin",

    async loadContent() {
      const c = context.siteConfig.presets.filter((s) => s[0] === "classic");
      const docs = c[0][1].docs.path;
      const docPath = path.join(__dirname, "../../..", docs);
      const mdxFiles = recurseFiles(docPath);

      function recurseFiles(dirPath, files = []) {
        const f = fs.readdirSync(dirPath, { withFileTypes: true });

        f.forEach((file) => {
          const fp = path.join(dirPath, file.name);
          if (file.isDirectory()) {
            recurseFiles(fp, files);
          } else if (file.isFile() && path.extname(file.name) === ".mdx") {
            if (!fp.match(/\/sui-api\/sui-graphql\//) && !fp.match(/snippets/))
              files.push(fp);
          }
        });

        return files;
      }

      let descriptions = [];

      mdxFiles.forEach((file) => {
        const markdown = fs.readFileSync(file, "utf8");
        const { data, content } = matter(markdown);
        let description = "";
        if (typeof data.description !== "undefined") {
          description = data.description;
        } else {
          const splits = content.split("\n");
          for (const s of splits) {
            if (
              s.trim() !== "" &&
              !s.match(/^import/) &&
              s.match(/^[a-zA-Z]{1}(.*)$/)
            ) {
              description = s.replace(/\[([^\]]+)\]\([^\)]+\)/g, "$1");
              break;
            }
          }
        }
        const re = new RegExp(".*" + docs + "/");
        descriptions.push({
          id: file.replace(re, "").replace(/\.mdx$/, ""),
          description,
        });
      });
      const descriptionData = {
        descriptions,
      };
      return descriptionData;
    },
    // This function exposes the loaded content to `globalData`
    async contentLoaded({ content, actions }) {
      const { setGlobalData } = actions;
      setGlobalData(content);
    },
  };
};

module.exports = descriptionPlugin;
