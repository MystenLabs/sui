// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LoadContext, Plugin } from "@docusaurus/types";
import path from "path";
import { RuleSetCondition, RuleSetUseItem } from "webpack";
import { cleanCopySharedFolders, copySharedFolders } from "./cli";
import { postBuildDeleteFolders } from "./postBuildDeletes";
import {
  IncludeLoaderOptionEmbeds,
  IncludeLoaderOptionReplacements,
  IncludesLoaderOptions,
  IncludesPluginOptions,
  SharedFoldersOption,
} from "./types";

export default function (
  context: LoadContext,
  pluginOptions: IncludesPluginOptions,
): Plugin<void> {
  return {
    name: "sui-docs-inject-code",

    configureWebpack(config, _isServer, _utils) {
      const pluginContentDocsPath = path.join(
        "plugin-content-docs",
        "lib",
        "markdown",
        "index.js",
      );
      let docsPluginInclude: RuleSetCondition = [];
      if (config.module && config.module.rules) {
        var foundContentDocsPlugin = false;
        config.module.rules.forEach((rule) => {
          if (rule === "...") {
            return;
          }

          if (!foundContentDocsPlugin && rule.use && rule.include) {
            const includesArray = rule.include as RuleSetCondition[];
            const useArray = rule.use as RuleSetUseItem[];
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

      const loaderOptions: IncludesLoaderOptions = {
        replacements:
          pluginOptions.replacements as IncludeLoaderOptionReplacements,
        embeds: pluginOptions.embeds as IncludeLoaderOptionEmbeds,
        sharedFolders: pluginOptions.sharedFolders,
      };

      return {
        module: {
          rules: [
            {
              test: /(\.mdx?)$/,
              include: docsPluginInclude,
              use: [
                {
                  loader: path.resolve(__dirname, "./includesLoader.js"),
                  options: loaderOptions,
                },
              ],
            },
          ],
        },
      };
    },

    injectHtmlTags() {
      if (pluginOptions.injectedHtmlTags) {
        return pluginOptions.injectedHtmlTags;
      }
      return {};
    },

    extendCli(cli) {
      cli
        .command("includes:copySharedFolders")
        .description("Copy the configured shared folders")
        .action(() => {
          copySharedFolders(
            pluginOptions.sharedFolders as SharedFoldersOption,
            context.siteDir,
          );
        });

      cli
        .command("includes:cleanCopySharedFolders")
        .description("Delete existing target folders first, copySharedFolders")
        .action(() => {
          cleanCopySharedFolders(
            pluginOptions.sharedFolders as SharedFoldersOption,
            context.siteDir,
          );
        });
    },

    async postBuild(_props) {
      if (pluginOptions.postBuildDeletedFolders) {
        await postBuildDeleteFolders(pluginOptions.postBuildDeletedFolders);
      }
    },
  };
}
