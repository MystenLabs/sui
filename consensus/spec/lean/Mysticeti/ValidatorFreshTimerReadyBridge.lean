/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorTimerSpreadRecurrence

namespace Mysticeti

/-! Current-state parent readiness for an actual fresh recovery timer.

The receiver-side capsule theorems already produce
`ValidatorReceiverUsableCorrectQuorumLayer`. Its `nextParentsReady` field is the
exact current accepted and retained parent quorum which can arm the next timer.

This module first derives the timer selected by that current state. It then
isolates one erased past-action rule: a timer-paced production maps back to its
exact timer start, and a validator has at most one timer start for one exact
`(validator, commit head, target round)` key. This rule does not assert that a
future timer, proposal, layer, or commit exists.
-/

/-- One actual timer start derived from a current parent-ready state.

The selected start can be in the current or bounded next local batch. Its
parent-ready observation is no later than `readyAt`. -/
structure ValidatorBoundedCurrentRecoveryTimerStart
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (readyAt validator : Time) : Type where
  start : ValidatorRecoveryTimerStart BlockId CommitId
  timerStarted : source.timerStarted start
  startValidator : start.validator = validator
  startCommitHead : start.commitHead =
    ((timed.execution.trace readyAt).validatorState validator).commitHead
  startTargetRound : start.targetRound =
    ((timed.execution.trace readyAt).validatorState
      validator).highestSignedRound + 1
  parentReadyByObservation : start.parentReadyAt ≤ readyAt
  startWithinLocalBound :
    start.startedAt ≤ readyAt + timed.localActionBound + 2

/-- Current timer input selects one exact bounded start or loses to a strict
commit-index advance at the same validator.

This theorem handles an empty worker, an occupied worker, and an already stored
exact timer. The caller supplies no timer-start observation. -/
theorem current_timer_input_gives_bounded_start_or_receiver_commit_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    {readyAt validator : Time}
    (input : ValidatorRecoveryTimerCurrentInputAt timed readyAt validator) :
    Nonempty (ValidatorBoundedCurrentRecoveryTimerStart source readyAt
      validator) ∨
      ValidatorReceiverCommitAdvance timed readyAt validator := by
  rcases input with armInput |
      ⟨_active, validatorInRange, validatorCorrectAvailable, armed⟩
  · cases pendingState : (arms.trace readyAt validator).pending with
    | none =>
        rcases arms.readyStateSelectsGoal readyAt validator armInput pendingState
          with ⟨goal, selected, goalValidator, goalHead, goalTarget,
            goalReadyAt, _eligible⟩
        have selectedAtGoal :
            arms.selectedGoal readyAt goal.validator = some goal := by
          simpa only [goalValidator] using selected
        rcases source.selected_goal_completes_timer_or_commit_race arms readyAt
            goal selectedAtGoal with
          ⟨completedAt, afterLatch, completionBound, _completed,
            timerStarted | commitAdvanced⟩
        · left
          let start := goal.toTimerStart (completedAt + 1)
          refine ⟨{
            start
            timerStarted := timerStarted
            startValidator := goalValidator
            startCommitHead := ?_
            startTargetRound := ?_
            parentReadyByObservation := ?_
            startWithinLocalBound := ?_ }⟩
          · simpa [start, ValidatorRecoveryTimerArmGoal.toTimerStart,
              goalValidator] using goalHead
          · simpa [start, ValidatorRecoveryTimerArmGoal.toTimerStart,
              goalValidator] using goalTarget
          · simp [start, ValidatorRecoveryTimerArmGoal.toTimerStart,
              goalReadyAt]
          · dsimp [start, ValidatorRecoveryTimerArmGoal.toTimerStart]
            have visible := Nat.add_le_add_right completionBound 1
            simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using visible
        · right
          refine ⟨completedAt + 1, ?_, ?_⟩
          · exact Nat.le_trans (Nat.le_add_right readyAt 1)
              (Nat.le_trans afterLatch (Nat.le_add_right completedAt 1))
          · simpa only [goalValidator, goalHead] using commitAdvanced
    | some goal =>
        have reservation := arms.pendingGoalKeepsReservation readyAt validator
          goal pendingState
        rcases source.pending_goal_completes_timer_or_commit_race arms readyAt
            validator goal pendingState with
          ⟨completedAt, afterReady, completionBound, _completed,
            timerStarted | commitAdvanced⟩
        · left
          let start := goal.toTimerStart (completedAt + 1)
          refine ⟨{
            start
            timerStarted := timerStarted
            startValidator := reservation.goalValidator
            startCommitHead := ?_
            startTargetRound := ?_
            parentReadyByObservation := ?_
            startWithinLocalBound := ?_ }⟩
          · simpa [start, ValidatorRecoveryTimerArmGoal.toTimerStart,
              reservation.goalValidator] using
                reservation.commitHeadCurrent.symm
          · simpa [start, ValidatorRecoveryTimerArmGoal.toTimerStart,
              reservation.goalValidator] using
                reservation.targetIsExactNext
          · simpa [start, ValidatorRecoveryTimerArmGoal.toTimerStart] using
              reservation.parentsReadyAtReached
          · dsimp [start, ValidatorRecoveryTimerArmGoal.toTimerStart]
            have visible := Nat.add_le_add_right completionBound 1
            exact Nat.le_trans visible (by omega)
        · right
          refine ⟨completedAt + 1, ?_, ?_⟩
          · exact Nat.le_trans afterReady (Nat.le_add_right completedAt 1)
          · simpa only [reservation.goalValidator,
              reservation.commitHeadCurrent] using commitAdvanced
  · rcases source.storedExactTimerHasSource readyAt validator
        validatorInRange validatorCorrectAvailable armed with
      ⟨start, timerStarted, startValidator, startHead, startTarget, _stored,
        startBeforeReady, _activeBefore⟩
    left
    refine ⟨{
      start
      timerStarted
      startValidator
      startCommitHead := startHead
      startTargetRound := startTarget
      parentReadyByObservation := ?_
      startWithinLocalBound := ?_ }⟩
    · exact Nat.le_trans (source.startsAfterParentsReady start timerStarted)
        startBeforeReady
    · exact Nat.le_trans startBeforeReady
        (Nat.le_add_right readyAt (timed.localActionBound + 2))

/-- The exact past timer source of an erased timer-paced production.

`ValidatorTimerPacedRoundProduction` stores the timer fields as data, but it
does not retain the `timerStarted` proof which created them. This source map
restores only that past identity. Exact timer keys are unique at one validator.
-/
structure ValidatorTimerPacedRecoveryOriginRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits) : Prop where
  productionHasTimerOrigin : ∀ {validator round}
      (production : ValidatorTimerPacedRoundProduction timed waits validator
        round),
    ∃ start : ValidatorRecoveryTimerStart BlockId CommitId,
      source.timerStarted start ∧
        start.validator = validator ∧
        start.commitHead = production.commitHead ∧
        start.targetRound = round ∧
        start.parentReadyAt = production.parentReadyAt ∧
        start.startedAt = production.timerStartedAt
  exactTimerKeyIsUnique : ∀ left right,
    source.timerStarted left →
    source.timerStarted right →
    left.validator = right.validator →
    left.commitHead = right.commitHead →
    left.targetRound = right.targetRound →
    left = right

/-- The past timer origin also proves that the production's stored commit head
is the current local head at its timer start. -/
theorem ValidatorTimerPacedRecoveryOriginRules.production_head_at_timer_start
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits}
    (rules : ValidatorTimerPacedRecoveryOriginRules source)
    {validator round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round) :
    production.commitHead =
      ((timed.execution.trace production.timerStartedAt).validatorState
        validator).commitHead := by
  rcases rules.productionHasTimerOrigin production with
    ⟨start, timerStarted, startValidator, startHead, _startTarget,
      _parentReady, startTime⟩
  have currentHead := source.commitHeadAtStart start timerStarted
  simpa only [← startValidator, ← startTime, ← startHead] using currentHead.symm

/-- A fresh production on a no-advance suffix uses the exact common head from
the start of that suffix. -/
theorem ValidatorTimerPacedRecoveryOriginRules.fresh_production_head_on_suffix
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits}
    (rules : ValidatorTimerPacedRecoveryOriginRules source)
    {start observation validator round : Nat}
    {prior : ValidatorCommitHead CommitId}
    (headsAtStart : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace start).validatorState validator).commitHead =
        prior)
    (startBeforeObservation : start ≤ observation)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start)
    (production : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation validator round) :
    production.production.commitHead = prior := by
  have validatorInRange : validator < config.authorityCount := by
    simpa [production.production.proposer] using
      production.production.snapshot.proposerInRange
  have validatorCorrect : faults.correctAvailable validator = true := by
    simpa [production.production.proposer] using
      production.production.snapshot.proposerCorrectAvailable
  have startBeforeTimer : start ≤ production.production.timerStartedAt :=
    Nat.le_trans startBeforeObservation
      (Nat.le_of_lt production.timerAfterObservation)
  have currentHead := no_commit_advance_keeps_correct_commit_head
    validatorInRange validatorCorrect startBeforeTimer noAdvance
  exact (rules.production_head_at_timer_start production.production).trans
    (currentHead.trans (headsAtStart validator validatorInRange validatorCorrect))

/-- Without a receiver-local commit-index advance, its commit head stays fixed
between two ordered trace times. -/
theorem receiver_head_stays_fixed_without_local_commit_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start later receiver : Time}
    (receiverInRange : receiver < config.authorityCount)
    (ordered : start ≤ later)
    (noAdvance : ¬ValidatorReceiverCommitAdvance timed start receiver) :
    ((timed.execution.trace later).validatorState receiver).commitHead =
      ((timed.execution.trace start).validatorState receiver).commitHead := by
  have durable := timed.execution.durableStateMonotone receiver start later
    receiverInRange ordered
  have notStrict : ¬
      ((timed.execution.trace start).validatorState
          receiver).commitHead.index <
        ((timed.execution.trace later).validatorState
          receiver).commitHead.index := by
    intro advanced
    exact noAdvance ⟨later, ordered, advanced⟩
  have sameIndex :
      ((timed.execution.trace start).validatorState
          receiver).commitHead.index =
        ((timed.execution.trace later).validatorState
          receiver).commitHead.index := by
    have monotone := durable.1
    omega
  exact (durable.2.2.1 sameIndex).symm

/-- If a correct validator already has an own block for one timer-paced round,
the actual persistence which produced that timer-paced block is already
visible.

The nontrivial order uses the durable own-block origin. A restored time-zero
value at the same round contradicts the timer's exact-next signer floor. A past
persistence of the same functional own-block reference must be the same action
batch as the production's persistence. -/
theorem owned_round_bounds_timer_paced_persistence
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {readyAt validator round : Time}
    (production : ValidatorTimerPacedRoundProduction timed waits validator round)
    (owned :
      (((timed.execution.trace readyAt).validatorState validator).ownBlockAt
        round).isSome = true) :
    production.persistTime + 1 ≤ readyAt := by
  by_cases alreadyStored : production.persistTime + 1 ≤ readyAt
  · exact alreadyStored
  · have readyBeforeStored : readyAt ≤ production.persistTime + 1 :=
      Nat.le_of_not_ge alreadyStored
    have validatorInRange : validator < config.authorityCount := by
      simpa [production.proposer] using production.snapshot.proposerInRange
    cases ownValue :
        ((timed.execution.trace readyAt).validatorState validator).ownBlockAt
          round with
    | none => simp [ownValue] at owned
    | some reference =>
        have ownedAtStored :
            ((timed.execution.trace
              (production.persistTime + 1)).validatorState validator).ownBlockAt
                round = some reference :=
          (timed.execution.durable_fields_persist validatorInRange
            readyBeforeStored).own_block_persists ownValue
        have productionOwnedAtStored :
            ((timed.execution.trace
              (production.persistTime + 1)).validatorState validator).ownBlockAt
                round = some production.snapshot.block.reference := by
          rw [← production.storedAfterPersistence]
          simpa [production.proposer, production.blockRound] using
            production.snapshot.blockStored
        have sameReference : reference = production.snapshot.block.reference :=
          Option.some.inj (ownedAtStored.symm.trans productionOwnedAtStored)
        rcases validator_trace_own_block_has_past_persist_origin timed ownValue
          with initial | persisted
        · have initialFloorCoversRound : round ≤
              ((timed.execution.trace 0).validatorState
                validator).highestSignedRound :=
            (timed.execution.statesWellFormed 0 validator validatorInRange)
              |>.ownBlockDoesNotExceedSignerFloor round reference initial
          have floorMonotone :=
            (timed.execution.durableStateMonotone validator 0
              production.timerStartedAt validatorInRange (Nat.zero_le _)
              ).2.2.2.2.2.2.1
          have timerFloor :
              ((timed.execution.trace production.timerStartedAt).validatorState
                validator).highestSignedRound + 1 = round := by
            exact production.timerStartsExactNext.symm
          have timerFloorBelowRound :
              ((timed.execution.trace production.timerStartedAt).validatorState
                validator).highestSignedRound < round := by
            omega
          have initialFloorBelowRound :=
            Nat.lt_of_le_of_lt floorMonotone timerFloorBelowRound
          exact False.elim
            ((Nat.not_lt_of_ge initialFloorCoversRound) initialFloorBelowRound)
        · rcases persisted with
            ⟨persistTime, block, persistBeforeReady, occurs, _blockRound,
              blockReference⟩
          have samePersistedReference :
              production.snapshot.block.reference = block.reference := by
            exact sameReference.symm.trans blockReference.symm
          have samePersistTime : production.persistTime = persistTime :=
            persist_proposal_reference_times_are_equal validatorInRange
              production.persistenceOccurs occurs samePersistedReference
          rw [samePersistTime]
          exact Nat.succ_le_iff.mpr persistBeforeReady

/-- One validator cannot persist two different proposal bodies in the same
round.

The first persistence raises the durable signer floor to that round. Thus, a
second persistence in a later batch cannot pass its basic guard. If both
actions are in the same batch, their durable `ownBlockAt` results identify the
same exact reference. -/
theorem persist_proposal_same_round_blocks_are_equal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator left right : Nat}
    {leftBlock rightBlock : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (leftOccurs : ValidatorLocalActionOccurs (timed.execution.events left)
      validator (.persistProposal leftBlock))
    (rightOccurs : ValidatorLocalActionOccurs (timed.execution.events right)
      validator (.persistProposal rightBlock))
    (sameRound : leftBlock.reference.round = rightBlock.reference.round) :
    left = right ∧ leftBlock.reference = rightBlock.reference := by
  have notLeftBeforeRight : ¬left < right := by
    intro leftBeforeRight
    have stored := persist_proposal_occurrence_stores_own_block
      timed.execution leftOccurs
    have floorAtLeft :=
      (timed.execution.statesWellFormed (left + 1) validator validatorInRange)
        |>.ownBlockDoesNotExceedSignerFloor leftBlock.reference.round
          leftBlock.reference stored
    have floorMonotone :=
      (timed.execution.durableStateMonotone validator (left + 1) right
        validatorInRange (Nat.succ_le_iff.mpr leftBeforeRight)).2.2.2.2.2.2.1
    have rightBelow := persist_proposal_occurrence_starts_below_proposed_round
      timed.execution rightOccurs
    rw [sameRound] at floorAtLeft
    exact (Nat.not_lt_of_ge (Nat.le_trans floorAtLeft floorMonotone)) rightBelow
  have notRightBeforeLeft : ¬right < left := by
    intro rightBeforeLeft
    have stored := persist_proposal_occurrence_stores_own_block
      timed.execution rightOccurs
    have floorAtRight :=
      (timed.execution.statesWellFormed (right + 1) validator validatorInRange)
        |>.ownBlockDoesNotExceedSignerFloor rightBlock.reference.round
          rightBlock.reference stored
    have floorMonotone :=
      (timed.execution.durableStateMonotone validator (right + 1) left
        validatorInRange (Nat.succ_le_iff.mpr rightBeforeLeft)).2.2.2.2.2.2.1
    have leftBelow := persist_proposal_occurrence_starts_below_proposed_round
      timed.execution leftOccurs
    rw [← sameRound] at floorAtRight
    exact (Nat.not_lt_of_ge (Nat.le_trans floorAtRight floorMonotone)) leftBelow
  have sameTime : left = right :=
    Nat.le_antisymm (Nat.le_of_not_gt notRightBeforeLeft)
      (Nat.le_of_not_gt notLeftBeforeRight)
  have leftStored := persist_proposal_occurrence_stores_own_block
    timed.execution leftOccurs
  have rightStored := persist_proposal_occurrence_stores_own_block
    timed.execution rightOccurs
  have rightStoredAtLeftRound :
      ((timed.execution.trace (left + 1)).validatorState validator).ownBlockAt
          leftBlock.reference.round = some rightBlock.reference := by
    simpa only [sameTime, sameRound] using rightStored
  exact ⟨sameTime, Option.some.inj (leftStored.symm.trans rightStoredAtLeftRound)⟩

/-- A synchronized exact prior-round quorum bounds the actual next fresh
timer's parent-ready observation and timer start.

The prior local persistence fixes the receiver's signer floor at `round` when
the synchronized quorum becomes usable. If the actual next timer already
started, its own parent-ready field is already before `readyAt`. Otherwise, the
current timer worker derives the same exact timer key within the local bound.
Only a commit advance at this receiver can terminate that comparison. -/
theorem synchronized_prior_quorum_bounds_actual_fresh_timer_or_receiver_advances
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (originRules : ValidatorTimerPacedRecoveryOriginRules source)
    {observation readyAt receiver round : Time}
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations waits
      observation receiver (round + 1))
    (previousStoredBeforeReady :
      previous.production.persistTime + 1 ≤ readyAt)
    (usable : ValidatorReceiverUsableCorrectQuorumLayer config faults
      (timed.execution.trace readyAt) receiver round)
    (activeAtReady : (timed.execution.trace readyAt).epochActive = true) :
    ValidatorReceiverCommitAdvance timed readyAt receiver ∨
      (next.production.parentReadyAt ≤ readyAt ∧
        next.production.timerStartedAt ≤
          readyAt + timed.localActionBound + 2) := by
  by_cases timerBeforeReady : next.production.timerStartedAt ≤ readyAt
  · right
    exact ⟨Nat.le_trans next.production.timerStartsAfterParentReady
      timerBeforeReady, Nat.le_trans timerBeforeReady (by omega)⟩
  · have readyBeforeTimer : readyAt ≤ next.production.timerStartedAt :=
      Nat.le_of_not_ge timerBeforeReady
    have receiverInRange : receiver < config.authorityCount := by
      simpa [next.production.proposer] using
        next.production.snapshot.proposerInRange
    have receiverCorrect : faults.correctAvailable receiver = true := by
      simpa [next.production.proposer] using
        next.production.snapshot.proposerCorrectAvailable
    have previousOwnAtStored :
        ((timed.execution.trace
          (previous.production.persistTime + 1)).validatorState receiver
          ).ownBlockAt round = some previous.production.snapshot.block.reference := by
      rw [← previous.production.storedAfterPersistence]
      simpa [previous.production.proposer, previous.production.blockRound] using
        previous.production.snapshot.blockStored
    have previousOwnAtReady :
        ((timed.execution.trace readyAt).validatorState receiver).ownBlockAt
          round = some previous.production.snapshot.block.reference :=
      (timed.execution.durable_fields_persist receiverInRange
        previousStoredBeforeReady).own_block_persists previousOwnAtStored
    have roundAtMostReadyFloor : round ≤
        ((timed.execution.trace readyAt).validatorState
          receiver).highestSignedRound :=
      (timed.execution.statesWellFormed readyAt receiver receiverInRange)
        |>.ownBlockDoesNotExceedSignerFloor round
          previous.production.snapshot.block.reference previousOwnAtReady
    have readyFloorAtMostTimerFloor :=
      (timed.execution.durableStateMonotone receiver readyAt
        next.production.timerStartedAt receiverInRange readyBeforeTimer
        ).2.2.2.2.2.2.1
    have timerFloor :
        ((timed.execution.trace next.production.timerStartedAt).validatorState
          receiver).highestSignedRound = round := by
      have exactNext := next.production.timerStartsExactNext
      omega
    have readyFloor :
        ((timed.execution.trace readyAt).validatorState
          receiver).highestSignedRound = round := by
      apply Nat.le_antisymm
      · simpa only [timerFloor] using readyFloorAtMostTimerFloor
      · exact roundAtMostReadyFloor
    have parentsReady : ValidatorRecoveryParentQuorumReadyAt config
        ((timed.execution.trace readyAt).validatorState receiver)
        (((timed.execution.trace readyAt).validatorState
          receiver).highestSignedRound + 1) := by
      simpa only [readyFloor] using usable.nextParentsReady
    have currentInput := source.active_parent_quorum_state_gives_current_timer_input
      readyAt receiver activeAtReady receiverInRange receiverCorrect parentsReady
    rcases current_timer_input_gives_bounded_start_or_receiver_commit_advance
        source arms currentInput with ⟨⟨bounded⟩⟩ | advanced
    · by_cases receiverAdvanced :
          ValidatorReceiverCommitAdvance timed readyAt receiver
      · exact Or.inl receiverAdvanced
      · rcases originRules.productionHasTimerOrigin next.production with
          ⟨actualStart, actualStarted, actualValidator, actualHead,
            actualTarget, actualParentReady, actualStartTime⟩
        have headAtActualStart :=
          receiver_head_stays_fixed_without_local_commit_advance receiverInRange
            readyBeforeTimer receiverAdvanced
        have sameValidator : actualStart.validator = bounded.start.validator := by
          exact actualValidator.trans bounded.startValidator.symm
        have sameHead : actualStart.commitHead = bounded.start.commitHead := by
          calc
            actualStart.commitHead = next.production.commitHead := actualHead
            _ = ((timed.execution.trace
                  next.production.timerStartedAt).validatorState
                    receiver).commitHead :=
              originRules.production_head_at_timer_start next.production
            _ = ((timed.execution.trace readyAt).validatorState
                    receiver).commitHead := headAtActualStart
            _ = bounded.start.commitHead := bounded.startCommitHead.symm
        have sameTarget : actualStart.targetRound = bounded.start.targetRound := by
          calc
            actualStart.targetRound = round + 1 := actualTarget
            _ = ((timed.execution.trace readyAt).validatorState
                    receiver).highestSignedRound + 1 := by rw [readyFloor]
            _ = bounded.start.targetRound := bounded.startTargetRound.symm
        have sameStart : actualStart = bounded.start :=
          originRules.exactTimerKeyIsUnique actualStart bounded.start
            actualStarted bounded.timerStarted sameValidator sameHead sameTarget
        right
        constructor
        · rw [← actualParentReady, sameStart]
          exact bounded.parentReadyByObservation
        · rw [← actualStartTime, sameStart]
          exact bounded.startWithinLocalBound
    · exact Or.inl advanced

/-- The exact receiver source-window result supplies both facts needed by the
timer-ready bridge: the receiver has its own block in the prior round, and the
synchronized exact quorum is accepted, retained, and ready for the next timer.

Thus, a higher theorem can pass the result of ordinary source synchronization
directly. It does not need to assume a separate timer-ready time. -/
theorem receiver_source_window_bounds_actual_fresh_timer_or_receiver_advances
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (originRules : ValidatorTimerPacedRecoveryOriginRules source)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {recoveryWait snapshot baseRound count receiver observation : Time}
    (window : ValidatorReceiverExactRecoverySourceWindow timed obligations pins
      recoveryWait snapshot baseRound count receiver)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (baseRound + count))
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations waits
      observation receiver (baseRound + count + 1))
    (activeAtFinish :
      (timed.execution.trace window.finish).epochActive = true) :
    ValidatorReceiverCommitAdvance timed window.finish receiver ∨
      (next.production.parentReadyAt ≤ window.finish ∧
        next.production.timerStartedAt ≤
          window.finish + timed.localActionBound + 2) := by
  have receiverInRange : receiver < config.authorityCount := by
    simpa [previous.production.proposer] using
      previous.production.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [previous.production.proposer] using
      previous.production.snapshot.proposerCorrectAvailable
  have ownedAtSourceFinish := window.sourceWindow.owned receiver receiverInRange
    receiverCorrect
  have ownedAtFinish :
      (((timed.execution.trace window.finish).validatorState receiver).ownBlockAt
        (baseRound + count)).isSome = true := by
    cases ownValue :
        ((timed.execution.trace
          window.sourceWindow.finish).validatorState receiver).ownBlockAt
            (baseRound + count) with
    | none => simp [ownValue] at ownedAtSourceFinish
    | some reference =>
        have persisted :
            ((timed.execution.trace window.finish).validatorState receiver
              ).ownBlockAt (baseRound + count) = some reference :=
          (timed.execution.durable_fields_persist receiverInRange
            window.sourceFinishBeforeFinish).own_block_persists ownValue
        simp [persisted]
  have previousStoredBeforeFinish :
      previous.production.persistTime + 1 ≤ window.finish :=
    owned_round_bounds_timer_paced_persistence previous.production ownedAtFinish
  exact synchronized_prior_quorum_bounds_actual_fresh_timer_or_receiver_advances
    source arms originRules previous next previousStoredBeforeFinish
      window.usable activeAtFinish

/-- The synchronized-quorum bound supplies the upper edge of one timer-spread
recurrence step. -/
theorem synchronized_prior_quorum_gives_next_start_upper_edge_or_receiver_advances
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution source)
    (originRules : ValidatorTimerPacedRecoveryOriginRules source)
    {observation readyAt receiver round previousLatest syncCost : Time}
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations waits
      observation receiver (round + 1))
    (previousStoredBeforeReady :
      previous.production.persistTime + 1 ≤ readyAt)
    (usable : ValidatorReceiverUsableCorrectQuorumLayer config faults
      (timed.execution.trace readyAt) receiver round)
    (activeAtReady : (timed.execution.trace readyAt).epochActive = true)
    (readyBound : readyAt ≤ previousLatest + waits.wait
      previous.production.commitHead round + syncCost) :
    ValidatorReceiverCommitAdvance timed readyAt receiver ∨
      next.production.timerStartedAt ≤
        previousLatest + waits.wait previous.production.commitHead round +
          (syncCost + timed.localActionBound + 2) := by
  rcases synchronized_prior_quorum_bounds_actual_fresh_timer_or_receiver_advances
      source arms originRules previous next previousStoredBeforeReady usable
        activeAtReady with advanced | bounded
  · exact Or.inl advanced
  · right
    have shifted := Nat.add_le_add_right readyBound
      (timed.localActionBound + 2)
    exact Nat.le_trans bounded.2 (by
      simpa [Nat.add_assoc] using shifted)

/-! Pairwise timer-spread recurrence for one actual fresh suffix. -/

/-- All actual fresh timer starts in one exact round have one directional
pairwise spread bound.

This predicate refers only to timer-paced productions which already occur in
the trace. -/
def ValidatorFreshRoundTimerStartSpreadAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (observation round spread : Nat) : Prop :=
  ∀ {left right}
    (leftProduction : ValidatorFreshTimerPacedExactRoundProduction timed
      obligations waits observation left round)
    (rightProduction : ValidatorFreshTimerPacedExactRoundProduction timed
      obligations waits observation right round),
    leftProduction.production.timerStartedAt ≤
      rightProduction.production.timerStartedAt + spread

/-- One current/past upper edge from an actual prior-round timer to each
actual next-round timer.

A receiver synchronization theorem supplies this result. The result does not
assert that a timer or proposal exists. -/
def ValidatorFreshTimerStartSuccessorUpperAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (observation round roundWait stepCost : Nat) : Prop :=
  ∀ {receiver}
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1)),
    ∃ author,
      ∃ previous : ValidatorFreshTimerPacedExactRoundProduction timed
        obligations waits observation author round,
        next.production.timerStartedAt ≤
          previous.production.timerStartedAt + roundWait + stepCost

/-- One current/past lower edge from an actual prior-round timer to each
actual next-round timer.

The correct initial-parent origin theorem supplies this result. It does not
assert that a timer or proposal exists. -/
def ValidatorFreshTimerStartSuccessorLowerAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (observation round roundWait : Nat) : Prop :=
  ∀ {receiver}
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1)),
    ∃ author,
      ∃ previous : ValidatorFreshTimerPacedExactRoundProduction timed
        obligations waits observation author round,
        previous.production.timerStartedAt + roundWait ≤
          next.production.timerStartedAt

/-- A fresh prior-round family supplies the concrete lower successor edge.

The next timer's actual initial quorum intersects the correct, available
authors. Durable signer-floor uniqueness identifies that correct parent with
the exact fresh production by the same author in the prior round. The existing
initial-parent theorem then puts the next timer after the prior deadline. -/
theorem fresh_family_gives_timer_start_successor_lower
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (ownership : ValidatorAuthenticatedAcceptedBodyOwnershipRules
      (timed := timed))
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Nat} {prior : ValidatorCommitHead CommitId}
    (previousFamily :
      EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed obligations
        waits observation round)
    (previousHeads : ∀ {author}
      (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
        waits observation author round),
      previous.production.commitHead = prior) :
    ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits observation
      round (waits.wait prior round) := by
  unfold ValidatorFreshTimerStartSuccessorLowerAt
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
  have weakDeadline := Nat.le_of_lt deadlineBeforeNext
  simpa only [previousHeads previous, startTime] using weakDeadline

/-- Two fresh production records for one validator, round, and exact commit
head have the same actual timer start. -/
theorem same_validator_round_and_head_fresh_timer_starts_equal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation validator round : Nat}
    {prior : ValidatorCommitHead CommitId}
    (left right : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation validator round)
    (leftHead : left.production.commitHead = prior)
    (rightHead : right.production.commitHead = prior) :
    left.production.timerStartedAt = right.production.timerStartedAt := by
  rcases originRules.productionHasTimerOrigin left.production with
    ⟨leftStart, leftStarted, leftValidator, leftOriginHead, leftTarget,
      _leftReady, leftTime⟩
  rcases originRules.productionHasTimerOrigin right.production with
    ⟨rightStart, rightStarted, rightValidator, rightOriginHead, rightTarget,
      _rightReady, rightTime⟩
  have sameValidator : leftStart.validator = rightStart.validator :=
    leftValidator.trans rightValidator.symm
  have sameHead : leftStart.commitHead = rightStart.commitHead := by
    rw [leftOriginHead, rightOriginHead, leftHead, rightHead]
  have sameTarget : leftStart.targetRound = rightStart.targetRound := by
    rw [leftTarget, rightTarget]
  have sameStart := originRules.exactTimerKeyIsUnique leftStart rightStart
    leftStarted rightStarted sameValidator sameHead sameTarget
  exact leftTime.symm.trans ((congrArg ValidatorRecoveryTimerStart.startedAt
    sameStart).trans rightTime)

/-- Maximum of one base time and the first `count` values. -/
def validatorTimerStartMaximumUpTo
    (base : Nat) (value : Nat → Nat) : Nat → Nat
  | 0 => base
  | count + 1 =>
      max (validatorTimerStartMaximumUpTo base value count) (value count)

/-- Every indexed value is at most its finite maximum. -/
theorem validator_timer_start_le_maximum_up_to
    (base : Nat) (value : Nat → Nat)
    {index count : Nat}
    (indexInRange : index < count) :
    value index ≤ validatorTimerStartMaximumUpTo base value count := by
  induction count with
  | zero => omega
  | succ previous inductionHypothesis =>
      simp only [validatorTimerStartMaximumUpTo]
      by_cases earlier : index < previous
      · exact Nat.le_trans (inductionHypothesis earlier)
          (Nat.le_max_left _ _)
      · have last : index = previous := by omega
        subst index
        exact Nat.le_max_right _ _

/-- One actual fresh family has a finite initial pairwise timer spread.

The bound is the absolute maximum selected timer start. It need not be tight.
Timer-origin uniqueness makes it apply to every proof record for the same
validator and round. -/
theorem fresh_family_has_finite_timer_start_spread
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Nat} {prior : ValidatorCommitHead CommitId}
    (family : EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed
      obligations waits observation round)
    (heads : ∀ {author}
      (production : ValidatorFreshTimerPacedExactRoundProduction timed
        obligations waits observation author round),
      production.production.commitHead = prior) :
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
      selected.production.timerStartedAt :=
    same_validator_round_and_head_fresh_timer_starts_equal originRules
      leftProduction selected (heads leftProduction) (heads selected)
  have selectedBound : selected.production.timerStartedAt ≤ spread := by
    have bounded := validator_timer_start_le_maximum_up_to 0 startFor leftInRange
    simpa [spread, startFor, leftInRange, leftCorrect, selected] using bounded
  rw [sameStart]
  exact Nat.le_trans selectedBound (Nat.le_add_left spread _)

/-- Pointwise upper and lower successor edges increase pairwise timer spread by
at most one fixed step cost.

The proof does not select a global earliest or latest timer. It selects one
prior timer for each of the two next-round timers and uses the prior pairwise
spread between those two selected timers. -/
theorem fresh_timer_start_pairwise_spread_successor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation round spread roundWait stepCost : Nat}
    (previousSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations
      waits observation round spread)
    (upper : ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
      observation round roundWait stepCost)
    (lower : ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
      observation round roundWait) :
    ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
      (round + 1) (spread + stepCost) := by
  unfold ValidatorFreshRoundTimerStartSpreadAt at previousSpread ⊢
  unfold ValidatorFreshTimerStartSuccessorUpperAt at upper
  unfold ValidatorFreshTimerStartSuccessorLowerAt at lower
  intro left right leftProduction rightProduction
  rcases upper leftProduction with
    ⟨upperAuthor, upperPrevious, leftUpper⟩
  rcases lower rightProduction with
    ⟨lowerAuthor, lowerPrevious, rightLower⟩
  have priorSpread := previousSpread upperPrevious lowerPrevious
  have priorShifted := Nat.add_le_add_right priorSpread
    (roundWait + stepCost)
  have lowerShifted := Nat.add_le_add_right rightLower
    (spread + stepCost)
  calc
    leftProduction.production.timerStartedAt ≤
        upperPrevious.production.timerStartedAt + roundWait + stepCost :=
      leftUpper
    _ ≤ (lowerPrevious.production.timerStartedAt + spread) +
          roundWait + stepCost := by
      simpa [Nat.add_assoc] using priorShifted
    _ ≤ rightProduction.production.timerStartedAt + spread + stepCost := by
      simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using lowerShifted
    _ = rightProduction.production.timerStartedAt +
          (spread + stepCost) := by
      simp [Nat.add_assoc]

/-- A prior-round pairwise spread and one lower successor edge give the
directional adjacent start bound used by the leader-to-voter proof. -/
theorem fresh_timer_start_spread_gives_adjacent_bound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation author receiver round spread roundWait : Nat}
    (previousSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations
      waits observation round spread)
    (lower : ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
      observation round roundWait)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations waits
      observation receiver (round + 1)) :
    previous.production.timerStartedAt ≤
      next.production.timerStartedAt + spread := by
  unfold ValidatorFreshRoundTimerStartSpreadAt at previousSpread
  unfold ValidatorFreshTimerStartSuccessorLowerAt at lower
  rcases lower next with ⟨lowerAuthor, lowerPrevious, lowerBound⟩
  have pairwise := previousSpread previous lowerPrevious
  have waitCanOnlyDelay :
      lowerPrevious.production.timerStartedAt + spread ≤
        (lowerPrevious.production.timerStartedAt + roundWait) + spread := by
    exact Nat.add_le_add_right
      (Nat.le_add_right lowerPrevious.production.timerStartedAt roundWait)
        spread
  have lowerShifted := Nat.add_le_add_right lowerBound spread
  exact Nat.le_trans pairwise
    (Nat.le_trans waitCanOnlyDelay lowerShifted)

/-- Repeated current/past successor edges give a linear pairwise spread over
one finite actual fresh suffix. -/
theorem fresh_timer_start_pairwise_spread_recurrence_is_linear
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation referenceRound base stepCost distance : Nat}
    (roundWait : Nat → Nat)
    (baseSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations waits
      observation referenceRound base)
    (upper : ∀ offset,
      offset < distance →
      ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
        observation (referenceRound + offset) (roundWait offset) stepCost)
    (lower : ∀ offset,
      offset < distance →
      ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
        observation (referenceRound + offset) (roundWait offset)) :
    ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
      (referenceRound + distance) (base + distance * stepCost) := by
  induction distance with
  | zero =>
      intro left right leftProduction rightProduction
      simpa using baseSpread leftProduction rightProduction
  | succ previous inductionHypothesis =>
      have prefixUpper : ∀ offset,
          offset < previous →
          ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
            observation (referenceRound + offset) (roundWait offset) stepCost :=
        fun offset inRange => upper offset (Nat.lt_succ_of_lt inRange)
      have prefixLower : ∀ offset,
          offset < previous →
          ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
            observation (referenceRound + offset) (roundWait offset) :=
        fun offset inRange => lower offset (Nat.lt_succ_of_lt inRange)
      have previousResult :
          ValidatorFreshRoundTimerStartSpreadAt timed obligations waits
            observation (referenceRound + previous)
              (base + previous * stepCost) :=
        inductionHypothesis prefixUpper prefixLower
      have nextResult :
          ValidatorFreshRoundTimerStartSpreadAt timed obligations waits
            observation ((referenceRound + previous) + 1)
              ((base + previous * stepCost) + stepCost) :=
        fresh_timer_start_pairwise_spread_successor previousResult
          (upper previous (Nat.lt_succ_self previous))
            (lower previous (Nat.lt_succ_self previous))
      intro left right leftProduction rightProduction
      have bounded := nextResult leftProduction rightProduction
      simpa [Nat.succ_eq_add_one, Nat.succ_mul, Nat.add_assoc,
        Nat.add_left_comm, Nat.add_comm] using bounded

end Mysticeti
