/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorAuthorLocalProposalContinuation
import Mysticeti.ValidatorRoundFrontierBridge

namespace Mysticeti

/-! Commit-tolerant author-local block progress.

This module removes the local commit race from one current recovery-timer
input. A ready proposal is already protected. An occupied or newly selected
timer-arm goal is one exact pre-attempt schedule. If a same-author commit wins
before that callback, the serialized Core handler returns an exact past
persistence or a legal normal safe-resume callback.

The result is one actual addressed proposal broadcast at the requested target
or at a higher round. It does not use a commit as progress. A higher theorem
must align the GC safe-resume cases across a quorum before it derives one common
DAG layer.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- A positive operational quorum at one correct holder supplies the public
DAG-layer witness.

The operational source keeps exact references. This theorem selects their
catalogued bodies and erases the stronger local parent-list source. -/
theorem positive_operational_quorum_gives_correct_held_total_quorum_layer
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {holder round : Nat}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (roundPositive : 0 < round)
    (roundAboveHolderGc : (world.validatorState holder).gcRound < round)
    (quorum : ValidatorOperationalQuorumAt config world holder round) :
    Nonempty (CorrectHeldTotalQuorumLayer config faults world round) := by
  have selectBodies : ∀ references : List (ValidatorBlockRef BlockId),
      (∀ reference, reference ∈ references →
        ∃ block : ValidatorBlock BlockId,
          world.blockCatalog reference.id = some block ∧
            block.reference = reference ∧
            reference.author < config.authorityCount ∧
            block.HasQuorumImmediateParents config) →
      ∃ blocks : List (ValidatorBlock BlockId),
        blocks.map (fun block => block.reference) = references ∧
          ∀ block, block ∈ blocks →
            world.blockCatalog block.reference.id = some block ∧
              block.reference.author < config.authorityCount ∧
              block.HasQuorumImmediateParents config := by
    intro references bodies
    induction references with
    | nil =>
        exact ⟨[], rfl, by simp⟩
    | cons reference references inductionHypothesis =>
        rcases bodies reference (by simp) with
          ⟨block, catalogued, blockReference, authorInRange, valid⟩
        rcases inductionHypothesis (fun later laterMember =>
            bodies later (by simp [laterMember])) with
          ⟨blocks, blockReferences, blockFacts⟩
        refine ⟨block :: blocks, ?_, ?_⟩
        · simp [blockReference, blockReferences]
        · intro selected selectedMember
          simp only [List.mem_cons] at selectedMember
          rcases selectedMember with selectedIsBlock | selectedInBlocks
          · subst selected
            simpa [blockReference] using
              And.intro catalogued (And.intro authorInRange valid)
          · exact blockFacts selected selectedInBlocks
  rcases selectBodies quorum.references (quorum.positiveBodies roundPositive)
      with ⟨blocks, blockReferences, blockFacts⟩
  have blockAuthors :
      blocks.map (fun block => block.reference.author) =
        quorum.references.map ValidatorBlockRef.author := by
    change
      blocks.map (ValidatorBlockRef.author ∘ fun block => block.reference) =
        quorum.references.map ValidatorBlockRef.author
    simpa only [List.map_map] using
      congrArg (List.map ValidatorBlockRef.author) blockReferences
  have blockReferenceMember : ∀ block, block ∈ blocks →
      block.reference ∈ quorum.references := by
    intro block blockMember
    rw [← blockReferences]
    exact List.mem_map.mpr ⟨block, blockMember, rfl⟩
  exact ⟨{
    holder := holder
    holderInRange := holderInRange
    holderCorrectAvailable := holderCorrectAvailable
    blocks := blocks
    blockAuthorsNodup := by
      rw [blockAuthors]
      exact quorum.ready.1
    blockAuthorsInRange := by
      intro block blockMember
      exact (blockFacts block blockMember).2.1
    blocksAtRound := by
      intro block blockMember
      have parentRound :=
        (quorum.ready.2.1 block.reference
          (blockReferenceMember block blockMember)).1
      omega
    blockStakeIsQuorum := by
      rw [blockReferences]
      exact quorum.ready.2.2
    roundPositive := roundPositive
    roundAboveHolderGc := roundAboveHolderGc
    blocksAccepted := by
      intro block blockMember
      exact (quorum.ready.2.1 block.reference
        (blockReferenceMember block blockMember)).2
    blocksRetained := by
      intro block blockMember
      exact quorum.retained block.reference
        (blockReferenceMember block blockMember)
    blocksCatalogued := by
      intro block blockMember
      exact (blockFacts block blockMember).1
    blocksValid := by
      intro block blockMember
      exact (blockFacts block blockMember).2.2
  }⟩

/-- A positive operational frontier at one correct holder projects directly to
the public DAG-layer witness. -/
theorem positive_operational_frontier_gives_correct_held_total_quorum_layer
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {holder round : Nat}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (roundPositive : 0 < round)
    (frontier : ValidatorOperationalQuorumFrontierAt config world holder round
      canonicalGenesisParents) :
    Nonempty (CorrectHeldTotalQuorumLayer config faults world round) := by
  have roundAboveHolderGc : (world.validatorState holder).gcRound < round := by
    rcases frontier.aboveGcOrGenesis with
      ⟨roundZero, _gcZero⟩ | ⟨_roundPositive, roundAboveGc⟩
    · omega
    · exact roundAboveGc
  exact positive_operational_quorum_gives_correct_held_total_quorum_layer
    holderInRange holderCorrectAvailable roundPositive roundAboveHolderGc
      frontier.quorum

/-- A deterministic peer used only to instantiate one exact recovery
broadcast. -/
private def dagProgressOtherReceiver (validator : Nat) : Nat :=
  if validator = 0 then 1 else 0

private theorem dag_progress_other_receiver_in_range
    {validator authorityCount : Nat}
    (authorityCountAtLeastTwo : 1 < authorityCount) :
    dagProgressOtherReceiver validator < authorityCount := by
  simp only [dagProgressOtherReceiver]
  split
  · exact authorityCountAtLeastTwo
  · omega

private theorem dag_progress_other_receiver_is_different
    {validator : Nat} :
    dagProgressOtherReceiver validator ≠ validator := by
  simp only [dagProgressOtherReceiver]
  split
  · omega
  · omega

/-- One exact addressed proposal broadcast at a selected round which is not
below the requested recovery target. -/
def ValidatorAuthorLocalAtOrAboveBroadcastAt
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (start validator minimumRound : Time) : Prop :=
  ∃ round,
    minimumRound ≤ round ∧
    Nonempty { result :
      ValidatorPersistedProposalBroadcastProduction timed obligations start
        validator //
      result.proposal.block.reference.round = round }

/-- Current latched proposal work gives one exact addressed broadcast. -/
private theorem ready_proposal_eventually_produces_exact_broadcast
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Time} {proposal : ValidatorReadyProposal BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ready : (obligations.trace start validator).readyProposal =
      some proposal) :
    Nonempty { result :
        ValidatorPersistedProposalBroadcastProduction timed obligations start
          validator //
      result.proposal.block = proposal.block } := by
  have legal := obligations.readyProposalIsLegal start validator proposal ready
  rcases latched_proposal_runs_within_bound obligations validatorInRange
      validatorCorrectAvailable ready with
    ⟨completion, startBeforePersistence, _persistenceBound, persisted⟩
  let exact := Classical.choice
    (persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable startBeforePersistence legal.2.1 persisted)
  exact ⟨⟨exact.1, exact.2.2⟩⟩

/-- Re-index an exact persistence and broadcast from an earlier observation
whose signer floor is below the produced block. -/
private theorem persisted_broadcast_starts_earlier
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {earlier start validator round : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (earlierBeforeStart : earlier ≤ start)
    (aboveEarlierFloor :
      ((timed.execution.trace earlier).validatorState
        validator).highestSignedRound < round)
    (production : ValidatorPersistedProposalBroadcastProduction timed
      obligations start validator)
    (exactRound : production.proposal.block.reference.round = round) :
    Nonempty { result :
        ValidatorPersistedProposalBroadcastProduction timed obligations earlier
          validator //
      result.proposal.block.reference.round = round } := by
  have earlierBeforePersistence : earlier ≤ production.persistedAt :=
    Nat.le_trans earlierBeforeStart production.startBeforePersistence
  let exact := Classical.choice
    (persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable earlierBeforePersistence
          (by simpa [exactRound] using aboveEarlierFloor)
            production.persistenceOccurs)
  exact ⟨⟨exact.1, by rw [exact.2.2, exactRound]⟩⟩

/-- Normalize the serialized first-install result to an exact broadcast from
an earlier observation. Both result branches keep the requested lower bound. -/
private theorem scheduled_broadcast_starts_earlier
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {earlier start validator scheduledTarget : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (earlierBeforeStart : earlier ≤ start)
    (scheduledAboveEarlierFloor :
      ((timed.execution.trace earlier).validatorState
        validator).highestSignedRound < scheduledTarget)
    (broadcast : ValidatorAuthorLocalScheduledBroadcastAt obligations start
      validator scheduledTarget) :
    ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations earlier validator
      scheduledTarget := by
  cases broadcast with
  | persisted production =>
      let exact := Classical.choice production
      exact ⟨scheduledTarget, Nat.le_refl _,
        persisted_broadcast_starts_earlier latchSource effects
          authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
            earlierBeforeStart scheduledAboveEarlierFloor exact.1 exact.2⟩
  | normal scheduledAt targetRound parents startBeforeScheduled targetNotReset
      _targetPreservedOrGcObsolete production =>
      let normal := (Classical.choice production).1
      have earlierBeforePersistence : earlier ≤ normal.persistedAt :=
        Nat.le_trans earlierBeforeStart
          (Nat.le_trans startBeforeScheduled
            (Nat.le_trans normal.startBeforeProposalAction
              (Nat.le_trans (Nat.le_succ _)
                normal.proposalBeforePersistence)))
      have targetAboveEarlierFloor :
          ((timed.execution.trace earlier).validatorState
            validator).highestSignedRound < targetRound :=
        Nat.lt_of_lt_of_le scheduledAboveEarlierFloor targetNotReset
      let exact := Classical.choice
        (persist_proposal_occurrence_eventually_produces_exact_broadcast
          latchSource effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable earlierBeforePersistence
              (by simpa [normal.proposalRound] using targetAboveEarlierFloor)
                normal.persistenceOccurs)
      refine ⟨targetRound, targetNotReset, ⟨⟨exact.1, ?_⟩⟩⟩
      rw [exact.2.2, normal.proposalRound]

/-- Normalize an exact strict recovery broadcast to the origin-neutral
addressed-broadcast record used by DAG progress. -/
private theorem strict_recovery_broadcast_is_at_target
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator receiver targetRound : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (targetAboveStartFloor :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < targetRound)
    (result : ValidatorStrictRecoveryBroadcast timed waits timerSource
      obligations validator receiver)
    (exactTarget : result.targetRound = targetRound)
    (startBeforePersistence : start ≤ result.persistTime) :
    ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations start validator
      targetRound := by
  have exactBlockRound : result.snapshot.block.reference.round = targetRound := by
    rw [result.completed.snapshotRound, exactTarget]
  let exact := Classical.choice
    (persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable startBeforePersistence
          (by simpa [exactBlockRound] using targetAboveStartFloor)
            result.persistenceOccurs)
  refine ⟨targetRound, Nat.le_refl _, ⟨⟨exact.1, ?_⟩⟩⟩
  rw [exact.2.2, exactBlockRound]

/-- One current recovery timer input produces an actual block at its exact
next target or at a higher GC safe-resume target.

The same-author commit race is consumed internally. An install by another
validator is irrelevant to this fixed author's protected work. The conclusion
contains no commit, future layer, window, or receiver-run alternative. -/
theorem current_recovery_input_eventually_produces_at_or_above_broadcast
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (continuation : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) (arms := arms) mode)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator receiver : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (receiverInRange : receiver < config.authorityCount)
    (differentReceiver : receiver ≠ validator)
    (input : ValidatorRecoveryTimerCurrentInputAt timed start validator)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true) :
    ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations start validator
      (((timed.execution.trace start).validatorState
        validator).highestSignedRound + 1) := by
  let targetRound :=
    ((timed.execution.trace start).validatorState
      validator).highestSignedRound + 1
  have targetAboveStartFloor :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < targetRound := by
    exact Nat.lt_succ_self _
  cases readyValue : (obligations.trace start validator).readyProposal with
  | some proposal =>
      let exact := Classical.choice
        (ready_proposal_eventually_produces_exact_broadcast latchSource effects
          authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
            readyValue)
      have targetAtMostProposal : targetRound ≤
          proposal.block.reference.round := by
        have legal := obligations.readyProposalIsLegal start validator proposal
          readyValue
        exact Nat.succ_le_iff.mpr legal.2.1
      refine ⟨proposal.block.reference.round, targetAtMostProposal,
        ⟨⟨exact.1, ?_⟩⟩⟩
      rw [exact.2]
  | none =>
      have pointwise :=
        current_recovery_state_builds_strict_broadcast_or_commit_advance timed
          timerSource pacing arms latchSource effects start validator receiver
            input active receiverInRange differentReceiver
      rcases pointwise with localAdvance | strict
      · rcases localAdvance with
          ⟨finish, startBeforeFinish, headAdvanced⟩
        have dispose (schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms
            obligations start validator)
            (scheduleTarget : schedule.targetRound = targetRound) :
            ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations start
              validator targetRound := by
          rcases commit_advance_disposes_scheduled_attempt_with_broadcast
              continuation latchSource effects authorityCountAtLeastTwo
                validatorInRange validatorCorrectAvailable schedule
                  startBeforeFinish headAdvanced active with
            ⟨_installTime, _startBeforeInstall, _installBeforeFinish,
              broadcast⟩
          have scheduleAboveStartFloor :
              ((timed.execution.trace start).validatorState
                validator).highestSignedRound < schedule.targetRound := by
            simpa [scheduleTarget] using targetAboveStartFloor
          have normalized := scheduled_broadcast_starts_earlier latchSource
            effects authorityCountAtLeastTwo validatorInRange
              validatorCorrectAvailable (Nat.le_refl start)
                scheduleAboveStartFloor broadcast
          simpa [scheduleTarget] using normalized
        rcases input with armInput |
            ⟨_activeAtStart, _validatorInRange, _validatorCorrect, armed⟩
        · cases pendingValue : (arms.trace start validator).pending with
          | some goal =>
              have reservation := arms.pendingGoalKeepsReservation start
                validator goal pendingValue
              let schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms
                  obligations start validator :=
                .recoveryArmPending goal pendingValue readyValue
              apply dispose schedule
              change goal.targetRound = targetRound
              simpa [targetRound] using reservation.targetIsExactNext
          | none =>
              rcases arms.readyStateSelectsGoal start validator armInput
                  pendingValue with
                ⟨goal, selected, goalValidator, goalHead, goalTarget,
                  _parentsReadyAt, _eligible⟩
              have selectedAtGoal : arms.selectedGoal start goal.validator =
                  some goal := by
                simpa [goalValidator] using selected
              have latched := arms.selectedGoalLatches start goal selectedAtGoal
              have armStep := arms.transitionsFollowRules start goal.validator
              rw [latched] at armStep
              have pendingAtNext :
                  (arms.trace (start + 1) validator).pending = some goal := by
                cases armStep with
                | latch _ afterPending =>
                    simpa only [goalValidator] using afterPending
              cases readyNext :
                  (obligations.trace (start + 1) validator).readyProposal with
              | some proposal =>
                  let exact := Classical.choice
                    (ready_proposal_eventually_produces_exact_broadcast
                      latchSource effects authorityCountAtLeastTwo
                        validatorInRange validatorCorrectAvailable readyNext)
                  have floorMonotone :=
                    (timed.execution.durableStateMonotone validator start
                      (start + 1) validatorInRange (Nat.le_succ start)
                      ).2.2.2.2.2.2.1
                  have legal := obligations.readyProposalIsLegal (start + 1)
                    validator proposal readyNext
                  have targetAtMostProposal : targetRound ≤
                      proposal.block.reference.round := by
                    exact Nat.le_trans (Nat.succ_le_succ floorMonotone)
                      (Nat.succ_le_iff.mpr legal.2.1)
                  let earlier := Classical.choice
                    (persisted_broadcast_starts_earlier latchSource effects
                      authorityCountAtLeastTwo validatorInRange
                        validatorCorrectAvailable (Nat.le_succ start)
                          (Nat.lt_of_lt_of_le targetAboveStartFloor
                            targetAtMostProposal)
                            exact.1 (by rw [exact.2]))
                  exact ⟨proposal.block.reference.round,
                    targetAtMostProposal, ⟨earlier⟩⟩
              | none =>
                  let schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms
                      obligations (start + 1) validator :=
                    .recoveryArmPending goal pendingAtNext readyNext
                  have headAtNext :=
                    (arms.pendingGoalKeepsReservation (start + 1) validator goal
                      pendingAtNext).commitHeadCurrent
                  have sameHeadIndex :
                      ((timed.execution.trace (start + 1)).validatorState
                          validator).commitHead.index =
                        ((timed.execution.trace start).validatorState
                          validator).commitHead.index := by
                    rw [headAtNext, goalHead]
                  have nextBeforeFinish : start + 1 ≤ finish := by
                    have startNeFinish : start ≠ finish := by
                      intro sameTime
                      subst finish
                      exact (Nat.lt_irrefl _ headAdvanced)
                    exact Nat.succ_le_iff.mpr
                      (Nat.lt_of_le_of_ne startBeforeFinish startNeFinish)
                  have advancedFromNext :
                      ((timed.execution.trace (start + 1)).validatorState
                          validator).commitHead.index <
                        ((timed.execution.trace finish).validatorState
                          validator).commitHead.index := by
                    simpa [sameHeadIndex] using headAdvanced
                  have activeFromNext : ∀ later, start + 1 ≤ later →
                      (timed.execution.trace later).epochActive = true := by
                    intro later nextBeforeLater
                    exact active later
                      (Nat.le_trans (Nat.le_succ start) nextBeforeLater)
                  rcases commit_advance_disposes_scheduled_attempt_with_broadcast
                      continuation latchSource effects authorityCountAtLeastTwo
                        validatorInRange validatorCorrectAvailable schedule
                          nextBeforeFinish advancedFromNext activeFromNext with
                    ⟨_installTime, _nextBeforeInstall, _installBeforeFinish,
                      broadcast⟩
                  have scheduleTarget : schedule.targetRound = targetRound := by
                    change goal.targetRound = targetRound
                    simpa [targetRound] using goalTarget
                  have scheduleAboveStartFloor :
                      ((timed.execution.trace start).validatorState
                        validator).highestSignedRound < schedule.targetRound := by
                    simpa [scheduleTarget] using targetAboveStartFloor
                  have normalized := scheduled_broadcast_starts_earlier
                    latchSource effects authorityCountAtLeastTwo
                      validatorInRange validatorCorrectAvailable
                        (Nat.le_succ start) scheduleAboveStartFloor broadcast
                  simpa [scheduleTarget] using normalized
        · let schedule : ValidatorAuthorLocalPreAttemptScheduleAt arms
              obligations start validator := .recovery armed readyValue
          apply dispose schedule
          rfl
      · rcases strict with
          ⟨result, exactTarget, startBeforePersistence, _startBeforeSend⟩
        exact strict_recovery_broadcast_is_at_target latchSource effects
          authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
            targetAboveStartFloor result exactTarget startBeforePersistence

/-- Every correct, available author has one actual addressed broadcast at or
above one common requested target. The exact output rounds can differ after a
GC safe-resume branch. -/
def EveryCorrectAvailableValidatorAtOrAboveBroadcast
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (start minimumRound : Time) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations start validator
      minimumRound

/-- Common current timer inputs give one commit-tolerant broadcast from every
correct, available author. This is an unaligned internal frontier, not yet one
common DAG layer. -/
theorem common_recovery_current_inputs_eventually_produce_at_or_above_broadcasts
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (continuation : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) (arms := arms) mode)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start round : Time}
    (inputs : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ValidatorRecoveryTimerCurrentInputAt timed start validator)
    (sameTarget : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound + 1 = round)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true) :
    EveryCorrectAvailableValidatorAtOrAboveBroadcast timed obligations start
      round := by
  intro validator validatorInRange validatorCorrectAvailable
  have localResult :=
    current_recovery_input_eventually_produces_at_or_above_broadcast
    arms continuation pacing latchSource effects authorityCountAtLeastTwo
      validatorInRange validatorCorrectAvailable
        (dag_progress_other_receiver_in_range authorityCountAtLeastTwo)
          dag_progress_other_receiver_is_different
            (inputs validator validatorInRange validatorCorrectAvailable) active
  simpa [sameTarget validator validatorInRange validatorCorrectAvailable] using
    localResult

end Mysticeti
