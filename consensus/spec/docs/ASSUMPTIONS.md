<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 proof assumptions and refinement obligations

This ledger records the assumptions that connect the Lean model to the Mysticeti
v3 Rust implementation and its operating environment. The review date is
2026-08-14. The analysis baseline is in the parent
[`README`](../README.md#analysis-baseline).

Lean checks each theorem from its stated inputs. An input to an assumption
structure is not a declared Lean `axiom`, but the theorem is still conditional on
that input. Apply a theorem to Rust only when each proof obligation is discharged or
each environmental assumption is explicitly accepted.

## Assumption boundary

A primitive liveness assumption must describe one simple boundary:

- one authenticated message is delivered after GST;
- one continuously enabled task at a correct validator eventually runs;
- one local consensus action that stays enabled at a correct validator completes
  within a stated post-GST bound;
- one correct local clock continues to advance;
- one durable write remains available after restart;
- one stated set of validators is Byzantine, crashed, or available;
- one random source has a stated distribution and independence rule.

A deterministic Rust function or state transition is a refinement theorem target.
It is not an environmental assumption. A quorum entering recovery, consecutive
quorum block layers, a usable anchor, and a greater commit index are protocol
results. The proof must derive them. It must not accept them as primitive
assumptions.

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
| Environmental assumption | 5 |
| Open proof obligation | 3 |
| Abstraction gap | 3 |
| Known mismatch | 6 |

The six known mismatches block an end-to-end claim for the reviewed baseline. The
other open conditions define the remaining refinement and environment boundary.

## Maintenance rules

1. Keep each `ASM-*` identifier stable. Do not reuse an old identifier for a new
   claim.
2. Reference the identifier in the Lean comment next to the related model input.
3. Reference the identifier from each implementation-gap item that can close it.
4. Change a status only with code, a proof, a test, or stated environment evidence.
5. Recheck all implementation evidence when the analysis baseline changes.
6. Do not add a protocol result as a primitive assumption. Put an unproved result
   under an open proof obligation and list the simple contracts that must derive it.

## ASM-MATH-THRESHOLDS

- **Claim:** For `N = 5f + 3c + 1`, `Q = 4f + 2c + 1`, and
  `A = 2f + c + 1`, the model has `0 < A`, `N + f < Q + A`, and
  `N + f + A <= 2Q`.
- **Type:** Mathematical.
- **Status:** Discharged in Lean.
- **Effect if false:** Safety.
- **Lean use:** [`Thresholds.nominalHybrid`](../lean/Mysticeti/Thresholds.lean)
  constructs the required inequalities.
- **Rust evidence:** `Committee::new_v3` uses these formulas only as the nominal
  case. It scales `f` and `c` to the actual validator set stake, sets
  `A = 2f + c + 1`, and sets `Q = N - f - c`. It then checks the two safety
  inequalities. Actual `N` does not always equal `5f + 3c + 1`.
- **Discharge:** No Lean work is required. Close the Rust mapping through
  `ASM-SAFE-PARAMETERS` and `ASM-REFINE-INTEGERS`.

## ASM-SAFE-PARAMETERS

- **Claim:** All correct nodes in one epoch use the same validator set, `N`, `f`,
  `Q`, `A`, validity threshold, `gc_depth`, and v3 leader configuration. From one
  common committed prefix, they derive the same leader schedule membership, round
  leader selection, and selected leader slot order.
- **Type:** Configuration refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** Every weighted quorum theorem uses one `Thresholds` value and one
  stake function.
- **Rust evidence:** `apply_v3_threshold_overrides` reads `f` and `c` from local
  process environment variables. Correct nodes can therefore derive different
  values.
- **Discharge:** Put all threshold inputs in authenticated epoch protocol state. Add
  all proof-relevant configuration values to the same state. Prove deterministic
  schedule and selection derivation from the common commit chain. Add
  mixed-configuration rejection tests.

## ASM-SAFE-FAULT-BOUND

- **Claim:** Byzantine validator stake is at most the configured `f` value. After
  GST, Byzantine plus crashed or otherwise non-progressing validator stake is at
  most `f + c`. A stable set of correct, non-crashed validators remains active.
- **Type:** Adversary and availability model.
- **Status:** Environmental assumption.
- **Effect if false:** Safety for the `f` bound; liveness for the `f + c` bound.
- **Lean use:** [`FaultBounded`](../lean/Mysticeti/Thresholds.lean) bounds the
  intersection of incompatible voter sets. The commit progress recovery view uses
  `nonProgress` for the union of Byzantine and crashed or unavailable validators.
- **Rust evidence:** The implementation cannot identify all Byzantine authorities
  while the protocol runs.
- **Discharge:** State both bounds in the protocol threat model. Ensure that epoch
  configuration does not claim smaller `f` or `c` values than the supported
  deployment model.

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

- **Claim:** Correct nodes process one common commit chain with no index gap and
  with each `previous_digest` equal to the preceding commit digest. A commit can be
  produced by the local commit rule or installed through commit sync.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** [`CommitStream`](../lean/Mysticeti/CommitChain.lean) represents one
  common stream. Its current `Continuous` predicate checks indices only.
- **Rust evidence:** `FlexCommitter::build_commit` produces local commits. Commit
  sync verifies the fetched index and digest chain, buffers gaps, and verifies
  quorum commit votes before Core installs the commits. The v3 Core path checks the
  local digest boundary. The finalizer checks consecutive indices.
- **Discharge:** Extend the Lean commit model with digests and previous digests.
  Prove that local commit production, commit-sync installation, replay, and recovery
  refine the same commit chain. Prove verification of quorum commit votes for the
  synced range tip as a condition of commit-sync installation, not as a condition
  of every commit.

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
  For a commit installed through commit sync,
  `FlexCommitter::handle_certified_commit` reconstructs the v3 sub-DAG from the
  explicit block list in `CertifiedCommit`. The v3 finalizer processes these
  sub-DAGs in order. No complete lemma connects both paths, a multi-leader commit,
  commit sync, and the finalizer prefix.
- **Discharge:** Prove the committed-prefix lemma for local commit production and
  commit-sync installation. Add an integration invariant for the exact first
  trigger.

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
  `CommittedSubDag`. The commit-sync path uses the explicit block list in
  `CertifiedCommit` instead of the local DFS. The complete local, commit-sync,
  replay, and recovery refinement is not proved.
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
- **Lean use:** `SafeRoundChanges` and
  `ConsensusLivenessStageObligations.safeRoundChanges`.
- **Rust evidence:** `ThresholdClock` jumps to the observed round, and `Proposer`
  creates at most one block for the final clock round. The current code does not
  create required intermediate blocks.
- **Discharge:** Implement the safe intermediate-proposal loop and add a
  deterministic v3 simulation test. This rule is sufficient for the stronger
  liveness property for old leader blocks.

## ASM-LIVE-COMMIT-PROGRESS-RECOVERY

- **Claim:** The local recovery rule at one correct validator has these parts. The
  validator reads its current last-commit timestamp, or the last flushed timestamp
  after restart. It enters recovery when the local timeout expires and it is not
  behind the observed quorum commit index. It stays in recovery until its commit
  index changes. If `P` is its highest known own proposal round, its only recovery
  proposal target is `P + 1`. It waits or starts block sync when quorum parents in
  round `P` are not available. It persists an enabled proposal before broadcast.
  While the commit index is unchanged, its schedule-independent proposal delay can
  grow without a fixed bound.
- **Type:** Protocol and Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** `NextRoundProposalTargets` models the local proposal target.
  `CommitProgressRecoveryStages` names four derived stage theorems that are still
  open. `commit_progress_recovery_stages_compose` proves only that these stages
  compose. It is not the end-to-end liveness theorem.
- **Rust evidence:** `DagState::last_commit_timestamp_ms` exposes the current
  in-memory commit timestamp. `DagState::flush` makes buffered commits durable, and
  `DagState::new` loads the last flushed commit after restart. `Context::clock`
  exposes local time. The current code does not have commit progress recovery, a
  commit-stall trigger for block production, or adaptive recovery pacing.
- **Discharge:** Implement the local rule. Then derive these results in Lean:
  recovery-quorum overlap from local clock progress, weak task fairness, persistent
  recovery, and the live correct stake bound; consecutive quorum block layers from
  the next-round proposal rule, parent synchronization, persistence, and broadcast;
  a usable anchor window from partial synchrony, growing propagation delay, and
  first-slot sampling; and commit-index progress from an executable model of the
  `FlexCommitter` scan. Use the epoch start timestamp when the commit index is zero.
  Use saturating subtraction for a future commit timestamp. Add deterministic v3
  simulation tests.

## ASM-LIVE-LEADER

- **Claim:** Let `N` be actual validator set stake, `S` be leader schedule stake,
  and `P_r` be round leader selection stake for one pending leader round `r`. The
  structural relation is `P_r <= S <= N`. Given the separate non-progress stake
  bound `f + c`, the schedule viability bound is `f + c < S`. A protocol that uses
  a smaller round leader selection also needs a leader fairness rule. The sufficient
  quorum-coverage bound is `A <= P_r`. Current v3 selects the full leader schedule
  for every pending leader round, so its structural and coverage bounds are
  `A <= S = P_r <= N`. The per-slot safety proof and the quorum-coverage lemma do
  not use the optional resource policy `P_r <= Q`. These static bounds do not prove
  a usable anchor.
- **Type:** Protocol and configuration refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Liveness.
- **Lean use:** `ConsensusLivenessStageObligations.fairRoundLeaderSelection` is a
  temporary stage obligation that must use these bounds with a separate fairness
  result. For commit progress recovery,
  `LeaderScheduleViabilityBounds` and
  `RoundLeaderSelectionCoverageBounds` separate schedule viability from per-round
  quorum coverage. `V3LeaderScheduleCoverageBounds` and
  `v3_coverage_applies_to_schedule_and_round` derive the v3 specialization from
  actual set weights.
  `quorum_and_round_leader_selection_intersect_outside_byzantine_stake` proves the
  per-round weighted intersection. The optional work cap has separate
  `RoundLeaderSelectionResourceBound` and `V3LeaderScheduleResourceBound` types.
- **Rust evidence:** `LeaderScheduleV3` builds the Rust `allowed_leaders` field by
  excluding low-score stake. This ordered field is the leader schedule.
  `FlexCommitter` creates one selected leader slot for every listed validator in
  each pending round at or above `min_next_leader_round`. It applies a
  deterministic round-based permutation to the slot order. Thus, current v3 uses
  the full leader schedule as the round leader selection for those rounds. The
  percentage rule does not directly enforce `A <= S`. The deterministic order has
  no proved fairness property that supplies the required anchor.
- **Discharge:** Define and enforce the schedule viability and round leader
  selection coverage bounds from the actual committee weights. Report `P_r <= Q`
  only if the deployment enables this resource policy. Prove usable-anchor progress
  only in recovery stage 3. Add stake-bound, equivocation, selection, and slot-order
  tests.

## ASM-LIVE-FIRST-SLOT-SAMPLING

- **Claim:** Assume that separate refinement results give one fixed leader schedule,
  one fixed non-progress set, and one common selected leader slot order at all
  correct validators while a commit index is stalled. Conditional on this state,
  each pending leader round samples one uniform random permutation of the leader
  schedule. Samples for different rounds are independent and are not controlled by
  the network or task scheduler. If the schedule has `m` members and `h > 0` of them
  are correct and non-crashed, the probability that the first selected leader slot
  is correct is `p = h / m`. A run of `k` rounds has a correct first slot in every
  round with probability `p^k`. The correct validators can be the same or different.
  Repeated disjoint runs contain such a run with probability one. Current v3 uses
  `k = indirectCommitDepth + 1 = 3` for the recovery proof.
- **Type:** Accepted probabilistic abstraction and Rust refinement.
- **Status:** Abstraction gap.
- **Effect if false:** Liveness.
- **Lean use:** The deterministic Lean trace does not model probability.
  `CommitProgressRecoveryStages.recoveryLayersToUsableAnchors` is the open theorem
  that must use this assumption with post-GST delivery and adaptive pacing. A future
  probabilistic model can prove that the failure probability after `j` disjoint
  three-round runs is `(1 - p^3)^j`, which approaches zero.
- **Rust evidence:** `FlexCommitter` seeds `StdRng` from the round number and
  shuffles the complete ordered leader schedule for each pending round. This is a
  deterministic pseudorandom sequence. The `rand` interface does not specify
  independence or a consecutive-seed coverage bound.
- **Discharge:** This ideal random-permutation model is explicitly accepted for the
  current recovery design. Rust approximates it with a deterministic pseudorandom
  permutation. To remove the abstraction, use protocol randomness with a stated
  probabilistic model, prove a weighted first-slot coverage bound for the exact
  deterministic order, or use a deterministic order with a bounded correct-first
  run guarantee. Discharge the fixed schedule and common-order preconditions through
  `ASM-SAFE-PARAMETERS` and the committed-prefix refinement. They are not random-source
  assumptions.
- **Decision, 2026-08-14:** Model each round's seeded shuffle as one common,
  independent, uniform random permutation of the leader schedule. Use a run of
  `indirectCommitDepth + 1` correct first slots. This is an accepted liveness
  abstraction for the deterministic Rust shuffle. It is not a safety assumption or
  a deterministic theorem about `StdRng`.

## ASM-LIVE-BLOCK-SYNC

- **Claim:** Target theorem: each required missing DAG block is eventually verified
  and accepted, or an installed commit advances the committed history and GC
  boundary so that Core no longer needs that block.
- **Type:** Derived Rust progress theorem.
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

- **Claim:** Target theorem: each missing commit needed for progress is eventually
  installed through commit sync from a verified commit range whose tip has quorum
  commit votes, or the local commit rule reproduces it after the live block path
  supplies the required DAG blocks.
- **Type:** Derived Rust progress theorem.
- **Status:** Partially verified.
- **Effect if false:** Liveness.
- **Lean use:** `FinalizerLivenessStageObligations.continuousCommitStream` and
  `triggerEventually` assume the resulting stream.
- **Rust evidence:** Commit sync schedules complete batches, retries across peers,
  verifies the chain and the quorum certificate for the final commit in each range,
  buffers gaps, and sends ordered ranges to Core. A trailing partial batch is
  delegated to broadcast, subscription, and block sync.
- **Discharge:** Prove eventual stream extension under the peer and task assumptions.
  Model full batches, partial results, the incomplete tail, backpressure, and
  retention of the commits, certifying vote blocks, and sub-DAG blocks used by
  commit sync.

## ASM-LIVE-PEER-FAIRNESS

- **Claim:** For each required block or commit, at least one known correct peer
  retains the item and returns it when requested after GST.
- **Type:** Network, storage, and peer-availability environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** This is the useful-peer environment input for the open block-sync
  and commit-sync progress theorems. Eventual selection of that peer must be derived
  from the retry transition and its fairness or random-selection rule.
- **Rust evidence:** Sync paths shuffle peers. The block-sync fallback selects one
  random peer per attempt. Some paths assume that the known-peer set is not empty.
- **Discharge:** State a retention, peer-discovery, and request-service contract.
  Then prove eventual useful-peer selection from deterministic fair peer rotation,
  or use an explicit probabilistic selection model.

## ASM-LIVE-TASK-FAIRNESS

- **Claim:** If one action in a correct proposer, block synchronizer, commit
  synchronizer, Core, finalizer, storage, or consumer task stays enabled, the
  runtime eventually schedules that action. This is weak task fairness. Queue
  capacity and consumer progress are separate conditions.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** A future transition model will use weak fairness to prove local
  enabled actions run. The current stage composition does not model the scheduler.
- **Rust evidence:** Tokio tasks, channels, retry loops, and monitors implement the
  work. The Lean trace does not model scheduling, shutdown, queue capacity, or task
  failure.
- **Discharge:** Define which shutdowns are outside the theorem. Model task guards
  and actions. State queue and consumer progress separately. Add progress monitors
  and tests for saturated queues, stalled consumers, task restart, and storage delay.

## ASM-LIVE-LOCAL-RESPONSE

- **Claim:** Local consensus computation takes at most `epsilon` time. The
  instantaneous model is the special case `epsilon = 0`. For Rust refinement,
  `epsilon` is a finite symbolic bound for one local consensus
  action that stays enabled at a correct, running validator. The aggregate bound
  includes task and queue scheduling, message verification, DAG acceptance,
  proposal construction, a required proposal flush, and handoff to the network
  transport. It also includes making a received result visible to the next local
  consensus action. The idealized model uses `epsilon = 0`. A Rust refinement can
  keep `epsilon` symbolic. The same uniform bound applies separately to each
  covered action. The message `sentAt` event is the transport send. The
  `deliveredAt` event is delivery to the receiving handler.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Commit progress liveness.
- **Lean use:** `BoundedLocalProcessing` states the general boundary.
  `protocol_packet_becomes_locally_visible` uses `delta + epsilon`.
  `protocol_packet_is_immediately_visible_after_delivery` proves the special case
  with `epsilon = 0`.
  Weak task fairness alone cannot ensure that a correct first-slot proposal runs
  and becomes visible before the next-round proposals.
- **Rust evidence:** The receive path includes task queues, blocking-pool
  verification, the Core channel, Core processing, and DAG insertion. The proposal
  path flushes the block before Core hands it to the broadcast path. Tokio, storage,
  and the operating system do not give one hard bound for this complete path.
  Production load controls and task monitoring try to keep local response finite.
- **Discharge:** This proof accepts the guaranteed bound as the processor-speed
  part of the partial-synchrony environment. Define the covered Rust actions. A
  probabilistic response bound is also possible, but it needs a separate
  probability model.
- **Decision, 2026-08-14:** Use a symbolic local-computation bound `epsilon`, or set
  `epsilon = 0` as an explicit instantaneous-computation idealization. Do not use a
  fixed processing constant. Do not infer a minimum network delay. With a finite
  `epsilon`, `delta + epsilon`
  bounds transport send through remote DAG visibility, and
  `delta + 2 * epsilon` bounds local proposal enablement through remote DAG
  visibility. A recovery timer also needs a derived bound from its local start
  event to the relevant proposal-enable or transport-send event.

## ASM-LIVE-PIPELINE-BOUNDS

- **Claim:** Target theorem: each modeled proposal, delivery, support, certificate,
  decision, and commit transition completes within its stated `delta` or
  `2 * delta` bound.
- **Type:** Derived timing refinement.
- **Status:** Abstraction gap.
- **Effect if false:** Liveness bound.
- **Lean use:** The fields of `ConsensusLivenessStageObligations` produce the
  `10 * delta` result.
- **Rust evidence:** Rust has separate scheduler periods, request timeouts, retry
  delays, processing time, and backpressure. They are not derived from the network
  `delta`.
- **Discharge:** Define each phase predicate on concrete Rust events. Either derive a
  bound that includes code timers and local processing, or weaken the production
  claim to eventual progress.

## ASM-LIVE-FINALIZER-TRIGGER

- **Claim:** Target theorem: every pending transaction eventually receives a later
  first eligible depth-two trigger, including at shutdown and the epoch tail.
- **Type:** Derived protocol and lifecycle theorem.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** `FinalizerLivenessStageObligations.triggerEventually`.
- **Rust evidence:** Normal continued commits can provide the trigger. Shutdown can
  leave pending finalizer state without a later same-epoch commit, and no explicit
  epoch-tail rule defines the result.
- **Discharge:** Keep consensus active through all required triggers, persist and
  replay pending state across the boundary, or define and prove a safe tail rule.

## ASM-LIVE-DURABILITY

- **Claim:** Target theorem: a produced commit enters the finalizer, a trigger
  produces one decision, and the decision becomes durable and reaches the consumer
  within the modeled bounds.
- **Type:** Derived Rust and timing theorem.
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
