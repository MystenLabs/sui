/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.RecursiveFlexAgreement

namespace Mysticeti

/-!
Recursive FlexCommitter agreement for a shared class of admissible exact
anchors.

The predicate is a static verifier and evidence classification. It is not a
claim that an anchor appears in a future execution. Initial direct commits are
in the class, and the indirect rule keeps the class closed. These facts prove
that each anchor used by the recursive scan is admissible. Direct results need
to agree only with these anchors.
-/

namespace ReferenceIndirectRule

variable {Digest History : Type}

/-- A final direct result agrees with every indirect result from an admissible
anchor. -/
def ReachableDirectResultConsistent
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (slot : ExactSelectedLeaderSlot)
    (directStatus : ReferenceSlotStatus Digest) : Prop :=
  ∀ anchor, anchorOK anchor → directStatus ≠ .undecided →
    rule.decide anchor slot = directStatus

/-- An admissible anchor can only produce admissible committed anchors. -/
def CommitAnchorClosed
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop) : Prop :=
  ∀ anchor slot block, anchorOK anchor →
    rule.decide anchor slot = .commit block → anchorOK block

theorem final_direct_compatible_reachable_decide
    (rule : ReferenceIndirectRule Digest History)
    {anchorOK : LeaderBlockRef Digest → Prop}
    {anchor : LeaderBlockRef Digest} {slot : ExactSelectedLeaderSlot}
    {directStatus : ReferenceSlotStatus Digest}
    (consistent :
      rule.ReachableDirectResultConsistent anchorOK slot directStatus)
    (anchorValid : anchorOK anchor)
    (final : directStatus ≠ .undecided) :
    directStatus.Compatible (rule.decide anchor slot) := by
  rw [consistent anchor anchorValid final]
  exact ReferenceSlotStatus.compatible_refl directStatus

theorem reachable_decide_compatible_final_direct
    (rule : ReferenceIndirectRule Digest History)
    {anchorOK : LeaderBlockRef Digest → Prop}
    {anchor : LeaderBlockRef Digest} {slot : ExactSelectedLeaderSlot}
    {directStatus : ReferenceSlotStatus Digest}
    (consistent :
      rule.ReachableDirectResultConsistent anchorOK slot directStatus)
    (anchorValid : anchorOK anchor)
    (final : directStatus ≠ .undecided) :
    (rule.decide anchor slot).Compatible directStatus := by
  rw [consistent anchor anchorValid final]
  exact ReferenceSlotStatus.compatible_refl directStatus

end ReferenceIndirectRule

/-- Direct-pass agreement for one slot, restricted to reachable anchors. -/
structure CrossViewReachableDirectSlotAgreement {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (left right : ReferenceSelectedSlotView Digest) : Prop where
  exactAgreement : CrossViewExactSlotAgreement left right
  leftConsistent :
    rule.ReachableDirectResultConsistent anchorOK left.slot left.status
  rightConsistent :
    rule.ReachableDirectResultConsistent anchorOK right.slot right.status

/-- Direct-pass agreement for one round, restricted to reachable anchors. -/
structure CrossViewReachableDirectRoundAgreement {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (left right : ReferenceFlexRoundView Digest) : Prop where
  sameRound : left.round = right.round
  selectedSlots : ExactListAgreement
    (CrossViewReachableDirectSlotAgreement rule anchorOK)
    left.selectedSlots right.selectedSlots

namespace ReferenceSelectedSlotView

/-- Every committed status in one selected-slot view has an admissible exact
anchor. -/
def CommittedAnchorValid {Digest : Type}
    (anchorOK : LeaderBlockRef Digest → Prop)
    (view : ReferenceSelectedSlotView Digest) : Prop :=
  match view.status with
  | .commit block => anchorOK block
  | .undecided | .skip => True

end ReferenceSelectedSlotView

/-- Every committed status in one ordered selected-slot list has an admissible
exact anchor. -/
def ReferenceSelectedSlotsCommittedAnchorsValid {Digest : Type}
    (anchorOK : LeaderBlockRef Digest → Prop) :
    List (ReferenceSelectedSlotView Digest) → Prop
  | [] => True
  | view :: tail =>
      view.CommittedAnchorValid anchorOK ∧
        ReferenceSelectedSlotsCommittedAnchorsValid anchorOK tail

namespace ReferenceFlexRoundView

/-- Every committed status in one round has an admissible exact anchor. -/
def CommittedAnchorsValid {Digest : Type}
    (anchorOK : LeaderBlockRef Digest → Prop)
    (round : ReferenceFlexRoundView Digest) : Prop :=
  ReferenceSelectedSlotsCommittedAnchorsValid anchorOK round.selectedSlots

end ReferenceFlexRoundView

/-- Every committed status in an ordered round list has an admissible exact
anchor. -/
def ReferenceFlexRoundsCommittedAnchorsValid {Digest : Type}
    (anchorOK : LeaderBlockRef Digest → Prop) :
    List (ReferenceFlexRoundView Digest) → Prop
  | [] => True
  | round :: tail =>
      round.CommittedAnchorsValid anchorOK ∧
        ReferenceFlexRoundsCommittedAnchorsValid anchorOK tail

/-- A successful selected-slot scan returns an admissible committed anchor. -/
theorem scan_reference_selected_slots_found_anchor_valid
    {Digest : Type} {anchorOK : LeaderBlockRef Digest → Prop}
    {views : List (ReferenceSelectedSlotView Digest)}
    (valid : ReferenceSelectedSlotsCommittedAnchorsValid anchorOK views)
    {anchor : LeaderBlockRef Digest}
    (found : scanReferenceSelectedSlots views = .found anchor) :
    anchorOK anchor := by
  induction views with
  | nil => simp [scanReferenceSelectedSlots] at found
  | cons view tail ih =>
      rcases valid with ⟨headValid, tailValid⟩
      rcases view with ⟨slot, status⟩
      cases status with
      | undecided => simp [scanReferenceSelectedSlots] at found
      | commit block =>
          have same : block = anchor := by
            simpa [scanReferenceSelectedSlots] using found
          rw [← same]
          exact headValid
      | skip =>
          exact ih tailValid (by simpa [scanReferenceSelectedSlots] using found)

/-- A successful eligible-round scan returns an admissible committed anchor. -/
theorem scan_reference_anchor_at_or_above_found_anchor_valid
    {Digest : Type} {anchorOK : LeaderBlockRef Digest → Prop}
    {minimumRound : Nat} {rounds : List (ReferenceFlexRoundView Digest)}
    (valid : ReferenceFlexRoundsCommittedAnchorsValid anchorOK rounds)
    {anchor : LeaderBlockRef Digest}
    (found : scanReferenceAnchorAtOrAbove minimumRound rounds = .found anchor) :
    anchorOK anchor := by
  induction rounds with
  | nil => simp [scanReferenceAnchorAtOrAbove] at found
  | cons round tail ih =>
      rcases valid with ⟨headValid, tailValid⟩
      by_cases below : round.round < minimumRound
      · exact ih tailValid
          (by simpa [scanReferenceAnchorAtOrAbove, below] using found)
      · cases scanned : scanReferenceSelectedSlots round.selectedSlots with
        | blocked =>
            simp [scanReferenceAnchorAtOrAbove, below, scanned] at found
        | noAnchor =>
            exact ih tailValid
              (by simpa [scanReferenceAnchorAtOrAbove, below, scanned]
                using found)
        | found block =>
            have blockValid := scan_reference_selected_slots_found_anchor_valid
              headValid scanned
            have same : block = anchor := by
              simpa [scanReferenceAnchorAtOrAbove, below, scanned] using found
            rw [← same]
            exact blockValid

/-- Finishing one slot preserves admissible committed anchors when the selected
anchor is admissible and the indirect rule is closed. -/
theorem finish_reference_selected_slot_preserves_valid_anchor
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (closed : rule.CommitAnchorClosed anchorOK)
    {anchor : ReferenceAnchorScanResult Digest}
    (anchorValid : ∀ block, anchor = .found block → anchorOK block)
    {view : ReferenceSelectedSlotView Digest}
    (valid : view.CommittedAnchorValid anchorOK) :
    (finishReferenceSelectedSlot rule anchor view).CommittedAnchorValid
      anchorOK := by
  rcases view with ⟨slot, status⟩
  cases status with
  | undecided =>
      cases anchor with
      | blocked => trivial
      | noAnchor => trivial
      | found selectedAnchor =>
          have selectedValid := anchorValid selectedAnchor rfl
          cases decision : rule.decide selectedAnchor slot with
          | undecided =>
              simp [finishReferenceSelectedSlot,
                ReferenceSelectedSlotView.CommittedAnchorValid, decision]
          | commit block =>
              simpa [finishReferenceSelectedSlot,
                ReferenceSelectedSlotView.CommittedAnchorValid, decision] using
                closed selectedAnchor slot block selectedValid decision
          | skip =>
              simp [finishReferenceSelectedSlot,
                ReferenceSelectedSlotView.CommittedAnchorValid, decision]
  | commit block => exact valid
  | skip => trivial

/-- Finishing an ordered slot list preserves admissible committed anchors. -/
theorem finish_reference_selected_slots_preserves_valid_anchors
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (closed : rule.CommitAnchorClosed anchorOK)
    {anchor : ReferenceAnchorScanResult Digest}
    (anchorValid : ∀ block, anchor = .found block → anchorOK block)
    {views : List (ReferenceSelectedSlotView Digest)}
    (valid : ReferenceSelectedSlotsCommittedAnchorsValid anchorOK views) :
    ReferenceSelectedSlotsCommittedAnchorsValid anchorOK
      (finishReferenceSelectedSlots rule anchor views) := by
  induction views with
  | nil => trivial
  | cons view tail ih =>
      rcases valid with ⟨headValid, tailValid⟩
      exact ⟨
        (finish_reference_selected_slot_preserves_valid_anchor
          rule anchorOK closed anchorValid headValid)
        , ih tailValid⟩

/-- The high-to-low recursive finisher preserves the admissible-anchor
invariant for every configured indirect depth. -/
theorem finish_reference_flex_rounds_at_depth_preserves_valid_anchors
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (closed : rule.CommitAnchorClosed anchorOK)
    (depth : Nat)
    {rounds : List (ReferenceFlexRoundView Digest)}
    (valid : ReferenceFlexRoundsCommittedAnchorsValid anchorOK rounds) :
    ReferenceFlexRoundsCommittedAnchorsValid anchorOK
      (finishReferenceFlexRoundsAtDepth rule depth rounds) := by
  induction rounds with
  | nil => trivial
  | cons round tail ih =>
      rcases valid with ⟨headValid, tailValid⟩
      have tailValidFinished := ih tailValid
      have foundValid : ∀ block,
          scanReferenceAnchorAtOrAbove (round.round + depth)
              (finishReferenceFlexRoundsAtDepth rule depth tail) = .found block →
            anchorOK block := by
        intro block found
        exact scan_reference_anchor_at_or_above_found_anchor_valid
          tailValidFinished found
      exact ⟨
        (finish_reference_selected_slots_preserves_valid_anchors
          rule anchorOK closed foundValid headValid)
        , tailValidFinished⟩

/-- Predicate-scoped direct agreement is preserved when each view applies its
own admissible selected anchor. -/
theorem finish_reference_selected_slot_reachable_agrees
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    {leftAnchor rightAnchor : ReferenceAnchorScanResult Digest}
    (anchorsAgree : leftAnchor.FoundAgreement rightAnchor)
    (leftAnchorValid : ∀ block, leftAnchor = .found block → anchorOK block)
    (rightAnchorValid : ∀ block, rightAnchor = .found block → anchorOK block)
    {left right : ReferenceSelectedSlotView Digest}
    (directAgreement :
      CrossViewReachableDirectSlotAgreement rule anchorOK left right) :
    CrossViewExactSlotAgreement
      (finishReferenceSelectedSlot rule leftAnchor left)
      (finishReferenceSelectedSlot rule rightAnchor right) := by
  rcases directAgreement with
    ⟨exactAgreement, leftConsistent, rightConsistent⟩
  rcases left with ⟨leftSlot, leftStatus⟩
  rcases right with ⟨rightSlot, rightStatus⟩
  rcases exactAgreement with ⟨sameSlot, statusesCompatible⟩
  simp only at sameSlot
  subst rightSlot
  constructor
  · cases leftAnchor <;> cases rightAnchor <;>
      cases leftStatus <;> cases rightStatus <;> rfl
  · cases leftAnchor <;> cases rightAnchor <;>
      cases leftStatus <;> cases rightStatus <;>
      simp [finishReferenceSelectedSlot,
        ReferenceAnchorScanResult.FoundAgreement,
        ReferenceSlotStatus.Compatible, ReferenceIndirectRule.decide_final]
        at anchorsAgree statusesCompatible ⊢
    all_goals try assumption
    all_goals try subst_vars
    all_goals
      first
      | exact ReferenceSlotStatus.compatible_refl _
      | exact rule.final_direct_compatible_reachable_decide
          leftConsistent (leftAnchorValid _ rfl) (by simp)
      | exact rule.final_direct_compatible_reachable_decide
          leftConsistent (rightAnchorValid _ rfl) (by simp)
      | exact rule.final_direct_compatible_reachable_decide
          rightConsistent (leftAnchorValid _ rfl) (by simp)
      | exact rule.final_direct_compatible_reachable_decide
          rightConsistent (rightAnchorValid _ rfl) (by simp)
      | exact rule.reachable_decide_compatible_final_direct
          leftConsistent (leftAnchorValid _ rfl) (by simp)
      | exact rule.reachable_decide_compatible_final_direct
          leftConsistent (rightAnchorValid _ rfl) (by simp)
      | exact rule.reachable_decide_compatible_final_direct
          rightConsistent (leftAnchorValid _ rfl) (by simp)
      | exact rule.reachable_decide_compatible_final_direct
          rightConsistent (rightAnchorValid _ rfl) (by simp)

/-- The reachable-anchor selected-slot result extends pointwise to one round. -/
theorem finish_reference_selected_slots_reachable_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    {leftAnchor rightAnchor : ReferenceAnchorScanResult Digest}
    (anchorsAgree : leftAnchor.FoundAgreement rightAnchor)
    (leftAnchorValid : ∀ block, leftAnchor = .found block → anchorOK block)
    (rightAnchorValid : ∀ block, rightAnchor = .found block → anchorOK block)
    {left right : List (ReferenceSelectedSlotView Digest)}
    (directAgreement : ExactListAgreement
      (CrossViewReachableDirectSlotAgreement rule anchorOK) left right) :
    ExactListAgreement CrossViewExactSlotAgreement
      (finishReferenceSelectedSlots rule leftAnchor left)
      (finishReferenceSelectedSlots rule rightAnchor right) := by
  induction directAgreement with
  | nil => exact .nil
  | cons headAgreement tailAgreement ih =>
      exact .cons
        (finish_reference_selected_slot_reachable_agrees rule anchorOK
          anchorsAgree leftAnchorValid rightAnchorValid headAgreement)
        ih

/-- Reachable-anchor recursive agreement permits different local high-round
suffixes. -/
theorem recursive_reachable_flex_rounds_at_depth_prefix_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (closed : rule.CommitAnchorClosed anchorOK)
    (depth : Nat)
    {left right : List (ReferenceFlexRoundView Digest)}
    (leftValid : ReferenceFlexRoundsCommittedAnchorsValid anchorOK left)
    (rightValid : ReferenceFlexRoundsCommittedAnchorsValid anchorOK right)
    (directAgreement : ExactPrefixAgreement
      (CrossViewReachableDirectRoundAgreement rule anchorOK) left right) :
    ExactPrefixAgreement CrossViewExactRoundAgreement
      (finishReferenceFlexRoundsAtDepth rule depth left)
      (finishReferenceFlexRoundsAtDepth rule depth right) := by
  induction directAgreement with
  | leftNil => exact .leftNil
  | rightNil => exact .rightNil
  | @cons leftRound rightRound leftTail rightTail roundAgreement tailAgreement ih =>
      rcases leftValid with ⟨leftRoundValid, leftTailValid⟩
      rcases rightValid with ⟨rightRoundValid, rightTailValid⟩
      have tailFinishedAgreement := ih leftTailValid rightTailValid
      have leftTailFinishedValid :=
        finish_reference_flex_rounds_at_depth_preserves_valid_anchors
          rule anchorOK closed depth leftTailValid
      have rightTailFinishedValid :=
        finish_reference_flex_rounds_at_depth_preserves_valid_anchors
          rule anchorOK closed depth rightTailValid
      rcases roundAgreement with ⟨sameRound, selectedAgreement⟩
      let leftAnchor := scanReferenceAnchorAtOrAbove
        (leftRound.round + depth)
        (finishReferenceFlexRoundsAtDepth rule depth leftTail)
      let rightAnchor := scanReferenceAnchorAtOrAbove
        (rightRound.round + depth)
        (finishReferenceFlexRoundsAtDepth rule depth rightTail)
      have leftAnchorValid : ∀ block, leftAnchor = .found block →
          anchorOK block := by
        intro block found
        exact scan_reference_anchor_at_or_above_found_anchor_valid
          leftTailFinishedValid found
      have rightAnchorValid : ∀ block, rightAnchor = .found block →
          anchorOK block := by
        intro block found
        exact scan_reference_anchor_at_or_above_found_anchor_valid
          rightTailFinishedValid found
      have anchorsAgree : leftAnchor.FoundAgreement rightAnchor := by
        simpa [leftAnchor, rightAnchor, sameRound] using
          reference_anchor_at_or_above_prefix_found_agreement
            (minimumRound := leftRound.round + depth) tailFinishedAgreement
      have selectedFinished :=
        finish_reference_selected_slots_reachable_agree
          rule anchorOK anchorsAgree leftAnchorValid rightAnchorValid
          selectedAgreement
      refine .cons ?_ tailFinishedAgreement
      constructor
      · simpa [finishReferenceFlexRoundAtDepth] using sameRound
      · simpa [finishReferenceFlexRoundsAtDepth,
          finishReferenceFlexRoundAtDepth, leftAnchor, rightAnchor]
          using selectedFinished

/-- Two reachable-anchor executions derive the same explicit-depth commit
candidate. -/
theorem recursive_reachable_flex_commit_candidates_at_depth_prefix_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (closed : rule.CommitAnchorClosed anchorOK)
    (depth : Nat)
    {left right : List (ReferenceFlexRoundView Digest)}
    (leftValid : ReferenceFlexRoundsCommittedAnchorsValid anchorOK left)
    (rightValid : ReferenceFlexRoundsCommittedAnchorsValid anchorOK right)
    (directAgreement : ExactPrefixAgreement
      (CrossViewReachableDirectRoundAgreement rule anchorOK) left right)
    {leftCandidate rightCandidate : ReferenceFlexCandidate Digest}
    (leftFound :
      findReferenceFlexCommitCandidate
          (finishReferenceFlexRoundsAtDepth rule depth left) =
        some leftCandidate)
    (rightFound :
      findReferenceFlexCommitCandidate
          (finishReferenceFlexRoundsAtDepth rule depth right) =
        some rightCandidate) :
    leftCandidate = rightCandidate := by
  have finishedAgreement :=
    recursive_reachable_flex_rounds_at_depth_prefix_agree
      rule anchorOK closed depth leftValid rightValid directAgreement
  exact reference_flex_commit_candidates_agree finishedAgreement
    leftFound rightFound

end Mysticeti
