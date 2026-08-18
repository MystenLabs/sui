/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorTimedExecutionLemmas
import Mysticeti.ValidatorCausalRecoveryCapsule

namespace Mysticeti

/-! Block synchronization in the main validator execution.

This module does not define a second trace or network. A correct requester walks
the finite validator set. A request action creates one addressed request packet.
A correct holder that retained the requested block serves it. Delivery of the
returned block enables the normal block-acceptance action.

The source premise names one correct holder and one block that the holder has
accepted and retained. The proof does not assume a useful peer, a ready server,
or completed synchronization.
-/

/-- The finite peer order used by one block synchronization retry cycle. -/
def validatorBlockSyncPeerOrder (authorityCount : Nat) : List Nat :=
  List.range authorityCount

/-- Each validator identity occurs in the finite peer order. -/
theorem validator_block_sync_peer_order_contains
    {authorityCount validator : Nat}
    (validatorInRange : validator < authorityCount) :
    validator ∈ validatorBlockSyncPeerOrder authorityCount := by
  simp [validatorBlockSyncPeerOrder, validatorInRange]

/-- One correct holder has an exact retained block at one execution time. -/
structure RetainedValidatorBlock
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (holder : Nat) (block : ValidatorBlock BlockId) (time : Time) : Prop where
  holderInRange : holder < config.authorityCount
  holderCorrectAvailable : faults.correctAvailable holder = true
  retained :
    ((execution.trace time).validatorState holder).retained block.reference = true
  accepted :
    ((execution.trace time).validatorState holder).accepted block.reference = true
  catalog :
    (execution.trace time).blockCatalog block.reference.id = some block
  authorInRange : block.reference.author < config.authorityCount
  validParents :
    block.reference.round = 0 ∨ block.HasQuorumImmediateParents config

/-- Simple local rules that bind block synchronization to the main execution.

`visitPeer` is the finite retry rule of one requester. All other fields describe
one local action or one delivered addressed packet. No field states that another
validator has data or that synchronization succeeds.
-/
structure ValidatorBlockSyncExecutionRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  /-- A source-specific pin prevents cleanup of one retained block. -/
  sourceProtected : Nat → ValidatorBlockRef BlockId → Time → Prop
  /-- A request goal can end because a later commit or cleanup boundary makes
  the old block unnecessary. -/
  goalObsolete : Nat → ValidatorBlockRef BlockId → Time → Prop
  /-- One local block request is durable work. -/
  requestLatched : Time → Nat → Nat → ValidatorBlockRef BlockId → Prop
  /-- One local block service response is durable work. -/
  serveLatched : Time → Nat → Nat → ValidatorBlockRef BlockId → Prop
  /-- One local block acceptance is durable work. -/
  acceptLatched : Time → Nat → ValidatorBlock BlockId → Prop
  requestLatchIsProtected : ∀ time requester peer reference,
    requestLatched time requester peer reference →
    timed.protectedAction time requester (.requestBlock peer reference)
  serveLatchIsProtected : ∀ time holder requester reference,
    serveLatched time holder requester reference →
    timed.protectedAction time holder (.serveBlock requester reference)
  acceptLatchIsProtected : ∀ time requester block,
    acceptLatched time requester block →
    timed.protectedAction time requester (.acceptBlock block)
  /-- A requester can latch work only above its current GC frontier. -/
  requestLatchIsAboveGc : ∀ time requester peer reference,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    requestLatched time requester peer reference →
    ((timed.execution.trace time).validatorState requester).gcRound <
      reference.round
  /-- A receiver can latch acceptance only above its current GC frontier. -/
  acceptLatchIsAboveGc : ∀ time requester block,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    acceptLatched time requester block →
    ((timed.execution.trace time).validatorState requester).gcRound <
      block.reference.round
  peerRotationBound : Nat
  peerRotationBoundPositive : 0 < peerRotationBound
  /-- Once a local request goal is obsolete, it stays obsolete. -/
  goalObsoletePersists : ∀ requester reference earlier later,
    earlier ≤ later →
    goalObsolete requester reference earlier →
    goalObsolete requester reference later
  /-- A correct requester treats a reference at or below its local GC frontier
  as a committed root. It does not request or accept that reference again. -/
  gcFrontierMakesGoalObsolete : ∀ requester reference time,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    reference.round ≤
      ((timed.execution.trace time).validatorState requester).gcRound →
    goalObsolete requester reference time
  /-- A request goal is obsolete only because the reference is at or below the
  GC round that is current when the result is processed. A failed fetch is not
  obsolete; normal retry remains applicable while the reference is above GC. -/
  goalObsoleteIsAtOrBelowGc : ∀ requester reference time,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    goalObsolete requester reference time →
    reference.round ≤
      ((timed.execution.trace time).validatorState requester).gcRound
  /-- A retained block stays retained while its source-specific pin is active. -/
  retainedBlockPersists : ∀ holder reference earlier later,
    holder < config.authorityCount →
    earlier ≤ later →
    (∀ time, earlier ≤ time → time ≤ later →
      sourceProtected holder reference time) →
    ((timed.execution.trace earlier).validatorState holder).retained reference =
      true →
    ((timed.execution.trace later).validatorState holder).retained reference =
      true
  /-- One retry cycle visits each member of the finite peer order. -/
  visitPeer : ∀ requester peer reference start,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    peer ∈ validatorBlockSyncPeerOrder config.authorityCount →
    (∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) →
    ((timed.execution.trace start).validatorState requester).accepted reference =
      false →
    ∃ enabledAt,
      start ≤ enabledAt ∧
      enabledAt ≤ start + peerRotationBound ∧
      (goalObsolete requester reference enabledAt ∨
        ((timed.execution.trace enabledAt).validatorState requester).accepted
            reference = true ∨
          requestLatched enabledAt requester peer reference)
  /-- A completed request action creates the exact addressed request packet. -/
  requestActionCreatesPacket : ∀ time requester peer reference,
    ValidatorLocalActionOccurs (timed.execution.events time) requester
      (.requestBlock peer reference) →
    ∃ (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      (timed.execution.trace (time + 1)).packets packetId = some packet ∧
      protocolPacket packet ∧
      packet.sender = requester ∧
      packet.receiver = peer ∧
      packet.payload = .blockRequest reference ∧
      packet.sentAt = time + 1
  /-- A delivered request for retained data enables its local service action. -/
  deliveredRequestEnablesServe : ∀ time packetId packet reference block,
    (timed.execution.trace time).packets packetId = some packet →
    packet.payload = .blockRequest reference →
    ValidatorPacketDeliveryOccurs (timed.execution.events time) packetId →
    ((timed.execution.trace (time + 1)).validatorState packet.receiver).retained
        reference = true →
    (timed.execution.trace (time + 1)).blockCatalog reference.id = some block →
    block.reference = reference →
    serveLatched (time + 1) packet.receiver packet.sender reference
  /-- A completed service action creates the exact addressed block packet. -/
  serveActionCreatesPacket : ∀ time holder requester reference block,
    ValidatorLocalActionOccurs (timed.execution.events time) holder
      (.serveBlock requester reference) →
    (timed.execution.trace time).blockCatalog reference.id = some block →
    block.reference = reference →
    ∃ (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      (timed.execution.trace (time + 1)).packets packetId = some packet ∧
      protocolPacket packet ∧
      packet.sender = holder ∧
      packet.receiver = requester ∧
      packet.payload = .block block ∧
      packet.sentAt = time + 1
  /-- Local packet processing accepts a valid ready block or enables the normal
  block-acceptance action. Parents at or below the GC round are committed roots
  and do not need to be accepted again. -/
  deliveredBlockEnablesAccept : ∀ time packetId packet block,
    (timed.execution.trace time).packets packetId = some packet →
    packet.payload = .block block →
    ValidatorPacketDeliveryOccurs (timed.execution.events time) packetId →
    block.reference.author < config.authorityCount →
    (block.reference.round = 0 ∨ block.HasQuorumImmediateParents config) →
    (∀ parent, parent ∈ block.parents →
      ((timed.execution.trace (time + 1)).validatorState
          packet.receiver).accepted parent = true ∨
        parent.round ≤
          ((timed.execution.trace (time + 1)).validatorState
            packet.receiver).gcRound) →
    ((timed.execution.trace (time + 1)).validatorState packet.receiver).gcRound <
      block.reference.round →
    (((timed.execution.trace (time + 1)).validatorState packet.receiver).accepted
        block.reference = true ∨
      acceptLatched (time + 1) packet.receiver block)

/-- A completed block-acceptance action stores the accepted block at the end of
its execution batch. -/
theorem accept_block_occurrence_stores_accepted_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
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
  rcases structural with ⟨_, _, _, _, acceptedEffect, _⟩
  have acceptedAfter :
      (actionAfter.validatorState validator).accepted block.reference = true := by
    exact acceptedEffect.1
  exact validator_world_step_accepted_block_persists suffixStep acceptedAfter

/-- One retained block remains a retained source at every later active time. -/
theorem retained_validator_block_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder : Nat} {block : ValidatorBlock BlockId} {earlier later : Time}
    (source : RetainedValidatorBlock timed.execution holder block earlier)
    (ordered : earlier ≤ later)
    (protection : ∀ time, earlier ≤ time → time ≤ later →
      rules.sourceProtected holder block.reference time) :
    RetainedValidatorBlock timed.execution holder block later := by
  refine
    { holderInRange := source.holderInRange
      holderCorrectAvailable := source.holderCorrectAvailable
      retained := rules.retainedBlockPersists holder block.reference earlier later
        source.holderInRange ordered protection source.retained
      accepted := timed.execution.accepted_block_persists source.holderInRange
        ordered source.accepted
      catalog := timed.execution.blockCatalogMonotone earlier later ordered
        block.reference.id block source.catalog
      authorInRange := source.authorInRange
      validParents := source.validParents }

/-- One exact block body is available at one host. A delivered packet can be
buffered before acceptance. An accepted body must also have its catalog entry. -/
inductive ValidatorLocalBlockBodyAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time) (block : ValidatorBlock BlockId) : Prop where
  | delivered
      (packetId : PacketId)
      (packet : AddressedPacket (ValidatorMessage BlockId CommitId))
      (packetPresent :
        (timed.execution.trace time).packets packetId = some packet)
      (packetIsProtocol : protocolPacket packet)
      (packetReceiver : packet.receiver = validator)
      (packetPayload : packet.payload = .block block)
      (deliveryOccurs :
        ValidatorPacketDeliveryOccurs (timed.execution.events time) packetId)
  | acceptedCatalogued
      (accepted :
        ((timed.execution.trace time).validatorState validator).accepted
          block.reference = true)
      (catalogued :
        (timed.execution.trace time).blockCatalog block.reference.id =
          some block)

/-- A block packet sent after GST is delivered in the main execution. -/
theorem validator_protocol_packet_is_delivered
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    (present : (execution.trace packet.sentAt).packets packetId = some packet)
    (protocol : protocolPacket packet)
    (senderInRange : packet.sender < config.authorityCount)
    (receiverInRange : packet.receiver < config.authorityCount)
    (senderCorrect : faults.correctAvailable packet.sender = true)
    (receiverCorrect : faults.correctAvailable packet.receiver = true)
    (afterGst : network.gst ≤ packet.sentAt) :
    ValidatorPacketDeliveryOccurs (execution.events packet.deliveredAt)
      packetId := by
  exact execution.protocolPacketsAreDelivered packetId packet present protocol
    senderInRange receiverInRange senderCorrect receiverCorrect afterGst

/-- The maximum time for one requester to rotate to one correct source, send a
request, receive a response, and accept one parent-ready block. -/
def validatorBlockSyncAcceptanceBound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorBlockSyncExecutionRules timed) : Nat :=
  rules.peerRotationBound + 3 * timed.localActionBound + 2 * network.delta + 5

private theorem block_sync_request_delivery_bound
    {start rotation localBound delta requestEnabledAt requestCompletedAt
      requestSentAt requestDeliveredAt : Nat}
    (requestEnabledBound : requestEnabledAt ≤ start + rotation)
    (requestCompletionBound :
      requestCompletedAt ≤ requestEnabledAt + localBound)
    (requestSent : requestSentAt = requestCompletedAt + 1)
    (requestDelivery : requestDeliveredAt ≤ requestSentAt + delta) :
    requestDeliveredAt ≤ start + rotation + localBound + 1 + delta := by
  omega

private theorem block_sync_request_arrival_within_acceptance_bound
    (start rotation localBound delta : Nat) :
    start + rotation + localBound + 1 + delta + 1 ≤
      start + rotation + 3 * localBound + 2 * delta + 5 := by
  omega

private theorem block_sync_response_delivery_bound
    {start rotation localBound delta requestDeliveredAt serveCompletedAt
      responseSentAt responseDeliveredAt : Nat}
    (requestDeliveryBound :
      requestDeliveredAt ≤ start + rotation + localBound + 1 + delta)
    (serveCompletionBound :
      serveCompletedAt ≤ requestDeliveredAt + 1 + localBound)
    (responseSent : responseSentAt = serveCompletedAt + 1)
    (responseDelivery : responseDeliveredAt ≤ responseSentAt + delta) :
    responseDeliveredAt ≤
      start + rotation + localBound + 1 + delta + 1 + localBound + 1 +
        delta := by
  omega

private theorem block_sync_response_arrival_within_acceptance_bound
    (start rotation localBound delta : Nat) :
    start + rotation + localBound + 1 + delta + 1 + localBound + 1 + delta + 1 ≤
      start + rotation + 3 * localBound + 2 * delta + 5 := by
  omega

private theorem block_sync_accept_visibility_within_acceptance_bound
    {start rotation localBound delta responseDeliveredAt acceptCompletedAt : Nat}
    (responseDeliveryBound : responseDeliveredAt ≤
      start + rotation + localBound + 1 + delta + 1 + localBound + 1 + delta)
    (acceptCompletionBound :
      acceptCompletedAt ≤ responseDeliveredAt + 1 + localBound) :
    acceptCompletedAt + 1 ≤
      start + rotation + 3 * localBound + 2 * delta + 5 := by
  omega

/-- A retained block body reaches one correct requester within one fixed
request bound, or the requester goal becomes obsolete first.

This theorem stops at exact packet delivery. It does not require the block's
parents to be ready and does not assume that the block is accepted. -/
theorem retained_validator_block_body_delivered_or_obsolete_within_bound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder requester : Nat} {block : ValidatorBlock BlockId} {start : Time}
    (source : RetainedValidatorBlock timed.execution holder block start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (protectedWhilePending : ∀ time,
      start ≤ time →
      ((timed.execution.trace time).validatorState requester).accepted
          block.reference = false →
      ¬rules.goalObsolete requester block.reference time →
      rules.sourceProtected holder block.reference time) :
    ∃ finish,
      start ≤ finish ∧
        finish ≤ start + validatorBlockSyncAcceptanceBound timed rules ∧
        (ValidatorLocalBlockBodyAt timed finish requester block ∨
          rules.goalObsolete requester block.reference finish) := by
  by_cases requesterIsHolder : requester = holder
  · subst requester
    exact ⟨start, Nat.le_refl start, Nat.le_add_right start _, Or.inl
      (.acceptedCatalogued source.accepted source.catalog)⟩

  cases acceptedAtStart :
      ((timed.execution.trace start).validatorState requester).accepted
        block.reference with
  | true =>
      exact ⟨start, Nat.le_refl start, Nat.le_add_right start _, Or.inl
        (.acceptedCatalogued acceptedAtStart source.catalog)⟩
  | false =>
      rcases rules.visitPeer requester holder block.reference start
          requesterInRange requesterCorrectAvailable
          (validator_block_sync_peer_order_contains source.holderInRange) active
          acceptedAtStart with
        ⟨requestEnabledAt, startBeforeRequest, requestEnabledBound, progress⟩
      have requestEnabledWithinBound : requestEnabledAt ≤
          start + validatorBlockSyncAcceptanceBound timed rules := by
        unfold validatorBlockSyncAcceptanceBound
        exact Nat.le_trans requestEnabledBound (by simp [Nat.add_assoc])
      rcases progress with
        obsoleteBeforeRequest | acceptedBeforeRequest | requestLatch
      · exact ⟨requestEnabledAt, startBeforeRequest,
          requestEnabledWithinBound, Or.inr obsoleteBeforeRequest⟩
      · have catalogAtRequest := timed.execution.blockCatalogMonotone start
            requestEnabledAt startBeforeRequest block.reference.id block
            source.catalog
        exact ⟨requestEnabledAt, startBeforeRequest,
          requestEnabledWithinBound, Or.inl
            (.acceptedCatalogued acceptedBeforeRequest catalogAtRequest)⟩

      let requestCompletion := timed.completeProtectedAction requester
        (.requestBlock holder block.reference) requestEnabledAt requesterInRange
        requesterCorrectAvailable
        (rules.requestLatchIsProtected requestEnabledAt requester holder
          block.reference requestLatch)
      have requestOccurs := requestCompletion.occurs
      have requestCompletionBound : requestCompletion.event.completedAt ≤
          requestEnabledAt + timed.localActionBound := by
        simpa [requestCompletion.sameEnableTime] using
          requestCompletion.completesWithinBound
      rcases rules.requestActionCreatesPacket requestCompletion.event.completedAt
          requester holder block.reference requestOccurs with
        ⟨requestPacketId, requestPacket, requestPresent, requestProtocol,
          requestSender, requestReceiver, requestPayload, requestSentAt⟩

      have requestSenderInRange :
          requestPacket.sender < config.authorityCount := by
        rw [requestSender]
        exact requesterInRange
      have requestReceiverInRange :
          requestPacket.receiver < config.authorityCount := by
        rw [requestReceiver]
        exact source.holderInRange
      have requestSenderCorrect :
          faults.correctAvailable requestPacket.sender = true := by
        rw [requestSender]
        exact requesterCorrectAvailable
      have requestReceiverCorrect :
          faults.correctAvailable requestPacket.receiver = true := by
        rw [requestReceiver]
        exact source.holderCorrectAvailable
      have startBeforeRequestCompletion :
          start ≤ requestCompletion.event.completedAt := by
        have enableBeforeCompletion :
            requestEnabledAt ≤ requestCompletion.event.completedAt := by
          simpa [requestCompletion.sameEnableTime] using
            requestCompletion.enableBeforeCompletion
        exact Nat.le_trans startBeforeRequest enableBeforeCompletion
      have startBeforeRequestSent : start ≤ requestPacket.sentAt := by
        rw [requestSentAt]
        exact Nat.le_trans startBeforeRequestCompletion (Nat.le_succ _)
      have requestAfterGst : network.gst ≤ requestPacket.sentAt :=
        Nat.le_trans afterGst startBeforeRequestSent
      have requestPresentAtSend :
          (timed.execution.trace requestPacket.sentAt).packets requestPacketId =
            some requestPacket := by
        simpa [requestSentAt] using requestPresent
      have requestDelivery := validator_protocol_packet_is_delivered
        timed.execution requestPresentAtSend requestProtocol requestSenderInRange
        requestReceiverInRange requestSenderCorrect requestReceiverCorrect
        requestAfterGst
      have requestTiming := network.postGstDelivery requestPacket requestProtocol
        requestSenderInRange requestReceiverInRange requestSenderCorrect
        requestReceiverCorrect requestAfterGst
      have startBeforeRequestDelivery : start ≤ requestPacket.deliveredAt :=
        Nat.le_trans startBeforeRequestSent requestTiming.1
      have requestDeliveryBound : requestPacket.deliveredAt ≤
          start + rules.peerRotationBound + timed.localActionBound + 1 +
            network.delta := by
        exact block_sync_request_delivery_bound requestEnabledBound
          requestCompletionBound requestSentAt requestTiming.2

      let requestArrival := requestPacket.deliveredAt + 1
      have requestArrivalBound : requestArrival ≤
          start + rules.peerRotationBound + timed.localActionBound + 1 +
            network.delta + 1 := by
        dsimp [requestArrival]
        exact Nat.add_le_add_right requestDeliveryBound 1
      have requestArrivalWithinBound : requestArrival ≤
          start + validatorBlockSyncAcceptanceBound timed rules := by
        unfold validatorBlockSyncAcceptanceBound
        simpa [Nat.add_assoc] using
          Nat.le_trans requestArrivalBound
            (block_sync_request_arrival_within_acceptance_bound start
              rules.peerRotationBound timed.localActionBound network.delta)
      cases acceptedAtRequestArrival :
          ((timed.execution.trace requestArrival).validatorState
            requester).accepted block.reference with
      | true =>
          have catalogAtArrival := timed.execution.blockCatalogMonotone start
            requestArrival
            (Nat.le_trans startBeforeRequestDelivery (Nat.le_succ _))
            block.reference.id block source.catalog
          exact ⟨requestArrival,
            Nat.le_trans startBeforeRequestDelivery (Nat.le_succ _),
            requestArrivalWithinBound, Or.inl
              (.acceptedCatalogued acceptedAtRequestArrival catalogAtArrival)⟩
      | false =>
      by_cases obsoleteAtRequestArrival :
          rules.goalObsolete requester block.reference requestArrival
      · exact ⟨requestArrival,
          Nat.le_trans startBeforeRequestDelivery (Nat.le_succ _),
          requestArrivalWithinBound, Or.inr obsoleteAtRequestArrival⟩
      · have sourceAtRequestDelivery := retained_validator_block_persists
            rules source (later := requestArrival)
              (Nat.le_trans startBeforeRequestDelivery (Nat.le_succ _)) (by
              intro time timeAfterStart timeBeforeArrival
              apply protectedWhilePending time timeAfterStart
              · cases acceptedAtTime :
                    ((timed.execution.trace time).validatorState
                      requester).accepted block.reference with
                | false => rfl
                | true =>
                    have persisted := timed.execution.accepted_block_persists
                      requesterInRange timeBeforeArrival acceptedAtTime
                    rw [acceptedAtRequestArrival] at persisted
                    contradiction
              · intro obsoleteAtTime
                exact obsoleteAtRequestArrival
                  (rules.goalObsoletePersists requester block.reference time
                    requestArrival timeBeforeArrival obsoleteAtTime))
        have serveLatch := rules.deliveredRequestEnablesServe
          requestPacket.deliveredAt requestPacketId requestPacket block.reference
          block (timed.execution.packetHistoryMonotone requestPacket.sentAt
            requestPacket.deliveredAt requestTiming.1 requestPacketId
            requestPacket requestPresentAtSend)
          requestPayload requestDelivery (by
            simpa [requestReceiver] using sourceAtRequestDelivery.retained)
          sourceAtRequestDelivery.catalog rfl

        let serveCompletion := timed.completeProtectedAction holder
          (.serveBlock requester block.reference) (requestPacket.deliveredAt + 1)
          source.holderInRange source.holderCorrectAvailable (by
            apply rules.serveLatchIsProtected
            simpa [requestSender, requestReceiver] using serveLatch)
        have serveCompletionBound : serveCompletion.event.completedAt ≤
            requestPacket.deliveredAt + 1 + timed.localActionBound := by
          simpa [serveCompletion.sameEnableTime] using
            serveCompletion.completesWithinBound
        have startBeforeServeCompletion :
            start ≤ serveCompletion.event.completedAt := by
          have enableBeforeCompletion : requestPacket.deliveredAt + 1 ≤
              serveCompletion.event.completedAt := by
            simpa [serveCompletion.sameEnableTime] using
              serveCompletion.enableBeforeCompletion
          exact Nat.le_trans
            (Nat.le_trans startBeforeRequestDelivery (Nat.le_succ _))
            enableBeforeCompletion
        have catalogAtServe :
            (timed.execution.trace serveCompletion.event.completedAt).blockCatalog
                block.reference.id = some block := by
          exact timed.execution.blockCatalogMonotone start
            serveCompletion.event.completedAt startBeforeServeCompletion
            block.reference.id block source.catalog
        rcases rules.serveActionCreatesPacket serveCompletion.event.completedAt
            holder requester block.reference block serveCompletion.occurs
            catalogAtServe rfl with
          ⟨responsePacketId, responsePacket, responsePresent, responseProtocol,
            responseSender, responseReceiver, responsePayload, responseSentAt⟩

        have responseSenderInRange :
            responsePacket.sender < config.authorityCount := by
          rw [responseSender]
          exact source.holderInRange
        have responseReceiverInRange :
            responsePacket.receiver < config.authorityCount := by
          rw [responseReceiver]
          exact requesterInRange
        have responseSenderCorrect :
            faults.correctAvailable responsePacket.sender = true := by
          rw [responseSender]
          exact source.holderCorrectAvailable
        have responseReceiverCorrect :
            faults.correctAvailable responsePacket.receiver = true := by
          rw [responseReceiver]
          exact requesterCorrectAvailable
        have startBeforeResponseSent : start ≤ responsePacket.sentAt := by
          rw [responseSentAt]
          exact Nat.le_trans startBeforeServeCompletion (Nat.le_succ _)
        have responseAfterGst : network.gst ≤ responsePacket.sentAt :=
          Nat.le_trans afterGst startBeforeResponseSent
        have responsePresentAtSend :
            (timed.execution.trace responsePacket.sentAt).packets
                responsePacketId = some responsePacket := by
          simpa [responseSentAt] using responsePresent
        have responseDelivery := validator_protocol_packet_is_delivered
          timed.execution responsePresentAtSend responseProtocol
          responseSenderInRange responseReceiverInRange responseSenderCorrect
          responseReceiverCorrect responseAfterGst
        have responseTiming := network.postGstDelivery responsePacket
          responseProtocol responseSenderInRange responseReceiverInRange
          responseSenderCorrect responseReceiverCorrect responseAfterGst
        have startBeforeResponseDelivery : start ≤ responsePacket.deliveredAt :=
          Nat.le_trans startBeforeResponseSent responseTiming.1
        have responseDeliveryBound : responsePacket.deliveredAt ≤
            start + rules.peerRotationBound + timed.localActionBound + 1 +
              network.delta + 1 + timed.localActionBound + 1 +
              network.delta := by
          exact block_sync_response_delivery_bound requestDeliveryBound
            serveCompletionBound responseSentAt responseTiming.2
        have responseArrivalBound : responsePacket.deliveredAt + 1 ≤
            start + rules.peerRotationBound + timed.localActionBound + 1 +
              network.delta + 1 + timed.localActionBound + 1 +
              network.delta + 1 :=
          Nat.add_le_add_right responseDeliveryBound 1
        have responseArrivalWithinBound : responsePacket.deliveredAt + 1 ≤
            start + validatorBlockSyncAcceptanceBound timed rules := by
          unfold validatorBlockSyncAcceptanceBound
          simpa [Nat.add_assoc] using
            Nat.le_trans responseArrivalBound
              (block_sync_response_arrival_within_acceptance_bound start
                rules.peerRotationBound timed.localActionBound network.delta)
        have responsePresentAtDelivery :
            (timed.execution.trace responsePacket.deliveredAt).packets
                responsePacketId = some responsePacket := by
          exact timed.execution.packetHistoryMonotone responsePacket.sentAt
            responsePacket.deliveredAt responseTiming.1 responsePacketId
            responsePacket responsePresentAtSend
        exact ⟨responsePacket.deliveredAt, startBeforeResponseDelivery,
          Nat.le_trans (Nat.le_succ _) responseArrivalWithinBound, Or.inl
            (.delivered responsePacketId responsePacket responsePresentAtDelivery
              responseProtocol responseReceiver responsePayload
              responseDelivery)⟩

/-- A retained parent-ready block is accepted within one fixed request bound,
or the local request goal becomes obsolete.

The source pin is required only while the request is not accepted and is not
obsolete. A later commit or cleanup boundary can therefore release the pin. -/
theorem retained_validator_block_accepted_or_obsolete_within_bound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder requester : Nat} {block : ValidatorBlock BlockId} {start : Time}
    (source : RetainedValidatorBlock timed.execution holder block start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (protectedWhilePending : ∀ time,
      start ≤ time →
      ((timed.execution.trace time).validatorState requester).accepted
          block.reference = false →
      ¬rules.goalObsolete requester block.reference time →
      rules.sourceProtected holder block.reference time)
    (parentsReady : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace start).validatorState requester).accepted parent =
          true ∨
        parent.round ≤
          ((timed.execution.trace start).validatorState requester).gcRound) :
    ∃ finish,
      start ≤ finish ∧
        finish ≤ start + validatorBlockSyncAcceptanceBound timed rules ∧
        (((timed.execution.trace finish).validatorState requester).accepted
            block.reference = true ∨
          rules.goalObsolete requester block.reference finish) := by
  by_cases requesterIsHolder : requester = holder
  · subst requester
    exact ⟨start, Nat.le_refl start, Nat.le_add_right start _,
      Or.inl source.accepted⟩

  cases acceptedAtStart :
      ((timed.execution.trace start).validatorState requester).accepted
        block.reference with
  | true =>
      exact ⟨start, Nat.le_refl start, Nat.le_add_right start _,
        Or.inl acceptedAtStart⟩
  | false =>
      rcases rules.visitPeer requester holder block.reference start
          requesterInRange requesterCorrectAvailable
          (validator_block_sync_peer_order_contains source.holderInRange) active
          acceptedAtStart with
        ⟨requestEnabledAt, startBeforeRequest, requestEnabledBound, progress⟩
      have requestEnabledWithinBound : requestEnabledAt ≤
          start + validatorBlockSyncAcceptanceBound timed rules := by
        unfold validatorBlockSyncAcceptanceBound
        exact Nat.le_trans requestEnabledBound (by
          simp [Nat.add_assoc])
      rcases progress with
        obsoleteBeforeRequest | acceptedBeforeRequest | requestLatch
      · exact ⟨requestEnabledAt, startBeforeRequest,
          requestEnabledWithinBound, Or.inr obsoleteBeforeRequest⟩
      · exact ⟨requestEnabledAt, startBeforeRequest,
          requestEnabledWithinBound, Or.inl acceptedBeforeRequest⟩

      let requestCompletion := timed.completeProtectedAction requester
        (.requestBlock holder block.reference) requestEnabledAt requesterInRange
        requesterCorrectAvailable
        (rules.requestLatchIsProtected requestEnabledAt requester holder
          block.reference requestLatch)
      have requestOccurs := requestCompletion.occurs
      have requestCompletionBound : requestCompletion.event.completedAt ≤
          requestEnabledAt + timed.localActionBound := by
        simpa [requestCompletion.sameEnableTime] using
          requestCompletion.completesWithinBound
      rcases rules.requestActionCreatesPacket requestCompletion.event.completedAt
          requester holder block.reference requestOccurs with
        ⟨requestPacketId, requestPacket, requestPresent, requestProtocol,
          requestSender, requestReceiver, requestPayload, requestSentAt⟩

      have requestSenderInRange :
          requestPacket.sender < config.authorityCount := by
        rw [requestSender]
        exact requesterInRange
      have requestReceiverInRange :
          requestPacket.receiver < config.authorityCount := by
        rw [requestReceiver]
        exact source.holderInRange
      have requestSenderCorrect :
          faults.correctAvailable requestPacket.sender = true := by
        rw [requestSender]
        exact requesterCorrectAvailable
      have requestReceiverCorrect :
          faults.correctAvailable requestPacket.receiver = true := by
        rw [requestReceiver]
        exact source.holderCorrectAvailable
      have startBeforeRequestCompletion :
          start ≤ requestCompletion.event.completedAt := by
        have enableBeforeCompletion :
            requestEnabledAt ≤ requestCompletion.event.completedAt := by
          simpa [requestCompletion.sameEnableTime] using
            requestCompletion.enableBeforeCompletion
        exact Nat.le_trans startBeforeRequest enableBeforeCompletion
      have startBeforeRequestSent : start ≤ requestPacket.sentAt := by
        rw [requestSentAt]
        exact Nat.le_trans startBeforeRequestCompletion (Nat.le_succ _)
      have requestAfterGst : network.gst ≤ requestPacket.sentAt := by
        exact Nat.le_trans afterGst startBeforeRequestSent
      have requestPresentAtSend :
          (timed.execution.trace requestPacket.sentAt).packets requestPacketId =
            some requestPacket := by
        simpa [requestSentAt] using requestPresent
      have requestDelivery := validator_protocol_packet_is_delivered
        timed.execution requestPresentAtSend requestProtocol requestSenderInRange
        requestReceiverInRange requestSenderCorrect requestReceiverCorrect
        requestAfterGst
      have requestTiming := network.postGstDelivery requestPacket requestProtocol
        requestSenderInRange requestReceiverInRange requestSenderCorrect
        requestReceiverCorrect requestAfterGst
      have startBeforeRequestDelivery : start ≤ requestPacket.deliveredAt := by
        exact Nat.le_trans startBeforeRequestSent requestTiming.1
      have requestDeliveryBound : requestPacket.deliveredAt ≤
          start + rules.peerRotationBound + timed.localActionBound + 1 +
            network.delta := by
        exact block_sync_request_delivery_bound requestEnabledBound
          requestCompletionBound requestSentAt requestTiming.2

      let requestArrival := requestPacket.deliveredAt + 1
      have requestArrivalBound : requestArrival ≤
          start + rules.peerRotationBound + timed.localActionBound + 1 +
            network.delta + 1 := by
        dsimp [requestArrival]
        exact Nat.add_le_add_right requestDeliveryBound 1
      have requestArrivalWithinBound : requestArrival ≤
          start + validatorBlockSyncAcceptanceBound timed rules := by
        unfold validatorBlockSyncAcceptanceBound
        simpa [Nat.add_assoc] using
          Nat.le_trans requestArrivalBound
            (block_sync_request_arrival_within_acceptance_bound start
              rules.peerRotationBound timed.localActionBound network.delta)
      cases acceptedAtRequestArrival :
          ((timed.execution.trace requestArrival).validatorState requester).accepted
            block.reference with
      | true =>
          exact ⟨requestArrival,
            Nat.le_trans startBeforeRequestDelivery (Nat.le_succ _),
            requestArrivalWithinBound,
            Or.inl acceptedAtRequestArrival⟩
      | false =>
      by_cases obsoleteAtRequestArrival :
          rules.goalObsolete requester block.reference requestArrival
      · exact ⟨requestArrival,
          Nat.le_trans startBeforeRequestDelivery (Nat.le_succ _),
          requestArrivalWithinBound,
          Or.inr obsoleteAtRequestArrival⟩
      · have sourceAtRequestDelivery := retained_validator_block_persists rules
            source (later := requestArrival)
              (Nat.le_trans startBeforeRequestDelivery (Nat.le_succ _)) (by
              intro time timeAfterStart timeBeforeArrival
              apply protectedWhilePending time timeAfterStart
              · cases acceptedAtTime :
                    ((timed.execution.trace time).validatorState requester).accepted
                      block.reference with
                | false => rfl
                | true =>
                    have persisted := timed.execution.accepted_block_persists
                      requesterInRange timeBeforeArrival acceptedAtTime
                    rw [acceptedAtRequestArrival] at persisted
                    contradiction
              · intro obsoleteAtTime
                exact obsoleteAtRequestArrival
                  (rules.goalObsoletePersists requester block.reference time
                    requestArrival timeBeforeArrival obsoleteAtTime))
        have serveLatch := rules.deliveredRequestEnablesServe
          requestPacket.deliveredAt requestPacketId requestPacket block.reference
          block (by
            exact timed.execution.packetHistoryMonotone requestPacket.sentAt
              requestPacket.deliveredAt requestTiming.1 requestPacketId
              requestPacket requestPresentAtSend)
          requestPayload requestDelivery (by
            simpa [requestReceiver] using sourceAtRequestDelivery.retained)
          sourceAtRequestDelivery.catalog rfl

        let serveCompletion := timed.completeProtectedAction holder
          (.serveBlock requester block.reference) (requestPacket.deliveredAt + 1)
          source.holderInRange source.holderCorrectAvailable (by
            apply rules.serveLatchIsProtected
            simpa [requestSender, requestReceiver] using serveLatch)
        have serveEnableBeforeCompletion : requestPacket.deliveredAt + 1 ≤
            serveCompletion.event.completedAt := by
          simpa [serveCompletion.sameEnableTime] using
            serveCompletion.enableBeforeCompletion
        have serveCompletionBound : serveCompletion.event.completedAt ≤
            requestPacket.deliveredAt + 1 + timed.localActionBound := by
          simpa [serveCompletion.sameEnableTime] using
            serveCompletion.completesWithinBound
        have startBeforeServeCompletion :
            start ≤ serveCompletion.event.completedAt := by
          exact Nat.le_trans
            (Nat.le_trans startBeforeRequestDelivery (Nat.le_succ _))
            serveEnableBeforeCompletion
        have catalogAtServe :
            (timed.execution.trace serveCompletion.event.completedAt).blockCatalog
                block.reference.id = some block := by
          exact timed.execution.blockCatalogMonotone start
            serveCompletion.event.completedAt startBeforeServeCompletion
            block.reference.id block source.catalog
        rcases rules.serveActionCreatesPacket serveCompletion.event.completedAt
            holder requester block.reference block serveCompletion.occurs
            catalogAtServe rfl with
          ⟨responsePacketId, responsePacket, responsePresent, responseProtocol,
            responseSender, responseReceiver, responsePayload, responseSentAt⟩

        have responseSenderInRange :
            responsePacket.sender < config.authorityCount := by
          rw [responseSender]
          exact source.holderInRange
        have responseReceiverInRange :
            responsePacket.receiver < config.authorityCount := by
          rw [responseReceiver]
          exact requesterInRange
        have responseSenderCorrect :
            faults.correctAvailable responsePacket.sender = true := by
          rw [responseSender]
          exact source.holderCorrectAvailable
        have responseReceiverCorrect :
            faults.correctAvailable responsePacket.receiver = true := by
          rw [responseReceiver]
          exact requesterCorrectAvailable
        have startBeforeResponseSent : start ≤ responsePacket.sentAt := by
          rw [responseSentAt]
          exact Nat.le_trans startBeforeServeCompletion (Nat.le_succ _)
        have responseAfterGst : network.gst ≤ responsePacket.sentAt := by
          exact Nat.le_trans afterGst startBeforeResponseSent
        have responsePresentAtSend :
            (timed.execution.trace responsePacket.sentAt).packets
                responsePacketId = some responsePacket := by
          simpa [responseSentAt] using responsePresent
        have responseDelivery := validator_protocol_packet_is_delivered
          timed.execution responsePresentAtSend responseProtocol
          responseSenderInRange responseReceiverInRange responseSenderCorrect
          responseReceiverCorrect responseAfterGst
        have responseTiming := network.postGstDelivery responsePacket
          responseProtocol responseSenderInRange responseReceiverInRange
          responseSenderCorrect responseReceiverCorrect responseAfterGst
        have startBeforeResponseDelivery : start ≤ responsePacket.deliveredAt := by
          exact Nat.le_trans startBeforeResponseSent responseTiming.1
        have responseDeliveryBound : responsePacket.deliveredAt ≤
            start + rules.peerRotationBound + timed.localActionBound + 1 +
              network.delta + 1 + timed.localActionBound + 1 +
              network.delta := by
          exact block_sync_response_delivery_bound requestDeliveryBound
            serveCompletionBound responseSentAt responseTiming.2
        have responseArrivalBound : responsePacket.deliveredAt + 1 ≤
            start + rules.peerRotationBound + timed.localActionBound + 1 +
              network.delta + 1 + timed.localActionBound + 1 +
              network.delta + 1 := by
          exact Nat.add_le_add_right responseDeliveryBound 1
        have responseArrivalWithinBound : responsePacket.deliveredAt + 1 ≤
            start + validatorBlockSyncAcceptanceBound timed rules := by
          unfold validatorBlockSyncAcceptanceBound
          simpa [Nat.add_assoc] using
            Nat.le_trans responseArrivalBound
              (block_sync_response_arrival_within_acceptance_bound start
                rules.peerRotationBound timed.localActionBound network.delta)
        have responsePresentAtDelivery :
            (timed.execution.trace responsePacket.deliveredAt).packets
                responsePacketId = some responsePacket := by
          exact timed.execution.packetHistoryMonotone responsePacket.sentAt
            responsePacket.deliveredAt responseTiming.1 responsePacketId
            responsePacket responsePresentAtSend
        have parentsReadyAtDelivery : ∀ parent, parent ∈ block.parents →
            ((timed.execution.trace
                (responsePacket.deliveredAt + 1)).validatorState
                  responsePacket.receiver).accepted parent = true ∨
              parent.round ≤
                ((timed.execution.trace
                  (responsePacket.deliveredAt + 1)).validatorState
                    responsePacket.receiver).gcRound := by
          intro parent parentInBlock
          rw [responseReceiver]
          rcases parentsReady parent parentInBlock with
            parentAccepted | parentAtGc
          · exact Or.inl (timed.execution.accepted_block_persists
              requesterInRange
              (Nat.le_trans startBeforeResponseDelivery (Nat.le_succ _))
              parentAccepted)
          · right
            have durable := timed.execution.durableStateMonotone requester start
              (responsePacket.deliveredAt + 1) requesterInRange
              (Nat.le_trans startBeforeResponseDelivery (Nat.le_succ _))
            rcases durable with
              ⟨_, _, _, _, _, _, _, _, _, _, _, gcMonotone⟩
            exact Nat.le_trans parentAtGc gcMonotone
        by_cases obsoleteAtResponseArrival : rules.goalObsolete requester
            block.reference (responsePacket.deliveredAt + 1)
        · exact ⟨responsePacket.deliveredAt + 1,
            Nat.le_trans startBeforeResponseDelivery (Nat.le_succ _),
            responseArrivalWithinBound,
            Or.inr obsoleteAtResponseArrival⟩
        · have blockAboveRequesterGc :
              ((timed.execution.trace
                (responsePacket.deliveredAt + 1)).validatorState requester).gcRound <
                block.reference.round := by
            apply Nat.lt_of_not_ge
            intro atOrBelowGc
            exact obsoleteAtResponseArrival
              (rules.gcFrontierMakesGoalObsolete requester block.reference
                (responsePacket.deliveredAt + 1) requesterInRange
                requesterCorrectAvailable atOrBelowGc)
          rcases rules.deliveredBlockEnablesAccept responsePacket.deliveredAt
              responsePacketId responsePacket block responsePresentAtDelivery
              responsePayload responseDelivery source.authorInRange
              source.validParents parentsReadyAtDelivery (by
                simpa [responseReceiver] using blockAboveRequesterGc) with
            acceptedAtDelivery | acceptLatch
          · exact ⟨responsePacket.deliveredAt + 1,
              Nat.le_trans startBeforeResponseDelivery (Nat.le_succ _), by
              exact responseArrivalWithinBound, by
              exact Or.inl (by
                simpa [responseReceiver] using acceptedAtDelivery)⟩
          · let acceptCompletion := timed.completeProtectedAction requester
              (.acceptBlock block) (responsePacket.deliveredAt + 1)
              requesterInRange requesterCorrectAvailable (by
                apply rules.acceptLatchIsProtected
                simpa [responseReceiver] using acceptLatch)
            have acceptCompletionBound : acceptCompletion.event.completedAt ≤
                responsePacket.deliveredAt + 1 + timed.localActionBound := by
              simpa [acceptCompletion.sameEnableTime] using
                acceptCompletion.completesWithinBound
            have acceptFinishWithinBound : acceptCompletion.event.completedAt + 1 ≤
                start + validatorBlockSyncAcceptanceBound timed rules := by
              unfold validatorBlockSyncAcceptanceBound
              simpa [Nat.add_assoc] using
                block_sync_accept_visibility_within_acceptance_bound
                  responseDeliveryBound acceptCompletionBound
            exact ⟨acceptCompletion.event.completedAt + 1, by
              have enableBeforeCompletion : responsePacket.deliveredAt + 1 ≤
                  acceptCompletion.event.completedAt := by
                simpa [acceptCompletion.sameEnableTime] using
                  acceptCompletion.enableBeforeCompletion
              exact Nat.le_trans
                (Nat.le_trans startBeforeResponseDelivery (Nat.le_succ _))
                (Nat.le_trans enableBeforeCompletion (Nat.le_succ _)),
              acceptFinishWithinBound,
              Or.inl
                (accept_block_occurrence_stores_accepted_block timed.execution
                  acceptCompletion.occurs)⟩

/-- A retained parent-ready block is eventually accepted, or the local request
goal becomes obsolete. -/
theorem retained_validator_block_eventually_accepted_or_obsolete
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder requester : Nat} {block : ValidatorBlock BlockId} {start : Time}
    (source : RetainedValidatorBlock timed.execution holder block start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (protectedWhilePending : ∀ time,
      start ≤ time →
      ((timed.execution.trace time).validatorState requester).accepted
          block.reference = false →
      ¬rules.goalObsolete requester block.reference time →
      rules.sourceProtected holder block.reference time)
    (parentsReady : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace start).validatorState requester).accepted parent =
          true ∨
        parent.round ≤
          ((timed.execution.trace start).validatorState requester).gcRound) :
    ∃ finish,
      start ≤ finish ∧
      (((timed.execution.trace finish).validatorState requester).accepted
          block.reference = true ∨
        rules.goalObsolete requester block.reference finish) := by
  rcases retained_validator_block_accepted_or_obsolete_within_bound rules source
      requesterInRange requesterCorrectAvailable afterGst active
      protectedWhilePending parentsReady with
    ⟨finish, startBeforeFinish, _finishWithinBound, result⟩
  exact ⟨finish, startBeforeFinish, result⟩

/-- A still-required protected block is eventually accepted. The pin can be
released immediately after acceptance. -/
theorem retained_validator_block_eventually_accepted
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder requester : Nat} {block : ValidatorBlock BlockId} {start : Time}
    (source : RetainedValidatorBlock timed.execution holder block start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (protectedWhilePending : ∀ time,
      start ≤ time →
      ((timed.execution.trace time).validatorState requester).accepted
          block.reference = false →
      ¬rules.goalObsolete requester block.reference time →
      rules.sourceProtected holder block.reference time)
    (requiredUntilAccepted : ∀ time,
      start ≤ time →
      ((timed.execution.trace time).validatorState requester).accepted
          block.reference = false →
      ¬rules.goalObsolete requester block.reference time)
    (parentsAccepted : ∀ parent, parent ∈ block.parents →
      ((timed.execution.trace start).validatorState requester).accepted parent =
        true) :
    ∃ finish,
      start ≤ finish ∧
      ((timed.execution.trace finish).validatorState requester).accepted
        block.reference = true := by
  rcases retained_validator_block_eventually_accepted_or_obsolete rules source
      requesterInRange requesterCorrectAvailable afterGst active
      protectedWhilePending
      (fun parent member => Or.inl (parentsAccepted parent member)) with
    ⟨finish, startBeforeFinish, accepted | obsolete⟩
  · exact ⟨finish, startBeforeFinish, accepted⟩
  · cases acceptedAtFinish :
        ((timed.execution.trace finish).validatorState requester).accepted
          block.reference with
    | true => exact ⟨finish, startBeforeFinish, acceptedAtFinish⟩
    | false =>
        exact False.elim
          ((requiredUntilAccepted finish startBeforeFinish acceptedAtFinish)
            obsolete)

/-- A finite block history is in parent-first order relative to an initial set
of accepted references. -/
def ParentFirstValidatorBlockHistory
    {BlockId : Type}
    (initiallyAccepted : ValidatorBlockRef BlockId → Prop) :
    List (ValidatorBlock BlockId) → Prop
  | [] => True
  | block :: remaining =>
      (∀ parent, parent ∈ block.parents → initiallyAccepted parent) ∧
        ParentFirstValidatorBlockHistory
          (fun reference =>
            initiallyAccepted reference ∨ reference = block.reference)
          remaining

/-- One reference is ready for causal processing when it is accepted or when
the receiver's current GC round has made it a committed root. -/
def ValidatorReferenceAcceptedOrGcRootAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Nat) (reference : ValidatorBlockRef BlockId) : Prop :=
  ((execution.trace time).validatorState validator).accepted reference = true ∨
    reference.round ≤
      ((execution.trace time).validatorState validator).gcRound

/-- Acceptance and GC-root readiness both persist in one correct validator's
trace. -/
theorem validator_reference_accepted_or_gc_root_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {earlier later validator : Nat} {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (ordered : earlier ≤ later)
    (ready : ValidatorReferenceAcceptedOrGcRootAt execution earlier validator
      reference) :
    ValidatorReferenceAcceptedOrGcRootAt execution later validator
      reference := by
  rcases ready with accepted | atGc
  · exact Or.inl
      (execution.accepted_block_persists validatorInRange ordered accepted)
  · right
    have durable := execution.durableStateMonotone validator earlier later
      validatorInRange ordered
    rcases durable with ⟨_, _, _, _, _, _, _, _, _, _, _, gcMonotone⟩
    exact Nat.le_trans atGc gcMonotone

/-- Parent-first order is preserved when the accepted-reference predicate gets
stronger. -/
theorem parent_first_validator_block_history_mono
    {BlockId : Type}
    {initiallyAccepted laterAccepted : ValidatorBlockRef BlockId → Prop}
    {blocks : List (ValidatorBlock BlockId)}
    (monotone : ∀ reference,
      initiallyAccepted reference → laterAccepted reference)
    (ordered : ParentFirstValidatorBlockHistory initiallyAccepted blocks) :
    ParentFirstValidatorBlockHistory laterAccepted blocks := by
  induction blocks generalizing initiallyAccepted laterAccepted with
  | nil => trivial
  | cons block remaining ih =>
      constructor
      · intro parent parentInBlock
        exact monotone parent (ordered.1 parent parentInBlock)
      · exact ih (fun reference accepted => by
          rcases accepted with acceptedInitially | sameBlock
          · exact Or.inl (monotone reference acceptedInitially)
          · exact Or.inr sameBlock) ordered.2

/-- In a list without duplicates, an item that occurs before a target belongs
to every front segment that ends immediately before that target. -/
theorem list_before_member_of_front
    {α : Type} {list front suffix before between after : List α} {item target : α}
    (nodup : list.Nodup)
    (split : list = front ++ (target :: suffix))
    (ordered : list = before ++ (item :: between ++ (target :: after))) :
    item ∈ front := by
  classical
  have nodupSplit : (front ++ (target :: suffix)).Nodup := by
    simpa [split] using nodup
  have nodupOrder :
      (before ++ (item :: between ++ (target :: after))).Nodup := by
    simpa [ordered] using nodup
  have targetNotInFront : target ∉ front := by
    intro targetMember
    have cross := (List.nodup_append.mp nodupSplit).2.2 target targetMember
      target (by simp)
    exact cross rfl
  have orderParts := List.nodup_append.mp nodupOrder
  have itemNotBefore : item ∉ before := by
    intro itemMember
    exact (orderParts.2.2 item itemMember item (by simp)) rfl
  have targetNotBefore : target ∉ before := by
    intro targetMember
    exact (orderParts.2.2 target targetMember target (by simp)) rfl
  have tailNodup : (item :: between ++ (target :: after)).Nodup :=
    orderParts.2.1
  have itemNeTarget : item ≠ target := by
    intro same
    subst target
    exact (List.nodup_cons.mp tailNodup).1 (by simp)
  have targetIndexAtFront : List.idxOf target list = front.length := by
    rw [split, List.idxOf_append]
    simp [targetNotInFront]
  have itemIndex : List.idxOf item list = before.length := by
    rw [ordered, List.idxOf_append]
    simp [itemNotBefore]
  have targetNotBeforeItem : target ∉ before ++ [item] := by
    simp [targetNotBefore, Ne.symm itemNeTarget]
  have ordered' :
      list = (before ++ [item]) ++ (between ++ (target :: after)) := by
    simpa [List.append_assoc] using ordered
  have targetAfterItem : before.length + 1 ≤ List.idxOf target list := by
    rw [ordered', List.idxOf_append]
    simp [targetNotBeforeItem]
  apply Classical.byContradiction
  intro itemNotInFront
  have itemIndexAfterFront :
      List.idxOf item list =
        List.idxOf item (target :: suffix) + front.length := by
    rw [split, List.idxOf_append]
    simp [itemNotInFront]
  omega

/-- One correct holder retains a finite parent-history list. -/
structure RetainedValidatorBlockHistory
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (holder : Nat) (blocks : List (ValidatorBlock BlockId))
    (time : Time) : Prop where
  holderInRange : holder < config.authorityCount
  holderCorrectAvailable : faults.correctAvailable holder = true
  retained : ∀ block, block ∈ blocks →
    ((execution.trace time).validatorState holder).retained block.reference = true
  accepted : ∀ block, block ∈ blocks →
    ((execution.trace time).validatorState holder).accepted block.reference = true
  catalog : ∀ block, block ∈ blocks →
    (execution.trace time).blockCatalog block.reference.id = some block
  authorInRange : ∀ block, block ∈ blocks →
    block.reference.author < config.authorityCount
  validParents : ∀ block, block ∈ blocks →
    block.reference.round = 0 ∨ block.HasQuorumImmediateParents config

namespace RetainedValidatorBlockHistory

/-- Select one exact block source from a retained finite history. -/
theorem item
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {holder : Nat} {blocks : List (ValidatorBlock BlockId)} {time : Time}
    (source : RetainedValidatorBlockHistory execution holder blocks time)
    {block : ValidatorBlock BlockId}
    (member : block ∈ blocks) :
    RetainedValidatorBlock execution holder block time :=
  { holderInRange := source.holderInRange
    holderCorrectAvailable := source.holderCorrectAvailable
    retained := source.retained block member
    accepted := source.accepted block member
    catalog := source.catalog block member
    authorInRange := source.authorInRange block member
    validParents := source.validParents block member }

end RetainedValidatorBlockHistory

/-- Every item in one retained finite history remains a source at a later active
time. -/
theorem retained_validator_block_history_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder : Nat} {blocks : List (ValidatorBlock BlockId)}
    {earlier later : Time}
    (source : RetainedValidatorBlockHistory timed.execution holder blocks earlier)
    (ordered : earlier ≤ later)
    (protection : ∀ block, block ∈ blocks → ∀ time,
      earlier ≤ time → time ≤ later →
      rules.sourceProtected holder block.reference time) :
    RetainedValidatorBlockHistory timed.execution holder blocks later := by
  refine
    { holderInRange := source.holderInRange
      holderCorrectAvailable := source.holderCorrectAvailable
      retained := ?_
      accepted := ?_
      catalog := ?_
      authorInRange := source.authorInRange
      validParents := source.validParents }
  · intro block member
    exact (retained_validator_block_persists rules (source.item member) ordered
      (protection block member)).retained
  · intro block member
    exact (retained_validator_block_persists rules (source.item member) ordered
      (protection block member)).accepted
  · intro block member
    exact (retained_validator_block_persists rules (source.item member) ordered
      (protection block member)).catalog

/-- Every block in a finite retained parent-first history eventually becomes
ready at one receiver.

Each block is either accepted or is at or below the receiver's current GC
round. The proof does not preserve a request job across a commit. Each fetch
response uses the GC round that is current when the response is processed. -/
theorem retained_parent_first_history_eventually_ready
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder requester : Nat} {blocks : List (ValidatorBlock BlockId)}
    {start : Time}
    (source :
      RetainedValidatorBlockHistory timed.execution holder blocks start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (protectedWhileIncomplete : ∀ time,
      start ≤ time →
      (¬∀ item, item ∈ blocks →
        ((timed.execution.trace time).validatorState requester).accepted
          item.reference = true) →
      ∀ item, item ∈ blocks →
        rules.sourceProtected holder item.reference time)
    (parentFirst : ParentFirstValidatorBlockHistory
      (ValidatorReferenceAcceptedOrGcRootAt timed.execution start requester)
      blocks) :
    ∃ finish,
      start ≤ finish ∧
      ∀ block, block ∈ blocks →
        ValidatorReferenceAcceptedOrGcRootAt timed.execution finish requester
          block.reference := by
  induction blocks generalizing start with
  | nil =>
      exact ⟨start, Nat.le_refl start, by simp⟩
  | cons block remaining ih =>
      have blockMember : block ∈ block :: remaining := by simp
      have blockSource := source.item blockMember
      rcases retained_validator_block_eventually_accepted_or_obsolete rules
          blockSource requesterInRange requesterCorrectAvailable afterGst active
          (by
            intro time timeAfterStart blockNotAccepted _notObsolete
            exact protectedWhileIncomplete time timeAfterStart (by
              intro complete
              have accepted := complete block blockMember
              rw [blockNotAccepted] at accepted
              simp at accepted) block blockMember)
          (by simpa [ValidatorReferenceAcceptedOrGcRootAt] using parentFirst.1) with
        ⟨blockFinish, startBeforeBlockFinish, blockAcceptedOrObsolete⟩
      have blockReady : ValidatorReferenceAcceptedOrGcRootAt timed.execution
          blockFinish requester block.reference := by
        rcases blockAcceptedOrObsolete with accepted | obsolete
        · exact Or.inl accepted
        · exact Or.inr (rules.goalObsoleteIsAtOrBelowGc requester
            block.reference blockFinish requesterInRange
              requesterCorrectAvailable obsolete)
      by_cases remainingAlreadyReady : ∀ item, item ∈ remaining →
          ValidatorReferenceAcceptedOrGcRootAt timed.execution blockFinish
            requester item.reference
      · refine ⟨blockFinish, startBeforeBlockFinish, ?_⟩
        intro item member
        simp only [List.mem_cons] at member
        rcases member with sameBlock | inRemaining
        · subst item
          exact blockReady
        · exact remainingAlreadyReady item inRemaining
      · have fullSourceAtBlockFinish :=
          retained_validator_block_history_persists rules source
            startBeforeBlockFinish (by
              intro item member time timeAfterStart timeBeforeBlockFinish
              exact protectedWhileIncomplete time timeAfterStart (by
                intro allAcceptedAtTime
                apply remainingAlreadyReady
                intro remainingItem remainingMember
                exact Or.inl
                  (timed.execution.accepted_block_persists requesterInRange
                    timeBeforeBlockFinish
                    (allAcceptedAtTime remainingItem
                      (by simp [remainingMember]))))
                item member)
        have remainingSource : RetainedValidatorBlockHistory timed.execution
            holder remaining blockFinish := by
          refine
            { holderInRange := fullSourceAtBlockFinish.holderInRange
              holderCorrectAvailable :=
                fullSourceAtBlockFinish.holderCorrectAvailable
              retained := ?_
              accepted := ?_
              catalog := ?_
              authorInRange := ?_
              validParents := ?_ }
          · intro item member
            exact fullSourceAtBlockFinish.retained item (by simp [member])
          · intro item member
            exact fullSourceAtBlockFinish.accepted item (by simp [member])
          · intro item member
            exact fullSourceAtBlockFinish.catalog item (by simp [member])
          · intro item member
            exact fullSourceAtBlockFinish.authorInRange item (by simp [member])
          · intro item member
            exact fullSourceAtBlockFinish.validParents item (by simp [member])
        have readyAtBlockFinish : ParentFirstValidatorBlockHistory
            (ValidatorReferenceAcceptedOrGcRootAt timed.execution blockFinish
              requester) remaining := by
          exact parent_first_validator_block_history_mono (by
            intro reference ready
            rcases ready with readyAtStart | sameBlock
            · exact validator_reference_accepted_or_gc_root_persists
                timed.execution requesterInRange startBeforeBlockFinish
                  readyAtStart
            · rw [sameBlock]
              exact blockReady) parentFirst.2
        rcases ih remainingSource
            (Nat.le_trans afterGst startBeforeBlockFinish) (by
              intro time blockFinishBeforeTime
              exact active time
                (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime))
            (by
              intro time blockFinishBeforeTime remainingIncomplete item member
              exact protectedWhileIncomplete time
                (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime) (by
                  intro allAccepted
                  apply remainingIncomplete
                  intro remainingItem remainingMember
                  exact allAccepted remainingItem (by simp [remainingMember]))
                item (by simp [member]))
            readyAtBlockFinish with
          ⟨finish, blockFinishBeforeFinish, remainingReady⟩
        refine ⟨finish, Nat.le_trans startBeforeBlockFinish
          blockFinishBeforeFinish, ?_⟩
        intro item member
        simp only [List.mem_cons] at member
        rcases member with sameBlock | inRemaining
        · subst item
          exact validator_reference_accepted_or_gc_root_persists
            timed.execution requesterInRange blockFinishBeforeFinish blockReady
        · exact remainingReady item inRemaining

/-- Every block in one finite retained parent-first history is eventually
accepted by one correct, available requester. -/
theorem retained_parent_first_history_eventually_accepted
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder requester : Nat} {blocks : List (ValidatorBlock BlockId)}
    {start : Time}
    (source :
      RetainedValidatorBlockHistory timed.execution holder blocks start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (protectedWhileIncomplete : ∀ time,
      start ≤ time →
      (¬∀ item, item ∈ blocks →
        ((timed.execution.trace time).validatorState requester).accepted
          item.reference = true) →
      ∀ item, item ∈ blocks →
        rules.sourceProtected holder item.reference time)
    (requiredUntilAccepted : ∀ item,
      item ∈ blocks →
      ∀ time, start ≤ time →
        ((timed.execution.trace time).validatorState requester).accepted
            item.reference = false →
        ¬rules.goalObsolete requester item.reference time)
    (parentFirst : ParentFirstValidatorBlockHistory
      (fun reference =>
        ((timed.execution.trace start).validatorState requester).accepted
          reference = true)
      blocks) :
    ∃ finish,
      start ≤ finish ∧
      ∀ block, block ∈ blocks →
        ((timed.execution.trace finish).validatorState requester).accepted
          block.reference = true := by
  induction blocks generalizing start with
  | nil =>
      exact ⟨start, Nat.le_refl start, by simp⟩
  | cons block remaining ih =>
      have blockMember : block ∈ block :: remaining := by simp
      have blockSource := source.item blockMember
      rcases retained_validator_block_eventually_accepted rules blockSource
          requesterInRange requesterCorrectAvailable afterGst active
          (by
            intro time timeAfterStart blockNotAccepted _
            exact protectedWhileIncomplete time timeAfterStart (by
              intro complete
              have accepted := complete block blockMember
              rw [blockNotAccepted] at accepted
              simp at accepted) block blockMember)
          (requiredUntilAccepted block blockMember)
          parentFirst.1 with
        ⟨blockFinish, startBeforeBlockFinish, blockAccepted⟩
      by_cases remainingAlreadyAccepted : ∀ item, item ∈ remaining →
          ((timed.execution.trace blockFinish).validatorState requester).accepted
            item.reference = true
      · refine ⟨blockFinish, startBeforeBlockFinish, ?_⟩
        intro item member
        simp only [List.mem_cons] at member
        rcases member with sameBlock | inRemaining
        · subst item
          exact blockAccepted
        · exact remainingAlreadyAccepted item inRemaining
      have fullSourceAtBlockFinish :=
        retained_validator_block_history_persists rules source
          startBeforeBlockFinish (by
            intro item member time timeAfterStart timeBeforeBlockFinish
            exact protectedWhileIncomplete time timeAfterStart (by
              intro allAcceptedAtTime
              apply remainingAlreadyAccepted
              intro remainingItem remainingMember
              exact timed.execution.accepted_block_persists requesterInRange
                timeBeforeBlockFinish
                (allAcceptedAtTime remainingItem (by simp [remainingMember])))
              item member)
      have remainingSource : RetainedValidatorBlockHistory timed.execution holder
          remaining blockFinish := by
        refine
          { holderInRange := fullSourceAtBlockFinish.holderInRange
            holderCorrectAvailable :=
              fullSourceAtBlockFinish.holderCorrectAvailable
            retained := ?_
            accepted := ?_
            catalog := ?_
            authorInRange := ?_
            validParents := ?_ }
        · intro item member
          exact fullSourceAtBlockFinish.retained item (by simp [member])
        · intro item member
          exact fullSourceAtBlockFinish.accepted item (by simp [member])
        · intro item member
          exact fullSourceAtBlockFinish.catalog item (by simp [member])
        · intro item member
          exact fullSourceAtBlockFinish.authorInRange item (by simp [member])
        · intro item member
          exact fullSourceAtBlockFinish.validParents item (by simp [member])
      have readyAtBlockFinish : ParentFirstValidatorBlockHistory
          (fun reference =>
            ((timed.execution.trace blockFinish).validatorState requester).accepted
              reference = true)
          remaining := by
        exact parent_first_validator_block_history_mono (by
          intro reference ready
          rcases ready with acceptedAtStart | sameBlock
          · exact timed.execution.accepted_block_persists requesterInRange
              startBeforeBlockFinish acceptedAtStart
          · rw [sameBlock]
            exact blockAccepted) parentFirst.2
      rcases ih remainingSource
          (Nat.le_trans afterGst startBeforeBlockFinish) (by
            intro time blockFinishBeforeTime
            exact active time
              (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime))
          (by
            intro time blockFinishBeforeTime remainingIncomplete item member
            exact protectedWhileIncomplete time
              (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime) (by
                intro allAccepted
                apply remainingIncomplete
                intro remainingItem remainingMember
                exact allAccepted remainingItem (by simp [remainingMember]))
              item (by simp [member]))
          (by
            intro item member time blockFinishBeforeTime notAccepted
            exact requiredUntilAccepted item (by simp [member]) time
              (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime)
              notAccepted)
          readyAtBlockFinish with
        ⟨finish, blockFinishBeforeFinish, remainingAccepted⟩
      refine ⟨finish, Nat.le_trans startBeforeBlockFinish
        blockFinishBeforeFinish, ?_⟩
      intro item member
      simp only [List.mem_cons] at member
      rcases member with sameBlock | inRemaining
      · subst item
        exact timed.execution.accepted_block_persists requesterInRange
          blockFinishBeforeFinish blockAccepted
      · exact remainingAccepted item inRemaining

/-- Every block in one retained parent-first history is accepted within one
fixed block-sync cost per history item.

The requester can already have some items. The bound still charges one full
cost for each list position. A goal cannot become obsolete while its exact
history item is still missing. -/
theorem retained_parent_first_history_accepted_within_length_bound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder requester : Nat} {blocks : List (ValidatorBlock BlockId)}
    {start : Time}
    (source :
      RetainedValidatorBlockHistory timed.execution holder blocks start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (protectedWhileIncomplete : ∀ time,
      start ≤ time →
      (¬∀ item, item ∈ blocks →
        ((timed.execution.trace time).validatorState requester).accepted
          item.reference = true) →
      ∀ item, item ∈ blocks →
        rules.sourceProtected holder item.reference time)
    (requiredUntilAccepted : ∀ item,
      item ∈ blocks →
      ∀ time, start ≤ time →
        ((timed.execution.trace time).validatorState requester).accepted
            item.reference = false →
        ¬rules.goalObsolete requester item.reference time)
    (parentFirst : ParentFirstValidatorBlockHistory
      (fun reference =>
        ((timed.execution.trace start).validatorState requester).accepted
          reference = true)
      blocks) :
    ∃ finish,
      start ≤ finish ∧
      finish ≤
        start + blocks.length * validatorBlockSyncAcceptanceBound timed rules ∧
      ∀ block, block ∈ blocks →
        ((timed.execution.trace finish).validatorState requester).accepted
          block.reference = true := by
  induction blocks generalizing start with
  | nil =>
      exact ⟨start, Nat.le_refl start, by simp, by simp⟩
  | cons block remaining ih =>
      have blockMember : block ∈ block :: remaining := by simp
      have blockSource := source.item blockMember
      rcases retained_validator_block_accepted_or_obsolete_within_bound rules
          blockSource requesterInRange requesterCorrectAvailable afterGst active
          (by
            intro time timeAfterStart blockNotAccepted notObsolete
            exact protectedWhileIncomplete time timeAfterStart (by
              intro complete
              have accepted := complete block blockMember
              rw [blockNotAccepted] at accepted
              simp at accepted) block blockMember)
          (fun parent member => Or.inl (parentFirst.1 parent member)) with
        ⟨blockFinish, startBeforeBlockFinish, blockFinishBound,
          blockAcceptedOrObsolete⟩
      have blockAccepted :
          ((timed.execution.trace blockFinish).validatorState requester).accepted
            block.reference = true := by
        rcases blockAcceptedOrObsolete with accepted | obsolete
        · exact accepted
        · cases acceptedAtFinish :
              ((timed.execution.trace blockFinish).validatorState
                requester).accepted block.reference with
          | true => simp
          | false =>
              exact False.elim
                ((requiredUntilAccepted block blockMember blockFinish
                  startBeforeBlockFinish acceptedAtFinish) obsolete)
      by_cases remainingAlreadyAccepted : ∀ item, item ∈ remaining →
          ((timed.execution.trace blockFinish).validatorState requester).accepted
            item.reference = true
      · refine ⟨blockFinish, startBeforeBlockFinish, ?_, ?_⟩
        · calc
            blockFinish ≤
                start + validatorBlockSyncAcceptanceBound timed rules :=
              blockFinishBound
            _ ≤ start + (block :: remaining).length *
                validatorBlockSyncAcceptanceBound timed rules := by
              simp [List.length_cons, Nat.succ_mul]
        · intro item member
          simp only [List.mem_cons] at member
          rcases member with sameBlock | inRemaining
          · subst item
            exact blockAccepted
          · exact remainingAlreadyAccepted item inRemaining
      · have fullSourceAtBlockFinish :=
          retained_validator_block_history_persists rules source
            startBeforeBlockFinish (by
              intro item member time timeAfterStart timeBeforeBlockFinish
              exact protectedWhileIncomplete time timeAfterStart (by
                intro allAcceptedAtTime
                apply remainingAlreadyAccepted
                intro remainingItem remainingMember
                exact timed.execution.accepted_block_persists requesterInRange
                  timeBeforeBlockFinish
                  (allAcceptedAtTime remainingItem (by simp [remainingMember])))
                item member)
        have remainingSource : RetainedValidatorBlockHistory timed.execution
            holder remaining blockFinish := by
          refine
            { holderInRange := fullSourceAtBlockFinish.holderInRange
              holderCorrectAvailable :=
                fullSourceAtBlockFinish.holderCorrectAvailable
              retained := ?_
              accepted := ?_
              catalog := ?_
              authorInRange := ?_
              validParents := ?_ }
          · intro item member
            exact fullSourceAtBlockFinish.retained item (by simp [member])
          · intro item member
            exact fullSourceAtBlockFinish.accepted item (by simp [member])
          · intro item member
            exact fullSourceAtBlockFinish.catalog item (by simp [member])
          · intro item member
            exact fullSourceAtBlockFinish.authorInRange item (by simp [member])
          · intro item member
            exact fullSourceAtBlockFinish.validParents item (by simp [member])
        have readyAtBlockFinish : ParentFirstValidatorBlockHistory
            (fun reference =>
              ((timed.execution.trace blockFinish).validatorState
                requester).accepted reference = true)
            remaining := by
          exact parent_first_validator_block_history_mono (by
            intro reference ready
            rcases ready with acceptedAtStart | sameBlock
            · exact timed.execution.accepted_block_persists requesterInRange
                startBeforeBlockFinish acceptedAtStart
            · rw [sameBlock]
              exact blockAccepted) parentFirst.2
        rcases ih remainingSource
            (Nat.le_trans afterGst startBeforeBlockFinish) (by
              intro time blockFinishBeforeTime
              exact active time
                (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime))
            (by
              intro time blockFinishBeforeTime remainingIncomplete item member
              exact protectedWhileIncomplete time
                (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime) (by
                  intro allAccepted
                  apply remainingIncomplete
                  intro remainingItem remainingMember
                  exact allAccepted remainingItem (by simp [remainingMember]))
                item (by simp [member]))
            (by
              intro item member time blockFinishBeforeTime notAccepted
              exact requiredUntilAccepted item (by simp [member]) time
                (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime)
                notAccepted)
            readyAtBlockFinish with
          ⟨finish, blockFinishBeforeFinish, finishBound, remainingAccepted⟩
        refine ⟨finish, Nat.le_trans startBeforeBlockFinish
          blockFinishBeforeFinish, ?_, ?_⟩
        · calc
            finish ≤ blockFinish + remaining.length *
                validatorBlockSyncAcceptanceBound timed rules := finishBound
            _ ≤
                (start + validatorBlockSyncAcceptanceBound timed rules) +
                  remaining.length *
                    validatorBlockSyncAcceptanceBound timed rules :=
              Nat.add_le_add_right blockFinishBound _
            _ = start + (block :: remaining).length *
                validatorBlockSyncAcceptanceBound timed rules := by
              simp only [List.length_cons, Nat.succ_mul]
              ac_rfl
        · intro item member
          simp only [List.mem_cons] at member
          rcases member with sameBlock | inRemaining
          · subst item
            exact timed.execution.accepted_block_persists requesterInRange
              blockFinishBeforeFinish blockAccepted
          · exact remainingAccepted item inRemaining

/-- Every correct, available requester eventually accepts every block in the
same retained finite parent-first history. Each requester has its own finish
time. -/
theorem retained_parent_first_history_accepted_by_each_correct_requester
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder : Nat} {blocks : List (ValidatorBlock BlockId)} {start : Time}
    (source :
      RetainedValidatorBlockHistory timed.execution holder blocks start)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (protectedWhileIncomplete : ∀ requester,
      requester < config.authorityCount →
      faults.correctAvailable requester = true →
      ∀ time, start ≤ time →
        (¬∀ item, item ∈ blocks →
          ((timed.execution.trace time).validatorState requester).accepted
            item.reference = true) →
        ∀ item, item ∈ blocks →
          rules.sourceProtected holder item.reference time)
    (requiredUntilAccepted : ∀ requester,
      requester < config.authorityCount →
      faults.correctAvailable requester = true →
      ∀ item, item ∈ blocks →
        ∀ time, start ≤ time →
          ((timed.execution.trace time).validatorState requester).accepted
              item.reference = false →
          ¬rules.goalObsolete requester item.reference time)
    (parentFirst : ∀ requester,
      requester < config.authorityCount →
      faults.correctAvailable requester = true →
      ParentFirstValidatorBlockHistory
        (fun reference =>
          ((timed.execution.trace start).validatorState requester).accepted
            reference = true)
        blocks) :
    ∀ requester,
      requester < config.authorityCount →
      faults.correctAvailable requester = true →
      ∃ finish,
        start ≤ finish ∧
        ∀ block, block ∈ blocks →
          ((timed.execution.trace finish).validatorState requester).accepted
            block.reference = true := by
  intro requester requesterInRange requesterCorrectAvailable
  exact retained_parent_first_history_eventually_accepted rules source
    requesterInRange requesterCorrectAvailable afterGst active
    (protectedWhileIncomplete requester requesterInRange
      requesterCorrectAvailable)
    (requiredUntilAccepted requester requesterInRange
      requesterCorrectAvailable)
    (parentFirst requester requesterInRange requesterCorrectAvailable)

namespace CausalRecoveryCapsule

variable {BlockId CommitId : Type}
variable {config : ValidatorEpochConfig CommitId}

/-- The topological capsule contract gives the parent-first order used by the
main-execution block synchronization theorem. Only the capsule's exact
configured genesis roots must already be accepted by the requester. -/
theorem parent_first_validator_history_from_capsule_genesis
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {initiallyAccepted : ValidatorBlockRef BlockId → Prop}
    (genesisAccepted : ∀ reference,
      reference ∈ capsule.genesisParents → initiallyAccepted reference) :
    ParentFirstValidatorBlockHistory initiallyAccepted capsule.history := by
  classical
  have advance : ∀ (remaining front : List (ValidatorBlock BlockId)),
      capsule.history = front ++ remaining →
      ParentFirstValidatorBlockHistory
        (fun reference =>
          initiallyAccepted reference ∨
            ∃ earlierBlock, earlierBlock ∈ front ∧
              earlierBlock.reference = reference)
        remaining := by
    intro remaining
    induction remaining with
    | nil =>
        intro front historySplit
        trivial
    | cons block tail inductionHypothesis =>
        intro front historySplit
        have blockMember : block ∈ capsule.history := by
          rw [historySplit]
          simp
        constructor
        · intro parent parentMember
          rcases capsule.historyClosed block blockMember parent parentMember with
            genesis | ⟨parentBlock, parentBlockMember, parentReference⟩
          · exact Or.inl (genesisAccepted parent genesis)
          · rcases capsule.historyTopological block parentBlock blockMember
                parentBlockMember (by
                  simpa [parentReference] using parentMember) with
              ⟨before, between, after, topological⟩
            have referenceSplit :
                capsule.history.map ValidatorBlock.reference =
                  front.map ValidatorBlock.reference ++
                    (block.reference :: tail.map ValidatorBlock.reference) := by
              simpa using
                congrArg (List.map ValidatorBlock.reference) historySplit
            have referenceOrder :
                capsule.history.map ValidatorBlock.reference =
                  before.map ValidatorBlock.reference ++
                    (parentBlock.reference ::
                      between.map ValidatorBlock.reference ++
                        (block.reference ::
                          after.map ValidatorBlock.reference)) := by
              simpa [List.append_assoc] using
                congrArg (List.map ValidatorBlock.reference) topological
            have parentReferenceInFront :
                parentBlock.reference ∈
                  front.map ValidatorBlock.reference :=
              list_before_member_of_front capsule.historyReferencesNodup
                referenceSplit referenceOrder
            rcases List.mem_map.mp parentReferenceInFront with
              ⟨earlierBlock, earlierBlockMember, sameReference⟩
            exact Or.inr ⟨earlierBlock, earlierBlockMember,
              sameReference.trans parentReference⟩
        · have nextSplit :
              capsule.history = (front ++ [block]) ++ tail := by
            simpa [List.append_assoc] using historySplit
          have tailOrdered :=
            inductionHypothesis (front ++ [block]) nextSplit
          exact parent_first_validator_block_history_mono (by
            intro reference accepted
            rcases accepted with acceptedInitially |
                ⟨earlierBlock, earlierMember, earlierReference⟩
            · exact Or.inl (Or.inl acceptedInitially)
            · rcases List.mem_append.mp earlierMember with
                memberOfFront | memberOfBlock
              · exact Or.inl (Or.inr
                  ⟨earlierBlock, memberOfFront, earlierReference⟩)
              · have sameBlock : earlierBlock = block := by
                  simpa using memberOfBlock
                subst earlierBlock
                exact Or.inr earlierReference.symm) tailOrdered
  have ordered := advance capsule.history [] (by simp)
  simpa using ordered

/-- Backward-compatible form for a caller that has every round-zero reference.
New recovery composition should use
`parent_first_validator_history_from_capsule_genesis`. -/
theorem parent_first_validator_history
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {initiallyAccepted : ValidatorBlockRef BlockId → Prop}
    (genesisAccepted : ∀ reference,
      reference.round = 0 → initiallyAccepted reference) :
    ParentFirstValidatorBlockHistory initiallyAccepted capsule.history := by
  apply capsule.parent_first_validator_history_from_capsule_genesis
  intro reference member
  exact genesisAccepted reference
    (capsule.genesisParentsAreRoundZero reference member)

end CausalRecoveryCapsule

/-- One causal recovery capsule stored in the main validator execution.

The holder pin is local. It applies only to this capsule history. A recovery
task can release it after the history is accepted or the recovery goal becomes
obsolete. -/
structure CausalRecoveryCapsuleExecutionSource
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (holder : Nat) (time : Time) : Prop where
  holderInRange : holder < config.authorityCount
  holderCorrectAvailable : faults.correctAvailable holder = true
  retained : ∀ block, block ∈ capsule.history →
    ((timed.execution.trace time).validatorState holder).retained
      block.reference = true
  accepted : ∀ block, block ∈ capsule.history →
    ((timed.execution.trace time).validatorState holder).accepted
      block.reference = true
  catalog : ∀ block, block ∈ capsule.history →
    (timed.execution.trace time).blockCatalog block.reference.id = some block
  authorInRange : ∀ block, block ∈ capsule.history →
    block.reference.author < config.authorityCount
  protectedAtStart : ∀ block, block ∈ capsule.history →
    rules.sourceProtected holder block.reference time

/-- Convert the causal capsule history to the main-execution retained-history
source used by the synchronization theorem. -/
theorem causal_recovery_capsule_to_retained_validator_history
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {rules : ValidatorBlockSyncExecutionRules timed}
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {holder : Nat} {time : Time}
    (source : CausalRecoveryCapsuleExecutionSource rules capsule holder time) :
    RetainedValidatorBlockHistory timed.execution holder capsule.history time := by
  refine
    { holderInRange := source.holderInRange
      holderCorrectAvailable := source.holderCorrectAvailable
      retained := source.retained
      accepted := source.accepted
      catalog := source.catalog
      authorInRange := source.authorInRange
      validParents := ?_ }
  intro block member
  by_cases genesis : block.reference.round = 0
  · exact Or.inl genesis
  · exact Or.inr (capsule.positiveHistoryBlocksValid block member (by omega))

/-- Convert one protected causal capsule to both main-execution inputs using
only its exact configured genesis roots. -/
theorem causal_recovery_capsule_to_parent_first_retained_history_from_capsule_genesis
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {rules : ValidatorBlockSyncExecutionRules timed}
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {holder requester : Nat} {time : Time}
    (source : CausalRecoveryCapsuleExecutionSource rules capsule holder time)
    (genesisAccepted : ∀ reference,
      reference ∈ capsule.genesisParents →
      ((timed.execution.trace time).validatorState requester).accepted reference =
        true) :
    RetainedValidatorBlockHistory timed.execution holder capsule.history time ∧
      ParentFirstValidatorBlockHistory
        (fun reference =>
          ((timed.execution.trace time).validatorState requester).accepted
            reference = true)
        capsule.history := by
  exact ⟨causal_recovery_capsule_to_retained_validator_history source,
    capsule.parent_first_validator_history_from_capsule_genesis
      genesisAccepted⟩

/-- Backward-compatible conversion for callers that already accept every
round-zero reference. -/
theorem causal_recovery_capsule_to_parent_first_retained_history
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {rules : ValidatorBlockSyncExecutionRules timed}
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {holder requester : Nat} {time : Time}
    (source : CausalRecoveryCapsuleExecutionSource rules capsule holder time)
    (genesisAccepted : ∀ reference,
      reference.round = 0 →
      ((timed.execution.trace time).validatorState requester).accepted reference =
        true) :
    RetainedValidatorBlockHistory timed.execution holder capsule.history time ∧
      ParentFirstValidatorBlockHistory
        (fun reference =>
          ((timed.execution.trace time).validatorState requester).accepted
            reference = true)
        capsule.history := by
  apply causal_recovery_capsule_to_parent_first_retained_history_from_capsule_genesis
    source
  intro reference member
  exact genesisAccepted reference
    (capsule.genesisParentsAreRoundZero reference member)

end Mysticeti
