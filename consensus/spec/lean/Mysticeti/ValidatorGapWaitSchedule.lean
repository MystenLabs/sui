/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCausalCatchupRate

namespace Mysticeti

/-! A concrete quadratic recovery wait.

Let `R` be the proposal round and let `Rc` be one shared reference round. The
gap is `d = R - Rc`. The wait starts at `baseWait`. Its next value adds

`linearCoefficient + quadraticCoefficient * (2 * d + 1)`.

The sum of the odd terms is quadratic in `d`. The adjacent difference is
linear in `d`. It can therefore cover two costs that grow linearly in `d` when
their combined slope is less than `2 * quadraticCoefficient`.

The reference round is a fixed parameter of this schedule. It is not read from
the local commit head. `ValidatorHeadRelativeGapWait` gives the separate policy
that uses the local committed leader round.
-/

/-- A quadratic sequence defined by its exact adjacent difference. -/
def quadraticGapWaitFromGap
    (baseWait linearCoefficient quadraticCoefficient : Nat) : Nat → Nat
  | 0 => baseWait
  | gap + 1 =>
      quadraticGapWaitFromGap baseWait linearCoefficient
          quadraticCoefficient gap +
        linearCoefficient + quadraticCoefficient * (2 * gap + 1)

@[simp]
theorem quadraticGapWaitFromGap_succ
    (baseWait linearCoefficient quadraticCoefficient gap : Nat) :
    quadraticGapWaitFromGap baseWait linearCoefficient quadraticCoefficient
        (gap + 1) =
      quadraticGapWaitFromGap baseWait linearCoefficient quadraticCoefficient
          gap +
        linearCoefficient + quadraticCoefficient * (2 * gap + 1) := by
  rfl

/-- Static parameters for one fixed-reference quadratic gap wait. -/
structure ValidatorQuadraticGapWaitParameters where
  referenceRound : Nat
  baseWait : Time
  linearCoefficient : Nat
  quadraticCoefficient : Nat
  quadraticPositive : 0 < quadraticCoefficient

namespace ValidatorQuadraticGapWaitParameters

/-- The gap from the shared reference round. -/
def gap (parameters : ValidatorQuadraticGapWaitParameters)
    (round : Nat) : Nat :=
  round - parameters.referenceRound

/-- The concrete wait at one absolute proposal round. -/
def wait (parameters : ValidatorQuadraticGapWaitParameters)
    (round : Nat) : Time :=
  quadraticGapWaitFromGap parameters.baseWait parameters.linearCoefficient
    parameters.quadraticCoefficient (parameters.gap round)

/-- Above the reference round, the next gap is the successor of the current
gap. -/
theorem gap_succ
    (parameters : ValidatorQuadraticGapWaitParameters)
    {round : Nat}
    (referenceBeforeRound : parameters.referenceRound ≤ round) :
    parameters.gap (round + 1) = parameters.gap round + 1 := by
  simp only [gap]
  omega

/-- The adjacent wait difference is an affine function of the current gap. -/
theorem wait_succ
    (parameters : ValidatorQuadraticGapWaitParameters)
    {round : Nat}
    (referenceBeforeRound : parameters.referenceRound ≤ round) :
    parameters.wait (round + 1) =
      parameters.wait round + parameters.linearCoefficient +
        parameters.quadraticCoefficient *
          (2 * parameters.gap round + 1) := by
  rw [wait, wait, parameters.gap_succ referenceBeforeRound]
  exact quadraticGapWaitFromGap_succ _ _ _ _

/-- The adjacent difference eventually covers two linear costs.

The first cost can represent causal catch-up. The second cost can represent
timer-start spread and the remaining pipeline work. The strict slope condition
leaves at least one time unit of extra adjacent growth per gap unit. -/
theorem wait_adjacent_eventually_dominates_two_linear_costs
    (parameters : ValidatorQuadraticGapWaitParameters)
    (catchupBase catchupSlope spreadBase spreadSlope : Nat)
    (slopeCondition :
      catchupSlope + spreadSlope <
        2 * parameters.quadraticCoefficient) :
    ∃ firstRound,
      parameters.referenceRound ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            parameters.wait round +
                ((catchupBase + catchupSlope * parameters.gap round) +
                  (spreadBase + spreadSlope * parameters.gap round)) ≤
              parameters.wait (round + 1) := by
  let fixedCost := catchupBase + spreadBase
  let totalSlope := catchupSlope + spreadSlope
  let firstRound := parameters.referenceRound + fixedCost
  refine ⟨firstRound, by simp [firstRound], ?_⟩
  intro round firstBeforeRound
  have referenceBeforeRound : parameters.referenceRound ≤ round := by
    exact Nat.le_trans (by simp [firstRound]) firstBeforeRound
  have fixedWithinGap : fixedCost ≤ parameters.gap round := by
    simp only [firstRound, fixedCost] at firstBeforeRound ⊢
    simp only [gap]
    omega
  have slopeWithUnitAtMost : totalSlope + 1 ≤
      2 * parameters.quadraticCoefficient := by
    simp only [totalSlope]
    omega
  have linearCostWithinDoubleQuadratic :
      fixedCost + totalSlope * parameters.gap round ≤
        (2 * parameters.quadraticCoefficient) * parameters.gap round := by
    calc
      fixedCost + totalSlope * parameters.gap round ≤
          parameters.gap round + totalSlope * parameters.gap round :=
        Nat.add_le_add_right fixedWithinGap _
      _ = (totalSlope + 1) * parameters.gap round := by
        simp [Nat.add_mul, Nat.add_comm]
      _ ≤ (2 * parameters.quadraticCoefficient) *
          parameters.gap round :=
        Nat.mul_le_mul_right _ slopeWithUnitAtMost
  have doubleQuadraticWithinIncrement :
      (2 * parameters.quadraticCoefficient) * parameters.gap round ≤
        parameters.quadraticCoefficient *
          (2 * parameters.gap round + 1) := by
    calc
      (2 * parameters.quadraticCoefficient) * parameters.gap round =
          parameters.quadraticCoefficient *
            (2 * parameters.gap round) := by
        simp [Nat.mul_assoc, Nat.mul_comm]
      _ ≤ parameters.quadraticCoefficient *
          (2 * parameters.gap round + 1) := by
        exact Nat.mul_le_mul_left _ (by omega)
  have costsReassociate :
      (catchupBase + catchupSlope * parameters.gap round) +
          (spreadBase + spreadSlope * parameters.gap round) =
        fixedCost + totalSlope * parameters.gap round := by
    simp only [fixedCost, totalSlope, Nat.add_mul]
    omega
  rw [parameters.wait_succ referenceBeforeRound]
  rw [costsReassociate]
  have costsWithinQuadratic := Nat.le_trans linearCostWithinDoubleQuadratic
    doubleQuadraticWithinIncrement
  have costsWithinIncrement :
      fixedCost + totalSlope * parameters.gap round ≤
        parameters.linearCoefficient +
          parameters.quadraticCoefficient *
            (2 * parameters.gap round + 1) := by
    exact Nat.le_trans costsWithinQuadratic (Nat.le_add_left _ _)
  simpa [Nat.add_assoc] using
    (Nat.add_le_add_left costsWithinIncrement (parameters.wait round))

/-- The quadratic wait as one fixed-reference recovery schedule. -/
def commonSchedule
    (parameters : ValidatorQuadraticGapWaitParameters)
    (CommitPrefix : Type) : CommonRoundWaitSchedule CommitPrefix where
  wait := fun _ round => parameters.wait round
  permanentSuccessiveMargin := by
    intro _ bound
    have zeroSlope : 0 + 0 < 2 * parameters.quadraticCoefficient := by
      have quadraticPositive := parameters.quadraticPositive
      omega
    rcases parameters.wait_adjacent_eventually_dominates_two_linear_costs
        bound 0 0 0 zeroSlope with
      ⟨firstRound, referenceBeforeFirst, covered⟩
    exact ⟨firstRound, fun round firstBeforeRound => by
      have result := covered round firstBeforeRound
      simpa using result⟩

end ValidatorQuadraticGapWaitParameters

/-! The remaining result connects the concrete schedule to the existing causal
catch-up rate record. -/

/-- A bounded-increase causal cost has a linear bound from the envelope start
round. -/
theorem ValidatorCausalCatchupEnvelope.catchup_cost_linear_bound
    (envelope : ValidatorCausalCatchupEnvelope)
    (costPerWorkUnit : Nat)
    {round : Nat}
    (startBeforeRound : envelope.startRound ≤ round) :
    envelope.catchupCost costPerWorkUnit round ≤
      envelope.catchupCost costPerWorkUnit envelope.startRound +
        (round - envelope.startRound) *
          (envelope.newWorkPerRound * costPerWorkUnit) := by
  obtain ⟨offset, roundShape⟩ :=
    Nat.exists_eq_add_of_le startBeforeRound
  subst round
  have bounded := CommonRoundWaitSchedule.value_with_bounded_increase_has_linear_bound
    (envelope.catchupCost costPerWorkUnit) envelope.startRound
      (envelope.newWorkPerRound * costPerWorkUnit) offset
      (envelope.catchup_cost_has_bounded_increase costPerWorkUnit)
  simpa [Nat.add_sub_cancel_left] using bounded

/-- The concrete quadratic wait dominates the existing causal catch-up
envelope when its adjacent slope is larger than the envelope's linear growth
slope. -/
theorem quadratic_gap_wait_dominates_causal_catchup
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (referenceMatches :
      parameters.referenceRound = rate.envelope.startRound)
    (slopeCondition :
      rate.envelope.newWorkPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules <
        2 * parameters.quadraticCoefficient) :
    ValidatorCausalCatchupRateDominatesWait rate
      (parameters.commonSchedule (ValidatorCommitHead CommitId)) := by
  constructor
  intro commitHead fixedPipelineCost
  let costPerWorkUnit := validatorBlockSyncAcceptanceBound timed syncRules
  let catchupBase :=
    rate.envelope.catchupCost costPerWorkUnit rate.envelope.startRound
  let catchupSlope := rate.envelope.newWorkPerRound * costPerWorkUnit
  have concreteSlope : catchupSlope + 0 <
      2 * parameters.quadraticCoefficient := by
    simpa [catchupSlope, costPerWorkUnit] using slopeCondition
  rcases parameters.wait_adjacent_eventually_dominates_two_linear_costs
      (fixedPipelineCost + catchupBase) catchupSlope 0 0 concreteSlope with
    ⟨firstRound, referenceBeforeFirst, adjacent⟩
  refine ⟨firstRound, ?_, ?_⟩
  · simpa [referenceMatches] using referenceBeforeFirst
  · intro round firstBeforeRound
    have envelopeBeforeRound : rate.envelope.startRound ≤ round := by
      exact Nat.le_trans (by simpa [referenceMatches] using referenceBeforeFirst)
        firstBeforeRound
    have costBound := rate.envelope.catchup_cost_linear_bound costPerWorkUnit
      envelopeBeforeRound
    have adjacentBound := adjacent round firstBeforeRound
    simp only [catchupBase, catchupSlope,
      ValidatorQuadraticGapWaitParameters.gap, referenceMatches] at adjacentBound
    have totalCostBound :
        fixedPipelineCost + rate.envelope.catchupCost costPerWorkUnit round ≤
          fixedPipelineCost + catchupBase +
            catchupSlope * (round - rate.envelope.startRound) := by
      simp only [catchupBase, catchupSlope]
      have := Nat.add_le_add_left costBound fixedPipelineCost
      simpa [Nat.mul_comm, Nat.add_assoc] using this
    change parameters.wait round +
        (fixedPipelineCost +
          rate.envelope.catchupCost costPerWorkUnit round) ≤
      parameters.wait (round + 1)
    calc
      parameters.wait round +
          (fixedPipelineCost +
            rate.envelope.catchupCost costPerWorkUnit round) ≤
          parameters.wait round +
            (fixedPipelineCost + catchupBase +
              catchupSlope * (round - rate.envelope.startRound)) :=
        Nat.add_le_add_left totalCostBound _
      _ ≤ parameters.wait (round + 1) := by
        simpa [costPerWorkUnit, Nat.add_assoc] using adjacentBound

end Mysticeti
