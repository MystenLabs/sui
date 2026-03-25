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
    const res = await axios.get(
      `https://raw.githubusercontent.com/MystenLabs/sui/${branch}/crates/sui-open-rpc/spec/openrpc.json`
    );

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