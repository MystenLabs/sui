/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorFreshTimerReadyBridge
import Mysticeti.ValidatorOrderedHeadFavorableTiming

namespace Mysticeti

/-!
Actual lower timer edges and ordered-head deadline transport.

The next timer's initial quorum contains one correct block from the prior
round. That block gives a lower edge from its exact timer deadline to the next
timer start. This statement keeps the block producer's actual commit head. It
does not require all correct validators to have the same commit head.

If one selected leader has a commit-head round at least as large as that lower
edge producer, its head-relative wait is no larger. A same-round start spread
then transports the leader's complete deadline across the lower edge.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- One actual next-round timer has a lower edge from one correct prior-round
fresh production and that production's own commit head. -/
def ValidatorFreshTimerStartSuccessorHeadRelativeLowerAt
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (observation round : Nat) : Prop :=
  ∀ {receiver}
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1)),
    ∃ author,
      ∃ previous : ValidatorFreshTimerPacedExactRoundProduction timed
        obligations waits observation author round,
        previous.production.timerStartedAt +
            waits.wait previous.production.commitHead round ≤
          next.production.timerStartedAt

/-- A fresh prior-round family supplies the head-relative lower edge.

Unlike `fresh_family_gives_timer_start_successor_lower`, this result does not
replace each actual producer head with one common head. -/
theorem fresh_family_gives_timer_start_successor_head_relative_lower
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (ownership : ValidatorAuthenticatedAcceptedBodyOwnershipRules
      (timed := timed))
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Nat}
    (previousFamily :
      EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed obligations
        waits observation round) :
    ValidatorFreshTimerStartSuccessorHeadRelativeLowerAt timed obligations
      waits observation round := by
  unfold ValidatorFreshTimerStartSuccessorHeadRelativeLowerAt
  intro receiver next
  rcases originRules.productionHasTimerOrigin next.production with
    ⟨start, started, startValidator, _startHead, startTarget,
      _parentReady, startTime⟩
  have initialReady := timerSource.initialQuorumReadyAtStart start started
  rcases two_quorums_have_correct_available_intersection faults
      faults.correct_available_stake_is_quorum initialReady.1.2.2 with
    ⟨author, authorInRange, authorCorrect, _correctMember,
      parentAuthorMember⟩
  simp only [validatorParentAuthors, List.any_eq_true, beq_iff_eq] at parentAuthorMember
  rcases parentAuthorMember with ⟨parent, parentIncluded, parentAuthor⟩
  let previous := Classical.choice
    (previousFamily author authorInRange authorCorrect)
  have parentRoundStep := (initialReady.1.2.1 parent parentIncluded).1
  have parentRound : parent.round = round := by
    rw [startTarget] at parentRoundStep
    omega
  have initialFloorMonotone :=
    (timed.execution.durableStateMonotone author 0
      previous.production.timerStartedAt authorInRange (Nat.zero_le _)
      ).2.2.2.2.2.2.1
  have previousTimerFloor :
      ((timed.execution.trace previous.production.timerStartedAt).validatorState
        author).highestSignedRound + 1 = round := by
    exact previous.production.timerStartsExactNext.symm
  have aboveInitialFloor :
      ((timed.execution.trace 0).validatorState
        parent.author).highestSignedRound < parent.round := by
    rw [parentAuthor, parentRound]
    omega
  rcases correct_initial_quorum_parent_has_past_persist_origin timerSource
      ownership started parentIncluded (by simpa only [parentAuthor] using
        authorInRange) (by simpa only [parentAuthor] using authorCorrect)
        aboveInitialFloor with
    ⟨persistTime, persistedBlock, _persistedBeforeStart, persisted,
      persistedReference⟩
  have persistedByAuthor : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author
        (.persistProposal persistedBlock) := by
    simpa only [parentAuthor] using persisted
  have sameRound : previous.production.snapshot.block.reference.round =
      persistedBlock.reference.round := by
    rw [previous.production.blockRound, persistedReference, parentRound]
  have sameBlock := persist_proposal_same_round_blocks_are_equal authorInRange
    previous.production.persistenceOccurs persistedByAuthor sameRound
  have exactParent :
      parent = previous.production.snapshot.block.reference := by
    exact (sameBlock.2.trans persistedReference).symm
  have deadlineBeforeNext := correct_initial_parent_gives_next_timer_lower_start
    timerSource ownership previous.production started parentIncluded exactParent
      aboveInitialFloor
  refine ⟨author, previous, ?_⟩
  simpa only [startTime] using Nat.le_of_lt deadlineBeforeNext

/-- On one no-advance suffix, two fresh productions by the same author use the
same author-local commit head. Other correct authors can use different heads. -/
theorem no_advance_fresh_productions_same_author_head
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation author round : Nat}
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation)
    (left right : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round) :
    left.production.commitHead = right.production.commitHead := by
  have authorInRange : author < config.authorityCount := by
    simpa [left.production.proposer] using
      left.production.snapshot.proposerInRange
  have authorCorrect : faults.correctAvailable author = true := by
    simpa [left.production.proposer] using
      left.production.snapshot.proposerCorrectAvailable
  have leftHeadAtObservation :
      ((timed.execution.trace left.production.timerStartedAt).validatorState
        author).commitHead =
      ((timed.execution.trace observation).validatorState author).commitHead :=
    no_commit_advance_keeps_correct_commit_head authorInRange authorCorrect
      (Nat.le_of_lt left.timerAfterObservation) noAdvance
  have rightHeadAtObservation :
      ((timed.execution.trace right.production.timerStartedAt).validatorState
        author).commitHead =
      ((timed.execution.trace observation).validatorState author).commitHead :=
    no_commit_advance_keeps_correct_commit_head authorInRange authorCorrect
      (Nat.le_of_lt right.timerAfterObservation) noAdvance
  exact (originRules.production_head_at_timer_start left.production).trans
    (leftHeadAtObservation.trans
      (rightHeadAtObservation.symm.trans
        (originRules.production_head_at_timer_start right.production).symm))

/-- One heterogeneous-head fresh family still has a finite pairwise timer-start
spread when each author's actual timer key is stable. -/
theorem fresh_family_has_finite_timer_start_spread_of_same_author_heads
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Nat}
    (family : EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed
      obligations waits observation round)
    (sameAuthorHead : ∀ {author}
      (left right : ValidatorFreshTimerPacedExactRoundProduction timed
        obligations waits observation author round),
      left.production.commitHead = right.production.commitHead) :
    ∃ spread,
      ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
        round spread := by
  classical
  let startFor := fun author =>
    if authorInRange : author < config.authorityCount then
      if authorCorrect : faults.correctAvailable author = true then
        (Classical.choice
          (family author authorInRange authorCorrect)).production.timerStartedAt
      else
        0
    else
      0
  let spread := validatorTimerStartMaximumUpTo 0 startFor
    config.authorityCount
  refine ⟨spread, ?_⟩
  intro left right leftProduction rightProduction
  have leftInRange : left < config.authorityCount := by
    simpa [leftProduction.production.proposer] using
      leftProduction.production.snapshot.proposerInRange
  have leftCorrect : faults.correctAvailable left = true := by
    simpa [leftProduction.production.proposer] using
      leftProduction.production.snapshot.proposerCorrectAvailable
  let selected := Classical.choice (family left leftInRange leftCorrect)
  have sameStart : leftProduction.production.timerStartedAt =
      selected.production.timerStartedAt := by
    exact same_validator_round_and_head_fresh_timer_starts_equal originRules
      leftProduction selected rfl (sameAuthorHead leftProduction selected).symm
  have selectedBound : selected.production.timerStartedAt ≤ spread := by
    have bounded := validator_timer_start_le_maximum_up_to 0 startFor leftInRange
    simpa [spread, startFor, leftInRange, leftCorrect, selected] using bounded
  rw [sameStart]
  exact Nat.le_trans selectedBound (Nat.le_add_left spread _)

/-- A no-advance suffix supplies a finite timer-start spread for one actual
fresh family, without common-head equality. -/
theorem fresh_family_has_finite_timer_start_spread_on_no_advance_suffix
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Nat}
    (family : EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed
      obligations waits observation round)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ spread,
      ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
        round spread := by
  exact fresh_family_has_finite_timer_start_spread_of_same_author_heads
    originRules family (fun left right =>
      no_advance_fresh_productions_same_author_head originRules noAdvance left
        right)

/-- Pure arithmetic for one ordered-head deadline transport. -/
theorem ordered_wait_transports_deadline_across_start_spread
    {leaderStart lowerStart nextStart leaderWait lowerWait spread : Nat}
    (startSpread : leaderStart ≤ lowerStart + spread)
    (waitOrder : leaderWait ≤ lowerWait)
    (lowerDeadline : lowerStart + lowerWait ≤ nextStart) :
    leaderStart + leaderWait ≤ nextStart + spread := by
  omega

/-- The exact minimal ordered-head condition for one concrete lower-edge
production. The leader head only needs to be no earlier than this one producer
head. -/
theorem ordered_head_fresh_spread_and_one_lower_give_deadline_bound
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    {observation leaderAuthor lowerAuthor receiver round spread : Nat}
    (previousSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations
      parameters.schedule.commonSchedule observation round spread)
    (leader : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      parameters.schedule.commonSchedule observation leaderAuthor round)
    (lower : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      parameters.schedule.commonSchedule observation lowerAuthor round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      parameters.schedule.commonSchedule observation receiver (round + 1))
    (lowerDeadline : lower.production.timerStartedAt +
        parameters.schedule.commonSchedule.wait lower.production.commitHead
          round ≤
      next.production.timerStartedAt)
    (headOrder : lower.production.commitHead.round ≤
      leader.production.commitHead.round) :
    leader.production.timerStartedAt +
          parameters.schedule.commonSchedule.wait
            leader.production.commitHead round ≤
      next.production.timerStartedAt + spread := by
  have startSpread := previousSpread leader lower
  have waitOrder :
      parameters.schedule.commonSchedule.wait leader.production.commitHead
          round ≤
        parameters.schedule.commonSchedule.wait lower.production.commitHead
          round := by
    change parameters.wait leader.production.commitHead round ≤
      parameters.wait lower.production.commitHead round
    exact parameters.wait_anti_mono_head_round headOrder
  exact ordered_wait_transports_deadline_across_start_spread startSpread
    waitOrder lowerDeadline

/-- A head-maximal leader transports its full prior-round deadline across the
actual lower edge of one next-round timer.

It is sufficient that the leader head is no earlier than every production in
the fresh prior-round family. A caller can use a weaker condition which covers
only the lower-edge production if that production is already known. -/
theorem ordered_head_fresh_spread_and_successor_lower_give_deadline_bound
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    {observation author receiver round spread : Nat}
    (previousSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations
      parameters.schedule.commonSchedule observation round spread)
    (lower : ValidatorFreshTimerStartSuccessorHeadRelativeLowerAt timed
      obligations parameters.schedule.commonSchedule observation round)
    (leader : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      parameters.schedule.commonSchedule observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      parameters.schedule.commonSchedule observation receiver (round + 1))
    (leaderHeadMaximal : ∀ {otherAuthor}
      (other : ValidatorFreshTimerPacedExactRoundProduction timed obligations
        parameters.schedule.commonSchedule observation otherAuthor round),
      other.production.commitHead.round ≤
        leader.production.commitHead.round) :
    leader.production.timerStartedAt +
          parameters.schedule.commonSchedule.wait
            leader.production.commitHead round ≤
      next.production.timerStartedAt + spread := by
  unfold ValidatorFreshTimerStartSuccessorHeadRelativeLowerAt at lower
  rcases lower next with ⟨lowerAuthor, lowerProduction, lowerDeadline⟩
  have headOrder := leaderHeadMaximal lowerProduction
  exact ordered_head_fresh_spread_and_one_lower_give_deadline_bound parameters
    previousSpread leader lowerProduction next lowerDeadline headOrder

end Mysticeti
