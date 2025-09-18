// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const { writeFile } = require("fs/promises");
const axios = require("axios");
const path = require("path");

const repo = {
  owner: "MystenLabs",
  name: "sui-apis",
  branch: "gen-docs",
  filePath: "documentation.json",
};
const PROTOCOL_PATH = path.join(
  __dirname,
  "../../../content/documentation.json",
);

const url = `https://raw.githubusercontent.com/${repo.owner}/${repo.name}/${repo.branch}/${repo.filePath}`;

async function downloadFile() {
  try {
    const res = await axios.get(url, { responseType: "text" });
    await writeFile(PROTOCOL_PATH, res.data);
    console.log(`✅ Saved ${repo.filePath} from branch ${repo.branch}`);
  } catch (err) {
    console.error("❌ Failed to download:", err.message);
  }
}

downloadFile();
