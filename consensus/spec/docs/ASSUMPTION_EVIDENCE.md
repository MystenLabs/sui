<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 assumption evidence

This document records the evidence behind selected entries in the
[assumption ledger](ASSUMPTIONS.md). The ledger is the canonical list of
claims and statuses. This document records why a status is justified, what the
evidence does not prove, and which changes require a focused review. The
[implementation gap report](IMPLEMENTATION_GAPS.md) records missing product
work.

Do not use an evidence entry as a theorem input. An entry can justify a local
source-to-model rule. The protocol proof must still derive network progress,
one exact commit chain, transaction decisions, and durable output.

## Review snapshot

- **Review date:** 2026-08-17.
- **Product source revision:** `2fecfec37462785ccd6684195aac9131e54ad251`.
- **Proof source:** the `tmw/mysticeti-v3-lean-main` working tree at that product
  revision. Some proof files are not committed. Use declaration names as the
  stable reference and replace this note with a proof revision before merge.
- **Scope:** network round progress, leader schedule and probability evidence,
  exact FlexCommitter prefix safety, cached decision origin, commit storage and
  restart, and transaction finalizer durability.

## Entry format

Each entry uses this format:

```text
Evidence ID
Related assumptions and review IDs
Exact claim
Classification and status
Rust evidence
Lean evidence
What this evidence does not prove
Revalidation triggers
Audit date and source revision
```

## EV-NETWORK-ROUND-PROGRESS

- **Related assumptions and review IDs:** `ASM-LIVE-LOCAL-PROPOSAL`,
  `ASM-LIVE-BLOCK-SYNC`, `ASM-LIVE-PEER-FAIRNESS`,
  `ASM-LIVE-TASK-FAIRNESS`, `REF-LOCAL-PROPOSAL-PROGRESS`, and
  `REF-CURRENT-TIP-REPLAY`.
- **Exact claim:** For every post-GST start, requested minimum round, and active
  epoch suffix, a later state contains a correct, available validator that
  holds an exact valid total-stake quorum layer at the requested round or a
  higher round. Every selected body is accepted, retained, catalogued, and
  above that holder's GC round. The quorum can contain Byzantine-authored
  blocks.
- **Classification and status:** Proved in Lean from local current-state,
  scheduler, storage, delivery, and acceptance source maps. Current Rust has
  the required high-level timeout retry and subscription replay mechanisms.
  Their exact Rust-to-Lean trace mappings are still open.
- **Rust evidence:**
  - `BlockManager` reads the current GC round, does not fetch dependencies at
    or below it, drops returned blocks at or below it, and removes newly
    obsolete suspended dependencies in
    [block_manager.rs](../../core/src/block_manager.rs#L212-L221),
    [block_manager.rs](../../core/src/block_manager.rs#L279-L374), and
    [block_manager.rs](../../core/src/block_manager.rs#L459-L527). An in-flight
    fetch does not need a commit-rebase operation.
  - A proposal reads the current threshold-clock round in
    [proposer.rs](../../core/src/proposer.rs#L348-L385). Commit-driven leader
    schedule changes can affect the normal leader wait, but the proof's final
    local progress rule is the forced max-timeout path.
  - The leader timeout makes one forced attempt for each observed round in
    [leader_timeout.rs](../../core/src/leader_timeout.rs#L80-L116). A change to
    the recovered own round makes another forced attempt. A propagation-delay
    change also makes another forced attempt when that blocker clears in
    [core_thread.rs](../../core/src/core_thread.rs#L146-L165).
  - `force = true` bypasses leader presence and the minimum round delay. A
    forced attempt can return no block because the callback round is stale, the
    current clock round already has an own block, the recovered own-round guard
    prevents a duplicate signature, the recovered value is not ready, or the
    propagation-delay blocker is active. The last two cases have the watcher
    retries above. A missing forced parent quorum is an assertion failure, not
    a normal `None` result, in
    [core.rs](../../core/src/core.rs#L522-L545) and
    [proposer.rs](../../core/src/proposer.rs#L348-L427).
  - Current tip replay is receiver-subscription-driven. Establishment has a
    timeout. An ended or idle stream returns to the retry loop in
    [subscriber.rs](../../core/src/subscriber.rs#L147-L299). A successful
    subscription sends cached own blocks after the receiver's resume round. If
    that list is empty, it sends the latest own block in
    [authority_service.rs](../../core/src/authority_service.rs#L336-L402).
  - Existing tests cover the recovered-own-round wake-up, propagation-delay
    blocker, ended-stream retry, idle-stream retry, stalled-establishment retry,
    and exact cached-or-latest snapshot behavior. See
    `test_core_try_new_block_leader_timeout`,
    `test_last_known_sync_wakes_threshold_clock_round`,
    `test_core_set_propagation_delay_per_authority`, `subscriber_retries`,
    `subscriber_reconnects_when_stream_makes_no_progress`,
    `subscriber_retries_when_subscribing_makes_no_progress`, and
    `test_handle_subscribe_blocks`.
- **Lean evidence:**
  - `operational_frontier_pacemaker_gives_strict_progress` proves one collective
    `H` to `H + 1` or later step from the local pacemaker, exact-or-newer
    subscription tip replay,
    ordinary delivery, current-GC parent processing, and finite correct-stake
    aggregation in
    [ValidatorOperationalFrontierCollectiveSuccessor.lean](../lean/Mysticeti/ValidatorOperationalFrontierCollectiveSuccessor.lean#L1306).
  - `operational_frontier_strict_progress_gives_network_dag_progress` repeats
    the strict step by well-founded induction in
    [ValidatorOperationalFrontierCollectiveSuccessor.lean](../lean/Mysticeti/ValidatorOperationalFrontierCollectiveSuccessor.lean#L1440).
  - `EndToEndLivenessInputs.network_dag_progress` connects the result to the
    public E2E input record in
    [EndToEndLiveness.lean](../lean/Mysticeti/EndToEndLiveness.lean#L494).
  - `goalObsoleteIsAtOrBelowGc` and
    `retained_parent_first_history_eventually_ready` use the GC round that is
    current when a fetch result is processed. They do not preserve or rebase
    an in-flight request across a commit in
    [ValidatorBlockSyncBridge.lean](../lean/Mysticeti/ValidatorBlockSyncBridge.lean#L126)
    and
    [ValidatorBlockSyncBridge.lean](../lean/Mysticeti/ValidatorBlockSyncBridge.lean#L1353).
- **What this evidence does not prove:** It does not prove commit progress,
  favorable leader windows, exact FlexCommitter runs, or durable commit output.
  It also does not prove that every correct validator produces in every round.
  The theorem does not receive a future layer, future block, commit, or
  synchronized install as an input.
- **Required refinement work:** Map the forced-return classification and the two
  watcher retries to `ValidatorNormalFrontierPacemakerRules`. Map subscription
  timeout, stream termination, retry, and the exact-or-newer snapshot result to
  `ValidatorCurrentTipSubscriptionExecution`. Add focused tests for both maps.
- **Revalidation triggers:** Changes to threshold-clock updates, proposal target
  selection, max-timeout behavior, current-tip replay, block-fetch completion,
  GC filtering, parent-first acceptance, operational-frontier source maps, or
  the three named Lean theorems.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## EV-SCHEDULE-HEAD-LOCAL

- **Related assumptions and review IDs:** `ASM-LIVE-LEADER`,
  `REF-LEADER-SCHEDULE`, `REF-ROUND-LEADER-SELECTION`, and
  `REF-LEADER-ORDER-COMPATIBILITY`.
- **Exact claim:** Within one running Core, the proposer waiter and each
  FlexCommitter call receive the current `NextCommitLeaderSchedule` from the
  same `LeaderScheduleV3`. At one `next_commit_index`, the ordered
  `allowed_leaders` list cannot change. After a local or synchronized commit,
  Core refreshes the schedule and gives the refreshed value to the proposer.
  An off-boundary commit keeps `allowed_leaders`. A boundary commit can change
  it. FlexCommitter keeps compatible pending work when the list is unchanged
  and resets schedule-dependent pending work when the list changes.
- **Classification and status:** Current single-process Rust behavior is
  verified. The proposal-attempt-to-Lean schedule read is still an open local
  refinement.
- **Rust evidence:**
  - Core gives the current schedule to each FlexCommitter call in
    [`try_commit_v3`](../../core/src/core.rs#L895-L917).
  - `post_commit` updates `LeaderScheduleV3` and then gives its current schedule
    to the proposer in [core.rs](../../core/src/core.rs#L926-L975).
  - The proposal waiter reads `allowed_leaders` in
    [proposer.rs](../../core/src/proposer.rs#L761-L788).
  - FlexCommitter checks same-index stability and separates unchanged-list
    prefix removal from changed-list reset in
    [flex_committer.rs](../../core/src/flex_committer.rs#L70-L138).
  - `LeaderScheduleV3` recomputes the full list only at a schedule boundary in
    [leader_schedule_v3.rs](../../core/src/leader_schedule_v3.rs#L430-L447).
- **Lean evidence:**
  - `LocalFlexCommitterSourceMap.selectedSlotsMatchConfig` binds an actual Flex
    state to the configured slot order in
    [ValidatorFlexCommitter.lean](../lean/Mysticeti/ValidatorFlexCommitter.lean#L154).
  - `ValidatorFlexPendingRefreshSourceMap.preparedSlotsMatchCurrentConfig`
    binds the prepared scan to the current commit head in
    [ValidatorFlexPendingRefresh.lean](../lean/Mysticeti/ValidatorFlexPendingRefresh.lean#L1127).
  - `causal_commit_head_schedule_for_matches_execution` and
    `adaptive_first_slot_maps_to_execution` bind the sampled first slot to the
    receiver's actual causal head in
    [EndToEndProbabilityCapstone.lean](../lean/Mysticeti/EndToEndProbabilityCapstone.lean#L133)
    and [EndToEndLiveness.lean](../lean/Mysticeti/EndToEndLiveness.lean#L1015).
- **What this evidence does not prove:**
  - It does not prove schedule equality between validators with different
    commit heads.
  - It does not allow the proof to assume that the schedule stayed equal across
    a commit install. The proof must inspect the refreshed schedule.
  - It does not prove that a waited leader is in the proposal's parent list.
  - It does not cover the `force = true` path, which bypasses the waiter.
- **Required proof use after an install:** Recompute the effective schedule key.
  If `allowed_leaders` is unchanged, retain only compatible membership and
  pending-round facts and remove the obsolete committed prefix. If
  `allowed_leaders` changed, reset schedule-dependent facts and start a new
  comparison. Do not remove all old facts only because the commit index moved.
- **Revalidation triggers:** Changes to `LeaderScheduleV3`,
  `NextCommitLeaderSchedule`, `Core::try_commit_v3`, `Core::post_commit`,
  `ProposalLeaderWaiter`, FlexCommitter refresh, schedule update intervals, or
  the Lean schedule-key fields.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## EV-FIRST-FLEX-LEADER-PARENT

- **Related assumptions and review IDs:** `ASM-LIVE-LEADER`,
  `ASM-LIVE-LOCAL-PROPOSAL`, and `REF-RECOVERY-PARENTS`.
- **Exact claim required by the proof:** For an actual non-forced round
  `R + 1` proposal, use the action's exact pre-state and effective schedule. If
  the exact first round-`R` Flex leader for that schedule has one accepted and
  retained representative before parent selection, the proposal includes that
  exact reference as an immediate parent. If an intervening install changes
  the effective schedule, re-evaluate the claim with the new schedule.
- **Classification and status:** Proposed single-validator selection behavior
  and open Rust-to-Lean refinement.
- **Rust evidence:**
  - The non-forced waiter waits for every current allowed leader in
    [proposer.rs](../../core/src/proposer.rs#L761-L788).
  - FlexCommitter selects the first slot from a round-specific permutation of
    the same allowed list in
    [flex_committer.rs](../../core/src/flex_committer.rs#L505-L529).
  - Current score-based ancestor selection can still omit a waited leader after
    another parent quorum is available. See
    [proposer.rs](../../core/src/proposer.rs#L130-L245). A forced proposal also
    bypasses the waiter in
    [proposer.rs](../../core/src/proposer.rs#L348-L426). Thus, current Rust does
    not yet enforce the exact claim.
- **Lean evidence:**
  - `selectedLeaderFromSchedule` proves that an actual selected slot belongs to
    the configured schedule in
    [ValidatorProcess.lean](../lean/Mysticeti/ValidatorProcess.lean#L46).
  - The current `ValidatorAnchorLocalRules` field is wider than the required
    rule because it covers every correct accepted immediate parent and does not
    bind the proposal action's schedule head. See
    [ValidatorAnchorBridge.lean](../lean/Mysticeti/ValidatorAnchorBridge.lean#L350).
- **What this evidence does not prove:** Schedule-set overlap is not enough. The
  overlap must contain the receiver's exact first Flex leader, and a quorum of
  actual next-round child blocks must reference that exact block. This entry
  does not assume a future proposal, child quorum, Flex run, or commit.
- **Revalidation triggers:** Changes to proposer waiting, smart ancestor
  selection, propagation scoring, forced proposal behavior, recovery parent
  selection, selected-slot ordering, or the action-scoped proposal snapshot.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## EV-FIRST-SLOT-PROBABILITY

- **Related assumptions and review IDs:** `ASM-LIVE-FIRST-SLOT-SAMPLING`,
  `ASM-LIVE-LEADER`, and `REF-LEADER-ORDER-COMPATIBILITY`.
- **Exact claim:** During one stable proof interval, independent uniform round
  orders have a fixed positive conditional probability of producing the
  required adjacent correct, available first-slot window. The probability that
  no such window occurs tends to zero.
- **Classification and status:** Accepted ideal probability model. It is not a
  verified current Rust property.
- **Rust evidence:** Current FlexCommitter uses a deterministic round-seeded
  shuffle in [flex_committer.rs](../../core/src/flex_committer.rs#L505-L529).
  Different deterministic seeds do not establish independent random samples or
  a deterministic coverage bound.
- **Lean evidence:** `IndependentUniformRoundRankingLaw` and
  `adaptive_viable_schedule_has_favorable_windows_probability_one` state and
  use the ideal law in
  [EndToEndLiveness.lean](../lean/Mysticeti/EndToEndLiveness.lean#L663).
  `DeterministicCausalHeadCompositionGap` remains a temporary composition
  premise in
  [EndToEndProbabilityCapstone.lean](../lean/Mysticeti/EndToEndProbabilityCapstone.lean#L395).
- **What this evidence does not prove:** It does not prove that the current Rust
  shuffle has independent outputs, has the required weighted coverage, or
  supplies the deterministic execution theorem after a favorable window.
- **Revalidation triggers:** Changes to the Rust random generator, shuffle seed,
  schedule membership, schedule update rule, Lean probability law, favorable
  window length, or indirect commit depth.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## EV-EXACT-COMMIT-PREFIX

- **Related assumptions and review IDs:** `ASM-SAFE-COMMIT-CHAIN`,
  `REF-COMMON-COMMIT-CHAIN`, and `REF-COMMIT-SYNC-CHECKS`.
- **Exact claim:** Given exact authenticated decision evidence, canonical commit
  materialization, a complete local durable prefix, and exact local or verified
  synchronized install provenance, Lean derives one unique exact successor from
  each exact prior head. It then derives that correct validators cannot install
  different commit references at the same index.
- **Classification and status:** The exact-prefix induction is proved in Lean.
  Its Rust source maps remain partially verified.
- **Rust evidence:**
  - FlexCommitter constructs the commit body and digest in
    [flex_committer.rs](../../core/src/flex_committer.rs#L315-L427).
  - `DagState::add_commit` enforces next-index installation in
    [dag_state.rs](../../core/src/dag_state.rs#L1040-L1112).
  - Commit sync checks continuous indices, digest links, exact references, and
    certified-tip quorum support in
    [commit_syncer.rs](../../core/src/commit_syncer.rs#L820-L905).
- **Lean evidence:**
  - `correct_local_flex_runs_same_prior_exact_output` proves exact same-prior
    output agreement in
    [ExactCommitPrefixSafety.lean](../lean/Mysticeti/ExactCommitPrefixSafety.lean#L1272).
  - `exactInstalledHeadHasPath`, `exactInstalledHeadsAtSameIndexAgree`,
    `storedIdsAtSameIndexAgree`, and `exactHeadAtOrBelowLocalHeadIsStored` build
    and compare exact finite commit paths in
    [ExactCommitPrefixSafety.lean](../lean/Mysticeti/ExactCommitPrefixSafety.lean#L1891).
- **What this evidence does not prove:** It does not derive
  `AuthenticatedFlexVoteSourceMap`, canonical materialization,
  `ExactCommitDurablePrefixSourceMap`, or `ExactCommitInstallProvenance` from
  Rust. It does not connect the exact consensus prefix to the transaction
  finalizer stream. A common chain must remain a theorem result, not an input.
- **Revalidation triggers:** Changes to Flex decision rules, commit body order,
  serialization, hashing, local install checks, commit sync verification,
  restart loading, or the named Lean source maps and theorems.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## EV-CACHED-INDIRECT-ORIGIN

- **Related assumptions and review IDs:**
  `ASM-SAFE-EVIDENCE-REFINEMENT` and `REF-DECISION-ORIGIN`.
- **Exact claim:** The first direct or indirect result for one slot is sticky.
  For an indirect result, the model also needs the exact deciding anchor, the
  ordered scan position, the historical result, and the immutable anchor
  history that produced the first result.
- **Classification and status:** The sticky direct-or-indirect tag is verified
  in Rust. The exact indirect origin is a known mapping gap.
- **Rust evidence:** `RoundState::update_slot_decision` changes only an
  undecided slot, stores the first `Decision`, and checks equality on later
  evaluation in
  [flex_committer.rs](../../core/src/flex_committer.rs#L557-L590).
  `Decision::Indirect` stores no anchor or history in
  [commit.rs](../../core/src/commit.rs#L502-L506).
- **Lean evidence:** `AuthenticatedFlexVoteSourceMap.firstDecisionProvenance`
  requires the exact first origin in
  [ExactCommitPrefixSafety.lean](../lean/Mysticeti/ExactCommitPrefixSafety.lean#L698).
  `ValidatorFlexUsedIndirectCommit` uses the exact selected anchor and history
  in
  [ValidatorFlexScanEvidence.lean](../lean/Mysticeti/ValidatorFlexScanEvidence.lean#L20).
- **What this evidence does not prove:** A stored `Indirect` tag does not prove
  which anchor was used. It also does not prove that restart or a schedule reset
  reconstructs the same first origin. Do not replace this local origin with a
  cross-validator agreement assumption.
- **Required implementation or proof change:** Store the exact immutable origin,
  or add a checked same-host trace reconstruction that proves the original
  anchor and history.
- **Revalidation triggers:** Changes to `Decision`, `LeaderSlot`, direct or
  indirect scan order, pending-state reset, cached result replay, restart, or
  `firstDecisionProvenance`.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## EV-DURABLE-COMMIT-PREFIX

- **Related assumptions and review IDs:** `ASM-SAFE-COMMIT-CHAIN`,
  `ASM-SAFE-GC`, `ASM-SAFE-NON-EQUIVOCATION`, `REF-COMMON-COMMIT-CHAIN`,
  `REF-COMMIT-STATE`, `REF-COMMIT-INSTALL-DAG`, and
  `REF-AMNESIA-SIGNER-GUARD`.
- **Exact claim:** On the normal storage path, a local commit appends one exact
  next entry. `DagState::flush` writes pending blocks, commits, and finalizer
  rows in one store batch. Normal restart reads a continuous stored prefix and
  replays unfinalized entries in order. Lean requires the same prefix, bodies,
  install sources, and signer floor in its initial recovered state.
- **Classification and status:** Main Rust write ordering and normal replay are
  verified. The complete Rust-to-Lean storage and positive-head restart mapping
  is open.
- **Rust evidence:**
  - Commit append and pending storage are in
    [dag_state.rs](../../core/src/dag_state.rs#L1040-L1112).
  - Atomic batch construction is in
    [dag_state.rs](../../core/src/dag_state.rs#L1240-L1292), and the RocksDB
    batch write is in
    [rocksdb_store.rs](../../core/src/storage/rocksdb_store.rs#L158-L216).
  - Continuous replay and gap checks are in
    [commit_observer.rs](../../core/src/commit_observer.rs#L144-L225).
- **Lean evidence:** `ExactCommitDurablePrefixSourceMap` and
  `ExactCommitInstallProvenance` state the required local prefix and origin
  mappings in
  [ExactCommitPrefixSafety.lean](../lean/Mysticeti/ExactCommitPrefixSafety.lean#L1461).
- **What this evidence does not prove:** The current E2E model starts correct
  validators at the index-zero genesis. It does not yet model a mid-epoch
  positive-head restart. Complete consensus-store loss is also outside the
  normal replay guarantee. Peer recovery alone is not a safety root for all
  earlier signatures or the exact commit prefix.
- **Required implementation or proof change:** Add a recovered-prefix initial
  relation or trusted checkpoint boundary. After complete state loss, restore
  an external durable signer floor and commit head, stop same-epoch signing,
  rotate the epoch key, or count the validator as faulty.
- **Revalidation triggers:** Changes to commit append, store batches, block or
  commit deletion, restart loading, replay range checks, signer-floor recovery,
  complete-state-loss recovery, or the Lean genesis and prefix inputs.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## EV-FINALIZER-TRIGGER-OUTPUT

- **Related assumptions and review IDs:** `ASM-SAFE-FIRST-TRIGGER`,
  `ASM-SAFE-COMMITTED-PREFIX`, `ASM-LIVE-FINALIZER-TRIGGER`,
  `ASM-LIVE-DURABILITY`, `REF-GC-EVIDENCE`, and `REF-FINALIZER-TAIL`.
- **Exact claim needed by the final system theorem:** The exact installed commit
  prefix determines one exact transaction finalizer stream. Each validator
  selects the least eligible trigger in its visible prefix. Every required vote
  witness is in the committed sub-DAG. The decision and its complete evidence
  are durable before exposure. Restart replays the unfinalized suffix, and the
  consumer applies duplicate output idempotently with a durable contiguous
  cursor.
- **Classification and status:** Rust has verified write-before-output and
  ordered replay behavior. The full consensus-prefix-to-finalizer proof and the
  epoch-tail liveness rule are open.
- **Rust evidence:** CommitFinalizer records newly finalized data and flushes
  before it sends output in
  [commit_finalizer.rs](../../core/src/commit_finalizer.rs#L135-L169).
  CommitObserver scans a continuous stored range and reconstructs the
  unfinalized suffix in
  [commit_observer.rs](../../core/src/commit_observer.rs#L144-L225).
- **Lean evidence:**
  - `first_trigger_agreement` proves a pure result for one shared abstract stream
    and certificate function in
    [CommitChain.lean](../lean/Mysticeti/CommitChain.lean#L121).
  - `TransactionEvidence.anchorInCommittedPrefix` is still an input in
    [Finalizer.lean](../lean/Mysticeti/Finalizer.lean#L122).
  - `FinalizerLivenessStageObligations.decisionToDurableOutput` is still a stage
    input in [Liveness.lean](../lean/Mysticeti/Liveness.lean#L246).
- **What this evidence does not prove:** It does not derive one shared finalizer
  stream from independently recovered validator prefixes. It does not prove
  committed-prefix witness inclusion, an eventual trigger, consumer
  idempotence, or a result for unresolved pending state at epoch shutdown. None
  of these future results can be an E2E input.
- **Revalidation triggers:** Changes to trigger eligibility, commit sub-DAG
  construction, transaction vote classification, finalizer GC, finalizer store
  rows, output ordering, restart replay, consumer acknowledgement, or epoch
  shutdown.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## Focused revalidation procedure

1. Diff the changed product files against the source revision in the affected
   entries.
2. Recheck only the entries whose revalidation triggers match those changes.
3. Run the relevant Rust tests and Lean builds.
4. Update the evidence, limits, date, and revision.
5. Change an assumption status only after the evidence supports the complete
   claim.

Line numbers are navigation aids. Declaration and function names define the
evidence boundary.
