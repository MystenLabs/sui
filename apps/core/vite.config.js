// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const ts = require("typescript");

module.exports.pathAlias = (base) => {
  const configFile = ts.findConfigFile(
    new URL(base).pathname,
    ts.sys.fileExists,
    "tsconfig.json"
  );

  if (!configFile) {
    throw new Error("tsconfig.json not found");
  }

  const { config } = ts.readConfigFile(configFile, ts.sys.readFile);
  const { options } = ts.parseJsonConfigFileContent(config, ts.sys, "./");

  const alias = {};
  Object.entries(options.paths || {}).forEach(([key, [value]]) => {
    const resolvedValue = new URL(value, base).pathname;

    // Rewrite TSConfig paths into Vite Resolve Alias:
    if (key.endsWith("/*")) {
      key = key.replace("/*", "/");
      value = value.replace("/*", "/");
    }

    alias[key] = new URL(value, base).pathname;
  });

  return alias;
};
