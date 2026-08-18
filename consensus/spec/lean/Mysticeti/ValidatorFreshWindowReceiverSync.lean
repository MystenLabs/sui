/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorFreshTimerReadyBridge
import Mysticeti.ValidatorFreshRoundPinnedSyncSource
import Mysticeti.ValidatorReceiverRelativeCausalBacklog

namespace Mysticeti

/-! Quantitative receiver synchronization for one actual fresh round.

The stable-window record keeps a finite completion time, but it does not bound
that time relative to the actual round timers. This module instead selects the
actual fresh productions, takes their finite latest timer start, and resolves
each exact peer broadcast within its GC-relative linear causal-backlog bound.

The projected target capsule is tied to the exact durable pin created by the
actual persistence action. A post-GST delivery then derives the active
parent-sync source. The receiver's current GC round supplies the causal cutoff.
No field states a future acceptance, timer, layer, or commit result.
-/

/-- One actual fresh production is selected for every correct, available
author in an already-derived exact round family. -/
structure ValidatorSelectedFreshTimerPacedRoundFamily
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
    (observation round : Time) where
  selectedAt : ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ValidatorFreshTimerPacedExactRoundProduction timed obligations waits
      observation author round

/-- An exact fresh family has one proof-selected production at every correct,
available author. -/
theorem fresh_timer_paced_exact_round_selects_family
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
    {observation round : Time}
    (family : EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed
      obligations waits observation round) :
    Nonempty (ValidatorSelectedFreshTimerPacedRoundFamily
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round) := by
  classical
  exact ⟨{
    selectedAt := fun author authorInRange authorCorrect =>
      Classical.choice (family author authorInRange authorCorrect) }⟩

/-- A finite maximum is either its base value or one of the indexed values. -/
private theorem validator_timer_start_maximum_up_to_base_or_attained
    (base : Nat) (value : Nat → Nat) (count : Nat) :
    validatorTimerStartMaximumUpTo base value count = base ∨
      ∃ index, index < count ∧
        value index = validatorTimerStartMaximumUpTo base value count := by
  induction count with
  | zero =>
      exact Or.inl rfl
  | succ previous inductionHypothesis =>
      by_cases previousLeLast :
          validatorTimerStartMaximumUpTo base value previous ≤ value previous
      · right
        refine ⟨previous, by omega, ?_⟩
        simp only [validatorTimerStartMaximumUpTo,
          Nat.max_eq_right previousLeLast]
      · have lastLePrevious : value previous ≤
            validatorTimerStartMaximumUpTo base value previous := by
          omega
        rw [validatorTimerStartMaximumUpTo,
          Nat.max_eq_left lastLePrevious]
        rcases inductionHypothesis with baseValue | attained
        · exact Or.inl baseValue
        · rcases attained with ⟨index, indexInRange, indexAtMaximum⟩
          exact Or.inr ⟨index, by omega, indexAtMaximum⟩

/-- Selected actual timers together with their finite, attained latest start. -/
structure ValidatorSelectedFreshRoundTimerStartEnvelope
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
    (observation round : Time) where
  family : ValidatorSelectedFreshTimerPacedRoundFamily
    (timed := timed) (obligations := obligations) (waits := waits)
      observation round
  latestStart : Time
  latestAuthor : Nat
  latestAuthorInRange : latestAuthor < config.authorityCount
  latestAuthorCorrect : faults.correctAvailable latestAuthor = true
  latestStartIsSelected :
    (family.selectedAt latestAuthor latestAuthorInRange
      latestAuthorCorrect).production.timerStartedAt = latestStart
  observationBeforeLatest : observation ≤ latestStart
  startAtMostLatest : ∀ author
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true),
    (family.selectedAt author authorInRange authorCorrect).production.timerStartedAt ≤
      latestStart

/-- Finiteness and positive correct stake give one selected actual timer which
attains the latest start. No timer is created by this theorem. -/
theorem fresh_timer_paced_exact_round_selects_timer_start_envelope
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
    {observation round : Time}
    (fresh : EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed
      obligations waits observation round) :
    Nonempty (ValidatorSelectedFreshRoundTimerStartEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round) := by
  classical
  let family := Classical.choice
    (fresh_timer_paced_exact_round_selects_family fresh)
  let startFor := fun author =>
    if authorInRange : author < config.authorityCount then
      if authorCorrect : faults.correctAvailable author = true then
        (family.selectedAt author authorInRange
          authorCorrect).production.timerStartedAt
      else
        observation
    else
      observation
  let latestStart := validatorTimerStartMaximumUpTo observation startFor
    config.authorityCount
  have startAtMostLatest : ∀ author
      (authorInRange : author < config.authorityCount)
      (authorCorrect : faults.correctAvailable author = true),
      (family.selectedAt author authorInRange
          authorCorrect).production.timerStartedAt ≤ latestStart := by
    intro author authorInRange authorCorrect
    have bounded := validator_timer_start_le_maximum_up_to observation startFor
      authorInRange
    simpa [latestStart, startFor, authorInRange, authorCorrect] using bounded
  have positiveCorrectWeight : 0 <
      weight config.authorityCount config.stake faults.correctAvailable :=
    Nat.lt_of_lt_of_le config.thresholds.quorum_positive
      faults.correct_available_stake_is_quorum
  rcases positive_weight_has_member positiveCorrectWeight with
    ⟨someAuthor, someAuthorInRange, someAuthorCorrect, _positiveStake⟩
  let someSelected := family.selectedAt someAuthor someAuthorInRange
    someAuthorCorrect
  have observationBeforeLatest : observation < latestStart :=
    Nat.lt_of_lt_of_le someSelected.timerAfterObservation
      (startAtMostLatest someAuthor someAuthorInRange someAuthorCorrect)
  rcases validator_timer_start_maximum_up_to_base_or_attained observation
      startFor config.authorityCount with baseValue | attained
  · have latestIsObservation : latestStart = observation := baseValue
    have impossible : observation < observation := by
      calc
        observation < latestStart := observationBeforeLatest
        _ = observation := latestIsObservation
    exact (Nat.lt_irrefl observation impossible).elim
  · rcases attained with
      ⟨latestAuthor, latestAuthorInRange, latestAuthorAtMaximum⟩
    have latestAuthorCorrect :
        faults.correctAvailable latestAuthor = true := by
      by_cases isCorrect : faults.correctAvailable latestAuthor = true
      · exact isCorrect
      · have startAtObservation : startFor latestAuthor = observation := by
          simp [startFor, latestAuthorInRange, isCorrect]
        have latestIsObservation : latestStart = observation := by
          rw [startAtObservation] at latestAuthorAtMaximum
          exact latestAuthorAtMaximum.symm
        have impossible : observation < observation := by
          calc
            observation < latestStart := observationBeforeLatest
            _ = observation := latestIsObservation
        exact (Nat.lt_irrefl observation impossible).elim
    have latestStartIsSelected :
        (family.selectedAt latestAuthor latestAuthorInRange
          latestAuthorCorrect).production.timerStartedAt = latestStart := by
      simpa [startFor, latestAuthorInRange, latestAuthorCorrect] using
        latestAuthorAtMaximum
    exact ⟨{
      family
      latestStart
      latestAuthor
      latestAuthorInRange
      latestAuthorCorrect
      latestStartIsSelected
      observationBeforeLatest := Nat.le_of_lt observationBeforeLatest
      startAtMostLatest }⟩

/-- One actual timer-paced delivery resolves its complete receiver-relative
causal backlog and accepts the target block within the resulting linear
bound. -/
theorem timer_paced_peer_broadcast_resolves_within_linear_backlog
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    {author receiver round floor : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt)
    (syncSource : ValidatorBlockParentSyncSource syncRules
      production.snapshot.block receiver author
        (admission.capsuleFor production.snapshot.block).history
          (broadcast.packet.deliveredAt + 1))
    (cutoff : ValidatorAcceptedCausalCapsuleRoundCutoffAt timed
      (admission.capsuleFor production.snapshot.block)
        (broadcast.packet.deliveredAt + 1) receiver floor)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (active : ∀ time, broadcast.packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (aboveGc : ∀ time, broadcast.packet.deliveredAt + 1 ≤ time →
      ((timed.execution.trace time).validatorState receiver).gcRound < round) :
    ∃ acceptedAt,
      broadcast.packet.deliveredAt + 1 ≤ acceptedAt ∧
        acceptedAt ≤ broadcast.packet.deliveredAt + 1 +
            ((production.snapshot.block.reference.round - floor) *
                maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1 ∧
        ((timed.execution.trace acceptedAt).validatorState receiver).accepted
          production.snapshot.block.reference = true := by
  have authorInRange : author < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have authorCorrect : faults.correctAvailable author = true := by
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  have deliveryFacts := timer_paced_peer_broadcast_is_delivered production
    broadcast receiverInRange receiverCorrect sentAfterGst
  have syncAfterGst : network.gst ≤ broadcast.packet.deliveredAt + 1 :=
    Nat.le_trans sentAfterGst
      (Nat.le_trans deliveryFacts.1 (Nat.le_add_right _ 1))
  rcases admission.history_ready_within_linear_backlog syncSource authorInRange
      authorCorrect production.persistenceOccurs cutoff receiverInRange
        receiverCorrect syncAfterGst active with
    ⟨parentsReadyAt, deliveryBeforeParentsReady, parentsReadyBound,
      historyReady⟩
  have parentsReady : ∀ parent,
      parent ∈ production.snapshot.block.parents →
      ((timed.execution.trace parentsReadyAt).validatorState receiver).accepted
            parent = true ∨
        parent.round ≤
          ((timed.execution.trace parentsReadyAt).validatorState
            receiver).gcRound := by
    intro parent parentMember
    rcases syncSource.coversDirectParents parent parentMember with
      acceptedAtStart | ⟨parentBlock, parentMember, parentReference⟩
    · exact Or.inl (timed.execution.accepted_block_persists receiverInRange
        deliveryBeforeParentsReady acceptedAtStart)
    · rw [← parentReference]
      exact historyReady parentBlock parentMember
  have packetAtDelivery := timed.execution.packetHistoryMonotone
    broadcast.packet.sentAt broadcast.packet.deliveredAt deliveryFacts.1
      broadcast.packetId broadcast.packet broadcast.packetInTrace
  rcases delivered_block_with_current_gc_ready_parents_is_accepted timed
      acceptance packetAtDelivery broadcast.packetPayload deliveryFacts.2.2
      (by simpa [broadcast.packetReceiver] using receiverInRange)
      (by simpa [broadcast.packetReceiver] using receiverCorrect)
      (by simpa [production.snapshot.blockIsOwnProposal,
        production.proposer] using authorInRange)
      (Or.inr production.validParents) deliveryBeforeParentsReady
      (by
        simpa [broadcast.packetReceiver, production.blockRound] using
          aboveGc parentsReadyAt deliveryBeforeParentsReady)
      (by
        intro parent parentMember
        simpa [broadcast.packetReceiver] using parentsReady parent parentMember)
    with ⟨acceptedAt, parentsBeforeAccepted, acceptedWithinBound, accepted⟩
  refine ⟨acceptedAt,
    Nat.le_trans deliveryBeforeParentsReady parentsBeforeAccepted, ?_,
      by simpa [broadcast.packetReceiver] using accepted⟩
  have shifted := Nat.add_le_add_right parentsReadyBound
    (timed.localActionBound + 1)
  exact Nat.le_trans acceptedWithinBound (by
    simpa only [Nat.add_assoc] using shifted)

/-- Common deadline for one selected fresh round at every correct receiver. -/
def validatorFreshRoundReceiverReadyDeadline
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (latestStart : Time) (prior : ValidatorCommitHead CommitId)
    (round maxAdmittedRefsPerRound : Nat) : Time :=
  latestStart + waits.wait prior round +
      3 * (timed.localActionBound + 1) + network.delta + 1 +
    ((round - timed.execution.gcRoundForCommitHead prior) *
        maxAdmittedRefsPerRound) *
      validatorBlockSyncAcceptanceBound timed syncRules +
    timed.localActionBound + 1

/-- The receiver synchronization, persistence, and timer-arm cost after the
latest actual prior-round timer. -/
def validatorFreshRoundReceiverTimerStepCost
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (prior : ValidatorCommitHead CommitId)
    (round maxAdmittedRefsPerRound : Nat) : Time :=
  3 * (timed.localActionBound + 1) + network.delta + 1 +
    ((round - timed.execution.gcRoundForCommitHead prior) *
        maxAdmittedRefsPerRound) *
      validatorBlockSyncAcceptanceBound timed syncRules +
    timed.localActionBound + 1 + timed.localActionBound + 2

/-- Correct, available authors are not Byzantine during the fixed interval. -/
private theorem fresh_sync_correct_available_not_byzantine
    {CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {validator : Nat}
    (correct : faults.correctAvailable validator = true) :
    faults.byzantine validator = false := by
  have notNonProgress : faults.nonProgress validator = false := by
    simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
      VoterSet.full] using correct
  have separated : faults.byzantine validator = false ∧
      faults.unavailable validator = false := by
    simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
      notNonProgress
  exact separated.1

/-- One selected exact block is a stable accepted and retained representative
at one correct receiver from the common quantitative deadline onward. -/
theorem selected_fresh_round_block_is_stable_by_common_deadline
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Time}
    (envelope : ValidatorSelectedFreshRoundTimerStartEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round)
    {prior : ValidatorCommitHead CommitId}
    (headsAtObservation : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).commitHead = prior)
    (priorBeforeRound : prior.round < round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation)
    {author receiver : Nat}
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true) :
    ∀ later,
      validatorFreshRoundReceiverReadyDeadline (syncRules := syncRules) timed
          waits envelope.latestStart prior round maxAdmittedRefsPerRound ≤
        later →
        ((timed.execution.trace later).validatorState receiver
            ).acceptedRepresentative round author =
              some (envelope.family.selectedAt author authorInRange
                authorCorrect).production.snapshot.block.reference ∧
          ((timed.execution.trace later).validatorState receiver).retained
              (envelope.family.selectedAt author authorInRange
                authorCorrect).production.snapshot.block.reference = true := by
  let fresh := envelope.family.selectedAt author authorInRange authorCorrect
  let production := fresh.production
  let deadline := validatorFreshRoundReceiverReadyDeadline
    (syncRules := syncRules) timed waits envelope.latestStart prior round
      maxAdmittedRefsPerRound
  have observationBeforeTimer : observation ≤ production.timerStartedAt :=
    Nat.le_of_lt fresh.timerAfterObservation
  have headAtTimer :
      ((timed.execution.trace production.timerStartedAt).validatorState
        author).commitHead = prior :=
    (no_commit_advance_keeps_correct_commit_head authorInRange authorCorrect
      observationBeforeTimer noAdvance).trans
        (headsAtObservation author authorInRange authorCorrect)
  have productionHead : production.commitHead = prior :=
    (originRules.production_head_at_timer_start production).trans headAtTimer
  have roundAboveReceiverGcAtObservation :
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round := by
    have gcAtMostHead := correct_validator_gc_round_at_most_commit_round
      (time := observation) timed.execution receiverInRange receiverCorrect
    rw [headsAtObservation receiver receiverInRange receiverCorrect] at gcAtMostHead
    omega
  have deadlineAfterObservation : observation ≤ deadline := by
    apply Nat.le_trans envelope.observationBeforeLatest
    dsimp [deadline, validatorFreshRoundReceiverReadyDeadline]
    have enlarged := Nat.le_add_right envelope.latestStart
      (waits.wait prior round + 3 * (timed.localActionBound + 1) +
        network.delta + 1 +
        ((round - timed.execution.gcRoundForCommitHead prior) *
            maxAdmittedRefsPerRound) *
          validatorBlockSyncAcceptanceBound timed syncRules +
        timed.localActionBound + 1)
    simpa only [Nat.add_assoc] using enlarged
  have stableGc : ∀ time, observation ≤ time →
      ((timed.execution.trace time).validatorState receiver).gcRound =
        ((timed.execution.trace observation).validatorState receiver).gcRound :=
    fun time ordered => no_commit_advance_keeps_correct_gc_round
      receiverInRange receiverCorrect ordered noAdvance
  have aboveGc : ∀ time, observation ≤ time →
      ((timed.execution.trace time).validatorState receiver).gcRound < round := by
    intro time ordered
    rw [stableGc time ordered]
    exact roundAboveReceiverGcAtObservation
  have authorNotByzantine : faults.byzantine author = false :=
    fresh_sync_correct_available_not_byzantine authorCorrect
  intro later deadlineBeforeLater
  by_cases receiverIsAuthor : receiver = author
  · subst receiver
    have storedBeforeDeadline : production.snapshot.storedAt ≤ deadline := by
      have stored := production.storedWithinPipelinePrefix
      rw [productionHead] at stored
      have timerBeforeLatest : production.timerStartedAt ≤
          envelope.latestStart := by
        simpa [production, fresh] using
          envelope.startAtMostLatest author authorInRange authorCorrect
      have timerPipelineBeforeLatest :
          production.timerStartedAt + waits.wait prior round +
              2 * (timed.localActionBound + 1) ≤
            envelope.latestStart + waits.wait prior round +
              2 * (timed.localActionBound + 1) := by
        have shifted := Nat.add_le_add_right timerBeforeLatest
          (waits.wait prior round + 2 * (timed.localActionBound + 1))
        simpa only [Nat.add_assoc] using shifted
      have latestPipeline :
          production.snapshot.storedAt ≤
            envelope.latestStart + waits.wait prior round +
              3 * (timed.localActionBound + 1) := by
        exact Nat.le_trans (Nat.le_trans stored timerPipelineBeforeLatest)
          (by omega)
      apply Nat.le_trans latestPipeline
      dsimp [deadline, validatorFreshRoundReceiverReadyDeadline]
      have enlarged := Nat.le_add_right
        (envelope.latestStart + waits.wait prior round +
          3 * (timed.localActionBound + 1))
        (network.delta + 1 +
          ((round - timed.execution.gcRoundForCommitHead prior) *
              maxAdmittedRefsPerRound) *
            validatorBlockSyncAcceptanceBound timed syncRules +
          timed.localActionBound + 1)
      simpa only [Nat.add_assoc] using enlarged
    have ownAtStored :
        ((timed.execution.trace production.snapshot.storedAt).validatorState
          author).ownBlockAt round =
            some production.snapshot.block.reference := by
      simpa [production.proposer, production.blockRound] using
        production.snapshot.blockStored
    have ownAtLater :
        ((timed.execution.trace later).validatorState author).ownBlockAt round =
          some production.snapshot.block.reference :=
      (timed.execution.durable_fields_persist authorInRange
        (Nat.le_trans storedBeforeDeadline deadlineBeforeLater)
        ).own_block_persists ownAtStored
    have ownFacts :=
      (timed.execution.statesWellFormed later author authorInRange
        ).ownBlockIsSound round production.snapshot.block.reference ownAtLater
    have recorded := representatives.acceptedCorrectReferenceIsRecorded later
      author production.snapshot.block.reference authorInRange authorCorrect
      (by simpa [production.snapshot.blockIsOwnProposal,
        production.proposer] using authorInRange)
      (by simpa [production.snapshot.blockIsOwnProposal,
        production.proposer] using authorNotByzantine)
      ownFacts.2.2.1
    exact ⟨by simpa [production.blockRound,
      production.snapshot.blockIsOwnProposal, production.proposer] using
        recorded, ownFacts.2.2.2.1⟩
  · let broadcast := Classical.choice
      (production.peerBroadcast receiver receiverInRange (by
        intro receiverEqualsAuthor
        exact receiverIsAuthor receiverEqualsAuthor))
    have sentAfterGst : network.gst ≤ broadcast.packet.sentAt := by
      have timerBeforeProposal : production.timerStartedAt ≤
          production.proposalActionAt := by
        exact Nat.le_trans (Nat.le_add_right _ _)
          production.deadlineBeforeProposal
      exact Nat.le_trans afterGst
        (Nat.le_trans observationBeforeTimer
          (Nat.le_trans timerBeforeProposal
            (Nat.le_trans (Nat.le_add_right _ 1)
              broadcast.proposalBeforeSend)))
    have activeFromDelivery : ∀ time,
        broadcast.packet.deliveredAt + 1 ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time deliveryBeforeTime
      apply active time
      have observationBeforeSend : observation ≤ broadcast.packet.sentAt := by
        exact Nat.le_trans observationBeforeTimer
          (Nat.le_trans (Nat.le_add_right _ _)
            (Nat.le_trans production.deadlineBeforeProposal
              (Nat.le_trans (Nat.le_add_right _ 1)
                broadcast.proposalBeforeSend)))
      have deliveryFacts := network.postGstDelivery broadcast.packet
        broadcast.packetIsProtocol
        (by simpa [broadcast.packetSender] using authorInRange)
        (by simpa [broadcast.packetReceiver] using receiverInRange)
        (by simpa [broadcast.packetSender] using authorCorrect)
        (by simpa [broadcast.packetReceiver] using receiverCorrect)
        sentAfterGst
      exact Nat.le_trans observationBeforeSend
        (Nat.le_trans deliveryFacts.1
          (Nat.le_trans (Nat.le_add_right _ 1) deliveryBeforeTime))
    have observationBeforePersistence : observation ≤
        production.persistTime + 1 := by
      exact Nat.le_trans observationBeforeTimer
        (Nat.le_trans (Nat.le_add_right _ _)
          (Nat.le_trans production.deadlineBeforeProposal
            (Nat.le_trans (Nat.le_add_right _ 1)
              (Nat.le_trans production.proposalBeforePersistence
                (Nat.le_add_right _ 1)))))
    have activeFromPersistence : ∀ time,
        production.persistTime + 1 ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time persistenceBeforeTime
      exact active time
        (Nat.le_trans observationBeforePersistence persistenceBeforeTime)
    let syncSource := sourceRules.sourceFor production broadcast receiverInRange
      receiverCorrect sentAfterGst activeFromPersistence
    have deliveryFacts := timer_paced_peer_broadcast_is_delivered production
      broadcast receiverInRange receiverCorrect sentAfterGst
    have observationBeforeDelivery : observation ≤
        broadcast.packet.deliveredAt + 1 := by
      exact Nat.le_trans observationBeforeTimer
        (Nat.le_trans (Nat.le_add_right _ _)
          (Nat.le_trans production.deadlineBeforeProposal
            (Nat.le_trans (Nat.le_add_right _ 1)
              (Nat.le_trans broadcast.proposalBeforeSend
                (Nat.le_trans deliveryFacts.1 (Nat.le_add_right _ 1))))))
    have headAtDelivery := no_commit_advance_keeps_correct_commit_head
      receiverInRange receiverCorrect observationBeforeDelivery noAdvance
    have gcAtDelivery :
        ((timed.execution.trace
          (broadcast.packet.deliveredAt + 1)).validatorState receiver).gcRound =
          timed.execution.gcRoundForCommitHead prior := by
      rw [timed.execution.correctGcRoundMatchesCommitHead
        (broadcast.packet.deliveredAt + 1) receiver receiverInRange
          receiverCorrect, headAtDelivery,
        headsAtObservation receiver receiverInRange receiverCorrect]
    have cutoff : ValidatorAcceptedCausalCapsuleRoundCutoffAt timed
        (admission.capsuleFor production.snapshot.block)
          (broadcast.packet.deliveredAt + 1) receiver
            (timed.execution.gcRoundForCommitHead prior) := by
      rw [← gcAtDelivery]
      exact validator_gc_round_gives_causal_capsule_round_cutoff
    rcases timer_paced_peer_broadcast_resolves_within_linear_backlog admission
        acceptance production broadcast syncSource cutoff receiverInRange
          receiverCorrect sentAfterGst activeFromDelivery (by
            intro time deliveryBeforeTime
            exact aboveGc time
              (Nat.le_trans observationBeforeDelivery deliveryBeforeTime)) with
      ⟨acceptedAt, deliveryBeforeAccepted, acceptedBound, accepted⟩
    have observationBeforeAccepted : observation ≤ acceptedAt :=
      Nat.le_trans observationBeforeDelivery deliveryBeforeAccepted
    have acceptedBeforeDeadline : acceptedAt ≤ deadline := by
      have sentBound := timer_paced_peer_broadcast_sent_within_round_pipeline
        production broadcast
      rw [productionHead] at sentBound
      have timerBeforeLatest : production.timerStartedAt ≤
          envelope.latestStart := by
        simpa [production, fresh] using
          envelope.startAtMostLatest author authorInRange authorCorrect
      have deliveredBound := deliveryFacts.2.1
      rw [production.blockRound] at acceptedBound
      let backlog := ((round - timed.execution.gcRoundForCommitHead prior) *
          maxAdmittedRefsPerRound) *
        validatorBlockSyncAcceptanceBound timed syncRules
      have acceptedAfterDelivery : acceptedAt ≤
          broadcast.packet.deliveredAt + 1 + backlog +
            timed.localActionBound + 1 := by
        simpa [backlog, Nat.add_assoc] using acceptedBound
      have deliveredToSent :
          broadcast.packet.deliveredAt + 1 + backlog +
                timed.localActionBound + 1 ≤
            broadcast.packet.sentAt + network.delta + 1 + backlog +
                timed.localActionBound + 1 := by
        have shifted := Nat.add_le_add_right deliveredBound
          (1 + backlog + timed.localActionBound + 1)
        simpa only [Nat.add_assoc] using shifted
      have sentToTimer :
          broadcast.packet.sentAt + network.delta + 1 + backlog +
                timed.localActionBound + 1 ≤
            production.timerStartedAt + waits.wait prior round +
                3 * (timed.localActionBound + 1) + network.delta + 1 +
                  backlog + timed.localActionBound + 1 := by
        have shifted := Nat.add_le_add_right sentBound
          (network.delta + 1 + backlog + timed.localActionBound + 1)
        simpa only [Nat.add_assoc] using shifted
      have timerToLatest :
          production.timerStartedAt + waits.wait prior round +
                3 * (timed.localActionBound + 1) + network.delta + 1 +
                  backlog + timed.localActionBound + 1 ≤
            envelope.latestStart + waits.wait prior round +
                3 * (timed.localActionBound + 1) + network.delta + 1 +
                  backlog + timed.localActionBound + 1 := by
        have shifted := Nat.add_le_add_right timerBeforeLatest
          (waits.wait prior round + 3 * (timed.localActionBound + 1) +
            network.delta + 1 + backlog + timed.localActionBound + 1)
        simpa only [Nat.add_assoc] using shifted
      exact Nat.le_trans acceptedAfterDelivery
        (Nat.le_trans deliveredToSent
          (Nat.le_trans sentToTimer (Nat.le_trans timerToLatest (by
            dsimp [deadline, validatorFreshRoundReceiverReadyDeadline]
            exact Nat.le_refl _))))
    have acceptedLater := timed.execution.accepted_block_persists
      receiverInRange (Nat.le_trans acceptedBeforeDeadline deadlineBeforeLater)
        accepted
    have packetAtDelivery := timed.execution.packetHistoryMonotone
      broadcast.packet.sentAt broadcast.packet.deliveredAt deliveryFacts.1
        broadcast.packetId broadcast.packet broadcast.packetInTrace
    have localBody : ValidatorLocalBlockBodyAt timed
        broadcast.packet.deliveredAt receiver production.snapshot.block :=
      .delivered broadcast.packetId broadcast.packet packetAtDelivery
        broadcast.packetIsProtocol broadcast.packetReceiver
        broadcast.packetPayload deliveryFacts.2.2
    have pinAtDelivery := sync.local_body_creates_durable_pin receiverInRange
      receiverCorrect
      (active (broadcast.packet.deliveredAt + 1) observationBeforeDelivery)
      (by
        rw [production.blockRound]
        exact aboveGc (broadcast.packet.deliveredAt + 1)
          observationBeforeDelivery)
      localBody
    have deliveryBeforeLater : broadcast.packet.deliveredAt + 1 ≤ later :=
      Nat.le_trans deliveryBeforeAccepted
        (Nat.le_trans acceptedBeforeDeadline deadlineBeforeLater)
    have pinLater := sync.body_pin_persists_while_head_is_current
      receiverInRange receiverCorrect deliveryBeforeLater pinAtDelivery.1
      (by
        intro time deliveryBeforeTime _timeBeforeLater
        exact active time
          (Nat.le_trans observationBeforeDelivery deliveryBeforeTime))
      (by
        intro time deliveryBeforeTime _timeBeforeLater
        rw [production.blockRound]
        exact aboveGc time
          (Nat.le_trans observationBeforeDelivery deliveryBeforeTime))
      (by
        intro time deliveryBeforeTime _timeBeforeLater
        have headAtTime := no_commit_advance_keeps_correct_commit_head
          receiverInRange receiverCorrect
            (Nat.le_trans observationBeforeDelivery deliveryBeforeTime)
              noAdvance
        have headAtDelivery := no_commit_advance_keeps_correct_commit_head
          receiverInRange receiverCorrect observationBeforeDelivery noAdvance
        rw [headAtTime, headAtDelivery]
        exact Nat.le_refl _)
    have retainedLater := sync.acceptedPinnedBodyIsRetained later receiver
      production.snapshot.block.reference _ receiverInRange receiverCorrect
      pinLater (by
        rw [production.blockRound]
        exact aboveGc later
          (Nat.le_trans deadlineAfterObservation deadlineBeforeLater))
      acceptedLater
    have recorded := representatives.acceptedCorrectReferenceIsRecorded later
      receiver production.snapshot.block.reference receiverInRange
      receiverCorrect
      (by simpa [production.snapshot.blockIsOwnProposal,
        production.proposer] using authorInRange)
      (by simpa [production.snapshot.blockIsOwnProposal,
        production.proposer] using authorNotByzantine)
      acceptedLater
    exact ⟨by simpa [production.blockRound,
      production.snapshot.blockIsOwnProposal, production.proposer] using
        recorded, retainedLater⟩

/-- One selected fresh round gives the exact receiver source-window record at
the common quantitative deadline.

The embedded source window has length zero. Its sole source is the actual
persisted fresh production for this round. The quantitative synchronization
above supplies durable own blocks and stable accepted representatives at all
correct hosts by the same deadline. -/
theorem selected_fresh_round_gives_quantitative_receiver_source_window
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {maxAdmittedRefsPerRound recoveryWait : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Time}
    (envelope : ValidatorSelectedFreshRoundTimerStartEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round)
    {prior : ValidatorCommitHead CommitId}
    (headsAtObservation : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).commitHead = prior)
    (priorBeforeRound : prior.round < round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation)
    {receiver : Nat}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true) :
    ∃ window : ValidatorReceiverExactRecoverySourceWindow timed obligations
        pins recoveryWait observation round 0 receiver,
      window.finish = validatorFreshRoundReceiverReadyDeadline
        (syncRules := syncRules) timed waits envelope.latestStart prior round
          maxAdmittedRefsPerRound := by
  let deadline := validatorFreshRoundReceiverReadyDeadline
    (syncRules := syncRules) timed waits envelope.latestStart prior round
      maxAdmittedRefsPerRound
  have observationBeforeDeadline : observation ≤ deadline := by
    apply Nat.le_trans envelope.observationBeforeLatest
    dsimp [deadline, validatorFreshRoundReceiverReadyDeadline]
    have enlarged := Nat.le_add_right envelope.latestStart
      (waits.wait prior round + 3 * (timed.localActionBound + 1) +
        network.delta + 1 +
        ((round - timed.execution.gcRoundForCommitHead prior) *
            maxAdmittedRefsPerRound) *
          validatorBlockSyncAcceptanceBound timed syncRules +
        timed.localActionBound + 1)
    simpa only [Nat.add_assoc] using enlarged
  have ownedAtDeadline : EveryCorrectAvailableValidatorOwnBlockAt faults
      (timed.execution.trace deadline) round := by
    intro author authorInRange authorCorrect
    let fresh := envelope.family.selectedAt author authorInRange authorCorrect
    let production := fresh.production
    have observationBeforeTimer : observation ≤
        production.timerStartedAt := Nat.le_of_lt fresh.timerAfterObservation
    have headAtTimer :
        ((timed.execution.trace production.timerStartedAt).validatorState
          author).commitHead = prior :=
      (no_commit_advance_keeps_correct_commit_head authorInRange authorCorrect
        observationBeforeTimer noAdvance).trans
          (headsAtObservation author authorInRange authorCorrect)
    have productionHead : production.commitHead = prior :=
      (originRules.production_head_at_timer_start production).trans headAtTimer
    have timerBeforeLatest : production.timerStartedAt ≤
        envelope.latestStart := by
      simpa [production, fresh] using
        envelope.startAtMostLatest author authorInRange authorCorrect
    have stored := production.storedWithinPipelinePrefix
    rw [productionHead] at stored
    have timerPipelineBeforeLatest :
        production.timerStartedAt + waits.wait prior round +
              2 * (timed.localActionBound + 1) ≤
          envelope.latestStart + waits.wait prior round +
              2 * (timed.localActionBound + 1) := by
      have shifted := Nat.add_le_add_right timerBeforeLatest
        (waits.wait prior round + 2 * (timed.localActionBound + 1))
      simpa only [Nat.add_assoc] using shifted
    have storedBeforeDeadline : production.snapshot.storedAt ≤ deadline := by
      apply Nat.le_trans (Nat.le_trans stored timerPipelineBeforeLatest)
      dsimp [deadline, validatorFreshRoundReceiverReadyDeadline]
      have firstIncrease :
          envelope.latestStart + waits.wait prior round +
                2 * (timed.localActionBound + 1) ≤
            envelope.latestStart + waits.wait prior round +
                3 * (timed.localActionBound + 1) := by
        exact Nat.add_le_add_left
          (Nat.mul_le_mul_right (timed.localActionBound + 1) (by omega)) _
      apply Nat.le_trans firstIncrease
      have enlarged := Nat.le_add_right
        (envelope.latestStart + waits.wait prior round +
          3 * (timed.localActionBound + 1))
        (network.delta + 1 +
          ((round - timed.execution.gcRoundForCommitHead prior) *
              maxAdmittedRefsPerRound) *
            validatorBlockSyncAcceptanceBound timed syncRules +
          timed.localActionBound + 1)
      simpa only [Nat.add_assoc] using enlarged
    have ownAtStored :
        ((timed.execution.trace production.snapshot.storedAt).validatorState
          author).ownBlockAt round =
            some production.snapshot.block.reference := by
      simpa [production.proposer, production.blockRound] using
        production.snapshot.blockStored
    have ownAtDeadline :=
      (timed.execution.durable_fields_persist authorInRange storedBeforeDeadline
        ).own_block_persists ownAtStored
    simp [ownAtDeadline]
  have stableFromDeadline :
      EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
        timed.execution.trace deadline round := by
    intro later deadlineBeforeLater observer author observerInRange
      observerCorrect authorInRange authorCorrect
    have stable := selected_fresh_round_block_is_stable_by_common_deadline
      admission pins sourceRules acceptance sync representatives originRules envelope
        headsAtObservation priorBeforeRound afterGst active noAdvance
          authorInRange authorCorrect observerInRange observerCorrect later
            deadlineBeforeLater
    exact ⟨_, stable⟩
  let sourceWindow : ValidatorExactRecoveryRoundSourceWindow timed obligations
      pins recoveryWait observation round 0 := {
    finish := deadline
    snapshotBeforeFinish := observationBeforeDeadline
    owned := by simpa using ownedAtDeadline
    stable := by simpa using stableFromDeadline
    sourceAt := by
      intro offset offsetAtMostZero
      have offsetZero : offset = 0 := Nat.eq_zero_of_le_zero offsetAtMostZero
      subst offset
      intro author authorInRange authorCorrect
      let fresh := envelope.family.selectedAt author authorInRange authorCorrect
      exact ⟨.persisted
        fresh.broadcast.production (by simpa using fresh.broadcast.exactRound)⟩ }
  have gcBelowRoundAtDeadline :
      ((timed.execution.trace deadline).validatorState receiver).gcRound <
        round := by
    rw [no_commit_advance_keeps_correct_gc_round receiverInRange
      receiverCorrect observationBeforeDeadline noAdvance]
    have gcAtMostHead := correct_validator_gc_round_at_most_commit_round
      (time := observation) timed.execution receiverInRange receiverCorrect
    rw [headsAtObservation receiver receiverInRange receiverCorrect] at gcAtMostHead
    omega
  have usable := stable_common_round_gives_receiver_usable_quorum_layer
    representatives receiverInRange receiverCorrect sourceWindow.stable
      gcBelowRoundAtDeadline
  let receiverWindow : ValidatorReceiverExactRecoverySourceWindow timed
      obligations pins recoveryWait observation round 0 receiver := {
    sourceWindow
    finish := deadline
    sourceFinishBeforeFinish := Nat.le_refl _
    usable := by simpa using usable }
  exact ⟨receiverWindow, rfl⟩

/-- Quantitative synchronization of one actual fresh round gives the upper
timer-start edge for the next actual fresh round.

The prior witness is the selected author whose timer attains the latest
prior-round start. Thus, the step cost pays only the pipeline, causal backlog,
and local timer arm. It does not pay the prior-round spread again. -/
theorem selected_fresh_round_gives_timer_start_successor_upper
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {maxAdmittedRefsPerRound recoveryWait : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Time}
    (envelope : ValidatorSelectedFreshRoundTimerStartEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round)
    {prior : ValidatorCommitHead CommitId}
    (headsAtObservation : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).commitHead = prior)
    (priorBeforeRound : prior.round < round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
      observation round (waits.wait prior round)
        (validatorFreshRoundReceiverTimerStepCost (syncRules := syncRules)
          timed prior round maxAdmittedRefsPerRound) := by
  intro receiver next
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerCorrectAvailable
  let previousAtReceiver := envelope.family.selectedAt receiver
    receiverInRange receiverCorrect
  rcases selected_fresh_round_gives_quantitative_receiver_source_window
      (recoveryWait := recoveryWait) admission pins sourceRules acceptance sync
        representatives originRules
        envelope headsAtObservation priorBeforeRound afterGst active noAdvance
          receiverInRange receiverCorrect with
    ⟨window, windowFinish⟩
  have observationBeforeFinish : observation ≤ window.finish :=
    Nat.le_trans window.sourceWindow.snapshotBeforeFinish
      window.sourceFinishBeforeFinish
  have activeAtFinish :
      (timed.execution.trace window.finish).epochActive = true :=
    active window.finish observationBeforeFinish
  rcases receiver_source_window_bounds_actual_fresh_timer_or_receiver_advances
      timerSource arms originRules pins window previousAtReceiver next
        activeAtFinish with receiverAdvanced | bounded
  · rcases receiverAdvanced with
      ⟨finish, windowBeforeFinish, headAdvanced⟩
    have observationHeadAtMostWindow :=
      (timed.execution.durableStateMonotone receiver observation window.finish
        receiverInRange observationBeforeFinish).1
    exact False.elim (noAdvance ⟨receiver, finish, receiverInRange,
      receiverCorrect, Nat.le_trans observationBeforeFinish windowBeforeFinish,
      Nat.lt_of_le_of_lt observationHeadAtMostWindow headAdvanced⟩)
  · refine ⟨envelope.latestAuthor,
      envelope.family.selectedAt envelope.latestAuthor
        envelope.latestAuthorInRange envelope.latestAuthorCorrect, ?_⟩
    calc
      next.production.timerStartedAt ≤
          window.finish + timed.localActionBound + 2 := bounded.2
      _ = envelope.latestStart + waits.wait prior round +
          validatorFreshRoundReceiverTimerStepCost (syncRules := syncRules)
            timed prior round maxAdmittedRefsPerRound := by
        rw [windowFinish]
        simp only [validatorFreshRoundReceiverReadyDeadline,
          validatorFreshRoundReceiverTimerStepCost, Nat.add_assoc]
      _ = (envelope.family.selectedAt envelope.latestAuthor
            envelope.latestAuthorInRange
              envelope.latestAuthorCorrect).production.timerStartedAt +
            waits.wait prior round +
              validatorFreshRoundReceiverTimerStepCost
                (syncRules := syncRules) timed prior round
                  maxAdmittedRefsPerRound := by
        rw [envelope.latestStartIsSelected]

/-- An already-derived exact fresh family selects its actual finite timer
envelope and gives one quantitative exact source window at the requested
correct receiver. -/
theorem fresh_timer_paced_exact_round_gives_quantitative_receiver_source_window
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {maxAdmittedRefsPerRound recoveryWait : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Time}
    (fresh : EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed
      obligations waits observation round)
    {prior : ValidatorCommitHead CommitId}
    (headsAtObservation : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).commitHead = prior)
    (priorBeforeRound : prior.round < round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation)
    {receiver : Nat}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true) :
    ∃ envelope : ValidatorSelectedFreshRoundTimerStartEnvelope
        (timed := timed) (obligations := obligations) (waits := waits)
          observation round,
      ∃ window : ValidatorReceiverExactRecoverySourceWindow timed obligations
          pins recoveryWait observation round 0 receiver,
        window.finish = validatorFreshRoundReceiverReadyDeadline
          (syncRules := syncRules) timed waits envelope.latestStart prior round
            maxAdmittedRefsPerRound := by
  let envelope := Classical.choice
    (fresh_timer_paced_exact_round_selects_timer_start_envelope fresh)
  exact ⟨envelope,
    selected_fresh_round_gives_quantitative_receiver_source_window admission
      pins sourceRules acceptance sync representatives originRules envelope
        headsAtObservation priorBeforeRound afterGst active noAdvance
          receiverInRange receiverCorrect⟩

/-- Select one actual round from an already-derived finite fresh window and
apply the quantitative receiver synchronization bridge. -/
theorem fresh_timer_paced_window_offset_gives_quantitative_receiver_source_window
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {maxAdmittedRefsPerRound recoveryWait : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation baseRound count offset : Time}
    (freshWindow : ValidatorFreshTimerPacedExactRoundWindow timed obligations
      waits observation baseRound count)
    (offsetInRange : offset < count)
    {prior : ValidatorCommitHead CommitId}
    (headsAtObservation : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).commitHead = prior)
    (priorBeforeRound : prior.round < baseRound + offset + 2)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation)
    {receiver : Nat}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true) :
    ∃ envelope : ValidatorSelectedFreshRoundTimerStartEnvelope
        (timed := timed) (obligations := obligations) (waits := waits)
          observation (baseRound + offset + 2),
      ∃ window : ValidatorReceiverExactRecoverySourceWindow timed obligations
          pins recoveryWait observation (baseRound + offset + 2) 0 receiver,
        window.finish = validatorFreshRoundReceiverReadyDeadline
          (syncRules := syncRules) timed waits envelope.latestStart prior
            (baseRound + offset + 2) maxAdmittedRefsPerRound := by
  exact fresh_timer_paced_exact_round_gives_quantitative_receiver_source_window
    admission pins sourceRules acceptance sync representatives originRules
      (freshWindow.freshAt offset offsetInRange) headsAtObservation
        priorBeforeRound afterGst active noAdvance receiverInRange receiverCorrect

/-- One already-derived exact fresh family gives its next-round timer upper
edge with the concrete receiver synchronization cost. -/
theorem fresh_timer_paced_exact_round_gives_timer_start_successor_upper
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {maxAdmittedRefsPerRound recoveryWait : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation round : Time}
    (fresh : EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed
      obligations waits observation round)
    {prior : ValidatorCommitHead CommitId}
    (headsAtObservation : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).commitHead = prior)
    (priorBeforeRound : prior.round < round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
      observation round (waits.wait prior round)
        (validatorFreshRoundReceiverTimerStepCost (syncRules := syncRules)
          timed prior round maxAdmittedRefsPerRound) := by
  let envelope := Classical.choice
    (fresh_timer_paced_exact_round_selects_timer_start_envelope fresh)
  unfold ValidatorFreshTimerStartSuccessorUpperAt
  intro receiver next
  exact selected_fresh_round_gives_timer_start_successor_upper
    (recoveryWait := recoveryWait) admission pins sourceRules acceptance sync
      representatives arms originRules envelope headsAtObservation
        priorBeforeRound afterGst active noAdvance next

/-- Every requested offset in one already-derived finite fresh window gives
the corresponding next-round timer upper edge. The theorem does not select a
future favorable slice. -/
theorem fresh_timer_paced_window_offset_gives_timer_start_successor_upper
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {maxAdmittedRefsPerRound recoveryWait : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation baseRound count offset : Time}
    (freshWindow : ValidatorFreshTimerPacedExactRoundWindow timed obligations
      waits observation baseRound count)
    (offsetInRange : offset < count)
    {prior : ValidatorCommitHead CommitId}
    (headsAtObservation : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).commitHead = prior)
    (priorBeforeRound : prior.round < baseRound + offset + 2)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
      observation (baseRound + offset + 2)
        (waits.wait prior (baseRound + offset + 2))
        (validatorFreshRoundReceiverTimerStepCost (syncRules := syncRules)
          timed prior (baseRound + offset + 2) maxAdmittedRefsPerRound) := by
  exact fresh_timer_paced_exact_round_gives_timer_start_successor_upper
    (recoveryWait := recoveryWait) admission pins sourceRules acceptance sync
      representatives arms originRules
        (freshWindow.freshAt offset offsetInRange) headsAtObservation
          priorBeforeRound afterGst active noAdvance

end Mysticeti
