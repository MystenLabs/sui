<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Spec obligation check

This document is a design for a workflow job. The job reports which proof
obligations a product change puts at risk. Nothing in this document is
implemented.

## Problem

The Lean specification does not read the product source. `lake build` stays
green for every change to `consensus/core`. The specification workflow also does
not run for such a change, because it selects the `consensus/spec/**` paths.
`check-assumption-ledger.sh` compares the documents against each other, not
against Rust.

The only current link is prose. Each evidence entry in
[assumption evidence](../docs/ASSUMPTION_EVIDENCE.md) has a **Revalidation
triggers** line that names Rust items. A reader must know that the line exists
and must look at it.

One example shows the cost. The adaptive-schedule fixpoint proof uses three
assertions in `FlexCommitter::maybe_refresh_pending_commit_state`. If a change
removes an assertion, the stratification that makes the schedule recursion well
founded is gone. Every Lean target still builds, and no check reports it.

## What the job does

The job runs on a pull request. It has three steps.

1. Collect the changed Rust items. Read the diff against the merge base. For
   each changed hunk, walk up to the enclosing `fn` or `impl` item.
2. Look up each item in a checked-in map from Rust items to proof obligations.
3. Report the obligations that the change puts at risk.

The job does not compile Lean and does not compile Rust. It reads a diff and a
map. One run takes seconds.

## The map

The map is one checked-in file. Each entry names one Rust item, the review rows
that depend on it, the Lean results that depend on it, and one sentence that
says what breaks.

```
consensus/core/src/flex_committer.rs::maybe_refresh_pending_commit_state
  rows:     REF-V3-SCHEDULE-SCORER, REF-V3-SCHEDULE-READERS
  results:  V3ScheduleGateSequence.governing_commits_are_below_round,
            V3AdaptiveScheduleRule.adaptive_run_unique
  breaks:   The three gate assertions give the schedule stratification. Without
            them the schedule recursion is not well founded.
```

Keep the map small. The documents name 208 backticked identifiers. Fifty-seven
of them exist as a function or a structure in `consensus/core`. The map must
contain only the items that a proof depends on, which is about twenty entries.
A large map reports items that no proof uses, and a report that is usually
irrelevant is not read.

## Cost and noise

Measurements over the last 200 commits of this branch:

| Selection | Commits | Share |
|---|---:|---:|
| Touch `consensus/core/src` | 9 | 4.5% |
| Touch a proof-critical file | 1 | 0.5% |

A file-level map fires about once in 200 commits. An item-level map fires less
often. The risk is not that the job is noisy. The risk is that the job is quiet
for so long that a reader does not know what the report means. The report must
therefore state the consequence, not only the identifier.

Three rules keep the report accurate.

- Use item granularity. `leader_schedule_v3.rs` has about 700 lines, and two
  functions matter.
- Ignore a hunk that changes only whitespace or only comments. Use a
  whitespace-insensitive diff and drop a hunk whose changed lines are all
  comment lines.
- Report, do not block. A check that fails for a comment edit is bypassed by
  habit, and then it is worse than no check.

## Staleness

Noise is not the main failure. Staleness is. If a change renames
`maybe_refresh_pending_commit_state`, the map stops matching, the job reports
nothing, and silence reads as "no risk".

The job must therefore also check that every Rust item in the map still exists,
and must fail when one does not. That check is a search over the source. It
catches a rename and a deletion, which are the changes that are most likely to
break a proof.

This check fails the build. It reports a broken map, not a risky change.

## Open decision

Whether the risk report blocks a merge.

The recommendation is to report only at first. The job fires for about one
commit in 200, so the first person to see a blocking failure has no context for
it.

If the report must block, add an acknowledgement token in the pull request body,
such as `spec-reviewed: REF-V3-SCHEDULE-SCORER`. The token clears the specific
row. This keeps the action cheap and leaves a record.

## What the job does not do

- It does not prove that a change is safe. It states which obligations need a
  human review.
- It does not detect a semantic change that keeps the item name. A change inside
  a mapped item always reports, and a reviewer decides.
- It does not replace the conformance tests that would encode an assumed Rust
  fact as a test. That is separate work with a stronger guarantee.

## Related work

The workflow currently selects the `consensus/spec/**` paths. It must also
select `consensus/core/**` before this job can run for a product change.
