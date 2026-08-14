<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 proof assumptions and refinement obligations

This ledger records the assumptions that connect the Lean model to the Mysticeti
v3 Rust implementation and its operating environment. The review date is
2026-08-13. The analysis baseline is in the parent
[`README`](../README.md#analysis-baseline).

Lean checks each theorem from its stated inputs. An input to an assumption
structure is not a declared Lean `axiom`, but the theorem is still conditional on
that input. Apply a theorem to Rust only when each proof obligation is discharged or
each environmental assumption is explicitly accepted.

## Status values

- **Discharged in Lean**: Lean proves the complete claim inside the model.
- **Enforced in Rust**: the reviewed Rust path rejects or prevents a violation.
- **Partially verified**: code checks or tests cover part of the claim, but no
  complete refinement proof exists.
- **Abstraction gap**: the Lean predicate does not yet map to exact Rust events or
  time.
- **Environmental assumption**: the claim is an operating, network, or adversary
  assumption outside the local decision function.
- **Open proof obligation**: the proof needs the claim, but the review found no
  sufficient evidence.
- **Known mismatch**: the reviewed implementation contradicts the claim or does not
  implement a required rule.

A `Known mismatch` blocks the affected implementation claim. An `Abstraction gap`,
`Environmental assumption`, `Open proof obligation`, or `Partially verified` status
identifies a remaining condition. It is not a proof failure inside Lean.

## Current status

| Status | Count |
|---|---:|
| Discharged in Lean | 1 |
| Enforced in Rust | 1 |
| Partially verified | 8 |
| Environmental assumption | 4 |
| Open proof obligation | 3 |
| Abstraction gap | 2 |
| Known mismatch | 5 |

The five known mismatches block an end-to-end claim for the reviewed baseline. The
other open conditions define the remaining refinement and environment boundary.

## Maintenance rules

1. Keep each `ASM-*` identifier stable. Do not reuse an old identifier for a new
   claim.
2. Reference the identifier in the Lean comment next to the related model input.
3. Reference the identifier from each implementation-gap item that can close it.
4. Change a status only with code, a proof, a test, or stated environment evidence.
5. Recheck all implementation evidence when the analysis baseline changes.

## ASM-MATH-THRESHOLDS

- **Claim:** For `N = 5f + 3c + 1`, `Q = 4f + 2c + 1`, and
  `A = 2f + c + 1`, the model has `0 < A`, `N + f < Q + A`, and
  `N + f + A <= 2Q`.
- **Type:** Mathematical.
- **Status:** Discharged in Lean.
- **Effect if false:** Safety.
- **Lean use:** [`Thresholds.nominalHybrid`](../lean/Mysticeti/Thresholds.lean)
  constructs the required inequalities.
- **Rust evidence:** Rust uses equivalent formulas in the reviewed v3 committee
  construction. Parameter agreement and integer bounds are separate assumptions.
- **Discharge:** No Lean work is required. Close the Rust mapping through
  `ASM-SAFE-PARAMETERS` and `ASM-REFINE-INTEGERS`.

## ASM-SAFE-PARAMETERS

- **Claim:** All correct nodes in one epoch use the same committee, `N`, `f`, `Q`,
  `A`, validity threshold, `gc_depth`, and v3 leader configuration.
- **Type:** Configuration refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** Every weighted quorum theorem uses one `Thresholds` value and one
  stake function.
- **Rust evidence:** `apply_v3_threshold_overrides` reads `f` and `c` from local
  process environment variables. Correct nodes can therefore derive different
  values.
- **Discharge:** Put all threshold inputs in authenticated epoch protocol state. Add
  all proof-relevant configuration values to the same state. Add
  mixed-configuration rejection tests.

## ASM-SAFE-FAULT-BOUND

- **Claim:** Byzantine authority stake is at most the configured `f` value.
- **Type:** Adversary model.
- **Status:** Environmental assumption.
- **Effect if false:** Safety.
- **Lean use:** [`FaultBounded`](../lean/Mysticeti/Thresholds.lean) bounds the
  intersection of incompatible voter sets.
- **Rust evidence:** The implementation cannot identify all Byzantine authorities
  while the protocol runs.
- **Discharge:** State this bound in the protocol threat model. Ensure that epoch
  configuration does not claim a smaller `f` than the supported deployment model.

## ASM-SAFE-AUTHENTICATION

- **Claim:** A verified signature binds the authority, epoch, round, block contents,
  transaction cutoff, transaction votes, and commit votes that the model uses.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Voter-set membership assumes that one authenticated block supplies
  the modeled vote for its named authority.
- **Rust evidence:** The block verifier checks signed blocks before Core accepts
  them. The model does not contain the serialization or signature relation.
- **Discharge:** Add conformance vectors for every signed field used by the v3 leader
  and transaction decision rules.

## ASM-SAFE-NON-EQUIVOCATION

- **Claim:** A correct authority contributes at most one compatible vote per leader
  slot and transaction. Byzantine equivocation can count once on each side, but it
  cannot count more than once on either side.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** `OnlyFaultyOverlap` is used by `LeaderEvidence` and
  `TransactionEvidence`.
- **Rust evidence:** Vote aggregation uses authority identity, and the proposer
  persists its own block state. Restart and recovery behavior remains part of the
  refinement boundary.
- **Discharge:** Add one invariant across proposal, restart, replay, and vote
  aggregation. Keep equivocation conformance vectors for one vote on each side.

## ASM-SAFE-PARENT-QUORUM

- **Claim:** Every accepted non-genesis block has at least `Q` stake in verified
  immediate parents.
- **Type:** Rust refinement.
- **Status:** Enforced in Rust.
- **Effect if false:** Safety.
- **Lean use:** The direct-versus-indirect argument needs an anchor quorum.
- **Rust evidence:** The block verifier checks immediate-parent quorum stake.
- **Discharge:** Keep a boundary test that uses the same configured `Q` as the Lean
  threshold model. Parameter agreement remains under `ASM-SAFE-PARAMETERS`.

## ASM-SAFE-EVIDENCE-REFINEMENT

- **Claim:** The Rust leader and transaction decision functions create the same
  voter sets, signed vote witnesses, cutoffs, unique-authority counts, and outcomes
  as the Lean evidence relations. A v3 proposer sets
  `transaction_votes_cutoff_round` to the maximum of its causal-history GC round
  and transaction vote-tracker GC round.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** [`LeaderEvidence`](../lean/Mysticeti/Leader.lean),
  [`TransactionVote`](../lean/Mysticeti/Finalizer.lean),
  [`TransactionVoteProduction`](../lean/Mysticeti/Finalizer.lean), and
  [`TransactionEvidence`](../lean/Mysticeti/Finalizer.lean). A counted direct
  accept has a signed witness that passes the numeric cutoff classifier.
- **Rust evidence:** Focused Rust tests cover direct and indirect decisions,
  cutoffs, and Byzantine equivocation. The Lean model is hand written and has no
  executable conformance suite.
- **Discharge:** Add shared decision vectors for leader decisions, transaction
  decisions, cutoffs, first-trigger selection, and equivocation.

## ASM-SAFE-COMMIT-CHAIN

- **Claim:** Correct nodes process one quorum-certified commit chain with no index
  gap and with each `previous_digest` equal to the preceding commit digest.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** [`CommitStream`](../lean/Mysticeti/CommitChain.lean) represents one
  common stream. Its current `Continuous` predicate checks indices only.
- **Rust evidence:** Commit sync verifies the fetched index and digest chain, buffers
  gaps, and verifies a quorum certificate. The v3 Core path checks the local digest
  boundary. The finalizer checks consecutive indices.
- **Discharge:** Extend the Lean commit model with digests, previous digests, and
  certification. Prove the local, synchronized, replayed, and recovered paths refine
  this model.

## ASM-SAFE-FIRST-TRIGGER

- **Claim:** Correct nodes use the same first eligible depth-two commit as the
  indirect decision trigger for one target.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** [`first_trigger_agreement`](../lean/Mysticeti/CommitChain.lean) uses
  two views of the same stream and the same first-eligible rule.
  [`firstEligible_predecessor`](../lean/Mysticeti/CommitChain.lean) proves that the
  commit before this trigger is still below the depth-two boundary.
- **Rust evidence:** The finalizer consumes consecutive commits in order. There is
  no complete cross-node proof that all input paths select the same trigger.
- **Discharge:** Derive trigger equality from `ASM-SAFE-COMMIT-CHAIN`, and check trigger
  order at every finalizer input boundary.

## ASM-SAFE-COMMITTED-PREFIX

- **Claim:** Before the first depth-two trigger, the committed prefix contains every
  correct direct accept voter that is also in the trigger anchor quorum.
- **Type:** Protocol and Rust refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety.
- **Lean use:** `correctCommitAnchorInCertificate` and
  `anchorInCommittedPrefix` are the remaining refinement inputs. Lean combines
  `anchorInCommittedPrefix` with the GC theorem and the signed transaction vote
  witness to derive the direct-versus-indirect inclusion fact.
- **Rust evidence:** `FlexCommitter::build_commit` constructs each local v3 sub-DAG.
  `FlexCommitter::handle_certified_commit` reconstructs each synced v3 sub-DAG from
  the certified commit's block list. The v3 finalizer processes these sub-DAGs in
  order. No complete lemma connects both paths, a multi-leader commit, commit sync,
  and the finalizer prefix.
- **Discharge:** Prove the committed-prefix lemma for local commits and certified
  commits. Add an integration invariant for the exact first trigger.

## ASM-SAFE-GC

- **Claim:** Core reads the GC boundary from the preceding commit before it records
  the new commit. The v3 schedule starts each next leader decision after the last
  committed leader round. For transaction votes, the signed cutoff covers both
  causal-history block GC and vote-tracker GC. V3 sub-DAG construction copies the
  required next-round anchor blocks into the finalizer's pending committed prefix
  before a later DAG GC operation can remove them.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** [`CoreGcState.evidence_retained`](../lean/Mysticeti/GarbageCollection.lean)
  proves the Core decision-to-anchor window.
  [`TransactionGcWindow.voting_round_retained`](../lean/Mysticeti/GarbageCollection.lean)
  proves the deep-target and near-target transaction cases.
  [`causal_history_gc_rejects`](../lean/Mysticeti/Finalizer.lean) and
  [`vote_tracker_gc_rejects`](../lean/Mysticeti/Finalizer.lean) prove the signed
  cutoff behavior. `BlockEvidenceStore` keeps the live DAG and pending committed
  prefix separate.
- **Rust evidence:** The v3 constructor checks `gc_depth > 2`. The proposer signs the
  maximum of the causal-history and vote-tracker GC rounds.
  `FlexCommitter::build_commit` reads the old `DagState::gc_round()` before
  `Core::post_commit` records the new commit. The finalizer keeps each pending
  `CommittedSubDag`. The certified-commit path uses the certified block list instead
  of the local DFS. The complete local, commit-sync, replay, and recovery refinement
  is not proved. The `CommitFinalizerV3` constructor comment still names the legacy
  `Linearizer`; that comment does not match the active v3 path.
- **Discharge:** Prove that every required anchor voting block enters the exact
  `CommittedSubDag` prefix on all input paths. Prove that the finalizer processes
  that prefix before it can lose the evidence. Test the minimum supported GC depth,
  a slow finalizer, commit sync, and restart recovery.

## ASM-CONFIG-V3-ACTIVATION

- **Claim:** The analyzed FlexCommitter and v3 finalizer path is active for the epoch
  to which a proof claim is applied.
- **Type:** Configuration applicability.
- **Status:** Known mismatch.
- **Effect if false:** Applicability.
- **Lean use:** All model definitions describe the v3 path.
- **Rust evidence:** The reviewed normal Sui startup path sets `enable_v3` to false.
- **Discharge:** Add versioned epoch activation, rollback rules, and mixed-version
  tests. Do not use a local process flag.

## ASM-CONFIG-VOTING

- **Claim:** `enable_v3` implies `transaction_voting_enabled` for every correct node
  in the epoch.
- **Type:** Configuration refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** The transaction safety theorem always uses the v3 voting rule.
- **Rust evidence:** `CommitFinalizerV3` bypasses voting when transaction voting is
  disabled. No constructor invariant connects the two flags.
- **Discharge:** Use one versioned feature value, or reject a configuration that
  enables v3 without transaction voting.

## ASM-REFINE-INTEGERS

- **Claim:** Rust integer types represent every natural number used by the model
  without overflow, truncation, or an invalid conversion.
- **Type:** Data refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety and liveness.
- **Lean use:** Stake, thresholds, rounds, commit indices, transaction indices, and
  schedule values use unbounded `Nat`.
- **Rust evidence:** The implementation uses several bounded integer types. There is
  no single checked bound for the full path.
- **Discharge:** Document maximum values, use checked arithmetic, and add exhaustive
  small-value and boundary-value property tests.

## ASM-LIVE-PARTIAL-SYNCHRONY

- **Claim:** There is an unknown GST after which each authenticated protocol message
  between correct processes is delivered within `delta`.
- **Type:** Network environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** [`PartialSynchrony`](../lean/Mysticeti/PartialSynchrony.lean) states
  this condition directly.
- **Rust evidence:** Network code can measure delay and reconnect, but it cannot
  enforce the network condition.
- **Discharge:** Keep this as an explicit deployment assumption. Do not use it to
  infer peer selection, data retention, task scheduling, or storage progress.

## ASM-LIVE-ROUND-CATCHUP

- **Claim:** After activation, a round jump creates each required intermediate
  proposal before it creates the proposal for the observed future round.
- **Type:** Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** `SafeRoundChanges` and `ConsensusLivenessAssumptions.safeRoundChanges`.
- **Rust evidence:** `ThresholdClock` jumps to the observed round, and `Proposer`
  creates at most one block for the final clock round.
- **Discharge:** Implement the safe intermediate-proposal loop and add a deterministic
  v3 simulation test.

## ASM-LIVE-LEADER

- **Claim:** The leader schedule eventually selects a live correct leader with
  enough live support for proposal and certificate progress.
- **Type:** Protocol and configuration refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Liveness.
- **Lean use:** `ConsensusLivenessAssumptions.fairLiveLeader`.
- **Rust evidence:** `LeaderScheduleV3` can exclude low-score stake. The reviewed
  configuration has no proved bound that preserves a live correct leader.
- **Discharge:** Connect maximum excluded stake, `f`, crash stake, and required live
  stake. Add boundary property tests.

## ASM-LIVE-BLOCK-SYNC

- **Claim:** Each required missing DAG block is eventually verified and accepted, or
  a certified committed prefix makes that block unnecessary.
- **Type:** Rust refinement.
- **Status:** Abstraction gap.
- **Effect if false:** Liveness.
- **Lean use:** Missing-ancestor recovery is hidden inside the bounded consensus
  phase transitions. There is no explicit block-sync state or theorem.
- **Rust evidence:** `BlockManager` suspends blocks with missing ancestors.
  `Synchronizer` uses direct requests, periodic requests, and a stored-history
  fallback after commit progress stalls.
- **Discharge:** Model missing blocks, accepted blocks, and the garbage-collection
  case. Prove
  eventual resolution under `ASM-LIVE-PEER-FAIRNESS` and
  `ASM-LIVE-TASK-FAIRNESS`. Use `Eventually` unless a code-derived bound exists.

## ASM-LIVE-COMMIT-SYNC

- **Claim:** Each missing certified commit needed for progress eventually enters
  Core in index and digest order, through commit sync or the live block path.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Liveness.
- **Lean use:** `FinalizerLivenessAssumptions.continuousCommitStream` and
  `triggerEventually` assume the resulting stream.
- **Rust evidence:** Commit sync schedules complete batches, retries across peers,
  verifies the chain and the quorum certificate for the final commit in each range,
  buffers gaps, and sends ordered ranges to Core. A trailing partial batch is
  delegated to broadcast, subscription, and block sync.
- **Discharge:** Prove eventual stream extension under the peer and task assumptions.
  Model full batches, partial results, the incomplete tail, backpressure, and
  certified block retention.

## ASM-LIVE-PEER-FAIRNESS

- **Claim:** At least one known correct peer retains each required block and commit,
  serves it after GST, and is eventually selected by the retry path.
- **Type:** Network, storage, and selection environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** Required by the abstract block-sync and commit-sync progress claims.
- **Rust evidence:** Sync paths shuffle peers. The block-sync fallback selects one
  random peer per attempt. Some paths assume that the known-peer set is not empty.
- **Discharge:** State a retention period and peer-discovery contract. Use deterministic
  fair peer rotation, or use an explicit probabilistic model and prove almost-sure
  selection of a useful peer.

## ASM-LIVE-TASK-FAIRNESS

- **Claim:** Correct proposer, block synchronizer, commit synchronizer, Core,
  finalizer, storage, and consumer tasks continue to run. Sustained backpressure
  eventually clears.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** All `LeadsToAfter` and `WithinAfter` phase obligations need local
  execution progress.
- **Rust evidence:** Tokio tasks, channels, retry loops, and monitors implement the
  work. The Lean trace does not model scheduling, shutdown, queue capacity, or task
  failure.
- **Discharge:** Define which shutdowns are outside the theorem. Add progress monitors
  and tests for saturated queues, stalled consumers, task restart, and storage delay.

## ASM-LIVE-PIPELINE-BOUNDS

- **Claim:** Each modeled proposal, delivery, support, certificate, decision, and
  commit transition completes within its stated `delta` or `2 * delta` bound.
- **Type:** Timing refinement.
- **Status:** Abstraction gap.
- **Effect if false:** Liveness bound.
- **Lean use:** The fields of `ConsensusLivenessAssumptions` produce the
  `10 * delta` result.
- **Rust evidence:** Rust has separate scheduler periods, request timeouts, retry
  delays, processing time, and backpressure. They are not derived from the network
  `delta`.
- **Discharge:** Define each phase predicate on concrete Rust events. Either derive a
  bound that includes code timers and local processing, or weaken the production
  claim to eventual progress.

## ASM-LIVE-FINALIZER-TRIGGER

- **Claim:** Every pending transaction eventually receives a later first eligible
  depth-two trigger, including at shutdown and the epoch tail.
- **Type:** Protocol and lifecycle refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** `FinalizerLivenessAssumptions.triggerEventually`.
- **Rust evidence:** Normal continued commits can provide the trigger. Shutdown can
  leave pending finalizer state without a later same-epoch commit, and no explicit
  epoch-tail rule defines the result.
- **Discharge:** Keep consensus active through all required triggers, persist and
  replay pending state across the boundary, or define and prove a safe tail rule.

## ASM-LIVE-DURABILITY

- **Claim:** A produced commit enters the finalizer, a trigger produces one decision,
  and the decision becomes durable and reaches the consumer within the modeled
  bounds.
- **Type:** Rust and timing refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety and liveness.
- **Lean use:** `triggerToDecision`, `decisionToDurableOutput`, and
  `transaction_liveness.commitEntersFinalizer`.
- **Rust evidence:** The v3 path sends ordered committed sub-DAGs to the finalizer.
  The finalizer persists finalized results before it sends them to the consumer.
  The model does not map crashes, channel delay, storage latency, or replay to the
  `delta` bound.
- **Discharge:** Define the durable event, crash boundary, and replay relation. Test a
  crash before persistence, after persistence, and before consumer delivery.
