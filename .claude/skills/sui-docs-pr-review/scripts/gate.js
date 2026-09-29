#!/usr/bin/env node
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Decides which verdict the docs review may cast on a pull request.
//
//   node gate.js --base <sha> --head <sha> [--findings findings.json]
//
// Prints a JSON verdict on stdout and always exits 0 unless the gate itself
// could not run (exit 2). Read `verdict`, never the exit code.
//
//   request-changes   at least one blocking finding
//   approve           no blocking findings, and the change is in scope
//   comment           everything else, including every case the gate cannot judge
//
// The scope rules exist because this review is reliable on prose and structure
// and unreliable on claims. The code-fence rule carries the most weight. The
// highest-value finding this method has produced was a referral_fee formula
// missing its 1e9 divisor, wrong by a factor of a hundred million on a page
// about money. Catching it meant reading expiry_market.move and math.move in
// the deepbookv3 repository at a pinned revision, and the page passed
// audit-docs.mjs completely clean. Claims cluster in code blocks, so a change
// that touches one is never approved here: it goes to a person.

const { execFileSync } = require('child_process');
const fs = require('fs');

const argv = process.argv.slice(2);
const arg = (name, fallback = null) => {
  const i = argv.indexOf(`--${name}`);
  return i === -1 || i === argv.length - 1 ? fallback : argv[i + 1];
};

const BASE = arg('base');
const HEAD = arg('head');
const FINDINGS = arg('findings');

// Raise MAX_CHANGED_LINES only alongside evidence that the review stays
// reliable at the larger size.
const ALLOWED_PREFIX = 'docs/content/';
const MAX_CHANGED_LINES = 400;
// Deliberately loose. A frontmatter resync legitimately touches dozens of pages
// for a handful of lines each, and refusing that shape would reject the most
// common mechanical docs change. MAX_CHANGED_LINES is the real guard on how
// much the review had to hold at once.
const MAX_FILES = 60;
const BLOCKING = new Set(['blocking', 'correctness', 'high']);

const git = (args, quiet = false) =>
  execFileSync('git', args, {
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    // `git show` on a path that does not exist at that revision writes to
    // stderr before failing. That is the normal case for an added or deleted
    // file, so silence it rather than filling the job log with `fatal:`.
    stdio: quiet ? ['ignore', 'pipe', 'ignore'] : ['ignore', 'pipe', 'pipe'],
  });

function bail(msg) {
  console.log(JSON.stringify({ verdict: 'comment', error: msg, reasons: [msg] }, null, 2));
  process.exit(2);
}

if (!BASE || !HEAD) bail('gate.js requires --base and --head');

// `<ImportContent>` pulls code into the page from another repository, pinned by
// branch, path and line range. Changing any of that changes the code a reader
// sees, without a single character of it appearing in the diff. It is not a
// fence, so the fence comparison below cannot see it, and it is exactly the
// kind of code claim that needs checking against source.
const IMPORT_TAG = /<ImportContent\b[^>]*>/g;

/** Every ImportContent tag in a page, for comparison across revisions. */
function imports(text) {
  return (text.match(IMPORT_TAG) || []).map((s) => s.replace(/\s+/g, ' '));
}

/** Fenced code blocks in a page, normalised for comparison. */
function fences(text) {
  const out = [];
  let open = null;
  let buf = [];
  for (const line of text.split('\n')) {
    const m = /^\s*(`{3,}|~{3,})(.*)$/.exec(line);
    if (m && !open) {
      open = m[1][0].repeat(3);
      buf = [m[2].trim()];
    } else if (m && open && m[1][0].repeat(3) === open) {
      out.push(buf.join('\n'));
      open = null;
      buf = [];
    } else if (open) {
      buf.push(line);
    }
  }
  // An unterminated fence is itself a defect; surface it as a difference.
  if (open) out.push(`${buf.join('\n')}\n<<unterminated>>`);
  return out;
}

const show = (ref, path) => {
  try {
    return git(['show', `${ref}:${path}`]);
  } catch {
    return null; // added on one side or deleted on the other
  }
};

const blockers = [];
const notes = [];

// --- which files changed -----------------------------------------------------

let changed;
try {
  changed = git(['diff', '--name-only', `${BASE}...${HEAD}`])
    .split('\n')
    .map((s) => s.trim())
    .filter(Boolean);
} catch (e) {
  bail(`could not diff ${BASE}...${HEAD}: ${e.message}`);
}

if (changed.length === 0) bail('no files changed between base and head');
if (changed.length > MAX_FILES) {
  blockers.push(`${changed.length} files changed, limit is ${MAX_FILES}`);
}

const outside = changed.filter((f) => !f.startsWith(ALLOWED_PREFIX));
if (outside.length) {
  blockers.push(
    `${outside.length} file(s) outside ${ALLOWED_PREFIX}: ` +
      outside.slice(0, 5).join(', ') +
      (outside.length > 5 ? ', and more' : ''),
  );
}

// Being under docs/content is not the same as being prose. sidebars.js and
// references.js live there and are JavaScript that drives the whole site's
// navigation. Approving a change to them off the back of a prose review is not
// something this gate is entitled to do.
const nonProse = changed.filter((f) => f.startsWith(ALLOWED_PREFIX) && !/\.mdx?$/.test(f));
if (nonProse.length) {
  blockers.push(
    `${nonProse.length} non-prose file(s) under ${ALLOWED_PREFIX}: ${nonProse.join(', ')}`,
  );
}

// --- generated trees ---------------------------------------------------------
// git check-ignore is the authority; the rules are spread across three
// .gitignore files and reading them by hand gets this wrong.

const generated = changed.filter((f) => {
  try {
    execFileSync('git', ['check-ignore', '-q', f], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
});
if (generated.length) {
  blockers.push(`${generated.length} file(s) in a generated tree: ${generated.join(', ')}`);
}

// --- code fences -------------------------------------------------------------

const differs = (a, b) => a.length !== b.length || a.some((x, i) => x !== b[i]);

const fenceChanged = [];
const importChanged = [];
for (const f of changed) {
  // Out-of-scope and non-prose paths already block on their own; checking their
  // code adds noise to the reasons and says nothing new.
  if (!f.startsWith(ALLOWED_PREFIX) || !/\.mdx?$/.test(f)) continue;
  const before = show(BASE, f);
  const after = show(HEAD, f);
  const beforeText = before === null ? '' : before;
  const afterText = after === null ? '' : after;
  if (differs(fences(beforeText), fences(afterText))) fenceChanged.push(f);
  if (differs(imports(beforeText), imports(afterText))) importChanged.push(f);
}
if (fenceChanged.length) {
  blockers.push(
    `code example(s) added or modified in ${fenceChanged.join(', ')}; ` +
      'claims in code need checking against source',
  );
}
if (importChanged.length) {
  blockers.push(
    `ImportContent pin changed in ${importChanged.join(', ')}; ` +
      'this changes the code the page shows without the code appearing in the diff',
  );
}

// --- diff size ---------------------------------------------------------------

let changedLines = 0;
try {
  for (const line of git(['diff', '--numstat', `${BASE}...${HEAD}`]).split('\n').filter(Boolean)) {
    const [add, del] = line.split('\t');
    if (add !== '-' && del !== '-') changedLines += Number(add) + Number(del);
  }
} catch {
  blockers.push('could not measure diff size');
}
if (changedLines > MAX_CHANGED_LINES) {
  blockers.push(`${changedLines} lines changed, limit is ${MAX_CHANGED_LINES}`);
}
notes.push(`${changed.length} file(s), ${changedLines} changed line(s)`);

// --- findings ----------------------------------------------------------------
// A missing or unreadable findings file means the review result is unknown,
// which is never an approval.

let blockingFindings = [];
let unknownResult = false;

if (FINDINGS && fs.existsSync(FINDINGS)) {
  try {
    const parsed = JSON.parse(fs.readFileSync(FINDINGS, 'utf8'));
    const list = Array.isArray(parsed) ? parsed : parsed.findings || [];
    blockingFindings = list.filter((f) => BLOCKING.has(String(f.severity || '').toLowerCase()));
    notes.push(`${list.length} finding(s), ${blockingFindings.length} blocking`);
  } catch (e) {
    unknownResult = true;
    blockers.push(`findings file is unreadable: ${e.message}`);
  }
} else {
  unknownResult = true;
  blockers.push('no findings file, so the review result is unknown');
}

// --- verdict -----------------------------------------------------------------
// Blocking findings win over everything: a defect is reported even when the
// change is outside the scope the gate would approve.

let verdict;
if (blockingFindings.length > 0) verdict = 'request-changes';
else if (blockers.length === 0 && !unknownResult) verdict = 'approve';
else verdict = 'comment';

console.log(
  JSON.stringify(
    {
      verdict,
      blockingFindings: blockingFindings.map((f) => ({
        file: f.file,
        line: f.line,
        summary: f.summary,
      })),
      reasons: blockers,
      notes,
      checked: {
        files: changed.length,
        changedLines,
        filesOutsideDocsContent: outside.length,
        generatedFiles: generated.length,
        filesWithCodeChanges: fenceChanged.length,
      },
    },
    null,
    2,
  ),
);
