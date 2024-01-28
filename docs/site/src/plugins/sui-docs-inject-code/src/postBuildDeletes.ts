// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { VersionInfo } from "./types";
import * as fse from "fs-extra";
const join = require("path").join;

export async function postBuildDeleteFolders(foldersToDelete: string[]) {
  const pluginLogPrefix = "[includes] ";

  console.log(`${pluginLogPrefix}Execute postBuildDeleteFolders...`);

  let versions: string[] = [];
  const CWD = process.cwd();

  // Check if docusaurus build directory exists
  const docusaurusBuildDir = join(CWD, "build");
  if (
    !fse.existsSync(docusaurusBuildDir) ||
    !fse.existsSync(join(docusaurusBuildDir, "index.html")) ||
    !fse.existsSync(join(docusaurusBuildDir, "404.html"))
  ) {
    throw new Error(
      `${pluginLogPrefix}Could not find a valid docusaurus build directory at "${docusaurusBuildDir}".`,
    );
  }

  // Read versions.json and prepare version infos
  try {
    versions = require(`${CWD}/versions.json`);
  } catch (e) {
    console.log(
      `${pluginLogPrefix}No versions.js file found. Continue without versions.`,
    );
  }
  let versionInfos: VersionInfo[] = [];
  if (versions.length == 0) {
    versionInfos.push({ version: "next", urlAddIn: "" });
  } else {
    versionInfos.push({ version: "next", urlAddIn: "next" });
    for (let index = 0; index < versions.length; index++) {
      const version = versions[index];
      versionInfos.push({
        version: version,
        urlAddIn: index === 0 ? "" : version,
      });
    }
  }

  const pathsToDelete: string[] = [];
  for (const deleteFolder of foldersToDelete) {
    for (const versionInfo of versionInfos) {
      const folderPath = join(
        docusaurusBuildDir,
        "docs",
        versionInfo.urlAddIn,
        deleteFolder,
      );
      if (fse.existsSync(folderPath)) {
        pathsToDelete.push(folderPath);
      }
    }
  }

  return new Promise<void>((resolve, reject) => {
    let i = pathsToDelete.length;
    pathsToDelete.forEach(function (filepath) {
      console.log(`${pluginLogPrefix}delete folder ${filepath}`);
      fse.remove(filepath, function (err) {
        i--;
        if (err) {
          console.error(
            `${pluginLogPrefix}Error while deleting the file ${filepath} due to error ${err}`,
          );
          reject(err);
          return;
        } else if (i <= 0) {
          console.log(`${pluginLogPrefix}postBuildDeleteFolders finished!`);
          resolve();
        }
      });
    });
  });
}
