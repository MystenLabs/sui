// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This plugin gets the descriptions from yaml header and
// adds them to global data as
// { title: doc title, id: docID, description: YAML header, section: the section of llms.txt the file should be listed in }

import path from "path";
import fs from "fs";
import matter from "gray-matter";
import TurndownService from "turndown";

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

      function createSection(path) {
        const parts = path.replace(/^\//, "").split("/");
        if (parts.length === 0) {
          return "";
        } else if (parts.length === 1) {
          return (parts[0][0].toUpperCase() + parts[0].substring(1)).replaceAll(
            "-",
            " ",
          );
        } else {
          return (
            parts[parts.length - 2][0].toUpperCase() +
            parts[parts.length - 2].substring(1)
          ).replaceAll("-", " ");
        }
      }

      let descriptions = [];

      mdxFiles.forEach((file) => {
        const markdown = fs.readFileSync(file, "utf8");
        const { data, content } = matter(markdown);
        if (!data.draft) {
          const re = new RegExp(".*" + docs + "/");
          const id = `/${file.replace(re, "").replace(/\.mdx$/, "")}`;
          const title = data.title ? data.title : "No title";
          const llmSection = data.section ? data.section : createSection(id);
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

          descriptions.push({
            llmSection,
            title,
            id,
            description,
          });
        }
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
    async postBuild({ content, siteConfig, routesPaths = [], outDir }) {
      let llms = [`# ${siteConfig.title}\n`, `${siteConfig.tagline}`];
      const grouped = content.descriptions.reduce((acc, item) => {
        if (!acc[item.llmSection]) {
          acc[item.llmSection] = [];
        }
        acc[item.llmSection].push(item);
        return acc;
      }, {});

      Object.keys(grouped)
        .sort()
        .forEach((section) => {
          llms.push(`\n## ${section}\n`);
          grouped[section].forEach((item) => {
            llms.push(
              `- [${item.title}](${item.id})${item.description !== "" ? `: ${item.description}` : ""}`,
            );
          });
        });
      fs.writeFileSync(`${outDir}/llms.txt`, llms.join(`\n`));

      const skips = ["/404.html", "/search", "/sui-api-ref", "/"];
      let mdContent = [];
      var turndownService = new TurndownService({
        headingStyle: "atx",
        preformattedCode: true,
      });
      turndownService.keep(["table"]);
      for (const c of routesPaths) {
        if (!skips.includes(c)) {
          const pathToFile = path.join(outDir, path.join(c, "index.html"));
          const raw = fs.readFileSync(`${pathToFile}`, "utf-8");
          let start = raw.match(/<div class="theme-doc-markdown markdown">/);
          if (!start) {
            start = raw.match(/<div.*class="main-wrapper/);
          }
          const end = raw.match(/<footer class=/);
          mdContent.push(
            turndownService.turndown(
              `<html>${raw.substring(start.index, end.index)}</html>`,
            ),
          );
        }
      }
      fs.writeFileSync(`${outDir}/llms-full.txt`, mdContent.join("\n\n"));
    },
  };
};

module.exports = descriptionPlugin;
