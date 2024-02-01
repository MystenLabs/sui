// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Copyright (c) Bucher + Suter.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
var __importDefault =
  (this && this.__importDefault) ||
  function (mod) {
    return mod && mod.__esModule ? mod : { default: mod };
  };
Object.defineProperty(exports, "__esModule", { value: true });
exports.copySharedFolders = exports.cleanCopySharedFolders = void 0;
const fs_extra_1 = __importDefault(require("fs-extra"));
const path_1 = __importDefault(require("path"));
function cleanCopySharedFolders(sharedFolders, siteDir) {
  copyFolders(true, sharedFolders, siteDir);
}
exports.cleanCopySharedFolders = cleanCopySharedFolders;
function copySharedFolders(sharedFolders, siteDir) {
  copyFolders(false, sharedFolders, siteDir);
}
exports.copySharedFolders = copySharedFolders;
function copyFolders(cleanFirst, sharedFolders, siteDir) {
  const pluginLogPrefix = "[includes] ";
  if (!sharedFolders) {
    throw new Error(
      `${pluginLogPrefix}The configuration option 'sharedFolders' is not defined.`,
    );
  }
  // First check if source folders exist
  sharedFolders.forEach((folderEntry) => {
    const sourceFolder = path_1.default.resolve(siteDir, folderEntry.source);
    if (!fs_extra_1.default.pathExistsSync(sourceFolder)) {
      throw new Error(
        `${pluginLogPrefix}The configured source folder '${folderEntry.source}' doesn't exist`,
      );
    }
  });
  // Clean folders (if requested) and copy the files
  sharedFolders.forEach((folderEntry) => {
    const sourceFolder = path_1.default.resolve(siteDir, folderEntry.source);
    const targetFolder = path_1.default.resolve(siteDir, folderEntry.target);
    fs_extra_1.default.ensureDirSync(targetFolder);
    if (cleanFirst) {
      console.log(`${pluginLogPrefix}Clean target folder '${targetFolder}'`);
      fs_extra_1.default.emptyDirSync(targetFolder);
    }
    console.log(
      `${pluginLogPrefix}Copy folder ${sourceFolder} to '${targetFolder}'`,
    );
    fs_extra_1.default.copySync(sourceFolder, targetFolder);
  });
}
