/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommonCommitInduction
import Mysticeti.ExactCommitPrefixSafety
import Mysticeti.ValidatorFlexPendingRefresh
import Mysticeti.ValidatorHeadRelativeGapWait

namespace Mysticeti

/-!
Ordered-head timing facts for the pointwise ahead branch.

A lagging receiver can keep the exact prior head while another correct
validator has a later head in the same exact commit prefix. Each exact Flex
successor has a strictly later leader round. Thus, the later head has a wait
that is no larger than the receiver's wait at one fixed absolute round.

These results use only current or past durable prefix facts. They do not select
a future leader, timer, proposal, favorable window, or commit result.
-/

/-! ### The current-state schedule condition -/

/-- One schedule is viable at one current state when it contains a correct,
available validator. This is the pointwise content of adaptive schedule
viability. -/
def ViableLeaderScheduleAt
    {authorityCount : Nat}
    (correctAvailable selected : VoterSet) : Prop :=
  ∃ leader : Fin authorityCount,
    selected leader.val = true ∧ correctAvailable leader.val = true

/-- The selected favorable leader has at least quorum stake of correct,
available validators whose current head round is not later than its head
round.

This is the minimum head-order condition used by the ordered-wait argument for
the leader-to-voter edge. All values are from one current state. -/
def LeaderScheduleHasCorrectHeadDominatingQuorum
    {authorityCount : Nat}
    (quorum : Nat)
    (stake : Nat → Nat)
    (correctAvailable selected : VoterSet)
    (headRound : Nat → Nat) : Prop :=
  ∃ leader : Fin authorityCount,
    selected leader.val = true ∧
      correctAvailable leader.val = true ∧
      quorum ≤ weight authorityCount stake (fun voter =>
        correctAvailable voter && decide (headRound voter ≤ headRound leader.val))

/-- A head-dominating-quorum schedule is viable. The reverse implication is
false, as the next theorem shows. -/
theorem head_dominating_quorum_schedule_is_viable
    {authorityCount quorum : Nat}
    {stake : Nat → Nat}
    {correctAvailable selected : VoterSet}
    {headRound : Nat → Nat}
    (dominates : LeaderScheduleHasCorrectHeadDominatingQuorum
      (authorityCount := authorityCount) quorum stake correctAvailable selected
        headRound) :
    ViableLeaderScheduleAt (authorityCount := authorityCount)
      correctAvailable selected := by
  rcases dominates with ⟨leader, selectedLeader, leaderCorrect, _quorum⟩
  exact ⟨leader, selectedLeader, leaderCorrect⟩

/-- Ordinary schedule viability does not imply a leader that dominates a
correct quorum of voter heads. In this two-validator state, the schedule
contains only the validator at head round zero. Both validators are correct,
but the other validator is at head round one. -/
theorem viable_schedule_does_not_imply_head_dominating_quorum :
    ∃ (stake : Nat → Nat)
      (correctAvailable selected : VoterSet)
      (headRound : Nat → Nat),
      ViableLeaderScheduleAt (authorityCount := 2) correctAvailable selected ∧
        ¬ LeaderScheduleHasCorrectHeadDominatingQuorum
          (authorityCount := 2) 2 stake
          correctAvailable selected headRound := by
  refine ⟨fun _ => 1, fun _ => true, fun validator => validator == 0,
    fun validator => validator, ?_⟩
  constructor
  · exact ⟨⟨0, by omega⟩, by decide, rfl⟩
  · rintro ⟨leader, selectedLeader, _leaderCorrect, quorumStake⟩
    have leaderZero : leader.val = 0 := by
      simpa using selectedLeader
    simp [weight, leaderZero] at quorumStake

/-! ### Exact-prefix round order -/

/-- A successful exact Flex successor has a strictly later leader round. -/
theorem exact_flex_successor_round_strict
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {prior next : ValidatorCommitHead CommitId}
    (successor : ExactFlexSuccessor runtime prior next) :
    prior.round < next.round := by
  rcases successor with ⟨run, runPrior, runOutput⟩
  have returned := mapping.returned_observation_uses_prepared_scan run.returned
  have preparedSuccess :
      (validatorFlexRunStateAt source schedule mapping.cacheAt
        mapping.highestAcceptedRound run.observation).output = some run.output := by
    rw [← returned]
    exact run.successful
  have candidate := mapping.actual_successful_candidate_is_inside_prepared_frontier
    run.occurs preparedSuccess
  rw [run.priorAtInput, schedule.minimumIsRoundSuccessor run.prior] at candidate
  have strict : run.prior.round < run.output.toCommitHead.round := by
    change run.prior.round < run.output.candidate.leaderRound
    omega
  calc
    prior.round = run.prior.round := congrArg ValidatorCommitHead.round runPrior.symm
    _ < run.output.toCommitHead.round := strict
    _ = next.round := congrArg ValidatorCommitHead.round runOutput

/-- Along one exact commit path, later indexes have no earlier leader round. -/
theorem exact_commit_path_round_monotone
    {CommitId : Type}
    {successor : ValidatorCommitHead CommitId →
      ValidatorCommitHead CommitId → Prop}
    {genesis : ValidatorCommitHead CommitId}
    (successorUnique : ∀ prior left right,
      successor prior left → successor prior right → left = right)
    (successorRoundStrict : ∀ {prior next},
      successor prior next → prior.round < next.round)
    {leftIndex rightIndex : Nat}
    {left right : ValidatorCommitHead CommitId}
    (indexOrder : leftIndex ≤ rightIndex)
    (leftPath : ExactCommitPath successor genesis leftIndex left)
    (rightPath : ExactCommitPath successor genesis rightIndex right) :
    left.round ≤ right.round := by
  induction rightPath generalizing leftIndex left with
  | genesis =>
      have leftIndexZero : leftIndex = 0 := by omega
      subst leftIndex
      cases leftPath
      exact Nat.le_refl _
  | @next pathIndex rightPrior rightReference rightPriorPath rightStep
      inductionHypothesis =>
      by_cases sameIndex : leftIndex = pathIndex + 1
      · subst leftIndex
        have sameReference := exact_commit_path_unique successorUnique leftPath
          (ExactCommitPath.next rightPriorPath rightStep)
        subst left
        exact Nat.le_refl _
      · have leftBeforePrior : leftIndex ≤ pathIndex := by omega
        exact Nat.le_trans
          (inductionHypothesis leftBeforePrior leftPath)
          (Nat.le_of_lt (successorRoundStrict rightStep))

/-- A provenance-backed installed prefix entry has a round that is no later
than the current complete head at the same correct validator. -/
theorem exact_installed_prefix_round_le_current_head
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    {priorTime priorValidator targetTime targetValidator : Nat}
    {prior : ValidatorCommitHead CommitId}
    (priorValidatorInRange : priorValidator < config.authorityCount)
    (priorValidatorCorrect :
      faults.correctAvailable priorValidator = true)
    (priorExact : durable.exactInstalledHead priorTime priorValidator prior)
    (targetValidatorInRange : targetValidator < config.authorityCount)
    (targetValidatorCorrect :
      faults.correctAvailable targetValidator = true)
    (priorIndexAtOrBelowHead : prior.index ≤
      (timed.execution.trace targetTime).localCommitIndex targetValidator) :
    prior.round ≤
      ((timed.execution.trace targetTime).validatorState
        targetValidator).commitHead.round := by
  let targetHead :=
    ((timed.execution.trace targetTime).validatorState targetValidator).commitHead
  have targetHeadStored :
      ((timed.execution.trace targetTime).validatorState targetValidator
        ).installedCommitAt targetHead.index = some targetHead.id :=
    (timed.execution.statesWellFormed targetTime targetValidator
      targetValidatorInRange).commitHeadIsInstalled
  rcases durable.storedIdHasExactHead targetTime targetValidator
      targetHead.index targetHead.id targetValidatorInRange
        targetValidatorCorrect targetHeadStored with
    ⟨exactTarget, exactTargetAtTime, exactTargetIndex, exactTargetId⟩
  have targetHeadShape : exactTarget = targetHead := by
    have localHeadShape := prefixMap.sameIndexInstalledHeadIsExact targetTime
      targetValidator exactTarget targetValidatorInRange targetValidatorCorrect
        (durable.exactHeadHasStoredId targetTime targetValidator exactTarget
          exactTargetAtTime) (by
            change targetHead.index = exactTarget.index
            exact exactTargetIndex.symm)
    exact localHeadShape.symm
  have targetExact : durable.exactInstalledHead targetTime targetValidator
      targetHead := by
    simpa only [targetHeadShape] using exactTargetAtTime
  have priorPath := provenance.exactInstalledHeadHasPath
    priorValidatorInRange priorValidatorCorrect priorExact
  have targetPath := provenance.exactInstalledHeadHasPath
    targetValidatorInRange targetValidatorCorrect targetExact
  have indexOrder : prior.index ≤ targetHead.index := by
    simpa [targetHead, ValidatorWorldState.localCommitIndex] using
      priorIndexAtOrBelowHead
  have successorUnique : ∀ prior left right,
      ExactFlexSuccessor runtime prior left →
        ExactFlexSuccessor runtime prior right → left = right := by
    intro pathPrior left right leftStep rightStep
    exact ExactFlexSuccessor.unique authenticated leftStep rightStep
  have successorRoundStrict : ∀ {prior next},
      ExactFlexSuccessor runtime prior next → prior.round < next.round := by
    intro pathPrior next step
    exact exact_flex_successor_round_strict mapping step
  exact exact_commit_path_round_monotone successorUnique successorRoundStrict
    indexOrder priorPath targetPath

/-- If all correct validators have installed one exact prefix entry, each
current correct head has a leader round at least as large as that entry. -/
theorem all_installed_exact_prefix_round_le_correct_current_head
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    {priorTime priorValidator start : Nat}
    {prior : ValidatorCommitHead CommitId}
    (priorValidatorInRange : priorValidator < config.authorityCount)
    (priorValidatorCorrect :
      faults.correctAvailable priorValidator = true)
    (priorExact : durable.exactInstalledHead priorTime priorValidator prior)
    (installed : AllCorrectAvailableInstalledExactAt faults
      timed.execution.trace start prior)
    {validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true) :
    prior.round ≤
      ((timed.execution.trace start).validatorState validator).commitHead.round := by
  exact exact_installed_prefix_round_le_current_head mapping durable
    authenticated provenance prefixMap priorValidatorInRange
      priorValidatorCorrect priorExact validatorInRange validatorCorrect
        (installed validator validatorInRange validatorCorrect).2

/-! ### Head-relative wait order -/

/-- The concrete quadratic gap wait is monotone in its gap. -/
theorem quadratic_gap_wait_from_gap_monotone
    (baseWait linearCoefficient quadraticCoefficient : Nat)
    {leftGap rightGap : Nat}
    (gapOrder : leftGap ≤ rightGap) :
    quadraticGapWaitFromGap baseWait linearCoefficient quadraticCoefficient
        leftGap ≤
      quadraticGapWaitFromGap baseWait linearCoefficient quadraticCoefficient
        rightGap := by
  obtain ⟨difference, rightShape⟩ := Nat.exists_eq_add_of_le gapOrder
  subst rightGap
  induction difference with
  | zero => simp
  | succ previous inductionHypothesis =>
      have prefixBound := inductionHypothesis
        (Nat.le_add_right leftGap previous)
      have step :
          quadraticGapWaitFromGap baseWait linearCoefficient
              quadraticCoefficient (leftGap + previous) ≤
            quadraticGapWaitFromGap baseWait linearCoefficient
              quadraticCoefficient ((leftGap + previous) + 1) := by
        rw [quadraticGapWaitFromGap_succ]
        omega
      exact Nat.le_trans prefixBound (by
        simpa [Nat.add_assoc] using step)

/-- At one absolute round, a head with a later commit round waits no longer. -/
theorem ValidatorHeadRelativeQuadraticWaitParameters.wait_anti_mono_head_round
    {CommitId : Type}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    {earlierHead laterHead : ValidatorCommitHead CommitId}
    {round : Nat}
    (headOrder : earlierHead.round ≤ laterHead.round) :
    parameters.wait laterHead round ≤ parameters.wait earlierHead round := by
  apply quadratic_gap_wait_from_gap_monotone
  omega

/-- A same-head adjacent margin for the receiver also covers a leader whose
installed prefix is at least as advanced as the receiver's prefix. -/
theorem ValidatorHeadRelativeQuadraticWaitParameters.ordered_head_margin
    {CommitId : Type}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    {receiverHead leaderHead : ValidatorCommitHead CommitId}
    {round cost : Nat}
    (headOrder : receiverHead.round ≤ leaderHead.round)
    (receiverMargin :
      parameters.wait receiverHead round + cost ≤
        parameters.wait receiverHead (round + 1)) :
    parameters.wait leaderHead round + cost ≤
      parameters.wait receiverHead (round + 1) := by
  exact Nat.le_trans
    (Nat.add_le_add_right
      (parameters.wait_anti_mono_head_round headOrder) cost)
    receiverMargin

end Mysticeti
