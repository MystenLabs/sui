/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ExactCommitPrefixSafety

namespace Mysticeti

/-! Exact-prefix safety for the performance-based v3 leader schedule.

The Rust schedule scorer replays committed sub-DAG performance in commit order.
This module models that replay as a deterministic relation. It proves that the
existing exact commit-prefix theorem also fixes the replayed schedule state.

The product refinement must bind `performanceFor` to the exact committed block
bodies and ancestors used by `LeaderScheduleV3::add_commit`. It must also bind
`allowedLeaders` and `selectedOrder` to the ordered Rust vectors. These are
current or past facts. They do not assert that a future favorable leader occurs.
-/

/-- Pure deterministic rules for replaying the v3 leader-schedule state.
`Performance` contains the exact committed material read by one scorer update. -/
structure ValidatorV3LeaderScheduleReplay
    (CommitId Performance ReplayState : Type) where
  initialState : ReplayState
  performanceFor : ValidatorCommitHead CommitId -> Performance
  addCommit : ReplayState -> Performance -> ReplayState
  /-- The order is significant. Rust uses this vector as the Flex cache key. -/
  allowedLeaders : ReplayState -> List Nat
  /-- The exact selected-slot order after the per-round permutation. -/
  selectedOrder : ReplayState -> Nat -> List Nat

namespace ValidatorV3LeaderScheduleReplay

variable {CommitId Performance ReplayState : Type}

/-- One replay state reached along an exact commit path. This relation stays in
`Prop`; it does not extract executable data from a proof. -/
inductive ReplayedState
    (replay : ValidatorV3LeaderScheduleReplay
      CommitId Performance ReplayState)
    (successor : ValidatorCommitHead CommitId ->
      ValidatorCommitHead CommitId -> Prop)
    (genesis : ValidatorCommitHead CommitId) :
    Nat -> ValidatorCommitHead CommitId -> ReplayState -> Prop where
  | genesis : ReplayedState replay successor genesis 0 genesis
      replay.initialState
  | next {length : Nat} {prior head : ValidatorCommitHead CommitId}
      {state : ReplayState} :
      ReplayedState replay successor genesis length prior state ->
      successor prior head ->
      ReplayedState replay successor genesis (length + 1) head
        (replay.addCommit state (replay.performanceFor head))

namespace ReplayedState

/-- Every exact commit path has a corresponding replay state. -/
theorem exists_of_exact_path
    (replay : ValidatorV3LeaderScheduleReplay
      CommitId Performance ReplayState)
    {successor : ValidatorCommitHead CommitId ->
      ValidatorCommitHead CommitId -> Prop}
    {genesis : ValidatorCommitHead CommitId}
    {length : Nat} {head : ValidatorCommitHead CommitId}
    (path : ExactCommitPath successor genesis length head) :
    exists state, ReplayedState replay successor genesis length head state := by
  induction path with
  | genesis => exact ⟨replay.initialState, .genesis⟩
  | @next length prior head priorPath step inductionHypothesis =>
      rcases inductionHypothesis with ⟨state, replayed⟩
      exact ⟨replay.addCommit state (replay.performanceFor head),
        .next replayed step⟩

/-- Every replay witness follows an exact commit path. -/
theorem exact_path
    {replay : ValidatorV3LeaderScheduleReplay
      CommitId Performance ReplayState}
    {successor : ValidatorCommitHead CommitId ->
      ValidatorCommitHead CommitId -> Prop}
    {genesis : ValidatorCommitHead CommitId}
    {length : Nat} {head : ValidatorCommitHead CommitId}
    {state : ReplayState}
    (replayed : ReplayedState replay successor genesis length head state) :
    ExactCommitPath successor genesis length head := by
  induction replayed with
  | genesis => exact .genesis
  | next _ step inductionHypothesis => exact .next inductionHypothesis step

/-- A functional exact commit successor gives one replay state at each index. -/
theorem unique
    {replay : ValidatorV3LeaderScheduleReplay
      CommitId Performance ReplayState}
    {successor : ValidatorCommitHead CommitId ->
      ValidatorCommitHead CommitId -> Prop}
    {genesis : ValidatorCommitHead CommitId}
    (successorUnique : forall prior left right,
      successor prior left -> successor prior right -> left = right)
    {length : Nat} {head : ValidatorCommitHead CommitId}
    {leftState rightState : ReplayState}
    (left : ReplayedState replay successor genesis length head leftState)
    (right : ReplayedState replay successor genesis length head rightState) :
    leftState = rightState := by
  induction left generalizing rightState with
  | genesis =>
      cases right
      rfl
  | next priorReplayed step inductionHypothesis =>
      cases right with
      | next rightPriorReplayed rightStep =>
          have samePrior := exact_commit_path_unique successorUnique
            (ValidatorV3LeaderScheduleReplay.ReplayedState.exact_path
              priorReplayed)
            (ValidatorV3LeaderScheduleReplay.ReplayedState.exact_path
              rightPriorReplayed)
          subst_vars
          have sameState := inductionHypothesis rightPriorReplayed
          rw [sameState]

end ReplayedState

end ValidatorV3LeaderScheduleReplay

namespace ExactCommitInstallProvenance

variable {BlockId CommitId History Encoding PacketId : Type}
variable {Performance ReplayState : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) -> Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}
variable {genesis : ValidatorCommitHead CommitId}
variable {durable : ExactCommitDurablePrefixSourceMap faults
  timed.execution.trace genesis}
variable {validChain : Nat -> List (CommonCommitRef CommitId) -> Prop}
variable {validBlocks : CommitSyncBundle BlockId CommitId -> Prop}

/-- Correct installed heads at the same index share one replayed v3 schedule
state. This is the joint prefix-and-schedule safety step. -/
theorem exactInstalledHeadsAtSameIndexShareV3ReplayState
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (replay : ValidatorV3LeaderScheduleReplay
      CommitId Performance ReplayState)
    {leftTime leftValidator rightTime rightValidator : Nat}
    {left right : ValidatorCommitHead CommitId}
    (leftValidatorInRange : leftValidator < config.authorityCount)
    (leftValidatorCorrect : faults.correctAvailable leftValidator = true)
    (rightValidatorInRange : rightValidator < config.authorityCount)
    (rightValidatorCorrect : faults.correctAvailable rightValidator = true)
    (leftInstalled :
      durable.exactInstalledHead leftTime leftValidator left)
    (rightInstalled :
      durable.exactInstalledHead rightTime rightValidator right)
    (sameIndex : left.index = right.index) :
    exists state,
      replay.ReplayedState (ExactFlexSuccessor runtime) genesis
          left.index left state /\
        replay.ReplayedState (ExactFlexSuccessor runtime) genesis
          right.index right state := by
  have sameHead := provenance.exactInstalledHeadsAtSameIndexAgree authenticated
    leftValidatorInRange leftValidatorCorrect rightValidatorInRange
    rightValidatorCorrect leftInstalled rightInstalled sameIndex
  rcases ValidatorV3LeaderScheduleReplay.ReplayedState.exists_of_exact_path
      replay
      (provenance.exactInstalledHeadHasPath leftValidatorInRange
        leftValidatorCorrect leftInstalled) with
    ⟨state, replayed⟩
  subst right
  exact ⟨state, replayed, replayed⟩

/-- The shared replay state gives one ordered allowed-leader vector and one
selected order for every round. -/
theorem exactInstalledHeadsAtSameIndexShareV3Schedule
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (replay : ValidatorV3LeaderScheduleReplay
      CommitId Performance ReplayState)
    {leftTime leftValidator rightTime rightValidator : Nat}
    {left right : ValidatorCommitHead CommitId}
    (leftValidatorInRange : leftValidator < config.authorityCount)
    (leftValidatorCorrect : faults.correctAvailable leftValidator = true)
    (rightValidatorInRange : rightValidator < config.authorityCount)
    (rightValidatorCorrect : faults.correctAvailable rightValidator = true)
    (leftInstalled :
      durable.exactInstalledHead leftTime leftValidator left)
    (rightInstalled :
      durable.exactInstalledHead rightTime rightValidator right)
    (sameIndex : left.index = right.index) :
    exists state,
      replay.ReplayedState (ExactFlexSuccessor runtime) genesis
          left.index left state /\
        replay.ReplayedState (ExactFlexSuccessor runtime) genesis
          right.index right state /\
        (forall other,
          replay.ReplayedState (ExactFlexSuccessor runtime) genesis
              left.index left other ->
            replay.allowedLeaders other = replay.allowedLeaders state /\
              forall round,
                replay.selectedOrder other round =
                  replay.selectedOrder state round) := by
  rcases provenance.exactInstalledHeadsAtSameIndexShareV3ReplayState
      authenticated replay leftValidatorInRange leftValidatorCorrect
      rightValidatorInRange rightValidatorCorrect leftInstalled rightInstalled
      sameIndex with ⟨state, leftReplayed, rightReplayed⟩
  refine ⟨state, leftReplayed, rightReplayed, ?_⟩
  intro other otherReplayed
  have successorUnique : forall prior first second,
      ExactFlexSuccessor runtime prior first ->
      ExactFlexSuccessor runtime prior second -> first = second := by
    intro prior first second firstStep secondStep
    exact ExactFlexSuccessor.unique authenticated firstStep secondStep
  have sameState :=
    ValidatorV3LeaderScheduleReplay.ReplayedState.unique successorUnique
      otherReplayed leftReplayed
  subst other
  exact ⟨rfl, fun _round => rfl⟩

end ExactCommitInstallProvenance

end Mysticeti
