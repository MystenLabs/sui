/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Rewrites goal.description for every .mdx page using the page's own
 * description frontmatter and archetype to produce reader-focused,
 * specific goals.
 *
 * Usage:
 *   node scripts/revise-goal-descriptions.mjs          # dry run
 *   node scripts/revise-goal-descriptions.mjs --apply   # write changes
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

// ─── Description generators ────────────────────────────────────────────────

// Capitalize first letter
function cap(s) { return s.charAt(0).toUpperCase() + s.slice(1); }

// Clean up a title for use in a sentence
function cleanTitle(t) {
  return t
    .replace(/^Sui\s+/i, '')
    .replace(/\s*->\s*/g, ' to ')
    .replace(/[`]/g, '')
    .trim();
}

// Turn a page description into a reader-outcome phrase.
// If it starts with a verb, lowercase it for "Reader can <verb>..."
// If it starts with a noun/article, use "understand" framing.
const ACTION_VERBS = /^(build|create|set up|configure|learn|use|deploy|integrate|connect|choose|write|control|combine|install|run|test|debug|monitor|manage|query|subscribe|fetch|implement|publish|upgrade|verify|sign|send|submit|execute|migrate|convert|add|enable|define|register|swap|trade|stake|mint|transfer|delete|remove|check|validate|request|resolve|browse|explore|compare|measure|optimize)/i;

function descToOutcome(desc, fallbackTitle) {
  if (!desc) return null;
  const cleaned = desc.replace(/\.$/, '').trim();
  if (ACTION_VERBS.test(cleaned)) {
    return cleaned.charAt(0).toLowerCase() + cleaned.slice(1);
  }
  // If it's short enough, use "understands" framing
  if (cleaned.length <= 150) {
    return null; // signal to caller to use "understands" framing
  }
  // Long description — take first sentence
  const first = cleaned.split(/[.!]/).filter(Boolean)[0].trim();
  return null; // signal to caller
}

function generateDescription(relPath, data, body) {
  const title = data.title || '';
  const desc = data.description || '';
  const ct = cleanTitle(title);
  const ctLower = ct.toLowerCase();

  // ── Index / landing pages ──
  if (relPath.endsWith('/index.mdx') || relPath.match(/^[^/]+\.mdx$/)) {
    if (desc) {
      return `Reader gets a clear overview of ${ctLower} and knows which subtopic to read next`;
    }
    return `Reader can orient themselves within the ${ctLower} section and find the right page`;
  }

  // ── Getting started: onboarding ──
  if (relPath.startsWith('getting-started/onboarding/')) {
    const specifics = {
      'sui-install.mdx': 'Reader can install the Sui CLI, verify it works, and understand available toolchain components',
      'configure-sui-client.mdx': 'Reader can configure the Sui client for Testnet, Devnet, or a custom network and verify connectivity',
      'get-address.mdx': 'Reader can create a Sui address, understand recovery phrases, and manage multiple addresses',
      'get-coins.mdx': 'Reader can request SUI tokens from the faucet and confirm they arrived in their wallet',
      'hello-world.mdx': 'Reader can clone, build, publish, and call a Move package, understanding each step along the way',
      'app-frontends.mdx': 'Reader can connect a React frontend to their published Move package and see it working locally',
      'local-network.mdx': 'Reader can start a local Sui network, fund addresses, and test against it instead of Testnet',
      'install-source.mdx': 'Reader can build the Sui CLI from source when prebuilt binaries are not suitable',
      'install-binaries.mdx': 'Reader can install prebuilt Sui binaries for their platform',
      'next-steps.mdx': 'Reader knows where to go after completing the onboarding path based on what they want to build',
    };
    const basename = path.basename(relPath);
    if (specifics[basename]) return specifics[basename];
    return `Reader can ${ctLower} on Sui and confirm it worked`;
  }

  // ── Getting started: examples ──
  if (relPath.startsWith('getting-started/examples/')) {
    if (relPath.endsWith('/index.mdx')) {
      return 'Reader can browse available examples and pick one that matches their learning goal';
    }
    if (desc) {
      // Reframe the description as a reader outcome
      const cleaned = desc.replace(/\.$/, '').replace(/^Build\s+/i, 'build ').replace(/^Write\s+/i, 'write ').replace(/^Create\s+/i, 'create ').replace(/^Learn\s+/i, 'learn ');
      return `Reader can ${cleaned.charAt(0).toLowerCase()}${cleaned.slice(1)}`;
    }
    return `Reader can clone and run the ${ctLower} example, understanding the key patterns it demonstrates`;
  }

  // ── Migration guides ──
  if (relPath.startsWith('getting-started/sui-for-')) {
    const platform = title.includes('->') ? title.split('->')[0].trim()
      : title.includes('→') ? title.split('→')[0].trim()
      : title.replace(/^Sui for /i, '').trim();
    return `${platform} developer can map their existing mental model to Sui's object-centric equivalents and start building`;
  }

  // ── Cheat sheet ──
  if (relPath === 'getting-started/dev-cheat-sheet.mdx') {
    return 'Reader can quickly look up best practices for Move, app development, signing, and zkLogin';
  }

  // ── Tooling catalog ──
  if (relPath === 'getting-started/tooling.mdx') {
    return 'Reader can find and choose the right developer tool for writing, testing, deploying, or auditing Move on Sui';
  }

  // ── CLI reference ──
  if (relPath.startsWith('references/cli/')) {
    if (relPath.endsWith('cheatsheet.mdx')) {
      return 'Reader can quickly find the right CLI command for common Sui operations';
    }
    return `Reader can look up ${ctLower} command syntax, flags, and usage examples`;
  }

  // ── IDE reference ──
  if (relPath.startsWith('references/ide/')) {
    return `Reader can set up and use ${ctLower} for a better Move development experience`;
  }

  // ── Other references ──
  if (relPath.startsWith('references/')) {
    if (relPath.includes('contribute/')) {
      return `Contributor understands the ${ctLower} requirements for the Sui documentation`;
    }
    if (relPath.includes('package-managers/')) {
      return `Reader can look up ${ctLower} syntax and configuration options`;
    }
    if (title.toLowerCase().includes('glossary')) {
      return 'Reader can look up unfamiliar Sui terms and concepts by name';
    }
    if (title.toLowerCase().includes('release notes')) {
      return 'Reader can review what changed in each Sui release';
    }
    if (title.toLowerCase().includes('framework')) {
      return 'Reader can browse the Sui framework module reference to find function signatures and type definitions';
    }
    if (desc) {
      const cleaned = desc.replace(/\.$/, '');
      return `Reader can reference ${cleaned.charAt(0).toLowerCase()}${cleaned.slice(1)}`;
    }
    return `Reader can look up ${ctLower} details for their implementation`;
  }

  // ── Operators ──
  if (relPath.startsWith('operators/')) {
    const role = relPath.includes('validator/') ? 'Validator operator'
      : relPath.includes('full-node/') ? 'Node operator'
      : 'Operator';
    if (desc) {
      const outcome = descToOutcome(desc);
      if (outcome) return `${role} can ${outcome}`;
      const cleaned = desc.replace(/\.$/, '');
      return `${role} understands ${cleaned.charAt(0).toLowerCase()}${cleaned.slice(1)}`;
    }
    return `${role} can set up and manage ${ctLower}`;
  }

  // ── SDK reference pages ──
  if (relPath.includes('-sdk/') || relPath.includes('-sdk.mdx')) {
    if (desc) {
      const outcome = descToOutcome(desc);
      if (outcome) return `Developer can ${outcome} using the SDK`;
      const cleaned = desc.replace(/\.$/, '');
      return `Developer understands ${cleaned.charAt(0).toLowerCase()}${cleaned.slice(1)} and can integrate it via the SDK`;
    }
    return `Developer can integrate ${ctLower} into their application using the SDK`;
  }

  // ── Sui stack ──
  if (relPath.startsWith('sui-stack/')) {
    if (desc) {
      const outcome = descToOutcome(desc);
      if (outcome) return `Reader can ${outcome}`;
      const cleaned = desc.replace(/\.$/, '');
      return `Reader understands ${cleaned.charAt(0).toLowerCase()}${cleaned.slice(1)} and knows when to use it`;
    }
    return `Reader understands what ${ctLower} provides and how to integrate it`;
  }

  // ── Troubleshooting / testing ──
  if (relPath.includes('testing-debugging/')) {
    if (title.toLowerCase().includes('error') || title.toLowerCase().includes('troubleshoot')) {
      return 'Reader can identify common Sui errors by their message and apply the documented fix';
    }
    if (desc) {
      const cleaned = desc.replace(/\.$/, '');
      return `Reader can ${cleaned.charAt(0).toLowerCase()}${cleaned.slice(1)}`;
    }
    return `Reader can ${ctLower} for their Move packages`;
  }

  // ── Onchain finance: examples/patterns ──
  if (relPath.includes('examples-patterns/')) {
    if (desc) {
      const cleaned = desc.replace(/\.$/, '');
      return `Reader can implement ${cleaned.charAt(0).toLowerCase()}${cleaned.slice(1)}`;
    }
    return `Reader can implement the ${ctLower} pattern in their Move package`;
  }

  // ── General develop/ and onchain-finance/ guides ──
  if (relPath.startsWith('develop/') || relPath.startsWith('onchain-finance/')) {
    if (desc) {
      const outcome = descToOutcome(desc);
      if (outcome) return `Reader can ${outcome}`;
      const cleaned = desc.replace(/\.$/, '');
      // Long descriptions: take first sentence
      if (cleaned.length > 150) {
        const firstSentence = cleaned.split(/[.!]/).filter(Boolean)[0].trim();
        return `Reader understands ${firstSentence.charAt(0).toLowerCase()}${firstSentence.slice(1)}`;
      }
      return `Reader understands ${cleaned.charAt(0).toLowerCase()}${cleaned.slice(1)}`;
    }
    return `Reader understands ${ctLower} and knows how to apply it in their project`;
  }

  // ── Fallback ──
  if (desc) {
    const cleaned = desc.replace(/\.$/, '');
    return `Reader understands ${cleaned.charAt(0).toLowerCase()}${cleaned.slice(1)}`;
  }
  return `Reader understands ${ctLower}`;
}

// ─── Main ───────────────────────────────────────────────────────────────────

function main() {
  const files = globMdx(CONTENT_ROOT);
  let updated = 0;
  let skipped = 0;
  let unchanged = 0;

  for (const filePath of files) {
    const relPath = path.relative(CONTENT_ROOT, filePath);
    const raw = fs.readFileSync(filePath, 'utf8');
    const { data, content: body } = matter(raw);

    if (!data.goal) {
      skipped++;
      continue;
    }

    const newDesc = generateDescription(relPath, data, body);
    const oldDesc = data.goal.description;

    if (newDesc === oldDesc) {
      unchanged++;
      continue;
    }

    if (dryRun) {
      console.log(`${relPath}`);
      console.log(`  OLD: ${oldDesc}`);
      console.log(`  NEW: ${newDesc}`);
      console.log();
    } else {
      data.goal.description = newDesc;
      const newRaw = matter.stringify(body, data);
      fs.writeFileSync(filePath, newRaw, 'utf8');
    }

    updated++;
  }

  console.log(`${'─'.repeat(50)}`);
  console.log(`${dryRun ? 'DRY RUN' : 'APPLIED'}`);
  console.log(`  Updated:   ${updated}`);
  console.log(`  Unchanged: ${unchanged}`);
  console.log(`  Skipped (no goal): ${skipped}`);

  if (dryRun && updated > 0) {
    console.log(`\nRun with --apply to write changes.`);
  }
}

main();
