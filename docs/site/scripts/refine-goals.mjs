/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * One-time refinement pass over goal frontmatter:
 *   1. Label all unlabeled has_frontmatter checks
 *   2. Trim long descriptions to ~120 chars max
 *   3. Fix awkward phrasing patterns
 *   4. Reclassify example-pattern pages to guide archetype
 *   5. Remove redundant "using the SDK" suffixes
 *
 * Usage:
 *   node scripts/refine-goals.mjs          # dry run
 *   node scripts/refine-goals.mjs --apply  # write changes
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import matter from 'gray-matter';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const CONTENT_ROOT = path.resolve(__dirname, '..', '..', 'content');
const dryRun = !process.argv.includes('--apply');

function globMdx(dir) {
  const results = [];
  function walk(d) {
    for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
      const full = path.join(d, entry.name);
      if (entry.isDirectory()) {
        if (['node_modules', '.docusaurus', 'build', 'dist'].includes(entry.name)) continue;
        walk(full);
      } else if (entry.name.endsWith('.mdx')) {
        results.push(full);
      }
    }
  }
  walk(dir);
  return results;
}

function countWords(body) {
  const cleaned = body.replace(/```[\s\S]*?```/g, '').replace(/`[^`\n]+`/g, '').replace(/^---[\s\S]*?---\n?/, '');
  return (cleaned.match(/[a-zA-Z0-9]+/g) || []).length;
}

// ─── Fix descriptions ───────────────────────────────────────────────────────

function fixDescription(desc, relPath) {
  if (!desc) return desc;
  let d = desc;

  // Remove "can learn how to" → "can"
  d = d.replace(/can learn how to /g, 'can ');

  // Fix "can implement <verb>" → "can <verb>" only when next word is clearly a verb
  d = d.replace(/can implement (use|create|build|deploy|configure|set up|write|add|enable|initialize|define)/g, 'can $1');

  // Remove redundant "using the SDK" when SDK is already mentioned
  if ((d.match(/SDK/gi) || []).length > 1) {
    d = d.replace(/ using the SDK$/, '');
  }

  // "understands correct configuration of your node ensures" → cleaner
  d = d.replace(/understands correct configuration of your node ensures/i,
    'can configure their node for');

  // Fix "Reader can <noun-phrase> are/is" — description was pasted verbatim
  // "Reader can address-owned objects are owned by..." → "Reader understands address-owned objects are owned by..."
  d = d.replace(/^(Reader can )([a-z].+?) (are |is |cannot |provide |enable )/,
    'Reader understands $2 $3');

  // Fix "Reader understands play solitaire..." → "Reader can play solitaire..."
  d = d.replace(/^Reader understands (play |build |create |deploy |use |run |set up )/,
    'Reader can $1');

  // Fix "understands implement ..." → "understands how ..."
  d = d.replace(/understands implement /g, 'understands how ');

  // Fix "understands reference ..." → "can reference ..."
  d = d.replace(/understands reference /g, 'can look up ');

  // Fix "can reference learn how to" → "can learn how to"
  d = d.replace(/can reference learn how to /g, 'can ');

  // Fix "can reference " → "can look up " (for reference pages)
  d = d.replace(/^(Reader )can reference /g, '$1can look up ');

  // Fix "understands learn" → "understands"
  d = d.replace(/understands learn /g, 'understands ');

  // Trim to ~120 chars at sentence boundary
  if (d.length > 130) {
    // Try to cut at a sentence boundary
    const sentences = d.split(/(?<=[.!])\s+/);
    let trimmed = '';
    for (const s of sentences) {
      if ((trimmed + ' ' + s).trim().length > 120 && trimmed.length > 40) break;
      trimmed = (trimmed + ' ' + s).trim();
    }
    // If first sentence is still too long, cut at a natural break
    if (trimmed.length > 130) {
      // Cut at last comma, conjunction, or word boundary before 130 chars
      const cutPoint = Math.max(
        trimmed.lastIndexOf(', ', 120),
        trimmed.lastIndexOf(' and ', 120),
        trimmed.lastIndexOf(' — ', 120),
        trimmed.lastIndexOf('. ', 120),
      );
      if (cutPoint > 40) {
        trimmed = trimmed.slice(0, cutPoint).trim();
      } else {
        // Cut at last space before 130 chars — always cut at word boundary
        const spacePoint = trimmed.lastIndexOf(' ', 130);
        trimmed = spacePoint > 40 ? trimmed.slice(0, spacePoint).trim() : trimmed.slice(0, 130).trim();
        // If we cut mid-word, back up to last space
        if (trimmed.length > 0 && !trimmed.endsWith(' ') && trimmed !== trimmed.replace(/\S+$/, '').trim()) {
          const lastSpace = trimmed.lastIndexOf(' ');
          if (lastSpace > 40) trimmed = trimmed.slice(0, lastSpace).trim();
        }
      }
    }
    // Remove trailing comma/conjunction
    trimmed = trimmed.replace(/[,;]\s*$/, '').replace(/\s+and\s*$/, '').trim();
    d = trimmed;
  }

  return d;
}

// ─── Fix requires array ─────────────────────────────────────────────────────

function fixRequires(requires, relPath) {
  if (!requires) return requires;

  const isExamplePattern = relPath.startsWith('onchain-finance/examples-patterns/') && !relPath.endsWith('/index.mdx');

  const newRequires = [];

  for (const req of requires) {
    // Label unlabeled has_frontmatter checks
    if (req.has_frontmatter && (!req.label || req.label === '(unlabeled)')) {
      req.label = 'Has required frontmatter fields';
    }

    // For example-pattern pages, replace bootcamp template checks with guide checks
    if (isExamplePattern) {
      // Skip bootcamp-specific checks
      if (req.label === 'Has standard example page sections') continue;
      if (req.label === 'Has code explanation section') continue;
      if (req.label === 'Has architecture diagram') continue;
      // Relax word count
      if (req.min_words && req.min_words >= 800) {
        req.min_words = 200;
        req.label = 'Has enough content to explain the pattern';
      }
      // Relax code block count
      if (req.label === 'Has code blocks for setup, source, and output') {
        req.min = 1;
        req.label = 'Has at least one code example';
      }
    }

    newRequires.push(req);
  }

  // For example-pattern pages, add guide-appropriate checks if missing
  if (isExamplePattern) {
    const hasWordCheck = newRequires.some(r => r.min_words !== undefined);
    if (!hasWordCheck) {
      newRequires.push({ min_words: 200, label: 'Has enough content to explain the pattern' });
    }
  }

  return newRequires;
}

// ─── Main ───────────────────────────────────────────────────────────────────

function main() {
  const files = globMdx(CONTENT_ROOT);
  let updated = 0;
  let descChanged = 0;
  let checksFixed = 0;
  let skipped = 0;

  for (const filePath of files) {
    const relPath = path.relative(CONTENT_ROOT, filePath);
    const raw = fs.readFileSync(filePath, 'utf8');
    const { data, content: body } = matter(raw);

    if (!data.goal) {
      skipped++;
      continue;
    }

    let changed = false;

    // Fix description
    const oldDesc = data.goal.description;
    const newDesc = fixDescription(oldDesc, relPath);
    if (newDesc !== oldDesc) {
      data.goal.description = newDesc;
      descChanged++;
      changed = true;
      if (dryRun) {
        console.log(`DESC ${relPath}`);
        console.log(`  OLD: ${oldDesc}`);
        console.log(`  NEW: ${newDesc}`);
      }
    }

    // Fix requires
    if (data.goal.requires) {
      const oldReqs = JSON.stringify(data.goal.requires);
      data.goal.requires = fixRequires(data.goal.requires, relPath);
      if (JSON.stringify(data.goal.requires) !== oldReqs) {
        checksFixed++;
        changed = true;
      }
    }

    if (changed) {
      updated++;
      if (!dryRun) {
        const newRaw = matter.stringify(body, data);
        fs.writeFileSync(filePath, newRaw, 'utf8');
      }
    }
  }

  console.log(`\n${'─'.repeat(50)}`);
  console.log(`${dryRun ? 'DRY RUN' : 'APPLIED'}`);
  console.log(`  Files updated:        ${updated}`);
  console.log(`  Descriptions changed: ${descChanged}`);
  console.log(`  Check arrays fixed:   ${checksFixed}`);
  console.log(`  Skipped (no goal):    ${skipped}`);

  if (dryRun && updated > 0) {
    console.log(`\nRun with --apply to write changes.`);
  }
}

main();
