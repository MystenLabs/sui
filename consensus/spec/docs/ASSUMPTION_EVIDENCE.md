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

- **Review date:** 2026-08-18.
- **Product and proof base revision:**
  `2e05fcf9cbeba4d42b0cc4145312ae053dba14dc` on
  `tmw/mysticeti-v3-lean-main`. This evidence update does not change proof or
  product logic.
- **Scope:** network round progress, fixed-reference pacing, V2 round catch-up,
  V2 current no-idle block production, pinned sync, commit-orthogonal
  retention, local Flex execution, exact-prefix induction, commit-sync safety
  and ordinary-sync failover, the non-adopted exact-replay proof experiment,
  leader schedule and probability evidence, cached decision origin, commit
  storage and restart, and transaction finalizer durability.

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

## EV-FIXED-REFERENCE-END-TO-END

- **Related assumptions and review IDs:** `ASM-LIVE-ROUND-CATCHUP`,
  `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`, `ASM-LIVE-LOCAL-PROPOSAL`,
  `ASM-LIVE-FIRST-SLOT-SAMPLING`, `ASM-LIVE-POST-GST-CAUSAL-SERVICE`,
  `ASM-LIVE-BLOCK-SYNC`, `REF-RECOVERY-PACING`, `REF-ROUND-CATCHUP`,
  `REF-POST-GST-CAUSAL-SERVICE`, and `REF-PARENT-SYNC`.
- **Exact claim:** Under the independent-uniform ranking law and the listed
  local current or past source maps, ordinary DAG behavior gives probability-one
  end-to-end liveness. The current green proof uses one fixed-reference
  quadratic wait, timer spread derived from actual prior broadcasts and pinned
  sync, V2 no-skip round catch-up, V2 current no-idle block production,
  commit-orthogonal retention, local FlexCommitter execution, and exact-prefix
  induction. It does not use future commit-sync service, commit votes, or exact
  replay.
- **Classification and status:** The full conditional composition and strict
  timing derivation are proved in Lean. The strict source record uses one
  proposed action-local exact-next timer-promptness rule. It does not contain a
  future timer or production. Several other source rules are proposed behavior
  or incomplete Rust-to-Lean refinements. This entry does not claim that current
  Rust satisfies them.
- **Lean evidence:**
  - `current_sources_give_end_to_end_liveness_probability_one` is the final
    theorem in
    [ValidatorFixedReferenceCurrentPacing.lean](../lean/Mysticeti/ValidatorFixedReferenceCurrentPacing.lean).
  - `current_sources_give_derived_receiver_progress_probability_one` derives
    receiver-local progress from the source package and the favorable ranking
    event.
  - `strict_v2_backfill_and_favorable_path_give_fixed_reference_direct_range`
    derives the timer spread, exact adjacent parent evidence, and the
    receiver-local direct range from actual V2 productions, pinned sync, and
    the strict current-source record.
  - `ValidatorFixedReferenceStrictCurrentSourceMaps` contains only the fixed
    wait mapping, action-local exact-next promptness, authenticated correct-body
    ownership, past timer-origin mapping, and the checked quadratic coefficient.
  - `ValidatorV2BlockProductionCurrentSourceMaps.blockProductionLiveness`
    derives semantic unbounded own-block production from the selected-support,
    recursive-need, queue-source, and no-idle rules.
  - `block_production_liveness_gives_backfilled_timer_paced_window` derives the
    finite exact production family from one actual later block and past-only
    commit-progress-recovery timer origins in
    [ValidatorV2RoundCatchup.lean](../lean/Mysticeti/ValidatorV2RoundCatchup.lean).
  - `derived_receiver_fixed_reference_progress_proves_end_to_end_goal` uses
    local Flex progress and exact-prefix induction in
    [ValidatorFixedReferenceNetworkCommitCapstone.lean](../lean/Mysticeti/ValidatorFixedReferenceNetworkCommitCapstone.lean).
- **Rust evidence:** Current Rust has ordinary proposal, persistence,
  broadcast, recursive block fetch, parent-first acceptance, local
  FlexCommitter execution, and durable commit installation. Current Rust does
  not implement the fixed-reference quadratic wait, no-skip proposal sequence,
  or all source maps used by the theorem.
- **What this evidence does not prove:** It does not prove current product
  conformance. It does not prove the independent-uniform law for the current
  deterministic shuffle. It does not prove transaction liveness. Exact replay
  remains a separate non-adopted experiment.
- **Required refinement work:** Implement or map the fixed-reference wait and
  action-local exact-next timer promptness. Implement V2 no-skip round catch-up.
  Complete the authenticated-body ownership, past timer-origin,
  selected-support, recursive-need, queue-source, no-idle, pinned-sync,
  retention, action-scoped leader-parent, local Flex, and exact-prefix source
  mappings. Keep commit sync outside the liveness premises.
- **Revalidation triggers:** Changes to fixed-reference wait parameters,
  timer-spread derivation, proposal round selection, timer origin, V2 no-idle sources,
  pinned block sync, retention across commit and GC, proposal parent selection,
  Flex pending refresh, exact-prefix induction, ranking law, or the three named
  Lean modules.
- **Audit date and source revision:** 2026-08-18 at
  `693cc4592c19a2471580ec2b176e3f4841f7a7bd`.

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

## EV-ROUND-CATCHUP

- **Related assumptions and review IDs:** `ASM-LIVE-ROUND-CATCHUP`,
  `ASM-LIVE-LOCAL-PROPOSAL`, `REF-ROUND-CATCHUP`,
  `REF-RECOVERY-TIMER-ORIGIN`, and `REF-RECOVERY-PARENTS`.
- **Exact claim:** For an active correct validator, each actual proposal
  persistence uses exactly one round after the pre-action durable signer floor.
  If that persistence is fresh relative to an earlier observation, it has one
  exact earlier timer generation, proposal action, and refreshed parent
  snapshot for the same block. These are current or past trace facts. Given one
  actual later own block, the first signer-floor crossing then identifies each
  requested intermediate persistence.
- **Classification and status:** Proposed Rust behavior and known mismatch.
  Lean proves the finite production family only from the two local source
  fields. Current Rust does not satisfy the no-skip or fixed-target timer rule.
- **Rust evidence:**
  - `ValidatorProposer::try_new_block` reads the current
    `threshold_clock_round()` and requires only that it is higher than the last
    own proposal. It can therefore create a block after one or more skipped own
    rounds in
    [proposer.rs](../../core/src/proposer.rs#L348-L385).
  - `Core::new_block` uses its callback round only for a stale-round check. It
    calls `try_propose`, which reads the current threshold-clock round again in
    [core.rs](../../core/src/core.rs#L522-L545) and
    [proposer.rs](../../core/src/proposer.rs#L361-L385). The callback does not
    bind the proposal to its requested round.
  - The minimum and maximum timeout callbacks fire once for one observed round.
    A higher round resets both timers in
    [leader_timeout.rs](../../core/src/leader_timeout.rs#L63-L116). The task
    does not retain one timer generation for each skipped intermediate round.
- **Lean evidence:**
  - `ValidatorV2RoundCatchupSourceMap.correctPersistIsExactNext` states the
    action-local no-skip rule.
    `freshExactNextPersistHasTimerOrigin` maps the same actual persistence to
    its past fixed timer and proposal snapshot in
    [ValidatorV2RoundCatchup.lean](../lean/Mysticeti/ValidatorV2RoundCatchup.lean#L32-L68).
  - `actual_high_own_block_gives_fresh_timer_paced_intermediate` selects the
    first signer-floor crossing and proves that its block has the requested
    exact round. It then derives the exact timer-paced production and addressed
    broadcasts in
    [ValidatorV2RoundCatchup.lean](../lean/Mysticeti/ValidatorV2RoundCatchup.lean#L96-L237).
  - `block_production_liveness_gives_backfilled_timer_paced_window` applies the
    pointwise result to each correct, available author and each finite offset.
    Its output converts directly to
    `ValidatorFreshTimerPacedExactRoundFamily` in
    [ValidatorV2RoundCatchup.lean](../lean/Mysticeti/ValidatorV2RoundCatchup.lean#L245-L340).
- **What this evidence does not prove:** It does not derive V2 unbounded
  own-block production. It does not give common retention, receiver acceptance,
  adjacent parent edges, a favorable leader window, a Flex result, or a commit.
  It also does not show that current Rust fills skipped own rounds.
- **Required refinement work:** Add an ordered intermediate-proposal worker, or
  an equivalent safe queue, for each skipped target. Keep one exact timer key,
  proposal action, and refreshed parent snapshot until each target persists.
  Preserve the queue across commit processing and restart. Add focused tests
  for threshold jumps, timer replacement, commit interference, and restart.
- **Revalidation triggers:** Changes to threshold-clock updates, signer-floor
  persistence, proposal target selection, timeout reset behavior, recovery
  timer keys, refreshed parent selection, or the three named Lean declarations.
- **Audit date and source revision:** 2026-08-18 at
  `693cc4592c19a2471580ec2b176e3f4841f7a7bd`.

## EV-POST-GST-CAUSAL-SERVICE

- **Related assumptions and review IDs:**
  `ASM-LIVE-POST-GST-CAUSAL-SERVICE`, `ASM-LIVE-BLOCK-SYNC`,
  `ASM-LIVE-LOCAL-PROPOSAL`, `REF-POST-GST-CAUSAL-SERVICE`,
  `REF-PARENT-SYNC`, and `REF-LOCAL-PROPOSAL-PROGRESS`.
- **Exact claim:** After GST, use one fixed service interval and finite bounds
  `C_add < C_service` for one correct, available validator. Advancing rounds add
  at most `C_add` required above-GC references to its causal-work queue in one
  interval. If at least `C_service` items are pending, fetch, verification, and
  acceptance remove at least `C_service` items. If fewer items are pending, all
  pending items finish or become obsolete because GC moved. Thus each fixed
  known above-GC history eventually becomes accepted. The validator can skip its
  own rounds. It must still store and send own blocks at later unbounded rounds.
  The claim does not require one own block in each intermediate round.
- **Classification and status:** Accepted performance model for the adopted
  ordinary-DAG route. The exact rate comparison is not verified in Rust. The
  conditional Lean composition is complete.
- **Rust evidence:**
  - `BlockManager` uses the receiver's current GC round, does not request
    references at or below it, and processes above-GC parent needs in
    [block_manager.rs](../../core/src/block_manager.rs#L212-L221) and
    [block_manager.rs](../../core/src/block_manager.rs#L279-L374).
  - The proposer always puts its last own block first in the ancestor list and
    can propose at a later round without filling every skipped own round in
    [proposer.rs](../../core/src/proposer.rs#L141-L180) and
    [proposer.rs](../../core/src/proposer.rs#L348-L427).
  - Current source and tests show batching, retry, GC filtering, and later-round
    proposal mechanisms. They do not establish a strict queue-service margin
    under the fastest permitted round creation.
- **Lean evidence:**
  - `BlockProductionLiveness` states unbounded later own-block production for
    each correct, available validator. It does not require contiguous own rounds
    in [ValidatorProcess.lean](../lean/Mysticeti/ValidatorProcess.lean).
  - `ValidatorV2BlockProductionCurrentSourceMaps.blockProductionLiveness`
    derives this property from selected support, recursive need, queue-source,
    and no-idle fields in
    [ValidatorFixedReferenceCurrentPacing.lean](../lean/Mysticeti/ValidatorFixedReferenceCurrentPacing.lean).
    These are proposed scheduler and source-refinement rules.
  - `block_production_liveness_gives_backfilled_timer_paced_window` uses the
    V2 no-skip source to recover the finite exact round family that the selected
    favorable path needs. Pinned sync and commit-orthogonal retention make the
    selected leaders usable at the receiver. Local Flex and exact-prefix
    induction complete the conditional theorem.
- **What this evidence does not prove:** It does not prove one own block per
  round, a fixed round-lag bound, or a permanently empty causal queue. It does
  not prove the strict service margin, GC replacement, or exact phase
  continuity for current Rust. It does not supply a future block, carrier,
  anchor, Flex run, or install as an input.
- **Required refinement work:** Define the queue-work unit, service interval,
  `C_add`, and `C_service`. Add overload and adversarial tests. Map the V2
  selected-support, recursive-need, queue-source, and no-idle rules. Complete
  the pinned-sync and retention mappings used by the final theorem.
- **Revalidation triggers:** Changes to round pacing, proposal target selection,
  causal-history construction, parent fetch, fetch batching, verification,
  acceptance, GC filtering, worker scheduling, resource limits, or the named
  Lean liveness interfaces.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## EV-FINITE-REFERENCE-SPACE-TIMING

- **Related assumptions and review IDs:**
  `ASM-LIVE-FINITE-REFERENCE-SPACE`,
  `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`, `ASM-LIVE-BLOCK-SYNC`,
  `REF-FINITE-BLOCK-ID-SPACE`, `REF-CAUSAL-CAPSULE-PROJECTION`,
  `REF-RECOVERY-PACING`, `REF-RECOVERY-TIMER-ORIGIN`,
  `REF-FLEX-ACCEPTED-BODY-OWNERSHIP`, and
  `REF-FLEX-POST-REFRESH-INPUT`.
- **Exact claim:** Let `M = authorityCount * blockIdCount`. One exact persisted
  causal capsule has at most `M` unique references at each round. If its target
  is in round `R` and the receiver GC round is `G`, the unresolved part has at
  most `(R - G) * M` items. Let `B` be the fixed acceptance bound for one sync
  item. The adopted theorem uses one fixed reference round and one quadratic
  wait. The proof derives an initial finite timer spread from actual proposals.
  It then derives each successor bound from actual broadcasts, pinned block
  sync, the finite admission cap, partial synchrony, retention, and one
  action-local timer-promptness rule. A sufficiently late adjacent wait margin
  covers causal visibility, timer spread, and the fixed pipeline cost.
- **Classification and status:** The finite Rust block-reference space and the
  Lean arithmetic are verified. The quadratic wait is proposed behavior. The
  exact capsule, timer-origin, accepted-body ownership, and post-refresh input
  mappings are current or past source refinements that still need a complete
  Rust-to-Lean review.
- **Rust evidence:**
  - `BlockDigest` is the fixed byte array
    `[u8; consensus_config::DIGEST_LENGTH]` in
    [block.rs](../../types/src/block.rs#L78-L90). Together with the finite
    committee, this gives a finite reference space. The resulting bound is very
    large. It is a sound liveness bound, not a practical resource limit.
  - Block signatures bind the full block body, and signature verification checks
    the author key in [block.rs](../../core/src/block.rs#L477-L490) and
    [block.rs](../../core/src/block.rs#L524-L550).
  - `FlexCommitter::try_commit` refreshes pending state before it runs the direct
    and indirect scans in
    [flex_committer.rs](../../core/src/flex_committer.rs#L50-L67).
  - Current leader timers use fixed configured durations and reset when the
    observed round changes in
    [leader_timeout.rs](../../core/src/leader_timeout.rs#L63-L116). Current Rust
    does not implement the fixed-reference quadratic wait.
- **Lean evidence:**
  - `validator_causal_history_items_at_round_le_finite_reference_space` proves
    the per-round cap, and
    `ValidatorPersistedCausalCapsuleFiniteReferenceSourceMap.toRoundAdmission`
    supplies it to the backlog proof in
    [ValidatorFiniteReferenceSpaceAdmission.lean](../lean/Mysticeti/ValidatorFiniteReferenceSpaceAdmission.lean).
  - `accepted_capsule_target_gives_receiver_cutoff` proves that every item in
    the exact capsule of an accepted target is accepted or at or below the
    receiver GC boundary in
    [ValidatorAcceptedCapsuleCutoff.lean](../lean/Mysticeti/ValidatorAcceptedCapsuleCutoff.lean).
    `unresolved_le_linear_backlog` and `history_ready_within_linear_backlog`
    give the quantitative receiver bound in
    [ValidatorReceiverRelativeCausalBacklog.lean](../lean/Mysticeti/ValidatorReceiverRelativeCausalBacklog.lean).
  - `ValidatorPersistedCausalCapsulePinProjectionRules` identifies the capsule
    added by an actual earlier proposal persistence action with the exact static
    projection in
    [ValidatorFreshRoundPinnedSyncSource.lean](../lean/Mysticeti/ValidatorFreshRoundPinnedSyncSource.lean).
  - `current_timer_input_gives_bounded_start_or_receiver_commit_advance` derives
    the current timer choice.
    `ValidatorTimerPacedRecoveryOriginRules` maps an actual timer-paced proposal
    to its exact earlier timer and gives key uniqueness in
    [ValidatorFreshTimerReadyBridge.lean](../lean/Mysticeti/ValidatorFreshTimerReadyBridge.lean).
  - `ValidatorAuthenticatedAcceptedBodyOwnershipRules` maps a current accepted
    and catalogued authenticated body from a correct author to the author's
    durable block. `correct_initial_quorum_parent_has_past_persist_origin` then
    derives its initial or exact earlier persistence origin in
    [ValidatorTimerSpreadRecurrence.lean](../lean/Mysticeti/ValidatorTimerSpreadRecurrence.lean).
  - `fresh_timer_paced_exact_round_gives_concrete_timer_start_successor_upper`
    derives each receiver-local successor bound from actual broadcasts and
    pinned synchronization in
    [ValidatorConcreteSuccessorReadiness.lean](../lean/Mysticeti/ValidatorConcreteSuccessorReadiness.lean).
    `strict_v2_backfill_and_favorable_path_give_fixed_reference_direct_range`
    fixes the base spread and late threshold before it selects the favorable
    suffix in
    [ValidatorFixedReferenceCurrentPacing.lean](../lean/Mysticeti/ValidatorFixedReferenceCurrentPacing.lean).
  - `ValidatorFlexPendingRefreshSourceMap.actualRunInternalInputIsPrepared`
    identifies the literal post-refresh input.
    `actualRunResultReconstructsFromInternalInput` derives result equality from
    the two views of the same actual action in
    [ValidatorFlexPendingRefresh.lean](../lean/Mysticeti/ValidatorFlexPendingRefresh.lean).
- **What this evidence does not prove:** It does not prove the required
  block-sync service or derived timer-spread bounds for current Rust. It does not
  claim that the current Rust timer is quadratic. It does not assume a future
  block, timer, favorable window, Flex run, or commit result.
- **Required refinement work:** Implement and configure the fixed-reference
  quadratic wait. Check its coefficient conditions with product integer limits.
  Complete the exact current or past source maps for the persisted capsule,
  timer origin, accepted-body ownership, timer-spread sources, and literal
  Flex input.
- **Revalidation triggers:** Changes to `BlockDigest`, authority indexing,
  causal-capsule construction, reference deduplication, proposal persistence or
  source pins, GC filtering, block-sync acceptance bounds, timer keys, recovery
  waits, block authentication, pending refresh, or the named Lean theorems.
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
  `all_validator_causal_head_favorable_windows_probability_one` derives the
  favorable event. `current_sources_give_end_to_end_liveness_probability_one`
  transfers that event through the fixed-reference ordinary-DAG capstone in
  [ValidatorFixedReferenceCurrentPacing.lean](../lean/Mysticeti/ValidatorFixedReferenceCurrentPacing.lean).
- **What this evidence does not prove:** It does not prove that the current Rust
  shuffle has independent outputs, has the required weighted coverage, or
  follows the ideal probability law. The probability-one theorem is
  conditional on the fixed-reference, V2 production, no-skip, pinned-sync,
  retention, local Flex, and exact-prefix source maps for each sampled
  execution. It does not require exact replay.
- **Revalidation triggers:** Changes to the Rust random generator, shuffle seed,
  schedule membership, schedule update rule, Lean probability law, favorable
  window length, or indirect commit depth.
- **Audit date and source revision:** 2026-08-17 at
  `2fecfec37462785ccd6684195aac9131e54ad251`.

## EV-COMMIT-SYNC-COVERAGE

- **Related assumptions and review IDs:** `ASM-LIVE-COMMIT-SYNC`,
  `ASM-LIVE-BLOCK-SYNC`, `ASM-LIVE-POST-GST-CAUSAL-SERVICE`,
  `REF-COMMIT-SYNC-CHECKS`, `REF-COMMIT-SYNC-PROGRESS`,
  `REF-BLOCK-SYNC-MECHANISMS`, `REF-LOCAL-PROPOSAL-PROGRESS`, and
  `REF-RECOVERY-GC-FRONTIER`.
- **Exact claim:** Commit sync is safe when an actual synchronized install has
  the checked exact-chain provenance. Commit-sync success is not a liveness
  premise. If a synchronized install increases one receiver's local commit
  index, it satisfies that receiver's progress branch. If the index does not
  increase, ordinary synchronization and recovery remain the liveness path.
  Commit-sync work must not starve those ordinary tasks.
- **Classification and status:** Exact synchronized-install safety and the
  source-independent receiver split are proved in Lean. The Rust subscription
  resume and periodic-sync failover transitions are verified and covered by
  focused tests. Eventual progress through those transitions still uses the
  accepted non-starvation rule and the existing partial-synchrony,
  peer-fairness, task-fairness, block-sync, and queue-service assumptions.
- **Rust evidence:**
  - `CommitSyncer::fetch_once` fetches the certified range, verifies it, fetches
    every exact block reference in each commit, and checks each returned
    reference in
    [commit_syncer.rs](../../core/src/commit_syncer.rs#L540-L720).
  - `verify_commits` checks the requested start, consecutive indices, digest
    links, full block verification, distinct-author vote aggregation, and quorum
    support for the range tip in
    [commit_syncer.rs](../../core/src/commit_syncer.rs#L820-L905).
  - Core filters synchronized commits to the next local index, checks the
    previous digest, accepts the exact commit blocks before each install, then
    runs `try_propose(false)` and `try_signal_new_round` in
    [core.rs](../../core/src/core.rs#L429-L466),
    [core.rs](../../core/src/core.rs#L548-L590), and
    [core.rs](../../core/src/core.rs#L796-L868).
  - Subscription connection attempts retry with bounded exponential backoff.
    Commit-lag suspension checks once per second and resumes inside a one-batch
    lag band in
    [observer_subscriber.rs](../../core/src/observer_subscriber.rs#L230-L304)
    and
    [observer_subscriber.rs](../../core/src/observer_subscriber.rs#L405-L468).
  - Periodic ordinary sync resumes after ten seconds without a local
    commit-index change and stays in failover until one batch of local progress
    occurs in
    [synchronizer.rs](../../core/src/synchronizer.rs#L936-L949) and
    [synchronizer.rs](../../core/src/synchronizer.rs#L1065-L1140).
  - After commit installation changes GC, BlockManager removes obsolete missing
    dependencies and accepts children that no longer need those dependencies in
    [block_manager.rs](../../core/src/block_manager.rs#L457-L529).
- **Lean evidence:**
  - `VerifiedSyncInstalledExactOrigin` and
    `ExactCommitInstallProvenance.verifiedSyncInstallOrigin` constrain each
    actual verified-sync install in
    [ExactCommitPrefixSafety.lean](../lean/Mysticeti/ExactCommitPrefixSafety.lean#L1639-L1743).
  - `favorable_event_and_current_sources_give_derived_receiver_progress`
    splits on source-independent `ValidatorReceiverCommitAdvance`. A local
    synchronized advance closes the left branch. The negative branch contains
    no local commit-index advance and derives the ordinary fixed-reference path
    in
    [ValidatorFixedReferenceCurrentPacing.lean](../lean/Mysticeti/ValidatorFixedReferenceCurrentPacing.lean#L1125-L1165).
- **Focused tests:**
  - `test_suspend_subscription_on_commit_lag_and_resume` passes.
  - `synchronizer_periodic_sync_resumes_when_commit_sync_stalled` passes.
  - All three `unsuspend_blocks_for_latest_gc_round` cases pass.
  - `commit_syncer_observer_node_basic`,
    `commit_syncer_observer_with_multiple_peers`, and
    `commit_syncer_start_and_pause_scheduling` pass.
  - `add_certified_commits_v3` and `add_certified_commits_v3_gced_blocks` pass.
- **What this evidence does not prove:** The suspension loop has no fixed
  maximum duration without local catch-up. The failover control path does not
  prove that a correct peer serves each request or that tasks receive enough
  CPU, network, and queue capacity. GC cleanup resolves obsolete data
  dependencies, but it does not by itself re-arm the proof's exact no-skip
  recovery phase. The accepted non-starvation rule and the existing recovery
  no-idle and safe-resume refinements cover these limits. Commit sync also does
  not replace recursive ordinary-DAG synchronization.
- **Revalidation triggers:** Changes to commit-range verification, certified
  install order, commit-source provenance, subscription suspension thresholds,
  subscription retry, periodic-sync gating or failover, GC unsuspension, Core
  proposal triggering after commit installation, or the receiver-progress
  split.
- **Audit date and source revision:** 2026-08-18 at
  `2e05fcf9cbeba4d42b0cc4145312ae053dba14dc`.

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
