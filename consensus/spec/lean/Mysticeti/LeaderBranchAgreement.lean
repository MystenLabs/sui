/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.Leader

namespace Mysticeti

/-! Exact block-reference agreement for one selected Mysticeti v3 leader slot.

`LeaderEvidence` proves result exclusion for one unnamed leader block. This file
binds that evidence to a full in-epoch block reference and compares two correct
views. The epoch configuration is fixed by the surrounding threshold parameters.
-/

/-- A full block reference for one selected leader slot in the fixed epoch. The
round and author identify the slot. `digest` identifies one branch when the
author equivocates. -/
structure LeaderBlockRef (Digest : Type) where
  round : Nat
  author : Nat
  digest : Digest
  deriving DecidableEq, Repr

namespace LeaderBlockRef

/-- Two block references name branches in the same selected leader slot. -/
def SameSelectedSlot {Digest : Type}
    (left right : LeaderBlockRef Digest) : Prop :=
  left.round = right.round ∧ left.author = right.author

end LeaderBlockRef

/-- One correct local view binds the evidence model to one exact block reference. -/
structure LeaderBranchView (Digest : Type) (authorityCount : Nat)
    (stake : Nat → Nat) (thresholds : Thresholds authorityCount stake) where
  blockRef : LeaderBlockRef Digest
  evidence : LeaderEvidence authorityCount stake thresholds

namespace LeaderBranchView

variable {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}

/-- These authorities support this exact branch in direct or indirect evidence. -/
def branchVotes
    (view : LeaderBranchView Digest authorityCount stake thresholds) : VoterSet :=
  VoterSet.union view.evidence.commitVotes view.evidence.certificateVotes

theorem commit_votes_subset_branch_votes
    (view : LeaderBranchView Digest authorityCount stake thresholds) :
    VoterSet.SubsetAt authorityCount view.evidence.commitVotes
      view.branchVotes := by
  intro authority _ included
  simp [branchVotes, VoterSet.union, included]

theorem certificate_votes_subset_branch_votes
    (view : LeaderBranchView Digest authorityCount stake thresholds) :
    VoterSet.SubsetAt authorityCount view.evidence.certificateVotes
      view.branchVotes := by
  intro authority _ included
  simp [branchVotes, VoterSet.union, included]

def Commits
    (view : LeaderBranchView Digest authorityCount stake thresholds) : Prop :=
  view.evidence.CanDecide .commit

def Skips
    (view : LeaderBranchView Digest authorityCount stake thresholds) : Prop :=
  view.evidence.CanDecide .skip

/-- The existing leader safety theorem applies to the exact reference in one
view. -/
theorem commit_not_skip
    (view : LeaderBranchView Digest authorityCount stake thresholds)
    (committed : view.Commits) (skipped : view.Skips) : False := by
  exact view.evidence.safety committed skipped

/-- A direct commit and an indirect skip cannot apply to the same view and exact
reference. -/
theorem direct_commit_not_indirect_skip
    (view : LeaderBranchView Digest authorityCount stake thresholds)
    (committed : view.evidence.DirectCommit)
    (skipped : view.evidence.IndirectSkip) : False := by
  exact view.evidence.direct_commit_not_indirect_skip committed skipped

/-- A direct skip and an indirect commit cannot apply to the same view and exact
reference. -/
theorem direct_skip_not_indirect_commit
    (view : LeaderBranchView Digest authorityCount stake thresholds)
    (skipped : view.evidence.DirectSkip)
    (committed : view.evidence.IndirectCommit) : False := by
  exact view.evidence.direct_skip_not_indirect_commit skipped committed

/-- An indirect commit and an indirect skip cannot apply to the same view and
exact reference. -/
theorem indirect_commit_not_indirect_skip
    (view : LeaderBranchView Digest authorityCount stake thresholds)
    (committed : view.evidence.IndirectCommit)
    (skipped : view.evidence.IndirectSkip) : False := by
  exact view.evidence.indirect_commit_not_indirect_skip committed skipped

end LeaderBranchView

/-- The cross-view conditions for two correct validators at one selected slot.

Both views use one Byzantine set. A correct authority can support at most one
exact branch in the slot. Byzantine authorities can support both branches. -/
structure TwoCorrectLeaderViews
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (left right : LeaderBranchView Digest authorityCount stake thresholds) : Prop where
  sameSelectedSlot : left.blockRef.SameSelectedSlot right.blockRef
  sameFaulty : left.evidence.faulty = right.evidence.faulty
  correctVotesForOneBranch :
    ∀ authority, authority < authorityCount →
      left.evidence.faulty authority = false →
      left.branchVotes authority = true →
      right.branchVotes authority = true →
      left.blockRef = right.blockRef

namespace TwoCorrectLeaderViews

variable {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}
variable {left right : LeaderBranchView Digest authorityCount stake thresholds}

theorem symm (views : TwoCorrectLeaderViews left right) :
    TwoCorrectLeaderViews right left := by
  refine
    { sameSelectedSlot := ⟨views.sameSelectedSlot.1.symm,
        views.sameSelectedSlot.2.symm⟩
      sameFaulty := views.sameFaulty.symm
      correctVotesForOneBranch := ?_ }
  intro authority inRange rightCorrect rightVote leftVote
  have leftCorrect : left.evidence.faulty authority = false := by
    rw [views.sameFaulty]
    exact rightCorrect
  exact (views.correctVotesForOneBranch authority inRange leftCorrect
    leftVote rightVote).symm

/-- If the references differ, the intersection of any two included branch-vote
sets contains only Byzantine authorities. -/
theorem only_faulty_overlap_of_different_refs
    (views : TwoCorrectLeaderViews left right)
    (different : left.blockRef ≠ right.blockRef)
    {leftVotes rightVotes : VoterSet}
    (leftIncluded :
      VoterSet.SubsetAt authorityCount leftVotes left.branchVotes)
    (rightIncluded :
      VoterSet.SubsetAt authorityCount rightVotes right.branchVotes) :
    OnlyFaultyOverlap authorityCount left.evidence.faulty
      leftVotes rightVotes := by
  intro authority inRange included
  have bothSelected :
      leftVotes authority = true ∧ rightVotes authority = true := by
    simpa [VoterSet.inter] using included
  cases faultyValue : left.evidence.faulty authority with
  | false =>
      exfalso
      apply different
      exact views.correctVotesForOneBranch authority inRange faultyValue
        (leftIncluded authority inRange bothSelected.1)
        (rightIncluded authority inRange bothSelected.2)
  | true => rfl

/-- Two direct-commit quorums for the same selected slot certify the same full
block reference. The proof permits all Byzantine stake to vote for both refs. -/
theorem direct_commit_refs_agree
    (views : TwoCorrectLeaderViews left right)
    (leftCommitted : left.evidence.DirectCommit)
    (rightCommitted : right.evidence.DirectCommit) :
    left.blockRef = right.blockRef := by
  classical
  by_cases sameReference : left.blockRef = right.blockRef
  · exact sameReference
  · have onlyFaulty := views.only_faulty_overlap_of_different_refs
      sameReference left.commit_votes_subset_branch_votes
      right.commit_votes_subset_branch_votes
    exact False.elim (incompatible_quorums_impossible
      left.evidence.faultBounded onlyFaulty leftCommitted rightCommitted)

/-- A direct-commit quorum and an indirect certificate for the same selected
slot certify the same full block reference. -/
theorem direct_indirect_commit_refs_agree
    (views : TwoCorrectLeaderViews left right)
    (leftCommitted : left.evidence.DirectCommit)
    (rightCommitted : right.evidence.IndirectCommit) :
    left.blockRef = right.blockRef := by
  classical
  by_cases sameReference : left.blockRef = right.blockRef
  · exact sameReference
  · have onlyFaulty := views.only_faulty_overlap_of_different_refs
      sameReference left.commit_votes_subset_branch_votes
      right.certificate_votes_subset_branch_votes
    exact False.elim (incompatible_quorum_certificate_impossible
      left.evidence.faultBounded onlyFaulty leftCommitted rightCommitted)

/-- If one of two commit results is direct, both results name the same full
block reference. This result does not cover two indirect commits. -/
theorem commit_refs_agree_if_one_is_direct
    (views : TwoCorrectLeaderViews left right)
    (leftCommitted : left.Commits) (rightCommitted : right.Commits)
    (oneDirect :
      left.evidence.DirectCommit ∨ right.evidence.DirectCommit) :
    left.blockRef = right.blockRef := by
  rcases oneDirect with leftDirect | rightDirect
  · rcases rightCommitted with rightDirect | rightIndirect
    · exact views.direct_commit_refs_agree leftDirect rightDirect
    · exact views.direct_indirect_commit_refs_agree leftDirect rightIndirect
  · rcases leftCommitted with leftDirect | leftIndirect
    · exact views.direct_commit_refs_agree leftDirect rightDirect
    · exact (views.symm.direct_indirect_commit_refs_agree
        rightDirect leftIndirect).symm

/-- Two views cannot commit and skip when they use the same complete evidence
value. Equality of only the block references is not sufficient. -/
theorem same_evidence_commit_not_skip
    (sameEvidence : left.evidence = right.evidence)
    (committed : left.Commits) (skipped : right.Skips) : False := by
  unfold LeaderBranchView.Commits at committed
  unfold LeaderBranchView.Skips at skipped
  rw [← sameEvidence] at skipped
  exact left.evidence.safety committed skipped

end TwoCorrectLeaderViews

/-- The decision-round vote sets that the Rust indirect rule reaches by a BFS
from one anchor block. `votingLayer` contains all reached voting authorities.
Each block-specific set contains the reached voters that reference that exact
block. -/
structure LeaderAnchorHistory
    (Digest : Type) (authorityCount : Nat) (stake : Nat → Nat)
    (thresholds : Thresholds authorityCount stake) where
  anchorRef : LeaderBlockRef Digest
  votingLayer : VoterSet
  certificateVotes : LeaderBlockRef Digest → VoterSet
  votingLayerQuorum :
    thresholds.quorum ≤ weight authorityCount stake votingLayer
  certificateVotesInVotingLayer :
    ∀ blockRef, VoterSet.SubsetAt authorityCount
      (certificateVotes blockRef) votingLayer

namespace LeaderAnchorHistory

variable {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}

def HasCertificate
    (history : LeaderAnchorHistory Digest authorityCount stake thresholds)
    (blockRef : LeaderBlockRef Digest) : Prop :=
  thresholds.certificate ≤
    weight authorityCount stake (history.certificateVotes blockRef)

/-- Every reached vote in the first anchor history also occurs in the second
anchor history. An ancestor-to-descendant causal relation must imply this
property before the theorem can be applied to Rust. -/
def Included
    (earlier later :
      LeaderAnchorHistory Digest authorityCount stake thresholds) : Prop :=
  ∀ blockRef, VoterSet.SubsetAt authorityCount
    (earlier.certificateVotes blockRef)
    (later.certificateVotes blockRef)

def Comparable
    (left right :
      LeaderAnchorHistory Digest authorityCount stake thresholds) : Prop :=
  left.Included right ∨ right.Included left

theorem has_certificate_mono
    {earlier later :
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    (included : earlier.Included later) {blockRef : LeaderBlockRef Digest}
    (certified : earlier.HasCertificate blockRef) :
    later.HasCertificate blockRef := by
  exact Nat.le_trans certified (weight_mono stake (included blockRef))

end LeaderAnchorHistory

/-- The Rust indirect rule commits the unique certified exact block reference in
one selected slot. Zero or two certified references produce a skip result. -/
structure ExactIndirectCommit
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (history : LeaderAnchorHistory Digest authorityCount stake thresholds)
    (selected : LeaderBlockRef Digest) : Prop where
  selectedCertified : history.HasCertificate selected
  onlyCertifiedAtSlot :
    ∀ candidate, candidate.SameSelectedSlot selected →
      history.HasCertificate candidate → candidate = selected

namespace ExactIndirectCommit

variable {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}
variable {leftHistory rightHistory :
  LeaderAnchorHistory Digest authorityCount stake thresholds}
variable {leftRef rightRef : LeaderBlockRef Digest}

/-- Causal-history inclusion preserves the first certificate. Since the second
history commits only its unique certified reference, both references are equal. -/
theorem refs_agree_of_history_included
    (leftCommitted : ExactIndirectCommit leftHistory leftRef)
    (rightCommitted : ExactIndirectCommit rightHistory rightRef)
    (sameSlot : leftRef.SameSelectedSlot rightRef)
    (included : leftHistory.Included rightHistory) :
    leftRef = rightRef := by
  have leftCertifiedInRight :=
    LeaderAnchorHistory.has_certificate_mono included
      leftCommitted.selectedCertified
  exact rightCommitted.onlyCertifiedAtSlot
    leftRef sameSlot leftCertifiedInRight

/-- Comparable anchor vote histories are sufficient for exact agreement. This
does not require equal anchors or a common commit candidate. -/
theorem refs_agree_of_comparable_histories
    (leftCommitted : ExactIndirectCommit leftHistory leftRef)
    (rightCommitted : ExactIndirectCommit rightHistory rightRef)
    (sameSlot : leftRef.SameSelectedSlot rightRef)
    (comparable : leftHistory.Comparable rightHistory) :
    leftRef = rightRef := by
  rcases comparable with leftIncluded | rightIncluded
  · exact leftCommitted.refs_agree_of_history_included
      rightCommitted sameSlot leftIncluded
  · exact (rightCommitted.refs_agree_of_history_included
      leftCommitted ⟨sameSlot.1.symm, sameSlot.2.symm⟩ rightIncluded).symm

end ExactIndirectCommit

/-- This structure connects the exact-anchor model to the existing unnamed
`LeaderEvidence` certificate set. -/
structure AnchoredIndirectCommit
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (view : LeaderBranchView Digest authorityCount stake thresholds)
    (history : LeaderAnchorHistory Digest authorityCount stake thresholds) : Prop where
  evidenceCommitted : view.evidence.IndirectCommit
  selectedVotesMatch :
    history.certificateVotes view.blockRef =
      view.evidence.certificateVotes
  onlyCertifiedAtSlot :
    ∀ candidate, candidate.SameSelectedSlot view.blockRef →
      history.HasCertificate candidate → candidate = view.blockRef

namespace AnchoredIndirectCommit

variable {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
variable {thresholds : Thresholds authorityCount stake}
variable {left right : LeaderBranchView Digest authorityCount stake thresholds}
variable {leftHistory rightHistory :
  LeaderAnchorHistory Digest authorityCount stake thresholds}

theorem exact
    {view : LeaderBranchView Digest authorityCount stake thresholds}
    {history : LeaderAnchorHistory Digest authorityCount stake thresholds}
    (committed : AnchoredIndirectCommit view history) :
    ExactIndirectCommit history view.blockRef := by
  refine
    { selectedCertified := ?_
      onlyCertifiedAtSlot := committed.onlyCertifiedAtSlot }
  unfold LeaderAnchorHistory.HasCertificate
  rw [committed.selectedVotesMatch]
  exact committed.evidenceCommitted

/-- Two correct branch views have the same indirect-commit reference when their
selected anchor histories are causally comparable. -/
theorem refs_agree_of_comparable_histories
    (views : TwoCorrectLeaderViews left right)
    (leftCommitted : AnchoredIndirectCommit left leftHistory)
    (rightCommitted : AnchoredIndirectCommit right rightHistory)
    (comparable : leftHistory.Comparable rightHistory) :
    left.blockRef = right.blockRef := by
  exact leftCommitted.exact.refs_agree_of_comparable_histories
    rightCommitted.exact views.sameSelectedSlot comparable

end AnchoredIndirectCommit

/-! ### A split-history witness

This closed example uses the nominal `f = 0`, `c = 1` configuration. It has
`N = 4`, `Q = 3`, and `A = 2`. Four correct voting authorities split between two
branches. Each of two valid depth-two anchor voting layers has quorum stake and
contains exactly one different certificate. This is the local behavior of
`LeaderSlotDecider` when the two anchor histories are not comparable.
-/

namespace SplitAnchorHistoryExample

inductive BranchDigest where
  | first
  | second
  deriving DecidableEq, Repr

def unitStake : Nat → Nat := fun _ => 1

def splitThresholds : Thresholds 4 unitStake :=
  Thresholds.nominalHybrid 0 1 (by decide)

def firstRef : LeaderBlockRef BranchDigest :=
  { round := 1, author := 0, digest := .first }

def secondRef : LeaderBlockRef BranchDigest :=
  { round := 1, author := 0, digest := .second }

def leftAnchorRef : LeaderBlockRef BranchDigest :=
  { round := 3, author := 0, digest := .first }

def rightAnchorRef : LeaderBlockRef BranchDigest :=
  { round := 3, author := 3, digest := .second }

def firstTwo : VoterSet
  | 0 => true
  | 1 => true
  | _ => false

def onlyZero : VoterSet
  | 0 => true
  | _ => false

def onlyTwo : VoterSet
  | 2 => true
  | _ => false

def lastTwo : VoterSet
  | 2 => true
  | 3 => true
  | _ => false

def leftVotingLayer : VoterSet
  | 0 => true
  | 1 => true
  | 2 => true
  | _ => false

def rightVotingLayer : VoterSet
  | 0 => true
  | 2 => true
  | 3 => true
  | _ => false

def leftCertificateVotes (blockRef : LeaderBlockRef BranchDigest) : VoterSet :=
  match blockRef.digest with
  | .first => firstTwo
  | .second => onlyTwo

def rightCertificateVotes (blockRef : LeaderBlockRef BranchDigest) : VoterSet :=
  match blockRef.digest with
  | .first => onlyZero
  | .second => lastTwo

def leftHistory : LeaderAnchorHistory BranchDigest 4 unitStake splitThresholds where
  anchorRef := leftAnchorRef
  votingLayer := leftVotingLayer
  certificateVotes := leftCertificateVotes
  votingLayerQuorum := by decide
  certificateVotesInVotingLayer := by
    intro blockRef authority inRange included
    rcases blockRef with ⟨round, author, digest⟩
    have authorityCases :
        authority = 0 ∨ authority = 1 ∨ authority = 2 ∨ authority = 3 := by
      omega
    rcases authorityCases with rfl | rfl | rfl | rfl <;>
      cases digest <;>
      simp [leftCertificateVotes, firstTwo, onlyTwo, leftVotingLayer] at included ⊢

def rightHistory : LeaderAnchorHistory BranchDigest 4 unitStake splitThresholds where
  anchorRef := rightAnchorRef
  votingLayer := rightVotingLayer
  certificateVotes := rightCertificateVotes
  votingLayerQuorum := by decide
  certificateVotesInVotingLayer := by
    intro blockRef authority inRange included
    rcases blockRef with ⟨round, author, digest⟩
    have authorityCases :
        authority = 0 ∨ authority = 1 ∨ authority = 2 ∨ authority = 3 := by
      omega
    rcases authorityCases with rfl | rfl | rfl | rfl <;>
      cases digest <;>
      simp [rightCertificateVotes, onlyZero, lastTwo, rightVotingLayer] at included ⊢

theorem left_commits_first : ExactIndirectCommit leftHistory firstRef := by
  refine
    { selectedCertified := ?_
      onlyCertifiedAtSlot := ?_ }
  · change 2 ≤ 2
    omega
  · intro candidate sameSlot certified
    rcases candidate with ⟨round, author, digest⟩
    simp [LeaderBlockRef.SameSelectedSlot, firstRef] at sameSlot
    rcases sameSlot with ⟨rfl, rfl⟩
    cases digest with
    | first => rfl
    | second =>
        change 2 ≤ 1 at certified
        omega

theorem right_commits_second : ExactIndirectCommit rightHistory secondRef := by
  refine
    { selectedCertified := ?_
      onlyCertifiedAtSlot := ?_ }
  · change 2 ≤ 2
    omega
  · intro candidate sameSlot certified
    rcases candidate with ⟨round, author, digest⟩
    simp [LeaderBlockRef.SameSelectedSlot, secondRef] at sameSlot
    rcases sameSlot with ⟨rfl, rfl⟩
    cases digest with
    | first =>
        change 2 ≤ 1 at certified
        omega
    | second => rfl

/-- All four voters are correct in this example. A voter that occurs in both
histories supports the same exact branch. -/
theorem correct_voters_support_one_branch
    (authority : Nat) (inRange : authority < 4)
    (leftBlock rightBlock : LeaderBlockRef BranchDigest)
    (sameSlot : leftBlock.SameSelectedSlot rightBlock)
    (leftVote : leftHistory.certificateVotes leftBlock authority = true)
    (rightVote : rightHistory.certificateVotes rightBlock authority = true) :
    leftBlock = rightBlock := by
  rcases leftBlock with ⟨leftRound, leftAuthor, leftDigest⟩
  rcases rightBlock with ⟨rightRound, rightAuthor, rightDigest⟩
  simp [LeaderBlockRef.SameSelectedSlot] at sameSlot
  rcases sameSlot with ⟨rfl, rfl⟩
  cases leftDigest <;> cases rightDigest
  · rfl
  · have authorityCases :
        authority = 0 ∨ authority = 1 ∨ authority = 2 ∨ authority = 3 := by
      omega
    rcases authorityCases with rfl | rfl | rfl | rfl <;>
      simp [leftHistory, rightHistory, leftCertificateVotes,
        rightCertificateVotes, firstTwo, lastTwo] at leftVote rightVote
  · have authorityCases :
        authority = 0 ∨ authority = 1 ∨ authority = 2 ∨ authority = 3 := by
      omega
    rcases authorityCases with rfl | rfl | rfl | rfl <;>
      simp [leftHistory, rightHistory, leftCertificateVotes,
        rightCertificateVotes, onlyTwo, onlyZero] at leftVote rightVote
  · rfl

theorem left_history_not_included_in_right :
    ¬leftHistory.Included rightHistory := by
  intro included
  have impossible := included firstRef 1 (by omega) (by decide)
  simp [rightHistory, rightCertificateVotes, firstRef, onlyZero]
    at impossible

theorem right_history_not_included_in_left :
    ¬rightHistory.Included leftHistory := by
  intro included
  have impossible := included secondRef 3 (by omega) (by decide)
  simp [leftHistory, leftCertificateVotes, secondRef, onlyTwo]
    at impossible

/-- Parent-quorum validity and correct one-branch voting alone do not exclude
different indirect commits. The missing condition is a relation between the two
selected anchor histories. -/
theorem incomparable_histories_allow_different_indirect_commits :
    ExactIndirectCommit leftHistory firstRef ∧
      ExactIndirectCommit rightHistory secondRef ∧
      firstRef ≠ secondRef ∧
      ¬leftHistory.Comparable rightHistory := by
  refine ⟨left_commits_first, right_commits_second, ?_, ?_⟩
  · decide
  · intro comparable
    rcases comparable with included | included
    · exact left_history_not_included_in_right included
    · exact right_history_not_included_in_left included

end SplitAnchorHistoryExample

/-!
The exact-reference result stops at these points:

* Comparable anchor vote histories give indirect/indirect exact-reference
  agreement. The current threshold inequalities and correct one-branch voting do
  not prove this comparability. Two certificate thresholds need not intersect in
  correct stake.
* `LeaderEvidence.IndirectSkip` gives low certificate stake for one unnamed
  block. It does not state that every certified equivocation in the selected
  slot is absent. Thus, the current model cannot prove cross-view commit/skip
  exclusion from equal block references alone.
* A product refinement must bind `commitVotes` and `certificateVotes` to the
  exact Rust `BlockRef`. It must also relate correct views to one common first
  depth-two trigger and committed prefix before the same-evidence theorem can
  apply across nodes.
-/

end Mysticeti
