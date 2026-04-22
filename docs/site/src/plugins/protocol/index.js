// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Plugin copies file from crates and creates fullnode doc

import path from "path";
import fs from "fs";

const PROTOCOL_PATH = path.join(
  __dirname,
  "../../../../content/documentation.json",
);
const MDX_TEST = /fullnode-protocol(?:-types)?\.mdx$/;
const SPEC_MD = fs.readFileSync(PROTOCOL_PATH, "utf-8");

const fullnodeProtocolPlugin = (context, options) => {
  return {
    name: "sui-fullnode-protocol-plugin",
    configureWebpack() {
      return {
        module: {
          rules: [
            {
              test: MDX_TEST,
              use: [
                {
                  loader: path.resolve(__dirname, "./protocolLoader-json.js"),
                  options: {
                    protocolSpec: SPEC_MD,
                  },
                },
              ],
            },
          ],
        },
      };
    },
  };
};

module.exports = fullnodeProtocolPlugin;
