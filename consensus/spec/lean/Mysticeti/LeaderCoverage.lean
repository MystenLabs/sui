/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.FlexCommitter
import Mysticeti.ValidatorProcess

namespace Mysticeti

/-! A deterministic first-slot coverage rule for a multi-leader schedule.

The rule keeps every schedule member selected in every round. It changes only
which selected leader slot is first. Each member is first for one complete
positive-length window before the rule moves to the next member.
-/

/-- A finite leader schedule and the selected validator set that it represents. -/
structure DeterministicLeaderSchedule (authorityCount : Nat) where
  memberCount : Nat
  memberCountPositive : 0 < memberCount
  memberAt : Fin memberCount → Nat
  memberInRange : ∀ index, memberAt index < authorityCount
  membersDistinct : Function.Injective memberAt
  selected : VoterSet
  selectedIffMember : ∀ authority, authority < authorityCount →
    (selected authority = true ↔ ∃ index, memberAt index = authority)

/-- The selected validator set does not depend on the round. -/
def repeatedFirstSelection {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount) (_round : Nat) :
    VoterSet :=
  schedule.selected

/-- The index in the first selected leader slot. A group of `windowLength`
rounds uses one index. The groups visit all schedule indexes in order. -/
def repeatedFirstIndex {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (windowLength round : Nat) : Fin schedule.memberCount :=
  ⟨(round / windowLength) % schedule.memberCount,
    Nat.mod_lt _ schedule.memberCountPositive⟩

/-- The validator in the first selected leader slot. -/
def repeatedFirstLeader {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (windowLength round : Nat) : Nat :=
  schedule.memberAt (repeatedFirstIndex schedule windowLength round)

/-- Reordering the first slot does not change the selected validator set. -/
theorem repeated_first_keeps_selected_set
    {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (round : Nat) :
    repeatedFirstSelection schedule round = schedule.selected :=
  rfl

/-- Reordering the first slot does not change the total selected stake. -/
theorem repeated_first_keeps_selected_stake
    {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (stake : Nat → Nat) (round : Nat) :
    weight authorityCount stake (repeatedFirstSelection schedule round) =
      weight authorityCount stake schedule.selected :=
  rfl

/-- The first leader is a selected schedule member in every round. -/
theorem repeated_first_leader_is_selected
    {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (windowLength round : Nat) :
    schedule.selected (repeatedFirstLeader schedule windowLength round) = true := by
  have selectedIff := schedule.selectedIffMember
    (authority := repeatedFirstLeader schedule windowLength round)
    (schedule.memberInRange (repeatedFirstIndex schedule windowLength round))
  exact selectedIff.mpr
    ⟨repeatedFirstIndex schedule windowLength round, rfl⟩

/-- One schedule member stays first for all rounds in its assigned window. -/
theorem repeated_first_covers_member
    {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    {windowLength : Nat} (windowLengthPositive : 0 < windowLength)
    (cycle : Nat) (member : Fin schedule.memberCount) :
    let base :=
      (cycle * schedule.memberCount + member.val) * windowLength
    ∀ offset, offset < windowLength →
      repeatedFirstLeader schedule windowLength (base + offset) =
        schedule.memberAt member := by
  intro base offset offsetInWindow
  unfold repeatedFirstLeader repeatedFirstIndex
  apply congrArg schedule.memberAt
  apply Fin.ext
  change (base + offset) / windowLength % schedule.memberCount = member.val
  have quotient :
      (base + offset) / windowLength =
        cycle * schedule.memberCount + member.val := by
    simp only [base]
    rw [Nat.mul_comm
      (cycle * schedule.memberCount + member.val) windowLength]
    rw [Nat.mul_add_div windowLengthPositive]
    simp [Nat.div_eq_of_lt offsetInWindow]
  rw [quotient]
  simp [Nat.add_mod, Nat.mod_eq_of_lt member.isLt]

/-- A favorable window has a progressing validator in the first selected leader
slot in each round. -/
def FavorableFirstLeaderWindow {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (progressing : VoterSet) (windowLength base : Nat) : Prop :=
  ∀ offset, offset < windowLength →
    progressing (repeatedFirstLeader schedule windowLength (base + offset)) = true

/-- If the schedule contains one progressing member, the deterministic order gives
that member a complete favorable window after any start round. -/
theorem repeated_first_has_favorable_window_after
    {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (progressing : VoterSet)
    {windowLength : Nat} (windowLengthPositive : 0 < windowLength)
    (containsProgressing :
      ∃ member, progressing (schedule.memberAt member) = true)
    (start : Nat) :
    ∃ base, start ≤ base ∧
      FavorableFirstLeaderWindow schedule progressing windowLength base := by
  rcases containsProgressing with ⟨member, memberProgressing⟩
  let cycle := start + 1
  let base :=
    (cycle * schedule.memberCount + member.val) * windowLength
  have cycleLeGroups :
      cycle ≤ cycle * schedule.memberCount :=
    Nat.le_mul_of_pos_right cycle schedule.memberCountPositive
  have groupsLeMemberGroup :
      cycle * schedule.memberCount ≤
        cycle * schedule.memberCount + member.val := by
    omega
  have memberGroupLeBase :
      cycle * schedule.memberCount + member.val ≤ base := by
    exact Nat.le_mul_of_pos_right _ windowLengthPositive
  have startLeBase : start ≤ base := by
    have startLtCycle : start < cycle := by simp [cycle]
    omega
  refine ⟨base, startLeBase, ?_⟩
  intro offset offsetInWindow
  have covered := repeated_first_covers_member schedule windowLengthPositive
    cycle member offset offsetInWindow
  rw [covered]
  exact memberProgressing

/-- For indirect depth `d`, the rule gives the progressing member `d + 1`
consecutive first-slot rounds. -/
theorem repeated_first_has_depth_window_after
    {authorityCount : Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (progressing : VoterSet)
    (depth : Nat)
    (containsProgressing :
      ∃ member, progressing (schedule.memberAt member) = true)
    (start : Nat) :
    ∃ base, start ≤ base ∧
      FavorableFirstLeaderWindow schedule progressing (depth + 1) base := by
  exact repeated_first_has_favorable_window_after schedule progressing
    (windowLength := depth + 1) (by omega) containsProgressing start

/-- A leader schedule whose stake is greater than the Byzantine and unavailable
stake budgets contains a correct, available member. -/
theorem viable_schedule_contains_correct_available_member
    {CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (schedule : DeterministicLeaderSchedule config.authorityCount)
    (scheduleViable :
      config.thresholds.fault + faults.unavailableStakeBound <
        weight config.authorityCount config.stake schedule.selected) :
    ∃ member,
      faults.correctAvailable (schedule.memberAt member) = true := by
  have nonProgressBound := faults.non_progress_stake_bounded
  have selectedNonProgressBound :
      weight config.authorityCount config.stake
          (VoterSet.inter schedule.selected faults.nonProgress) ≤
        weight config.authorityCount config.stake faults.nonProgress := by
    exact weight_mono config.stake
      (VoterSet.inter_subset_right config.authorityCount schedule.selected
        faults.nonProgress)
  have partition := weight_diff_add_inter config.authorityCount config.stake
    schedule.selected faults.nonProgress
  have progressStakePositive :
      0 < weight config.authorityCount config.stake
        (VoterSet.diff schedule.selected faults.nonProgress) := by
    omega
  rcases positive_weight_has_member progressStakePositive with
    ⟨validator, validatorInRange, validatorSelectedAndLive, _positiveStake⟩
  have selectedAndAvailable :
      schedule.selected validator = true ∧
        faults.nonProgress validator = false := by
    simpa [VoterSet.diff] using validatorSelectedAndLive
  have validatorSelected : schedule.selected validator = true := by
    exact selectedAndAvailable.1
  have validatorAvailable : faults.correctAvailable validator = true := by
    simpa [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full]
      using selectedAndAvailable.2
  rcases (schedule.selectedIffMember validator validatorInRange).mp
      validatorSelected with ⟨member, memberIsValidator⟩
  exact ⟨member, by simpa [memberIsValidator] using validatorAvailable⟩

end Mysticeti
