---
name: review-author
description: Classify a PR under the tiered review policy, meet the requirements of its tier, and write the tier and review tags in the PR description.
---

# Tiered review: author side

This skill tells an authoring agent how to prepare a PR for the tiered review policy. The
`review-approver` skill checks the result and approves the PR when the tier allows it.

`/send-pr` invokes this skill. Invoke it directly to classify a PR or to refresh its tags.

## Policy summary

- The policy applies only to PRs whose author is a member of the MystenLabs GitHub organization.
- The policy is opt in. A PR opts in when its description contains a tier tag (see Step 4).
  A PR without a tier tag goes through normal human code review.
- Every PR under the policy gets an AI review, and the human author accepts or rejects each
  finding before the PR merges. The authoring agent enforces this locally: it runs the review,
  and it does not finish until the author has decided each finding. The findings and their
  resolution are not posted or recorded in the PR. The PR only gets a tag that says the review
  was done.
- Anyone can request a revert of a PR that merged under the policy. A PR can be reverted without
  discussion if it is an emergency or the author is unavailable.

## Eligible paths

A file is eligible if it matches an entry in the eligible list and no entry in the excluded list.
Entries are string prefixes of paths relative to the repository root, so
`crates/sui-bridge` matches `crates/sui-bridge/` and `crates/sui-bridge-cli/`. An excluded entry
wins over an eligible entry. A team opts its code in by adding its paths here, and opts a subpath
out by adding it to the excluded list.

The approver reads these lists from the PR's base branch. A PR that edits these lists does not
change the eligibility of its own files.

Eligible:

```
consensus/
crates/
docker/
scripts/
```

Excluded:

```
.claude/skills/review-author/
.claude/skills/review-approver/
.github/CODEOWNERS
crates/sui-framework
crates/sui-bridge
crates/sui-package-
```

A PR that changes ineligible files can still opt in and follow its tier. The approver agent does
not approve such a PR. It runs its checks and reports the result, and a human must review the
ineligible files and approve the PR. Tell the user which files are ineligible. If the eligible
part of the change is large, suggest moving the ineligible files to a separate PR so that the rest
can be approved automatically.

## Path floors

A path floor is the lowest tier that a PR can claim when it changes a file under that path. The
shape of a change does not express the consequences of a mistake in it. A one-line fix in the
commit rule can fork the network. The same fix in a metrics file cannot. The floor expresses
those consequences.

The effective tier of a PR is the higher of its shape tier from Step 1 and the highest floor of
any changed file. A floor raises a tier and never lowers it. A floor does not apply to a Tier 0
change, because a Tier 0 change changes no compiled code. The approver reads this table from the
PR's base branch, as it does the eligibility lists.

Entries are path prefixes. The longest matching prefix decides, so a more specific entry
overrides a less specific one. `shape` means that the shape of the change decides on its own.
A file with no matching entry has no floor.

### Consensus

Floor 3 covers code whose output every validator must compute identically from the same DAG:
block format, DAG state, the commit rule and leader schedule, linearization,
transaction voting and finalization, and persisted encoding. A divergence here forks or halts
the network, and a revert does not repair it.

The rest of the consensus code has no floor: proposal, synchronization, networking, services,
observer streaming, node parameters, tests, and metrics. A bug there can stall or slow the
network, and a revert repairs it, so the shape of the change decides the tier.

| Path | Floor |
|---|---|
| `consensus/` | shape |
| `consensus/types/` | 3 |
| `consensus/core/src/base_committer.rs` | 3 |
| `consensus/core/src/block.rs` | 3 |
| `consensus/core/src/block_manager.rs` | 3 |
| `consensus/core/src/commit.rs` | 3 |
| `consensus/core/src/commit_finalizer.rs` | 3 |
| `consensus/core/src/commit_observer.rs` | 3 |
| `consensus/core/src/core.rs` | 3 |
| `consensus/core/src/dag_state.rs` | 3 |
| `consensus/core/src/flex_committer.rs` | 3 |
| `consensus/core/src/leader_schedule.rs` | 3 |
| `consensus/core/src/leader_schedule_v3.rs` | 3 |
| `consensus/core/src/leader_scoring.rs` | 3 |
| `consensus/core/src/leader_slot_decider.rs` | 3 |
| `consensus/core/src/linearizer.rs` | 3 |
| `consensus/core/src/storage/rocksdb_store.rs` | 3 |
| `consensus/core/src/transaction_vote_tracker.rs` | 3 |
| `consensus/core/src/universal_committer.rs` | 3 |

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

This gives the shape tier. Then look up every changed file in the path floors table and take the
highest floor. The effective tier is the higher of the two, and it is the tier the PR claims.
When the floor raises the tier, add one line to `## Description` that names the file with the
highest floor, so that reviewers know why the PR claims a tier above its shape.

## Step 2 — Meet the tier requirements

Requirements accumulate. A PR whose effective tier is above its shape tier meets the
requirements of both. A bug fix that a floor raises to Tier 2 still needs the Tier 1 regression
test, and also needs the Tier 2 approval. A bug fix that a floor raises to Tier 3 still needs the
regression test, and its MDD can be a few bullets that state the bug, the fix, and why the fix
is correct.

### Tier 0

No requirement beyond the AI review in Step 3.

### Tier 1

Add a regression test that fails without the fix and passes with it. Confirm the failure yourself:
revert the non-test part of the change, run the test, observe the failure, and restore the change.

If such a test is infeasible or unnecessary, write a short explanation of why. A race that only a
deterministic simulation test reaches, or an optimization with no observable behavior change, are
examples of acceptable explanations.

Add a few bullets to the `## Test plan` section of the PR description. They name the regression
test and the bug it reproduces, and say that the test failed with the fix reverted. If there is
no regression test, they give the explanation instead.

### Tier 2

New code must have tests that cover its main behavior and its error paths. Name the tests in the
test plan.

One human other than the author must approve the PR. That approver signals agreement that the
feature should be built. They are not expected to read the code. Tell the user to request this
sign-off from someone who knows the motivation for the feature.

### Tier 3

Tier 3 requires a mini design doc (MDD) in the PR description, in a `## Mini design doc` section
directly after `## Description`. The MDD is a bullet list of design decisions, requirements,
caveats, and similar points. It states what the change does and why, at the level of detail a
coding agent needs to reproduce the important decisions. It does not need to describe every
detail, because reimplementation is cheap.

Collaboration matters most before implementation starts. When the author starts a Tier 3 change,
recommend opening a draft PR with only the MDD so that reviewers can comment before code exists.

Rules for the agent:

- Do not mark a PR Tier 3 unless the code matches the MDD. Before writing the tier tag, compare
  the diff to each bullet. Fix the code or stop and report each mismatch to the author.
- Do not edit or remove a bullet to match the code without the author's approval. The bullets are
  what the human reviewers approve.
- Add a bullet whenever implementation forces a decision that the MDD does not cover, or reveals
  something the MDD did not expect. Start each such bullet with `Implementation note:`.

Two humans other than the author, who are experts on the affected systems, must approve the PR.
Use the `.github/CODEOWNERS` owners of the changed paths as the expert reviewers where entries
exist. The reviewers review the MDD. Their bar is whether they agree with what they expect an AI
to build from the MDD used as a prompt. They may ask for any change to the MDD, including one that
discards the implementation. When a reviewer changes the MDD, update the code to match it.

## Step 3 — AI review

Every tier requires an AI review of the full `$BASE...HEAD` diff by `/code-review max`. Run it
locally, report each finding to the author, and wait until the author has accepted or rejected
each one. Apply the accepted fixes. Do not post the findings to the PR. `/send-pr` Step 4 performs
this review.

The review must run on a model in this list, identified by model ID:

```
claude-opus-5-5
```

If the review would run on any other model, stop and tell the author. Do not write the review tag
for a review that ran on a model outside the list.

For Tier 3, tell the reviewer to focus on these questions in addition to correctness:

- Does the implementation match the MDD in the important details?
- Is the test coverage adequate, and what kinds of bugs could pass the tests?
- What is the ecosystem impact? Does the change break other features? What rollout does it need?

Pass the full text of the MDD to the reviewer along with these questions. The MDD is in the PR
description, not in the code, so the reviewer cannot find it on its own.

For Tier 2, tell the reviewer to check that the new code is tested appropriately, and to look for
unintended consequences of the change.

## Step 4 — Write the tags

The policy uses two tags in the PR description. A tag is a single line and can go anywhere in the
description, except after the last release-notes checkbox, where nothing may follow.

The tier tag opts the PR in and states its tier:

```
[review-tier: <0|1|2|3>]
```

The review tag records that the AI review in Step 3 ran and that the author decided every
finding:

```
[🤖🔍 ai-review: /code-review max <model> <sha>]
```

`<model>` is the model ID that ran the review. `<sha>` is the full hash of the PR head commit that
the review covered, after the push. If commits
are added after the tag, review the new commits and update the tag to the new head.

Refresh the tags whenever the diff changes. If the new diff fits a higher tier, raise the tier and
meet the new tier's requirements. If the PR gains ineligible files, tell the author.
