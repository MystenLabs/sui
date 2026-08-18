<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Flex committer execution

This document states the local Mysticeti v3 Flex committer control flow. It
also states the Lean refinement boundary in
`ValidatorFlexPendingRefresh.lean`.

The rules in this document are local execution and source-refinement rules.
They are not liveness assumptions. They do not require a future run, block,
leader window, or commit.

## Pending state before a scan

The Rust pending state contains:

- the commit head that created the cache;
- the minimum leader round for that head;
- the ordered leader-schedule key; and
- a finite consecutive list of pending rounds and their decisions.

Each actual `runCommitter` call prepares its input from the action-before local
state. The preparation has these steps:

1. Read the current local commit head and the current highest accepted DAG
   round.
2. Refresh the cached pending state.
3. Append an undecided entry for each missing round in the half-open interval
   from the current minimum leader round to the highest accepted DAG round.
4. Run the direct pass.
5. If the direct pass has no candidate, run the indirect pass.
6. Store the complete post-scan cache, including decisions made by an
   unsuccessful scan.

The refresh has three cases:

1. If the commit index is unchanged, Rust requires the same exact head,
   minimum round, and ordered schedule. It keeps the cache.
2. If the commit index advanced and the ordered schedule is unchanged, Rust
   drops all cached rounds below the new minimum.
3. If the commit index advanced and the ordered schedule changed, Rust clears
   the cache.

The prepared list is consecutive. Its exact length is:

```text
highest accepted DAG round - current minimum leader round
```

All retained and appended slots use the current ordered leader selection. A
schedule key alone is not sufficient evidence for this rule.

## Commit consumption

`build_commit` does not remove the processed prefix. A successful scan first
returns a commit candidate. The matching local `recordCommit` operation then
installs its new head. The next actual `runCommitter` refresh uses that head and
drops the processed prefix.

The Lean model uses this next-run-boundary abstraction. The theorem
`later_actual_run_drops_recorded_frontier` is conditional on an actual later
run and on an exact matching `recordCommit` action before that run. “Before”
includes an earlier event in the same logical-time batch. The theorem does not
assert that the later run exists.

## Conditional fixed-frontier rank

For one fixed accepted DAG frontier, define the rank as:

```text
highest accepted DAG round - current minimum leader round
```

A successful scan selects a candidate inside this finite interval. After its
matching local record operation, an actual later scan starts strictly after
that candidate. Therefore, if the accepted DAG frontier did not change, the
later rank is strictly smaller.

`fixed_frontier_recorded_success_decreases_prepared_rank` proves the one-step
decrease. `fixed_frontier_commit_chain_length_le_prepared_rank` proves that an
already observed fixed-frontier chain has at most its initial rank many
recorded-success transitions. These theorems bound only an already observed
chain whose accepted DAG frontier stays fixed. They do not prove that one
complete Core call reaches its terminal `none` result.

Rust processes one external input in one synchronous handler. When
`Core::add_blocks` accepts at least one input block, Core repeats local commit
processing until the scan returns `none`. It then calls `try_propose(false)`.
Another external batch cannot interleave with this loop. However, `post_commit`
can accept blocks from the finite suspended-block store and increase the
accepted frontier inside the loop. Therefore, the fixed-frontier rank does not
prove termination of the complete loop.

`ValidatorCoreHandlerRefinementRules` states the approved local handler
boundary. `ValidatorCoreHandlerInputObservation` identifies one qualifying
external `add_blocks` batch with at least one accepted ordinary block. It does
not classify every internal accept action as a new handler.
`ValidatorPacketDrivenBlockAcceptanceAt` records exact past packet delivery and
acceptance. `packetDrivenAcceptanceHasInputOrigin` maps that acceptance to its
exact qualifying handler input. This field is a proposed local refinement. The
Rust mapping must distinguish direct input acceptance from later
GC-unsuspension. A GC-unsuspended block uses its enclosing handler episode; it
does not start a second handler.
`ValidatorFiniteCoreHandlerEpisode` covers the finite Core-owned store at
handler entry, cleanup-driven block unsuspension, each successful scan and
`post_commit`, the terminal `none`, and the following `try_propose(false)`
invocation before return. This is a past and current control-flow boundary.
`qualifying_core_handler_input_has_terminal_scan_and_normal_proposal_attempt`
exposes this order for one qualifying input. It does not say that the proposal
attempt succeeds. It does not return a produced block, a future commit, or
future DAG progress. The Rust-to-Lean source refinement for these rules is
still open. Certified-commit processing is outside this positive DAG handler
record and needs a separate model.

## Role in the liveness proof

The liveness proof uses two stages. It first proves unbounded post-GST quorum
block production and common DAG acceptance. It then derives commit progress
from that growing DAG and favorable leader windows.

`ValidatorCoreHandlerRefinementRules` treats commit processing in one qualifying
external handler as a finite interruption in the first stage. It is not an
alternative result that can discharge network DAG progress. The fixed-frontier
result covers one important case. The complete-handler interface also covers
finite suspended input and returns the proposal-attempt invocation.

The pure DAG stage does not inspect a commit result to obtain its block traffic.
Each covered finite commit-processing handler invokes the ordinary proposal
attempt before return. Protected proposal work must remain fairly scheduled
across handlers. Proposal, persistence, send, delivery, and acceptance rules
then prove DAG growth.

`ValidatorFlexScanEvidence.lean` and
`ValidatorFlexRemoteCommitTraffic.lean` remain useful local audit results for
commit evidence. They are not the source of network DAG liveness.

This stage does not use `applySyncedCommit`, future commit synchronization, or
a future successful FlexCommitter run. An actual synchronized install remains
subject to safety and state-rebase rules, but it is not a positive liveness
step.

## Evidence origin

Each scan has a finite exact list of accepted references and catalogued block
bodies at its action-before state. Each accepted body has one of these local
origins:

- exact finite durable support at trace time zero;
- an earlier or same-batch local proposal-persistence action;
- an earlier or same-batch local acceptance action; or
- an earlier or same-batch authenticated ordinary-block delivery.

Delivery to one observer is not evidence of traffic from that observer. For a
verified block with a correct author, signature authentication and correct
non-equivocation identify the author's exact durable own block. Reverse trace
induction then gives either the finite time-zero author support or the author's
exact earlier proposal-persistence action.

## Current refinement limits

The source map has an exact deterministic internal prepared input. The existing
runtime observation does not return this internal input. It only returns the
scan result. Therefore,
`actualRunResultReconstructsFromInternalInput` is an extensional result
refinement, not a claim that the runtime observation exposes literal input
equality.

The Rust status cache records `Direct` or `Indirect`, but it does not record the
exact indirect anchor. Exact retained indirect-anchor provenance needs either
an added Rust anchor field or a checked same-host reconstruction.

The remaining DAG composition must connect the proposal-attempt invocation to
the existing proposal guards and protected proposal work. It must then
aggregate ordinary proposal bodies into aligned same-round quorum layers. The
handler interface does not prove that an attempt succeeds. Its attempt input is
the handler-exit state.
`ValidatorFiniteCoreHandlerEpisode.proposal_attempt_input_and_suffix` exposes
that state and the remaining finite event suffix to the next trace state. The
one-host `ValidatorCoreProposalAttemptContinuationRules` classifies the
already-actual attempt as an exact proposal action in that suffix, exact
protected normal work, or current durable proposal, parent-need, or timer work.
`qualifying_core_handler_input_has_current_proposal_continuation` exposes this
split only for an actual qualifying handler input. The remaining composition
must run the current protected branch through the existing execution rules. It
must derive success without a commit-result branch. The Rust source mapping and
the distributed composition remain gaps, not future-action inputs.

The literal prepared-input identity, retained indirect-decision provenance,
the Rust mapping for `ValidatorCoreHandlerRefinementRules`, and the internal
`try_propose(false)` observation are implementation-refinement gaps. They are
not permissions to add future commit or proposal results to the liveness
theorem inputs.
