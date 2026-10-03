/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorFreshTimerReadyBridge
import Mysticeti.ValidatorReceiverRelativeCausalBacklog
import Mysticeti.ValidatorOperationalFrontierCollectiveSuccessor

namespace Mysticeti

/-! Source derivation for one delivered fresh timer-paced block.

Proposal persistence already creates a durable source pin. The block-sync
bridge already turns that pin, a post-GST proposal packet, and the receiver's
current genesis frontier into a protected parent-sync source. This module
connects those two results to the causal capsule used by the linear backlog
proof.

The only added local rule states that proposal persistence pins the exact same
causal-history list that the backlog projection names. It does not state a
future delivery, acceptance, proposal, layer, or commit.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- The persisted projection and the durable pin use one exact history.

`ValidatorRecoverySourcePinExecution.persistedProposalAddsCapsule` already
creates the entry and its pin. This rule only identifies that entry with the
static history projection used by the causal-backlog bound. Full capsule
equality is not required. -/
structure ValidatorPersistedCausalCapsulePinProjectionRules
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound) : Prop where
  persistedAdditionUsesProjectedHistory : ∀
    {time author : Nat} {block : ValidatorBlock BlockId}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId}
    {entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config},
    ValidatorLocalActionOccurs (timed.execution.events time) author
      (.persistProposal block) →
    (pins.event time author).addCapsule capsuleId = some entry →
    entry.capsule.targetBlock = block →
    entry.capsule.history = (admission.capsuleFor block).history

/-- Current local rules that derive a fresh block's sync source from its pin.

The operational-frontier map supplies only current genesis-root facts. The
projection rule supplies only the identity of the capsule added by an actual
past persistence action. The method below still needs the concrete post-GST
packet and active-epoch interval before it can produce a sync source. -/
structure ValidatorFreshRoundPinnedSyncSourceRules
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound) : Type where
  canonicalGenesisParents : List (ValidatorBlockRef BlockId)
  frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
    canonicalGenesisParents
  sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents
  projection : ValidatorPersistedCausalCapsulePinProjectionRules pins admission

/-- Erase timer-specific packet data and retain the exact completed block
packet used by the block-sync bridge. -/
theorem ValidatorTimerPacedPeerBroadcast.toCompletedBlockBroadcast
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {author receiver round : Nat}
    {production : ValidatorTimerPacedRoundProduction timed waits author round}
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt) :
    ValidatorCompletedBlockBroadcast timed production.snapshot.block author
      receiver broadcast.packetId broadcast.packet := by
  refine {
    packetInTrace := broadcast.packetInTrace
    packetIsProtocol := broadcast.packetIsProtocol
    packetSender := broadcast.packetSender
    packetReceiver := broadcast.packetReceiver
    packetPayload := broadcast.packetPayload
    blockAuthor := production.snapshot.blockIsOwnProposal.trans
      production.proposer
    blockCataloguedAtSend := ?_ }
  exact timed.execution.blockCatalogMonotone production.snapshot.storedAt
    broadcast.packet.sentAt broadcast.storedBeforeSend
      production.snapshot.block.reference.id production.snapshot.block
        production.snapshot.blockInCatalog

/-- One actual post-GST timer-paced proposal gets the protected parent-sync
source used by the linear receiver-backlog proof.

The source starts when packet delivery is visible. Its history is the exact
persisted projection. The pin supplies retained bodies and source protection;
the current operational frontier supplies the receiver's genesis roots. -/
theorem timer_paced_peer_broadcast_has_projected_parent_sync_source
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (projection : ValidatorPersistedCausalCapsulePinProjectionRules pins
      admission)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    {author receiver round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (active : ∀ time, production.persistTime + 1 ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorBlockParentSyncSource syncRules production.snapshot.block receiver
      author (admission.capsuleFor production.snapshot.block).history
        (broadcast.packet.deliveredAt + 1) := by
  have authorInRange : author < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have authorCorrect : faults.correctAvailable author = true := by
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  have activeAfterPersistence :
      (timed.execution.trace (production.persistTime + 1)).epochActive = true :=
    active _ (Nat.le_refl _)
  rcases pins.persistedProposalAddsCapsule production.persistTime author
      production.snapshot.block authorInRange authorCorrect
      activeAfterPersistence production.persistenceOccurs with
    ⟨capsuleId, entry, added, targetBlock, _baseline⟩
  have exactHistory :
      entry.capsule.history =
        (admission.capsuleFor production.snapshot.block).history :=
    projection.persistedAdditionUsesProjectedHistory
      production.persistenceOccurs added targetBlock
  have transition := pins.transitionsFollowRules production.persistTime author
  have stored :
      (pins.trace (production.persistTime + 1) author).capsuleAt capsuleId =
        some entry :=
    (transition.capsuleUpdateExact capsuleId entry).2 (Or.inr added)
  have addedIsSome :
      ((pins.event production.persistTime author).addCapsule capsuleId).isSome =
        true := by
    simp [added]
  have notReleased := transition.activeEpochPreventsRelease
    activeAfterPersistence capsuleId
  have pinned :
      (pins.trace (production.persistTime + 1) author).pinned capsuleId = true :=
    (transition.pinUpdateExact capsuleId).2
      ⟨Or.inr addedIsSome, notReleased⟩
  have sourceBeforeSend : production.persistTime + 1 ≤
      broadcast.packet.sentAt := by
    rw [← production.storedAfterPersistence]
    exact broadcast.storedBeforeSend
  have source :=
    pinned_successor_and_live_goals_give_block_parent_sync_source pins frontiers
      sameGenesis authorInRange authorCorrect receiverInRange receiverCorrect
        targetBlock stored pinned broadcast.toCompletedBlockBroadcast
          sourceBeforeSend sentAfterGst active
  simpa [exactHistory] using source

/-- The bundled current local rules derive the same pointwise source. -/
theorem ValidatorFreshRoundPinnedSyncSourceRules.sourceFor
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound}
    (rules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    {author receiver round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (active : ∀ time, production.persistTime + 1 ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorBlockParentSyncSource syncRules production.snapshot.block receiver
      author (admission.capsuleFor production.snapshot.block).history
        (broadcast.packet.deliveredAt + 1) :=
  timer_paced_peer_broadcast_has_projected_parent_sync_source pins admission
    rules.projection rules.frontiers rules.sameGenesis production broadcast
      receiverInRange receiverCorrect sentAfterGst active

end Mysticeti
