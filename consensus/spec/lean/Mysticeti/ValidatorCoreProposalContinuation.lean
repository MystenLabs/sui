/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCoreHandlerRefinement
import Mysticeti.ValidatorNormalBlockLiveness

namespace Mysticeti

/-! Present-state continuation after one finite Core handler.

The finite handler refinement ends at the actual `try_propose(false)` call.
This module classifies only facts that already hold in that handler batch or in
the next trace state:

* the exact normal proposal action already occurred in the handler suffix;
* its exact target and parent list are ready in the next trace state; or
* durable proposal, parent-acquisition, or recovery-timer work remains.

The classification does not return a future proposal, block, packet, quorum
layer, or commit. Existing protected-work rules derive later execution from the
current-state branches.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- The exact normal target and parent list selected by one Core attempt are
ready in the current trace state. The program guard records that this concrete
attempt can run now; protection remains a separate one-host rule. -/
def ValidatorCoreNormalAttemptReadyAt
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time) (targetRound : Nat)
    (parents : List (ValidatorBlockRef BlockId)) : Prop :=
  (timed.execution.trace time).epochActive = true ∧
    ((timed.execution.trace time).validatorState
      validator).highestSignedRound < targetRound ∧
    ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator) targetRound
        parents ∧
    program.actions.enabled validator (.proposeNormal targetRound parents)
      ((timed.execution.trace time).validatorState validator)

/-- Present durable work after a normal proposal attempt returns.

Proposal obligations cover a proposal which already latched and every
persisted-but-unsent per-peer block. Parent needs cover unresolved normal or
recovery parent acquisition. The last two cases cover timer-arm work and the
exact recovery timer after the arm worker completes. -/
inductive ValidatorCoreDurableProposalRetryAt
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {recoveryWait : Time}
    (obligations : ValidatorProposalObligationExecution timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (time validator : Time) : Prop where
  | proposalWork
      (work : (obligations.trace time validator).HasWork) :
      ValidatorCoreDurableProposalRetryAt obligations needs time validator
  | parentNeed
      (need : ValidatorRecoveryParentNeed BlockId CommitId config)
      (active : (needs.trace time validator).active = some need) :
      ValidatorCoreDurableProposalRetryAt obligations needs time validator
  | timerArm
      (goal : ValidatorRecoveryTimerArmGoal BlockId CommitId)
      (pending : (arms.trace time validator).pending = some goal) :
      ValidatorCoreDurableProposalRetryAt obligations needs time validator
  | armedTimer
      (armed : ValidatorArmedExactRecoveryTimerAt timed time validator) :
      ValidatorCoreDurableProposalRetryAt obligations needs time validator

/-- A normal attempt which did not run its proposal action in the same handler
left one exact protected or durable retry state. -/
inductive ValidatorCoreProposalContinuationAt
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {recoveryWait : Time}
    (obligations : ValidatorProposalObligationExecution timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (time validator : Time) : Prop where
  | protectedNormal
      (proposal : ValidatorNormalProposalAt timed time validator) :
      ValidatorCoreProposalContinuationAt obligations needs time validator
  | durableRetry
      (retry : ValidatorCoreDurableProposalRetryAt obligations needs time
        validator) :
      ValidatorCoreProposalContinuationAt obligations needs time validator

/-- One-host source mapping for the result of the already-actual normal
proposal attempt in a finite Core handler.

The target and parent functions are deterministic projections of the actual
attempt. A successful attempt names only an event in the handler's existing
suffix. A no-op attempt names only current work at `time + 1`. -/
structure ValidatorCoreProposalAttemptContinuationRules
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {recoveryWait : Time}
    (core : ValidatorCoreHandlerRefinementRules effects)
    (obligations : ValidatorProposalObligationExecution timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait) where
  attemptTarget : ValidatorNormalProposalAttemptObservation BlockId CommitId →
    Nat
  attemptParents : ValidatorNormalProposalAttemptObservation BlockId CommitId →
    List (ValidatorBlockRef BlockId)
  /-- An actual `try_propose(false)` attempt reads its target from the
  threshold-clock round in its exact input state. -/
  forceFalseAttemptReadsThresholdClockRound : ∀ input
      (episode : ValidatorFiniteCoreHandlerEpisode effects input),
    core.handlerInputOccurs input →
    episode.proposalAttempt.force = false →
    attemptTarget episode.proposalAttempt =
      episode.proposalAttempt.input.thresholdClockRound
  forceFalseAttemptContinues : ∀ input
      (episode : ValidatorFiniteCoreHandlerEpisode effects input),
    core.handlerInputOccurs input →
    episode.proposalAttempt.force = false →
      let targetRound := attemptTarget episode.proposalAttempt
      let parents := attemptParents episode.proposalAttempt
      (.localAction input.validator (.proposeNormal targetRound parents)) ∈
          episode.afterHandlerEvents ∨
        ValidatorCoreNormalAttemptReadyAt timed (input.time + 1) input.validator
          targetRound parents ∨
        ValidatorCoreDurableProposalRetryAt obligations needs (input.time + 1)
          input.validator

namespace ValidatorCoreProposalAttemptContinuationRules

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {effects : ValidatorExactExecutionEffects faults protocolPacket network
  program timed.execution}
variable {syncRules : ValidatorBlockSyncExecutionRules timed}
variable {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
variable {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
  network program timed waits}
variable {pins : ValidatorRecoverySourcePinExecution syncRules}
variable {arms : ValidatorRecoveryTimerArmExecution timerSource}
variable {recoveryWait : Time}
variable {obligations : ValidatorProposalObligationExecution timed}
variable {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}

/-- Membership of an exact action in an event list gives its standard
occurrence witness. -/
private theorem local_action_member_gives_occurrence
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {validator : Nat} {action : ValidatorLocalAction BlockId CommitId}
    (member : (.localAction validator action) ∈ events) :
    ValidatorLocalActionOccurs events validator action := by
  induction events with
  | nil => simp at member
  | cons event events inductionHypothesis =>
      simp only [List.mem_cons] at member
      rcases member with eventExact | member
      · subst event
        exact ⟨[], events, rfl⟩
      · rcases inductionHypothesis member with
          ⟨beforeEvents, afterEvents, eventsExact⟩
        subst events
        exact ⟨event :: beforeEvents, afterEvents, rfl⟩

/-- An action in the post-attempt handler suffix is an actual action in the
already-observed execution batch. -/
theorem suffix_proposal_is_actual
    {input : ValidatorCoreHandlerInputObservation BlockId CommitId}
    {targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (episode : ValidatorFiniteCoreHandlerEpisode effects input)
    (member : (.localAction input.validator
      (.proposeNormal targetRound parents)) ∈
      episode.afterHandlerEvents) :
    ValidatorLocalActionOccurs (timed.execution.events input.time)
      input.validator
      (.proposeNormal targetRound parents) := by
  apply local_action_member_gives_occurrence
  rw [episode.eventSplit]
  simp only [List.mem_append]
  exact Or.inr member

/-- Signer-floor monotonicity through one finite event slice. -/
private theorem world_step_highest_signed_round_monotone
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {time validator : Time}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after) :
    (before.validatorState validator).highestSignedRound ≤
      (after.validatorState validator).highestSignedRound := by
  induction step with
  | nil => exact Nat.le_refl _
  | cons firstStep remainingSteps inductionHypothesis =>
      exact Nat.le_trans
        (validator_atomic_step_durable_monotone firstStep validator
          |>.2.2.2.2.2.2.1)
        inductionHypothesis

/-- An exact normal proposal action which already occurred follows the usual
latch, persistence, and addressed-broadcast pipeline.

This is a mechanical adapter for the successful same-handler branch. The only
input action is in the past execution trace; the existing protected proposal
obligation derives its persistence and sends. -/
theorem normal_proposal_occurrence_eventually_produces_exact_broadcast
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (proposalOccurs : ValidatorLocalActionOccurs
      (timed.execution.events time) validator
        (.proposeNormal targetRound parents)) :
    Nonempty { production : ValidatorNormalProposalBroadcastProduction timed
        obligations time validator targetRound //
      production.parents = parents } := by
  have validatorFacts := validator_local_action_occurrence_is_correct_available
    (timed.execution.stepsFollowRules time) proposalOccurs
  have validatorInRange := validatorFacts.1
  have validatorCorrectAvailable := validatorFacts.2
  have targetAboveStart :
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound < targetRound := by
    rcases proposalOccurs with ⟨headEvents, tailEvents, eventsExact⟩
    have batchStep := timed.execution.stepsFollowRules time
    rw [eventsExact] at batchStep
    rcases validator_world_step_append_split batchStep with
      ⟨actionBefore, prefixStep, actionAndSuffix⟩
    cases actionAndSuffix with
    | cons actionStep _suffixStep =>
        have prefixMonotone := world_step_highest_signed_round_monotone
          (validator := validator) prefixStep
        have guard := validator_atomic_local_action_has_basic_guard actionStep
        have actionFloorBelowTarget :
            (actionBefore.validatorState validator).highestSignedRound <
              targetRound := by
          simpa [BasicValidatorActionGuard] using guard.1
        exact Nat.lt_of_le_of_lt prefixMonotone actionFloorBelowTarget
  rcases effects.normalProposalEnablesPersistence time validator targetRound
      parents proposalOccurs with
    ⟨block, blockAuthor, blockRound, blockParents, persistenceEnabled⟩
  rcases latchSource.normalProposalResultIsLatched time validator targetRound
      parents block proposalOccurs blockAuthor blockRound blockParents
        persistenceEnabled with
    ⟨proposal, latched, latchedAt, proposalOrigin, proposalBlock⟩
  have ready := latch_event_sets_ready_proposal
    obligations.transitionsFollowRules latched
  have broadcasts := ready_proposal_broadcasts_to_every_other_validator effects
    validatorInRange validatorCorrectAvailable ready
  let receiver := validatorOtherReceiver validator
  have receiverInRange : receiver < config.authorityCount :=
    validator_other_receiver_in_range authorityCountAtLeastTwo
  have receiverIsOther : receiver ≠ validator :=
    validator_other_receiver_is_different
  rcases broadcasts receiver receiverInRange receiverIsOther with ⟨broadcast⟩
  have ownAtSend :=
    (timed.execution.durableStateMonotone validator
      (broadcast.persistedAt + 1) broadcast.sendActionAt validatorInRange
        broadcast.persistenceBeforeSend)
      |>.own_block_persists broadcast.ownBlockStored
  have sendGoalAtAction :
      (obligations.trace broadcast.sendActionAt validator).sendGoal
        proposal.block.reference receiver = true := by
    have reflected := obligations.sendActionIsReflected broadcast.sendActionAt
      validator receiver proposal.block.reference broadcast.sendOccurs
    have transition := obligations.transitionsFollowRules broadcast.sendActionAt
      validator
    rw [reflected] at transition
    cases transition with
    | markBlockSent required _ _ _ => exact required
  have serialized := obligations.sendGoalSerializesProposalPersistence
    broadcast.sendActionAt validator receiver proposal.block.reference
      sendGoalAtAction ownAtSend
  have floorAtFinish := serialized.2 broadcast.sendOccurs
  have ownAtFinish :=
    (timed.execution.durableStateMonotone validator
      (broadcast.persistedAt + 1) (broadcast.sendActionAt + 1)
      validatorInRange (Nat.le_trans broadcast.persistenceBeforeSend
        (Nat.le_succ _)))
      |>.own_block_persists broadcast.ownBlockStored
  have proposalAuthor : proposal.block.reference.author = validator := by
    rw [proposalBlock]
    exact blockAuthor
  have proposalRound : proposal.block.reference.round = targetRound := by
    rw [proposalBlock, blockRound]
  have proposalParents : proposal.block.parents = parents := by
    rw [proposalBlock]
    exact blockParents
  have actionBeforePersist : time + 1 ≤ broadcast.persistedAt := by
    simpa only [latchedAt] using broadcast.readyBeforePersistence
  exact ⟨⟨
    { parents
      proposalActionAt := time
      proposal
      persistedAt := broadcast.persistedAt
      finish := broadcast.sendActionAt + 1
      startBeforeProposalAction := Nat.le_refl _
      proposalActionWithinBound := Nat.le_add_right time timed.localActionBound
      proposalActionOccurs := proposalOccurs
      proposalLatched := latched
      proposalLatchedAt := latchedAt
      proposalOrigin
      proposalAuthor
      proposalRound
      proposalParents
      targetAboveStartFloor := targetAboveStart
      proposalBeforePersistence := actionBeforePersist
      persistenceBeforeFinish := Nat.le_trans
        broadcast.persistenceBeforeSend (Nat.le_succ _)
      persistenceOccurs := broadcast.persistenceOccurs
      ownBlockStoredAtFinish := ownAtFinish
      sentOwnBlockAtFinish := broadcast.sentOwnBlockRecorded
      signerFloorAtFinish := floorAtFinish
      broadcasts }, rfl⟩⟩

/-- The exact ready branch becomes protected normal work through the existing
one-host attempt-protection rule. -/
def ready_attempt_is_protected
    (protection : ValidatorReadyNormalProposalProtectionRule timed)
    {time validator : Time} {targetRound : Nat}
    {parents : List (ValidatorBlockRef BlockId)}
    (ready : ValidatorCoreNormalAttemptReadyAt timed time validator targetRound
      parents) :
    ValidatorNormalProposalAt timed time validator := by
  rcases ready with ⟨active, aboveFloor, parentsReady, enabled⟩
  exact
    { targetRound
      parents
      actionProtected := protection.readyProgramActionIsProtected time validator
        targetRound parents active aboveFloor parentsReady enabled }

/-- Project the finite Core attempt into an actual same-batch proposal, exact
protected normal work, or exact durable retry work in the next trace state. -/
theorem finite_handler_attempt_has_current_continuation
    {core : ValidatorCoreHandlerRefinementRules effects}
    (rules : ValidatorCoreProposalAttemptContinuationRules (effects := effects)
      core obligations needs)
    (protection : ValidatorReadyNormalProposalProtectionRule timed)
    {input : ValidatorCoreHandlerInputObservation BlockId CommitId}
    (inputOccurs : core.handlerInputOccurs input)
    (episode : ValidatorFiniteCoreHandlerEpisode effects input) :
    ValidatorCoreProposalContinuationAt obligations needs (input.time + 1)
      input.validator ∨
      ∃ targetRound parents,
        ValidatorLocalActionOccurs (timed.execution.events input.time)
          input.validator
          (.proposeNormal targetRound parents) := by
  have disposition := rules.forceFalseAttemptContinues input episode inputOccurs
    episode.proposalAttemptIsNormal
  dsimp only at disposition
  rcases disposition with proposed | ready | retry
  · exact Or.inr ⟨rules.attemptTarget episode.proposalAttempt,
        rules.attemptParents episode.proposalAttempt,
        suffix_proposal_is_actual episode proposed⟩
  · exact Or.inl (.protectedNormal
        (ready_attempt_is_protected protection ready))
  · exact Or.inl (.durableRetry retry)

/-- A qualifying already-actual Core input reaches the same current
continuation split after its finite commit-processing episode. -/
theorem qualifying_core_handler_input_has_current_proposal_continuation
    (core : ValidatorCoreHandlerRefinementRules effects)
    (rules : ValidatorCoreProposalAttemptContinuationRules (effects := effects)
      core obligations needs)
    (protection : ValidatorReadyNormalProposalProtectionRule timed)
    {input : ValidatorCoreHandlerInputObservation BlockId CommitId}
    (occurs : core.handlerInputOccurs input) :
    ValidatorCoreProposalContinuationAt obligations needs (input.time + 1)
      input.validator ∨
      ∃ targetRound parents,
        ValidatorLocalActionOccurs (timed.execution.events input.time)
          input.validator
          (.proposeNormal targetRound parents) := by
  rcases core.qualifyingInputHasFiniteHandler input occurs with ⟨episode⟩
  exact finite_handler_attempt_has_current_continuation rules protection occurs
    episode

/-! ## Commit-index reverse provenance

The pure DAG composition restarts one fixed author's proposal work after a
local commit. The lower continuation theorem starts from an actual
`recordCommit` action, while the recovery split observes only a durable index
increase. The next lemmas recover the concrete install action from that
increase. They use only the structural execution rules and do not add commit
synchronization as a progress source. -/

/-- Prefix one unrelated event to an already-occurring local action. -/
private theorem local_action_occurs_after_cons
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
    {validator : Nat} {action : ValidatorLocalAction BlockId CommitId}
    (occurs : ValidatorLocalActionOccurs events validator action) :
    ValidatorLocalActionOccurs (event :: events) validator action := by
  rcases occurs with ⟨beforeEvents, afterEvents, eventsExact⟩
  subst events
  exact ⟨event :: beforeEvents, afterEvents, by simp⟩

/-- An atomic increase of one validator's commit index is that validator's
local record or synchronized-commit action. -/
private theorem atomic_commit_index_advance_is_install
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
    {validator : Nat}
    (step : ValidatorAtomicStep config faults protocolPacket program time before
      event after)
    (advanced :
      (before.validatorState validator).commitHead.index <
        (after.validatorState validator).commitHead.index) :
    ∃ head,
      event = .localAction validator (.recordCommit head) ∨
        event = .localAction validator (.applySyncedCommit head) := by
  cases step with
  | localAction =>
      rename_i actingValidator action _ _ _ _ structural otherUnchanged _ _
      by_cases sameValidator : validator = actingValidator
      · subst actingValidator
        have installEffect := structural.2.2.2.2.2
        cases action with
        | recordCommit head => exact ⟨head, Or.inl rfl⟩
        | applySyncedCommit head => exact ⟨head, Or.inr rfl⟩
        | enterRecovery | requestBlock | serveBlock | acceptBlock |
            persistProposal | sendBlock | sendReplayManifest | proposeNormal |
            proposeNext | alignProposal | runCommitter | runReplayCommitter =>
            simp only [CommitInstallActionEffect] at installEffect
            rw [installEffect.1] at advanced
            omega
      · rw [otherUnchanged validator sameValidator] at advanced
        omega
  | deliverPacket =>
      rename_i _ packet _ _ _ _ _ _ _ structural otherUnchanged _ _ _
      by_cases sameValidator : validator = packet.receiver
      · subst validator
        rw [structural.2.2.1] at advanced
        omega
      · rw [otherUnchanged validator sameValidator] at advanced
        omega
  | clockTick =>
      rename_i _ _ updated
      subst after
      simp [ValidatorWorldState.updateClocks] at advanced

/-- A commit-index increase across one execution batch contains an exact local
commit-install action in that batch. -/
private theorem world_step_commit_index_advance_has_install
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {validator : Nat}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (advanced :
      (before.validatorState validator).commitHead.index <
        (after.validatorState validator).commitHead.index) :
    ∃ head, ValidatorCommitInstallOccurs events validator head := by
  induction step with
  | nil => omega
  | @cons before middle after event events firstStep remainingStep
      inductionHypothesis =>
      have firstMonotone :=
        (validator_atomic_step_durable_monotone firstStep validator).1
      by_cases firstAdvanced :
          (before.validatorState validator).commitHead.index <
            (middle.validatorState validator).commitHead.index
      · rcases atomic_commit_index_advance_is_install firstStep firstAdvanced
          with ⟨head, recorded | synchronized⟩
        · subst event
          exact ⟨head, Or.inl ⟨[], events, by simp⟩⟩
        · subst event
          exact ⟨head, Or.inr ⟨[], events, by simp⟩⟩
      · have remainingAdvanced :
            (middle.validatorState validator).commitHead.index <
              (after.validatorState validator).commitHead.index := by
          omega
        rcases inductionHypothesis remainingAdvanced with
          ⟨head, recorded | synchronized⟩
        · exact ⟨head, Or.inl (local_action_occurs_after_cons recorded)⟩
        · exact ⟨head,
            Or.inr (local_action_occurs_after_cons synchronized)⟩

/-- A durable commit-index increase across a finite time interval has one
concrete install action in an intermediate batch. -/
theorem commit_index_advance_has_install_before
    {start finish validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (ordered : start ≤ finish)
    (advanced :
      ((timed.execution.trace start).validatorState
          validator).commitHead.index <
        ((timed.execution.trace finish).validatorState
          validator).commitHead.index) :
    ∃ time head,
      start ≤ time ∧ time < finish ∧
        ValidatorCommitInstallOccurs (timed.execution.events time) validator
          head := by
  let indexAt := fun time =>
    ((timed.execution.trace time).validatorState validator).commitHead.index
  have go : ∀ offset,
      indexAt start < indexAt (start + offset) →
        ∃ time head,
          start ≤ time ∧ time < start + offset ∧
            ValidatorCommitInstallOccurs (timed.execution.events time)
              validator head := by
    intro offset
    induction offset with
    | zero => simp [indexAt]
    | succ previous inductionHypothesis =>
        intro advancedToNext
        by_cases advancedEarlier :
            indexAt start < indexAt (start + previous)
        · rcases inductionHypothesis advancedEarlier with
            ⟨time, head, startBeforeTime, timeBeforePrevious, installed⟩
          exact ⟨time, head, startBeforeTime,
            Nat.lt_trans timeBeforePrevious (by omega), installed⟩
        · have startIndexAtMostPrevious :
              indexAt start ≤ indexAt (start + previous) := by
            exact (timed.execution.durableStateMonotone validator start
              (start + previous) validatorInRange
                (Nat.le_add_right start previous)).1
          have startIndexEqualsPrevious :
              indexAt start = indexAt (start + previous) := by
            omega
          have advancedToNext' :
              indexAt start < indexAt ((start + previous) + 1) := by
            simpa [Nat.add_assoc] using advancedToNext
          have previousAdvances :
              indexAt (start + previous) <
                indexAt ((start + previous) + 1) := by
            simpa only [← startIndexEqualsPrevious] using advancedToNext'
          rcases world_step_commit_index_advance_has_install
              (timed.execution.stepsFollowRules (start + previous))
                (by simpa [indexAt] using previousAdvances) with
            ⟨head, installed⟩
          exact ⟨start + previous, head, Nat.le_add_right _ _,
            by simp, installed⟩
  have finishExact : start + (finish - start) = finish :=
    Nat.add_sub_of_le ordered
  have advancedToOffset :
      indexAt start < indexAt (start + (finish - start)) := by
    simpa [indexAt, finishExact] using advanced
  rcases go (finish - start) advancedToOffset with
    ⟨time, head, startBeforeTime, timeBeforeFinish, installed⟩
  exact ⟨time, head, startBeforeTime, by simpa [finishExact] using
    timeBeforeFinish, installed⟩

/-- If synchronized commits are absent from the interval, the concrete action
which caused a durable index increase is a same-author local `recordCommit`.

The absence premise is a trace restriction derived by the caller. It is not a
commit-synchronization liveness assumption. -/
theorem commit_index_advance_without_sync_has_record_commit_before
    {start finish validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (ordered : start ≤ finish)
    (advanced :
      ((timed.execution.trace start).validatorState
          validator).commitHead.index <
        ((timed.execution.trace finish).validatorState
          validator).commitHead.index)
    (noSyncedCommit : ∀ time head,
      start ≤ time → time < finish →
      ¬ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.applySyncedCommit head)) :
    ∃ time head,
      start ≤ time ∧ time < finish ∧
        ValidatorLocalActionOccurs (timed.execution.events time) validator
          (.recordCommit head) := by
  rcases commit_index_advance_has_install_before validatorInRange ordered
      advanced with
    ⟨time, head, startBeforeTime, timeBeforeFinish,
      recorded | synchronized⟩
  · exact ⟨time, head, startBeforeTime, timeBeforeFinish, recorded⟩
  · exact False.elim
      (noSyncedCommit time head startBeforeTime timeBeforeFinish synchronized)

end ValidatorCoreProposalAttemptContinuationRules

end Mysticeti
