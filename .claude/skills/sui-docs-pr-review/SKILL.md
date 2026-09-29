---
name: sui-docs-pr-review
description: >-
  Review a pull request that touches docs/content against the documentation
  style guide and the actual source of truth, then report findings as inline
  review comments. Use when asked to review a docs PR, audit a docs branch, or
  check a docs PR against the style guide. Runs locally on demand, and on a
  schedule from MystenLabs/docs-data-dashboard.
---

# Reviewing a Sui docs PR

Produces a findings list where every item names a file, a line, and the exact
edit.

The one rule everything follows: **check a page against the source it
describes, not against itself.** A page can be internally consistent, pass every
frontmatter and link check, and still document an API that never existed. That
is the normal case for the defects worth finding.

## Where this runs

Locally on demand, and on a schedule from
`MystenLabs/docs-data-dashboard/.github/workflows/sui-docs-pr-review.yml`, which
checks this repository out at a pull request's head and runs the method from
here. That workflow reads four paths out of this repository, so moving or
renaming any of them breaks it:

```
.claude/skills/sui-docs-pr-review/            the method
.claude/skills/sui-docs-pr-review/scripts/sweep.sh
.claude/skills/sui-docs-pr-review/scripts/gate.js
docs/sui-documentation-style-guide.skill      the canonical style guide
```

It runs from outside this repository because what grants the right to act on a
pull request here is the GitHub App installation, not where the workflow lives.
The cost of that choice is that a scheduled run is not a status check, so it
cannot block a merge. It approves, which is what satisfies the repository's
one-approval requirement.

## What this skill decides

The model reports findings. `scripts/gate.js` decides the verdict, and it is
deliberately conservative:

| Outcome | When |
|---|---|
| `request-changes` | At least one `blocking` finding. |
| `approve` | No blocking findings, and the change is in scope. |
| `comment` | Everything else, including every case the gate cannot judge. |

In scope means all of: every path under `docs/content`, prose files only, no
generated trees, no code fence added or modified, no `ImportContent` pin
changed, and inside the size limits. Anything else routes to a person.

The code rules carry the most weight and are worth understanding rather than
loosening. The highest-value finding this method has produced was a
`referral_fee` formula missing its `1e9` divisor, wrong by a factor of a hundred
million on a page about money. Catching it meant reading
`expiry_market.move:1185` and `math.move:73` in the `deepbookv3` repository at a
pinned revision, and the page passed `audit-docs.mjs` completely clean. An
automated pass is reliable on prose and structure and unreliable on claims;
claims cluster in code. So copy-editing gets approved and anything that changes
what a reader would run does not.

## Load before reviewing

Everything this skill needs is vendored under `reference/`. Nothing resolves to
a path outside the repository, because CI cannot see a reviewer's home
directory.

### The style guide, and which copy wins

The authority is **`docs/sui-documentation-style-guide.skill`**, in this
repository. It is a zip, so read it with:

```bash
unzip -p docs/sui-documentation-style-guide.skill \
  'sui-documentation-style-guide/SKILL.md'
```

Use that copy and no other. At least three versions of this guide exist: the
canonical one above, a consolidation in `reference/style-guide.md`, and whatever
sits in a given reviewer's `~/.claude/skills/synced/`. They have already drifted
apart. As of 2026-09-29 the canonical copy carried rules the synced copy did
not, among them numbered-subheading consistency and the ban on manual footer
sections. Reviewing against a stale copy quietly under-reports.

### Everything else

| File | Load when |
|---|---|
| `checks.md` | Always. The defect catalogue and what each check proves. |
| `reference/audit-method.md` | Always. Sweep discipline, generated trees, how to write a finding. |
| `reference/technical-documentation-writing.md` | Always. Governs prose: no invented source code, inline citations for technical claims. |
| `reference/style-guide.md` | Supplementary only. A consolidation that expands on the canonical guide. Where the two disagree, the canonical guide wins. |
| `reference/source-code-policy.md` | Any PR containing code blocks. |
| `reference/ts-docs-patterns.md` | **Any PR containing TypeScript.** Client construction, `.core` versus top-level methods, coin and balance APIs, keypairs, `FailedTransaction` semantics, sponsored transactions. |
| `reference/security-compliance.md` | A PR touching keys, signing, custody, or access control. |
| `reference/writing-voice.md` | Always, but lightly. House voice: how an explanation is built and how a page should sound. Findings from it are suggestions, never blocking, because voice is a preference where house style is a rule. |
| `posting.md` | Publishing to GitHub. Anchoring, rate limits, suggestion blocks. |

Everything under `reference/` is a vendored copy of a hand-maintained local
skill that no registry tracks, so `npx skills update` will never refresh them.
When an upstream skill changes, re-copy it here in the same pull request. A
stale copy reviews against last quarter's rules without saying so.

## Workflow

### 1. Get the files, not the diff

```bash
gh pr view <PR> --json title,body,author,headRefName,files
gh pr diff <PR> > pr.diff
```

Then fetch each changed file **whole** from the head branch:

```bash
curl -sL "https://raw.githubusercontent.com/MystenLabs/sui/<headRefName>/<path>" -o <name>
```

**Line numbers come from the fetched file, never from the diff.** This is the
easiest mistake to make and it anchors findings tens of lines off. Verify every
anchor with `grep -n` against the fetched file before it leaves your hands.

### 2. Run the sweep

```bash
scripts/sweep.sh <files>
```

Prints every hit with its matched marker, so the output is checkable. It finds
mechanical violations only. It cannot find a wrong claim.

Restrict the results to lines the PR actually adds. A sweep over whole files
reports pre-existing prose as though this PR wrote it.

### 3. Verify claims against source

Where the real defects are. Never accept a technical claim because the page is
internally consistent, and never verify a page against another page.

| Claim about | Verify against |
|---|---|
| Move framework, stdlib, error constants | `crates/sui-framework/packages/**/sources/*.move` |
| DeepBook, Predict, margin | `MystenLabs/deepbookv3` at the revision the page pins |
| TypeScript SDK behaviour | `MystenLabs/ts-sdks`, fetched fresh |
| gRPC services, RPCs, messages | `MystenLabs/sui-apis` |
| Protocol behaviour | root `CLAUDE.md`, `docs/content/develop/transactions/transaction-lifecycle.mdx` |
| CLI flags and output | `crates/sui/src/…`, and run the command |
| What a page renders as | `https://docs.sui.io/…` |

Pin the revision in the finding. "the protos at `618b6c84`, which `Cargo.toml`
pins, say X" survives review; "the protos say X" does not.

A skill is not a source. Skills drift on the same schedule as the docs they
describe. Use one to learn where the answer lives, then cite the code.

### 4. Run the repo's own audit

```bash
cd docs/site && node scripts/audit-docs.mjs --only-failures
```

Expect false positives on links into generated trees and on directory index
pages, which resolve live. Verify with `curl` before reporting one.

### 5. Write the findings

Number them `F1..Fn`: file, verified line, what is wrong, the exact replacement.
Order by severity, code defects first.

Report separately, always:

- **Checks that passed.** What you looked for and found clean.
- **Deliberately not raised.** What you judged out of scope, and why.
- **Not verified.** What you could not run. An honest gap costs nothing; an
  unstated one costs trust in everything else.

### 6. Severity

Use these labels so downstream tooling can read the result:

| Severity | Meaning |
|---|---|
| `blocking` | A wrong claim, a broken example, a defect a reader would act on. |
| `suggestion` | Style, register, structure. Correct as written, better if changed. |

## Publishing

Locally, delivering findings in conversation is the default and the complete
deliverable. A reviewer's own configuration may carry a standing "never post
comments on pull requests" rule, in which case posting needs an explicit
instruction for that specific PR. See `posting.md`.

On a scheduled run the findings go up as one review with event `COMMENT`, and a
separate step casts the verdict from `scripts/gate.js`. Do not approve or
request changes from inside the review itself: the gate applies scope rules the
model is not in a position to evaluate about its own output.

No `Co-Authored-By`, no "generated by Claude", no AI attribution anywhere.

## Repo specifics

`upstream` is `MystenLabs/sui`, `origin` is the personal fork. Branch from
`upstream/main` and fetch immediately first; `main` moves several times a day.

Generated trees, never edited directly:

```
references/framework/**                     from Move source
references/sui-api/sui-graphql/*            from the schema
sui-stack/seal/*.mdx                        from MystenLabs/seal
sui-stack/messaging/*.mdx                   from MystenLabs/sui-stack-messaging
references/release-notes.mdx                from release notes
references/awesome-sui{,-gaming}{.mdx,/}    from awesome-sui
**/subtree/**/*.mdx                         vendored
```

`git check-ignore -v <path>` is the authority. Two surfaces sit outside
`docs/content` and are easy to forget: `/skills` renders
`docs/site/src/data/skills.json`, and every redirect lives in
`docs/site/vercel.json`.
