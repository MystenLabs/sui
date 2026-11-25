/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorAnchorBridge
import Mysticeti.ValidatorDeliveryAcceptance
import Mysticeti.ValidatorTimedExecutionLemmas

namespace Mysticeti

/-! Concrete trace facts for recovery pacing.

This module connects the small-step validator execution to the pure pacing
theorems. Its input records describe one timer or one local source rule. They do
not state that a block layer, direct-vote quorum, anchor, or commit exists.
-/

namespace AddressedPacket

/-- Remove addresses and payload from one packet, but keep its real-time
network timestamps. -/
def toTimingPacket {Payload : Type} (packet : AddressedPacket Payload) : Packet :=
  { sentAt := packet.sentAt, deliveredAt := packet.deliveredAt }

end AddressedPacket

/-- Timing packets obtained from authenticated packets between correct,
available validators. -/
def CorrectAvailableAddressedTimingPacket
    {CommitId Payload : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket : AddressedPacket Payload → Prop) : Packet → Prop :=
  fun timingPacket => ∃ addressedPacket,
    protocolPacket addressedPacket ∧
      addressedPacket.sender < config.authorityCount ∧
      addressedPacket.receiver < config.authorityCount ∧
      faults.correctAvailable addressedPacket.sender = true ∧
      faults.correctAvailable addressedPacket.receiver = true ∧
      timingPacket = addressedPacket.toTimingPacket

namespace AddressedPartialSynchrony

/-- Addressed partial synchrony gives the same delay bound after addresses and
payload are removed. This theorem introduces no second network assumption. -/
def toTimingNetwork
    {CommitId Payload : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket : AddressedPacket Payload → Prop}
    (network : AddressedPartialSynchrony config faults protocolPacket) :
    PartialSynchrony
      (CorrectAvailableAddressedTimingPacket config faults protocolPacket) where
  gst := network.gst
  delta := network.delta
  deltaPositive := network.deltaPositive
  postGstDelivery := by
    intro timingPacket valid afterGst
    rcases valid with
      ⟨addressedPacket, protocol, senderInRange, receiverInRange,
        senderCorrectAvailable, receiverCorrectAvailable, rfl⟩
    exact network.postGstDelivery addressedPacket protocol senderInRange
      receiverInRange senderCorrectAvailable receiverCorrectAvailable afterGst

end AddressedPartialSynchrony

/-- One ghost timing action whose duration has already been derived from
concrete validator events. -/
def TraceActionWithin (bound : Nat) (action : LocalConsensusAction) : Prop :=
  action.enabledAt ≤ action.completedAt ∧
    action.completedAt ≤ action.enabledAt + bound

/-- Derived timing actions satisfy bounded local processing by definition. -/
def traceActionProcessing
    {protocolPacket : Packet → Prop}
    (network : PartialSynchrony protocolPacket) (bound : Nat) :
    BoundedLocalProcessing network (TraceActionWithin bound) where
  epsilon := bound
  postGstCompletion := by
    intro action within _afterGst
    exact within

/-- A recovery proposal uses three bounded local actions: select the proposal,
persist it, and send it. Each completed event becomes visible in the next trace
state. -/
def validatorProposalPipelineBound (localActionBound : Nat) : Nat :=
  3 * (localActionBound + 1)

/-- One local recovery timer and its exact proposal action.

Rust must keep the timer start when later rounds arrive. It must store the
parent-ready time and the deadline for the unchanged local commit head. At the
deadline, the exact-next proposal with the selected parent snapshot must be
enabled. The fields below map those local facts to trace time. -/
structure ValidatorRecoveryProposalReady
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (commitHead : ValidatorCommitHead CommitId)
    (targetRound validator : Nat) where
  validatorInRange : validator < config.authorityCount
  validatorCorrectAvailable : faults.correctAvailable validator = true
  parentReadyAt : Time
  startedAt : Time
  highestObservedRound : Nat
  startsAfterParentReady : parentReadyAt ≤ startedAt
  parents : List (ValidatorBlockRef BlockId)
  recovery : ValidatorRecoveryState BlockId CommitId
  recoveryAtDeadline :
    ((timed.execution.trace
      (startedAt + waits.wait commitHead targetRound)).validatorState
        validator).recovery = some recovery
  recoveryBaseline : recovery.baselineCommit = commitHead
  recoveryTarget : recovery.targetRound = targetRound
  recoveryParentsReady : recovery.parentsReadyAt = some parentReadyAt
  recoveryDeadline :
    recovery.deadline = some (startedAt + waits.wait commitHead targetRound)
  recoveryHasNoAlignment : recovery.alignmentWitness = none
  proposalProtected : timed.protectedAction
    (startedAt + waits.wait commitHead targetRound) validator
    (.proposeNext parents)

namespace ValidatorRecoveryProposalReady

/-- Convert one stored timer observation to the pure timer model. -/
def timer
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
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound validator : Nat}
    (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound validator) :
    RecoveryTargetTimer commitHead targetRound :=
  { parentQuorumReadyAt := ready.parentReadyAt
    startedAt := ready.startedAt
    highestObservedRound := ready.highestObservedRound
    startsAfterParentQuorum := ready.startsAfterParentReady }

/-- The pure timer made from a ready proposal has the stored absolute deadline. -/
@[simp]
theorem timer_deadline
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
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound validator : Nat}
    (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound validator) :
    ready.timer.deadline waits =
      ready.startedAt + waits.wait commitHead targetRound := by
  rfl

/-- The enabled proposal uses the local recovery target and one valid parent
snapshot. -/
theorem parent_list_ready
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
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound validator : Nat}
    (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound validator) :
    ValidatorParentListReady config
      ((timed.execution.trace
        (ready.startedAt + waits.wait commitHead targetRound)).validatorState
          validator)
      targetRound ready.parents := by
  have enabled := timed.protectedActionIsEnabled _ _ _ ready.proposalProtected
  have guard := enabled.2.1
  change ∃ recovery,
      ((timed.execution.trace
        (ready.startedAt + waits.wait commitHead targetRound)).validatorState
          validator).recovery = some recovery ∧
        recovery.alignmentWitness = none ∧
        recovery.targetRound =
          ((timed.execution.trace
            (ready.startedAt + waits.wait commitHead targetRound)).validatorState
              validator).highestSignedRound + 1 ∧
        (∃ deadline,
          recovery.deadline = some deadline ∧
            deadline ≤
              ((timed.execution.trace
                (ready.startedAt + waits.wait commitHead targetRound)).validatorState
                  validator).clock) ∧
        ValidatorParentListReady config
          ((timed.execution.trace
            (ready.startedAt + waits.wait commitHead targetRound)).validatorState
              validator)
          recovery.targetRound ready.parents at guard
  rcases guard with
    ⟨recovery, recoveryAtDeadline, _noAlignment, _exactNext, _deadline,
      parentsReady⟩
  have sameRecovery : recovery = ready.recovery := by
    rw [ready.recoveryAtDeadline] at recoveryAtDeadline
    exact Option.some.inj recoveryAtDeadline.symm
  subst recovery
  simpa [ready.recoveryTarget] using parentsReady

end ValidatorRecoveryProposalReady

/-- A positive-quorum parent list contains at least one block reference. -/
theorem validator_parent_list_ready_nonempty
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {state : ValidatorLocalState BlockId CommitId}
    {targetRound : Nat} {parents : List (ValidatorBlockRef BlockId)}
    (ready : ValidatorParentListReady config state targetRound parents) :
    parents ≠ [] := by
  intro empty
  subst parents
  have quorumPositive := config.thresholds.quorum_positive
  have quorumParents := ready.2.2
  have noParentAuthors :
      validatorParentAuthors ([] : List (ValidatorBlockRef BlockId)) =
        VoterSet.empty := by
    rfl
  rw [noParentAuthors, weight_empty] at quorumParents
  omega

/-- One-validator source rules for the proposal pipeline.

The exact execution effects map proposal, persistence, send, and delivery
operations to state changes. The additional field says that each stored correct
proposal enables its local send operation for one correct receiver. It does not
say that the send runs or that the packet arrives. Bounded local execution and
partial synchrony prove those results. -/
structure ValidatorRecoveryProposalPipelineRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (program : ValidatorExecutionProgram BlockId CommitId)
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  effects : ValidatorExactExecutionEffects faults protocolPacket network
    program timed.execution
  acceptance : ValidatorParentReadyAcceptanceRules timed
  proposedBlockPersistenceIsProtected : ∀ time validator parents,
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.proposeNext parents) →
    ∀ block,
      block.reference.author = validator →
      block.parents = parents →
      timed.protectedAction (time + 1) validator (.persistProposal block)
  storedProposalProtectsSend : ∀
      (snapshot : ValidatorProposalSnapshot config faults timed.execution.trace)
      receiver,
    receiver < config.authorityCount →
    faults.correctAvailable receiver = true →
    timed.protectedAction snapshot.storedAt snapshot.proposer
      (.sendBlock receiver snapshot.block.reference)

/-- One completed proposal made from one local recovery timer. -/
structure ValidatorCompletedRecoveryProposal
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
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound validator : Nat}
    (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound validator)
    (snapshot : ValidatorProposalSnapshot config faults timed.execution.trace) :
    Prop where
  snapshotProposer : snapshot.proposer = validator
  snapshotAtDeadline :
    snapshot.snapshotAt =
      ready.startedAt + waits.wait commitHead targetRound
  snapshotRound : snapshot.block.reference.round = targetRound
  snapshotParents : snapshot.block.parents = ready.parents
  validParents : snapshot.block.HasQuorumImmediateParents config
  storedWithinPipelinePrefix :
    snapshot.storedAt ≤
      ready.startedAt + waits.wait commitHead targetRound +
        2 * (timed.localActionBound + 1)

/-- Bounded execution of `proposeNext` and `persistProposal` constructs one
actual recovery proposal snapshot. -/
theorem complete_recovery_proposal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorRecoveryProposalPipelineRules faults protocolPacket
      network program timed)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound validator : Nat}
    (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound validator) :
    ∃ snapshot, ValidatorCompletedRecoveryProposal ready snapshot := by
  let deadline := ready.startedAt + waits.wait commitHead targetRound
  let proposalCompletion := timed.completeProtectedAction validator
    (.proposeNext ready.parents) deadline ready.validatorInRange
      ready.validatorCorrectAvailable ready.proposalProtected
  have proposalOccurs : ValidatorLocalActionOccurs
      (timed.execution.events proposalCompletion.event.completedAt) validator
        (.proposeNext ready.parents) :=
    proposalCompletion.occurs
  rcases propose_next_parents_lead_to_persisted_block timed rules.effects
      (rules.proposedBlockPersistenceIsProtected
        proposalCompletion.event.completedAt validator ready.parents
        proposalOccurs) proposalOccurs with
    ⟨block, persistenceCompletion, blockAuthor, blockParents,
      persistenceCompletedBy, blockStored, blockInCatalog⟩
  have proposalCompletedBy :
      proposalCompletion.event.completedAt ≤
        deadline + timed.localActionBound := by
    simpa [proposalCompletion.sameEnableTime] using
      proposalCompletion.completesWithinBound
  have persistenceEnabledAt : ValidatorActionEnabledAt timed.execution
      (proposalCompletion.event.completedAt + 1) validator
        (.persistProposal block) := by
    simpa [persistenceCompletion.sameEnableTime] using
      persistenceCompletion.enabled
  have persistenceGuard := persistenceEnabledAt.2.1
  change block.reference.author = validator ∧
      ((timed.execution.trace
        (proposalCompletion.event.completedAt + 1)).validatorState
          validator).highestSignedRound < block.reference.round ∧
      block.HasQuorumImmediateParents config ∧
      ∀ parent, parent ∈ block.parents →
        ((timed.execution.trace
          (proposalCompletion.event.completedAt + 1)).validatorState
            validator).accepted parent = true at persistenceGuard
  have blockParentsValid := persistenceGuard.2.2.1
  have proposalParentsReady := ready.parent_list_ready
  have parentsNonempty :=
    validator_parent_list_ready_nonempty proposalParentsReady
  rcases List.exists_mem_of_ne_nil ready.parents parentsNonempty with
    ⟨parent, parentInReady⟩
  have parentInBlock : parent ∈ block.parents := by
    rw [blockParents]
    exact parentInReady
  have parentTargetsRound :=
    (proposalParentsReady.2.1 parent parentInReady).1
  have parentTargetsBlock := blockParentsValid.2.1 parent parentInBlock
  have blockRound : block.reference.round = targetRound := by
    omega
  have snapshotBeforeStore :
      deadline ≤ persistenceCompletion.event.completedAt + 1 := by
    have proposalStartsBeforeCompletion :
        deadline ≤ proposalCompletion.event.completedAt := by
      simpa [proposalCompletion.sameEnableTime] using
        proposalCompletion.enableBeforeCompletion
    have persistenceStartsBeforeCompletion :
        proposalCompletion.event.completedAt + 1 ≤
          persistenceCompletion.event.completedAt := by
      simpa [persistenceCompletion.sameEnableTime] using
        persistenceCompletion.enableBeforeCompletion
    exact Nat.le_trans proposalStartsBeforeCompletion
      (Nat.le_trans (Nat.le_add_right _ _)
        (Nat.le_trans persistenceStartsBeforeCompletion (Nat.le_add_right _ _)))
  let snapshot : ValidatorProposalSnapshot config faults timed.execution.trace :=
    { proposer := validator
      snapshotAt := deadline
      storedAt := persistenceCompletion.event.completedAt + 1
      block := block
      proposerInRange := ready.validatorInRange
      proposerCorrectAvailable := ready.validatorCorrectAvailable
      snapshotBeforeStore := snapshotBeforeStore
      recoveryTargetsProposalRound := ⟨ready.recovery,
        ready.recoveryAtDeadline, by simpa [blockRound] using ready.recoveryTarget,
        ready.recoveryHasNoAlignment⟩
      blockIsOwnProposal := blockAuthor
      blockStored := blockStored
      blockInCatalog := blockInCatalog
      parentAuthorsNodup := blockParentsValid.1
      parentsAreImmediate := blockParentsValid.2.1
      parentsAcceptedAtSnapshot := by
        intro listedParent listed
        have listedInReady : listedParent ∈ ready.parents := by
          rw [← blockParents]
          exact listed
        exact (proposalParentsReady.2.1 listedParent listedInReady).2 }
  refine ⟨snapshot,
    { snapshotProposer := rfl
      snapshotAtDeadline := rfl
      snapshotRound := blockRound
      snapshotParents := blockParents
      validParents := blockParentsValid
      storedWithinPipelinePrefix := ?_ }⟩
  dsimp [snapshot, deadline]
  have proposalPlusOne :
      proposalCompletion.event.completedAt + 1 ≤
        (ready.startedAt + waits.wait commitHead targetRound +
          timed.localActionBound) + 1 :=
    Nat.add_le_add_right proposalCompletedBy 1
  have proposalPlusOneAndBound :
      proposalCompletion.event.completedAt + 1 + timed.localActionBound ≤
        (ready.startedAt + waits.wait commitHead targetRound +
          timed.localActionBound) + 1 + timed.localActionBound :=
    Nat.add_le_add_right proposalPlusOne timed.localActionBound
  have persistenceBound := Nat.le_trans persistenceCompletedBy
    proposalPlusOneAndBound
  have storedBound := Nat.add_le_add_right persistenceBound 1
  have normalized :
      (ready.startedAt + waits.wait commitHead targetRound +
          timed.localActionBound) + 1 + timed.localActionBound + 1 ≤
        ready.startedAt + waits.wait commitHead targetRound +
          2 * (timed.localActionBound + 1) := by
    simp [Nat.mul_add, Nat.succ_mul, Nat.add_assoc, Nat.add_comm,
      Nat.add_left_comm]
    omega
  exact Nat.le_trans storedBound normalized

/-- One sent copy of one stored proposal. -/
structure ValidatorCompletedProposalBroadcast
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (snapshot : ValidatorProposalSnapshot config faults timed.execution.trace)
    (receiver : Nat) (packetId : PacketId)
    (packet : AddressedPacket (ValidatorMessage BlockId CommitId)) : Prop where
  packetInTrace :
    (timed.execution.trace packet.sentAt).packets packetId = some packet
  packetIsProtocol : protocolPacket packet
  packetSender : packet.sender = snapshot.proposer
  packetReceiver : packet.receiver = receiver
  packetPayload : packet.payload = .block snapshot.block
  storedBeforeSend : snapshot.storedAt ≤ packet.sentAt
  sentWithinBound :
    packet.sentAt ≤ snapshot.storedAt + timed.localActionBound + 1
  sentRecord :
    ((timed.execution.trace packet.sentAt).validatorState
      snapshot.proposer).sentOwnBlockAt snapshot.block.reference.round = true

/-- One delivered proposal has all required parents at its receiver.

This is the exact pointwise obligation that a prior-round delivery proof or the
block-sync proof must establish. Packet delivery alone does not establish it. -/
def ValidatorProposalBroadcastParentsAcceptedAtDelivery
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (snapshot : ValidatorProposalSnapshot config faults timed.execution.trace)
    (receiver : Nat) : Prop :=
  ∀ packetId packet,
    ValidatorCompletedProposalBroadcast snapshot receiver packetId packet →
    ∀ parent, parent ∈ snapshot.block.parents →
      ((timed.execution.trace packet.deliveredAt).validatorState
        receiver).accepted parent = true

/-- One delivered proposal is still above the receiver's GC frontier when the
receiver can process it. Canonical recovery theorems return commit progress as
the other branch when this condition is false. -/
def ValidatorProposalBroadcastAboveGcAfterDelivery
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (snapshot : ValidatorProposalSnapshot config faults timed.execution.trace)
    (receiver : Nat) : Prop :=
  ∀ packetId packet,
    ValidatorCompletedProposalBroadcast snapshot receiver packetId packet →
      ((timed.execution.trace (packet.deliveredAt + 1)).validatorState
        receiver).gcRound < snapshot.block.reference.round

/-- Parent availability for the exact proposal constructed from one recovery
timer. This callback names no future block, packet, layer, vote, or anchor. -/
def ValidatorRecoveryBroadcastParentAvailability
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
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound validator : Nat}
    (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound validator)
    (receiver : Nat) : Prop :=
  ∀ snapshot,
    ValidatorCompletedRecoveryProposal ready snapshot →
    ValidatorProposalBroadcastParentsAcceptedAtDelivery snapshot receiver

/-- The GC condition for the exact proposal constructed from one recovery
timer. This is used only by the older pointwise flow helper. -/
def ValidatorRecoveryBroadcastAboveGcAfterDelivery
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
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound validator : Nat}
    (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound validator)
    (receiver : Nat) : Prop :=
  ∀ snapshot,
    ValidatorCompletedRecoveryProposal ready snapshot →
      ValidatorProposalBroadcastAboveGcAfterDelivery snapshot receiver

private theorem validator_world_step_block_catalog_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {blockId : BlockId} {block : ValidatorBlock BlockId}
    (present : before.blockCatalog blockId = some block) :
    after.blockCatalog blockId = some block := by
  induction step with
  | nil => exact present
  | cons firstStep remainingSteps ih =>
      have firstPresent :=
        (validator_atomic_step_history_monotone firstStep).1 blockId block
          present
      exact ih firstPresent

private theorem validator_world_step_sent_record_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {validator round : Nat}
    (sent : (before.validatorState validator).sentOwnBlockAt round = true) :
    (after.validatorState validator).sentOwnBlockAt round = true := by
  induction step with
  | nil => exact sent
  | cons firstStep remainingSteps ih =>
      have firstSent :=
        (validator_atomic_step_durable_monotone firstStep validator)
          |>.sent_own_block_persists sent
      exact ih firstSent

/-- Bounded execution of the enabled send action creates the exact addressed
packet for a stored proposal. -/
theorem broadcast_stored_recovery_proposal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorRecoveryProposalPipelineRules faults protocolPacket
      network program timed)
    (snapshot : ValidatorProposalSnapshot config faults timed.execution.trace)
    {receiver : Nat}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true) :
    ∃ packetId packet,
      ValidatorCompletedProposalBroadcast snapshot receiver packetId packet := by
  have sendProtected := rules.storedProposalProtectsSend snapshot receiver
    receiverInRange receiverCorrectAvailable
  let completion := timed.completeProtectedAction snapshot.proposer
    (.sendBlock receiver snapshot.block.reference) snapshot.storedAt
      snapshot.proposerInRange snapshot.proposerCorrectAvailable sendProtected
  have sendOccurs := completion.occurs
  rcases sendOccurs with ⟨headEvents, tailEvents, eventsEqual⟩
  have completeStep := timed.execution.stepsFollowRules
    completion.event.completedAt
  rw [eventsEqual] at completeStep
  rcases validator_world_step_append_split completeStep with
    ⟨actionBefore, headStep, actionAndSuffix⟩
  cases actionAndSuffix with
  | cons actionStep suffixStep =>
      have storedBeforeCompletion :
          snapshot.storedAt ≤ completion.event.completedAt := by
        simpa [completion.sameEnableTime] using completion.enableBeforeCompletion
      have catalogAtCompletion := timed.execution.blockCatalogMonotone
        snapshot.storedAt completion.event.completedAt storedBeforeCompletion
        snapshot.block.reference.id snapshot.block snapshot.blockInCatalog
      have catalogAtAction := validator_world_step_block_catalog_persists
        headStep catalogAtCompletion
      rcases rules.effects.sendBlockCreatesPacket
          completion.event.completedAt actionBefore _ snapshot.proposer receiver
            snapshot.block.reference actionStep with
        ⟨packetId, sentBlock, packet, catalogAtAction', sentReference,
          _packetAbsent, packetAfterAction, packetProtocol, packetSender,
          packetReceiver, packetPayload, packetSentAt⟩
      have sameBlock : sentBlock = snapshot.block := by
        have sameCatalog : some snapshot.block = some sentBlock := by
          rw [← catalogAtAction, catalogAtAction']
        exact Option.some.inj sameCatalog.symm
      subst sentBlock
      have packetAfterBatch := validator_world_step_packet_persists suffixStep
        packetAfterAction
      have structural :=
        validator_atomic_local_action_has_structural_effect actionStep
      have sentEffect := structural.2.2.2.1
      have sentAfterAction := sentEffect.1
      have sentAfterBatch := validator_world_step_sent_record_persists suffixStep
        sentAfterAction
      have packetStoredAtSent :
          (timed.execution.trace packet.sentAt).packets packetId = some packet := by
        rw [packetSentAt]
        exact packetAfterBatch
      have storedBeforePacket : snapshot.storedAt ≤ packet.sentAt := by
        rw [packetSentAt]
        have storedBeforeCompletion :
            snapshot.storedAt ≤ completion.event.completedAt := by
          simpa [completion.sameEnableTime] using
            completion.enableBeforeCompletion
        exact Nat.le_trans storedBeforeCompletion (Nat.le_add_right _ _)
      have packetBound :
          packet.sentAt ≤
            snapshot.storedAt + timed.localActionBound + 1 := by
        rw [packetSentAt]
        have completionBound :
            completion.event.completedAt ≤
              snapshot.storedAt + timed.localActionBound := by
          simpa [completion.sameEnableTime] using completion.completesWithinBound
        exact Nat.add_le_add_right completionBound 1
      refine ⟨packetId, packet,
        { packetInTrace := packetStoredAtSent
          packetIsProtocol := packetProtocol
          packetSender := packetSender
          packetReceiver := packetReceiver
          packetPayload := packetPayload
          storedBeforeSend := storedBeforePacket
          sentWithinBound := packetBound
          sentRecord := ?_ }⟩
      rw [packetSentAt]
      exact sentAfterBatch

/-- Partial synchrony delivers one broadcast. Accepted causal parents then
enable the protected local acceptance task. -/
theorem completed_broadcast_is_accepted
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorRecoveryProposalPipelineRules faults protocolPacket
      network program timed)
    {snapshot : ValidatorProposalSnapshot config faults timed.execution.trace}
    {receiver : Nat} {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (broadcast : ValidatorCompletedProposalBroadcast snapshot receiver packetId
      packet)
    (parentsAccepted : ∀ parent, parent ∈ snapshot.block.parents →
      ((timed.execution.trace packet.deliveredAt).validatorState
        receiver).accepted parent = true)
    (blockAboveGcAfterDelivery :
      ((timed.execution.trace (packet.deliveredAt + 1)).validatorState
        receiver).gcRound < snapshot.block.reference.round)
    (validParents : snapshot.block.HasQuorumImmediateParents config)
    (sentAfterGst : network.gst ≤ packet.sentAt) :
    ((timed.execution.trace
      (packet.deliveredAt + 1 + timed.localActionBound + 1)).validatorState
        receiver).accepted snapshot.block.reference = true := by
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
  have parentsAcceptedAfterDelivery : ∀ parent,
      parent ∈ snapshot.block.parents →
      ((timed.execution.trace (packet.deliveredAt + 1)).validatorState
        packet.receiver).accepted parent = true := by
    intro parent included
    have acceptedAtDelivery :
        ((timed.execution.trace packet.deliveredAt).validatorState
          packet.receiver).accepted parent = true := by
      simpa [broadcast.packetReceiver] using parentsAccepted parent included
    exact timed.execution.accepted_block_persists
      (by simpa [broadcast.packetReceiver] using receiverInRange)
      (Nat.le_add_right _ _) acceptedAtDelivery
  have accepted := delivered_parent_ready_block_is_accepted_by_bound timed
    rules.acceptance packetAtDelivery broadcast.packetPayload delivered
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    (by simpa [snapshot.blockIsOwnProposal] using snapshot.proposerInRange)
    (Or.inr validParents)
    (by simpa [broadcast.packetReceiver] using blockAboveGcAfterDelivery)
    parentsAcceptedAfterDelivery
  simpa [broadcast.packetReceiver] using accepted

/-- One delivery transition is within the proposal-pipeline timing bound. -/
theorem delivery_action_within_pipeline_bound
    (localActionBound deliveredAt : Nat) :
    TraceActionWithin (validatorProposalPipelineBound localActionBound)
      { enabledAt := deliveredAt, completedAt := deliveredAt + 1 } := by
  unfold TraceActionWithin
  constructor
  · change deliveredAt ≤ deliveredAt + 1
    exact Nat.le_add_right _ _
  · change deliveredAt + 1 ≤
      deliveredAt + validatorProposalPipelineBound localActionBound
    apply Nat.add_le_add_left
    have oneWithinOneAction : 1 ≤ localActionBound + 1 := by
      exact Nat.succ_le_succ (Nat.zero_le _)
    have oneActionWithinPipeline :
        localActionBound + 1 ≤
          validatorProposalPipelineBound localActionBound := by
      unfold validatorProposalPipelineBound
      exact Nat.le_mul_of_pos_left _ (by decide)
    exact Nat.le_trans oneWithinOneAction oneActionWithinPipeline

/-- Buffered acceptance, including its next-state visibility, stays within the
proposal-pipeline local processing bound. -/
theorem buffered_acceptance_action_within_pipeline_bound
    (localActionBound deliveredAt : Nat) :
    TraceActionWithin (validatorProposalPipelineBound localActionBound)
      { enabledAt := deliveredAt
        completedAt := deliveredAt + 1 + localActionBound + 1 } := by
  unfold TraceActionWithin
  constructor
  · change deliveredAt ≤ deliveredAt + 1 + localActionBound + 1
    exact Nat.le_trans (Nat.le_add_right _ 1)
      (Nat.le_trans (Nat.le_add_right _ localActionBound)
        (Nat.le_add_right _ 1))
  · change deliveredAt + 1 + localActionBound + 1 ≤
      deliveredAt + validatorProposalPipelineBound localActionBound
    have localWithinPipeline :
        1 + localActionBound + 1 ≤ 3 * (localActionBound + 1) := by
      have oneWithinOneAction : 1 ≤ localActionBound + 1 :=
        Nat.succ_le_succ (Nat.zero_le _)
      let actionCost := localActionBound + 1
      have prefixWithinTwoActions :
          1 + localActionBound + 1 ≤ actionCost + actionCost := by
        have expanded :
            1 + localActionBound + 1 = actionCost + 1 := by
          dsimp [actionCost]
          ac_rfl
        rw [expanded]
        exact Nat.add_le_add_left oneWithinOneAction _
      have twoActionsWithinThree :
          actionCost + actionCost ≤
            (actionCost + actionCost) + actionCost :=
        Nat.le_add_right _ _
      have threeActionsShape :
          (actionCost + actionCost) + actionCost =
            3 * (localActionBound + 1) := by
        dsimp [actionCost]
        simp only [Nat.succ_mul, Nat.zero_mul]
        ac_rfl
      rw [← threeActionsShape]
      exact Nat.le_trans prefixWithinTwoActions twoActionsWithinThree
    calc
      deliveredAt + 1 + localActionBound + 1 =
          deliveredAt + (1 + localActionBound + 1) := by
        simp [Nat.add_assoc]
      _ ≤ deliveredAt + 3 * (localActionBound + 1) :=
        Nat.add_le_add_left localWithinPipeline deliveredAt
      _ = deliveredAt + validatorProposalPipelineBound localActionBound := by
        rfl

/-- Two concrete recovery timers produce one pointwise timed block flow.

The first timer produces and sends the round-`r` block. The second timer marks
the real parent snapshot for the receiver's round-`r + 1` proposal. The result
does not state that the first block is in that parent snapshot. The pacing and
parent-selection proofs establish that fact later. -/
theorem recovery_timers_produce_timed_block_flow
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorRecoveryProposalPipelineRules faults protocolPacket
      network program timed)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {commitHead : ValidatorCommitHead CommitId}
    {round leader receiver : Nat}
    (leaderReady : ValidatorRecoveryProposalReady faults timed waits commitHead
      round leader)
    (nextReady : ValidatorRecoveryProposalReady faults timed waits commitHead
      (round + 1) receiver)
    (parentAvailability :
      ValidatorRecoveryBroadcastParentAvailability leaderReady receiver)
    (blockAboveGcAfterDelivery :
      ValidatorRecoveryBroadcastAboveGcAfterDelivery leaderReady receiver)
    (leaderTimerStartsAfterGst : network.gst ≤ leaderReady.startedAt) :
    ∃ flow : ValidatorTimedBlockFlow config faults timed.execution.trace
        (timingProtocolPacket :=
          CorrectAvailableAddressedTimingPacket config faults protocolPacket)
        (timingProtocolAction :=
          TraceActionWithin
            (validatorProposalPipelineBound timed.localActionBound))
        waits commitHead round,
      flow.leader = leader ∧ flow.receiver = receiver ∧
        flow.leaderTimer = leaderReady.timer ∧
        flow.nextTimer = nextReady.timer := by
  rcases complete_recovery_proposal timed rules leaderReady with
    ⟨leaderSnapshot, leaderProposal⟩
  rcases complete_recovery_proposal timed rules nextReady with
    ⟨nextSnapshot, nextProposal⟩
  rcases broadcast_stored_recovery_proposal timed rules leaderSnapshot
      nextReady.validatorInRange nextReady.validatorCorrectAvailable with
    ⟨packetId, packet, broadcast⟩
  have deadlineBeforeStored :
      leaderReady.startedAt + waits.wait commitHead round ≤
        leaderSnapshot.storedAt := by
    rw [← leaderProposal.snapshotAtDeadline]
    exact leaderSnapshot.snapshotBeforeStore
  have sentAfterGst : network.gst ≤ packet.sentAt := by
    exact Nat.le_trans leaderTimerStartsAfterGst
      (Nat.le_trans (Nat.le_add_right _ _)
        (Nat.le_trans deadlineBeforeStored broadcast.storedBeforeSend))
  have acceptedAtCompletion := completed_broadcast_is_accepted timed rules
    nextReady.validatorInRange nextReady.validatorCorrectAvailable broadcast
    (parentAvailability leaderSnapshot leaderProposal packetId packet broadcast)
    (blockAboveGcAfterDelivery leaderSnapshot leaderProposal packetId packet
      broadcast)
    leaderProposal.validParents sentAfterGst
  let pipelineBound := validatorProposalPipelineBound timed.localActionBound
  let timingPacket := packet.toTimingPacket
  let proposalAction : LocalConsensusAction :=
    { enabledAt := leaderReady.startedAt + waits.wait commitHead round
      completedAt := packet.sentAt }
  have proposalActionWithin : TraceActionWithin pipelineBound proposalAction := by
    constructor
    · exact Nat.le_trans deadlineBeforeStored broadcast.storedBeforeSend
    · dsimp [proposalAction, pipelineBound]
      have sentBound := broadcast.sentWithinBound
      have storedBound := leaderProposal.storedWithinPipelinePrefix
      unfold validatorProposalPipelineBound
      calc
        packet.sentAt ≤
            leaderSnapshot.storedAt + timed.localActionBound + 1 := sentBound
        _ ≤
            (leaderReady.startedAt + waits.wait commitHead round +
                2 * (timed.localActionBound + 1)) +
              timed.localActionBound + 1 :=
          Nat.add_le_add_right
            (Nat.add_le_add_right storedBound timed.localActionBound) 1
        _ = leaderReady.startedAt + waits.wait commitHead round +
              3 * (timed.localActionBound + 1) := by
          simp only [Nat.succ_mul]
          ac_rfl
  have timingPacketValid :
      CorrectAvailableAddressedTimingPacket config faults protocolPacket
        timingPacket := by
    refine ⟨packet, broadcast.packetIsProtocol, ?_, ?_, ?_, ?_, rfl⟩
    · simpa [broadcast.packetSender, leaderProposal.snapshotProposer] using
        leaderReady.validatorInRange
    · simpa [broadcast.packetReceiver] using nextReady.validatorInRange
    · simpa [broadcast.packetSender, leaderProposal.snapshotProposer] using
        leaderReady.validatorCorrectAvailable
    · simpa [broadcast.packetReceiver] using
        nextReady.validatorCorrectAvailable
  let proposal : TimedLeaderProposal
      (protocolPacket :=
        CorrectAvailableAddressedTimingPacket config faults protocolPacket)
      (protocolAction := TraceActionWithin pipelineBound)
      waits leaderReady.timer timingPacket :=
    { action := proposalAction
      actionIsCovered := proposalActionWithin
      enabledAtDeadline := rfl
      packetIsProtocol := timingPacketValid
      sentAtCompletion := rfl }
  let acceptanceAction : LocalConsensusAction :=
    { enabledAt := packet.deliveredAt
      completedAt := packet.deliveredAt + 1 + timed.localActionBound + 1 }
  have acceptanceActionWithin :
      TraceActionWithin pipelineBound acceptanceAction := by
    simpa [acceptanceAction, pipelineBound] using
      buffered_acceptance_action_within_pipeline_bound timed.localActionBound
        packet.deliveredAt
  let acceptance : TimedBlockAcceptance
      (protocolAction := TraceActionWithin pipelineBound) timingPacket :=
    { action := acceptanceAction
      actionIsCovered := acceptanceActionWithin
      enabledAtDelivery := rfl }
  let parentSelection : TimedParentSelection waits nextReady.timer :=
    { snapshotAt := nextSnapshot.snapshotAt
      startsAfterDeadline := by
        rw [nextProposal.snapshotAtDeadline]
        exact Nat.le_refl _ }
  have ownBlockAtSend :
      ((timed.execution.trace packet.sentAt).validatorState
        leaderSnapshot.proposer).ownBlockAt
          leaderSnapshot.block.reference.round =
        some leaderSnapshot.block.reference := by
    exact (timed.execution.durable_fields_persist
      leaderSnapshot.proposerInRange broadcast.storedBeforeSend)
        |>.own_block_persists leaderSnapshot.blockStored
  refine ⟨
    { leader := leader
      receiver := receiver
      leaderInRange := leaderReady.validatorInRange
      receiverInRange := nextReady.validatorInRange
      leaderCorrectAvailable := leaderReady.validatorCorrectAvailable
      receiverCorrectAvailable := nextReady.validatorCorrectAvailable
      leaderBlock := leaderSnapshot.block
      leaderBlockAuthor := by
        simpa [leaderProposal.snapshotProposer] using
          leaderSnapshot.blockIsOwnProposal
      leaderBlockRound := leaderProposal.snapshotRound
      leaderTimer := leaderReady.timer
      nextTimer := nextReady.timer
      timingPacket := timingPacket
      proposal := proposal
      acceptance := acceptance
      nextProposal := nextSnapshot
      nextProposalOwner := nextProposal.snapshotProposer
      nextProposalRound := nextProposal.snapshotRound
      nextProposalValidParents := nextProposal.validParents
      parentSelection := parentSelection
      parentSnapshotMatches := rfl
      addressedPacketId := packetId
      addressedPacket := packet
      addressedSender := by
        simpa [leaderProposal.snapshotProposer] using broadcast.packetSender
      addressedReceiver := broadcast.packetReceiver
      addressedPayload := broadcast.packetPayload
      addressedSentAt := rfl
      addressedDeliveredAt := rfl
      packetInTrace := broadcast.packetInTrace
      proposalStoredAtCompletion := by
        simpa [proposal, proposalAction, leaderProposal.snapshotProposer,
          leaderProposal.snapshotRound] using ownBlockAtSend
      proposalSentAtCompletion := by
        simpa [proposal, proposalAction, leaderProposal.snapshotProposer,
          leaderProposal.snapshotRound] using broadcast.sentRecord
      acceptanceVisibleAtCompletion := by
        simpa [acceptance, acceptanceAction] using acceptedAtCompletion },
    rfl, rfl, rfl, rfl⟩

end Mysticeti
