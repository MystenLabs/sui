/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.FlexCommitter
import Mysticeti.Liveness

namespace Mysticeti

/-! Commit-progress recovery definitions and stage composition for Mysticeti v3.

The local FlexCommitter scan is executable and proved. The current top theorem
composes abstract distributed recovery contracts. It does not yet derive them
from validator-addressed messages and single-validator actions. It does not prove
liveness for old leader blocks or transaction inclusion. See
`ASM-LIVE-COMMIT-PROGRESS-RECOVERY`, `ASM-LIVE-LEADER-STAKE`,
`ASM-LIVE-LEADER-SCHEDULE`,
`ASM-LIVE-FIRST-SLOT-SAMPLING`, and `ASM-LIVE-LOCAL-RESPONSE`.
-/

/-- Nominal v3 validator set stake for Byzantine stake bound `f` and crash stake
bound `c`. -/
def nominalHybridStake (f c : Nat) : Nat :=
  5 * f + 3 * c + 1

/-- Nominal v3 quorum threshold. -/
def nominalHybridQuorum (f c : Nat) : Nat :=
  4 * f + 2 * c + 1

/-- Nominal v3 certification threshold. -/
def nominalHybridCertificate (f c : Nat) : Nat :=
  2 * f + c + 1

/-- The maximum stake that can prevent progress because it is Byzantine or crashed. -/
def nominalNonProgressStakeBound (f c : Nat) : Nat :=
  f + c

/-- One round's selected validators are a subset of the leader schedule. Each
selected validator defines one selected leader slot for that round. -/
structure RoundLeaderSelection (authorityCount : Nat) where
  schedule : VoterSet
  selected : VoterSet
  selectedFromSchedule :
    VoterSet.SubsetAt authorityCount selected schedule

namespace RoundLeaderSelection

theorem selected_stake_le_schedule
    {authorityCount : Nat} {stake : Nat → Nat}
    (selection : RoundLeaderSelection authorityCount) :
    weight authorityCount stake selection.selected ≤
      weight authorityCount stake selection.schedule := by
  exact weight_mono stake selection.selectedFromSchedule

/-- The round leader selection stake is at most the leader schedule stake, and the
leader schedule stake is at most the validator set stake. -/
theorem stakes_bounded_by_validator_set
    {authorityCount : Nat} {stake : Nat → Nat}
    (selection : RoundLeaderSelection authorityCount) :
    weight authorityCount stake selection.selected ≤
        weight authorityCount stake selection.schedule ∧
      weight authorityCount stake selection.schedule ≤
        totalWeight authorityCount stake := by
  exact ⟨selection.selected_stake_le_schedule,
    weight_le_total authorityCount stake selection.schedule⟩

end RoundLeaderSelection

/-- After both nominal fault budgets are removed, the remaining stake is one
nominal quorum. Actual available stake can be larger. -/
theorem nominal_stake_after_fault_budgets_is_quorum (f c : Nat) :
    nominalHybridStake f c - nominalNonProgressStakeBound f c =
      nominalHybridQuorum f c := by
  simp [nominalHybridStake, nominalNonProgressStakeBound, nominalHybridQuorum]
  omega

/-- The nominal certification threshold is above both fault budgets. -/
theorem nominal_certificate_exceeds_non_progress_bound (f c : Nat) :
    nominalNonProgressStakeBound f c < nominalHybridCertificate f c := by
  simp [nominalNonProgressStakeBound, nominalHybridCertificate]
  omega

/-- The nominal certification threshold is at most the nominal quorum threshold. -/
theorem nominal_certificate_le_quorum (f c : Nat) :
    nominalHybridCertificate f c ≤ nominalHybridQuorum f c := by
  simp [nominalHybridCertificate, nominalHybridQuorum]
  omega

/-- The nominal quorum threshold is at most the nominal validator set stake. -/
theorem nominal_quorum_le_validator_set (f c : Nat) :
    nominalHybridQuorum f c ≤ nominalHybridStake f c := by
  simp [nominalHybridQuorum, nominalHybridStake]
  omega

/-- General viability bounds for one leader schedule.

The lower bound ensures that the schedule contains stake outside the Byzantine and
crash budgets. A leader fairness condition is still necessary when one round uses a
smaller selection. -/
structure LeaderScheduleViabilityBounds
    (totalStake nonProgressStakeBound scheduleStake : Nat) : Prop where
  containsProgressCandidate : nonProgressStakeBound < scheduleStake
  boundedByValidatorSet : scheduleStake ≤ totalStake

/-- The structural and quorum-coverage bounds for v3 leader schedule stake. -/
structure V3LeaderScheduleCoverageBounds
    (totalStake certificationThreshold scheduleStake : Nat) : Prop where
  reachesCertification : certificationThreshold ≤ scheduleStake
  boundedByValidatorSet : scheduleStake ≤ totalStake

/-- The stake bounds that give quorum coverage for one round leader selection. -/
structure RoundLeaderSelectionCoverageBounds
    (certificationThreshold scheduleStake selectionStake : Nat) : Prop where
  reachesCertification : certificationThreshold ≤ selectionStake
  selectionStakeLeSchedule : selectionStake ≤ scheduleStake

/-- An optional work cap for one round leader selection. -/
structure RoundLeaderSelectionResourceBound
    (quorumThreshold selectionStake : Nat) : Prop where
  selectionStakeLeQuorum : selectionStake ≤ quorumThreshold

/-- An optional work cap for a v3 leader schedule. V3 applies the schedule to every
pending leader round, so this also caps each round leader selection. -/
structure V3LeaderScheduleResourceBound
    (quorumThreshold scheduleStake : Nat) : Prop where
  scheduleStakeLeQuorum : scheduleStake ≤ quorumThreshold

/-- The full nominal validator set satisfies the general schedule viability bounds.
This is the schedule case for protocols that select one leader from all validators. -/
theorem nominal_validator_set_satisfies_schedule_viability (f c : Nat) :
    LeaderScheduleViabilityBounds
      (nominalHybridStake f c)
      (nominalNonProgressStakeBound f c)
      (nominalHybridStake f c) := by
  constructor
  · simp [nominalNonProgressStakeBound, nominalHybridStake]
    omega
  · exact Nat.le_refl _

/-- The optional nominal work cap is compatible with the v3 coverage bounds. -/
theorem nominal_quorum_satisfies_v3_schedule_coverage_and_resource_bounds
    (f c : Nat) :
    V3LeaderScheduleCoverageBounds
        (nominalHybridStake f c)
        (nominalHybridCertificate f c)
        (nominalHybridQuorum f c) ∧
      V3LeaderScheduleResourceBound
        (nominalHybridQuorum f c) (nominalHybridQuorum f c) := by
  exact ⟨⟨nominal_certificate_le_quorum f c,
      nominal_quorum_le_validator_set f c⟩, ⟨Nat.le_refl _⟩⟩

/-- The validator set upper bound alone permits an empty leader schedule, so it is
not a liveness condition. -/
theorem schedule_upper_bound_alone_is_not_sufficient (f c : Nat) :
    0 ≤ nominalHybridStake f c ∧
      ¬LeaderScheduleViabilityBounds
        (nominalHybridStake f c) (nominalNonProgressStakeBound f c) 0 := by
  constructor
  · exact Nat.zero_le _
  · intro bounds
    have containsProgress := bounds.containsProgressCandidate
    simp [nominalNonProgressStakeBound] at containsProgress

/-- A leader schedule above the non-progress stake bound contains positive stake
outside the Byzantine-or-crashed set. -/
theorem leader_schedule_contains_progress_stake
    {authorityCount : Nat} {stake : Nat → Nat}
    {schedule nonProgress : VoterSet} {nonProgressStakeBound : Nat}
    (scheduleLower :
      nonProgressStakeBound < weight authorityCount stake schedule)
    (nonProgressBound :
      weight authorityCount stake nonProgress ≤ nonProgressStakeBound) :
    0 < weight authorityCount stake (VoterSet.diff schedule nonProgress) := by
  have partition := weight_diff_add_inter authorityCount stake schedule nonProgress
  have interBound :
      weight authorityCount stake (VoterSet.inter schedule nonProgress) ≤
        nonProgressStakeBound := by
    have subset := VoterSet.inter_subset_right authorityCount schedule nonProgress
    have mono := weight_mono stake subset
    omega
  omega

/-- A leader schedule contains positive stake that is both progressing and locally
included when the two excluded stake sets together are smaller than the schedule.

This is an existence result. It does not state that the first selected leader slot
uses this stake, or that different proposers use the same local exclusion set. -/
theorem leader_schedule_contains_progress_and_locally_included_stake
    {authorityCount : Nat} {stake : Nat → Nat}
    {schedule nonProgress locallyExcluded : VoterSet}
    (combinedStakeBelowSchedule :
      weight authorityCount stake nonProgress +
          weight authorityCount stake locallyExcluded <
        weight authorityCount stake schedule) :
    0 < weight authorityCount stake
      (VoterSet.diff schedule
        (VoterSet.union nonProgress locallyExcluded)) := by
  have unionBound := weight_union_le_add authorityCount stake nonProgress
    locallyExcluded
  have unionBelowSchedule :
      weight authorityCount stake
          (VoterSet.union nonProgress locallyExcluded) <
        weight authorityCount stake schedule := by
    omega
  exact leader_schedule_contains_progress_stake
    (nonProgressStakeBound :=
      weight authorityCount stake (VoterSet.union nonProgress locallyExcluded))
    unionBelowSchedule (Nat.le_refl _)

/-- A small local exclusion set can still contain one fixed first-slot validator.
Thus, an exclusion stake cap does not by itself give the direct votes for that
selected leader slot. -/
def firstSlotExclusionExample : VoterSet := fun authority => authority == 0

/-- A stake cap on local exclusions does not protect one fixed first-slot
validator. Other validators can remain available while that slot stays excluded. -/
theorem exclusion_cap_does_not_protect_a_fixed_first_slot :
    weight 2 (fun _ => 1) firstSlotExclusionExample < 2 ∧
      firstSlotExclusionExample 0 = true ∧
      0 < weight 2 (fun _ => 1)
        (VoterSet.diff VoterSet.full firstSlotExclusionExample) := by
  decide

/-- If non-progress stake plus the quorum threshold is at most total validator set
stake, the validators outside the non-progress set have quorum stake. -/
theorem progress_stake_reaches_quorum
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {nonProgress : VoterSet}
    (nonProgressLeavesQuorum :
      weight authorityCount stake nonProgress + thresholds.quorum ≤
        totalWeight authorityCount stake) :
    thresholds.quorum ≤
      weight authorityCount stake
        (VoterSet.diff VoterSet.full nonProgress) := by
  have fullIntersection :
      VoterSet.inter VoterSet.full nonProgress = nonProgress := by
    funext authority
    simp [VoterSet.inter, VoterSet.full]
  have partition := weight_diff_add_inter authorityCount stake
    VoterSet.full nonProgress
  rw [fullIntersection] at partition
  simp [totalWeight] at nonProgressLeavesQuorum
  omega

/-- The actual v3 definition `Q = N - (f + c)` supplies the stake inequality used
by `progress_stake_reaches_quorum`. -/
theorem actual_hybrid_fault_budgets_leave_quorum
    {totalStake byzantineBound unavailableBound quorum nonProgressStake : Nat}
    (faultBudgetsFit : byzantineBound + unavailableBound ≤ totalStake)
    (nonProgressBound :
      nonProgressStake ≤ byzantineBound + unavailableBound)
    (quorumDefinition :
      quorum = totalStake - (byzantineBound + unavailableBound)) :
    nonProgressStake + quorum ≤ totalStake := by
  omega

/-- V3 schedule coverage bounds imply general schedule viability when the
certification threshold is above the non-progress stake bound. -/
theorem v3_schedule_coverage_implies_viability
    {totalStake certificationThreshold nonProgressStakeBound scheduleStake : Nat}
    (bounds : V3LeaderScheduleCoverageBounds
      totalStake certificationThreshold scheduleStake)
    (certificationExceedsNonProgress :
      nonProgressStakeBound < certificationThreshold) :
    LeaderScheduleViabilityBounds
      totalStake nonProgressStakeBound scheduleStake := by
  constructor
  · have reachesCertification := bounds.reachesCertification
    omega
  · exact bounds.boundedByValidatorSet

/-- In v3, selecting the full schedule transfers the certification lower bound to
the round leader selection. -/
theorem v3_full_schedule_gives_round_leader_selection_coverage
    {totalStake certificationThreshold scheduleStake selectionStake : Nat}
    (scheduleBounds : V3LeaderScheduleCoverageBounds
      totalStake certificationThreshold scheduleStake)
    (fullScheduleSelected : selectionStake = scheduleStake) :
    RoundLeaderSelectionCoverageBounds
      certificationThreshold scheduleStake selectionStake := by
  subst selectionStake
  exact ⟨scheduleBounds.reachesCertification, Nat.le_refl _⟩

/-- In v3, the optional schedule work cap transfers to the round leader selection. -/
theorem v3_full_schedule_gives_round_leader_selection_resource_bound
    {quorumThreshold scheduleStake selectionStake : Nat}
    (scheduleBound : V3LeaderScheduleResourceBound quorumThreshold scheduleStake)
    (fullScheduleSelected : selectionStake = scheduleStake) :
    RoundLeaderSelectionResourceBound quorumThreshold selectionStake := by
  subst selectionStake
  exact ⟨scheduleBound.scheduleStakeLeQuorum⟩

/-- A viable leader schedule does not by itself give round leader selection
coverage. The selector must supply a leader fairness condition or a per-round stake
bound. -/
theorem schedule_bounds_do_not_force_round_leader_selection (f c : Nat) :
    LeaderScheduleViabilityBounds
        (nominalHybridStake f c)
        (nominalNonProgressStakeBound f c)
        (nominalHybridStake f c) ∧
      ¬RoundLeaderSelectionCoverageBounds
        (nominalHybridCertificate f c) (nominalHybridStake f c) 0 := by
  constructor
  · exact nominal_validator_set_satisfies_schedule_viability f c
  · intro selectionBounds
    have reachesCertification := selectionBounds.reachesCertification
    simp [nominalHybridCertificate] at reachesCertification

/-- When v3 selects the full schedule, the certification-to-validator set bounds
apply to the schedule and the round leader selection. -/
theorem v3_coverage_applies_to_schedule_and_round
    {totalStake certificationThreshold scheduleStake selectionStake : Nat}
    (scheduleBounds : V3LeaderScheduleCoverageBounds
      totalStake certificationThreshold scheduleStake)
    (fullScheduleSelected : selectionStake = scheduleStake) :
    certificationThreshold ≤ scheduleStake ∧
      scheduleStake ≤ totalStake ∧
      certificationThreshold ≤ selectionStake ∧
      selectionStake ≤ totalStake := by
  subst selectionStake
  exact ⟨scheduleBounds.reachesCertification, scheduleBounds.boundedByValidatorSet,
    scheduleBounds.reachesCertification, scheduleBounds.boundedByValidatorSet⟩

/-- A quorum block layer and a round leader selection with at least
certification-threshold stake overlap in positive stake outside the Byzantine set. -/
theorem quorum_and_round_leader_selection_intersect_outside_byzantine_stake
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {faulty selected layer : VoterSet}
    (faultBounded : FaultBounded thresholds faulty)
    (selectedStake :
      thresholds.certificate ≤ weight authorityCount stake selected)
    (layerQuorum :
      thresholds.quorum ≤ weight authorityCount stake layer) :
    0 < weight authorityCount stake
      (VoterSet.diff (VoterSet.inter selected layer) faulty) := by
  have overlapLower := intersection_lower_bound authorityCount stake
    selectedStake layerQuorum
  have partition := weight_diff_add_inter authorityCount stake
    (VoterSet.inter selected layer) faulty
  have faultyPartBound :
      weight authorityCount stake
          (VoterSet.inter (VoterSet.inter selected layer) faulty) ≤
        thresholds.fault := by
    have subset := VoterSet.inter_subset_right authorityCount
      (VoterSet.inter selected layer) faulty
    have mono := weight_mono stake subset
    unfold FaultBounded at faultBounded
    omega
  have thresholdIntersection := thresholds.quorum_certificate_intersection
  omega

/-- For one common selected leader slot, the threshold safety facts do not depend
on the leader schedule, the round leader selection, or the selected leader slot
count. Global commit safety also requires correct validators to use the same
schedule version, round leader selection, and selected leader slot order. -/
theorem threshold_safety_is_independent_of_leader_selection
    {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (_schedule _selection : VoterSet) :
    totalWeight authorityCount stake + thresholds.fault <
        thresholds.quorum + thresholds.certificate ∧
      totalWeight authorityCount stake + thresholds.fault + thresholds.certificate ≤
        thresholds.quorum + thresholds.quorum := by
  exact ⟨thresholds.quorum_certificate_intersection,
    thresholds.quorum_preserves_certificate⟩

/-! ### Per-round shuffle does not give adjacent correct first slots

The next definitions give a small counterexample. Each round uses a permutation of
the same two-member leader schedule. A correct validator is first after every start
round, but a Byzantine validator is first between each two such rounds. Thus,
ordinary leader fairness does not imply two adjacent rounds with a correct validator
in the first selected leader slot.
-/

/-- Validator identities for the per-round shuffle counterexample. -/
inductive ShuffleExampleValidator where
  | correct
  | byzantine
  deriving DecidableEq, Repr

/-- A valid per-round order that alternates its first selected leader slot. -/
def alternatingRoundLeaderOrder (round : Nat) : List ShuffleExampleValidator :=
  if round % 2 = 0 then
    [.correct, .byzantine]
  else
    [.byzantine, .correct]

/-- Every counterexample round contains a permutation of the complete leader
schedule. -/
theorem alternating_round_order_is_schedule_permutation (round : Nat) :
    List.Perm (alternatingRoundLeaderOrder round) [.correct, .byzantine] := by
  unfold alternatingRoundLeaderOrder
  split
  · simp
  · exact List.Perm.swap ShuffleExampleValidator.correct
      ShuffleExampleValidator.byzantine []

/-- The first selected leader slot in the counterexample. -/
def alternatingFirstSelectedLeader (round : Nat) : ShuffleExampleValidator :=
  if round % 2 = 0 then .correct else .byzantine

/-- A correct validator is first after every start round. -/
theorem alternating_order_has_eventual_correct_first (start : Nat) :
    ∃ round, start ≤ round ∧
      alternatingFirstSelectedLeader round = .correct := by
  refine ⟨2 * start, ?_, ?_⟩
  · omega
  · simp [alternatingFirstSelectedLeader]

/-- No two adjacent rounds put a correct validator first. This proves that a valid
per-round shuffle plus ordinary leader fairness is not sufficient for the adjacent
anchor opportunity used by the current depth-two recovery argument. -/
theorem alternating_order_has_no_adjacent_correct_first (round : Nat) :
    ¬ (alternatingFirstSelectedLeader round = .correct ∧
      alternatingFirstSelectedLeader (round + 1) = .correct) := by
  simp [alternatingFirstSelectedLeader]
  omega

/-- V3 leader blocks receive direct votes from the next block round. -/
def directVoteRoundOffset : Nat := 1

/-- A depth-`d` descending scan needs `d` anchors for the older prefix and one
later anchor to finalize the first recovery round. -/
def requiredRecoveryAnchorCount (depth : Nat) : Nat :=
  depth + 1

/-- A window of `anchorCount` adjacent anchors needs the candidate quorum block
layers and the last anchor's direct-vote layer. -/
def requiredRecoveryLayerCount (anchorCount : Nat) : Nat :=
  anchorCount + directVoteRoundOffset

/-- The current v3 indirect depth requires three usable anchor rounds. -/
theorem current_v3_recovery_anchor_count :
    requiredRecoveryAnchorCount indirectCommitDepth = 3 := by
  rfl

/-- The current v3 distances derive four quorum block layers. The value is not an
independent recovery constant. -/
theorem current_v3_recovery_layer_count :
    requiredRecoveryLayerCount
      (requiredRecoveryAnchorCount indirectCommitDepth) = 4 := by
  rfl

/-- An abstract identity for one committed prefix. The Rust refinement can use the
commit digest at the matching commit index. -/
abbrev CommitPrefixId := Nat

/-- The state fields used by commit progress recovery. Stakes are derived from the
actual validator sets. -/
structure CommitProgressRecoveryView (State : Type) where
  authorityCount : Nat
  stake : Nat → Nat
  commitIndex : State → Nat
  committedPrefixId : State → CommitPrefixId
  /-- The protocol round at index zero of Rust's pending-round array. -/
  firstPendingLeaderRound : State → Nat
  /-- The number of entries in Rust's pending-round array. -/
  pendingRoundCount : State → Nat
  /-- The greatest pending-round index visited by the indirect decision scan. -/
  highestIndirectDecisionIndex : State → Nat
  stallExpired : State → Prop
  correctRecoveryAuthorities : State → VoterSet
  highestKnownOwnProposalRound : Nat → State → Nat
  recoveryProposalTarget : Nat → State → Nat
  quorumBlockLayerAuthors : Nat → State → VoterSet
  byzantine : State → VoterSet
  crashedOrUnavailable : State → VoterSet
  nonProgress : State → VoterSet
  gcBoundary : State → Nat
  leaderSchedule : State → VoterSet
  roundLeaderSelection : Nat → State → VoterSet
  pendingLeaderRound : Nat → State → Prop
  selectedLeaderSlots : Nat → State → List SelectedLeaderSlotView
  roundSelectionFromSchedule :
    ∀ state round,
      VoterSet.SubsetAt authorityCount
        (roundLeaderSelection round state) (leaderSchedule state)
  selectedLeaderSlotValidatorsNodup :
    ∀ state round,
      ((selectedLeaderSlots round state).map SelectedLeaderSlotView.validator).Nodup
  selectedLeaderSlotValidatorsInRange :
    ∀ state round validator,
      validator ∈
          (selectedLeaderSlots round state).map SelectedLeaderSlotView.validator →
      validator < authorityCount
  selectedLeaderSlotsMatchSelection :
    ∀ state round validator,
      validator < authorityCount →
      (validator ∈
          (selectedLeaderSlots round state).map SelectedLeaderSlotView.validator ↔
        roundLeaderSelection round state validator = true)

namespace CommitProgressRecoveryView

/-- The largest round value selected from a finite validator range. -/
def maxSelectedRound : Nat → VoterSet → (Nat → Nat) → Nat
  | 0, _, _ => 0
  | authorityCount + 1, selected, roundOf =>
      if selected authorityCount = true then
        max (maxSelectedRound authorityCount selected roundOf)
          (roundOf authorityCount)
      else
        maxSelectedRound authorityCount selected roundOf

/-- Each selected validator's round is at most the finite selected maximum. -/
theorem selected_round_le_max_selected_round
    {authorityCount : Nat} {selected : VoterSet} {roundOf : Nat → Nat}
    {authority : Nat}
    (authorityInRange : authority < authorityCount)
    (authoritySelected : selected authority = true) :
    roundOf authority ≤ maxSelectedRound authorityCount selected roundOf := by
  induction authorityCount generalizing authority with
  | zero => omega
  | succ authorityCount ih =>
      by_cases isLast : authority = authorityCount
      · subst authority
        rw [maxSelectedRound, if_pos authoritySelected]
        exact Nat.le_max_right _ _
      · have inEarlierRange : authority < authorityCount := by omega
        have earlierBound := ih inEarlierRange authoritySelected
        by_cases lastSelected : selected authorityCount = true
        · rw [maxSelectedRound, if_pos lastSelected]
          exact Nat.le_trans earlierBound (Nat.le_max_left _ _)
        · simpa [maxSelectedRound, lastSelected] using earlierBound

/-- The finite selected maximum is zero or is the round of one selected
validator. -/
theorem max_selected_round_zero_or_attained
    (authorityCount : Nat) (selected : VoterSet) (roundOf : Nat → Nat) :
    maxSelectedRound authorityCount selected roundOf = 0 ∨
      ∃ authority, authority < authorityCount ∧ selected authority = true ∧
        roundOf authority = maxSelectedRound authorityCount selected roundOf := by
  induction authorityCount with
  | zero => simp [maxSelectedRound]
  | succ authorityCount ih =>
      by_cases lastSelected : selected authorityCount = true
      · by_cases previousLeLast :
          maxSelectedRound authorityCount selected roundOf ≤
            roundOf authorityCount
        · right
          refine ⟨authorityCount, by omega, lastSelected, ?_⟩
          rw [maxSelectedRound, if_pos lastSelected, Nat.max_eq_right previousLeLast]
        · have lastLtPrevious :
            roundOf authorityCount <
              maxSelectedRound authorityCount selected roundOf := by omega
          rcases ih with previousZero | ⟨authority, authorityInRange,
              authoritySelected, authorityAtMaximum⟩
          · rw [previousZero] at lastLtPrevious
            omega
          · right
            refine ⟨authority, by omega, authoritySelected, ?_⟩
            rw [maxSelectedRound, if_pos lastSelected,
              Nat.max_eq_left (Nat.le_of_lt lastLtPrevious)]
            exact authorityAtMaximum
      · rcases ih with previousZero | ⟨authority, authorityInRange,
            authoritySelected, authorityAtMaximum⟩
        · left
          simpa [maxSelectedRound, lastSelected] using previousZero
        · right
          refine ⟨authority, by omega, authoritySelected, ?_⟩
          simpa [maxSelectedRound, lastSelected] using authorityAtMaximum

/-- A nonempty selected set has one validator at the finite selected maximum. -/
theorem max_selected_round_is_attained
    {authorityCount : Nat} {selected : VoterSet} {roundOf : Nat → Nat}
    (selectedNonempty :
      ∃ authority, authority < authorityCount ∧ selected authority = true) :
    ∃ authority, authority < authorityCount ∧ selected authority = true ∧
      roundOf authority = maxSelectedRound authorityCount selected roundOf := by
  rcases max_selected_round_zero_or_attained authorityCount selected roundOf with
    maximumZero | attained
  · rcases selectedNonempty with ⟨authority, authorityInRange, authoritySelected⟩
    have authorityBound := selected_round_le_max_selected_round
      (roundOf := roundOf) authorityInRange authoritySelected
    rw [maximumZero] at authorityBound
    exact ⟨authority, authorityInRange, authoritySelected, by omega⟩
  · exact attained

/-- The recovery frontier is the largest last signed round among the validators
that are in commit progress recovery. This value exists only in the proof. The
protocol does not send or agree on it. -/
def recoveryFrontierRound {State : Type}
    (view : CommitProgressRecoveryView State) (state : State) : Nat :=
  maxSelectedRound view.authorityCount
    (view.correctRecoveryAuthorities state)
    (fun authority => view.highestKnownOwnProposalRound authority state)

/-- Every validator in commit progress recovery starts at or below the recovery
frontier chosen in the proof. -/
theorem recovery_authority_round_le_frontier
    {State : Type} (view : CommitProgressRecoveryView State)
    {state : State} {authority : Nat}
    (authorityInRange : authority < view.authorityCount)
    (authorityInRecovery :
      view.correctRecoveryAuthorities state authority = true) :
    view.highestKnownOwnProposalRound authority state ≤
      view.recoveryFrontierRound state := by
  exact selected_round_le_max_selected_round
    (roundOf := fun selectedAuthority =>
      view.highestKnownOwnProposalRound selectedAuthority state)
    authorityInRange authorityInRecovery

/-- A recovery quorum contains one validator whose last signed round attains the
recovery frontier chosen in the proof. -/
theorem recovery_frontier_is_attained
    {State : Type} (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    {state : State}
    (recoveryStake : thresholds.quorum ≤
      weight view.authorityCount view.stake
        (view.correctRecoveryAuthorities state)) :
    ∃ authority,
      authority < view.authorityCount ∧
      view.correctRecoveryAuthorities state authority = true ∧
      view.highestKnownOwnProposalRound authority state =
        view.recoveryFrontierRound state := by
  have positiveRecoveryStake : 0 <
      weight view.authorityCount view.stake
        (view.correctRecoveryAuthorities state) := by
    have quorumPositive := thresholds.quorum_positive
    omega
  rcases positive_weight_has_member positiveRecoveryStake with
    ⟨someAuthority, someAuthorityInRange, someAuthorityRecovering,
      someAuthorityStake⟩
  have selectedNonempty :
      ∃ authority, authority < view.authorityCount ∧
        view.correctRecoveryAuthorities state authority = true :=
    ⟨someAuthority, someAuthorityInRange, someAuthorityRecovering⟩
  rcases max_selected_round_is_attained
      (roundOf := fun authority =>
        view.highestKnownOwnProposalRound authority state)
      selectedNonempty with
    ⟨authority, authorityInRange, authorityRecovering, authorityAtFrontier⟩
  exact ⟨authority, authorityInRange, authorityRecovering, authorityAtFrontier⟩

def correctRecoveryStake {State : Type}
    (view : CommitProgressRecoveryView State) (state : State) : Nat :=
  weight view.authorityCount view.stake (view.correctRecoveryAuthorities state)

def quorumBlockLayerStake {State : Type}
    (view : CommitProgressRecoveryView State) (round : Nat) (state : State) : Nat :=
  weight view.authorityCount view.stake (view.quorumBlockLayerAuthors round state)

def selectedLeaderSlotStatuses {State : Type}
    (view : CommitProgressRecoveryView State) (round : Nat) (state : State) :
    List SelectedLeaderSlotStatus :=
  (view.selectedLeaderSlots round state).map SelectedLeaderSlotView.status

/-- Ordered selected leader slot statuses at one index in Rust's pending-round
array. -/
def pendingSelectedLeaderSlotStatuses {State : Type}
    (view : CommitProgressRecoveryView State) (index : Nat) (state : State) :
    List SelectedLeaderSlotStatus :=
  view.selectedLeaderSlotStatuses
    (view.firstPendingLeaderRound state + index) state

/-- The status-level FlexCommitter state derived from the recovery view. -/
def flexCommitState {State : Type}
    (view : CommitProgressRecoveryView State) (state : State) : FlexCommitState :=
  flexCommitStateFromSlotStatuses
    (view.commitIndex state)
    (view.pendingRoundCount state)
    (fun index => view.pendingSelectedLeaderSlotStatuses index state)

end CommitProgressRecoveryView

def CommitStalledAt {State : Type}
    (view : CommitProgressRecoveryView State) (baseline : Nat) : State → Prop :=
  fun state => view.commitIndex state = baseline ∧ view.stallExpired state

def CommitAdvancedFrom {State : Type}
    (view : CommitProgressRecoveryView State) (baseline : Nat) : State → Prop :=
  fun state => baseline < view.commitIndex state

def AtCommitIndex {State : Type}
    (view : CommitProgressRecoveryView State) (baseline : Nat) : State → Prop :=
  fun state => view.commitIndex state = baseline

/-- Every correct authority in recovery can target only the round after its highest
known own proposal. A higher threshold-clock round does not change this target. -/
def NextRoundProposalTargets {State : Type}
    (view : CommitProgressRecoveryView State) : State → Prop :=
  fun state => ∀ authority,
    authority < view.authorityCount →
    view.correctRecoveryAuthorities state authority = true →
    view.recoveryProposalTarget authority state =
      view.highestKnownOwnProposalRound authority state + 1

def RecoveryQuorum {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake) : State → Prop :=
  fun state =>
    thresholds.quorum ≤ view.correctRecoveryStake state ∧
      VoterSet.SubsetAt view.authorityCount
        (view.correctRecoveryAuthorities state)
        (VoterSet.diff VoterSet.full (view.nonProgress state)) ∧
      NextRoundProposalTargets view state

/-- One retained pending-round block layer from quorum stake of validators that are
in commit progress recovery. -/
def RetainedQuorumBlockLayer {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (round : Nat) : State → Prop :=
  fun state =>
    view.pendingLeaderRound round state ∧
      thresholds.quorum ≤ view.quorumBlockLayerStake round state ∧
      VoterSet.SubsetAt view.authorityCount
        (view.quorumBlockLayerAuthors round state)
        (view.correctRecoveryAuthorities state) ∧
      Retained (view.gcBoundary state) round

/-- A proposer has quorum stake of immediate-round parent blocks in its retained
local DAG. The parent authors do not need to be in commit progress recovery. -/
def AvailableImmediateParentQuorum {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (round : Nat) : State → Prop :=
  fun state =>
    thresholds.quorum ≤ view.quorumBlockLayerStake round state ∧
      Retained (view.gcBoundary state) round

/-- Each retained recovery quorum block layer is an available immediate-parent
quorum for the next exact proposal round. -/
theorem retained_recovery_layer_gives_parent_quorum
    {State : Type} (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    {state : State} {round : Nat}
    (layer : RetainedQuorumBlockLayer view thresholds round state) :
    AvailableImmediateParentQuorum view thresholds round state :=
  ⟨layer.2.1, layer.2.2.2⟩

def ConsecutiveQuorumBlockLayers {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (base count : Nat) : State → Prop :=
  fun state => ∀ offset, offset < count →
    RetainedQuorumBlockLayer view thresholds (base + offset) state

/-- The base round is derived from the execution. The protocol does not select it.
Each witness layer is also above the modeled block-GC boundary. -/
def HasQuorumBlockLayerWindow {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (count : Nat) : State → Prop :=
  fun state => ∃ base, ConsecutiveQuorumBlockLayers view thresholds base count state

def UsableAnchorRound {State : Type}
    (view : CommitProgressRecoveryView State) (round : Nat) : State → Prop :=
  fun state => UsableAnchorOrder (view.selectedLeaderSlotStatuses round state)

def ConsecutiveUsableAnchors {State : Type}
    (view : CommitProgressRecoveryView State) (base count : Nat) : State → Prop :=
  fun state => ∀ offset, offset < count → UsableAnchorRound view (base + offset) state

def HasUsableAnchorWindow {State : Type}
    (view : CommitProgressRecoveryView State) (count : Nat) : State → Prop :=
  fun state => ∃ base, ConsecutiveUsableAnchors view base count state

def RecoveryQuorumAt {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (baseline : Nat) : State → Prop :=
  fun state => AtCommitIndex view baseline state ∧ RecoveryQuorum view thresholds state

def HasQuorumBlockLayerWindowAt {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (baseline count : Nat) : State → Prop :=
  fun state => AtCommitIndex view baseline state ∧
    HasQuorumBlockLayerWindow view thresholds count state

/-- One state contains both a recovery quorum and the required quorum block layers. -/
def RecoveryLayerWindowAt {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (baseline count : Nat) : State → Prop :=
  fun state => RecoveryQuorumAt view thresholds baseline state ∧
    HasQuorumBlockLayerWindowAt view thresholds baseline count state

/-- The indirect scan contains one usable anchor window. The base is an index in
Rust's pending-round array. -/
def HasCoveredUsableAnchorWindowAt {State : Type}
    (view : CommitProgressRecoveryView State)
    (baseline depth count : Nat) : State → Prop :=
  fun state =>
    AtCommitIndex view baseline state ∧
      ∃ baseIndex,
        baseIndex ≤ view.highestIndirectDecisionIndex state ∧
          baseIndex + depth < view.pendingRoundCount state ∧
          ∀ offset, offset < count →
            UsableAnchorRound view
              (view.firstPendingLeaderRound state + baseIndex + offset) state

/-- The executable Lean FlexCommitter model advances for every valid indirect
result. Lean cannot inspect the Rust call path that records this result. -/
def ModeledFlexCommitterAdvancesAt {State : Type}
    (view : CommitProgressRecoveryView State)
    (baseline depth : Nat) : State → Prop :=
  fun state =>
    AtCommitIndex view baseline state ∧
      ∀ outcome : Nat → IndirectRoundOutcome,
        let model := view.flexCommitState state
        let result := runFlexIndirectDescending depth outcome
          (view.highestIndirectDecisionIndex state)
          (view.highestIndirectDecisionIndex state + 1) model
        (recordFlexCommitResult result).commitIndex = baseline + 1

/-- A covered usable anchor window advances the executable Lean FlexCommitter
model. This is the deterministic fourth recovery result. -/
theorem covered_usable_anchor_window_advances_modeled_flex_committer
    {State : Type} (view : CommitProgressRecoveryView State)
    {state : State} {baseline depth : Nat}
    (depthPositive : 0 < depth)
    (window : HasCoveredUsableAnchorWindowAt view baseline depth
      (depth + 1) state) :
    ModeledFlexCommitterAdvancesAt view baseline depth state := by
  rcases window with
    ⟨atBaseline, baseIndex, baseLeHighest, windowInRange, anchors⟩
  constructor
  · exact atBaseline
  · intro outcome
    let model := view.flexCommitState state
    have modelAnchors : FlexAnchorWindow model baseIndex (depth + 1) := by
      apply usable_orders_give_flex_anchor_window
      intro offset beforeEnd
      have usable := anchors offset beforeEnd
      simpa [model, CommitProgressRecoveryView.flexCommitState,
        CommitProgressRecoveryView.pendingSelectedLeaderSlotStatuses,
        UsableAnchorRound, Nat.add_assoc] using usable
    have advances := full_flex_anchor_window_advances_commit_index
      (outcome := outcome) depthPositive baseLeHighest modelAnchors windowInRange
    have modelIndex : model.commitIndex = baseline := by
      simpa [model, CommitProgressRecoveryView.flexCommitState,
        flexCommitStateFromSlotStatuses, AtCommitIndex] using atBaseline
    simpa [modelIndex] using advances

/-- Consecutive quorum block layers give the required adjacent anchor window when
the multi-leader recovery conditions give one usable anchor per candidate round. -/
theorem quorum_block_layer_window_yields_anchor_window
    {State : Type} (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    {state : State} {base anchorCount : Nat}
    (usableAnchorFromLayers :
      ∀ round,
        RetainedQuorumBlockLayer view thresholds round state →
        RetainedQuorumBlockLayer view thresholds
          (round + directVoteRoundOffset) state →
        UsableAnchorRound view round state)
    (layers : ConsecutiveQuorumBlockLayers view thresholds base
      (requiredRecoveryLayerCount anchorCount) state) :
    ConsecutiveUsableAnchors view base anchorCount state := by
  intro offset beforeEnd
  have leaderLayer := layers offset (by
    simp [requiredRecoveryLayerCount, directVoteRoundOffset]
    omega)
  have voteLayer := layers (offset + directVoteRoundOffset) (by
    simp [requiredRecoveryLayerCount, directVoteRoundOffset]
    omega)
  have voteLayerAtRound :
      RetainedQuorumBlockLayer view thresholds
        ((base + offset) + directVoteRoundOffset) state := by
    simpa [Nat.add_assoc] using voteLayer
  exact usableAnchorFromLayers (base + offset) leaderLayer voteLayerAtRound

/-- A next-round recovery target cannot skip forward or reuse an old own
proposal round. -/
theorem next_round_proposal_target_is_not_old
    {State : Type} (view : CommitProgressRecoveryView State)
    {state : State} {authority : Nat}
    (nextRoundTargets : NextRoundProposalTargets view state)
    (authorityInRange : authority < view.authorityCount)
    (authorityInRecovery :
      view.correctRecoveryAuthorities state authority = true) :
    ¬view.recoveryProposalTarget authority state ≤
      view.highestKnownOwnProposalRound authority state := by
  rw [nextRoundTargets authority authorityInRange authorityInRecovery]
  omega

/-! ### Exact-next catch-up to an execution-derived frontier -/

/-- A finite proposal path that increases the last signed round by exactly one at
each step. Each step needs a quorum of immediate parents in its source round. -/
inductive ExactNextCatchUp (parentQuorumAvailable : Nat → Prop) : Nat → Nat → Prop
  | done (round : Nat) : ExactNextCatchUp parentQuorumAvailable round round
  | next {round target : Nat} :
      parentQuorumAvailable round →
      ExactNextCatchUp parentQuorumAvailable (round + 1) target →
      ExactNextCatchUp parentQuorumAvailable round target

/-- Parent quorums for one finite round interval give an exact-next path across the
interval. No proposal in the path skips a round. -/
theorem parent_quorums_give_exact_next_catch_up
    (parentQuorumAvailable : Nat → Prop) (start count : Nat)
    (parents : ∀ offset, offset < count →
      parentQuorumAvailable (start + offset)) :
    ExactNextCatchUp parentQuorumAvailable start (start + count) := by
  induction count generalizing start with
  | zero =>
      simpa using (ExactNextCatchUp.done (parentQuorumAvailable :=
        parentQuorumAvailable) start)
  | succ count ih =>
      apply ExactNextCatchUp.next
      · simpa using parents 0 (by omega)
      · have remainingParents : ∀ offset, offset < count →
            parentQuorumAvailable ((start + 1) + offset) := by
          intro offset offsetInRange
          have parent := parents (offset + 1) (by omega)
          simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using parent
        have remainingPath := ih (start + 1) remainingParents
        simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using remainingPath

/-- A validator in recovery has an exact-next path from its last signed round to
the recovery frontier when the required finite parent interval is available. -/
theorem recovery_authority_has_exact_next_path_to_frontier
    {State : Type} (view : CommitProgressRecoveryView State)
    {state : State} {authority : Nat}
    (authorityInRange : authority < view.authorityCount)
    (authorityInRecovery :
      view.correctRecoveryAuthorities state authority = true)
    (parentQuorumAvailable : Nat → Prop)
    (parentsUntilFrontier : ∀ round,
      view.highestKnownOwnProposalRound authority state ≤ round →
      round < view.recoveryFrontierRound state →
      parentQuorumAvailable round) :
    ExactNextCatchUp parentQuorumAvailable
      (view.highestKnownOwnProposalRound authority state)
      (view.recoveryFrontierRound state) := by
  let start := view.highestKnownOwnProposalRound authority state
  let frontier := view.recoveryFrontierRound state
  have startLeFrontier : start ≤ frontier :=
    view.recovery_authority_round_le_frontier authorityInRange authorityInRecovery
  let count := frontier - start
  have parentsByOffset : ∀ offset, offset < count →
      parentQuorumAvailable (start + offset) := by
    intro offset offsetInRange
    apply parentsUntilFrontier
    · omega
    · dsimp [count] at offsetInRange
      omega
  have path := parent_quorums_give_exact_next_catch_up
    parentQuorumAvailable start count parentsByOffset
  have endpoint : start + count = frontier := by
    dsimp [count]
    omega
  simpa [endpoint] using path

/-! ### Abstract recovery-entry contract -/

/-- Commit indexes do not decrease along one execution trace. -/
def CommitIndexMonotone {State : Type}
    (trace : Trace State) (view : CommitProgressRecoveryView State) : Prop :=
  ∀ earlier later,
    earlier ≤ later →
    view.commitIndex (trace earlier) ≤ view.commitIndex (trace later)

/-- An abstract contract for entry into commit progress recovery.

The entry actions are local, but `liveStakeIsQuorum` is an aggregate result. The
final theorem must derive that result from the fixed fault and availability
sets. -/
structure RecoveryEntryProcess
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (protocolAction : LocalConsensusAction → Prop)
    (processing : BoundedLocalProcessing network protocolAction) where
  liveAuthorities : VoterSet
  liveStakeIsQuorum :
    thresholds.quorum ≤
      weight view.authorityCount view.stake liveAuthorities
  commitIndexMonotone : CommitIndexMonotone trace view
  entryAction : Nat → Time → LocalConsensusAction
  entryActionEnabledAt :
    ∀ authority start,
      (entryAction authority start).enabledAt = start
  entryActionIsLocalConsensus :
    ∀ authority start,
      authority < view.authorityCount →
      liveAuthorities authority = true →
      protocolAction (entryAction authority start)
  entryActionResult :
    ∀ baseline authority start,
      authority < view.authorityCount →
      liveAuthorities authority = true →
      CommitStalledAt view baseline (trace start) →
      CommitAdvancedFrom view baseline
          (trace (entryAction authority start).completedAt) ∨
        view.correctRecoveryAuthorities
          (trace (entryAction authority start).completedAt) authority = true
  recoveryPersistsWhileStalled :
    ∀ baseline authority entered later,
      entered ≤ later →
      view.commitIndex (trace later) = baseline →
      view.correctRecoveryAuthorities (trace entered) authority = true →
      view.correctRecoveryAuthorities (trace later) authority = true
  recoveryAuthoritiesAreProgressing :
    ∀ time,
      VoterSet.SubsetAt view.authorityCount
        (view.correctRecoveryAuthorities (trace time))
        (VoterSet.diff VoterSet.full (view.nonProgress (trace time)))
  recoveryUsesNextRoundPolicy :
    ∀ time, NextRoundProposalTargets view (trace time)

/-- Bounded local recovery-entry actions form a recovery quorum by one common
deadline, unless the commit index advances first. -/
theorem stalled_reaches_recovery_quorum_within
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    (process : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    (baseline : Nat) :
    WithinAfter network.gst processing.epsilon trace
      (CommitStalledAt view baseline)
      (fun state =>
        CommitAdvancedFrom view baseline state ∨
          RecoveryQuorumAt view thresholds baseline state) := by
  intro start afterGst stalled
  let deadline := start + processing.epsilon
  have startIndex : view.commitIndex (trace start) = baseline := stalled.1
  have startBeforeDeadline : start ≤ deadline := by
    simp [deadline]
  have baselineLeDeadline :
      baseline ≤ view.commitIndex (trace deadline) := by
    rw [← startIndex]
    exact process.commitIndexMonotone start deadline startBeforeDeadline
  by_cases advanced : baseline < view.commitIndex (trace deadline)
  · exact ⟨deadline, startBeforeDeadline, by simp [deadline], Or.inl advanced⟩
  · have atBaseline : view.commitIndex (trace deadline) = baseline := by omega
    have liveInRecovery :
        VoterSet.SubsetAt view.authorityCount process.liveAuthorities
          (view.correctRecoveryAuthorities (trace deadline)) := by
      intro authority authorityInRange authorityLive
      let action := process.entryAction authority start
      have actionValid : protocolAction action :=
        process.entryActionIsLocalConsensus authority start authorityInRange
          authorityLive
      have actionAfterGst : network.gst ≤ action.enabledAt := by
        rw [process.entryActionEnabledAt authority start]
        exact afterGst
      have completion := processing.postGstCompletion action actionValid actionAfterGst
      have completionBeforeDeadline : action.completedAt ≤ deadline := by
        have completionBound := completion.2
        rw [process.entryActionEnabledAt authority start] at completionBound
        exact completionBound
      have result := process.entryActionResult baseline authority start
        authorityInRange authorityLive stalled
      rcases result with advancedAtCompletion | enteredRecovery
      · have completionIndexLeDeadline := process.commitIndexMonotone
          action.completedAt deadline completionBeforeDeadline
        have advancedAction :
            baseline < view.commitIndex (trace action.completedAt) := by
          simpa [CommitAdvancedFrom, action] using advancedAtCompletion
        omega
      · have enteredAction :
            view.correctRecoveryAuthorities (trace action.completedAt) authority =
              true := by
          simpa [action] using enteredRecovery
        exact process.recoveryPersistsWhileStalled baseline authority
          action.completedAt deadline completionBeforeDeadline atBaseline
          enteredAction
    have recoveryStake :
        thresholds.quorum ≤ view.correctRecoveryStake (trace deadline) := by
      have liveWeightLeRecovery := weight_mono view.stake liveInRecovery
      exact Nat.le_trans process.liveStakeIsQuorum liveWeightLeRecovery
    have recoveryQuorum : RecoveryQuorum view thresholds (trace deadline) := by
      exact ⟨recoveryStake,
        process.recoveryAuthoritiesAreProgressing deadline,
        process.recoveryUsesNextRoundPolicy deadline⟩
    exact ⟨deadline, startBeforeDeadline, by simp [deadline],
      Or.inr ⟨atBaseline, recoveryQuorum⟩⟩

/-- The bounded recovery-entry theorem also gives the first unbounded recovery
stage used by commit-progress composition. -/
theorem stalled_to_recovery_quorum
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    (process : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    (baseline : Nat) :
    LeadsToAfter network.gst trace
      (CommitStalledAt view baseline)
      (fun state =>
        CommitAdvancedFrom view baseline state ∨
          RecoveryQuorumAt view thresholds baseline state) :=
  (stalled_reaches_recovery_quorum_within process baseline).toLeadsTo

/-- A recovery quorum persists while the commit index remains unchanged. -/
theorem recovery_quorum_persists_while_stalled
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    (process : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    {baseline start later : Nat}
    (startBeforeLater : start ≤ later)
    (recoveryAtStart :
      RecoveryQuorumAt view thresholds baseline (trace start))
    (atBaselineLater : view.commitIndex (trace later) = baseline) :
    RecoveryQuorumAt view thresholds baseline (trace later) := by
  have recoverySubset :
      VoterSet.SubsetAt view.authorityCount
        (view.correctRecoveryAuthorities (trace start))
        (view.correctRecoveryAuthorities (trace later)) := by
    intro authority authorityInRange recoveringAtStart
    exact process.recoveryPersistsWhileStalled baseline authority start later
      startBeforeLater atBaselineLater recoveringAtStart
  have recoveryStake :
      thresholds.quorum ≤ view.correctRecoveryStake (trace later) := by
    have startStake := recoveryAtStart.2.1
    have stakeMonotone := weight_mono view.stake recoverySubset
    exact Nat.le_trans startStake stakeMonotone
  exact ⟨atBaselineLater, recoveryStake,
    process.recoveryAuthoritiesAreProgressing later,
    process.recoveryUsesNextRoundPolicy later⟩

/-! ### Adaptive recovery pacing -/

/-- A recovery wait schedule never decreases and can exceed every finite bound. -/
structure RecoveryWaitSchedule where
  delay : Nat → Time
  delayMonotone : ∀ earlier later, earlier ≤ later → delay earlier ≤ delay later
  delayUnbounded : ∀ bound, ∃ attempt, bound ≤ delay attempt

/-- An unbounded recovery wait eventually covers the timing difference between two
validators, one post-GST message delivery, and one local acceptance action. -/
theorem recovery_wait_eventually_covers_block_visibility
    {protocolPacket : Packet → Prop}
    {network : PartialSynchrony protocolPacket}
    {protocolAction : LocalConsensusAction → Prop}
    (processing : BoundedLocalProcessing network protocolAction)
    (wait : RecoveryWaitSchedule)
    {timerStart proposalSendDifference : Time}
    (packet : Packet)
    (packetValid : protocolPacket packet)
    (packetAfterGst : network.gst ≤ packet.sentAt)
    (packetSentWithinDifference :
      packet.sentAt ≤ timerStart + proposalSendDifference)
    (accept : LocalConsensusAction)
    (acceptValid : protocolAction accept)
    (acceptStartsAtDelivery : accept.enabledAt = packet.deliveredAt) :
  ∃ attempt,
      accept.completedAt ≤ timerStart + wait.delay attempt := by
  rcases wait.delayUnbounded
      (proposalSendDifference + network.delta + processing.epsilon) with
    ⟨attempt, waitCovers⟩
  refine ⟨attempt, ?_⟩
  have visible := processing.protocol_packet_becomes_locally_visible packet
    packetValid accept acceptValid acceptStartsAtDelivery packetAfterGst
  calc
    accept.completedAt ≤
        packet.sentAt + network.delta + processing.epsilon := visible
    _ ≤ (timerStart + proposalSendDifference) + network.delta +
        processing.epsilon := by
      exact Nat.add_le_add_right
        (Nat.add_le_add_right packetSentWithinDifference network.delta)
        processing.epsilon
    _ = timerStart +
        (proposalSendDifference + network.delta + processing.epsilon) := by
      simp [Nat.add_assoc]
    _ ≤ timerStart + wait.delay attempt := Nat.add_le_add_left waitCovers _

/-! ### Quorum block layers from per-validator proposals -/

/-- An abstract block-flow helper for one recovery block.

`proposalReadyAt` is when the next-round proposal has an immediate-parent quorum
and its recovery pacing wait has expired. `deadlineCoversFlow` is arithmetic: the
window allows one proposal action, one network delay, and one acceptance action.
The helper has no sender or receiver state, and it accepts local parent-quorum
availability as an input. It is not a final theorem boundary. -/
structure RecoveryBlockFlow
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (protocolAction : LocalConsensusAction → Prop)
    (processing : BoundedLocalProcessing network protocolAction)
    (entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    (baseRound : Time → Nat)
    (windowBound : Nat → Time) where
  proposalReadyAt : Time → Nat → Nat → Time
  proposalReadyAfterStart :
    ∀ start count offset,
      start ≤ proposalReadyAt start count offset
  parentQuorumAvailableAtProposalReady :
    ∀ baseline authority start count offset,
      RecoveryQuorumAt view thresholds baseline (trace start) →
      offset < count →
      authority < view.authorityCount →
      view.correctRecoveryAuthorities (trace start) authority = true →
      AvailableImmediateParentQuorum view thresholds (baseRound start + offset)
        (trace (proposalReadyAt start count offset))
  proposalAction : Nat → Time → Nat → Nat → LocalConsensusAction
  proposalActionEnabledAt :
    ∀ authority start count offset,
      (proposalAction authority start count offset).enabledAt =
        proposalReadyAt start count offset
  proposalActionEnabledByParentQuorum :
    ∀ authority start count offset,
      offset < count →
      authority < view.authorityCount →
      view.correctRecoveryAuthorities (trace start) authority = true →
      AvailableImmediateParentQuorum view thresholds (baseRound start + offset)
        (trace (proposalReadyAt start count offset)) →
      protocolAction (proposalAction authority start count offset)
  blockPacket : Nat → Time → Nat → Nat → Packet
  blockPacketSentAtProposalCompletion :
    ∀ authority start count offset,
      (blockPacket authority start count offset).sentAt =
        (proposalAction authority start count offset).completedAt
  blockPacketIsProtocolPacket :
    ∀ authority start count offset,
      offset < count →
      authority < view.authorityCount →
      view.correctRecoveryAuthorities (trace start) authority = true →
      protocolPacket (blockPacket authority start count offset)
  acceptAction : Nat → Time → Nat → Nat → LocalConsensusAction
  acceptActionEnabledAtDelivery :
    ∀ authority start count offset,
      (acceptAction authority start count offset).enabledAt =
        (blockPacket authority start count offset).deliveredAt
  acceptActionIsLocalConsensus :
    ∀ authority start count offset,
      offset < count →
      authority < view.authorityCount →
      view.correctRecoveryAuthorities (trace start) authority = true →
      protocolAction (acceptAction authority start count offset)
  deadlineCoversFlow :
    ∀ start count offset,
      offset < count →
      proposalReadyAt start count offset + processing.epsilon +
          network.delta + processing.epsilon ≤
        start + windowBound count
  acceptedBlockResult :
    ∀ baseline authority start count offset,
      RecoveryQuorumAt view thresholds baseline (trace start) →
      offset < count →
      authority < view.authorityCount →
      view.correctRecoveryAuthorities (trace start) authority = true →
      CommitAdvancedFrom view baseline
          (trace (acceptAction authority start count offset).completedAt) ∨
        view.quorumBlockLayerAuthors (baseRound start + offset)
          (trace (acceptAction authority start count offset).completedAt)
          authority = true
  acceptedBlockPersistsWhileStalled :
    ∀ baseline authority round accepted later,
      accepted ≤ later →
      view.commitIndex (trace later) = baseline →
      view.quorumBlockLayerAuthors round (trace accepted) authority = true →
      view.quorumBlockLayerAuthors round (trace later) authority = true

/-- The local proposal, network delivery, and local acceptance bounds make one
recovery block visible by the common layer-window deadline. -/
theorem recovery_block_flow_visible_at_deadline
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    {entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing}
    {baseRound : Time → Nat} {windowBound : Nat → Time}
    (flow : RecoveryBlockFlow trace network view thresholds protocolAction
      processing entry baseRound windowBound)
    {baseline authority start count offset : Nat}
    (afterGst : network.gst ≤ start)
    (recoveryAtStart :
      RecoveryQuorumAt view thresholds baseline (trace start))
    (offsetInWindow : offset < count)
    (authorityInRange : authority < view.authorityCount)
    (authorityRecovering :
      view.correctRecoveryAuthorities (trace start) authority = true) :
    let deadline := start + windowBound count
    CommitAdvancedFrom view baseline (trace deadline) ∨
      view.quorumBlockLayerAuthors
        (baseRound start + offset) (trace deadline) authority = true := by
  let proposal := flow.proposalAction authority start count offset
  let packet := flow.blockPacket authority start count offset
  let accept := flow.acceptAction authority start count offset
  let deadline := start + windowBound count
  have parentQuorum := flow.parentQuorumAvailableAtProposalReady baseline authority
    start count offset recoveryAtStart offsetInWindow authorityInRange
    authorityRecovering
  have proposalValid : protocolAction proposal :=
    flow.proposalActionEnabledByParentQuorum authority start count offset
      offsetInWindow authorityInRange authorityRecovering parentQuorum
  have proposalAfterGst : network.gst ≤ proposal.enabledAt := by
    rw [flow.proposalActionEnabledAt authority start count offset]
    exact Nat.le_trans afterGst
      (flow.proposalReadyAfterStart start count offset)
  have proposalCompletion :=
    processing.postGstCompletion proposal proposalValid proposalAfterGst
  have packetValid : protocolPacket packet :=
    flow.blockPacketIsProtocolPacket authority start count offset
      offsetInWindow authorityInRange authorityRecovering
  have packetAfterGst : network.gst ≤ packet.sentAt := by
    rw [flow.blockPacketSentAtProposalCompletion authority start count offset]
    exact Nat.le_trans proposalAfterGst proposalCompletion.1
  have delivery := network.postGstDelivery packet packetValid packetAfterGst
  have acceptValid : protocolAction accept :=
    flow.acceptActionIsLocalConsensus authority start count offset
      offsetInWindow authorityInRange authorityRecovering
  have acceptAfterGst : network.gst ≤ accept.enabledAt := by
    rw [flow.acceptActionEnabledAtDelivery authority start count offset]
    exact Nat.le_trans packetAfterGst delivery.1
  have acceptCompletion :=
    processing.postGstCompletion accept acceptValid acceptAfterGst
  have acceptBeforeDeadline : accept.completedAt ≤ deadline := by
    have proposalBound := proposalCompletion.2
    rw [flow.proposalActionEnabledAt authority start count offset] at proposalBound
    have proposalBound' :
        proposal.completedAt ≤
          flow.proposalReadyAt start count offset + processing.epsilon := by
      simpa [proposal] using proposalBound
    have deliveryBound := delivery.2
    rw [flow.blockPacketSentAtProposalCompletion authority start count offset]
      at deliveryBound
    have deliveryBound' :
        packet.deliveredAt ≤ proposal.completedAt + network.delta := by
      simpa [packet, proposal] using deliveryBound
    have acceptBound := acceptCompletion.2
    rw [flow.acceptActionEnabledAtDelivery authority start count offset]
      at acceptBound
    have acceptBound' :
        accept.completedAt ≤ packet.deliveredAt + processing.epsilon := by
      simpa [accept, packet] using acceptBound
    have covers := flow.deadlineCoversFlow start count offset offsetInWindow
    have covers' :
        flow.proposalReadyAt start count offset + processing.epsilon +
            network.delta + processing.epsilon ≤ deadline := by
      simpa [deadline] using covers
    calc
      accept.completedAt ≤ packet.deliveredAt + processing.epsilon :=
        acceptBound'
      _ ≤ (proposal.completedAt + network.delta) + processing.epsilon :=
        Nat.add_le_add_right deliveryBound' processing.epsilon
      _ ≤ ((flow.proposalReadyAt start count offset + processing.epsilon) +
            network.delta) + processing.epsilon :=
        Nat.add_le_add_right
          (Nat.add_le_add_right proposalBound' network.delta)
          processing.epsilon
      _ ≤ deadline := covers'
  have result := flow.acceptedBlockResult baseline authority start count offset
    recoveryAtStart offsetInWindow authorityInRange authorityRecovering
  rcases result with advancedAtAccept | acceptedBlock
  · have commitMonotone := entry.commitIndexMonotone
      accept.completedAt deadline acceptBeforeDeadline
    have advancedAccept :
        baseline < view.commitIndex (trace accept.completedAt) := by
      simpa [CommitAdvancedFrom, accept] using advancedAtAccept
    have advancedDeadline :
        baseline < view.commitIndex (trace deadline) :=
      Nat.lt_of_lt_of_le advancedAccept commitMonotone
    exact Or.inl advancedDeadline
  · by_cases advancedAtDeadline :
        baseline < view.commitIndex (trace deadline)
    · exact Or.inl advancedAtDeadline
    · have startBeforeDeadline : start ≤ deadline := by simp [deadline]
      have baselineLeDeadline :
          baseline ≤ view.commitIndex (trace deadline) := by
        rw [← recoveryAtStart.1]
        exact entry.commitIndexMonotone start deadline startBeforeDeadline
      have atBaseline : view.commitIndex (trace deadline) = baseline := by omega
      have acceptedAtAction :
          view.quorumBlockLayerAuthors (baseRound start + offset)
            (trace accept.completedAt) authority = true := by
        simpa [accept] using acceptedBlock
      exact Or.inr (flow.acceptedBlockPersistsWhileStalled baseline authority
        (baseRound start + offset) accept.completedAt deadline
        acceptBeforeDeadline atBaseline acceptedAtAction)

/-- Abstract block-visibility contract for a finite recovery window.

Its central field already states a future visibility result for each validator.
It is an internal composition interface. A final theorem must derive the field
from local proposal, synchronization, storage, and delivery rules. -/
structure RecoveryLayerProductionProcess
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (protocolAction : LocalConsensusAction → Prop)
    (processing : BoundedLocalProcessing network protocolAction)
    (entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing) where
  baseRound : Time → Nat
  baseRoundIsRecoveryFrontier :
    ∀ baseline start,
      RecoveryQuorumAt view thresholds baseline (trace start) →
      baseRound start = view.recoveryFrontierRound (trace start)
  windowBound : Nat → Time
  recoveryBlockVisibleAtDeadline :
    ∀ baseline start count offset authority,
      network.gst ≤ start →
      RecoveryQuorumAt view thresholds baseline (trace start) →
      offset < count →
      authority < view.authorityCount →
      view.correctRecoveryAuthorities (trace start) authority = true →
      let deadline := start + windowBound count
      CommitAdvancedFrom view baseline (trace deadline) ∨
        view.quorumBlockLayerAuthors
          (baseRound start + offset) (trace deadline) authority = true
  layerContainsOnlyRecoveringValidators :
    ∀ start count offset,
      offset < count →
      let deadline := start + windowBound count
      VoterSet.SubsetAt view.authorityCount
        (view.quorumBlockLayerAuthors
          (baseRound start + offset) (trace deadline))
        (view.correctRecoveryAuthorities (trace deadline))
  layerRoundIsPendingAtDeadline :
    ∀ baseline start count offset,
      RecoveryQuorumAt view thresholds baseline (trace start) →
      offset < count →
      let deadline := start + windowBound count
      view.commitIndex (trace deadline) = baseline →
      view.pendingLeaderRound (baseRound start + offset) (trace deadline)
  layerIsRetainedAtDeadline :
    ∀ baseline start count offset,
      RecoveryQuorumAt view thresholds baseline (trace start) →
      offset < count →
      let deadline := start + windowBound count
      view.commitIndex (trace deadline) = baseline →
      Retained (view.gcBoundary (trace deadline)) (baseRound start + offset)

/-- Build the layer-production process from one local block-flow proof and the
three state-mapping rules for recovery membership, pending rounds, and retention.
-/
def recovery_layer_production_from_block_flow
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    {entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing}
    {baseRound : Time → Nat} {windowBound : Nat → Time}
    (flow : RecoveryBlockFlow trace network view thresholds protocolAction
      processing entry baseRound windowBound)
    (baseRoundIsRecoveryFrontier :
      ∀ baseline start,
        RecoveryQuorumAt view thresholds baseline (trace start) →
        baseRound start = view.recoveryFrontierRound (trace start))
    (layerContainsOnlyRecoveringValidators :
      ∀ start count offset,
        offset < count →
        let deadline := start + windowBound count
        VoterSet.SubsetAt view.authorityCount
          (view.quorumBlockLayerAuthors
            (baseRound start + offset) (trace deadline))
          (view.correctRecoveryAuthorities (trace deadline)))
    (layerRoundIsPendingAtDeadline :
      ∀ baseline start count offset,
        RecoveryQuorumAt view thresholds baseline (trace start) →
        offset < count →
        let deadline := start + windowBound count
        view.commitIndex (trace deadline) = baseline →
        view.pendingLeaderRound (baseRound start + offset) (trace deadline))
    (layerIsRetainedAtDeadline :
      ∀ baseline start count offset,
        RecoveryQuorumAt view thresholds baseline (trace start) →
        offset < count →
        let deadline := start + windowBound count
        view.commitIndex (trace deadline) = baseline →
        Retained (view.gcBoundary (trace deadline))
          (baseRound start + offset)) :
    RecoveryLayerProductionProcess trace network view thresholds protocolAction
      processing entry where
  baseRound := baseRound
  baseRoundIsRecoveryFrontier := baseRoundIsRecoveryFrontier
  windowBound := windowBound
  recoveryBlockVisibleAtDeadline := by
    intro baseline start count offset authority afterGst recoveryAtStart
      offsetInWindow authorityInRange authorityRecovering
    exact recovery_block_flow_visible_at_deadline flow afterGst recoveryAtStart
      offsetInWindow authorityInRange authorityRecovering
  layerContainsOnlyRecoveringValidators :=
    layerContainsOnlyRecoveringValidators
  layerRoundIsPendingAtDeadline := layerRoundIsPendingAtDeadline
  layerIsRetainedAtDeadline := layerIsRetainedAtDeadline

/-- Per-validator recovery proposals from quorum stake form consecutive quorum
block layers at the recovery frontier chosen in the proof by the common deadline. -/
theorem recovery_quorum_has_chosen_layers_by_deadline
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    {entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing}
    (production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry)
    (baseline count start : Nat)
    (afterGst : network.gst ≤ start)
    (recoveryAtStart :
      RecoveryQuorumAt view thresholds baseline (trace start)) :
    let deadline := start + production.windowBound count
    CommitAdvancedFrom view baseline (trace deadline) ∨
      (RecoveryQuorumAt view thresholds baseline (trace deadline) ∧
        ConsecutiveQuorumBlockLayers view thresholds
          (view.recoveryFrontierRound (trace start)) count (trace deadline)) := by
  let deadline := start + production.windowBound count
  have startBeforeDeadline : start ≤ deadline := by simp [deadline]
  have startAtBaseline : view.commitIndex (trace start) = baseline :=
    recoveryAtStart.1
  have baselineLeDeadline :
      baseline ≤ view.commitIndex (trace deadline) := by
    rw [← startAtBaseline]
    exact entry.commitIndexMonotone start deadline startBeforeDeadline
  by_cases advanced : baseline < view.commitIndex (trace deadline)
  · exact Or.inl advanced
  · have atBaseline : view.commitIndex (trace deadline) = baseline := by omega
    have startRecoverySubsetDeadline :
        VoterSet.SubsetAt view.authorityCount
          (view.correctRecoveryAuthorities (trace start))
          (view.correctRecoveryAuthorities (trace deadline)) := by
      intro authority authorityInRange recoveringAtStart
      exact entry.recoveryPersistsWhileStalled baseline authority start deadline
        startBeforeDeadline atBaseline recoveringAtStart
    have recoveryStakeAtDeadline :
        thresholds.quorum ≤ view.correctRecoveryStake (trace deadline) := by
      have startStake := recoveryAtStart.2.1
      have monotoneStake := weight_mono view.stake startRecoverySubsetDeadline
      exact Nat.le_trans startStake monotoneStake
    have recoveryAtDeadline :
        RecoveryQuorumAt view thresholds baseline (trace deadline) := by
      refine ⟨atBaseline, recoveryStakeAtDeadline, ?_, ?_⟩
      · exact entry.recoveryAuthoritiesAreProgressing deadline
      · exact entry.recoveryUsesNextRoundPolicy deadline
    have layersAtDeadline :
        ConsecutiveQuorumBlockLayers view thresholds
          (production.baseRound start) count (trace deadline) := by
      intro offset offsetInWindow
      have startRecoverySubsetLayer :
          VoterSet.SubsetAt view.authorityCount
            (view.correctRecoveryAuthorities (trace start))
            (view.quorumBlockLayerAuthors
              (production.baseRound start + offset) (trace deadline)) := by
        intro authority authorityInRange recoveringAtStart
        have visible := production.recoveryBlockVisibleAtDeadline baseline start
          count offset authority afterGst recoveryAtStart offsetInWindow
          authorityInRange recoveringAtStart
        rcases visible with advancedAtDeadline | blockVisible
        · exact False.elim (advanced advancedAtDeadline)
        · exact blockVisible
      have layerStake :
          thresholds.quorum ≤
            view.quorumBlockLayerStake
              (production.baseRound start + offset) (trace deadline) := by
        have startStake := recoveryAtStart.2.1
        have monotoneStake := weight_mono view.stake startRecoverySubsetLayer
        exact Nat.le_trans startStake monotoneStake
      exact ⟨production.layerRoundIsPendingAtDeadline baseline start count offset
          recoveryAtStart offsetInWindow atBaseline,
        layerStake,
        production.layerContainsOnlyRecoveringValidators start count offset
          offsetInWindow,
        production.layerIsRetainedAtDeadline baseline start count offset
          recoveryAtStart offsetInWindow atBaseline⟩
    have frontierLayers :
        ConsecutiveQuorumBlockLayers view thresholds
          (view.recoveryFrontierRound (trace start)) count (trace deadline) := by
      rw [← production.baseRoundIsRecoveryFrontier baseline start recoveryAtStart]
      exact layersAtDeadline
    exact Or.inr ⟨recoveryAtDeadline, frontierLayers⟩

/-- Pointwise recovery proposals from quorum stake form one consecutive quorum
block-layer window by the common deadline. -/
theorem recovery_quorum_reaches_layers_within
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    {entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing}
    (production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry)
    (baseline count : Nat) :
    WithinAfter network.gst (production.windowBound count) trace
      (RecoveryQuorumAt view thresholds baseline)
      (fun state =>
        CommitAdvancedFrom view baseline state ∨
          RecoveryLayerWindowAt view thresholds baseline count state) := by
  intro start afterGst recoveryAtStart
  let deadline := start + production.windowBound count
  have startBeforeDeadline : start ≤ deadline := by simp [deadline]
  have result := recovery_quorum_has_chosen_layers_by_deadline production
    baseline count start afterGst recoveryAtStart
  rcases result with advanced | ⟨recoveryAtDeadline, layersAtDeadline⟩
  · exact ⟨deadline, startBeforeDeadline, by simp [deadline], Or.inl advanced⟩
  · have windowAtDeadline :
        HasQuorumBlockLayerWindowAt view thresholds baseline count
          (trace deadline) :=
      ⟨recoveryAtDeadline.1, view.recoveryFrontierRound (trace start),
        layersAtDeadline⟩
    exact ⟨deadline, startBeforeDeadline, by simp [deadline],
      Or.inr ⟨recoveryAtDeadline, windowAtDeadline⟩⟩

/-- The bounded per-validator production theorem gives the second recovery stage
used by commit-progress composition. -/
theorem recovery_quorum_to_layers
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    {entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing}
    (production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry)
    (baseline count : Nat) :
    LeadsToAfter network.gst trace
      (RecoveryQuorumAt view thresholds baseline)
      (fun state =>
        CommitAdvancedFrom view baseline state ∨
          RecoveryLayerWindowAt view thresholds baseline count state) :=
  (recovery_quorum_reaches_layers_within production baseline count).toLeadsTo

/-! ### Usable anchors from timely first-slot voting -/

/-- The validator in the first selected leader slot is outside this abstract
model's non-progress set. This old stage model does not connect that set to the
Byzantine and unavailable sets. -/
def FirstSelectedLeaderIsProgressing {State : Type}
    (view : CommitProgressRecoveryView State)
    (round : Nat) : State → Prop :=
  fun state => ∃ validator,
    (view.selectedLeaderSlots round state).head?.map
        SelectedLeaderSlotView.validator = some validator ∧
      view.nonProgress state validator = false

/-- An author-level abstraction of recovery parent inclusion.

It is schedule-independent and represents at most one counted parent per author.
It does not model block references or the choice between equivocation branches.
That choice remains a source-to-model obligation. -/
structure RecoveryImmediateParentRule {State : Type}
    (view : CommitProgressRecoveryView State) where
  parentAvailable : State → Nat → Nat → Nat → Prop
  parentIncluded : State → Nat → Nat → Nat → Prop
  includesAvailableParent :
    ∀ state round leader voter,
      voter < view.authorityCount →
      view.quorumBlockLayerAuthors (round + directVoteRoundOffset) state voter =
        true →
      parentAvailable state round leader voter →
      parentIncluded state round leader voter

/-- Abstract first-slot availability contract. Its field assumes the result that
the final validator-indexed proof must derive from pacing and delivery. -/
structure TimelyFirstSlotParentAvailability {State : Type}
    (view : CommitProgressRecoveryView State)
    (parents : RecoveryImmediateParentRule view) where
  progressingFirstSlotIsAvailable :
    ∀ state round leader voter,
      (view.selectedLeaderSlots round state).head?.map
          SelectedLeaderSlotView.validator = some leader →
      view.nonProgress state leader = false →
      voter < view.authorityCount →
      view.quorumBlockLayerAuthors (round + directVoteRoundOffset) state voter =
        true →
      parents.parentAvailable state round leader voter

/-- The direct decision rule counts an included immediate parent as one direct vote
and changes the first selected leader slot to `Commit` after quorum stake votes. -/
structure DirectFirstSlotDecisionRule {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (parents : RecoveryImmediateParentRule view) where
  directVoters : Nat → State → VoterSet
  includedParentIsDirectVote :
    ∀ state round leader voter,
      voter < view.authorityCount →
      parents.parentIncluded state round leader voter →
      directVoters round state voter = true
  directQuorumCommitsFirstSlot :
    ∀ state round validator,
      (view.selectedLeaderSlots round state).head?.map
          SelectedLeaderSlotView.validator = some validator →
      thresholds.quorum ≤
        weight view.authorityCount view.stake (directVoters round state) →
      ∃ tail,
        view.selectedLeaderSlots round state =
          { validator := validator, status := .commit } :: tail

/-- Abstract direct-vote observations and the selected-slot status update.

The first field already states aggregate next-round inclusion. The final theorem
must derive it from pacing, delivery, and local parent selection. -/
structure TimelyFirstSlotVoting {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake) where
  directVoters : Nat → State → VoterSet
  progressingFirstSlotIncludedByNextLayer :
    ∀ state round validator,
      (view.selectedLeaderSlots round state).head?.map
          SelectedLeaderSlotView.validator = some validator →
      view.nonProgress state validator = false →
      VoterSet.SubsetAt view.authorityCount
        (view.quorumBlockLayerAuthors (round + directVoteRoundOffset) state)
        (directVoters round state)
  directQuorumCommitsFirstSlot :
    ∀ state round validator,
      (view.selectedLeaderSlots round state).head?.map
          SelectedLeaderSlotView.validator = some validator →
      thresholds.quorum ≤
        weight view.authorityCount view.stake (directVoters round state) →
      ∃ tail,
        view.selectedLeaderSlots round state =
          { validator := validator, status := .commit } :: tail

/-- The schedule-independent recovery parent rule, timely delivery, and the direct
decision rule construct the first-slot voting condition used by the anchor proof. -/
def timely_first_slot_voting_from_parent_rule
    {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (parents : RecoveryImmediateParentRule view)
    (availability : TimelyFirstSlotParentAvailability view parents)
    (direct : DirectFirstSlotDecisionRule view thresholds parents) :
    TimelyFirstSlotVoting view thresholds where
  directVoters := direct.directVoters
  progressingFirstSlotIncludedByNextLayer := by
    intro state round leader firstSlot leaderProgressing
    intro voter voterInRange voterInLayer
    have available := availability.progressingFirstSlotIsAvailable state round
      leader voter firstSlot leaderProgressing voterInRange voterInLayer
    have included := parents.includesAvailableParent state round leader voter
      voterInRange voterInLayer available
    exact direct.includedParentIsDirectVote state round leader voter voterInRange
      included
  directQuorumCommitsFirstSlot := direct.directQuorumCommitsFirstSlot

/-- A progressing first selected leader and one next-round quorum block layer make
the round a usable anchor. -/
theorem timely_progressing_first_slot_is_usable
    {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (voting : TimelyFirstSlotVoting view thresholds)
    {state : State} {round : Nat}
    (progressing : FirstSelectedLeaderIsProgressing view round state)
    (voteLayer : RetainedQuorumBlockLayer view thresholds
      (round + directVoteRoundOffset) state) :
    UsableAnchorRound view round state := by
  rcases progressing with ⟨validator, firstSlot, validatorProgressing⟩
  have layerInVoters :=
    voting.progressingFirstSlotIncludedByNextLayer state round validator
      firstSlot validatorProgressing
  have voterStake :
      thresholds.quorum ≤
        weight view.authorityCount view.stake (voting.directVoters round state) := by
    have layerStake := voteLayer.2.1
    have layerStakeLeVoters := weight_mono view.stake layerInVoters
    exact Nat.le_trans layerStake layerStakeLeVoters
  rcases voting.directQuorumCommitsFirstSlot state round validator firstSlot
      voterStake with ⟨tail, committedFirst⟩
  unfold UsableAnchorRound CommitProgressRecoveryView.selectedLeaderSlotStatuses
  rw [committedFirst]
  simp [UsableAnchorOrder]

/-- A consecutive run of progressing first selected leaders gives the required
usable anchors when the next-round quorum block layers are present. -/
theorem progressing_first_slot_run_gives_usable_anchors
    {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (voting : TimelyFirstSlotVoting view thresholds)
    {state : State} {base anchorCount : Nat}
    (layers : ConsecutiveQuorumBlockLayers view thresholds base
      (requiredRecoveryLayerCount anchorCount) state)
    (progressingRun : ∀ offset, offset < anchorCount →
      FirstSelectedLeaderIsProgressing view (base + offset) state) :
    ConsecutiveUsableAnchors view base anchorCount state := by
  intro offset offsetInRun
  have voteLayer := layers (offset + directVoteRoundOffset) (by
    simp [requiredRecoveryLayerCount, directVoteRoundOffset]
    omega)
  have voteLayerAtRound :
      RetainedQuorumBlockLayer view thresholds
        ((base + offset) + directVoteRoundOffset) state := by
    simpa [Nat.add_assoc] using voteLayer
  exact timely_progressing_first_slot_is_usable view thresholds voting
    (progressingRun offset offsetInRun) voteLayerAtRound

/-- A covered recovery layer window and a progressing first-slot run give the
covered usable anchor window consumed by the executable FlexCommitter theorem. -/
theorem progressing_first_slot_run_gives_covered_anchor_window
    {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (voting : TimelyFirstSlotVoting view thresholds)
    {state : State} {baseline depth baseIndex : Nat}
    (atBaseline : AtCommitIndex view baseline state)
    (baseIndexScanned : baseIndex ≤ view.highestIndirectDecisionIndex state)
    (windowInRange : baseIndex + depth < view.pendingRoundCount state)
    (layers : ConsecutiveQuorumBlockLayers view thresholds
      (view.firstPendingLeaderRound state + baseIndex)
      (requiredRecoveryLayerCount (depth + 1)) state)
    (progressingRun : ∀ offset, offset < depth + 1 →
      FirstSelectedLeaderIsProgressing view
        (view.firstPendingLeaderRound state + baseIndex + offset) state) :
    HasCoveredUsableAnchorWindowAt view baseline depth (depth + 1) state := by
  refine ⟨atBaseline, baseIndex, baseIndexScanned, windowInRange, ?_⟩
  have anchors := progressing_first_slot_run_gives_usable_anchors
    view thresholds voting layers progressingRun
  intro offset offsetInRun
  exact anchors offset offsetInRun

/-- A favorable first-slot trace contract.

After a recovery layer window exists, it eventually selects a later candidate
window whose first selected leader slots are progressing, unless the commit index
has already changed. It selects only the leader order. It does not assume votes,
anchors, or a commit result. However, it already supplies the successful future
leader run. A final probabilistic theorem must take a probability law and prove
that almost every sampled trace has this property. -/
structure FirstSlotSamplingTrace
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (protocolAction : LocalConsensusAction → Prop)
    (processing : BoundedLocalProcessing network protocolAction)
    (entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    (production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry)
    (anchorCount : Nat) where
  eventuallyProgressingCandidateWindow :
    ∀ baseline start,
      network.gst ≤ start →
      RecoveryLayerWindowAt view thresholds baseline
        (requiredRecoveryLayerCount anchorCount) (trace start) →
      ∃ candidate, start ≤ candidate ∧
        let deadline := candidate +
          production.windowBound (requiredRecoveryLayerCount anchorCount)
        view.commitIndex (trace deadline) = baseline →
        ∀ offset, offset < anchorCount →
          FirstSelectedLeaderIsProgressing view
            (production.baseRound candidate + offset) (trace deadline)

/-- Mapping from the process-selected candidate round to the pending-round array
and indirect scan used by FlexCommitter. -/
structure RecoveryAnchorWindowMapping
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (protocolAction : LocalConsensusAction → Prop)
    (processing : BoundedLocalProcessing network protocolAction)
    (entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    (production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry)
    (depth : Nat) where
  baseIndex : Time → Nat
  chosenBaseMatchesPendingRound :
    ∀ baseline start,
      RecoveryQuorumAt view thresholds baseline (trace start) →
      let deadline := start + production.windowBound
        (requiredRecoveryLayerCount (depth + 1))
      view.commitIndex (trace deadline) = baseline →
      production.baseRound start =
        view.firstPendingLeaderRound (trace deadline) + baseIndex start
  chosenBaseIndexIsScanned :
    ∀ baseline start,
      RecoveryQuorumAt view thresholds baseline (trace start) →
      let deadline := start + production.windowBound
        (requiredRecoveryLayerCount (depth + 1))
      view.commitIndex (trace deadline) = baseline →
      baseIndex start ≤ view.highestIndirectDecisionIndex (trace deadline)
  chosenWindowIsInRange :
    ∀ baseline start,
      RecoveryQuorumAt view thresholds baseline (trace start) →
      let deadline := start + production.windowBound
        (requiredRecoveryLayerCount (depth + 1))
      view.commitIndex (trace deadline) = baseline →
      baseIndex start + depth < view.pendingRoundCount (trace deadline)

/-- Direct state rules for Rust's pending-round array and descending scan. -/
structure PendingRoundArrayRules {State : Type}
    (view : CommitProgressRecoveryView State) (depth : Nat) where
  pendingRoundIsInStoredRange :
    ∀ state round,
      view.pendingLeaderRound round state →
      view.firstPendingLeaderRound state ≤ round ∧
        round < view.firstPendingLeaderRound state +
          view.pendingRoundCount state
  coveredWindowBaseIsScanned :
    ∀ state index,
      index + depth < view.pendingRoundCount state →
      index ≤ view.highestIndirectDecisionIndex state

/-- The zero-based index of a round in the pending-round array. -/
def pendingRoundIndex {State : Type}
    (view : CommitProgressRecoveryView State) (round : Nat) (state : State) : Nat :=
  round - view.firstPendingLeaderRound state

/-- One pending leader round maps to one in-range stored index. -/
theorem pending_round_maps_to_stored_index
    {State : Type}
    (view : CommitProgressRecoveryView State)
    {depth : Nat}
    (rules : PendingRoundArrayRules view depth)
    {state : State} {round : Nat}
    (pending : view.pendingLeaderRound round state) :
    round = view.firstPendingLeaderRound state +
        pendingRoundIndex view round state ∧
      pendingRoundIndex view round state < view.pendingRoundCount state := by
  have bounds := rules.pendingRoundIsInStoredRange state round pending
  have roundMatch :
      round = view.firstPendingLeaderRound state +
        pendingRoundIndex view round state := by
    simp [pendingRoundIndex]
    omega
  have indexInRange :
      pendingRoundIndex view round state <
        view.pendingRoundCount state := by
    simp [pendingRoundIndex]
    omega
  exact ⟨roundMatch, indexInRange⟩

/-- If the first and last rounds of an anchor window are stored, the first round
is in the indirect-decision scan. Newer stored rounds can be anchor evidence only. -/
theorem pending_window_base_maps_to_scanned_index
    {State : Type}
    (view : CommitProgressRecoveryView State)
    {depth : Nat}
    (rules : PendingRoundArrayRules view depth)
    {state : State} {round : Nat}
    (basePending : view.pendingLeaderRound round state)
    (lastPending : view.pendingLeaderRound (round + depth) state) :
    round = view.firstPendingLeaderRound state +
        pendingRoundIndex view round state ∧
      pendingRoundIndex view round state + depth <
        view.pendingRoundCount state ∧
      pendingRoundIndex view round state ≤
        view.highestIndirectDecisionIndex state := by
  have baseMapped := pending_round_maps_to_stored_index view rules basePending
  have lastBounds := rules.pendingRoundIsInStoredRange state (round + depth)
    lastPending
  have windowInRange :
      pendingRoundIndex view round state + depth <
        view.pendingRoundCount state := by
    omega
  exact ⟨baseMapped.1, windowInRange,
    rules.coveredWindowBaseIsScanned state _ windowInRange⟩

/-- Pending-round range and scan rules construct the candidate-window mapping used
by the recovery theorem. -/
def recovery_anchor_window_mapping_from_pending_round_rules
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    {entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing}
    {production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry}
    (depth : Nat)
    (rules : PendingRoundArrayRules view depth) :
    RecoveryAnchorWindowMapping trace network view thresholds protocolAction
      processing entry production depth where
  baseIndex := fun start =>
    let deadline := start + production.windowBound
      (requiredRecoveryLayerCount (depth + 1))
    pendingRoundIndex view (production.baseRound start) (trace deadline)
  chosenBaseMatchesPendingRound := by
    intro baseline start recoveryAtStart
    dsimp
    intro atBaseline
    let layerCount := requiredRecoveryLayerCount (depth + 1)
    let deadline := start + production.windowBound layerCount
    have firstLayerPending := production.layerRoundIsPendingAtDeadline baseline
      start layerCount 0 recoveryAtStart (by
        simp [layerCount, requiredRecoveryLayerCount, directVoteRoundOffset])
      atBaseline
    have mapped := pending_round_maps_to_stored_index view rules
      firstLayerPending
    simpa [deadline, layerCount] using mapped.1
  chosenBaseIndexIsScanned := by
    intro baseline start recoveryAtStart
    dsimp
    intro atBaseline
    let layerCount := requiredRecoveryLayerCount (depth + 1)
    let deadline := start + production.windowBound layerCount
    have firstLayerPending := production.layerRoundIsPendingAtDeadline baseline
      start layerCount 0 recoveryAtStart (by
        simp [layerCount, requiredRecoveryLayerCount, directVoteRoundOffset])
      atBaseline
    have lastAnchorPending := production.layerRoundIsPendingAtDeadline baseline
      start layerCount depth recoveryAtStart (by
        simp [layerCount, requiredRecoveryLayerCount, directVoteRoundOffset]
        omega)
      atBaseline
    have mapped := pending_window_base_maps_to_scanned_index view rules
      firstLayerPending lastAnchorPending
    simpa [deadline, layerCount] using mapped.2.2
  chosenWindowIsInRange := by
    intro baseline start recoveryAtStart
    dsimp
    intro atBaseline
    let layerCount := requiredRecoveryLayerCount (depth + 1)
    let deadline := start + production.windowBound layerCount
    have firstLayerPending := production.layerRoundIsPendingAtDeadline baseline
      start layerCount 0 recoveryAtStart (by
        simp [layerCount, requiredRecoveryLayerCount, directVoteRoundOffset])
      atBaseline
    have lastAnchorPending := production.layerRoundIsPendingAtDeadline baseline
      start layerCount depth recoveryAtStart (by
        simp [layerCount, requiredRecoveryLayerCount, directVoteRoundOffset]
        omega)
      atBaseline
    have mapped := pending_window_base_maps_to_scanned_index view rules
      firstLayerPending lastAnchorPending
    simpa [deadline, layerCount] using mapped.2.1

/-- One candidate recovery window with a progressing first-slot run, timely voting,
and the pending-array mapping produces a covered usable anchor window by its
deadline. -/
theorem recovery_candidate_reaches_usable_anchors_by_deadline
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    {entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing}
    {production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry}
    (voting : TimelyFirstSlotVoting view thresholds)
    {depth : Nat}
    (mapping : RecoveryAnchorWindowMapping trace network view thresholds
      protocolAction processing entry production depth)
    {baseline start : Nat}
    (afterGst : network.gst ≤ start)
    (recoveryAtStart :
      RecoveryQuorumAt view thresholds baseline (trace start))
    (progressingIfStalled :
      let deadline := start +
        production.windowBound (requiredRecoveryLayerCount (depth + 1))
      view.commitIndex (trace deadline) = baseline →
      ∀ offset, offset < depth + 1 →
        FirstSelectedLeaderIsProgressing view
          (production.baseRound start + offset) (trace deadline)) :
    let deadline := start +
      production.windowBound (requiredRecoveryLayerCount (depth + 1))
    CommitAdvancedFrom view baseline (trace deadline) ∨
      HasCoveredUsableAnchorWindowAt view baseline depth (depth + 1)
        (trace deadline) := by
  let layerCount := requiredRecoveryLayerCount (depth + 1)
  let deadline := start + production.windowBound layerCount
  have result := recovery_quorum_has_chosen_layers_by_deadline production
    baseline layerCount start afterGst recoveryAtStart
  rcases result with advanced | ⟨recoveryAtDeadline, layersAtDeadline⟩
  · exact Or.inl advanced
  · have atBaseline := recoveryAtDeadline.1
    have baseMatch := mapping.chosenBaseMatchesPendingRound baseline start
      recoveryAtStart atBaseline
    have baseScanned := mapping.chosenBaseIndexIsScanned baseline start
      recoveryAtStart atBaseline
    have windowInRange := mapping.chosenWindowIsInRange baseline start
      recoveryAtStart atBaseline
    have layersAtPendingBase :
        ConsecutiveQuorumBlockLayers view thresholds
          (view.firstPendingLeaderRound (trace deadline) + mapping.baseIndex start)
          (requiredRecoveryLayerCount (depth + 1)) (trace deadline) := by
      rw [← baseMatch]
      rw [production.baseRoundIsRecoveryFrontier baseline start recoveryAtStart]
      exact layersAtDeadline
    have sampledRun := progressingIfStalled atBaseline
    have progressingAtPendingBase :
        ∀ offset, offset < depth + 1 →
          FirstSelectedLeaderIsProgressing view
            (view.firstPendingLeaderRound (trace deadline) +
              mapping.baseIndex start + offset) (trace deadline) := by
      intro offset offsetInRun
      rw [← baseMatch]
      exact sampledRun offset offsetInRun
    have covered := progressing_first_slot_run_gives_covered_anchor_window
      view thresholds voting atBaseline baseScanned windowInRange
      layersAtPendingBase progressingAtPendingBase
    exact Or.inr covered

/-- The bounded anchor-formation theorem gives the third unbounded recovery stage
used by commit-progress composition. -/
theorem recovery_layers_to_usable_anchors
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    {entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing}
    {production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry}
    (voting : TimelyFirstSlotVoting view thresholds)
    {depth : Nat}
    (sampling : FirstSlotSamplingTrace trace network view thresholds
      protocolAction processing entry production (depth + 1))
    (mapping : RecoveryAnchorWindowMapping trace network view thresholds
      protocolAction processing entry production depth)
    (baseline : Nat) :
    LeadsToAfter network.gst trace
      (RecoveryLayerWindowAt view thresholds baseline
        (requiredRecoveryLayerCount (depth + 1)))
      (fun state =>
        CommitAdvancedFrom view baseline state ∨
          HasCoveredUsableAnchorWindowAt view baseline depth (depth + 1) state) := by
  intro start afterGst layerWindowAtStart
  rcases sampling.eventuallyProgressingCandidateWindow baseline start afterGst
      layerWindowAtStart with ⟨candidate, startBeforeCandidate, sampledRun⟩
  have candidateAfterGst : network.gst ≤ candidate :=
    Nat.le_trans afterGst startBeforeCandidate
  have recoveryAtStart := layerWindowAtStart.1
  have baselineAtStart := recoveryAtStart.1
  have baselineLeCandidate :
      baseline ≤ view.commitIndex (trace candidate) := by
    rw [← baselineAtStart]
    exact entry.commitIndexMonotone start candidate startBeforeCandidate
  by_cases advancedAtCandidate :
      baseline < view.commitIndex (trace candidate)
  · exact ⟨candidate, startBeforeCandidate, Or.inl advancedAtCandidate⟩
  · have atBaselineCandidate :
        view.commitIndex (trace candidate) = baseline := by omega
    have recoveryAtCandidate := recovery_quorum_persists_while_stalled entry
      startBeforeCandidate recoveryAtStart atBaselineCandidate
    let deadline := candidate +
      production.windowBound (requiredRecoveryLayerCount (depth + 1))
    have candidateBeforeDeadline : candidate ≤ deadline := by simp [deadline]
    have result := recovery_candidate_reaches_usable_anchors_by_deadline
      voting mapping candidateAfterGst recoveryAtCandidate sampledRun
    exact ⟨deadline, Nat.le_trans startBeforeCandidate candidateBeforeDeadline,
      result⟩

/-- Distributed stages and the Rust mapping condition for commit progress
recovery.

The first three fields form a stable interface for stage composition.
`recovery_stages_from_abstract_process_contracts` constructs them from the
abstract contracts above. The last field states that the product records the
commit that the executable Lean model finds. -/
structure CommitProgressRecoveryStages
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake) where
  /-- Unless a commit occurs first, one set of correct validators with quorum stake
  stays in commit progress recovery. Once the stall predicate holds, derive this
  result from bounded local processing, recovery entry, recovery persistence, and
  the live correct stake bound. -/
  stalledToRecoveryQuorum :
    ∀ baseline,
      LeadsToAfter network.gst trace
        (CommitStalledAt view baseline)
        (fun state =>
          CommitAdvancedFrom view baseline state ∨
            RecoveryQuorumAt view thresholds baseline state)
  /-- Unless a commit occurs first, these validators are all in commit progress
  recovery in the same proposal rounds. They produce and exchange blocks for enough
  consecutive rounds, with quorum stake in each round. Derive this result from
  next-round proposals, parent synchronization, persistence, broadcast, bounded
  local processing, and post-GST delivery. -/
  recoveryQuorumToLayers :
    ∀ baseline,
      LeadsToAfter network.gst trace
        (RecoveryQuorumAt view thresholds baseline)
        (fun state =>
          CommitAdvancedFrom view baseline state ∨
            RecoveryLayerWindowAt view thresholds baseline
              (requiredRecoveryLayerCount
                (requiredRecoveryAnchorCount indirectCommitDepth)) state)
  /-- Unless a commit occurs first, enough consecutive rounds start with a correct
  leader whose block receives enough next-round votes. FlexCommitter can use these
  blocks to resolve older undecided rounds. Derive this result from timely delivery,
  leader-order sampling, the direct decision rule, recovery persistence, and
  retained pending-round state. -/
  recoveryLayersToUsableAnchors :
    ∀ baseline,
      LeadsToAfter network.gst trace
        (RecoveryLayerWindowAt view thresholds baseline
          (requiredRecoveryLayerCount
            (requiredRecoveryAnchorCount indirectCommitDepth)))
        (fun state =>
          CommitAdvancedFrom view baseline state ∨
            HasCoveredUsableAnchorWindowAt view baseline indirectCommitDepth
              (requiredRecoveryAnchorCount indirectCommitDepth) state)
  /-- When the executable Lean FlexCommitter model finds a commit, the Rust
  `FlexCommitter::try_commit` result is passed to `Core::post_commit`, which records
  the greater commit index. The current Rust code does this synchronously. This
  behavior is verified by code trace and focused Rust tests, and future changes
  must preserve it. The exact Rust-state mapping is not machine checked, so this
  field remains because Lean does not inspect Rust source. -/
  rustFlexCommitterResultIsRecorded :
    ∀ baseline,
      LeadsToAfter network.gst trace
        (ModeledFlexCommitterAdvancesAt view baseline indirectCommitDepth)
        (CommitAdvancedFrom view baseline)

/-- Construct the recovery-stage interface from abstract process contracts.

The inputs still contain distributed results. This theorem only converts those
contracts into the stable stage-composition interface. -/
theorem recovery_stages_from_abstract_process_contracts
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    (entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    (production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry)
    (voting : TimelyFirstSlotVoting view thresholds)
    (sampling : FirstSlotSamplingTrace trace network view thresholds
      protocolAction processing entry production
      (indirectCommitDepth + 1))
    (mapping : RecoveryAnchorWindowMapping trace network view thresholds
      protocolAction processing entry production indirectCommitDepth)
    (rustRecordsResult : ∀ baseline,
      LeadsToAfter network.gst trace
        (ModeledFlexCommitterAdvancesAt view baseline indirectCommitDepth)
        (CommitAdvancedFrom view baseline)) :
    CommitProgressRecoveryStages trace network view thresholds where
  stalledToRecoveryQuorum := fun baseline =>
    stalled_to_recovery_quorum entry baseline
  recoveryQuorumToLayers := fun baseline =>
    recovery_quorum_to_layers production baseline
      (requiredRecoveryLayerCount
        (requiredRecoveryAnchorCount indirectCommitDepth))
  recoveryLayersToUsableAnchors := fun baseline =>
    recovery_layers_to_usable_anchors voting sampling mapping baseline
  rustFlexCommitterResultIsRecorded := rustRecordsResult

/-- Construct the recovery stages from the two direct pending-array rules instead
of a complete candidate-window mapping. -/
theorem recovery_stages_from_abstract_process_contracts_and_pending_round_rules
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    (entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    (production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry)
    (voting : TimelyFirstSlotVoting view thresholds)
    (sampling : FirstSlotSamplingTrace trace network view thresholds
      protocolAction processing entry production
      (indirectCommitDepth + 1))
    (pendingRules : PendingRoundArrayRules view indirectCommitDepth)
    (rustRecordsResult : ∀ baseline,
      LeadsToAfter network.gst trace
        (ModeledFlexCommitterAdvancesAt view baseline indirectCommitDepth)
        (CommitAdvancedFrom view baseline)) :
    CommitProgressRecoveryStages trace network view thresholds := by
  let mapping := recovery_anchor_window_mapping_from_pending_round_rules
    (production := production) indirectCommitDepth pendingRules
  exact recovery_stages_from_abstract_process_contracts entry production voting
    sampling mapping rustRecordsResult

/-- The recovery stages and Rust mapping condition compose to commit-index
progress.

This theorem is the stable stage-composition lemma.
`commit_progress_recovery_from_abstract_process_contracts` derives its distributed
stages from the abstract process contracts in this file. -/
theorem commit_progress_recovery_stages_compose
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {baseline : Nat}
    (stages : CommitProgressRecoveryStages trace network view thresholds) :
    LeadsToAfter network.gst trace
      (CommitStalledAt view baseline)
      (CommitAdvancedFrom view baseline) := by
  intro start afterGst stalled
  rcases stages.stalledToRecoveryQuorum baseline start afterGst stalled with
    ⟨recoveryTime, startToRecovery, advanced | recoveryQuorumAt⟩
  · exact ⟨recoveryTime, startToRecovery, advanced⟩
  · have recoveryAfterGst : network.gst ≤ recoveryTime :=
      Nat.le_trans afterGst startToRecovery
    rcases stages.recoveryQuorumToLayers baseline recoveryTime
        recoveryAfterGst recoveryQuorumAt with
      ⟨layerTime, recoveryToLayers, advanced | layerWindowAt⟩
    · exact ⟨layerTime, Nat.le_trans startToRecovery recoveryToLayers, advanced⟩
    · have layerAfterGst : network.gst ≤ layerTime :=
        Nat.le_trans recoveryAfterGst recoveryToLayers
      rcases stages.recoveryLayersToUsableAnchors baseline layerTime
          layerAfterGst layerWindowAt with
        ⟨anchorTime, layersToAnchors, advanced | anchorWindowAt⟩
      · exact ⟨anchorTime,
          Nat.le_trans startToRecovery
            (Nat.le_trans recoveryToLayers layersToAnchors),
          advanced⟩
      · have anchorAfterGst : network.gst ≤ anchorTime :=
          Nat.le_trans layerAfterGst layersToAnchors
        have modeledAdvance :=
          covered_usable_anchor_window_advances_modeled_flex_committer view
          (depthPositive := by simp [indirectCommitDepth]) anchorWindowAt
        rcases stages.rustFlexCommitterResultIsRecorded baseline anchorTime
            anchorAfterGst modeledAdvance with
          ⟨commitTime, anchorsToCommit, advanced⟩
        exact ⟨commitTime,
          Nat.le_trans startToRecovery
            (Nat.le_trans recoveryToLayers
              (Nat.le_trans layersToAnchors anchorsToCommit)),
          advanced⟩

/-- Commit-progress composition from abstract process contracts.

This is not the final theorem boundary. `RecoveryEntryProcess.liveStakeIsQuorum`,
`RecoveryLayerProductionProcess`, `TimelyFirstSlotVoting`, and
`FirstSlotSamplingTrace` contain distributed results that the final proof must
derive from fundamental network conditions and single-validator rules. The
`rustRecordsResult` argument is a temporal progress result, not only a static
source-to-model condition. -/
theorem commit_progress_recovery_from_abstract_process_contracts
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    (entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    (production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry)
    (voting : TimelyFirstSlotVoting view thresholds)
    (sampling : FirstSlotSamplingTrace trace network view thresholds
      protocolAction processing entry production
      (indirectCommitDepth + 1))
    (mapping : RecoveryAnchorWindowMapping trace network view thresholds
      protocolAction processing entry production indirectCommitDepth)
    (rustRecordsResult : ∀ baseline,
      LeadsToAfter network.gst trace
        (ModeledFlexCommitterAdvancesAt view baseline indirectCommitDepth)
        (CommitAdvancedFrom view baseline))
    (baseline : Nat) :
    LeadsToAfter network.gst trace
      (CommitStalledAt view baseline)
      (CommitAdvancedFrom view baseline) := by
  let stages := recovery_stages_from_abstract_process_contracts entry production
    voting sampling mapping rustRecordsResult
  exact commit_progress_recovery_stages_compose stages

/-- Commit-progress composition from abstract process contracts and the two direct
pending-array rules. -/
theorem commit_progress_recovery_from_abstract_process_contracts_and_pending_round_rules
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {protocolAction : LocalConsensusAction → Prop}
    {processing : BoundedLocalProcessing network protocolAction}
    (entry : RecoveryEntryProcess trace network view thresholds
      protocolAction processing)
    (production : RecoveryLayerProductionProcess trace network view thresholds
      protocolAction processing entry)
    (voting : TimelyFirstSlotVoting view thresholds)
    (sampling : FirstSlotSamplingTrace trace network view thresholds
      protocolAction processing entry production
      (indirectCommitDepth + 1))
    (pendingRules : PendingRoundArrayRules view indirectCommitDepth)
    (rustRecordsResult : ∀ baseline,
      LeadsToAfter network.gst trace
        (ModeledFlexCommitterAdvancesAt view baseline indirectCommitDepth)
        (CommitAdvancedFrom view baseline))
    (baseline : Nat) :
    LeadsToAfter network.gst trace
      (CommitStalledAt view baseline)
      (CommitAdvancedFrom view baseline) := by
  let stages :=
    recovery_stages_from_abstract_process_contracts_and_pending_round_rules entry
      production voting sampling pendingRules rustRecordsResult
  exact commit_progress_recovery_stages_compose stages

end Mysticeti
