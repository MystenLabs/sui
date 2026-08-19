/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.CommonCommitInduction
import Mysticeti.ExactCommitPrefixSafety
import Mysticeti.LeaderCoverage
import Mysticeti.ValidatorAnchorBridge
import Mysticeti.ValidatorAuthorLocalProposalContinuation
import Mysticeti.ValidatorBlockSyncBridge
import Mysticeti.ValidatorCommitCausalCarry
import Mysticeti.ValidatorCoreHandlerRefinement
import Mysticeti.ValidatorCoreProposalContinuation
import Mysticeti.ValidatorFlexCommitter
import Mysticeti.ValidatorLocalDagCommitPropagation
import Mysticeti.ValidatorNormalBlockLiveness
import Mysticeti.ValidatorOperationalFrontierCollectiveSuccessor
import Mysticeti.ValidatorOperationalFrontierPacemaker
import Mysticeti.ValidatorPacing
import Mysticeti.ValidatorExactNextRecovery
import Mysticeti.ValidatorProposalLatchBridge
import Mysticeti.ValidatorRecoveryCapsuleSyncExecution
import Mysticeti.ValidatorRecoveryParentNeedExecution
import Mysticeti.ValidatorRecoverySourcePinExecution
import Mysticeti.ValidatorRecoveryTimerDerivation
import Mysticeti.ValidatorRecoveryTipRebroadcastExecution
import Mysticeti.ValidatorReferenceFlexTrace
import Mysticeti.ValidatorRoundFrontierBridge
import Mysticeti.ValidatorTimedExecutionLemmas
import Mysticeti.ValidatorTraceFavorableWindow

namespace Mysticeti

/-!
The permitted input boundary for the end-to-end liveness proof.

This module proves unconditional network quorum-round progress from the local
execution inputs. The later DAG-to-commit composition is not complete. It is
not sound to add that remaining result as an unchecked premise or input field.

The inputs below contain only:

* static validator, fault, and leader-schedule facts;
* one-validator action and delivery rules;
* one initial genesis fact for each correct validator;
* standard partial synchrony; and
* bounded completion for each enabled local action.

The target remains `EndToEndLivenessGoal`. The design note in
`design/end_to_end_liveness.md` lists the internal theorems that must prove the
missing composition edges.
-/

/-- Static leader-schedule facts used by both leader-order designs.

This input does not say that a favorable leader window occurs. The
independent-uniform probability proof must derive that event from its sampling
law. The repeated-first alternative can derive it from its fixed order. -/
structure StaticLeaderScheduleInput
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config) where
  indirectDepth : Nat
  indirectDepthPositive : 0 < indirectDepth
  scheduleViable : ∀ commitId,
    config.thresholds.fault + faults.unavailableStakeBound <
      weight config.authorityCount config.stake
        (config.leaderSchedule commitId)

/-- One installed commit head has a favorable first-slot window after one
start round. -/
def CommitHeadFirstSlotWindowAfter
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (indirectDepth : Nat) (commitId : CommitId) (startRound : Nat) : Prop :=
  ∃ baseRound,
    startRound ≤ baseRound ∧
      ∀ offset,
        offset < indirectDepth + 1 →
        ∃ leader,
          leader < config.authorityCount ∧
            (config.selectedLeaderOrder commitId (baseRound + offset)).head? =
                some leader ∧
              faults.correctAvailable leader = true

/-- One installed commit head has favorable first-slot windows throughout the
unobserved future suffix used by one commit step. Past rounds are not included.
-/
def CommitHeadFirstSlotLeaderPathCoverageAfter
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (indirectDepth : Nat) (commitId : CommitId)
    (firstFutureRound : Nat) : Prop :=
  ∀ startRound,
    firstFutureRound ≤ startRound →
      CommitHeadFirstSlotWindowAfter config faults indirectDepth commitId
        startRound

/-- One installed commit head has a favorable first-slot window after each
start round. The deterministic repeated-first proof uses this stronger form. -/
def CommitHeadFirstSlotLeaderPathCoverage
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (indirectDepth : Nat) (commitId : CommitId) : Prop :=
  CommitHeadFirstSlotLeaderPathCoverageAfter config faults indirectDepth
    commitId 0

/-- Every commit head has a favorable first-slot window after each start round.

This is a path event. It is not a field of `EndToEndLivenessInputs`. The correct
first-slot validator can be different in each round of the window. -/
def FirstSlotLeaderPathCoverage
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (indirectDepth : Nat) : Prop :=
  ∀ commitId,
    CommitHeadFirstSlotLeaderPathCoverage config faults indirectDepth commitId

/-- Static repeated-first leader coverage for every installed commit head.

The selected validator set stays equal to the leader schedule. The first
selected leader slot follows the repeated-first rule. The schedule stake bound
guarantees that the schedule contains a correct, available validator. -/
structure DeterministicLeaderCoverageInput
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config) where
  indirectDepth : Nat
  indirectDepthPositive : 0 < indirectDepth
  schedule : CommitId → DeterministicLeaderSchedule config.authorityCount
  selectedSetMatches : ∀ commitId,
    (schedule commitId).selected = config.leaderSchedule commitId
  firstSelectedSlotMatches : ∀ commitId round,
    (config.selectedLeaderOrder commitId round).head? =
      some (repeatedFirstLeader (schedule commitId) (indirectDepth + 1) round)
  scheduleViable : ∀ commitId,
    config.thresholds.fault + faults.unavailableStakeBound <
      weight config.authorityCount config.stake (schedule commitId).selected

namespace DeterministicLeaderCoverageInput

variable {CommitId : Type}
  {config : ValidatorEpochConfig CommitId}
  {faults : FixedFaultInterval config}

/-- The static schedule bound gives one correct, available schedule member. -/
theorem contains_correct_available_member
    (coverage : DeterministicLeaderCoverageInput config faults)
    (commitId : CommitId) :
    ∃ member,
      faults.correctAvailable
        ((coverage.schedule commitId).memberAt member) = true := by
  exact viable_schedule_contains_correct_available_member faults
    (coverage.schedule commitId) (coverage.scheduleViable commitId)

/-- The static part of the repeated-first design supplies the common
leader-schedule input. -/
def toStaticLeaderScheduleInput
    (coverage : DeterministicLeaderCoverageInput config faults) :
    StaticLeaderScheduleInput config faults where
  indirectDepth := coverage.indirectDepth
  indirectDepthPositive := coverage.indirectDepthPositive
  scheduleViable := by
    intro commitId
    rw [← coverage.selectedSetMatches commitId]
    exact coverage.scheduleViable commitId

/-- The repeated-first order proves the derived fixed-path event. -/
theorem first_slot_path_coverage
    (coverage : DeterministicLeaderCoverageInput config faults) :
    FirstSlotLeaderPathCoverage config faults coverage.indirectDepth := by
    intro commitId startRound _startIsAfterZero
    rcases coverage.contains_correct_available_member commitId with
      ⟨member, memberCorrectAvailable⟩
    rcases repeated_first_has_depth_window_after
        (coverage.schedule commitId) faults.correctAvailable
        coverage.indirectDepth ⟨member, memberCorrectAvailable⟩ startRound with
      ⟨baseRound, startBeforeBase, favorable⟩
    refine ⟨baseRound, startBeforeBase, ?_⟩
    intro offset offsetInWindow
    let leader := repeatedFirstLeader (coverage.schedule commitId)
      (coverage.indirectDepth + 1) (baseRound + offset)
    refine ⟨leader, ?_, ?_, ?_⟩
    · exact (coverage.schedule commitId).memberInRange
        (repeatedFirstIndex (coverage.schedule commitId)
          (coverage.indirectDepth + 1) (baseRound + offset))
    · simpa [leader] using
        coverage.firstSelectedSlotMatches commitId (baseRound + offset)
    · exact favorable offset offsetInWindow

end DeterministicLeaderCoverageInput

namespace EndToEndInternal

/-- The anchor source map reads the same status-level pending state as the
validator execution model. -/
theorem anchor_input_matches_pending_status_state
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    (rules : ValidatorAnchorLocalRules config faults trace)
    (time validator : Nat) :
    (rules.flexInputAt time validator).toFlexState =
      validatorPendingFlexState ((trace time).validatorState validator) := by
  unfold ValidatorFlexInput.toFlexState validatorPendingFlexState
  rw [rules.flexInputCommitIndexMatches]
  rw [rules.flexInputRoundCountMatches]
  rw [rules.flexInputStatusesMatch]
  rfl

end EndToEndInternal

/-- Static genesis facts for the correct, available validators.

The genesis commit is configuration data. This structure does not constrain a
Byzantine validator and does not state a future progress result. -/
structure ValidatorGenesisInput
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (genesis : ValidatorCommitHead CommitId) : Prop where
  genesisIndex : genesis.index = 0
  genesisRound : genesis.round = 0
  installedAtCorrectValidator : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (world.validatorState validator).installedCommitAt genesis.index =
      some genesis.id
  currentHeadAtCorrectValidator : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (world.validatorState validator).commitHead = genesis

/-- Exact local action effects with an origin for each committer observation.

The base effect record contains block proposal, storage, and send results. Its
committer result is ghost data. The last field prevents this ghost data from
creating a successful run that did not occur in the main execution. -/
structure ValidatorExactExecutionEffectsSourceMap
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (program : ValidatorExecutionProgram BlockId CommitId)
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  effects : ValidatorExactExecutionEffects faults protocolPacket network
    program execution
  everyCommitterObservationOccurs : ∀ observation,
    effects.committerReturned observation →
    ∃ beforeEvents afterEvents actionBefore actionAfter,
      execution.events observation.time =
        beforeEvents ++
          (.localAction observation.validator .runCommitter :: afterEvents) ∧
      ValidatorWorldStep config faults protocolPacket program observation.time
        (execution.trace observation.time) beforeEvents actionBefore ∧
      ValidatorAtomicStep config faults protocolPacket program observation.time
        actionBefore (.localAction observation.validator .runCommitter)
          actionAfter ∧
      ValidatorWorldStep config faults protocolPacket program observation.time
        actionAfter afterEvents (execution.trace (observation.time + 1)) ∧
      observation.input =
        actionBefore.validatorState observation.validator

/-- The current common inputs for the future end-to-end theorem.

The record now includes the concrete one-validator recovery, post-GC bootstrap,
ordinary causal-block synchronization, proposal, and local FlexCommitter
mappings. The remaining work is distributed composition over these mappings.

The independent-uniform leader law belongs to the outer probability theorem.
It ranges over a family of these deterministic executions. A favorable sampled
path is not an input to either layer.

Do not add a recovery quorum, common round, block layer, favorable execution,
anchor, commit candidate, certificate, ready server, or later installed commit
to this structure. Each such fact is a theorem result. -/
structure EndToEndLivenessInputs
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop)
    (network : AddressedPartialSynchrony config faults protocolPacket) where
  authorityCountAtLeastTwo : 1 < config.authorityCount
  leaderSchedule : StaticLeaderScheduleInput config faults
  commitReferenceFunctions : CommitReferenceFunctions
    CommitId (LeaderBlockRef BlockId) Encoding
  genesis : ValidatorCommitHead CommitId
  program : ValidatorExecutionProgram BlockId CommitId
  timedExecution : ValidatorBoundedExecution (PacketId := PacketId) config faults
    protocolPacket network program
  executionEffects : ValidatorExactExecutionEffectsSourceMap faults
    protocolPacket network program timedExecution.execution
  genesisParents : ValidatorCanonicalGenesisParentRules timedExecution
  /-- Each active correct host has one finite current operational quorum
  frontier. Parent-first acceptance gives its current causal closure above the
  local GC boundary; this is not an independent availability result. This
  current-state map does not state a future proposal, delivery, layer, or
  commit. -/
  operationalQuorumFrontier : ValidatorOperationalQuorumFrontierSourceMap
    timedExecution genesisParents.parents
  /-- Exact ready normal frontier work has one local disposition. The rule
  models the one-shot forced timeout and the existing last-known-round and
  propagation-delay watcher retries. A stale target can end only because its
  block exists or the threshold frontier moved higher. Existing action,
  persistence, latch, and send rules derive any later broadcast. -/
  normalFrontierPacemaker : ValidatorNormalFrontierPacemakerRules
    timedExecution
  /-- Every qualifying nonempty `add_blocks` input completes all local commit
  work, observes one terminal no-more-commits scan, and then invokes
  `try_propose(false)` before it returns. Packet-driven acceptance has an exact
  past input origin. This is same-handler control-flow refinement only. It does
  not state that the proposal attempt succeeds. -/
  coreHandlerRefinement : ValidatorCoreHandlerRefinementRules
    executionEffects.effects
  installedCommitParents : ValidatorInstalledCommitParentSourceMap
    commitReferenceFunctions timedExecution
  installedHeadBootstrap : ValidatorInstalledHeadBootstrapSourceMap
    commitReferenceFunctions timedExecution
  commitPrefix : ValidatorCommitPrefixSourceMap faults
    timedExecution.execution.trace
  exactCommitPrefix : ExactCommitDurablePrefixSourceMap faults
    timedExecution.execution.trace genesis
  /-- These static validators constrain optional synchronized installs. The
  positive liveness proof does not use commit-sync or commit-vote progress. -/
  validCommitChain : Nat → List (CommonCommitRef CommitId) → Prop
  validCommitBlocks : CommitSyncBundle BlockId CommitId → Prop
  recoveryParentAcceptance :
    ValidatorRecoveryGcParentReadyAcceptanceRules timedExecution
  blockSync : ValidatorBlockSyncExecutionRules timedExecution
  recoverySourcePins : ValidatorRecoverySourcePinExecution blockSync
  recoveryCapsuleSync : ValidatorRecoveryCapsuleSyncExecution blockSync
  proposalObligations : ValidatorProposalObligationExecution timedExecution
  proposalLatch : ValidatorProposalLatchSourceMap proposalObligations
  /-- A concrete ready normal proposal attempt becomes protected local work.
  This is a current one-host action rule, not a future proposal result. -/
  readyNormalProposalProtection :
    ValidatorReadyNormalProposalProtectionRule timedExecution
  acceptedRepresentatives : ValidatorAcceptedRepresentativeRules
    timedExecution.execution
  anchorRules : ValidatorAnchorLocalRules config faults
    timedExecution.execution.trace
  flexCommitterHistory : Type
  flexCommitterContext : Nat → ValidatorLocalState BlockId CommitId →
    ReferenceFlexCommitterContext BlockId flexCommitterHistory
  flexCommitterSource : LocalFlexCommitterSourceMap config
    commitReferenceFunctions flexCommitterContext program
  /-- The leader-window depth and the actual FlexCommitter depth are the same
  static protocol parameter. The source map already proves that the latter is
  constant, so one reference-state equality is sufficient. -/
  flexCommitterDepthMatchesLeaderSchedule :
    (flexCommitterContext 0
      ((timedExecution.execution.trace 0).validatorState 0)).depth =
        leaderSchedule.indirectDepth
  flexCausalView : Type
  exactAnchorCausalData :
    ExactAnchorCausalDataMap flexCausalView BlockId
  localGcCutoffCausal : ValidatorLocalGcCutoffCausalSourceMap
    (PacketId := PacketId) (faults := faults)
    (protocolPacket := protocolPacket) (network := network)
    (timed := timedExecution) flexCommitterSource exactAnchorCausalData
  exactPendingIngestion : ValidatorExactPendingIngestionRules
    flexCommitterSource
  exactDirectRule : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
    (faults := faults) flexCommitterSource
  successfulFlexScanWork : ValidatorSuccessfulFlexScanWorkRules
    (PacketId := PacketId) (faults := faults) (timed := timedExecution)
    flexCommitterSource
  flexCommitterRuntime : LocalFlexCommitterRuntime timedExecution
    flexCommitterSource
  authenticatedFlexVotes : AuthenticatedFlexVoteSourceMap faults
    commitReferenceFunctions flexCommitterContext flexCommitterSource
  commitMaterialCausalClosure :
    ValidatorCommitMaterialCausalClosureSourceMap (PacketId := PacketId)
      (faults := faults) (protocolPacket := protocolPacket)
      (network := network) (timed := timedExecution) flexCommitterSource
  /-- This reverse map keeps optional synchronized installs safe. It is not a
  source of liveness, certification, or delivery. -/
  exactCommitInstallProvenance : ExactCommitInstallProvenance
    flexCommitterRuntime exactCommitPrefix validCommitChain validCommitBlocks
  recoveryMode : ValidatorCommitProgressRecoveryModeRules timedExecution
  recoveryProposalRounds : ValidatorCommitProgressProposalRoundRules
    timedExecution recoveryMode.recoveryWait
  blockProgressRecoveryThresholds : ValidatorBlockProgressRecoveryThresholds
  blockProgressRecoveryMode : ValidatorBlockProgressRecoveryModeExecution
    timedExecution blockProgressRecoveryThresholds
  /-- In active block-progress recovery above GC, an actual persisted ready
  proposal has recovery origin. This is a same-host action refinement, not a
  future proposal or progress result. -/
  blockProgressProposalOrigin : ValidatorBlockProgressProposalOriginRules
    (obligations := proposalObligations) blockProgressRecoveryMode
  blockProgressRecoveryWaitMatches :
    blockProgressRecoveryThresholds.recoveryWait = recoveryMode.recoveryWait
  recoveryWait : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)
  recoveryTimerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
    network program timedExecution recoveryWait
  recoveryProposalPacing :
    ValidatorCommitProgressProposalPacingRules recoveryTimerSource
  /-- An actual recovery proposal runs in the bounded window of the timer job
  that owns it. This is past same-host action provenance, not a future action
  or progress result. -/
  recoveryProposalActionTiming :
    ValidatorRecoveryProposalActionTimingRules recoveryTimerSource
  recoveryTimerArms : ValidatorRecoveryTimerArmExecution recoveryTimerSource
  recoveryParentNeeds : ValidatorRecoveryParentNeedExecution
    recoverySourcePins recoveryTimerArms recoveryMode.recoveryWait
  /-- The already-actual `try_propose(false)` attempt either ran its exact
  proposal action in the same handler batch or left exact current proposal,
  parent-acquisition, or timer work. It does not state future completion. -/
  coreProposalContinuation :
    ValidatorCoreProposalAttemptContinuationRules
      (effects := executionEffects.effects) coreHandlerRefinement
        proposalObligations recoveryParentNeeds
  /-- Proposed one-host behavior. Recovery entry must refresh an active normal
  parent need to the current safe-resume target. The Rust implementation and
  the executable transition model do not yet implement this refresh. -/
  blockProgressRecoveryNeedRules : ValidatorBlockProgressRecoveryNeedRules
    blockProgressRecoveryMode recoveryParentNeeds
  /-- One actual local commit handler preserves the threshold-clock proposal
  round. Existing installed-parent, parent-need, and bounded-execution rules
  preserve the remaining above-GC proposal pipeline. This local refinement
  does not state a future proposal or network result. -/
  commitProposalNonInterference :
    ValidatorCommitProposalNonInterferenceRules (timed := timedExecution)
  /-- The execution trace begins after `recover_validator`. At the first
  same-host commit-install batch after an exact pre-attempt normal callback, an
  armed recovery timer, or an occupied protected timer-arm goal is scheduled,
  the callback has either already persisted its exact proposal or the
  post-install state schedules one legal protected normal callback. Core does
  not interleave a commit after the callback starts. Local records and
  synchronized installs are interference only. The result contains no future
  proposal, block, layer, or commit. -/
  authorLocalCommitContinuation :
    ValidatorAuthorLocalCommitContinuationRules
      (obligations := proposalObligations) recoveryTimerArms
        blockProgressRecoveryMode
  recoveryCapsuleGenesisExact :
    recoverySourcePins.canonicalGenesisParents = genesisParents.parents
  /-- Receiver-driven replay of one current signed tip. Broken or idle streams
  terminate and the correct receiver retries its subscription. A successful
  subscription emits the exact observed tip or a newer own tip. The newer tip
  is already higher frontier evidence. This rule does not state delivery,
  acceptance, a future layer, or a commit. -/
  currentTipSubscription : ValidatorCurrentTipSubscriptionExecution
    recoverySourcePins
  recoveryTipRebroadcast : ValidatorRecoveryTipRebroadcastExecution
    recoverySourcePins recoveryMode.recoveryWait
  initial : ValidatorGenesisInput config faults
    (timedExecution.execution.trace 0) genesis

namespace EndToEndLivenessInputs

/-- The one reference-state equality and the FlexCommitter source-map
stability rule give the configured depth at every local state. -/
theorem flex_committer_depth_matches_leader_schedule
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    (validator : Nat) (state : ValidatorLocalState BlockId CommitId) :
    (inputs.flexCommitterContext validator state).depth =
      inputs.leaderSchedule.indirectDepth := by
  calc
    (inputs.flexCommitterContext validator state).depth =
        (inputs.flexCommitterContext 0
          ((inputs.timedExecution.execution.trace 0).validatorState 0)).depth :=
      inputs.flexCommitterSource.contextDepthStable validator state 0 _
    _ = inputs.leaderSchedule.indirectDepth :=
      inputs.flexCommitterDepthMatchesLeaderSchedule

/-- The ordinary validator path gives unbounded network quorum-round progress.

This theorem does not use FlexCommitter output, commit synchronization, commit
votes, a favorable leader window, or a future layer premise. Commit installs
can change GC and the leader schedule. The local frontier and pacemaker rules
read the resulting current state. -/
theorem network_dag_progress
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network) :
    NetworkDagProgressLiveness config faults network
      inputs.timedExecution.execution.trace := by
  have strict := operational_frontier_pacemaker_gives_strict_progress
    inputs.recoverySourcePins inputs.currentTipSubscription
      inputs.recoveryParentAcceptance.toValidatorParentReadyAcceptanceRules
        inputs.operationalQuorumFrontier inputs.recoveryCapsuleGenesisExact
          inputs.normalFrontierPacemaker inputs.proposalLatch
            inputs.executionEffects.effects inputs.authorityCountAtLeastTwo
  exact operational_frontier_strict_progress_gives_network_dag_progress
    inputs.operationalQuorumFrontier strict

end EndToEndLivenessInputs

/-- The exact result that the future end-to-end theorem must prove. -/
def EndToEndLivenessGoal
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network) :
    Prop :=
  NetworkDagProgressLiveness config faults network
      inputs.timedExecution.execution.trace ∧
    NetworkCommitProgressLiveness config faults network
      inputs.timedExecution.execution.trace ∧
    PointwiseCommitCatchUpLiveness config faults network
      inputs.timedExecution.execution.trace

/-- Upper deterministic composition after pure DAG progress and the internal
exact one-step theorem are proved.

This lemma is not the public end-to-end theorem. `dagProgress` and `step` are
internal theorem results, not fields of `EndToEndLivenessInputs`. The lower
proof must first derive unconditional DAG growth. It must then use that growth
and the favorable receiver path to derive `step`. Finite exact-index induction
derives the two commit results here. -/
theorem network_dag_progress_and_derived_common_commit_step_prove_end_to_end_goal
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    (dagProgress : NetworkDagProgressLiveness config faults network
      inputs.timedExecution.execution.trace)
    (step : DerivedPointwiseCommonCommitStep faults network
      inputs.timedExecution.execution.trace) :
    EndToEndLivenessGoal inputs := by
  refine ⟨dagProgress, ?_, ?_⟩
  · exact derived_common_commit_step_proves_network_commit_progress
      inputs.timedExecution.execution step inputs.genesis
        inputs.initial.genesisIndex inputs.initial.installedAtCorrectValidator
  · exact derived_common_commit_step_proves_pointwise_commit_catch_up
      inputs.timedExecution.execution step inputs.genesis
        inputs.initial.genesisIndex inputs.initial.installedAtCorrectValidator

/-- One deterministic execution in the outer probability family. -/
structure EndToEndExecutionInstance
    (BlockId CommitId PacketId Encoding : Type)
    [DecidableEq BlockId] where
  config : ValidatorEpochConfig CommitId
  faults : FixedFaultInterval config
  protocolPacket :
    AddressedPacket (ValidatorMessage BlockId CommitId) → Prop
  network : AddressedPartialSynchrony config faults protocolPacket
  inputs : EndToEndLivenessInputs (PacketId := PacketId)
    (Encoding := Encoding) config faults protocolPacket network

namespace EndToEndExecutionInstance

/-- The liveness goal for one member of an execution family. -/
def goal
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    (executionInstance : EndToEndExecutionInstance BlockId CommitId PacketId
      Encoding) : Prop :=
  EndToEndLivenessGoal executionInstance.inputs

end EndToEndExecutionInstance

/-- One ordering of the complete validator set. -/
structure ValidatorRanking (authorityCount : Nat) where
  memberAt : Fin authorityCount → Fin authorityCount
  memberAtInjective : Function.Injective memberAt
  memberAtSurjective : Function.Surjective memberAt

/-- The identity order is one complete validator ranking. -/
def identityValidatorRanking (authorityCount : Nat) :
    ValidatorRanking authorityCount where
  memberAt := id
  memberAtInjective := Function.injective_id
  memberAtSurjective := Function.surjective_id

/-- One complete-ranking representation for each round's first-slot sample. -/
abbrev UniformRoundRankingTrace (authorityCount : Nat) :=
  Nat → ValidatorRanking authorityCount

/-- The sampled rankings strictly before one round. -/
def rankingTracePrefix
    {authorityCount : Nat}
    (trace : UniformRoundRankingTrace authorityCount)
    (round : Nat) : List (ValidatorRanking authorityCount) :=
  List.ofFn (fun index : Fin round => trace index.val)

/-- Restrict one complete validator ranking to one leader schedule. -/
def restrictedValidatorRanking
    {authorityCount : Nat}
    (ranking : ValidatorRanking authorityCount)
    (selected : Fin authorityCount → Bool) : List Nat :=
  ((List.ofFn ranking.memberAt).filter selected).map Fin.val

/-- The head of a restricted ranking is the first selected full-set member. -/
theorem restricted_validator_ranking_head
    {authorityCount : Nat}
    (ranking : ValidatorRanking authorityCount)
    (selected : Fin authorityCount → Bool) :
    (restrictedValidatorRanking ranking selected).head? =
      ((List.ofFn ranking.memberAt).find? selected).map Fin.val := by
  unfold restrictedValidatorRanking
  generalize membersEq : List.ofFn ranking.memberAt = members
  induction members with
  | nil => rfl
  | cons member members _inductionHypothesis =>
      cases selectedMember : selected member <;>
        simp [selectedMember]

/-- A leader schedule that can depend only on earlier sampled rankings.

Every schedule contains at least one correct, available validator. The schedule
can change after a commit because `selectedAfter` can inspect the earlier
history. It cannot inspect the ranking of the round that it selects. -/
structure AdaptiveViableLeaderSchedule
    (authorityCount : Nat)
    (correctAvailable : Fin authorityCount → Bool) where
  selectedAfter :
    List (ValidatorRanking authorityCount) → Fin authorityCount → Bool
  hasCorrectAvailable : ∀ history,
    ∃ validator,
      selectedAfter history validator = true ∧
        correctAvailable validator = true

/-- The first scheduled validator in one sampled complete ranking is correct and
available. -/
def adaptiveFirstSlotIsCorrect
    {authorityCount : Nat}
    {correctAvailable : Fin authorityCount → Bool}
    (schedule : AdaptiveViableLeaderSchedule authorityCount correctAvailable)
    (round : Nat)
    (trace : UniformRoundRankingTrace authorityCount) : Prop :=
  match (List.ofFn (trace round).memberAt).find?
      (schedule.selectedAfter (rankingTracePrefix trace round)) with
  | none => False
  | some validator => correctAvailable validator = true

/-- Events on the first-slot sample space represented by complete rankings. -/
abbrev RoundRankingEvent (authorityCount : Nat) :=
  UniformRoundRankingTrace authorityCount → Prop

/-- One event for each round of the represented first-slot sample. -/
abbrev RoundRankingTrial (authorityCount : Nat) :=
  Nat → RoundRankingEvent authorityCount

/-- One consecutive run of trial successes occurs after a fixed start. -/
def HasConsecutiveRankingTrialAfter
    {authorityCount : Nat}
    (trial : RoundRankingTrial authorityCount)
    (length start : Nat) : RoundRankingEvent authorityCount :=
  fun trace => ∃ base,
    start ≤ base ∧
      ∀ offset, offset < length → trial (base + offset) trace

/-- The independent-uniform first-slot law, represented with complete round
rankings.

`conditionalChanceAtLeast` is the probability model's conditional lower-bound
relation. `adaptiveFirstSlotChance` states the uniform-ranking fact: after every
past history, the chance that the first member of any viable current schedule is
correct and available is at least `1 / authorityCount`.

Only the first selected slot is used by the liveness proof. The remaining order
can follow the deterministic Rust shuffle and does not need an independent
uniform interpretation.

The last three fields are standard probability rules. They do not state that a
protocol execution succeeds. -/
structure IndependentUniformRoundRankingLaw (authorityCount : Nat) where
  authorityCountPositive : 0 < authorityCount
  probabilityOne : RoundRankingEvent authorityCount → Prop
  conditionalChanceAtLeast :
    RoundRankingTrial authorityCount → Nat → Nat → Prop
  adaptiveFirstSlotChance : ∀
      (correctAvailable : Fin authorityCount → Bool)
      (schedule :
        AdaptiveViableLeaderSchedule authorityCount correctAvailable),
    conditionalChanceAtLeast
      (fun round trace => adaptiveFirstSlotIsCorrect schedule round trace)
      1 authorityCount
  positiveConditionalRunAfter : ∀
      trial numerator denominator length start,
    0 < numerator →
    numerator ≤ denominator →
    conditionalChanceAtLeast trial numerator denominator →
    probabilityOne
      (HasConsecutiveRankingTrialAfter trial length start)
  probabilityOneMono : ∀ {left right},
    probabilityOne left →
    (∀ trace, left trace → right trace) →
    probabilityOne right
  probabilityOneCountableInter : ∀ events : Nat →
      RoundRankingEvent authorityCount,
    (∀ index, probabilityOne (events index)) →
    probabilityOne (fun trace => ∀ index, events index trace)

/-- The sampled ranking has a favorable window after every start, for every
leader schedule that uses only earlier samples and remains viable. -/
def AdaptiveFavorableWindowsAfterEveryRound
    {authorityCount : Nat}
    {correctAvailable : Fin authorityCount → Bool}
    (schedule : AdaptiveViableLeaderSchedule authorityCount correctAvailable)
    (length : Nat) : RoundRankingEvent authorityCount :=
  fun trace => ∀ start,
    HasConsecutiveRankingTrialAfter
      (fun round sample => adaptiveFirstSlotIsCorrect schedule round sample)
      length start trace

/-- The sampled ranking has favorable windows after each start in one future
suffix. No condition is placed on an observed past round. -/
def AdaptiveFavorableWindowsFrom
    {authorityCount : Nat}
    {correctAvailable : Fin authorityCount → Bool}
    (schedule : AdaptiveViableLeaderSchedule authorityCount correctAvailable)
    (length firstFutureRound : Nat) : RoundRankingEvent authorityCount :=
  fun trace => ∀ start,
    firstFutureRound ≤ start →
      HasConsecutiveRankingTrialAfter
        (fun round sample => adaptiveFirstSlotIsCorrect schedule round sample)
        length start trace

/-- Independent uniform complete rankings give favorable windows after every
start for an adaptive viable schedule. This result uses no favorable trace as an
input. -/
theorem adaptive_viable_schedule_has_favorable_windows_probability_one
    {authorityCount : Nat}
    (law : IndependentUniformRoundRankingLaw authorityCount)
    (correctAvailable : Fin authorityCount → Bool)
    (schedule : AdaptiveViableLeaderSchedule authorityCount correctAvailable)
    (length : Nat) :
    law.probabilityOne
      (AdaptiveFavorableWindowsAfterEveryRound schedule length) := by
  apply law.probabilityOneCountableInter
  intro start
  exact law.positiveConditionalRunAfter _ 1 authorityCount length start (by
    simp) (by
    exact law.authorityCountPositive) (law.adaptiveFirstSlotChance
      correctAvailable schedule)

/-- First-slot samples mapped to deterministic protocol executions.

The validator set and correct-available set stay fixed across samples. Each
commit head has one fixed leader schedule. For each round, only the head of the
selected leader order must match the sampled ranking restricted to that
schedule. The tail order is not constrained by the probability model. -/
structure UniformRankingEndToEndExecutionFamily
    (BlockId CommitId PacketId Encoding : Type)
    [DecidableEq BlockId]
    (authorityCount : Nat) where
  depth : Nat
  depthPositive : 0 < depth
  correctAvailable : Fin authorityCount → Bool
  leaderSchedule : CommitId → Fin authorityCount → Bool
  execution : UniformRoundRankingTrace authorityCount →
    EndToEndExecutionInstance BlockId CommitId PacketId Encoding
  authorityCountMatches : ∀ sampled,
    (execution sampled).config.authorityCount = authorityCount
  indirectDepthMatches : ∀ sampled,
    (execution sampled).inputs.leaderSchedule.indirectDepth = depth
  correctAvailableMatches : ∀ sampled validator,
    (execution sampled).faults.correctAvailable validator.val =
      correctAvailable validator
  leaderScheduleMatches : ∀ sampled commitId validator,
    (execution sampled).config.leaderSchedule commitId validator.val =
      leaderSchedule commitId validator
  firstSelectedLeaderMatchesRanking : ∀ sampled commitId round,
    ((execution sampled).config.selectedLeaderOrder commitId round).head? =
      (restrictedValidatorRanking
        (sampled round) (leaderSchedule commitId)).head?

/-- Static data that does not use a sampled round ranking. -/
structure UniformRankingStaticData
    (BlockId CommitId Encoding : Type) where
  stake : Nat → Nat
  thresholdFault : Nat
  thresholdQuorum : Nat
  thresholdCertificate : Nat
  byzantine : VoterSet
  unavailable : VoterSet
  unavailableStakeBound : Nat
  protocolPacket :
    AddressedPacket (ValidatorMessage BlockId CommitId) → Prop
  program : ValidatorExecutionProgram BlockId CommitId
  commitReferenceFunctions : CommitReferenceFunctions
    CommitId (LeaderBlockRef BlockId) Encoding
  recoveryWait : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)
  recoveryEntryWait : Time
  genesis : ValidatorCommitHead CommitId
  gst : Time
  delta : Nat
  localActionBound : Nat

/-- Extract the sample-independent data from one deterministic execution. -/
def EndToEndExecutionInstance.staticData
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    (executionInstance : EndToEndExecutionInstance
      BlockId CommitId PacketId Encoding) :
    UniformRankingStaticData BlockId CommitId Encoding where
  stake := executionInstance.config.stake
  thresholdFault := executionInstance.config.thresholds.fault
  thresholdQuorum := executionInstance.config.thresholds.quorum
  thresholdCertificate := executionInstance.config.thresholds.certificate
  byzantine := executionInstance.faults.byzantine
  unavailable := executionInstance.faults.unavailable
  unavailableStakeBound := executionInstance.faults.unavailableStakeBound
  protocolPacket := executionInstance.protocolPacket
  program := executionInstance.inputs.program
  commitReferenceFunctions :=
    executionInstance.inputs.commitReferenceFunctions
  recoveryWait := executionInstance.inputs.recoveryWait
  recoveryEntryWait := executionInstance.inputs.recoveryMode.recoveryWait
  genesis := executionInstance.inputs.genesis
  gst := executionInstance.network.gst
  delta := executionInstance.network.delta
  localActionBound := executionInstance.inputs.timedExecution.localActionBound

/-- Fixed non-random data and prefix causality for a ranking-indexed family.

The selected leader order is the only configuration field that can use the
current round ranking. The main state before round `r` and all earlier events
depend only on rankings before `r`. This is a deterministic execution rule. It
does not state that a future action, block, or commit exists. -/
structure UniformRankingExecutionSourceMap
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    (family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount) where
  staticData : UniformRankingStaticData BlockId CommitId Encoding
  staticDataMatches : ∀ sampled,
    (family.execution sampled).staticData = staticData
  stateBeforeRound : Nat → Time
  stateBeforeRoundMonotone : ∀ earlier later,
    earlier ≤ later → stateBeforeRound earlier ≤ stateBeforeRound later
  /-- Every finite execution prefix reads only a finite ranking prefix. This
  does not say that the protocol reaches the returned round or produces a
  block in it. -/
  finiteTimeHasFutureRankingBoundary : ∀ time,
    ∃ firstFutureRound, time ≤ stateBeforeRound firstFutureRound
  tracePrefixCausal : ∀ left right round,
    rankingTracePrefix left round = rankingTracePrefix right round →
    ∀ time,
      time ≤ stateBeforeRound round →
      (family.execution left).inputs.timedExecution.execution.trace time =
        (family.execution right).inputs.timedExecution.execution.trace time
  eventsPrefixCausal : ∀ left right round,
    rankingTracePrefix left round = rankingTracePrefix right round →
    ∀ time,
      time < stateBeforeRound round →
      (family.execution left).inputs.timedExecution.execution.events time =
        (family.execution right).inputs.timedExecution.execution.events time
  flexScanUsesOnlyStartedRounds : ∀ sampled observation,
    (family.execution sampled).inputs.flexCommitterRuntime.returned
        observation →
    ∀ index,
      index < ((family.execution sampled).inputs.flexCommitterSource.snapshot
        observation.validator observation.input).pending.roundCount →
      stateBeforeRound
          (((family.execution sampled).inputs.flexCommitterSource.snapshot
            observation.validator observation.input).pending.rounds index).round ≤
        observation.time

namespace UniformRankingExecutionSourceMap

/-- The local commit head before a round is fixed by the earlier ranking
prefix. -/
theorem commit_head_before_round_is_prefix_causal
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family)
    (left right : UniformRoundRankingTrace authorityCount)
    (round validator : Nat)
    (samePrefix : rankingTracePrefix left round =
      rankingTracePrefix right round) :
    (((family.execution left).inputs.timedExecution.execution.trace
      (source.stateBeforeRound round)).validatorState validator).commitHead =
    (((family.execution right).inputs.timedExecution.execution.trace
      (source.stateBeforeRound round)).validatorState validator).commitHead := by
  exact congrArg
    (fun world => (world.validatorState validator).commitHead)
    (source.tracePrefixCausal left right round samePrefix
      (source.stateBeforeRound round) (Nat.le_refl _))

/-- A FlexCommitter scan before one sample boundary reads only earlier rounds.

This is an information-flow result. It does not state that a scan occurs or
that the protocol reaches a later round. -/
theorem flex_scan_before_boundary_uses_earlier_rounds
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family)
    (sampled : UniformRoundRankingTrace authorityCount)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (returned :
      (family.execution sampled).inputs.flexCommitterRuntime.returned
        observation)
    (index boundaryRound : Nat)
    (indexInRange : index <
      ((family.execution sampled).inputs.flexCommitterSource.snapshot
        observation.validator observation.input).pending.roundCount)
    (beforeBoundary : observation.time < source.stateBeforeRound boundaryRound) :
    (((family.execution sampled).inputs.flexCommitterSource.snapshot
      observation.validator observation.input).pending.rounds index).round <
        boundaryRound := by
  have scanRoundStarted := source.flexScanUsesOnlyStartedRounds sampled
    observation returned index indexInRange
  apply Nat.lt_of_not_ge
  intro boundaryBeforeScanRound
  have boundaryTimeBeforeScanRound := source.stateBeforeRoundMonotone
    boundaryRound
    (((family.execution sampled).inputs.flexCommitterSource.snapshot
      observation.validator observation.input).pending.rounds index).round
    boundaryBeforeScanRound
  have boundaryTimeBeforeObservation := Nat.le_trans
    boundaryTimeBeforeScanRound scanRoundStarted
  exact (Nat.not_lt_of_ge boundaryTimeBeforeObservation) beforeBoundary

end UniformRankingExecutionSourceMap

namespace UniformRankingEndToEndExecutionFamily

/-- The static stake bound gives a correct, available member of every leader
schedule in the first-slot execution family. -/
theorem leader_schedule_has_correct_available
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    (family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount)
    (commitId : CommitId) :
    ∃ validator : Fin authorityCount,
      family.leaderSchedule commitId validator = true ∧
        family.correctAvailable validator = true := by
  let sampled : UniformRoundRankingTrace authorityCount :=
    fun _round => identityValidatorRanking authorityCount
  let executionInstance := family.execution sampled
  have viable :=
    executionInstance.inputs.leaderSchedule.scheduleViable commitId
  have nonProgressBound := executionInstance.faults.non_progress_stake_bounded
  have selectedNonProgressBound :
      weight executionInstance.config.authorityCount
          executionInstance.config.stake
          (VoterSet.inter
            (executionInstance.config.leaderSchedule commitId)
            executionInstance.faults.nonProgress) ≤
        weight executionInstance.config.authorityCount
          executionInstance.config.stake
          executionInstance.faults.nonProgress := by
    exact weight_mono executionInstance.config.stake
      (VoterSet.inter_subset_right executionInstance.config.authorityCount
        (executionInstance.config.leaderSchedule commitId)
        executionInstance.faults.nonProgress)
  have partition := weight_diff_add_inter
    executionInstance.config.authorityCount executionInstance.config.stake
    (executionInstance.config.leaderSchedule commitId)
    executionInstance.faults.nonProgress
  have selectedCorrectAvailableStakePositive :
      0 < weight executionInstance.config.authorityCount
        executionInstance.config.stake
        (VoterSet.diff (executionInstance.config.leaderSchedule commitId)
          executionInstance.faults.nonProgress) := by
    omega
  rcases positive_weight_has_member selectedCorrectAvailableStakePositive with
    ⟨validator, validatorInRange, validatorSelectedAndAvailable,
      _positiveStake⟩
  have selectedAndAvailable :
      executionInstance.config.leaderSchedule commitId validator = true ∧
        executionInstance.faults.nonProgress validator = false := by
    simpa [VoterSet.diff] using validatorSelectedAndAvailable
  have validatorCorrectAvailable :
      executionInstance.faults.correctAvailable validator = true := by
    simpa [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full]
      using selectedAndAvailable.2
  have validatorInFamilyRange : validator < authorityCount := by
    rw [← family.authorityCountMatches sampled]
    exact validatorInRange
  let familyValidator : Fin authorityCount :=
    ⟨validator, validatorInFamilyRange⟩
  refine ⟨familyValidator, ?_, ?_⟩
  · rw [← family.leaderScheduleMatches sampled commitId familyValidator]
    exact selectedAndAvailable.1
  · rw [← family.correctAvailableMatches sampled familyValidator]
    exact validatorCorrectAvailable

/-- The commit head selected after a finite ranking history. The type prevents
this choice from inspecting the current or a future ranking. -/
structure PastDependentCommitHeadChoice
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    (_family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount) where
  headAfter : List (ValidatorRanking authorityCount) → CommitId

namespace PastDependentCommitHeadChoice

/-- Restrict each new complete ranking to the schedule of the commit head that
the earlier ranking history selected. -/
def adaptiveSchedule
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (choice : PastDependentCommitHeadChoice family) :
    AdaptiveViableLeaderSchedule authorityCount family.correctAvailable where
  selectedAfter := fun history =>
    family.leaderSchedule (choice.headAfter history)
  hasCorrectAvailable := fun history =>
    family.leader_schedule_has_correct_available (choice.headAfter history)

end PastDependentCommitHeadChoice

/-- One favorable adaptive first slot is the actual first selected leader slot
for the commit head selected by the earlier sample history. -/
theorem adaptive_first_slot_maps_to_execution
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (choice : PastDependentCommitHeadChoice family)
    (sampled : UniformRoundRankingTrace authorityCount)
    (round : Nat)
    (correctFirst : adaptiveFirstSlotIsCorrect choice.adaptiveSchedule round
      sampled) :
    ∃ leader,
      leader < (family.execution sampled).config.authorityCount ∧
        ((family.execution sampled).config.selectedLeaderOrder
          (choice.headAfter (rankingTracePrefix sampled round)) round).head? =
            some leader ∧
          (family.execution sampled).faults.correctAvailable leader = true := by
  let selected := family.leaderSchedule
    (choice.headAfter (rankingTracePrefix sampled round))
  let members := List.ofFn (sampled round).memberAt
  cases found : members.find? selected with
  | none =>
      simp [adaptiveFirstSlotIsCorrect,
        PastDependentCommitHeadChoice.adaptiveSchedule, selected, members,
        found] at correctFirst
  | some validator =>
      have validatorCorrect : family.correctAvailable validator = true := by
        simpa [adaptiveFirstSlotIsCorrect,
          PastDependentCommitHeadChoice.adaptiveSchedule, selected, members,
          found] using correctFirst
      refine ⟨validator.val, ?_, ?_, ?_⟩
      · rw [family.authorityCountMatches sampled]
        exact validator.isLt
      · rw [family.firstSelectedLeaderMatchesRanking]
        rw [restricted_validator_ranking_head]
        simp [selected, members, found]
      · rw [family.correctAvailableMatches sampled validator]
        exact validatorCorrect

/-- If no next commit changes the selected head, one adaptive favorable suffix
is exactly the head-specific suffix needed by one commit step. Head stability
is an internal case of the progress proof, not a public input. -/
theorem favorable_future_gives_stable_commit_head_path
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (choice : PastDependentCommitHeadChoice family)
    (sampled : UniformRoundRankingTrace authorityCount)
    (commitId : CommitId) (depth firstFutureRound : Nat)
    (favorable : AdaptiveFavorableWindowsFrom choice.adaptiveSchedule
      (depth + 1) firstFutureRound sampled)
    (headStays : ∀ round,
      firstFutureRound ≤ round →
      choice.headAfter (rankingTracePrefix sampled round) = commitId) :
    CommitHeadFirstSlotLeaderPathCoverageAfter
      (family.execution sampled).config (family.execution sampled).faults
      depth commitId firstFutureRound := by
  intro startRound startInFuture
  rcases favorable startRound startInFuture with
    ⟨baseRound, startBeforeBase, favorableAtBase⟩
  refine ⟨baseRound, startBeforeBase, ?_⟩
  intro offset offsetInWindow
  have roundInFuture : firstFutureRound ≤ baseRound + offset :=
    Nat.le_trans startInFuture
      (Nat.le_trans startBeforeBase (Nat.le_add_right baseRound offset))
  rcases adaptive_first_slot_maps_to_execution choice sampled
      (baseRound + offset) (favorableAtBase offset offsetInWindow) with
    ⟨leader, leaderInRange, firstSlot, leaderCorrect⟩
  refine ⟨leader, leaderInRange, ?_, leaderCorrect⟩
  simpa [headStays (baseRound + offset) roundInFuture] using firstSlot

end UniformRankingEndToEndExecutionFamily

end Mysticeti
