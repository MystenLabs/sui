/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega

namespace Mysticeti

/-! The network and round-change assumptions used by the liveness proof. -/

abbrev Time := Nat

/-- One authenticated protocol message between correct processes. -/
structure Packet where
  sentAt : Time
  deliveredAt : Time
  deriving DecidableEq, Repr

/-- Standard partial synchrony: GST is unknown, and post-GST delay is at most
`delta`. This is the environmental assumption `ASM-LIVE-PARTIAL-SYNCHRONY`. -/
structure PartialSynchrony (protocolPacket : Packet → Prop) where
  gst : Time
  delta : Nat
  deltaPositive : 0 < delta
  postGstDelivery : ∀ packet,
    protocolPacket packet →
    gst ≤ packet.sentAt →
    packet.sentAt ≤ packet.deliveredAt ∧
      packet.deliveredAt ≤ packet.sentAt + delta

namespace PartialSynchrony

theorem protocol_packet_is_delivered
    {protocolPacket : Packet → Prop}
    (network : PartialSynchrony protocolPacket)
    (packet : Packet) (valid : protocolPacket packet)
    (afterGst : network.gst ≤ packet.sentAt) :
    ∃ deliveryTime,
      packet.sentAt ≤ deliveryTime ∧
      deliveryTime ≤ packet.sentAt + network.delta := by
  exact ⟨packet.deliveredAt, network.postGstDelivery packet valid afterGst⟩

end PartialSynchrony

/-- One local consensus action at a correct validator. The action becomes enabled
when its required input is available. A covered action stays enabled until it
completes. Its result then becomes visible to later local consensus actions. -/
structure LocalConsensusAction where
  enabledAt : Time
  completedAt : Time
  deriving DecidableEq, Repr

/-- Bounded post-GST local processing for actions that stay enabled. `epsilon` can
be zero in an idealized model. The proof uses this bound with the network delay
bound. It does not require a positive lower bound on network delay.

This is the environmental assumption `ASM-LIVE-LOCAL-RESPONSE`. -/
structure BoundedLocalProcessing
    {protocolPacket : Packet → Prop}
    (network : PartialSynchrony protocolPacket)
    (protocolAction : LocalConsensusAction → Prop) where
  epsilon : Nat
  postGstCompletion : ∀ action,
    protocolAction action →
    network.gst ≤ action.enabledAt →
    action.enabledAt ≤ action.completedAt ∧
      action.completedAt ≤ action.enabledAt + epsilon

namespace BoundedLocalProcessing

theorem protocol_action_completes
    {protocolPacket : Packet → Prop}
    {network : PartialSynchrony protocolPacket}
    {protocolAction : LocalConsensusAction → Prop}
    (processing : BoundedLocalProcessing network protocolAction)
    (action : LocalConsensusAction) (valid : protocolAction action)
    (afterGst : network.gst ≤ action.enabledAt) :
    ∃ completionTime,
      action.enabledAt ≤ completionTime ∧
      completionTime ≤ action.enabledAt + processing.epsilon := by
  exact ⟨action.completedAt, processing.postGstCompletion action valid afterGst⟩

/-- A post-GST message becomes visible to later local consensus actions within the
combined delivery and processing bound. -/
theorem protocol_packet_becomes_locally_visible
    {protocolPacket : Packet → Prop}
    {network : PartialSynchrony protocolPacket}
    {protocolAction : LocalConsensusAction → Prop}
    (processing : BoundedLocalProcessing network protocolAction)
    (packet : Packet) (validPacket : protocolPacket packet)
    (action : LocalConsensusAction) (validAction : protocolAction action)
    (actionStartsAtDelivery : action.enabledAt = packet.deliveredAt)
    (afterGst : network.gst ≤ packet.sentAt) :
    action.completedAt ≤
      packet.sentAt + network.delta + processing.epsilon := by
  rcases network.postGstDelivery packet validPacket afterGst with
    ⟨sentBeforeDelivery, deliveryBound⟩
  have actionAfterGst : network.gst ≤ action.enabledAt := by
    rw [actionStartsAtDelivery]
    exact Nat.le_trans afterGst sentBeforeDelivery
  rcases processing.postGstCompletion action validAction actionAfterGst with
    ⟨_, completionBound⟩
  rw [actionStartsAtDelivery] at completionBound
  have deliveryWithProcessing :
      packet.deliveredAt + processing.epsilon ≤
        packet.sentAt + network.delta + processing.epsilon :=
    Nat.add_le_add_right deliveryBound processing.epsilon
  exact Nat.le_trans completionBound deliveryWithProcessing

/-- Under the proof's instantaneous-local-computation idealization, the network
delay bound also bounds when the message becomes visible to consensus logic. -/
theorem protocol_packet_is_immediately_visible_after_delivery
    {protocolPacket : Packet → Prop}
    {network : PartialSynchrony protocolPacket}
    {protocolAction : LocalConsensusAction → Prop}
    (processing : BoundedLocalProcessing network protocolAction)
    (instantaneous : processing.epsilon = 0)
    (packet : Packet) (validPacket : protocolPacket packet)
    (action : LocalConsensusAction) (validAction : protocolAction action)
    (actionStartsAtDelivery : action.enabledAt = packet.deliveredAt)
    (afterGst : network.gst ≤ packet.sentAt) :
    action.completedAt ≤ packet.sentAt + network.delta := by
  have visible := protocol_packet_becomes_locally_visible processing packet
    validPacket action validAction actionStartsAtDelivery afterGst
  simpa [instantaneous] using visible

end BoundedLocalProcessing

/-- The local data relevant to a catch-up operation. -/
structure RoundState where
  currentRound : Nat
  proposed : Nat → Bool
  decided : Nat → Bool

/-- The required part of the modified Mysticeti catch-up rule. The Rust refinement
obligation is `ASM-LIVE-ROUND-CATCHUP`. -/
def SafeIntermediateProposals
    (before after : RoundState) (observedRound : Nat) : Prop :=
  ∀ intermediate,
    before.currentRound < intermediate →
    intermediate < observedRound →
    3 ≤ intermediate →
    before.decided (intermediate - 2) = false →
    after.proposed intermediate = true

/-- Catch up directly to one observed round and propose only in that round. -/
def jumpWithoutIntermediate (before : RoundState) (observedRound : Nat) : RoundState :=
  { currentRound := observedRound
    proposed := fun round =>
      if round = observedRound then true else before.proposed round
    decided := before.decided }

/-- A direct jump does not establish the safe catch-up rule when one required round is missing. -/
theorem direct_jump_can_violate_safe_catchup
    (before : RoundState) (observedRound : Nat)
    (gap : before.currentRound + 1 < observedRound)
    (roundInRule : 3 ≤ before.currentRound + 1)
    (undecided : before.decided (before.currentRound + 1 - 2) = false)
    (notProposed : before.proposed (before.currentRound + 1) = false) :
    ¬SafeIntermediateProposals before
      (jumpWithoutIntermediate before observedRound) observedRound := by
  intro safe
  have required := safe (before.currentRound + 1)
    (by omega) gap roundInRule undecided
  have differentRound : before.currentRound + 1 ≠ observedRound := by omega
  simp [jumpWithoutIntermediate, differentRound, notProposed] at required

end Mysticeti
