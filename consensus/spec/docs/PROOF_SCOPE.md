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

In the current `consensus/` tree, the leader-rule model maps to the
`FlexCommitter` path in `Core`. The proposer does not create `BlockV3`, and the
tree does not contain the modeled v3 transaction finalizer. Thus, the transaction
cutoff and transaction-finalization results do not yet have a current Rust mapping.

The [assumption ledger](ASSUMPTIONS.md) gives each condition a stable identifier,
status, refinement mapping, and discharge condition. A project with no declared
Lean `axiom` can still have conditional theorems because assumptions can be theorem
inputs.

## Shared proof assumptions

The proof suite uses one global
[assumption catalog](ASSUMPTIONS.md#shared-proof-model). Commit progress recovery
does not own the network, fault, runtime, storage, configuration, or leader
conditions. Safety and liveness theorems use the applicable parts of the same
catalog.

The standard conditions are bounded Byzantine stake, authenticated votes,
post-GST message delivery, and weak task fairness. The separate unavailable-stake
budget `c` is part of the v3 hybrid fault model.

These conditions are stronger or less standard:

- local consensus actions have a positive finite bound `epsilon`, with
  `epsilon < delta`, when a bounded liveness result needs it;
- each pending round's complete leader-slot order uses an accepted independent
  uniform permutation model.

All correct validators still compute the same deterministic leader-slot order. The
probabilistic statement describes its distribution for liveness analysis. The
leader schedule bounds `f + c < S` and `A <= P_r` are configuration and protocol
conditions. They are not environment assumptions.

Useful-peer retention is conditional. Steady-state consensus does not need it for
new messages that arrive under partial synchrony. A lagging or restarted validator
needs either an available peer for old consensus blocks and commits or verified
commit sync that moves it past those items. Transaction payloads can be resubmitted
and are not part of this condition.

## Safety model

The safety proof uses weighted authority sets. A Byzantine authority can count once
on each side after equivocation. It cannot count more than once on either side.

The model uses these names:

- `N` is actual validator set stake.
- `f` is the maximum Byzantine stake.
- `Q` is the direct quorum threshold.
- `A` is the indirect certification threshold.

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

Lean proves both required inequalities for all natural `f` and `c`. This is a
nominal instance. The main safety and recovery structures use actual set weights
and actual threshold values. Rust `Committee::new_v3` can scale `f` and `c` to an
actual `N` that is not `5f + 3c + 1`. It sets `A = 2f + c + 1` and
`Q = N - f - c`, then checks the two inequalities.

[`LeaderEvidence.safety`](../lean/Mysticeti/Leader.lean) proves that one leader
block cannot have both a commit result and a skip result. It covers these cases:

- direct commit against direct skip;
- direct skip against indirect commit;
- direct commit against indirect skip;
- indirect commit against indirect skip.

This is a per-slot threshold result for one fixed, common selected leader slot.
Global commit safety also requires correct validators to derive the same
leader schedule version, round leader selection, and selected leader slot order
from the common commit chain.

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

GC does not require permanent transaction retention. A validator or user can
resubmit a transaction. Its safety role is different: deleting consensus evidence
must not make two validators decide the same leader slot or transaction
differently. Data can be deleted after the decision rule no longer needs it, or
after the required evidence has moved into the committed prefix. For liveness, old
data availability matters only when a lagging or restarted validator still needs
that consensus state.

The Rust `Linearizer` type is not on the current v3 path.
`FlexCommitter::build_commit` performs local v3 sub-DAG construction. It
reads the old GC round, selects uncommitted ancestors above that round, and returns
the commit and `CommittedSubDag`. `Core::post_commit` then records the commit and
sends the saved sub-DAG to the finalizer. For a commit installed through commit
sync, `FlexCommitter::handle_certified_commit` reconstructs the sub-DAG from the
explicit block list in `CertifiedCommit`. The proof uses the term "v3 sub-DAG
construction" for these two paths.

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

This is a model property. The current proposer does not produce the signed v3
cutoff, and the current `CommitFinalizer` uses a different transaction rule. The
transaction GC theorem therefore does not make a claim about current Rust behavior.

This result does not prove that every Rust input path copies the required anchor
voting blocks into that prefix. `anchorInCommittedPrefix` is still a Rust refinement
obligation. It covers local `FlexCommitter::build_commit`, commit-sync
reconstruction, replay, and recovery. The proof also does not give a direct-decision
liveness guarantee when a slow finalizer loses live cached blocks. The indirect
buffered-prefix path must provide progress in that case.

## Common commit chain

In this model, a commit is the normal output of the commit rule. Core can produce
it locally or install it through commit sync. `CertifiedCommit` is specific to the
commit-sync path: it carries a synchronized commit and the blocks needed to
reconstruct its sub-DAG after the syncer verifies the chain and quorum commit votes
for the range tip. `CommitStream` models the resulting ordered commits. It does not
require each commit to be a `CertifiedCommit`.

The term "committed prefix" means the prefix of `CommittedSubDag` inputs already
buffered by the finalizer. It does not mean a prefix of certified commits.

Indirect rejection is not monotone for an arbitrary larger prefix. A later prefix
can contain a new accept certificate. The theorem
[`arbitrary_prefix_decision_can_flip`](../lean/Mysticeti/CommitChain.lean) gives a
small checked example of this fact.

The safe rule uses the first eligible depth-two trigger in one continuous common
commit stream. Lean proves these properties:

- the first eligible trigger is unique;
- it stays the first trigger when the visible prefix grows;
- two nodes with the same stream and trigger make the same indirect decision.

This is the proof boundary for the modeled v3 transaction finalizer. That finalizer
is not present in the current tree. A proof that only compares two arbitrary commit
prefixes is not sufficient.

## Liveness model

[`PartialSynchrony`](../lean/Mysticeti/PartialSynchrony.lean) specifies standard
partial synchrony. GST is unknown. Before GST, a message can have an arbitrary
delay. After GST, each authenticated protocol message between correct processes is
delivered within `delta`.

The liveness results also use the applicable shared fault, runtime, storage,
configuration, and leader conditions. Derived protocol stages, such as block sync,
commit sync, a recovery quorum, or a usable anchor window, are proof goals. They are
not primitive assumptions.

Lean proves these results:

- `good_window_commits_within`: the modeled network steps finish within
  `10 * delta` after a good leader window starts;
- `consensus_liveness`: a post-activation open round eventually produces a commit;
- `full_flex_anchor_window_advances_commit_index`: an in-range usable anchor
  window makes the complete modeled FlexCommitter scan advance;
- `commit_progress_recovery_stages_compose`: three distributed recovery results
  and one Rust mapping condition compose to a greater commit index;
- `finalizer_liveness`: a pending transaction on a continuous commit stream
  eventually gets a durable decision;
- `transaction_liveness`: the consensus and finalizer results compose.

### Consensus and liveness for old leader blocks

The model for liveness of old leader blocks also has a catch-up activation time. Its
liveness result starts after both GST and this activation time. The commit progress
recovery theorem does not use this activation time.

[`ConsensusLivenessStageObligations`](../lean/Mysticeti/Liveness.lean) collects
temporary Rust refinement goals for the progress checkpoints. Its fields are
derived stage results, not primitive assumptions. It requires:

- safe intermediate proposals during a round jump;
- the eventual selection of a correct, non-crashed leader;
- bounded proposal, supporter, certificate, decision, and commit steps;
- continued operation of the protocol tasks.

`consensus_liveness` and the consensus part of `transaction_liveness` prove the
stronger liveness property for old leader blocks. Their safe intermediate-proposal
condition is sufficient but is not known to be necessary for commit-index progress.

The `10 * delta` value is a model bound. It is not a measured Rust latency bound.
The Rust timers and pipeline can use smaller or larger constants. A later refinement
proof must map each checkpoint to the exact code timer and message path.

### Commit progress recovery

Commit progress recovery targets only commit-index growth. In that design,
validators enter recovery after local commit progress stalls and stay eligible
until a commit occurs. The validators do not select a common round.

[`CommitProgressRecoveryStages`](../lean/Mysticeti/CommitProgressRecovery.lean)
names the current unproved recovery-stage results. These results are theorem goals,
not basic assumptions. The recovery-window base is existential. Current v3 uses
direct votes in the next round. Thus, any layer count must be derived from the
required anchor count and the direct-vote offset.

For each correct authority in the recovery quorum, the Lean view defines the only
permitted proposal target as one round above that authority's highest known own
proposal. A higher threshold-clock round does not change this target. The theorem
does not yet model the transition that creates the proposal. The layer-production
stage is also not yet derived from synchronization and weak task fairness.

The recovery view separates these sets:

- the validator set contains all epoch validators;
- the non-progress set is intended to contain Byzantine and crashed validators;
- `RecoveryQuorum` contains only validators outside the stated non-progress set;
- the leader schedule is a subset of the validator set for one commit-index
  leader schedule interval;
- the round leader selection is a subset of the leader schedule;
- each member of the round leader selection has one selected leader slot.

Let `N` be actual validator set stake, `S` be leader schedule stake, and `P_r` be
round leader selection stake in pending leader round `r`. The structural relation
is:

```text
P_r <= S <= N
```

If Byzantine and crashed or otherwise non-progressing stake is at most `f + c`,
the leader schedule viability bound is:

```text
f + c < S <= N
```

This condition ensures that the schedule contains positive stake from a correct,
non-crashed validator. It does not ensure that a smaller round leader selection
chooses that validator. Such a selector needs a leader fairness condition.

For one round, the quorum-coverage lemma uses this sufficient lower bound:

```text
A <= P_r
```

The lower bound makes a quorum block layer contain positive stake from a correct
validator in the round leader selection. It does not prove direct finality or
commit progress. The optional `P_r <= Q` rule limits work for selected leader
slots. The per-slot safety proof and the quorum-coverage lemma do not use it. A
larger selection can add an undecided slot, so its effect on anchor-scan liveness
remains in the usable-anchor obligation.

Current v3 selects the full leader schedule in every pending leader round at or
above `min_next_leader_round`. Therefore, `P_r = S`, and the structural and
quorum-coverage bounds reduce to:

```text
A <= S = P_r <= N
```

If the optional resource policy is enabled, it adds `P_r <= Q`. The Lean model
proves the coverage derivation and the resource derivation separately. It also
proves that a viable leader schedule does not by itself give round leader selection
coverage.

The consecutive quorum block layer window also requires each layer author to be in
the recovery set and each witness layer to be above the modeled block-GC boundary.
The Rust refinement must show that block sync obtains the recent blocks before Core
evaluates the window.

[`commit_progress_recovery_stages_compose`](../lean/Mysticeti/CommitProgressRecovery.lean)
is a composition lemma. It does not prove end-to-end recovery liveness. Its inputs
contain these three distributed results and one Rust mapping condition:

1. unless a commit occurs first, one set of correct validators with quorum stake
   stays in commit progress recovery;
2. unless a commit occurs first, these validators are all in commit progress
   recovery in the same proposal rounds and produce and exchange blocks for enough
   consecutive rounds, with quorum stake in each round;
3. unless a commit occurs first, enough consecutive rounds start with a correct
   leader whose block gets enough next-round votes for FlexCommitter to resolve old
   undecided rounds;
4. Rust records the commit that the executable Lean FlexCommitter model finds.

The proof must derive the first three results from simple process and environment
contracts.
These contracts include post-GST delivery, bounded post-GST local processing, weak
task fairness, local clock progress, the local recovery-entry rule, recovery
persistence, the next-round proposal rule, parent synchronization, durable proposal
storage, broadcast, a growing recovery wait, and the stated leader-order sampling
model. The direct decision function and the `FlexCommitter` scan are deterministic
transition models, not environmental assumptions. The status-level FlexCommitter
scan is now proved.

The current Rust code satisfies the call-sequence part of the fourth
result. There is no separate queued commit action. The
[design evidence](../design/commit_progress_recovery.md#current-flexcommitter-to-core-path)
traces the call path and lists the tests that cover it. Lean does not verify Rust
source, so the Rust-state mapping remains a mapping condition. One focused
old-prefix recovery-window regression test is still missing. Future Rust changes
must preserve this call path and these results, or the model and proof must change.

The Lean view includes an abstract committed-prefix identity. The recovery
transition model must show when equal commit indices identify one common prefix.
It must then derive one leader schedule and selected leader slot order from that
prefix. The Rust refinement can map the abstract identity to the commit digest.

During one process run, the Rust recovery trigger can use the current `DagState`
commit timestamp as its base. After restart, it uses the last flushed commit that
`DagState::new` loads. It uses the epoch start timestamp before the first commit.
It computes elapsed time with saturating subtraction because a commit timestamp can
be ahead of the local clock. This rule does not need a separate persisted recovery
flag. It is not implemented and remains a known implementation gap.

### Transaction finalization liveness

`FinalizerLivenessStageObligations` requires a continuous common commit stream, an
eventual eligible trigger, and durable output. `finalizer_liveness` proves that
these stages give a durable transaction decision. `transaction_liveness` composes
the consensus and finalizer results. Commit progress recovery can help supply the
continuous commit stream, but it is not the complete transaction-liveness proof.

The current Rust tree does not implement the modeled v3 transaction finalizer.
Thus, its trigger and durability conditions remain model-to-implementation goals.

## Checked implementation counterexample

[`direct_jump_can_violate_safe_catchup`](../lean/Mysticeti/PartialSynchrony.lean)
proves a local counterexample. If a node jumps to one future round and proposes only
in that round, it can omit a required intermediate proposal. Partial synchrony does
not repair this omission.

This result matches the round-jump risk in
[`ThresholdClock::add_block`](../../core/src/threshold_clock.rs). It does not by
itself prove the full published infinite attack for the new v3 code. It proves that
the present catch-up transition cannot establish the safe round-change assumption
used by `consensus_liveness`.

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
   process one continuous commit chain with no gap.
9. [`ASM-SAFE-FIRST-TRIGGER`](ASSUMPTIONS.md#asm-safe-first-trigger): correct nodes
   use the same first depth-two trigger.
10. [`ASM-SAFE-GC`](ASSUMPTIONS.md#asm-safe-gc): Core uses the pre-commit GC
    boundary, the signed transaction cutoff is the maximum of both proposer-side
    GC rounds, and v3 sub-DAG construction copies required voting blocks before
    later DAG GC.

Items 6, 7, 9, and 10 include v3 transaction-finalization conditions that the
current Rust tree does not implement. They are model-to-implementation requirements,
not verified current behavior.

`consensus_liveness` and the consensus phase of `transaction_liveness` are also
composition theorems. Their primitive environment inputs are
`ASM-LIVE-PARTIAL-SYNCHRONY` and `ASM-LIVE-TASK-FAIRNESS`. Their local refinement
goals are `ASM-LIVE-ROUND-CATCHUP` and the static parts of `ASM-LIVE-LEADER`.
When a theorem starts from missing local consensus state, it also uses the
conditional availability part of `ASM-LIVE-PEER-FAIRNESS`.
`ASM-LIVE-BLOCK-SYNC` and `ASM-LIVE-PIPELINE-BOUNDS` are derived stage goals, not
primitive assumptions.

The target end-to-end commit progress recovery theorem does not use
`ASM-LIVE-ROUND-CATCHUP`.

Its primitive environment assumptions are:

1. [`ASM-SAFE-FAULT-BOUND`](ASSUMPTIONS.md#asm-safe-fault-bound) for bounded
   Byzantine and unavailable stake.
2. [`ASM-LIVE-PARTIAL-SYNCHRONY`](ASSUMPTIONS.md#asm-live-partial-synchrony) for
   post-GST message delivery.
3. [`ASM-LIVE-TASK-FAIRNESS`](ASSUMPTIONS.md#asm-live-task-fairness) for enabled
   local work.
4. [`ASM-LIVE-LOCAL-RESPONSE`](ASSUMPTIONS.md#asm-live-local-response) for
   local computation bounded by a positive symbolic `epsilon`, with
   `epsilon < delta`.
5. [`ASM-LIVE-FIRST-SLOT-SAMPLING`](ASSUMPTIONS.md#asm-live-first-slot-sampling)
   for the accepted independent uniform shuffle model. Its fixed-schedule and
   common-order preconditions are separate refinement goals.

If a validator starts without required old consensus state, recovery also uses
[`ASM-LIVE-PEER-FAIRNESS`](ASSUMPTIONS.md#asm-live-peer-fairness). The peer supplies
only an old consensus block or commit that remains necessary. Retry selection is a
local proof goal. Transaction payload retention is not required.

Its local rules and deterministic refinement goals are:

1. [`ASM-SAFE-PARAMETERS`](ASSUMPTIONS.md#asm-safe-parameters) for common actual
   thresholds and common schedule derivation.
2. [`ASM-LIVE-COMMIT-PROGRESS-RECOVERY`](ASSUMPTIONS.md#asm-live-commit-progress-recovery)
   for the local recovery rule, next-round proposal target, and growing pacing
   delay.
3. [`ASM-LIVE-LEADER`](ASSUMPTIONS.md#asm-live-leader) for schedule construction,
   schedule viability, and round leader selection coverage.
4. [`ASM-SAFE-GC`](ASSUMPTIONS.md#asm-safe-gc) for retained recent parents and
   decision evidence.
5. The peer-retry transition and its deterministic fairness or probabilistic
   selection theorem.

These distributed protocol results remain open refinement theorems:

1. [`ASM-LIVE-BLOCK-SYNC`](ASSUMPTIONS.md#asm-live-block-sync).
2. [`ASM-LIVE-COMMIT-SYNC`](ASSUMPTIONS.md#asm-live-commit-sync).

The recovery quorum, quorum block layers, and usable anchor window are derived proof
goals. The status-level commit advance is proved. The current Rust call path
implements the final local sequence, but its state mapping is not machine checked.
None of these results is an additional network assumption.

The finalizer phase of `transaction_liveness` composes three more derived goals:
`ASM-LIVE-COMMIT-SYNC`, `ASM-LIVE-FINALIZER-TRIGGER`, and
`ASM-LIVE-DURABILITY`. These goals must be proved from the process, network, and
storage rules.

The [gap report](IMPLEMENTATION_GAPS.md) gives the code changes that are needed to
discharge these proof obligations.
