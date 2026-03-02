// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const fs = require("fs");
const path = require("path");

const SPECS = [
  { network: "mainnet", relPath: "src/open-spec/mainnet/openrpc.json", strictJson: true },
  { network: "testnet", relPath: "src/open-spec/testnet/openrpc.json", strictJson: false },
  { network: "devnet", relPath: "src/open-spec/devnet/openrpc.json", strictJson: false },
];

// Treat these as "expected non-JSON stubs".
// Add/remove patterns to match how your repo represents redirects.
function isAllowedNonJsonStub(raw) {
  const s = String(raw || "").trim();

  // Examples this permits:
  // "../tests/..." or "../../something"
  // "/some/path"
  // "https://example.com/openrpc.json"
  if (s.startsWith("../") || s.startsWith("./") || s.startsWith("/")) return true;
  if (/^https?:\/\/\S+$/i.test(s)) return true;

  // If your stub sometimes is a single relative filename, you can allow this:
  // if (/^[\w./-]+\.json$/i.test(s)) return true;

  return false;
}

function fail(msg) {
  const banner = "\n🛑 OpenRPC spec validation failed\n";
  throw new Error(`${banner}${msg}\n`);
}

function validateOpenRpcShape(doc, absPath) {
  const problems = [];

  if (!doc || typeof doc !== "object") problems.push("Root must be an object.");
  if (!doc.openrpc || typeof doc.openrpc !== "string")
    problems.push("Missing/invalid `openrpc` (string).");
  if (!doc.info || typeof doc.info !== "object")
    problems.push("Missing/invalid `info` (object).");
  if (!doc.info?.version || typeof doc.info.version !== "string")
    problems.push("Missing/invalid `info.version` (string).");
  if (!Array.isArray(doc.methods))
    problems.push("Missing/invalid `methods` (array).");

  // Your nav groups by tags[0].name — catch obvious breaks early
  const methods = Array.isArray(doc.methods) ? doc.methods : [];
  const badTags = [];
  for (const m of methods) {
    if (!m || typeof m !== "object") continue;
    if (!Array.isArray(m.tags) || !m.tags[0] || typeof m.tags[0].name !== "string") {
      badTags.push(m.name || "(unnamed method)");
      if (badTags.length >= 8) break;
    }
  }
  if (badTags.length) {
    problems.push(
      `Some methods are missing tags[0].name (used for nav grouping). Examples:\n` +
        badTags.map((x) => `  - ${x}`).join("\n"),
    );
  }

  // UI expects components.schemas to exist for schema nav + ref rendering
  const schemas = doc.components?.schemas;
  if (!schemas || typeof schemas !== "object") {
    problems.push("Missing/invalid `components.schemas` (object).");
  }

  if (problems.length) {
    fail(
      `File:\n  - ${absPath}\n\nProblems:\n` +
        problems.map((p) => `  • ${p}`).join("\n"),
    );
  }
}

function main() {
  const repoRoot = process.cwd();

  let validCount = 0;
  const warnings = [];

  for (const spec of SPECS) {
    const abs = path.join(repoRoot, spec.relPath);

    let raw;
    try {
      raw = fs.readFileSync(abs, "utf8");
    } catch (e) {
      if (spec.strictJson) {
        fail(`Missing required file for ${spec.network}:\n  - ${abs}\n\n${e.message}`);
      } else {
        warnings.push(`⚠️ Missing ${spec.network} spec (allowed): ${abs}`);
        continue;
      }
    }

    // Strict networks must be real JSON.
    if (spec.strictJson) {
      let doc;
      try {
        doc = JSON.parse(raw);
      } catch (e) {
        fail(
          `Invalid JSON in REQUIRED ${spec.network} spec:\n  - ${abs}\n\n` +
            `JSON parse error: ${e.message}\n\n` +
            `First 120 chars:\n${raw.trim().slice(0, 120)}`,
        );
      }
      validateOpenRpcShape(doc, abs);
      validCount++;
      continue;
    }

    // Non-strict: try JSON; if it fails, allow certain stub formats.
    try {
      const doc = JSON.parse(raw);
      validateOpenRpcShape(doc, abs);
      validCount++;
    } catch (e) {
      if (isAllowedNonJsonStub(raw)) {
        warnings.push(
          `⚠️ ${spec.network} spec is a non-JSON stub (allowed): ${abs}\n` +
            `    Content: ${raw.trim().slice(0, 120)}`,
        );
      } else {
        // Not JSON and not an allowed stub → fail.
        fail(
          `Invalid ${spec.network} spec (not JSON and not an allowed stub):\n  - ${abs}\n\n` +
            `JSON parse error: ${e.message}\n\n` +
            `First 120 chars:\n${raw.trim().slice(0, 120)}\n\n` +
            `If this is an intentional stub, update isAllowedNonJsonStub() to match it.`,
        );
      }
    }
  }

  // Safety: ensure at least one real spec validated
  if (validCount < 1) {
    fail(
      "No valid OpenRPC specs were validated. At least one network must provide a real OpenRPC JSON spec.",
    );
  }

  if (warnings.length) {
    console.warn(warnings.join("\n"));
  }

  console.log("✅ OpenRPC spec validation passed.");
}

if (require.main === module) main();

module.exports = { main };
