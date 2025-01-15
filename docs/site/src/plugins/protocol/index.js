// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Plugin copies file from crates and creates fullnode doc

import path from "path";
import fs from "fs";
import Protocol from "../../components/Protocol";

const PROTOCOL_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-rpc-api/proto/documentation.json",
);

const fullnodeProtocolPlugin = (context, options) => {
  return {
    name: "sui-fullnode-protocol-plugin",

    async loadContent() {
      if (fs.existsSync(PROTOCOL_PATH)) {
        const doc = fs.readFileSync(PROTOCOL_PATH, "utf-8");

        return JSON.parse(doc);
      } else {
        console.log("\n******\nProtocol doc(s) not found.\n******");
        return {};
      }
    },
    async contentLoaded({ content, actions }) {
      const { createData, addRoute } = actions;
      const jsonDataPath = await createData("doc.json", JSON.stringify(content));


      addRoute({
        path: options.route || '/generated-doc',
        component: 'Protocol',
        modules: {
          jsonData: jsonDataPath,
        },
      });
    },
  };
};

module.exports = fullnodeProtocolPlugin;
