// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { visit } from "unist-util-visit";

const fs = require("fs");
const yaml = require("js-yaml");
const path = require("path");

const VARIABLE_FILE = path.join(
  __dirname,
  "../../../../content/variables.yaml",
);

function getNestedValue(obj, path) {
  return path
    .split(".")
    .reduce(
      (acc, key) => (acc && acc[key] !== undefined ? acc[key] : undefined),
      obj,
    );
}

function remarkVariableReplacer() {
  let variables = {};

  try {
    const fileContent = fs.readFileSync(VARIABLE_FILE, "utf8");
    variables = yaml.load(fileContent);
  } catch (err) {
    console.error("Error loading YAML variable file:", err);
  }

  function replaceVars(str) {
    return str.replace(/@(.*?)@/g, (match, p1) => {
      const key = p1.trim();
      const value = getNestedValue(variables, key);
      return value !== undefined ? value : match;
    });
  }

  return function transformer(tree, file) {
    visit(tree, "text", (node) => {
      node.value = replaceVars(node.value);
    });

    visit(tree, "code", (node) => {
      node.value = replaceVars(node.value);
    });

    visit(tree, "inlineCode", (node) => {
      node.value = replaceVars(node.value);
    });

    visit(tree, "link", (node) => {
      if (node.url) {
        node.url = replaceVars(node.url);
      }
    });

    visit(tree, "image", (node) => {
      if (node.url) {
        node.url = replaceVars(node.url);
      }
    });
  };
}

module.exports = remarkVariableReplacer;
