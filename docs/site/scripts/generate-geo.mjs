/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Generates GEO/AEO frontmatter (questions + answer) for docs pages.
 *
 * Derives questions from:
 *   - Page title (→ "What is X?", "How do I X?")
 *   - Page headings (→ question-form conversions)
 *   - Page description (→ primary answer)
 *
 * Derives answer from:
 *   - Page description + first substantive paragraph
 *
 * Usage:
 *   node scripts/generate-geo.mjs          # dry run
 *   node scripts/generate-geo.mjs --apply  # write changes
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

function getHeadings(body) {
  const headings = [];
  for (const line of body.split('\n')) {
    const m = line.match(/^(#{1,3})\s+(.*)$/);
    if (m) headings.push({ level: m[1].length, text: m[2].trim() });
  }
  return headings;
}

function getIntro(body) {
  // Get text before first ## heading, strip JSX/HTML tags
  const match = body.match(/^([\s\S]*?)(?=^##\s)/m);
  const intro = match ? match[1] : body.slice(0, 500);
  return intro
    .replace(/<[^>]+>/g, '')
    .replace(/```[\s\S]*?```/g, '')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\n{2,}/g, ' ')
    .trim()
    .slice(0, 300);
}

// ─── Question generation ────────────────────────────────────────────────────

// Detect if title is already a question
function isQuestion(text) {
  return /^(what|how|why|when|where|can|do|is|are|should|which)\b/i.test(text) && text.includes('?');
}

// Detect archetype from path
function getArchetype(relPath) {
  if (relPath.startsWith('getting-started/onboarding/')) return 'onboarding';
  if (relPath.startsWith('getting-started/examples/')) return 'example';
  if (relPath.startsWith('getting-started/sui-for-')) return 'migration';
  if (relPath.startsWith('operators/')) return 'operator';
  if (relPath.includes('-sdk/') || relPath.includes('-sdk.mdx')) return 'sdk';
  if (relPath.startsWith('references/')) return 'reference';
  if (relPath.endsWith('/index.mdx') || !relPath.includes('/')) return 'index';
  return 'guide';
}

function generateQuestions(title, description, headings, relPath) {
  const questions = [];
  const titleClean = title.replace(/[`]/g, '').trim();
  const archetype = getArchetype(relPath);

  // If title is already a question, use it
  if (isQuestion(titleClean)) {
    questions.push(titleClean);
  }

  // Clean title for use in questions — strip leading "What is", question marks, gerunds
  let topicName = titleClean
    .replace(/^(what is|what are|how to|how do i)\s+/i, '')
    .replace(/\?$/, '')
    .trim();
  const topicLower = topicName.toLowerCase();

  // Convert gerund titles to base form for "How do I" questions
  // "Querying Data" → "query data", "Emitting Events" → "emit events"
  let actionForm = topicLower
    .replace(/^(querying|emitting|building|creating|configuring|installing|deploying|testing|debugging|connecting|running|monitoring|signing|sending|minting|transferring|upgrading|publishing|starting|stopping|verifying|integrating|optimizing)\b/i,
      (m) => {
        const map = {
          querying: 'query', emitting: 'emit', building: 'build', creating: 'create',
          configuring: 'configure', installing: 'install', deploying: 'deploy',
          testing: 'test', debugging: 'debug', connecting: 'connect', running: 'run',
          monitoring: 'monitor', signing: 'sign', sending: 'send', minting: 'mint',
          transferring: 'transfer', upgrading: 'upgrade', publishing: 'publish',
          starting: 'start', stopping: 'stop', verifying: 'verify',
          integrating: 'integrate', optimizing: 'optimize',
          updating: 'update', logging: 'log', submitting: 'submit', adding: 'add',
        };
        return map[m.toLowerCase()] || m;
      });

  // Generate primary questions from title based on archetype
  switch (archetype) {
    case 'onboarding':
      questions.push(`How do I ${actionForm} on Sui?`);
      break;
    case 'example':
      questions.push(`How do I build a ${topicLower} on Sui?`);
      questions.push(`What is the ${topicLower} pattern in Sui?`);
      break;
    case 'migration': {
      const platform = topicName.replace(/->|→/g, '').replace(/\s+to\s+Sui/i, '').trim();
      questions.push(`How does Sui compare to ${platform}?`);
      questions.push(`How do I migrate from ${platform} to Sui?`);
      break;
    }
    case 'operator':
      questions.push(`How do I ${actionForm} for Sui?`);
      break;
    case 'sdk':
      questions.push(`How do I use the ${topicName} SDK?`);
      break;
    case 'reference':
      questions.push(`What is ${topicName} in Sui?`);
      break;
    case 'index':
      questions.push(`What is ${topicName} in Sui?`);
      break;
    default: // guide
      // Detect conceptual vs procedural from title
      if (/^(build|create|set up|configure|install|deploy|write|test|debug|connect|query|emit|use|optimiz|integrat|run|monitor|sign|send)/i.test(actionForm)) {
        questions.push(`How do I ${actionForm} on Sui?`);
      } else {
        questions.push(`What is ${topicName} in Sui?`);
        questions.push(`How does ${topicLower} work on Sui?`);
      }
  }

  // Generate questions from H2 headings (up to 3)
  const h2s = headings.filter(h => h.level === 2).slice(0, 5);
  for (const h of h2s) {
    const text = h.text.replace(/[`{}\[\]]/g, '').trim();
    if (!text || text.length < 5) continue;
    if (isQuestion(text)) {
      questions.push(text.endsWith('?') ? text : text + '?');
      continue;
    }
    // Skip generic headings
    if (/^(overview|introduction|summary|prerequisites|setup|resources|related|see also|next steps)/i.test(text)) continue;
    // Convert procedural headings to questions
    const headingLower = text.toLowerCase()
      .replace(/^(querying|emitting|building|creating|configuring|installing|deploying|testing|debugging|connecting|running|monitoring|signing|sending|minting|transferring|upgrading|publishing|starting|stopping|verifying|integrating|optimizing|submitting|adding)\b/i,
        (m) => {
          const map = {
            querying: 'query', emitting: 'emit', building: 'build', creating: 'create',
            configuring: 'configure', installing: 'install', deploying: 'deploy',
            testing: 'test', debugging: 'debug', connecting: 'connect', running: 'run',
            monitoring: 'monitor', signing: 'sign', sending: 'send', minting: 'mint',
            transferring: 'transfer', upgrading: 'upgrade', publishing: 'publish',
            starting: 'start', stopping: 'stop', verifying: 'verify',
            integrating: 'integrate', optimizing: 'optimize', submitting: 'submit',
            adding: 'add', updating: 'update', logging: 'log',
          };
          return map[m.toLowerCase()] || m;
        });
    if (/^(install|create|build|configure|set up|deploy|run|test|use|add|enable|connect|query|submit|verify|sign|send|mint|transfer|upgrade|publish|start|stop|monitor|integrate|optimize|emit)/i.test(headingLower)) {
      questions.push(`How do I ${headingLower}?`);
    }
  }

  // Deduplicate and limit
  const seen = new Set();
  const unique = [];
  for (const q of questions) {
    const norm = q.toLowerCase().replace(/[?!.]/g, '').trim();
    if (seen.has(norm)) continue;
    seen.add(norm);
    unique.push(q);
    if (unique.length >= 5) break;
  }

  return unique;
}

// ─── Answer generation ──────────────────────────────────────────────────────

function generateAnswer(title, description, intro) {
  // Use description as the base — it's usually the best one-liner
  if (description && description.length >= 20) {
    let answer = description.replace(/\.$/, '').trim();
    // Cap at ~200 chars
    if (answer.length > 200) {
      const cutPoint = answer.lastIndexOf('. ', 200);
      if (cutPoint > 50) {
        answer = answer.slice(0, cutPoint + 1).trim();
      } else {
        const spacePoint = answer.lastIndexOf(' ', 200);
        answer = answer.slice(0, spacePoint > 50 ? spacePoint : 200).trim();
      }
    }
    // Make sure it ends with a period
    if (!/[.!]$/.test(answer)) answer += '.';
    return answer;
  }

  // Fall back to intro if no description
  if (intro && intro.length >= 20) {
    let answer = intro.split(/[.!]/)[0].trim();
    if (answer.length < 20) answer = intro.slice(0, 200).trim();
    if (!/[.!]$/.test(answer)) answer += '.';
    return answer;
  }

  return null;
}

// ─── Main ───────────────────────────────────────────────────────────────────

function main() {
  const files = globMdx(CONTENT_ROOT);
  let updated = 0;
  let skipped = 0;
  let alreadyHas = 0;

  for (const filePath of files) {
    const relPath = path.relative(CONTENT_ROOT, filePath);
    const raw = fs.readFileSync(filePath, 'utf8');
    const { data, content: body } = matter(raw);

    // Skip snippets and auto-generated pages
    if (relPath.startsWith('snippets/')) { skipped++; continue; }
    if (relPath.includes('sui-graphql/beta/reference/')) { skipped++; continue; }

    // Skip if already has both
    if (data.questions && data.answer) { alreadyHas++; continue; }

    // Skip pages without title
    if (!data.title) { skipped++; continue; }

    const headings = getHeadings(body);
    const intro = getIntro(body);
    let changed = false;

    if (!data.questions) {
      const questions = generateQuestions(data.title, data.description, headings, relPath);
      if (questions.length > 0) {
        data.questions = questions;
        changed = true;
      }
    }

    if (!data.answer) {
      const answer = generateAnswer(data.title, data.description, intro);
      if (answer) {
        data.answer = answer;
        changed = true;
      }
    }

    if (!changed) { skipped++; continue; }

    if (dryRun) {
      console.log(`${relPath}`);
      if (data.questions) console.log(`  Q: ${data.questions.slice(0, 3).join(' | ')}`);
      if (data.answer) console.log(`  A: ${data.answer.slice(0, 100)}${data.answer.length > 100 ? '...' : ''}`);
    } else {
      const newRaw = matter.stringify(body, data);
      fs.writeFileSync(filePath, newRaw, 'utf8');
    }

    updated++;
  }

  console.log(`\n${'─'.repeat(50)}`);
  console.log(`${dryRun ? 'DRY RUN' : 'APPLIED'}`);
  console.log(`  Pages updated:     ${updated}`);
  console.log(`  Already has both:  ${alreadyHas}`);
  console.log(`  Skipped:           ${skipped}`);

  if (dryRun && updated > 0) {
    console.log(`\nRun with --apply to write changes.`);
  }
}

main();
