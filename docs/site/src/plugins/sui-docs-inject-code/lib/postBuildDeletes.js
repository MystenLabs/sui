// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

var __createBinding =
  (this && this.__createBinding) ||
  (Object.create
    ? function (o, m, k, k2) {
        if (k2 === undefined) k2 = k;
        var desc = Object.getOwnPropertyDescriptor(m, k);
        if (
          !desc ||
          ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)
        ) {
          desc = {
            enumerable: true,
            get: function () {
              return m[k];
            },
          };
        }
        Object.defineProperty(o, k2, desc);
      }
    : function (o, m, k, k2) {
        if (k2 === undefined) k2 = k;
        o[k2] = m[k];
      });
var __setModuleDefault =
  (this && this.__setModuleDefault) ||
  (Object.create
    ? function (o, v) {
        Object.defineProperty(o, "default", { enumerable: true, value: v });
      }
    : function (o, v) {
        o["default"] = v;
      });
var __importStar =
  (this && this.__importStar) ||
  function (mod) {
    if (mod && mod.__esModule) return mod;
    var result = {};
    if (mod != null)
      for (var k in mod)
        if (k !== "default" && Object.prototype.hasOwnProperty.call(mod, k))
          __createBinding(result, mod, k);
    __setModuleDefault(result, mod);
    return result;
  };
Object.defineProperty(exports, "__esModule", { value: true });
exports.postBuildDeleteFolders = void 0;
const fse = __importStar(require("fs-extra"));
const join = require("path").join;
async function postBuildDeleteFolders(foldersToDelete) {
  const pluginLogPrefix = "[includes] ";
  console.log(`${pluginLogPrefix}Execute postBuildDeleteFolders...`);
  let versions = [];
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
  let versionInfos = [];
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
  const pathsToDelete = [];
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
  return new Promise((resolve, reject) => {
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
exports.postBuildDeleteFolders = postBuildDeleteFolders;
