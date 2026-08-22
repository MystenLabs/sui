/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorLocalDagSuccessorLiveness

namespace Mysticeti

/-! Collective direct-vote evidence from one fresh recovery window.

The finite window supplies each concrete proposal. The caller supplies only the
already-derived adjacent-parent evidence for those proposals. The main theorem
selects the receiver's own round-plus-two proposal as the carrier. It does not
take a future carrier or delivery result as an input.
-/

/-- Exact accepted direct-vote bodies at one receiver.

This is the pointwise evidence used by the local FlexCommitter bridge. It does
not include acceptance of the leader block itself. -/
def ValidatorReceiverAcceptedDirectVoteQuorum
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (receiver : Nat)
    (leader : ValidatorBlockRef BlockId) : Prop :=
  ∃ voters : VoterSet,
    config.thresholds.quorum ≤
        weight config.authorityCount config.stake voters ∧
      ∀ voter,
        voter < config.authorityCount →
        voters voter = true →
        ∃ voteReference voteBlock,
          (world.validatorState receiver).acceptedRepresentative
                (leader.round + 1) voter = some voteReference ∧
            (world.validatorState receiver).accepted voteReference = true ∧
            world.blockCatalog voteReference.id = some voteBlock ∧
            voteBlock.reference = voteReference ∧
            voteReference.author = voter ∧
            voteReference.round = leader.round + 1 ∧
            leader ∈ voteBlock.parents

/-- One exact direct-vote frontier gives the accepted vote bodies that the
receiver-local FlexCommitter bridge reads. -/
theorem direct_vote_frontier_gives_receiver_accepted_quorum
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time receiver : Nat}
    {leader : ValidatorBlockRef BlockId}
    {carrier : ValidatorBlock BlockId}
    (receiverInRange : receiver < config.authorityCount)
    (frontier : ValidatorCorrectAvailableDirectVoteFrontier config faults
      (execution.trace time) receiver leader carrier) :
    ValidatorReceiverAcceptedDirectVoteQuorum config (execution.trace time)
      receiver leader := by
  refine ⟨faults.correctAvailable,
    faults.correct_available_stake_is_quorum, ?_⟩
  intro voter voterInRange voterCorrect
  rcases frontier voter voterInRange voterCorrect with
    ⟨voteBlock, catalog, voteAuthor, voteRound, leaderParent,
      representative, _retained, _carrierParent, _directVote⟩
  have accepted :=
    (execution.statesWellFormed time receiver receiverInRange)
      |>.acceptedRepresentativeIsSound (leader.round + 1) voter
        voteBlock.reference representative
  exact ⟨voteBlock.reference, voteBlock, representative, accepted.2.2.1,
    catalog, rfl, voteAuthor, voteRound, leaderParent⟩

/-- The selected leader, the receiver's exact round-plus-two carrier, and the
receiver-local direct-vote quorum at the carrier snapshot. -/
structure ValidatorFreshReceiverDirectVoteQuorum
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
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
    (observation leaderAuthor receiver round : Nat) where
  leader : ValidatorFreshTimerPacedExactRoundProduction timed obligations waits
    observation leaderAuthor round
  carrier : ValidatorFreshTimerPacedExactRoundProduction timed obligations waits
    observation receiver (round + 2)
  frontier : ValidatorCorrectAvailableDirectVoteFrontier config faults
    (timed.execution.trace carrier.production.snapshot.snapshotAt) receiver
      leader.production.snapshot.block.reference
      carrier.production.snapshot.block
  quorum : config.thresholds.quorum ≤
    weight config.authorityCount config.stake
      (traceDirectVoters
        (timed.execution.trace carrier.production.snapshot.snapshotAt) receiver
        leader.production.snapshot.block.reference)
  leaderAccepted :
    ((timed.execution.trace
      carrier.production.snapshot.snapshotAt).validatorState receiver).accepted
        leader.production.snapshot.block.reference = true
  acceptedQuorum : ValidatorReceiverAcceptedDirectVoteQuorum config
    (timed.execution.trace carrier.production.snapshot.snapshotAt) receiver
      leader.production.snapshot.block.reference

/-- A finite family of already-actual fresh timer-paced proposals. This is the
part of `ValidatorFreshTimerPacedExactRoundWindow` used by the direct-vote
consumer. It does not require the old stable-common-round source record. -/
def ValidatorFreshTimerPacedExactRoundFamily
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
    (observation baseRound count : Nat) : Prop :=
  ∀ offset,
    offset < count →
    EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed obligations
      waits observation (baseRound + offset + 2)

/-- A finite already-actual fresh family and its adjacent-parent edges select
one exact round-plus-two receiver carrier with a full direct-vote quorum.

All proposal values come from `family`. `adjacentEvidence` is a past-trace
relation between those concrete values. It does not create a proposal, carrier,
delivery, or commit. -/
theorem fresh_family_and_adjacent_evidence_give_receiver_direct_vote_quorum
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
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation baseRound count offset leaderAuthor receiver : Nat}
    (family : ValidatorFreshTimerPacedExactRoundFamily timed obligations waits
      observation baseRound count)
    (carrierOffsetInRange : offset + 2 < count)
    (leaderInRange : leaderAuthor < config.authorityCount)
    (leaderCorrect : faults.correctAvailable leaderAuthor = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (adjacentEvidence :
      ∀
        (leader : ValidatorFreshTimerPacedExactRoundProduction timed obligations
          waits observation leaderAuthor (baseRound + offset + 2))
        (carrier : ValidatorFreshTimerPacedExactRoundProduction timed
          obligations waits observation receiver
            ((baseRound + offset + 2) + 2))
        voter,
        voter < config.authorityCount →
        faults.correctAvailable voter = true →
        ∀ vote : ValidatorFreshTimerPacedExactRoundProduction timed
          obligations waits observation voter
            ((baseRound + offset + 2) + 1),
          ValidatorAdjacentTimerPacedParentEvidence leader.production
              vote.production ∧
            ValidatorAdjacentTimerPacedParentEvidence vote.production
              carrier.production) :
    Nonempty (ValidatorFreshReceiverDirectVoteQuorum timed obligations waits
      observation leaderAuthor receiver (baseRound + offset + 2)) := by
  have leaderOffsetInRange : offset < count := by omega
  have voteOffsetInRange : offset + 1 < count := by omega
  let leader := Classical.choice
    (family offset leaderOffsetInRange leaderAuthor leaderInRange
      leaderCorrect)
  let carrierRaw := Classical.choice
    (family (offset + 2) carrierOffsetInRange receiver receiverInRange
      receiverCorrect)
  let carrier : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver ((baseRound + offset + 2) + 2) := by
    simpa only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using carrierRaw
  let receiverVoteRaw := Classical.choice
    (family (offset + 1) voteOffsetInRange receiver receiverInRange
      receiverCorrect)
  let receiverVote : ValidatorFreshTimerPacedExactRoundProduction timed
      obligations waits observation receiver
        ((baseRound + offset + 2) + 1) := by
    simpa only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
      receiverVoteRaw
  have receiverEdges := adjacentEvidence leader carrier receiver
    receiverInRange receiverCorrect receiverVote
  have leaderAcceptedAtVote :
      ((timed.execution.trace
        receiverVote.production.snapshot.snapshotAt).validatorState
          receiver).accepted leader.production.snapshot.block.reference = true :=
    (timed.execution.statesWellFormed
      receiverVote.production.snapshot.snapshotAt receiver receiverInRange)
        |>.acceptedRepresentativeIsSound
          (baseRound + offset + 2)
          leader.production.snapshot.block.reference.author
          leader.production.snapshot.block.reference
          receiverEdges.1.representative
        |>.2.2.1
  have votePersistenceBeforeCarrierTimer :=
    next_recovery_round_timer_starts_after_previous_persistence
      receiverVote.production carrier.production
  have carrierTimerBeforeSnapshot : carrier.production.timerStartedAt ≤
      carrier.production.snapshot.snapshotAt := by
    rw [carrier.production.snapshotAtDeadline]
    exact Nat.le_add_right _ _
  have voteSnapshotBeforeCarrierSnapshot :
      receiverVote.production.snapshot.snapshotAt ≤
        carrier.production.snapshot.snapshotAt :=
    Nat.le_trans receiverVote.production.snapshotBeforePersistence
      (Nat.le_trans (Nat.le_add_right receiverVote.production.persistTime 1)
        (Nat.le_trans votePersistenceBeforeCarrierTimer
          carrierTimerBeforeSnapshot))
  have leaderAcceptedAtCarrier := timed.execution.accepted_block_persists
    receiverInRange voteSnapshotBeforeCarrierSnapshot leaderAcceptedAtVote
  have voteEvidence : ∀ voter,
      voter < config.authorityCount →
      faults.correctAvailable voter = true →
      ∃ vote : ValidatorTimerPacedRoundProduction timed waits voter
          ((baseRound + offset + 2) + 1),
        ValidatorAdjacentTimerPacedParentEvidence leader.production vote ∧
          ValidatorAdjacentTimerPacedParentEvidence vote carrier.production := by
    intro voter voterInRange voterCorrect
    let voteRaw := Classical.choice
      (family (offset + 1) voteOffsetInRange voter voterInRange
        voterCorrect)
    let vote : ValidatorFreshTimerPacedExactRoundProduction timed obligations
        waits observation voter ((baseRound + offset + 2) + 1) := by
      simpa only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using voteRaw
    have edges := adjacentEvidence leader carrier voter voterInRange
      voterCorrect vote
    exact ⟨vote.production, edges⟩
  have aggregated :=
    adjacent_parent_evidence_family_gives_full_direct_vote_quorum
      leader.production carrier.production voteEvidence
  refine ⟨{
    leader := leader
    carrier := carrier
    frontier := aggregated.1
    quorum := aggregated.2
    leaderAccepted := leaderAcceptedAtCarrier
    acceptedQuorum := ?_ }⟩
  exact direct_vote_frontier_gives_receiver_accepted_quorum timed.execution
    receiverInRange aggregated.1

/-- Compatibility adapter from the older stable-common-round window. -/
theorem fresh_window_and_adjacent_evidence_give_receiver_direct_vote_quorum
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
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation baseRound count offset leaderAuthor receiver : Nat}
    (window : ValidatorFreshTimerPacedExactRoundWindow timed obligations waits
      observation baseRound count)
    (carrierOffsetInRange : offset + 2 < count)
    (leaderInRange : leaderAuthor < config.authorityCount)
    (leaderCorrect : faults.correctAvailable leaderAuthor = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (adjacentEvidence :
      ∀
        (leader : ValidatorFreshTimerPacedExactRoundProduction timed obligations
          waits observation leaderAuthor (baseRound + offset + 2))
        (carrier : ValidatorFreshTimerPacedExactRoundProduction timed
          obligations waits observation receiver
            ((baseRound + offset + 2) + 2))
        voter,
        voter < config.authorityCount →
        faults.correctAvailable voter = true →
        ∀ vote : ValidatorFreshTimerPacedExactRoundProduction timed
          obligations waits observation voter
            ((baseRound + offset + 2) + 1),
          ValidatorAdjacentTimerPacedParentEvidence leader.production
              vote.production ∧
            ValidatorAdjacentTimerPacedParentEvidence vote.production
              carrier.production) :
    Nonempty (ValidatorFreshReceiverDirectVoteQuorum timed obligations waits
      observation leaderAuthor receiver (baseRound + offset + 2)) := by
  exact fresh_family_and_adjacent_evidence_give_receiver_direct_vote_quorum
    window.freshAt carrierOffsetInRange leaderInRange leaderCorrect
      receiverInRange receiverCorrect adjacentEvidence

/-- Exact accepted direct-vote bodies persist to a later receiver state. -/
theorem receiver_accepted_direct_vote_quorum_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {earlier later receiver : Nat}
    {leader : ValidatorBlockRef BlockId}
    (receiverInRange : receiver < config.authorityCount)
    (ordered : earlier ≤ later)
    (quorum : ValidatorReceiverAcceptedDirectVoteQuorum config
      (execution.trace earlier) receiver leader) :
    ValidatorReceiverAcceptedDirectVoteQuorum config (execution.trace later)
      receiver leader := by
  rcases quorum with ⟨voters, voterWeight, voteEvidence⟩
  refine ⟨voters, voterWeight, ?_⟩
  intro voter voterInRange voterSelected
  rcases voteEvidence voter voterInRange voterSelected with
    ⟨voteReference, voteBlock, representative, accepted, catalog,
      blockReference, voteAuthor, voteRound, leaderParent⟩
  have representativeLater :=
    (execution.durable_fields_persist receiverInRange ordered)
      |>.accepted_representative_persists representative
  have acceptedLater := execution.accepted_block_persists receiverInRange
    ordered accepted
  have catalogLater := execution.blockCatalogMonotone earlier later ordered
    voteReference.id voteBlock catalog
  exact ⟨voteReference, voteBlock, representativeLater, acceptedLater,
    catalogLater, blockReference, voteAuthor, voteRound, leaderParent⟩

/-- The maximum of one base time and the first `count` selected carrier times.
-/
def recoveryCarrierMaximumTime
    (base : Time) (timeAt : Nat → Time) : Nat → Time
  | 0 => base
  | count + 1 =>
      Nat.max (recoveryCarrierMaximumTime base timeAt count) (timeAt count)

theorem recovery_carrier_base_le_maximum
    (base : Time) (timeAt : Nat → Time) (count : Nat) :
    base ≤ recoveryCarrierMaximumTime base timeAt count := by
  induction count with
  | zero => exact Nat.le_refl _
  | succ count inductionHypothesis =>
      exact Nat.le_trans inductionHypothesis (Nat.le_max_left _ _)

theorem recovery_carrier_time_le_maximum
    (base : Time) (timeAt : Nat → Time)
    {offset count : Nat}
    (offsetInRange : offset < count) :
    timeAt offset ≤ recoveryCarrierMaximumTime base timeAt count := by
  induction count with
  | zero => omega
  | succ count inductionHypothesis =>
      by_cases earlier : offset < count
      · exact Nat.le_trans (inductionHypothesis earlier)
          (Nat.le_max_left _ _)
      · have last : offset = count := by omega
        subst offset
        exact Nat.le_max_right _ _

/-- A finite range of exact favorable leaders and their accepted direct-vote
quorums at one receiver and one trace time. -/
structure ValidatorReceiverAcceptedDirectVoteRange
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (receiver baseRound count : Nat)
    (leaderAuthorAt : Nat → Nat) where
  leaderAt : Nat → ValidatorBlockRef BlockId
  leaderRound : ∀ offset, offset < count →
    (leaderAt offset).round = baseRound + offset
  leaderAuthor : ∀ offset, offset < count →
    (leaderAt offset).author = leaderAuthorAt offset
  acceptedVoteEvidence : ∀ offset, offset < count →
    (world.validatorState receiver).accepted (leaderAt offset) = true ∧
      ValidatorReceiverAcceptedDirectVoteQuorum config world receiver
        (leaderAt offset)

/-- The range record has the exact expanded evidence shape used by the local
FlexCommitter trace theorem. -/
theorem ValidatorReceiverAcceptedDirectVoteRange.toAcceptedVoteEvidence
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {receiver baseRound count : Nat}
    {leaderAuthorAt : Nat → Nat}
    (range : ValidatorReceiverAcceptedDirectVoteRange config world receiver
      baseRound count leaderAuthorAt) :
    ∀ offset,
      offset < count →
      (world.validatorState receiver).accepted (range.leaderAt offset) = true ∧
        ∃ voters : VoterSet,
          config.thresholds.quorum ≤
              weight config.authorityCount config.stake voters ∧
            ∀ voter,
              voter < config.authorityCount →
              voters voter = true →
              ∃ voteReference voteBlock,
                (world.validatorState receiver).acceptedRepresentative
                      ((range.leaderAt offset).round + 1) voter =
                    some voteReference ∧
                  (world.validatorState receiver).accepted voteReference = true ∧
                  world.blockCatalog voteReference.id = some voteBlock ∧
                  voteBlock.reference = voteReference ∧
                  voteReference.author = voter ∧
                  voteReference.round = (range.leaderAt offset).round + 1 ∧
                  range.leaderAt offset ∈ voteBlock.parents := by
  intro offset offsetInRange
  exact range.acceptedVoteEvidence offset offsetInRange

/-- A finite favorable proposal family has one common receiver time with every
exact leader and direct-vote quorum.

The leader authors are arguments for actual offsets in `family`. The theorem
does not add them to a global execution input. -/
theorem fresh_family_and_adjacent_evidence_give_receiver_direct_vote_range
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
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation baseRound windowCount leaderCount receiver : Nat}
    (family : ValidatorFreshTimerPacedExactRoundFamily timed obligations waits
      observation baseRound windowCount)
    (leaderCountPositive : 0 < leaderCount)
    (windowCoversCarriers : leaderCount + 2 ≤ windowCount)
    (leaderAuthorAt : Nat → Nat)
    (leaderInRange : ∀ offset, offset < leaderCount →
      leaderAuthorAt offset < config.authorityCount)
    (leaderCorrect : ∀ offset, offset < leaderCount →
      faults.correctAvailable (leaderAuthorAt offset) = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (adjacentEvidence : ∀ offset,
      offset < leaderCount →
      ∀
        (leader : ValidatorFreshTimerPacedExactRoundProduction timed obligations
          waits observation (leaderAuthorAt offset)
            (baseRound + offset + 2))
        (carrier : ValidatorFreshTimerPacedExactRoundProduction timed
          obligations waits observation receiver
            ((baseRound + offset + 2) + 2))
        voter,
        voter < config.authorityCount →
        faults.correctAvailable voter = true →
        ∀ vote : ValidatorFreshTimerPacedExactRoundProduction timed
          obligations waits observation voter
            ((baseRound + offset + 2) + 1),
          ValidatorAdjacentTimerPacedParentEvidence leader.production
              vote.production ∧
            ValidatorAdjacentTimerPacedParentEvidence vote.production
              carrier.production) :
    ∃ finish,
      observation ≤ finish ∧
        Nonempty (ValidatorReceiverAcceptedDirectVoteRange config
          (timed.execution.trace finish) receiver (baseRound + 2) leaderCount
            leaderAuthorAt) := by
  have carrierOffsetInRange : ∀ offset, offset < leaderCount →
      offset + 2 < windowCount := by
    intro offset offsetInRange
    omega
  let resultAt := fun (offset : Nat) (offsetInRange : offset < leaderCount) =>
    Classical.choice
      (fresh_family_and_adjacent_evidence_give_receiver_direct_vote_quorum
        family (carrierOffsetInRange offset offsetInRange)
          (leaderInRange offset offsetInRange)
          (leaderCorrect offset offsetInRange) receiverInRange receiverCorrect
          (adjacentEvidence offset offsetInRange))
  let first := resultAt 0 leaderCountPositive
  let timeAt := fun offset =>
    if offsetInRange : offset < leaderCount then
      (resultAt offset offsetInRange).carrier.production.snapshot.snapshotAt
    else observation
  let finish := recoveryCarrierMaximumTime observation timeAt leaderCount
  let leaderAt := fun offset =>
    if offsetInRange : offset < leaderCount then
      (resultAt offset offsetInRange).leader.production.snapshot.block.reference
    else first.leader.production.snapshot.block.reference
  have resultBeforeFinish : ∀ offset (offsetInRange : offset < leaderCount),
      (resultAt offset offsetInRange).carrier.production.snapshot.snapshotAt ≤
        finish := by
    intro offset offsetInRange
    have bounded := recovery_carrier_time_le_maximum observation timeAt
      offsetInRange
    simpa only [finish, timeAt, dif_pos offsetInRange] using bounded
  refine ⟨finish, recovery_carrier_base_le_maximum observation timeAt
    leaderCount, ⟨{
      leaderAt := leaderAt
      leaderRound := ?_
      leaderAuthor := ?_
      acceptedVoteEvidence := ?_ }⟩⟩
  · intro offset offsetInRange
    have blockRound := (resultAt offset offsetInRange).leader.production.blockRound
    simpa only [leaderAt, dif_pos offsetInRange, Nat.add_assoc, Nat.add_comm,
      Nat.add_left_comm] using blockRound
  · intro offset offsetInRange
    have blockAuthor :=
      (resultAt offset offsetInRange).leader.production.snapshot.blockIsOwnProposal
        |>.trans (resultAt offset offsetInRange).leader.production.proposer
    simpa only [leaderAt, dif_pos offsetInRange] using blockAuthor
  · intro offset offsetInRange
    have ordered := resultBeforeFinish offset offsetInRange
    have accepted := timed.execution.accepted_block_persists receiverInRange
      ordered (resultAt offset offsetInRange).leaderAccepted
    have quorum := receiver_accepted_direct_vote_quorum_persists
      timed.execution receiverInRange ordered
        (resultAt offset offsetInRange).acceptedQuorum
    simpa only [leaderAt, dif_pos offsetInRange] using And.intro accepted quorum

/-- Compatibility adapter from the older stable-common-round window. -/
theorem fresh_window_and_adjacent_evidence_give_receiver_direct_vote_range
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
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation baseRound windowCount leaderCount receiver : Nat}
    (window : ValidatorFreshTimerPacedExactRoundWindow timed obligations waits
      observation baseRound windowCount)
    (leaderCountPositive : 0 < leaderCount)
    (windowCoversCarriers : leaderCount + 2 ≤ windowCount)
    (leaderAuthorAt : Nat → Nat)
    (leaderInRange : ∀ offset, offset < leaderCount →
      leaderAuthorAt offset < config.authorityCount)
    (leaderCorrect : ∀ offset, offset < leaderCount →
      faults.correctAvailable (leaderAuthorAt offset) = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (adjacentEvidence : ∀ offset,
      offset < leaderCount →
      ∀
        (leader : ValidatorFreshTimerPacedExactRoundProduction timed obligations
          waits observation (leaderAuthorAt offset)
            (baseRound + offset + 2))
        (carrier : ValidatorFreshTimerPacedExactRoundProduction timed
          obligations waits observation receiver
            ((baseRound + offset + 2) + 2))
        voter,
        voter < config.authorityCount →
        faults.correctAvailable voter = true →
        ∀ vote : ValidatorFreshTimerPacedExactRoundProduction timed
          obligations waits observation voter
            ((baseRound + offset + 2) + 1),
          ValidatorAdjacentTimerPacedParentEvidence leader.production
              vote.production ∧
            ValidatorAdjacentTimerPacedParentEvidence vote.production
              carrier.production) :
    ∃ finish,
      observation ≤ finish ∧
        Nonempty (ValidatorReceiverAcceptedDirectVoteRange config
          (timed.execution.trace finish) receiver (baseRound + 2) leaderCount
            leaderAuthorAt) := by
  exact fresh_family_and_adjacent_evidence_give_receiver_direct_vote_range
    window.freshAt leaderCountPositive windowCoversCarriers leaderAuthorAt
      leaderInRange leaderCorrect receiverInRange receiverCorrect
        adjacentEvidence

/-- A common-time receiver range supplies the exact evidence input of the
protected local FlexCommitter theorem. -/
theorem receiver_direct_vote_range_records_local_commit_or_installed_next
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
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
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (pending : ValidatorExactPendingIngestionRules source)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    (work : ValidatorSuccessfulFlexScanWorkRules (faults := faults)
      (network := network) (timed := timed) source)
    {start finish observer baseRound count : Nat}
    {prior : ValidatorCommitHead CommitId}
    {leaderAuthorAt : Nat → Nat}
    (range : ValidatorReceiverAcceptedDirectVoteRange config
      (timed.execution.trace finish) observer baseRound count leaderAuthorAt)
    (countMatchesDepth : count =
      (context observer
        ((timed.execution.trace finish).validatorState observer)).depth + 1)
    (startBeforeFinish : start ≤ finish)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState observer).commitHead = prior)
    (baseAfterCommitHead : prior.round < baseRound)
    (firstSelectedAuthor : ∀ offset, offset < count →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAuthorAt offset)) :
    (∃ completedAt witnessId,
      start ≤ completedAt ∧
        ((timed.execution.trace completedAt).validatorState
          observer).installedCommitAt (prior.index + 1) = some witnessId) ∨
      ∃ (run : CorrectExactFlexRun runtime) (installedAt : Time),
        start ≤ run.observation.time ∧
          run.observation.validator = observer ∧
          run.prior = prior ∧
          run.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              observer).installedCommitAt run.output.reference.index =
            some run.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              observer).commitInstallSourceAt run.output.reference.index =
            some .localExecution := by
  apply fresh_adjacent_direct_range_records_local_commit_or_installed_next
    source runtime prefixMap pending direct work range.leaderAt
      startBeforeFinish observerInRange observerCorrect headAtStart
        baseAfterCommitHead
  · intro offset offsetInRange
    have offsetInCount : offset < count := by
      simpa only [countMatchesDepth] using offsetInRange
    exact range.leaderRound offset offsetInCount
  · intro offset offsetInRange
    have offsetInCount : offset < count := by
      simpa only [countMatchesDepth] using offsetInRange
    rw [range.leaderAuthor offset offsetInCount]
    exact firstSelectedAuthor offset offsetInCount
  · intro offset offsetInRange
    have offsetInCount : offset < count := by
      simpa only [countMatchesDepth] using offsetInRange
    exact range.toAcceptedVoteEvidence offset offsetInCount

end Mysticeti
