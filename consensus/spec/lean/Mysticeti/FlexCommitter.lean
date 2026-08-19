/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.Leader

namespace Mysticeti

/-! An executable status-level model of the Mysticeti v3 `FlexCommitter` scan.

The model contains no network or timing assumption. An indirect decision can choose
either final result for an open round. The proofs hold for every such choice.
-/

/-- The state of one selected leader slot in the order used by the committer. -/
inductive SelectedLeaderSlotStatus where
  | undecided
  | commit
  | skip
  deriving DecidableEq, Repr

/-- One selected leader slot and its status at its position in the Rust scan. -/
structure SelectedLeaderSlotView where
  validator : Nat
  status : SelectedLeaderSlotStatus
  deriving DecidableEq, Repr

/-- Every selected leader slot in the round has a final commit-or-skip result. -/
def AllSelectedLeaderSlotsFinal
    (statuses : List SelectedLeaderSlotStatus) : Prop :=
  .undecided ∉ statuses

/-- At least one selected leader slot in the round has a commit result. -/
def HasCommitResultInSelectedLeaderSlot
    (statuses : List SelectedLeaderSlotStatus) : Prop :=
  .commit ∈ statuses

/-- One round's ordered scan fragment finds a commit before an undecided slot.

Rust concatenates this scan fragment with the selected leader slot orders for later
pending rounds. A skip lets the scan continue. A commit supplies an anchor. An
undecided slot stops the scan. -/
def UsableAnchorOrder : List SelectedLeaderSlotStatus → Prop
  | [] => False
  | .commit :: _ => True
  | .undecided :: _ => False
  | .skip :: tail => UsableAnchorOrder tail

/-- Full finality plus one selected leader slot with a commit result is a simple,
order-independent sufficient condition for a usable anchor. -/
theorem all_selected_leader_slots_final_with_commit_is_usable
    {statuses : List SelectedLeaderSlotStatus}
    (allFinal : AllSelectedLeaderSlotsFinal statuses)
    (hasCommit : HasCommitResultInSelectedLeaderSlot statuses) :
    UsableAnchorOrder statuses := by
  induction statuses with
  | nil => simp [HasCommitResultInSelectedLeaderSlot] at hasCommit
  | cons head tail ih =>
      cases head with
      | undecided =>
          exact False.elim (allFinal (by simp))
      | commit => simp [UsableAnchorOrder]
      | skip =>
          simp only [UsableAnchorOrder]
          apply ih
          · intro member
            exact allFinal (by simp [member])
          · simpa [HasCommitResultInSelectedLeaderSlot] using hasCommit

/-- A usable scan fragment can still contain an undecided slot after its first
commit. Thus, usable anchors for older rounds do not by themselves make the anchor
round eligible for commit construction. -/
theorem usable_anchor_order_need_not_be_fully_final :
    UsableAnchorOrder [.commit, .undecided] ∧
      ¬AllSelectedLeaderSlotsFinal [.commit, .undecided] := by
  constructor
  · simp [UsableAnchorOrder]
  · intro allFinal
    exact allFinal (by simp)

/-- The information about one pending leader round that affects the two ordered
FlexCommitter scans. -/
inductive FlexRoundStatus where
  /-- The anchor scan reaches an undecided selected leader slot before a commit. -/
  | blockedWithoutCommit
  /-- The scan is blocked, but a later selected leader slot has a commit result. -/
  | blockedWithCommit
  /-- The scan finds a commit result before its first undecided slot. -/
  | usableAnchor
  /-- Every selected leader slot is final and all results are Skip. -/
  | finalSkip
  /-- Every selected leader slot is final and at least one result is Commit. -/
  | finalCommit
  deriving DecidableEq, Repr

namespace FlexRoundStatus

def IsAnchor : FlexRoundStatus → Prop
  | .usableAnchor | .finalCommit => True
  | _ => False

def IsFinal : FlexRoundStatus → Prop
  | .finalSkip | .finalCommit => True
  | _ => False

def HasCommitResult : FlexRoundStatus → Prop
  | .blockedWithCommit | .usableAnchor | .finalCommit => True
  | _ => False

end FlexRoundStatus

def selectedLeaderSlotsHaveCommit : List SelectedLeaderSlotStatus → Bool
  | [] => false
  | .commit :: _ => true
  | _ :: tail => selectedLeaderSlotsHaveCommit tail

def selectedLeaderSlotsAreFinal : List SelectedLeaderSlotStatus → Bool
  | [] => true
  | .undecided :: _ => false
  | _ :: tail => selectedLeaderSlotsAreFinal tail

theorem selected_leader_slots_have_commit_iff
    (statuses : List SelectedLeaderSlotStatus) :
    selectedLeaderSlotsHaveCommit statuses = true ↔
      HasCommitResultInSelectedLeaderSlot statuses := by
  induction statuses with
  | nil => simp [selectedLeaderSlotsHaveCommit,
      HasCommitResultInSelectedLeaderSlot]
  | cons head tail ih =>
      cases head <;>
        simp [selectedLeaderSlotsHaveCommit,
          HasCommitResultInSelectedLeaderSlot, ih]

theorem selected_leader_slots_are_final_iff
    (statuses : List SelectedLeaderSlotStatus) :
    selectedLeaderSlotsAreFinal statuses = true ↔
      AllSelectedLeaderSlotsFinal statuses := by
  induction statuses with
  | nil => simp [selectedLeaderSlotsAreFinal, AllSelectedLeaderSlotsFinal]
  | cons head tail ih =>
      cases head <;>
        simp [selectedLeaderSlotsAreFinal, AllSelectedLeaderSlotsFinal, ih]

/-- Convert one ordered selected leader slot list to the information used by the
round-level proof model. -/
def summarizeFlexRound : List SelectedLeaderSlotStatus → FlexRoundStatus
  | [] => .finalSkip
  | .skip :: tail => summarizeFlexRound tail
  | .undecided :: tail =>
      if selectedLeaderSlotsHaveCommit tail then
        .blockedWithCommit
      else
        .blockedWithoutCommit
  | .commit :: tail =>
      if selectedLeaderSlotsAreFinal tail then
        .finalCommit
      else
        .usableAnchor

/-- The round summary reports an anchor exactly when the Rust order finds a commit
before its first undecided selected leader slot. -/
theorem summarize_flex_round_is_anchor_iff
    (statuses : List SelectedLeaderSlotStatus) :
    (summarizeFlexRound statuses).IsAnchor ↔ UsableAnchorOrder statuses := by
  induction statuses with
  | nil => simp [summarizeFlexRound, FlexRoundStatus.IsAnchor, UsableAnchorOrder]
  | cons head tail ih =>
      cases head
      · cases selected : selectedLeaderSlotsHaveCommit tail <;>
          simp [summarizeFlexRound, selected, FlexRoundStatus.IsAnchor,
            UsableAnchorOrder]
      · cases selected : selectedLeaderSlotsAreFinal tail <;>
          simp [summarizeFlexRound, selected, FlexRoundStatus.IsAnchor,
            UsableAnchorOrder]
      · simpa [summarizeFlexRound, UsableAnchorOrder] using ih

/-- The round summary is final exactly when no selected leader slot is undecided. -/
theorem summarize_flex_round_is_final_iff
    (statuses : List SelectedLeaderSlotStatus) :
    (summarizeFlexRound statuses).IsFinal ↔
      AllSelectedLeaderSlotsFinal statuses := by
  induction statuses with
  | nil => simp [summarizeFlexRound, FlexRoundStatus.IsFinal,
      AllSelectedLeaderSlotsFinal]
  | cons head tail ih =>
      cases head
      · cases selected : selectedLeaderSlotsHaveCommit tail <;>
          simp [summarizeFlexRound, selected, FlexRoundStatus.IsFinal,
            AllSelectedLeaderSlotsFinal]
      · cases selected : selectedLeaderSlotsAreFinal tail
        · have notFinal : ¬AllSelectedLeaderSlotsFinal tail := by
            intro final
            have finalBool :=
              (selected_leader_slots_are_final_iff tail).2 final
            simp [selected] at finalBool
          have undecided : SelectedLeaderSlotStatus.undecided ∈ tail := by
            by_cases present : SelectedLeaderSlotStatus.undecided ∈ tail
            · exact present
            · exact False.elim (notFinal present)
          simp [summarizeFlexRound, selected, FlexRoundStatus.IsFinal,
            AllSelectedLeaderSlotsFinal, undecided]
        · have final : AllSelectedLeaderSlotsFinal tail :=
            (selected_leader_slots_are_final_iff tail).1 selected
          have noUndecided : SelectedLeaderSlotStatus.undecided ∉ tail := final
          simp [summarizeFlexRound, selected, FlexRoundStatus.IsFinal,
            AllSelectedLeaderSlotsFinal, noUndecided]
      · simpa [summarizeFlexRound, AllSelectedLeaderSlotsFinal] using ih

/-- The final-commit summary is exact: all selected leader slots are final and at
least one slot has a commit result. -/
theorem summarize_flex_round_eq_final_commit_iff
    (statuses : List SelectedLeaderSlotStatus) :
    summarizeFlexRound statuses = .finalCommit ↔
      AllSelectedLeaderSlotsFinal statuses ∧
        HasCommitResultInSelectedLeaderSlot statuses := by
  induction statuses with
  | nil => simp [summarizeFlexRound, AllSelectedLeaderSlotsFinal,
      HasCommitResultInSelectedLeaderSlot]
  | cons head tail ih =>
      cases head
      · cases selected : selectedLeaderSlotsHaveCommit tail <;>
          simp [summarizeFlexRound, selected, AllSelectedLeaderSlotsFinal,
            HasCommitResultInSelectedLeaderSlot]
      · cases selected : selectedLeaderSlotsAreFinal tail
        · have notFinal : ¬AllSelectedLeaderSlotsFinal tail := by
            intro final
            have finalBool :=
              (selected_leader_slots_are_final_iff tail).2 final
            simp [selected] at finalBool
          have undecided : SelectedLeaderSlotStatus.undecided ∈ tail := by
            by_cases present : SelectedLeaderSlotStatus.undecided ∈ tail
            · exact present
            · exact False.elim (notFinal present)
          simp [summarizeFlexRound, selected, AllSelectedLeaderSlotsFinal,
            HasCommitResultInSelectedLeaderSlot, undecided]
        · have final : AllSelectedLeaderSlotsFinal tail :=
            (selected_leader_slots_are_final_iff tail).1 selected
          have noUndecided : SelectedLeaderSlotStatus.undecided ∉ tail := final
          simp [summarizeFlexRound, selected, AllSelectedLeaderSlotsFinal,
            HasCommitResultInSelectedLeaderSlot, noUndecided]
      · simpa [summarizeFlexRound, AllSelectedLeaderSlotsFinal,
          HasCommitResultInSelectedLeaderSlot] using ih

/-- The round summary records whether any selected leader slot already has a
commit result. -/
theorem summarize_flex_round_has_commit_iff
    (statuses : List SelectedLeaderSlotStatus) :
    (summarizeFlexRound statuses).HasCommitResult ↔
      HasCommitResultInSelectedLeaderSlot statuses := by
  induction statuses with
  | nil => simp [summarizeFlexRound, FlexRoundStatus.HasCommitResult,
      HasCommitResultInSelectedLeaderSlot]
  | cons head tail ih =>
      cases head
      · cases selected : selectedLeaderSlotsHaveCommit tail
        · have noCommit : ¬HasCommitResultInSelectedLeaderSlot tail := by
            intro hasCommit
            have commitBool :=
              (selected_leader_slots_have_commit_iff tail).2 hasCommit
            simp [selected] at commitBool
          have noCommitMembership : SelectedLeaderSlotStatus.commit ∉ tail :=
            noCommit
          simp [summarizeFlexRound, selected, FlexRoundStatus.HasCommitResult,
            HasCommitResultInSelectedLeaderSlot, noCommitMembership]
        · have hasCommit : HasCommitResultInSelectedLeaderSlot tail :=
            (selected_leader_slots_have_commit_iff tail).1 selected
          have commitMembership : SelectedLeaderSlotStatus.commit ∈ tail :=
            hasCommit
          simp [summarizeFlexRound, selected, FlexRoundStatus.HasCommitResult,
            HasCommitResultInSelectedLeaderSlot, commitMembership]
      · cases selected : selectedLeaderSlotsAreFinal tail <;>
          simp [summarizeFlexRound, selected, FlexRoundStatus.HasCommitResult,
            HasCommitResultInSelectedLeaderSlot]
      · simpa [summarizeFlexRound, HasCommitResultInSelectedLeaderSlot] using ih

/-- The possible final result of the indirect decision rule for an open round that
does not already contain a commit result. -/
inductive IndirectRoundOutcome where
  | skip
  | commit
  deriving DecidableEq, Repr

/-- Finish one pending round. Existing commit results are preserved. -/
def finishFlexRound
    (status : FlexRoundStatus) (outcome : IndirectRoundOutcome) :
    FlexRoundStatus :=
  match status with
  | .blockedWithoutCommit =>
      match outcome with
      | .skip => .finalSkip
      | .commit => .finalCommit
  | .blockedWithCommit | .usableAnchor => .finalCommit
  | .finalSkip => .finalSkip
  | .finalCommit => .finalCommit

theorem finish_flex_round_is_final (status : FlexRoundStatus)
    (outcome : IndirectRoundOutcome) :
    (finishFlexRound status outcome).IsFinal := by
  cases status <;> cases outcome <;> simp [finishFlexRound, FlexRoundStatus.IsFinal]

/-- Indirect decisions preserve every existing commit result. -/
theorem finish_flex_round_preserves_commit
    {status : FlexRoundStatus} (hasCommit : status.HasCommitResult)
    (outcome : IndirectRoundOutcome) :
    (finishFlexRound status outcome).HasCommitResult := by
  cases status <;> cases outcome <;>
    simp [finishFlexRound, FlexRoundStatus.HasCommitResult] at hasCommit ⊢

theorem finish_flex_anchor_is_final_commit
    {status : FlexRoundStatus} (anchor : status.IsAnchor)
    (outcome : IndirectRoundOutcome) :
    finishFlexRound status outcome = .finalCommit := by
  cases status <;> simp [FlexRoundStatus.IsAnchor] at anchor <;>
    simp [finishFlexRound]

/-- The result of scanning a finite suffix for an anchor. -/
def findFlexAnchorFrom
    (rounds : Nat → FlexRoundStatus) : Nat → Nat → Bool
  | _, 0 => false
  | round, fuel + 1 =>
      match rounds round with
      | .usableAnchor | .finalCommit => true
      | .finalSkip => findFlexAnchorFrom rounds (round + 1) fuel
      | .blockedWithoutCommit | .blockedWithCommit => false

theorem find_flex_anchor_at_start
    {rounds : Nat → FlexRoundStatus} {start fuel : Nat}
    (fuelPositive : 0 < fuel) (anchor : (rounds start).IsAnchor) :
    findFlexAnchorFrom rounds start fuel = true := by
  cases fuel with
  | zero => omega
  | succ remaining =>
      cases status : rounds start <;>
        simp [FlexRoundStatus.IsAnchor, status] at anchor <;>
        simp [findFlexAnchorFrom, status]

/-- Final rounds cannot block a scan that ends at a final commit round. -/
theorem find_flex_anchor_after_final_prefix
    {rounds : Nat → FlexRoundStatus} {start : Nat}
    (distance fuel : Nat)
    (fuelCoversEnd : distance < fuel)
    (finalBefore : ∀ offset, offset < distance →
      (rounds (start + offset)).IsFinal)
    (commitAtEnd : rounds (start + distance) = .finalCommit) :
    findFlexAnchorFrom rounds start fuel = true := by
  induction distance generalizing start fuel with
  | zero =>
      have atStart : rounds start = .finalCommit := by
        simpa using commitAtEnd
      cases fuel with
      | zero => omega
      | succ remaining => simp [findFlexAnchorFrom, atStart]
  | succ distance ih =>
      cases fuel with
      | zero => omega
      | succ remaining =>
      have firstFinal := finalBefore 0 (by omega)
      cases firstStatus : rounds start with
      | blockedWithoutCommit | blockedWithCommit | usableAnchor =>
          simp [firstStatus, FlexRoundStatus.IsFinal] at firstFinal
      | finalCommit =>
          simp [findFlexAnchorFrom, firstStatus]
      | finalSkip =>
          rw [findFlexAnchorFrom, firstStatus]
          have tailFinal : ∀ offset, offset < distance →
              (rounds (start + 1 + offset)).IsFinal := by
            intro offset beforeEnd
            have nextFinal := finalBefore (offset + 1) (by omega)
            simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using nextFinal
          have tailCommit : rounds (start + 1 + distance) = .finalCommit := by
            simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using commitAtEnd
          exact ih (start := start + 1) remaining (by omega) tailFinal tailCommit

/-- The finite pending state used by the status-level FlexCommitter model. Index
zero is Rust's first pending leader round. Indexes at or above `roundCount` are not
scanned. -/
structure FlexCommitState where
  commitIndex : Nat
  roundCount : Nat
  rounds : Nat → FlexRoundStatus

def setFlexRound
    (rounds : Nat → FlexRoundStatus) (target : Nat)
    (status : FlexRoundStatus) : Nat → FlexRoundStatus :=
  fun round => if round = target then status else rounds round

@[simp]
theorem set_flex_round_same
    (rounds : Nat → FlexRoundStatus) (target : Nat)
    (status : FlexRoundStatus) :
    setFlexRound rounds target status target = status := by
  simp [setFlexRound]

@[simp]
theorem set_flex_round_other
    (rounds : Nat → FlexRoundStatus) (target round : Nat)
    (status : FlexRoundStatus) (different : round ≠ target) :
    setFlexRound rounds target status round = rounds round := by
  simp [setFlexRound, different]

/-- One Rust `decide_with_anchor_block` step at the status level. -/
def flexIndirectStep
    (depth : Nat)
    (outcome : Nat → IndirectRoundOutcome)
    (decisionRound : Nat)
    (state : FlexCommitState) : FlexCommitState :=
  let anchorRound := decisionRound + depth
  let anchorFuel := state.roundCount - anchorRound
  if findFlexAnchorFrom state.rounds anchorRound anchorFuel then
    { state with
      rounds := setFlexRound state.rounds decisionRound
        (finishFlexRound (state.rounds decisionRound) (outcome decisionRound)) }
  else
    state

/-- Apply `stepCount` indirect decisions in descending order from `highestRound`.
For valid calls, `stepCount ≤ highestRound + 1`. -/
def runFlexIndirectDescending
    (depth : Nat)
    (outcome : Nat → IndirectRoundOutcome)
    (highestRound : Nat) : Nat → FlexCommitState → FlexCommitState
  | 0, state => state
  | stepCount + 1, state =>
      let previous := runFlexIndirectDescending depth outcome highestRound stepCount state
      flexIndirectStep depth outcome (highestRound - stepCount) previous

/-- Split one descending scan after `prefixCount` steps. -/
theorem run_flex_indirect_descending_append
    (depth : Nat)
    (outcome : Nat → IndirectRoundOutcome)
    (highestRound prefixCount suffixCount : Nat)
    (state : FlexCommitState) :
    runFlexIndirectDescending depth outcome highestRound
        (prefixCount + suffixCount) state =
      runFlexIndirectDescending depth outcome (highestRound - prefixCount)
        suffixCount
        (runFlexIndirectDescending depth outcome highestRound prefixCount state) := by
  induction suffixCount with
  | zero => simp [runFlexIndirectDescending]
  | succ count ih =>
      simp only [runFlexIndirectDescending]
      change flexIndirectStep depth outcome
          (highestRound - (prefixCount + count))
          (runFlexIndirectDescending depth outcome highestRound
            (prefixCount + count) state) =
        flexIndirectStep depth outcome
          ((highestRound - prefixCount) - count)
          (runFlexIndirectDescending depth outcome
            (highestRound - prefixCount) count
            (runFlexIndirectDescending depth outcome highestRound
              prefixCount state))
      rw [ih, Nat.sub_sub]

/-- The descending indirect scan changes round decisions only. Recording a found
candidate is the separate operation that changes the commit index. -/
@[simp]
theorem run_flex_indirect_commit_index
    (depth highestRound stepCount : Nat)
    (outcome : Nat → IndirectRoundOutcome)
    (state : FlexCommitState) :
    (runFlexIndirectDescending depth outcome highestRound stepCount state).commitIndex =
      state.commitIndex := by
  induction stepCount with
  | zero => rfl
  | succ count ih =>
      simp only [runFlexIndirectDescending, flexIndirectStep]
      split <;> exact ih

@[simp]
theorem run_flex_indirect_round_count
    (depth highestRound stepCount : Nat)
    (outcome : Nat → IndirectRoundOutcome)
    (state : FlexCommitState) :
    (runFlexIndirectDescending depth outcome highestRound stepCount state).roundCount =
      state.roundCount := by
  induction stepCount with
  | zero => rfl
  | succ count ih =>
      simp only [runFlexIndirectDescending, flexIndirectStep]
      split <;> exact ih

/-- `count` adjacent rounds have usable anchors, starting at `base`. -/
def FlexAnchorWindow
    (state : FlexCommitState) (base count : Nat) : Prop :=
  ∀ offset, offset < count → (state.rounds (base + offset)).IsAnchor

/-- Build the round-level model from the ordered selected leader slot status lists.
The indexes are offsets in Rust's pending-round array. -/
def flexCommitStateFromSlotStatuses
    (commitIndex roundCount : Nat)
    (slotStatuses : Nat → List SelectedLeaderSlotStatus) : FlexCommitState :=
  { commitIndex
    roundCount
    rounds := fun index => summarizeFlexRound (slotStatuses index) }

/-- A usable selected leader slot order gives the corresponding round-level anchor
window. This theorem connects the detailed slot view to the scan model. -/
theorem usable_orders_give_flex_anchor_window
    {commitIndex roundCount base count : Nat}
    {slotStatuses : Nat → List SelectedLeaderSlotStatus}
    (usable : ∀ offset, offset < count →
      UsableAnchorOrder (slotStatuses (base + offset))) :
    FlexAnchorWindow
      (flexCommitStateFromSlotStatuses commitIndex roundCount slotStatuses)
      base count := by
  intro offset beforeEnd
  exact (summarize_flex_round_is_anchor_iff _).2
    (usable offset beforeEnd)

/-- The `stepCount` rounds already processed by a descending scan are final.
The arithmetic form avoids subtraction at round zero. -/
def FlexProcessedRoundsFinal
    (state : FlexCommitState) (base stepCount : Nat) : Prop :=
  ∀ round, round ≤ base → base < round + stepCount →
    (state.rounds round).IsFinal

theorem flex_indirect_step_at_exact_anchor
    {depth decisionRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (anchorInRange : decisionRound + depth < state.roundCount)
    (anchor : (state.rounds (decisionRound + depth)).IsAnchor) :
    (flexIndirectStep depth outcome decisionRound state).rounds decisionRound =
      finishFlexRound (state.rounds decisionRound) (outcome decisionRound) := by
  have fuelPositive : 0 < state.roundCount - (decisionRound + depth) := by
    omega
  have found := find_flex_anchor_at_start fuelPositive anchor
  simp [flexIndirectStep, found]

/-- If the anchor scan succeeds, the indirect step gives the decision round a
final result. -/
theorem flex_indirect_step_when_anchor_found
    {depth decisionRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (found : findFlexAnchorFrom state.rounds (decisionRound + depth)
      (state.roundCount - (decisionRound + depth)) = true) :
    ((flexIndirectStep depth outcome decisionRound state).rounds
      decisionRound).IsFinal := by
  simp only [flexIndirectStep, found, if_true, set_flex_round_same]
  exact finish_flex_round_is_final _ _

/-- If the decision round is itself a usable anchor, a successful indirect step
preserves its commit result and makes the complete round final. -/
theorem flex_indirect_step_anchor_becomes_final_commit
    {depth decisionRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (found : findFlexAnchorFrom state.rounds (decisionRound + depth)
      (state.roundCount - (decisionRound + depth)) = true)
    (decisionAnchor : (state.rounds decisionRound).IsAnchor) :
    (flexIndirectStep depth outcome decisionRound state).rounds decisionRound =
      .finalCommit := by
  simp only [flexIndirectStep, found, if_true, set_flex_round_same]
  exact finish_flex_anchor_is_final_commit decisionAnchor _

/-- A final interval that ends in a commit result supplies an anchor for an
earlier indirect step. -/
theorem flex_indirect_step_after_final_interval
    {depth decisionRound base : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (anchorStartLeBase : decisionRound + depth ≤ base)
    (baseInRange : base < state.roundCount)
    (finalBeforeBase : ∀ round,
      decisionRound + depth ≤ round → round < base →
        (state.rounds round).IsFinal)
    (commitAtBase : state.rounds base = .finalCommit) :
    ((flexIndirectStep depth outcome decisionRound state).rounds
      decisionRound).IsFinal := by
  let anchorStart := decisionRound + depth
  let distance := base - anchorStart
  let fuel := state.roundCount - anchorStart
  have reachesBase : anchorStart + distance = base := by
    simp only [distance]
    omega
  have fuelCoversEnd : distance < fuel := by
    simp only [distance, fuel, anchorStart]
    omega
  have finalBefore : ∀ offset, offset < distance →
      (state.rounds (anchorStart + offset)).IsFinal := by
    intro offset beforeEnd
    apply finalBeforeBase
    · simp [anchorStart]
    · simp only [distance] at beforeEnd
      omega
  have commitAtEnd : state.rounds (anchorStart + distance) = .finalCommit := by
    rw [reachesBase]
    exact commitAtBase
  have found : findFlexAnchorFrom state.rounds anchorStart fuel = true :=
    find_flex_anchor_after_final_prefix distance fuel fuelCoversEnd
      finalBefore commitAtEnd
  exact flex_indirect_step_when_anchor_found found

theorem flex_indirect_step_preserves_anchor
    {depth decisionRound protectedRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (anchor : (state.rounds protectedRound).IsAnchor) :
    ((flexIndirectStep depth outcome decisionRound state).rounds
      protectedRound).IsAnchor := by
  by_cases found : findFlexAnchorFrom state.rounds (decisionRound + depth)
      (state.roundCount - (decisionRound + depth)) = true
  · by_cases same : protectedRound = decisionRound
    · subst protectedRound
      simp only [flexIndirectStep, found, if_true]
      rw [set_flex_round_same]
      rw [finish_flex_anchor_is_final_commit anchor]
      simp [FlexRoundStatus.IsAnchor]
    · simp only [flexIndirectStep, found, if_true]
      rw [set_flex_round_other _ _ _ _ same]
      exact anchor
  · simp [flexIndirectStep, found]
    exact anchor

theorem flex_indirect_step_preserves_final
    {depth decisionRound protectedRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (final : (state.rounds protectedRound).IsFinal) :
    ((flexIndirectStep depth outcome decisionRound state).rounds
      protectedRound).IsFinal := by
  by_cases found : findFlexAnchorFrom state.rounds (decisionRound + depth)
      (state.roundCount - (decisionRound + depth)) = true
  · by_cases same : protectedRound = decisionRound
    · subst protectedRound
      simp only [flexIndirectStep, found, if_true]
      rw [set_flex_round_same]
      exact finish_flex_round_is_final _ _
    · simp only [flexIndirectStep, found, if_true]
      rw [set_flex_round_other _ _ _ _ same]
      exact final
  · simp [flexIndirectStep, found]
    exact final

theorem flex_indirect_step_preserves_final_commit
    {depth decisionRound protectedRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (committed : state.rounds protectedRound = .finalCommit) :
    (flexIndirectStep depth outcome decisionRound state).rounds protectedRound =
      .finalCommit := by
  by_cases found : findFlexAnchorFrom state.rounds (decisionRound + depth)
      (state.roundCount - (decisionRound + depth)) = true
  · by_cases same : protectedRound = decisionRound
    · subst protectedRound
      simp only [flexIndirectStep, found, if_true]
      rw [set_flex_round_same, committed]
      simp [finishFlexRound]
    · simp only [flexIndirectStep, found, if_true]
      rw [set_flex_round_other _ _ _ _ same]
      exact committed
  · simp [flexIndirectStep, found]
    exact committed

theorem run_flex_indirect_preserves_anchor
    {depth highestRound stepCount protectedRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (anchor : (state.rounds protectedRound).IsAnchor) :
    ((runFlexIndirectDescending depth outcome highestRound stepCount state).rounds
      protectedRound).IsAnchor := by
  induction stepCount with
  | zero => exact anchor
  | succ count ih =>
      exact flex_indirect_step_preserves_anchor ih

/-- A complete descending scan does not reopen a final round. -/
theorem run_flex_indirect_preserves_final
    {depth highestRound stepCount protectedRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (final : (state.rounds protectedRound).IsFinal) :
    ((runFlexIndirectDescending depth outcome highestRound stepCount state).rounds
      protectedRound).IsFinal := by
  induction stepCount with
  | zero => exact final
  | succ count ih =>
      exact flex_indirect_step_preserves_final ih

/-- A complete descending scan cannot replace an existing final commit. -/
theorem run_flex_indirect_preserves_final_commit
    {depth highestRound stepCount protectedRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (committed : state.rounds protectedRound = .finalCommit) :
    (runFlexIndirectDescending depth outcome highestRound stepCount state).rounds
      protectedRound = .finalCommit := by
  induction stepCount with
  | zero => exact committed
  | succ count ih =>
      exact flex_indirect_step_preserves_final_commit ih

/-- A descending scan closes each round that it processes. The top processed
round keeps a commit result. These properties hold for every indirect outcome.

The anchor window has `depth + 1` rounds. The last anchor closes `base`. The
earlier anchors start the descending scan. After that, the final prefix and its
commit result supply each next anchor. -/
theorem run_flex_indirect_closes_processed_rounds
    {depth base stepCount : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (depthPositive : 0 < depth)
    (stepCountBound : stepCount ≤ base + 1)
    (anchorWindow : FlexAnchorWindow state base (depth + 1))
    (windowInRange : base + depth < state.roundCount) :
    let result := runFlexIndirectDescending depth outcome base stepCount state
    FlexProcessedRoundsFinal result base stepCount ∧
      (0 < stepCount → result.rounds base = .finalCommit) := by
  induction stepCount with
  | zero =>
      constructor
      · intro round roundLe processed
        omega
      · intro positive
        omega
  | succ count ih =>
      have countLeBase : count ≤ base := by omega
      have previousInvariant := ih (by omega)
      let previous := runFlexIndirectDescending depth outcome base count state
      have previousFinal : FlexProcessedRoundsFinal previous base count := by
        exact previousInvariant.1
      have previousBaseCommit : 0 < count →
          previous.rounds base = .finalCommit := by
        exact previousInvariant.2
      have decisionFinal :
          ((flexIndirectStep depth outcome (base - count) previous).rounds
            (base - count)).IsFinal := by
        by_cases countWithinWindow : count ≤ depth
        · have originalAnchor :
              (state.rounds (base + (depth - count))).IsAnchor := by
            exact anchorWindow (depth - count) (by omega)
          have anchorRoundEq :
              base - count + depth = base + (depth - count) := by
            omega
          have previousAnchor :
              (previous.rounds (base - count + depth)).IsAnchor := by
            rw [anchorRoundEq]
            exact run_flex_indirect_preserves_anchor originalAnchor
          have anchorInRange :
              base - count + depth < previous.roundCount := by
            simp only [previous, run_flex_indirect_round_count]
            omega
          rw [flex_indirect_step_at_exact_anchor anchorInRange previousAnchor]
          exact finish_flex_round_is_final _ _
        · have depthBeforeCount : depth < count := by omega
          have anchorStartLeBase : base - count + depth ≤ base := by omega
          have baseInRange : base < previous.roundCount := by
            simp only [previous, run_flex_indirect_round_count]
            omega
          have finalBeforeBase : ∀ round,
              base - count + depth ≤ round → round < base →
                (previous.rounds round).IsFinal := by
            intro round startLe roundBeforeBase
            apply previousFinal round (by omega)
            omega
          exact flex_indirect_step_after_final_interval
            anchorStartLeBase baseInRange finalBeforeBase
            (previousBaseCommit (by omega))
      have nextFinal : FlexProcessedRoundsFinal
          (flexIndirectStep depth outcome (base - count) previous)
          base (count + 1) := by
        intro round roundLe processed
        by_cases current : round = base - count
        · subst round
          exact decisionFinal
        · have wasProcessed : base < round + count := by omega
          exact flex_indirect_step_preserves_final
            (previousFinal round roundLe wasProcessed)
      have nextBaseCommit :
          (flexIndirectStep depth outcome (base - count) previous).rounds base =
            .finalCommit := by
        by_cases firstStep : count = 0
        · subst count
          have decisionRoundEq : base - 0 = base := by omega
          have anchorAtBase : (previous.rounds base).IsAnchor := by
            simp only [previous, runFlexIndirectDescending]
            exact anchorWindow 0 (by omega)
          have anchorAtDepth :
              (previous.rounds (base + depth)).IsAnchor := by
            simp only [previous, runFlexIndirectDescending]
            exact anchorWindow depth (by omega)
          have anchorInRange : base + depth < previous.roundCount := by
            simpa [previous] using windowInRange
          have fuelPositive :
              0 < previous.roundCount - (base + depth) := by omega
          have found := find_flex_anchor_at_start fuelPositive anchorAtDepth
          simpa [decisionRoundEq] using
            (flex_indirect_step_anchor_becomes_final_commit found anchorAtBase)
        · exact flex_indirect_step_preserves_final_commit
            (previousBaseCommit (by omega))
      change FlexProcessedRoundsFinal
          (flexIndirectStep depth outcome (base - count) previous)
          base (count + 1) ∧
        (0 < count + 1 →
          (flexIndirectStep depth outcome (base - count) previous).rounds base =
            .finalCommit)
      exact ⟨nextFinal, fun _ => nextBaseCommit⟩

/-- The status scan used by `find_commit_leader_round`. -/
def findFlexCommitRoundFrom
    (rounds : Nat → FlexRoundStatus) : Nat → Nat → Option Nat
  | _, 0 => none
  | round, fuel + 1 =>
      match rounds round with
      | .finalCommit => some round
      | .finalSkip => findFlexCommitRoundFrom rounds (round + 1) fuel
      | .blockedWithoutCommit | .blockedWithCommit | .usableAnchor => none

def findFlexCommitRound (state : FlexCommitState) : Option Nat :=
  findFlexCommitRoundFrom state.rounds 0 state.roundCount

/-- The abstract Core result after FlexCommitter finds a commit round and Core
records the returned commit. -/
def recordFlexCommitResult (state : FlexCommitState) : FlexCommitState :=
  match findFlexCommitRound state with
  | none => state
  | some _ => { state with commitIndex := state.commitIndex + 1 }

/-- A final prefix that ends in a commit result contains a commit candidate. -/
theorem find_flex_commit_after_final_prefix
    {rounds : Nat → FlexRoundStatus} {start : Nat}
    (distance fuel : Nat)
    (fuelCoversEnd : distance < fuel)
    (finalBefore : ∀ offset, offset < distance →
      (rounds (start + offset)).IsFinal)
    (commitAtEnd : rounds (start + distance) = .finalCommit) :
    ∃ round, findFlexCommitRoundFrom rounds start fuel = some round := by
  induction distance generalizing start fuel with
  | zero =>
      have atStart : rounds start = .finalCommit := by
        simpa using commitAtEnd
      cases fuel with
      | zero => omega
      | succ remaining =>
          exact ⟨start, by simp [findFlexCommitRoundFrom, atStart]⟩
  | succ distance ih =>
      cases fuel with
      | zero => omega
      | succ remaining =>
          have firstFinal := finalBefore 0 (by omega)
          cases firstStatus : rounds start with
          | blockedWithoutCommit | blockedWithCommit | usableAnchor =>
              simp [firstStatus, FlexRoundStatus.IsFinal] at firstFinal
          | finalCommit =>
              exact ⟨start, by simp [findFlexCommitRoundFrom, firstStatus]⟩
          | finalSkip =>
              have tailFinal : ∀ offset, offset < distance →
                  (rounds (start + 1 + offset)).IsFinal := by
                intro offset beforeEnd
                have nextFinal := finalBefore (offset + 1) (by omega)
                simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using nextFinal
              have tailCommit :
                  rounds (start + 1 + distance) = .finalCommit := by
                simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
                  commitAtEnd
              rcases ih (start := start + 1) remaining (by omega)
                  tailFinal tailCommit with ⟨round, found⟩
              exact ⟨round, by
                rw [findFlexCommitRoundFrom, firstStatus]
                exact found⟩

/-- After the complete descending prefix scan, every round through `base` is
final and `base` has a commit result. -/
theorem complete_flex_indirect_prefix_is_committable
    {depth base : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (depthPositive : 0 < depth)
    (anchorWindow : FlexAnchorWindow state base (depth + 1))
    (windowInRange : base + depth < state.roundCount) :
    let result :=
      runFlexIndirectDescending depth outcome base (base + 1) state
    (∀ round, round ≤ base → (result.rounds round).IsFinal) ∧
      result.rounds base = .finalCommit := by
  have closed := run_flex_indirect_closes_processed_rounds
    (outcome := outcome) depthPositive (Nat.le_refl _) anchorWindow windowInRange
  constructor
  · intro round roundLe
    exact closed.1 round roundLe (by omega)
  · exact closed.2 (by omega)

/-- A complete descending prefix scan produces a commit candidate. -/
theorem complete_flex_indirect_prefix_finds_commit
    {depth base : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (depthPositive : 0 < depth)
    (anchorWindow : FlexAnchorWindow state base (depth + 1))
    (windowInRange : base + depth < state.roundCount) :
    let result :=
      runFlexIndirectDescending depth outcome base (base + 1) state
    ∃ round, findFlexCommitRound result = some round := by
  let result := runFlexIndirectDescending depth outcome base (base + 1) state
  have committable := complete_flex_indirect_prefix_is_committable
    (outcome := outcome) depthPositive anchorWindow windowInRange
  have baseInRange : base < result.roundCount := by
    simp only [result, run_flex_indirect_round_count]
    omega
  have finalBefore : ∀ offset, offset < base →
      (result.rounds (0 + offset)).IsFinal := by
    intro offset beforeBase
    simpa using committable.1 offset (by omega)
  have commitAtEnd : result.rounds (0 + base) = .finalCommit := by
    simpa using committable.2
  exact find_flex_commit_after_final_prefix base result.roundCount
    baseInRange finalBefore commitAtEnd

/-- The executable FlexCommitter scan and the abstract recorded result increase the
commit index. The theorem uses no network, scheduler, or outcome assumption. -/
theorem flex_anchor_window_advances_commit_index
    {depth base : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (depthPositive : 0 < depth)
    (anchorWindow : FlexAnchorWindow state base (depth + 1))
    (windowInRange : base + depth < state.roundCount) :
    let result :=
      runFlexIndirectDescending depth outcome base (base + 1) state
    (recordFlexCommitResult result).commitIndex = state.commitIndex + 1 := by
  let result := runFlexIndirectDescending depth outcome base (base + 1) state
  rcases complete_flex_indirect_prefix_finds_commit
      (outcome := outcome) depthPositive anchorWindow windowInRange with
    ⟨round, found⟩
  have resultIndex : result.commitIndex = state.commitIndex := by
    simp [result]
  have foundResult : findFlexCommitRound result = some round := by
    simpa [result] using found
  change (recordFlexCommitResult result).commitIndex = state.commitIndex + 1
  rw [recordFlexCommitResult, foundResult]
  exact congrArg (fun index => index + 1) resultIndex

/-- The same result holds for the complete Rust-style descending scan. Steps above
the anchor window preserve its usable anchors. The remaining steps close the
prefix and expose a commit candidate. -/
theorem full_flex_anchor_window_advances_commit_index
    {depth base highestRound : Nat}
    {outcome : Nat → IndirectRoundOutcome}
    {state : FlexCommitState}
    (depthPositive : 0 < depth)
    (baseLeHighest : base ≤ highestRound)
    (anchorWindow : FlexAnchorWindow state base (depth + 1))
    (windowInRange : base + depth < state.roundCount) :
    let result := runFlexIndirectDescending depth outcome highestRound
      (highestRound + 1) state
    (recordFlexCommitResult result).commitIndex = state.commitIndex + 1 := by
  let prefixCount := highestRound - base
  let prefixState := runFlexIndirectDescending depth outcome highestRound
    prefixCount state
  have prefixEndsAtBase : highestRound - prefixCount = base := by
    simp only [prefixCount]
    omega
  have splitCount : prefixCount + (base + 1) = highestRound + 1 := by
    simp only [prefixCount]
    omega
  have splitScan :
      runFlexIndirectDescending depth outcome highestRound
          (highestRound + 1) state =
        runFlexIndirectDescending depth outcome base (base + 1) prefixState := by
    rw [← splitCount]
    rw [run_flex_indirect_descending_append]
    rw [prefixEndsAtBase]
  have prefixAnchors : FlexAnchorWindow prefixState base (depth + 1) := by
    intro offset beforeEnd
    exact run_flex_indirect_preserves_anchor
      (anchorWindow offset beforeEnd)
  have prefixWindowInRange : base + depth < prefixState.roundCount := by
    simpa [prefixState] using windowInRange
  have advancesPrefix := flex_anchor_window_advances_commit_index
    (outcome := outcome) depthPositive prefixAnchors prefixWindowInRange
  have prefixIndex : prefixState.commitIndex = state.commitIndex := by
    simp [prefixState]
  rw [splitScan]
  simpa [prefixIndex] using advancesPrefix

/-! ### Why current v3 needs three usable anchor rounds -/

def twoAnchorExampleState : FlexCommitState :=
  { commitIndex := 0
    roundCount := 5
    rounds := fun index =>
      match index with
      | 2 | 3 => .usableAnchor
      | _ => .blockedWithoutCommit }

/-- With depth two, usable anchors at only two adjacent indexes can close the two
older indexes, but they do not close the first anchor index. No commit candidate is
available in this example. -/
theorem two_anchors_do_not_always_find_a_depth_two_commit :
    findFlexCommitRound
      (runFlexIndirectDescending indirectCommitDepth
        (fun _ => .skip) 2 3 twoAnchorExampleState) = none := by
  rfl

def threeAnchorExampleState : FlexCommitState :=
  { commitIndex := 0
    roundCount := 5
    rounds := fun index =>
      match index with
      | 2 | 3 | 4 => .usableAnchor
      | _ => .blockedWithoutCommit }

/-- The additional anchor closes the first anchor index. The same descending scan
then exposes a commit candidate and increases the commit index. -/
theorem three_anchors_find_a_depth_two_commit :
    (recordFlexCommitResult
      (runFlexIndirectDescending indirectCommitDepth
        (fun _ => .skip) 2 3 threeAnchorExampleState)).commitIndex = 1 := by
  rfl

end Mysticeti
