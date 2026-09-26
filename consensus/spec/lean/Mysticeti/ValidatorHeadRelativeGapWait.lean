/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorGapWaitSchedule
import Mysticeti.ValidatorRecoveryTimerDerivation

namespace Mysticeti

/-! A head-relative recovery wait and adapters for the timer model.

The run-time policy uses `W (round - commitHead.round)`. The
`commonSchedule` adapter supplies this policy directly to the existing timer
types. The optional `freezeAtHead` adapter uses one fixed reference round. It
supports proof intervals where all relevant timer heads have that round.
-/

/-- A wait function on the gap from the local commit-head round.

The margin rule is only about one fixed head. It does not compare timers that
use different commit-head rounds. -/
structure ValidatorHeadRelativeGapWaitSchedule where
  waitFromGap : Nat → Time
  permanentAdjacentMargin : ∀ bound, ∃ firstGap, ∀ gap,
    firstGap ≤ gap →
      waitFromGap gap + bound ≤ waitFromGap (gap + 1)

namespace ValidatorHeadRelativeGapWaitSchedule

/-- The wait for one local commit head and one absolute target round. -/
def wait
    {CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    (commitHead : ValidatorCommitHead CommitId)
    (round : Nat) : Time :=
  schedule.waitFromGap (round - commitHead.round)

/-- Above the commit-head round, the next target has the next gap. -/
theorem gap_succ
    {CommitId : Type}
    (commitHead : ValidatorCommitHead CommitId)
    {round : Nat}
    (headBeforeRound : commitHead.round ≤ round) :
    round + 1 - commitHead.round =
      (round - commitHead.round) + 1 := by
  omega

/-- For one fixed head, a late adjacent wait difference covers each fixed
cost. -/
theorem eventual_same_head_adjacent_margin
    {CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    (commitHead : ValidatorCommitHead CommitId)
    (bound : Nat) :
    ∃ firstRound,
      commitHead.round ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            schedule.wait commitHead round + bound ≤
              schedule.wait commitHead (round + 1) := by
  rcases schedule.permanentAdjacentMargin bound with
    ⟨firstGap, margin⟩
  let firstRound := commitHead.round + firstGap
  refine ⟨firstRound, by simp [firstRound], ?_⟩
  intro round firstBeforeRound
  have headBeforeRound : commitHead.round ≤ round := by
    exact Nat.le_trans (by simp [firstRound]) firstBeforeRound
  have firstGapBeforeGap : firstGap ≤ round - commitHead.round := by
    simp only [firstRound] at firstBeforeRound
    omega
  have covered := margin (round - commitHead.round) firstGapBeforeGap
  simpa [wait, gap_succ commitHead headBeforeRound] using covered

/-- Supply the head-relative policy through the existing schedule type. -/
def commonSchedule
    {CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule) :
    CommonRoundWaitSchedule (ValidatorCommitHead CommitId) where
  wait := schedule.wait
  permanentSuccessiveMargin := by
    intro commitHead bound
    rcases schedule.eventual_same_head_adjacent_margin commitHead bound with
      ⟨firstRound, _, margin⟩
    exact ⟨firstRound, margin⟩

/-- Freeze the reference round. This optional adapter ignores the head argument
of the schedule type. -/
def freezeAtRound
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    (referenceRound : Nat)
    (CommitPrefix : Type) : CommonRoundWaitSchedule CommitPrefix where
  wait := fun _ round => schedule.waitFromGap (round - referenceRound)
  permanentSuccessiveMargin := by
    intro _ bound
    rcases schedule.permanentAdjacentMargin bound with
      ⟨firstGap, margin⟩
    let firstRound := referenceRound + firstGap
    refine ⟨firstRound, ?_⟩
    intro round firstBeforeRound
    have referenceBeforeRound : referenceRound ≤ round := by
      exact Nat.le_trans (by simp [firstRound]) firstBeforeRound
    have firstGapBeforeGap : firstGap ≤ round - referenceRound := by
      simp only [firstRound] at firstBeforeRound
      omega
    have covered := margin (round - referenceRound) firstGapBeforeGap
    have nextGap : round + 1 - referenceRound =
        (round - referenceRound) + 1 := by
      omega
    simpa [nextGap] using covered

/-- Freeze a head-relative schedule at one exact commit head. -/
def freezeAtHead
    {CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    (commitHead : ValidatorCommitHead CommitId) :
    CommonRoundWaitSchedule (ValidatorCommitHead CommitId) :=
  schedule.freezeAtRound commitHead.round (ValidatorCommitHead CommitId)

/-- The frozen adapter gives the exact run-time value for its fixed head. The
other head argument cannot change this frozen value. -/
@[simp]
theorem freezeAtHead_wait_eq
    {CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    (commitHead otherHead : ValidatorCommitHead CommitId)
    (round : Nat) :
    (schedule.freezeAtHead commitHead).wait otherHead round =
      schedule.wait commitHead round := by
  rfl

/-- The frozen adapter also gives the exact run-time value for a different
head with the same commit round. -/
theorem freezeAtHead_wait_eq_of_round_eq
    {CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    (fixedHead timerHead : ValidatorCommitHead CommitId)
    {round : Nat}
    (sameHeadRound : timerHead.round = fixedHead.round) :
    (schedule.freezeAtHead fixedHead).wait timerHead round =
      schedule.wait timerHead round := by
  simp only [freezeAtHead, freezeAtRound, wait]
  rw [sameHeadRound]

/-- Existing `RecoveryTargetTimer` values need no new timer type. On a fixed
head interval, only the schedule argument changes. -/
theorem recovery_target_deadline_freeze_eq
    {CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound : Nat}
    (timer : RecoveryTargetTimer commitHead targetRound) :
    timer.deadline (schedule.freezeAtHead commitHead) =
      timer.startedAt + schedule.wait commitHead targetRound := by
  rfl

/-- Existing `RecoveryTargetTimer` values use the head-relative policy without
a timer-type conversion. -/
theorem recovery_target_deadline_eq
    {CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound : Nat}
    (timer : RecoveryTargetTimer commitHead targetRound) :
    timer.deadline schedule.commonSchedule =
      timer.startedAt + schedule.wait commitHead targetRound := by
  rfl

/-- The execution timer-start record uses the same frozen deadline. -/
theorem recovery_timer_start_deadline_freeze_eq
    {BlockId CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    (start : ValidatorRecoveryTimerStart BlockId CommitId) :
    start.deadline (schedule.freezeAtHead start.commitHead) =
      start.startedAt + schedule.wait start.commitHead start.targetRound := by
  rfl

/-- The execution timer-start record also uses the direct head-relative
schedule without a record conversion. -/
theorem recovery_timer_start_deadline_eq
    {BlockId CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    (start : ValidatorRecoveryTimerStart BlockId CommitId) :
    start.deadline schedule.commonSchedule =
      start.startedAt + schedule.wait start.commitHead start.targetRound := by
  rfl

/-- The removed head-independent rule is not compatible with a head-relative
policy when two heads have different wait values at one absolute round. -/
theorem no_global_head_independent_adapter_when_head_waits_differ
    {CommitId : Type}
    (schedule : ValidatorHeadRelativeGapWaitSchedule)
    (leftHead rightHead : ValidatorCommitHead CommitId)
    (round : Nat)
    (waitsDiffer :
      schedule.wait leftHead round ≠ schedule.wait rightHead round) :
    ¬ ∃ common : CommonRoundWaitSchedule (ValidatorCommitHead CommitId),
        (∀ left right targetRound,
          common.wait left targetRound = common.wait right targetRound) ∧
          ∀ commitHead targetRound,
            common.wait commitHead targetRound =
              schedule.wait commitHead targetRound := by
  rintro ⟨common, headIndependent, agrees⟩
  apply waitsDiffer
  calc
    schedule.wait leftHead round = common.wait leftHead round :=
      (agrees leftHead round).symm
    _ = common.wait rightHead round :=
      headIndependent leftHead rightHead round
    _ = schedule.wait rightHead round := agrees rightHead round

/-- If the voter head is one round ahead, both adjacent timers use the same
gap. No gap wait can then cover a positive flow cost. -/
theorem one_round_ahead_has_no_positive_adjacent_margin
    (waitFromGap : Nat → Time)
    {senderHeadRound voterHeadRound round cost : Nat}
    (senderHeadBeforeRound : senderHeadRound ≤ round)
    (voterOneRoundAhead : voterHeadRound = senderHeadRound + 1)
    (costPositive : 0 < cost) :
    ¬ waitFromGap (round - senderHeadRound) + cost ≤
        waitFromGap (round + 1 - voterHeadRound) := by
  have sameGap : round + 1 - voterHeadRound =
      round - senderHeadRound := by
    omega
  rw [sameGap]
  intro covered
  have oneBeforeCost : 1 ≤ cost := by omega
  have successorCovered :
      waitFromGap (round - senderHeadRound) + 1 ≤
        waitFromGap (round - senderHeadRound) := by
    exact Nat.le_trans
      (Nat.add_le_add_left oneBeforeCost
        (waitFromGap (round - senderHeadRound)))
      covered
  exact Nat.not_succ_le_self _ (by simpa using successorCovered)

end ValidatorHeadRelativeGapWaitSchedule

/-! A concrete quadratic instance of the head-relative interface. -/

/-- Parameters for `W (round - commitHead.round)`. -/
structure ValidatorHeadRelativeQuadraticWaitParameters where
  baseWait : Time
  linearCoefficient : Nat
  quadraticCoefficient : Nat
  quadraticPositive : 0 < quadraticCoefficient

namespace ValidatorHeadRelativeQuadraticWaitParameters

/-- Reuse the fixed-reference quadratic proof at one reference round. -/
def atReference
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    (referenceRound : Nat) : ValidatorQuadraticGapWaitParameters where
  referenceRound
  baseWait := parameters.baseWait
  linearCoefficient := parameters.linearCoefficient
  quadraticCoefficient := parameters.quadraticCoefficient
  quadraticPositive := parameters.quadraticPositive

/-- The run-time wait for one head and target round. -/
def wait
    {CommitId : Type}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    (commitHead : ValidatorCommitHead CommitId)
    (round : Nat) : Time :=
  quadraticGapWaitFromGap parameters.baseWait parameters.linearCoefficient
    parameters.quadraticCoefficient (round - commitHead.round)

/-- The quadratic policy as a head-relative gap schedule. -/
def schedule
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters) :
    ValidatorHeadRelativeGapWaitSchedule where
  waitFromGap := quadraticGapWaitFromGap parameters.baseWait
    parameters.linearCoefficient parameters.quadraticCoefficient
  permanentAdjacentMargin := by
    intro bound
    let fixed := parameters.atReference 0
    have positiveSlope : 0 + 0 < 2 * fixed.quadraticCoefficient := by
      simp only [fixed, atReference]
      have quadraticPositive := parameters.quadraticPositive
      omega
    rcases fixed.wait_adjacent_eventually_dominates_two_linear_costs
        bound 0 0 0 positiveSlope with
      ⟨firstGap, _, covered⟩
    refine ⟨firstGap, ?_⟩
    intro gap firstBeforeGap
    have result := covered gap firstBeforeGap
    simpa [fixed, atReference,
      ValidatorQuadraticGapWaitParameters.wait,
      ValidatorQuadraticGapWaitParameters.gap] using result

/-- The concrete quadratic schedule uses the same wait value as its parameter
record for each head and round. -/
@[simp]
theorem schedule_wait_eq
    {CommitId : Type}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    (commitHead : ValidatorCommitHead CommitId)
    (round : Nat) :
    parameters.schedule.wait commitHead round =
      parameters.wait commitHead round := by
  rfl

/-- This is the exact same-head adjacent dominance result for the concrete
quadratic wait. The two cost slopes must be smaller than the adjacent
quadratic slope. -/
theorem wait_adjacent_eventually_dominates_two_linear_costs
    {CommitId : Type}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    (commitHead : ValidatorCommitHead CommitId)
    (catchupBase catchupSlope spreadBase spreadSlope : Nat)
    (slopeCondition :
      catchupSlope + spreadSlope <
        2 * parameters.quadraticCoefficient) :
    ∃ firstRound,
      commitHead.round ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            parameters.wait commitHead round +
                ((catchupBase +
                    catchupSlope * (round - commitHead.round)) +
                  (spreadBase +
                    spreadSlope * (round - commitHead.round))) ≤
              parameters.wait commitHead (round + 1) := by
  let fixed := parameters.atReference commitHead.round
  have concreteSlope : catchupSlope + spreadSlope <
      2 * fixed.quadraticCoefficient := by
    simpa [fixed, atReference] using slopeCondition
  rcases fixed.wait_adjacent_eventually_dominates_two_linear_costs
      catchupBase catchupSlope spreadBase spreadSlope concreteSlope with
    ⟨firstRound, headBeforeFirst, covered⟩
  refine ⟨firstRound, by simpa [fixed, atReference] using headBeforeFirst, ?_⟩
  intro round firstBeforeRound
  have result := covered round firstBeforeRound
  simpa [fixed, atReference, wait,
    ValidatorQuadraticGapWaitParameters.wait,
    ValidatorQuadraticGapWaitParameters.gap] using result

end ValidatorHeadRelativeQuadraticWaitParameters

end Mysticeti
