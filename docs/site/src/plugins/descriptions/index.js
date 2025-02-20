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

      // Creates a default section name if one is not provided in frontmatter
      // The section name is currently only used in the llm text file
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
    // Create llm text file after build so that all processed content like
    // imports and tabs are included
    async postBuild({ content, siteConfig, routesPaths = [], outDir }) {
      // Build a doc that adheres to the early spec: https://llmstxt.org/
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

      // Build a doc that puts all site content into a text file
      // Array of pages that don't need to be included in the llm file
      const skips = ["/404.html", "/search", "/sui-api-ref", "/"];
      let llmsFull = [`# ${siteConfig.title}\n`, `${siteConfig.tagline}`];
      var turndownService = new TurndownService({
        headingStyle: "atx",
        preformattedCode: true,
      });
      turndownService.keep(["table"]);
      for (const route of routesPaths) {
        if (!skips.includes(route)) {
          const pathToFile = path.join(outDir, path.join(route, "index.html"));
          const raw = fs.readFileSync(`${pathToFile}`, "utf-8");
          let start = raw.match(/<div class="theme-doc-markdown markdown">/);
          if (!start) {
            start = raw.match(/<div.*class="main-wrapper/);
          }
          const end = raw.match(/<footer class=/);
          llmsFull.push(
            turndownService.turndown(
              `<html>${raw.substring(start.index, end.index)}</html>`,
            ),
          );
        }
      }
      fs.writeFileSync(`${outDir}/llms-full.txt`, llmsFull.join("\n\n"));
    },
  };
};

module.exports = descriptionPlugin;
