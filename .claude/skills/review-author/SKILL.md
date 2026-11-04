---
name: review-author
description: Classify a PR under the tiered review policy, meet the requirements of its tier, and write the tier and review annotations in the PR description.
---

# Tiered review: author side

This skill tells an authoring agent how to prepare a PR for the tiered review policy. The
`review-approver` skill checks the result and approves the PR when the tier allows it.

`/send-pr` invokes this skill. Invoke it directly to classify a PR or to refresh its annotations.

## Policy summary

- The policy applies only to PRs whose author is a member of the MystenLabs GitHub organization.
- The policy is opt in. A PR opts in when its description contains a `## Review tier` section.
  A PR without that section goes through normal human code review.
- Every PR under the policy gets an AI review, and the human author accepts or rejects each
  finding before the PR merges.
- A PR that merged under the policy can be reverted on request by any MystenLabs member, with no
  discussion required first. The author is notified so the issue can be discussed afterwards.

## Eligible paths

A PR is eligible only if every file it changes matches an entry in the eligible list and no entry
in the excluded list. Entries are path prefixes relative to the repository root. An excluded entry
wins over an eligible entry. A team opts its code in by adding its paths here, and opts a subpath
out by adding it to the excluded list.

The approver reads these lists from the PR's base branch. A PR that edits these lists does not
change its own eligibility.

Eligible:

```
(no paths yet)
```

Excluded:

```
.claude/skills/review-author/
.claude/skills/review-approver/
.github/CODEOWNERS
```

If any changed file is not eligible, the PR cannot opt in. Do not write a `## Review tier`
section. Tell the user which files are ineligible, and suggest splitting them into a separate PR
if the rest of the change could use the policy.

## Step 1 — Classify the tier

Read the full diff against the base branch and the stated motivation. Pick the lowest tier whose
definition covers every part of the change. If parts of the change fall in different tiers, the
PR takes the highest of them.

**Tier 0: comments and moves.** The change has no effect on compiled code or behavior. Examples
are comment edits, documentation edits, and file moves or renames whose contents do not change
apart from the module paths that the move requires.

**Tier 1: simple bug fixes and optimizations.** The change fixes one or more implementation bugs,
or makes an optimization that does not change the architecture. The intended behavior of the code
does not change; only the code's conformance to that behavior changes.

**Tier 2: small features.** The change adds a feature with limited scope that does not affect
existing users. Existing behavior, APIs, and on-disk or on-wire formats stay the same.

**Tier 3: everything else.** This includes breaking changes, new APIs, architectural changes, and
changes to the protocol. In this repository a change is always Tier 3 if it does any of these:

- changes `crates/sui-protocol-config/` or reads a new protocol config flag;
- changes `crates/sui-types/tests/snapshots/format__sui.yaml.snap` or any BCS-serialized type;
- changes the result of executing any transaction, or the contents of checkpoints or effects;
- changes the schema or key encoding of a persisted table;
- adds or changes a gRPC, JSON-RPC, GraphQL, CLI, or Rust SDK interface;
- changes a system package in `crates/sui-framework/packages/`.

When the choice between two tiers is unclear, pick the higher tier.

## Step 2 — Meet the tier requirements

### Tier 0

No requirement beyond the AI review in Step 3.

### Tier 1

Add a regression test that fails without the fix and passes with it. Confirm the failure yourself:
revert the non-test part of the change, run the test, observe the failure, and restore the change.

If such a test is infeasible or unnecessary, write a short explanation of why. A race that only a
deterministic simulation test reaches, or an optimization with no observable behavior change, are
examples of acceptable explanations.

### Tier 2

New code must have tests that cover its main behavior and its error paths. Name the tests in the
test plan.

One human other than the author must approve the PR. That approver signals agreement that the
feature should be built. They are not expected to read the code. Tell the user to request this
sign-off from someone who knows the motivation for the feature.

### Tier 3

Tier 3 requires a mini design doc (MDD) in the PR description, in a `## Mini design doc` section
directly after `## Description`. The MDD states what the change does and why, at the level of
detail a coding agent needs to reproduce the important decisions. It does not need to describe
every detail, because reimplementation is cheap. It has these subsections:

- `### Motivation`: the problem and why it matters now.
- `### Design`: the important decisions, the alternatives that were rejected and why, and the
  non-goals.
- `### Ecosystem impact and rollout`: the users, services, and SDKs that the change affects, the
  rollout plan, and any protocol config or two-PR sequencing (see the root `CLAUDE.md`).
- `### Test strategy`: how the tests show that the design works, and which bugs they cannot catch.
- `### Implementation notes`: decisions made during implementation and surprises that came up.

Collaboration matters most before implementation starts. When the author starts a Tier 3 change,
recommend opening a draft PR with only the MDD so that reviewers can comment before code exists.

Rules for the agent:

- Do not mark a PR Tier 3 unless the code matches the MDD. Before writing the annotation, compare
  the diff to each statement in `### Design` and `### Ecosystem impact and rollout`. Fix the code
  or stop and report each mismatch to the author.
- Do not edit `### Motivation`, `### Design`, or `### Ecosystem impact and rollout` to match the
  code without the author's approval. These sections are what the human reviewers approve.
- Add to `### Implementation notes` whenever implementation forces a decision that the MDD does
  not cover, or reveals something the MDD did not expect.

Two humans other than the author, who are experts on the affected systems, must approve the PR.
Use the `.github/CODEOWNERS` owners of the changed paths as the expert reviewers where entries
exist. The reviewers review the MDD. Their bar is whether they agree with what they expect an AI
to build from the MDD used as a prompt. They may ask for any change to the MDD, including one that
discards the implementation. When a reviewer changes the MDD, update the code to match it.

## Step 3 — AI review

Every tier requires an AI review of the full `$BASE...HEAD` diff by `/code-review max`. The author
must accept or reject each finding. A local review satisfies this requirement when it is recorded
as described below. `/send-pr` Step 4 performs this review.

For Tier 3, tell the reviewer to focus on these questions in addition to correctness:

- Does the implementation match the MDD in the important details?
- Is the test coverage adequate, and what kinds of bugs could pass the tests?
- What is the ecosystem impact? Does the change break other features? What rollout does it need?

For Tier 2, tell the reviewer to check that the new code is tested appropriately.

## Step 4 — Write the annotations

### Review stamp

The last line of `## Test plan` records the AI review:

```
Reviewed with `/code-review max` at <sha>: <outcome>.
```

`<sha>` is the full hash of the PR head commit that the review covered, after the push. `<outcome>`
states the number of findings and how the author resolved them, for example "no meaningful
findings" or "3 findings, 2 fixed, 1 rejected as a false positive". Every finding must be resolved
by the author before the stamp is written.

If commits are added after the stamp, review the new commits and update the stamp to the new head.

### Tier section

Add a `## Review tier` section after `## Test plan` and before `## Release notes`. Nothing may
follow the last release-notes checkbox in a PR body, so this section must not go at the end.

```
## Review tier

Tier: <0|1|2|3>
Rationale: <one sentence on why the change fits this tier>
```

Tier 1 adds one of these lines:

```
Regression test: `<path to test>::<test name>`
Regression test not feasible: <one or two sentences>
```

Refresh the section whenever the diff changes. If the new diff fits a higher tier, raise the tier
and meet the new tier's requirements. If the PR becomes ineligible, remove the section and tell
the author.
