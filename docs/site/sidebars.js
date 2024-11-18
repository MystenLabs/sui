// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const getStarted = require("../content/sidebars/getStarted.js");
const guides = require("../content/sidebars/guides.js");
const concepts = require("../content/sidebars/concepts.js");
const standards = require("../content/sidebars/standards.js");
const references = require("../content/sidebars/references.js");

const sidebars = {
  getStartedSidebar: getStarted,
  guidesSidebar: guides,
  conceptsSidebar: concepts,
  standardsSidebar: standards,
  referencesSidebar: references,
};

module.exports = sidebars;
