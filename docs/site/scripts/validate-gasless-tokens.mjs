// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Validates that the gasless stablecoin table in the docs matches the
// MAINNET_* token constants in crates/sui-protocol-config/src/lib.rs.
//
// Exits 0 if the table is up-to-date, exits 1 with a diff if not.
// Wired into the build pipeline via build-and-check.sh.

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const LIB_RS = path.resolve(
  __dirname,
  "../../../crates/sui-protocol-config/src/lib.rs",
);
const MDX_FILE = path.resolve(
  __dirname,
  "../../content/develop/transaction-payment/gasless-stablecoin-transfers.mdx",
);

// Human-readable issuer names keyed by the Rust constant name.
// Update this map when a new token is added to lib.rs.
const ISSUER_MAP = new Map([
  ["MAINNET_USDC", "Circle"],
  ["MAINNET_USDSUI", "Bridge/Stripe"],
  ["MAINNET_SUI_USDE", "Ethena"],
  ["MAINNET_USDY", "Ondo"],
  ["MAINNET_FDUSD", "First Digital"],
  ["MAINNET_AUSD", "Agora"],
  ["MAINNET_USDB", "Bucket Protocol"],
]);

// ---------------------------------------------------------------------------
// Parse lib.rs
// ---------------------------------------------------------------------------

let src;
try {
  src = fs.readFileSync(LIB_RS, "utf8");
} catch {
  console.warn(
    "⚠️  validate-gasless-tokens: could not read lib.rs — skipping validation",
  );
  process.exit(0);
}

// 1. Extract every `const MAINNET_*: &str = "…";` constant.
const CONST_RE = /const\s+(MAINNET_\w+):\s*&str\s*=\s*\n?\s*"([^"]+)"/g;
const constants = new Map();
let m;
while ((m = CONST_RE.exec(src))) {
  constants.set(m[1], m[2]);
}

if (constants.size === 0) {
  console.error(
    "❌ validate-gasless-tokens: no MAINNET_* constants found in lib.rs",
  );
  process.exit(1);
}

// 2. Find which constants are in the active mainnet allowlist.
const allowlistBlock = src.match(
  /if\s+chain\s*==\s*Chain::Mainnet\s*\{[^}]*gasless_allowed_token_types\s*=\s*Some\(vec!\[([^\]]*)\]/s,
);

const activeNames = new Set();
if (allowlistBlock) {
  const refs = allowlistBlock[1].matchAll(/\b(MAINNET_\w+)\b/g);
  for (const ref of refs) {
    activeNames.add(ref[1]);
  }
}

// 3. Build expected rows from active allowlist tokens.
const expectedRows = [];
for (const [name, addr] of constants) {
  if (activeNames.size > 0 && !activeNames.has(name)) continue;

  const symbol = addr.split("::").pop();
  const issuer = ISSUER_MAP.get(name);
  if (!issuer) {
    console.warn(
      `⚠️  validate-gasless-tokens: no issuer mapping for ${name} — add it to ISSUER_MAP in this script`,
    );
  }
  expectedRows.push(`| ${symbol} | ${issuer || "Unknown"} | ${addr} |`);
}

if (expectedRows.length === 0) {
  console.error(
    "❌ validate-gasless-tokens: no active mainnet tokens found",
  );
  process.exit(1);
}

const expectedTable = [
  "| Symbol | Issuer | Package Address |",
  "| --------|--------------|--------------|",
  ...expectedRows,
].join("\n");

// ---------------------------------------------------------------------------
// Parse the existing table from the MDX file
// ---------------------------------------------------------------------------

let mdx;
try {
  mdx = fs.readFileSync(MDX_FILE, "utf8");
} catch {
  console.error(
    `❌ validate-gasless-tokens: could not read ${MDX_FILE}`,
  );
  process.exit(1);
}

// Extract the table block: starts with "| Symbol" header row, ends when
// we hit a line that doesn't start with "|".
const lines = mdx.split("\n");
const tableStart = lines.findIndex((l) => /^\|\s*Symbol\s*\|/.test(l));
if (tableStart === -1) {
  console.error(
    "❌ validate-gasless-tokens: could not find gasless token table in MDX file",
  );
  process.exit(1);
}

let tableEnd = tableStart + 1;
while (tableEnd < lines.length && lines[tableEnd].trimStart().startsWith("|")) {
  tableEnd++;
}

const actualTable = lines
  .slice(tableStart, tableEnd)
  .map((l) => l.trim())
  .join("\n");

// ---------------------------------------------------------------------------
// Compare
// ---------------------------------------------------------------------------

if (actualTable === expectedTable) {
  console.log(
    `✅ validate-gasless-tokens: table is up-to-date (${expectedRows.length} tokens)`,
  );
  process.exit(0);
}

// Mismatch — show diff and fail.
console.error(
  "❌ validate-gasless-tokens: gasless token table is out of date!\n",
);
console.error("Expected (from lib.rs):");
console.error(expectedTable);
console.error("\nActual (in MDX):");
console.error(actualTable);
console.error(
  "\nUpdate the table in docs/content/develop/transaction-payment/gasless-stablecoin-transfers.mdx to match lib.rs.",
);
process.exit(1);
