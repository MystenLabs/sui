/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

// This plugin does error checking to
// prevent any URLs from blank anchors #
// as these break the build

import { visit } from "unist-util-visit";

// Treat only true local paths as "files". Everything else is ignored.
const ABSOLUTE_URL = /^[a-z]+:\/\//i;                // http:, https:, etc
const SKIP_SCHEMES = /^(#|mailto:|tel:|data:|javascript:)/i;

function isLocalFileUrl(u) {
  if (!u) return false;
  if (typeof u !== "string") return false;
  const s = u.trim();
  if (!s) return false;
  if (SKIP_SCHEMES.test(s)) return false;           // ← "#", "mailto:", etc.
  if (ABSOLUTE_URL.test(s)) return false;           // ← external
  if (s.startsWith("//")) return false;             // ← protocol-relative
  return true;                                      // ← likely a site-local file path
}

export default function rehypeFixAnchorUrls() {
  return (tree) => {
    visit(tree, "element", (node) => {
      if (!node.properties) return;

      // Normalize href/src/poster early so downstream plugins never see "#"
      for (const key of ["href", "src", "poster"]) {
        const val = node.properties[key];
        if (typeof val === "string") {
          const s = val.trim();

          // If it’s anchor-only or a non-file scheme, mark it & make it inert for file handlers
          if (!isLocalFileUrl(s)) {
            // Prefer to keep behavior the same for anchors, but avoid "/#"
            if (s === "#") {
              node.properties[key] = "#_";          // still an in-page hash, but not just '#'
              node.properties["data-anchor"] = "true";
            }
            continue;
          }

          // It *is* a local file path—leave it alone (other plugins can handle it)
        }
      }

      // srcset can contain multiple URLs — drop non-file entries so no one tries to lstat "#"
      if (typeof node.properties.srcset === "string") {
        const items = node.properties.srcset
          .split(",")
          .map((p) => p.trim())
          .filter(Boolean)
          .filter((part) => {
            const url = part.split(/\s+/)[0];
            return isLocalFileUrl(url);
          });
        node.properties.srcset = items.join(", ");
      }
    });
  };
}
