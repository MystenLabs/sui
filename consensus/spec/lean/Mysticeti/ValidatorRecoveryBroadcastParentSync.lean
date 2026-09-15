/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorBlockSyncBridge
import Mysticeti.ValidatorTracePacing

namespace Mysticeti

/-! Parent synchronization for one delivered recovery proposal.

A proposal packet can arrive before its causal parents. The receiver then keeps
the proposal in its suspended-block store. One correct holder retains a finite
parent-first history. The main-execution block-sync proof accepts that history.
The normal protected acceptance task then accepts the proposal.

This result does not use `ValidatorRecoveryBroadcastParentAvailability`. That
predicate requires every parent to be accepted at the exact packet-delivery
time. Parent synchronization after delivery cannot establish that predicate.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- One correct holder supplies the finite causal history needed by one
receiver for one proposal.

The source is protected only while the receiver has not accepted the complete
history. Each item remains a local request goal until that item is accepted.
The last field connects the supplied history to the proposal's direct parents.
-/
structure ValidatorBroadcastParentSyncSource
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    (snapshot : ValidatorProposalSnapshot config faults timed.execution.trace)
    (receiver holder : Nat) (blocks : List (ValidatorBlock BlockId))
    (start : Time) : Prop where
  history : RetainedValidatorBlockHistory timed.execution holder blocks start
  protectedWhileIncomplete : ∀ time,
    start ≤ time →
    (¬∀ block, block ∈ blocks →
      ((timed.execution.trace time).validatorState receiver).accepted
        block.reference = true) →
    ∀ block, block ∈ blocks →
      syncRules.sourceProtected holder block.reference time
  requiredUntilAccepted : ∀ block,
    block ∈ blocks →
    ∀ time, start ≤ time →
      ((timed.execution.trace time).validatorState receiver).accepted
          block.reference = false →
      ¬syncRules.goalObsolete receiver block.reference time
  parentFirst : ParentFirstValidatorBlockHistory
    (fun reference =>
      ((timed.execution.trace start).validatorState receiver).accepted
        reference = true)
    blocks
  coversDirectParents : ∀ parent,
    parent ∈ snapshot.block.parents →
    ((timed.execution.trace start).validatorState receiver).accepted parent =
        true ∨
      ∃ block, block ∈ blocks ∧ block.reference = parent

/-- A delivered stored proposal is eventually accepted after finite
parent-first synchronization.

The proposal can be buffered at delivery. The supplied source begins when that
delivery is visible. Block synchronization first accepts every missing parent.
The protected local acceptance task then accepts the proposal. -/
theorem completed_broadcast_parents_ready_then_processed_via_parent_sync
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    {snapshot : ValidatorProposalSnapshot config faults timed.execution.trace}
    {receiver holder : Nat} {blocks : List (ValidatorBlock BlockId)}
    {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (broadcast : ValidatorCompletedProposalBroadcast snapshot receiver packetId
      packet)
    (validParents : snapshot.block.HasQuorumImmediateParents config)
    (sentAfterGst : network.gst ≤ packet.sentAt)
    (active : ∀ time, packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (source : ValidatorBroadcastParentSyncSource syncRules snapshot receiver
      holder blocks (packet.deliveredAt + 1)) :
    ∃ parentsReadyAt acceptedAt,
      packet.deliveredAt + 1 ≤ parentsReadyAt ∧
      parentsReadyAt ≤ acceptedAt ∧
      (∀ parent, parent ∈ snapshot.block.parents →
        ((timed.execution.trace parentsReadyAt).validatorState receiver).accepted
          parent = true) ∧
      (((timed.execution.trace acceptedAt).validatorState receiver).accepted
          snapshot.block.reference = true ∨
        snapshot.block.reference.round ≤
          ((timed.execution.trace acceptedAt).validatorState receiver).gcRound) := by
  have deliveryBounds := network.postGstDelivery packet
    broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using snapshot.proposerInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using snapshot.proposerCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have delivered := timed.execution.protocolPacketsAreDelivered packetId packet
    broadcast.packetInTrace broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using snapshot.proposerInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using snapshot.proposerCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have packetAtDelivery := timed.execution.packetHistoryMonotone packet.sentAt
    packet.deliveredAt deliveryBounds.1 packetId packet broadcast.packetInTrace
  have syncStartsAfterGst : network.gst ≤ packet.deliveredAt + 1 := by
    exact Nat.le_trans sentAfterGst
      (Nat.le_trans deliveryBounds.1 (Nat.le_add_right _ _))
  rcases retained_parent_first_history_eventually_accepted syncRules
      source.history receiverInRange receiverCorrectAvailable syncStartsAfterGst
      active source.protectedWhileIncomplete source.requiredUntilAccepted
      source.parentFirst with
    ⟨parentsReadyAt, deliveryBeforeParentsReady, historyAccepted⟩
  have parentsAccepted : ∀ parent, parent ∈ snapshot.block.parents →
      ((timed.execution.trace parentsReadyAt).validatorState
        packet.receiver).accepted parent = true := by
    intro parent parentMember
    rw [broadcast.packetReceiver]
    rcases source.coversDirectParents parent parentMember with
      acceptedAtStart | ⟨block, blockMember, blockReference⟩
    · exact timed.execution.accepted_block_persists receiverInRange
        deliveryBeforeParentsReady acceptedAtStart
    · simpa [blockReference] using historyAccepted block blockMember
  by_cases blockAtRoot : snapshot.block.reference.round ≤
      ((timed.execution.trace parentsReadyAt).validatorState receiver).gcRound
  · exact ⟨parentsReadyAt, parentsReadyAt, deliveryBeforeParentsReady,
      Nat.le_refl _, by
        intro parent parentMember
        simpa [broadcast.packetReceiver] using parentsAccepted parent parentMember,
      Or.inr blockAtRoot⟩
  · have blockAboveGc :
        ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).gcRound < snapshot.block.reference.round := by
      rw [broadcast.packetReceiver]
      omega
    rcases delivered_block_with_ready_parents_is_accepted timed
        acceptanceRules packetAtDelivery broadcast.packetPayload delivered
        (by simpa [broadcast.packetReceiver] using receiverInRange)
        (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
        (by simpa [snapshot.blockIsOwnProposal] using snapshot.proposerInRange)
        (Or.inr validParents) deliveryBeforeParentsReady blockAboveGc
        parentsAccepted with
      ⟨acceptedAt, parentsBeforeAccepted, _acceptedWithinBound, accepted⟩
    refine ⟨parentsReadyAt, acceptedAt, deliveryBeforeParentsReady,
      parentsBeforeAccepted, ?_, Or.inl ?_⟩
    · intro parent parentMember
      simpa [broadcast.packetReceiver] using parentsAccepted parent parentMember
    · simpa [broadcast.packetReceiver] using accepted

/-- The result-only form of
`completed_broadcast_parents_ready_then_processed_via_parent_sync`. -/
theorem completed_broadcast_eventually_accepted_or_gc_root_via_parent_sync
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    {snapshot : ValidatorProposalSnapshot config faults timed.execution.trace}
    {receiver holder : Nat} {blocks : List (ValidatorBlock BlockId)}
    {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (broadcast : ValidatorCompletedProposalBroadcast snapshot receiver packetId
      packet)
    (validParents : snapshot.block.HasQuorumImmediateParents config)
    (sentAfterGst : network.gst ≤ packet.sentAt)
    (active : ∀ time, packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (source : ValidatorBroadcastParentSyncSource syncRules snapshot receiver
      holder blocks (packet.deliveredAt + 1)) :
    ∃ acceptedAt,
      packet.deliveredAt + 1 ≤ acceptedAt ∧
      (((timed.execution.trace acceptedAt).validatorState receiver).accepted
          snapshot.block.reference = true ∨
        snapshot.block.reference.round ≤
          ((timed.execution.trace acceptedAt).validatorState receiver).gcRound) := by
  rcases completed_broadcast_parents_ready_then_processed_via_parent_sync timed
      acceptanceRules syncRules receiverInRange receiverCorrectAvailable
      broadcast validParents sentAfterGst active source with
    ⟨parentsReadyAt, acceptedAt, deliveryBeforeParentsReady,
      parentsReadyBeforeAcceptance, _parentsAccepted, childResult⟩
  exact ⟨acceptedAt,
    Nat.le_trans deliveryBeforeParentsReady parentsReadyBeforeAcceptance,
    childResult⟩

/-- The recovery-proposal form of
`completed_broadcast_eventually_accepted_or_gc_root_via_parent_sync`. -/
theorem completed_recovery_broadcast_eventually_accepted_or_gc_root_via_parent_sync
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound validator receiver holder : Nat}
    {ready : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound validator}
    {snapshot : ValidatorProposalSnapshot config faults timed.execution.trace}
    {blocks : List (ValidatorBlock BlockId)} {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (proposal : ValidatorCompletedRecoveryProposal ready snapshot)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (broadcast : ValidatorCompletedProposalBroadcast snapshot receiver packetId
      packet)
    (sentAfterGst : network.gst ≤ packet.sentAt)
    (active : ∀ time, packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (source : ValidatorBroadcastParentSyncSource syncRules snapshot receiver
      holder blocks (packet.deliveredAt + 1)) :
    ∃ acceptedAt,
      packet.deliveredAt + 1 ≤ acceptedAt ∧
      (((timed.execution.trace acceptedAt).validatorState receiver).accepted
          snapshot.block.reference = true ∨
        snapshot.block.reference.round ≤
          ((timed.execution.trace acceptedAt).validatorState receiver).gcRound) := by
  exact completed_broadcast_eventually_accepted_or_gc_root_via_parent_sync timed
    acceptanceRules syncRules receiverInRange receiverCorrectAvailable
    broadcast proposal.validParents sentAfterGst active source

end Mysticeti
