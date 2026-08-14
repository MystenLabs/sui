/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.Leader
import Mysticeti.Liveness

namespace Mysticeti

/-! Conditional commit-progress liveness for Mysticeti v3 recovery.

This theorem does not prove liveness for old leader blocks or transaction
inclusion. It proves that a stalled commit index eventually increases, subject to
the explicit recovery, multi-leader, synchronization, first-slot sampling, and
`FlexCommitter` refinement obligations in
`ASM-LIVE-COMMIT-PROGRESS-RECOVERY`, `ASM-LIVE-LEADER`, and
`ASM-LIVE-FIRST-SLOT-SAMPLING`.
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
  ∀ status, status ∈ statuses → status ≠ .undecided

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
          have finalHead := allFinal .undecided (by simp)
          exact False.elim (finalHead rfl)
      | commit => simp [UsableAnchorOrder]
      | skip =>
          simp only [UsableAnchorOrder]
          apply ih
          · intro status member
            exact allFinal status (by simp [member])
          · simpa [HasCommitResultInSelectedLeaderSlot] using hasCommit

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
  split <;> simp_all

/-- The first selected leader slot in the counterexample. -/
def alternatingFirstSelectedLeader (round : Nat) : ShuffleExampleValidator :=
  if round % 2 = 0 then .correct else .byzantine

theorem alternating_round_order_has_expected_first (round : Nat) :
    (alternatingRoundLeaderOrder round).head? =
      some (alternatingFirstSelectedLeader round) := by
  simp [alternatingRoundLeaderOrder, alternatingFirstSelectedLeader]

/-- A correct validator is first after every start round. -/
theorem alternating_order_has_eventual_correct_first (start : Nat) :
    ∃ round, start ≤ round ∧
      alternatingFirstSelectedLeader round = .correct := by
  refine ⟨2 * start, ?_, ?_⟩
  · omega
  · simp [alternatingFirstSelectedLeader, Nat.mul_mod]

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

/-- A window of `anchorCount` adjacent anchors needs the candidate quorum block
layers and the last anchor's direct-vote layer. -/
def requiredRecoveryLayerCount (anchorCount : Nat) : Nat :=
  anchorCount + directVoteRoundOffset

/-- The current v3 distances derive the layer count. The value is not an independent
recovery constant. -/
theorem current_v3_recovery_layer_count :
    requiredRecoveryLayerCount indirectCommitDepth = 3 := by
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
  stallExpired : State → Prop
  correctRecoveryAuthorities : State → VoterSet
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

def leaderScheduleStake {State : Type}
    (view : CommitProgressRecoveryView State) (state : State) : Nat :=
  weight view.authorityCount view.stake (view.leaderSchedule state)

def roundLeaderSelectionStake {State : Type}
    (view : CommitProgressRecoveryView State) (round : Nat) (state : State) : Nat :=
  weight view.authorityCount view.stake (view.roundLeaderSelection round state)

def correctRecoveryStake {State : Type}
    (view : CommitProgressRecoveryView State) (state : State) : Nat :=
  weight view.authorityCount view.stake (view.correctRecoveryAuthorities state)

def quorumBlockLayerStake {State : Type}
    (view : CommitProgressRecoveryView State) (round : Nat) (state : State) : Nat :=
  weight view.authorityCount view.stake (view.quorumBlockLayerAuthors round state)

def selectedLeaderSlotValidators {State : Type}
    (view : CommitProgressRecoveryView State) (round : Nat) (state : State) :
    List Nat :=
  (view.selectedLeaderSlots round state).map SelectedLeaderSlotView.validator

def selectedLeaderSlotStatuses {State : Type}
    (view : CommitProgressRecoveryView State) (round : Nat) (state : State) :
    List SelectedLeaderSlotStatus :=
  (view.selectedLeaderSlots round state).map SelectedLeaderSlotView.status

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

def RecoveryQuorum {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake) : State → Prop :=
  fun state =>
    thresholds.quorum ≤ view.correctRecoveryStake state ∧
      VoterSet.SubsetAt view.authorityCount
        (view.correctRecoveryAuthorities state)
        (VoterSet.diff VoterSet.full (view.nonProgress state))

def RetainedQuorumBlockLayer {State : Type}
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (round : Nat) : State → Prop :=
  fun state =>
    view.pendingLeaderRound round state ∧
      thresholds.quorum ≤ view.quorumBlockLayerStake round state ∧
      Retained (view.gcBoundary state) round

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

def FullyDecidedCommitRound {State : Type}
    (view : CommitProgressRecoveryView State) (round : Nat) : State → Prop :=
  fun state =>
    let statuses := view.selectedLeaderSlotStatuses round state
    AllSelectedLeaderSlotsFinal statuses ∧
      HasCommitResultInSelectedLeaderSlot statuses

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

def HasUsableAnchorWindowAt {State : Type}
    (view : CommitProgressRecoveryView State)
    (baseline count : Nat) : State → Prop :=
  fun state => AtCommitIndex view baseline state ∧
    HasUsableAnchorWindow view count state

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

theorem quorum_block_layer_window_yields_usable_anchor_window
    {State : Type} (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    {state : State} {anchorCount : Nat}
    (usableAnchorFromLayers :
      ∀ round,
        RetainedQuorumBlockLayer view thresholds round state →
        RetainedQuorumBlockLayer view thresholds
          (round + directVoteRoundOffset) state →
        UsableAnchorRound view round state)
    (window : HasQuorumBlockLayerWindow view thresholds
      (requiredRecoveryLayerCount anchorCount) state) :
    HasUsableAnchorWindow view anchorCount state := by
  rcases window with ⟨base, layers⟩
  exact ⟨base, quorum_block_layer_window_yields_anchor_window view thresholds
    usableAnchorFromLayers layers⟩

/-- A recovery proposal must move forward from the last block signed by the same
authority. -/
structure RecoveryProposal where
  lastSignedRound : Nat
  proposalRound : Nat
  strictlyForward : lastSignedRound < proposalRound

theorem recovery_proposal_is_not_old (proposal : RecoveryProposal) :
    ¬proposal.proposalRound ≤ proposal.lastSignedRound := by
  exact Nat.not_le_of_gt proposal.strictlyForward

/-- Refinement obligations for commit progress recovery.

The temporal fields are assumed progress stages. They include post-GST delivery,
correct clock progress, block sync, fair task execution, persistent recovery despite
future round jumps, and retained recent evidence. The selected leader slot field is
the main multi-leader obligation. The last field maps the anchor window to the
descending `FlexCommitter` scan. -/
structure CommitProgressRecoveryAssumptions
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (view : CommitProgressRecoveryView State)
    (thresholds : Thresholds view.authorityCount view.stake)
    (nonProgressStakeBound : Nat) where
  recoveryTimeout : Time
  recoveryTimeoutPositive : 0 < recoveryTimeout
  /-- Byzantine stake is bounded in every modeled state. -/
  faultBounded :
    ∀ state, FaultBounded thresholds (view.byzantine state)
  /-- Byzantine plus crashed stake is at most the non-progress bound after GST. -/
  nonProgressStakeBounded :
    ∀ state,
      weight view.authorityCount view.stake (view.nonProgress state) ≤
        nonProgressStakeBound
  certificationExceedsNonProgress :
    nonProgressStakeBound < thresholds.certificate
  /-- The v3 leader schedule reaches the certification threshold. -/
  scheduleReachesCertification :
    ∀ state, thresholds.certificate ≤ view.leaderScheduleStake state
  /-- Current v3 selects the full leader schedule for each pending leader round. -/
  v3SelectsFullSchedule :
    ∀ state round,
      view.pendingLeaderRound round state →
      view.roundLeaderSelection round state = view.leaderSchedule state
  /-- Equal commit indices identify one common committed prefix in the modeled
  execution. This is the common commit chain assumption. -/
  commonCommittedPrefixAtIndex :
    ∀ left right,
      view.commitIndex left = view.commitIndex right →
      view.committedPrefixId left = view.committedPrefixId right
  /-- One committed prefix determines one leader schedule membership set. -/
  scheduleStableAtCommittedPrefix :
    ∀ left right,
      view.committedPrefixId left = view.committedPrefixId right →
      view.leaderSchedule left = view.leaderSchedule right
  /-- One committed prefix and round determine one selected leader slot order. -/
  selectedLeaderSlotOrderStableAtCommittedPrefix :
    ∀ left right round,
      view.committedPrefixId left = view.committedPrefixId right →
      view.selectedLeaderSlotValidators round left =
        view.selectedLeaderSlotValidators round right
  /-- Every Byzantine validator is in the non-progress set. -/
  byzantineIncludedInNonProgress :
    ∀ state,
      VoterSet.SubsetAt view.authorityCount
        (view.byzantine state) (view.nonProgress state)
  /-- The non-progress set is the union of Byzantine and crashed or unavailable
  validators. -/
  nonProgressSetDefinition :
    ∀ state,
      view.nonProgress state =
        VoterSet.union
          (view.byzantine state) (view.crashedOrUnavailable state)
  /-- Each validator counted in the recovery quorum is correct and non-crashed. -/
  correctRecoveryAuthoritiesExcludeNonProgress :
    ∀ state,
      VoterSet.SubsetAt view.authorityCount
        (view.correctRecoveryAuthorities state)
        (VoterSet.diff VoterSet.full (view.nonProgress state))
  /-- A commit can advance before the timers overlap. Otherwise, correct,
  non-crashed validators with quorum stake are eventually in recovery at the same
  time. This stage includes local clock progress and bounded drift.
  `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`. -/
  stalledToRecoveryQuorum :
    0 < recoveryTimeout →
    ∀ baseline,
      LeadsToAfter network.gst trace
        (CommitStalledAt view baseline)
        (fun state =>
          CommitAdvancedFrom view baseline state ∨
            RecoveryQuorumAt view thresholds baseline state)
  /-- Persistent recovery at the baseline commit index eventually creates a recent
  consecutive quorum block layer window. This includes future jumps and does not
  choose a base round in advance.
  `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`. -/
  recoveryQuorumToLayers :
    ∀ baseline,
      LeadsToAfter network.gst trace
        (RecoveryQuorumAt view thresholds baseline)
        (fun state =>
          CommitAdvancedFrom view baseline state ∨
            HasQuorumBlockLayerWindowAt view thresholds baseline
              (requiredRecoveryLayerCount indirectCommitDepth) state)
  /-- Recovery conditions make the per-round Rust scan fragment find a commit
  result before an undecided selected leader slot. Quorum coverage alone does not
  prove this property. The current design combines a schedule-independent
  propagation delay with the accepted independent uniform shuffle abstraction. This
  abstraction makes required adjacent rounds eventually have correct validators in
  their first selected leader slots. `ASM-LIVE-LEADER` and
  `ASM-LIVE-FIRST-SLOT-SAMPLING`. -/
  usableAnchorFromRecoveryConditions :
    ∀ state round,
      RoundLeaderSelectionCoverageBounds thresholds.certificate
        (view.leaderScheduleStake state)
        (view.roundLeaderSelectionStake round state) →
      0 < weight view.authorityCount view.stake
        (VoterSet.diff
          (VoterSet.inter
            (view.roundLeaderSelection round state)
            (view.quorumBlockLayerAuthors round state))
          (view.byzantine state)) →
      RetainedQuorumBlockLayer view thresholds round state →
      RetainedQuorumBlockLayer view thresholds
        (round + directVoteRoundOffset) state →
      UsableAnchorRound view round state
  /-- A depth-sized usable anchor window at the baseline commit index lets the
  descending indirect scan finish an older pending prefix and increase the index.
  `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`. -/
  anchorWindowToCommit :
    ∀ baseline,
      LeadsToAfter network.gst trace
        (HasUsableAnchorWindowAt view baseline indirectCommitDepth)
        (CommitAdvancedFrom view baseline)

/-- Under standard partial synchrony and the stated recovery refinement
obligations, a stalled commit index eventually increases. -/
theorem commit_progress_recovery_liveness
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {view : CommitProgressRecoveryView State}
    {thresholds : Thresholds view.authorityCount view.stake}
    {nonProgressStakeBound baseline : Nat}
    (assumptions : CommitProgressRecoveryAssumptions
      trace network view thresholds nonProgressStakeBound) :
    LeadsToAfter network.gst trace
      (CommitStalledAt view baseline)
      (CommitAdvancedFrom view baseline) := by
  intro start afterGst stalled
  rcases assumptions.stalledToRecoveryQuorum
      assumptions.recoveryTimeoutPositive baseline start afterGst stalled with
    ⟨recoveryTime, startToRecovery, advanced | recoveryQuorumAt⟩
  · exact ⟨recoveryTime, startToRecovery, advanced⟩
  · have recoveryAfterGst : network.gst ≤ recoveryTime :=
      Nat.le_trans afterGst startToRecovery
    rcases assumptions.recoveryQuorumToLayers baseline recoveryTime
        recoveryAfterGst recoveryQuorumAt with
      ⟨layerTime, recoveryToLayers, advanced | layerWindowAt⟩
    · exact ⟨layerTime, Nat.le_trans startToRecovery recoveryToLayers, advanced⟩
    · have layerAfterGst : network.gst ≤ layerTime :=
        Nat.le_trans recoveryAfterGst recoveryToLayers
      rcases layerWindowAt with ⟨sameCommitIndex, layerWindow⟩
      have usableAnchors :
          ∀ round,
            RetainedQuorumBlockLayer view thresholds round (trace layerTime) →
            RetainedQuorumBlockLayer view thresholds
              (round + directVoteRoundOffset) (trace layerTime) →
            UsableAnchorRound view round (trace layerTime) := by
        intro round leaderLayer voteLayer
        have fullSchedule := assumptions.v3SelectsFullSchedule
          (trace layerTime) round leaderLayer.1
        have selectionStakeEqSchedule :
            view.roundLeaderSelectionStake round (trace layerTime) =
              view.leaderScheduleStake (trace layerTime) := by
          simp [CommitProgressRecoveryView.roundLeaderSelectionStake,
            CommitProgressRecoveryView.leaderScheduleStake, fullSchedule]
        have selectionStakeLeSchedule :
            view.roundLeaderSelectionStake round (trace layerTime) ≤
              view.leaderScheduleStake (trace layerTime) := by
          exact weight_mono view.stake
            (view.roundSelectionFromSchedule (trace layerTime) round)
        have selectionCoverage :
            RoundLeaderSelectionCoverageBounds thresholds.certificate
              (view.leaderScheduleStake (trace layerTime))
              (view.roundLeaderSelectionStake round (trace layerTime)) := by
          constructor
          · have scheduleLower := assumptions.scheduleReachesCertification
              (trace layerTime)
            omega
          · exact selectionStakeLeSchedule
        have correctSelectionInLayer :=
          quorum_and_round_leader_selection_intersect_outside_byzantine_stake
            (assumptions.faultBounded (trace layerTime))
            selectionCoverage.reachesCertification leaderLayer.2.1
        exact assumptions.usableAnchorFromRecoveryConditions
          (trace layerTime) round selectionCoverage correctSelectionInLayer
          leaderLayer voteLayer
      have anchorWindow :=
        quorum_block_layer_window_yields_usable_anchor_window view thresholds
          usableAnchors layerWindow
      have anchorWindowAt :
          HasUsableAnchorWindowAt view baseline indirectCommitDepth
            (trace layerTime) :=
        ⟨sameCommitIndex, anchorWindow⟩
      rcases assumptions.anchorWindowToCommit baseline layerTime
          layerAfterGst anchorWindowAt with
        ⟨commitTime, layersToCommit, advanced⟩
      exact ⟨commitTime,
        Nat.le_trans startToRecovery (Nat.le_trans recoveryToLayers layersToCommit),
        advanced⟩

end Mysticeti
