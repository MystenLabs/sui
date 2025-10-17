// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fileURLToPath } from "url";
import path from "path";
import math from "remark-math";
import katex from "rehype-katex";
//import rehypeRawFiles from "./src/rehype/rehype-raw-only.mjs";
//import rehypeTabsMd from "./src/rehype/rehype-tabs.mjs";
//import rehypeFixAnchorUrls from "./src/rehype/rehype-fix-anchor-urls.mjs";
const npm2yarn = require("@docusaurus/remark-plugin-npm2yarn");

const effortRemarkPlugin = require("./src/plugins/effort");
const betaRemarkPlugin = require("./src/plugins/betatag");

const lightCodeTheme = require("prism-react-renderer/themes/github");
const darkCodeTheme = require("prism-react-renderer/themes/nightOwl");

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const SIDEBARS_PATH = fileURLToPath(new URL("./sidebars.js", import.meta.url));

require("dotenv").config();

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: "Sui Documentation",
  tagline:
    "Sui is a next-generation smart contract platform with high throughput, low latency, and an asset-oriented programming model powered by Move",
  favicon: "/img/favicon.ico",
  headTags: [
    {
      tagName: "meta",
      attributes: {
        name: "algolia-site-verification",
        content: "BCA21DA2879818D2",
      },
    },
  ],
  // Set the production url of your site here
  url: "https://docs.sui.io",
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: "/",
  customFields: {
    amplitudeKey: process.env.AMPLITUDE_KEY,
  },

  onBrokenLinks: "throw",
  onBrokenMarkdownLinks: "throw",

  markdown: {
    format: "detect",
    mermaid: true,
  },
  clientModules: [require.resolve("./src/client/pushfeedback-toc.js")],
  plugins: [
    //require.resolve('./src/plugins/framework'),
    [
      "posthog-docusaurus",
      {
        apiKey: process.env.POSTHOG_API_KEY || "dev", // required
        appUrl: "https://us.i.posthog.com", // optional, defaults to "https://us.i.posthog.com"
        enableInDevelopment: false, // optional
      },
    ],
    [
      "@graphql-markdown/docusaurus",
      {
        id: "alpha",
        schema: "../../crates/sui-graphql-rpc/schema.graphql",
        rootPath: "../content", // docs will be generated under rootPath/baseURL
        baseURL: "references/sui-api/sui-graphql/alpha/reference",
        loaders: {
          GraphQLFileLoader: "@graphql-tools/graphql-file-loader",
        },
      },
    ],
    function stepHeadingLoader() {
      return {
        name: "step-heading-loader",
        configureWebpack() {
          return {
            module: {
              rules: [
                {
                  test: /\.mdx?$/, // run on .md and .mdx
                  enforce: "pre", // make sure it runs BEFORE @docusaurus/mdx-loader
                  include: [
                    // adjust these to match where your Markdown lives
                    path.resolve(__dirname, "../content"),
                  ],
                  use: [
                    {
                      loader: path.resolve(
                        __dirname,
                        "./src/plugins/inject-code/stepLoader.js",
                      ),
                    },
                  ],
                },
              ],
            },
            resolve: {
              alias: {
                "@repo": path.resolve(__dirname, "../../"),
                "@docs": path.resolve(__dirname, "../content/"),
              },
            },
          };
        },
      };
    },
    [
      "@graphql-markdown/docusaurus",
      {
        id: "beta",
        schema: "../../crates/sui-indexer-alt-graphql/schema.graphql",
        rootPath: "../content",
        baseURL: "references/sui-api/sui-graphql/beta/reference",
        docOptions: {
          frontMatter: {
            isGraphQlBeta: true,
          },
        },
        loaders: {
          GraphQLFileLoader: "@graphql-tools/graphql-file-loader",
        },
      },
    ],
    //require.resolve("./src/plugins/tabs-md-client/index.mjs"),
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
      /** @type {import('@docusaurus/preset-classic').Options} */
      "classic",
      {
        docs: {
          path: "../content",
          routeBasePath: "/",
          sidebarPath: SIDEBARS_PATH,
          // the double docs below is a fix for having the path set to ../content
          editUrl: "https://github.com/MystenLabs/sui/tree/main/docs/docs",
          exclude: [
            "**/snippets/**",
            "**/standards/deepbook-ref/**",
            "**/submodules/**",
            "**/app-examples/ts-sdk-ref/**",
          ],
          admonitions: {
            keywords: ["checkpoint"],
            extendDefaults: true,
          },
          beforeDefaultRemarkPlugins: [
          ],
          remarkPlugins: [
            math,
            [npm2yarn, { sync: true, converters: ["yarn", "pnpm"] }],
            effortRemarkPlugin,
            betaRemarkPlugin,
          ],
          //beforeDefaultRehypePlugins: [rehypeFixAnchorUrls],
          //rehypePlugins: [katex, rehypeRawFiles, rehypeTabsMd],
          rehypePlugins: [katex],
        },
        theme: {
          customCss: [
            require.resolve("./src/css/fonts.css"),
            require.resolve("./src/css/custom.css"),
            require.resolve("./src/css/details.css"),
          ],
        },
      },
    ],
  ],

  scripts: [
    //{ src: "./src/js/tabs-md.js", defer: true },
    {
      src: "/js/clarity.js",
      async: true,
    },
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
  themes: ["@docusaurus/theme-mermaid", "docusaurus-theme-github-codeblock"],
  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
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
      codeblock: {
        showGithubLink: true,
        githubLinkLabel: "View on GitHub",
      },
      prism: {
        theme: lightCodeTheme,
        darkTheme: darkCodeTheme,
        additionalLanguages: ["rust", "typescript", "toml", "json"],
      },
    }),
};

export default config;
