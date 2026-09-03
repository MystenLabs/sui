// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const axios = require('axios');
const fs = require('fs');
const path = require('path');

// Create directory
const topdir = path.join(__dirname, "../src/open-spec");

if (!fs.existsSync(topdir)){
    fs.mkdirSync(topdir);
}

// The spec moved when the fullnode JSON-RPC server was removed; release
// branches cut before that still carry it at the legacy path.
const SPEC_PATHS = [
  "crates/sui-indexer-alt-jsonrpc/openrpc.json",
  "crates/sui-open-rpc/spec/openrpc.json",
];

const fetchSpec = async (branch) => {
  let lastError;
  for (const specPath of SPEC_PATHS) {
    try {
      return await axios.get(
        `https://raw.githubusercontent.com/MystenLabs/sui/${branch}/${specPath}`
      );
    } catch (err) {
      lastError = err;
    }
  }
  throw lastError;
};

const downloadFile = async (branch) => {
  const branchDir = path.join(topdir, branch);
  const specDir = path.join(__dirname, `../src/open-spec/${branch}`);
  const specFile = path.join(specDir, "openrpc.json");
  const backupFile = path.join(specDir, "openrpc_backup.json");

  if (!fs.existsSync(branchDir)) {
    fs.mkdirSync(branchDir, { recursive: true });
  }

  if (!fs.existsSync(specDir)) {
    fs.mkdirSync(specDir, { recursive: true });
  }

  try {
    const res = await fetchSpec(branch);

    if (fs.existsSync(backupFile)) {
      fs.unlinkSync(backupFile);
      console.log(`Deleted ${branch} backup spec.`);
    }

    if (fs.existsSync(specFile)) {
      fs.renameSync(specFile, backupFile);
      console.log(`Moved ${branch} spec to backup.`);
    }

    fs.writeFileSync(specFile, JSON.stringify(res.data, null, 2), "utf8");
    console.log(`Downloaded ${branch} spec.`);
  } catch (err) {
    console.error(`Error downloading ${branch} openrpc spec.`, err.message);
  }
};

// Download Mainnet OpenRPC spec
downloadFile("mainnet");

// Download Testnet OpenRPC spec
downloadFile("testnet");

// Download Devnet OpenRPC spec
downloadFile("devnet");