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

/-- Standard partial synchrony: GST is unknown, and post-GST delay is at most `delta`. -/
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

/-- The local data relevant to a catch-up operation. -/
structure RoundState where
  currentRound : Nat
  proposed : Nat → Bool
  decided : Nat → Bool

/-- The required part of the modified Mysticeti catch-up rule. -/
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
