<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 Lean proof scope

## Result

The Lean project checks the core safety arithmetic and a conditional liveness
argument. The project has no `sorry`, `admit`, or declared `axiom`.

This work is not an end-to-end proof of the Rust program. The Lean model states the
remaining Rust refinement conditions as explicit assumptions. A theorem is a valid
protocol claim only when the implementation establishes these conditions.

## Safety model

The safety proof uses weighted authority sets. An authority can count one time on
each side after equivocation. It cannot count two times on one side.

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

[`TransactionEvidence.safety`](../lean/Mysticeti/Finalizer.lean) proves the same
four cases for one transaction. The model also specifies the v3 vote rule. A
next-round block accepts a transaction only when all these conditions are true:

- the target round is above the signed cutoff;
- the voting block references the target block;
- the voting block does not contain an explicit reject for the transaction.

Every other next-round block is a reject vote.

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
- an eventual live correct leader opportunity;
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

1. All correct nodes use the same epoch committee and the same `N`, `f`, `Q`, and
   `A` values.
2. Byzantine stake is at most `f`.
3. Signatures bind an authority, round, block contents, cutoff, and transaction
   votes.
4. A correct authority does not equivocate.
5. Every accepted non-genesis block has `Q` immediate-parent stake.
6. The DAG keeps the causal evidence that the indirect rules use.
7. Correct nodes process one continuous common commit chain.
8. Correct nodes use the same first depth-two trigger.
9. Garbage collection does not remove evidence before the trigger can use it.

The liveness result also needs these facts:

1. The standard partial synchrony condition holds.
2. Round catch-up follows the safe intermediate-proposal rule after activation.
3. The allowed leader set has a live correct leader opportunity.
4. Correct proposers, synchronizers, committers, and finalizers continue to run.
5. Commit synchronization eventually extends the continuous common stream.
6. A pending transaction receives a later depth-two trigger.

The gap report gives the code changes that are needed to establish these facts.
