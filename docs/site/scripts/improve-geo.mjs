/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Improves GEO/AEO quality across all docs pages:
 *
 *   1. Rewrites questions to be more natural and search-aligned
 *   2. Rewrites answers to be concise direct statements (not description repeats)
 *   3. Ensures the first paragraph after frontmatter is a direct answer
 *   4. Converts H2 headings to question format where safe
 *
 * Usage:
 *   node scripts/improve-geo.mjs                # dry run
 *   node scripts/improve-geo.mjs --apply        # write changes
 *   node scripts/improve-geo.mjs --apply --skip-headings  # skip heading conversion
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import matter from 'gray-matter';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const CONTENT_ROOT = path.resolve(__dirname, '..', '..', 'content');
const dryRun = !process.argv.includes('--apply');
const skipHeadings = process.argv.includes('--skip-headings');

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

function getArchetype(relPath) {
  if (relPath.startsWith('snippets/') || relPath.includes('sui-graphql/beta/reference/')) return 'skip';
  if (relPath.startsWith('getting-started/onboarding/')) return 'onboarding';
  if (relPath.startsWith('getting-started/examples/')) return 'example';
  if (relPath.startsWith('getting-started/sui-for-')) return 'migration';
  if (relPath.startsWith('operators/')) return 'operator';
  if (relPath.includes('-sdk/') || relPath.includes('-sdk.mdx')) return 'sdk';
  if (relPath.startsWith('references/')) return 'reference';
  if (relPath.endsWith('/index.mdx') || !relPath.includes('/')) return 'index';
  return 'guide';
}

function getHeadings(body) {
  const headings = [];
  for (const line of body.split('\n')) {
    const m = line.match(/^(#{2,3})\s+(.*)$/);
    if (m) headings.push({ level: m[1].length, text: m[2].trim(), raw: line });
  }
  return headings;
}

function getFirstParagraph(body) {
  // Get the first non-empty, non-JSX, non-heading prose paragraph
  const lines = body.split('\n');
  let para = '';
  let started = false;
  let inJsx = false;
  for (const line of lines) {
    const trimmed = line.trim();
    // Track multi-line JSX
    if (trimmed.startsWith('<') && !trimmed.startsWith('</') && !trimmed.endsWith('/>') && !trimmed.endsWith('>')) {
      inJsx = true; continue;
    }
    if (inJsx) {
      if (trimmed.endsWith('/>') || trimmed.endsWith('>')) inJsx = false;
      continue;
    }
    if (!started) {
      if (!trimmed) continue;
      if (trimmed.startsWith('<') || trimmed.startsWith('import ') || trimmed.startsWith(':::')) continue;
      if (trimmed.startsWith('#')) continue;
      // Skip lines that look like JSX attributes
      if (/^\w+=/.test(trimmed)) continue;
      started = true;
    }
    if (started) {
      if (!trimmed && para) break;
      if (trimmed.startsWith('#') || trimmed.startsWith('<') || trimmed.startsWith(':::') || trimmed.startsWith('```')) break;
      para += (para ? ' ' : '') + trimmed;
    }
  }
  return para;
}

// ─── 1. Improve questions ───────────────────────────────────────────────────

const GERUND_MAP = {
  querying: 'query', emitting: 'emit', building: 'build', creating: 'create',
  configuring: 'configure', installing: 'install', deploying: 'deploy',
  testing: 'test', debugging: 'debug', connecting: 'connect', running: 'run',
  monitoring: 'monitor', signing: 'sign', sending: 'send', minting: 'mint',
  transferring: 'transfer', upgrading: 'upgrade', publishing: 'publish',
  starting: 'start', stopping: 'stop', verifying: 'verify',
  integrating: 'integrate', optimizing: 'optimize', updating: 'update',
  submitting: 'submit', adding: 'add', logging: 'log',
};

function deGerund(text) {
  return text.replace(/^(\w+ing)\b/i, (m) => GERUND_MAP[m.toLowerCase()] || m);
}

function improveQuestions(questions, title, description, headings, archetype) {
  if (!questions || !Array.isArray(questions)) return null;

  const titleClean = title.replace(/[`]/g, '').replace(/^(what is|what are|how to)\s+/i, '').replace(/\?$/, '').trim();
  const topicName = titleClean;
  const topicAction = deGerund(topicName.toLowerCase());

  const newQuestions = [];
  const seen = new Set();

  function addQ(q) {
    if (!q) return;
    // Clean up
    q = q.replace(/SDK SDK/g, 'SDK')
         .replace(/pattern pattern/g, 'pattern')
         .replace(/in Sui in Sui/g, 'in Sui')
         .replace(/on Sui on Sui/g, 'on Sui')
         .replace(/install sui\b(?! CLI)/gi, 'install the Sui CLI')
         .replace(/\s{2,}/g, ' ')
         .trim();
    if (!q.endsWith('?')) q += '?';
    const norm = q.toLowerCase().replace(/[?!.]/g, '').trim();
    if (seen.has(norm) || norm.length < 10) return;
    seen.add(norm);
    newQuestions.push(q);
  }

  // Primary question based on archetype
  switch (archetype) {
    case 'onboarding':
      addQ(`How do I ${topicAction}?`);
      break;
    case 'example':
      addQ(`How do I build a ${topicAction.replace(/\s+pattern$/i, '')} on Sui?`);
      break;
    case 'migration': {
      const platform = topicName.replace(/->|→/g, ' ').replace(/\s+(to\s+)?Sui$/i, '').trim();
      addQ(`How is Sui different from ${platform}?`);
      addQ(`How do I migrate from ${platform} to Sui?`);
      break;
    }
    case 'operator':
      addQ(`How do I ${topicAction}?`);
      break;
    case 'sdk': {
      const sdkName = topicName.replace(/\s+SDK$/i, '');
      addQ(`How do I use the ${sdkName} SDK?`);
      break;
    }
    default:
      if (/^(build|create|set up|configure|install|deploy|write|test|debug|connect|query|emit|use|run|monitor|sign|send|mint|transfer|upgrade|publish)/i.test(topicAction)) {
        addQ(`How do I ${topicAction}?`);
      } else {
        addQ(`What is ${topicName} in Sui?`);
      }
  }

  // Add questions from H2 headings (convert to question form)
  for (const h of headings.filter(h => h.level === 2).slice(0, 4)) {
    const text = h.text.replace(/[`{}\[\]#]/g, '').replace(/\s*\{.*\}$/, '').trim();
    if (!text || text.length < 4) continue;
    if (/^(overview|introduction|summary|prerequisites|setup|resources|related|see also|next steps|key\s)/i.test(text)) continue;

    // Already a question
    if (/^(what|how|why|when|where|can|do|is|are|should|which)\b/i.test(text)) {
      addQ(text.endsWith('?') ? text : text + '?');
      continue;
    }

    // Procedural heading → "How do I X?"
    const actionText = deGerund(text.toLowerCase());
    if (/^(build|create|set up|configure|install|deploy|run|test|use|add|enable|connect|query|submit|verify|sign|send|mint|transfer|upgrade|publish|start|stop|monitor|integrate|optimize|emit)/i.test(actionText)) {
      addQ(`How do I ${actionText}?`);
    }
  }

  // Add a "What is X" if we only have "How" questions and archetype isn't procedural
  if (newQuestions.length > 0 && newQuestions.every(q => q.startsWith('How')) && !['onboarding', 'operator'].includes(archetype)) {
    addQ(`What is ${topicName} in Sui?`);
  }

  // Limit to 5
  return newQuestions.slice(0, 5);
}

// ─── 2. Improve answers ────────────────────────────────────────────────────

function improveAnswer(answer, title, description, firstParagraph) {
  // The answer should be a direct, concise statement — not a repeat of the description
  // Prefer the first paragraph of actual content if it's good
  let best = answer || '';

  // If the first paragraph is a good direct statement, use it
  if (firstParagraph && firstParagraph.length >= 30 && firstParagraph.length <= 250) {
    // Check it's not a preamble
    if (!/^(in this|this guide|this page|this document|this section|learn how|learn about|this topic)/i.test(firstParagraph)) {
      best = firstParagraph;
    }
  }

  // Fall back to description if answer is weak
  if ((!best || best.length < 20) && description) {
    best = description;
  }

  if (!best || best.length < 10) return null;

  // Clean up
  best = best
    .replace(/\.$/, '').trim()
    .replace(/\s{2,}/g, ' ');

  // Truncate to ~200 chars at sentence boundary
  if (best.length > 220) {
    const sentences = best.split(/(?<=[.!])\s+/);
    let trimmed = '';
    for (const s of sentences) {
      if ((trimmed + ' ' + s).trim().length > 200 && trimmed.length > 30) break;
      trimmed = (trimmed + ' ' + s).trim();
    }
    best = trimmed;
  }

  // Ensure it ends with a period
  if (!/[.!]$/.test(best)) best += '.';

  return best;
}

// ─── 3. Fix first paragraph ────────────────────────────────────────────────

const PREAMBLE_RE = /^(in this (guide|page|section|document|topic)|this (guide|page|section|document) (shows|explains|covers|describes|walks|provides|demonstrates)|learn (how|about|to)|you (will|can|should) learn|the following|below is)/i;

function fixFirstParagraph(body, answer) {
  if (!answer) return body;

  const lines = body.split('\n');
  let firstContentIdx = -1;
  let firstParaEndIdx = -1;

  // Find the first content line (skip JSX, imports, empty lines)
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line) continue;
    if (line.startsWith('<') || line.startsWith('import ') || line.startsWith(':::') || line.startsWith('#') || line.startsWith('```')) continue;
    firstContentIdx = i;
    break;
  }

  if (firstContentIdx === -1) return body;

  // Check if the first paragraph is a preamble
  const firstLine = lines[firstContentIdx].trim();
  if (!PREAMBLE_RE.test(firstLine)) return body; // Already starts with a direct answer

  // Find end of the preamble paragraph
  for (let i = firstContentIdx + 1; i < lines.length; i++) {
    if (!lines[i].trim()) { firstParaEndIdx = i; break; }
    if (lines[i].startsWith('#') || lines[i].startsWith('<') || lines[i].startsWith('```')) { firstParaEndIdx = i; break; }
  }
  if (firstParaEndIdx === -1) firstParaEndIdx = firstContentIdx + 1;

  // Prepend the answer before the preamble
  const answerClean = answer.replace(/\.$/, '').trim() + '.';
  const newLines = [
    ...lines.slice(0, firstContentIdx),
    answerClean,
    '',
    ...lines.slice(firstContentIdx),
  ];

  return newLines.join('\n');
}

// ─── 4. Convert headings to questions ──────────────────────────────────────

function convertHeadingsToQuestions(body) {
  const lines = body.split('\n');
  let changed = false;

  for (let i = 0; i < lines.length; i++) {
    const match = lines[i].match(/^(##)\s+(.+?)(\s*\{#[\w-]+\})?\s*$/);
    if (!match) continue;

    const prefix = match[1];
    let text = match[2].trim();
    const anchor = match[3] || '';

    // Skip if already a question
    if (/^(what|how|why|when|where|can|do|is|are|should|which)\b/i.test(text)) continue;
    // Skip generic headings that don't convert well
    if (/^(overview|introduction|summary|prerequisites|setup|resources|related|see also|next steps|key |more |additional|troubleshoot|common)/i.test(text)) continue;
    // Skip very short headings
    if (text.length < 5) continue;
    // Skip headings with special formatting
    if (/[`\[\]{}|]/.test(text)) continue;

    // Convert to question format
    const actionText = deGerund(text.toLowerCase());

    let newText;
    if (/^(build|create|set up|configure|install|deploy|run|test|use|add|enable|connect|query|submit|verify|sign|send|mint|transfer|upgrade|publish|start|stop|monitor|integrate|optimize|emit)/i.test(actionText)) {
      newText = `How do I ${actionText}?`;
    } else {
      newText = `What is ${text.toLowerCase()}?`;
      // Capitalize first letter after "What is "
      newText = newText.replace(/what is (.)/i, (_, c) => `What is ${c.toUpperCase()}`);
    }

    // Preserve custom anchor ID
    lines[i] = `${prefix} ${newText}${anchor}`;
    changed = true;
  }

  return changed ? lines.join('\n') : body;
}

// ─── Main ───────────────────────────────────────────────────────────────────

function main() {
  const files = globMdx(CONTENT_ROOT);
  let questionsImproved = 0;
  let answersImproved = 0;
  let introsFixed = 0;
  let headingsConverted = 0;
  let skipped = 0;

  for (const filePath of files) {
    const relPath = path.relative(CONTENT_ROOT, filePath);
    const archetype = getArchetype(relPath);
    if (archetype === 'skip') { skipped++; continue; }

    const raw = fs.readFileSync(filePath, 'utf8');
    const { data, content: body } = matter(raw);

    if (!data.title) { skipped++; continue; }

    let newBody = body;
    let changed = false;

    const headings = getHeadings(body);
    const firstPara = getFirstParagraph(body);

    // 1. Improve questions
    if (data.questions && Array.isArray(data.questions)) {
      const improved = improveQuestions(data.questions, data.title, data.description, headings, archetype);
      if (improved && JSON.stringify(improved) !== JSON.stringify(data.questions)) {
        data.questions = improved;
        questionsImproved++;
        changed = true;
      }
    }

    // 2. Improve answer
    if (data.answer) {
      const improved = improveAnswer(data.answer, data.title, data.description, firstPara);
      if (improved && improved !== data.answer) {
        data.answer = improved;
        answersImproved++;
        changed = true;
      }
    }

    // 3. Fix first paragraph (add direct answer if starts with preamble)
    const fixedBody = fixFirstParagraph(newBody, data.answer);
    if (fixedBody !== newBody) {
      newBody = fixedBody;
      introsFixed++;
      changed = true;
    }

    // 4. Convert headings to questions
    if (!skipHeadings) {
      const convertedBody = convertHeadingsToQuestions(newBody);
      if (convertedBody !== newBody) {
        newBody = convertedBody;
        headingsConverted++;
        changed = true;
      }
    }

    if (!changed) continue;

    if (dryRun) {
      const changes = [];
      if (questionsImproved > 0 && data.questions) changes.push(`Q: ${data.questions[0]}`);
      if (answersImproved > 0 && data.answer) changes.push(`A: ${data.answer.slice(0, 60)}...`);
      if (introsFixed > 0) changes.push('intro fixed');
      if (headingsConverted > 0) changes.push('headings converted');
      // Only print if something changed for this specific file
      if (changes.length > 0) {
        console.log(`${relPath}: ${changes.join(' | ')}`);
      }
    } else {
      const newRaw = matter.stringify(newBody, data);
      fs.writeFileSync(filePath, newRaw, 'utf8');
    }
  }

  console.log(`\n${'─'.repeat(50)}`);
  console.log(`${dryRun ? 'DRY RUN' : 'APPLIED'}`);
  console.log(`  Questions improved:    ${questionsImproved}`);
  console.log(`  Answers improved:      ${answersImproved}`);
  console.log(`  Intros fixed:          ${introsFixed}`);
  console.log(`  Headings converted:    ${headingsConverted}`);
  console.log(`  Skipped:               ${skipped}`);

  if (dryRun) {
    console.log(`\nRun with --apply to write changes.`);
    console.log(`Use --skip-headings to skip heading conversion.`);
  }
}

main();
