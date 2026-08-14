<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 Lean proof scope

## Result

The Lean project checks the core safety arithmetic, the modeled GC retention
rules, the signed transaction cutoff rule, and a conditional liveness argument.
The project has no `sorry`, `admit`, or declared `axiom`.

This work is not an end-to-end proof of the Rust program. The Lean model states the
remaining Rust refinement conditions as explicit assumptions. A theorem is a valid
protocol claim only when the implementation establishes these conditions.

The [assumption ledger](ASSUMPTIONS.md) gives each condition a stable identifier,
status, refinement mapping, and discharge condition. A project with no declared
Lean `axiom` can still have conditional theorems because assumptions can be theorem
inputs.

## Safety model

The safety proof uses weighted authority sets. A Byzantine authority can count once
on each side after equivocation. It cannot count more than once on either side.

The model uses these names:

- `N` is total stake.
- `f` is the maximum Byzantine stake.
- `Q` is the direct quorum threshold.
- `A` is the indirect certificate threshold.

The proof requires these inequalities:

```text
N + f < Q + A
N + f + A <= 2Q
```

[`Thresholds.nominalHybrid`](../lean/Mysticeti/Thresholds.lean) constructs the
nominal v3 values:

```text
N = 5f + 3c + 1
Q = 4f + 2c + 1
A = 2f + c + 1
```

Lean proves both required inequalities for all natural `f` and `c`.

[`LeaderEvidence.safety`](../lean/Mysticeti/Leader.lean) proves that one leader
block cannot have both a commit result and a skip result. It covers these cases:

- direct commit against direct skip;
- direct skip against indirect commit;
- direct commit against indirect skip;
- indirect commit against indirect skip.

The direct-commit against indirect-skip case uses the second threshold inequality.
A direct quorum and an anchor quorum preserve at least `A` correct commit votes.
The proof also uses the Core GC state. The GC boundary is calculated from the last
committed leader before Core records the new commit. The next leader decision round
is greater than that last committed round. Lean proves that the decision block, its
next-round votes, and the complete path to a valid anchor are above this boundary.

[`TransactionEvidence.safety`](../lean/Mysticeti/Finalizer.lean) proves the same
four cases for one transaction. The model also specifies the v3 vote rule. A
next-round block accepts a transaction only when all these conditions are true:

- the target round is above the signed cutoff;
- the voting block references the target block;
- the voting block does not contain an explicit reject for the transaction.

Every other next-round block is a reject vote.

The model uses the numeric signed `transaction_votes_cutoff_round`. It does not use
an abstract `targetAboveCutoff` input. The modeled proposer sets the cutoff to the
maximum of these values:

- the causal-history block-GC round;
- the transaction vote-tracker GC round.

Lean proves these properties:

- A target at or below either source GC round is a reject vote.
- An accept vote is above both source GC rounds.
- A valid next-round block has a cutoff that is before or equal to the target round.

Each counted direct accept voter has a signed vote witness. The committed-prefix
proof uses the same witness for a correct authority. Thus, the direct-against-
indirect proof uses the signed cutoff classifier.

## Garbage collection and the committed prefix

[`GarbageCollection.lean`](../lean/Mysticeti/GarbageCollection.lean) models two GC
paths.

The Rust `Linearizer` type is not on the active v3 path in the reviewed combined
state. `FlexCommitter::build_commit` performs local v3 sub-DAG construction. It
reads the old GC round, selects uncommitted ancestors above that round, and returns
the commit and `CommittedSubDag`. `Core::post_commit` then records the commit and
sends the saved sub-DAG to the finalizer. For a synced certified commit,
`FlexCommitter::handle_certified_commit` reconstructs the sub-DAG from the signed
commit block list. The proof uses the term "v3 sub-DAG construction" for these two
paths.

For the leader rule, `CoreGcState` uses this boundary:

```text
gc_round = last_commit_round - gc_depth
```

The v3 schedule starts the next decision round after `last_commit_round`. Lean
therefore proves that all leader evidence from the decision round through the
depth-two anchor is retained when Core makes the decision.

For transaction finalization, the target block can be far below its commit leader.
The proof uses two cases:

1. If the first commit leader is at least two rounds above the target,
   `FlexCommitter::build_commit` preserves the target's next-round voting blocks in
   the local path.
2. If the target is near the first commit leader, the first depth-two trigger
   preserves the voting blocks. The preceding trigger commit is still below the
   depth-two boundary. With `gc_depth > 2`, its GC boundary is below the voting
   round.

[`firstEligible_predecessor`](../lean/Mysticeti/CommitChain.lean) proves the
preceding-commit bound from the first-trigger rule.
[`TransactionGcWindow.voting_round_retained`](../lean/Mysticeti/GarbageCollection.lean)
proves the two-case GC arithmetic.

The model also separates live DAG evidence from the finalizer's buffered committed
prefix. Block GC can change the live DAG store. It cannot change evidence that the
finalizer already copied into the pending committed prefix.

This result does not prove that every Rust input path copies the required anchor
voting blocks into that prefix. `anchorInCommittedPrefix` is still a Rust refinement
obligation. It covers local `FlexCommitter::build_commit`, certified commit
reconstruction, replay, and recovery. The proof also does not give a direct-decision
liveness guarantee when a slow finalizer loses live cached blocks. The indirect
buffered-prefix path must provide progress in that case.

## Common commit chain

Indirect rejection is not monotone for an arbitrary larger prefix. A later prefix
can contain a new accept certificate. The theorem
[`arbitrary_prefix_decision_can_flip`](../lean/Mysticeti/CommitChain.lean) gives a
small checked example of this fact.

The safe rule uses the first eligible depth-two trigger in one continuous common
commit stream. Lean proves these properties:

- the first eligible trigger is unique;
- it stays the first trigger when the visible prefix grows;
- two nodes with the same stream and trigger make the same indirect decision.

This is the proof boundary for `CommitFinalizerV3`. A proof that only compares two
arbitrary commit prefixes is not sufficient.

## Liveness model

[`PartialSynchrony`](../lean/Mysticeti/PartialSynchrony.lean) specifies standard
partial synchrony. GST is unknown. Before GST, a message can have an arbitrary
delay. After GST, each authenticated protocol message between correct processes is
delivered within `delta`.

The model also has a catch-up activation time. Strong liveness starts after both
GST and this activation time.

[`ConsensusLivenessAssumptions`](../lean/Mysticeti/Liveness.lean) states the Rust
refinement obligations for the progress checkpoints. It requires:

- safe intermediate proposals during a round jump;
- the eventual selection of a live correct leader;
- bounded proposal, supporter, certificate, decision, and commit steps;
- continued operation of the protocol tasks.

Lean proves these results:

- `good_window_commits_within`: the modeled network steps finish within
  `10 * delta` after a good leader window starts;
- `consensus_liveness`: a post-activation open round eventually produces a commit;
- `finalizer_liveness`: a pending transaction on a continuous commit stream
  eventually gets a durable decision;
- `transaction_liveness`: the consensus and finalizer results compose.

The `10 * delta` value is a model bound. It is not a measured Rust latency bound.
The Rust timers and pipeline can use smaller or larger constants. A later refinement
proof must map each checkpoint to the exact code timer and message path.

## Checked implementation counterexample

[`direct_jump_can_violate_safe_catchup`](../lean/Mysticeti/PartialSynchrony.lean)
proves a local counterexample. If a node jumps to one future round and proposes only
in that round, it can omit a required intermediate proposal. Partial synchrony does
not repair this omission.

This result matches the round-jump risk in
[`ThresholdClock::add_block`](../../core/src/threshold_clock.rs). It does not by
itself prove the full published infinite attack for the new v3 code. It proves that
the present catch-up transition cannot establish the safe round-change assumption
used by this liveness theorem.

## Rust refinement conditions

The safety result needs all these implementation facts:

1. [`ASM-SAFE-PARAMETERS`](ASSUMPTIONS.md#asm-safe-parameters): all correct nodes use
   the same epoch committee and the same `N`, `f`, `Q`, `A`, and `gc_depth`
   values.
2. [`ASM-SAFE-FAULT-BOUND`](ASSUMPTIONS.md#asm-safe-fault-bound): Byzantine stake is
   at most `f`.
3. [`ASM-SAFE-AUTHENTICATION`](ASSUMPTIONS.md#asm-safe-authentication): signatures
   bind the authority, round, block contents, cutoff, and votes.
4. [`ASM-SAFE-NON-EQUIVOCATION`](ASSUMPTIONS.md#asm-safe-non-equivocation): a correct
   authority does not produce incompatible votes.
5. [`ASM-SAFE-PARENT-QUORUM`](ASSUMPTIONS.md#asm-safe-parent-quorum): every accepted
   non-genesis block has `Q` immediate-parent stake.
6. [`ASM-SAFE-EVIDENCE-REFINEMENT`](ASSUMPTIONS.md#asm-safe-evidence-refinement): the
   Rust decision rules create the same evidence and outcomes as the Lean model.
7. [`ASM-SAFE-COMMITTED-PREFIX`](ASSUMPTIONS.md#asm-safe-committed-prefix): the
   committed prefix contains the causal evidence used by the indirect rules.
8. [`ASM-SAFE-COMMIT-CHAIN`](ASSUMPTIONS.md#asm-safe-commit-chain): correct nodes
   process one certified commit chain with no gap.
9. [`ASM-SAFE-FIRST-TRIGGER`](ASSUMPTIONS.md#asm-safe-first-trigger): correct nodes
   use the same first depth-two trigger.
10. [`ASM-SAFE-GC`](ASSUMPTIONS.md#asm-safe-gc): Core uses the pre-commit GC
    boundary, the signed transaction cutoff is the maximum of both proposer-side
    GC rounds, and v3 sub-DAG construction copies required voting blocks before
    later DAG GC.

The liveness result also needs these facts:

1. [`ASM-LIVE-PARTIAL-SYNCHRONY`](ASSUMPTIONS.md#asm-live-partial-synchrony): the
   standard partial synchrony condition holds.
2. [`ASM-LIVE-ROUND-CATCHUP`](ASSUMPTIONS.md#asm-live-round-catchup): round catch-up
   follows the safe intermediate-proposal rule after activation.
3. [`ASM-LIVE-LEADER`](ASSUMPTIONS.md#asm-live-leader): the leader schedule
   eventually selects a live correct leader.
4. [`ASM-LIVE-BLOCK-SYNC`](ASSUMPTIONS.md#asm-live-block-sync): block synchronization
   eventually resolves each required missing block.
5. [`ASM-LIVE-COMMIT-SYNC`](ASSUMPTIONS.md#asm-live-commit-sync): commit
   synchronization eventually extends the certified commit stream.
6. [`ASM-LIVE-PEER-FAIRNESS`](ASSUMPTIONS.md#asm-live-peer-fairness): a correct peer
   retains the required data and is eventually selected.
7. [`ASM-LIVE-TASK-FAIRNESS`](ASSUMPTIONS.md#asm-live-task-fairness): correct protocol
   tasks and consumers continue to make progress.
8. [`ASM-LIVE-PIPELINE-BOUNDS`](ASSUMPTIONS.md#asm-live-pipeline-bounds): each modeled
   phase completes within its stated bound.
9. [`ASM-LIVE-FINALIZER-TRIGGER`](ASSUMPTIONS.md#asm-live-finalizer-trigger): a pending
   transaction receives a later depth-two trigger.
10. [`ASM-LIVE-DURABILITY`](ASSUMPTIONS.md#asm-live-durability): a decision becomes
    durable and reaches the consumer.

The [gap report](IMPLEMENTATION_GAPS.md) gives the code changes that are needed to
discharge these proof obligations.
