<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 implementation gaps

This report applies to the synthetic combined state of PR 27505 and PR 27655. It
uses the PR heads that are listed in the parent README. This branch contains only
the formal artifacts on top of `origin/main`.

The [assumption ledger](ASSUMPTIONS.md) defines the stable identifiers used in this
report. A gap closes only when its related proof obligation is discharged or its
environmental assumption is explicitly accepted.

## P0: implement and test a safe round-jump rule

Related assumption: `ASM-LIVE-ROUND-CATCHUP`.

This is a confirmed proof gap and an activation blocker. The current transition
does not establish the safe catch-up condition that the liveness proof needs.

[`ThresholdClock::add_block`](../../core/src/threshold_clock.rs) accepts one block
from a future round. It clears the old aggregator and moves the local clock to that
future round or the next round. It does not request proposals for the omitted
intermediate rounds.

[`Proposer::try_propose`](../../core/src/proposer.rs) creates a block for the current
clock round. It does not create the required intermediate blocks first.

Standard partial synchrony does not establish this missing condition. The 2026
mechanized Mysticeti analysis gives an infinite counterexample for original
Mysticeti and a
[modified round-jump rule](https://www.cs.yale.edu/flint/certikos/publications/sp26.pdf).
That paper studies an older protocol and implementation. This work does not claim
that its complete infinite trace applies unchanged to the multi-leader v3 rules.
However, the same direct-jump mechanism is present, and the Lean counterexample
shows that the v3 code cannot satisfy the stated liveness contract without another
rule.

The implementation must do this after the v3 catch-up rule is active:

1. Update leader decisions from the current DAG.
2. For each intermediate round `r`, check the decision for round `r - 2`.
3. If that decision is not final, create and persist a block in round `r`.
4. Create the block in the observed future round only after this loop.

The implementation must keep the immediate-parent quorum rule for each generated
block. It must also prevent an unbounded block burst. A protocol rule can skip an
intermediate block only when the related old leader decision is final and can be
shared with other nodes.

Add a deterministic simulation test for the published round-jump trace. The test
must run with v3, FlexCommitter, and the v3 leader schedule active.

## P0: put v3 activation in epoch protocol state

Related assumption: `ASM-CONFIG-V3-ACTIVATION`.

This is a confirmed activation gap.

[`to_consensus_protocol_config`](../../../crates/sui-core/src/consensus_manager/mod.rs)
sets `enable_v3` to `false`. Therefore, normal Sui startup does not use the new
FlexCommitter and finalizer code.

Add a versioned `ProtocolConfig` field. Use an epoch-bound activation value. Add a
rollback plan and mixed-version tests. Do not use a node-local flag for activation.

## P0: remove node-local threshold inputs

Related assumptions: `ASM-MATH-THRESHOLDS`, `ASM-SAFE-PARAMETERS`, and
`ASM-REFINE-INTEGERS`.

This is a confirmed safety boundary.

[`apply_v3_threshold_overrides`](../../../crates/sui-core/src/consensus_manager/mod.rs)
reads `SUI_CONSENSUS_V3_MALICIOUS_STAKE` and
`SUI_CONSENSUS_V3_CRASH_STAKE` from each process environment. Two correct nodes can
therefore construct different `Q`, `A`, and validity thresholds for the same epoch.

Move `f` and `c`, or all derived thresholds, into authenticated epoch state. Every
correct node must derive the same values. Include the values in diagnostics and in
the protocol compatibility checks.

Use checked arithmetic in
[`Committee::new_v3`](../../config/src/committee.rs). In particular, check the
`5 * f + 3 * c` calculation and all threshold additions and multiplications before
v3 activation.

## P0: make v3 and transaction voting one valid configuration

Related assumptions: `ASM-CONFIG-VOTING` and `ASM-SAFE-EVIDENCE-REFINEMENT`.

[`CommitFinalizerV3::run`](../../core/src/commit_finalizer_v3.rs) bypasses voting
when `transaction_voting_enabled` is false. The current Sui conversion sets this
value to true. A future configuration change can still enable v3 without the vote
semantics that the safety proof uses.

Add a constructor check for this condition:

```text
enable_v3 implies transaction_voting_enabled
```

A single versioned v3 feature value is safer than two independent values.

## P1: establish the allowed-leader liveness condition

Related assumption: `ASM-LIVE-LEADER`.

[`LeaderScheduleV3::select_allowed_leaders_with_fixed_config`](../../core/src/leader_schedule_v3.rs)
can remove low-score stake. The Lean liveness theorem requires the schedule to
eventually select a live correct leader from the remaining set.

Add a runtime and protocol-config condition that connects these values:

- maximum excluded stake;
- Byzantine stake `f`;
- crash stake `c`;
- required live stake for proposal and certificate progress.

Prove that the allowed set cannot contain only Byzantine or crashed authorities.
Also require a positive schedule window and update interval. Add property tests for
all boundary values.

## P1: specify block-sync and commit-sync liveness

Related assumptions: `ASM-LIVE-BLOCK-SYNC`, `ASM-LIVE-COMMIT-SYNC`,
`ASM-LIVE-PEER-FAIRNESS`, `ASM-LIVE-TASK-FAIRNESS`, and
`ASM-LIVE-PIPELINE-BOUNDS`.

The Lean model does not represent missing DAG blocks, suspended blocks, peers,
request retries, commit ranges, or consumer backpressure. It assumes the progress
that these mechanisms must provide.

The implementation has important recovery mechanisms:

- `BlockManager` suspends a block until its required ancestors are accepted.
- `Synchronizer` uses direct fetches, periodic fetches, and stored-history fetches.
- Periodic block sync resumes when commit sync does not make commit progress.
- `CommitSyncer` retries commit ranges, verifies the commit chain and certificate,
  buffers ranges across gaps, and sends consecutive ranges to Core.

These mechanisms do not by themselves prove liveness. Add explicit contracts for
these conditions:

1. A correct known peer retains each required block and certified commit for the
   required recovery period.
2. Peer discovery eventually provides such a peer.
3. Retry selection is fair. If selection remains random, use a probabilistic model
   and prove almost-sure progress.
4. Correct protocol tasks continue to run, and sustained consumer backpressure
   eventually clears.
5. A missing block above the GC boundary is eventually accepted, or a certified
   commit makes it unnecessary.
6. Commit sync and the live block path together process the trailing partial batch.

Add separate Lean state and progress theorems for block sync and commit sync. Use an
eventual-progress property unless a bound includes the exact scheduler periods,
timeouts, retry delays, storage time, and processing time. Do not derive this bound
from the network `delta` alone.

For the implementation, replace an empty-peer assertion with wait-and-retry behavior
where an empty peer set is valid. Use deterministic fair peer rotation, or document
the random selection model. Add tests for peer loss, data retention at the GC
boundary, commit-sync stall fallback, incomplete commit batches, and consumer
backpressure.

## P1: define finalizer tail behavior at shutdown

Related assumptions: `ASM-LIVE-FINALIZER-TRIGGER` and `ASM-LIVE-DURABILITY`.

[`CommitFinalizerHandle::stop`](../../core/src/commit_finalizer.rs) closes the input
channel and waits for the task. The task drains received commits. However,
[`CommitFinalizerV3`](../../core/src/commit_finalizer_v3.rs) can still contain
pending commits that need a later depth-two trigger.

Same-epoch recovery can replay unfinalized commits from storage. This does not by
itself define the epoch-end result for the last pending commits.

Add one explicit rule:

- keep consensus active until every accepted commit has its required trigger; or
- persist and replay the pending finalizer state across the epoch boundary; or
- define and prove a safe epoch-tail decision rule.

Add a shutdown test where the last input commit contains a pending transaction and
has no depth-two successor.

## P1: make the common commit-chain contract explicit

Related assumptions: `ASM-SAFE-COMMIT-CHAIN`, `ASM-SAFE-FIRST-TRIGGER`, and
`ASM-LIVE-COMMIT-SYNC`.

The indirect safety proof requires one continuous common commit stream and the same
first eligible trigger. The implementation has useful checks:

- the finalizer checks consecutive commit indices;
- the v3 certified-commit path checks the previous digest;
- recovery checks for an index gap.

Complete the refinement proof for local commits, certified commits, commit sync,
recovery, and garbage collection. Add one invariant helper that validates the index,
previous digest, and trigger order at every input boundary. Use the helper in tests
and in debug builds.

This item is a proof-closure gap. The current review did not find a concrete fork in
the combined branch.

## P1: prove the committed-prefix and garbage-collection lemma

Related assumptions: `ASM-SAFE-COMMITTED-PREFIX`, `ASM-SAFE-GC`, and
`ASM-SAFE-PARENT-QUORUM`.

The transaction indirect rule counts accept voters from the complete buffered
commit prefix. This is necessary because one v3 commit can have more than one
leader. The rule rejects when that prefix has less than certificate stake.

Lean now checks the GC arithmetic in the protocol model:

- Core calculates `gc_round = last_commit_round - gc_depth` before it records the
  new commit.
- The next v3 leader decision round is after `last_commit_round`. Thus, the leader
  decision, next-round votes, and anchor path are above the old GC boundary.
- A transaction voting block uses the signed numeric cutoff. The cutoff is the
  maximum of the causal-history block-GC round and the vote-tracker GC round.
- A target at or below either proposer-side GC boundary is a reject vote.
- For a target far below its commit leader, the first commit preserves next-round
  evidence. For a target near its commit leader, the first trigger preserves it.
  The second case uses `gc_depth > 2` and the commit before the first trigger.
- Later DAG GC does not change the modeled pending committed-prefix store.

The proof still needs these implementation facts:

1. A correct accept voter cannot commit before its target block.
2. A depth-two leader has a verified immediate-parent quorum.
3. `FlexCommitter::build_commit` includes the target only above the GC boundary
   that it read before `Core::post_commit` records the new commit.
4. The complete anchor causal history has a quorum of voting-round blocks, not only
   a quorum of immediate parents at the anchor round.
5. Local `FlexCommitter::build_commit` and certified
   `FlexCommitter::handle_certified_commit` include each required accept voter in
   the exact `CommittedSubDag` sequence before the first trigger.
6. Commit sync, replay, and recovery produce the same prefix and first trigger.
7. A slow finalizer keeps the blocks in its pending `CommittedSubDag` values after
   the live DAG cache removes those rounds.

The constructor checks `gc_depth > 2`. The block verifier checks immediate-parent
quorum stake. The proposer signs both GC sources through one maximum cutoff. These
checks discharge only the arithmetic and local vote-classification parts. Add an
integration invariant that covers both v3 sub-DAG paths, commit-sync recovery, the
pending finalizer prefix, and the exact GC boundary. This item is still a
proof-closure gap.

The `CommitFinalizerV3` constructor comment says that the proof depends on
`Linearizer::linearize_sub_dag`. This statement is stale for v3. Replace it with the
`FlexCommitter::build_commit` and `Core::post_commit` ordering contract. Add a
separate statement for `FlexCommitter::handle_certified_commit`.

`prepare_direct_voting_blocks` reads live cached blocks. A slow finalizer can lose a
direct-decision opportunity after DAG GC. This loss does not make a false quorum,
but it affects liveness. Test that the buffered indirect path completes after this
event.

## P2: close the natural-number to Rust-integer refinement

Related assumption: `ASM-REFINE-INTEGERS`.

Lean uses unbounded natural numbers. Rust uses `u32`, `u64`, `u128`, `u16`, and
`usize` in this path.

Document and check these bounds:

- the epoch cannot approach the maximum round or commit index;
- stake sums and threshold products cannot overflow;
- the maximum transaction count stays below the reserved `TransactionIndex` range;
- schedule counters and score products cannot overflow.

Reuse the Lean threshold equations in Rust property tests. Run the tests over small
exhaustive values and large boundary values.

## P2: add a Rust-to-Lean conformance suite

Related assumptions: `ASM-SAFE-AUTHENTICATION`, `ASM-SAFE-NON-EQUIVOCATION`, and
`ASM-SAFE-EVIDENCE-REFINEMENT`.

The Lean model is hand written. It does not yet prove that the Rust decision
functions implement the same relation.

Export small test vectors from Lean or duplicate the executable Lean functions in a
stable data format. Check these Rust functions against the vectors:

- `LeaderSlotDecider::try_direct_decide`;
- `LeaderSlotDecider::try_indirect_decide`;
- `CommitFinalizerV3::compute_direct_decisions`;
- `CommitFinalizerV3::compute_indirect_decisions`;
- the first depth-two trigger selection;
- the v3 transaction cutoff rule, including both source GC rounds;
- the deep-target and near-target v3 sub-DAG GC cases.

Include equivocation vectors. One Byzantine authority can count once on each side,
but it cannot count more than once on either side.
