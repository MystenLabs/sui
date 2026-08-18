/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.LeaderCoverage

namespace Mysticeti

/-!
The probability boundary for first selected leader slots.

The main interface is a positive conditional chance law. It does not contain a
successful trace. For each disjoint candidate window, it gives a positive lower
bound on the chance that all first selected leader slots in that window are
correct and available. The bound must hold after every failed prefix.

This Lean project does not include a probability measure or limit library.
Therefore, this module proves the exact finite failure bound and an operational
reciprocal-tolerance limit-zero result. It does not label these results as an
almost-sure theorem. A measure-theory layer must add a countable product measure,
the measurable run event, and the link from limit zero to probability one.
-/

/-- A finite probability is represented by a numerator and a positive common
denominator. The module does not use division. -/
structure FiniteProbabilityFraction where
  numerator : Nat
  denominator : Nat
  denominatorPositive : 0 < denominator
  numeratorAtMostDenominator : numerator ≤ denominator

/-- A positive conditional chance law for disjoint candidate windows.

`totalExtensions` is the common weight of all extensions of one failed prefix.
`successfulExtensions` is a lower bound on the extension weight that gives a
complete correct-and-available first-slot window. `failedPrefixWeight k` is the
weight of prefixes in which the first `k` candidate windows all fail.

The step inequality is the conditional chance law. It is weaker than
independence because the exact distribution can depend on the failed prefix.
-/
structure PositiveConditionalFirstSlotLaw where
  windowLength : Nat
  windowLengthPositive : 0 < windowLength
  totalExtensions : Nat
  successfulExtensions : Nat
  totalExtensionsPositive : 0 < totalExtensions
  successfulExtensionsPositive : 0 < successfulExtensions
  successfulExtensionsAtMostTotal : successfulExtensions ≤ totalExtensions
  failedPrefixWeight : Nat → Nat
  failedPrefixWeightZero : failedPrefixWeight 0 = 1
  failedPrefixStep : ∀ attempts,
    failedPrefixWeight (attempts + 1) ≤
      failedPrefixWeight attempts *
        (totalExtensions - successfulExtensions)

namespace PositiveConditionalFirstSlotLaw

/-- A finite-probability sequence tends to zero in the operational rational
sense when it eventually becomes smaller than every reciprocal natural
tolerance. This definition does not use a measure space or real-number limits. -/
def FailureFractionTendsToZero
    (law : PositiveConditionalFirstSlotLaw) : Prop :=
  ∀ toleranceDenominator, 0 < toleranceDenominator →
    ∃ attempts,
      toleranceDenominator * law.failedPrefixWeight attempts <
        law.totalExtensions ^ attempts

private theorem pow_add_one_first_terms
    (base exponent : Nat) :
    base ^ exponent + exponent * base ^ (exponent - 1) ≤
      (base + 1) ^ exponent := by
  induction exponent with
  | zero => simp
  | succ exponent ih =>
      cases exponent with
      | zero => simp
      | succ exponent =>
      calc
        base ^ (exponent + 1 + 1) +
              (exponent + 1 + 1) *
                base ^ ((exponent + 1 + 1) - 1) ≤
            (base ^ (exponent + 1) +
                (exponent + 1) * base ^ ((exponent + 1) - 1)) *
              (base + 1) := by
          simp only [Nat.add_sub_cancel, Nat.pow_succ, Nat.mul_add,
            Nat.add_mul]
          grind
        _ ≤ (base + 1) ^ (exponent + 1) * (base + 1) := by
          exact Nat.mul_le_mul_right (base + 1) ih
        _ = (base + 1) ^ (exponent + 1 + 1) := by
          simp only [Nat.pow_succ, Nat.mul_assoc]

private theorem scaled_geometric_is_eventually_small
    {failure total toleranceDenominator : Nat}
    (failurePositive : 0 < failure)
    (failureBelowTotal : failure < total) :
    ∃ attempts,
      toleranceDenominator * failure ^ attempts < total ^ attempts := by
  let attempts := toleranceDenominator * failure + 1
  have firstTerms := pow_add_one_first_terms failure attempts
  have powerPositive : 0 < failure ^ (toleranceDenominator * failure) :=
    Nat.pow_pos failurePositive
  have scaledBelowFirstTerms :
      toleranceDenominator * failure ^ attempts <
        failure ^ attempts + attempts * failure ^ (attempts - 1) := by
    simp only [attempts, Nat.add_sub_cancel, Nat.pow_succ]
    have scaledIdentity :
        toleranceDenominator *
            (failure ^ (toleranceDenominator * failure) * failure) =
          (toleranceDenominator * failure) *
            failure ^ (toleranceDenominator * failure) := by
      ac_rfl
    rw [scaledIdentity, Nat.add_mul]
    omega
  have nextBaseAtMostTotal : failure + 1 ≤ total := by
    omega
  have nextBasePowerAtMostTotal :
      (failure + 1) ^ attempts ≤ total ^ attempts :=
    Nat.pow_le_pow_left nextBaseAtMostTotal attempts
  exact ⟨attempts,
    Nat.lt_of_lt_of_le scaledBelowFirstTerms
      (Nat.le_trans firstTerms nextBasePowerAtMostTotal)⟩

/-- The normalized failure bound after `attempts` disjoint windows. -/
def geometricFailureFraction
    (law : PositiveConditionalFirstSlotLaw) (attempts : Nat) :
    FiniteProbabilityFraction :=
  { numerator :=
      (law.totalExtensions - law.successfulExtensions) ^ attempts
    denominator := law.totalExtensions ^ attempts
    denominatorPositive := by
      exact Nat.pow_pos law.totalExtensionsPositive
    numeratorAtMostDenominator := by
      exact Nat.pow_le_pow_left
        (Nat.sub_le law.totalExtensions law.successfulExtensions) attempts }

/-- The failed-prefix weight is at most the geometric failure numerator. -/
theorem failed_prefix_weight_le_geometric
    (law : PositiveConditionalFirstSlotLaw) (attempts : Nat) :
    law.failedPrefixWeight attempts ≤
      (law.totalExtensions - law.successfulExtensions) ^ attempts := by
  induction attempts with
  | zero =>
      rw [law.failedPrefixWeightZero]
      simp
  | succ attempts ih =>
      calc
        law.failedPrefixWeight (attempts + 1) ≤
            law.failedPrefixWeight attempts *
              (law.totalExtensions - law.successfulExtensions) :=
          law.failedPrefixStep attempts
        _ ≤ (law.totalExtensions - law.successfulExtensions) ^ attempts *
              (law.totalExtensions - law.successfulExtensions) := by
          exact Nat.mul_le_mul_right
            (law.totalExtensions - law.successfulExtensions) ih
        _ = (law.totalExtensions - law.successfulExtensions) ^
              (attempts + 1) := by
          rw [Nat.pow_succ]

/-- The finite failure probability uses the common denominator
`totalExtensions ^ attempts` and is at most the geometric fraction. -/
theorem finite_failure_bound
    (law : PositiveConditionalFirstSlotLaw) (attempts : Nat) :
    law.failedPrefixWeight attempts ≤
        (law.geometricFailureFraction attempts).numerator ∧
      (law.geometricFailureFraction attempts).denominator =
        law.totalExtensions ^ attempts := by
  exact ⟨law.failed_prefix_weight_le_geometric attempts, rfl⟩

/-- If one candidate window can both succeed and fail, the operational failure
fraction tends to zero. The proof uses only natural-number arithmetic. -/
theorem failure_fraction_tends_to_zero
    (law : PositiveConditionalFirstSlotLaw)
    (successNotCertain :
      law.successfulExtensions < law.totalExtensions) :
    law.FailureFractionTendsToZero := by
  intro toleranceDenominator _tolerancePositive
  let failure := law.totalExtensions - law.successfulExtensions
  have failurePositive : 0 < failure := by
    simp only [failure]
    have successBelowTotal := successNotCertain
    omega
  have failureBelowTotal : failure < law.totalExtensions := by
    simp only [failure]
    have totalPositive := law.totalExtensionsPositive
    have successPositive := law.successfulExtensionsPositive
    omega
  rcases scaled_geometric_is_eventually_small
      (toleranceDenominator := toleranceDenominator)
      failurePositive failureBelowTotal with ⟨attempts, geometricSmall⟩
  refine ⟨attempts, Nat.lt_of_le_of_lt ?_ geometricSmall⟩
  exact Nat.mul_le_mul_left toleranceDenominator
    (law.failed_prefix_weight_le_geometric attempts)

/-- One candidate window has a positive chance of success. Thus, its failure
weight is strictly smaller than the total extension weight. -/
theorem one_window_failure_is_strictly_bounded
    (law : PositiveConditionalFirstSlotLaw) :
    law.failedPrefixWeight 1 < law.totalExtensions := by
  have step := law.failedPrefixStep 0
  rw [law.failedPrefixWeightZero] at step
  simp only [Nat.zero_add, Nat.one_mul] at step
  have remainingIsSmaller :
      law.totalExtensions - law.successfulExtensions <
        law.totalExtensions := by
    have totalPositive := law.totalExtensionsPositive
    have successPositive := law.successfulExtensionsPositive
    omega
  omega

end PositiveConditionalFirstSlotLaw

/-! ### Independent uniform first-slot sampling -/

/-- Parameters for an ideal independent uniform first-slot law.

Each round chooses the first selected leader slot uniformly from the stable
leader schedule. Choices in different rounds are independent. A candidate window
has `scheduleMemberCount ^ windowLength` equally weighted results. Exactly
`correctAvailableMemberCount ^ windowLength` results have a correct, available
first slot in every round.

This is an ideal probability model. It is not a claim about the current Rust
seeded shuffle.
-/
structure IndependentUniformFirstSlotLaw where
  scheduleMemberCount : Nat
  correctAvailableMemberCount : Nat
  scheduleMemberCountPositive : 0 < scheduleMemberCount
  correctAvailableMemberCountPositive : 0 < correctAvailableMemberCount
  correctAvailableMemberCountAtMostSchedule :
    correctAvailableMemberCount ≤ scheduleMemberCount

namespace IndependentUniformFirstSlotLaw

/-- The independent uniform law gives an exact positive conditional chance law
for windows of `depth + 1` rounds. -/
def toPositiveConditionalLaw
    (law : IndependentUniformFirstSlotLaw) (depth : Nat) :
    PositiveConditionalFirstSlotLaw :=
  let windowLength := depth + 1
  let totalExtensions := law.scheduleMemberCount ^ windowLength
  let successfulExtensions :=
    law.correctAvailableMemberCount ^ windowLength
  { windowLength := windowLength
    windowLengthPositive := by omega
    totalExtensions := totalExtensions
    successfulExtensions := successfulExtensions
    totalExtensionsPositive := by
      exact Nat.pow_pos law.scheduleMemberCountPositive
    successfulExtensionsPositive := by
      exact Nat.pow_pos law.correctAvailableMemberCountPositive
    successfulExtensionsAtMostTotal := by
      exact Nat.pow_le_pow_left
        law.correctAvailableMemberCountAtMostSchedule windowLength
    failedPrefixWeight := fun attempts =>
      (totalExtensions - successfulExtensions) ^ attempts
    failedPrefixWeightZero := by simp
    failedPrefixStep := by
      intro attempts
      rw [Nat.pow_succ]
      exact Nat.le_refl _ }

/-- For `attempts` disjoint windows, uniform independent sampling gives the exact
finite failure numerator
`(scheduleSize^(d+1) - correctAvailableSize^(d+1))^attempts`.
The denominator is `scheduleSize^((d+1)*attempts)`. -/
theorem independent_uniform_finite_failure_bound
    (law : IndependentUniformFirstSlotLaw) (depth attempts : Nat) :
    let windowLength := depth + 1
    let totalWindowResults := law.scheduleMemberCount ^ windowLength
    let successfulWindowResults :=
      law.correctAvailableMemberCount ^ windowLength
    let conditionalLaw := law.toPositiveConditionalLaw depth
    conditionalLaw.failedPrefixWeight attempts =
        (totalWindowResults - successfulWindowResults) ^ attempts ∧
      (conditionalLaw.geometricFailureFraction attempts).denominator =
        law.scheduleMemberCount ^ (windowLength * attempts) := by
  simp [toPositiveConditionalLaw, PositiveConditionalFirstSlotLaw.geometricFailureFraction,
    Nat.pow_mul]

/-- If the stable schedule also contains a non-progressing member, the uniform
independent failure fraction becomes smaller than every reciprocal natural
tolerance. -/
theorem independent_uniform_failure_fraction_tends_to_zero
    (law : IndependentUniformFirstSlotLaw) (depth : Nat)
    (notAllMembersProgress :
      law.correctAvailableMemberCount < law.scheduleMemberCount) :
    (law.toPositiveConditionalLaw depth).FailureFractionTendsToZero := by
  apply PositiveConditionalFirstSlotLaw.failure_fraction_tends_to_zero
  change law.correctAvailableMemberCount ^ (depth + 1) <
    law.scheduleMemberCount ^ (depth + 1)
  exact Nat.pow_lt_pow_left notAllMembersProgress (by omega)

end IndependentUniformFirstSlotLaw

/-! ### Infinite traces and operational probability one -/

/-- An infinite trace of first selected leader slot indexes. Schedule members can
be renamed so that indexes below `correctAvailableMemberCount` are the correct,
available members. Uniform sampling is unchanged by this renaming. -/
abbrev UniformFirstSlotTrace (law : IndependentUniformFirstSlotLaw) :=
  Nat → Fin law.scheduleMemberCount

/-- One sampled first slot is correct and available. -/
def uniformFirstSlotIsCorrectAvailable
    (law : IndependentUniformFirstSlotLaw)
    (sample : Fin law.scheduleMemberCount) : Prop :=
  sample.val < law.correctAvailableMemberCount

/-- All first selected leader slots in one window are correct and available. -/
def UniformFavorableWindowAt
    (law : IndependentUniformFirstSlotLaw) (depth : Nat)
    (trace : UniformFirstSlotTrace law) (base : Nat) : Prop :=
  ∀ offset, offset < depth + 1 →
    uniformFirstSlotIsCorrectAvailable law (trace (base + offset))

/-- A favorable `depth + 1` window occurs after a fixed start round. -/
def EventuallyUniformFavorableWindowAfter
    (law : IndependentUniformFirstSlotLaw) (depth start : Nat)
    (trace : UniformFirstSlotTrace law) : Prop :=
  ∃ base, start ≤ base ∧
    UniformFavorableWindowAt law depth trace base

/-- One of the first `attempts` disjoint windows after `start` is favorable. -/
def HasAlignedUniformFavorableWindow
    (law : IndependentUniformFirstSlotLaw) (depth start attempts : Nat)
    (trace : UniformFirstSlotTrace law) : Prop :=
  ∃ attempt, attempt < attempts ∧
    UniformFavorableWindowAt law depth trace
      (start + attempt * (depth + 1))

/-- An aligned favorable window is also an eventual favorable window. -/
theorem aligned_uniform_window_is_eventual
    (law : IndependentUniformFirstSlotLaw) (depth start attempts : Nat)
    (trace : UniformFirstSlotTrace law)
    (aligned : HasAlignedUniformFavorableWindow
      law depth start attempts trace) :
    EventuallyUniformFavorableWindowAfter law depth start trace := by
  rcases aligned with ⟨attempt, _attemptInRange, favorable⟩
  refine ⟨start + attempt * (depth + 1), ?_, favorable⟩
  omega

/-- The exact uniform failure numerator for `attempts` disjoint windows. -/
def uniformAlignedFailureNumerator
    (law : IndependentUniformFirstSlotLaw) (depth attempts : Nat) : Nat :=
  let windowLength := depth + 1
  let totalWindowResults := law.scheduleMemberCount ^ windowLength
  let successfulWindowResults :=
    law.correctAvailableMemberCount ^ windowLength
  (totalWindowResults - successfulWindowResults) ^ attempts

/-- The exact uniform denominator for `attempts` disjoint windows. -/
def uniformAlignedTotalDenominator
    (law : IndependentUniformFirstSlotLaw) (depth attempts : Nat) : Nat :=
  law.scheduleMemberCount ^ ((depth + 1) * attempts)

/-- Operational probability one under independent uniform first-slot sampling.

For every reciprocal tolerance `1 / n`, a finite cylinder event is contained in
`event`, and the complement of that cylinder has probability less than `1 / n`.
The numerator and denominator use exact finite uniform counts.

This definition is sufficient for monotonic liveness arguments. It is not a full
measure space and does not provide countable event operations.
-/
def UniformPrefixProbabilityOne
    (law : IndependentUniformFirstSlotLaw) (depth start : Nat)
    (event : UniformFirstSlotTrace law → Prop) : Prop :=
  ∀ toleranceDenominator, 0 < toleranceDenominator →
    ∃ attempts,
      toleranceDenominator *
          uniformAlignedFailureNumerator law depth attempts <
        uniformAlignedTotalDenominator law depth attempts ∧
      ∀ trace,
        HasAlignedUniformFavorableWindow law depth start attempts trace →
          event trace

/-- Operational probability one is monotone. This rule lets a later proof
transfer the favorable-window result to a liveness result. -/
theorem uniform_prefix_probability_one_mono
    (law : IndependentUniformFirstSlotLaw) (depth start : Nat)
    {left right : UniformFirstSlotTrace law → Prop}
    (leftProbabilityOne :
      UniformPrefixProbabilityOne law depth start left)
    (impliesRight : ∀ trace, left trace → right trace) :
    UniformPrefixProbabilityOne law depth start right := by
  intro toleranceDenominator tolerancePositive
  rcases leftProbabilityOne toleranceDenominator tolerancePositive with
    ⟨attempts, failureSmall, included⟩
  exact ⟨attempts, failureSmall, fun trace aligned =>
    impliesRight trace (included trace aligned)⟩

/-- For each fixed start round, an independent uniform first-slot trace has a
favorable `depth + 1` window with operational probability one. -/
theorem eventual_uniform_favorable_window_probability_one
    (law : IndependentUniformFirstSlotLaw) (depth start : Nat) :
    UniformPrefixProbabilityOne law depth start
      (EventuallyUniformFavorableWindowAfter law depth start) := by
  by_cases notAllMembersProgress :
      law.correctAvailableMemberCount < law.scheduleMemberCount
  · intro toleranceDenominator tolerancePositive
    have tendsToZero :=
      law.independent_uniform_failure_fraction_tends_to_zero depth
        notAllMembersProgress
    rcases tendsToZero toleranceDenominator tolerancePositive with
      ⟨attempts, failureSmall⟩
    refine ⟨attempts, ?_, ?_⟩
    · simpa [uniformAlignedFailureNumerator,
        uniformAlignedTotalDenominator,
        IndependentUniformFirstSlotLaw.toPositiveConditionalLaw,
        Nat.pow_mul] using failureSmall
    · intro trace aligned
      exact aligned_uniform_window_is_eventual law depth start attempts trace
        aligned
  · have allMembersProgress :
        law.correctAvailableMemberCount = law.scheduleMemberCount := by
      have atMost := law.correctAvailableMemberCountAtMostSchedule
      omega
    intro toleranceDenominator _tolerancePositive
    refine ⟨1, ?_, ?_⟩
    · simp [uniformAlignedFailureNumerator,
        uniformAlignedTotalDenominator, allMembersProgress,
        Nat.pow_pos law.scheduleMemberCountPositive]
    · intro trace aligned
      exact aligned_uniform_window_is_eventual law depth start 1 trace aligned

/-- The probability-one result is available for each fixed start. A full measure
theory can use countable intersection to obtain one event that covers all starts. -/
theorem every_fixed_start_has_favorable_window_probability_one
    (law : IndependentUniformFirstSlotLaw) (depth : Nat) :
    ∀ start,
      UniformPrefixProbabilityOne law depth start
        (EventuallyUniformFavorableWindowAfter law depth start) := by
  intro start
  exact eventual_uniform_favorable_window_probability_one law depth start

/-- A deterministic protocol implication transfers the first-slot result to an
operational probability-one liveness result. This is the probability boundary
for a later end-to-end theorem. -/
theorem favorable_window_transfers_to_liveness_probability_one
    (law : IndependentUniformFirstSlotLaw) (depth start : Nat)
    (liveness : UniformFirstSlotTrace law → Prop)
    (favorableImpliesLiveness : ∀ trace,
      EventuallyUniformFavorableWindowAfter law depth start trace →
        liveness trace) :
    UniformPrefixProbabilityOne law depth start liveness := by
  exact uniform_prefix_probability_one_mono law depth start
    (eventual_uniform_favorable_window_probability_one law depth start)
    favorableImpliesLiveness

/-! ### Separate deterministic alternative -/

/-- Repeated-first ordering is a deterministic alternative to the probability
law. It gives a `depth + 1` window after each start round when the schedule has a
correct, available member. This theorem does not describe the current Rust
seeded shuffle. -/
theorem deterministic_repeated_first_depth_instance
    {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (correctAvailable : VoterSet)
    (depth : Nat)
    (containsCorrectAvailable :
      ∃ member, correctAvailable (schedule.memberAt member) = true)
    (start : Nat) :
    ∃ base, start ≤ base ∧
      FavorableFirstLeaderWindow schedule correctAvailable
        (depth + 1) base := by
  exact repeated_first_has_depth_window_after schedule correctAvailable depth
    containsCorrectAvailable start

end Mysticeti
