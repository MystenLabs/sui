/**
 * Copyright (c) Bucher + Suter.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import { SharedFoldersOption } from "./types";
import fse from "fs-extra";
import path from "path";

export function cleanCopySharedFolders(
  sharedFolders: SharedFoldersOption,
  siteDir: string,
): void {
  copyFolders(true, sharedFolders, siteDir);
}

export function copySharedFolders(
  sharedFolders: SharedFoldersOption,
  siteDir: string,
): void {
  copyFolders(false, sharedFolders, siteDir);
}

function copyFolders(
  cleanFirst: boolean,
  sharedFolders: SharedFoldersOption,
  siteDir: string,
): void {
  const pluginLogPrefix = "[includes] ";

  if (!sharedFolders) {
    throw new Error(
      `${pluginLogPrefix}The configuration option 'sharedFolders' is not defined.`,
    );
  }

  // First check if source folders exist
  sharedFolders.forEach((folderEntry) => {
    const sourceFolder = path.resolve(siteDir, folderEntry.source);
    if (!fse.pathExistsSync(sourceFolder)) {
      throw new Error(
        `${pluginLogPrefix}The configured source folder '${folderEntry.source}' doesn't exist`,
      );
    }
  });

  // Clean folders (if requested) and copy the files
  sharedFolders.forEach((folderEntry) => {
    const sourceFolder = path.resolve(siteDir, folderEntry.source);
    const targetFolder = path.resolve(siteDir, folderEntry.target);
    fse.ensureDirSync(targetFolder);
    if (cleanFirst) {
      console.log(`${pluginLogPrefix}Clean target folder '${targetFolder}'`);
      fse.emptyDirSync(targetFolder);
    }
    console.log(
      `${pluginLogPrefix}Copy folder ${sourceFolder} to '${targetFolder}'`,
    );
    fse.copySync(sourceFolder, targetFolder);
  });
}
