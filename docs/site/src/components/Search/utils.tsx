// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const { decode } = require("he");

/**
 * Strip tooltip text injected by the Algolia crawler.
 *
 * The docs use inline <Tooltip> components whose hidden definition text
 * gets concatenated by the crawler, producing strings like:
 *   "Create, build, and test a MoveMoveAn open source programming language used for all activity on Sui. project​"
 *
 * Pattern: a word is immediately followed by itself + a capital-letter
 * definition ending in a period. We replace the duplicated word + definition
 * with just the original word.
 */
export function cleanTooltipText(text: string): string {
  // Remove zero-width spaces (&#8203; / \u200B / ​)
  let cleaned = text.replace(/\u200B/g, "");
  // Strip duplicated-word tooltip definitions:  Word + Word + UppercaseDefinition...period
  cleaned = cleaned.replace(/(\b\w{2,})\1[A-Z][^.]*\.\s?/g, "$1 ");
  return cleaned.trim();
}

export function truncateAtWord(text, maxChars = 250) {
  if (text.length <= maxChars) return text;
  const decoded = decode(text);
  const truncated = decoded.slice(0, maxChars);
  return truncated.slice(0, truncated.lastIndexOf(" ")) + "…";
}

export function getDeepestHierarchyLabel(hierarchy) {
  const levels = ["lvl0", "lvl1", "lvl2", "lvl3", "lvl4", "lvl5", "lvl6"];
  let lastValue = null;

  for (const lvl of levels) {
    const value = hierarchy[lvl];
    if (value == null) {
      break;
    }
    lastValue = value;
  }

  return lastValue || hierarchy.lvl6 || "";
}

/**
 * Build an ordered breadcrumb array from a DocSearch hierarchy object.
 * Deduplicates adjacent identical levels (e.g. lvl0 === lvl1).
 * Strips crawler tooltip artefacts from each level.
 */
export function getHierarchyBreadcrumbs(hierarchy): string[] {
  if (!hierarchy) return [];
  const levels = ["lvl0", "lvl1", "lvl2", "lvl3", "lvl4", "lvl5", "lvl6"];
  const crumbs: string[] = [];
  for (const lvl of levels) {
    const raw = hierarchy[lvl];
    if (raw == null) break;
    const value = cleanTooltipText(raw);
    if (crumbs.length === 0 || crumbs[crumbs.length - 1] !== value) {
      crumbs.push(value);
    }
  }
  return crumbs;
}
