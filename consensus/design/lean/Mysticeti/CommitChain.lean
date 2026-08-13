/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.Finalizer

namespace Mysticeti

/-! The continuous common commit chain used by indirect decisions. -/

structure Commit where
  index : Nat
  leaderRound : Nat
  deriving DecidableEq, Repr

abbrev CommitStream := Nat → Commit

/-- Commit indices have no gap. This is checked by the observer and finalizer. -/
def Continuous (stream : CommitStream) : Prop :=
  ∀ position, (stream (position + 1)).index = (stream position).index + 1

theorem continuous_index (stream : CommitStream) (continuous : Continuous stream)
    (position : Nat) :
    (stream position).index = (stream 0).index + position := by
  induction position with
  | zero => simp
  | succ position ih =>
      have step := continuous position
      change (stream (position + 1)).index = (stream 0).index + (position + 1)
      omega

/-- A commit is deep enough to trigger indirect finalization for a target round. -/
def DepthTwoEligible (targetRound : Nat) (commit : Commit) : Prop :=
  targetRound + 2 ≤ commit.leaderRound

/-- `position` is the first eligible commit after `start`. -/
def FirstEligible (stream : CommitStream) (targetRound start position : Nat) : Prop :=
  start ≤ position ∧
    DepthTwoEligible targetRound (stream position) ∧
    ∀ earlier, start ≤ earlier → earlier < position →
      ¬DepthTwoEligible targetRound (stream earlier)

theorem firstEligible_unique (stream : CommitStream) {targetRound start left right : Nat}
    (leftFirst : FirstEligible stream targetRound start left)
    (rightFirst : FirstEligible stream targetRound start right) : left = right := by
  apply Nat.le_antisymm
  · apply Nat.le_of_not_gt
    intro rightBeforeLeft
    exact (leftFirst.2.2 right rightFirst.1 rightBeforeLeft) rightFirst.2.1
  · apply Nat.le_of_not_gt
    intro leftBeforeRight
    exact (rightFirst.2.2 left leftFirst.1 leftBeforeRight) leftFirst.2.1

/-- A node can see the first trigger in a prefix of this length. -/
def VisibleFirst (stream : CommitStream) (targetRound start prefixLength position : Nat) : Prop :=
  position < prefixLength ∧ FirstEligible stream targetRound start position

theorem first_trigger_is_prefix_stable (stream : CommitStream)
    {targetRound start shortLength longLength position : Nat}
    (visible : VisibleFirst stream targetRound start shortLength position)
    (extension : shortLength ≤ longLength) :
    VisibleFirst stream targetRound start longLength position := by
  constructor
  · have positionShort := visible.1
    omega
  · exact visible.2

/-- Certificate-or-reject at the one permitted trigger position. -/
def indirectAt (hasCertificate : Nat → Bool) (position : Nat) :
    TransactionEvidence.Outcome :=
  if hasCertificate position = true then .accept else .reject

/-- Nodes with the same stream and first trigger choose the same indirect result. -/
theorem first_trigger_agreement (stream : CommitStream) (hasCertificate : Nat → Bool)
    {targetRound start leftLength rightLength left right : Nat}
    (leftVisible : VisibleFirst stream targetRound start leftLength left)
    (rightVisible : VisibleFirst stream targetRound start rightLength right) :
    indirectAt hasCertificate left = indirectAt hasCertificate right := by
  have samePosition := firstEligible_unique stream leftVisible.2 rightVisible.2
  subst right
  rfl

/-- A later arbitrary prefix can add an accept certificate. This is not a safe decision rule. -/
def flippingCertificate : Nat → Bool
  | 0 => false
  | _ + 1 => true

theorem arbitrary_prefix_decision_can_flip :
    indirectAt flippingCertificate 0 = .reject ∧
      indirectAt flippingCertificate 1 = .accept := by
  constructor <;> rfl

/-- The safe rule uses the same first trigger, so later prefixes cannot flip its result. -/
theorem first_trigger_result_is_prefix_stable
    (stream : CommitStream) (hasCertificate : Nat → Bool)
    {targetRound start shortLength longLength position : Nat}
    (visible : VisibleFirst stream targetRound start shortLength position)
    (extension : shortLength ≤ longLength) :
    ∃ stablePosition,
      VisibleFirst stream targetRound start longLength stablePosition ∧
        indirectAt hasCertificate stablePosition = indirectAt hasCertificate position := by
  exact ⟨position, first_trigger_is_prefix_stable stream visible extension, rfl⟩

end Mysticeti
