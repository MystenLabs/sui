// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Plugin copies file from crates and creates fullnode doc

import path from "path";
import fs from "fs";
import matter from "gray-matter";

const PROTOCOL_PATH = path.join(
  __dirname,
  "../../../../../crates/sui-rpc-api/proto/documentation.md",
);
const MDX_PATH = path.join(
  __dirname,
  "../../../../content/references/fullnode-protocol.mdx",
);

const fullnodeProtocolPlugin = (context, options) => {
  return {
    name: "sui-fullnode-protocol-plugin",

    async loadContent() {
      if (fs.existsSync(PROTOCOL_PATH) && fs.existsSync(MDX_PATH)) {
        const doc = fs.readFileSync(PROTOCOL_PATH);
        const mdx = fs.readFileSync(MDX_PATH);
        const content = doc.toString();
        const mdxContent = mdx.toString();
        // Remove extra linespaces in tables, escape curlies, remove extra lines,
        // remove "top" anchors, replace name anchors with md style.
        const contentOut = content
          .replace(/(^\|.*)\n\n(?!\|)/gm, "$1 ")
          .replace(/\{/g, "&#123;")
          .replace(/^(?:.*?<a name="[^"]+"><\/a>){2}.*?##.*?\n/s, "")
          .replace(/<p align="right"><a href="#top">Top<\/a><\/p>/g, "")
          .replace(/<a name="([^"]+)"><\/a>\n+(#+) ([^\n]+)/g, "$2 $3 {#$1}");
        //const fm = matter(mdxContent.match(/^---\n([\s\S]*?)\n---\n/)[0]);
        const fm = matter(mdxContent);
        const desc = fm.data.description;
        const contents = matter.stringify(
          [
            "\n<style>{`\ntable {\ndisplay: table;\nmin-width: 100%;\n}\n`}</style>\n",
            desc,
            contentOut,
          ].join("\n"),
          fm.data,
        );
        fs.writeFileSync(MDX_PATH, contents, "utf-8");
      } else {
        console.log("\n******\nProtocol doc(s) not found.\n******");
        return;
      }
    },
  };
};

module.exports = fullnodeProtocolPlugin;
