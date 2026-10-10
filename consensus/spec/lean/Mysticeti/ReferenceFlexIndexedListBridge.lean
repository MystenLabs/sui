/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ReferenceFlexCommitterProgress

namespace Mysticeti

/-!
Exact conversion between the finite indexed FlexCommitter loop and the ordered
list model used by the cross-view safety proof.
-/

/-- A finite indexed anchor scan is the same ordered scan as its list view. -/
theorem find_indexed_reference_anchor_eq_round_list_scan
    {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest)
    (start fuel : Nat) :
    findIndexedReferenceAnchorFrom rounds start fuel =
      scanReferenceAnchorRounds
        (indexedReferenceRoundsFrom rounds start fuel) := by
  induction fuel generalizing start with
  | zero => rfl
  | succ remaining inductionHypothesis =>
      simp only [findIndexedReferenceAnchorFrom, indexedReferenceRoundsFrom,
        scanReferenceAnchorRounds]
      cases head : scanReferenceSelectedSlots (rounds start).selectedSlots with
      | blocked => rfl
      | found block => rfl
      | noAnchor => exact inductionHypothesis (start + 1)

/-- If every round is eligible, the minimum-round scan is the full ordered
anchor scan. -/
theorem scan_reference_anchor_at_or_above_all_eligible
    {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest)
    (start fuel minimumRound : Nat)
    (eligible : ∀ offset, offset < fuel →
      ¬(rounds (start + offset)).round < minimumRound) :
    scanReferenceAnchorAtOrAbove minimumRound
        (indexedReferenceRoundsFrom rounds start fuel) =
      scanReferenceAnchorRounds
        (indexedReferenceRoundsFrom rounds start fuel) := by
  induction fuel generalizing start with
  | zero => rfl
  | succ remaining inductionHypothesis =>
      have headEligible : ¬(rounds start).round < minimumRound := by
        simpa using eligible 0 (by omega)
      have tailEligible : ∀ offset, offset < remaining →
          ¬(rounds (start + 1 + offset)).round < minimumRound := by
        intro offset inRange
        have next := eligible (offset + 1) (by omega)
        simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using next
      simp only [indexedReferenceRoundsFrom, scanReferenceAnchorAtOrAbove,
        scanReferenceAnchorRounds]
      simp only [if_neg headEligible]
      cases head : scanReferenceSelectedSlots (rounds start).selectedSlots with
      | blocked => rfl
      | found block => rfl
      | noAnchor => exact inductionHypothesis (start + 1) tailEligible

/-- On consecutive pending rounds, an absolute-round anchor scan is the indexed
scan that starts at the matching array offset. -/
theorem scan_reference_anchor_at_or_above_consecutive_eq_indexed
    {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest)
    (start fuel firstRound distance : Nat)
    (consecutive : ∀ offset, offset < fuel →
      (rounds (start + offset)).round = firstRound + offset) :
    scanReferenceAnchorAtOrAbove (firstRound + distance)
        (indexedReferenceRoundsFrom rounds start fuel) =
      findIndexedReferenceAnchorFrom rounds (start + distance)
        (fuel - distance) := by
  induction distance generalizing start fuel firstRound with
  | zero =>
      simp only [Nat.add_zero, Nat.sub_zero]
      have eligible : ∀ offset, offset < fuel →
          ¬(rounds (start + offset)).round < firstRound := by
        intro offset inRange
        rw [consecutive offset inRange]
        omega
      rw [scan_reference_anchor_at_or_above_all_eligible
        rounds start fuel firstRound eligible]
      simpa using (find_indexed_reference_anchor_eq_round_list_scan
        rounds start fuel).symm
  | succ priorDistance ih =>
      cases fuel with
      | zero =>
          simp [indexedReferenceRoundsFrom, findIndexedReferenceAnchorFrom,
            scanReferenceAnchorAtOrAbove]
      | succ remaining =>
          have headRound : (rounds start).round = firstRound := by
            simpa using consecutive 0 (by omega)
          have headBelow : (rounds start).round < firstRound + (priorDistance + 1) := by
            rw [headRound]
            omega
          have tailConsecutive : ∀ offset, offset < remaining →
              (rounds (start + 1 + offset)).round =
                firstRound + 1 + offset := by
            intro offset inRange
            have next := consecutive (offset + 1) (by omega)
            simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using next
          simp only [indexedReferenceRoundsFrom, scanReferenceAnchorAtOrAbove,
            if_pos headBelow]
          have tail := ih (start + 1) remaining (firstRound + 1) tailConsecutive
          simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using tail

/-- Without a selected anchor, an indirect selected-slot pass is a no-op. -/
theorem finish_reference_selected_slots_without_anchor
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History)
    (slots : List (ReferenceSelectedSlotView Digest)) :
    (∀ result : ReferenceAnchorScanResult Digest,
      result = .blocked ∨ result = .noAnchor →
        finishReferenceSelectedSlots rule result slots = slots) := by
  intro result noAnchor
  rcases noAnchor with rfl | rfl <;>
    induction slots with
    | nil => rfl
    | cons head tail inductionHypothesis =>
        rcases head with ⟨slot, status⟩
        cases status <;>
          change _ :: finishReferenceSelectedSlots rule _ tail = _ :: tail <;>
          rw [inductionHypothesis] <;>
          simp [finishReferenceSelectedSlot]

/-- One indexed indirect step is the same as finishing the matching list round
against its already processed higher suffix. -/
theorem indexed_reference_indirect_step_round_eq_list_finish
    {Digest History : Type}
    {depth decisionIndex firstRound : Nat}
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest)
    (depthPositive : 0 < depth)
    (decisionInRange : decisionIndex < state.roundCount)
    (consecutive : state.RoundsConsecutive firstRound) :
    (indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        decisionIndex =
      finishReferenceFlexRoundAtDepth rule depth
        (indexedReferenceRoundsFrom state.rounds (decisionIndex + 1)
          (state.roundCount - (decisionIndex + 1)))
        (state.rounds decisionIndex) := by
  have currentRound := consecutive decisionIndex decisionInRange
  have suffixConsecutive : ∀ offset,
      offset < state.roundCount - (decisionIndex + 1) →
        (state.rounds (decisionIndex + 1 + offset)).round =
          firstRound + decisionIndex + 1 + offset := by
    intro offset offsetInRange
    have inRange : decisionIndex + 1 + offset < state.roundCount := by omega
    have roundEq := consecutive (decisionIndex + 1 + offset) inRange
    simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using roundEq
  have scanBridge := scan_reference_anchor_at_or_above_consecutive_eq_indexed
    state.rounds (decisionIndex + 1)
      (state.roundCount - (decisionIndex + 1))
      (firstRound + decisionIndex + 1) (depth - 1) suffixConsecutive
  have anchorEq :
      scanReferenceAnchorAtOrAbove
          ((state.rounds decisionIndex).round + depth)
          (indexedReferenceRoundsFrom state.rounds (decisionIndex + 1)
            (state.roundCount - (decisionIndex + 1))) =
        findIndexedReferenceAnchorFrom state.rounds (decisionIndex + depth)
          (state.roundCount - (decisionIndex + depth)) := by
    rw [currentRound]
    have thresholdEq : firstRound + decisionIndex + depth =
        firstRound + decisionIndex + 1 + (depth - 1) := by omega
    have startEq : decisionIndex + depth =
        decisionIndex + 1 + (depth - 1) := by omega
    have fuelEq : state.roundCount - (decisionIndex + depth) =
        state.roundCount - (decisionIndex + 1) - (depth - 1) := by omega
    rw [thresholdEq, fuelEq, startEq]
    exact scanBridge
  unfold indexedReferenceIndirectStep finishReferenceFlexRoundAtDepth
  rw [anchorEq]
  cases anchor : findIndexedReferenceAnchorFrom state.rounds
      (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) with
  | blocked =>
      simp [anchor, finish_reference_selected_slots_without_anchor]
  | noAnchor =>
      simp [anchor, finish_reference_selected_slots_without_anchor]
  | found block =>
      simp [anchor, setIndexedReferenceRound]

/-- One indirect step preserves every pending round number. -/
theorem indexed_reference_indirect_step_preserves_round_number
    {Digest History : Type}
    (depth decisionIndex index : Nat)
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest) :
    ((indexedReferenceIndirectStep depth rule decisionIndex state).rounds
      index).round = (state.rounds index).round := by
  by_cases same : index = decisionIndex
  · subst index
    unfold indexedReferenceIndirectStep
    cases anchor : findIndexedReferenceAnchorFrom state.rounds
        (decisionIndex + depth)
        (state.roundCount - (decisionIndex + depth)) <;>
      simp [anchor, setIndexedReferenceRound]
  · rw [indexed_reference_indirect_step_round_other depth decisionIndex
      index rule state same]

/-- One indirect step preserves consecutive pending-round indexes. -/
theorem indexed_reference_indirect_step_preserves_consecutive
    {Digest History : Type}
    (depth decisionIndex firstRound : Nat)
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest)
    (consecutive : state.RoundsConsecutive firstRound) :
    (indexedReferenceIndirectStep depth rule decisionIndex state).RoundsConsecutive
      firstRound := by
  intro index inRange
  rw [indexed_reference_indirect_step_preserves_round_number]
  apply consecutive index
  simpa using inRange

/-- The full descending loop preserves consecutive pending-round indexes. -/
theorem run_indexed_reference_indirect_preserves_consecutive
    {Digest History : Type}
    (depth highestIndex stepCount firstRound : Nat)
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest)
    (consecutive : state.RoundsConsecutive firstRound) :
    (runIndexedReferenceIndirectDescending depth rule highestIndex stepCount
      state).RoundsConsecutive firstRound := by
  induction stepCount with
  | zero => exact consecutive
  | succ count inductionHypothesis =>
      exact indexed_reference_indirect_step_preserves_consecutive
        depth (highestIndex - count) firstRound rule _ inductionHypothesis

/-- One indirect step does not change the finite suffix above its decision
index. -/
theorem indexed_reference_indirect_step_range_above
    {Digest History : Type}
    (depth decisionIndex start fuel : Nat)
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest)
    (above : decisionIndex < start) :
    indexedReferenceRoundsFrom
        (indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        start fuel =
      indexedReferenceRoundsFrom state.rounds start fuel := by
  induction fuel generalizing start with
  | zero => rfl
  | succ remaining inductionHypothesis =>
      simp only [indexedReferenceRoundsFrom]
      rw [indexed_reference_indirect_step_round_other depth decisionIndex
        start rule state (by omega)]
      exact congrArg (fun suffix => state.rounds start :: suffix)
        (inductionHypothesis (start + 1) (by omega))

/-- One indirect step does not change the finite suffix above its decision
index. -/
theorem indexed_reference_indirect_step_suffix_above
    {Digest History : Type}
    (depth decisionIndex fuel : Nat)
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest) :
    indexedReferenceRoundsFrom
        (indexedReferenceIndirectStep depth rule decisionIndex state).rounds
        (decisionIndex + 1) fuel =
      indexedReferenceRoundsFrom state.rounds (decisionIndex + 1) fuel := by
  exact indexed_reference_indirect_step_range_above depth decisionIndex
    (decisionIndex + 1) fuel rule state (by omega)

/-- A consecutive indexed suffix with at most `depth` rounds has no indirect
decision index. The recursive list finisher is therefore the identity. -/
theorem finish_reference_indexed_short_suffix_identity
    {Digest History : Type}
    {depth start fuel firstRound : Nat}
    (rule : ReferenceIndirectRule Digest History)
    (rounds : Nat → ReferenceFlexRoundView Digest)
    (depthPositive : 0 < depth)
    (short : fuel ≤ depth)
    (consecutive : ∀ offset, offset < fuel →
      (rounds (start + offset)).round = firstRound + offset) :
    finishReferenceFlexRoundsAtDepth rule depth
        (indexedReferenceRoundsFrom rounds start fuel) =
      indexedReferenceRoundsFrom rounds start fuel := by
  induction fuel generalizing start firstRound with
  | zero => rfl
  | succ remaining inductionHypothesis =>
      have headRound : (rounds start).round = firstRound := by
        simpa using consecutive 0 (by omega)
      have tailConsecutive : ∀ offset, offset < remaining →
          (rounds (start + 1 + offset)).round = firstRound + 1 + offset := by
        intro offset inRange
        have next := consecutive (offset + 1) (by omega)
        simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using next
      have tailIdentity := inductionHypothesis (by omega) tailConsecutive
      have anchorBridge := scan_reference_anchor_at_or_above_consecutive_eq_indexed
        rounds (start + 1) remaining (firstRound + 1) (depth - 1)
          tailConsecutive
      have noAnchor : scanReferenceAnchorAtOrAbove
          ((rounds start).round + depth)
          (indexedReferenceRoundsFrom rounds (start + 1) remaining) =
        .noAnchor := by
        rw [headRound]
        have thresholdEq : firstRound + depth =
            firstRound + 1 + (depth - 1) := by omega
        rw [thresholdEq, anchorBridge]
        have noFuel : remaining - (depth - 1) = 0 := by omega
        rw [noFuel]
        rfl
      simp only [indexedReferenceRoundsFrom, finishReferenceFlexRoundsAtDepth]
      rw [tailIdentity]
      simp [finishReferenceFlexRoundAtDepth, noAnchor,
        finish_reference_selected_slots_without_anchor]

/-- Rounds below the processed descending suffix are unchanged. -/
theorem run_indexed_reference_indirect_lower_unchanged
    {Digest History : Type}
    {depth highestIndex stepCount index : Nat}
    (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest)
    (stepBound : stepCount ≤ highestIndex + 1)
    (below : index < highestIndex + 1 - stepCount) :
    (runIndexedReferenceIndirectDescending depth rule highestIndex stepCount
      state).rounds index = state.rounds index := by
  induction stepCount with
  | zero => rfl
  | succ count inductionHypothesis =>
      simp only [runIndexedReferenceIndirectDescending]
      rw [indexed_reference_indirect_step_round_other depth
        (highestIndex - count) index rule _ (by omega)]
      exact inductionHypothesis (by omega) (by omega)

/-- After `stepCount` high-to-low steps, the processed suffix is exactly the
recursive list finisher on the same original suffix. -/
theorem run_indexed_reference_indirect_suffix_eq_list_finish
    {Digest History : Type}
    {state : IndexedReferenceFlexState Digest}
    {firstRound depth highestIndex stepCount : Nat}
    (rule : ReferenceIndirectRule Digest History)
    (consecutive : state.RoundsConsecutive firstRound)
    (depthPositive : 0 < depth)
    (shape : state.roundCount = highestIndex + depth + 1)
    (stepBound : stepCount ≤ highestIndex + 1) :
    let boundary := highestIndex + 1 - stepCount
    let result := runIndexedReferenceIndirectDescending depth rule
      highestIndex stepCount state
    indexedReferenceRoundsFrom result.rounds boundary
        (state.roundCount - boundary) =
      finishReferenceFlexRoundsAtDepth rule depth
        (indexedReferenceRoundsFrom state.rounds boundary
          (state.roundCount - boundary)) := by
  induction stepCount with
  | zero =>
      simp only [Nat.sub_zero, runIndexedReferenceIndirectDescending]
      have short : state.roundCount - (highestIndex + 1) ≤ depth := by omega
      have suffixConsecutive : ∀ offset,
          offset < state.roundCount - (highestIndex + 1) →
            (state.rounds (highestIndex + 1 + offset)).round =
              firstRound + highestIndex + 1 + offset := by
        intro offset inRange
        have indexed : highestIndex + 1 + offset < state.roundCount := by omega
        have roundEq := consecutive (highestIndex + 1 + offset) indexed
        simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using roundEq
      exact (finish_reference_indexed_short_suffix_identity rule state.rounds
        depthPositive short suffixConsecutive).symm
  | succ count inductionHypothesis =>
      have countBound : count ≤ highestIndex + 1 := by omega
      have countLeHighest : count ≤ highestIndex := by omega
      let previous := runIndexedReferenceIndirectDescending depth rule
        highestIndex count state
      let decisionIndex := highestIndex - count
      have previousConsecutive : previous.RoundsConsecutive firstRound :=
        run_indexed_reference_indirect_preserves_consecutive
          depth highestIndex count firstRound rule state consecutive
      have previousRoundCount : previous.roundCount = state.roundCount := by
        simp [previous]
      have decisionInRange : decisionIndex < previous.roundCount := by
        simp only [decisionIndex, previousRoundCount, shape]
        omega
      have previousBoundary : highestIndex + 1 - count = decisionIndex + 1 := by
        simp only [decisionIndex]
        omega
      have nextBoundary : highestIndex + 1 - (count + 1) = decisionIndex := by
        simp only [decisionIndex]
        omega
      have currentUnchanged : previous.rounds decisionIndex =
          state.rounds decisionIndex := by
        apply run_indexed_reference_indirect_lower_unchanged rule state countBound
        rw [previousBoundary]
        omega
      have previousTail :
          indexedReferenceRoundsFrom previous.rounds (decisionIndex + 1)
              (previous.roundCount - (decisionIndex + 1)) =
            finishReferenceFlexRoundsAtDepth rule depth
              (indexedReferenceRoundsFrom state.rounds (decisionIndex + 1)
                (state.roundCount - (decisionIndex + 1))) := by
        simpa [previous, previousRoundCount, previousBoundary] using
          inductionHypothesis countBound
      have headFinished :
          (indexedReferenceIndirectStep depth rule decisionIndex previous).rounds
              decisionIndex =
            finishReferenceFlexRoundAtDepth rule depth
              (finishReferenceFlexRoundsAtDepth rule depth
                (indexedReferenceRoundsFrom state.rounds (decisionIndex + 1)
                  (state.roundCount - (decisionIndex + 1))))
              (state.rounds decisionIndex) := by
        rw [indexed_reference_indirect_step_round_eq_list_finish rule previous
          depthPositive decisionInRange previousConsecutive,
          previousTail, currentUnchanged]
      have tailUnchanged :
          indexedReferenceRoundsFrom
              (indexedReferenceIndirectStep depth rule decisionIndex previous).rounds
              (decisionIndex + 1)
              (state.roundCount - (decisionIndex + 1)) =
            indexedReferenceRoundsFrom previous.rounds (decisionIndex + 1)
              (state.roundCount - (decisionIndex + 1)) :=
        indexed_reference_indirect_step_range_above depth decisionIndex
          (decisionIndex + 1) (state.roundCount - (decisionIndex + 1))
          rule previous (by omega)
      have fuelSucc : state.roundCount - decisionIndex =
          (state.roundCount - (decisionIndex + 1)) + 1 := by omega
      simp only [runIndexedReferenceIndirectDescending]
      rw [nextBoundary, fuelSucc]
      simp only [indexedReferenceRoundsFrom,
        finishReferenceFlexRoundsAtDepth]
      rw [headFinished, tailUnchanged]
      have previousTailSameFuel :
          indexedReferenceRoundsFrom previous.rounds (decisionIndex + 1)
              (state.roundCount - (decisionIndex + 1)) =
            finishReferenceFlexRoundsAtDepth rule depth
              (indexedReferenceRoundsFrom state.rounds (decisionIndex + 1)
                (state.roundCount - (decisionIndex + 1))) := by
        simpa [previousRoundCount] using previousTail
      rw [previousTailSameFuel]

/-- The complete finite indexed loop is exactly the recursive high-to-low list
finisher. The positive-depth and consecutive-round conditions are the Rust v3
pending-array shape. -/
theorem run_full_indexed_reference_indirect_to_round_list
    {Digest History : Type}
    {state : IndexedReferenceFlexState Digest}
    {firstRound depth : Nat}
    (rule : ReferenceIndirectRule Digest History)
    (consecutive : state.RoundsConsecutive firstRound)
    (depthPositive : 0 < depth) :
    (runFullIndexedReferenceIndirect depth rule state).toRoundList =
      finishReferenceFlexRoundsAtDepth rule depth state.toRoundList := by
  by_cases hasDecision : depth < state.roundCount
  · let highestIndex := state.roundCount - depth - 1
    let stepCount := state.roundCount - depth
    have shape : state.roundCount = highestIndex + depth + 1 := by
      simp only [highestIndex]
      omega
    have stepBound : stepCount ≤ highestIndex + 1 := by
      simp only [stepCount, highestIndex]
      omega
    have bridge := run_indexed_reference_indirect_suffix_eq_list_finish
      rule consecutive depthPositive shape stepBound
    simp only [highestIndex, stepCount] at bridge
    have boundaryZero :
        state.roundCount - depth - 1 + 1 -
          (state.roundCount - depth) = 0 := by omega
    simp only [boundaryZero, Nat.sub_zero] at bridge
    simpa [runFullIndexedReferenceIndirect, hasDecision,
      IndexedReferenceFlexState.toRoundList] using bridge
  · have short : state.roundCount ≤ depth := by omega
    have suffixConsecutive : ∀ offset, offset < state.roundCount →
        (state.rounds (0 + offset)).round = firstRound + offset := by
      intro offset inRange
      simpa using consecutive offset inRange
    have unchanged := finish_reference_indexed_short_suffix_identity
      (depth := depth) (start := 0) (fuel := state.roundCount)
      rule state.rounds depthPositive short suffixConsecutive
    simp only [IndexedReferenceFlexState.toRoundList] at unchanged ⊢
    simp [runFullIndexedReferenceIndirect, hasDecision]
    exact unchanged.symm

/-- The candidate selected after the indexed indirect loop is exactly the
candidate selected after the recursive list finisher. -/
theorem find_full_indexed_reference_candidate_eq_list_finish
    {Digest History : Type}
    {state : IndexedReferenceFlexState Digest}
    {firstRound depth : Nat}
    (rule : ReferenceIndirectRule Digest History)
    (consecutive : state.RoundsConsecutive firstRound)
    (depthPositive : 0 < depth) :
    findIndexedReferenceFlexCandidate
        (runFullIndexedReferenceIndirect depth rule state) =
      findReferenceFlexCommitCandidate
        (finishReferenceFlexRoundsAtDepth rule depth state.toRoundList) := by
  rw [find_indexed_reference_candidate_eq_state_list_scan,
    run_full_indexed_reference_indirect_to_round_list rule consecutive
      depthPositive]

/-- The direct pass preserves the indexed pending-round shape. -/
theorem run_reference_direct_pass_preserves_consecutive
    {Digest : Type}
    (rule : ReferenceDirectRule Digest)
    (state : IndexedReferenceFlexState Digest)
    (firstRound : Nat)
    (consecutive : state.RoundsConsecutive firstRound) :
    (runReferenceDirectPass rule state).RoundsConsecutive firstRound := by
  intro index inRange
  simpa [runReferenceDirectPass, runReferenceDirectRound] using
    consecutive index inRange

/-- Every candidate returned by the Rust-order two-scan function appears in the
one recursive finished list. This normalizes the direct-success and
indirect-success paths without assuming one common candidate. -/
theorem successful_try_reference_flex_commit_candidate_in_finished_list
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId)
    (stateValid : ReferenceFlexTryCommitStateValid context input)
    {output : LocalFlexCommitOutput BlockId CommitId}
    (found : tryReferenceFlexCommitWithContext functions context input =
      some output) :
    findReferenceFlexCommitCandidate
        (finishReferenceFlexRoundsAtDepth context.indirectRule context.depth
          (runReferenceDirectPass context.directRule input.pending).toRoundList) =
      some output.candidate := by
  let directState := runReferenceDirectPass context.directRule input.pending
  have directConsecutive : directState.RoundsConsecutive
      stateValid.firstPendingRound :=
    run_reference_direct_pass_preserves_consecutive context.directRule
      input.pending stateValid.firstPendingRound stateValid.roundsConsecutive
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
      have listFound : findReferenceFlexCommitCandidate directState.toRoundList =
          some candidate := by
        rw [← find_indexed_reference_candidate_eq_state_list_scan]
        exact directFound
      have preserved :=
        finish_reference_flex_rounds_at_depth_preserves_candidate
          context.indirectRule context.depth listFound
      rw [← outputEq]
      exact preserved
  | none =>
      cases indirectFound : findIndexedReferenceFlexCandidate
          (runFullIndexedReferenceIndirect context.depth context.indirectRule
            directState) with
      | none => simp [directFound, indirectFound] at found
      | some candidate =>
          simp only [directFound, indirectFound] at found
          have outputEq : input.buildOutput functions candidate = output :=
            Option.some.inj found
          have normalized := find_full_indexed_reference_candidate_eq_list_finish
            context.indirectRule directConsecutive context.depthPositive
          rw [indirectFound] at normalized
          rw [← outputEq]
          exact normalized.symm

/-- Any successful two-scan run builds its output from its returned candidate
and the exact local materializer. -/
theorem successful_try_reference_flex_commit_exact_output
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId)
    {output : LocalFlexCommitOutput BlockId CommitId}
    (found : tryReferenceFlexCommitWithContext functions context input =
      some output) :
    output.builderInput = output.candidate.toBuilderInput input.prior
        (input.materialize output.candidate) ∧
      output.record = output.builderInput.toCommitRecord ∧
      output.reference =
        constructExactCommitReference functions output.record := by
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
      rw [← Option.some.inj found]
      exact ⟨rfl, rfl, rfl⟩
  | none =>
      cases indirectFound : findIndexedReferenceFlexCandidate
          (runFullIndexedReferenceIndirect context.depth context.indirectRule
            directState) with
      | none => simp [directFound, indirectFound] at found
      | some candidate =>
          simp only [directFound, indirectFound] at found
          rw [← Option.some.inj found]
          exact ⟨rfl, rfl, rfl⟩

/-- Two successful Rust-order executions produce the same complete exact
output. Candidate equality is a conclusion. The premise compares only the
post-direct ordered slot results and their local direct-versus-indirect safety
facts. -/
theorem successful_cross_view_try_reference_flex_outputs_agree
    {LocalView BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockId CommitId)
    (leftContext rightContext :
      ReferenceFlexCommitterContext BlockId History)
    (sameDepth : leftContext.depth = rightContext.depth)
    (sameIndirectRule : leftContext.indirectRule = rightContext.indirectRule)
    (leftView rightView : LocalView)
    (leftInput rightInput : ReferenceFlexTryCommitInput BlockId CommitId)
    (leftStateValid : ReferenceFlexTryCommitStateValid leftContext leftInput)
    (rightStateValid : ReferenceFlexTryCommitStateValid rightContext rightInput)
    (directAgreement : ExactPrefixAgreement
      (CrossViewDirectRoundAgreement leftContext.indirectRule)
      (runReferenceDirectPass leftContext.directRule
        leftInput.pending).toRoundList
      (runReferenceDirectPass rightContext.directRule
        rightInput.pending).toRoundList)
    (leftMaterialValid : ReferenceFlexTryCommitMaterialValid
      leftContext mapping leftView leftInput)
    (rightMaterialValid : ReferenceFlexTryCommitMaterialValid
      rightContext mapping rightView rightInput)
    (samePrior : leftInput.prior = rightInput.prior)
    {leftOutput rightOutput : LocalFlexCommitOutput BlockId CommitId}
    (leftFound : tryReferenceFlexCommitWithContext functions leftContext
      leftInput = some leftOutput)
    (rightFound : tryReferenceFlexCommitWithContext functions rightContext
      rightInput = some rightOutput) :
    leftOutput = rightOutput := by
  have leftNormalized :=
    successful_try_reference_flex_commit_candidate_in_finished_list
      functions leftContext leftInput leftStateValid leftFound
  have rightNormalized :=
    successful_try_reference_flex_commit_candidate_in_finished_list
      functions rightContext rightInput rightStateValid rightFound
  have rightNormalizedCommon :
      findReferenceFlexCommitCandidate
          (finishReferenceFlexRoundsAtDepth leftContext.indirectRule
            leftContext.depth
            (runReferenceDirectPass rightContext.directRule
              rightInput.pending).toRoundList) =
        some rightOutput.candidate := by
    rw [sameDepth, sameIndirectRule]
    exact rightNormalized
  have sameCandidate :=
    recursive_flex_commit_candidates_at_depth_prefix_agree
      leftContext.indirectRule leftContext.depth directAgreement
      leftNormalized rightNormalizedCommon
  have leftReturned := successful_try_reference_flex_commit_candidate_returned
    functions leftContext leftInput leftFound
  have rightReturned := successful_try_reference_flex_commit_candidate_returned
    functions rightContext rightInput rightFound
  have leftComplete := leftMaterialValid.completeForReturned
    leftOutput.candidate leftReturned
  have rightComplete := rightMaterialValid.completeForReturned
    rightOutput.candidate rightReturned
  have sameMappedInput := mapping.complete_local_inputs_agree samePrior
    sameCandidate leftComplete rightComplete
  have leftExact := successful_try_reference_flex_commit_exact_output
    functions leftContext leftInput leftFound
  have rightExact := successful_try_reference_flex_commit_exact_output
    functions rightContext rightInput rightFound
  have leftBuilder : leftOutput.builderInput =
      mapping.localInput leftView leftInput.prior leftOutput.candidate := by
    rw [leftExact.1, leftMaterialValid.materialMatches]
    rfl
  have rightBuilder : rightOutput.builderInput =
      mapping.localInput rightView rightInput.prior rightOutput.candidate := by
    rw [rightExact.1, rightMaterialValid.materialMatches]
    rfl
  have sameBuilder : leftOutput.builderInput = rightOutput.builderInput := by
    rw [leftBuilder, rightBuilder]
    exact sameMappedInput
  have sameRecord : leftOutput.record = rightOutput.record := by
    rw [leftExact.2.1, rightExact.2.1, sameBuilder]
  have sameReference : leftOutput.reference = rightOutput.reference := by
    rw [leftExact.2.2, rightExact.2.2, sameRecord]
  cases leftOutput
  cases rightOutput
  simp only at sameCandidate sameBuilder sameRecord sameReference
  subst_vars
  rfl

end Mysticeti
