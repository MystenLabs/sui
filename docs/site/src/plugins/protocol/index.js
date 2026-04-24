// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Plugin copies file from crates and creates fullnode doc

import path from "path";
import fs from "fs";

const PROTOCOL_PATH = path.join(
  __dirname,
  "../../../../content/documentation.json",
);
const MDX_TEST = /fullnode-protocol(?:-types|-messages)?\.mdx$/;
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
    async postBuild({ outDir }) {
      // Generate a standalone full-detail page outside the docs tree.
      // Not in llms.txt or sitemap, so afdocs won't crawl it.
      const spec = JSON.parse(SPEC_MD);
      const createId = (name) =>
        String(name ?? "").replace(/[\._]/g, "-").replace(/\//g, "_");
      const esc = (s) => String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

      const files = spec.files.filter((f) => !f.name.startsWith("google/"));
      const rows = [];
      for (const file of files) {
        for (const msg of file.messages || []) {
          rows.push(`<h3 id="${createId(msg.fullName)}">${esc(msg.name)}</h3>`);
          if (msg.description) rows.push(`<p>${esc(msg.description).replace(/\n/g, "<br/>")}</p>`);
          if (msg.fields && msg.fields.length) {
            rows.push('<table><thead><tr><th>Field</th><th>Type</th><th>Label</th><th>Description</th></tr></thead><tbody>');
            for (const f of msg.fields) {
              rows.push(
                `<tr><td><b>${esc(f.name)}</b></td>` +
                `<td><a href="#${createId(f.fullType)}">${esc(f.type)}</a></td>` +
                `<td>${esc(f.label)}</td>` +
                `<td>${esc(f.description)}</td></tr>`
              );
            }
            rows.push('</tbody></table>');
          }
        }
      }

      const html = `<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"/>
<title>Sui gRPC Message Definitions — Full Reference</title>
<style>
body{font-family:system-ui,sans-serif;max-width:960px;margin:2rem auto;padding:0 1rem;line-height:1.6;color:#222}
h2{border-bottom:1px solid #ddd;padding-bottom:.3rem}
h3{margin-top:2rem}
table{width:100%;border-collapse:collapse;font-size:14px;margin:0.5rem 0 1.5rem}
th,td{text-align:left;padding:6px 10px;border:1px solid #ddd}
th{background:#f5f5f5;font-weight:600}
a{color:#0066cc}
@media(prefers-color-scheme:dark){body{background:#1a1a1a;color:#ddd}th{background:#2a2a2a}th,td{border-color:#444}a{color:#6cb6ff}}
</style>
</head><body>
<h1>Sui gRPC Message Definitions</h1>
<p>Complete field-level reference for all Sui Full Node gRPC message types.
See also: <a href="/references/fullnode-protocol-messages">Message Definitions</a> |
<a href="/references/fullnode-protocol-types">Enum &amp; Scalar Types</a> |
<a href="/references/fullnode-protocol">Methods</a></p>
${rows.join("\n")}
</body></html>`;

      const dir = path.join(outDir, "doc");
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(path.join(dir, "protocol-messages-full.html"), html);
      console.log("✓ Generated doc/protocol-messages-full.html");
    },
  };
};

module.exports = fullnodeProtocolPlugin;
