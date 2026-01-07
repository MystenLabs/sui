// Copyright (c) Mysten Labs, Inc.
// Copyright (c) Walrus Foundation
// SPDX-License-Identifier: Apache-2.0

const path = require('path');
const fs = require('fs');

module.exports = function sharedThemeAliasPlugin(context, options) {
  const sharedThemesDir = path.resolve(context.siteDir, 'src/shared/themes');
  
  // Auto-discover all theme folders
  const aliases = {};
  if (fs.existsSync(sharedThemesDir)) {
    fs.readdirSync(sharedThemesDir).forEach(folder => {
      const folderPath = path.join(sharedThemesDir, folder);
      if (fs.statSync(folderPath).isDirectory()) {
        aliases[`@theme/${folder}`] = folderPath;
      }
    });
  }

  return {
    name: 'shared-theme-alias',
    configureWebpack() {
      return {
        resolve: { alias: aliases },
      };
    },
  };
};