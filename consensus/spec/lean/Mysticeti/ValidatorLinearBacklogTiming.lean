/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorHeadRelativeGapWait
import Mysticeti.ValidatorFreshTimerReadyBridge
import Mysticeti.ValidatorReceiverRelativeCausalBacklog

namespace Mysticeti

/-! Minimal timing and receiver-backlog lemmas used by fixed-reference recovery. -/

/-- The quadratic term is a lower bound on the concrete quadratic gap wait. -/
theorem quadraticGapWaitFromGap_quadratic_lower
    (baseWait linearCoefficient quadraticCoefficient gap : Nat) :
    quadraticCoefficient * gap * gap ≤
      quadraticGapWaitFromGap baseWait linearCoefficient
        quadraticCoefficient gap := by
  induction gap with
  | zero => simp [quadraticGapWaitFromGap]
  | succ previous inductionHypothesis =>
      have squareStep :
          quadraticCoefficient * (previous + 1) * (previous + 1) =
            quadraticCoefficient * previous * previous +
              quadraticCoefficient * (2 * previous + 1) := by
        have twice : 2 * previous = previous + previous := by omega
        rw [twice]
        simp [Nat.mul_add, Nat.add_mul, Nat.mul_assoc,
          Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
      rw [squareStep, quadraticGapWaitFromGap_succ]
      exact Nat.le_trans
        (Nat.add_le_add_right inductionHypothesis _)
        (by omega)

theorem head_relative_quadratic_value_eventually_covers_quadratic_spread_and_cutoff_backlog
    {CommitId : Type}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    (prior : ValidatorCommitHead CommitId)
    (cutoffRound spreadBase spreadLinear spreadQuadratic backlogSlope
      fixedPipelineCost : Nat)
    (quadraticCondition :
      spreadQuadratic < parameters.quadraticCoefficient) :
    ∃ firstRound,
      prior.round ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            (spreadBase + spreadLinear * (round - prior.round) +
                spreadQuadratic * (round - prior.round) *
                  (round - prior.round)) +
                ((round - cutoffRound) * backlogSlope +
                  fixedPipelineCost) ≤
              parameters.wait prior (round + 1) := by
  let fixedCost := spreadBase + fixedPipelineCost +
    (prior.round - cutoffRound) * backlogSlope
  let totalLinear := spreadLinear + backlogSlope
  let firstGap := max fixedCost (totalLinear + 1)
  let firstRound := prior.round + firstGap
  refine ⟨firstRound, by simp [firstRound], ?_⟩
  intro round firstBeforeRound
  have headBeforeRound : prior.round ≤ round :=
    Nat.le_trans (by simp [firstRound]) firstBeforeRound
  let gap := round - prior.round
  have firstGapBeforeGap : firstGap ≤ gap := by
    simp only [firstRound, gap] at firstBeforeRound ⊢
    omega
  have fixedWithinGap : fixedCost ≤ gap :=
    Nat.le_trans (Nat.le_max_left _ _) firstGapBeforeGap
  have linearPlusOneWithinGap : totalLinear + 1 ≤ gap :=
    Nat.le_trans (Nat.le_max_right _ _) firstGapBeforeGap
  have fixedAndLinearWithinSquare :
      fixedCost + totalLinear * gap ≤ gap * gap := by
    calc
      fixedCost + totalLinear * gap ≤ gap + totalLinear * gap :=
        Nat.add_le_add_right fixedWithinGap _
      _ = (totalLinear + 1) * gap := by
        simp [Nat.add_mul, Nat.add_comm]
      _ ≤ gap * gap :=
        Nat.mul_le_mul_right gap linearPlusOneWithinGap
  have coefficientWithUnit : spreadQuadratic + 1 ≤
      parameters.quadraticCoefficient := by
    omega
  have polynomialWithinQuadratic :
      fixedCost + totalLinear * gap + spreadQuadratic * gap * gap ≤
        parameters.quadraticCoefficient * gap * gap := by
    calc
      fixedCost + totalLinear * gap + spreadQuadratic * gap * gap ≤
          gap * gap + spreadQuadratic * gap * gap :=
        Nat.add_le_add_right fixedAndLinearWithinSquare _
      _ = (spreadQuadratic + 1) * (gap * gap) := by
        simp [Nat.add_mul, Nat.mul_assoc, Nat.add_comm]
      _ ≤ parameters.quadraticCoefficient * (gap * gap) :=
        Nat.mul_le_mul_right (gap * gap) coefficientWithUnit
      _ = parameters.quadraticCoefficient * gap * gap := by
        simp [Nat.mul_assoc]
  have cutoffGapBound : round - cutoffRound ≤
      (prior.round - cutoffRound) + gap := by
    simp only [gap]
    omega
  have scaledCutoffGapBound : (round - cutoffRound) * backlogSlope ≤
      (prior.round - cutoffRound) * backlogSlope +
        gap * backlogSlope := by
    simpa only [Nat.add_mul] using
      Nat.mul_le_mul_right backlogSlope cutoffGapBound
  have actualCostWithinPolynomial :
      (spreadBase + spreadLinear * gap +
          spreadQuadratic * gap * gap) +
          ((round - cutoffRound) * backlogSlope + fixedPipelineCost) ≤
        fixedCost + totalLinear * gap + spreadQuadratic * gap * gap := by
    have shifted := Nat.add_le_add_left scaledCutoffGapBound
      ((spreadBase + spreadLinear * gap +
        spreadQuadratic * gap * gap) + fixedPipelineCost)
    simpa [fixedCost, totalLinear, Nat.add_mul, Nat.mul_comm,
      Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using shifted
  have waitAtRoundLower : parameters.quadraticCoefficient * gap * gap ≤
      parameters.wait prior round := by
    simpa only [ValidatorHeadRelativeQuadraticWaitParameters.wait, gap] using
      quadraticGapWaitFromGap_quadratic_lower parameters.baseWait
        parameters.linearCoefficient parameters.quadraticCoefficient gap
  have nextGap : round + 1 - prior.round = gap + 1 := by
    simp only [gap]
    omega
  have waitMonotone : parameters.wait prior round ≤
      parameters.wait prior (round + 1) := by
    simp only [ValidatorHeadRelativeQuadraticWaitParameters.wait]
    rw [nextGap]
    change quadraticGapWaitFromGap parameters.baseWait
        parameters.linearCoefficient parameters.quadraticCoefficient gap ≤
      quadraticGapWaitFromGap parameters.baseWait
        parameters.linearCoefficient parameters.quadraticCoefficient (gap + 1)
    rw [quadraticGapWaitFromGap_succ]
    omega
  exact Nat.le_trans actualCostWithinPolynomial
    (Nat.le_trans polynomialWithinQuadratic
      (Nat.le_trans waitAtRoundLower waitMonotone))


/-- Pairwise spread and the actual lower successor edge retain the complete
prior-round wait in the deadline comparison. -/
theorem fresh_timer_start_spread_and_successor_lower_gives_deadline_bound
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
    previous.production.timerStartedAt + roundWait ≤
      next.production.timerStartedAt + spread := by
  unfold ValidatorFreshRoundTimerStartSpreadAt at previousSpread
  unfold ValidatorFreshTimerStartSuccessorLowerAt at lower
  rcases lower next with ⟨lowerAuthor, lowerPrevious, lowerBound⟩
  have pairwise := previousSpread previous lowerPrevious
  have pairwiseWithWait := Nat.add_le_add_right pairwise roundWait
  have lowerWithSpread := Nat.add_le_add_right lowerBound spread
  exact Nat.le_trans (by
    simpa only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
      pairwiseWithWait) lowerWithSpread

/-- A lower-edge deadline bound places any concrete receiver-resolution cost
before the next proposal snapshot.

The prior-round wait is already in `previousDeadlineBound`. Therefore, the
remaining visibility margin does not include that wait a second time. -/
theorem adjacent_lower_edge_margin_covers_timer_paced_resolution_cost
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
    {author receiver round startDifference resolutionCost : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (broadcast : ValidatorTimerPacedPeerBroadcast timed previous.snapshot
      author receiver previous.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (previousDeadlineBound :
      previous.timerStartedAt + waits.wait previousPrior round ≤
        next.timerStartedAt + startDifference)
    (visibilityMargin :
      startDifference + 3 * (timed.localActionBound + 1) + network.delta +
          resolutionCost ≤
        waits.wait receiverPrior (round + 1)) :
    broadcast.packet.deliveredAt + resolutionCost ≤
      next.snapshot.snapshotAt := by
  have deliveryFacts := timer_paced_peer_broadcast_is_delivered previous
    broadcast receiverInRange receiverCorrectAvailable sentAfterGst
  have sentBound :=
    timer_paced_peer_broadcast_sent_within_round_pipeline previous broadcast
  rw [previousHead] at sentBound
  rw [next.snapshotAtDeadline, nextHead]
  calc
    broadcast.packet.deliveredAt + resolutionCost ≤
        broadcast.packet.sentAt + network.delta + resolutionCost :=
      Nat.add_le_add_right deliveryFacts.2.1 resolutionCost
    _ ≤ (previous.timerStartedAt + waits.wait previousPrior round +
          3 * (timed.localActionBound + 1)) + network.delta +
            resolutionCost := by
      exact Nat.add_le_add_right
        (Nat.add_le_add_right sentBound network.delta) resolutionCost
    _ ≤ (next.timerStartedAt + startDifference +
          3 * (timed.localActionBound + 1)) + network.delta +
            resolutionCost := by
      exact Nat.add_le_add_right
        (Nat.add_le_add_right
          (Nat.add_le_add_right previousDeadlineBound
            (3 * (timed.localActionBound + 1))) network.delta)
        resolutionCost
    _ = next.timerStartedAt +
          (startDifference + 3 * (timed.localActionBound + 1) +
            network.delta + resolutionCost) := by
      ac_rfl
    _ ≤ next.timerStartedAt + waits.wait receiverPrior (round + 1) :=
      Nat.add_le_add_left visibilityMargin next.timerStartedAt

/-! Actual-block receiver-relative causal resolution. -/

/-- Current and past source facts for one delivered actual block.

The receiver has one cutoff in the actual child capsule. The exact protected
parent-sync source starts when the delivered block is available. The proposed
per-round admission rule supplies the linear unresolved-work bound. No field
states a future acceptance, timer, layer, or commit result. -/
structure ValidatorTimerPacedLinearBacklogSyncSource
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {author receiver round floor : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt) : Type where
  cutoff : ValidatorAcceptedCausalCapsuleRoundCutoffAt timed
    (admission.capsuleFor production.snapshot.block)
      (broadcast.packet.deliveredAt + 1) receiver floor
  targetSyncSource : ValidatorBlockParentSyncSource syncRules
    production.snapshot.block receiver author
      (admission.capsuleFor production.snapshot.block).history
        (broadcast.packet.deliveredAt + 1)

/-- One actual block resolves within its linear receiver-relative backlog.

The only proposed implementation rule is the transitive per-round admission
cap in `admission`. Current Rust does not yet enforce that cap. All other
premises describe the actual packet, persisted block, receiver cutoff, and
protected synchronization source. -/
theorem timer_paced_peer_broadcast_resolves_within_linear_backlog_or_gc
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    {author receiver round floor : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt)
    (source : ValidatorTimerPacedLinearBacklogSyncSource admission production
      broadcast (floor := floor))
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (active : ∀ time, broadcast.packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ acceptedAt,
      broadcast.packet.deliveredAt + 1 ≤ acceptedAt ∧
        acceptedAt ≤ broadcast.packet.deliveredAt + 1 +
            ((round - floor) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1 ∧
        (((timed.execution.trace acceptedAt).validatorState receiver).accepted
            production.snapshot.block.reference = true ∨
          production.snapshot.block.reference.round ≤
            ((timed.execution.trace acceptedAt).validatorState
              receiver).gcRound) := by
  have authorInRange : author < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have authorCorrectAvailable : faults.correctAvailable author = true := by
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  have deliveryBounds := network.postGstDelivery broadcast.packet
    broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using authorInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using authorCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have delivered := timed.execution.protocolPacketsAreDelivered
    broadcast.packetId broadcast.packet broadcast.packetInTrace
      broadcast.packetIsProtocol
      (by simpa [broadcast.packetSender] using authorInRange)
      (by simpa [broadcast.packetReceiver] using receiverInRange)
      (by simpa [broadcast.packetSender] using authorCorrectAvailable)
      (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
      sentAfterGst
  have packetAtDelivery := timed.execution.packetHistoryMonotone
    broadcast.packet.sentAt broadcast.packet.deliveredAt deliveryBounds.1
      broadcast.packetId broadcast.packet broadcast.packetInTrace
  have syncStartsAfterGst : network.gst ≤
      broadcast.packet.deliveredAt + 1 :=
    Nat.le_trans sentAfterGst
      (Nat.le_trans deliveryBounds.1 (Nat.le_add_right _ _))
  rcases admission.history_ready_within_linear_backlog source.targetSyncSource
      authorInRange authorCorrectAvailable production.persistenceOccurs
        source.cutoff receiverInRange receiverCorrectAvailable
          syncStartsAfterGst active with
    ⟨parentsReadyAt, deliveryBeforeParentsReady, parentsReadyBound,
      historyReady⟩
  have parentsReadyBound' : parentsReadyAt ≤
      broadcast.packet.deliveredAt + 1 +
        ((round - floor) * maxAdmittedRefsPerRound) *
          validatorBlockSyncAcceptanceBound timed syncRules := by
    simpa only [production.blockRound] using parentsReadyBound
  have parentsReady : ∀ parent,
      parent ∈ production.snapshot.block.parents →
        ((timed.execution.trace parentsReadyAt).validatorState
            broadcast.packet.receiver).accepted parent = true ∨
          parent.round ≤
            ((timed.execution.trace parentsReadyAt).validatorState
              broadcast.packet.receiver).gcRound := by
    intro parent parentMember
    rw [broadcast.packetReceiver]
    rcases source.targetSyncSource.coversDirectParents parent parentMember with
      acceptedAtStart | ⟨parentBlock, blockMember, blockReference⟩
    · exact Or.inl (timed.execution.accepted_block_persists receiverInRange
        deliveryBeforeParentsReady acceptedAtStart)
    · simpa [ValidatorReferenceAcceptedOrGcRootAt, blockReference] using
        historyReady parentBlock blockMember
  by_cases blockAtRoot : production.snapshot.block.reference.round ≤
      ((timed.execution.trace parentsReadyAt).validatorState receiver).gcRound
  · refine ⟨parentsReadyAt, deliveryBeforeParentsReady, ?_, Or.inr blockAtRoot⟩
    exact Nat.le_trans parentsReadyBound' (by
      simpa only [Nat.add_assoc] using Nat.le_add_right
        (broadcast.packet.deliveredAt + 1 +
          ((round - floor) * maxAdmittedRefsPerRound) *
            validatorBlockSyncAcceptanceBound timed syncRules)
        (timed.localActionBound + 1))
  · have blockAboveGc :
        ((timed.execution.trace parentsReadyAt).validatorState
          broadcast.packet.receiver).gcRound <
            production.snapshot.block.reference.round := by
      rw [broadcast.packetReceiver]
      omega
    rcases delivered_block_with_current_gc_ready_parents_is_accepted timed
        acceptance packetAtDelivery broadcast.packetPayload delivered
          (by simpa [broadcast.packetReceiver] using receiverInRange)
          (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
          (by simpa [production.snapshot.blockIsOwnProposal,
            production.proposer] using authorInRange)
          (Or.inr production.validParents) deliveryBeforeParentsReady
          blockAboveGc parentsReady with
      ⟨acceptedAt, parentsBeforeAccepted, acceptedWithinBound, accepted⟩
    refine ⟨acceptedAt,
      Nat.le_trans deliveryBeforeParentsReady parentsBeforeAccepted, ?_,
        Or.inl (by simpa [broadcast.packetReceiver] using accepted)⟩
    exact Nat.le_trans acceptedWithinBound
      (Nat.add_le_add_right parentsReadyBound'
        (timed.localActionBound + 1))


namespace ValidatorCausalCapsuleCatchupRateRules

/-- An accepted and retained correct timer-paced block gives the complete
exact parent evidence at the next-round proposal snapshot. -/
theorem accepted_retained_timer_paced_block_gives_parent_evidence
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
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {author receiver round : Nat}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (accepted :
      ((timed.execution.trace next.snapshot.snapshotAt).validatorState
        receiver).accepted previous.snapshot.block.reference = true)
    (retained :
      ((timed.execution.trace next.snapshot.snapshotAt).validatorState
        receiver).retained previous.snapshot.block.reference = true) :
    ValidatorAdjacentTimerPacedParentEvidence previous next := by
  have authorInRange : author < config.authorityCount := by
    simpa [previous.proposer] using previous.snapshot.proposerInRange
  have authorCorrect : faults.correctAvailable author = true := by
    simpa [previous.proposer] using
      previous.snapshot.proposerCorrectAvailable
  have authorNotByzantine : faults.byzantine author = false := by
    have notNonProgress : faults.nonProgress author = false := by
      simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
        VoterSet.full] using authorCorrect
    have separated : faults.byzantine author = false ∧
        faults.unavailable author = false := by
      simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
        notNonProgress
    exact separated.1
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.proposer] using next.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.proposer] using next.snapshot.proposerCorrectAvailable
  have recorded := representatives.acceptedCorrectReferenceIsRecorded
    next.snapshot.snapshotAt receiver previous.snapshot.block.reference
      receiverInRange receiverCorrect
      (by simpa [previous.snapshot.blockIsOwnProposal, previous.proposer] using
        authorInRange)
      (by simpa [previous.snapshot.blockIsOwnProposal, previous.proposer] using
        authorNotByzantine)
      accepted
  have included := timer_paced_round_includes_retained_current_parent next
    authorInRange
    (by simpa [previous.snapshot.blockIsOwnProposal, previous.proposer,
      previous.blockRound] using recorded)
    retained
  have sound :=
    (timed.execution.statesWellFormed next.snapshot.snapshotAt receiver
      receiverInRange).acceptedRepresentativeIsSound round
        previous.snapshot.block.reference.author
        previous.snapshot.block.reference
        (by simpa [previous.blockRound] using recorded)
  rcases sound.2.2.2 with ⟨catalogBlock, catalogAtNext,
    _catalogBlockReference⟩
  have exactCatalog :
      (timed.execution.trace next.snapshot.snapshotAt).blockCatalog
          previous.snapshot.block.reference.id = some previous.snapshot.block := by
    have catalogBlockExact : catalogBlock = previous.snapshot.block := by
      by_cases storedBeforeNext : previous.snapshot.storedAt ≤
          next.snapshot.snapshotAt
      · have previousAtNext := timed.execution.blockCatalogMonotone
          previous.snapshot.storedAt next.snapshot.snapshotAt storedBeforeNext
            previous.snapshot.block.reference.id previous.snapshot.block
              previous.snapshot.blockInCatalog
        exact Option.some.inj (catalogAtNext.symm.trans previousAtNext)
      · have nextBeforeStored : next.snapshot.snapshotAt ≤
            previous.snapshot.storedAt := Nat.le_of_not_ge storedBeforeNext
        have catalogAtStored := timed.execution.blockCatalogMonotone
          next.snapshot.snapshotAt previous.snapshot.storedAt nextBeforeStored
            previous.snapshot.block.reference.id catalogBlock catalogAtNext
        exact Option.some.inj
          (catalogAtStored.symm.trans previous.snapshot.blockInCatalog)
    simpa [catalogBlockExact] using catalogAtNext
  exact {
    included
    representative := by simpa [previous.blockRound] using recorded
    retained
    catalog := exactCatalog }


end ValidatorCausalCapsuleCatchupRateRules


end Mysticeti
