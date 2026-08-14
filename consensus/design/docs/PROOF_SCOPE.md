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
   the same epoch committee and the same `N`, `f`, `Q`, and `A` values.
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
10. [`ASM-SAFE-GC`](ASSUMPTIONS.md#asm-safe-gc): garbage collection does not remove
    required evidence before the trigger uses it.

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
