/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorRecoverySourcePinExecution

namespace Mysticeti

/-! Receiver-driven replay of one correct validator's current signed tip.

The receiver retries its block subscription. A broken or idle stream ends and
the receiver starts another subscription. On a successful subscription, the
sender snapshots either cached own blocks after the receiver's resume round or
its latest own block. Thus, the receiver gets the exact observed tip or a newer
own tip. A newer tip is useful progress because its immediate parents certify a
higher operational frontier.

This interface records an actual replay packet and its source snapshot. It does
not require proactive sender-side work for every peer. It does not state a
future quorum, layer, commit, or receiver acceptance result.
-/

/-- One actual block packet emitted by a successful subscription snapshot. -/
structure ValidatorSubscriptionReplayPacketAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start sender receiver : Time)
    (block : ValidatorBlock BlockId) where
  sentAt : Time
  packetId : PacketId
  packet : AddressedPacket (ValidatorMessage BlockId CommitId)
  startBeforeSend : start ≤ sentAt
  packetInTrace :
    (timed.execution.trace (sentAt + 1)).packets packetId = some packet
  packetIsProtocol : protocolPacket packet
  packetSender : packet.sender = sender
  packetReceiver : packet.receiver = receiver
  packetPayload : packet.payload = .block block
  packetSentAt : packet.sentAt = sentAt + 1
  blockAuthor : block.reference.author = sender
  blockCataloguedAtSend :
    (timed.execution.trace sentAt).blockCatalog block.reference.id = some block

/-- A successful subscription replays the exact observed tip, or a newer tip
which was current when the sender took the subscription snapshot. -/
inductive ValidatorCurrentTipSubscriptionDispositionAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start sender receiver : Time)
    (targetBlock : ValidatorBlock BlockId) : Prop where
  | exactReplay
      (replay : ValidatorSubscriptionReplayPacketAt timed start sender receiver
        targetBlock) :
      ValidatorCurrentTipSubscriptionDispositionAt timed start sender receiver
        targetBlock
  | newerTip
      (snapshotAt : Time)
      (replayedBlock : ValidatorBlock BlockId)
      (replay : ValidatorSubscriptionReplayPacketAt timed start sender receiver
        replayedBlock)
      (startBeforeSnapshot : start ≤ snapshotAt)
      (snapshotBeforeSend : snapshotAt ≤
        (ValidatorSubscriptionReplayPacketAt.sentAt replay))
      (targetBeforeReplay :
        targetBlock.reference.round < replayedBlock.reference.round)
      (replayIsSnapshotTip :
        ((timed.execution.trace snapshotAt).validatorState
          sender).highestSignedRound = replayedBlock.reference.round)
      :
      ValidatorCurrentTipSubscriptionDispositionAt timed start sender receiver
        targetBlock

/-- Receiver-driven subscription retry and snapshot behavior for a pinned
current tip.

The implementation mapping must cover stream establishment timeout, idle or
broken stream termination, retry, and the successful snapshot rule. -/
structure ValidatorCurrentTipSubscriptionExecution
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
    (pins : ValidatorRecoverySourcePinExecution syncRules) where
  currentPinnedTipHasSubscriptionDisposition :
    ∀ start sender receiver targetBlock capsuleId entry,
      sender < config.authorityCount →
      faults.correctAvailable sender = true →
      receiver < config.authorityCount →
      faults.correctAvailable receiver = true →
      receiver ≠ sender →
      network.gst ≤ start →
      (∀ time, start ≤ time →
        (timed.execution.trace time).epochActive = true) →
      0 < ((timed.execution.trace start).validatorState
        sender).highestSignedRound →
      ((timed.execution.trace start).validatorState sender).ownBlockAt
          ((timed.execution.trace start).validatorState
            sender).highestSignedRound = some targetBlock.reference →
      entry.capsule.targetBlock = targetBlock →
      (pins.trace start sender).capsuleAt capsuleId = some entry →
      (pins.trace start sender).pinned capsuleId = true →
      ValidatorCurrentTipSubscriptionDispositionAt timed start sender receiver
        targetBlock

end Mysticeti
