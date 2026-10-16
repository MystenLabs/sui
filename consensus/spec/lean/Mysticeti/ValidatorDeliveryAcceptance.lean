/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorTimedExecutionLemmas

namespace Mysticeti

/-! Parent-ready block acceptance in one validator process.

Packet delivery and block acceptance are different local operations. A valid
delivered block can wait in a local buffer while its causal parents are absent.
After those parents are accepted, the protected acceptance task runs within the
local action bound. No rule in this file assumes that synchronization completes
or that a parent quorum becomes available.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- One-host rules for a block that waits for its direct causal parents.

`buffered` is a source-mapping predicate for the local suspended-block store.
The rules do not require another validator to have or send any parent.
-/
structure ValidatorParentReadyAcceptanceRules
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  /-- The local suspended-block store contains this block. -/
  buffered : Nat → ValidatorBlock BlockId → Time → Prop
  /-- Delivery above local GC either completes acceptance or records the valid
  block in the local suspended-block store. Stale delivery is ignored. -/
  deliveryBuffersOrAccepts : ∀ time packetId packet block,
    (timed.execution.trace time).packets packetId = some packet →
    packet.payload = .block block →
    ValidatorPacketDeliveryOccurs (timed.execution.events time) packetId →
    block.reference.author < config.authorityCount →
    (block.reference.round = 0 ∨ block.HasQuorumImmediateParents config) →
    ((timed.execution.trace (time + 1)).validatorState
      packet.receiver).gcRound < block.reference.round →
    (((timed.execution.trace (time + 1)).validatorState
          packet.receiver).accepted block.reference = true ∨
      buffered packet.receiver block (time + 1))
  /-- A pending above-GC block stays buffered until it is accepted. -/
  bufferedPersistsWhilePending : ∀ validator block earlier later,
    earlier ≤ later →
    buffered validator block earlier →
    ((timed.execution.trace later).validatorState validator).accepted
        block.reference = false →
    ((timed.execution.trace later).validatorState validator).gcRound <
      block.reference.round →
    buffered validator block later
  /-- When every direct parent is accepted or is now a GC root, the suspended
  block is already accepted or its acceptance task is durably protected. -/
  parentReadyBufferEnablesAccept : ∀ validator block time,
    buffered validator block time →
    ((timed.execution.trace time).validatorState validator).gcRound <
      block.reference.round →
    (∀ parent, parent ∈ block.parents →
      ((timed.execution.trace time).validatorState validator).accepted parent =
          true ∨
        parent.round ≤
          ((timed.execution.trace time).validatorState validator).gcRound) →
    (((timed.execution.trace time).validatorState validator).accepted
          block.reference = true ∨
      timed.protectedAction time validator (.acceptBlock block))

/-- A completed normal acceptance action stores the exact block reference. -/
theorem accept_block_action_makes_block_accepted
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time validator : Nat} {block : ValidatorBlock BlockId}
    (occurs : ValidatorLocalActionOccurs (execution.events time) validator
      (.acceptBlock block)) :
    ((execution.trace (time + 1)).validatorState validator).accepted
        block.reference = true := by
  rcases validator_world_step_local_action_with_suffix
      (execution.stepsFollowRules time) occurs with
    ⟨_, actionAfter, _, actionStep, suffixStep⟩
  have structural :=
    validator_atomic_local_action_has_structural_effect actionStep
  have acceptedAfter :
      (actionAfter.validatorState validator).accepted block.reference = true := by
    exact structural.2.2.2.2.1.1
  exact validator_world_step_accepted_block_persists suffixStep acceptedAfter

/-- Delivery followed by current-GC causal-parent readiness leads to
acceptance.

The parent-ready time can be later than the delivery time. The theorem does not
state why the parent blocks arrive. A causal block-sync proof can supply each
parent fact separately. A parent that is at or below the current GC round is a
committed root and does not need its body to remain accepted.
-/
theorem delivered_block_with_current_gc_ready_parents_is_accepted
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorParentReadyAcceptanceRules timed)
    {deliveryTime parentsReadyAt : Time} {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    {block : ValidatorBlock BlockId}
    (packetPresent :
      (timed.execution.trace deliveryTime).packets packetId = some packet)
    (packetPayload : packet.payload = .block block)
    (delivered : ValidatorPacketDeliveryOccurs
      (timed.execution.events deliveryTime) packetId)
    (receiverInRange : packet.receiver < config.authorityCount)
    (receiverCorrectAvailable :
      faults.correctAvailable packet.receiver = true)
    (authorInRange : block.reference.author < config.authorityCount)
    (validParents :
      block.reference.round = 0 ∨ block.HasQuorumImmediateParents config)
    (deliveryBeforeParentsReady : deliveryTime + 1 ≤ parentsReadyAt)
    (blockAboveGc :
      ((timed.execution.trace parentsReadyAt).validatorState
        packet.receiver).gcRound < block.reference.round)
    (parentsReady : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).accepted parent = true ∨
        parent.round ≤
          ((timed.execution.trace parentsReadyAt).validatorState
            packet.receiver).gcRound) :
    ∃ acceptedAt,
      parentsReadyAt ≤ acceptedAt ∧
        acceptedAt ≤ parentsReadyAt + timed.localActionBound + 1 ∧
        ((timed.execution.trace acceptedAt).validatorState
          packet.receiver).accepted block.reference = true := by
  have gcAtDeliveryLe :
      ((timed.execution.trace (deliveryTime + 1)).validatorState
          packet.receiver).gcRound ≤
        ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).gcRound := by
    have durable := timed.execution.durableStateMonotone packet.receiver
      (deliveryTime + 1) parentsReadyAt receiverInRange
      deliveryBeforeParentsReady
    rcases durable with ⟨_, _, _, _, _, _, _, _, _, _, _, gcMonotone⟩
    exact gcMonotone
  have blockAboveGcAtDelivery :
      ((timed.execution.trace (deliveryTime + 1)).validatorState
        packet.receiver).gcRound < block.reference.round := by
    omega
  rcases rules.deliveryBuffersOrAccepts deliveryTime packetId packet block
      packetPresent packetPayload delivered authorInRange validParents
      blockAboveGcAtDelivery with
    acceptedAtDelivery | bufferedAtDelivery
  · have acceptedAtReady := timed.execution.accepted_block_persists
      receiverInRange deliveryBeforeParentsReady acceptedAtDelivery
    exact ⟨parentsReadyAt, Nat.le_refl _,
      Nat.le_trans (Nat.le_add_right _ timed.localActionBound)
        (Nat.le_add_right _ 1), acceptedAtReady⟩
  · by_cases acceptedAtReady :
        ((timed.execution.trace parentsReadyAt).validatorState
          packet.receiver).accepted block.reference = true
    · exact ⟨parentsReadyAt, Nat.le_refl _,
        Nat.le_trans (Nat.le_add_right _ timed.localActionBound)
          (Nat.le_add_right _ 1), acceptedAtReady⟩
    · have bufferedAtReady := rules.bufferedPersistsWhilePending
        packet.receiver block (deliveryTime + 1) parentsReadyAt
        deliveryBeforeParentsReady bufferedAtDelivery (by
          simpa using acceptedAtReady) blockAboveGc
      rcases rules.parentReadyBufferEnablesAccept packet.receiver block
          parentsReadyAt bufferedAtReady blockAboveGc parentsReady with
        acceptedNow | acceptEnabled
      · exact ⟨parentsReadyAt, Nat.le_refl _,
          Nat.le_trans (Nat.le_add_right _ timed.localActionBound)
            (Nat.le_add_right _ 1), acceptedNow⟩
      · let completion := timed.completeProtectedAction packet.receiver
          (.acceptBlock block) parentsReadyAt receiverInRange
          receiverCorrectAvailable acceptEnabled
        have acceptedAfter := accept_block_action_makes_block_accepted
          timed.execution completion.occurs
        refine ⟨completion.event.completedAt + 1, ?_, ?_, acceptedAfter⟩
        · have startsBeforeCompletion :
              parentsReadyAt ≤ completion.event.completedAt := by
            simpa [completion.sameEnableTime] using
              completion.enableBeforeCompletion
          exact Nat.le_trans startsBeforeCompletion (Nat.le_add_right _ _)
        · have completionBound :
              completion.event.completedAt ≤
                parentsReadyAt + timed.localActionBound := by
            simpa [completion.sameEnableTime] using
              completion.completesWithinBound
          exact Nat.add_le_add_right completionBound 1

/-- The accepted-parent specialization of
`delivered_block_with_current_gc_ready_parents_is_accepted`. -/
theorem delivered_block_with_ready_parents_is_accepted
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorParentReadyAcceptanceRules timed)
    {deliveryTime parentsReadyAt : Time} {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    {block : ValidatorBlock BlockId}
    (packetPresent :
      (timed.execution.trace deliveryTime).packets packetId = some packet)
    (packetPayload : packet.payload = .block block)
    (delivered : ValidatorPacketDeliveryOccurs
      (timed.execution.events deliveryTime) packetId)
    (receiverInRange : packet.receiver < config.authorityCount)
    (receiverCorrectAvailable :
      faults.correctAvailable packet.receiver = true)
    (authorInRange : block.reference.author < config.authorityCount)
    (validParents :
      block.reference.round = 0 ∨ block.HasQuorumImmediateParents config)
    (deliveryBeforeParentsReady : deliveryTime + 1 ≤ parentsReadyAt)
    (blockAboveGc :
      ((timed.execution.trace parentsReadyAt).validatorState
        packet.receiver).gcRound < block.reference.round)
    (parentsAccepted : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace parentsReadyAt).validatorState
        packet.receiver).accepted parent = true) :
    ∃ acceptedAt,
      parentsReadyAt ≤ acceptedAt ∧
        acceptedAt ≤ parentsReadyAt + timed.localActionBound + 1 ∧
        ((timed.execution.trace acceptedAt).validatorState
          packet.receiver).accepted block.reference = true := by
  exact delivered_block_with_current_gc_ready_parents_is_accepted timed rules
    packetPresent packetPayload delivered receiverInRange
    receiverCorrectAvailable authorInRange validParents
    deliveryBeforeParentsReady blockAboveGc
      (fun parent member => Or.inl (parentsAccepted parent member))

/-- The same theorem when all parents are ready as soon as delivery is visible.
-/
theorem delivered_parent_ready_block_is_accepted
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorParentReadyAcceptanceRules timed)
    {deliveryTime : Time} {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    {block : ValidatorBlock BlockId}
    (packetPresent :
      (timed.execution.trace deliveryTime).packets packetId = some packet)
    (packetPayload : packet.payload = .block block)
    (delivered : ValidatorPacketDeliveryOccurs
      (timed.execution.events deliveryTime) packetId)
    (receiverInRange : packet.receiver < config.authorityCount)
    (receiverCorrectAvailable :
      faults.correctAvailable packet.receiver = true)
    (authorInRange : block.reference.author < config.authorityCount)
    (validParents :
      block.reference.round = 0 ∨ block.HasQuorumImmediateParents config)
    (blockAboveGc :
      ((timed.execution.trace (deliveryTime + 1)).validatorState
        packet.receiver).gcRound < block.reference.round)
    (parentsAccepted : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace (deliveryTime + 1)).validatorState
        packet.receiver).accepted parent = true) :
    ∃ acceptedAt,
      deliveryTime + 1 ≤ acceptedAt ∧
        acceptedAt ≤ deliveryTime + 1 + timed.localActionBound + 1 ∧
        ((timed.execution.trace acceptedAt).validatorState
          packet.receiver).accepted block.reference = true := by
  exact delivered_block_with_ready_parents_is_accepted timed rules
    packetPresent packetPayload delivered receiverInRange
    receiverCorrectAvailable authorInRange validParents (Nat.le_refl _)
    blockAboveGc parentsAccepted

/-- Parent-ready delivery is accepted by one fixed trace time.

The fixed time makes the result easy to use in the pure pacing model. If local
acceptance finishes earlier, durable acceptance carries the fact to this time.
-/
theorem delivered_parent_ready_block_is_accepted_by_bound
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorParentReadyAcceptanceRules timed)
    {deliveryTime : Time} {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    {block : ValidatorBlock BlockId}
    (packetPresent :
      (timed.execution.trace deliveryTime).packets packetId = some packet)
    (packetPayload : packet.payload = .block block)
    (delivered : ValidatorPacketDeliveryOccurs
      (timed.execution.events deliveryTime) packetId)
    (receiverInRange : packet.receiver < config.authorityCount)
    (receiverCorrectAvailable :
      faults.correctAvailable packet.receiver = true)
    (authorInRange : block.reference.author < config.authorityCount)
    (validParents :
      block.reference.round = 0 ∨ block.HasQuorumImmediateParents config)
    (blockAboveGc :
      ((timed.execution.trace (deliveryTime + 1)).validatorState
        packet.receiver).gcRound < block.reference.round)
    (parentsAccepted : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace (deliveryTime + 1)).validatorState
        packet.receiver).accepted parent = true) :
    ((timed.execution.trace
      (deliveryTime + 1 + timed.localActionBound + 1)).validatorState
        packet.receiver).accepted block.reference = true := by
  rcases delivered_parent_ready_block_is_accepted timed rules packetPresent
      packetPayload delivered receiverInRange receiverCorrectAvailable
      authorInRange validParents blockAboveGc parentsAccepted with
    ⟨acceptedAt, _readyBeforeAccepted, acceptedBeforeBound, accepted⟩
  exact timed.execution.accepted_block_persists receiverInRange
    acceptedBeforeBound accepted

end Mysticeti
