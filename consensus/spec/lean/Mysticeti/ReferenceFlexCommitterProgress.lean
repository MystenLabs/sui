/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.FlexCommitter
import Mysticeti.RecursiveFlexAgreement
import Mysticeti.ReferenceFlexCommitterExecution

namespace Mysticeti

/-!
An executable exact-reference progress model for the Mysticeti v3
`FlexCommitter::try_commit` order.

The model runs the direct pass, checks for a candidate, runs the indirect pass
from high rounds to low rounds, and checks for a candidate again. A commit
status always keeps the complete selected leader block reference.

The progress theorem starts from `d + 1` adjacent usable anchor rounds in the
post-direct state. It proves that the full indirect scan closes an arbitrary
earlier pending prefix and returns an exact candidate. It does not take a
candidate or a successful output as an input.
-/

/-- One pure direct-decider result for each selected leader slot. -/
structure ReferenceDirectRule (Digest : Type) where
  decide : ExactSelectedLeaderSlot → ReferenceSlotStatus Digest
  commitMatchesSlot : ∀ slot block,
    decide slot = .commit block → block.AtSelectedSlot slot

/-- The executable rules and configured indirect depth for one local committer. -/
structure ReferenceFlexCommitterContext (Digest History : Type) where
  depth : Nat
  depthPositive : 0 < depth
  directRule : ReferenceDirectRule Digest
  indirectRule : ReferenceIndirectRule Digest History
  indirectCommitMatchesSlot : ∀ anchor slot block,
    indirectRule.decide anchor slot = .commit block →
      block.AtSelectedSlot slot

/-- Apply a direct result only to a slot that is still undecided. -/
def applyReferenceDirectDecision {Digest : Type}
    (rule : ReferenceDirectRule Digest)
    (view : ReferenceSelectedSlotView Digest) :
    ReferenceSelectedSlotView Digest :=
  match view.status with
  | .undecided => { view with status := rule.decide view.slot }
  | .commit _ | .skip => view

/-- Apply the direct pass to one pending round. -/
def runReferenceDirectRound {Digest : Type}
    (rule : ReferenceDirectRule Digest)
    (round : ReferenceFlexRoundView Digest) : ReferenceFlexRoundView Digest :=
  { round with
    selectedSlots := round.selectedSlots.map (applyReferenceDirectDecision rule) }

/-- A finite pending-round array. Index zero is Rust's first pending round. -/
structure IndexedReferenceFlexState (Digest : Type) where
  commitIndex : Nat
  roundCount : Nat
  rounds : Nat → ReferenceFlexRoundView Digest

namespace IndexedReferenceFlexState

/-- Pending-array indexes name consecutive absolute leader rounds. -/
def RoundsConsecutive {Digest : Type}
    (state : IndexedReferenceFlexState Digest) (firstRound : Nat) : Prop :=
  ∀ index, index < state.roundCount →
    (state.rounds index).round = firstRound + index

/-- Every stored commit reference names its selected round and validator. -/
def SlotsWellFormed {Digest : Type}
    (state : IndexedReferenceFlexState Digest) : Prop :=
  ∀ index, index < state.roundCount →
    ∀ slot, slot ∈ (state.rounds index).selectedSlots → slot.WellFormed

/-- Every selected slot stored for one pending round names that round. -/
def SelectedSlotRoundsMatch {Digest : Type}
    (state : IndexedReferenceFlexState Digest) : Prop :=
  ∀ index, index < state.roundCount →
    ∀ slot, slot ∈ (state.rounds index).selectedSlots →
      slot.slot.round = (state.rounds index).round

end IndexedReferenceFlexState

/-- Apply the direct pass to all stored pending rounds. -/
def runReferenceDirectPass {Digest : Type}
    (rule : ReferenceDirectRule Digest)
    (state : IndexedReferenceFlexState Digest) :
    IndexedReferenceFlexState Digest :=
  { state with rounds := fun index => runReferenceDirectRound rule (state.rounds index) }

/-- Scan a finite pending suffix in Rust round and selected-slot order. -/
def findIndexedReferenceAnchorFrom {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest) :
    Nat → Nat → ReferenceAnchorScanResult Digest
  | _, 0 => .noAnchor
  | index, fuel + 1 =>
      match scanReferenceSelectedSlots (rounds index).selectedSlots with
      | .blocked => .blocked
      | .found block => .found block
      | .noAnchor => findIndexedReferenceAnchorFrom rounds (index + 1) fuel

/-- Replace one pending round. -/
def setIndexedReferenceRound {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest) (target : Nat)
    (round : ReferenceFlexRoundView Digest) :
    Nat → ReferenceFlexRoundView Digest :=
  fun index => if index = target then round else rounds index

@[simp]
theorem set_indexed_reference_round_same {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest) (target : Nat)
    (round : ReferenceFlexRoundView Digest) :
    setIndexedReferenceRound rounds target round target = round := by
  simp [setIndexedReferenceRound]

/-- One exact `decide_with_anchor_block` transition. -/
def indexedReferenceIndirectStep {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (decisionIndex : Nat) (state : IndexedReferenceFlexState Digest) :
    IndexedReferenceFlexState Digest :=
  let anchorIndex := decisionIndex + depth
  let anchor := findIndexedReferenceAnchorFrom state.rounds anchorIndex
    (state.roundCount - anchorIndex)
  match anchor with
  | .found block =>
      let current := state.rounds decisionIndex
      let finished : ReferenceFlexRoundView Digest :=
        { current with
          selectedSlots := finishReferenceSelectedSlots rule (.found block)
            current.selectedSlots }
      { state with
        rounds := setIndexedReferenceRound state.rounds decisionIndex finished }
  | .blocked | .noAnchor => state

/-- Apply exact indirect decisions in descending pending-array order. -/
def runIndexedReferenceIndirectDescending {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (highestIndex : Nat) : Nat → IndexedReferenceFlexState Digest →
      IndexedReferenceFlexState Digest
  | 0, state => state
  | stepCount + 1, state =>
      let previous := runIndexedReferenceIndirectDescending depth rule
        highestIndex stepCount state
      indexedReferenceIndirectStep depth rule (highestIndex - stepCount) previous

/-- Run every indirect decision index that has room for an anchor at depth `d`. -/
def runFullIndexedReferenceIndirect {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest) :
    IndexedReferenceFlexState Digest :=
  if depth < state.roundCount then
    runIndexedReferenceIndirectDescending depth rule
      (state.roundCount - depth - 1) (state.roundCount - depth) state
  else
    state

/-- One round is final when every selected leader slot has a final decision. -/
def ReferenceRoundFinal {Digest : Type}
    (round : ReferenceFlexRoundView Digest) : Prop :=
  ∃ decisions, finalReferenceDecisions? round.selectedSlots = some decisions

/-- One final round contains at least one exact committed leader reference. -/
def ReferenceRoundFinalCommit {Digest : Type}
    (round : ReferenceFlexRoundView Digest) : Prop :=
  ∃ decisions,
    finalReferenceDecisions? round.selectedSlots = some decisions ∧
      orderedDecisionsHaveCommit decisions = true

/-- The ordered anchor scan finds one exact committed leader reference. -/
def ReferenceRoundUsableAnchor {Digest : Type}
    (round : ReferenceFlexRoundView Digest) : Prop :=
  ∃ block, scanReferenceSelectedSlots round.selectedSlots = .found block

/-- `count` adjacent pending-array indexes are usable anchors. -/
def ReferenceAnchorWindow {Digest : Type}
    (state : IndexedReferenceFlexState Digest) (base count : Nat) : Prop :=
  ∀ offset, offset < count →
    ReferenceRoundUsableAnchor (state.rounds (base + offset))

/-- One exact candidate scan over the finite pending-round array. -/
def findIndexedReferenceFlexCandidateFrom {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest) :
    Nat → Nat → Option (ReferenceFlexCandidate Digest)
  | _, 0 => none
  | index, fuel + 1 =>
      let round := rounds index
      match finalReferenceDecisions? round.selectedSlots with
      | none => none
      | some decisions =>
          if orderedDecisionsHaveCommit decisions then
            some {
              leaderRound := round.round
              orderedCommittedLeaders :=
                committedLeaderRefsFromDecisions decisions }
          else
            findIndexedReferenceFlexCandidateFrom rounds (index + 1) fuel

def findIndexedReferenceFlexCandidate {Digest : Type}
    (state : IndexedReferenceFlexState Digest) :
    Option (ReferenceFlexCandidate Digest) :=
  findIndexedReferenceFlexCandidateFrom state.rounds 0 state.roundCount

/-- Read one finite indexed range as a list without an out-of-range default. -/
def indexedReferenceRoundsFrom {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest) :
    Nat → Nat → List (ReferenceFlexRoundView Digest)
  | _, 0 => []
  | start, fuel + 1 =>
      rounds start :: indexedReferenceRoundsFrom rounds (start + 1) fuel

def IndexedReferenceFlexState.toRoundList {Digest : Type}
    (state : IndexedReferenceFlexState Digest) :
    List (ReferenceFlexRoundView Digest) :=
  indexedReferenceRoundsFrom state.rounds 0 state.roundCount

/-- Read one list position. Out-of-range indexes use an empty round, which is
never part of the finite indexed range. -/
def referenceRoundAtList {Digest : Type} :
    List (ReferenceFlexRoundView Digest) → Nat → ReferenceFlexRoundView Digest
  | [], _ => { round := 0, selectedSlots := [] }
  | round :: _, 0 => round
  | _ :: tail, index + 1 => referenceRoundAtList tail index

/-- Convert one Rust-ordered pending-round list to the finite indexed model. -/
def indexedReferenceStateFromList {Digest : Type}
    (commitIndex : Nat) (rounds : List (ReferenceFlexRoundView Digest)) :
    IndexedReferenceFlexState Digest :=
  { commitIndex
    roundCount := rounds.length
    rounds := referenceRoundAtList rounds }

@[simp]
theorem reference_round_at_list_cons_succ
    {Digest : Type} (head : ReferenceFlexRoundView Digest)
    (tail : List (ReferenceFlexRoundView Digest)) (index : Nat) :
    referenceRoundAtList (head :: tail) (index + 1) =
      referenceRoundAtList tail index := by
  rfl

theorem indexed_reference_rounds_from_cons_succ
    {Digest : Type} (head : ReferenceFlexRoundView Digest)
    (tail : List (ReferenceFlexRoundView Digest)) (start fuel : Nat) :
    indexedReferenceRoundsFrom (referenceRoundAtList (head :: tail))
        (start + 1) fuel =
      indexedReferenceRoundsFrom (referenceRoundAtList tail) start fuel := by
  induction fuel generalizing start with
  | zero => rfl
  | succ remaining inductionHypothesis =>
      simp only [indexedReferenceRoundsFrom,
        reference_round_at_list_cons_succ]
      exact congrArg (fun suffix => referenceRoundAtList tail start :: suffix)
        (inductionHypothesis (start + 1))

/-- Converting a pending-round list to the indexed model and back preserves the
complete list and its order. -/
@[simp]
theorem indexed_reference_state_from_list_to_round_list
    {Digest : Type} (commitIndex : Nat)
    (rounds : List (ReferenceFlexRoundView Digest)) :
    (indexedReferenceStateFromList commitIndex rounds).toRoundList = rounds := by
  induction rounds with
  | nil => rfl
  | cons head tail inductionHypothesis =>
      simp only [indexedReferenceStateFromList, IndexedReferenceFlexState.toRoundList,
        List.length_cons, indexedReferenceRoundsFrom, referenceRoundAtList]
      rw [indexed_reference_rounds_from_cons_succ head tail 0 tail.length]
      exact congrArg (List.cons head) inductionHypothesis

/-- Reading an indexed range returns exactly the requested number of rounds. -/
@[simp]
theorem indexed_reference_rounds_from_length
    {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest)
    (start fuel : Nat) :
    (indexedReferenceRoundsFrom rounds start fuel).length = fuel := by
  induction fuel generalizing start with
  | zero => rfl
  | succ remaining =>
      simp [indexedReferenceRoundsFrom, *]

/-- The indexed candidate scan is exactly the existing ordered list scan. -/
theorem find_indexed_reference_candidate_eq_list_scan
    {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest)
    (start fuel : Nat) :
    findIndexedReferenceFlexCandidateFrom rounds start fuel =
      findReferenceFlexCommitCandidate
        (indexedReferenceRoundsFrom rounds start fuel) := by
  induction fuel generalizing start with
  | zero => rfl
  | succ remaining =>
      simp only [findIndexedReferenceFlexCandidateFrom,
        indexedReferenceRoundsFrom, findReferenceFlexCommitCandidate]
      cases final : finalReferenceDecisions? (rounds start).selectedSlots with
      | none => rfl
      | some decisions =>
          cases committed : orderedDecisionsHaveCommit decisions with
          | false =>
              simp [*]
          | true => simp [committed]

theorem find_indexed_reference_candidate_eq_state_list_scan
    {Digest : Type} (state : IndexedReferenceFlexState Digest) :
    findIndexedReferenceFlexCandidate state =
      findReferenceFlexCommitCandidate state.toRoundList := by
  exact find_indexed_reference_candidate_eq_list_scan state.rounds 0
    state.roundCount

/-- Local inputs needed to build the exact Rust commit body after a scan. -/
structure ReferenceFlexTryCommitInput (BlockId CommitId : Type) where
  prior : ExactCommitReference CommitId
  pending : IndexedReferenceFlexState BlockId
  materialize : ReferenceFlexCandidate BlockId →
    ExactCommitBuildMaterial (LeaderBlockRef BlockId)

namespace ReferenceFlexTryCommitInput

def buildOutput {BlockId CommitId Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (input : ReferenceFlexTryCommitInput BlockId CommitId)
    (candidate : ReferenceFlexCandidate BlockId) :
    LocalFlexCommitOutput BlockId CommitId :=
  buildLocalFlexCommitOutput functions
    { prior := input.prior
      flexState := {
        commitIndex := input.pending.commitIndex
        rounds := [] }
      materialize := input.materialize }
    candidate

end ReferenceFlexTryCommitInput

/-- Exact-reference `try_commit`: direct pass, direct scan, indirect pass, and
second scan. The output includes the exact candidate, commit body, and digest. -/
def tryReferenceFlexCommit {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (depth : Nat)
    (directRule : ReferenceDirectRule BlockId)
    (indirectRule : ReferenceIndirectRule BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId) :
    Option (LocalFlexCommitOutput BlockId CommitId) :=
  let directState := runReferenceDirectPass directRule input.pending
  match findIndexedReferenceFlexCandidate directState with
  | some candidate => some (input.buildOutput functions candidate)
  | none =>
      let finished := runFullIndexedReferenceIndirect depth indirectRule directState
      match findIndexedReferenceFlexCandidate finished with
      | some candidate => some (input.buildOutput functions candidate)
      | none => none

/-- Run exact `try_commit` with one grouped protocol context. -/
def tryReferenceFlexCommitWithContext
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId) :
    Option (LocalFlexCommitOutput BlockId CommitId) :=
  tryReferenceFlexCommit functions context.depth context.directRule
    context.indirectRule input

/-- One exact candidate selected by the direct or indirect scan in the
executable `try_commit` order. -/
def ReferenceFlexCandidateReturned
    {BlockId CommitId History : Type}
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId)
    (candidate : ReferenceFlexCandidate BlockId) : Prop :=
  let directState := runReferenceDirectPass context.directRule input.pending
  findIndexedReferenceFlexCandidate directState = some candidate ∨
    (findIndexedReferenceFlexCandidate directState = none ∧
      findIndexedReferenceFlexCandidate
          (runFullIndexedReferenceIndirect context.depth context.indirectRule
            directState) = some candidate)

/-- Local shape and reference validity for an actual Rust pending array.

Direct-versus-indirect consistency is not a shape fact. The safety refinement
proves it only for anchors that the executable scan can reach. -/
structure ReferenceFlexTryCommitStateValid
    {BlockId CommitId History : Type}
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId) where
  firstPendingRound : Nat
  commitIndexMatchesPrior : input.pending.commitIndex = input.prior.index
  roundsConsecutive : input.pending.RoundsConsecutive firstPendingRound
  pendingSlotsWellFormed : input.pending.SlotsWellFormed
  selectedSlotRoundsMatch : input.pending.SelectedSlotRoundsMatch

/-- One-validator materializer mapping for every candidate that this exact run
can return. -/
structure ReferenceFlexTryCommitMaterialValid
    {LocalView BlockId CommitId History : Type}
    (context : ReferenceFlexCommitterContext BlockId History)
    (mapping : ReferenceCommitMaterializerSourceMap LocalView BlockId CommitId)
    (view : LocalView)
    (input : ReferenceFlexTryCommitInput BlockId CommitId) : Prop where
  materialMatches : ∀ candidate,
    input.materialize candidate =
      mapping.localMaterial view input.prior candidate
  completeForReturned : ∀ candidate,
    ReferenceFlexCandidateReturned context input candidate →
      mapping.complete view input.prior candidate

namespace ReferenceFlexCandidate

/-- Every selected leader reference in a candidate names the candidate round,
and the candidate contains at least one committed leader. -/
def RoundWellFormed {Digest : Type}
    (candidate : ReferenceFlexCandidate Digest) : Prop :=
  candidate.orderedCommittedLeaders ≠ [] ∧
    ∀ block, block ∈ candidate.orderedCommittedLeaders →
      block.round = candidate.leaderRound

end ReferenceFlexCandidate

/-! ### Exact round facts -/

/-- Applying a found anchor gives every selected slot a final result. -/
theorem finish_reference_selected_slots_found_final
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchor : LeaderBlockRef Digest)
    (slots : List (ReferenceSelectedSlotView Digest)) :
    ∃ decisions,
      finalReferenceDecisions?
          (finishReferenceSelectedSlots rule (.found anchor) slots) =
        some decisions := by
  induction slots with
  | nil => exact ⟨[], rfl⟩
  | cons view tail ih =>
      rcases ih with ⟨tailDecisions, tailFinal⟩
      simp only [finishReferenceSelectedSlots] at tailFinal ⊢
      rcases view with ⟨slot, status⟩
      cases status with
      | undecided =>
          have decided := rule.decide_final anchor slot
          cases decisionEq : rule.decide anchor slot with
          | undecided => exact False.elim (decided decisionEq)
          | commit block =>
              refine ⟨{ slot, decision := .commit block } :: tailDecisions, ?_⟩
              simp [finishReferenceSelectedSlot,
                ReferenceSelectedSlotView.finalDecision?, finalReferenceDecisions?,
                decisionEq, tailFinal]
          | skip =>
              refine ⟨{ slot, decision := .skip } :: tailDecisions, ?_⟩
              simp [finishReferenceSelectedSlot,
                ReferenceSelectedSlotView.finalDecision?, finalReferenceDecisions?,
                decisionEq, tailFinal]
      | commit block =>
          refine ⟨{ slot, decision := .commit block } :: tailDecisions, ?_⟩
          simp [finishReferenceSelectedSlot,
            ReferenceSelectedSlotView.finalDecision?, finalReferenceDecisions?,
            tailFinal]
      | skip =>
          refine ⟨{ slot, decision := .skip } :: tailDecisions, ?_⟩
          simp [finishReferenceSelectedSlot,
            ReferenceSelectedSlotView.finalDecision?, finalReferenceDecisions?,
            tailFinal]

/-- Finishing another round cannot remove the exact first anchor in this round. -/
theorem finish_reference_selected_slots_preserves_found
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchor : LeaderBlockRef Digest)
    {slots : List (ReferenceSelectedSlotView Digest)}
    {block : LeaderBlockRef Digest}
    (found : scanReferenceSelectedSlots slots = .found block) :
    scanReferenceSelectedSlots
        (finishReferenceSelectedSlots rule (.found anchor) slots) =
      .found block := by
  induction slots with
  | nil => simp [scanReferenceSelectedSlots] at found
  | cons view tail ih =>
      rcases view with ⟨slot, status⟩
      cases status with
      | undecided => simp [scanReferenceSelectedSlots] at found
      | commit committed =>
          simpa [finishReferenceSelectedSlots, finishReferenceSelectedSlot,
            scanReferenceSelectedSlots] using found
      | skip =>
          simp [finishReferenceSelectedSlots, finishReferenceSelectedSlot,
            scanReferenceSelectedSlots] at found ⊢
          exact ih found

/-- A final selected-slot list that scans to a commit has a commit decision. -/
theorem final_reference_decisions_found_have_commit
    {Digest : Type}
    {slots : List (ReferenceSelectedSlotView Digest)}
    {decisions : List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))}
    {block : LeaderBlockRef Digest}
    (final : finalReferenceDecisions? slots = some decisions)
    (found : scanReferenceSelectedSlots slots = .found block) :
    orderedDecisionsHaveCommit decisions = true := by
  induction slots generalizing decisions with
  | nil => simp [scanReferenceSelectedSlots] at found
  | cons view tail ih =>
      rcases final_reference_decisions_cons_parts final with
        ⟨headDecision, tailDecisions, headFinal, tailFinal, decisionsShape⟩
      rcases view with ⟨slot, status⟩
      cases status with
      | undecided =>
          simp [ReferenceSelectedSlotView.finalDecision?] at headFinal
      | commit committed =>
          have headShape :
              headDecision = { slot, decision := .commit committed } := by
            simpa [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm
          subst headDecision
          subst decisions
          simp [orderedDecisionsHaveCommit]
      | skip =>
          have headShape : headDecision = { slot, decision := .skip } := by
            simpa [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm
          subst headDecision
          subst decisions
          simp [scanReferenceSelectedSlots] at found
          simpa [orderedDecisionsHaveCommit] using ih tailFinal found

/-- A final selected-slot list with a commit scans to an exact block. -/
theorem final_reference_decisions_with_commit_scan_found
    {Digest : Type}
    {slots : List (ReferenceSelectedSlotView Digest)}
    {decisions : List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))}
    (final : finalReferenceDecisions? slots = some decisions)
    (hasCommit : orderedDecisionsHaveCommit decisions = true) :
    ∃ block, scanReferenceSelectedSlots slots = .found block := by
  induction slots generalizing decisions with
  | nil =>
      have empty : decisions = [] := by
        simpa [finalReferenceDecisions?] using Option.some.inj final.symm
      subst decisions
      simp [orderedDecisionsHaveCommit] at hasCommit
  | cons view tail ih =>
      rcases final_reference_decisions_cons_parts final with
        ⟨headDecision, tailDecisions, headFinal, tailFinal, decisionsShape⟩
      rcases view with ⟨slot, status⟩
      cases status with
      | undecided => simp [ReferenceSelectedSlotView.finalDecision?] at headFinal
      | commit block =>
          exact ⟨block, by simp [scanReferenceSelectedSlots]⟩
      | skip =>
          have headShape : headDecision = { slot, decision := .skip } := by
            simpa [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm
          have tailCommit : orderedDecisionsHaveCommit tailDecisions = true := by
            rw [decisionsShape, headShape] at hasCommit
            simpa [orderedDecisionsHaveCommit] using hasCommit
          rcases ih tailFinal tailCommit with ⟨block, tailFound⟩
          exact ⟨block, by simpa [scanReferenceSelectedSlots] using tailFound⟩

/-- A final round with a commit is a usable exact anchor. -/
theorem reference_final_commit_is_usable_anchor
    {Digest : Type} {round : ReferenceFlexRoundView Digest}
    (committed : ReferenceRoundFinalCommit round) :
    ReferenceRoundUsableAnchor round := by
  rcases committed with ⟨decisions, final, hasCommit⟩
  exact final_reference_decisions_with_commit_scan_found final hasCommit

/-- A found higher anchor makes the decision round final. If the decision round
was already a usable anchor, it becomes a final commit round. -/
theorem finish_reference_round_found_properties
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (anchor : LeaderBlockRef Digest)
    (round : ReferenceFlexRoundView Digest) :
    let finished : ReferenceFlexRoundView Digest :=
      { round with selectedSlots :=
          finishReferenceSelectedSlots rule (.found anchor) round.selectedSlots }
    ReferenceRoundFinal finished ∧
      (ReferenceRoundUsableAnchor round → ReferenceRoundFinalCommit finished) := by
  dsimp
  rcases finish_reference_selected_slots_found_final rule anchor
      round.selectedSlots with ⟨decisions, final⟩
  constructor
  · exact ⟨decisions, final⟩
  · intro usable
    rcases usable with ⟨block, found⟩
    have stillFound := finish_reference_selected_slots_preserves_found
      rule anchor found
    exact ⟨decisions, final,
      final_reference_decisions_found_have_commit final stillFound⟩

/-- A final selected-slot list without a commit scans past the round. -/
theorem final_reference_decisions_without_commit_scan_no_anchor
    {Digest : Type}
    {slots : List (ReferenceSelectedSlotView Digest)}
    {decisions : List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))}
    (final : finalReferenceDecisions? slots = some decisions)
    (noCommit : orderedDecisionsHaveCommit decisions = false) :
    scanReferenceSelectedSlots slots = .noAnchor := by
  induction slots generalizing decisions with
  | nil => rfl
  | cons view tail ih =>
      rcases final_reference_decisions_cons_parts final with
        ⟨headDecision, tailDecisions, headFinal, tailFinal, decisionsShape⟩
      rcases view with ⟨slot, status⟩
      cases status with
      | undecided => simp [ReferenceSelectedSlotView.finalDecision?] at headFinal
      | commit block =>
          have headShape :
              headDecision = { slot, decision := .commit block } := by
            simpa [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm
          rw [decisionsShape, headShape] at noCommit
          simp [orderedDecisionsHaveCommit] at noCommit
      | skip =>
          have headShape : headDecision = { slot, decision := .skip } := by
            simpa [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm
          have tailNoCommit :
              orderedDecisionsHaveCommit tailDecisions = false := by
            rw [decisionsShape, headShape] at noCommit
            simpa [orderedDecisionsHaveCommit] using noCommit
          simpa [scanReferenceSelectedSlots] using ih tailFinal tailNoCommit

/-- A final round either scans past with no anchor or supplies an exact anchor. -/
theorem reference_final_round_scan_decided
    {Digest : Type} {round : ReferenceFlexRoundView Digest}
    (final : ReferenceRoundFinal round) :
    scanReferenceSelectedSlots round.selectedSlots = .noAnchor ∨
      ∃ block, scanReferenceSelectedSlots round.selectedSlots = .found block := by
  rcases final with ⟨decisions, final⟩
  cases hasCommit : orderedDecisionsHaveCommit decisions with
  | false =>
      exact Or.inl
        (final_reference_decisions_without_commit_scan_no_anchor final hasCommit)
  | true =>
      exact Or.inr
        (final_reference_decisions_with_commit_scan_found final hasCommit)

/-! ### Indexed anchor and indirect-step lemmas -/

/-- A usable round at the first scanned index returns its exact anchor. -/
theorem find_indexed_reference_anchor_at_start
    {Digest : Type}
    {rounds : Nat → ReferenceFlexRoundView Digest}
    {start fuel : Nat}
    (fuelPositive : 0 < fuel)
    (usable : ReferenceRoundUsableAnchor (rounds start)) :
    ∃ block,
      findIndexedReferenceAnchorFrom rounds start fuel = .found block := by
  rcases usable with ⟨block, found⟩
  cases fuel with
  | zero => omega
  | succ remaining =>
      exact ⟨block, by simp [findIndexedReferenceAnchorFrom, found]⟩

/-- A final interval ending in a final commit supplies an exact anchor. -/
theorem find_indexed_reference_anchor_after_final_prefix
    {Digest : Type}
    {rounds : Nat → ReferenceFlexRoundView Digest}
    {start : Nat}
    (distance fuel : Nat)
    (fuelCoversEnd : distance < fuel)
    (finalBefore : ∀ offset, offset < distance →
      ReferenceRoundFinal (rounds (start + offset)))
    (commitAtEnd : ReferenceRoundFinalCommit (rounds (start + distance))) :
    ∃ block,
      findIndexedReferenceAnchorFrom rounds start fuel = .found block := by
  induction distance generalizing start fuel with
  | zero =>
      have atStart : ReferenceRoundFinalCommit (rounds start) := by
        simpa using commitAtEnd
      have usable := reference_final_commit_is_usable_anchor atStart
      exact find_indexed_reference_anchor_at_start (by omega) usable
  | succ distance ih =>
      cases fuel with
      | zero => omega
      | succ remaining =>
          have firstFinal := finalBefore 0 (by omega)
          rcases reference_final_round_scan_decided firstFinal with
            noAnchor | ⟨block, found⟩
          · have firstNoAnchor :
                scanReferenceSelectedSlots (rounds start).selectedSlots =
                  .noAnchor := by simpa using noAnchor
            have tailFinal : ∀ offset, offset < distance →
                ReferenceRoundFinal (rounds (start + 1 + offset)) := by
              intro offset beforeEnd
              have nextFinal := finalBefore (offset + 1) (by omega)
              simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using nextFinal
            have tailCommit :
                ReferenceRoundFinalCommit (rounds (start + 1 + distance)) := by
              simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
                commitAtEnd
            rcases ih remaining (by omega) tailFinal tailCommit with
              ⟨tailBlock, tailFound⟩
            exact ⟨tailBlock, by
              simp [findIndexedReferenceAnchorFrom, firstNoAnchor, tailFound]⟩
          · have firstFound :
                scanReferenceSelectedSlots (rounds start).selectedSlots =
                  .found block := by simpa using found
            exact ⟨block, by
              simp [findIndexedReferenceAnchorFrom, firstFound]⟩

/-- One exact indirect step preserves the finite pending-round count. -/
@[simp]
theorem indexed_reference_indirect_step_round_count
    {Digest History : Type}
    (depth decisionIndex : Nat)
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest) :
    (indexedReferenceIndirectStep depth rule decisionIndex state).roundCount =
      state.roundCount := by
  cases found : findIndexedReferenceAnchorFrom state.rounds
      (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) <;>
    simp [indexedReferenceIndirectStep, found]

theorem indexed_reference_indirect_step_round_other
    {Digest History : Type}
    (depth decisionIndex protectedIndex : Nat)
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest)
    (different : protectedIndex ≠ decisionIndex) :
    (indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        protectedIndex = state.rounds protectedIndex := by
  unfold indexedReferenceIndirectStep
  cases found : findIndexedReferenceAnchorFrom state.rounds
      (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) <;>
    simp [found, setIndexedReferenceRound, different]

/-- If the exact anchor scan succeeds, the indirect step finalizes the decision
round. -/
theorem indexed_reference_indirect_step_when_anchor_found
    {Digest History : Type}
    {depth decisionIndex : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    {anchor : LeaderBlockRef Digest}
    (found : findIndexedReferenceAnchorFrom state.rounds
      (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) =
        .found anchor) :
    ReferenceRoundFinal
      ((indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        decisionIndex) := by
  simp only [indexedReferenceIndirectStep, found,
    set_indexed_reference_round_same]
  exact (finish_reference_round_found_properties rule anchor
    (state.rounds decisionIndex)).1

/-- A successful indirect step changes a usable decision round to a final
commit round. -/
theorem indexed_reference_indirect_step_anchor_becomes_final_commit
    {Digest History : Type}
    {depth decisionIndex : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    {anchor : LeaderBlockRef Digest}
    (found : findIndexedReferenceAnchorFrom state.rounds
      (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) =
        .found anchor)
    (decisionAnchor : ReferenceRoundUsableAnchor
      (state.rounds decisionIndex)) :
    ReferenceRoundFinalCommit
      ((indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        decisionIndex) := by
  simp only [indexedReferenceIndirectStep, found,
    set_indexed_reference_round_same]
  exact (finish_reference_round_found_properties rule anchor
    (state.rounds decisionIndex)).2 decisionAnchor

/-- An exact anchor at the configured depth makes the indirect step succeed. -/
theorem indexed_reference_indirect_step_at_exact_anchor
    {Digest History : Type}
    {depth decisionIndex : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (anchorInRange : decisionIndex + depth < state.roundCount)
    (anchor : ReferenceRoundUsableAnchor
      (state.rounds (decisionIndex + depth))) :
    ReferenceRoundFinal
      ((indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        decisionIndex) := by
  have fuelPositive :
      0 < state.roundCount - (decisionIndex + depth) := by omega
  rcases find_indexed_reference_anchor_at_start fuelPositive anchor with
    ⟨block, found⟩
  exact indexed_reference_indirect_step_when_anchor_found found

/-- A final interval that ends in a final commit supplies an anchor for an
earlier indirect step. -/
theorem indexed_reference_indirect_step_after_final_interval
    {Digest History : Type}
    {depth decisionIndex base : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (anchorStartLeBase : decisionIndex + depth ≤ base)
    (baseInRange : base < state.roundCount)
    (finalBeforeBase : ∀ index,
      decisionIndex + depth ≤ index → index < base →
        ReferenceRoundFinal (state.rounds index))
    (commitAtBase : ReferenceRoundFinalCommit (state.rounds base)) :
    ReferenceRoundFinal
      ((indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        decisionIndex) := by
  let anchorStart := decisionIndex + depth
  let distance := base - anchorStart
  let fuel := state.roundCount - anchorStart
  have reachesBase : anchorStart + distance = base := by
    simp only [distance]
    omega
  have fuelCoversEnd : distance < fuel := by
    simp only [distance, fuel, anchorStart]
    omega
  have finalBefore : ∀ offset, offset < distance →
      ReferenceRoundFinal (state.rounds (anchorStart + offset)) := by
    intro offset beforeEnd
    apply finalBeforeBase
    · simp [anchorStart]
    · simp only [distance] at beforeEnd
      omega
  have commitAtEnd :
      ReferenceRoundFinalCommit (state.rounds (anchorStart + distance)) := by
    rw [reachesBase]
    exact commitAtBase
  rcases find_indexed_reference_anchor_after_final_prefix distance fuel
      fuelCoversEnd finalBefore commitAtEnd with ⟨block, found⟩
  exact indexed_reference_indirect_step_when_anchor_found found

/-- One indirect step preserves a usable anchor at every pending index. -/
theorem indexed_reference_indirect_step_preserves_anchor
    {Digest History : Type}
    {depth decisionIndex protectedIndex : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (anchor : ReferenceRoundUsableAnchor (state.rounds protectedIndex)) :
    ReferenceRoundUsableAnchor
      ((indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        protectedIndex) := by
  by_cases same : protectedIndex = decisionIndex
  · subst protectedIndex
    rcases anchor with ⟨block, originalFound⟩
    cases found : findIndexedReferenceAnchorFrom state.rounds
        (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) with
    | blocked =>
        exact ⟨block, by
          simpa [indexedReferenceIndirectStep, found] using originalFound⟩
    | noAnchor =>
        exact ⟨block, by
          simpa [indexedReferenceIndirectStep, found] using originalFound⟩
    | found higherAnchor =>
        refine ⟨block, ?_⟩
        simp only [indexedReferenceIndirectStep, found,
          set_indexed_reference_round_same]
        exact finish_reference_selected_slots_preserves_found
          rule higherAnchor originalFound
  · rw [indexed_reference_indirect_step_round_other depth decisionIndex
      protectedIndex rule state same]
    exact anchor

/-- One indirect step preserves a final round at every pending index. -/
theorem indexed_reference_indirect_step_preserves_final
    {Digest History : Type}
    {depth decisionIndex protectedIndex : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (final : ReferenceRoundFinal (state.rounds protectedIndex)) :
    ReferenceRoundFinal
      ((indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        protectedIndex) := by
  by_cases same : protectedIndex = decisionIndex
  · subst protectedIndex
    cases found : findIndexedReferenceAnchorFrom state.rounds
        (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) with
    | blocked => simpa [indexedReferenceIndirectStep, found] using final
    | noAnchor => simpa [indexedReferenceIndirectStep, found] using final
    | found block =>
        exact indexed_reference_indirect_step_when_anchor_found found
  · rw [indexed_reference_indirect_step_round_other depth decisionIndex
      protectedIndex rule state same]
    exact final

/-- One indirect step preserves a final commit round at every pending index. -/
theorem indexed_reference_indirect_step_preserves_final_commit
    {Digest History : Type}
    {depth decisionIndex protectedIndex : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (committed : ReferenceRoundFinalCommit (state.rounds protectedIndex)) :
    ReferenceRoundFinalCommit
      ((indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        protectedIndex) := by
  by_cases same : protectedIndex = decisionIndex
  · subst protectedIndex
    cases found : findIndexedReferenceAnchorFrom state.rounds
        (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) with
    | blocked => simpa [indexedReferenceIndirectStep, found] using committed
    | noAnchor => simpa [indexedReferenceIndirectStep, found] using committed
    | found block =>
        exact indexed_reference_indirect_step_anchor_becomes_final_commit found
          (reference_final_commit_is_usable_anchor committed)
  · rw [indexed_reference_indirect_step_round_other depth decisionIndex
      protectedIndex rule state same]
    exact committed

/-- Split a descending exact scan after `prefixCount` steps. -/
theorem run_indexed_reference_indirect_descending_append
    {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (highestIndex prefixCount suffixCount : Nat)
    (state : IndexedReferenceFlexState Digest) :
    runIndexedReferenceIndirectDescending depth rule highestIndex
        (prefixCount + suffixCount) state =
      runIndexedReferenceIndirectDescending depth rule
        (highestIndex - prefixCount) suffixCount
        (runIndexedReferenceIndirectDescending depth rule highestIndex
          prefixCount state) := by
  induction suffixCount with
  | zero => simp [runIndexedReferenceIndirectDescending]
  | succ count ih =>
      simp only [runIndexedReferenceIndirectDescending]
      change indexedReferenceIndirectStep depth rule
          (highestIndex - (prefixCount + count))
          (runIndexedReferenceIndirectDescending depth rule highestIndex
            (prefixCount + count) state) =
        indexedReferenceIndirectStep depth rule
          ((highestIndex - prefixCount) - count)
          (runIndexedReferenceIndirectDescending depth rule
            (highestIndex - prefixCount) count
            (runIndexedReferenceIndirectDescending depth rule highestIndex
              prefixCount state))
      rw [ih, Nat.sub_sub]

/-- The exact descending scan changes pending decisions only. It does not change
the current commit index. -/
@[simp]
theorem run_indexed_reference_indirect_commit_index
    {Digest History : Type}
    (depth highestIndex stepCount : Nat)
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest) :
    (runIndexedReferenceIndirectDescending depth rule highestIndex stepCount
      state).commitIndex = state.commitIndex := by
  induction stepCount with
  | zero => rfl
  | succ count =>
      simp only [runIndexedReferenceIndirectDescending,
        indexedReferenceIndirectStep]
      split <;> simp_all

@[simp]
theorem run_indexed_reference_indirect_round_count
    {Digest History : Type}
    (depth highestIndex stepCount : Nat)
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest) :
    (runIndexedReferenceIndirectDescending depth rule highestIndex stepCount
      state).roundCount = state.roundCount := by
  induction stepCount with
  | zero => rfl
  | succ count =>
      simp only [runIndexedReferenceIndirectDescending,
        indexedReferenceIndirectStep]
      split <;> simp_all

theorem run_indexed_reference_indirect_preserves_anchor
    {Digest History : Type}
    {depth highestIndex stepCount protectedIndex : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (anchor : ReferenceRoundUsableAnchor (state.rounds protectedIndex)) :
    ReferenceRoundUsableAnchor
      ((runIndexedReferenceIndirectDescending depth rule highestIndex stepCount
        state).rounds protectedIndex) := by
  induction stepCount with
  | zero => exact anchor
  | succ count =>
      exact indexed_reference_indirect_step_preserves_anchor ‹_›

/-- A complete exact descending scan does not reopen a final pending round. -/
theorem run_indexed_reference_indirect_preserves_final
    {Digest History : Type}
    {depth highestIndex stepCount protectedIndex : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (final : ReferenceRoundFinal (state.rounds protectedIndex)) :
    ReferenceRoundFinal
      ((runIndexedReferenceIndirectDescending depth rule highestIndex stepCount
        state).rounds protectedIndex) := by
  induction stepCount with
  | zero => exact final
  | succ count =>
      exact indexed_reference_indirect_step_preserves_final ‹_›

/-- A complete exact descending scan keeps an existing final commit result. -/
theorem run_indexed_reference_indirect_preserves_final_commit
    {Digest History : Type}
    {depth highestIndex stepCount protectedIndex : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (committed : ReferenceRoundFinalCommit (state.rounds protectedIndex)) :
    ReferenceRoundFinalCommit
      ((runIndexedReferenceIndirectDescending depth rule highestIndex stepCount
        state).rounds protectedIndex) := by
  induction stepCount with
  | zero => exact committed
  | succ count =>
      exact indexed_reference_indirect_step_preserves_final_commit ‹_›

/-- The exact rounds already processed by a descending scan are final. -/
def ReferenceProcessedRoundsFinal {Digest : Type}
    (state : IndexedReferenceFlexState Digest) (base stepCount : Nat) : Prop :=
  ∀ index, index ≤ base → base < index + stepCount →
    ReferenceRoundFinal (state.rounds index)

/-- A descending exact scan closes every processed round. The first processed
usable anchor stays a final commit and then anchors the arbitrary earlier
prefix. -/
theorem run_indexed_reference_indirect_closes_processed_rounds
    {Digest History : Type}
    {depth base stepCount : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (depthPositive : 0 < depth)
    (stepCountBound : stepCount ≤ base + 1)
    (anchorWindow : ReferenceAnchorWindow state base (depth + 1))
    (windowInRange : base + depth < state.roundCount) :
    let result := runIndexedReferenceIndirectDescending depth rule base
      stepCount state
    ReferenceProcessedRoundsFinal result base stepCount ∧
      (0 < stepCount → ReferenceRoundFinalCommit (result.rounds base)) := by
  induction stepCount with
  | zero =>
      constructor
      · intro index indexLe processed
        omega
      · intro positive
        omega
  | succ count ih =>
      have countLeBase : count ≤ base := by omega
      have previousInvariant := ih (by omega)
      let previous := runIndexedReferenceIndirectDescending depth rule base
        count state
      have previousFinal : ReferenceProcessedRoundsFinal previous base count :=
        previousInvariant.1
      have previousBaseCommit : 0 < count →
          ReferenceRoundFinalCommit (previous.rounds base) :=
        previousInvariant.2
      have decisionFinal : ReferenceRoundFinal
          ((indexedReferenceIndirectStep depth rule (base - count) previous).rounds
            (base - count)) := by
        by_cases countWithinWindow : count ≤ depth
        · have originalAnchor : ReferenceRoundUsableAnchor
              (state.rounds (base + (depth - count))) :=
            anchorWindow (depth - count) (by omega)
          have anchorRoundEq :
              base - count + depth = base + (depth - count) := by omega
          have previousAnchor : ReferenceRoundUsableAnchor
              (previous.rounds (base - count + depth)) := by
            rw [anchorRoundEq]
            exact run_indexed_reference_indirect_preserves_anchor originalAnchor
          have anchorInRange :
              base - count + depth < previous.roundCount := by
            simp only [previous, run_indexed_reference_indirect_round_count]
            omega
          exact indexed_reference_indirect_step_at_exact_anchor anchorInRange
            previousAnchor
        · have anchorStartLeBase : base - count + depth ≤ base := by omega
          have baseInRange : base < previous.roundCount := by
            simp only [previous, run_indexed_reference_indirect_round_count]
            omega
          have finalBeforeBase : ∀ index,
              base - count + depth ≤ index → index < base →
                ReferenceRoundFinal (previous.rounds index) := by
            intro index startLe indexBeforeBase
            apply previousFinal index (by omega)
            omega
          exact indexed_reference_indirect_step_after_final_interval
            anchorStartLeBase baseInRange finalBeforeBase
            (previousBaseCommit (by omega))
      have nextFinal : ReferenceProcessedRoundsFinal
          (indexedReferenceIndirectStep depth rule (base - count) previous)
          base (count + 1) := by
        intro index indexLe processed
        by_cases current : index = base - count
        · subst index
          exact decisionFinal
        · have wasProcessed : base < index + count := by omega
          exact indexed_reference_indirect_step_preserves_final
            (previousFinal index indexLe wasProcessed)
      have nextBaseCommit : ReferenceRoundFinalCommit
          ((indexedReferenceIndirectStep depth rule (base - count) previous).rounds
            base) := by
        by_cases firstStep : count = 0
        · subst count
          have anchorAtBase : ReferenceRoundUsableAnchor
              (previous.rounds base) := by
            simp only [previous, runIndexedReferenceIndirectDescending]
            exact anchorWindow 0 (by omega)
          have anchorAtDepth : ReferenceRoundUsableAnchor
              (previous.rounds (base + depth)) := by
            simp only [previous, runIndexedReferenceIndirectDescending]
            exact anchorWindow depth (by omega)
          have fuelPositive :
              0 < previous.roundCount - (base + depth) := by
            simpa [previous] using (show
              0 < state.roundCount - (base + depth) by omega)
          rcases find_indexed_reference_anchor_at_start fuelPositive
              anchorAtDepth with ⟨block, found⟩
          simpa using
            (indexed_reference_indirect_step_anchor_becomes_final_commit
              found anchorAtBase)
        · exact indexed_reference_indirect_step_preserves_final_commit
            (previousBaseCommit (by omega))
      change ReferenceProcessedRoundsFinal
          (indexedReferenceIndirectStep depth rule (base - count) previous)
          base (count + 1) ∧
        (0 < count + 1 → ReferenceRoundFinalCommit
          ((indexedReferenceIndirectStep depth rule
            (base - count) previous).rounds base))
      exact ⟨nextFinal, fun _ => nextBaseCommit⟩

/-! ### Candidate and exact output existence -/

/-- A final prefix that ends in a final commit contains an exact candidate. -/
theorem find_indexed_reference_candidate_after_final_prefix
    {Digest : Type}
    {rounds : Nat → ReferenceFlexRoundView Digest}
    {start : Nat}
    (distance fuel : Nat)
    (fuelCoversEnd : distance < fuel)
    (finalBefore : ∀ offset, offset < distance →
      ReferenceRoundFinal (rounds (start + offset)))
    (commitAtEnd : ReferenceRoundFinalCommit (rounds (start + distance))) :
    ∃ candidate,
      findIndexedReferenceFlexCandidateFrom rounds start fuel = some candidate := by
  induction distance generalizing start fuel with
  | zero =>
      have atStart : ReferenceRoundFinalCommit (rounds start) := by
        simpa using commitAtEnd
      rcases atStart with ⟨decisions, final, hasCommit⟩
      cases fuel with
      | zero => omega
      | succ remaining =>
          let candidate : ReferenceFlexCandidate Digest :=
            { leaderRound := (rounds start).round
              orderedCommittedLeaders :=
                committedLeaderRefsFromDecisions decisions }
          exact ⟨candidate, by
            simp [findIndexedReferenceFlexCandidateFrom, final, hasCommit,
              candidate]⟩
  | succ distance ih =>
      cases fuel with
      | zero => omega
      | succ remaining =>
          have firstFinal := finalBefore 0 (by omega)
          rcases firstFinal with ⟨decisions, final⟩
          have normalizedFinal :
              finalReferenceDecisions? (rounds start).selectedSlots =
                some decisions := by simpa using final
          cases hasCommit : orderedDecisionsHaveCommit decisions with
          | true =>
              let candidate : ReferenceFlexCandidate Digest :=
                { leaderRound := (rounds start).round
                  orderedCommittedLeaders :=
                    committedLeaderRefsFromDecisions decisions }
              exact ⟨candidate, by
                simp [findIndexedReferenceFlexCandidateFrom, normalizedFinal,
                  hasCommit, candidate]⟩
          | false =>
              have tailFinal : ∀ offset, offset < distance →
                  ReferenceRoundFinal (rounds (start + 1 + offset)) := by
                intro offset beforeEnd
                have nextFinal := finalBefore (offset + 1) (by omega)
                simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
                  nextFinal
              have tailCommit :
                  ReferenceRoundFinalCommit (rounds (start + 1 + distance)) := by
                simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
                  commitAtEnd
              rcases ih remaining (by omega) tailFinal tailCommit with
                ⟨candidate, found⟩
              exact ⟨candidate, by
                simp [findIndexedReferenceFlexCandidateFrom, normalizedFinal,
                  hasCommit, found]⟩

/-- A complete descending prefix scan returns an exact candidate. -/
theorem complete_indexed_reference_indirect_prefix_finds_candidate
    {Digest History : Type}
    {depth base : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (depthPositive : 0 < depth)
    (anchorWindow : ReferenceAnchorWindow state base (depth + 1))
    (windowInRange : base + depth < state.roundCount) :
    let result := runIndexedReferenceIndirectDescending depth rule base
      (base + 1) state
    ∃ candidate, findIndexedReferenceFlexCandidate result = some candidate := by
  let result := runIndexedReferenceIndirectDescending depth rule base
    (base + 1) state
  have closed := run_indexed_reference_indirect_closes_processed_rounds
    (rule := rule) depthPositive (Nat.le_refl _) anchorWindow windowInRange
  have finalBefore : ∀ offset, offset < base →
      ReferenceRoundFinal (result.rounds (0 + offset)) := by
    intro offset beforeBase
    simpa [result] using closed.1 offset (by omega) (by omega)
  have commitAtEnd : ReferenceRoundFinalCommit (result.rounds (0 + base)) := by
    simpa using closed.2 (by omega)
  have baseInRange : base < result.roundCount := by
    simp only [result, run_indexed_reference_indirect_round_count]
    omega
  exact find_indexed_reference_candidate_after_final_prefix base
    result.roundCount baseInRange finalBefore commitAtEnd

/-- The full Rust-style high-to-low indirect scan returns an exact candidate
from `d + 1` adjacent usable anchors, even with an arbitrary earlier prefix. -/
theorem full_indexed_reference_anchor_window_finds_candidate
    {Digest History : Type}
    {depth base : Nat}
    {rule : ReferenceIndirectRule Digest History}
    {state : IndexedReferenceFlexState Digest}
    (depthPositive : 0 < depth)
    (anchorWindow : ReferenceAnchorWindow state base (depth + 1))
    (windowInRange : base + depth < state.roundCount) :
    ∃ candidate,
      findIndexedReferenceFlexCandidate
          (runFullIndexedReferenceIndirect depth rule state) =
        some candidate := by
  have depthInRange : depth < state.roundCount := by omega
  let highestIndex := state.roundCount - depth - 1
  have baseLeHighest : base ≤ highestIndex := by
    simp only [highestIndex]
    omega
  let prefixCount := highestIndex - base
  let prefixState := runIndexedReferenceIndirectDescending depth rule
    highestIndex prefixCount state
  have prefixEndsAtBase : highestIndex - prefixCount = base := by
    simp only [prefixCount]
    omega
  have fullStepCount : state.roundCount - depth = highestIndex + 1 := by
    simp only [highestIndex]
    omega
  have splitCount : prefixCount + (base + 1) = state.roundCount - depth := by
    simp only [prefixCount, highestIndex]
    omega
  have splitScan : runFullIndexedReferenceIndirect depth rule state =
      runIndexedReferenceIndirectDescending depth rule base (base + 1)
        prefixState := by
    calc
      runFullIndexedReferenceIndirect depth rule state =
          runIndexedReferenceIndirectDescending depth rule highestIndex
            (state.roundCount - depth) state := by
        simp [runFullIndexedReferenceIndirect, depthInRange, highestIndex]
      _ = runIndexedReferenceIndirectDescending depth rule highestIndex
            (prefixCount + (base + 1)) state := by rw [splitCount]
      _ = runIndexedReferenceIndirectDescending depth rule
            (highestIndex - prefixCount) (base + 1) prefixState := by
        exact run_indexed_reference_indirect_descending_append depth rule
          highestIndex prefixCount (base + 1) state
      _ = runIndexedReferenceIndirectDescending depth rule base (base + 1)
            prefixState := by rw [prefixEndsAtBase]
  have prefixAnchors :
      ReferenceAnchorWindow prefixState base (depth + 1) := by
    intro offset beforeEnd
    exact run_indexed_reference_indirect_preserves_anchor
      (anchorWindow offset beforeEnd)
  have prefixWindowInRange : base + depth < prefixState.roundCount := by
    simpa [prefixState] using windowInRange
  have found := complete_indexed_reference_indirect_prefix_finds_candidate
    (rule := rule) depthPositive prefixAnchors prefixWindowInRange
  rw [splitScan]
  exact found

/-- The executable exact-reference `try_commit` returns an exact output after
the post-direct state contains the sufficient anchor window. -/
theorem reference_anchor_window_gives_exact_try_commit_output
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    {depth base : Nat}
    (directRule : ReferenceDirectRule BlockId)
    (indirectRule : ReferenceIndirectRule BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId)
    (depthPositive : 0 < depth)
    (anchorWindow : ReferenceAnchorWindow
      (runReferenceDirectPass directRule input.pending) base (depth + 1))
    (windowInRange : base + depth <
      (runReferenceDirectPass directRule input.pending).roundCount) :
    ∃ output, tryReferenceFlexCommit functions depth directRule indirectRule
      input = some output := by
  let directState := runReferenceDirectPass directRule input.pending
  cases directFound : findIndexedReferenceFlexCandidate directState with
  | some candidate =>
      exact ⟨input.buildOutput functions candidate, by
        simp [tryReferenceFlexCommit, directState, directFound]⟩
  | none =>
      rcases full_indexed_reference_anchor_window_finds_candidate
          (rule := indirectRule) depthPositive anchorWindow windowInRange with
        ⟨candidate, indirectFound⟩
      exact ⟨input.buildOutput functions candidate, by
        simp [tryReferenceFlexCommit, directState, directFound, indirectFound]⟩

/-- Every successful grouped run records which of the two Rust scans selected
its exact candidate. -/
theorem successful_try_reference_flex_commit_candidate_returned
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId)
    {output : LocalFlexCommitOutput BlockId CommitId}
    (found : tryReferenceFlexCommitWithContext functions context input =
      some output) :
    ReferenceFlexCandidateReturned context input output.candidate := by
  let directState := runReferenceDirectPass context.directRule input.pending
  unfold tryReferenceFlexCommitWithContext tryReferenceFlexCommit at found
  change (match findIndexedReferenceFlexCandidate directState with
    | some candidate => some (input.buildOutput functions candidate)
    | none =>
        match findIndexedReferenceFlexCandidate
            (runFullIndexedReferenceIndirect context.depth context.indirectRule
              directState) with
        | some candidate => some (input.buildOutput functions candidate)
        | none => none) = some output at found
  cases directFound : findIndexedReferenceFlexCandidate directState with
  | some candidate =>
      simp only [directFound] at found
      have outputEq : input.buildOutput functions candidate = output :=
        Option.some.inj found
      rw [← outputEq]
      exact Or.inl directFound
  | none =>
      cases indirectFound : findIndexedReferenceFlexCandidate
          (runFullIndexedReferenceIndirect context.depth context.indirectRule
            directState) with
      | none => simp [directFound, indirectFound] at found
      | some candidate =>
          simp only [directFound, indirectFound] at found
          have outputEq : input.buildOutput functions candidate = output :=
            Option.some.inj found
          rw [← outputEq]
          exact Or.inr ⟨directFound, indirectFound⟩

/-- The source-valid full scan returns an exact next commit body and a complete
one-validator materialization. -/
theorem reference_anchor_window_gives_source_valid_exact_output
    {LocalView BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockId CommitId)
    (view : LocalView)
    (input : ReferenceFlexTryCommitInput BlockId CommitId)
    (_stateValid : ReferenceFlexTryCommitStateValid context input)
    (materialValid : ReferenceFlexTryCommitMaterialValid
      context mapping view input)
    {base : Nat}
    (anchorWindow : ReferenceAnchorWindow
      (runReferenceDirectPass context.directRule input.pending) base
      (context.depth + 1))
    (windowInRange : base + context.depth <
      (runReferenceDirectPass context.directRule input.pending).roundCount) :
    ∃ output,
      tryReferenceFlexCommitWithContext functions context input = some output ∧
        ReferenceFlexCandidateReturned context input output.candidate ∧
        mapping.complete view input.prior output.candidate ∧
        output.reference.index = input.prior.index + 1 ∧
        output.builderInput = output.candidate.toBuilderInput input.prior
          (mapping.localMaterial view input.prior output.candidate) := by
  rcases reference_anchor_window_gives_exact_try_commit_output functions
      context.directRule context.indirectRule input context.depthPositive
      anchorWindow windowInRange with ⟨output, found⟩
  have returned := successful_try_reference_flex_commit_candidate_returned
    functions context input found
  have complete := materialValid.completeForReturned output.candidate returned
  refine ⟨output, found, returned, complete, ?_, ?_⟩
  · unfold tryReferenceFlexCommit at found
    let directState := runReferenceDirectPass context.directRule input.pending
    cases directFound : findIndexedReferenceFlexCandidate directState with
    | some candidate =>
        simp [directState, directFound] at found
        rw [← found]
        rfl
    | none =>
        cases indirectFound : findIndexedReferenceFlexCandidate
            (runFullIndexedReferenceIndirect context.depth context.indirectRule
              directState) with
        | none => simp [directState, directFound, indirectFound] at found
        | some candidate =>
            simp [directState, directFound, indirectFound] at found
            rw [← found]
            rfl
  · unfold tryReferenceFlexCommit at found
    let directState := runReferenceDirectPass context.directRule input.pending
    cases directFound : findIndexedReferenceFlexCandidate directState with
    | some candidate =>
        simp [directState, directFound] at found
        rw [← found]
        simp [ReferenceFlexTryCommitInput.buildOutput,
          buildLocalFlexCommitOutput, materialValid.materialMatches]
    | none =>
        cases indirectFound : findIndexedReferenceFlexCandidate
            (runFullIndexedReferenceIndirect context.depth context.indirectRule
              directState) with
        | none => simp [directState, directFound, indirectFound] at found
        | some candidate =>
            simp [directState, directFound, indirectFound] at found
            rw [← found]
            simp [ReferenceFlexTryCommitInput.buildOutput,
              buildLocalFlexCommitOutput, materialValid.materialMatches]

end Mysticeti
