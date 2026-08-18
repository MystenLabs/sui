/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ReferenceFlexCommitter

namespace Mysticeti

/-!
Recursive exact-reference agreement for the ordered Mysticeti v3 FlexCommitter
scan.

The indirect rule can return different results for different causal histories.
This file does not require those histories to be comparable. Instead, the
ordered recursion proves that two successful scans use the same exact anchor
unless a higher selected-slot result already conflicts.

The local safety boundary has two parts. Direct results from two correct views
must be compatible. A final direct result must also equal the indirect result
for every valid exact anchor. These are vote-evidence obligations. They do not
assume agreement for two indirect results or one common commit candidate.
-/

/-- One deterministic indirect rule. The exact anchor reference selects its
immutable causal history. Different anchors can select incomparable histories
and can produce different results. -/
structure ReferenceIndirectRule (Digest History : Type) where
  historyOf : LeaderBlockRef Digest → History
  decideFromHistory : History → ExactSelectedLeaderSlot → ReferenceSlotStatus Digest
  decisionFinal :
    ∀ history slot, decideFromHistory history slot ≠ .undecided

namespace ReferenceIndirectRule

variable {Digest History : Type}

def decide (rule : ReferenceIndirectRule Digest History)
    (anchor : LeaderBlockRef Digest) (slot : ExactSelectedLeaderSlot) :
    ReferenceSlotStatus Digest :=
  rule.decideFromHistory (rule.historyOf anchor) slot

/-- The slot-level vote proof needed for one direct result that occurs in one
execution. Only valid anchors are supplied to this rule by the refinement. -/
def DirectResultConsistent (rule : ReferenceIndirectRule Digest History)
    (slot : ExactSelectedLeaderSlot)
    (directStatus : ReferenceSlotStatus Digest) : Prop :=
  ∀ anchor, directStatus ≠ .undecided →
    rule.decide anchor slot = directStatus

theorem decide_final (rule : ReferenceIndirectRule Digest History)
    (anchor : LeaderBlockRef Digest) (slot : ExactSelectedLeaderSlot) :
    rule.decide anchor slot ≠ .undecided := by
  exact rule.decisionFinal (rule.historyOf anchor) slot

theorem final_direct_compatible_decide
    (rule : ReferenceIndirectRule Digest History)
    {anchor : LeaderBlockRef Digest} {slot : ExactSelectedLeaderSlot}
    {directStatus : ReferenceSlotStatus Digest}
    (consistent : rule.DirectResultConsistent slot directStatus)
    (final : directStatus ≠ .undecided) :
    directStatus.Compatible (rule.decide anchor slot) := by
  rw [consistent anchor final]
  cases directStatus <;> simp [ReferenceSlotStatus.Compatible] at final ⊢

theorem decide_compatible_final_direct
    (rule : ReferenceIndirectRule Digest History)
    {anchor : LeaderBlockRef Digest} {slot : ExactSelectedLeaderSlot}
    {directStatus : ReferenceSlotStatus Digest}
    (consistent : rule.DirectResultConsistent slot directStatus)
    (final : directStatus ≠ .undecided) :
    (rule.decide anchor slot).Compatible directStatus := by
  rw [consistent anchor final]
  cases directStatus <;> simp [ReferenceSlotStatus.Compatible] at final ⊢

/-- Different indirect results require different causal histories. -/
theorem different_decisions_require_different_histories
    (rule : ReferenceIndirectRule Digest History)
    {leftAnchor rightAnchor : LeaderBlockRef Digest}
    {slot : ExactSelectedLeaderSlot}
    (different : rule.decide leftAnchor slot ≠ rule.decide rightAnchor slot) :
    rule.historyOf leftAnchor ≠ rule.historyOf rightAnchor := by
  intro sameHistory
  apply different
  simp only [decide, sameHistory]

end ReferenceIndirectRule

/-- Direct-pass agreement for one corresponding selected slot. The two local
direct results are compatible. Each final direct result also agrees with every
valid exact-anchor indirect result. -/
structure CrossViewDirectSlotAgreement {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (left right : ReferenceSelectedSlotView Digest) : Prop where
  exactAgreement : CrossViewExactSlotAgreement left right
  leftConsistent : rule.DirectResultConsistent left.slot left.status
  rightConsistent : rule.DirectResultConsistent right.slot right.status

/-- Direct-pass agreement for one corresponding round. -/
structure CrossViewDirectRoundAgreement {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (left right : ReferenceFlexRoundView Digest) : Prop where
  sameRound : left.round = right.round
  selectedSlots : ExactListAgreement (CrossViewDirectSlotAgreement rule)
    left.selectedSlots right.selectedSlots

namespace ReferenceSlotStatus

theorem compatible_refl {Digest : Type}
    (status : ReferenceSlotStatus Digest) : status.Compatible status := by
  cases status <;> simp [Compatible]

theorem compatible_symm {Digest : Type}
    {left right : ReferenceSlotStatus Digest}
    (compatible : left.Compatible right) : right.Compatible left := by
  cases left <;> cases right <;>
    simp [Compatible] at compatible ⊢
  case commit.commit => exact compatible.symm

end ReferenceSlotStatus

namespace LeaderBlockRef

/-- An exact block reference belongs to one selected leader slot. The fixed
epoch configuration is outside both values. -/
def AtSelectedSlot {Digest : Type} (block : LeaderBlockRef Digest)
    (slot : ExactSelectedLeaderSlot) : Prop :=
  block.round = slot.round ∧ block.author = slot.validator

theorem same_selected_slot_of_at_slot {Digest : Type}
    {left right : LeaderBlockRef Digest} {slot : ExactSelectedLeaderSlot}
    (leftAt : left.AtSelectedSlot slot) (rightAt : right.AtSelectedSlot slot) :
    left.SameSelectedSlot right := by
  exact ⟨leftAt.1.trans rightAt.1.symm, leftAt.2.trans rightAt.2.symm⟩

end LeaderBlockRef

/-- Exact evidence for one result of the Rust direct slot rule. A direct slot
skip supplies a reject quorum for every exact branch in the slot. For an omitted
branch, dependency closure must derive this quorum from the complete voting
layer. -/
def ExactDirectStatusValid
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (commitVotes skipVotes : LeaderBlockRef Digest → VoterSet)
    (slot : ExactSelectedLeaderSlot) : ReferenceSlotStatus Digest → Prop
  | .undecided => True
  | .commit block =>
      block.AtSelectedSlot slot ∧
        thresholds.quorum ≤ weight authorityCount stake (commitVotes block)
  | .skip =>
      ∀ block, block.AtSelectedSlot slot →
        thresholds.quorum ≤ weight authorityCount stake (skipVotes block)

/-- Exact evidence for the Rust indirect rule. It commits the unique certified
branch. It skips when no unique certified branch exists. This includes both zero
certificates and multiple certified equivocations. -/
def ExactIndirectStatusValid
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (history : LeaderAnchorHistory Digest authorityCount stake thresholds)
    (slot : ExactSelectedLeaderSlot) : ReferenceSlotStatus Digest → Prop
  | .undecided => False
  | .commit block =>
      block.AtSelectedSlot slot ∧ ExactIndirectCommit history block
  | .skip =>
      ¬∃ block, block.AtSelectedSlot slot ∧ ExactIndirectCommit history block

/-- Cross-view vote and causal-closure facts for one direct view and one exact
anchor history. -/
structure DirectAnchorEvidence
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (commitVotes skipVotes : LeaderBlockRef Digest → VoterSet)
    (history : LeaderAnchorHistory Digest authorityCount stake thresholds) where
  faulty : VoterSet
  faultBounded : FaultBounded thresholds faulty
  commitOtherCertificateOverlap :
    ∀ directRef certifiedRef,
      certifiedRef.SameSelectedSlot directRef → directRef ≠ certifiedRef →
      OnlyFaultyOverlap authorityCount faulty
        (commitVotes directRef) (history.certificateVotes certifiedRef)
  skipCertificateOverlap :
    ∀ block, OnlyFaultyOverlap authorityCount faulty
      (skipVotes block) (history.certificateVotes block)
  correctCommitAnchorInCertificate :
    ∀ block, VoterSet.SubsetAt authorityCount
      (VoterSet.diff
        (VoterSet.inter (commitVotes block) history.votingLayer) faulty)
      (history.certificateVotes block)

namespace DirectAnchorEvidence

variable {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}
variable {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
variable {history : LeaderAnchorHistory Digest authorityCount stake thresholds}

/-- A direct quorum forces its exact branch to be the unique certificate in
every valid anchor history. This also excludes a multi-certificate skip. -/
theorem direct_commit_forces_exact_indirect_commit
    (evidence : DirectAnchorEvidence thresholds commitVotes skipVotes history)
    {block : LeaderBlockRef Digest}
    (committed :
      thresholds.quorum ≤ weight authorityCount stake (commitVotes block)) :
    ExactIndirectCommit history block := by
  have preserved := quorum_intersection_preserves_certificate
    evidence.faultBounded committed history.votingLayerQuorum
  have included := weight_mono stake
    (evidence.correctCommitAnchorInCertificate block)
  have blockCertified : history.HasCertificate block := by
    unfold LeaderAnchorHistory.HasCertificate
    exact Nat.le_trans preserved included
  refine
    { selectedCertified := blockCertified
      onlyCertifiedAtSlot := ?_ }
  intro candidate sameSlot candidateCertified
  by_cases same : candidate = block
  · exact same
  · have directDifferent : block ≠ candidate := by
      intro blockEq
      exact same blockEq.symm
    have overlap := evidence.commitOtherCertificateOverlap
      block candidate sameSlot directDifferent
    exact False.elim (incompatible_quorum_certificate_impossible
      evidence.faultBounded overlap committed candidateCertified)

theorem direct_commit_is_certified
    (evidence : DirectAnchorEvidence thresholds commitVotes skipVotes history)
    {block : LeaderBlockRef Digest}
    (committed :
      thresholds.quorum ≤ weight authorityCount stake (commitVotes block)) :
    history.HasCertificate block :=
  (evidence.direct_commit_forces_exact_indirect_commit committed).selectedCertified

theorem direct_commit_excludes_other_certificate
    (evidence : DirectAnchorEvidence thresholds commitVotes skipVotes history)
    {block other : LeaderBlockRef Digest}
    (committed :
      thresholds.quorum ≤ weight authorityCount stake (commitVotes block))
    (sameSlot : other.SameSelectedSlot block)
    (otherCertified : history.HasCertificate other) :
    other = block :=
  (evidence.direct_commit_forces_exact_indirect_commit committed).onlyCertifiedAtSlot
    other sameSlot otherCertified

theorem direct_skip_excludes_certificate
    (evidence : DirectAnchorEvidence thresholds commitVotes skipVotes history)
    {block : LeaderBlockRef Digest}
    (skipped :
      thresholds.quorum ≤ weight authorityCount stake (skipVotes block))
    (certified : history.HasCertificate block) : False := by
  exact incompatible_quorum_certificate_impossible evidence.faultBounded
    (evidence.skipCertificateOverlap block) skipped certified

/-- One valid final direct result equals one valid exact indirect result. This
theorem covers direct commit versus multi-certificate skip and direct skip versus
indirect commit. -/
theorem valid_direct_indirect_statuses_equal
    (evidence : DirectAnchorEvidence thresholds commitVotes skipVotes history)
    {slot : ExactSelectedLeaderSlot}
    {directStatus indirectStatus : ReferenceSlotStatus Digest}
    (directValid : ExactDirectStatusValid thresholds commitVotes skipVotes
      slot directStatus)
    (directFinal : directStatus ≠ .undecided)
    (indirectValid : ExactIndirectStatusValid history slot indirectStatus) :
    indirectStatus = directStatus := by
  cases directStatus with
  | undecided => exact False.elim (directFinal rfl)
  | commit directBlock =>
      rcases directValid with ⟨directAt, directQuorum⟩
      have forced := evidence.direct_commit_forces_exact_indirect_commit directQuorum
      cases indirectStatus with
      | undecided => exact False.elim indirectValid
      | commit indirectBlock =>
          rcases indirectValid with ⟨indirectAt, indirectCommitted⟩
          have sameSlot := LeaderBlockRef.same_selected_slot_of_at_slot
            directAt indirectAt
          have sameBlock := indirectCommitted.onlyCertifiedAtSlot
            directBlock sameSlot forced.selectedCertified
          rw [sameBlock]
      | skip =>
          exact False.elim (indirectValid ⟨directBlock, directAt, forced⟩)
  | skip =>
      cases indirectStatus with
      | undecided => exact False.elim indirectValid
      | commit indirectBlock =>
          rcases indirectValid with ⟨indirectAt, indirectCommitted⟩
          have skippedQuorum := directValid indirectBlock indirectAt
          have overlap := evidence.skipCertificateOverlap indirectBlock
          exact False.elim (incompatible_quorum_certificate_impossible
            evidence.faultBounded overlap skippedQuorum
              indirectCommitted.selectedCertified)
      | skip => rfl

end DirectAnchorEvidence

/-- Cross-view vote facts for two direct passes on one selected slot. -/
structure TwoDirectSlotEvidence
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (leftCommitVotes leftSkipVotes rightCommitVotes rightSkipVotes :
      LeaderBlockRef Digest → VoterSet) where
  faulty : VoterSet
  faultBounded : FaultBounded thresholds faulty
  commitCommitOverlap :
    ∀ leftBlock rightBlock,
      leftBlock.SameSelectedSlot rightBlock → leftBlock ≠ rightBlock →
      OnlyFaultyOverlap authorityCount faulty
        (leftCommitVotes leftBlock) (rightCommitVotes rightBlock)
  leftCommitRightSkipOverlap :
    ∀ block, OnlyFaultyOverlap authorityCount faulty
      (leftCommitVotes block) (rightSkipVotes block)
  rightCommitLeftSkipOverlap :
    ∀ block, OnlyFaultyOverlap authorityCount faulty
      (rightCommitVotes block) (leftSkipVotes block)

namespace TwoDirectSlotEvidence

variable {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}
variable {leftCommitVotes leftSkipVotes rightCommitVotes rightSkipVotes :
  LeaderBlockRef Digest → VoterSet}

/-- Authenticated direct evidence gives the direct-pass compatibility premise
used by the recursive Flex theorem. -/
theorem valid_direct_statuses_compatible
    (evidence : TwoDirectSlotEvidence thresholds
      leftCommitVotes leftSkipVotes rightCommitVotes rightSkipVotes)
    {slot : ExactSelectedLeaderSlot}
    {leftStatus rightStatus : ReferenceSlotStatus Digest}
    (leftValid : ExactDirectStatusValid thresholds
      leftCommitVotes leftSkipVotes slot leftStatus)
    (rightValid : ExactDirectStatusValid thresholds
      rightCommitVotes rightSkipVotes slot rightStatus) :
    leftStatus.Compatible rightStatus := by
  cases leftStatus with
  | undecided => simp [ReferenceSlotStatus.Compatible]
  | commit leftBlock =>
      rcases leftValid with ⟨leftAt, leftQuorum⟩
      cases rightStatus with
      | undecided => simp [ReferenceSlotStatus.Compatible]
      | commit rightBlock =>
          rcases rightValid with ⟨rightAt, rightQuorum⟩
          by_cases same : leftBlock = rightBlock
          · simpa [ReferenceSlotStatus.Compatible] using same
          · have sameSlot := LeaderBlockRef.same_selected_slot_of_at_slot
              leftAt rightAt
            have overlap := evidence.commitCommitOverlap
              leftBlock rightBlock sameSlot same
            exact False.elim (incompatible_quorums_impossible
              evidence.faultBounded overlap leftQuorum rightQuorum)
      | skip =>
          have rightSkipQuorum := rightValid leftBlock leftAt
          have overlap := evidence.leftCommitRightSkipOverlap leftBlock
          exact False.elim (incompatible_quorums_impossible
            evidence.faultBounded overlap leftQuorum rightSkipQuorum)
  | skip =>
      cases rightStatus with
      | undecided => simp [ReferenceSlotStatus.Compatible]
      | commit rightBlock =>
          rcases rightValid with ⟨rightAt, rightQuorum⟩
          have leftSkipQuorum := leftValid rightBlock rightAt
          have overlap := evidence.rightCommitLeftSkipOverlap rightBlock
          exact False.elim (incompatible_quorums_impossible
            evidence.faultBounded overlap rightQuorum leftSkipQuorum)
      | skip => simp [ReferenceSlotStatus.Compatible]

end TwoDirectSlotEvidence

/-- Exact vote evidence discharges one `CrossViewDirectSlotAgreement` premise.
The indirect rule can still use different, incomparable histories for different
anchor references. -/
theorem cross_view_direct_slot_agreement_of_exact_evidence
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (rule : ReferenceIndirectRule Digest
      (LeaderAnchorHistory Digest authorityCount stake thresholds))
    {left right : ReferenceSelectedSlotView Digest}
    (sameSlot : left.slot = right.slot)
    {leftCommitVotes leftSkipVotes rightCommitVotes rightSkipVotes :
      LeaderBlockRef Digest → VoterSet}
    (directEvidence : TwoDirectSlotEvidence thresholds
      leftCommitVotes leftSkipVotes rightCommitVotes rightSkipVotes)
    (leftValid : ExactDirectStatusValid thresholds
      leftCommitVotes leftSkipVotes left.slot left.status)
    (rightValid : ExactDirectStatusValid thresholds
      rightCommitVotes rightSkipVotes right.slot right.status)
    (leftAnchorEvidence : ∀ anchor,
      DirectAnchorEvidence thresholds leftCommitVotes leftSkipVotes
        (rule.historyOf anchor))
    (rightAnchorEvidence : ∀ anchor,
      DirectAnchorEvidence thresholds rightCommitVotes rightSkipVotes
        (rule.historyOf anchor))
    (indirectValid : ∀ anchor slot,
      ExactIndirectStatusValid (rule.historyOf anchor) slot
        (rule.decide anchor slot)) :
    CrossViewDirectSlotAgreement rule left right := by
  refine
    { exactAgreement := ⟨sameSlot, ?_⟩
      leftConsistent := ?_
      rightConsistent := ?_ }
  · have rightValidSame : ExactDirectStatusValid thresholds
        rightCommitVotes rightSkipVotes left.slot right.status := by
      rw [sameSlot]
      exact rightValid
    exact directEvidence.valid_direct_statuses_compatible
      leftValid rightValidSame
  · intro anchor final
    exact (leftAnchorEvidence anchor).valid_direct_indirect_statuses_equal
      leftValid final (indirectValid anchor left.slot)
  · intro anchor final
    exact (rightAnchorEvidence anchor).valid_direct_indirect_statuses_equal
      rightValid final (indirectValid anchor right.slot)

namespace ReferenceAnchorScanResult

/-- Two scans can differ because one local horizon ends or one view is blocked.
If both scans find an anchor, they must find the same exact reference. -/
def FoundAgreement {Digest : Type} :
    ReferenceAnchorScanResult Digest → ReferenceAnchorScanResult Digest → Prop
  | .found left, .found right => left = right
  | _, _ => True

theorem compatible_implies_found_agreement {Digest : Type}
    {left right : ReferenceAnchorScanResult Digest}
    (compatible : left.Compatible right) : left.FoundAgreement right := by
  cases left <;> cases right <;>
    simp [Compatible, FoundAgreement] at compatible ⊢
  assumption

end ReferenceAnchorScanResult

/-- Scan only rounds at or above `minimumRound`. This is the exact
`find_anchor_block(decision_round + INDIRECT_COMMIT_DEPTH)` order. -/
def scanReferenceAnchorAtOrAbove {Digest : Type} (minimumRound : Nat) :
    List (ReferenceFlexRoundView Digest) → ReferenceAnchorScanResult Digest
  | [] => .noAnchor
  | round :: tail =>
      if round.round < minimumRound then
        scanReferenceAnchorAtOrAbove minimumRound tail
      else
        match scanReferenceSelectedSlots round.selectedSlots with
        | .blocked => .blocked
        | .found block => .found block
        | .noAnchor => scanReferenceAnchorAtOrAbove minimumRound tail

/-- Compatible corresponding round results give compatible eligible-anchor
scans. -/
theorem reference_anchor_at_or_above_scans_compatible
    {Digest : Type} {minimumRound : Nat}
    {left right : List (ReferenceFlexRoundView Digest)}
    (agreement : ExactListAgreement CrossViewExactRoundAgreement left right) :
    (scanReferenceAnchorAtOrAbove minimumRound left).Compatible
      (scanReferenceAnchorAtOrAbove minimumRound right) := by
  induction agreement with
  | nil =>
      simp [scanReferenceAnchorAtOrAbove,
        ReferenceAnchorScanResult.Compatible]
  | @cons leftRound rightRound leftTail rightTail roundAgreement tailAgreement ih =>
      rcases roundAgreement with ⟨sameRound, slotAgreement⟩
      by_cases below : leftRound.round < minimumRound
      · have rightBelow : rightRound.round < minimumRound := by
          rwa [← sameRound]
        simp [scanReferenceAnchorAtOrAbove, below, rightBelow] at ih ⊢
        exact ih
      · have rightNotBelow : ¬rightRound.round < minimumRound := by
          rwa [← sameRound]
        have headCompatible :=
          reference_selected_slot_scans_compatible slotAgreement
        cases leftScan : scanReferenceSelectedSlots leftRound.selectedSlots <;>
          cases rightScan : scanReferenceSelectedSlots rightRound.selectedSlots <;>
          simp [scanReferenceAnchorAtOrAbove, below, rightNotBelow,
            ReferenceAnchorScanResult.Compatible, leftScan, rightScan]
            at headCompatible ⊢
        case noAnchor.noAnchor => exact ih
        case noAnchor.blocked =>
          cases scanReferenceAnchorAtOrAbove minimumRound leftTail <;>
            simp
        case found.found => exact headCompatible

/-- Two successful eligible-anchor scans return the same exact reference. -/
theorem reference_anchor_at_or_above_scans_agree
    {Digest : Type} {minimumRound : Nat}
    {left right : List (ReferenceFlexRoundView Digest)}
    (agreement : ExactListAgreement CrossViewExactRoundAgreement left right)
    {leftAnchor rightAnchor : LeaderBlockRef Digest}
    (leftFound :
      scanReferenceAnchorAtOrAbove minimumRound left = .found leftAnchor)
    (rightFound :
      scanReferenceAnchorAtOrAbove minimumRound right = .found rightAnchor) :
    leftAnchor = rightAnchor := by
  have compatible := reference_anchor_at_or_above_scans_compatible
    (minimumRound := minimumRound) agreement
  rw [leftFound, rightFound] at compatible
  exact compatible

/-- Different local highest accepted rounds are allowed. If both eligible scans
find anchors in their common ordered prefix, the exact references agree. -/
theorem reference_anchor_at_or_above_prefix_scans_agree
    {Digest : Type} {minimumRound : Nat}
    {left right : List (ReferenceFlexRoundView Digest)}
    (agreement : ExactPrefixAgreement CrossViewExactRoundAgreement left right)
    {leftAnchor rightAnchor : LeaderBlockRef Digest}
    (leftFound :
      scanReferenceAnchorAtOrAbove minimumRound left = .found leftAnchor)
    (rightFound :
      scanReferenceAnchorAtOrAbove minimumRound right = .found rightAnchor) :
    leftAnchor = rightAnchor := by
  induction agreement with
  | leftNil => simp [scanReferenceAnchorAtOrAbove] at leftFound
  | rightNil => simp [scanReferenceAnchorAtOrAbove] at rightFound
  | @cons leftRound rightRound leftTail rightTail roundAgreement tailAgreement ih =>
      rcases roundAgreement with ⟨sameRound, slotAgreement⟩
      by_cases below : leftRound.round < minimumRound
      · have rightBelow : rightRound.round < minimumRound := by
          rwa [← sameRound]
        simp [scanReferenceAnchorAtOrAbove, below, rightBelow]
          at leftFound rightFound
        exact ih leftFound rightFound
      · have rightNotBelow : ¬rightRound.round < minimumRound := by
          rwa [← sameRound]
        have headCompatible :=
          reference_selected_slot_scans_compatible slotAgreement
        cases leftScan : scanReferenceSelectedSlots leftRound.selectedSlots <;>
          cases rightScan : scanReferenceSelectedSlots rightRound.selectedSlots <;>
          simp [scanReferenceAnchorAtOrAbove, below, rightNotBelow,
            ReferenceAnchorScanResult.Compatible, leftScan, rightScan]
            at headCompatible leftFound rightFound
        case noAnchor.noAnchor => exact ih leftFound rightFound
        case found.found =>
          exact leftFound.symm.trans (headCompatible.trans rightFound)

theorem reference_anchor_at_or_above_prefix_found_agreement
    {Digest : Type} {minimumRound : Nat}
    {left right : List (ReferenceFlexRoundView Digest)}
    (agreement : ExactPrefixAgreement CrossViewExactRoundAgreement left right) :
    (scanReferenceAnchorAtOrAbove minimumRound left).FoundAgreement
      (scanReferenceAnchorAtOrAbove minimumRound right) := by
  cases leftScan : scanReferenceAnchorAtOrAbove minimumRound left <;>
    cases rightScan : scanReferenceAnchorAtOrAbove minimumRound right <;>
    simp [ReferenceAnchorScanResult.FoundAgreement]
  case found.found =>
    exact reference_anchor_at_or_above_prefix_scans_agree agreement
      leftScan rightScan

/-- If successful scans return different exact anchors, a higher corresponding
round or selected-slot result is already incompatible. -/
theorem different_anchors_require_higher_slot_conflict
    {Digest : Type} {minimumRound : Nat}
    {left right : List (ReferenceFlexRoundView Digest)}
    {leftAnchor rightAnchor : LeaderBlockRef Digest}
    (leftFound :
      scanReferenceAnchorAtOrAbove minimumRound left = .found leftAnchor)
    (rightFound :
      scanReferenceAnchorAtOrAbove minimumRound right = .found rightAnchor)
    (different : leftAnchor ≠ rightAnchor) :
    ¬ExactListAgreement CrossViewExactRoundAgreement left right := by
  intro agreement
  exact different (reference_anchor_at_or_above_scans_agree agreement
    leftFound rightFound)

/-- Apply one selected anchor to one directly undecided slot. Direct results are
retained, as in `RoundState::update_slot_decision`. -/
def finishReferenceSelectedSlot {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchor : ReferenceAnchorScanResult Digest)
    (view : ReferenceSelectedSlotView Digest) :
    ReferenceSelectedSlotView Digest :=
  match view.status, anchor with
  | .undecided, .found block =>
      { slot := view.slot, status := rule.decide block view.slot }
  | _, _ => view

def finishReferenceSelectedSlots {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchor : ReferenceAnchorScanResult Digest)
    (views : List (ReferenceSelectedSlotView Digest)) :
    List (ReferenceSelectedSlotView Digest) :=
  views.map (finishReferenceSelectedSlot rule anchor)

/-- An indirect pass does not change a selected-slot list that is already
final. -/
theorem finish_reference_selected_slots_preserves_final_decisions
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchor : ReferenceAnchorScanResult Digest)
    {views : List (ReferenceSelectedSlotView Digest)}
    {decisions : List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))}
    (final : finalReferenceDecisions? views = some decisions) :
    finishReferenceSelectedSlots rule anchor views = views := by
  induction views generalizing decisions with
  | nil => rfl
  | cons view tail ih =>
      rcases final_reference_decisions_cons_parts final with
        ⟨headDecision, tailDecisions, headFinal, tailFinal, decisionsShape⟩
      have tailSame := ih tailFinal
      rcases view with ⟨slot, status⟩
      cases status with
      | undecided =>
          simp [ReferenceSelectedSlotView.finalDecision?] at headFinal
      | commit block =>
          change { slot := slot, status := ReferenceSlotStatus.commit block } ::
              finishReferenceSelectedSlots rule anchor tail =
            { slot := slot, status := ReferenceSlotStatus.commit block } :: tail
          rw [tailSame]
      | skip =>
          change { slot := slot, status := ReferenceSlotStatus.skip } ::
              finishReferenceSelectedSlots rule anchor tail =
            { slot := slot, status := ReferenceSlotStatus.skip } :: tail
          rw [tailSame]

/-- Direct agreement and the local direct-versus-indirect theorem are preserved
when two views apply their separately selected anchors. -/
theorem finish_reference_selected_slot_agrees
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    {leftAnchor rightAnchor : ReferenceAnchorScanResult Digest}
    (anchorsCompatible : leftAnchor.FoundAgreement rightAnchor)
    {left right : ReferenceSelectedSlotView Digest}
    (directAgreement : CrossViewDirectSlotAgreement rule left right) :
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
        at anchorsCompatible statusesCompatible ⊢
    all_goals try assumption
    all_goals try subst_vars
    all_goals
      first
      | exact ReferenceSlotStatus.compatible_refl _
      | exact rule.final_direct_compatible_decide leftConsistent (by simp)
      | exact rule.final_direct_compatible_decide rightConsistent (by simp)
      | exact rule.decide_compatible_final_direct leftConsistent (by simp)
      | exact rule.decide_compatible_final_direct rightConsistent (by simp)

/-- The selected-slot result extends pointwise to one complete round. -/
theorem finish_reference_selected_slots_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    {leftAnchor rightAnchor : ReferenceAnchorScanResult Digest}
    (anchorsCompatible : leftAnchor.FoundAgreement rightAnchor)
    {left right : List (ReferenceSelectedSlotView Digest)}
    (directAgreement :
      ExactListAgreement (CrossViewDirectSlotAgreement rule) left right) :
    ExactListAgreement CrossViewExactSlotAgreement
      (finishReferenceSelectedSlots rule leftAnchor left)
      (finishReferenceSelectedSlots rule rightAnchor right) := by
  induction directAgreement with
  | nil => exact .nil
  | cons headAgreement tailAgreement ih =>
      exact .cons
        (finish_reference_selected_slot_agrees rule anchorsCompatible headAgreement)
        ih

/-- Finish one round with the first eligible anchor in the already processed
higher-round suffix. -/
def finishReferenceFlexRound {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (higherRounds : List (ReferenceFlexRoundView Digest))
    (round : ReferenceFlexRoundView Digest) : ReferenceFlexRoundView Digest :=
  { round := round.round
    selectedSlots := finishReferenceSelectedSlots rule
      (scanReferenceAnchorAtOrAbove (round.round + 2) higherRounds)
      round.selectedSlots }

/-- Input rounds are in increasing round order. Recursion first finishes the
higher tail, which models the Rust high-to-low indirect loop. -/
def finishReferenceFlexRounds {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History) :
    List (ReferenceFlexRoundView Digest) → List (ReferenceFlexRoundView Digest)
  | [] => []
  | round :: higherDirectRounds =>
      let higherFinished := finishReferenceFlexRounds rule higherDirectRounds
      finishReferenceFlexRound rule higherFinished round :: higherFinished

/-- The same high-to-low exact finisher for an explicit indirect depth. -/
def finishReferenceFlexRoundAtDepth {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History) (depth : Nat)
    (higherRounds : List (ReferenceFlexRoundView Digest))
    (round : ReferenceFlexRoundView Digest) : ReferenceFlexRoundView Digest :=
  { round := round.round
    selectedSlots := finishReferenceSelectedSlots rule
      (scanReferenceAnchorAtOrAbove (round.round + depth) higherRounds)
      round.selectedSlots }

/-- Input rounds are in increasing round order. The recursive call finishes the
higher tail before the current round, for any configured indirect depth. -/
def finishReferenceFlexRoundsAtDepth {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History) (depth : Nat) :
    List (ReferenceFlexRoundView Digest) → List (ReferenceFlexRoundView Digest)
  | [] => []
  | round :: higherDirectRounds =>
      let higherFinished :=
        finishReferenceFlexRoundsAtDepth rule depth higherDirectRounds
      finishReferenceFlexRoundAtDepth rule depth higherFinished round ::
        higherFinished

/-- The existing v3 exact finisher is the explicit-depth finisher at depth two. -/
theorem finish_reference_flex_rounds_at_depth_two
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (rounds : List (ReferenceFlexRoundView Digest)) :
    finishReferenceFlexRoundsAtDepth rule 2 rounds =
      finishReferenceFlexRounds rule rounds := by
  induction rounds with
  | nil => rfl
  | cons round tail ih =>
      simp only [finishReferenceFlexRoundsAtDepth, finishReferenceFlexRounds]
      rw [ih]
      rfl

/-- Recursive ordered-anchor agreement for an explicit indirect depth. -/
theorem recursive_flex_rounds_at_depth_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History) (depth : Nat)
    {left right : List (ReferenceFlexRoundView Digest)}
    (directAgreement :
      ExactListAgreement (CrossViewDirectRoundAgreement rule) left right) :
    ExactListAgreement CrossViewExactRoundAgreement
      (finishReferenceFlexRoundsAtDepth rule depth left)
      (finishReferenceFlexRoundsAtDepth rule depth right) := by
  induction directAgreement with
  | nil => exact .nil
  | @cons leftRound rightRound leftTail rightTail roundAgreement tailAgreement ih =>
      rcases roundAgreement with ⟨sameRound, selectedAgreement⟩
      have anchorsCompatible :=
        ReferenceAnchorScanResult.compatible_implies_found_agreement
          (reference_anchor_at_or_above_scans_compatible
            (minimumRound := leftRound.round + depth) ih)
      have selectedFinished := finish_reference_selected_slots_agree
        rule anchorsCompatible selectedAgreement
      refine .cons ?_ ih
      constructor
      · simpa [finishReferenceFlexRoundAtDepth] using sameRound
      · simpa [finishReferenceFlexRoundsAtDepth,
          finishReferenceFlexRoundAtDepth, sameRound]
          using selectedFinished

/-- Explicit-depth agreement also permits different local high-round suffixes. -/
theorem recursive_flex_rounds_at_depth_prefix_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History) (depth : Nat)
    {left right : List (ReferenceFlexRoundView Digest)}
    (directAgreement :
      ExactPrefixAgreement (CrossViewDirectRoundAgreement rule) left right) :
    ExactPrefixAgreement CrossViewExactRoundAgreement
      (finishReferenceFlexRoundsAtDepth rule depth left)
      (finishReferenceFlexRoundsAtDepth rule depth right) := by
  induction directAgreement with
  | leftNil => exact .leftNil
  | rightNil => exact .rightNil
  | @cons leftRound rightRound leftTail rightTail roundAgreement tailAgreement ih =>
      rcases roundAgreement with ⟨sameRound, selectedAgreement⟩
      have anchorsAgree :=
        reference_anchor_at_or_above_prefix_found_agreement
          (minimumRound := leftRound.round + depth) ih
      have selectedFinished := finish_reference_selected_slots_agree
        rule anchorsAgree selectedAgreement
      refine .cons ?_ ih
      constructor
      · simpa [finishReferenceFlexRoundAtDepth] using sameRound
      · simpa [finishReferenceFlexRoundsAtDepth,
          finishReferenceFlexRoundAtDepth, sameRound]
          using selectedFinished

/-- Two explicit-depth executions derive the same exact candidate. -/
theorem recursive_flex_commit_candidates_at_depth_prefix_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History) (depth : Nat)
    {left right : List (ReferenceFlexRoundView Digest)}
    (directAgreement :
      ExactPrefixAgreement (CrossViewDirectRoundAgreement rule) left right)
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
  have finishedAgreement := recursive_flex_rounds_at_depth_prefix_agree
    rule depth directAgreement
  exact reference_flex_commit_candidates_agree finishedAgreement
    leftFound rightFound

/-- The indirect pass preserves a candidate already returned by the direct
scan. -/
theorem finish_reference_flex_rounds_at_depth_preserves_candidate
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History) (depth : Nat)
    {rounds : List (ReferenceFlexRoundView Digest)}
    {candidate : ReferenceFlexCandidate Digest}
    (found : findReferenceFlexCommitCandidate rounds = some candidate) :
    findReferenceFlexCommitCandidate
        (finishReferenceFlexRoundsAtDepth rule depth rounds) =
      some candidate := by
  induction rounds with
  | nil => simp [findReferenceFlexCommitCandidate] at found
  | cons round tail ih =>
      cases final : finalReferenceDecisions? round.selectedSlots with
      | none => simp [findReferenceFlexCommitCandidate, final] at found
      | some decisions =>
          have slotsSame := finish_reference_selected_slots_preserves_final_decisions
            rule
            (scanReferenceAnchorAtOrAbove (round.round + depth)
              (finishReferenceFlexRoundsAtDepth rule depth tail))
            final
          cases committed : orderedDecisionsHaveCommit decisions with
          | true =>
              simpa [finishReferenceFlexRoundsAtDepth,
                finishReferenceFlexRoundAtDepth, slotsSame,
                findReferenceFlexCommitCandidate, final, committed] using found
          | false =>
              have tailFound : findReferenceFlexCommitCandidate tail =
                  some candidate := by
                simpa [findReferenceFlexCommitCandidate, final, committed] using found
              have tailFinished := ih tailFound
              simpa [finishReferenceFlexRoundsAtDepth,
                finishReferenceFlexRoundAtDepth, slotsSame,
                findReferenceFlexCommitCandidate, final, committed]
                using tailFinished

/-- Recursive ordered-anchor agreement.

The premise covers only the direct pass. The theorem derives agreement for all
indirect results. It does not assume common anchors, comparable histories, final
selected-slot agreement, or one common commit candidate. -/
theorem recursive_flex_rounds_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    {left right : List (ReferenceFlexRoundView Digest)}
    (directAgreement :
      ExactListAgreement (CrossViewDirectRoundAgreement rule) left right) :
    ExactListAgreement CrossViewExactRoundAgreement
      (finishReferenceFlexRounds rule left)
      (finishReferenceFlexRounds rule right) := by
  induction directAgreement with
  | nil => exact .nil
  | @cons leftRound rightRound leftTail rightTail roundAgreement tailAgreement ih =>
      rcases roundAgreement with ⟨sameRound, selectedAgreement⟩
      have anchorsCompatible :=
        ReferenceAnchorScanResult.compatible_implies_found_agreement
          (reference_anchor_at_or_above_scans_compatible
            (minimumRound := leftRound.round + 2) ih)
      have selectedFinished := finish_reference_selected_slots_agree
        rule anchorsCompatible selectedAgreement
      refine .cons ?_ ih
      constructor
      · simpa [finishReferenceFlexRound] using sameRound
      · simpa [finishReferenceFlexRounds, finishReferenceFlexRound, sameRound]
          using selectedFinished

/-- Recursive agreement also permits different local highest accepted rounds.
The common low-to-high pending prefix has the same final results. -/
theorem recursive_flex_rounds_prefix_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    {left right : List (ReferenceFlexRoundView Digest)}
    (directAgreement :
      ExactPrefixAgreement (CrossViewDirectRoundAgreement rule) left right) :
    ExactPrefixAgreement CrossViewExactRoundAgreement
      (finishReferenceFlexRounds rule left)
      (finishReferenceFlexRounds rule right) := by
  induction directAgreement with
  | leftNil => exact .leftNil
  | rightNil => exact .rightNil
  | @cons leftRound rightRound leftTail rightTail roundAgreement tailAgreement ih =>
      rcases roundAgreement with ⟨sameRound, selectedAgreement⟩
      have anchorsAgree :=
        reference_anchor_at_or_above_prefix_found_agreement
          (minimumRound := leftRound.round + 2) ih
      have selectedFinished := finish_reference_selected_slots_agree
        rule anchorsAgree selectedAgreement
      refine .cons ?_ ih
      constructor
      · simpa [finishReferenceFlexRound] using sameRound
      · simpa [finishReferenceFlexRounds, finishReferenceFlexRound, sameRound]
          using selectedFinished

theorem ExactListAgreement.toPrefix
    {Left Right : Type} {relation : Left → Right → Prop}
    {left : List Left} {right : List Right}
    (agreement : ExactListAgreement relation left right) :
    ExactPrefixAgreement relation left right := by
  induction agreement with
  | nil => exact .leftNil
  | cons headAgreement tailAgreement ih => exact .cons headAgreement ih

/-- Two separate complete ordered executions return the same exact commit
candidate. Candidate equality is a conclusion, not an input. -/
theorem recursive_flex_commit_candidates_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    {left right : List (ReferenceFlexRoundView Digest)}
    (directAgreement :
      ExactListAgreement (CrossViewDirectRoundAgreement rule) left right)
    {leftCandidate rightCandidate : ReferenceFlexCandidate Digest}
    (leftFound :
      findReferenceFlexCommitCandidate (finishReferenceFlexRounds rule left) =
        some leftCandidate)
    (rightFound :
      findReferenceFlexCommitCandidate (finishReferenceFlexRounds rule right) =
        some rightCandidate) :
    leftCandidate = rightCandidate := by
  have finishedAgreement := recursive_flex_rounds_agree
    rule directAgreement
  exact reference_flex_commit_candidates_agree finishedAgreement.toPrefix
    leftFound rightFound

/-- Separate complete executions can have different local high-round suffixes.
If both return a commit candidate, the complete exact candidates still agree. -/
theorem recursive_flex_commit_candidates_prefix_agree
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    {left right : List (ReferenceFlexRoundView Digest)}
    (directAgreement :
      ExactPrefixAgreement (CrossViewDirectRoundAgreement rule) left right)
    {leftCandidate rightCandidate : ReferenceFlexCandidate Digest}
    (leftFound :
      findReferenceFlexCommitCandidate (finishReferenceFlexRounds rule left) =
        some leftCandidate)
    (rightFound :
      findReferenceFlexCommitCandidate (finishReferenceFlexRounds rule right) =
        some rightCandidate) :
    leftCandidate = rightCandidate := by
  have finishedAgreement := recursive_flex_rounds_prefix_agree
    rule directAgreement
  exact reference_flex_commit_candidates_agree finishedAgreement
    leftFound rightFound

theorem recursive_flex_builder_inputs_prefix_agree
    {LocalView BlockDigest CommitDigest History : Type}
    (rule : ReferenceIndirectRule BlockDigest History)
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockDigest CommitDigest)
    {leftView rightView : LocalView}
    {left right : List (ReferenceFlexRoundView BlockDigest)}
    (directAgreement :
      ExactPrefixAgreement (CrossViewDirectRoundAgreement rule) left right)
    {leftCandidate rightCandidate : ReferenceFlexCandidate BlockDigest}
    (leftFound :
      findReferenceFlexCommitCandidate (finishReferenceFlexRounds rule left) =
        some leftCandidate)
    (rightFound :
      findReferenceFlexCommitCandidate (finishReferenceFlexRounds rule right) =
        some rightCandidate)
    {leftPrior rightPrior : ExactCommitReference CommitDigest}
    (samePrior : leftPrior = rightPrior)
    (leftComplete : mapping.complete leftView leftPrior leftCandidate)
    (rightComplete : mapping.complete rightView rightPrior rightCandidate) :
    mapping.localInput leftView leftPrior leftCandidate =
      mapping.localInput rightView rightPrior rightCandidate := by
  have sameCandidate := recursive_flex_commit_candidates_prefix_agree
    rule directAgreement leftFound rightFound
  exact mapping.complete_local_inputs_agree samePrior sameCandidate
    leftComplete rightComplete

/-- Complete recursive scans and one equal prior exact reference produce the
same deterministic next commit reference. -/
theorem recursive_flex_built_references_prefix_agree
    {LocalView BlockDigest CommitDigest History Encoding : Type}
    (functions : CommitReferenceFunctions CommitDigest
      (LeaderBlockRef BlockDigest) Encoding)
    (rule : ReferenceIndirectRule BlockDigest History)
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockDigest CommitDigest)
    {leftView rightView : LocalView}
    {left right : List (ReferenceFlexRoundView BlockDigest)}
    (directAgreement :
      ExactPrefixAgreement (CrossViewDirectRoundAgreement rule) left right)
    {leftCandidate rightCandidate : ReferenceFlexCandidate BlockDigest}
    (leftFound :
      findReferenceFlexCommitCandidate (finishReferenceFlexRounds rule left) =
        some leftCandidate)
    (rightFound :
      findReferenceFlexCommitCandidate (finishReferenceFlexRounds rule right) =
        some rightCandidate)
    {leftPrior rightPrior : ExactCommitReference CommitDigest}
    (samePrior : leftPrior = rightPrior)
    (leftComplete : mapping.complete leftView leftPrior leftCandidate)
    (rightComplete : mapping.complete rightView rightPrior rightCandidate) :
    constructExactCommitReferenceFromInput functions
        (mapping.localInput leftView leftPrior leftCandidate) =
      constructExactCommitReferenceFromInput functions
        (mapping.localInput rightView rightPrior rightCandidate) := by
  have sameInput := recursive_flex_builder_inputs_prefix_agree
    rule mapping directAgreement leftFound rightFound samePrior
      leftComplete rightComplete
  rw [sameInput]

/-- A commit in the first selected slot of the first eligible round is the
anchor. No later slot or round can change this result. -/
theorem first_selected_commit_is_reference_anchor
    {Digest : Type} {minimumRound roundNumber : Nat}
    (eligible : ¬roundNumber < minimumRound)
    {slot : ExactSelectedLeaderSlot} {block : LeaderBlockRef Digest}
    {tailSlots : List (ReferenceSelectedSlotView Digest)}
    {tailRounds : List (ReferenceFlexRoundView Digest)} :
    scanReferenceAnchorAtOrAbove minimumRound
      ({ round := roundNumber
         selectedSlots := { slot := slot, status := .commit block } :: tailSlots } ::
        tailRounds) = .found block := by
  simp [scanReferenceAnchorAtOrAbove, eligible, scanReferenceSelectedSlots]

/-- Two views with the same exact first committed block select that common
anchor independently. -/
theorem common_first_selected_commit_gives_same_anchor
    {Digest : Type} {minimumRound roundNumber : Nat}
    (eligible : ¬roundNumber < minimumRound)
    {leftSlot rightSlot : ExactSelectedLeaderSlot}
    {block : LeaderBlockRef Digest}
    {leftTailSlots rightTailSlots : List (ReferenceSelectedSlotView Digest)}
    {leftTailRounds rightTailRounds : List (ReferenceFlexRoundView Digest)} :
    scanReferenceAnchorAtOrAbove minimumRound
        ({ round := roundNumber
           selectedSlots :=
             { slot := leftSlot, status := .commit block } :: leftTailSlots } ::
          leftTailRounds) = .found block ∧
      scanReferenceAnchorAtOrAbove minimumRound
        ({ round := roundNumber
           selectedSlots :=
             { slot := rightSlot, status := .commit block } :: rightTailSlots } ::
          rightTailRounds) = .found block := by
  exact ⟨first_selected_commit_is_reference_anchor eligible,
    first_selected_commit_is_reference_anchor eligible⟩

/-! ### A complete scan with an invalid higher conflict

This example embeds the two incomparable histories from
`SplitAnchorHistoryExample` in two complete high-to-low Flex scans. The lower
rounds disagree exactly because the higher direct inputs already contain a
commit-versus-skip conflict. Thus, it is not a protocol counterexample. It shows
the base conflict that every finite recursive counterexample must contain.
-/

namespace OrderedAnchorConflictExample

open SplitAnchorHistoryExample

abbrev SplitHistory :=
  LeaderAnchorHistory BranchDigest 4 unitStake splitThresholds

def splitIndirectRule : ReferenceIndirectRule BranchDigest SplitHistory where
  historyOf anchor :=
    if anchor = leftAnchorRef then leftHistory else rightHistory
  decideFromHistory history _ :=
    if history.anchorRef = leftAnchorRef then
      .commit firstRef
    else
      .commit secondRef
  decisionFinal := by
    intro history slot
    by_cases left : history.anchorRef = leftAnchorRef <;>
      simp [left]

def targetSlot : ExactSelectedLeaderSlot :=
  { round := 1, validator := 0 }

def firstAnchorSlot : ExactSelectedLeaderSlot :=
  { round := 3, validator := 0 }

def secondAnchorSlot : ExactSelectedLeaderSlot :=
  { round := 3, validator := 3 }

def leftDirectRounds : List (ReferenceFlexRoundView BranchDigest) :=
  [ { round := 1
      selectedSlots := [{ slot := targetSlot, status := .undecided }] },
    { round := 3
      selectedSlots :=
        [ { slot := firstAnchorSlot, status := .commit leftAnchorRef },
          { slot := secondAnchorSlot, status := .skip } ] } ]

def rightDirectRounds : List (ReferenceFlexRoundView BranchDigest) :=
  [ { round := 1
      selectedSlots := [{ slot := targetSlot, status := .undecided }] },
    { round := 3
      selectedSlots :=
        [ { slot := firstAnchorSlot, status := .skip },
          { slot := secondAnchorSlot, status := .commit rightAnchorRef } ] } ]

def leftFinishedRounds : List (ReferenceFlexRoundView BranchDigest) :=
  [ { round := 1
      selectedSlots := [{ slot := targetSlot, status := .commit firstRef }] },
    { round := 3
      selectedSlots :=
        [ { slot := firstAnchorSlot, status := .commit leftAnchorRef },
          { slot := secondAnchorSlot, status := .skip } ] } ]

def rightFinishedRounds : List (ReferenceFlexRoundView BranchDigest) :=
  [ { round := 1
      selectedSlots := [{ slot := targetSlot, status := .commit secondRef }] },
    { round := 3
      selectedSlots :=
        [ { slot := firstAnchorSlot, status := .skip },
          { slot := secondAnchorSlot, status := .commit rightAnchorRef } ] } ]

theorem split_histories_are_selected :
    splitIndirectRule.historyOf leftAnchorRef = leftHistory ∧
      splitIndirectRule.historyOf rightAnchorRef = rightHistory := by
  constructor <;>
    simp [splitIndirectRule, leftAnchorRef, rightAnchorRef]

theorem split_selected_histories_are_incomparable :
    ¬(splitIndirectRule.historyOf leftAnchorRef).Comparable
      (splitIndirectRule.historyOf rightAnchorRef) := by
  rw [split_histories_are_selected.1, split_histories_are_selected.2]
  intro comparable
  rcases comparable with included | included
  · exact left_history_not_included_in_right included
  · exact right_history_not_included_in_left included

theorem complete_scans_select_incomparable_anchors :
    scanReferenceAnchorAtOrAbove 3 leftDirectRounds.tail =
        .found leftAnchorRef ∧
      scanReferenceAnchorAtOrAbove 3 rightDirectRounds.tail =
        .found rightAnchorRef := by
  constructor <;> rfl

theorem complete_scans_produce_different_lower_commits :
    finishReferenceFlexRounds splitIndirectRule leftDirectRounds =
        leftFinishedRounds ∧
      finishReferenceFlexRounds splitIndirectRule rightDirectRounds =
        rightFinishedRounds := by
  constructor <;> rfl

/-- The complete scans differ only after a higher direct commit-versus-skip
conflict is allowed. This direct input does not satisfy the recursive theorem. -/
theorem higher_direct_commit_skip_conflict :
    ¬ExactListAgreement CrossViewExactRoundAgreement
      leftDirectRounds rightDirectRounds := by
  intro agreement
  cases agreement with
  | cons firstAgreement tailAgreement =>
      cases tailAgreement with
      | cons anchorRoundAgreement emptyAgreement =>
          cases anchorRoundAgreement.selectedSlots with
          | cons firstSlotAgreement remainingSlots =>
              exact firstSlotAgreement.2

end OrderedAnchorConflictExample

end Mysticeti
