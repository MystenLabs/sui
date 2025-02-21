// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { themes } from "prism-react-renderer";
import path from "path";
import math from "remark-math";
import katex from "rehype-katex";

const effortRemarkPlugin = require("./src/plugins/effort");
const betaRemarkPlugin = require("./src/plugins/betatag");

require("dotenv").config();

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: "Sui Documentation",
  tagline:
    "Sui is a next-generation smart contract platform with high throughput, low latency, and an asset-oriented programming model powered by Move",
  favicon: "/img/favicon.ico",

  // Set the production url of your site here
  url: "https://docs.sui.io",
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: "/",
  customFields: {
    amplitudeKey: process.env.AMPLITUDE_KEY,
  },

  onBrokenLinks: "throw",
  onBrokenMarkdownLinks: "warn",

  // Even if you don't use internationalization, you can use this field to set
  // useful metadata like html lang. For example, if your site is Chinese, you
  // may want to replace "en" with "zh-Hans".
  /*  i18n: {
    defaultLocale: "en",
    locales: [
      "en",
      "el",
      "fr",
      "ko",
      "tr",
      "vi",
      "zh-CN",
      "zh-TW",
    ],
  },*/
  markdown: {
    format: "detect",
    mermaid: true,
  },
  plugins: [
    // ....
    // path.resolve(__dirname, `./src/plugins/examples`),
    [
      "posthog-docusaurus",
      {
        apiKey: process.env.POSTHOG_API_KEY || "dev", // required
        appUrl: "https://us.i.posthog.com", // optional, defaults to "https://us.i.posthog.com"
        enableInDevelopment: false, // optional
      },
    ],
    [path.resolve(__dirname, "src/plugins/inject-code"), {}],
    [
      "@graphql-markdown/docusaurus",
      {
        schema: "../../crates/sui-graphql-rpc/schema.graphql",
        rootPath: "../content", // docs will be generated under rootPath/baseURL
        baseURL: "references/sui-api/sui-graphql/reference",
        loaders: {
          GraphQLFileLoader: "@graphql-tools/graphql-file-loader",
        },
      },
    ],
    [
      "docusaurus-plugin-includes",
      {
        postBuildDeletedFolders: ["../snippets"],
      },
    ],
    async function myPlugin(context, options) {
      return {
        name: "docusaurus-tailwindcss",
        configurePostCss(postcssOptions) {
          // Appends TailwindCSS and AutoPrefixer.
          postcssOptions.plugins.push(require("tailwindcss"));
          postcssOptions.plugins.push(require("autoprefixer"));
          return postcssOptions;
        },
      };
    },
    path.resolve(__dirname, `./src/plugins/descriptions`),
    path.resolve(__dirname, `./src/plugins/framework`),
    path.resolve(__dirname, `./src/plugins/askcookbook`),
    path.resolve(__dirname, `./src/plugins/protocol`),
  ],
  presets: [
    [
      "classic",
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          path: "../content",
          routeBasePath: "/",
          sidebarPath: require.resolve("./sidebars.js"),
          // the double docs below is a fix for having the path set to ../content
          editUrl: "https://github.com/MystenLabs/sui/tree/main/docs/docs",
          /*disableVersioning: true,
          lastVersion: "current",
          versions: {
            current: {
              label: "Latest",
              path: "/",
            },
          },
          onlyIncludeVersions: [
            "current",
            "1.0.0",
          ],*/
          admonitions: {
            keywords: ["checkpoint"],
            extendDefaults: true,
          },
          remarkPlugins: [
            math,
            [
              require("@docusaurus/remark-plugin-npm2yarn"),
              { sync: true, converters: ["yarn", "pnpm"] },
            ],
            effortRemarkPlugin,
            betaRemarkPlugin,
          ],
          rehypePlugins: [katex],
        },
        theme: {
          customCss: [
            require.resolve("./src/css/fonts.css"),
            require.resolve("./src/css/custom.css"),
          ],
        },
      }),
    ],
  ],
  stylesheets: [
    {
      href: "https://fonts.googleapis.com/css2?family=Inter:wght@400;500;700&display=swap",
      type: "text/css",
    },
    {
      href: "https://cdn.jsdelivr.net/npm/katex@0.13.24/dist/katex.min.css",
      type: "text/css",
      integrity:
        "sha384-odtC+0UGzzFL/6PNoE8rX/SPcQDXBJ+uRepguP4QkPCm2LBxH3FA3y+fKSiJ+AmM",
      crossorigin: "anonymous",
    },
    {
      href: "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css",
      type: "text/css",
    },
  ],
  themes: ["@docusaurus/theme-mermaid", "docusaurus-theme-frontmatter"],
  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      algolia: {
        // The application ID provided by Algolia
        appId: "ZF283DJAYX",

        // Public API key: it is safe to commit it
        apiKey: "7f24db6c4ec06d6905592deb228f4460",

        indexName: "sui",

        // Optional: see doc section below
        contextualSearch: false,

        // Optional: Specify domains where the navigation should occur through window.location instead on history.push. Useful when our Algolia config crawls multiple documentation sites and we want to navigate with window.location.href to them.
        // externalUrlRegex: "external\\.com|domain\\.com",

        // Optional: Replace parts of the item URLs from Algolia. Useful when using the same search index for multiple deployments using a different baseUrl. You can use regexp or string in the `from` param. For example: localhost:3000 vs myCompany.com/docs
        //replaceSearchResultPathname: {
        //from: "/docs/", // or as RegExp: /\/docs\//
        //to: "/",
        //},

        // Optional: Algolia search parameters
        //searchParameters: {},

        // Optional: path for search page that enabled by default (`false` to disable it)
        searchPagePath: "search",

        //... other Algolia params
      },
      image: "img/sui-doc-og.png",
      docs: {
        sidebar: {
          autoCollapseCategories: false,
        },
      },
      navbar: {
        title: "Sui Documentation",
        logo: {
          alt: "Sui Docs Logo",
          src: "img/sui-logo.svg",
        },
        items: [
          {
            label: "Guides",
            to: "guides",
          },
          {
            label: "Concepts",
            to: "concepts",
          },
          {
            label: "Standards",
            to: "standards",
          },
          {
            label: "References",
            to: "references",
          },

          /*
          {
            type: "docsVersionDropdown",
            position: "right",
            dropdownActiveClassDisabled: true,
          },
          {
            type: "localeDropdown",
            position: "right",
          },
          */
        ],
      },
      footer: {
        logo: {
          alt: "Sui Logo",
          src: "img/sui-logo-footer.svg",
          href: "https://sui.io",
        },
        style: "dark",
        copyright: `© ${new Date().getFullYear()} Sui Foundation | Documentation distributed under <a href="https://github.com/MystenLabs/sui/blob/main/docs/site/LICENSE">CC BY 4.0</a>`,
      },
      prism: {
        theme: themes.github,
        darkTheme: themes.nightOwl,
        additionalLanguages: ["rust", "typescript", "toml", "json"],
      },
    }),
};

export default config;
