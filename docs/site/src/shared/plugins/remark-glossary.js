/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

// plugins/remark-glossary.js
// Auto-wraps glossary terms with <Term>…</Term> in MDX content.
//
// Requires: `js-yaml`, `unist-util-visit`
//   pnpm add -D js-yaml unist-util-visit

import fs from "fs";
import path from "path";
import * as yaml from "js-yaml";
import { visit } from "unist-util-visit";

function escapeRegex(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function buildMatcher(entries) {
    // Build alternation, longest-first to prefer "JSON API" over "API"
    const terms = entries.flatMap((e) => [e.label, ...(e.aliases || [])]).filter(Boolean);

    // De-duplicate (case-insensitive)
    const seen = new Set();
    const unique = [];
    for (const t of terms) {
        const key = t.toLowerCase();
        if (!seen.has(key)) {
            seen.add(key);
            unique.push(t);
        }
    }

    unique.sort((a, b) => b.length - a.length);

    // \b isn't great for Unicode; use custom boundaries: start|non-word on left/right
    // Also allow inside parentheses/quotes by treating those as boundaries too.
    const pattern = unique.map(escapeRegex).join("|");
    // If no entries, make a regex that never matches
    if (!pattern) return { regex: /$a/, keys: [] };

    // Left boundary: start or not letter/number/underscore
    // Right boundary: end or not letter/number/underscore
    const regex = new RegExp(`(^|[^\\p{L}\\p{N}_])(${pattern})(?=[^\\p{L}\\p{N}_]|$)`, "giu");
    return { regex, keys: unique };
}

function loadGlossary(glossaryPath) {
    const raw = fs.readFileSync(glossaryPath, "utf8");
    const data = yaml.load(raw);

    /** @type {{label:string,definition:string,id?:string,aliases?:string[]}[]} */
    const entries = [];

    if (Array.isArray(data)) {
        for (const item of data) {
            if (!item?.label || !item?.definition) continue;
            entries.push({
                label: String(item.label),
                definition: String(item.definition),
                id: item.id ? String(item.id) : undefined,
                aliases: Array.isArray(item.aliases) ? item.aliases.map(String) : [],
            });
        }
    } else if (data && typeof data === "object") {
        for (const [label, def] of Object.entries(data)) {
            entries.push({ label, definition: String(def) });
        }
    }
    return entries;
}

/**
 * Remark plugin factory
 * @param {{glossaryFile?: string}} options
 */
function remarkGlossary(options = {}) {
    console.log(">>> remarkGlossary plugin loaded");

    const glossaryFile = options.glossaryFile || "static/glossary.json";
    const absPath = path.isAbsolute(glossaryFile)
        ? glossaryFile
        : path.join(process.cwd(), glossaryFile);

    const entries = loadGlossary(absPath);
    const { regex } = buildMatcher(entries);

    // Build a fast lookup map (case-insensitive)
    const byKey = new Map();
    for (const e of entries) {
        const keys = [e.label, ...(e.aliases || []), ...(e.id ? [e.id] : [])];
        for (const k of keys) byKey.set(k.toLowerCase(), e);
    }

    // Nodes we should NOT process beneath
    const BLOCK_SKIP = new Set(["code", "pre"]);
    const INLINE_SKIP = new Set(["inlineCode", "link", "linkReference"]);

    return function transformer(tree) {
        console.log(">>> Processing file with", entries.length, "glossary entries");

        // Track which terms have been processed on this page (case-insensitive)
        const processedTerms = new Set();

        visit(tree, (node, _index, parent) => {
            // Only handle plain text nodes
            if (!node || node.type !== "text" || !node.value) return;

            // Skip if inside code/pre/inlineCode/link/mdx Term
            if (parent) {
                if (BLOCK_SKIP.has(parent.type) || INLINE_SKIP.has(parent.type)) return;
                if (
                    (parent.type === "mdxJsxTextElement" || parent.type === "mdxJsxFlowElement") &&
                    parent.name === "Term"
                ) {
                    return;
                }
            }

            const value = node.value;
            let m;
            let last = 0;
            /** @type {any[]} */
            const nextChildren = [];

            // Reset regex lastIndex for each node
            regex.lastIndex = 0;

            // Iterate all matches while preserving unmatched segments
            while ((m = regex.exec(value)) !== null) {
                const [full, leftBoundary, term] = m;
                const matchStart = m.index;
                const termStart = matchStart + (leftBoundary?.length || 0);

                // Text before the match (not including leftBoundary)
                const before = value.slice(last, matchStart);
                if (before) {
                    nextChildren.push({ type: "text", value: before });
                }

                // Add leftBoundary as separate text if present
                if (leftBoundary) {
                    nextChildren.push({ type: "text", value: leftBoundary });
                }

                const key = String(term).toLowerCase();
                const entry = byKey.get(key);

                if (!entry) {
                    // No entry? Just emit the raw match and keep going
                    nextChildren.push({ type: "text", value: term });
                } else {
                    // Check if this term (or any of its aliases) has already been processed
                    const termKeys = [
                        entry.label.toLowerCase(),
                        ...(entry.aliases || []).map((a) => a.toLowerCase()),
                    ];
                    const isFirstOccurrence = !termKeys.some((k) => processedTerms.has(k));

                    if (isFirstOccurrence) {
                        // Mark all variants of this term as processed
                        termKeys.forEach((k) => processedTerms.add(k));

                        // Emit <Term lookup="entry.label">term</Term> as mdxJsxTextElement
                        nextChildren.push({
                            type: "mdxJsxTextElement",
                            name: "Term",
                            attributes: [
                                { type: "mdxJsxAttribute", name: "lookup", value: entry.label },
                            ],
                            children: [{ type: "text", value: term }],
                        });
                    } else {
                        // Not the first occurrence, just emit the plain text
                        nextChildren.push({ type: "text", value: term });
                    }
                }

                // Move past the full match
                last = matchStart + full.length;
            }

            if (last === 0) return; // no matches, keep node as-is

            const tail = value.slice(last);
            if (tail) nextChildren.push({ type: "text", value: tail });

            // Replace the original text node with the new sequence
            // by splicing into parent.children
            if (parent && Array.isArray(parent.children)) {
                const idx = parent.children.indexOf(node);
                parent.children.splice(idx, 1, ...nextChildren);
            }
        });
    };
}

export default remarkGlossary;