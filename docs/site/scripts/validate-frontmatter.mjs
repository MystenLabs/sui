/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Validate docs page frontmatter against the canonical JSON Schema
 * (docs/site/frontmatter.schema.json).
 *
 * This is the authoritative, structural frontmatter gate. The audit
 * pipeline (audit-docs.mjs) still reports on content quality; this script
 * only enforces that frontmatter matches the schema so downstream tooling
 * (the evals harness, the dashboard) can rely on its shape.
 *
 * Usage:
 *   node scripts/validate-frontmatter.mjs                 # validate all pages
 *   node scripts/validate-frontmatter.mjs file1 file2     # validate specific files
 *   node scripts/validate-frontmatter.mjs --summary       # human-readable summary
 *
 * Exit code is non-zero if any validated page fails schema validation.
 *
 * Snippets and generated GraphQL reference pages are excluded (they are
 * partials / machine-generated and do not carry full page frontmatter).
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import matter from 'gray-matter';
import Ajv from 'ajv/dist/2020.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const SITE_ROOT = path.resolve(__dirname, '..');
const CONTENT_ROOT = path.resolve(SITE_ROOT, '..', 'content');
const SCHEMA_PATH = path.resolve(SITE_ROOT, 'frontmatter.schema.json');

// Paths excluded from frontmatter validation. Snippets are reusable partials;
// the sui-graphql reference is generated at build time.
const EXCLUDE_PATTERNS = [
  /(^|\/)snippets\//,
  /(^|\/)sui-graphql\//,
];

function isExcluded(relPath) {
  return EXCLUDE_PATTERNS.some((re) => re.test(relPath));
}

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

// YAML (via js-yaml/gray-matter) parses unquoted ISO timestamps into Date
// objects. Normalize them back to ISO strings so string-typed schema fields
// (e.g. last_verified) validate as authored.
function normalizeDates(value) {
  if (value instanceof Date) return value.toISOString();
  if (Array.isArray(value)) return value.map(normalizeDates);
  if (value && typeof value === 'object') {
    const out = {};
    for (const [k, v] of Object.entries(value)) out[k] = normalizeDates(v);
    return out;
  }
  return value;
}

function formatErrors(errors) {
  if (!errors) return [];
  return errors.map((e) => {
    const loc = e.instancePath || '(root)';
    let msg = `${loc} ${e.message}`;
    if (e.keyword === 'additionalProperties' && e.params?.additionalProperty) {
      msg = `${loc} has unknown property "${e.params.additionalProperty}"`;
    } else if (e.keyword === 'enum' && e.params?.allowedValues) {
      msg = `${loc} ${e.message}: ${e.params.allowedValues.join(', ')}`;
    } else if (e.keyword === 'anyOf') {
      msg = `${loc} does not match any known goal check type (needs one of pattern, headings, links_to, has_tables, has_images, has_frontmatter, min_words, has_questions, has_answer, answer_in_intro, question_headings, steps_present, code_explanation_ratio)`;
    }
    return msg;
  });
}

function main() {
  const args = process.argv.slice(2);
  const showSummary = args.includes('--summary');
  const fileArgs = args.filter((a) => !a.startsWith('--'));

  const schema = JSON.parse(fs.readFileSync(SCHEMA_PATH, 'utf8'));
  const ajv = new Ajv({ allErrors: true, strict: false });
  const validate = ajv.compile(schema);

  // Resolve the set of files to check.
  let files;
  if (fileArgs.length > 0) {
    files = fileArgs
      .map((f) => path.resolve(process.cwd(), f))
      .filter((f) => f.endsWith('.mdx') && fs.existsSync(f));
  } else {
    files = globMdx(CONTENT_ROOT);
  }

  const failures = [];
  let checked = 0;

  for (const filePath of files) {
    const relPath = path.relative(CONTENT_ROOT, filePath);
    if (isExcluded(relPath)) continue;

    checked++;

    let data;
    try {
      ({ data } = matter(fs.readFileSync(filePath, 'utf8')));
      data = normalizeDates(data);
    } catch (err) {
      failures.push({ path: relPath, errors: [`frontmatter parse error: ${err.message}`] });
      continue;
    }

    const valid = validate(data);
    if (!valid) {
      failures.push({ path: relPath, errors: formatErrors(validate.errors) });
    }
  }

  const output = { checked, failed: failures.length, failures };

  if (showSummary) {
    console.error('\n── Frontmatter Schema Validation ──────────────────────');
    console.error(`Pages checked: ${checked}`);
    console.error(`Pages failing: ${failures.length}`);
    if (failures.length > 0) {
      console.error('');
      for (const f of failures) {
        console.error(`  ✗ ${f.path}`);
        for (const err of f.errors) console.error(`      - ${err}`);
      }
    } else {
      console.error('All pages conform to the frontmatter schema. ✓');
    }
    console.error('────────────────────────────────────────────────────────\n');
  } else {
    console.log(JSON.stringify(output, null, 2));
  }

  process.exit(failures.length > 0 ? 1 : 0);
}

main();
