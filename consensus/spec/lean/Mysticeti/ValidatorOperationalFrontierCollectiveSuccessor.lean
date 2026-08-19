/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorOperationalFrontierPacemaker
import Mysticeti.ValidatorRecoveryBroadcastParentSync

namespace Mysticeti

/-! Collective operational-frontier progress.

This module is above the one-host frontier and pacemaker results. It separates
the finite arithmetic iteration from the distributed successor construction.
The final protocol proof must derive one strict successor from actual proposal
broadcasts, full dependency synchronization, and correct-stake aggregation.
No future layer is an end-to-end input.
-/

/-- One completed exact block packet without a recovery-mode snapshot.

Operational frontier progress can use a normal proposal or a replayed current
tip. This smaller packet record keeps the common delivery facts and does not
claim that either block came from a recovery timer. -/
structure ValidatorCompletedBlockBroadcast
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (block : ValidatorBlock BlockId) (sender receiver : Nat)
    (packetId : PacketId)
    (packet : AddressedPacket (ValidatorMessage BlockId CommitId)) : Prop where
  packetInTrace :
    (timed.execution.trace packet.sentAt).packets packetId = some packet
  packetIsProtocol : protocolPacket packet
  packetSender : packet.sender = sender
  packetReceiver : packet.receiver = receiver
  packetPayload : packet.payload = .block block
  blockAuthor : block.reference.author = sender
  blockCataloguedAtSend :
    (timed.execution.trace packet.sentAt).blockCatalog block.reference.id =
      some block

/-- One correct holder supplies a finite parent-first history for an exact
normal or replayed block packet.

Fetch completion is independent of commit installation. A returned dependency
is accepted when it remains above the current GC round. It is a completed GC
root when the current GC round has passed it. -/
structure ValidatorBlockParentSyncSource
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    (block : ValidatorBlock BlockId)
    (receiver holder : Nat) (blocks : List (ValidatorBlock BlockId))
    (start : Time) : Prop where
  history : RetainedValidatorBlockHistory timed.execution holder blocks start
  protectedWhileIncomplete : ∀ time,
    start ≤ time →
    (¬∀ item, item ∈ blocks →
      ((timed.execution.trace time).validatorState receiver).accepted
        item.reference = true) →
    ∀ item, item ∈ blocks →
      syncRules.sourceProtected holder item.reference time
  parentFirst : ParentFirstValidatorBlockHistory
    (fun reference =>
      ((timed.execution.trace start).validatorState receiver).accepted
        reference = true)
    blocks
  coversDirectParents : ∀ parent,
    parent ∈ block.parents →
    ((timed.execution.trace start).validatorState receiver).accepted parent =
        true ∨
      ∃ item, item ∈ blocks ∧ item.reference = parent

/-- One latched proposal send is a completed origin-neutral block packet. -/
theorem ValidatorLatchedProposalBroadcast.toCompletedBlockBroadcast
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
    {readyAt validator receiver : Nat}
    {proposal : ValidatorReadyProposal BlockId}
    (broadcast : ValidatorLatchedProposalBroadcast timed obligations readyAt
      validator receiver proposal)
    (blockAuthor : proposal.block.reference.author = validator) :
    ValidatorCompletedBlockBroadcast timed proposal.block validator receiver
      broadcast.packetId broadcast.packet := by
  refine {
    packetInTrace := ?_
    packetIsProtocol := broadcast.packetIsProtocol
    packetSender := broadcast.packetSender
    packetReceiver := broadcast.packetReceiver
    packetPayload := broadcast.packetPayload
    blockAuthor := blockAuthor
    blockCataloguedAtSend := ?_ }
  simpa only [broadcast.packetSentAt] using broadcast.packetInTrace
  have cataloguedAtSend := timed.execution.blockCatalogMonotone
    (broadcast.persistedAt + 1) (broadcast.sendActionAt + 1)
      (Nat.le_trans broadcast.persistenceBeforeSend (Nat.le_succ _))
        proposal.block.reference.id proposal.block broadcast.proposalCataloged
  simpa only [broadcast.packetSentAt] using cataloguedAtSend

/-- One origin-neutral persisted production has a completed exact block packet
for each other validator. -/
theorem persisted_production_has_completed_block_broadcast
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
    {start author receiver : Nat}
    (production : ValidatorPersistedProposalBroadcastProduction timed
      obligations start author)
    (receiverInRange : receiver < config.authorityCount)
    (differentReceiver : receiver ≠ author) :
    ∃ packetId packet,
      Nonempty (ValidatorCompletedBlockBroadcast timed
        production.proposal.block author receiver packetId packet) := by
  rcases production.broadcasts receiver receiverInRange differentReceiver with
    ⟨broadcast⟩
  exact ⟨broadcast.packetId, broadcast.packet,
    ⟨broadcast.toCompletedBlockBroadcast production.proposalAuthor⟩⟩

/-- One subscription replay packet is a completed origin-neutral block
broadcast. -/
theorem subscription_replay_packet_is_completed_block_broadcast
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start sender receiver : Time}
    {block : ValidatorBlock BlockId}
    (replay : ValidatorSubscriptionReplayPacketAt timed start sender receiver
      block) :
    Nonempty (ValidatorCompletedBlockBroadcast timed block sender receiver
      replay.packetId replay.packet) := by
  have cataloguedAtPacket := timed.execution.blockCatalogMonotone replay.sentAt
    replay.packet.sentAt (by simp [replay.packetSentAt]) block.reference.id
      block replay.blockCataloguedAtSend
  refine ⟨{
    packetInTrace := ?_
    packetIsProtocol := replay.packetIsProtocol
    packetSender := replay.packetSender
    packetReceiver := replay.packetReceiver
    packetPayload := replay.packetPayload
    blockAuthor := replay.blockAuthor
    blockCataloguedAtSend := cataloguedAtPacket }⟩
  simpa only [replay.packetSentAt] using replay.packetInTrace

/-- A newer tip returned by a successful subscription proves that the sender's
operational frontier already advanced beyond the requested successor. -/
theorem newer_subscription_tip_gives_later_layer
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
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    {start sender base snapshotAt : Time}
    {targetBlock replayedBlock : ValidatorBlock BlockId}
    (senderInRange : sender < config.authorityCount)
    (senderCorrectAvailable : faults.correctAvailable sender = true)
    (activeAtSnapshot :
      (timed.execution.trace snapshotAt).epochActive = true)
    (startBeforeSnapshot : start ≤ snapshotAt)
    (targetRound : targetBlock.reference.round = base + 1)
    (targetBeforeReplay :
      targetBlock.reference.round < replayedBlock.reference.round)
    (replayIsSnapshotTip :
      ((timed.execution.trace snapshotAt).validatorState
        sender).highestSignedRound = replayedBlock.reference.round) :
    ∃ finish round,
      start ≤ finish ∧
      base + 1 ≤ round ∧
      Nonempty (CorrectHeldTotalQuorumLayer config faults
        (timed.execution.trace finish) round) := by
  rcases frontiers.currentSource snapshotAt sender senderInRange
      senderCorrectAvailable activeAtSnapshot with ⟨source⟩
  have floorBound := operational_maximum_owner_signer_floor_le_successor pins
    sameGenesis senderInRange senderCorrectAvailable activeAtSnapshot source
  have targetBelowFloor : base + 1 <
      ((timed.execution.trace snapshotAt).validatorState
        sender).highestSignedRound := by
    rw [replayIsSnapshotTip, ← targetRound]
    exact targetBeforeReplay
  have successorAtMostFrontier : base + 1 ≤
      frontiers.frontier snapshotAt sender := by
    have belowFrontierSuccessor : base + 1 <
        frontiers.frontier snapshotAt sender + 1 :=
      Nat.lt_of_lt_of_le targetBelowFloor floorBound
    exact Nat.lt_succ_iff.mp (by
      simpa [Nat.succ_eq_add_one] using belowFrontierSuccessor)
  have frontierPositive : 0 < frontiers.frontier snapshotAt sender :=
    Nat.lt_of_lt_of_le (Nat.zero_lt_succ base) successorAtMostFrontier
  exact ⟨snapshotAt, frontiers.frontier snapshotAt sender,
    startBeforeSnapshot, successorAtMostFrontier,
    positive_operational_frontier_gives_correct_held_total_quorum_layer
      senderInRange senderCorrectAvailable frontierPositive source⟩

/-- One exact correct-authored successor block with its durable causal source
and one completed packet for every other validator.

The carrier can come from a new proposal or from replay of an already-signed
current tip. The two cases use the same downstream delivery proof. -/
structure ValidatorOperationalSuccessorCarrier
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
    (start author base : Nat) where
  sourceAt : Time
  sourceAfterStart : start ≤ sourceAt
  block : ValidatorBlock BlockId
  capsuleId : ValidatorRecoveryCapsuleKey BlockId
  entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config
  exactRound : block.reference.round = base + 1
  blockAuthor : block.reference.author = author
  validParents : block.HasQuorumImmediateParents config
  targetBlock : entry.capsule.targetBlock = block
  stored : (pins.trace sourceAt author).capsuleAt capsuleId = some entry
  pinned : (pins.trace sourceAt author).pinned capsuleId = true
  broadcasts : ∀ receiver,
    receiver < config.authorityCount →
    faults.correctAvailable receiver = true →
    receiver ≠ author →
    ∃ packetId packet,
      Nonempty (ValidatorCompletedBlockBroadcast timed block author receiver
        packetId packet) ∧
      sourceAt ≤ packet.sentAt

/-- Normalize a one-host pacemaker result into a later public layer or one
exact successor carrier. -/
theorem current_pacemaker_result_gives_later_layer_or_successor_carrier
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (subscription : ValidatorCurrentTipSubscriptionExecution pins)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    {start author base : Nat}
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (result : ValidatorOperationalMaximumCurrentPacemakerResult timed
      obligations pins start author base) :
    (∃ finish round,
      start ≤ finish ∧ base + 1 ≤ round ∧
        Nonempty (CorrectHeldTotalQuorumLayer config faults
          (timed.execution.trace finish) round)) ∨
      Nonempty (ValidatorOperationalSuccessorCarrier pins start author base) := by
  let laterExists := ∃ finish round,
    start ≤ finish ∧ base + 1 ≤ round ∧
      Nonempty (CorrectHeldTotalQuorumLayer config faults
        (timed.execution.trace finish) round)
  by_cases later : laterExists
  · exact Or.inl later
  · rcases result with produced | carrier
    · rcases produced with
        ⟨production, childAtSuccessor, _ownParentsAccepted⟩ |
        ⟨finish, round, startBeforeFinish, successorAtMostRound, layer⟩
      · right
        have activeAfterPersistence :
            (timed.execution.trace
              (production.persistedAt + 1)).epochActive = true :=
          active _ (Nat.le_trans production.startBeforePersistence
            (Nat.le_succ _))
        rcases pins.persisted_proposal_has_pinned_capsule_source authorInRange
            authorCorrectAvailable activeAfterPersistence
              production.persistenceOccurs with
          ⟨capsuleId, entry, targetBlock, stored, pinned, _source⟩
        have validParents :
            production.proposal.block.HasQuorumImmediateParents config := by
          rw [← targetBlock]
          exact entry.capsule.targetValid
        have startBeforeFinish : start ≤ production.finish :=
          Nat.le_trans production.startBeforePersistence
            (Nat.le_trans (Nat.le_succ _) production.persistenceBeforeFinish)
        have finishAfterGst : network.gst ≤ production.finish :=
          Nat.le_trans afterGst startBeforeFinish
        have activeFromFinish : ∀ time, production.finish ≤ time →
            (timed.execution.trace time).epochActive = true := by
          intro time finishBeforeTime
          exact active time (Nat.le_trans startBeforeFinish finishBeforeTime)
        have pinAtFinish := pins.pin_persists_while_epoch_active
          production.persistenceBeforeFinish stored pinned (by
            intro time persistenceBeforeTime _timeBeforeFinish
            exact active time (Nat.le_trans production.startBeforePersistence
              (Nat.le_trans (Nat.le_succ _) persistenceBeforeTime)))
        refine ⟨{
          sourceAt := production.finish
          sourceAfterStart := startBeforeFinish
          block := production.proposal.block
          capsuleId := capsuleId
          entry := entry
          exactRound := childAtSuccessor
          blockAuthor := production.proposalAuthor
          validParents := validParents
          targetBlock := targetBlock
          stored := pinAtFinish.1
          pinned := pinAtFinish.2
          broadcasts := ?_ }⟩
        intro receiver receiverInRange receiverCorrectAvailable
          differentReceiver
        have positiveTip : 0 <
            ((timed.execution.trace production.finish).validatorState
              author).highestSignedRound := by
          rw [production.signerFloorAtFinish, childAtSuccessor]
          exact Nat.zero_lt_succ base
        have ownTip :
            ((timed.execution.trace production.finish).validatorState
              author).ownBlockAt
                ((timed.execution.trace production.finish).validatorState
                  author).highestSignedRound =
                    some production.proposal.block.reference := by
          rw [production.signerFloorAtFinish]
          exact production.ownBlockStoredAtFinish
        cases subscription.currentPinnedTipHasSubscriptionDisposition
            production.finish author receiver production.proposal.block
              capsuleId entry authorInRange authorCorrectAvailable
                receiverInRange receiverCorrectAvailable differentReceiver
                  finishAfterGst activeFromFinish positiveTip ownTip targetBlock
                    pinAtFinish.1 pinAtFinish.2 with
        | exactReplay replay =>
            refine ⟨replay.packetId, replay.packet,
              subscription_replay_packet_is_completed_block_broadcast replay,
                ?_⟩
            rw [replay.packetSentAt]
            exact Nat.le_trans replay.startBeforeSend (Nat.le_succ _)
        | newerTip snapshotAt replayedBlock replay startBeforeSnapshot
            _snapshotBeforeSend targetBeforeReplay replayIsSnapshotTip =>
            have laterFromReplay := newer_subscription_tip_gives_later_layer
              pins frontiers sameGenesis authorInRange authorCorrectAvailable
                (activeFromFinish snapshotAt startBeforeSnapshot)
                  startBeforeSnapshot childAtSuccessor targetBeforeReplay
                    replayIsSnapshotTip
            exact False.elim (later (by
              rcases laterFromReplay with
                ⟨laterFinish, laterRound, finishBeforeLater,
                  successorAtMostLater, layer⟩
              exact ⟨laterFinish, laterRound,
                Nat.le_trans startBeforeFinish finishBeforeLater,
                  successorAtMostLater, layer⟩))
      · exact False.elim (later ⟨finish, round, startBeforeFinish,
          successorAtMostRound, layer⟩)
    · right
      have validParents : carrier.block.HasQuorumImmediateParents config := by
        rw [← carrier.targetBlock]
        exact carrier.entry.capsule.targetValid
      refine ⟨{
        sourceAt := start
        sourceAfterStart := Nat.le_refl _
        block := carrier.block
        capsuleId := carrier.capsuleKey
        entry := carrier.entry
        exactRound := carrier.exactRound
        blockAuthor := ?_
        validParents := validParents
        targetBlock := carrier.targetBlock
        stored := carrier.stored
        pinned := carrier.pinned
        broadcasts := ?_ }⟩
      · exact (timed.execution.statesWellFormed start author authorInRange
          ).ownBlockIsSound (base + 1) carrier.block.reference
            carrier.ownTip |>.1
      · intro receiver receiverInRange receiverCorrectAvailable
          differentReceiver
        cases carrier.subscriptionReplays receiver receiverInRange
            receiverCorrectAvailable differentReceiver with
        | exactReplay replay =>
            refine ⟨replay.packetId, replay.packet,
              subscription_replay_packet_is_completed_block_broadcast replay,
                ?_⟩
            rw [replay.packetSentAt]
            exact Nat.le_trans replay.startBeforeSend (Nat.le_succ _)
        | newerTip snapshotAt replayedBlock replay startBeforeSnapshot
            _snapshotBeforeSend targetBeforeReplay replayIsSnapshotTip =>
            have laterFromReplay := newer_subscription_tip_gives_later_layer
              pins frontiers sameGenesis authorInRange authorCorrectAvailable
                (active snapshotAt startBeforeSnapshot) startBeforeSnapshot
                  carrier.exactRound targetBeforeReplay replayIsSnapshotTip
            exact False.elim (later laterFromReplay)

/-- A pinned exact successor forms the parent-sync
source for one completed packet.

All holder storage and source protection are derived from the current pin.
Requester fetches can finish after a local commit. Response processing reads
the current GC round and either accepts the dependency or treats it as a GC
root. -/
theorem pinned_successor_and_live_goals_give_block_parent_sync_source
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
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    {sourceAt author receiver : Nat}
    {block : ValidatorBlock BlockId}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId}
    {entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config}
    {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (targetBlock : entry.capsule.targetBlock = block)
    (stored : (pins.trace sourceAt author).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace sourceAt author).pinned capsuleId = true)
    (broadcast : ValidatorCompletedBlockBroadcast timed block author receiver
      packetId packet)
    (sourceBeforeSend : sourceAt ≤ packet.sentAt)
    (sentAfterGst : network.gst ≤ packet.sentAt)
    (active : ∀ time, sourceAt ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorBlockParentSyncSource syncRules block receiver author
      entry.capsule.history (packet.deliveredAt + 1) := by
  have deliveryBounds := network.postGstDelivery packet
    broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using authorInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using authorCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have sourceBeforeSync : sourceAt ≤ packet.deliveredAt + 1 :=
    Nat.le_trans sourceBeforeSend
      (Nat.le_trans deliveryBounds.1 (Nat.le_add_right _ _))
  have pinAtSync := pins.pin_persists_while_epoch_active sourceBeforeSync
    stored pinned (by
      intro time sourceBeforeTime _timeBeforeSync
      exact active time sourceBeforeTime)
  have sourceAtSync := pins.pinned_capsule_is_execution_source authorInRange
    authorCorrectAvailable pinAtSync.1 pinAtSync.2
  have capsuleGenesis := pins.correctCapsuleUsesCanonicalGenesis sourceAt author
    capsuleId entry authorInRange authorCorrectAvailable stored
  rcases frontiers.currentSource (packet.deliveredAt + 1) receiver
      receiverInRange receiverCorrectAvailable
      (active _ sourceBeforeSync) with
    ⟨receiverFrontier⟩
  have genesisAccepted : ∀ reference,
      reference ∈ entry.capsule.genesisParents →
      ((timed.execution.trace (packet.deliveredAt + 1)).validatorState
        receiver).accepted reference = true := by
    intro reference member
    have canonicalMember : reference ∈ canonicalGenesisParents := by
      simpa [sameGenesis, capsuleGenesis] using member
    exact (receiverFrontier.canonicalGenesisReady.2.1 reference
      canonicalMember).2
  refine {
    history := causal_recovery_capsule_to_retained_validator_history
      sourceAtSync
    protectedWhileIncomplete := ?_
    parentFirst :=
      entry.capsule.parent_first_validator_history_from_capsule_genesis
        genesisAccepted
    coversDirectParents := ?_ }
  · intro time syncBeforeTime _incomplete item itemMember
    have sourceBeforeTime := Nat.le_trans sourceBeforeSync syncBeforeTime
    have pinAtTime := pins.pin_persists_while_epoch_active sourceBeforeTime
      stored pinned (by
        intro current sourceBeforeCurrent _currentBeforeTime
        exact active current sourceBeforeCurrent)
    exact pins.pinnedReferenceIsSourceProtected time author item.reference
      ⟨capsuleId, entry, item, pinAtTime.1, pinAtTime.2, itemMember, rfl⟩
  · intro parent parentMember
    have targetParent : parent ∈ entry.capsule.targetBlock.parents := by
      simpa [targetBlock] using parentMember
    rcases entry.capsule.targetParentsInHistory parent targetParent with
      genesisParent | ⟨parentBlock, parentBlockMember, parentReference⟩
    · exact Or.inl (genesisAccepted parent genesisParent)
    · exact Or.inr ⟨parentBlock, parentBlockMember, parentReference⟩

/-- Parent-first synchronization accepts one exact normal or replayed block,
or the receiver's GC frontier has already reached it. -/
theorem completed_block_broadcast_eventually_accepted_or_gc_root_via_parent_sync
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    {block : ValidatorBlock BlockId}
    {sender receiver holder : Nat} {blocks : List (ValidatorBlock BlockId)}
    {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (senderInRange : sender < config.authorityCount)
    (senderCorrectAvailable : faults.correctAvailable sender = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (broadcast : ValidatorCompletedBlockBroadcast timed block sender receiver
      packetId packet)
    (validParents : block.HasQuorumImmediateParents config)
    (sentAfterGst : network.gst ≤ packet.sentAt)
    (active : ∀ time, packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (source : ValidatorBlockParentSyncSource syncRules block receiver holder
      blocks (packet.deliveredAt + 1)) :
    ∃ acceptedAt,
      packet.deliveredAt + 1 ≤ acceptedAt ∧
      (((timed.execution.trace acceptedAt).validatorState receiver).accepted
          block.reference = true ∨
        block.reference.round ≤
          ((timed.execution.trace acceptedAt).validatorState receiver).gcRound) := by
  have deliveryBounds := network.postGstDelivery packet
    broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using senderInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using senderCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have delivered := timed.execution.protocolPacketsAreDelivered packetId packet
    broadcast.packetInTrace broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using senderInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using senderCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have packetAtDelivery := timed.execution.packetHistoryMonotone packet.sentAt
    packet.deliveredAt deliveryBounds.1 packetId packet broadcast.packetInTrace
  have syncStartsAfterGst : network.gst ≤ packet.deliveredAt + 1 :=
    Nat.le_trans sentAfterGst
      (Nat.le_trans deliveryBounds.1 (Nat.le_add_right _ _))
  have parentFirstReady : ParentFirstValidatorBlockHistory
      (ValidatorReferenceAcceptedOrGcRootAt timed.execution
        (packet.deliveredAt + 1) receiver) blocks := by
    exact parent_first_validator_block_history_mono (by
      intro reference accepted
      exact Or.inl accepted) source.parentFirst
  rcases retained_parent_first_history_eventually_ready syncRules
      source.history receiverInRange receiverCorrectAvailable
      syncStartsAfterGst active source.protectedWhileIncomplete
      parentFirstReady with
    ⟨parentsReadyAt, deliveryBeforeParentsReady, historyReady⟩
  have parentsReady : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).accepted parent = true ∨
        parent.round ≤
          ((timed.execution.trace parentsReadyAt).validatorState
            packet.receiver).gcRound := by
    intro parent parentMember
    rw [broadcast.packetReceiver]
    rcases source.coversDirectParents parent parentMember with
      acceptedAtStart | ⟨parentBlock, blockMember, blockReference⟩
    · exact Or.inl (timed.execution.accepted_block_persists receiverInRange
        deliveryBeforeParentsReady acceptedAtStart)
    · simpa [ValidatorReferenceAcceptedOrGcRootAt, blockReference] using
        historyReady parentBlock blockMember
  by_cases blockAtRoot : block.reference.round ≤
      ((timed.execution.trace parentsReadyAt).validatorState receiver).gcRound
  · exact ⟨parentsReadyAt, deliveryBeforeParentsReady, Or.inr blockAtRoot⟩
  · have blockAboveGc :
        ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).gcRound < block.reference.round := by
      rw [broadcast.packetReceiver]
      omega
    rcases delivered_block_with_current_gc_ready_parents_is_accepted timed
        acceptanceRules packetAtDelivery broadcast.packetPayload delivered
        (by simpa [broadcast.packetReceiver] using receiverInRange)
        (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
        (by simpa [broadcast.blockAuthor] using senderInRange)
        (Or.inr validParents) deliveryBeforeParentsReady blockAboveGc
        parentsReady with
      ⟨acceptedAt, parentsBeforeAccepted, _acceptedWithinBound, accepted⟩
    exact ⟨acceptedAt,
      Nat.le_trans deliveryBeforeParentsReady parentsBeforeAccepted,
        Or.inl (by simpa [broadcast.packetReceiver] using accepted)⟩

/-- Parent-first synchronization gives a concrete bound for one completed
block broadcast.

The bound charges one block-sync cost for each retained history item and one
local acceptance cost for the target block. If GC reaches the target first,
the result is the current GC-root branch. -/
theorem completed_block_broadcast_accepted_or_gc_root_within_parent_sync_bound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    {block : ValidatorBlock BlockId}
    {sender receiver holder : Nat} {blocks : List (ValidatorBlock BlockId)}
    {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (senderInRange : sender < config.authorityCount)
    (senderCorrectAvailable : faults.correctAvailable sender = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (broadcast : ValidatorCompletedBlockBroadcast timed block sender receiver
      packetId packet)
    (validParents : block.HasQuorumImmediateParents config)
    (sentAfterGst : network.gst ≤ packet.sentAt)
    (active : ∀ time, packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (source : ValidatorBlockParentSyncSource syncRules block receiver holder
      blocks (packet.deliveredAt + 1)) :
    ∃ acceptedAt,
      packet.deliveredAt + 1 ≤ acceptedAt ∧
      acceptedAt ≤ packet.deliveredAt + 1 +
          blocks.length * validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1 ∧
      (((timed.execution.trace acceptedAt).validatorState receiver).accepted
          block.reference = true ∨
        block.reference.round ≤
          ((timed.execution.trace acceptedAt).validatorState receiver).gcRound) := by
  have deliveryBounds := network.postGstDelivery packet
    broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using senderInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using senderCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have delivered := timed.execution.protocolPacketsAreDelivered packetId packet
    broadcast.packetInTrace broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using senderInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using senderCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have packetAtDelivery := timed.execution.packetHistoryMonotone packet.sentAt
    packet.deliveredAt deliveryBounds.1 packetId packet broadcast.packetInTrace
  have syncStartsAfterGst : network.gst ≤ packet.deliveredAt + 1 :=
    Nat.le_trans sentAfterGst
      (Nat.le_trans deliveryBounds.1 (Nat.le_add_right _ _))
  have parentFirstReady : ParentFirstValidatorBlockHistory
      (ValidatorReferenceAcceptedOrGcRootAt timed.execution
        (packet.deliveredAt + 1) receiver) blocks := by
    exact parent_first_validator_block_history_mono (by
      intro reference accepted
      exact Or.inl accepted) source.parentFirst
  rcases retained_parent_first_history_ready_within_length_bound syncRules
      source.history receiverInRange receiverCorrectAvailable
      syncStartsAfterGst active source.protectedWhileIncomplete
      parentFirstReady with
    ⟨parentsReadyAt, deliveryBeforeParentsReady, parentsReadyBound,
      historyReady⟩
  have parentsReady : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).accepted parent = true ∨
        parent.round ≤
          ((timed.execution.trace parentsReadyAt).validatorState
            packet.receiver).gcRound := by
    intro parent parentMember
    rw [broadcast.packetReceiver]
    rcases source.coversDirectParents parent parentMember with
      acceptedAtStart | ⟨parentBlock, blockMember, blockReference⟩
    · exact Or.inl (timed.execution.accepted_block_persists receiverInRange
        deliveryBeforeParentsReady acceptedAtStart)
    · simpa [ValidatorReferenceAcceptedOrGcRootAt, blockReference] using
        historyReady parentBlock blockMember
  by_cases blockAtRoot : block.reference.round ≤
      ((timed.execution.trace parentsReadyAt).validatorState receiver).gcRound
  · refine ⟨parentsReadyAt, deliveryBeforeParentsReady, ?_, Or.inr blockAtRoot⟩
    exact Nat.le_trans parentsReadyBound (by
      exact Nat.le_add_right _ (timed.localActionBound + 1))
  · have blockAboveGc :
        ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).gcRound < block.reference.round := by
      rw [broadcast.packetReceiver]
      omega
    rcases delivered_block_with_current_gc_ready_parents_is_accepted timed
        acceptanceRules packetAtDelivery broadcast.packetPayload delivered
        (by simpa [broadcast.packetReceiver] using receiverInRange)
        (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
        (by simpa [broadcast.blockAuthor] using senderInRange)
        (Or.inr validParents) deliveryBeforeParentsReady blockAboveGc
        parentsReady with
      ⟨acceptedAt, parentsBeforeAccepted, acceptedWithinBound, accepted⟩
    refine ⟨acceptedAt,
      Nat.le_trans deliveryBeforeParentsReady parentsBeforeAccepted, ?_,
        Or.inl (by simpa [broadcast.packetReceiver] using accepted)⟩
    exact Nat.le_trans acceptedWithinBound
      (Nat.add_le_add_right parentsReadyBound (timed.localActionBound + 1))

/-- One public layer is an attained accepted quorum at its correct holder. -/
theorem correct_held_total_quorum_layer_gives_accepted_quorum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {round : Nat}
    (layer : CorrectHeldTotalQuorumLayer config faults world round) :
    ValidatorAcceptedQuorumAt config (world.validatorState layer.holder)
      round := by
  refine ⟨layer.blocks.map (fun block => block.reference), ?_⟩
  refine ⟨?_, ?_, layer.blockStakeIsQuorum⟩
  · simpa only [List.map_map, Function.comp_def] using
      layer.blockAuthorsNodup
  intro reference referenceMember
  rcases List.mem_map.mp referenceMember with
    ⟨block, blockMember, blockReference⟩
  subst reference
  refine ⟨?_, layer.blocksAccepted block blockMember⟩
  rw [layer.blocksAtRound block blockMember]

/-- A public layer at one trace time is no later than that time's finite
correct-host operational maximum. -/
theorem correct_held_total_quorum_layer_le_operational_maximum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time round : Nat}
    (active : (timed.execution.trace time).epochActive = true)
    (layer : CorrectHeldTotalQuorumLayer config faults
      (timed.execution.trace time) round) :
    round ≤ correctOperationalQuorumFrontierMaximumUpTo frontiers time
      config.authorityCount := by
  rcases frontiers.currentSource time layer.holder layer.holderInRange
      layer.holderCorrectAvailable active with ⟨holderSource⟩
  have roundAtMostHolder := holderSource.upperBound round
    (correct_held_total_quorum_layer_gives_accepted_quorum layer)
  have holderAtMostMaximum :=
    correct_operational_quorum_frontier_le_maximum frontiers
      (time := time) layer.holderInRange layer.holderCorrectAvailable
  exact Nat.le_trans roundAtMostHolder holderAtMostMaximum

/-- A correct host's operational quorum frontier cannot decrease while its
accepted parent references persist. -/
theorem operational_quorum_frontier_mono
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {earlier later holder : Nat}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (ordered : earlier ≤ later)
    (activeEarlier : (timed.execution.trace earlier).epochActive = true)
    (activeLater : (timed.execution.trace later).epochActive = true) :
    frontiers.frontier earlier holder ≤ frontiers.frontier later holder := by
  rcases frontiers.currentSource earlier holder holderInRange
      holderCorrectAvailable activeEarlier with ⟨earlierSource⟩
  rcases frontiers.currentSource later holder holderInRange
      holderCorrectAvailable activeLater with ⟨laterSource⟩
  have acceptedLater : ValidatorAcceptedQuorumAt config
      ((timed.execution.trace later).validatorState holder)
        (frontiers.frontier earlier holder) := by
    refine ⟨earlierSource.quorum.references, ?_⟩
    refine ⟨earlierSource.quorum.ready.1, ?_,
      earlierSource.quorum.ready.2.2⟩
    intro parent parentMember
    have earlierParent := earlierSource.quorum.ready.2.1 parent parentMember
    exact ⟨earlierParent.1,
      timed.execution.accepted_block_persists holderInRange ordered
        earlierParent.2⟩
  exact laterSource.upperBound _ acceptedLater

/-- Acceptance of one exact successor block's causal closure puts the
receiver's operational frontier at or above the block's immediate-parent
round. If the parent round has crossed GC, the current frontier is already
strictly above that GC boundary. -/
theorem accepted_successor_closure_gives_operational_frontier_at_least
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time receiver parentRound : Nat}
    {block : ValidatorBlock BlockId}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (active : (timed.execution.trace time).epochActive = true)
    (blockRound : block.reference.round = parentRound + 1)
    (blockCatalogued :
      (timed.execution.trace time).blockCatalog block.reference.id = some block)
    (blockValid : block.HasQuorumImmediateParents config)
    (closure : ValidatorAcceptedCausalClosureAboveRound
      (timed.execution.trace time) receiver
        ((timed.execution.trace time).validatorState receiver).gcRound
          block.reference) :
    parentRound ≤ frontiers.frontier time receiver := by
  rcases frontiers.currentSource time receiver receiverInRange
      receiverCorrectAvailable active with ⟨receiverSource⟩
  by_cases parentAtOrBelowGc : parentRound ≤
      ((timed.execution.trace time).validatorState receiver).gcRound
  · by_cases parentPositive : 0 < parentRound
    · rcases receiverSource.aboveGcOrGenesis with
        ⟨frontierZero, gcZero⟩ | ⟨_frontierPositive, frontierAboveGc⟩
      · omega
      · omega
    · omega
  · have parentAboveGc :
        ((timed.execution.trace time).validatorState receiver).gcRound <
          parentRound := by omega
    have acceptedParents : ValidatorAcceptedQuorumAt config
        ((timed.execution.trace time).validatorState receiver) parentRound := by
      refine ⟨block.parents, ?_⟩
      refine ⟨blockValid.1, ?_, blockValid.2.2⟩
      intro parent parentMember
      have parentImmediate := blockValid.2.1 parent parentMember
      have parentExact : parent.round = parentRound := by
        rw [blockRound] at parentImmediate
        omega
      have parentAboveCutoff :
          ((timed.execution.trace time).validatorState receiver).gcRound <
            parent.round := by
        simpa [parentExact] using parentAboveGc
      have anchorAboveCutoff :
          ((timed.execution.trace time).validatorState receiver).gcRound <
            block.reference.round := by
        rw [blockRound]
        omega
      have path : ValidatorCausalClosureReferenceAboveRound
          (timed.execution.trace time)
          ((timed.execution.trace time).validatorState receiver).gcRound
          block.reference parent := by
        exact .parent (.anchor anchorAboveCutoff) blockCatalogued rfl
          parentMember parentAboveCutoff
      exact ⟨by omega, closure parent path⟩
    exact receiverSource.upperBound parentRound acceptedParents

/-- Acceptance of one exact successor, or collection of that successor by GC,
advances one receiver's operational state without using commit progress as an
outcome.

If the successor remains above GC and is accepted, its immediate parents give
the receiver an accepted quorum at `parentRound`. If it is already a GC root,
the receiver's current positive operational frontier is strictly later and
projects directly to a public layer. -/
theorem accepted_successor_or_gc_root_gives_receiver_frontier_or_later_layer
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time receiver parentRound : Nat}
    {block : ValidatorBlock BlockId}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (active : (timed.execution.trace time).epochActive = true)
    (blockRound : block.reference.round = parentRound + 1)
    (blockCatalogued :
      (timed.execution.trace time).blockCatalog block.reference.id = some block)
    (blockValid : block.HasQuorumImmediateParents config)
    (acceptedOrGc :
      ((timed.execution.trace time).validatorState receiver).accepted
          block.reference = true ∨
        block.reference.round ≤
          ((timed.execution.trace time).validatorState receiver).gcRound) :
    ((((timed.execution.trace time).validatorState receiver).accepted
          block.reference = true ∧
        parentRound ≤ frontiers.frontier time receiver) ∨
      ∃ round,
        parentRound + 1 ≤ round ∧
          Nonempty (CorrectHeldTotalQuorumLayer config faults
            (timed.execution.trace time) round)) := by
  rcases frontiers.currentSource time receiver receiverInRange
      receiverCorrectAvailable active with ⟨receiverSource⟩
  rcases acceptedOrGc with accepted | atGc
  · left
    refine ⟨accepted, ?_⟩
    by_cases parentAtOrBelowGc : parentRound ≤
        ((timed.execution.trace time).validatorState receiver).gcRound
    · rcases receiverSource.aboveGcOrGenesis with
        ⟨frontierZero, _gcZero⟩ |
          ⟨_frontierPositive, frontierAboveGc⟩
      · omega
      · omega
    · have parentAboveGc :
          ((timed.execution.trace time).validatorState receiver).gcRound <
            parentRound := by
        omega
      have acceptedParents : ValidatorAcceptedQuorumAt config
          ((timed.execution.trace time).validatorState receiver) parentRound := by
        refine ⟨block.parents, ?_⟩
        refine ⟨blockValid.1, ?_, blockValid.2.2⟩
        intro parent parentMember
        have parentImmediate := blockValid.2.1 parent parentMember
        have parentExact : parent.round = parentRound := by
          rw [blockRound] at parentImmediate
          omega
        have parentAboveGc' :
            ((timed.execution.trace time).validatorState receiver).gcRound <
              parent.round := by
          simpa [parentExact] using parentAboveGc
        have parentAccepted :=
          receiverSource.acceptedCausalClosure
            |>.acceptedBodyHasAcceptedParentsAboveGc block.reference block
              parent blockCatalogued rfl accepted parentMember parentAboveGc'
        exact ⟨by simp [parentExact], parentAccepted⟩
      exact receiverSource.upperBound parentRound acceptedParents
  · right
    have successorAtMostGc : parentRound + 1 ≤
        ((timed.execution.trace time).validatorState receiver).gcRound := by
      simpa [blockRound] using atGc
    rcases receiverSource.aboveGcOrGenesis with
      ⟨_frontierZero, gcZero⟩ |
        ⟨frontierPositive, frontierAboveGc⟩
    · omega
    · refine ⟨frontiers.frontier time receiver, ?_, ?_⟩
      · omega
      · exact
          positive_operational_frontier_gives_correct_held_total_quorum_layer
            receiverInRange receiverCorrectAvailable frontierPositive
              receiverSource

/-- Parent-first synchronization of one completed successor broadcast gives
the receiver's accepted parent frontier, or exposes a public layer strictly
after that parent frontier.

The retained parent-sync source is an internal construction obligation. This
theorem does not add it to the end-to-end input record. -/
theorem completed_successor_broadcast_gives_receiver_frontier_or_later_layer
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {snapshot : ValidatorProposalSnapshot config faults timed.execution.trace}
    {receiver holder parentRound : Nat} {blocks : List (ValidatorBlock BlockId)}
    {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (broadcast : ValidatorCompletedProposalBroadcast snapshot receiver packetId
      packet)
    (blockRound : snapshot.block.reference.round = parentRound + 1)
    (validParents : snapshot.block.HasQuorumImmediateParents config)
    (sentAfterGst : network.gst ≤ packet.sentAt)
    (active : ∀ time, packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (source : ValidatorBroadcastParentSyncSource syncRules snapshot receiver
      holder blocks (packet.deliveredAt + 1)) :
    ∃ acceptedAt,
      packet.deliveredAt + 1 ≤ acceptedAt ∧
        (((timed.execution.trace acceptedAt).validatorState receiver).accepted
              snapshot.block.reference = true ∧
            parentRound ≤ frontiers.frontier acceptedAt receiver ∨
          ∃ round,
            parentRound + 1 ≤ round ∧
              Nonempty (CorrectHeldTotalQuorumLayer config faults
                (timed.execution.trace acceptedAt) round)) := by
  rcases completed_broadcast_eventually_accepted_or_gc_root_via_parent_sync
      timed acceptanceRules syncRules receiverInRange receiverCorrectAvailable
        broadcast validParents sentAfterGst active source with
    ⟨acceptedAt, deliveryBeforeAccepted, acceptedOrGc⟩
  have catalogued := timed.execution.blockCatalogMonotone snapshot.storedAt
    acceptedAt (Nat.le_trans broadcast.storedBeforeSend
      (Nat.le_trans (network.postGstDelivery packet broadcast.packetIsProtocol
        (by simpa [broadcast.packetSender] using snapshot.proposerInRange)
        (by simpa [broadcast.packetReceiver] using receiverInRange)
        (by simpa [broadcast.packetSender] using
          snapshot.proposerCorrectAvailable)
        (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
        sentAfterGst).1
          (Nat.le_trans (Nat.le_add_right packet.deliveredAt 1)
            deliveryBeforeAccepted)))
      snapshot.block.reference.id snapshot.block snapshot.blockInCatalog
  exact ⟨acceptedAt, deliveryBeforeAccepted,
    accepted_successor_or_gc_root_gives_receiver_frontier_or_later_layer
      frontiers receiverInRange receiverCorrectAvailable
        (active acceptedAt deliveryBeforeAccepted) blockRound catalogued
          validParents acceptedOrGc⟩

/-- The origin-neutral block-packet form of the receiver successor theorem. -/
theorem completed_successor_block_broadcast_gives_receiver_frontier_or_later_layer
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {block : ValidatorBlock BlockId}
    {sender receiver holder parentRound : Nat}
    {blocks : List (ValidatorBlock BlockId)}
    {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (senderInRange : sender < config.authorityCount)
    (senderCorrectAvailable : faults.correctAvailable sender = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (broadcast : ValidatorCompletedBlockBroadcast timed block sender receiver
      packetId packet)
    (blockRound : block.reference.round = parentRound + 1)
    (validParents : block.HasQuorumImmediateParents config)
    (sentAfterGst : network.gst ≤ packet.sentAt)
    (active : ∀ time, packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (source : ValidatorBlockParentSyncSource syncRules block receiver holder
      blocks (packet.deliveredAt + 1)) :
    ∃ acceptedAt,
      packet.deliveredAt + 1 ≤ acceptedAt ∧
        (((timed.execution.trace acceptedAt).validatorState receiver).accepted
              block.reference = true ∧
            parentRound ≤ frontiers.frontier acceptedAt receiver ∨
          ∃ round,
            parentRound + 1 ≤ round ∧
              Nonempty (CorrectHeldTotalQuorumLayer config faults
                (timed.execution.trace acceptedAt) round)) := by
  rcases completed_block_broadcast_eventually_accepted_or_gc_root_via_parent_sync
      timed acceptanceRules syncRules senderInRange senderCorrectAvailable
        receiverInRange receiverCorrectAvailable broadcast validParents
          sentAfterGst active source with
    ⟨acceptedAt, deliveryBeforeAccepted, acceptedOrGc⟩
  have sentBeforeAccepted : packet.sentAt ≤ acceptedAt := by
    have deliveryBounds := network.postGstDelivery packet
      broadcast.packetIsProtocol
      (by simpa [broadcast.packetSender] using senderInRange)
      (by simpa [broadcast.packetReceiver] using receiverInRange)
      (by simpa [broadcast.packetSender] using senderCorrectAvailable)
      (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
      sentAfterGst
    exact Nat.le_trans deliveryBounds.1
      (Nat.le_trans (Nat.le_add_right packet.deliveredAt 1)
        deliveryBeforeAccepted)
  have catalogued := timed.execution.blockCatalogMonotone packet.sentAt
    acceptedAt sentBeforeAccepted block.reference.id block
      broadcast.blockCataloguedAtSend
  exact ⟨acceptedAt, deliveryBeforeAccepted,
    accepted_successor_or_gc_root_gives_receiver_frontier_or_later_layer
      frontiers receiverInRange receiverCorrectAvailable
        (active acceptedAt deliveryBeforeAccepted) blockRound catalogued
          validParents acceptedOrGc⟩

/-- Exact accepted successor blocks from every correct, available author give
one attained operational frontier at the successor round or later.

The blocks need not remain retained until the finite aggregation time. Their
accepted references persist. The holder's current operational-frontier source
then supplies an attained retained quorum at its current frontier. -/
theorem accepted_correct_successors_give_later_operational_layer
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {time holder base : Nat}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (active : (timed.execution.trace time).epochActive = true)
    (defaultBlock : ValidatorBlock BlockId)
    (accepted : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ block : ValidatorBlock BlockId,
        block.reference.author = author ∧
        block.reference.round = base + 1 ∧
        ((timed.execution.trace time).validatorState holder).accepted
          block.reference = true) :
    ∃ round,
      base + 1 ≤ round ∧
      Nonempty (CorrectHeldTotalQuorumLayer config faults
        (timed.execution.trace time) round) := by
  let authors := scheduledCorrectValidators config faults
  let blockFor : Nat → ValidatorBlock BlockId := fun author =>
    if h : author < config.authorityCount ∧
        faults.correctAvailable author = true then
      Classical.choose (accepted author h.1 h.2)
    else
      defaultBlock
  have blockForFacts : ∀ author, author ∈ authors →
      (blockFor author).reference.author = author ∧
      (blockFor author).reference.round = base + 1 ∧
      ((timed.execution.trace time).validatorState holder).accepted
        (blockFor author).reference = true := by
    intro author authorMember
    have authorFacts : author < config.authorityCount ∧
        faults.correctAvailable author = true := by
      simpa [authors, scheduledCorrectValidators] using authorMember
    simpa only [blockFor, dif_pos authorFacts] using
      Classical.choose_spec (accepted author authorFacts.1 authorFacts.2)
  let references := authors.map (fun author => (blockFor author).reference)
  have acceptedQuorum : ValidatorAcceptedQuorumAt config
      ((timed.execution.trace time).validatorState holder) (base + 1) := by
    refine ⟨references, ?_⟩
    refine ⟨?_, ?_, ?_⟩
    · have authorMap : authors.map
          (fun author => (blockFor author).reference.author) = authors := by
        have mapped := List.map_congr_left (l := authors)
          (f := fun author => (blockFor author).reference.author)
          (g := id) (by
            intro author authorMember
            exact (blockForFacts author authorMember).1)
        simpa using mapped
      have referenceAuthors : references.map ValidatorBlockRef.author =
          authors := by
        simpa [references, List.map_map, Function.comp_def] using authorMap
      rw [referenceAuthors]
      exact scheduled_correct_validators_nodup config faults
    · intro reference referenceMember
      rcases List.mem_map.mp referenceMember with
        ⟨author, authorMember, referenceExact⟩
      subst reference
      exact ⟨by
        have roundExact := (blockForFacts author authorMember).2.1
        omega,
        (blockForFacts author authorMember).2.2⟩
    · apply Nat.le_trans faults.correct_available_stake_is_quorum
      apply weight_mono config.stake
      intro author authorInRange authorCorrectAvailable
      have authorMember := mem_scheduled_correct_validators config faults
        authorInRange authorCorrectAvailable
      have selectedReference : (blockFor author).reference ∈ references := by
        exact List.mem_map.mpr ⟨author, authorMember, rfl⟩
      have authorExact := (blockForFacts author authorMember).1
      simp only [validatorParentAuthors, List.any_eq_true]
      exact ⟨(blockFor author).reference, selectedReference, by
        simp [authorExact]⟩
  rcases frontiers.currentSource time holder holderInRange
      holderCorrectAvailable active with ⟨holderSource⟩
  have successorAtMostFrontier : base + 1 ≤ frontiers.frontier time holder :=
    holderSource.upperBound (base + 1) acceptedQuorum
  have frontierPositive : 0 < frontiers.frontier time holder :=
    Nat.lt_of_lt_of_le (Nat.zero_lt_succ base) successorAtMostFrontier
  exact ⟨frontiers.frontier time holder, successorAtMostFrontier,
    positive_operational_frontier_gives_correct_held_total_quorum_layer
      holderInRange holderCorrectAvailable frontierPositive holderSource⟩

/-- One exact successor carrier reaches one correct receiver, or the receiver's
GC state already exposes a public layer at the successor round or later.

The self-receiver case uses the carrier's local pin. The remote case uses the
completed packet, parent-first fetch, and current-GC response processing. -/
theorem operational_successor_carrier_reaches_receiver_or_later_layer
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
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    {carrierStart author receiver base : Nat}
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (afterGst : network.gst ≤ carrierStart)
    (active : ∀ time, carrierStart ≤ time →
      (timed.execution.trace time).epochActive = true)
    (carrier : ValidatorOperationalSuccessorCarrier pins carrierStart author
      base) :
    ∃ finish,
      carrierStart ≤ finish ∧
      ((((timed.execution.trace finish).validatorState receiver).accepted
            carrier.block.reference = true ∧
          base ≤ frontiers.frontier finish receiver) ∨
        ∃ round,
          base + 1 ≤ round ∧
          Nonempty (CorrectHeldTotalQuorumLayer config faults
            (timed.execution.trace finish) round)) := by
  by_cases self : receiver = author
  · subst receiver
    have targetMember : carrier.block ∈ carrier.entry.capsule.history := by
      simpa [carrier.targetBlock] using
        carrier.entry.capsule.target_and_parents_in_history.1
    have localTarget := pins.pinnedHistoryIsLocal carrier.sourceAt author
      carrier.capsuleId carrier.entry carrier.stored carrier.pinned carrier.block
        targetMember
    rcases frontiers.currentSource carrier.sourceAt author authorInRange
        authorCorrectAvailable
          (active carrier.sourceAt carrier.sourceAfterStart) with
      ⟨authorSource⟩
    have parentsReady := pinned_current_tip_parents_are_ready pins sameGenesis
      authorInRange authorCorrectAvailable authorSource carrier.stored
        carrier.pinned carrier.targetBlock
    have acceptedParentQuorum : ValidatorAcceptedQuorumAt config
        ((timed.execution.trace carrier.sourceAt).validatorState author) base := by
      refine ⟨carrier.block.parents, ?_⟩
      simpa only [carrier.exactRound] using parentsReady
    refine ⟨carrier.sourceAt, carrier.sourceAfterStart, Or.inl
      ⟨localTarget.1, ?_⟩⟩
    exact authorSource.upperBound base acceptedParentQuorum
  · rcases carrier.broadcasts receiver receiverInRange
      receiverCorrectAvailable self with
      ⟨packetId, packet, ⟨broadcast⟩, sourceBeforeSend⟩
    have sentAfterGst : network.gst ≤ packet.sentAt :=
      Nat.le_trans afterGst
        (Nat.le_trans carrier.sourceAfterStart sourceBeforeSend)
    have activeFromSource : ∀ time, carrier.sourceAt ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time sourceBeforeTime
      exact active time (Nat.le_trans carrier.sourceAfterStart sourceBeforeTime)
    have source := pinned_successor_and_live_goals_give_block_parent_sync_source
      pins frontiers sameGenesis authorInRange authorCorrectAvailable
        receiverInRange receiverCorrectAvailable carrier.targetBlock
          carrier.stored carrier.pinned broadcast sourceBeforeSend sentAfterGst
            activeFromSource
    have deliveryBounds := network.postGstDelivery packet
      broadcast.packetIsProtocol
      (by simpa [broadcast.packetSender] using authorInRange)
      (by simpa [broadcast.packetReceiver] using receiverInRange)
      (by simpa [broadcast.packetSender] using authorCorrectAvailable)
      (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
      sentAfterGst
    have startBeforeDelivery : carrierStart ≤ packet.deliveredAt + 1 :=
      Nat.le_trans carrier.sourceAfterStart
        (Nat.le_trans sourceBeforeSend
          (Nat.le_trans deliveryBounds.1 (Nat.le_add_right _ _)))
    have activeFromDelivery : ∀ time, packet.deliveredAt + 1 ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time deliveryBeforeTime
      exact active time (Nat.le_trans startBeforeDelivery deliveryBeforeTime)
    rcases completed_successor_block_broadcast_gives_receiver_frontier_or_later_layer
        timed acceptanceRules syncRules frontiers authorInRange
          authorCorrectAvailable receiverInRange receiverCorrectAvailable
            broadcast carrier.exactRound carrier.validParents sentAfterGst
              activeFromDelivery source with
      ⟨finish, deliveryBeforeFinish, result⟩
    exact ⟨finish,
      Nat.le_trans startBeforeDelivery deliveryBeforeFinish, result⟩

/-- One common successor carrier brings one correct host to frontier `base`.
That host then produces or replays its own exact successor, unless its current
frontier already exposes a later public layer. -/
theorem operational_successor_carrier_gives_host_successor_or_later_layer
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (tipSubscription : ValidatorCurrentTipSubscriptionExecution pins)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (pacemaker : ValidatorNormalFrontierPacemakerRules timed)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start sourceAuthor receiver base : Nat}
    (sourceAuthorInRange : sourceAuthor < config.authorityCount)
    (sourceAuthorCorrectAvailable :
      faults.correctAvailable sourceAuthor = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (sourceCarrier : ValidatorOperationalSuccessorCarrier pins start
      sourceAuthor base) :
    (∃ finish round,
      start ≤ finish ∧
      base + 1 ≤ round ∧
      Nonempty (CorrectHeldTotalQuorumLayer config faults
        (timed.execution.trace finish) round)) ∨
    (∃ carrierStart,
      start ≤ carrierStart ∧
      Nonempty (ValidatorOperationalSuccessorCarrier pins carrierStart
        receiver base)) := by
  rcases operational_successor_carrier_reaches_receiver_or_later_layer pins
      acceptanceRules frontiers sameGenesis sourceAuthorInRange
        sourceAuthorCorrectAvailable receiverInRange receiverCorrectAvailable
          afterGst active sourceCarrier with
    ⟨readyAt, startBeforeReady, ready⟩
  rcases ready with ⟨_accepted, baseAtMostFrontier⟩ |
      ⟨laterRound, successorAtMostLater, laterLayer⟩
  · by_cases frontierLater : base < frontiers.frontier readyAt receiver
    · rcases frontiers.currentSource readyAt receiver receiverInRange
        receiverCorrectAvailable (active readyAt startBeforeReady) with
        ⟨receiverSource⟩
      have frontierPositive : 0 < frontiers.frontier readyAt receiver :=
        Nat.lt_of_le_of_lt (Nat.zero_le base) frontierLater
      exact Or.inl ⟨readyAt, frontiers.frontier readyAt receiver,
        startBeforeReady, by omega,
        positive_operational_frontier_gives_correct_held_total_quorum_layer
          receiverInRange receiverCorrectAvailable frontierPositive
            receiverSource⟩
    · have frontierExact : frontiers.frontier readyAt receiver = base := by
        omega
      have activeFromReady : ∀ time, readyAt ≤ time →
          (timed.execution.trace time).epochActive = true := by
        intro time readyBeforeTime
        exact active time (Nat.le_trans startBeforeReady readyBeforeTime)
      have readyAfterGst : network.gst ≤ readyAt :=
        Nat.le_trans afterGst startBeforeReady
      have localResult :=
        operational_frontier_host_current_tip_pacemaker_gives_successor pins
          tipSubscription frontiers sameGenesis pacemaker latchSource effects
            authorityCountAtLeastTwo receiverInRange receiverCorrectAvailable
              frontierExact readyAfterGst activeFromReady
      rcases current_pacemaker_result_gives_later_layer_or_successor_carrier
          pins tipSubscription frontiers sameGenesis receiverInRange
            receiverCorrectAvailable readyAfterGst activeFromReady localResult with
        later | carrier
      · rcases later with
          ⟨finish, round, readyBeforeFinish, successorAtMostRound, layer⟩
        exact Or.inl ⟨finish, round,
          Nat.le_trans startBeforeReady readyBeforeFinish,
            successorAtMostRound, layer⟩
      · exact Or.inr ⟨readyAt, startBeforeReady, carrier⟩
  · exact Or.inl ⟨readyAt, laterRound, startBeforeReady,
      successorAtMostLater, laterLayer⟩

/-- A strict operational-frontier step is an internal theorem result.

It advances beyond the finite correct-host maximum at the supplied start and
returns a public layer. The protocol-specific construction can keep stronger
proposal, source, and delivery witnesses before it projects to this result. -/
def ValidatorOperationalFrontierStrictProgress
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents) : Prop :=
  ∀ start,
    network.gst ≤ start →
    (∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) →
    ∃ finish round,
      start ≤ finish ∧
      correctOperationalQuorumFrontierMaximumUpTo frontiers start
          config.authorityCount + 1 ≤ round ∧
      Nonempty (CorrectHeldTotalQuorumLayer config faults
        (timed.execution.trace finish) round)

/-- The local frontier pacemaker, current-tip replay, ordinary delivery, and
parent-first synchronization give one strict collective frontier step.

The proof first sends one maximum-owner successor to every correct, available
validator. Each validator then produces or replays its own successor. A finite
enumeration collects those exact accepted references at the maximum owner.
Correct, available stake is a quorum, so the owner's operational frontier is
at least the successor round. Commit installation is never a progress result;
it can only move GC and make an old dependency a completed GC root. -/
theorem operational_frontier_pacemaker_gives_strict_progress
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (tipSubscription : ValidatorCurrentTipSubscriptionExecution pins)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (pacemaker : ValidatorNormalFrontierPacemakerRules timed)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount) :
    ValidatorOperationalFrontierStrictProgress timed frontiers := by
  intro start afterGst active
  let maximum := correctOperationalQuorumFrontierMaximumUpTo frontiers start
    config.authorityCount
  by_cases laterExists : ∃ finish round,
      start ≤ finish ∧
      maximum + 1 ≤ round ∧
      Nonempty (CorrectHeldTotalQuorumLayer config faults
        (timed.execution.trace finish) round)
  · exact laterExists
  · rcases operational_maximum_current_tip_pacemaker_gives_successor pins
        tipSubscription frontiers sameGenesis pacemaker latchSource effects
          authorityCountAtLeastTwo afterGst active with
      ⟨owner, ownerInRange, ownerCorrectAvailable, ownerResult⟩
    rcases current_pacemaker_result_gives_later_layer_or_successor_carrier
        pins tipSubscription frontiers sameGenesis ownerInRange
          ownerCorrectAvailable afterGst active ownerResult with
      ownerLater | ownerCarrierExists
    · exact False.elim (laterExists (by simpa [maximum] using ownerLater))
    · rcases ownerCarrierExists with ⟨ownerCarrier⟩
      have eachCarrier : ∀ author,
          author < config.authorityCount →
          faults.correctAvailable author = true →
          ∃ carrierStart,
            start ≤ carrierStart ∧
            Nonempty (ValidatorOperationalSuccessorCarrier pins carrierStart
              author maximum) := by
        intro author authorInRange authorCorrectAvailable
        rcases operational_successor_carrier_gives_host_successor_or_later_layer
            pins tipSubscription acceptanceRules frontiers sameGenesis pacemaker
              latchSource effects authorityCountAtLeastTwo ownerInRange
                ownerCorrectAvailable authorInRange authorCorrectAvailable
                  afterGst active (by simpa [maximum] using ownerCarrier) with
          later | carrier
        · exact False.elim (laterExists (by simpa [maximum] using later))
        · simpa [maximum] using carrier
      have eachAccepted : ∀ author,
          author < config.authorityCount →
          faults.correctAvailable author = true →
          ∃ finish,
            start ≤ finish ∧
            ∃ block : ValidatorBlock BlockId,
              block.reference.author = author ∧
              block.reference.round = maximum + 1 ∧
              ((timed.execution.trace finish).validatorState owner).accepted
                block.reference = true := by
        intro author authorInRange authorCorrectAvailable
        rcases eachCarrier author authorInRange authorCorrectAvailable with
          ⟨carrierStart, startBeforeCarrier, ⟨carrier⟩⟩
        have carrierAfterGst : network.gst ≤ carrierStart :=
          Nat.le_trans afterGst startBeforeCarrier
        have activeFromCarrier : ∀ time, carrierStart ≤ time →
            (timed.execution.trace time).epochActive = true := by
          intro time carrierBeforeTime
          exact active time (Nat.le_trans startBeforeCarrier carrierBeforeTime)
        rcases operational_successor_carrier_reaches_receiver_or_later_layer
            pins acceptanceRules frontiers sameGenesis authorInRange
              authorCorrectAvailable ownerInRange ownerCorrectAvailable
                carrierAfterGst activeFromCarrier carrier with
          ⟨finish, carrierBeforeFinish, reached⟩
        have startBeforeFinish := Nat.le_trans startBeforeCarrier
          carrierBeforeFinish
        rcases reached with acceptedAtOwner |
            ⟨round, successorAtMostRound, layer⟩
        · exact ⟨finish, startBeforeFinish, carrier.block,
            carrier.blockAuthor, carrier.exactRound, acceptedAtOwner.1⟩
        · exact False.elim (laterExists ⟨finish, round, startBeforeFinish,
            by simpa [maximum] using successorAtMostRound, layer⟩)
      let acceptedByOwner := fun author time =>
        ∃ block : ValidatorBlock BlockId,
          block.reference.author = author ∧
          block.reference.round = maximum + 1 ∧
          ((timed.execution.trace time).validatorState owner).accepted
            block.reference = true
      have acceptancePersists : ∀ author earlier later,
          earlier ≤ later →
          acceptedByOwner author earlier →
          acceptedByOwner author later := by
        intro author earlier later earlierBeforeLater
        rintro ⟨block, blockAuthor, blockRound, acceptedEarlier⟩
        exact ⟨block, blockAuthor, blockRound,
          timed.execution.accepted_block_persists ownerInRange
            earlierBeforeLater acceptedEarlier⟩
      have eachEventually : ∀ author,
          author < config.authorityCount →
          faults.correctAvailable author = true →
          ∃ finish,
            start ≤ finish ∧
            acceptedByOwner author finish := by
        intro author authorInRange authorCorrectAvailable
        simpa [acceptedByOwner] using
          eachAccepted author authorInRange authorCorrectAvailable
      rcases eventually_every_selected_validator faults.correctAvailable
          acceptedByOwner start acceptancePersists eachEventually with
        ⟨finish, startBeforeFinish, allAccepted⟩
      rcases accepted_correct_successors_give_later_operational_layer frontiers
          ownerInRange ownerCorrectAvailable (active finish startBeforeFinish)
            ownerCarrier.block (by
              intro author authorInRange authorCorrectAvailable
              exact allAccepted author authorInRange authorCorrectAvailable) with
        ⟨round, successorAtMostRound, layer⟩
      exact ⟨finish, round, startBeforeFinish,
        by simpa [maximum] using successorAtMostRound, layer⟩

/-- Strict progress of the finite operational maximum gives unbounded public
DAG progress by well-founded iteration.

The `strictProgress` argument is not an end-to-end field. A higher theorem must
derive it from the one-host pacemaker, ordinary broadcasts, recursive block
synchronization, and finite correct-stake aggregation. -/
theorem operational_frontier_strict_progress_gives_network_dag_progress
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (strictProgress : ValidatorOperationalFrontierStrictProgress timed
      frontiers) :
    NetworkDagProgressLiveness config faults network timed.execution.trace := by
  intro start minimumRound afterGst active
  let maximumAt := fun time =>
    correctOperationalQuorumFrontierMaximumUpTo frontiers time
      config.authorityCount
  have advance : ∀ gap start minimumRound,
      network.gst ≤ start →
      (∀ time, start ≤ time →
        (timed.execution.trace time).epochActive = true) →
      minimumRound - maximumAt start = gap →
      ∃ finish round,
        start ≤ finish ∧
        minimumRound ≤ round ∧
        Nonempty (CorrectHeldTotalQuorumLayer config faults
          (timed.execution.trace finish) round) := by
    intro gap
    induction gap using Nat.strongRecOn with
    | ind gap inductionHypothesis =>
        intro current requested currentAfterGst activeFromCurrent gapExact
        by_cases currentEnough :
            0 < maximumAt current ∧ requested ≤ maximumAt current
        · rcases currentEnough with ⟨maximumPositive, requestedAtMost⟩
          rcases correct_operational_quorum_frontier_maximum_has_source
              frontiers (activeFromCurrent current (Nat.le_refl current)) with
            ⟨holder, holderInRange, holderCorrect, holderMaximum,
              holderSource⟩
          let source := Classical.choice holderSource
          have layer :=
            positive_operational_frontier_gives_correct_held_total_quorum_layer
              holderInRange holderCorrect (by
                simpa [maximumAt, holderMaximum] using maximumPositive) source
          have layerAtMaximum : Nonempty (CorrectHeldTotalQuorumLayer config
              faults (timed.execution.trace current) (maximumAt current)) := by
            simpa [maximumAt, holderMaximum] using layer
          exact ⟨current, maximumAt current, Nat.le_refl _, requestedAtMost,
            layerAtMaximum⟩
        · rcases strictProgress current currentAfterGst activeFromCurrent with
            ⟨finish, round, currentBeforeFinish, successorAtMostRound,
              layer⟩
          by_cases requestedAtMostRound : requested ≤ round
          · exact ⟨finish, round, currentBeforeFinish,
              requestedAtMostRound, layer⟩
          · have activeAtFinish := activeFromCurrent finish
              currentBeforeFinish
            let selectedLayer := Classical.choice layer
            have roundAtMostLaterMaximum : round ≤ maximumAt finish := by
              exact correct_held_total_quorum_layer_le_operational_maximum
                frontiers activeAtFinish selectedLayer
            have laterGapSmaller : requested - maximumAt finish < gap := by
              have requestedAboveRound : round < requested := by omega
              have maximumStrictlyAdvances :
                  maximumAt current < maximumAt finish := by
                have currentSuccessorAtMostRound :
                    maximumAt current + 1 ≤ round := by
                  simpa [maximumAt] using successorAtMostRound
                omega
              rw [← gapExact]
              omega
            have finishAfterGst : network.gst ≤ finish :=
              Nat.le_trans currentAfterGst currentBeforeFinish
            have activeFromFinish : ∀ time, finish ≤ time →
                (timed.execution.trace time).epochActive = true := by
              intro time finishBeforeTime
              exact activeFromCurrent time
                (Nat.le_trans currentBeforeFinish finishBeforeTime)
            rcases inductionHypothesis (requested - maximumAt finish)
                laterGapSmaller finish requested finishAfterGst activeFromFinish
                  rfl with
              ⟨laterFinish, laterRound, finishBeforeLater,
                requestedAtMostLater, laterLayer⟩
            exact ⟨laterFinish, laterRound,
              Nat.le_trans currentBeforeFinish finishBeforeLater,
                requestedAtMostLater, laterLayer⟩
  exact advance (minimumRound - maximumAt start) start minimumRound afterGst
    active rfl

end Mysticeti
