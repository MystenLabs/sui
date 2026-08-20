/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega

namespace Mysticeti

/-! Weighted voting facts used by the v3 leader decider and transaction finalizer.

The Rust refinement uses `ASM-SAFE-PARAMETERS`, `ASM-SAFE-FAULT-BOUND`, and
`ASM-REFINE-INTEGERS` from the assumption ledger.
-/

/-- A voter set indexed by the same natural numbers as `AuthorityIndex`. -/
abbrev VoterSet := Nat → Bool

namespace VoterSet

def empty : VoterSet := fun _ => false

def full : VoterSet := fun _ => true

def union (left right : VoterSet) : VoterSet := fun authority =>
  left authority || right authority

def inter (left right : VoterSet) : VoterSet := fun authority =>
  left authority && right authority

def diff (left right : VoterSet) : VoterSet := fun authority =>
  left authority && !right authority

/-- Inclusion restricted to valid authority indices. -/
def SubsetAt (authorityCount : Nat) (left right : VoterSet) : Prop :=
  ∀ authority, authority < authorityCount →
    left authority = true → right authority = true

theorem subset_full (authorityCount : Nat) (voters : VoterSet) :
    SubsetAt authorityCount voters full := by
  intro authority _ _
  rfl

theorem inter_subset_right (authorityCount : Nat) (left right : VoterSet) :
    SubsetAt authorityCount (inter left right) right := by
  intro authority _ included
  simp [inter] at included
  exact included.2

end VoterSet

/-- Stake in a voter set. Authorities at indices `authorityCount` and above do not count. -/
def weight : Nat → (Nat → Nat) → VoterSet → Nat
  | 0, _, _ => 0
  | authorityCount + 1, stake, voters =>
      weight authorityCount stake voters +
        if voters authorityCount = true then stake authorityCount else 0

/-- The total committee stake. -/
def totalWeight (authorityCount : Nat) (stake : Nat → Nat) : Nat :=
  weight authorityCount stake VoterSet.full

@[simp]
theorem weight_empty (authorityCount : Nat) (stake : Nat → Nat) :
    weight authorityCount stake VoterSet.empty = 0 := by
  induction authorityCount with
  | zero => rfl
  | succ authorityCount ih => simp [weight, VoterSet.empty, ih]

theorem weight_mono {authorityCount : Nat} (stake : Nat → Nat)
    {left right : VoterSet}
    (subset : VoterSet.SubsetAt authorityCount left right) :
    weight authorityCount stake left ≤ weight authorityCount stake right := by
  induction authorityCount with
  | zero => simp [weight]
  | succ authorityCount ih =>
      simp only [weight]
      apply Nat.add_le_add
      · apply ih
        intro authority inRange included
        exact subset authority (by omega) included
      · cases leftValue : left authorityCount with
        | false => simp
        | true =>
            have rightValue := subset authorityCount (by omega) leftValue
            simp [rightValue]

theorem weight_le_total (authorityCount : Nat) (stake : Nat → Nat)
    (voters : VoterSet) :
    weight authorityCount stake voters ≤ totalWeight authorityCount stake := by
  exact weight_mono stake (VoterSet.subset_full authorityCount voters)

/-- Positive set stake gives one in-range selected authority with positive stake. -/
theorem positive_weight_has_member
    {authorityCount : Nat} {stake : Nat → Nat} {voters : VoterSet}
    (positive : 0 < weight authorityCount stake voters) :
    ∃ authority, authority < authorityCount ∧ voters authority = true ∧
      0 < stake authority := by
  induction authorityCount with
  | zero => simp [weight] at positive
  | succ authorityCount ih =>
      by_cases selected : voters authorityCount = true
      · by_cases earlierPositive :
          0 < weight authorityCount stake voters
        · rcases ih earlierPositive with
            ⟨authority, authorityInRange, authoritySelected, authorityStake⟩
          exact ⟨authority, by omega, authoritySelected, authorityStake⟩
        · have earlierZero : weight authorityCount stake voters = 0 := by omega
          have lastPositive : 0 < stake authorityCount := by
            simp [weight, selected, earlierZero] at positive
            exact positive
          exact ⟨authorityCount, by omega, selected, lastPositive⟩
      · have earlierPositive : 0 < weight authorityCount stake voters := by
          simpa [weight, selected] using positive
        rcases ih earlierPositive with
          ⟨authority, authorityInRange, authoritySelected, authorityStake⟩
        exact ⟨authority, by omega, authoritySelected, authorityStake⟩

theorem weight_union_add_inter (authorityCount : Nat) (stake : Nat → Nat)
    (left right : VoterSet) :
    weight authorityCount stake (VoterSet.union left right) +
        weight authorityCount stake (VoterSet.inter left right) =
      weight authorityCount stake left + weight authorityCount stake right := by
  induction authorityCount with
  | zero => simp [weight]
  | succ authorityCount ih =>
      simp only [weight]
      have recurrent := ih
      cases leftValue : left authorityCount <;>
        cases rightValue : right authorityCount <;>
        simp [VoterSet.union, VoterSet.inter, leftValue, rightValue] at * <;>
        omega

/-- The stake of a union is at most the sum of the two input stakes. -/
theorem weight_union_le_add (authorityCount : Nat) (stake : Nat → Nat)
    (left right : VoterSet) :
    weight authorityCount stake (VoterSet.union left right) ≤
      weight authorityCount stake left + weight authorityCount stake right := by
  have identity := weight_union_add_inter authorityCount stake left right
  omega

theorem weight_diff_add_inter (authorityCount : Nat) (stake : Nat → Nat)
    (left right : VoterSet) :
    weight authorityCount stake (VoterSet.diff left right) +
        weight authorityCount stake (VoterSet.inter left right) =
      weight authorityCount stake left := by
  induction authorityCount with
  | zero => simp [weight]
  | succ authorityCount ih =>
      simp only [weight]
      have recurrent := ih
      cases leftValue : left authorityCount <;>
        cases rightValue : right authorityCount <;>
        simp [VoterSet.diff, VoterSet.inter, leftValue, rightValue] at * <;>
        omega

theorem intersection_lower_bound (authorityCount : Nat) (stake : Nat → Nat)
    {left right : VoterSet} {leftThreshold rightThreshold : Nat}
    (leftWeight : leftThreshold ≤ weight authorityCount stake left)
    (rightWeight : rightThreshold ≤ weight authorityCount stake right) :
    leftThreshold + rightThreshold ≤
      totalWeight authorityCount stake +
        weight authorityCount stake (VoterSet.inter left right) := by
  have unionBound :=
    weight_le_total authorityCount stake (VoterSet.union left right)
  have identity := weight_union_add_inter authorityCount stake left right
  omega

/-- The threshold facts used by the safety proof. `ASM-MATH-THRESHOLDS` gives one
nominal instance. `ASM-SAFE-PARAMETERS` maps the actual values and inequalities to
one epoch configuration. -/
structure Thresholds (authorityCount : Nat) (stake : Nat → Nat) where
  /-- Maximum Byzantine stake. -/
  fault : Nat
  /-- Direct decision threshold. -/
  quorum : Nat
  /-- Indirect certification threshold. -/
  certificate : Nat
  certificate_positive : 0 < certificate
  quorum_certificate_intersection :
    totalWeight authorityCount stake + fault < quorum + certificate
  quorum_preserves_certificate :
    totalWeight authorityCount stake + fault + certificate ≤ quorum + quorum

namespace Thresholds

/-- The checked threshold inequalities imply that the quorum threshold is
positive. -/
theorem quorum_positive {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake) :
    0 < thresholds.quorum := by
  have certificatePositive := thresholds.certificate_positive
  have preservesCertificate := thresholds.quorum_preserves_certificate
  omega

/-- The nominal v3 thresholds for `N = 5f + 3c + 1`. -/
def nominalHybrid {authorityCount : Nat} {stake : Nat → Nat}
    (malicious crash : Nat)
    (totalStake :
      totalWeight authorityCount stake = 5 * malicious + 3 * crash + 1) :
    Thresholds authorityCount stake where
  fault := malicious
  quorum := 4 * malicious + 2 * crash + 1
  certificate := 2 * malicious + crash + 1
  certificate_positive := by omega
  quorum_certificate_intersection := by omega
  quorum_preserves_certificate := by omega

theorem two_quorums_intersect {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake) :
    totalWeight authorityCount stake + thresholds.fault <
      thresholds.quorum + thresholds.quorum := by
  have positive := thresholds.certificate_positive
  have preserve := thresholds.quorum_preserves_certificate
  omega

end Thresholds

/-- The selected Byzantine authorities have at most the configured fault stake.
This is the adversary assumption `ASM-SAFE-FAULT-BOUND`. -/
def FaultBounded {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake) (faulty : VoterSet) : Prop :=
  weight authorityCount stake faulty ≤ thresholds.fault

/-- A correct authority cannot occur in both incompatible voter sets. The Rust
refinement obligation is `ASM-SAFE-VOTE-SET-OVERLAP`. -/
def OnlyFaultyOverlap (authorityCount : Nat)
    (faulty left right : VoterSet) : Prop :=
  VoterSet.SubsetAt authorityCount (VoterSet.inter left right) faulty

section Intersections

variable {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}
variable {faulty left right : VoterSet}

theorem incompatible_thresholds_impossible
    (faultBounded : FaultBounded thresholds faulty)
    (onlyFaulty : OnlyFaultyOverlap authorityCount faulty left right)
    {leftThreshold rightThreshold : Nat}
    (leftWeight : leftThreshold ≤ weight authorityCount stake left)
    (rightWeight : rightThreshold ≤ weight authorityCount stake right)
    (thresholdIntersection :
      totalWeight authorityCount stake + thresholds.fault <
        leftThreshold + rightThreshold) : False := by
  have overlapLower :=
    intersection_lower_bound authorityCount stake leftWeight rightWeight
  have overlapUpper := weight_mono stake onlyFaulty
  unfold FaultBounded at faultBounded
  omega

theorem incompatible_quorums_impossible
    (faultBounded : FaultBounded thresholds faulty)
    (onlyFaulty : OnlyFaultyOverlap authorityCount faulty left right)
    (leftQuorum : thresholds.quorum ≤ weight authorityCount stake left)
    (rightQuorum : thresholds.quorum ≤ weight authorityCount stake right) : False := by
  exact incompatible_thresholds_impossible faultBounded onlyFaulty
    leftQuorum rightQuorum thresholds.two_quorums_intersect

theorem incompatible_quorum_certificate_impossible
    (faultBounded : FaultBounded thresholds faulty)
    (onlyFaulty : OnlyFaultyOverlap authorityCount faulty left right)
    (quorumWeight : thresholds.quorum ≤ weight authorityCount stake left)
    (certificateWeight :
      thresholds.certificate ≤ weight authorityCount stake right) : False := by
  exact incompatible_thresholds_impossible faultBounded onlyFaulty
    quorumWeight certificateWeight thresholds.quorum_certificate_intersection

/-- Two quorum sets contain certificate stake outside the Byzantine set. -/
theorem quorum_intersection_preserves_certificate
    (faultBounded : FaultBounded thresholds faulty)
    (leftQuorum : thresholds.quorum ≤ weight authorityCount stake left)
    (rightQuorum : thresholds.quorum ≤ weight authorityCount stake right) :
    thresholds.certificate ≤
      weight authorityCount stake
        (VoterSet.diff (VoterSet.inter left right) faulty) := by
  have thresholdPreservation := thresholds.quorum_preserves_certificate
  have overlapLower :=
    intersection_lower_bound authorityCount stake leftQuorum rightQuorum
  have partition :=
    weight_diff_add_inter authorityCount stake (VoterSet.inter left right) faulty
  have faultyPartBound :
      weight authorityCount stake
          (VoterSet.inter (VoterSet.inter left right) faulty) ≤
        thresholds.fault := by
    have subset :=
      VoterSet.inter_subset_right authorityCount (VoterSet.inter left right) faulty
    have mono := weight_mono stake subset
    unfold FaultBounded at faultBounded
    omega
  omega

end Intersections

end Mysticeti
