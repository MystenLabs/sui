/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.ValidatorBlockSyncBridge
import Mysticeti.ValidatorDurableWork

namespace Mysticeti

/-!
Authenticated replay-manifest delivery on the main validator execution.

A sender names the exact prior and next commit heads and the finite block
references needed for one full local replay. A receiver creates durable local
block needs only after it receives that exact protocol message and checks that
the prior is in its own durable commit prefix. Thus a proof-level replay value
cannot create a requester goal.

This module does not say that a sender has replay material. The caller must bind
the manifest to retained material with an actual local-run or verified-sync
origin before it protects the send action.
-/

/-- Convert replay-material order to the block-sync order after one validator
has accepted every explicit root. -/
theorem replay_parent_first_to_block_sync
    {BlockId : Type}
    {roots accepted : ValidatorBlockRef BlockId → Prop}
    {blocks : List (ValidatorBlock BlockId)}
    (rootsAccepted : ∀ reference, roots reference → accepted reference)
    (ordered : ValidatorReplayParentFirst roots blocks) :
    ParentFirstValidatorBlockHistory accepted blocks := by
  induction blocks generalizing roots accepted with
  | nil => trivial
  | cons block remaining ih =>
      constructor
      · intro parent member
        exact rootsAccepted parent (ordered.1 parent member)
      · exact ih (fun reference known => by
          rcases known with root | same
          · exact Or.inl (rootsAccepted reference root)
          · exact Or.inr same) ordered.2

/-- The exact manifest for one retained full-committer replay. -/
def ValidatorExactReplayMaterial.toReplayManifest
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    (material : ValidatorExactReplayMaterial functions context) :
    ValidatorReplayManifest BlockId CommitId :=
  { prior := material.sourceInput.commitHead
    head := material.output.toCommitHead
    blockReferences := material.blocks.map ValidatorBlock.reference }

/-- A receiver can check this manifest header from its own durable prefix. -/
def ValidatorReplayManifest.LocallyValidAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Nat)
    (manifest : ValidatorReplayManifest BlockId CommitId) : Prop :=
  manifest.head.index = manifest.prior.index + 1 ∧
    ((execution.trace time).validatorState validator).installedCommitAt
        manifest.prior.index = some manifest.prior.id

/-- Durable replay-manifest knowledge owned by one validator. -/
structure ValidatorReplayManifestNeedState
    (BlockId CommitId : Type) where
  acceptedManifest : ValidatorReplayManifest BlockId CommitId → Bool
  neededBlock : ValidatorReplayManifest BlockId CommitId →
    ValidatorBlockRef BlockId → Bool

/-- Exact local processing of all verified manifests delivered in one batch. -/
structure ValidatorReplayManifestNeedTransition
    {BlockId CommitId : Type}
    (before : ValidatorReplayManifestNeedState BlockId CommitId)
    (received : List (ValidatorReplayManifest BlockId CommitId))
    (after : ValidatorReplayManifestNeedState BlockId CommitId) : Prop where
  acceptedExact : ∀ manifest,
    after.acceptedManifest manifest = true ↔
      before.acceptedManifest manifest = true ∨ manifest ∈ received
  neededExact : ∀ manifest reference,
    after.neededBlock manifest reference = true ↔
      before.neededBlock manifest reference = true ∨
        (manifest ∈ received ∧ reference ∈ manifest.blockReferences)

/-- Main-trace source map for replay manifests and durable requester needs.

The two packet fields bind both directions. A receive-list member has an actual
delivered authenticated packet, and each delivered locally valid packet enters
that list. `sendActionCreatesPacket` binds a protected main-trace send action to
the exact manifest bytes sent to one receiver.
-/
structure ValidatorReplayManifestExecution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  trace : Time → Nat → ValidatorReplayManifestNeedState BlockId CommitId
  received : Time → Nat → List (ValidatorReplayManifest BlockId CommitId)
  transitionsFollowRules : ∀ time validator,
    ValidatorReplayManifestNeedTransition (trace time validator)
      (received time validator) (trace (time + 1) validator)
  initialStateEmpty : ∀ validator,
    (∀ manifest, (trace 0 validator).acceptedManifest manifest = false) ∧
      ∀ manifest reference,
        (trace 0 validator).neededBlock manifest reference = false
  receivedManifestHasOrigin : ∀ time validator manifest,
    manifest ∈ received time validator →
    ∃ (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      (timed.execution.trace time).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.receiver = validator ∧
        packet.payload = .replayManifest manifest ∧
        ValidatorPacketDeliveryOccurs (timed.execution.events time) packetId ∧
        manifest.LocallyValidAt timed.execution time validator
  deliveredValidManifestIsReceived : ∀ time packetId packet manifest,
    (timed.execution.trace time).packets packetId = some packet →
    protocolPacket packet →
    packet.payload = .replayManifest manifest →
    ValidatorPacketDeliveryOccurs (timed.execution.events time) packetId →
    manifest.LocallyValidAt timed.execution time packet.receiver →
    manifest ∈ received time packet.receiver
  sendActionCreatesPacket : ∀ time sender receiver manifest,
    ValidatorLocalActionOccurs (timed.execution.events time) sender
      (.sendReplayManifest receiver manifest) →
    ∃ (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      (timed.execution.trace (time + 1)).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.sender = sender ∧
        packet.receiver = receiver ∧
        packet.payload = .replayManifest manifest ∧
        packet.sentAt = time + 1

namespace ValidatorReplayManifestExecution

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}

/-- A processed manifest creates each block need that it names. -/
theorem received_manifest_creates_needs
    (rules : ValidatorReplayManifestExecution timed)
    {time validator : Nat}
    {manifest : ValidatorReplayManifest BlockId CommitId}
    (received : manifest ∈ rules.received time validator) :
    (rules.trace (time + 1) validator).acceptedManifest manifest = true ∧
      ∀ reference, reference ∈ manifest.blockReferences →
        (rules.trace (time + 1) validator).neededBlock manifest reference =
          true := by
  have step := rules.transitionsFollowRules time validator
  constructor
  · exact (step.acceptedExact manifest).2 (Or.inr received)
  · intro reference member
    exact (step.neededExact manifest reference).2
      (Or.inr ⟨received, member⟩)

/-- One requester-local manifest bit remains set. -/
theorem accepted_manifest_persists
    (rules : ValidatorReplayManifestExecution timed)
    {earlier later validator : Nat}
    {manifest : ValidatorReplayManifest BlockId CommitId}
    (ordered : earlier ≤ later)
    (accepted :
      (rules.trace earlier validator).acceptedManifest manifest = true) :
    (rules.trace later validator).acceptedManifest manifest = true := by
  induction later with
  | zero =>
      have same : earlier = 0 := by omega
      simpa [same] using accepted
  | succ previous ih =>
      by_cases same : earlier = previous + 1
      · simpa [same] using accepted
      · have earlierBeforePrevious : earlier ≤ previous := by omega
        have acceptedBefore := ih earlierBeforePrevious
        have step := rules.transitionsFollowRules previous validator
        apply (step.acceptedExact manifest).2
        exact Or.inl acceptedBefore

/-- One requester-local block need remains set. -/
theorem needed_block_persists
    (rules : ValidatorReplayManifestExecution timed)
    {earlier later validator : Nat}
    {manifest : ValidatorReplayManifest BlockId CommitId}
    {reference : ValidatorBlockRef BlockId}
    (ordered : earlier ≤ later)
    (needed :
      (rules.trace earlier validator).neededBlock manifest reference = true) :
    (rules.trace later validator).neededBlock manifest reference = true := by
  induction later with
  | zero =>
      have same : earlier = 0 := by omega
      simpa [same] using needed
  | succ previous ih =>
      by_cases same : earlier = previous + 1
      · simpa [same] using needed
      · have earlierBeforePrevious : earlier ≤ previous := by omega
        have neededBefore := ih earlierBeforePrevious
        have step := rules.transitionsFollowRules previous validator
        apply (step.neededExact manifest reference).2
        exact Or.inl neededBefore

/-- Every durable requester need came from an earlier delivered manifest that
named that exact block reference. -/
theorem needed_block_has_delivered_manifest_origin
    (rules : ValidatorReplayManifestExecution timed)
    {time validator : Nat}
    {manifest : ValidatorReplayManifest BlockId CommitId}
    {reference : ValidatorBlockRef BlockId}
    (needed :
      (rules.trace time validator).neededBlock manifest reference = true) :
    ∃ (deliveredAt : Time) (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      deliveredAt < time ∧
        (timed.execution.trace deliveredAt).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.receiver = validator ∧
        packet.payload = .replayManifest manifest ∧
        ValidatorPacketDeliveryOccurs
          (timed.execution.events deliveredAt) packetId ∧
        manifest.LocallyValidAt timed.execution deliveredAt validator ∧
        reference ∈ manifest.blockReferences := by
  induction time with
  | zero =>
      rw [(rules.initialStateEmpty validator).2 manifest reference] at needed
      contradiction
  | succ previous ih =>
      have step := rules.transitionsFollowRules previous validator
      have neededAtNext :
          (rules.trace (previous + 1) validator).neededBlock manifest reference =
            true := by
        simpa [Nat.succ_eq_add_one] using needed
      rcases (step.neededExact manifest reference).1 neededAtNext with
        neededBefore | ⟨received, referenceMember⟩
      · rcases ih neededBefore with
          ⟨deliveredAt, packetId, packet, deliveredBefore, stored, protocol,
            receiver, payload, delivered, valid, member⟩
        exact ⟨deliveredAt, packetId, packet,
          Nat.lt_trans deliveredBefore (Nat.lt_succ_self previous), stored,
          protocol, receiver, payload, delivered, valid, member⟩
      · rcases rules.receivedManifestHasOrigin previous validator manifest
          received with
          ⟨packetId, packet, stored, protocol, receiver, payload, delivered,
            valid⟩
        exact ⟨previous, packetId, packet, Nat.lt_succ_self previous, stored,
          protocol, receiver, payload, delivered, valid, referenceMember⟩

/-- A protected exact manifest send reaches one correct receiver and creates
durable local needs for all references in the message. -/
theorem protected_manifest_send_creates_receiver_needs
    (rules : ValidatorReplayManifestExecution timed)
    {start sender receiver : Nat}
    {manifest : ValidatorReplayManifest BlockId CommitId}
    (senderInRange : sender < config.authorityCount)
    (senderCorrect : faults.correctAvailable sender = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (afterGst : network.gst ≤ start)
    (nextIndex : manifest.head.index = manifest.prior.index + 1)
    (priorInstalled :
      ((timed.execution.trace start).validatorState receiver).installedCommitAt
          manifest.prior.index = some manifest.prior.id)
    (sendProtected : timed.protectedAction start sender
      (.sendReplayManifest receiver manifest)) :
    ∃ readyAt,
      start ≤ readyAt ∧
        (rules.trace readyAt receiver).acceptedManifest manifest = true ∧
        ∀ reference, reference ∈ manifest.blockReferences →
          (rules.trace readyAt receiver).neededBlock manifest reference = true := by
  rcases protected_validator_action_completes_within_bound timed senderInRange
      senderCorrect sendProtected with
    ⟨completion, startBeforeCompletion, _completionBound, sendOccurs⟩
  rcases rules.sendActionCreatesPacket completion.event.completedAt sender
      receiver manifest sendOccurs with
    ⟨packetId, packet, storedAfterSend, packetProtocol, packetSender,
      packetReceiver, packetPayload, packetSentAt⟩
  have storedAtSentTime :
      (timed.execution.trace packet.sentAt).packets packetId = some packet := by
    rw [packetSentAt]
    exact storedAfterSend
  have senderPacketInRange : packet.sender < config.authorityCount := by
    rw [packetSender]
    exact senderInRange
  have senderPacketCorrect :
      faults.correctAvailable packet.sender = true := by
    rw [packetSender]
    exact senderCorrect
  have receiverPacketInRange : packet.receiver < config.authorityCount := by
    rw [packetReceiver]
    exact receiverInRange
  have receiverPacketCorrect :
      faults.correctAvailable packet.receiver = true := by
    rw [packetReceiver]
    exact receiverCorrect
  have sentAfterStart : start ≤ packet.sentAt := by
    rw [packetSentAt]
    exact Nat.le_trans startBeforeCompletion
      (Nat.le_add_right completion.event.completedAt 1)
  have packetAfterGst : network.gst ≤ packet.sentAt :=
    Nat.le_trans afterGst sentAfterStart
  have deliveryBounds := network.postGstDelivery packet packetProtocol
    senderPacketInRange receiverPacketInRange senderPacketCorrect
    receiverPacketCorrect packetAfterGst
  have delivered := timed.execution.protocolPacketsAreDelivered packetId packet
    storedAtSentTime packetProtocol senderPacketInRange receiverPacketInRange
    senderPacketCorrect receiverPacketCorrect packetAfterGst
  have storedAtDelivery :
      (timed.execution.trace packet.deliveredAt).packets packetId = some packet :=
    timed.execution.packetHistoryMonotone packet.sentAt packet.deliveredAt
      deliveryBounds.1 packetId packet storedAtSentTime
  have startBeforeDelivery : start ≤ packet.deliveredAt :=
    Nat.le_trans sentAfterStart deliveryBounds.1
  have priorInstalledAtDelivery :
      ValidatorLocalState.installedCommitAt
          ((timed.execution.trace packet.deliveredAt).validatorState receiver)
          manifest.prior.index = some manifest.prior.id :=
    (timed.execution.durable_fields_persist receiverInRange
      startBeforeDelivery).installed_commit_persists priorInstalled
  have locallyValid : manifest.LocallyValidAt timed.execution packet.deliveredAt
      packet.receiver := by
    constructor
    · exact nextIndex
    · simpa [packetReceiver] using priorInstalledAtDelivery
  have received := rules.deliveredValidManifestIsReceived packet.deliveredAt
    packetId packet manifest storedAtDelivery packetProtocol packetPayload
    delivered locallyValid
  have created := rules.received_manifest_creates_needs received
  refine ⟨packet.deliveredAt + 1, ?_, ?_, ?_⟩
  · exact Nat.le_trans startBeforeDelivery
      (Nat.le_add_right packet.deliveredAt 1)
  · simpa [packetReceiver] using created.1
  · intro reference member
    simpa [packetReceiver] using created.2 reference member

end ValidatorReplayManifestExecution

end Mysticeti
