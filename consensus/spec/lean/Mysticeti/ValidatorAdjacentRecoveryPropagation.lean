/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorExactNextRecovery
import Mysticeti.ValidatorRecoveryCapsuleSyncExecution
import Mysticeti.ValidatorStrictRecoveryDirectQuorum

namespace Mysticeti

/-! Adjacent timer-paced recovery propagation.

This module is above both block production and recursive recovery sync in the
import graph. It keeps the pointwise GC race out of the lower direct-quorum
module.
-/

/-- If no correct, available validator advances after `start`, one selected
correct validator keeps its exact commit head at every later time. -/
private theorem no_correct_advance_keeps_exact_head
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start later validator : Time} {prior : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ later)
    (headAtStart :
      ((timed.execution.trace start).validatorState validator).commitHead = prior)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ((timed.execution.trace later).validatorState validator).commitHead = prior := by
  have durable := timed.execution.durableStateMonotone validator start later
    validatorInRange ordered
  have notStrict : ¬prior.index <
      ((timed.execution.trace later).validatorState validator).commitHead.index := by
    intro advanced
    exact noAdvance ⟨validator, later, validatorInRange,
      validatorCorrectAvailable, ordered, by simpa [headAtStart] using advanced⟩
  have sameIndex :
      ((timed.execution.trace start).validatorState validator).commitHead.index =
        ((timed.execution.trace later).validatorState validator).commitHead.index := by
    have monotone := durable.1
    rw [headAtStart] at monotone ⊢
    omega
  exact (durable.2.2.1 sameIndex).symm.trans headAtStart

/-- The head-independent recovery schedule eventually covers one complete
adjacent-round propagation edge, even when the sender and receiver have
different local commit heads. -/
theorem mixed_head_recovery_wait_eventually_covers_adjacent_flow
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (senderHead receiverHead : ValidatorCommitHead CommitId)
    (startDifference : Nat) :
    ∃ firstRound, ∀ round,
      firstRound ≤ round →
        waits.wait senderHead round +
            (startDifference + 3 * (timed.localActionBound + 1) +
              network.delta + 3 * (timed.localActionBound + 1)) ≤
          waits.wait receiverHead (round + 1) := by
  exact waits.eventually_covers_cross_head_visibility senderHead receiverHead
    (startDifference + 3 * (timed.localActionBound + 1) + network.delta +
      3 * (timed.localActionBound + 1))

/-- One correct timer-paced block becomes an exact parent of one correct
next-round timer-paced block, unless a correct local commit advances first.

For different validators, the proof uses the actual addressed packet. The
receiver pins the delivered body, accepts it after its already-known parents,
and keeps it above GC until the next proposal snapshot. For the same validator,
the durable own-block record gives the same result without a network packet. -/
theorem adjacent_timer_paced_productions_include_previous_or_commit_advance
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
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {commonStart author receiver round startDifference : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (commonStartBeforePrevious : commonStart ≤ previous.timerStartedAt)
    (commonStartBeforeNext : commonStart ≤ next.timerStartedAt)
    (receiverHeadAtStart :
      ((timed.execution.trace commonStart).validatorState receiver).commitHead =
        receiverPrior)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (referenceAfterPrior : receiverPrior.round < round)
    (previousStartBound :
      previous.timerStartedAt ≤ next.timerStartedAt + startDifference)
    (visibilityMargin :
      waits.wait previousPrior round +
          (startDifference + 3 * (timed.localActionBound + 1) + network.delta +
            3 * (timed.localActionBound + 1)) ≤
        waits.wait receiverPrior (round + 1))
    (previousStartsAfterGst : network.gst ≤ previous.timerStartedAt)
    (active : ∀ time, commonStart ≤ time →
      (timed.execution.trace time).epochActive = true)
    (previousParentsAcceptedAtStart : ∀ parent,
      parent ∈ previous.snapshot.block.parents →
        ((timed.execution.trace commonStart).validatorState receiver).accepted
          parent = true) :
    SomeCorrectAvailableCommitAdvance timed commonStart ∨
      previous.snapshot.block.reference ∈ next.snapshot.block.parents := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed commonStart
  · exact Or.inl advanced
  · right
    have authorInRange : author < config.authorityCount := by
      simpa [previous.proposer] using previous.snapshot.proposerInRange
    have authorCorrect : faults.correctAvailable author = true := by
      simpa [previous.proposer] using
        previous.snapshot.proposerCorrectAvailable
    have receiverInRange : receiver < config.authorityCount := by
      simpa [next.proposer] using next.snapshot.proposerInRange
    have receiverCorrect : faults.correctAvailable receiver = true := by
      simpa [next.proposer] using next.snapshot.proposerCorrectAvailable
    have nextStartBeforeSnapshot : next.timerStartedAt ≤
        next.snapshot.snapshotAt := by
      rw [next.snapshotAtDeadline]
      exact Nat.le_add_right _ _
    have commonStartBeforeNextSnapshot : commonStart ≤
        next.snapshot.snapshotAt :=
      Nat.le_trans commonStartBeforeNext nextStartBeforeSnapshot
    have headAt : ∀ time, commonStart ≤ time →
        ((timed.execution.trace time).validatorState receiver).commitHead =
          receiverPrior := by
      intro time ordered
      exact no_correct_advance_keeps_exact_head receiverInRange receiverCorrect
        ordered receiverHeadAtStart advanced
    have gcBelowPrevious : ∀ time, commonStart ≤ time →
        ((timed.execution.trace time).validatorState receiver).gcRound <
          previous.snapshot.block.reference.round := by
      intro time ordered
      have gcAtMostHead :
          ((timed.execution.trace time).validatorState receiver).gcRound ≤
            ((timed.execution.trace time).validatorState
              receiver).commitHead.round :=
        correct_validator_gc_round_at_most_commit_round
          (time := time) timed.execution receiverInRange receiverCorrect
      rw [headAt time ordered] at gcAtMostHead
      rw [previous.blockRound]
      omega
    by_cases sameAuthor : author = receiver
    · subst author
      have persistenceBeforeNextStart :=
        next_recovery_round_timer_starts_after_previous_persistence previous next
      have storedBeforeNextSnapshot : previous.snapshot.storedAt ≤
          next.snapshot.snapshotAt := by
        rw [previous.storedAfterPersistence]
        exact Nat.le_trans persistenceBeforeNextStart nextStartBeforeSnapshot
      have ownAtNext :=
        (timed.execution.durableStateMonotone receiver
          previous.snapshot.storedAt next.snapshot.snapshotAt receiverInRange
          storedBeforeNextSnapshot).own_block_persists
            (by simpa [previous.proposer] using previous.snapshot.blockStored)
      have ownFacts :=
        (timed.execution.statesWellFormed next.snapshot.snapshotAt receiver
          receiverInRange).ownBlockIsSound round
            previous.snapshot.block.reference (by
              simpa [previous.blockRound] using ownAtNext)
      have recorded := representatives.acceptedCorrectReferenceIsRecorded
        next.snapshot.snapshotAt receiver previous.snapshot.block.reference
        receiverInRange receiverCorrect
        (by simpa [previous.snapshot.blockIsOwnProposal, previous.proposer] using
          authorInRange)
        (by
          have notNonProgress : faults.nonProgress receiver = false := by
            simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
              VoterSet.full] using receiverCorrect
          have separated : faults.byzantine receiver = false ∧
              faults.unavailable receiver = false := by
            simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
              notNonProgress
          simpa [previous.snapshot.blockIsOwnProposal, previous.proposer] using
            separated.1)
        ownFacts.2.2.1
      apply timer_paced_round_includes_retained_current_parent next authorInRange
      · simpa [previous.snapshot.blockIsOwnProposal, previous.proposer,
          previous.blockRound] using recorded
      · exact ownFacts.2.2.2.1
    · let broadcast := Classical.choice
          (previous.peerBroadcast receiver receiverInRange (by
            intro receiverIsAuthor
            exact sameAuthor receiverIsAuthor.symm))
      have sentAfterGst : network.gst ≤ broadcast.packet.sentAt := by
        exact Nat.le_trans previousStartsAfterGst
          (Nat.le_trans (Nat.le_add_right _ _)
            (Nat.le_trans previous.deadlineBeforeProposal
              (Nat.le_trans (Nat.le_add_right _ 1)
                broadcast.proposalBeforeSend)))
      have deliveryFacts := timer_paced_peer_broadcast_is_delivered previous
        broadcast receiverInRange receiverCorrect sentAfterGst
      have sentBound :=
        timer_paced_peer_broadcast_sent_within_round_pipeline previous broadcast
      rw [previousHead] at sentBound
      have deliveryAcceptanceBeforeNextSnapshot :
          broadcast.packet.deliveredAt + 1 + timed.localActionBound + 1 ≤
            next.snapshot.snapshotAt := by
        have deliveredWithin : broadcast.packet.deliveredAt ≤
            broadcast.packet.sentAt + network.delta := deliveryFacts.2.1
        have acceptanceCost : 1 + timed.localActionBound + 1 ≤
            3 * (timed.localActionBound + 1) := by
          omega
        have acceptedByPreviousPipeline :
            broadcast.packet.deliveredAt + 1 + timed.localActionBound + 1 ≤
              previous.timerStartedAt + waits.wait previousPrior round +
                3 * (timed.localActionBound + 1) + network.delta +
                  3 * (timed.localActionBound + 1) := by
          calc
            broadcast.packet.deliveredAt + 1 + timed.localActionBound + 1 =
                broadcast.packet.deliveredAt +
                  (1 + timed.localActionBound + 1) := by
              simp only [Nat.add_assoc]
            _ ≤ (broadcast.packet.sentAt + network.delta) +
                  (1 + timed.localActionBound + 1) :=
              Nat.add_le_add_right deliveredWithin _
            _ ≤ (broadcast.packet.sentAt + network.delta) +
                  (3 * (timed.localActionBound + 1)) :=
              Nat.add_le_add_left acceptanceCost _
            _ ≤ (previous.timerStartedAt + waits.wait previousPrior round +
                    3 * (timed.localActionBound + 1)) + network.delta +
                  3 * (timed.localActionBound + 1) := by
              have bounded := Nat.add_le_add_right sentBound
                (network.delta + 3 * (timed.localActionBound + 1))
              simpa only [Nat.add_assoc] using bounded
        have shiftedToNext :
            previous.timerStartedAt + waits.wait previousPrior round +
                  3 * (timed.localActionBound + 1) + network.delta +
                    3 * (timed.localActionBound + 1) ≤
              next.timerStartedAt +
                (waits.wait previousPrior round +
                  (startDifference + 3 * (timed.localActionBound + 1) +
                    network.delta + 3 * (timed.localActionBound + 1))) := by
          have shifted := Nat.add_le_add_right previousStartBound
            (waits.wait previousPrior round + 3 * (timed.localActionBound + 1) +
              network.delta + 3 * (timed.localActionBound + 1))
          simpa only [Nat.add_assoc, Nat.add_left_comm, Nat.add_comm] using shifted
        have nextWaitBound := Nat.add_le_add_left visibilityMargin
          next.timerStartedAt
        rw [next.snapshotAtDeadline, nextHead]
        exact Nat.le_trans acceptedByPreviousPipeline
          (Nat.le_trans shiftedToNext nextWaitBound)
      have commonStartBeforeDelivery : commonStart ≤
          broadcast.packet.deliveredAt := by
        exact Nat.le_trans commonStartBeforePrevious
          (Nat.le_trans (Nat.le_add_right _ _)
            (Nat.le_trans previous.deadlineBeforeProposal
              (Nat.le_trans (Nat.le_add_right _ 1)
                (Nat.le_trans broadcast.proposalBeforeSend deliveryFacts.1))))
      have parentsAcceptedAtDelivery : ∀ parent,
          parent ∈ previous.snapshot.block.parents →
          ((timed.execution.trace (broadcast.packet.deliveredAt + 1)).validatorState
            receiver).accepted parent = true := by
        intro parent parentIncluded
        exact timed.execution.accepted_block_persists receiverInRange
          (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
          (previousParentsAcceptedAtStart parent parentIncluded)
      have aboveGcAtDelivery := gcBelowPrevious (broadcast.packet.deliveredAt + 1)
        (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
      rcases timer_paced_peer_broadcast_is_accepted_after_parents acceptance
          previous broadcast receiverInRange receiverCorrect sentAfterGst
          (parentsReadyAt := broadcast.packet.deliveredAt + 1)
          (Nat.le_refl _) aboveGcAtDelivery parentsAcceptedAtDelivery with
        ⟨acceptedAt, deliveryBeforeAccepted, acceptedWithin, accepted⟩
      have acceptedBeforeNextSnapshot : acceptedAt ≤ next.snapshot.snapshotAt :=
        Nat.le_trans acceptedWithin deliveryAcceptanceBeforeNextSnapshot
      have acceptedAtNext := timed.execution.accepted_block_persists
        receiverInRange acceptedBeforeNextSnapshot accepted
      have packetAtDelivery := timed.execution.packetHistoryMonotone
        broadcast.packet.sentAt broadcast.packet.deliveredAt deliveryFacts.1
        broadcast.packetId broadcast.packet broadcast.packetInTrace
      have localBody : ValidatorLocalBlockBodyAt timed
          broadcast.packet.deliveredAt receiver previous.snapshot.block :=
        .delivered broadcast.packetId broadcast.packet packetAtDelivery
          broadcast.packetIsProtocol broadcast.packetReceiver
          broadcast.packetPayload deliveryFacts.2.2
      have bodyPin := sync.local_body_creates_durable_pin receiverInRange
        receiverCorrect
        (active (broadcast.packet.deliveredAt + 1)
          (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1)))
        aboveGcAtDelivery localBody
      have deliveryBeforeNextSnapshot : broadcast.packet.deliveredAt + 1 ≤
          next.snapshot.snapshotAt :=
        Nat.le_trans (Nat.le_add_right _ (timed.localActionBound + 1))
          (by simpa [Nat.add_assoc] using deliveryAcceptanceBeforeNextSnapshot)
      have pinAtNext := sync.body_pin_persists_while_head_is_current
        receiverInRange receiverCorrect deliveryBeforeNextSnapshot bodyPin.1
        (by
          intro time _startBeforeTime _timeBeforeFinish
          exact active time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            _startBeforeTime))
        (by
          intro time startBeforeTime _timeBeforeFinish
          exact gcBelowPrevious time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime))
        (by
          intro time startBeforeTime _timeBeforeFinish
          have laterHead := headAt time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime)
          have initialHead := headAt (broadcast.packet.deliveredAt + 1)
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
          rw [laterHead, initialHead]
          exact Nat.le_refl _)
      have retainedAtNext := sync.acceptedPinnedBodyIsRetained
        next.snapshot.snapshotAt receiver previous.snapshot.block.reference
        _ receiverInRange receiverCorrect pinAtNext
        (gcBelowPrevious next.snapshot.snapshotAt commonStartBeforeNextSnapshot)
        acceptedAtNext
      have authorNotByzantine : faults.byzantine author = false := by
        have notNonProgress : faults.nonProgress author = false := by
          simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
            VoterSet.full] using authorCorrect
        have separated : faults.byzantine author = false ∧
            faults.unavailable author = false := by
          simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
            notNonProgress
        exact separated.1
      have recorded := representatives.acceptedCorrectReferenceIsRecorded
        next.snapshot.snapshotAt receiver previous.snapshot.block.reference
        receiverInRange receiverCorrect
        (by simpa [previous.snapshot.blockIsOwnProposal, previous.proposer] using
          authorInRange)
        (by simpa [previous.snapshot.blockIsOwnProposal, previous.proposer] using
          authorNotByzantine)
        acceptedAtNext
      apply timer_paced_round_includes_retained_current_parent next authorInRange
      · simpa [previous.snapshot.blockIsOwnProposal, previous.proposer,
          previous.blockRound] using recorded
      · exact retainedAtNext

/-- Exact one-host evidence left by one successful adjacent recovery edge.

The next proposal uses the previous block as a parent. At the same proposal
snapshot, the receiver has that exact reference as its current representative,
retains its body, and has the exact block in the global catalog. -/
structure ValidatorAdjacentTimerPacedParentEvidence
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
    {author receiver round : Nat}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1)) :
    Prop where
  included : previous.snapshot.block.reference ∈ next.snapshot.block.parents
  representative :
    ((timed.execution.trace next.snapshot.snapshotAt).validatorState receiver).acceptedRepresentative
        round previous.snapshot.block.reference.author =
      some previous.snapshot.block.reference
  retained :
    ((timed.execution.trace next.snapshot.snapshotAt).validatorState receiver).retained
        previous.snapshot.block.reference = true
  catalog :
    (timed.execution.trace next.snapshot.snapshotAt).blockCatalog
        previous.snapshot.block.reference.id = some previous.snapshot.block

/-- The adjacent propagation theorem also preserves the exact local evidence
needed by the direct-vote and causal-carrier proofs. -/
theorem adjacent_timer_paced_productions_give_parent_evidence_or_commit_advance
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
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {commonStart author receiver round startDifference : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (commonStartBeforePrevious : commonStart ≤ previous.timerStartedAt)
    (commonStartBeforeNext : commonStart ≤ next.timerStartedAt)
    (receiverHeadAtStart :
      ((timed.execution.trace commonStart).validatorState receiver).commitHead =
        receiverPrior)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (referenceAfterPrior : receiverPrior.round < round)
    (previousStartBound :
      previous.timerStartedAt ≤ next.timerStartedAt + startDifference)
    (visibilityMargin :
      waits.wait previousPrior round +
          (startDifference + 3 * (timed.localActionBound + 1) + network.delta +
            3 * (timed.localActionBound + 1)) ≤
        waits.wait receiverPrior (round + 1))
    (previousStartsAfterGst : network.gst ≤ previous.timerStartedAt)
    (active : ∀ time, commonStart ≤ time →
      (timed.execution.trace time).epochActive = true)
    (previousParentsAcceptedAtStart : ∀ parent,
      parent ∈ previous.snapshot.block.parents →
        ((timed.execution.trace commonStart).validatorState receiver).accepted
          parent = true) :
    SomeCorrectAvailableCommitAdvance timed commonStart ∨
      ValidatorAdjacentTimerPacedParentEvidence previous next := by
  rcases adjacent_timer_paced_productions_include_previous_or_commit_advance
      acceptance sync representatives previous next commonStartBeforePrevious
      commonStartBeforeNext receiverHeadAtStart previousHead nextHead
      referenceAfterPrior previousStartBound visibilityMargin
      previousStartsAfterGst active previousParentsAcceptedAtStart with
    advanced | included
  · exact Or.inl advanced
  · right
    have receiverInRange : receiver < config.authorityCount := by
      simpa [next.proposer] using next.snapshot.proposerInRange
    have selected :=
      next.refreshedParentList.selectedCurrentRepresentatives
        previous.snapshot.block.reference included
    have representative :
        ((timed.execution.trace next.snapshot.snapshotAt).validatorState
          receiver).acceptedRepresentative round
            previous.snapshot.block.reference.author =
          some previous.snapshot.block.reference := by
      simpa using selected
    have retained :
        ((timed.execution.trace next.snapshot.snapshotAt).validatorState
          receiver).retained previous.snapshot.block.reference = true :=
      (next.refreshedParentList.ready.2 previous.snapshot.block.reference
        included).1
    have sound :=
      (timed.execution.statesWellFormed next.snapshot.snapshotAt receiver
        receiverInRange).acceptedRepresentativeIsSound round
          previous.snapshot.block.reference.author
          previous.snapshot.block.reference representative
    rcases sound.2.2.2 with ⟨catalogBlock, catalogAtNext,
      catalogBlockReference⟩
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
    exact
      { included
        representative
        retained
        catalog := exactCatalog }

/-- Two adjacent recovery edges give the exact direct-vote evidence at the
carrier snapshot.

The first edge puts the leader in the voter block. The second edge puts that
exact voter block in the carrier and records its body as the carrier host's
current retained representative. -/
theorem adjacent_parent_evidence_pair_gives_vote_ready
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
    {leaderAuthor voter observer round : Nat}
    (leader : ValidatorTimerPacedRoundProduction timed waits leaderAuthor round)
    (vote : ValidatorTimerPacedRoundProduction timed waits voter (round + 1))
    (carrier : ValidatorTimerPacedRoundProduction timed waits observer
      (round + 2))
    (leaderToVote : ValidatorAdjacentTimerPacedParentEvidence leader vote)
    (voteToCarrier : ValidatorAdjacentTimerPacedParentEvidence vote carrier) :
    ∃ voteBlock : ValidatorBlock BlockId,
      (timed.execution.trace carrier.snapshot.snapshotAt).blockCatalog
          voteBlock.reference.id = some voteBlock ∧
        voteBlock.reference.author = voter ∧
        voteBlock.reference.round = leader.snapshot.block.reference.round + 1 ∧
        leader.snapshot.block.reference ∈ voteBlock.parents ∧
        ((timed.execution.trace carrier.snapshot.snapshotAt).validatorState
            observer).acceptedRepresentative
              (leader.snapshot.block.reference.round + 1) voter =
            some voteBlock.reference ∧
        ((timed.execution.trace carrier.snapshot.snapshotAt).validatorState
            observer).retained voteBlock.reference = true := by
  have voteAuthor : vote.snapshot.block.reference.author = voter := by
    exact vote.snapshot.blockIsOwnProposal.trans vote.proposer
  have voteRound : vote.snapshot.block.reference.round =
      leader.snapshot.block.reference.round + 1 := by
    rw [vote.blockRound, leader.blockRound]
  have representative :
      ((timed.execution.trace carrier.snapshot.snapshotAt).validatorState
          observer).acceptedRepresentative
            (leader.snapshot.block.reference.round + 1) voter =
        some vote.snapshot.block.reference := by
    rw [leader.blockRound]
    simpa [voteAuthor] using voteToCarrier.representative
  exact ⟨vote.snapshot.block, voteToCarrier.catalog, voteAuthor, voteRound,
    leaderToVote.included, representative, voteToCarrier.retained⟩

/-- Exact adjacent propagation for every correct, available voter gives the
full direct-vote frontier and its quorum weight at the carrier snapshot.

This lemma removes the `votesReady` adapter from later trace composition. The
finite recovery-window proof constructs `voteEvidence` from actual proposal,
delivery, acceptance, and refreshed-parent events. -/
theorem adjacent_parent_evidence_family_gives_full_direct_vote_quorum
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {leaderAuthor observer round : Nat}
    (leader : ValidatorTimerPacedRoundProduction timed waits leaderAuthor round)
    (carrier : ValidatorTimerPacedRoundProduction timed waits observer
      (round + 2))
    (voteEvidence : ∀ voter,
      voter < config.authorityCount →
      faults.correctAvailable voter = true →
      ∃ vote : ValidatorTimerPacedRoundProduction timed waits voter
          (round + 1),
        ValidatorAdjacentTimerPacedParentEvidence leader vote ∧
          ValidatorAdjacentTimerPacedParentEvidence vote carrier) :
    ValidatorCorrectAvailableDirectVoteFrontier config faults
        (timed.execution.trace carrier.snapshot.snapshotAt) observer
        leader.snapshot.block.reference carrier.snapshot.block ∧
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters
            (timed.execution.trace carrier.snapshot.snapshotAt) observer
            leader.snapshot.block.reference) := by
  apply timer_paced_carrier_contains_full_direct_vote_quorum carrier
    leader.snapshot.block.reference
  · rw [leader.blockRound]
  · intro voter voterInRange voterCorrect
    rcases voteEvidence voter voterInRange voterCorrect with
      ⟨vote, leaderToVote, voteToCarrier⟩
    exact adjacent_parent_evidence_pair_gives_vote_ready leader vote carrier
      leaderToVote voteToCarrier

end Mysticeti
