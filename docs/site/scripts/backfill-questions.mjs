/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Backfills pages that have < 3 questions with better generated questions
 * derived from the page's title, description, and content topic.
 *
 * Strategy: instead of converting headings to questions (which produces junk),
 * generate questions from what a user would actually search for:
 *   - "What is X?" (conceptual)
 *   - "How do I X?" (procedural — only when title implies an action)
 *   - "What are the Y of X?" (structural — from description keywords)
 *   - "How does X work?" (mechanism)
 *   - "Why use X?" (justification)
 *
 * Usage:
 *   node scripts/backfill-questions.mjs          # dry run
 *   node scripts/backfill-questions.mjs --apply  # write changes
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import matter from 'gray-matter';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const CONTENT_ROOT = path.resolve(__dirname, '..', '..', 'content');
const dryRun = !process.argv.includes('--apply');
const MIN_QUESTIONS = 3;

function globMdx(dir) {
  const results = [];
  function walk(d) {
    for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
      const full = path.join(d, entry.name);
      if (entry.isDirectory()) {
        if (['node_modules', '.docusaurus', 'build', 'dist'].includes(entry.name)) continue;
        walk(full);
      } else if (entry.name.endsWith('.mdx')) results.push(full);
    }
  }
  walk(dir);
  return results;
}

function cleanTitle(t) {
  return t.replace(/[`]/g, '').replace(/^(what is|what are|how to)\s+/i, '').replace(/\?$/, '').trim();
}

function getArchetype(relPath) {
  if (relPath.startsWith('snippets/') || relPath.includes('sui-graphql/beta/reference/')) return 'skip';
  if (relPath.startsWith('getting-started/onboarding/')) return 'onboarding';
  if (relPath.startsWith('getting-started/examples/')) return 'example';
  if (relPath.startsWith('getting-started/sui-for-')) return 'migration';
  if (relPath.startsWith('operators/')) return 'operator';
  if (relPath.includes('-sdk/') || relPath.includes('-sdk.mdx')) return 'sdk';
  if (relPath.startsWith('references/cli/')) return 'cli';
  if (relPath.startsWith('references/')) return 'reference';
  if (relPath.endsWith('/index.mdx') || !relPath.includes('/')) return 'index';
  return 'guide';
}

function generateQuestions(title, description, archetype, existing) {
  const topic = cleanTitle(title);
  const topicLower = topic.toLowerCase();
  const desc = (description || '').toLowerCase();

  const candidates = [];
  const seen = new Set(existing.map(q => q.toLowerCase().replace(/[?!.]/g, '').trim()));

  function add(q) {
    if (!q) return;
    if (!q.endsWith('?')) q += '?';
    const norm = q.toLowerCase().replace(/[?!.]/g, '').trim();
    if (norm.length < 10 || seen.has(norm)) return;
    seen.add(norm);
    candidates.push(q);
  }

  // Archetype-specific questions
  switch (archetype) {
    case 'onboarding':
      add(`What do I need before I can ${topicLower}?`);
      add(`How do I verify that ${topicLower} worked?`);
      break;
    case 'example':
      add(`What does the ${topicLower} example demonstrate?`);
      add(`What are the prerequisites for the ${topicLower} example?`);
      break;
    case 'migration':
      add(`What are the key differences between Sui and other blockchains?`);
      break;
    case 'operator':
      add(`How do I set up ${topicLower}?`);
      add(`What are the requirements for ${topicLower}?`);
      break;
    case 'sdk':
      add(`What methods does the ${topic} provide?`);
      add(`How do I install the ${topic}?`);
      break;
    case 'cli':
      add(`What are the available ${topicLower} commands?`);
      add(`What flags does ${topicLower} support?`);
      break;
    case 'index':
      add(`What topics does ${topic} cover?`);
      break;
    case 'reference':
      add(`Where can I find the ${topicLower} reference?`);
      break;
    default: // guide
      break;
  }

  // "How does X work?" — only for noun-phrase topics (not action phrases)
  const isActionPhrase = /^(build|create|set up|configure|install|deploy|run|test|use|add|enable|connect|query|submit|verify|sign|send|mint|transfer|upgrade|publish|start|stop|monitor|integrate|optimize|emit|write|debug|check|migrate)/i.test(topicLower);
  if (!isActionPhrase) {
    add(`How does ${topicLower} work on Sui?`);
  }

  // Description-derived questions — use the topic noun, not the full action title
  // For action-phrase titles, extract the noun part: "Build a Custom Indexer" → "custom indexer"
  const topicNoun = isActionPhrase
    ? topicLower.replace(/^(build|create|set up|configure|install|deploy|run|test|use|add|enable|connect|query|submit|verify|sign|send|mint|transfer|upgrade|publish|start|stop|monitor|integrate|optimize|emit|write|debug|check|migrate)\s+(a |the |an |your )?/i, '')
    : topicLower;

  if (desc.includes('configur') && !isActionPhrase) add(`How do I configure ${topicNoun}?`);
  if (desc.includes('deploy') && !topicLower.includes('deploy')) add(`How do I deploy ${topicNoun}?`);
  if (desc.includes('monitor')) add(`How do I monitor ${topicNoun}?`);
  if (desc.includes('debug') || desc.includes('troubleshoot')) add(`How do I troubleshoot ${topicNoun}?`);
  if (desc.includes('security') || desc.includes('secure')) add(`What are the security considerations for ${topicNoun}?`);
  if (desc.includes('performance') || desc.includes('optimiz')) add(`How do I optimize ${topicNoun} performance?`);
  if (desc.includes('migrate') || desc.includes('migration')) add(`How do I migrate to ${topicNoun}?`);
  if (desc.includes('example') || desc.includes('tutorial')) add(`Where can I find ${topicNoun} examples?`);
  if (desc.includes('api') || desc.includes('endpoint')) add(`What API does ${topicNoun} provide?`);
  if (desc.includes('event')) add(`How do I subscribe to ${topicNoun} events?`);
  if (desc.includes('upgrade') && !topicLower.includes('upgrade')) add(`How do I upgrade ${topicNoun}?`);
  if (desc.includes('test') && !topicLower.includes('test')) add(`How do I test ${topicNoun}?`);

  // Fallback: "Why use X?"
  if (candidates.length < 2) {
    add(`Why should I use ${topicLower}?`);
  }

  return candidates;
}

function main() {
  const files = globMdx(CONTENT_ROOT);
  let backfilled = 0;
  let questionsAdded = 0;
  let skipped = 0;

  for (const filePath of files) {
    const relPath = path.relative(CONTENT_ROOT, filePath);
    const archetype = getArchetype(relPath);
    if (archetype === 'skip') { skipped++; continue; }

    const raw = fs.readFileSync(filePath, 'utf8');
    const { data, content } = matter(raw);

    if (!data.questions || !Array.isArray(data.questions)) { skipped++; continue; }
    if (data.questions.length >= MIN_QUESTIONS) { skipped++; continue; }
    if (!data.title) { skipped++; continue; }

    const needed = MIN_QUESTIONS - data.questions.length;
    const newQs = generateQuestions(data.title, data.description, archetype, data.questions);

    if (newQs.length === 0) { skipped++; continue; }

    const toAdd = newQs.slice(0, needed);
    data.questions = [...data.questions, ...toAdd];
    questionsAdded += toAdd.length;
    backfilled++;

    if (dryRun) {
      console.log(`${relPath} [${data.questions.length - toAdd.length} → ${data.questions.length}]`);
      for (const q of toAdd) console.log(`  + ${q}`);
    } else {
      fs.writeFileSync(filePath, matter.stringify(content, data), 'utf8');
    }
  }

  console.log(`\n${'─'.repeat(50)}`);
  console.log(`${dryRun ? 'DRY RUN' : 'APPLIED'}`);
  console.log(`  Pages backfilled:  ${backfilled}`);
  console.log(`  Questions added:   ${questionsAdded}`);
  console.log(`  Skipped:           ${skipped}`);

  if (dryRun) console.log(`\nRun with --apply to write changes.`);
}

main();
