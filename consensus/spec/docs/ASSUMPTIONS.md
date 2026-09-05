<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 proof assumptions and product obligations

This ledger records the conditions that connect the formal model to the product
and its operating environment. A theorem is conditional even when the proof has
no declared axiom. Apply it to the product only when each related condition is
satisfied or explicitly accepted.

The [assumption evidence ledger](ASSUMPTION_EVIDENCE.md) records the exact Rust
and Lean evidence, limits, source revisions, and focused revalidation triggers
for reviewed assumptions. The [implementation gap report](IMPLEMENTATION_GAPS.md)
records missing product work.

The v3 transaction path is in progress on the branch
`tmw/mysticeti-v3-transaction-voting`, and the product intends to merge it.
A row that cites that branch is verified against the branch code, and it must
be revalidated when the branch merges.

## Assumption boundary

A basic environment assumption describes one simple fact, such as bounded message
delivery, bounded local work, fair task execution, durable storage, a fault set,
or a probability distribution.

A quorum entering recovery, a sequence of block layers, a usable anchor, or a new
commit is not a basic assumption. The protocol proof must derive each such result.
The proof must show that each fixed product rule matches the model.

The network-round theorem meets this boundary. It derives each future quorum
layer from local current-state and execution rules. The adopted commit route
uses ordinary DAG propagation, fixed-reference quadratic waits, V2 no-skip
round catch-up, pinned block sync, commit-orthogonal retention, local
FlexCommitter execution, and exact-prefix induction. The theorem
`current_sources_give_end_to_end_liveness_probability_one` completes this route
in Lean under the listed proposed source rules. The strict proof derives timer
spread from actual prior broadcasts, pinned sync, and one action-local
exact-next timer-promptness rule. Its fields are static, local, current, or
past.

## Shared proof model

### Fundamental environment inputs

- Byzantine stake is at most `f`. Byzantine plus unavailable stake is at most
  `f + c`. The Byzantine and unavailable sets stay fixed during one stable proof
  interval.
- After network stabilization, messages between correct validators arrive within
  `delta`.
- Required local work finishes within a finite bound `epsilon`.
- Correct local clocks continue to advance. Timers do not expire early, and each
  finite timer expires.
- Continuously enabled tasks at correct validators eventually run.
- Commit-sync requests, responses, verification, and installation do not starve
  ordinary block fetch, subscription retry, Core proposal callbacks, or recovery
  timers. Commit-sync success itself is not assumed.

### Source-to-model and local product conditions

- Correct validators use one authenticated epoch configuration, validator set,
  threshold set, and protocol-versioned leader-order algorithm. For the same
  exact commit head and effective schedule key, that algorithm gives the same
  leader schedule and selected slot order.
- Signatures bind all decision data. One validator counts at most once on each
  side of one decision.
- Durable storage, accepted blocks, cleanup state, pending rounds, and local
  commit actions have the meanings used in Lean.
- The running-Core execution trace starts after `recover_validator` finishes.
  Startup recovery is outside this trace and must establish its initial durable
  state before trace time zero.
- Each qualifying external Core input handler completes all finite commit work
  that its batch enables, including finite cleanup-driven unsuspension. In the
  positive DAG proof, a qualifying handler is an `add_blocks` call that accepts
  at least one ordinary block. It observes the terminal no-more-commits result
  and invokes the normal proposal attempt before it returns. Core executes the
  handler and proposal callback serially. A commit or block-accept action does
  not interleave after that callback starts. This rule does not say that the
  proposal succeeds.
- Leader-schedule stake `S` satisfies `f + c < S`. Round-selection stake `P_r`
  satisfies `A <= P_r`. Current v3 has `P_r = S`.

### Accepted probability model

During one stalled commit index, each round's complete leader order follows the
stated probability law. The accepted design uses independent uniform orders, but
the current trace theorem still takes a favorable trace. A proved deterministic
coverage rule can replace this input.

### Derived proof goals

The protocol proofs must derive synchronization progress, one common commit
chain, recovery quorum stake, total-quorum block layers at unbounded rounds,
usable direct ranges, and unbounded local commit progress at every correct,
available validator. The public network-DAG goal does not require
per-validator production. The adopted commit proof derives unbounded later own
blocks through the V2 current source package. It then uses the proposed
no-skip rule to recover only the finite intermediate window that the favorable
path needs. The ledger keeps these results as proof goals, not final-theorem
inputs.

## Type values

An assumption must be one of five kinds. Nothing else belongs in this ledger.

- **Environment** means that the operating environment supplies the fact:
  message delay, clocks, fault sets, task scheduling, transfer budget, or a
  probability law.
- **Refinement** means that one Rust code path behaves as one model field says.
  A reviewer can check it by reading that path. This is a basic implementation
  abstraction.
- **Model** means a deliberate simplification of product behavior, kept because
  the exact behavior is not needed or not tractable.
- **Mathematical** means a fact about the numbers alone.
- **Cryptographic** means a standard property of a hash or a signature. No
  code review can establish it; a reviewer can only check that the product
  uses the primitive over the complete body.

A protocol result is not an assumption. A quorum entering recovery, a sequence
of block layers, a usable anchor, a new commit, or agreement between two
validators must be derived. If a row states such a result, the row is in the
wrong place: move the result to a theorem and keep only the local facts that the
theorem consumes.

Each `Type` field must contain the word `environment`, `refinement`, `model`,
`Mathematical`, or `Cryptographic`. The ledger check enforces this.

## Status values

- **Discharged in Lean** means that the formal model proves the claim.
- **Enforced in Rust** means that the current product prevents a violation.
- **Partially verified** means that product evidence covers part of the claim.
- **Environmental assumption** means that the operating environment supplies the
  claim.
- **Open proof obligation** means that a required result is not yet established.
- **Accepted modeling assumption** means that the proof intentionally uses the
  stated model.
- **Known mismatch** means that the current product contradicts or lacks the rule.

## Current status

| Status | Count |
|---|---:|
| Discharged in Lean | 1 |
| Enforced in Rust | 2 |
| Partially verified | 11 |
| Environmental assumption | 5 |
| Open proof obligation | 9 |
| Accepted modeling assumption | 3 |
| Known mismatch | 7 |

A known mismatch blocks the affected product claim. Other open statuses identify
a condition; they do not indicate a failed proof inside the model.

## Assumption roles

Every assumption in this ledger must be a **source**. A source supplies at least
one named input field of a goal theorem. The four goal theorems are:

| Tag | Goal theorem | Module |
|---|---|---|
| `CS` | `mysticeti_v3_safety` (leader half) and `ExactCommitInstallProvenance.correct_validators_agree_on_commit_at_index` | `Safety`, `MysticetiSafetyCapstone` |
| `TS` | `mysticeti_v3_safety` (transaction half) | `Safety` |
| `CL` | `current_sources_give_end_to_end_liveness_probability_one` | `ValidatorFixedReferenceCurrentPacing` |
| `TL` | `transaction_liveness_stage_composition` | `Liveness` |

An assumption that supplies no such field is not a source. It is only an
interesting property, and it must be deleted. Do not keep it as a note, and do
not open proof work for it.

To apply this rule to new work, name the input field first. If you cannot name
one, the result is not needed for the goals.

### Source map

| Assumption | Goal | Input field it supplies |
|---|---|---|
| `ASM-MATH-THRESHOLDS` | CS, TS | `Thresholds.quorum_certificate_intersection`, `Thresholds.quorum_preserves_certificate` |
| `ASM-SAFE-PARAMETERS` | CS, TS, CL | The shared `config`, `thresholds`, and `faults` binders of every goal theorem, and `EndToEndLivenessInputs.authorityCountAtLeastTwo` |
| `ASM-SAFE-FAULT-BOUND` | CS, TS, CL | `FixedFaultInterval.byzantineStakeBounded`, `.unavailableStakeBounded`, `.faultBudgetsFit`, `LeaderEvidence.faultBounded`, `TransactionEvidence.faultBounded`, `UniformRankingEndToEndExecutionFamily.correctAvailableMatches` |
| `ASM-SAFE-AUTHENTICATION` | CS | `AuthenticatedFlexVoteSourceMap.authenticatedCommitVote`, `.authenticatedSkipVote` |
| `ASM-SAFE-NON-EQUIVOCATION` | CS, TS | `AuthenticatedFlexVoteSourceMap.correctCommitVoteIsUnique`, `.correctCommitSkipIsExcluded`, `TransactionEvidence.correctAcceptVoteStable` |
| `ASM-SAFE-VOTE-SET-OVERLAP` | CS, TS | `LeaderEvidence.commitSkipOverlap`, `.skipCertificateOverlap`, `TransactionEvidence.acceptRejectOverlap`, `.rejectCertificateOverlap` |
| `ASM-SAFE-PARENT-QUORUM` | CS | `LeaderEvidence.anchorVotes`, `.skipCertificateOverlap`, `AuthenticatedFlexVoteSourceMap.admissibleAnchorEvidence` |
| `ASM-SAFE-EVIDENCE-REFINEMENT` | CS, TS, CL | `AuthenticatedFlexVoteSourceMap.admissibleAnchor`, `.firstDecisionBase`, `.firstDecisionProvenance`, `.admissibleIndirectStatusValid`, `.commitVoteMembership`, `.skipVoteMembership`, `.firstPendingRoundMatchesHead`, `.indirectCommitAnchorIsAdmissible`, `TransactionEvidence.directAcceptVoteEvidence`, `.committedAcceptVotesComplete`, and the `EndToEndLivenessInputs` Flex execution maps: `.flexCommitterSource`, `.flexCommitterRuntime`, `.exactPendingIngestion`, `.exactDirectRule`, `.successfulFlexScanWork`, `.anchorRules`, `.commitMaterialCausalClosure` |
| `ASM-SAFE-INDIRECT-ORIGIN` | CS | `AuthenticatedFlexVoteSourceMap.exactAnchorHistory` |
| `ASM-SAFE-DIGEST-IDENTITY` | CS, CL | `ExactCommitDurablePrefixSourceMap.validBodyDigestBinding`, `ExactCommitInstallProvenance.validChainIsDigestChain`, and the injective encoding of `ValidatorFiniteBlockIdEncoding` |
| `ASM-SAFE-COMMIT-STORE` | CS, TS, TL, CL | `ExactCommitDurablePrefixSourceMap.zeroHeadIsGenesis`, `.installedAtOrBelowHead`, `.installedHeadLinksToPredecessor`, `.exactInstalledHeadIsValid`, `.exactHeadHasStoredId`, `.storedIdHasExactHead`, `EndToEndLivenessInputs.commitPrefix`, `FinalizerLivenessStageObligations.continuousCommitStream`, and the prefix-length half of the `VisibleFirst` binder |
| `ASM-SAFE-INSTALL-PROVENANCE` | CS | The 3 origin and ordering fields of `ExactCommitInstallProvenance`: `.localInstallOrigin`, `.verifiedSyncInstallOrigin`, `.correctCarrierFollowsInstalledCommit` |
| `ASM-SAFE-FIRST-TRIGGER` | TS | The `FirstEligible` half of the `VisibleFirst` binder of `mysticeti_v3_safety` |
| `ASM-SAFE-COMMITTED-PREFIX` | CS, TS | `TransactionEvidence.anchorInCommittedPrefix` and `LeaderEvidence.correctCommitAnchorInCertificate`. Both fields are shared with `ASM-SAFE-GC`, which supplies the retained-window hypothesis each one needs |
| `ASM-SAFE-GC` | TS, CS, CL | `TransactionEvidence.gcWindow`, `.blockEvidence`, `LeaderEvidence.coreGc`, `.anchorDepth`, `EndToEndLivenessInputs.installedCommitParents`, `.installedHeadBootstrap` |
| `ASM-LIVE-GC-FRONTIER` | CL | The `retention` binder of the CL goal: `ValidatorCommitOrthogonalAcceptedRetentionRules.acceptedAboveGcIsRetained` |
| `ASM-CONFIG-V3-ACTIVATION` | CS, TS, CL | Global. The source maps model v3 code, so every field is void when v3 is off |
| `ASM-CONFIG-VOTING` | TS | `TransactionEvidence.directAcceptVote`, `.committedVote` |
| `ASM-REFINE-INTEGERS` | CS, TS, CL | Global. Lean uses unbounded naturals for every numeric field |
| `ASM-LIVE-PARTIAL-SYNCHRONY` | CL | `AddressedPartialSynchrony.postGstDelivery` under `EndToEndLivenessInputs.timedExecution` |
| `ASM-LIVE-COMMON-GENESIS` | CL | `EndToEndLivenessInputs.initial` and `.genesisParents` |
| `ASM-LIVE-FINITE-REFERENCE-SPACE` | CL | `ValidatorFiniteBlockIdEncoding`, one component of the `admission` input through `ValidatorPersistedCausalCapsuleFiniteReferenceSourceMap.toRoundAdmission` |
| `ASM-LIVE-CAPSULE-PROJECTION` | CL | `ValidatorPersistedCausalCapsuleFiniteReferenceSourceMap`, the `maxAdmittedRefsPerRound` binder, and the `admission` input `ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap` |
| `ASM-LIVE-ROUND-CATCHUP` | CL | `ValidatorV2RoundCatchupSourceMap` |
| `ASM-LIVE-COMMIT-PROGRESS-RECOVERY` | CL | `ValidatorFixedReferenceStrictCurrentSourceMaps`, the `parameters` binder `ValidatorQuadraticGapWaitParameters`, the `syncSources` binder `ValidatorFreshRoundPinnedSyncSourceRules`, and the `EndToEndLivenessInputs` recovery fields: `.recoveryMode`, `.recoveryProposalRounds`, `.blockProgressRecoveryThresholds`, `.blockProgressRecoveryMode`, `.blockProgressProposalOrigin`, `.blockProgressRecoveryWaitMatches`, `.recoveryWait`, `.recoveryTimerSource`, `.recoveryProposalPacing`, `.recoveryProposalActionTiming`, `.recoveryTimerArms`, `.recoveryParentNeeds`, `.recoveryParentAcceptance`, `.readyNormalProposalProtection`, `.blockProgressRecoveryNeedRules`, `.recoverySourcePins`, `.recoveryCapsuleSync`, `.recoveryCapsuleGenesisExact`, `.currentTipSubscription`, `.recoveryTipRebroadcast`, `.acceptedRepresentatives` |
| `ASM-LIVE-LOCAL-PROPOSAL` | CL | `EndToEndLivenessInputs.normalFrontierPacemaker`, `.proposalObligations`, `.proposalLatch` |
| `ASM-LIVE-CORE-HANDLER` | CL | `EndToEndLivenessInputs.coreHandlerRefinement`, `.coreProposalContinuation`, `.authorLocalCommitContinuation`, `.commitProposalNonInterference` |
| `ASM-LIVE-LEADER-STAKE` | CL | `EndToEndLivenessInputs.leaderSchedule.scheduleViable` |
| `ASM-LIVE-LEADER-SCHEDULE` | CL | `EndToEndLivenessInputs.leaderSchedule.indirectDepth`, `.indirectDepthPositive`, `EndToEndLivenessInputs.flexCommitterDepthMatchesLeaderSchedule`, and the `family` binder fields `UniformRankingEndToEndExecutionFamily.leaderScheduleMatches` and `.indirectDepthMatches` |
| `ASM-LIVE-FIRST-SLOT-SAMPLING` | CL | `IndependentUniformRoundRankingLaw`, `UniformRankingEndToEndExecutionFamily.firstSelectedLeaderMatchesRanking`, and the `rankingSource` binder `UniformRankingExecutionSourceMap` |
| `ASM-LIVE-POST-GST-CAUSAL-SERVICE` | CL | `ValidatorPostGstCausalQueueServiceRules.cAdd`, `.cService`, `.serviceMargin`, `.highBacklogService`, `.lowBacklogClearsOld`, `.workAddedBound`, and the `blockSources` binder `ValidatorV2BlockProductionCurrentSourceMaps` |
| `ASM-LIVE-TRANSFER-BUDGET` | CL | The same fields, through `ValidatorCausalQueueTransferBudget.toServiceRules` |
| `ASM-LIVE-BLOCK-SYNC` | CL | `EndToEndLivenessInputs.blockSync` and `.operationalQuorumFrontier` |
| `ASM-LIVE-PEER-FAIRNESS` | CL | `ValidatorBlockSyncExecutionRules.deliveredRequestEnablesServe`, `.serveActionCreatesPacket`, `.peerRotationBound` |
| `ASM-LIVE-TASK-FAIRNESS` | CL, TL | `ValidatorActionContinuouslyEnabled`, `ValidatorActionCompletion.enabled`, and `FinalizerLivenessStageObligations.triggerEventually`, shared with `ASM-LIVE-FINALIZER-TRIGGER` |
| `ASM-LIVE-LOCAL-RESPONSE` | CL, TL | `ValidatorActionCompletion.completesWithinBound`, `FinalizerLivenessStageObligations.triggerToDecision`, `.decisionToDurableOutput` |
| `ASM-LIVE-FINALIZER-TRIGGER` | TL | `FinalizerLivenessStageObligations.triggerEventually` |
| `ASM-LIVE-DURABILITY` | TL | `FinalizerLivenessStageObligations.triggerToDecision` and `.decisionToDurableOutput`, both shared with `ASM-LIVE-LOCAL-RESPONSE`, and the `commitEntersFinalizer` binder of `transaction_liveness_stage_composition` |

### Structural fields

The goal theorems also take fields that define the modeled execution itself.
These are not separate claims, so they have no row. They are the data fields of
`EndToEndLivenessInputs` (`genesis`, `program`, `commitReferenceFunctions`,
`flexCommitterHistory`, `flexCommitterContext`, `validCommitChain`,
`validCommitBlocks`), the base
execution and action-semantics maps (`timedExecution`, `executionEffects`), the
abstract predicate parameters of the safety route (`blockCarriesCommitVote` and
the vote and head predicates of the two safety source maps), and the sampling
family's data fields (`UniformRankingEndToEndExecutionFamily.depth`,
`.correctAvailable`, `.execution`, `.authorityCountMatches`). Their meaning is
the boundary stated in "Source-to-model and local product conditions", and
`ASM-CONFIG-V3-ACTIVATION` gates them globally. The `commitLiveness` binder of
`transaction_liveness_stage_composition` is the CL conclusion, not an
assumption. With these named, every rule-bearing input field of a goal theorem
has an owner row in the table above.

### Deleted assumptions

`ASM-LIVE-PIPELINE-BOUNDS` was deleted on 2026-08-19. It supplied no input field
of a goal theorem. It supplied only the per-stage bounds of the consensus stage
ladder in `Liveness`, which gave a `10 * delta` figure. The derived commit-liveness
route in `ValidatorFixedReferenceCurrentPacing` replaced that ladder, so the
ladder and the assumption were removed together.

`ASM-LIVE-COMMIT-SYNC` was folded into `ASM-LIVE-TASK-FAIRNESS` on 2026-08-19.
Its sync-install safety content was already `ASM-SAFE-INSTALL-PROVENANCE`, its
gap-free stream content was already `ASM-SAFE-COMMIT-STORE`, and no goal theorem
takes commit-sync success as an input. Only its non-starvation content was an
assumption, and that is a fairness claim.

`ASM-SAFE-COMMIT-CHAIN` was split into `ASM-SAFE-DIGEST-IDENTITY`,
`ASM-SAFE-COMMIT-STORE`, and `ASM-SAFE-INSTALL-PROVENANCE`.
`ASM-SAFE-NON-EQUIVOCATION` kept the signing rule and its counting rule moved to
`ASM-SAFE-VOTE-SET-OVERLAP`. `ASM-LIVE-LEADER` was split into
`ASM-LIVE-LEADER-STAKE` and `ASM-LIVE-LEADER-SCHEDULE`.
`ASM-SAFE-EVIDENCE-REFINEMENT` kept the shared rule set and its cached-anchor
half moved to `ASM-SAFE-INDIRECT-ORIGIN`. `ASM-SAFE-GC` kept the safety
retention rules and its frontier half moved to `ASM-LIVE-GC-FRONTIER`.
`ASM-LIVE-FINITE-REFERENCE-SPACE` kept the finite block-identifier encoding and
its capsule half moved to `ASM-LIVE-CAPSULE-PROJECTION`. `ASM-LIVE-GC-FRONTIER`
moved from the dead field `localGcCutoffCausal`, which was deleted, to the CL
`retention` binder on 2026-08-19.

## Maintenance rules

1. Keep each `ASM-*` and `REF-*` identifier stable.
2. Change a status only with proof, product, test, or environment evidence.
3. Do not turn a protocol result into a basic environment assumption.
4. Review all affected mappings after a product change.
5. Keep missing behavior separate from verified current behavior.
6. Keep detailed source evidence and revalidation triggers in the assumption
   evidence ledger.
7. Give each assumption a row in the source map. Delete an assumption that
   supplies no input field of a goal theorem.
8. Write each claim as what one host does, or as what the environment supplies.
   Do not write a claim that states a result which the specification proves. A
   claim that quantifies over two validators is almost always such a result.
   `ASM-SAFE-COMMIT-CHAIN` had this defect, and was split into
   `ASM-SAFE-DIGEST-IDENTITY`, `ASM-SAFE-COMMIT-STORE`, and
   `ASM-SAFE-INSTALL-PROVENANCE`. `ASM-SAFE-FIRST-TRIGGER` had it too.

## Periodic Lean-to-Rust refinement review

This is the canonical source-to-model checklist. A verified row covers only its
stated behavior. It does not prove an end-to-end theorem.

Last complete source review: 2026-08-15, at `d630b4452a8`.

Focused leader-schedule, exact-prefix, restart, and finalizer review:
2026-08-17, at `2fecfec37462785ccd6684195aac9131e54ad251`. See the
[assumption evidence ledger](ASSUMPTION_EVIDENCE.md).

Focused commit-sync safety, subscription-resume, periodic-sync-failover, and GC
review: 2026-08-18, at `2e05fcf9cbeba4d42b0cc4145312ae053dba14dc`.
See [EV-COMMIT-SYNC-COVERAGE](ASSUMPTION_EVIDENCE.md#ev-commit-sync-coverage).

### Missing Rust behavior and open source refinements

#### Commit progress recovery

| Review ID | Required behavior or guarantee |
|---|---|
| `REF-RECOVERY-ENTRY` | Use the normal `threshold_clock_round()` candidate only as the round-gap probe. Enter when this gap reaches `P_enter` or the time since any commit install reaches `T`. Stay active while the gap reaches `P_exit` or the time reaches `T`, where `P_exit < P_enter`. Exit only when both signals clear. Keep the exact-next recovery target separate. |
| `REF-NEXT-ROUND-TARGET` | Exact-next recovery proposes only one round after the highest known own proposal. A separate, unproved cleanup safe-resume step can use one witnessed jump when old parent history is unusable. |
| `REF-RECOVERY-PACING` | Use one fixed reference round `R_c` and the quadratic wait `W(R) = b + l * (R - R_c) + q * (R - R_c)^2`. The wait value does not depend on the local commit head. Derive timer spread from actual prior broadcasts and sync. Use one action-local exact-next timer-promptness rule. Start a frontier timer only after quorum parents are ready. One timer generation owns one proposal action in its bounded deadline window. Do not reuse an earlier expired or stale timer. The coefficient `q` must dominate the derived fixed-reference timer-spread and causal-visibility costs. |
| `REF-RECOVERY-PARENTS` | Wait for immediate-parent quorum and request missing parents. At parent selection, include the current locally accepted and retained representative for every in-range author for which one exists. Count an equivocating author once, ignore its other branches, do not consult the predicted leader schedule, and do not use score exclusion there. |
| `REF-RECOVERY-PROPOSAL-ORIGIN` | While block-progress recovery is active above GC, cancel or replace stale normal ready work before persistence. An actual persisted ready proposal in this state must come from the current recovery timer and its refreshed parent selection. |
| `REF-RECOVERY-TIMER-ORIGIN` | Map each actual timer-paced proposal to its exact earlier timer start for the same validator, commit head, and target round. Make that timer key unique. From a current parent-ready state, select a bounded exact timer start or show that the local commit head already advanced. These are current or past facts. They do not state that a future timer or proposal exists. |
| `REF-RECOVERY-FRONTIER` | Connect every recovered own round to a durable signed block and the required parent history. The common maximum round remains a proof value. |
| `REF-RECOVERY-GC-FRONTIER` | Keep each recovery target above the old-block cleanup boundary or define a safe resume rule. |
| `REF-RECOVERY-LAYER-MAPPING` | Retain and identify all rounds in the complete recovery anchor window. |
| `REF-CAUSAL-CAPSULE-PROJECTION` | For each actual correct proposal persistence action, identify the durable pinned recovery capsule with the exact static capsule used by the backlog projection. Keep unique references, in-range authors, and the target-round upper bound. This equality is a past persistence refinement. It does not state future delivery or acceptance. |
| `REF-LOCAL-PROPOSAL-PROGRESS` | Map the current one-shot maximum timeout and its watcher retries. A forced attempt can stop for the old round only if that round is already signed or the threshold clock moved higher. A missing recovered own-round value or excessive propagation delay is temporary. The recovered-round watcher makes a forced attempt on every value change; the delay watcher makes one only when the full proposal gate flips from blocked to clear. Installing a commit must not interrupt an active Core callback. |
| `REF-CURRENT-TIP-REPLAY` | Map the current receiver-driven subscription path. A broken, ended, or idle stream terminates. The correct receiver retries. A successful subscription sends the requested cached own block, if it is available, or the sender's latest own block. The proof uses the exact requested tip or treats a newer tip as higher frontier progress. |
| `REF-POST-GST-CAUSAL-SERVICE` | Map the V2 current no-idle source package. Its selected support, recursive need, queue-source, and no-idle rules derive unbounded later own-block production at each correct, available host. Map pinned ordinary block sync and commit-orthogonal above-GC retention separately. The no-skip round-catch-up rule derives only the finite intermediate window that fixed-reference pacing needs. None of these source rules supplies a future block, window, Flex result, or commit. |

#### Other features

| Review ID | Required behavior or guarantee |
|---|---|
| `REF-EPOCH-CONFIG` | Put all proof-relevant values in authenticated epoch state and reject incompatible values. |
| `REF-INTEGER-BOUNDS` | Set explicit numeric limits and use checked calculations for all modeled values. |
| `REF-FINITE-BLOCK-ID-SPACE` | Map `BlockId` injectively into one fixed finite space. For current Rust, use the fixed byte-array space of `BlockDigest`. Together with the finite authority set and unique capsule references, this gives the static per-round reference cap. |
| `REF-V3-ACTIVATION` | Activate v3 from shared epoch state. |
| `REF-V3-TRANSACTION-PATH` | Implement the modeled v3 proposal, transaction-vote, cutoff, and finalization path. In progress on the branch: v3 proposals sign votes and the cutoff, the verifier checks them, and `CommitFinalizerV3` finalizes. The merge remains. |
| `REF-AMNESIA-SIGNER-GUARD` | Prevent conflicting signatures after complete local consensus-state loss. |
| `REF-CORE-HANDLER-COMPLETION` | Map each ordinary `add_blocks` call that accepts at least one block to `ValidatorCoreHandlerInputObservation` through `handlerInputOccurs` and `qualifyingInputHasFiniteHandler`; a nonempty batch whose blocks are all duplicate or suspended runs neither the commit loop nor the proposal attempt. Map direct packet-driven input acceptance through `ValidatorPacketDrivenBlockAcceptanceAt` and `packetDrivenAcceptanceHasInputOrigin`. Do not classify later GC-unsuspension as a new input; use its enclosing handler episode. Cover the finite input batch and GC-unsuspended block work, observe the terminal no-more-commits result, and invoke `try_propose(false)` before return. Map the already-actual attempt through `ValidatorCoreProposalAttemptContinuationRules` to a same-batch proposal action, protected normal work, or current durable retry work. These mappings must not assert proposal success or future DAG progress. |
| `REF-PARENT-SYNC` | For an ordinary block, keep fetching each missing causal-history block above the requester's local cleanup round. Handle empty peer sets, recursive direct-parent discovery, and fair retries. References at or below local cleanup are committed roots and require no recovery. |
| `REF-FLEX-ACCEPTED-BODY-OWNERSHIP` | At one actual Flex action-before state, map each authenticated accepted body from a correct author to that author's exact durable `ownBlockAt` entry. Derive the restored time-zero origin or an exact earlier proposal-persistence action from this current fact. Do not state future production. |
| `REF-FLEX-POST-REFRESH-INPUT` | Map each actual `runCommitter` action to the literal scan input that Rust reads after it refreshes pending rounds. The same action must return the result computed from that input. Result equality is derived from the two views of the same action; it is not a separate future-result premise. |
| `REF-COMMIT-SYNC-PROGRESS` | Optional commit synchronization can accelerate catch-up. Subscription suspension must be released after local catch-up, and periodic ordinary sync must resume when commit-index progress stalls. Commit-sync work must not starve ordinary sync or proposal work. Commit sync can still stop after commit-index catch-up while the local ordinary DAG lags. Therefore, the final liveness proof must not depend on commit-sync success or commit votes in blocks. |
| `REF-COMMON-COMMIT-CHAIN` | Establish one index-and-digest commit chain across local production, synchronization, and restart. |
| `REF-GC-EVIDENCE` | Keep complete decision evidence across local production, synchronization, replay, restart, and transaction finalization. |
| `REF-LEADER-BOUNDS` | Enforce `f + c < S` and `A <= P_r` from actual epoch stake. |
| `REF-LEADER-ORDER-COMPATIBILITY` | Use a protocol-stable or version-gated schedule and round-order algorithm, with compatibility vectors. |
| `REF-FINALIZER-TAIL` | Define the result for pending finalizer state when an epoch ends before a later trigger. |
| `REF-ROUND-CATCHUP` | If fixed-reference liveness needs skipped rounds, make each active correct proposal persistence use exactly `highestSignedRound + 1`. The final path binds each recovered intermediate persistence to its earlier commit-progress-recovery timer generation, exact `proposeNext` action, and refreshed parent snapshot for the same block. These are current or past facts. They do not state that a future intermediate block exists. |

### Verified current Rust behavior

| Review ID | Verified behavior |
|---|---|
| `REF-THRESHOLD-CONSTRUCTION` | For non-overflowing inputs, threshold construction checks both safety inequalities. |
| `REF-AUTHENTICATION` | Protocol signatures cover the complete block, author keys are checked, and validator connections use mutual authentication. |
| `REF-VOTE-DEDUP` | One validator identity counts once in each voter set. |
| `REF-COMMIT-STATE` | One durable commit supplies the local index, reference, and protocol timestamp after restart. |
| `REF-OWN-PROPOSAL-ROUND` | Normal restart restores the own-round floor from the store. The peer-assisted fetch sets a verified floor; its trigger is `boot_counter == 0` with the sync timeout configured on a validator, not an empty store. The counter increments only after a run that handled a commit, so the fetch also runs on a normal process restart and on epoch starts during a multi-epoch catch-up. Complete amnesia remains open. |
| `REF-DURABLE-PROPOSAL` | A local proposal is durable before broadcast: `try_new_block` calls `DagState::flush()` before it returns the block for broadcast, and durability is the store write guarantee of that flush. |
| `REF-DURABLE-COMMIT-OUTPUT` | Finalized commits and their committed blocks are flushed to storage before they are sent to the commit handler. `CommitFinalizer` calls `dag_state.flush()` and only then sends each commit on `commit_sender`. |
| `REF-PARENT-QUORUM` | An ordinary accepted non-genesis block has immediate parents from distinct validators with quorum stake. |
| `REF-DIGEST-CHAIN` | Every digest is taken over the complete serialized body. A commit body holds `previous_digest`, so commits form a hash chain. A block body holds each ancestor as a `BlockRef` with a digest, the proposer always places its own last proposed block first, and the verifier rejects a block whose first ancestor is not its own author, whose later ancestors repeat an author, or whose ancestor round is not below the block round. So each author's own blocks form a hash chain and the block DAG is hash-linked. |
| `REF-BLOCK-PARENT-ACCEPTANCE` | The ordinary live path accepts required above-boundary parents before their child. Certified commits use a separate checked path. |
| `REF-BLOCK-SYNC-MECHANISMS` | Direct, periodic, history, and stall-recovery block-fetch paths exist. When commit lag suppresses periodic sync, a commit-index stall starts the periodic failover path. Their existence and this transition do not by themselves prove peer service or scheduling progress. |
| `REF-COMMIT-SYNC-CHECKS` | Synchronized ranges are checked for indexes, digest links, block references, gaps, order, and quorum support on the range tip. Each commit does not have a separate certificate. |
| `REF-LEADER-SCHEDULE` | The same prefix and same fixed build and random-generator configuration produce the same ordered schedule and interval. |
| `REF-ROUND-LEADER-SELECTION` | Each stored pending v3 round contains the full schedule in one deterministic round order. Thus, `P_r = S`. |
| `REF-DIRECT-DECISION` | Direct selected-slot decisions read the voting round one above the slot. Without a quorum of accepted voting-round stake, the slot stays undecided. A leader block commits on a quorum of linking voting blocks. A skip vote is a voting-round block that does not link to the leader block; a block with a skip-vote quorum is skipped, and the slot is skipped when every block in it is skipped, including an empty slot. Rust asserts that no block reaches both quorums. |
| `REF-INDIRECT-DECISION` | Indirect selected-slot decisions walk once from the anchor through ancestors above the decision round, and count a vote only from a block one round above the decision round to a decision block, one vote per author. A decision block with certification-threshold stake is certified. The slot commits when exactly one of its blocks is certified; zero or several certified blocks skip the slot. |
| `REF-DECISION-ORIGIN` | `update_slot_decision` records whether the first result was direct or indirect. It changes only an undecided slot and asserts result equality on a later update. The exact indirect anchor and history are not stored. |
| `REF-PENDING-ROUNDS` | Pending rounds are consecutive. The direct scan decides undecided slots from the minimum next leader round up to one round below the highest accepted round. The indirect scan runs from high to low over bases up to two rounds below the highest accepted round; each base uses the anchor at base plus two, which is the first committed slot at or above that round with no undecided slot before it. |
| `REF-GC-BOUNDARY` | Local cleanup keeps above-boundary blocks. Pending commit state and finalizer state own copied evidence independently of the live cache. |
| `REF-COMMIT-INSTALL-DAG` | Before a local or synchronized commit install moves GC, accept and catalogue every exact commit-body block. Retain the accumulated closed frontier above the new GC round. |
| `REF-FLEX-RESULT` | When the current decision scan finds a commit, the local commit state records it through the normal commit path. |
| `REF-COMMIT-MATERIALIZER-WALK` | With `enable_v3`, `FlexCommitter::build_commit` marks every committed leader of the commit round and walks their causal history. It follows an ancestor only when the reference is above the local GC round and no earlier commit took it. It marks each followed reference before it pushes the body. `get_block(..).unwrap()` must find every such body in the local `DagState`. Lean records a finite duplicate-free catalog domain. Every successful catalog read must use an identifier in that domain. `buildCommit_terminates` gives a fuel value at most one more than the domain length. A repeated ancestor reference is dropped by `if !set_committed(..) { continue; }`; block verification also rejects two ancestors from one author. `Linearizer::linearize_sub_dag` is the one-leader pre-v3 form of the same walk. The V3 source map requires a nonempty leader list and requires every leader to have the commit-head round. Rust also aborts on a leader that is already committed and on a committed block at or below GC. The raw Lean walk has no such failure, so an unmapped modeled run can succeed where Rust panics. |
| `REF-COMMIT-BODY-ORDER` | `sort_committed_blocks` keys on the block round and on `hash(seed \|\| digest)`, with a seed over the committed leader digests. Distinct committed references must therefore get distinct keys. The named leader is the last block of the sorted vector, and `calculate_commit_timestamp` takes the stake-weighted median of the committed leaders' own timestamps when their stake reaches the certification threshold; below that threshold it falls back to the leaders' one-round-below ancestors from `DagState`, which can include blocks that an earlier commit already took. The pre-v3 `sort_sub_dag_blocks` keys only on round and author and has no such tie-break. |
| `REF-V3-SCHEDULE-SCORER` | `LeaderScheduleV3::add_commit` keeps a three-deep pending window and scores `C-3` against `[C-2, C-1, C]`. Its scoring calculation must be a deterministic function of those four committed materials. It must read only the commit index, the commit digest, the named leader, the sorted committed block bodies with their `ancestors()`, and the epoch-fixed committee stakes, schedule configuration, epoch start timestamp, and epoch number. It must not read other mutable local state. The model takes that calculation as one function and does not reproduce its arithmetic, so the voting scan, the certifying scan, the equivocation rule, and the distinct-author stake sums stay source obligations. Its running per-authority totals move by `checked_add` and `checked_sub` over the sliding entry window. `refresh_current_schedule` recomputes `allowed_leaders` only at an update-interval boundary. `select_allowed_leaders_with_fixed_config` seeds its shuffle from the commit digest of the last pending commit, and from the epoch start timestamp and epoch number when no commit is pending. The source map requires the same ordered committed-leader list at two hosts for one exact head. Exact commit-decision replay must supply this condition because `compute_sort_seed` hashes leader digests in list order. Rust also asserts consecutive commit indexes, strictly increasing leader rounds across the pending window, a leader set that contains the named leader, and a scan that ends above its bound. The Lean model does not carry these remaining invariants, so it accepts histories that Rust would abort. |
| `REF-V3-SCHEDULE-READERS` | The FlexCommitter reads the minimum next leader round and the allowed-leader vector of the current replayed schedule state, and derives each round's slot order from that vector with a round-seeded shuffle; the schedule state stores no round order. The proposer reads the same state only to wait for leader blocks. Proposer ancestor selection does not read the schedule; it excludes ancestors by propagation scores, and `ASM-LIVE-LEADER-SCHEDULE` records that this exclusion must not drop the exact first leader. |
| `REF-V3-SCHEDULE-INPUTS` | `LeaderScheduleV3::add_commit` is also called for a synchronized commit that `FlexCommitter::handle_certified_commit` built, and `from_store` replays committed sub-DAGs from storage over a bounded suffix that starts at `replay_start`. The Lean model replays from genesis and maps only the local build path. Map the other two input routes, and show that the bounded suffix replay reaches the same schedule state. |

### Accepted model and environment

Independent uniform leader ordering is an accepted probability model. It is not
a claim about current Rust. The product uses a common deterministic round-based
order for one fixed compatible build. The model does not prove independent
samples for that exact sequence.

Post-stabilization delivery, local response time, and fair task execution are
environment conditions. Retention, serving, peer retry, and request processing
are local product rules.

For each review, record the source revision and date, check every row affected by
the change, and update the related assumption when a meaning changes.

The current review does not establish finalizer recovery for a pending decision
window with a nonzero cleanup boundary. A test that can hide failure in detached
finalizer work is not evidence for this result.

## ASM-MATH-THRESHOLDS

- **Claim:** The nominal values `N = 5f + 3c + 1`, `Q = 4f + 2c + 1`, and `A = 2f + c + 1` satisfy both safety inequalities.
- **Type:** Mathematical.
- **Status:** Discharged in Lean.
- **Effect if false:** Safety.
- **Lean use:** The safety proofs use the two threshold inequalities.
- **Rust evidence:** Product threshold construction checks both inequalities for actual stake.
- **Discharge:** Keep the actual-threshold mapping and boundary tests current.

## ASM-SAFE-PARAMETERS

- **Claim:** All correct validators in one epoch use one authenticated validator set, threshold set, cleanup depth, leader schedule, and feature set. The epoch validator set has at least two validators.
- **Type:** Configuration refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** Every weighted decision uses one common configuration.
  `EndToEndLivenessInputs.authorityCountAtLeastTwo` records the minimum
  validator count.
- **Rust evidence:** The v3 flag and the schedule parameters are fixed constants of the node build, and the bad-nodes threshold comes from the versioned protocol config, so different builds can use different values.
- **Discharge:** Move all proof-relevant inputs into authenticated epoch state.

## ASM-SAFE-FAULT-BOUND

- **Claim:** During one stable liveness interval, the Byzantine and unavailable sets are fixed. Byzantine stake is at most `f`, and their combined stake is at most `f + c`.
- **Type:** Adversary and availability model.
- **Status:** Environmental assumption.
- **Effect if false:** Safety for `f`; liveness for `f + c`.
- **Lean use:** Quorum intersection and progress results use these bounds.
- **Rust evidence:** The product cannot identify all faulty or unavailable validators.
- **Discharge:** State and monitor the deployment fault model.

## ASM-SAFE-AUTHENTICATION

- **Claim:** A verified signature binds the validator, epoch, round, complete block, and all decision data.
- **Type:** Rust refinement.
- **Status:** Enforced in Rust.
- **Effect if false:** Safety.
- **Lean use:** Voter sets treat each authenticated block as one validator's statement.
- **Rust evidence:** Complete-block signatures and mutual validator authentication enforce this claim.
- **Discharge:** Preserve these checks and their compatibility tests.

## ASM-SAFE-NON-EQUIVOCATION

- **Claim:** One non-Byzantine identity does not authenticate two different
  commit votes for the same selected leader slot, and does not authenticate both
  a commit and a skip for the same leader block. The same identity also does not
  authenticate two different votes for the same transaction target: its signed
  vote block and the committed-prefix copy of that block are one signed
  statement.
- **Type:** Single-validator Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** `AuthenticatedFlexVoteSourceMap.correctCommitVoteIsUnique` and
  `.correctCommitSkipIsExcluded`, and
  `TransactionEvidence.correctAcceptVoteStable`.
- **Rust evidence:** A durable restart keeps the signed-round floor, and the
  proposer signs one block for each round it proposes.
- **Derivation candidate:** This may follow from how proposal works rather than
  stay an assumption. `highestSignedRound` is a durable floor, and
  `try_new_block` refuses a round at or below the last proposed round, so one
  correct host produces at most one block for each round. The model does not
  connect that rule to `authenticatedCommitVote`, so the link is not made.
- **Discharge:** Derive the rule from one-block-per-round, or add a signer guard
  for complete local-state loss and count that validator as faulty.

## ASM-SAFE-VOTE-SET-OVERLAP

- **Claim:** Two opposite sides of one decision share no correct identity. A
  commit-vote set and a skip-vote set overlap only in Byzantine stake, and the
  same holds for skip against certificate, and for accept against reject.
- **Type:** Single-validator Rust refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety.
- **Lean use:** `LeaderEvidence.commitSkipOverlap` and `.skipCertificateOverlap`,
  and `TransactionEvidence.acceptRejectOverlap` and `.rejectCertificateOverlap`.
  Each is `OnlyFaultyOverlap`, so the two sides intersect inside the faulty set.
- **Derived elsewhere, in part:** The overlap shape is a result, not a source.
  `AuthenticatedFlexVoteSourceMap.twoViewEvidence` proves the
  commit-against-commit and commit-against-skip overlaps from
  `correctCommitVoteIsUnique`, `correctCommitSkipIsExcluded`, and the two
  membership fields; see `ExactCommitPrefixSafety.lean` at `twoViewEvidence`.
  The certificate overlaps and the transaction overlaps are not derived
  anywhere: the model has no per-validator certificate or transaction-vote
  source yet. The row survives because nothing builds `LeaderEvidence` or
  `TransactionEvidence` from the vote source map, so both bundles still take
  all four overlap fields as input.
- **Rust evidence:** Vote sets deduplicate identities, so one identity is
  counted once for each side.
- **Discharge:** Build `LeaderEvidence` and `TransactionEvidence` from
  `AuthenticatedFlexVoteSourceMap` so the existing derivation supplies the
  commit and skip overlaps. Add per-validator certificate and transaction-vote
  sources for the other fields. Then delete this row.

## ASM-SAFE-PARENT-QUORUM

- **Claim:** Every accepted non-genesis block has at least `Q` distinct stake in verified immediate parents.
- **Type:** Rust refinement.
- **Status:** Enforced in Rust.
- **Effect if false:** Safety.
- **Lean use:** Indirect safety uses the anchor's parent quorum.
- **Rust evidence:** Ordinary block verification rejects insufficient or duplicate parent stake.
- **Discharge:** Preserve the check for every accepted block path.

## ASM-SAFE-EVIDENCE-REFINEMENT

- **Claim:** Product leader and transaction decisions use the same evidence,
  voter accounting, and result rules as the model. Once a slot is committed or
  skipped, later direct and indirect rule runs preserve that result. The signed
  cutoff is at least the block-cleanup and vote-cleanup rounds; vote-target
  truncation can raise it, and every removed target is at or below it.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** The first-decision relation keeps direct evidence for direct
  results and proves that later direct and indirect passes preserve the first
  result. Safety compares the reconstructed direct bases. It does not treat a
  cached indirect result as direct. The cached-anchor half of the old claim is
  now `ASM-SAFE-INDIRECT-ORIGIN`. The counted vote sets are exactly the
  authenticated votes (`commitVoteMembership`, `skipVoteMembership`), the
  pending window starts at a deterministic function of the commit head
  (`firstPendingRoundMatchesHead`), and an indirect commit from an admissible
  anchor stays admissible (`indirectCommitAnchorIsAdmissible`). On the liveness
  route the same claim covers the local Flex execution maps:
  `EndToEndLivenessInputs.flexCommitterSource`, `.flexCommitterRuntime`,
  `.exactPendingIngestion`, `.exactDirectRule`, `.successfulFlexScanWork`,
  `.anchorRules`, and `.commitMaterialCausalClosure`.
- **Rust evidence:** `RoundState::update_slot_decision` changes only an
  undecided slot, records the first direct or indirect origin, and asserts
  equality on a later update. A fully decided round can skip another indirect
  run. On the branch, `try_new_block` computes the signed cutoff: it starts at
  the DAG GC round, which the tracker GC round never exceeds, and
  `truncate_transaction_votes` raises it to cover every removed vote target.
  The verifier requires the cutoff below the block round and every explicit
  vote target above the cutoff and below the block round.
- **Evidence record:** [EV-CACHED-INDIRECT-ORIGIN](ASSUMPTION_EVIDENCE.md#ev-cached-indirect-origin).
- **Discharge:** Merge the v3 transaction path and add shared conformance
  vectors.

## ASM-SAFE-INDIRECT-ORIGIN

- **Claim:** A cached indirect result retains its exact deciding anchor and
  ordered history for as long as the result itself is retained.
- **Type:** Single-validator Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** `AuthenticatedFlexVoteSourceMap.exactAnchorHistory` maps the
  implementation's immutable causal history to the exact anchor history that
  the vote proof reads. Safety reconstructs an indirect decision from that
  anchor and history.
- **Rust evidence:** No decision is durable. `PendingCommitState` is in-memory,
  starts from its default value, and never reaches the store. `WriteBatch`
  carries no slot status and no decision origin. A restart therefore recomputes
  every verdict from recovered blocks and commits, and
  `maybe_refresh_pending_commit_state` discards the pending rounds on a
  schedule change, so no stale indirect result survives either event. The
  mismatch is inside one pending-state lifetime: `Decision::Indirect` records
  that the indirect rule decided the slot,
  `LeaderSlotDecider::try_indirect_decide` returns statuses without the anchor,
  and `FlexCommitter::decide_with_anchor_block` only logs it, so the anchor and
  the ordered scan are not readable from the stored state.
- **Evidence record:** [EV-CACHED-INDIRECT-ORIGIN](ASSUMPTION_EVIDENCE.md#ev-cached-indirect-origin).
- **Discharge:** Retain the exact indirect anchor and scan origin inside one
  pending-state lifetime, or show that the Lean field does not need them once
  no decision is durable. Restart and reset need no separate mapping, because
  both discard the pending state.

## ASM-SAFE-DIGEST-IDENTITY

- **Claim:** A digest names exactly one body. Two bodies have the same digest if
  and only if they are the same body. This holds for block digests and for
  commit digests.
- **Chain note:** Rust holds the back-links, and the model now carries them, so
  the chain rule is **derived and no longer assumed**. A commit body carries
  `previous_digest` (`consensus/core/src/commit.rs:106`), and the commit digest
  hashes the complete serialized commit (`consensus/core/src/commit.rs:194`), so
  the digest covers the index, the previous digest, the timestamp, the leader
  reference, and the committed blocks. A block carries every ancestor as a
  `BlockRef` with a digest (`consensus/types/src/block.rs:32`), and the block
  digest hashes the complete serialized block with its signature
  (`consensus/core/src/block.rs:621`). `ValidatorCommitHead` now has
  `previousId`, `DigestLinkedCommits` defines the chain, and
  `ExactCommitDurablePrefixSourceMap.digest_chain_entry_matches_installed_prefix`
  proves that walking down a checked chain from an installed tip stays inside
  the same host's durable prefix. That result was a field of
  `ASM-SAFE-INSTALL-PROVENANCE` and is now a theorem.
- **Type:** Cryptographic.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** `ExactCommitDurablePrefixSourceMap.validBodyDigestBinding` is
  the commit half: two valid commit bodies with the same index and the same
  digest are equal. Safety needs this because a commit vote signs
  `(index, digest)` and does not sign the named leader round as a separate
  field. Rust still binds that round, because `CommitV1.leader` is inside the
  hashed body. `ExactCommitInstallProvenance.validChainIsDigestChain` is the
  other field: the synchronization chain check is the digest-link check
  `DigestLinkedCommits`. The block half supports the injective encoding of
  `ASM-LIVE-FINITE-REFERENCE-SPACE`.
- **Rust evidence:** Both digests are fixed byte arrays taken over the complete
  serialized body, so each digest covers every field of its body. This is now
  reviewed; see [REF-DIGEST-CHAIN](#verified-current-rust-behavior). What
  remains is the standard hash property, plus the fact that the serialization
  sends different bodies to different bytes.
- **Discharge:** Confirm that the serialization is injective for a fixed schema,
  and map the synchronization range check onto `DigestLinkedCommits`. The hash
  half stays a cryptographic assumption. Carrying the link into the model is
  done.

## ASM-SAFE-COMMIT-STORE

- **Claim:** The durable commit store of one host is well formed. Index 0 holds
  genesis. If the head is at index `n`, the store holds a commit at every index
  from 0 to `n`, with no gap. The store keeps two forms of each commit, the
  short `(index, digest)` and the full body, and the two forms always agree in
  both directions. Each stored commit above genesis names the identifier of the
  commit one index below it. The host delivers each stored commit to its
  finalizer in index order with no gap.
- **Store note:** The model has one durable notion, `installedCommitAt`, which
  maps a commit index to a commit identifier. Rust has two commit tables. The
  `commits` table holds the commit record. The `finalized_commits` table holds
  the same `CommitRef` with the rejected transaction indices, and
  `CommitFinalizer` writes it only after every transaction in the commit is
  finalized or rejected. Both tables key on the same index and digest, so the
  single modeled notion is adequate for agreement on which commit sits at an
  index. Transaction outcomes are a separate durable fact that the commit-safety
  statement does not cover.
- **Type:** Single-host storage refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Supplies `zeroHeadIsGenesis`, `installedAtOrBelowHead`,
  `installedHeadLinksToPredecessor`, `exactInstalledHeadIsValid`,
  `exactHeadHasStoredId`, and `storedIdHasExactHead` of
  `ExactCommitDurablePrefixSourceMap`. The same gap-free rule supplies
  `Continuous` on the commit stream,
  `FinalizerLivenessStageObligations.continuousCommitStream`, and the
  long-enough visible prefix that the `VisibleFirst` binder of
  `mysticeti_v3_safety` needs. `EndToEndLivenessInputs.commitPrefix` restates
  the same head-contains-every-earlier-index and stored-form agreement facts
  for the liveness route.
- **Rust evidence:** Commit installs are ordered, and both commit tables key on
  index and digest. `CommitV1.index` starts at 1 after genesis and increases by
  one for each commit, and `CommitV1.previous_digest` links each commit to the
  one before it, so a produced commit sequence has no gap by construction. No
  review covers restart, or the case of a store that keeps the short form
  without the full body.
- **Evidence record:** [EV-DURABLE-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-durable-commit-prefix).
- **Discharge:** Read the commit write path and the restart path. Show that no
  index below the head is ever missing, and that the two stored forms cannot
  disagree.

## ASM-SAFE-INSTALL-PROVENANCE

- **Claim:** Every commit that a correct host installed has a real origin. A
  commit installed by local execution came from a committer run on that same
  host. A commit installed by sync came from a verified, certified bundle. A
  host signs and stores a commit vote only after that same host installed the
  commit.
- **Type:** Single-host Rust refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety.
- **Lean use:** Supplies `localInstallOrigin`, `verifiedSyncInstallOrigin`, and
  `correctCarrierFollowsInstalledCommit` of `ExactCommitInstallProvenance`.
  These three fields are what
  `correct_validators_agree_on_commit_at_index` consumes to reach two-host
  agreement. No field compares two hosts. The chain-to-prefix rule was a fourth
  field of this row. It is now the theorem
  `ExactCommitDurablePrefixSourceMap.digest_chain_entry_matches_installed_prefix`,
  and `ExactCommitInstallProvenance.verifiedChainEntryMatchesInstalledPrefix`
  re-exposes it with the signature the field had.
- **Rust evidence:** None recorded. `CommitObserver` and `CommitSyncer` are the
  two install paths, and no source review maps either one to its origin rule.
- **Evidence record:** [EV-EXACT-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-exact-commit-prefix).
- **Discharge:** Read `CommitObserver` and `CommitSyncer`. Show that each
  install records the origin the rule names, and that no commit vote leaves the
  host before the install.

## ASM-SAFE-FIRST-TRIGGER

- **Claim:** The Rust finalizer keeps a queue of unfinalized commits ordered by
  index. Each new commit first reruns direct finalization for all pending
  commits. For a target block, the first local descendant on each authority
  chain can give an implicit accept vote through the round after the commit
  leader; a vote cutoff that covers the target or an explicit reject blocks the
  implicit accept, and accept votes count only when the target is above the GC
  round of its commit. The transaction vote tracker supplies explicit reject
  votes. An accept quorum accepts, and a reject quorum rejects. If the earliest
  commit stays pending and at least one later commit exists, indirect
  finalization applies the same first-vote rule to committed descendants only
  and accepts transactions with certification stake. When the newest leader
  round is at least `indirectCommitDepth` rounds ahead of the earliest, the
  finalizer rejects every transaction that still has no committed accept
  certificate. A commit leaves the queue when every one of its transactions has
  a decision, so depth two is the latest release point. This row assumes that
  the depth comparison is the modeled `FirstEligible` choice. Use of the *same*
  trigger by two hosts is the conclusion, not the claim.
- **Type:** Single-host Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Supplies the `FirstEligible` half of the `VisibleFirst` binder
  of `mysticeti_v3_safety`. `first_trigger_agreement` then derives trigger
  equality from `firstEligible_unique` and the one commit stream.
  `mysticeti_v3_safety` returns that equality as part of its conclusion. The
  other half of the binder, that the host has the stream up to the trigger
  position, is gap-free delivery and belongs to `ASM-SAFE-COMMIT-STORE`.
- **Rust evidence:** The in-progress branch
  `tmw/mysticeti-v3-transaction-voting` implements this algorithm as
  `CommitFinalizerV3` in `commit_finalizer_v3.rs`, with the shared depth
  constant `INDIRECT_COMMIT_DEPTH` of two and no remote-commit wait. Commit
  leader rounds increase along the stream, so the first deep-enough commit is
  the one whose arrival forces the release of a still-pending earliest commit.
  This tree still carries the older finalizer; the mapping revalidates on
  merge.
- **Evidence record:** [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Merge the v3 finalizer. Then record the mapping from
  `process_commit` and `compute_indirect_decisions` to `FirstEligible` with
  file and line, and confirm that the comparison uses the earliest commit's
  leader round as the pending round and the newest commit's leader round as the
  test value.

## ASM-SAFE-COMMITTED-PREFIX

- **Claim:** Before the first trigger, the committed prefix contains every decision witness required by the indirect rule.
- **Type:** Protocol and Rust refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety.
- **Lean use:** Direct-against-indirect safety needs the missing witness in the prefix.
- **Rust evidence:** Local and synchronized paths construct prefixes. The branch finalizer counts only strict descendants of the target and drops targets at or below the commit GC round, which keeps required accept evidence inside the commit stream until the depth-two decision. Complete inclusion is not proved.
- **Evidence record:** [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Prove witness inclusion for local, synchronization, replay, and restart paths.

## ASM-SAFE-GC

- **Claim:** Old-block cleanup uses the preceding commit boundary. Before a
  commit install moves GC, the host accepts and catalogues every exact
  commit-body block. It retains decision evidence until the rule no longer
  needs it or copies the evidence into the committed prefix.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Leader and transaction safety use retained anchor and vote
  evidence through `TransactionEvidence.gcWindow` and `LeaderEvidence.coreGc`.
  `LeaderEvidence.anchorDepth` keeps the anchor inside the retained window, and
  `TransactionEvidence.blockEvidence` keeps the live DAG and the buffered
  committed prefix as separate stores, so later block GC changes only the live
  store. On the liveness route,
  `EndToEndLivenessInputs.installedCommitParents` and `.installedHeadBootstrap`
  are the install-time catalogue of commit material above GC. The liveness half
  of the old claim, the retained above-GC frontier, is now
  `ASM-LIVE-GC-FRONTIER`.
- **Rust evidence:** The v3 certified-commit path accepts each commit's blocks
  before it handles and records the commit, and asserts the chain link to the
  last local commit. Atomic persistence, restart, and the complete evidence
  path still need a full refinement check.
- **Evidence records:** [EV-DURABLE-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-durable-commit-prefix) and [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Enforce the required depth, test accept-before-GC ordering for
  local and synchronized installs, and close the complete evidence mapping.

## ASM-LIVE-GC-FRONTIER

- **Claim:** A commit install does not remove an accepted block that stays
  above the new local GC round. The block stays retained, so a later proposal
  can use it as a parent.
- **Type:** Single-validator storage refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Liveness.
- **Lean use:** The `retention` binder of
  `current_sources_give_end_to_end_liveness_probability_one`:
  `ValidatorCommitOrthogonalAcceptedRetentionRules.acceptedAboveGcIsRetained`.
  The head-independent proposal gate uses the retained above-GC block as a
  legal parent. This row used to point at
  `EndToEndLivenessInputs.localGcCutoffCausal`; that field had no consumer and
  stated Flex-scan cutoff rules instead of this retention rule, so the field
  was deleted.
- **Rust evidence:** Accumulated-prefix retention across a GC move has no
  refinement check.
- **Discharge:** Map the post-GC frontier retention across local and
  synchronized installs.

## ASM-CONFIG-V3-ACTIVATION

- **Claim:** Authenticated epoch state enables the analyzed v3 leader and transaction paths.
- **Type:** Configuration applicability refinement.
- **Status:** Known mismatch.
- **Effect if false:** Applicability.
- **Lean use:** The model describes the v3 protocol path.
- **Rust evidence:** Normal startup does not enable the analyzed path from epoch state.
- **Discharge:** Add versioned activation, rollback rules, and mixed-version tests.

## ASM-CONFIG-VOTING

- **Claim:** V3 activation also activates the modeled transaction-voting rule for every correct validator.
- **Type:** Configuration refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** Transaction safety always uses v3 voting semantics.
- **Rust evidence:** V3 and transaction voting can be configured independently. This also holds on the branch: with transaction voting off, the v3 finalizer passes commits through as already finalized.
- **Discharge:** Use one feature value or reject an incompatible pair.

## ASM-REFINE-INTEGERS

- **Claim:** Product integer types and calculations represent every modeled value without overflow, truncation, or invalid conversion.
- **Type:** Data refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety and liveness.
- **Lean use:** The model uses unbounded natural numbers.
- **Rust evidence:** The product uses bounded types and does not check every modeled operation.
- **Discharge:** Set limits, use checked calculations, and test all boundaries.

## ASM-LIVE-PARTIAL-SYNCHRONY

- **Claim:** After an unknown stabilization time, each protocol message between correct validators arrives within `delta`.
- **Type:** Network environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** Progress proofs use bounded delivery only after stabilization.
- **Rust evidence:** The product can retry and measure delays but cannot enforce the network bound.
- **Discharge:** Keep this condition in the deployment model.

## ASM-LIVE-COMMON-GENESIS

- **Claim:** Every correct, available validator starts from one common genesis.
  The genesis commit has index zero and round zero. At trace time zero the
  validator has that commit installed at index zero and holds it as its commit
  head. The validator also starts with one common round-zero parent list, one
  block for each author in a set with quorum stake. These round-zero blocks
  start accepted in local storage and stay accepted at every trace time.
- **Type:** Startup boundary model.
- **Status:** Accepted modeling assumption.
- **Effect if false:** Liveness.
- **Lean use:** `EndToEndLivenessInputs.initial` gives the installed genesis
  commit and the time-zero head at every correct, available validator.
  `.genesisParents` gives the common parent list, its quorum stake, initial
  acceptance, and permanent retention. Round-zero proposal and
  `.operationalQuorumFrontier` build on these blocks. The trace-start bullet in
  "Source-to-model and local product conditions" states the same time-zero
  boundary.
- **Rust evidence:** Each validator derives the same genesis blocks and the
  same virtual genesis commit reference, `GENESIS_COMMIT_INDEX` with
  `CommitDigest::MIN`, from the epoch committee. `DagState` keeps genesis
  bodies in a dedicated map that GC eviction does not touch. A validator that
  restarts inside the epoch starts its trace with a higher commit head, so the
  time-zero head condition selects epoch-start executions. That is a scope
  choice, not a Rust rule.
- **Discharge:** Keep the deterministic genesis derivation and the dedicated
  genesis storage in Rust. To remove the epoch-start scope choice, extend the
  liveness analysis to a trace that starts at a later installed head.

## ASM-LIVE-FINITE-REFERENCE-SPACE

- **Claim:** `BlockId` has one fixed finite injective encoding.
- **Type:** Data refinement.
- **Status:** Partially verified.
- **Effect if false:** Commit liveness.
- **Lean use:** `ValidatorFiniteBlockIdEncoding`. With the capsule facts of
  `ASM-LIVE-CAPSULE-PROJECTION`,
  `ValidatorPersistedCausalCapsuleFiniteReferenceSourceMap.toRoundAdmission`
  turns the encoding into the `admission` input of the CL goal at the static
  per-round cap `authorityCount * blockIdCount`. The cap is a conclusion, not a
  claim.
- **Rust evidence:** `BlockDigest` is a fixed byte array, and the committee has
  a finite authority count. Injectivity for a fixed schema is the block half of
  `ASM-SAFE-DIGEST-IDENTITY`.
- **Evidence record:** [EV-FINITE-REFERENCE-SPACE-TIMING](ASSUMPTION_EVIDENCE.md#ev-finite-reference-space-timing).
- **Discharge:** Complete
  [REF-FINITE-BLOCK-ID-SPACE](#missing-rust-behavior-and-open-source-refinements),
  and check all modeled arithmetic against product integer limits.

## ASM-LIVE-CAPSULE-PROJECTION

- **Claim:** When a correct validator persists a proposal, it also persists one
  causal recovery capsule for that block, and that capsule is the one the
  backlog projection reads. Its block references are unique, each author is in
  the finite authority set, and each history round is at or below the target
  block round.
- **Type:** Single-validator source refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Commit liveness.
- **Lean use:** `ValidatorPersistedCausalCapsuleFiniteReferenceSourceMap` and
  the `maxAdmittedRefsPerRound` binder with
  `ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap`, the `admission`
  input of the CL goal. The receiver's GC round then gives a linear bound on
  unresolved history. The adopted proof combines pointwise visibility bounds
  with one fixed-reference quadratic wait. The wait does not change when a
  local commit head changes.
- **Rust evidence:** The exact persisted-capsule pin projection and all capsule
  source fields still need a complete Rust-to-Lean map. Current Rust limits
  immediate parents, and it does not enforce the transitive per-round cap when
  equivocating branches merge.
- **Evidence record:** [EV-FINITE-REFERENCE-SPACE-TIMING](ASSUMPTION_EVIDENCE.md#ev-finite-reference-space-timing).
- **Discharge:** Complete
  [REF-CAUSAL-CAPSULE-PROJECTION](#missing-rust-behavior-and-open-source-refinements):
  capsule uniqueness, author range, target-round upper bound, and persisted
  pin-projection equality.

## ASM-LIVE-ROUND-CATCHUP

- **Claim:** When fixed-reference liveness needs skipped rounds, an active
  correct validator persists only round `highestSignedRound + 1`. Each actual
  fresh intermediate persistence used by the final path has an earlier
  commit-progress-recovery timer generation, exact `proposeNext` action, and
  refreshed parent snapshot for the same block. The source rules classify
  current or past actions. They do not supply a future block or window.
- **Type:** Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** The completed conditional theorem does not apply to the
  product. Lean cannot reconstruct its finite exact timer-paced window from
  later unbounded own-block production.
- **Lean use:** `ValidatorV2RoundCatchupSourceMap` gives the two past-only
  source rules. From them the proof derives, rather than assumes, that one
  actual later own block proves each required intermediate persistence; a
  generic normal-proposal origin is not an input.
  `actual_high_own_block_gives_fresh_timer_paced_intermediate`
  uses the first signer-floor crossing to recover one exact target.
  `block_production_liveness_gives_backfilled_timer_paced_window` applies this
  result to every required author and offset. It returns only actual
  timer-paced productions.
- **Rust evidence:** `try_new_block` reads `threshold_clock_round()` and checks
  only that this round is higher than the last own proposal. The local round
  can skip intermediate own rounds. The timeout callback then calls
  `try_propose`, which reads the current threshold again. The current timeout
  task does not keep one unique fixed timer generation for each skipped round.
- **Evidence record:** [EV-ROUND-CATCHUP](ASSUMPTION_EVIDENCE.md#ev-round-catchup).
- **Discharge:** Add a safe no-skip proposal queue, or an equivalent
  intermediate-round worker. Persist each target in order. Keep the exact
  timer key and refreshed parent snapshot until that target runs. Add tests for
  threshold-clock jumps, timer resets, commit interference, and restart.

## ASM-LIVE-COMMIT-PROGRESS-RECOVERY

- **Claim:** An active v3 validator enters recovery when its normal
  threshold-clock proposal gap reaches `P_enter` or the time since any commit
  install reaches `T`. It stays active while the gap reaches `P_exit` or the
  time reaches `T`, where `P_exit < P_enter`. It exits only when both signals
  clear. This probe is separate from the exact-next target. The validator
  requests missing parents and proposes only after it accepts an
  immediate-parent quorum. Parent selection includes every current accepted and
  retained representative, one per author, without schedule prediction or
  score exclusion. A correct receiver with a broken or idle tip stream retries
  its subscription, and a successful subscription replays the current signed
  tip or a newer own tip. A validator in recovery also sends its current
  signed tip to every other validator again. Exact-next recovery uses the
  round after its highest durable own round. Cleanup safe resume durably records the canonical target
  `max(P + 1, G + 2)` and its parent need before proposal selection. It does not
  require an accepted future block at the target. While recovery is active
  above GC, stale normal ready work cannot persist. Persistence must use the
  current recovery proposal and the unique timer for the same validator and
  target round. Recovery pacing uses one fixed reference round `R_c` and the
  wait `W(R) = b + l * (R - R_c) + q * (R - R_c)^2`, and the exact-next timer
  is prompt as an action-local rule. Timer spread is not claimed; the proof
  derives it from actual prior broadcasts and sync.
- **Split note:** This row is one proposed feature that is one known mismatch
  today. When implementation starts, split it along the `REF-RECOVERY-*` rows,
  one row per mapped behavior.
- **Type:** Protocol and Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** `ValidatorFixedReferenceStrictCurrentSourceMaps` supplies the
  fixed wait, one action-local exact-next timer-promptness rule, authenticated
  correct-body ownership, past recovery-timer origins, and the checked
  quadratic coefficient. The `parameters` binder carries the wait coefficients.
  The `syncSources` binder, `ValidatorFreshRoundPinnedSyncSourceRules`, gives
  pinned sync over `EndToEndLivenessInputs.recoverySourcePins`, and the
  capsule-sync, genesis, tip-subscription, and rebroadcast fields supply
  recovery parent acquisition and replay. The proof derives timer spread from
  actual prior broadcasts and pinned sync. V2 round catch-up supplies the finite exact
  production family. Commit-orthogonal retention makes each selected leader
  usable before the next proposal snapshot. Local Flex execution and
  exact-prefix induction then derive commit progress and pointwise catch-up. The theorem
  `current_sources_give_end_to_end_liveness_probability_one` completes this
  conditional chain.
- **Rust evidence:** The current product has no commit progress recovery mode.
  It uses fixed proposal waits, not this fixed-reference quadratic wait.
  `force=true` does not supply the full recovery parent-selection rule or the
  action-local exact-next timer-promptness rule.
- **Evidence record:** [EV-FINITE-REFERENCE-SPACE-TIMING](ASSUMPTION_EVIDENCE.md#ev-finite-reference-space-timing).
- **Discharge:** Implement the recovery design and complete every recovery
  `REF-*` mapping. Implement the fixed-reference wait, exact-next promptness,
  V2 round catch-up, and the remaining current-source mappings. Exact
  replay is not a required product refinement or liveness route.

## ASM-LIVE-LOCAL-PROPOSAL

- **Claim:** While the epoch is active, a correct, available validator with a
  valid current frontier target eventually stores and sends an own block,
  already has that round, or observes a higher threshold frontier. The one-shot
  forced timeout and the last-known-own-round and propagation-delay watchers
  supply the attempts. Non-interruption of an active Core callback is
  `ASM-LIVE-CORE-HANDLER`, not part of this row.
- **Type:** Single-validator Rust refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Block-production and commit liveness.
- **Lean use:** The pure network-DAG proof applies this local rule after it
  derives the required parent and timer guards. That public DAG goal does not
  require per-validator production. The adopted commit proof also uses
  `ValidatorV2BlockProductionCurrentSourceMaps.blockProductionLiveness` to
  derive unbounded later own blocks before it applies finite no-skip catch-up.
- **Rust evidence:** The maximum timeout makes one forced attempt. A change to the recovered own round makes another forced attempt. A propagation-delay change makes another forced attempt only when the full proposal gate flips from blocked to clear, so a cleared delay alone forces nothing while the recovered own round is missing or higher. A forced attempt bypasses leader presence and minimum-delay checks. It can still stop because the round is stale, the block already exists, the recovered own-round guard blocks a duplicate signature, or one of the two temporary blockers is active.
- **Evidence record:** [EV-NETWORK-ROUND-PROGRESS](ASSUMPTION_EVIDENCE.md#ev-network-round-progress).
- **Discharge:** Map each forced-return branch and the two watcher retries to `ValidatorNormalFrontierPacemakerRules`. Add tests for the stale, already-signed, recovered-round, and temporary-blocker cases.

## ASM-LIVE-CORE-HANDLER

- **Claim:** The running-Core trace starts after `recover_validator` finishes. Each qualifying external Core input handler then completes all commit work enabled by its finite batch and the finite blocks that GC cleanup unsuspends. In this positive DAG refinement, a qualifying handler is an `add_blocks` call that accepts at least one ordinary block. The local commit loop observes a terminal no-more-commits result. Core then invokes the normal proposal attempt before that handler returns. Core is single-threaded: a commit or block-accept action cannot interleave after the proposal callback starts.
- **Type:** Single-validator Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Block-production and commit liveness.
- **Lean use:** Trace time zero is the post-recovery running state; startup recovery is not an action source in this trace. `ValidatorPacketDrivenBlockAcceptanceAt` records exact past packet delivery and block acceptance. `packetDrivenAcceptanceHasInputOrigin` maps it to a qualifying `ValidatorCoreHandlerInputObservation`. `ValidatorCoreHandlerRefinementRules` then returns a `ValidatorFiniteCoreHandlerEpisode`. `qualifying_core_handler_input_has_terminal_scan_and_normal_proposal_attempt` exposes the terminal scan and later normal proposal attempt before return. `ValidatorFiniteCoreHandlerEpisode.proposal_attempt_input_and_suffix` connects the attempt input to the handler-exit state and the remaining finite event suffix. `qualifying_core_handler_input_has_current_proposal_continuation` then returns a same-batch proposal action or a `ValidatorCoreProposalContinuationAt` current-state witness. `ValidatorAuthorLocalPreAttemptScheduleAt` narrows commit interference to a protected normal callback, an armed exact recovery timer, or an occupied protected timer-arm goal, each before proposal latch. Raw parent readiness is not a schedule. The first-install rule returns exact past persistence or one protected post-install normal callback. `EndToEndLivenessInputs.commitProposalNonInterference` is the same serial rule as the last claim sentence: one local commit handler preserves the threshold-clock proposal round. These results do not return proposal success as an input, a common DAG layer, or a commit result.
- **Rust evidence:** On the v3 path, `try_commit_v3` loops until `FlexCommitter::try_commit` returns `None`. Each successful iteration calls `post_commit`, which can unsuspend blocks after GC changes. `Core::add_blocks` calls this loop and then `try_propose(false)` when the input accepts at least one block. The current action origin does not yet distinguish direct `add_blocks` input acceptance from later GC-unsuspension.
- **Discharge:** Map `ValidatorPacketDrivenBlockAcceptanceAt`, `packetDrivenAcceptanceHasInputOrigin`, `handlerInputOccurs`, `qualifyingInputHasFiniteHandler`, and `ValidatorCoreProposalAttemptContinuationRules` to the guarded ordinary `add_blocks` path. Add an exact direct-input acceptance origin. A GC-unsuspended block must use its enclosing handler continuation, not start another handler. Prove that finite input and finite GC-unsuspension give finite local commit work. Test the terminal no-more-commits result, the following proposal-attempt invocation, and its exact past or current continuation. Do not require that the attempt succeeds. Model certified-commit processing separately; do not use it as positive DAG progress.

## ASM-LIVE-LEADER-STAKE

- **Claim:** The epoch stake numbers satisfy `P_r <= S <= N`, `f + c < S`, and
  `A <= P_r`. The selected leader set of every commit head carries more stake
  than the faulty plus unavailable budget.
- **Type:** Configuration refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Liveness.
- **Lean use:** `EndToEndLivenessInputs.leaderSchedule.scheduleViable`.
  Progress derives correct available scheduled stake and quorum coverage from
  the stake bounds.
- **Rust evidence:** Current v3 has `P_r = S`. Startup does not check the stake
  bounds against actual epoch stake.
- **Discharge:** Check the bounds against actual epoch stake at startup.

## ASM-LIVE-LEADER-SCHEDULE

- **Claim:** The schedule Rust computes behaves as the modeled `config`
  functions. One commit head determines one selected leader set
  `config.leaderSchedule` and one selected slot order
  `config.selectedLeaderOrder`. The slot decider, the FlexCommitter, and the
  v3 finalizer use one shared indirect depth.
- **Not claimed here:** The row this came from also said that correct validators
  derive the same selected leader slot order. That is not an assumption. The
  model gives every validator one shared `ValidatorEpochConfig`, so
  `config.leaderSchedule` is one function by construction, and agreement is
  built in rather than supplied. The old row also described what the proof
  checks after a commit install. A claim states what the system does, not what
  the proof does.
- **Type:** Single-validator Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Liveness.
- **Lean use:** `EndToEndLivenessInputs.leaderSchedule.indirectDepth`,
  `.indirectDepthPositive`, and
  `EndToEndLivenessInputs.flexCommitterDepthMatchesLeaderSchedule`. The CL goal
  reads the schedule through its `family` binder:
  `UniformRankingEndToEndExecutionFamily.leaderScheduleMatches` keeps one fixed
  selected set for each commit head, and `.indirectDepthMatches` keeps one
  fixed depth. The deterministic repeated-first structure
  (`DeterministicLeaderCoverageInput` with `.schedule`, `.selectedSetMatches`,
  and `.firstSelectedSlotMatches`) is the alternative route and is not an input
  of the CL goal. A separate order rule derives a favorable leader window, and
  an action-scoped parent rule must include the exact first Flex leader.
- **Rust evidence:** Within one Core, the proposer and the FlexCommitter receive
  the current `NextCommitLeaderSchedule`. Off-boundary commits keep
  `allowed_leaders`; a boundary commit can change it. After each commit the
  FlexCommitter keeps its pending rounds only when the new vector is identical
  and then drops rounds below the new minimum next leader round; any vector
  difference resets all pending round state. The branch v3 finalizer imports
  the same `INDIRECT_COMMIT_DEPTH` as the slot decider. No review maps the
  schedule algorithm itself onto the modeled `DeterministicLeaderSchedule`.
- **Evidence records:** [EV-SCHEDULE-HEAD-LOCAL](ASSUMPTION_EVIDENCE.md#ev-schedule-head-local) and [EV-FIRST-FLEX-LEADER-PARENT](ASSUMPTION_EVIDENCE.md#ev-first-flex-leader-parent).
- **Discharge:** Map the schedule algorithm to the modeled one, add the
  action-scoped proposal schedule read, and stop score exclusion from dropping
  the accepted and retained exact first leader.

## ASM-LIVE-FIRST-SLOT-SAMPLING

- **Claim:** For each round, the round-seeded shuffle is modeled as an
  independent uniform permutation of the current allowed-leader list. The list
  is chosen before that round's shuffle.
- **Not claimed here:** Cross-validator agreement on the shuffle result is
  built in: the model computes the order from one shared
  `ValidatorEpochConfig`, so agreement is not supplied as an assumption. The
  fixed Byzantine and unavailable sets are `ASM-SAFE-FAULT-BOUND`, not part of
  this row.
- **Type:** Accepted probabilistic protocol model.
- **Status:** Accepted modeling assumption.
- **Effect if false:** Liveness.
- **Lean use:** Lean derives the probability-one causal-head favorable-window
  event from the abstract independent-uniform law. The conditional commit
  capstone then uses that event. The final entry theorem is
  `current_sources_give_end_to_end_liveness_probability_one`. Its `law` binder
  is `IndependentUniformRoundRankingLaw`, its `family` binder connects the
  sampled ranking to the configuration through
  `UniformRankingEndToEndExecutionFamily.firstSelectedLeaderMatchesRanking`,
  and its `rankingSource` binder, `UniformRankingExecutionSourceMap`, keeps the
  execution before a round dependent only on earlier rankings.
- **Rust evidence:** The product seeds the same deterministic shuffle from the
  round number. This gives validator agreement and crash-recovery
  reproducibility. The assumption treats the shuffle results for distinct round
  seeds as independent uniform pseudorandom permutations.
- **Boundary:** This is a pseudorandomness assumption. It is not a claim that the
  deterministic runtime uses fresh entropy. Lean uses only the first selected
  slot, so the proof also works with the weaker first-slot-only assumption. The
  first output word of the generator is not the proof boundary because the
  shuffle can consume more than one output word.
- **Evidence record:** [EV-FIRST-SLOT-PROBABILITY](ASSUMPTION_EVIDENCE.md#ev-first-slot-probability).
- **Discharge:** Keep the first-slot pseudorandomness model explicit. Pin the
  generator and shuffle algorithm if exact cross-version reproducibility is a
  protocol requirement, or replace the assumption with a proved deterministic
  coverage rule.

## ASM-LIVE-POST-GST-CAUSAL-SERVICE

- **Claim:** After GST, there is one fixed service interval and two finite bounds
  `C_add < C_service` for each correct, available validator. New rounds add at
  most `C_add` required above-GC references to its causal-work queue in one
  interval. If at least `C_service` items are pending, fetch, verification, and
  acceptance remove at least `C_service` items in that interval. If fewer items
  are pending, all pending items finish or become obsolete because GC moved.
  One finite removal cap bounds `workRemoved` in every interval.
- **Derived, not claimed:** Two results used to sit in the claim above. That
  each fixed known above-GC causal history eventually becomes accepted is the
  conclusion these rates give, not an input. The margin `C_add < C_service` is
  not assumed on its own either; `ValidatorCausalQueueTransferBudget` derives it
  from a coarse block-transfer budget and a block-production bound, so the
  margin belongs to `ASM-LIVE-TRANSFER-BUDGET`.
- **Type:** Single-validator performance model.
- **Status:** Partially verified.
- **Effect if false:** The completed conditional commit theorem does not apply
  to the product.
- **Lean use:** `BlockProductionLiveness` requires unbounded own-block
  production by each correct, available validator, not contiguous own rounds.
  `ValidatorV2BlockProductionCurrentSourceMaps.blockProductionLiveness` derives
  this result from selected support, recursive need, queue-source, and no-idle
  rules. The GC replacement and exact phase-continuity parts of these rules are
  proposed scheduler and refinement behavior. They are not derived from
  current Rust. V2 round catch-up then recovers the finite exact intermediate
  family. Fixed-reference pacing, pinned sync, retention, local Flex, and
  exact-prefix induction complete the Lean route.
- **Rust evidence:** Fetch, verification, parent-first acceptance, GC filtering,
  and batched block processing exist. Current evidence does not prove the
  strict sustained-rate comparison or unbounded per-validator authorship under
  continuous round advancement.
- **Evidence record:** [EV-POST-GST-CAUSAL-SERVICE](ASSUMPTION_EVIDENCE.md#ev-post-gst-causal-service).
- **Discharge:** Define measurable queue work and round-created work. Prove or
  enforce the strict service margin. Map the V2 selected support, recursive
  need, queue source, no-idle rule, and skipped-round proposal continuity.

## ASM-LIVE-TRANSFER-BUDGET

- **Claim:** After GST, at most a fixed number of whole blocks can reach one
  correct, available validator in one `delta`. That budget is large enough for
  ordinary round advancement, with room to spare. The production bound is the
  committee size times a positive rounds-per-interval bound. One author can add
  at most one block in each round. The above-GC references that new rounds
  require stay strictly below the transfer budget. The queue removal cap equals
  that interval budget. A validator whose causal-work queue holds at least one
  interval's budget uses the whole budget in that interval.
- **Type:** Network and pipeline environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness. If arrivals reach the budget, the causal-work
  backlog cannot shrink, so catch-up stops.
- **Lean use:** `ValidatorCausalQueueTransferBudget.toServiceRules` supplies the
  service margin of `ASM-LIVE-POST-GST-CAUSAL-SERVICE`.
  `productionBoundPositive` prevents a zero production bound for a populated
  committee and a positive round bound.
  `high_backlog_drains_by_spare_capacity` gives the drain rate as the surplus
  over the production bound, and `saturated_interval_does_not_drain` shows that
  the budget is necessary and not only sufficient.
- **Rust evidence:** `max_blocks_per_fetch` and `max_blocks_per_sync` cap the
  blocks in one fetch and one sync response, and `max_transactions_in_block_bytes`
  and `max_num_transactions_in_block` cap one block. No component measures or
  enforces a link budget, and no configured value is derived from one.
- **Discharge:** Keep the budget in the deployment model. Choose the block and
  fetch limits so that the budget stays above the production bound for the
  planned committee size and round rate, and measure both.

## ASM-LIVE-BLOCK-SYNC

- **Claim:** After a correct, available validator receives an ordinary block body, its synchronizer fetches every missing causal-history block above the applicable cleanup round. Each fetch completes independently with bodies or an error. When a body is processed, the receiver uses its current GC round: it drops the body at or below GC, or discovers and fetches its missing above-GC parents. An error does not make an above-GC reference obsolete; normal retry can try it again. A commit does not rebase an in-flight fetch job.
- **Type:** Accepted single-validator synchronization model.
- **Status:** Accepted modeling assumption.
- **Effect if false:** Liveness.
- **Lean use:** Recursive direct-parent needs plus partial synchrony derive the
  finite above-cleanup causal closure of one received block. Parent-ready
  buffering then accepts it from parents to children.
  `EndToEndLivenessInputs.operationalQuorumFrontier` records the resulting
  current causal closure above the local GC boundary. The finite
  reference-space theorem and the receiver's current GC cutoff give the
  pointwise visibility bound. The adopted final proof uses pinned sync and
  commit-orthogonal retention with fixed-reference pacing. It does not use
  commit synchronization as positive progress.
- **Rust evidence:** Several fetch mechanisms exist. The proof currently accepts their best-effort above-cleanup behavior. A complete source check is deferred.
- **Evidence record:** [EV-NETWORK-ROUND-PROGRESS](ASSUMPTION_EVIDENCE.md#ev-network-round-progress).
- **Discharge:** Later check peer choice, retries, buffering, restart recovery, cleanup handling, and exact-reference reads against Rust. Do not add a below-cleanup recovery path.

## ASM-LIVE-PEER-FAIRNESS

- **Claim:** A correct validator retains and serves active local recovery data. A pending request retries authenticated validator peers in a fair order until the item arrives or becomes unnecessary. For ordinary blocks, this rule covers every recursively discovered missing causal-history reference above the applicable cleanup round.
- **Type:** Single-validator storage and synchronization refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness after missed state or restart.
- **Lean use:** The proof derives a correct data source, then combines local retry and service with post-stabilization delivery.
- **Rust evidence:** Storage and serving paths exist. Recovery-data retention and fair retry selection are incomplete.
- **Discharge:** Implement the local retention, service, and retry rules.

## ASM-LIVE-TASK-FAIRNESS

- **Claim:** A continuously enabled protocol task at a correct validator eventually runs.
- **Commit-sync note:** The main known starvation risk is optional commit
  synchronization, whose traffic and local work share the host with ordinary
  block fetch, subscription retry, proposal work, and recovery timers. This
  used to be its own row, `ASM-LIVE-COMMIT-SYNC`. That row supplied no field of
  its own: sync-install safety is `ASM-SAFE-INSTALL-PROVENANCE`, a gap-free
  installed stream is `ASM-SAFE-COMMIT-STORE`, and no goal theorem takes
  commit-sync success as an input. What remains is this fairness claim.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** Every temporal progress chain needs enabled work to continue.
- **Rust evidence:** Tasks and queues implement the work but cannot guarantee
  scheduler fairness, and no resource reservation isolates commit sync from the
  ordinary path. The control transitions exist: a suspended subscription checks
  local commit progress once per second, connection attempts retry with bounded
  backoff, and ten seconds without local commit-index movement enables periodic
  block-sync failover until one batch of progress occurs.
- **Evidence record:** [EV-COMMIT-SYNC-COVERAGE](ASSUMPTION_EVIDENCE.md#ev-commit-sync-coverage).
- **Discharge:** Define allowed shutdown and failure states in the runtime
  model, and keep the commit-sync control transitions until scheduling or
  resource isolation enforces non-starvation.

## ASM-LIVE-LOCAL-RESPONSE

- **Claim:** Correct local clocks advance. A correct timer does not expire early, and every finite timer expires. Each covered local consensus action completes within a finite bound `epsilon`.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Timely-vote liveness.
- **Lean use:** Recovery pacing uses a bound for proposal, storage, acceptance, and voting work.
- **Rust evidence:** The product has no end-to-end deadline for all covered local work.
- **Discharge:** Validate a deployment bound or weaken the timed result.

## ASM-LIVE-FINALIZER-TRIGGER

- **Claim:** Every pending transaction eventually reaches a later commit that
  is deep enough to trigger it.
- **Not claimed here:** The row used to end with "or a defined safe epoch-tail
  result". That clause supplies no field. `triggerEventually` says the trigger
  happens, with no alternative branch. What an epoch end should do with still
  pending transactions is product work, tracked as
  [REF-FINALIZER-TAIL](#missing-rust-behavior-and-open-source-refinements).
- **Type:** Protocol liveness refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Liveness.
- **Lean use:** Transaction progress needs a trigger after the target commit.
- **Rust evidence:** The branch `CommitFinalizerV3` rejects every remaining
  pending transaction at depth two, so a deep enough later commit completes
  the earliest pending commit. That such a commit arrives is not derived from
  the commit liveness result.
- **Evidence record:** [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Derive the trigger arrival from commit liveness: committed
  leader rounds increase without bound.

## ASM-LIVE-DURABILITY

- **Claim:** After the stable start, a produced commit enters the finalizer
  within one message delay, a transaction that reached a deep enough trigger is
  decided within one message delay, and a decided transaction reaches durable
  output within one more message delay.
- **Moved out:** This row used to claim two more things: that a decision is
  durable before it is exposed, and that it survives restart. Neither supplies a
  field of a goal theorem, and both are current product behavior rather than
  open conditions. They are now
  [REF-DURABLE-PROPOSAL](#verified-current-rust-behavior) and
  [REF-DURABLE-COMMIT-OUTPUT](#verified-current-rust-behavior), and restart is
  described in [PROOF_SCOPE.md](PROOF_SCOPE.md#committed-material). Dropping
  them is why `Effect if false` is now liveness alone.
- **Type:** Single-validator timing model.
- **Status:** Partially verified.
- **Effect if false:** Liveness.
- **Lean use:** `FinalizerLivenessStageObligations.triggerToDecision` and
  `.decisionToDurableOutput`. Both are `WithinAfter` bounds of one
  `network.delta`. The `commitEntersFinalizer` binder of
  `transaction_liveness_stage_composition` is the same kind of stage bound:
  commit production to finalizer entry within one `network.delta`. Transaction
  progress ends at durable output, not at an in-memory decision.
- **Rust evidence:** The proposal path flushes the block in `try_new_block`
  before broadcast, and the finalizer writes its decision before output, so
  what this row assumes is how long each stage takes, not whether write and
  send are ordered. The middle stage, a produced commit entering the
  finalizer, is an in-process send with no flush in `post_commit`; its
  durability arrives inside the finalizer. No measurement bounds any stage.
- **Evidence records:** [EV-DURABLE-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-durable-commit-prefix) and [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Bound the time from a finalizer decision to the flushed write,
  and from the flushed write to the consumer.
