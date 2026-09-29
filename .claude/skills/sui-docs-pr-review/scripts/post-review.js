#!/usr/bin/env node
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Publish review findings to a GitHub PR.
//
//   node post-review.js review.json <PR> [pendingReviewNodeId] [--repo owner/name] [--dry-run]
//
// With a pendingReviewNodeId: appends every comment to that existing pending
// review via GraphQL (preserving any draft comment already in it), then submits.
// Without: creates and submits a fresh review in one REST call.
//
// review.json: { body, event, comments: [{ path, line, side, body }] }
//
// Paces writes at 1.5s and backs off on GitHub's secondary rate limit, which is
// separate from the 5000/hr core limit and reports as a 403.

const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const argv = process.argv.slice(2);
const flags = new Set(argv.filter((a) => a.startsWith('--')));
const pos = argv.filter((a) => !a.startsWith('--'));
const repoFlag = argv.find((a) => a.startsWith('--repo='));

const REVIEW_FILE = pos[0];
const PR = pos[1];
const PENDING_ID = pos[2] || null;
const REPO = repoFlag ? repoFlag.split('=')[1] : 'MystenLabs/sui';
const DRY = flags.has('--dry-run');

if (!REVIEW_FILE || !PR) {
  console.error('usage: post-review.js review.json <PR> [pendingReviewNodeId] [--repo owner/name] [--dry-run]');
  process.exit(2);
}

const review = JSON.parse(fs.readFileSync(REVIEW_FILE, 'utf8'));
const comments = review.comments || [];
const outDir = path.dirname(path.resolve(REVIEW_FILE));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const gh = (args) =>
  execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });

function sanityCheck() {
  const bad = comments.filter(
    (c) => !c.path || !Number.isInteger(c.line) || c.line < 1 || !c.body,
  );
  if (bad.length) {
    console.error(`${bad.length} malformed comment(s):`);
    for (const b of bad) console.error('  ', JSON.stringify(b).slice(0, 160));
    process.exit(1);
  }
  const byFile = {};
  for (const c of comments) byFile[c.path] = (byFile[c.path] || 0) + 1;
  console.log(`${comments.length} comments across ${Object.keys(byFile).length} files:`);
  for (const [f, n] of Object.entries(byFile)) console.log(`  ${n.toString().padStart(3)}  ${f}`);
  console.log(`review body: ${(review.body || '').length} chars`);
  console.log(`repo: ${REPO}  pr: ${PR}  mode: ${PENDING_ID ? 'append+submit' : 'create'}`);
}

const ADD = `mutation($rid:ID!,$path:String!,$line:Int!,$body:String!){
  addPullRequestReviewThread(input:{
    pullRequestReviewId:$rid, path:$path, line:$line, side:RIGHT, body:$body
  }){ thread { id } }
}`;

const SUBMIT = `mutation($rid:ID!,$body:String!){
  submitPullRequestReview(input:{pullRequestReviewId:$rid, event:COMMENT, body:$body}){
    pullRequestReview { url state submittedAt }
  }
}`;

async function appendAndSubmit() {
  const done = [];
  const failed = [];

  for (let i = 0; i < comments.length; i++) {
    const c = comments[i];
    let ok = false;
    for (let attempt = 1; attempt <= 4 && !ok; attempt++) {
      try {
        const out = gh(['api', 'graphql',
          '-f', `query=${ADD}`, '-f', `rid=${PENDING_ID}`,
          '-f', `path=${c.path}`, '-F', `line=${c.line}`, '-f', `body=${c.body}`]);
        done.push({ i, path: c.path, line: c.line, id: JSON.parse(out).data.addPullRequestReviewThread.thread.id });
        ok = true;
      } catch (e) {
        const msg = (e.stderr || e.stdout || String(e)).slice(0, 300);
        if (attempt === 4) { failed.push({ i, path: c.path, line: c.line, msg }); }
        else { console.log(`  retry ${attempt} (${msg.slice(0, 80).replace(/\n/g, ' ')})`); await sleep(5000 * attempt); }
      }
    }
    console.log(`${i + 1}/${comments.length} ${ok ? 'ok  ' : 'FAIL'} ${c.path.split('/').pop()}:${c.line}`);
    await sleep(1500);
  }

  fs.writeFileSync(path.join(outDir, 'add_result.json'), JSON.stringify({ done, failed }, null, 2));
  console.log(`\nadded=${done.length} failed=${failed.length}`);
  for (const f of failed) console.log('  FAILED', f.path, f.line, f.msg.replace(/\n/g, ' '));

  if (failed.length) {
    console.error('\nNot submitting: fix the failures, then submit manually with the SUBMIT mutation.');
    process.exit(1);
  }

  const out = gh(['api', 'graphql', '-f', `query=${SUBMIT}`,
    '-f', `rid=${PENDING_ID}`, '-f', `body=${review.body || ''}`]);
  const r = JSON.parse(out).data.submitPullRequestReview.pullRequestReview;
  console.log(`\nsubmitted ${r.state} ${r.submittedAt}\n${r.url}`);
}

function createFresh() {
  const tmp = path.join(outDir, '.review-payload.json');
  fs.writeFileSync(tmp, JSON.stringify(review));
  try {
    const out = gh(['api', '--method', 'POST', `/repos/${REPO}/pulls/${PR}/reviews`, '--input', tmp]);
    const r = JSON.parse(out);
    console.log(`\nsubmitted ${r.state}\n${r.html_url}`);
  } catch (e) {
    const msg = (e.stderr || e.stdout || String(e));
    console.error('\nFailed:', msg.slice(0, 600));
    if (/422/.test(msg)) {
      console.error('\n422 usually means an existing PENDING review for this user, or a line');
      console.error('anchored outside the diff. Check:');
      console.error(`  gh api /repos/${REPO}/pulls/${PR}/reviews --jq '.[] | "\\(.id) \\(.user.login) \\(.state)"'`);
    }
    process.exit(1);
  } finally {
    fs.unlinkSync(tmp);
  }
}

function verify() {
  try {
    const paths = gh(['api', `/repos/${REPO}/pulls/${PR}/comments`, '--paginate', '--jq', '.[].path'])
      .trim().split('\n').filter(Boolean);
    const by = {};
    for (const p of paths) { const k = p.split('/').pop(); by[k] = (by[k] || 0) + 1; }
    console.log(`\npublished review comments: ${paths.length}`);
    for (const [f, n] of Object.entries(by)) console.log(`  ${n.toString().padStart(3)}  ${f}`);
  } catch { /* verification is best-effort */ }
}

(async () => {
  sanityCheck();
  if (DRY) { console.log('\n--dry-run: nothing posted.'); return; }
  if (PENDING_ID) await appendAndSubmit();
  else createFresh();
  verify();
})();
