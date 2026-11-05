/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorProcess

namespace Mysticeti

/-! Validator-local block synchronization.

This module models one protected synchronization goal. A correct validator keeps
the goal, walks the finite validator set with a request cursor, and accepts a
returned parent-history item only after the earlier items in that history are
accepted.

The final theorems do not assume that a useful peer exists or that synchronization
finishes. The source is the correct owner that retained the item. The finite retry
schedule derives an attempt to that owner from the validator identities. Addressed
partial synchrony delivers the request and response. Fair protected local actions
then serve and accept the item.
-/

/-- Messages for one retained parent-history item. -/
inductive BlockSyncMessage (Item : Type) where
  | request (item : Item)
  | data (item : Item)

/-- Local state for protected block synchronization. -/
structure BlockSyncLocalState (Item : Type) where
  retained : Item → Bool
  needed : Item → Bool
  accepted : Item → Bool
  /-- Position in the current finite validator retry schedule. -/
  requestCursor : Nat

/-- A validator-indexed block-sync execution. -/
abbrev BlockSyncTrace (Item : Type) := Trace (Nat → BlockSyncLocalState Item)

def RetainedAt {Item : Type} (trace : BlockSyncTrace Item)
    (validator : Nat) (item : Item) (time : Time) : Prop :=
  (trace time validator).retained item = true

def NeededAt {Item : Type} (trace : BlockSyncTrace Item)
    (validator : Nat) (item : Item) (time : Time) : Prop :=
  (trace time validator).needed item = true

def AcceptedAt {Item : Type} (trace : BlockSyncTrace Item)
    (validator : Nat) (item : Item) (time : Time) : Prop :=
  (trace time validator).accepted item = true

/-- A local retention transition stores one item and does not remove an item that
was already retained. -/
structure RetentionTransition {Item : Type} (item : Item)
    (before after : BlockSyncLocalState Item) : Prop where
  retainedAfter : after.retained item = true
  preservesRetained : ∀ other,
    before.retained other = true → after.retained other = true

/-- Retry all validator identities in their canonical finite order. The sender's
own identity can occur in this list. A local implementation can skip it. The proof
uses an owner different from the requester unless the requester already owns and
accepts the item. -/
def validatorRetrySchedule (authorityCount : Nat) : List Nat :=
  List.range authorityCount

/-- Every validator identity occurs in the finite retry schedule. -/
theorem validator_retry_schedule_contains
    {authorityCount validator : Nat}
    (validatorInRange : validator < authorityCount) :
    validator ∈ validatorRetrySchedule authorityCount := by
  simp [validatorRetrySchedule, validatorInRange]

/-- One local request action advances the cursor by one position and sends an
addressed request to the validator at the old cursor. -/
structure RequestCursorTransition
    {Item CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop)
    (trace : BlockSyncTrace Item)
    (requester : Nat) (item : Item) (attempt enabledAt completedAt : Nat)
    (packet : AddressedPacket (BlockSyncMessage Item)) : Prop where
  startsBeforeCompletion : enabledAt ≤ completedAt
  neededWhenEnabled : NeededAt trace requester item enabledAt
  cursorBefore : (trace enabledAt requester).requestCursor = attempt
  cursorAfter : (trace completedAt requester).requestCursor = attempt + 1
  attemptInSchedule : attempt ∈ validatorRetrySchedule config.authorityCount
  selectedPeer : packet.receiver = attempt
  sender : packet.sender = requester
  payload : packet.payload = .request item
  sentAt : packet.sentAt = completedAt
  protocol : protocolPacket packet

/-- The network delivery transition for one addressed packet. -/
structure DeliveryTransition
    {Item CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop}
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (packet : AddressedPacket (BlockSyncMessage Item)) : Prop where
  sentBeforeDelivery : packet.sentAt ≤ packet.deliveredAt
  withinDelay : packet.deliveredAt ≤ packet.sentAt + network.delta

/-- Addressed partial synchrony supplies the delivery transition. -/
theorem addressed_packet_delivery
    {Item CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop}
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (packet : AddressedPacket (BlockSyncMessage Item))
    (valid : protocolPacket packet)
    (senderInRange : packet.sender < config.authorityCount)
    (receiverInRange : packet.receiver < config.authorityCount)
    (senderCorrect : faults.correctAvailable packet.sender = true)
    (receiverCorrect : faults.correctAvailable packet.receiver = true)
    (afterGst : network.gst ≤ packet.sentAt) :
    DeliveryTransition network packet := by
  rcases network.postGstDelivery packet valid senderInRange receiverInRange
      senderCorrect receiverCorrect afterGst with ⟨sentBefore, within⟩
  exact ⟨sentBefore, within⟩

/-- One local service action responds to a delivered request with the retained
item. -/
structure ServeTransition
    {Item : Type}
    (trace : BlockSyncTrace Item)
    (item : Item)
    (request response : AddressedPacket (BlockSyncMessage Item)) : Prop where
  requestPayload : request.payload = .request item
  retainedAtRequestDelivery :
    RetainedAt trace request.receiver item request.deliveredAt
  responseStartsAfterRequest : request.deliveredAt ≤ response.sentAt
  responseSender : response.sender = request.receiver
  responseReceiver : response.receiver = request.sender
  responsePayload : response.payload = .data item

/-- All earlier items in one parent-history order are accepted. -/
def EarlierHistoryAccepted
    {Item : Type}
    (trace : BlockSyncTrace Item)
    (validator : Nat)
    (earlierItems : List Item)
    (time : Time) : Prop :=
  ∀ earlierItem,
    earlierItem ∈ earlierItems →
    AcceptedAt trace validator earlierItem time

/-- One ordered local acceptance action accepts a delivered item only after every
earlier item in the declared parent-history order is accepted. -/
structure OrderedAcceptTransition
    {Item : Type}
    (trace : BlockSyncTrace Item)
    (requester : Nat)
    (history : List Item)
    (earlierItems laterItems : List Item)
    (item : Item)
    (response : AddressedPacket (BlockSyncMessage Item))
    (completedAt : Time) : Prop where
  responseReceiver : response.receiver = requester
  responsePayload : response.payload = .data item
  historyOrder : history = earlierItems ++ item :: laterItems
  earlierAccepted :
    EarlierHistoryAccepted trace requester earlierItems response.deliveredAt
  deliveryBeforeCompletion : response.deliveredAt ≤ completedAt
  acceptedAfter : AcceptedAt trace requester item completedAt

/-- Stable local storage rules. They are single-validator rules, not a statement
about network progress. -/
structure BlockSyncStorageRules
    {Item : Type}
    (trace : BlockSyncTrace Item) : Prop where
  retainedMonotone : ∀ validator item earlier later,
    earlier ≤ later →
    RetainedAt trace validator item earlier →
    RetainedAt trace validator item later
  acceptedMonotone : ∀ validator item earlier later,
    earlier ≤ later →
    AcceptedAt trace validator item earlier →
    AcceptedAt trace validator item later

/-- Fair protected request actions execute each position of the finite validator
schedule unless the item is already accepted. This is a local scheduler contract.
It does not state that a peer has the item or that a request succeeds. -/
structure FairProtectedRequestActions
    {Item CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop)
    (trace : BlockSyncTrace Item) : Prop where
  runSchedulePosition : ∀ requester item start attempt,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    NeededAt trace requester item start →
    attempt ∈ validatorRetrySchedule config.authorityCount →
    ∃ enabledAt completedAt packet,
      start ≤ enabledAt ∧
      (AcceptedAt trace requester item enabledAt ∨
        RequestCursorTransition config protocolPacket trace requester item
          attempt enabledAt completedAt packet)

/-- Fair protected service actions respond when the local stable store contains
the requested item. -/
structure FairProtectedServeActions
    {Item CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop)
    (trace : BlockSyncTrace Item) : Prop where
  serveRetained : ∀ item request,
    protocolPacket request →
    request.payload = .request item →
    request.sender < config.authorityCount →
    request.receiver < config.authorityCount →
    faults.correctAvailable request.sender = true →
    faults.correctAvailable request.receiver = true →
    RetainedAt trace request.receiver item request.deliveredAt →
    ∃ response,
      protocolPacket response ∧
      ServeTransition trace item request response

/-- Fair protected acceptance actions execute one enabled ordered acceptance
transition. -/
structure FairProtectedAcceptActions
    {Item CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop)
    (trace : BlockSyncTrace Item) : Prop where
  acceptDelivered : ∀ requester history earlierItems laterItems item response,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    protocolPacket response →
    response.receiver = requester →
    response.payload = .data item →
    history = earlierItems ++ item :: laterItems →
    EarlierHistoryAccepted trace requester earlierItems response.deliveredAt →
    ∃ completedAt,
      OrderedAcceptTransition trace requester history earlierItems laterItems item response
        completedAt

/-- The retained owner is explicit data provenance. It is not an existential
"useful peer" assumption. The owner accepted and retained the item before it made
the child history available. -/
structure RetainedCorrectOwnerItem
    {Item CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (trace : BlockSyncTrace Item)
    (owner : Nat)
    (item : Item)
    (start : Time) : Prop where
  ownerInRange : owner < config.authorityCount
  ownerCorrect : faults.correctAvailable owner = true
  retainedAtStart : RetainedAt trace owner item start
  acceptedAtStart : AcceptedAt trace owner item start

/-- A retained correct-owner parent-history item is eventually accepted by one
correct, available requester.

The proof selects the owner's position in `List.range authorityCount`. It then
uses two addressed deliveries and two protected local actions. It does not take a
useful peer, parent availability, or completed synchronization as an input. -/
theorem retained_owner_history_item_eventually_accepted
    {Item CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {trace : BlockSyncTrace Item}
    (storage : BlockSyncStorageRules trace)
    (requests : FairProtectedRequestActions config faults protocolPacket trace)
    (serving : FairProtectedServeActions config faults protocolPacket trace)
    (accepting : FairProtectedAcceptActions config faults protocolPacket trace)
    {owner requester : Nat}
    {item : Item}
    {history earlierItems laterItems : List Item}
    {start : Nat}
    (source : RetainedCorrectOwnerItem config faults trace owner item start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (requesterNeeds : requester ≠ owner → NeededAt trace requester item start)
    (historyOrder : history = earlierItems ++ item :: laterItems)
    (earlierAcceptedAtStart :
      EarlierHistoryAccepted trace requester earlierItems start) :
    ∃ finish,
      start ≤ finish ∧
      AcceptedAt trace requester item finish := by
  by_cases requesterIsOwner : requester = owner
  · subst requester
    exact ⟨start, Nat.le_refl start, source.acceptedAtStart⟩

  have needAtStart := requesterNeeds requesterIsOwner
  rcases requests.runSchedulePosition requester item start owner
      requesterInRange requesterCorrect needAtStart
      (validator_retry_schedule_contains source.ownerInRange) with
    ⟨requestEnabledAt, requestCompletedAt, request, startBeforeRequest,
      alreadyAccepted | requestStep⟩
  · exact ⟨requestEnabledAt, startBeforeRequest, alreadyAccepted⟩

  have requestReceiverIsOwner : request.receiver = owner := by
    exact requestStep.selectedPeer

  have requestSenderInRange : request.sender < config.authorityCount := by
    rw [requestStep.sender]
    exact requesterInRange
  have requestReceiverInRange : request.receiver < config.authorityCount := by
    rw [requestReceiverIsOwner]
    exact source.ownerInRange
  have requestSenderCorrect :
      faults.correctAvailable request.sender = true := by
    rw [requestStep.sender]
    exact requesterCorrect
  have requestReceiverCorrect :
      faults.correctAvailable request.receiver = true := by
    rw [requestReceiverIsOwner]
    exact source.ownerCorrect
  have startBeforeRequestSent : start ≤ request.sentAt := by
    rw [requestStep.sentAt]
    exact Nat.le_trans startBeforeRequest requestStep.startsBeforeCompletion
  have requestAfterGst : network.gst ≤ request.sentAt :=
    Nat.le_trans afterGst startBeforeRequestSent
  have requestDelivery := addressed_packet_delivery network request
    requestStep.protocol requestSenderInRange requestReceiverInRange
    requestSenderCorrect requestReceiverCorrect requestAfterGst
  have startBeforeRequestDelivery : start ≤ request.deliveredAt :=
    Nat.le_trans startBeforeRequestSent requestDelivery.sentBeforeDelivery
  have ownerRetainedAtRequest :
      RetainedAt trace request.receiver item request.deliveredAt := by
    rw [requestReceiverIsOwner]
    exact storage.retainedMonotone owner item start request.deliveredAt
      startBeforeRequestDelivery source.retainedAtStart

  rcases serving.serveRetained item request requestStep.protocol
      requestStep.payload requestSenderInRange requestReceiverInRange
      requestSenderCorrect requestReceiverCorrect ownerRetainedAtRequest with
    ⟨response, responseProtocol, serveStep⟩

  have responseSenderInRange : response.sender < config.authorityCount := by
    rw [serveStep.responseSender]
    exact requestReceiverInRange
  have responseReceiverInRange : response.receiver < config.authorityCount := by
    rw [serveStep.responseReceiver]
    exact requestSenderInRange
  have responseSenderCorrect :
      faults.correctAvailable response.sender = true := by
    rw [serveStep.responseSender]
    exact requestReceiverCorrect
  have responseReceiverCorrect :
      faults.correctAvailable response.receiver = true := by
    rw [serveStep.responseReceiver]
    exact requestSenderCorrect
  have startBeforeResponseSent : start ≤ response.sentAt :=
    Nat.le_trans startBeforeRequestDelivery serveStep.responseStartsAfterRequest
  have responseAfterGst : network.gst ≤ response.sentAt :=
    Nat.le_trans afterGst startBeforeResponseSent
  have responseDelivery := addressed_packet_delivery network response
    responseProtocol responseSenderInRange responseReceiverInRange
    responseSenderCorrect responseReceiverCorrect responseAfterGst
  have startBeforeResponseDelivery : start ≤ response.deliveredAt :=
    Nat.le_trans startBeforeResponseSent responseDelivery.sentBeforeDelivery

  have responseReceiverIsRequester : response.receiver = requester := by
    calc
      response.receiver = request.sender := serveStep.responseReceiver
      _ = requester := requestStep.sender
  have earlierAcceptedAtResponse :
      EarlierHistoryAccepted trace requester earlierItems
        response.deliveredAt := by
    intro earlierItem earlierItemInPrefix
    exact storage.acceptedMonotone requester earlierItem start
      response.deliveredAt startBeforeResponseDelivery
      (earlierAcceptedAtStart earlierItem earlierItemInPrefix)

  rcases accepting.acceptDelivered requester history earlierItems laterItems item response
      requesterInRange requesterCorrect responseProtocol
      responseReceiverIsRequester serveStep.responsePayload historyOrder
      earlierAcceptedAtResponse with
    ⟨finish, acceptStep⟩
  exact ⟨finish,
    Nat.le_trans startBeforeResponseDelivery
      acceptStep.deliveryBeforeCompletion,
    acceptStep.acceptedAfter⟩

/-- The same retained item is eventually accepted by every correct, available
requester. Each requester gets its own completion time. -/
theorem retained_owner_history_item_accepted_by_each_correct_requester
    {Item CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket : AddressedPacket (BlockSyncMessage Item) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {trace : BlockSyncTrace Item}
    (storage : BlockSyncStorageRules trace)
    (requests : FairProtectedRequestActions config faults protocolPacket trace)
    (serving : FairProtectedServeActions config faults protocolPacket trace)
    (accepting : FairProtectedAcceptActions config faults protocolPacket trace)
    {owner : Nat}
    {item : Item}
    {history earlierItems laterItems : List Item}
    {start : Nat}
    (source : RetainedCorrectOwnerItem config faults trace owner item start)
    (afterGst : network.gst ≤ start)
    (needs : ∀ requester,
      requester < config.authorityCount →
      faults.correctAvailable requester = true →
      requester ≠ owner →
      NeededAt trace requester item start)
    (historyOrder : history = earlierItems ++ item :: laterItems)
    (orderedPrefix : ∀ requester,
      requester < config.authorityCount →
      faults.correctAvailable requester = true →
      EarlierHistoryAccepted trace requester earlierItems start) :
    ∀ requester,
      requester < config.authorityCount →
      faults.correctAvailable requester = true →
      ∃ finish,
        start ≤ finish ∧
        AcceptedAt trace requester item finish := by
  intro requester requesterInRange requesterCorrect
  exact retained_owner_history_item_eventually_accepted storage requests serving
    accepting source requesterInRange requesterCorrect afterGst
    (needs requester requesterInRange requesterCorrect) historyOrder
    (orderedPrefix requester requesterInRange requesterCorrect)

end Mysticeti
