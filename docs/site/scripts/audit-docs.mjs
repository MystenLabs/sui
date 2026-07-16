/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Deterministic docs audit pipeline.
 *
 * Three layers:
 *   1. Base checks   – frontmatter, staleness, links, images, code fences, TODOs, word count, duplicates
 *   2. Goal checklist – evaluates goal.requires from page frontmatter
 *   3. Concept map    – cross-references pages against concept-map.yaml
 *
 * Usage:
 *   node scripts/audit-docs.mjs                  # JSON to stdout
 *   node scripts/audit-docs.mjs --summary        # compact table to stderr, JSON to stdout
 *   node scripts/audit-docs.mjs --only-failures  # only pages with issues
 */

import fs from 'fs';
import path from 'path';
import { execFileSync } from 'child_process';
import { fileURLToPath } from 'url';
import matter from 'gray-matter';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const SITE_ROOT = path.resolve(__dirname, '..');
const CONTENT_ROOT = path.resolve(SITE_ROOT, '..', 'content');
const REPO_ROOT = path.resolve(SITE_ROOT, '..', '..');
const CONCEPT_MAP_PATH = path.resolve(SITE_ROOT, '..', 'concept-map.yaml');

// ─── Helpers ────────────────────────────────────────────────────────────────

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

function relativeTo(filePath, root) {
  return path.relative(root, filePath);
}

function stripCodeBlocks(text) {
  return text.replace(/```[\s\S]*?```/g, '').replace(/`[^`\n]+`/g, '');
}

function stripFrontmatter(raw) {
  return raw.replace(/^---[\s\S]*?---\n?/, '');
}

function countWords(text) {
  const cleaned = stripCodeBlocks(stripFrontmatter(text));
  const words = cleaned.match(/[a-zA-Z0-9]+/g);
  return words ? words.length : 0;
}

function getHeadings(body) {
  const headings = [];
  for (const line of body.split('\n')) {
    const m = line.match(/^(#{1,6})\s+(.*)$/);
    if (m) {
      headings.push({ level: m[1].length, text: m[2].trim() });
    }
  }
  return headings;
}

function getGitLastModified(filePath) {
  try {
    const ts = execFileSync(
      'git', ['log', '-1', '--format=%at', '--', filePath],
      { cwd: REPO_ROOT, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }
    ).trim();
    if (!ts) return null;
    return new Date(parseInt(ts, 10) * 1000);
  } catch {
    return null;
  }
}

function daysSince(date) {
  if (!date) return null;
  return Math.floor((Date.now() - date.getTime()) / (1000 * 60 * 60 * 24));
}

// Build a set of all valid internal doc paths (without extensions)
function buildDocPathSet(contentRoot) {
  const paths = new Set();
  // Include both .mdx and .md files (framework references use .md)
  const files = [];
  function walkAll(d) {
    for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
      const full = path.join(d, entry.name);
      if (entry.isDirectory()) {
        if (['node_modules', '.docusaurus', 'build', 'dist'].includes(entry.name)) continue;
        walkAll(full);
      } else if (entry.name.endsWith('.mdx') || entry.name.endsWith('.md')) {
        files.push(full);
      }
    }
  }
  walkAll(contentRoot);
  for (const f of files) {
    let rel = relativeTo(f, contentRoot);
    // Remove .mdx/.md extension
    rel = rel.replace(/\.(mdx|md)$/, '');
    // Remove /index suffix
    rel = rel.replace(/\/index$/, '');
    // Add with leading slash
    paths.add('/' + rel);
  }
  return paths;
}

// ─── Layer 1: Base Checks ───────────────────────────────────────────────────

function checkFrontmatter(data) {
  const required = ['title', 'description', 'keywords'];
  const missing = required.filter(f => !data[f]);
  return {
    pass: missing.length === 0,
    missing,
  };
}

function checkBrokenInternalLinks(body, docPaths, filePath) {
  const broken = [];
  // Match markdown links [text](/path) and [text](/path#anchor)
  const linkRe = /\[([^\]]*)\]\((\/?[^)#\s]+)(#[^)]*)?\)/g;
  let m;
  while ((m = linkRe.exec(body)) !== null) {
    const target = m[2];
    // Skip external URLs, anchors-only, relative file refs, images, mailto
    if (target.startsWith('http://') || target.startsWith('https://')) continue;
    if (target.startsWith('#')) continue;
    if (target.startsWith('mailto:')) continue;
    if (/\.\w+$/.test(target) && !target.endsWith('.mdx') && !target.endsWith('.md')) continue;

    // Normalize the target path
    let normalized = target;
    // Handle relative paths
    if (!normalized.startsWith('/')) {
      const dir = '/' + relativeTo(path.dirname(filePath), CONTENT_ROOT);
      normalized = path.posix.join(dir, normalized);
    }
    // Remove .mdx/.md extension
    normalized = normalized.replace(/\.(mdx|md)$/, '');
    // Remove trailing /index
    normalized = normalized.replace(/\/index$/, '');

    if (!docPaths.has(normalized)) {
      broken.push({ text: m[1], target: m[2] });
    }
  }
  return broken;
}

function checkBrokenImports(body, filePath) {
  const broken = [];
  // Strip code blocks so we don't match example imports in documentation
  const bodyNoCode = stripCodeBlocks(body);
  // Match the full tag to check for remote attributes (org, repo)
  const importRe = /<ImportContent\s([^>]+)>/g;
  let m;
  while ((m = importRe.exec(bodyNoCode)) !== null) {
    const attrs = m[1];
    // Skip remote imports (fetched from external repos at build time)
    if (/\borg=/.test(attrs) || /\brepo=/.test(attrs)) continue;

    const sourceMatch = attrs.match(/source="([^"]+)"/);
    if (!sourceMatch) continue;
    const source = sourceMatch[1];

    // Skip snippet-mode imports (short names without paths)
    if (!source.includes('/') && !source.includes('.')) continue;

    // Try resolving from repo root and content root
    const candidates = [
      path.resolve(REPO_ROOT, source),
      path.resolve(REPO_ROOT, source.replace(/^\//, '')),
      path.resolve(CONTENT_ROOT, source),
      path.resolve(CONTENT_ROOT, source.replace(/^\//, '')),
      path.resolve(path.dirname(filePath), source),
    ];
    const exists = candidates.some(c => fs.existsSync(c));
    if (!exists) {
      broken.push(source);
    }
  }
  return broken;
}

function checkCodeFences(body) {
  // Count opening and closing triple-backtick fences
  const fences = body.match(/^```/gm) || [];
  return {
    pass: fences.length % 2 === 0,
    count: fences.length,
  };
}

function checkTodos(body) {
  const matches = [];
  const lines = body.split('\n');
  for (let i = 0; i < lines.length; i++) {
    if (/\b(TODO|FIXME|HACK|PLACEHOLDER|XXX)\b/i.test(lines[i])) {
      matches.push({ line: i + 1, text: lines[i].trim() });
    }
  }
  return matches;
}

function checkMissingImages(body, filePath) {
  const missing = [];
  // Markdown images: ![alt](path)
  const mdImgRe = /!\[[^\]]*\]\(([^)]+)\)/g;
  // HTML images: <img src="path"
  const htmlImgRe = /<img\s[^>]*src=["']([^"']+)["']/g;

  const checkPath = (imgPath) => {
    if (imgPath.startsWith('http://') || imgPath.startsWith('https://')) return;
    const resolved = imgPath.startsWith('/')
      ? path.resolve(CONTENT_ROOT, imgPath.slice(1))
      : path.resolve(path.dirname(filePath), imgPath);
    if (!fs.existsSync(resolved)) {
      missing.push(imgPath);
    }
  };

  let m;
  while ((m = mdImgRe.exec(body)) !== null) checkPath(m[1]);
  while ((m = htmlImgRe.exec(body)) !== null) checkPath(m[1]);

  return missing;
}

function runBaseChecks(filePath, raw, data, body, docPaths) {
  const lastModified = getGitLastModified(filePath);
  const staleDays = daysSince(lastModified);
  const wordCount = countWords(raw);
  const frontmatter = checkFrontmatter(data);
  const brokenLinks = checkBrokenInternalLinks(body, docPaths, filePath);
  const brokenImports = checkBrokenImports(body, filePath);
  const codeFences = checkCodeFences(body);
  const todos = checkTodos(body);
  const missingImages = checkMissingImages(body, filePath);

  const issues = [];
  if (!frontmatter.pass) issues.push(`Missing frontmatter: ${frontmatter.missing.join(', ')}`);
  if (brokenLinks.length > 0) issues.push(`${brokenLinks.length} broken internal link(s)`);
  if (brokenImports.length > 0) issues.push(`${brokenImports.length} broken import(s)`);
  if (!codeFences.pass) issues.push(`Unclosed code fence (${codeFences.count} backtick lines)`);
  if (todos.length > 0) issues.push(`${todos.length} TODO/FIXME marker(s)`);
  if (missingImages.length > 0) issues.push(`${missingImages.length} missing image(s)`);
  if (wordCount < 100) issues.push(`Very short page (${wordCount} words)`);

  // GEO/AEO readiness
  const hasQuestions = Array.isArray(data.questions) && data.questions.length > 0;
  const hasAnswer = typeof data.answer === 'string' && data.answer.trim().length > 0;

  return {
    frontmatter,
    lastModified: lastModified ? lastModified.toISOString().slice(0, 10) : null,
    staleDays,
    wordCount,
    brokenLinks,
    brokenImports,
    codeFences,
    todos,
    missingImages,
    issues,
    geo: { hasQuestions, questionCount: hasQuestions ? data.questions.length : 0, hasAnswer },
  };
}

function findDuplicateTitles(allPages) {
  const titleMap = new Map();
  for (const page of allPages) {
    const title = page.data?.title;
    if (!title) continue;
    if (!titleMap.has(title)) titleMap.set(title, []);
    titleMap.get(title).push(page.relativePath);
  }
  const duplicates = {};
  for (const [title, files] of titleMap) {
    if (files.length > 1) {
      duplicates[title] = files;
    }
  }
  return duplicates;
}

// ─── Layer 2: Goal Checklist ────────────────────────────────────────────────

function evaluateGoalRequires(goal, body, data, headings) {
  if (!goal || !goal.requires) return null;

  const results = [];

  for (const req of goal.requires) {
    const result = { label: req.label || '(unlabeled)', pass: false };

    if (req.pattern !== undefined && req.min !== undefined) {
      // Count regex pattern occurrences in body
      const re = new RegExp(req.pattern, 'gi');
      const matches = body.match(re) || [];
      result.pass = matches.length >= req.min;
      result.detail = `found ${matches.length}, need >= ${req.min}`;
    } else if (req.headings) {
      // Check that headings matching each pattern exist
      const missing = [];
      for (const h of req.headings) {
        const hPattern = h.pattern || h;
        const re = new RegExp(hPattern, 'i');
        const found = headings.some(hd => re.test(hd.text));
        if (!found) missing.push(hPattern);
      }
      result.pass = missing.length === 0;
      result.detail = missing.length > 0 ? `missing headings: ${missing.join(', ')}` : 'all present';
    } else if (req.links_to) {
      // Check that body links to specific paths, via either a markdown link
      // [text](/path) or a JSX component attribute href="/path" (e.g. <Card href=...>).
      const missing = [];
      for (const target of req.links_to) {
        // Escape special regex chars in path
        const escaped = target.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const mdLink = new RegExp(`\\]\\(${escaped}`);
        const hrefAttr = new RegExp(`href=["']${escaped}(["'#/])`);
        if (!mdLink.test(body) && !hrefAttr.test(body)) missing.push(target);
      }
      result.pass = missing.length === 0;
      result.detail = missing.length > 0 ? `missing links to: ${missing.join(', ')}` : 'all present';
    } else if (req.has_tables !== undefined) {
      // Count markdown tables (lines starting with |)
      const tableRows = (body.match(/^\|.+\|$/gm) || []).length;
      // A table needs at least a header row + separator + one data row = 3 rows
      const tableCount = Math.floor(tableRows / 3);
      const min = typeof req.min === 'number' ? req.min : 1;
      result.pass = tableCount >= min;
      result.detail = `~${tableCount} table(s), need >= ${min}`;
    } else if (req.has_images !== undefined) {
      const hasImg = /!\[[^\]]*\]\([^)]+\)/.test(body) || /<img\s/.test(body);
      result.pass = req.has_images ? hasImg : !hasImg;
      result.detail = hasImg ? 'has images' : 'no images';
    } else if (req.has_frontmatter) {
      const missing = req.has_frontmatter.filter(f => !data[f]);
      result.pass = missing.length === 0;
      result.detail = missing.length > 0 ? `missing: ${missing.join(', ')}` : 'all present';
    } else if (req.min_words !== undefined) {
      const wc = countWords(body);
      result.pass = wc >= req.min_words;
      result.detail = `${wc} words, need >= ${req.min_words}`;
    } else if (req.has_questions !== undefined) {
      const has = Array.isArray(data.questions) && data.questions.length > 0;
      // Also check for junk questions
      const junkPatterns = [
        /How do I (?!implement|supplement|augment|comment|document)[a-z]+(tion|ment|ness|ity|ing|cap|ticket) /i,   // noun after "How do I"
        /How do I [a-z]+ (and [a-z]+ing|[a-z]+ing)\?$/i,          // "How do I test and debugging?"
        /How do I [a-z]+ [a-z]+(ture|ure|icy|ity)\?$/i,           // "How do I user signature?"
        /How do I [a-z]+ and [a-z]+\?$/i,                          // "How do I create and share?" (no object)
        /\b(\w{4,}) \1\b/i,                                        // repeated word ("reference reference")
        /How do I (the|a|an|sui|set up sui|operator|data) /i,     // bad starts
        /verify that .* worked\?/i,                                 // template leak
        /need before I can .*\?/i,                                  // template leak
        /overview [a-z]+-[a-z]+\?/i,                                // anchor fragment leak
        /How do I [a-z]+ [a-z]+ [a-z]+-[a-z]+\?$/i,               // "How do I upgradecap upgradecap?"
        /How do I build a [a-z]+ on Sui\?$/i,                      // "How do I build a plinko on Sui?" (single word)
        /How do I build a [a-z]+ (testing|apps) on Sui/i,         // "How do I build a scenario testing on Sui?"
        /How do I add your /i,                                      // "How do I add your sdk?"
        /^What you learn\??$/i,                                     // heading fragment "What you learn?"
        /^What is [A-Z][a-z]+ a [A-Z]/i,                          // "What is Build a Custom Indexer in Sui?"
      ];
      let junkCount = 0;
      if (has) {
        for (const q of data.questions) {
          if (junkPatterns.some(p => p.test(q))) junkCount++;
        }
      }
      result.pass = req.has_questions ? (has && junkCount === 0) : !has;
      result.detail = !has ? 'no questions field' : junkCount > 0 ? `${junkCount}/${data.questions.length} question(s) look malformed` : `${data.questions.length} question(s)`;
    } else if (req.has_answer !== undefined) {
      const has = typeof data.answer === 'string' && data.answer.trim().length > 0;
      result.pass = req.has_answer ? has : !has;
      result.detail = has ? `${data.answer.trim().length} chars` : 'no answer field';
    } else if (req.answer_in_intro !== undefined) {
      // Check that the first N words before the first heading contain a direct statement
      const introMatch = body.match(/^([\s\S]*?)(?=^##?\s)/m);
      const intro = introMatch ? introMatch[1] : body.slice(0, 1000);
      const introWords = (intro.match(/[a-zA-Z0-9]+/g) || []).length;
      const minWords = typeof req.answer_in_intro === 'number' ? req.answer_in_intro : 30;
      result.pass = introWords >= minWords;
      result.detail = `${introWords} words before first heading, need >= ${minWords}`;
    } else if (req.question_headings !== undefined) {
      // Check if headings use question format (What/How/Why/When/Where/Can/Do/Is)
      const questionRe = /^(what|how|why|when|where|can|do|is|are|should|which)\b/i;
      const qHeadings = headings.filter(h => questionRe.test(h.text));
      const min = typeof req.question_headings === 'number' ? req.question_headings : 1;
      result.pass = qHeadings.length >= min;
      result.detail = `${qHeadings.length} question-style heading(s), need >= ${min}`;
    } else if (req.steps_present !== undefined) {
      // Check for numbered list items (1. 2. 3. etc.)
      const steps = (body.match(/^\d+\.\s/gm) || []).length;
      const min = typeof req.steps_present === 'number' ? req.steps_present : 3;
      result.pass = steps >= min;
      result.detail = `${steps} numbered step(s), need >= ${min}`;
    } else if (req.code_explanation_ratio !== undefined) {
      // Ratio of explanation words to total content (including code)
      const totalWords = (body.match(/[a-zA-Z0-9]+/g) || []).length;
      const explanationWords = countWords(body); // strips code blocks
      const ratio = totalWords > 0 ? explanationWords / totalWords : 1;
      const minRatio = typeof req.code_explanation_ratio === 'number' ? req.code_explanation_ratio : 0.4;
      result.pass = ratio >= minRatio;
      result.detail = `ratio ${ratio.toFixed(2)}, need >= ${minRatio}`;
    }

    results.push(result);
  }

  const allPass = results.every(r => r.pass);
  return { description: goal.description || null, allPass, checks: results };
}

// ─── Layer 3: Concept Map ───────────────────────────────────────────────────

function loadConceptMap() {
  if (!fs.existsSync(CONCEPT_MAP_PATH)) return null;

  // Simple YAML parser for our flat structure (avoids adding a dependency)
  const raw = fs.readFileSync(CONCEPT_MAP_PATH, 'utf8');
  const concepts = {};
  let currentConcept = null;
  let currentField = null;

  for (const line of raw.split('\n')) {
    // Top-level concept key (indented 2 spaces under concepts:)
    const conceptMatch = line.match(/^  ([a-z0-9_-]+):\s*$/);
    if (conceptMatch) {
      currentConcept = conceptMatch[1];
      concepts[currentConcept] = { must_appear_in: [], terms: [] };
      currentField = null;
      continue;
    }

    // Field key under a concept
    const fieldMatch = line.match(/^\s{4}(must_appear_in|terms):\s*$/);
    if (fieldMatch && currentConcept) {
      currentField = fieldMatch[1];
      continue;
    }

    // Array item under a field
    const itemMatch = line.match(/^\s{6}-\s+(.+)$/);
    if (itemMatch && currentConcept && currentField) {
      let val = itemMatch[1].trim();
      // Remove quotes
      val = val.replace(/^["']|["']$/g, '');
      concepts[currentConcept][currentField].push(val);
    }
  }

  return concepts;
}

function runConceptAudit(concepts, allPages) {
  if (!concepts) return null;

  const results = {};

  for (const [conceptName, concept] of Object.entries(concepts)) {
    const pageResults = [];

    for (const pagePath of concept.must_appear_in) {
      const page = allPages.find(p => p.relativePath === pagePath);
      if (!page) {
        pageResults.push({ page: pagePath, pass: false, detail: 'page not found', foundTerms: [], missingTerms: concept.terms });
        continue;
      }

      const body = page.body.toLowerCase();
      const foundTerms = [];
      const missingTerms = [];

      for (const term of concept.terms) {
        // Case-insensitive search, also try with escaped special chars
        const escaped = term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const re = new RegExp(escaped, 'i');
        if (re.test(page.body)) {
          foundTerms.push(term);
        } else {
          missingTerms.push(term);
        }
      }

      // Pass if at least half the terms appear
      const threshold = Math.ceil(concept.terms.length / 2);
      const pass = foundTerms.length >= threshold;

      pageResults.push({
        page: pagePath,
        pass,
        detail: `${foundTerms.length}/${concept.terms.length} terms found (need >= ${threshold})`,
        foundTerms,
        missingTerms,
      });
    }

    // Orphan detection: pages that heavily mention this concept's terms but aren't in must_appear_in
    const orphans = [];
    for (const page of allPages) {
      if (concept.must_appear_in.includes(page.relativePath)) continue;

      let termHits = 0;
      for (const term of concept.terms) {
        const escaped = term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const re = new RegExp(escaped, 'i');
        if (re.test(page.body)) termHits++;
      }

      // Flag if page has >= 75% of the concept's terms
      if (concept.terms.length > 0 && termHits / concept.terms.length >= 0.75) {
        orphans.push({ page: page.relativePath, termsFound: termHits, totalTerms: concept.terms.length });
      }
    }

    const allPass = pageResults.every(r => r.pass);
    results[conceptName] = { allPass, pages: pageResults, orphans };
  }

  return results;
}

// ─── Main ───────────────────────────────────────────────────────────────────

function main() {
  const args = process.argv.slice(2);
  const showSummary = args.includes('--summary');
  const onlyFailures = args.includes('--only-failures');

  // Collect all mdx files
  const files = globMdx(CONTENT_ROOT);
  const docPaths = buildDocPathSet(CONTENT_ROOT);

  // Parse all files
  const allPages = files.map(filePath => {
    const raw = fs.readFileSync(filePath, 'utf8');
    const { data, content: body } = matter(raw);
    const relPath = relativeTo(filePath, CONTENT_ROOT);
    return { filePath, relativePath: relPath, raw, data, body };
  });

  // Run audits
  const pageResults = allPages.map(page => {
    const headings = getHeadings(page.body);
    const base = runBaseChecks(page.filePath, page.raw, page.data, page.body, docPaths);
    const goal = evaluateGoalRequires(page.data.goal, page.body, page.data, headings);

    return {
      path: page.relativePath,
      title: page.data.title || null,
      base,
      goal,
    };
  });

  // Duplicate titles (global check)
  const duplicateTitles = findDuplicateTitles(allPages);

  // Concept map audit
  const concepts = loadConceptMap();
  const conceptAudit = runConceptAudit(concepts, allPages);

  // Assemble output
  let output = {
    summary: {
      totalPages: pageResults.length,
      pagesWithIssues: pageResults.filter(p => p.base.issues.length > 0).length,
      pagesWithGoal: pageResults.filter(p => p.goal !== null).length,
      pagesPassingGoal: pageResults.filter(p => p.goal?.allPass).length,
      pagesFailingGoal: pageResults.filter(p => p.goal && !p.goal.allPass).length,
      duplicateTitles: Object.keys(duplicateTitles).length > 0 ? duplicateTitles : null,
      geo: {
        pagesWithQuestions: pageResults.filter(p => p.base.geo?.hasQuestions).length,
        pagesWithAnswer: pageResults.filter(p => p.base.geo?.hasAnswer).length,
        pagesWithBoth: pageResults.filter(p => p.base.geo?.hasQuestions && p.base.geo?.hasAnswer).length,
        pagesWithNeither: pageResults.filter(p => !p.base.geo?.hasQuestions && !p.base.geo?.hasAnswer).length,
      },
    },
    pages: onlyFailures
      ? pageResults.filter(p => p.base.issues.length > 0 || (p.goal && !p.goal.allPass))
      : pageResults,
  };

  if (conceptAudit) {
    output.conceptAudit = conceptAudit;
  }

  // Print JSON to stdout
  console.log(JSON.stringify(output, null, 2));

  // Optional summary table to stderr
  if (showSummary) {
    console.error('\n── Audit Summary ──────────────────────────────────────');
    console.error(`Total pages:       ${output.summary.totalPages}`);
    console.error(`Pages with issues: ${output.summary.pagesWithIssues}`);
    console.error(`Pages with goal:   ${output.summary.pagesWithGoal}`);
    console.error(`  Passing:         ${output.summary.pagesPassingGoal}`);
    console.error(`  Failing:         ${output.summary.pagesFailingGoal}`);

    const geo = output.summary.geo;
    console.error(`GEO/AEO readiness:`);
    console.error(`  With questions:  ${geo.pagesWithQuestions}`);
    console.error(`  With answer:     ${geo.pagesWithAnswer}`);
    console.error(`  With both:       ${geo.pagesWithBoth}`);
    console.error(`  With neither:    ${geo.pagesWithNeither}`);

    if (output.summary.duplicateTitles) {
      console.error(`\nDuplicate titles:`);
      for (const [title, files] of Object.entries(output.summary.duplicateTitles)) {
        console.error(`  "${title}": ${files.join(', ')}`);
      }
    }

    // Pages with most issues
    const worst = [...pageResults]
      .sort((a, b) => b.base.issues.length - a.base.issues.length)
      .slice(0, 10)
      .filter(p => p.base.issues.length > 0);

    if (worst.length > 0) {
      console.error('\nTop pages by issue count:');
      for (const p of worst) {
        console.error(`  [${p.base.issues.length}] ${p.path}`);
        for (const issue of p.base.issues) {
          console.error(`      - ${issue}`);
        }
      }
    }

    // Goal failures
    const goalFailures = pageResults.filter(p => p.goal && !p.goal.allPass);
    if (goalFailures.length > 0) {
      console.error('\nGoal checklist failures:');
      for (const p of goalFailures) {
        console.error(`  ${p.path}`);
        for (const check of p.goal.checks.filter(c => !c.pass)) {
          console.error(`    ✗ ${check.label}: ${check.detail}`);
        }
      }
    }

    // Concept coverage failures
    if (conceptAudit) {
      const conceptFailures = Object.entries(conceptAudit).filter(([, v]) => !v.allPass);
      if (conceptFailures.length > 0) {
        console.error('\nConcept coverage gaps:');
        for (const [name, result] of conceptFailures) {
          for (const pr of result.pages.filter(p => !p.pass)) {
            console.error(`  ${name} → ${pr.page}: ${pr.detail}`);
          }
        }
      }

      const allOrphans = Object.entries(conceptAudit).flatMap(([name, result]) =>
        result.orphans.map(o => ({ concept: name, ...o }))
      );
      if (allOrphans.length > 0) {
        console.error('\nPotential orphans (pages covering concepts not in their must_appear_in):');
        for (const o of allOrphans) {
          console.error(`  ${o.concept} → ${o.page} (${o.termsFound}/${o.totalTerms} terms)`);
        }
      }
    }

    console.error('──────────────────────────────────────────────────────\n');
  }

  // Exit with error code if there are issues
  const hasIssues = output.summary.pagesWithIssues > 0 ||
    output.summary.pagesFailingGoal > 0 ||
    (conceptAudit && Object.values(conceptAudit).some(v => !v.allPass));

  process.exit(hasIssues ? 1 : 0);
}

main();
