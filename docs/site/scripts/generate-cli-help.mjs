#!/usr/bin/env node

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, chmodSync } from "node:fs";
import { pipeline } from "node:stream/promises";
import { writeFile, rm } from "node:fs/promises";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";


const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname);
const CACHE_DIR = join(ROOT, ".cache", "sui");
const OUT_DIR = resolve(__dirname, "..", "..", "content", "snippets", "console-output");
const FORCE = process.argv.includes("--force");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getPlatformAsset(tag) {
  const version = tag.replace("testnet-v", "");
  const platform = process.platform;
  const arch = process.arch;

  let suffix;
  if (platform === "darwin" && arch === "arm64") {
    suffix = "macos-arm64";
  } else if (platform === "darwin" && arch === "x64") {
    suffix = "macos-x86_64";
  } else if (platform === "linux" && arch === "arm64") {
    suffix = "ubuntu-aarch64";
  } else if (platform === "linux" && arch === "x64") {
    suffix = "ubuntu-x86_64";
  } else if (platform === "win32" && arch === "x64") {
    suffix = "windows-x86_64";
  } else {
    throw new Error(`Unsupported platform/arch: ${platform}/${arch}`);
  }

  return `sui-testnet-v${version}-${suffix}.tgz`;
}

async function fetchJSON(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`GET ${url} → ${res.status}`);
  return res.json();
}

async function download(url, dest) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`GET ${url} → ${res.status}`);
  const fileStream = createWriteStream(dest);
  await pipeline(res.body, fileStream);
}

// ---------------------------------------------------------------------------
// 1. Determine latest testnet release tag
// ---------------------------------------------------------------------------

console.log("Fetching latest Sui testnet release…");
const releases = await fetchJSON(
  "https://api.github.com/repos/MystenLabs/sui/releases"
);
const release = releases.find((r) => r.tag_name.startsWith("testnet-"));
if (!release) {
  console.error("No testnet release found.");
  process.exit(1);
}
const TAG = release.tag_name;
console.log(`  Latest release: ${TAG}`);

// ---------------------------------------------------------------------------
// 2. Download & cache the binary
// ---------------------------------------------------------------------------

const binDir = join(CACHE_DIR, TAG);
const binPath = join(binDir, "sui");

if (!FORCE && existsSync(binPath)) {
  console.log(`  Using cached binary: ${binPath}`);
} else {
  mkdirSync(binDir, { recursive: true });

  const asset = getPlatformAsset(TAG);
  const url = `https://github.com/MystenLabs/sui/releases/download/${TAG}/${asset}`;
  const tmpTar = join(tmpdir(), `sui-${TAG}.tgz`);

  console.log(`  Downloading ${asset}…`);
  await download(url, tmpTar);

  console.log("  Extracting…");
  execFileSync("tar", ["-xzf", tmpTar, "-C", binDir]);

  // The tarball may nest the binary — find it.
  const find = execFileSync("find", [binDir, "-name", "sui", "-type", "f"], {
    encoding: "utf-8",
  }).trim().split("\n")[0];

  if (!find) {
    console.error("Could not locate `sui` binary in extracted archive.");
    process.exit(1);
  }

  if (resolve(find) !== resolve(binPath)) {
    const { renameSync } = await import("node:fs");
    renameSync(find, binPath);
  }

  chmodSync(binPath, 0o755);
  await rm(tmpTar, { force: true });
  console.log(`  Binary ready: ${binPath}`);
}

// Smoke-test
try {
  execFileSync(binPath, ["--version"], { encoding: "utf-8" });
} catch {
  console.warn("⚠ Sui binary not compatible with this platform — using existing snippets.");
  process.exit(0);
}

// ---------------------------------------------------------------------------
// 3. Generate MDX snippets
// ---------------------------------------------------------------------------

mkdirSync(OUT_DIR, { recursive: true });

const SNIPPETS = [
  ["sui-help.mdx", ["--help"]],
  ["sui-client-help.mdx", ["client", "--help"]],
  ["sui-client-call-help.mdx", ["client", "call", "--help"]],
  ["sui-client-ptb-help.mdx", ["client", "ptb", "--help"]],
  ["sui-replay-help.mdx", ["replay", "--help"]],
  ["sui-keytool-sign-help.mdx", ["keytool", "sign", "--help"]],
  ["sui-keytool-help.mdx", ["keytool", "--help"]],
  ["sui-move-help.mdx", ["move", "--help"]],
  ["sui-move-build-help.mdx", ["move", "build", "--help"]],
  ["sui-validator-help.mdx", ["validator", "--help"]],
  ["sui-validator-report-validator-help.mdx", ["validator", "report-validator", "--help"]],
];

console.log(`Generating ${SNIPPETS.length} help snippets…`);

for (const [filename, args] of SNIPPETS) {
  const output = execFileSync(binPath, args, { encoding: "utf-8" });
  const mdx = "```sh\n" + output + "```\n";
  await writeFile(join(OUT_DIR, filename), mdx);
  console.log(`  ✓ ${filename}`);
}

console.log(`\nDone — snippets written to ${OUT_DIR}`);