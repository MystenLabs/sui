// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const path = require("path");

const injectCode = (context, opts) => {
  return {
    name: "sui-inject-code-plugin",

    configureWebpack(config, _isServer, _utils) {
      const pluginContentDocsPath = path.join(
        "plugin-content-docs",
        "lib",
        "markdown",
        "index.js",
      );
      let docsPluginInclude = [];
      if (config.module && config.module.rules) {
        var foundContentDocsPlugin = false;
        config.module.rules.forEach((rule) => {
          if (rule === "...") {
            return;
          }

          if (!foundContentDocsPlugin && rule.use && rule.include) {
            const includesArray = rule.include;
            const useArray = rule.use;
            useArray.forEach((useItem) => {
              if (typeof useItem == "object" && useItem.loader) {
                if (useItem.loader.endsWith(pluginContentDocsPath)) {
                  foundContentDocsPlugin = true;
                }
              }
            });
            if (foundContentDocsPlugin) {
              docsPluginInclude = [...includesArray]; // copy the include paths docusaurus-plugin-content-docs
            }
          }
        });
      }

      const loaderOptions = {
        replacements: opts.replacements,
        embeds: opts.embeds,
        sharedFolders: opts.sharedFolders,
      };

      return {
        module: {
          rules: [
            {
              test: /(\.mdx?)$/,
              include: docsPluginInclude,
              use: [
                {
                  loader: path.resolve(__dirname, "./injectLoader.js"),
                  options: loaderOptions,
                },
              ],
            },
          ],
        },
      };
    },
  };
};

module.exports = injectCode;
