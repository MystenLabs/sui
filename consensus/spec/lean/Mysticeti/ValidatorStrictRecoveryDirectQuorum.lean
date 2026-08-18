/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorExactNextRecovery
import Mysticeti.ValidatorReferenceFlexTrace

namespace Mysticeti

/-! Strict recovery proposals give exact FlexCommitter direct votes.

This module uses the main validator trace. It follows a persisted recovery
block through its finite source pin and the requester's protected block-sync
work. It does not assume a delivered block, timely parent inclusion, a direct
vote, a quorum layer, an anchor window, or a successful committer result.
-/

/-- Proposal persistence creates one exact source-local capsule. Its pin stays
active for the remaining active epoch, including across local commit changes. -/
theorem persisted_proposal_has_stable_capsule_source
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {persistTime author : Nat} {block : ValidatorBlock BlockId}
    (active : ∀ time, persistTime + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (persisted : ValidatorLocalActionOccurs (timed.execution.events persistTime)
      author (.persistProposal block)) :
    ∃ capsuleId entry,
      entry.capsule.targetBlock = block ∧
        entry.baselineCommit =
          ((timed.execution.trace (persistTime + 1)).validatorState
            author).commitHead ∧
        ∀ current, persistTime + 1 ≤ current →
          (pins.trace current author).capsuleAt capsuleId = some entry ∧
            (pins.trace current author).pinned capsuleId = true ∧
            CausalRecoveryCapsuleExecutionSource syncRules entry.capsule
              author current := by
  have authorFacts := validator_local_action_occurrence_is_correct_available
    (timed.execution.stepsFollowRules persistTime) persisted
  rcases pins.persistedProposalAddsCapsule persistTime author block authorFacts.1
      authorFacts.2 (active (persistTime + 1) (Nat.le_refl _)) persisted with
    ⟨capsuleId, entry, added, target, baseline⟩
  have step := pins.transitionsFollowRules persistTime author
  have stored := (step.capsuleUpdateExact capsuleId entry).2 (Or.inr added)
  have notReleased :
      (pins.event persistTime author).releaseCapsule capsuleId = false := by
    exact step.activeEpochPreventsRelease
      (active (persistTime + 1) (Nat.le_refl _)) capsuleId
  have addedSome :
      ((pins.event persistTime author).addCapsule capsuleId).isSome = true := by
    simp [added]
  have pinned := (step.pinUpdateExact capsuleId).2
    ⟨Or.inr addedSome, notReleased⟩
  refine ⟨capsuleId, entry, target, baseline, ?_⟩
  intro current startBeforeCurrent
  have stable := pins.pin_persists_while_epoch_active startBeforeCurrent
    stored pinned (by
      intro time startBeforeTime _timeBeforeCurrent
      exact active time startBeforeTime)
  exact ⟨stable.1, stable.2,
    pins.pinned_capsule_is_execution_source authorFacts.1 authorFacts.2
      stable.1 stable.2⟩

/-! ### Full direct-vote frontier in one later recovery block -/

/-- One concrete later block contains the exact direct-vote block from every
correct, available validator.

This is stronger than one quorum-intersection witness. A peer can fetch and
accept these exact parent bodies, then reconstruct the same direct-vote quorum.
-/
def ValidatorCorrectAvailableDirectVoteFrontier
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (observer : Nat)
    (leader : ValidatorBlockRef BlockId)
    (carrier : ValidatorBlock BlockId) : Prop :=
  ∀ voter,
    voter < config.authorityCount →
    faults.correctAvailable voter = true →
    ∃ voteBlock : ValidatorBlock BlockId,
      world.blockCatalog voteBlock.reference.id = some voteBlock ∧
        voteBlock.reference.author = voter ∧
        voteBlock.reference.round = leader.round + 1 ∧
        leader ∈ voteBlock.parents ∧
        (world.validatorState observer).acceptedRepresentative
            (leader.round + 1) voter = some voteBlock.reference ∧
        (world.validatorState observer).retained voteBlock.reference = true ∧
        voteBlock.reference ∈ carrier.parents ∧
        traceDirectVoters world observer leader voter = true

/-- Fresh recovery parent selection places the full correct, available
direct-vote frontier in one block after the voter layer. Its direct voters have
quorum stake by the static availability bound.

`votesReady` contains only one-host facts at the actual proposal snapshot. The
strict trace proof must derive these facts from the preceding common layers,
delivery, acceptance, and recovery retention. -/
theorem timer_paced_carrier_contains_full_direct_vote_quorum
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
    {validator round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round)
    (leader : ValidatorBlockRef BlockId)
    (carrierRound : round = leader.round + 2)
    (votesReady : ∀ voter,
      voter < config.authorityCount →
      faults.correctAvailable voter = true →
      ∃ voteBlock : ValidatorBlock BlockId,
        (timed.execution.trace production.snapshot.snapshotAt).blockCatalog
            voteBlock.reference.id = some voteBlock ∧
          voteBlock.reference.author = voter ∧
          voteBlock.reference.round = leader.round + 1 ∧
          leader ∈ voteBlock.parents ∧
          ((timed.execution.trace production.snapshot.snapshotAt).validatorState
              validator).acceptedRepresentative
                (leader.round + 1) voter = some voteBlock.reference ∧
          ((timed.execution.trace production.snapshot.snapshotAt).validatorState
              validator).retained voteBlock.reference = true) :
    ValidatorCorrectAvailableDirectVoteFrontier config faults
        (timed.execution.trace production.snapshot.snapshotAt) validator leader
        production.snapshot.block ∧
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters
            (timed.execution.trace production.snapshot.snapshotAt)
            validator leader) := by
  have targetRound : production.snapshot.block.reference.round =
      leader.round + 2 := by
    rw [production.blockRound, carrierRound]
  have parentRound : round - 1 = leader.round + 1 := by
    omega
  have allVotes : ∀ voter,
      voter < config.authorityCount →
      faults.correctAvailable voter = true →
      traceDirectVoters
        (timed.execution.trace production.snapshot.snapshotAt)
        validator leader voter = true := by
    intro voter voterInRange voterCorrect
    rcases votesReady voter voterInRange voterCorrect with
      ⟨voteBlock, catalog, voteAuthor, voteRound, leaderParent,
        representative, retained⟩
    exact accepted_child_with_leader_parent_is_direct_voter
      representative catalog rfl voteAuthor voteRound leaderParent
  constructor
  · intro voter voterInRange voterCorrect
    rcases votesReady voter voterInRange voterCorrect with
      ⟨voteBlock, catalog, voteAuthor, voteRound, leaderParent,
        representative, retained⟩
    have representativeAtParentRound :
        ((timed.execution.trace production.snapshot.snapshotAt).validatorState
            validator).acceptedRepresentative (round - 1) voter =
          some voteBlock.reference := by
      simpa [parentRound] using representative
    have included := timer_paced_round_includes_retained_current_parent
      production voterInRange representativeAtParentRound retained
    exact ⟨voteBlock, catalog, voteAuthor, voteRound, leaderParent,
      representative, retained, included,
      allVotes voter voterInRange voterCorrect⟩
  · exact all_correct_available_children_vote_gives_quorum faults allVotes

/-- If one correct peer accepts the exact parent frontier from the carrier, it
reconstructs the full direct-vote quorum for the same leader.

The premise is pointwise parent acceptance, not a quorum or anchor result. The
parent-first recovery sync and GC split must derive it for each parent above the
peer's cutoff. -/
theorem accepted_full_direct_vote_frontier_gives_peer_quorum
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
    (representatives : ValidatorAcceptedRepresentativeRules execution)
    {sourceTime finish sourceObserver peer : Nat}
    {leader : ValidatorBlockRef BlockId}
    {carrier : ValidatorBlock BlockId}
    (sourceBeforeFinish : sourceTime ≤ finish)
    (peerInRange : peer < config.authorityCount)
    (peerCorrect : faults.correctAvailable peer = true)
    (frontier : ValidatorCorrectAvailableDirectVoteFrontier config faults
      (execution.trace sourceTime) sourceObserver leader carrier)
    (frontierParentsAccepted : ∀ parent,
      parent ∈ carrier.parents →
        ((execution.trace finish).validatorState peer).accepted parent = true) :
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters (execution.trace finish) peer leader) := by
  apply all_correct_available_children_vote_gives_quorum faults
  intro voter voterInRange voterCorrect
  rcases frontier voter voterInRange voterCorrect with
    ⟨voteBlock, catalogAtSource, voteAuthor, voteRound, leaderParent,
      _sourceRepresentative, _sourceRetained, carrierParent, _sourceVote⟩
  have catalogAtFinish :
      (execution.trace finish).blockCatalog voteBlock.reference.id =
        some voteBlock :=
    execution.blockCatalogMonotone sourceTime finish sourceBeforeFinish
      voteBlock.reference.id voteBlock catalogAtSource
  have acceptedAtPeer := frontierParentsAccepted voteBlock.reference
    carrierParent
  have voterNotByzantine : faults.byzantine voter = false := by
    have notNonProgress : faults.nonProgress voter = false := by
      simpa [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full]
        using voterCorrect
    have separated : faults.byzantine voter = false ∧
        faults.unavailable voter = false := by
      simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
        notNonProgress
    exact separated.1
  have peerRepresentative :=
    representatives.acceptedCorrectReferenceIsRecorded finish peer
      voteBlock.reference peerInRange peerCorrect
      (by simpa [voteAuthor] using voterInRange)
      (by simpa [voteAuthor] using voterNotByzantine) acceptedAtPeer
  have representativeAtVoteRound :
      ((execution.trace finish).validatorState peer).acceptedRepresentative
          (leader.round + 1) voter = some voteBlock.reference := by
    simpa [voteAuthor, voteRound] using peerRepresentative
  exact accepted_child_with_leader_parent_is_direct_voter
    representativeAtVoteRound catalogAtFinish rfl voteAuthor voteRound
    leaderParent

end Mysticeti
