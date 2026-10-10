/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.LeaderCoverage
import Mysticeti.ValidatorExecution
import Mysticeti.ValidatorExecutionLemmas
import Mysticeti.ValidatorParents
import Mysticeti.ValidatorPacing
import Mysticeti.ValidatorTimedExecution

namespace Mysticeti

/-! Local interfaces for the trace-to-anchor proof.

This file does not assume a block layer, a direct-vote quorum, or a usable
anchor. It defines the local facts that a later trace proof must compose.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

private instance validatorBlockRefDecidableEq
    {BlockId : Type} [DecidableEq BlockId] :
    DecidableEq (ValidatorBlockRef BlockId) := by
  intro left right
  cases left with
  | mk leftId leftAuthor leftRound =>
      cases right with
      | mk rightId rightAuthor rightRound =>
          by_cases sameId : leftId = rightId
          · subst rightId
            by_cases sameAuthor : leftAuthor = rightAuthor
            · subst rightAuthor
              by_cases sameRound : leftRound = rightRound
              · subst rightRound
                exact isTrue rfl
              · exact isFalse (by intro equal; cases equal; exact sameRound rfl)
            · exact isFalse (by intro equal; cases equal; exact sameAuthor rfl)
          · exact isFalse (by intro equal; cases equal; exact sameId rfl)

/-- One local recovery parent-selection snapshot and the block that uses it. -/
structure ValidatorProposalSnapshot
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) where
  proposer : Nat
  snapshotAt : Time
  storedAt : Time
  block : ValidatorBlock BlockId
  proposerInRange : proposer < config.authorityCount
  proposerCorrectAvailable : faults.correctAvailable proposer = true
  snapshotBeforeStore : snapshotAt ≤ storedAt
  recoveryTargetsProposalRound : ∃ recovery,
    ((trace snapshotAt).validatorState proposer).recovery = some recovery ∧
      recovery.targetRound = block.reference.round ∧
      recovery.alignmentWitness = none
  blockIsOwnProposal : block.reference.author = proposer
  blockStored :
    ((trace storedAt).validatorState proposer).ownBlockAt
        block.reference.round = some block.reference
  blockInCatalog :
    (trace storedAt).blockCatalog block.reference.id = some block
  parentAuthorsNodup : block.ParentAuthorsNodup
  parentsAreImmediate : block.ParentsAreImmediate
  parentsAcceptedAtSnapshot : ∀ parent,
    parent ∈ block.parents →
      ((trace snapshotAt).validatorState proposer).accepted parent = true

/-- One pointwise timing flow linked to actual trace state.

A later theorem must derive this witness from local action transitions and packet
creation. A final liveness theorem must not take a successful family of these
witnesses as an input. -/
structure ValidatorTimedBlockFlow
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    {timingProtocolPacket : Packet → Prop}
    {timingProtocolAction : LocalConsensusAction → Prop}
    {CommitPrefix : Type}
    (waits : CommonRoundWaitSchedule CommitPrefix)
    (commitHead : CommitPrefix) (round : Nat) where
  leader : Nat
  receiver : Nat
  leaderInRange : leader < config.authorityCount
  receiverInRange : receiver < config.authorityCount
  leaderCorrectAvailable : faults.correctAvailable leader = true
  receiverCorrectAvailable : faults.correctAvailable receiver = true
  leaderBlock : ValidatorBlock BlockId
  leaderBlockAuthor : leaderBlock.reference.author = leader
  leaderBlockRound : leaderBlock.reference.round = round
  leaderTimer : RecoveryTargetTimer commitHead round
  nextTimer : RecoveryTargetTimer commitHead (round + 1)
  timingPacket : Packet
  proposal : TimedLeaderProposal
    (protocolPacket := timingProtocolPacket)
    (protocolAction := timingProtocolAction)
    waits leaderTimer timingPacket
  acceptance : TimedBlockAcceptance
    (protocolAction := timingProtocolAction) timingPacket
  nextProposal : ValidatorProposalSnapshot config faults trace
  nextProposalOwner : nextProposal.proposer = receiver
  nextProposalRound : nextProposal.block.reference.round = round + 1
  nextProposalValidParents :
    nextProposal.block.HasQuorumImmediateParents config
  parentSelection : TimedParentSelection waits nextTimer
  parentSnapshotMatches :
    parentSelection.snapshotAt = nextProposal.snapshotAt
  addressedPacketId : PacketId
  addressedPacket : AddressedPacket (ValidatorMessage BlockId CommitId)
  addressedSender : addressedPacket.sender = leader
  addressedReceiver : addressedPacket.receiver = receiver
  addressedPayload : addressedPacket.payload = .block leaderBlock
  addressedSentAt : addressedPacket.sentAt = timingPacket.sentAt
  addressedDeliveredAt : addressedPacket.deliveredAt = timingPacket.deliveredAt
  packetInTrace :
    (trace addressedPacket.sentAt).packets addressedPacketId =
      some addressedPacket
  proposalStoredAtCompletion :
    ((trace proposal.action.completedAt).validatorState leader).ownBlockAt round =
      some leaderBlock.reference
  proposalSentAtCompletion :
    ((trace proposal.action.completedAt).validatorState leader).sentOwnBlockAt
        round = true
  acceptanceVisibleAtCompletion :
    ((trace acceptance.action.completedAt).validatorState receiver).accepted
        leaderBlock.reference = true

namespace ValidatorTimedBlockFlow

/-- A completed acceptance action stays visible at the later parent snapshot. -/
theorem acceptance_visible_at_parent_snapshot
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {timingProtocolPacket : Packet → Prop}
    {timingProtocolAction : LocalConsensusAction → Prop}
    {CommitPrefix : Type}
    {waits : CommonRoundWaitSchedule CommitPrefix}
    {commitHead : CommitPrefix} {round : Nat}
    (flow : ValidatorTimedBlockFlow config faults execution.trace
      (timingProtocolPacket := timingProtocolPacket)
      (timingProtocolAction := timingProtocolAction)
      waits commitHead round)
    (acceptanceBeforeSnapshot :
      flow.acceptance.action.completedAt ≤ flow.nextProposal.snapshotAt) :
    ((execution.trace flow.nextProposal.snapshotAt).validatorState
        flow.receiver).accepted flow.leaderBlock.reference = true := by
  exact execution.accepted_block_persists flow.receiverInRange
    acceptanceBeforeSnapshot flow.acceptanceVisibleAtCompletion

/-- The spread form of the pacing theorem makes an accepted correct leader block
visible in the actual local state before the next proposal selects parents. -/
theorem acceptance_visible_at_parent_snapshot_from_spread
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {timingProtocolPacket : Packet → Prop}
    {timingProtocolAction : LocalConsensusAction → Prop}
    {timingNetwork : PartialSynchrony timingProtocolPacket}
    (processing : BoundedLocalProcessing timingNetwork timingProtocolAction)
    {CommitPrefix : Type}
    {waits : CommonRoundWaitSchedule CommitPrefix}
    {commitHead : CommitPrefix} {round : Nat}
    (flow : ValidatorTimedBlockFlow config faults execution.trace
      (timingProtocolPacket := timingProtocolPacket)
      (timingProtocolAction := timingProtocolAction)
      waits commitHead round)
    (earliestRoundStart startSpread : Nat)
    (leaderStartWithinSpread :
      flow.leaderTimer.startedAt ≤ earliestRoundStart + startSpread)
    (nextTimerStartsAfterEarliestDeadline :
      earliestRoundStart + waits.wait commitHead round ≤
        flow.nextTimer.startedAt)
    (waitDominatesSpread : startSpread ≤ waits.wait commitHead round)
    (leaderTimerStartsAfterGst :
      timingNetwork.gst ≤ flow.leaderTimer.startedAt)
    (nextWaitCoversVisibility :
      waits.wait commitHead round +
          (processing.epsilon + timingNetwork.delta + processing.epsilon) ≤
        waits.wait commitHead (round + 1)) :
    ((execution.trace flow.nextProposal.snapshotAt).validatorState
        flow.receiver).accepted flow.leaderBlock.reference = true := by
  have completedBeforeParentSelection :=
    leader_acceptance_action_completes_before_parent_snapshot_from_spread
      processing waits flow.leaderTimer flow.nextTimer flow.timingPacket
      flow.proposal flow.acceptance flow.parentSelection earliestRoundStart
      startSpread leaderStartWithinSpread
      nextTimerStartsAfterEarliestDeadline waitDominatesSpread
      leaderTimerStartsAfterGst nextWaitCoversVisibility
  have completedBeforeSnapshot :
      flow.acceptance.action.completedAt ≤ flow.nextProposal.snapshotAt := by
    simpa [flow.parentSnapshotMatches] using completedBeforeParentSelection
  exact flow.acceptance_visible_at_parent_snapshot execution
    completedBeforeSnapshot

end ValidatorTimedBlockFlow

/-- Authors whose accepted next-round block contains the exact leader reference.
The set counts each author once, even if that author made more than one block. -/
def traceDirectVoters
    [DecidableEq BlockId]
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (observer : Nat) (leader : ValidatorBlockRef BlockId) : VoterSet :=
  fun voter =>
    match (world.validatorState observer).acceptedRepresentative
        (leader.round + 1) voter with
    | none => false
    | some childReference =>
        match world.blockCatalog childReference.id with
        | none => false
        | some child => decide
            (child.reference = childReference ∧
              childReference.author = voter ∧
              childReference.round = leader.round + 1 ∧
              leader ∈ child.parents)

/-- An accepted next-round child with the exact leader parent is one direct
voter. This is a local DAG fact, not a quorum result. -/
theorem accepted_child_with_leader_parent_is_direct_voter
    [DecidableEq BlockId]
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {observer voter : Nat}
    {leader childReference : ValidatorBlockRef BlockId}
    {child : ValidatorBlock BlockId}
    (representative :
      (world.validatorState observer).acceptedRepresentative
          (leader.round + 1) voter = some childReference)
    (catalog : world.blockCatalog childReference.id = some child)
    (childMatchesReference : child.reference = childReference)
    (childAuthor : childReference.author = voter)
    (childRound : childReference.round = leader.round + 1)
    (leaderIsParent : leader ∈ child.parents) :
    traceDirectVoters world observer leader voter = true := by
  simp [traceDirectVoters, representative, catalog, childMatchesReference,
    childAuthor, childRound, leaderIsParent]

/-- If every correct, available validator has one accepted next-round block that
votes for the exact leader block, those votes have quorum stake.

This is an internal aggregation lemma. A trace proof must derive the pointwise
`allVote` premise from delivery, pacing, and the local parent-selection rule. -/
theorem all_correct_available_children_vote_gives_quorum
    [DecidableEq BlockId]
    (faults : FixedFaultInterval config)
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {observer : Nat} {leader : ValidatorBlockRef BlockId}
    (allVote : ∀ voter,
      voter < config.authorityCount →
      faults.correctAvailable voter = true →
      traceDirectVoters world observer leader voter = true) :
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters world observer leader) := by
  have included : VoterSet.SubsetAt config.authorityCount
      faults.correctAvailable (traceDirectVoters world observer leader) := by
    intro voter voterInRange voterCorrectAvailable
    exact allVote voter voterInRange voterCorrectAvailable
  exact Nat.le_trans faults.correct_available_stake_is_quorum
    (weight_mono config.stake included)

/-- The pure local input to one FlexCommitter scan. Absolute rounds are mapped to
indexes through `firstPendingRound`. -/
structure ValidatorFlexInput where
  commitIndex : Nat
  firstPendingRound : Nat
  roundCount : Nat
  slotStatuses : Nat → List SelectedLeaderSlotStatus

namespace ValidatorFlexInput

/-- Convert the local input to the executable status-level FlexCommitter model. -/
def toFlexState (input : ValidatorFlexInput) : FlexCommitState :=
  flexCommitStateFromSlotStatuses input.commitIndex input.roundCount
    input.slotStatuses

/-- One absolute-round window is covered by the local pending range and contains
usable FlexCommitter anchors. This is a proof result, not an input record field. -/
def HasActualAnchorWindow
    (input : ValidatorFlexInput) (baseRound count : Nat) : Prop :=
  ∃ baseIndex,
    input.firstPendingRound + baseIndex = baseRound ∧
      baseIndex + count ≤ input.roundCount ∧
      FlexAnchorWindow input.toFlexState baseIndex count

end ValidatorFlexInput

/-- Local trace rules needed by the anchor bridge.

Each field describes one validator's parent selection or one local FlexCommitter
evaluation. No field says that a future proposal, vote quorum, or anchor exists. -/
structure ValidatorAnchorLocalRules
    [DecidableEq BlockId]
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) where
  /-- The pure FlexCommitter input read from one validator's local state. -/
  flexInputAt : Time → Nat → ValidatorFlexInput
  flexInputCommitIndexMatches : ∀ time observer,
    (flexInputAt time observer).commitIndex =
      (trace time).localCommitIndex observer
  flexInputRoundCountMatches : ∀ time observer,
    (flexInputAt time observer).roundCount =
      ((trace time).validatorState observer).committer.pendingRounds.length
  flexInputStatusesMatch : ∀ time observer,
    (flexInputAt time observer).slotStatuses =
      validatorPendingSlotStatusesAt
        ((trace time).validatorState observer).committer.pendingRounds
  /-- Pending rounds use consecutive absolute round numbers. This is one local
  array invariant. An index inside the array maps to exactly one round. -/
  flexInputRoundsMatch : ∀ time observer index,
    validatorPendingRoundAt
        ((trace time).validatorState observer).committer.pendingRounds index =
      if index < (flexInputAt time observer).roundCount then
        some ((flexInputAt time observer).firstPendingRound + index)
      else none
  /-- Accepting one correct-author block records the exact representative used
  for stake accounting. Equivocating authors need only one local branch. This is
  a one-validator state update. -/
  acceptedCorrectBlockRecordsRepresentative : ∀ time observer
      (block : ValidatorBlock BlockId),
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    block.reference.author < config.authorityCount →
    faults.correctAvailable block.reference.author = true →
    (trace time).blockCatalog block.reference.id = some block →
    ((trace time).validatorState observer).accepted block.reference = true →
    ((trace time).validatorState observer).acceptedRepresentative
        block.reference.round block.reference.author = some block.reference
  /-- Recovery parent selection includes a correct immediate parent that the
  proposer accepted before its snapshot. -/
  includesAcceptedCorrectImmediateParent : ∀
      (snapshot : ValidatorProposalSnapshot config faults trace)
      (parent : ValidatorBlockRef BlockId),
    parent.author < config.authorityCount →
    faults.correctAvailable parent.author = true →
    parent.round + 1 = snapshot.block.reference.round →
    ((trace snapshot.snapshotAt).validatorState snapshot.proposer).accepted
        parent = true →
    parent ∈ snapshot.block.parents
  /-- If a covered first selected leader has quorum direct-vote stake, the pure
  local direct decider puts `Commit` first in that round's status list. -/
  directQuorumCommitsCoveredFirstSlot : ∀
      time observer round (leader : ValidatorBlockRef BlockId),
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    (config.selectedLeaderOrder
        ((trace time).validatorState observer).commitHead.id round).head? =
      some leader.author →
    ((trace time).validatorState observer).accepted leader = true →
    (flexInputAt time observer).firstPendingRound ≤ round →
    round < (flexInputAt time observer).firstPendingRound +
      (flexInputAt time observer).roundCount →
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters (trace time) observer leader) →
    ∃ tail,
      (flexInputAt time observer).slotStatuses
          (round - (flexInputAt time observer).firstPendingRound) =
        .commit :: tail

namespace ValidatorAnchorLocalRules

/-- An accepted correct leader is included when the next correct validator
selects its recovery parents. This result uses only one parent snapshot. -/
theorem timed_flow_includes_visible_leader
    [DecidableEq BlockId]
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    {timingProtocolPacket : Packet → Prop}
    {timingProtocolAction : LocalConsensusAction → Prop}
    {CommitPrefix : Type}
    {waits : CommonRoundWaitSchedule CommitPrefix}
    {commitHead : CommitPrefix} {round : Nat}
    (rules : ValidatorAnchorLocalRules config faults trace)
    (flow : ValidatorTimedBlockFlow config faults trace
      (timingProtocolPacket := timingProtocolPacket)
      (timingProtocolAction := timingProtocolAction)
      waits commitHead round)
    (acceptedAtSnapshot :
      ((trace flow.nextProposal.snapshotAt).validatorState flow.receiver).accepted
        flow.leaderBlock.reference = true) :
    flow.leaderBlock.reference ∈ flow.nextProposal.block.parents := by
  apply rules.includesAcceptedCorrectImmediateParent flow.nextProposal
    flow.leaderBlock.reference
  · simpa [flow.leaderBlockAuthor] using flow.leaderInRange
  · simpa [flow.leaderBlockAuthor] using flow.leaderCorrectAvailable
  · rw [flow.leaderBlockRound, flow.nextProposalRound]
  · simpa [flow.nextProposalOwner] using acceptedAtSnapshot

/-- The pacing bounds and local parent rule make the correct leader an actual
parent of the next proposal. Timely parent inclusion is a result, not an input. -/
theorem timed_flow_includes_leader_from_spread
    [DecidableEq BlockId]
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {timingProtocolPacket : Packet → Prop}
    {timingProtocolAction : LocalConsensusAction → Prop}
    {timingNetwork : PartialSynchrony timingProtocolPacket}
    (processing : BoundedLocalProcessing timingNetwork timingProtocolAction)
    {CommitPrefix : Type}
    {waits : CommonRoundWaitSchedule CommitPrefix}
    {commitHead : CommitPrefix} {round : Nat}
    (rules : ValidatorAnchorLocalRules config faults execution.trace)
    (flow : ValidatorTimedBlockFlow config faults execution.trace
      (timingProtocolPacket := timingProtocolPacket)
      (timingProtocolAction := timingProtocolAction)
      waits commitHead round)
    (earliestRoundStart startSpread : Nat)
    (leaderStartWithinSpread :
      flow.leaderTimer.startedAt ≤ earliestRoundStart + startSpread)
    (nextTimerStartsAfterEarliestDeadline :
      earliestRoundStart + waits.wait commitHead round ≤
        flow.nextTimer.startedAt)
    (waitDominatesSpread : startSpread ≤ waits.wait commitHead round)
    (leaderTimerStartsAfterGst :
      timingNetwork.gst ≤ flow.leaderTimer.startedAt)
    (nextWaitCoversVisibility :
      waits.wait commitHead round +
          (processing.epsilon + timingNetwork.delta + processing.epsilon) ≤
        waits.wait commitHead (round + 1)) :
    flow.leaderBlock.reference ∈ flow.nextProposal.block.parents := by
  have acceptedAtSnapshot :=
    flow.acceptance_visible_at_parent_snapshot_from_spread execution processing
      earliestRoundStart startSpread leaderStartWithinSpread
      nextTimerStartsAfterEarliestDeadline waitDominatesSpread
      leaderTimerStartsAfterGst nextWaitCoversVisibility
  exact rules.timed_flow_includes_visible_leader flow acceptedAtSnapshot

/-- A proposal remains in the ghost block catalog at each later trace time. -/
theorem proposal_catalog_entry_persists
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (snapshot : ValidatorProposalSnapshot config faults execution.trace)
    {later : Time} (storedBeforeLater : snapshot.storedAt ≤ later) :
    (execution.trace later).blockCatalog snapshot.block.reference.id =
      some snapshot.block := by
  exact execution.blockCatalogMonotone snapshot.storedAt later storedBeforeLater
    snapshot.block.reference.id snapshot.block snapshot.blockInCatalog

/-- If an observer accepts one next-round proposal, that proposal supplies one
direct vote for each exact parent that it contains. This result counts only one
author and does not state a quorum result. -/
theorem accepted_proposal_with_parent_is_direct_voter
    [DecidableEq BlockId]
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorAnchorLocalRules config faults execution.trace)
    (snapshot : ValidatorProposalSnapshot config faults execution.trace)
    {observer later : Nat} {parent : ValidatorBlockRef BlockId}
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (storedBeforeLater : snapshot.storedAt ≤ later)
    (accepted :
      ((execution.trace later).validatorState observer).accepted
        snapshot.block.reference = true)
    (parentIncluded : parent ∈ snapshot.block.parents) :
    traceDirectVoters (execution.trace later) observer parent snapshot.proposer =
      true := by
  have catalog := proposal_catalog_entry_persists execution snapshot
    storedBeforeLater
  have childAuthorInRange :
      snapshot.block.reference.author < config.authorityCount := by
    simpa [snapshot.blockIsOwnProposal] using snapshot.proposerInRange
  have childAuthorCorrectAvailable :
      faults.correctAvailable snapshot.block.reference.author = true := by
    simpa [snapshot.blockIsOwnProposal] using snapshot.proposerCorrectAvailable
  have representative := rules.acceptedCorrectBlockRecordsRepresentative later
    observer snapshot.block observerInRange observerCorrectAvailable
    childAuthorInRange childAuthorCorrectAvailable catalog accepted
  have childRound := snapshot.parentsAreImmediate parent parentIncluded
  have representativeForParent :
      ((execution.trace later).validatorState observer).acceptedRepresentative
          (parent.round + 1) snapshot.proposer =
        some snapshot.block.reference := by
    simpa [childRound, snapshot.blockIsOwnProposal] using representative
  apply accepted_child_with_leader_parent_is_direct_voter
    representativeForParent catalog
  · rfl
  · exact snapshot.blockIsOwnProposal
  · exact childRound.symm
  · exact parentIncluded

/-- A direct-vote quorum for the covered first selected leader slot gives one
usable ordered anchor in the local FlexCommitter input. -/
theorem covered_first_slot_quorum_is_usable
    [DecidableEq BlockId]
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    (rules : ValidatorAnchorLocalRules config faults trace)
    {time observer round : Nat} {leader : ValidatorBlockRef BlockId}
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (firstSelected :
      (config.selectedLeaderOrder
          ((trace time).validatorState observer).commitHead.id round).head? =
        some leader.author)
    (leaderAccepted :
      ((trace time).validatorState observer).accepted leader = true)
    (roundCovered :
      (rules.flexInputAt time observer).firstPendingRound ≤ round ∧
        round < (rules.flexInputAt time observer).firstPendingRound +
          (rules.flexInputAt time observer).roundCount)
    (directQuorum :
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (trace time) observer leader)) :
    UsableAnchorOrder
      ((rules.flexInputAt time observer).slotStatuses
        (round - (rules.flexInputAt time observer).firstPendingRound)) := by
  rcases rules.directQuorumCommitsCoveredFirstSlot time observer round leader
      observerInRange observerCorrectAvailable firstSelected leaderAccepted
      roundCovered.1 roundCovered.2 directQuorum with ⟨tail, status⟩
  rw [status]
  simp [UsableAnchorOrder]

/-- Pointwise direct-vote quorums for a covered consecutive range give the
corresponding local FlexCommitter anchor window. -/
theorem covered_direct_quorum_range_gives_anchor_window
    [DecidableEq BlockId]
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    (rules : ValidatorAnchorLocalRules config faults trace)
    {time observer baseIndex count : Nat}
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (windowCovered :
      baseIndex + count ≤ (rules.flexInputAt time observer).roundCount)
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (firstSelected : ∀ offset,
      offset < count →
      (config.selectedLeaderOrder
          ((trace time).validatorState observer).commitHead.id
          ((rules.flexInputAt time observer).firstPendingRound + baseIndex +
            offset)).head? =
        some (leaderAt offset).author)
    (leaderAccepted : ∀ offset,
      offset < count →
      ((trace time).validatorState observer).accepted (leaderAt offset) = true)
    (directQuorum : ∀ offset,
      offset < count →
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (trace time) observer (leaderAt offset))) :
    FlexAnchorWindow (rules.flexInputAt time observer).toFlexState
      baseIndex count := by
  apply usable_orders_give_flex_anchor_window
  intro offset offsetInRange
  let firstRound := (rules.flexInputAt time observer).firstPendingRound
  let round := firstRound + baseIndex + offset
  have roundCovered :
      (rules.flexInputAt time observer).firstPendingRound ≤ round ∧
        round < (rules.flexInputAt time observer).firstPendingRound +
          (rules.flexInputAt time observer).roundCount := by
    dsimp [round, firstRound]
    constructor <;> omega
  have usable := rules.covered_first_slot_quorum_is_usable
    observerInRange observerCorrectAvailable
    (firstSelected offset offsetInRange)
    (leaderAccepted offset offsetInRange) roundCovered
    (directQuorum offset offsetInRange)
  have indexEquality :
      round - (rules.flexInputAt time observer).firstPendingRound =
        baseIndex + offset := by
    dsimp [round, firstRound]
    omega
  rw [← indexEquality]
  exact usable

end ValidatorAnchorLocalRules

/-- The trace-level target for the anchor bridge.

After every start time, each correct, available validator eventually has a
usable consecutive anchor window in its actual pending-round input. A theorem
for this target must start from network, local processing, leader coverage, and
one-validator rules. It must derive block delivery, parent inclusion, direct
vote stake, and the anchor window. -/
def TraceUsableAnchorWindowLiveness
    [DecidableEq BlockId]
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (rules : ValidatorAnchorLocalRules config faults trace)
    (anchorCount : Nat) : Prop :=
  ∀ start observer,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    ∃ finish baseRound,
      start ≤ finish ∧
        (rules.flexInputAt finish observer).HasActualAnchorWindow
          baseRound anchorCount

end Mysticeti
