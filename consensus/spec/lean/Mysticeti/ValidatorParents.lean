/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorProcess

namespace Mysticeti

/-! Local immediate-parent selection for one validator.

This model uses explicit block references. It does not require two correct
validators to select the same branch from a Byzantine author. Stake is indexed by
the author, not by the number of branches that the author creates.
-/

/-- The local facts used to select parents for one proposal. -/
structure ImmediateParentView (BlockId : Type) where
  authorityCount : Nat
  proposalRound : Nat
  accepted : ValidatorBlockRef BlockId → Bool
  valid : ValidatorBlockRef BlockId → Bool
  timely : ValidatorBlockRef BlockId → Bool

namespace ImmediateParentView

/-- A locally accepted and valid block from the round before the proposal. -/
def AcceptedValidImmediateParent {BlockId : Type}
    (view : ImmediateParentView BlockId)
    (parent : ValidatorBlockRef BlockId) : Prop :=
  parent.author < view.authorityCount ∧
    parent.round + 1 = view.proposalRound ∧
    view.accepted parent = true ∧
    view.valid parent = true

/-- One timely parent is the only timely accepted and valid branch from its
author in this local view. -/
structure TimelyUniqueRepresentative {BlockId : Type}
    (view : ImmediateParentView BlockId)
    (author : Nat)
    (parent : ValidatorBlockRef BlockId) : Prop where
  parentAuthor : parent.author = author
  acceptedValid : view.AcceptedValidImmediateParent parent
  arrivedOnTime : view.timely parent = true
  unique : ∀ other,
    view.AcceptedValidImmediateParent other →
    other.author = author →
    view.timely other = true →
    other = parent

end ImmediateParentView

/-- One local parent choice per author. Every selected reference is accepted,
valid, and from the immediate parent round. -/
structure ImmediateParentSelection {BlockId : Type}
    (view : ImmediateParentView BlockId) where
  choice : Nat → Option (ValidatorBlockRef BlockId)
  selectedMatchesAuthor : ∀ author parent,
    choice author = some parent → parent.author = author
  selectedIsAcceptedValid : ∀ author parent,
    choice author = some parent →
      view.AcceptedValidImmediateParent parent

namespace ImmediateParentSelection

variable {BlockId : Type} {view : ImmediateParentView BlockId}

/-- Authors that supply one selected parent branch. -/
def selectedAuthors (selection : ImmediateParentSelection view) : VoterSet :=
  fun author => (selection.choice author).isSome

/-- Selected parent stake. The author set makes the stake count independent of
the number of known branches. -/
def selectedAuthorStake (stake : Nat → Nat)
    (selection : ImmediateParentSelection view) : Nat :=
  weight view.authorityCount stake selection.selectedAuthors

/-- Build the explicit selected-parent list for the first `authorityCount`
authors. -/
def selectedParentRefsFrom (authorityCount : Nat)
    (choice : Nat → Option (ValidatorBlockRef BlockId)) :
    List (ValidatorBlockRef BlockId) :=
  match authorityCount with
  | 0 => []
  | count + 1 =>
      selectedParentRefsFrom count choice ++
        match choice count with
        | none => []
        | some parent => [parent]

/-- The explicit parent references selected by one validator. -/
def selectedParentRefs (selection : ImmediateParentSelection view) :
    List (ValidatorBlockRef BlockId) :=
  selectedParentRefsFrom view.authorityCount selection.choice

/-- Stake summed over the explicit selected-parent references. -/
def selectedReferenceStake (stake : Nat → Nat)
    (selection : ImmediateParentSelection view) : Nat :=
  (selection.selectedParentRefs.map (fun parent => stake parent.author)).sum

/-- An explicit block reference is included when it is the selected branch for
its author. -/
def Includes (selection : ImmediateParentSelection view)
    (parent : ValidatorBlockRef BlockId) : Prop :=
  selection.choice parent.author = some parent

private theorem selected_parent_mem_from
    {choice : Nat → Option (ValidatorBlockRef BlockId)}
    {authorityCount author : Nat} {parent : ValidatorBlockRef BlockId}
    (authorInRange : author < authorityCount)
    (selected : choice author = some parent) :
    parent ∈ selectedParentRefsFrom authorityCount choice := by
  induction authorityCount generalizing author parent with
  | zero => omega
  | succ count ih =>
      by_cases lastAuthor : author = count
      · subst author
        simp [selectedParentRefsFrom, selected]
      · have authorBeforeLast : author < count := by omega
        have earlier := ih authorBeforeLast selected
        cases lastChoice : choice count <;>
          simp [selectedParentRefsFrom, lastChoice, earlier]

private theorem selected_parent_author_lt
    {choice : Nat → Option (ValidatorBlockRef BlockId)}
    (matchesAuthor : ∀ author parent,
      choice author = some parent → parent.author = author)
    {authorityCount : Nat} {parent : ValidatorBlockRef BlockId}
    (selected : parent ∈ selectedParentRefsFrom authorityCount choice) :
    parent.author < authorityCount := by
  induction authorityCount with
  | zero => simp [selectedParentRefsFrom] at selected
  | succ count ih =>
      cases lastChoice : choice count with
      | none =>
          have earlier :
              parent ∈ selectedParentRefsFrom count choice := by
            simpa [selectedParentRefsFrom, lastChoice] using selected
          exact Nat.lt_succ_of_lt (ih earlier)
      | some lastParent =>
          have selectedCases :
              parent ∈ selectedParentRefsFrom count choice ∨
                parent = lastParent := by
            simpa [selectedParentRefsFrom, lastChoice] using selected
          rcases selectedCases with earlier | isLast
          · exact Nat.lt_succ_of_lt (ih earlier)
          · have authorMatches := matchesAuthor count lastParent lastChoice
            rw [isLast]
            omega

private theorem selected_parent_authors_nodup_from
    {choice : Nat → Option (ValidatorBlockRef BlockId)}
    (matchesAuthor : ∀ author parent,
      choice author = some parent → parent.author = author)
    (authorityCount : Nat) :
    ((selectedParentRefsFrom authorityCount choice).map
      ValidatorBlockRef.author).Nodup := by
  induction authorityCount with
  | zero => simp [selectedParentRefsFrom]
  | succ count ih =>
      cases lastChoice : choice count with
      | none =>
          simpa [selectedParentRefsFrom, lastChoice] using ih
      | some lastParent =>
          have lastAuthor := matchesAuthor count lastParent lastChoice
          have lastNotEarlier :
              count ∉ (selectedParentRefsFrom count choice).map
                ValidatorBlockRef.author := by
            intro member
            rcases List.mem_map.mp member with
              ⟨parent, parentSelected, parentAuthor⟩
            have parentBeforeLast :=
              selected_parent_author_lt matchesAuthor parentSelected
            omega
          have appended :
              (((selectedParentRefsFrom count choice).map
                  ValidatorBlockRef.author) ++ [count]).Nodup := by
            rw [List.nodup_append]
            refine ⟨ih, by simp, ?_⟩
            intro author authorSelected last lastSelected
            simp only [List.mem_singleton] at lastSelected
            subst last
            intro authorIsLast
            subst author
            exact lastNotEarlier authorSelected
          simpa [selectedParentRefsFrom, lastChoice, lastAuthor] using appended

private theorem selected_reference_stake_eq_weight_from
    (stake : Nat → Nat)
    {choice : Nat → Option (ValidatorBlockRef BlockId)}
    (matchesAuthor : ∀ author parent,
      choice author = some parent → parent.author = author)
    (authorityCount : Nat) :
    ((selectedParentRefsFrom authorityCount choice).map
        (fun parent => stake parent.author)).sum =
      weight authorityCount stake (fun author => (choice author).isSome) := by
  induction authorityCount with
  | zero => simp [selectedParentRefsFrom, weight]
  | succ count ih =>
      cases lastChoice : choice count with
      | none =>
          simpa [selectedParentRefsFrom, lastChoice, weight] using ih
      | some lastParent =>
          have lastAuthor := matchesAuthor count lastParent lastChoice
          simp [selectedParentRefsFrom, lastChoice, weight, lastAuthor, ih]

/-- A selected reference occurs in the explicit proposal-parent list. -/
theorem selected_parent_mem
    (selection : ImmediateParentSelection view)
    {author : Nat} {parent : ValidatorBlockRef BlockId}
    (authorInRange : author < view.authorityCount)
    (selected : selection.choice author = some parent) :
    parent ∈ selection.selectedParentRefs := by
  exact selected_parent_mem_from authorInRange selected

/-- Two included references from the same author are the same reference. -/
theorem included_branch_unique
    (selection : ImmediateParentSelection view)
    {left right : ValidatorBlockRef BlockId}
    (leftIncluded : selection.Includes left)
    (rightIncluded : selection.Includes right)
    (sameAuthor : left.author = right.author) :
    left = right := by
  have sameOption : some left = some right := by
    calc
      some left = selection.choice left.author := leftIncluded.symm
      _ = selection.choice right.author := congrArg selection.choice sameAuthor
      _ = some right := rightIncluded
  exact Option.some.inj sameOption

/-- The explicit parent list has no duplicate author. -/
theorem selected_parent_authors_nodup
    (selection : ImmediateParentSelection view) :
    (selection.selectedParentRefs.map ValidatorBlockRef.author).Nodup := by
  exact selected_parent_authors_nodup_from
    selection.selectedMatchesAuthor view.authorityCount

/-- Summing stake over explicit selected references gives the same result as
deduplicated author stake. -/
theorem selected_reference_stake_eq_deduplicated_stake
    (stake : Nat → Nat)
    (selection : ImmediateParentSelection view) :
    selection.selectedReferenceStake stake =
      selection.selectedAuthorStake stake := by
  exact selected_reference_stake_eq_weight_from stake
    selection.selectedMatchesAuthor view.authorityCount

private def oneAuthor (author : Nat) : VoterSet :=
  fun candidate => candidate == author

private theorem weight_eq_zero_when_no_author_selected
    {authorityCount : Nat} {stake : Nat → Nat} {authors : VoterSet}
    (noneSelected : ∀ author, author < authorityCount → authors author = false) :
    weight authorityCount stake authors = 0 := by
  induction authorityCount with
  | zero => rfl
  | succ count ih =>
      have earlierNone : ∀ author, author < count → authors author = false := by
        intro author authorInRange
        exact noneSelected author (by omega)
      have lastNone := noneSelected count (by omega)
      simp [weight, ih earlierNone, lastNone]

private theorem weight_one_author
    (stake : Nat → Nat) {authorityCount author : Nat}
    (authorInRange : author < authorityCount) :
    weight authorityCount stake (oneAuthor author) = stake author := by
  induction authorityCount generalizing author with
  | zero => omega
  | succ count ih =>
      by_cases lastAuthor : author = count
      · subst author
        have earlierZero :
            weight count stake (oneAuthor count) = 0 := by
          apply weight_eq_zero_when_no_author_selected
          intro candidate candidateInRange
          simp [oneAuthor, Nat.ne_of_lt candidateInRange]
        simp [weight, oneAuthor, earlierZero]
      · have authorBeforeLast : author < count := by omega
        have earlier := ih authorBeforeLast
        simp [weight, oneAuthor, earlier, Ne.symm lastAuthor]

/-- Selecting one branch counts the complete stake of its author. Extra branches
cannot add or remove that author's stake. -/
theorem selected_author_stake_is_not_reduced_by_extra_branches
    (stake : Nat → Nat)
    (selection : ImmediateParentSelection view)
    {author : Nat} {parent : ValidatorBlockRef BlockId}
    (selected : selection.choice author = some parent) :
    stake author ≤ selection.selectedAuthorStake stake := by
  have selectedFacts := selection.selectedIsAcceptedValid author parent selected
  have authorInRange : author < view.authorityCount := by
    rw [← selection.selectedMatchesAuthor author parent selected]
    exact selectedFacts.1
  have subset :
      VoterSet.SubsetAt view.authorityCount (oneAuthor author)
        selection.selectedAuthors := by
    intro candidate _ candidateIsAuthor
    have candidateMatches : candidate = author := by
      simpa [oneAuthor] using candidateIsAuthor
    subst candidate
    simp [selectedAuthors, selected]
  have selectedWeight := weight_mono stake subset
  rw [weight_one_author stake authorInRange] at selectedWeight
  exact selectedWeight

/-- One local rule includes every timely unique block from a correct author. This
is a single-validator behavior, not a statement about a quorum or a block layer. -/
structure TimelyCorrectParentInclusion
    (correct : VoterSet)
    (selection : ImmediateParentSelection view) : Prop where
  includesTimelyUnique : ∀ author parent,
    author < view.authorityCount →
    correct author = true →
    view.TimelyUniqueRepresentative author parent →
    selection.choice author = some parent

/-- Applying the local inclusion rule to a correct leader gives the exact leader
block reference in the proposal-parent list. -/
theorem timely_unique_correct_leader_is_included
    {correct : VoterSet}
    {selection : ImmediateParentSelection view}
    (rule : TimelyCorrectParentInclusion correct selection)
    {leader : Nat} {leaderBlock : ValidatorBlockRef BlockId}
    (leaderInRange : leader < view.authorityCount)
    (leaderCorrect : correct leader = true)
    (representative :
      view.TimelyUniqueRepresentative leader leaderBlock) :
    selection.Includes leaderBlock ∧
      leaderBlock ∈ selection.selectedParentRefs := by
  have selected := rule.includesTimelyUnique leader leaderBlock
    leaderInRange leaderCorrect representative
  constructor
  · simpa [Includes, representative.parentAuthor] using selected
  · exact selection.selected_parent_mem leaderInRange selected

end ImmediateParentSelection

end Mysticeti
