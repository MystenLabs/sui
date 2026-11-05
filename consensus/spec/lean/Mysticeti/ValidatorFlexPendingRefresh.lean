/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorFlexCommitter

namespace Mysticeti

/-!
One-host refinement for the pending state used by
`FlexCommitter::try_commit`.

Rust refreshes its cached pending rounds before each scan. An unchanged commit
index keeps the cache. A changed leader schedule clears it. An advanced commit
index with the same schedule drops the processed prefix. The scan then appends
undecided rounds up to the current local DAG frontier.

This module binds that internal work to an actual `runCommitter` action. It
does not require a future run, block, commit, or network result.
-/

/-- The exact cached pending state before one Rust refresh. -/
structure ValidatorFlexPendingCache
    (BlockId CommitId ScheduleKey : Type) where
  baseline : ValidatorCommitHead CommitId
  minimumLeaderRound : Nat
  scheduleKey : ScheduleKey
  rounds : List (ReferenceFlexRoundView BlockId)

/-- Drop cached rounds that precede the current minimum leader round. -/
def dropValidatorFlexRoundsBefore {BlockId : Type}
    (minimumLeaderRound : Nat)
    (rounds : List (ReferenceFlexRoundView BlockId)) :
    List (ReferenceFlexRoundView BlockId) :=
  rounds.dropWhile fun round => decide (round.round < minimumLeaderRound)

/-- Rust's three-case pending-state refresh. -/
def refreshValidatorFlexPendingCache
    {BlockId CommitId ScheduleKey : Type} [DecidableEq ScheduleKey]
    (minimumLeaderRoundForHead : ValidatorCommitHead CommitId → Nat)
    (scheduleKeyForHead : ValidatorCommitHead CommitId → ScheduleKey)
    (head : ValidatorCommitHead CommitId)
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey) :
    ValidatorFlexPendingCache BlockId CommitId ScheduleKey :=
  if cache.baseline.index = head.index then
    cache
  else if cache.scheduleKey = scheduleKeyForHead head then
    { baseline := head
      minimumLeaderRound := minimumLeaderRoundForHead head
      scheduleKey := scheduleKeyForHead head
      rounds := dropValidatorFlexRoundsBefore
        (minimumLeaderRoundForHead head) cache.rounds }
  else
    { baseline := head
      minimumLeaderRound := minimumLeaderRoundForHead head
      scheduleKey := scheduleKeyForHead head
      rounds := [] }

/-- An unchanged commit index keeps the complete pending-round cache. -/
@[simp]
theorem refresh_validator_flex_pending_cache_same_index
    {BlockId CommitId ScheduleKey : Type} [DecidableEq ScheduleKey]
    (minimumLeaderRoundForHead : ValidatorCommitHead CommitId → Nat)
    (scheduleKeyForHead : ValidatorCommitHead CommitId → ScheduleKey)
    (head : ValidatorCommitHead CommitId)
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (sameIndex : cache.baseline.index = head.index) :
    refreshValidatorFlexPendingCache minimumLeaderRoundForHead
      scheduleKeyForHead head cache = cache := by
  simp [refreshValidatorFlexPendingCache, sameIndex]

/-- A changed commit index and a changed schedule key clear all cached rounds. -/
@[simp]
theorem refresh_validator_flex_pending_cache_schedule_change
    {BlockId CommitId ScheduleKey : Type} [DecidableEq ScheduleKey]
    (minimumLeaderRoundForHead : ValidatorCommitHead CommitId → Nat)
    (scheduleKeyForHead : ValidatorCommitHead CommitId → ScheduleKey)
    (head : ValidatorCommitHead CommitId)
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (indexChanged : cache.baseline.index ≠ head.index)
    (scheduleChanged : cache.scheduleKey ≠ scheduleKeyForHead head) :
    refreshValidatorFlexPendingCache minimumLeaderRoundForHead
      scheduleKeyForHead head cache =
        { baseline := head
          minimumLeaderRound := minimumLeaderRoundForHead head
          scheduleKey := scheduleKeyForHead head
          rounds := [] } := by
  simp [refreshValidatorFlexPendingCache, indexChanged, scheduleChanged]

/-- A changed commit index with the same schedule key keeps only rounds at or
after the new minimum leader round. -/
@[simp]
theorem refresh_validator_flex_pending_cache_same_schedule
    {BlockId CommitId ScheduleKey : Type} [DecidableEq ScheduleKey]
    (minimumLeaderRoundForHead : ValidatorCommitHead CommitId → Nat)
    (scheduleKeyForHead : ValidatorCommitHead CommitId → ScheduleKey)
    (head : ValidatorCommitHead CommitId)
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (indexChanged : cache.baseline.index ≠ head.index)
    (scheduleSame : cache.scheduleKey = scheduleKeyForHead head) :
    refreshValidatorFlexPendingCache minimumLeaderRoundForHead
      scheduleKeyForHead head cache =
        { baseline := head
          minimumLeaderRound := minimumLeaderRoundForHead head
          scheduleKey := scheduleKeyForHead head
          rounds := dropValidatorFlexRoundsBefore
            (minimumLeaderRoundForHead head) cache.rounds } := by
  simp [refreshValidatorFlexPendingCache, indexChanged, scheduleSame]

/-- Create one new round with the exact selected-slot order and no decision. -/
def undecidedReferenceFlexRound {BlockId : Type} (round : Nat)
    (slots : List ExactSelectedLeaderSlot) :
    ReferenceFlexRoundView BlockId :=
  { round
    selectedSlots := slots.map fun slot =>
      { slot
        status := .undecided } }

/-- Append every missing round below the local highest-accepted frontier. -/
def appendMissingValidatorFlexRounds {BlockId : Type}
    (minimumLeaderRound highestAcceptedRound : Nat)
    (selectedSlots : Nat → List ExactSelectedLeaderSlot)
    (retained : List (ReferenceFlexRoundView BlockId)) :
    List (ReferenceFlexRoundView BlockId) :=
  let firstMissing := minimumLeaderRound + retained.length
  retained ++
    (List.range (highestAcceptedRound - firstMissing)).map fun offset =>
      undecidedReferenceFlexRound (firstMissing + offset)
        (selectedSlots (firstMissing + offset))

/-- Build the exact input that Rust scans after refresh and append. -/
def prepareValidatorFlexScanInput
    {BlockId CommitId ScheduleKey : Type} [DecidableEq ScheduleKey]
    (minimumLeaderRoundForHead : ValidatorCommitHead CommitId → Nat)
    (scheduleKeyForHead : ValidatorCommitHead CommitId → ScheduleKey)
    (selectedSlots : Nat → List ExactSelectedLeaderSlot)
    (highestAcceptedRound : Nat)
    (head : ValidatorCommitHead CommitId)
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (materialize : ReferenceFlexCandidate BlockId →
      ExactCommitBuildMaterial (LeaderBlockRef BlockId)) :
    ReferenceFlexTryCommitInput BlockId CommitId :=
  let refreshed := refreshValidatorFlexPendingCache
    minimumLeaderRoundForHead scheduleKeyForHead head cache
  let rounds := appendMissingValidatorFlexRounds
    (minimumLeaderRoundForHead head) highestAcceptedRound selectedSlots
    refreshed.rounds
  { prior := { index := head.index, digest := head.id }
    pending := indexedReferenceStateFromList head.index rounds
    materialize }

/-- The exact state and result of one complete direct-then-indirect scan. -/
structure ReferenceFlexRunState
    (BlockId CommitId : Type) where
  prepared : ReferenceFlexTryCommitInput BlockId CommitId
  postScan : IndexedReferenceFlexState BlockId
  output : Option (LocalFlexCommitOutput BlockId CommitId)

/-- Execute the exact scan and retain its internal post-scan state. -/
def executeReferenceFlexCommitWithContext
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId) :
    ReferenceFlexRunState BlockId CommitId :=
  let directState := runReferenceDirectPass context.directRule input.pending
  match findIndexedReferenceFlexCandidate directState with
  | some candidate =>
      { prepared := input
        postScan := directState
        output := some (input.buildOutput functions candidate) }
  | none =>
      let finished := runFullIndexedReferenceIndirect context.depth
        context.indirectRule directState
      { prepared := input
        postScan := finished
        output :=
          match findIndexedReferenceFlexCandidate finished with
          | some candidate => some (input.buildOutput functions candidate)
          | none => none }

@[simp]
theorem execute_reference_flex_commit_output
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId) :
    (executeReferenceFlexCommitWithContext functions context input).output =
      tryReferenceFlexCommitWithContext functions context input := by
  simp only [executeReferenceFlexCommitWithContext,
    tryReferenceFlexCommitWithContext, tryReferenceFlexCommit]
  cases foundDirect : findIndexedReferenceFlexCandidate
      (runReferenceDirectPass context.directRule input.pending) with
  | none => rfl
  | some candidate => rfl

/-- The execution record keeps the exact prepared input that the scan used. -/
@[simp]
theorem execute_reference_flex_commit_prepared
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId) :
    (executeReferenceFlexCommitWithContext functions context input).prepared =
      input := by
  simp only [executeReferenceFlexCommitWithContext]
  cases foundDirect : findIndexedReferenceFlexCandidate
      (runReferenceDirectPass context.directRule input.pending) with
  | none => rfl
  | some candidate => rfl

/-- A returned candidate names one round inside the scanned finite range. -/
theorem find_indexed_reference_candidate_has_round_index
    {Digest : Type}
    (rounds : Nat → ReferenceFlexRoundView Digest)
    (start fuel : Nat) {candidate : ReferenceFlexCandidate Digest}
    (found : findIndexedReferenceFlexCandidateFrom rounds start fuel =
      some candidate) :
    ∃ offset, offset < fuel ∧
      candidate.leaderRound = (rounds (start + offset)).round := by
  induction fuel generalizing start with
  | zero =>
      simp [findIndexedReferenceFlexCandidateFrom] at found
  | succ remaining inductionHypothesis =>
      simp only [findIndexedReferenceFlexCandidateFrom] at found
      cases final : finalReferenceDecisions? (rounds start).selectedSlots with
      | none => simp [final] at found
      | some decisions =>
          cases committed : orderedDecisionsHaveCommit decisions with
          | true =>
              simp [final, committed] at found
              subst candidate
              exact ⟨0, Nat.zero_lt_succ _, by simp⟩
          | false =>
              have tailFound :
                  findIndexedReferenceFlexCandidateFrom rounds (start + 1)
                    remaining = some candidate := by
                simpa [final, committed] using found
              rcases inductionHypothesis (start + 1) tailFound with
                ⟨offset, offsetInRange, roundShape⟩
              exact ⟨offset + 1, by omega, by
                simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
                  roundShape⟩

@[simp]
theorem run_reference_direct_pass_round
    {Digest : Type} (rule : ReferenceDirectRule Digest)
    (state : IndexedReferenceFlexState Digest) (index : Nat) :
    ((runReferenceDirectPass rule state).rounds index).round =
      (state.rounds index).round := by
  rfl

/-- One indirect update does not change any stored round number. -/
theorem indexed_reference_indirect_step_preserves_round
    {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (decisionIndex : Nat) (state : IndexedReferenceFlexState Digest)
    (index : Nat) :
    ((indexedReferenceIndirectStep depth rule decisionIndex state).rounds
      index).round =
      (state.rounds index).round := by
  simp only [indexedReferenceIndirectStep]
  cases found : findIndexedReferenceAnchorFrom state.rounds
      (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) with
  | blocked => rfl
  | noAnchor => rfl
  | found block =>
      by_cases same : index = decisionIndex
      · subst index
        simp [setIndexedReferenceRound]
      · simp [setIndexedReferenceRound, same]

@[simp]
theorem indexed_reference_indirect_step_preserves_round_count
    {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (decisionIndex : Nat) (state : IndexedReferenceFlexState Digest) :
    (indexedReferenceIndirectStep depth rule decisionIndex state).roundCount =
      state.roundCount := by
  simp only [indexedReferenceIndirectStep]
  cases found : findIndexedReferenceAnchorFrom state.rounds
      (decisionIndex + depth) (state.roundCount - (decisionIndex + depth)) <;>
    rfl

/-- A descending indirect pass does not change stored round numbers. -/
theorem run_indexed_reference_indirect_preserves_round
    {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (highestIndex count : Nat) (state : IndexedReferenceFlexState Digest)
    (index : Nat) :
    ((runIndexedReferenceIndirectDescending depth rule highestIndex count state).rounds
        index).round =
      (state.rounds index).round := by
  induction count with
  | zero => rfl
  | succ remaining inductionHypothesis =>
      simp only [runIndexedReferenceIndirectDescending]
      rw [indexed_reference_indirect_step_preserves_round,
        inductionHypothesis]

@[simp]
theorem run_indexed_reference_indirect_preserves_round_count
    {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (highestIndex count : Nat) (state : IndexedReferenceFlexState Digest) :
    (runIndexedReferenceIndirectDescending depth rule highestIndex count state).roundCount =
      state.roundCount := by
  induction count with
  | zero => rfl
  | succ remaining inductionHypothesis =>
      simp only [runIndexedReferenceIndirectDescending]
      rw [indexed_reference_indirect_step_preserves_round_count,
        inductionHypothesis]

/-- The full indirect pass does not change stored round numbers. -/
theorem run_full_indexed_reference_indirect_preserves_round
    {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest) (index : Nat) :
    ((runFullIndexedReferenceIndirect depth rule state).rounds index).round =
      (state.rounds index).round := by
  simp only [runFullIndexedReferenceIndirect]
  split
  · exact run_indexed_reference_indirect_preserves_round depth rule
      (state.roundCount - depth - 1) (state.roundCount - depth) state index
  · rfl

@[simp]
theorem run_full_indexed_reference_indirect_preserves_round_count
    {Digest History : Type}
    (depth : Nat) (rule : ReferenceIndirectRule Digest History)
    (state : IndexedReferenceFlexState Digest) :
    (runFullIndexedReferenceIndirect depth rule state).roundCount =
      state.roundCount := by
  simp only [runFullIndexedReferenceIndirect]
  split
  · exact run_indexed_reference_indirect_preserves_round_count depth rule
      (state.roundCount - depth - 1) (state.roundCount - depth) state
  · rfl

/-- The exact post-scan state keeps every prepared round number. -/
theorem execute_reference_flex_commit_preserves_round
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId) (index : Nat) :
    ((executeReferenceFlexCommitWithContext functions context input).postScan.rounds
        index).round =
      (input.pending.rounds index).round := by
  simp only [executeReferenceFlexCommitWithContext]
  cases foundDirect : findIndexedReferenceFlexCandidate
      (runReferenceDirectPass context.directRule input.pending) with
  | some candidate =>
      exact run_reference_direct_pass_round context.directRule input.pending index
  | none =>
      rw [run_full_indexed_reference_indirect_preserves_round,
        run_reference_direct_pass_round]

/-- The exact post-scan state keeps the prepared finite range length. -/
theorem execute_reference_flex_commit_preserves_round_count
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId) :
    (executeReferenceFlexCommitWithContext functions context input).postScan.roundCount =
      input.pending.roundCount := by
  simp only [executeReferenceFlexCommitWithContext]
  cases foundDirect : findIndexedReferenceFlexCandidate
      (runReferenceDirectPass context.directRule input.pending) with
  | some candidate => rfl
  | none =>
      exact run_full_indexed_reference_indirect_preserves_round_count
        context.depth context.indirectRule
        (runReferenceDirectPass context.directRule input.pending)

/-- A successful exact scan returns a candidate from its post-scan range. -/
theorem execute_reference_flex_commit_candidate_has_round_index
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding)
    (context : ReferenceFlexCommitterContext BlockId History)
    (input : ReferenceFlexTryCommitInput BlockId CommitId)
    {output : LocalFlexCommitOutput BlockId CommitId}
    (successful :
      (executeReferenceFlexCommitWithContext functions context input).output =
        some output) :
    ∃ index,
      index < input.pending.roundCount ∧
        output.candidate.leaderRound = (input.pending.rounds index).round := by
  simp only [executeReferenceFlexCommitWithContext] at successful
  cases foundDirect : findIndexedReferenceFlexCandidate
      (runReferenceDirectPass context.directRule input.pending) with
  | some candidate =>
      have outputShape : output = input.buildOutput functions candidate := by
        simpa [foundDirect] using successful.symm
      subst output
      rcases find_indexed_reference_candidate_has_round_index
          (runReferenceDirectPass context.directRule input.pending).rounds 0
          (runReferenceDirectPass context.directRule input.pending).roundCount
          foundDirect with ⟨index, indexInRange, roundShape⟩
      have roundShape' :
          candidate.leaderRound =
            ((runReferenceDirectPass context.directRule input.pending).rounds
              index).round := by
        simpa only [Nat.zero_add] using roundShape
      exact ⟨index, indexInRange,
        roundShape'.trans
          (run_reference_direct_pass_round context.directRule input.pending
            index)⟩
  | none =>
      let finished := runFullIndexedReferenceIndirect context.depth
        context.indirectRule
        (runReferenceDirectPass context.directRule input.pending)
      cases foundIndirect : findIndexedReferenceFlexCandidate finished with
      | none => simp [foundDirect, finished, foundIndirect] at successful
      | some candidate =>
          have outputShape : output = input.buildOutput functions candidate := by
            simpa [foundDirect, finished, foundIndirect] using successful.symm
          subst output
          rcases find_indexed_reference_candidate_has_round_index
              finished.rounds 0 finished.roundCount foundIndirect with
            ⟨index, indexInRange, roundShape⟩
          have countShape : finished.roundCount = input.pending.roundCount := by
            unfold finished
            rw [run_full_indexed_reference_indirect_preserves_round_count]
            rfl
          have directRoundShape := run_reference_direct_pass_round
            context.directRule input.pending index
          have indirectRoundShape :=
            run_full_indexed_reference_indirect_preserves_round context.depth
              context.indirectRule
              (runReferenceDirectPass context.directRule input.pending) index
          have roundShape' :
              candidate.leaderRound = (finished.rounds index).round := by
            simpa only [Nat.zero_add] using roundShape
          exact ⟨index, by simpa [countShape] using indexInRange, by
            simpa [finished, ReferenceFlexTryCommitInput.buildOutput,
              buildLocalFlexCommitOutput] using roundShape'.trans
              (indirectRoundShape.trans directRoundShape)⟩

/-- Pure schedule data used by the Rust pending-state refresh. -/
structure ValidatorFlexPendingSchedule
    (CommitId ScheduleKey : Type) where
  minimumLeaderRoundForHead : ValidatorCommitHead CommitId → Nat
  scheduleKeyForHead : ValidatorCommitHead CommitId → ScheduleKey
  minimumIsRoundSuccessor : ∀ head,
    minimumLeaderRoundForHead head = head.round + 1

namespace ValidatorFlexPendingCache

/-- Cached rounds are consecutive from the stored minimum. -/
def RoundsConsecutive {BlockId CommitId ScheduleKey : Type}
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey) : Prop :=
  (indexedReferenceStateFromList cache.baseline.index cache.rounds)
    |>.RoundsConsecutive cache.minimumLeaderRound

/-- No cached round is beyond the current local DAG frontier. -/
def RoundsBelow {BlockId CommitId ScheduleKey : Type}
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (highestAcceptedRound : Nat) : Prop :=
  ∀ round, round ∈ cache.rounds → round.round < highestAcceptedRound

end ValidatorFlexPendingCache

/-- Local validity of the cache that exists before one actual refresh. -/
structure ValidatorFlexPendingCacheValid
    {BlockId CommitId ScheduleKey : Type}
    (schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey)
    (head : ValidatorCommitHead CommitId)
    (highestAcceptedRound : Nat)
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey) : Prop where
  baselineIndexNotFuture : cache.baseline.index ≤ head.index
  sameIndexHasExactHead : cache.baseline.index = head.index →
    cache.baseline = head
  changedIndexAdvancesMinimum : cache.baseline.index < head.index →
    schedule.minimumLeaderRoundForHead cache.baseline <
      schedule.minimumLeaderRoundForHead head
  minimumMatchesBaseline : cache.minimumLeaderRound =
    schedule.minimumLeaderRoundForHead cache.baseline
  scheduleMatchesBaseline : cache.scheduleKey =
    schedule.scheduleKeyForHead cache.baseline
  roundsConsecutive : cache.RoundsConsecutive
  roundsBelowFrontier : cache.RoundsBelow highestAcceptedRound

/-- The refresh is valid for a scan from the current action-before state. -/
structure ValidatorFlexPreparedScanValid
    {BlockId CommitId History ScheduleKey : Type} [DecidableEq ScheduleKey]
    (schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey)
    (context : ReferenceFlexCommitterContext BlockId History)
    (head : ValidatorCommitHead CommitId)
    (highestAcceptedRound : Nat)
    (selectedSlots : Nat → List ExactSelectedLeaderSlot)
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (materialize : ReferenceFlexCandidate BlockId →
      ExactCommitBuildMaterial (LeaderBlockRef BlockId)) where
  cacheValid : ValidatorFlexPendingCacheValid schedule head
    highestAcceptedRound cache
  refreshedRoundsConsecutive :
    (indexedReferenceStateFromList head.index
      (refreshValidatorFlexPendingCache schedule.minimumLeaderRoundForHead
        schedule.scheduleKeyForHead head cache).rounds)
      |>.RoundsConsecutive (schedule.minimumLeaderRoundForHead head)
  refreshedRoundsBelowFrontier :
    (refreshValidatorFlexPendingCache schedule.minimumLeaderRoundForHead
      schedule.scheduleKeyForHead head cache).RoundsBelow highestAcceptedRound
  refreshedRoundsFitFrontier :
    schedule.minimumLeaderRoundForHead head +
      (refreshValidatorFlexPendingCache schedule.minimumLeaderRoundForHead
        schedule.scheduleKeyForHead head cache).rounds.length ≤
      highestAcceptedRound
  preparedStateValid :
    ReferenceFlexTryCommitStateValid context
      (prepareValidatorFlexScanInput schedule.minimumLeaderRoundForHead
        schedule.scheduleKeyForHead selectedSlots highestAcceptedRound head cache
        materialize)
  firstPendingIsCurrentMinimum :
    preparedStateValid.firstPendingRound =
      schedule.minimumLeaderRoundForHead head

@[simp]
theorem append_missing_validator_flex_rounds_length
    {BlockId : Type}
    (minimumLeaderRound highestAcceptedRound : Nat)
    (selectedSlots : Nat → List ExactSelectedLeaderSlot)
    (retained : List (ReferenceFlexRoundView BlockId))
    (fits : minimumLeaderRound + retained.length ≤ highestAcceptedRound) :
    (appendMissingValidatorFlexRounds minimumLeaderRound highestAcceptedRound
      selectedSlots retained).length =
        highestAcceptedRound - minimumLeaderRound := by
  simp only [appendMissingValidatorFlexRounds, List.length_append,
    List.length_map, List.length_range]
  omega

theorem prepared_validator_flex_scan_round_count
    {BlockId CommitId History ScheduleKey : Type} [DecidableEq ScheduleKey]
    (schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey)
    (context : ReferenceFlexCommitterContext BlockId History)
    (head : ValidatorCommitHead CommitId)
    (highestAcceptedRound : Nat)
    (selectedSlots : Nat → List ExactSelectedLeaderSlot)
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (materialize : ReferenceFlexCandidate BlockId →
      ExactCommitBuildMaterial (LeaderBlockRef BlockId))
    (valid : ValidatorFlexPreparedScanValid schedule context head
      highestAcceptedRound selectedSlots cache materialize) :
    (prepareValidatorFlexScanInput schedule.minimumLeaderRoundForHead
      schedule.scheduleKeyForHead selectedSlots highestAcceptedRound head cache
      materialize).pending.roundCount =
        highestAcceptedRound - schedule.minimumLeaderRoundForHead head := by
  simp only [prepareValidatorFlexScanInput, indexedReferenceStateFromList]
  exact append_missing_validator_flex_rounds_length
    (schedule.minimumLeaderRoundForHead head) highestAcceptedRound selectedSlots
    (refreshValidatorFlexPendingCache schedule.minimumLeaderRoundForHead
      schedule.scheduleKeyForHead head cache).rounds
    valid.refreshedRoundsFitFrontier

/-- The exact prefix state around one actual `runCommitter` action. -/
def ValidatorFlexRunActionPrefix
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (beforeEvents afterEvents :
      List (ValidatorAtomicEvent BlockId CommitId PacketId))
    (actionBefore actionAfter :
      ValidatorWorldState BlockId CommitId PacketId) : Prop :=
  timed.execution.events observation.time =
      beforeEvents ++
        (.localAction observation.validator .runCommitter :: afterEvents) ∧
    ValidatorWorldStep config faults protocolPacket program observation.time
      (timed.execution.trace observation.time) beforeEvents actionBefore ∧
    ValidatorAtomicStep config faults protocolPacket program observation.time
      actionBefore (.localAction observation.validator .runCommitter)
        actionAfter ∧
    ValidatorWorldStep config faults protocolPacket program observation.time
      actionAfter afterEvents (timed.execution.trace (observation.time + 1)) ∧
    observation.input = actionBefore.validatorState observation.validator

/-- One local action precedes an actual run, including earlier events in the
same logical-time batch. -/
def ValidatorActionBeforeFlexRun
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (actor : Nat) (action : ValidatorLocalAction BlockId CommitId) : Prop :=
  (∃ time, time < observation.time ∧
    ValidatorLocalActionOccurs (timed.execution.events time) actor action) ∨
  ∃ beforeEvents afterEvents actionBefore actionAfter,
    ValidatorFlexRunActionPrefix timed observation beforeEvents afterEvents
      actionBefore actionAfter ∧
      ValidatorLocalActionOccurs beforeEvents actor action

/-- One exact authenticated block delivery precedes an actual local run. -/
def ValidatorAuthenticatedBlockDeliveryBeforeFlexRun
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (block : ValidatorBlock BlockId) : Prop :=
  ∃ (packetId : PacketId)
      (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
    protocolPacket packet ∧
      packet.receiver = observation.validator ∧
      packet.payload = .block block ∧
      ((∃ deliveryTime,
          deliveryTime < observation.time ∧
            (timed.execution.trace deliveryTime).packets packetId = some packet ∧
            ValidatorPacketDeliveryOccurs
              (timed.execution.events deliveryTime) packetId) ∨
        ∃ beforeEvents afterEvents actionBefore actionAfter,
          ValidatorFlexRunActionPrefix timed observation beforeEvents afterEvents
              actionBefore actionAfter ∧
            actionBefore.packets packetId = some packet ∧
            ValidatorPacketDeliveryOccurs beforeEvents packetId)

/-- A new own-block value in one atomic event has one exact persistence event. -/
theorem validator_atomic_step_new_own_block_requires_persist
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
    (step : ValidatorAtomicStep config faults protocolPacket program time before
      event after)
    {validator round : Nat} {reference : ValidatorBlockRef BlockId}
    (absent : (before.validatorState validator).ownBlockAt round = none)
    (added : (after.validatorState validator).ownBlockAt round = some reference) :
    ∃ block,
      event = .localAction validator (.persistProposal block) ∧
        block.reference.round = round ∧ block.reference = reference := by
  cases event with
  | localAction actionValidator action =>
      by_cases sameValidator : actionValidator = validator
      · subst actionValidator
        have structural :=
          validator_atomic_local_action_has_structural_effect step
        rcases new_own_block_requires_persist_proposal structural absent added with
          ⟨block, actionShape, roundShape, referenceShape⟩
        subst action
        exact ⟨block, rfl, roundShape, referenceShape⟩
      · cases step with
        | localAction _ _ _ _ _ unchanged _ _ =>
            have different : validator ≠ actionValidator := by
              intro equality
              exact sameValidator equality.symm
            have sameState := unchanged validator different
            rw [sameState, absent] at added
            contradiction
  | deliverPacket packetId =>
      cases step with
      | @deliverPacket _ _ packet _ _ _ _ _ _ _ structural unchanged _ _ _ =>
          by_cases sameReceiver : validator = packet.receiver
          · subst validator
            rcases structural with
              ⟨_, _, _, _, _, _, _, ownUnchanged, _⟩
            rw [ownUnchanged, absent] at added
            contradiction
          · have sameState := unchanged validator sameReceiver
            rw [sameState, absent] at added
            contradiction
  | clockTick =>
      cases step
      simp_all [ValidatorWorldState.updateClocks]

private theorem validator_local_action_occurs_in_cons
    {BlockId CommitId PacketId : Type}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {validator : Nat} {action : ValidatorLocalAction BlockId CommitId}
    (occurs : ValidatorLocalActionOccurs events validator action) :
    ValidatorLocalActionOccurs (event :: events) validator action := by
  rcases occurs with ⟨beforeEvents, afterEvents, eventSplit⟩
  subst events
  exact ⟨event :: beforeEvents, afterEvents, rfl⟩

/-- A new own block in one event batch has one exact persistence action. -/
theorem validator_world_step_new_own_block_requires_persist
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {validator round : Nat} {reference : ValidatorBlockRef BlockId}
    (absent : (before.validatorState validator).ownBlockAt round = none)
    (added : (after.validatorState validator).ownBlockAt round = some reference) :
    ∃ block,
      ValidatorLocalActionOccurs events validator (.persistProposal block) ∧
        block.reference.round = round ∧ block.reference = reference := by
  induction step with
  | nil =>
      rw [absent] at added
      contradiction
  | @cons _ middle _ event remainingEvents firstStep remainingStep
      inductionHypothesis =>
      cases middleValue :
          (middle.validatorState validator).ownBlockAt round with
      | none =>
          rcases inductionHypothesis middleValue added with
            ⟨block, occurs, roundShape, referenceShape⟩
          exact ⟨block, validator_local_action_occurs_in_cons occurs,
            roundShape, referenceShape⟩
      | some middleReference =>
          have persisted := validator_world_step_own_block_persists
            remainingStep middleValue
          have sameReference : middleReference = reference := by
            exact Option.some.inj (persisted.symm.trans added)
          subst middleReference
          rcases validator_atomic_step_new_own_block_requires_persist firstStep
              absent middleValue with
            ⟨block, eventShape, roundShape, referenceShape⟩
          subst event
          exact ⟨block, ⟨[], remainingEvents, rfl⟩, roundShape,
            referenceShape⟩

/-- A complete logical-time batch cannot decrease one local commit round. -/
theorem validator_world_step_commit_round_monotone
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (validator : Nat) :
    (before.validatorState validator).commitHead.round ≤
      (after.validatorState validator).commitHead.round := by
  induction step with
  | nil => exact Nat.le_refl _
  | cons firstStep remainingSteps inductionHypothesis =>
      exact Nat.le_trans
        (validator_atomic_step_durable_monotone firstStep validator).2.1
        inductionHypothesis

/-- Durable signer state has a bounded initial origin or an earlier exact
proposal-persistence action. -/
theorem validator_trace_own_block_has_past_persist_origin
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time validator round : Nat} {reference : ValidatorBlockRef BlockId}
    (owned : ((timed.execution.trace time).validatorState validator).ownBlockAt
      round = some reference) :
    ((timed.execution.trace 0).validatorState validator).ownBlockAt round =
        some reference ∨
      ∃ persistTime block,
        persistTime < time ∧
          ValidatorLocalActionOccurs (timed.execution.events persistTime)
            validator (.persistProposal block) ∧
          block.reference.round = round ∧ block.reference = reference := by
  induction time with
  | zero => exact Or.inl owned
  | succ previous inductionHypothesis =>
      cases previousValue :
          ((timed.execution.trace previous).validatorState validator).ownBlockAt
            round with
      | some previousReference =>
          have persisted := validator_world_step_own_block_persists
            (timed.execution.stepsFollowRules previous) previousValue
          have sameReference : previousReference = reference := by
            exact Option.some.inj (persisted.symm.trans owned)
          subst previousReference
          rcases inductionHypothesis previousValue with initialOrigin | laterOrigin
          · exact Or.inl initialOrigin
          · rcases laterOrigin with
              ⟨persistTime, block, beforePrevious, occurs, roundShape,
                referenceShape⟩
            exact Or.inr ⟨persistTime, block, Nat.lt_trans beforePrevious
              (Nat.lt_succ_self previous), occurs, roundShape, referenceShape⟩
      | none =>
          rcases validator_world_step_new_own_block_requires_persist
              (timed.execution.stepsFollowRules previous) previousValue owned with
            ⟨block, occurs, roundShape, referenceShape⟩
          exact Or.inr ⟨previous, block, Nat.lt_succ_self previous, occurs,
            roundShape, referenceShape⟩

/-- Finite accepted DAG data restored before the first local call. -/
structure ValidatorFlexInitialDagSupport
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  references : Nat → List (ValidatorBlockRef BlockId)
  ownReferences : Nat → List (ValidatorBlockRef BlockId)
  roundBound : Nat → Nat
  initialCacheEmpty : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((timed.execution.trace 0).validatorState validator).committer.pendingRounds =
      []
  acceptedMembership : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∀ reference,
      reference ∈ references validator ↔
        ((timed.execution.trace 0).validatorState validator).accepted reference =
          true
  referenceHasBody : ∀ validator reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    reference ∈ references validator →
    ∃ block,
      (timed.execution.trace 0).blockCatalog reference.id = some block ∧
        block.reference = reference
  referenceWithinBound : ∀ validator reference,
    reference ∈ references validator →
    reference.round ≤ roundBound validator
  ownMembership : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∀ reference,
      reference ∈ ownReferences validator ↔
        ((timed.execution.trace 0).validatorState validator).ownBlockAt
            reference.round = some reference
  ownReferenceWithinBound : ∀ validator reference,
    reference ∈ ownReferences validator →
    reference.round ≤ roundBound validator

/-- Accepted-body provenance for one exact action-before scan. -/
inductive ValidatorFlexAcceptedBodyOrigin
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (initial : ValidatorFlexInitialDagSupport timed)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId) :
    ValidatorBlockRef BlockId → Prop where
  | restored {reference} :
      reference ∈ initial.references observation.validator →
      ValidatorFlexAcceptedBodyOrigin initial observation reference
  | persisted {block} :
      ValidatorActionBeforeFlexRun timed observation observation.validator
        (.persistProposal block) →
      ValidatorFlexAcceptedBodyOrigin initial observation block.reference
  | accepted {block} :
      ValidatorActionBeforeFlexRun timed observation observation.validator
        (.acceptBlock block) →
      ValidatorFlexAcceptedBodyOrigin initial observation block.reference
  | delivered {block} :
      ValidatorAuthenticatedBlockDeliveryBeforeFlexRun timed observation block →
      ValidatorFlexAcceptedBodyOrigin initial observation block.reference

/-- A correct author's accepted reference has a durable author-side origin. -/
def ValidatorFlexCorrectAuthorOrigin
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (initial : ValidatorFlexInitialDagSupport timed)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (reference : ValidatorBlockRef BlockId) : Prop :=
  reference ∈ initial.ownReferences reference.author ∧
      ((timed.execution.trace 0).validatorState reference.author).ownBlockAt
        reference.round = some reference ∨
    ∃ block,
      block.reference = reference ∧
        ValidatorActionBeforeFlexRun timed observation reference.author
          (.persistProposal block)

/-- Current durable signer ownership gives a bounded initial origin or one
exact earlier persistence action. -/
theorem action_before_flex_run_own_block_has_past_origin
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (initial : ValidatorFlexInitialDagSupport timed)
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    {beforeEvents afterEvents :
      List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {actionBefore actionAfter :
      ValidatorWorldState BlockId CommitId PacketId}
    (runPrefix : ValidatorFlexRunActionPrefix timed observation beforeEvents
      afterEvents actionBefore actionAfter)
    {reference : ValidatorBlockRef BlockId}
    (authorInRange : reference.author < config.authorityCount)
    (authorCorrect : faults.correctAvailable reference.author = true)
    (owned : (actionBefore.validatorState reference.author).ownBlockAt
      reference.round = some reference) :
    ValidatorFlexCorrectAuthorOrigin timed initial observation reference := by
  rcases runPrefix with
    ⟨eventSplit, prefixStep, actionStep, suffixStep, inputIsActionBefore⟩
  cases traceValue :
      ((timed.execution.trace observation.time).validatorState
        reference.author).ownBlockAt reference.round with
  | none =>
      rcases validator_world_step_new_own_block_requires_persist prefixStep
          traceValue owned with
        ⟨block, occurs, roundShape, referenceShape⟩
      exact Or.inr ⟨block, referenceShape, Or.inr
        ⟨beforeEvents, afterEvents, actionBefore, actionAfter,
          ⟨eventSplit, prefixStep, actionStep, suffixStep, inputIsActionBefore⟩,
          occurs⟩⟩
  | some traceReference =>
      have persisted := validator_world_step_own_block_persists prefixStep
        traceValue
      have sameReference : traceReference = reference := by
        exact Option.some.inj (persisted.symm.trans owned)
      subst traceReference
      rcases validator_trace_own_block_has_past_persist_origin timed traceValue
          with initialOrigin | persistedOrigin
      · have member := (initial.ownMembership reference.author authorInRange
          authorCorrect reference).2 initialOrigin
        exact Or.inl ⟨member, initialOrigin⟩
      · rcases persistedOrigin with
          ⟨persistTime, block, beforeRun, occurs, roundShape, referenceShape⟩
        exact Or.inr ⟨block, referenceShape, Or.inl
          ⟨persistTime, beforeRun, occurs⟩⟩

/-- The exact post-refresh input for one actual local run observation. -/
def validatorFlexPreparedInputAt
    {BlockId CommitId History Encoding ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (source : LocalFlexCommitterSourceMap config functions context program)
    (schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey)
    (cacheAt : Nat → ValidatorLocalState BlockId CommitId →
      ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (highestAcceptedRound : Nat → ValidatorLocalState BlockId CommitId → Nat)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId) :
    ReferenceFlexTryCommitInput BlockId CommitId :=
  prepareValidatorFlexScanInput schedule.minimumLeaderRoundForHead
    schedule.scheduleKeyForHead
    (source.selectedLeaderSlots observation.validator observation.input)
    (highestAcceptedRound observation.validator observation.input)
    observation.input.commitHead
    (cacheAt observation.validator observation.input)
    (source.snapshot observation.validator observation.input).materialize

/-- The full exact scan state for one actual local run observation. -/
def validatorFlexRunStateAt
    {BlockId CommitId History Encoding ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (source : LocalFlexCommitterSourceMap config functions context program)
    (schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey)
    (cacheAt : Nat → ValidatorLocalState BlockId CommitId →
      ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (highestAcceptedRound : Nat → ValidatorLocalState BlockId CommitId → Nat)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId) :
    ReferenceFlexRunState BlockId CommitId :=
  executeReferenceFlexCommitWithContext functions
    (context observation.validator observation.input)
    (validatorFlexPreparedInputAt source schedule cacheAt highestAcceptedRound
      observation)

/-- Exact finite DAG support read by one actual local scan. -/
structure ValidatorFlexRunDagSupport
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (initial : ValidatorFlexInitialDagSupport timed)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (highestAcceptedRound : Nat) where
  references : List (ValidatorBlockRef BlockId)
  acceptedMembership : ∀ reference,
    reference ∈ references ↔ observation.input.accepted reference = true
  referencesAtOrBelowFrontier : ∀ reference,
    reference ∈ references → reference.round ≤ highestAcceptedRound
  frontierIsZeroOrAttained : highestAcceptedRound = 0 ∨
    ∃ reference, reference ∈ references ∧
      reference.round = highestAcceptedRound
  acceptedBodyHasPastOrigin : ∀ reference,
    reference ∈ references →
    ValidatorFlexAcceptedBodyOrigin initial observation reference
  body : ValidatorBlockRef BlockId → ValidatorBlock BlockId
  bodyHasExactReference : ∀ reference,
    reference ∈ references → (body reference).reference = reference
  bodyIsCataloguedBeforeRun : ∀ reference beforeEvents afterEvents actionBefore
      actionAfter,
    reference ∈ references →
    ValidatorFlexRunActionPrefix timed observation beforeEvents afterEvents
      actionBefore actionAfter →
    actionBefore.blockCatalog reference.id = some (body reference)

/-- The literal post-refresh input read by one actual local Flex run.

The base observation fixes the main-trace action, its action-before local
state, and its host. The additional field is the input that Rust passes to the
scan after it refreshes the pending rounds. -/
structure LocalFlexCommitterPostRefreshRunObservation
    (BlockId CommitId : Type)
    (base : LocalFlexCommitterRunObservation BlockId CommitId) where
  internalInput : ReferenceFlexTryCommitInput BlockId CommitId

namespace LocalFlexCommitterPostRefreshRunObservation

/-- Forget the internal input and expose the result that the literal input
computes through the existing runtime observation interface. -/
def toRunObservation
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {base : LocalFlexCommitterRunObservation BlockId CommitId}
    (observation : LocalFlexCommitterPostRefreshRunObservation BlockId CommitId
      base) :
    LocalFlexCommitterRunObservation BlockId CommitId :=
  { time := base.time
    validator := base.validator
    input := base.input
    result := tryReferenceFlexCommitWithContext functions
      (context base.validator base.input) observation.internalInput }

end LocalFlexCommitterPostRefreshRunObservation

/-- One-host source mapping for Rust pending refresh and exact scan effects.

All forward fields are conditional on an actual main-trace `runCommitter`
action. They map the action-before cache and accepted DAG to the internal scan,
then map the post-scan cache to the same atomic action's result state. -/
structure ValidatorFlexPendingRefreshSourceMap
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (initial : ValidatorFlexInitialDagSupport timed)
    (schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey) where
  cacheAt : Nat → ValidatorLocalState BlockId CommitId →
    ValidatorFlexPendingCache BlockId CommitId ScheduleKey
  highestAcceptedRound : Nat → ValidatorLocalState BlockId CommitId → Nat
  /-- The actual run observation carries the literal input read after the
  internal refresh. -/
  actualRunObservation : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    observation.OccursIn timed →
    LocalFlexCommitterPostRefreshRunObservation BlockId CommitId observation
  /-- The literal post-refresh observation is the result returned by this same
  main-trace action. -/
  actualRunObservationIsReturned : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    (occurs : observation.OccursIn timed) →
    runtime.returned
      ((actualRunObservation observation occurs).toRunObservation
        (functions := functions) (context := context))
  /-- The exact cache projects to the status-only field in the main model. -/
  cacheProjectsMainState : ∀ validator state,
    (cacheAt validator state).rounds.map referenceRoundPendingProjection =
      state.committer.pendingRounds
  /-- A same-index cache names the exact current local head. -/
  actualRunCacheIsValid : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    observation.OccursIn timed →
    ValidatorFlexPendingCacheValid schedule observation.input.commitHead
      (highestAcceptedRound observation.validator observation.input)
      (cacheAt observation.validator observation.input)
  /-- Every prepared input has the exact consecutive finite interval shape. -/
  actualRunPreparedScanIsValid : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    observation.OccursIn timed →
    ValidatorFlexPreparedScanValid schedule
      (context observation.validator observation.input)
      observation.input.commitHead
      (highestAcceptedRound observation.validator observation.input)
      (source.selectedLeaderSlots observation.validator observation.input)
      (cacheAt observation.validator observation.input)
      (source.snapshot observation.validator observation.input).materialize
  /-- Slot selection for appended rounds uses the current configured order. -/
  selectedSlotsMatchCurrentConfig : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId) round,
    observation.OccursIn timed →
    source.selectedLeaderSlots observation.validator observation.input round =
      (config.selectedLeaderOrder observation.input.commitHead.id round).map
        (fun selectedValidator =>
          { round
            validator := selectedValidator })
  /-- Retained and appended rounds both use the current ordered schedule. -/
  preparedSlotsMatchCurrentConfig : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    observation.OccursIn timed →
    ∀ index,
      index < (validatorFlexPreparedInputAt source schedule cacheAt
        highestAcceptedRound observation).pending.roundCount →
    let round := (validatorFlexPreparedInputAt source schedule cacheAt
      highestAcceptedRound observation).pending.rounds index
    round.selectedSlots.map ReferenceSelectedSlotView.slot =
      (config.selectedLeaderOrder observation.input.commitHead.id
        round.round).map fun selectedValidator =>
          { round := round.round
            validator := selectedValidator }
  /-- The finite support is the exact accepted DAG before this action. -/
  dagSupport : ∀ observation,
    observation.OccursIn timed →
    ValidatorFlexRunDagSupport initial observation
      (highestAcceptedRound observation.validator observation.input)
  /-- A committed status in the prepared cache names accepted local support. -/
  preparedCommitStatusHasSupport : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    (occurs : observation.OccursIn timed) →
    ∀ index,
      index < (validatorFlexPreparedInputAt source schedule cacheAt
        highestAcceptedRound observation).pending.roundCount →
    ∀ slot : ReferenceSelectedSlotView BlockId,
      slot ∈ ((validatorFlexPreparedInputAt source schedule cacheAt
        highestAcceptedRound observation).pending.rounds index).selectedSlots →
    ∀ block,
      slot.status = .commit block →
      referenceLeaderBlockToValidatorBlockRef block ∈
        (dagSupport observation occurs).references
  /-- A committed status created by this scan also names accepted support. -/
  postScanCommitStatusHasSupport : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    (occurs : observation.OccursIn timed) →
    ∀ index,
      index < (validatorFlexRunStateAt source schedule cacheAt
        highestAcceptedRound observation).postScan.roundCount →
    ∀ slot : ReferenceSelectedSlotView BlockId,
      slot ∈ ((validatorFlexRunStateAt source schedule cacheAt
        highestAcceptedRound observation).postScan.rounds index).selectedSlots →
    ∀ block,
      slot.status = .commit block →
      referenceLeaderBlockToValidatorBlockRef block ∈
        (dagSupport observation occurs).references
  /-- The internal input is exactly the deterministic refreshed input. -/
  actualRunInternalInputIsPrepared : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    (occurs : observation.OccursIn timed) →
    (actualRunObservation observation occurs).internalInput =
      validatorFlexPreparedInputAt source schedule cacheAt
        highestAcceptedRound observation
  /-- The same atomic run stores the exact post-scan pending cache. -/
  actualRunStoresPostScanCache : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId)
      (beforeEvents afterEvents :
        List (ValidatorAtomicEvent BlockId CommitId PacketId))
      (actionBefore actionAfter :
        ValidatorWorldState BlockId CommitId PacketId),
    ValidatorFlexRunActionPrefix timed observation beforeEvents afterEvents
      actionBefore actionAfter →
    cacheAt observation.validator
        (actionAfter.validatorState observation.validator) =
      { baseline := observation.input.commitHead
        minimumLeaderRound := schedule.minimumLeaderRoundForHead
          observation.input.commitHead
        scheduleKey := schedule.scheduleKeyForHead observation.input.commitHead
        rounds := (validatorFlexRunStateAt source schedule cacheAt
          highestAcceptedRound observation).postScan.toRoundList }
  /-- Process construction and restart start with an exact empty cache. -/
  initialCacheIsEmpty : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    cacheAt validator ((timed.execution.trace 0).validatorState validator) =
      { baseline :=
          ((timed.execution.trace 0).validatorState validator).commitHead
        minimumLeaderRound := schedule.minimumLeaderRoundForHead
          ((timed.execution.trace 0).validatorState validator).commitHead
        scheduleKey := schedule.scheduleKeyForHead
          ((timed.execution.trace 0).validatorState validator).commitHead
        rounds := [] }
  /-- No other atomic event changes this internal cache. A `recordCommit`
  advances the head; the following actual run performs the prefix drop. -/
  nonRunAtomicStepPreservesCache : ∀ time
      (before : ValidatorWorldState BlockId CommitId PacketId)
      (event : ValidatorAtomicEvent BlockId CommitId PacketId)
      (after : ValidatorWorldState BlockId CommitId PacketId) validator,
    ValidatorAtomicStep config faults protocolPacket program time before event
      after →
    event ≠ .localAction validator .runCommitter →
    cacheAt validator (before.validatorState validator) =
      cacheAt validator (after.validatorState validator)

/-- Cryptographic block authentication at the exact action-before world.

For a correct signer, one verified accepted body identifies the signer's exact
durable own block. Past persistence is derived from this current-state fact and
the main transition relation. -/
structure ValidatorFlexAuthenticatedBodySourceMap
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule) where
  authenticated : ValidatorBlock BlockId → Prop
  supportBodyIsAuthenticated : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    (occurs : observation.OccursIn timed) →
    ∀ reference,
      reference ∈ (mapping.dagSupport observation occurs).references →
      authenticated ((mapping.dagSupport observation occurs).body reference)
  correctAuthenticatedBodyIsOwnedAtRun : ∀
      (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    (occurs : observation.OccursIn timed) →
    ∀ (beforeEvents afterEvents :
        List (ValidatorAtomicEvent BlockId CommitId PacketId))
      (actionBefore actionAfter :
        ValidatorWorldState BlockId CommitId PacketId),
      ValidatorFlexRunActionPrefix timed observation beforeEvents afterEvents
        actionBefore actionAfter →
    ∀ reference,
      reference ∈ (mapping.dagSupport observation occurs).references →
      reference.author < config.authorityCount →
      faults.correctAvailable reference.author = true →
      authenticated ((mapping.dagSupport observation occurs).body reference) →
      ((mapping.dagSupport observation occurs).body reference).reference =
        reference →
      observation.input.accepted reference = true →
      actionBefore.blockCatalog reference.id =
        some ((mapping.dagSupport observation occurs).body reference) →
      (actionBefore.validatorState reference.author).ownBlockAt reference.round =
        some reference

namespace ValidatorFlexPendingRefreshSourceMap

variable {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
variable [DecidableEq ScheduleKey]
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions CommitId
  (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {faults : FixedFaultInterval config}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}
variable {initial : ValidatorFlexInitialDagSupport timed}
variable {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}

/-- The old runtime result and the literal post-refresh result are equal
because they are two views of the same returned local action. -/
theorem actualRunResultReconstructsFromInternalInput
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (occurs : observation.OccursIn timed) :
    tryReferenceFlexCommitWithContext functions
        (context observation.validator observation.input)
        (source.snapshot observation.validator observation.input) =
      tryReferenceFlexCommitWithContext functions
        (context observation.validator observation.input)
        (mapping.actualRunObservation observation occurs).internalInput := by
  have returned := mapping.actualRunObservationIsReturned observation occurs
  have exactResult := runtime.everyReturnedResultIsExact
    ((mapping.actualRunObservation observation occurs).toRunObservation
      (functions := functions) (context := context)) returned
  exact exactResult.symm

/-- The finite rank of one actual prepared input. -/
def preparedRank
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId) : Nat :=
  mapping.highestAcceptedRound observation.validator observation.input -
    schedule.minimumLeaderRoundForHead observation.input.commitHead

/-- One actual run has exactly its finite frontier-minus-minimum rank. -/
theorem actual_run_has_finite_prepared_rank
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    (occurs : observation.OccursIn timed) :
    (validatorFlexPreparedInputAt source schedule mapping.cacheAt
      mapping.highestAcceptedRound observation).pending.roundCount =
        mapping.preparedRank observation := by
  exact prepared_validator_flex_scan_round_count schedule
    (context observation.validator observation.input)
    observation.input.commitHead
    (mapping.highestAcceptedRound observation.validator observation.input)
    (source.selectedLeaderSlots observation.validator observation.input)
    (mapping.cacheAt observation.validator observation.input)
    (source.snapshot observation.validator observation.input).materialize
    (mapping.actualRunPreparedScanIsValid observation occurs)

/-- A successful actual scan processes one round inside the finite prepared
interval. The lower endpoint is the current minimum leader round, and the
upper endpoint is the exact accepted-DAG frontier. -/
theorem actual_successful_candidate_is_inside_prepared_frontier
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (occurs : observation.OccursIn timed)
    (successful :
      (validatorFlexRunStateAt source schedule mapping.cacheAt
        mapping.highestAcceptedRound observation).output = some output) :
    schedule.minimumLeaderRoundForHead observation.input.commitHead ≤
        output.candidate.leaderRound ∧
      output.candidate.leaderRound <
        mapping.highestAcceptedRound observation.validator observation.input := by
  let prepared := validatorFlexPreparedInputAt source schedule mapping.cacheAt
    mapping.highestAcceptedRound observation
  have runStateShape :
      validatorFlexRunStateAt source schedule mapping.cacheAt
          mapping.highestAcceptedRound observation =
        executeReferenceFlexCommitWithContext functions
          (context observation.validator observation.input) prepared := rfl
  rw [runStateShape] at successful
  rcases execute_reference_flex_commit_candidate_has_round_index functions
      (context observation.validator observation.input) prepared successful with
    ⟨index, indexInRange, candidateRound⟩
  let valid := mapping.actualRunPreparedScanIsValid observation occurs
  have consecutive := valid.preparedStateValid.roundsConsecutive index
    indexInRange
  rw [valid.firstPendingIsCurrentMinimum] at consecutive
  have candidateShape : output.candidate.leaderRound =
      schedule.minimumLeaderRoundForHead observation.input.commitHead + index := by
    exact candidateRound.trans consecutive
  have countShape := actual_run_has_finite_prepared_rank mapping occurs
  have countShape' : prepared.pending.roundCount =
      mapping.highestAcceptedRound observation.validator observation.input -
        schedule.minimumLeaderRoundForHead observation.input.commitHead := by
    simpa only [prepared, ValidatorFlexPendingRefreshSourceMap.preparedRank]
      using countShape
  have minimumFits :
      schedule.minimumLeaderRoundForHead observation.input.commitHead ≤
        mapping.highestAcceptedRound observation.validator observation.input := by
    have := valid.refreshedRoundsFitFrontier
    omega
  constructor <;> omega

/-- An actual runtime result is the exact result of the refreshed scan. -/
theorem returned_observation_uses_prepared_scan
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    (returned : runtime.returned observation) :
    observation.result =
      (validatorFlexRunStateAt source schedule mapping.cacheAt
        mapping.highestAcceptedRound observation).output := by
  have occurs := runtime.everyReturnedObservationOccurs observation returned
  rw [runtime.everyReturnedResultIsExact observation returned]
  rw [mapping.actualRunResultReconstructsFromInternalInput observation occurs,
    mapping.actualRunInternalInputIsPrepared observation occurs]
  exact (execute_reference_flex_commit_output functions
    (context observation.validator observation.input)
    (validatorFlexPreparedInputAt source schedule mapping.cacheAt
      mapping.highestAcceptedRound observation)).symm

/-- Authentication and current signer ownership derive exact past production.
The result has only a finite time-zero exception or a prior main-trace
`persistProposal` action. -/
theorem correct_support_body_has_past_author_origin
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (authenticated : ValidatorFlexAuthenticatedBodySourceMap mapping)
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    (occurs : observation.OccursIn timed)
    {beforeEvents afterEvents :
      List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {actionBefore actionAfter :
      ValidatorWorldState BlockId CommitId PacketId}
    (runPrefix : ValidatorFlexRunActionPrefix timed observation beforeEvents
      afterEvents actionBefore actionAfter)
    {reference : ValidatorBlockRef BlockId}
    (member : reference ∈ (mapping.dagSupport observation occurs).references)
    (authorInRange : reference.author < config.authorityCount)
    (authorCorrect : faults.correctAvailable reference.author = true) :
    ValidatorFlexCorrectAuthorOrigin timed initial observation reference := by
  let support := mapping.dagSupport observation occurs
  have bodyAuthenticated := authenticated.supportBodyIsAuthenticated
    observation occurs reference member
  have exactReference := support.bodyHasExactReference reference member
  have accepted := (support.acceptedMembership reference).1 member
  have catalog := support.bodyIsCataloguedBeforeRun reference beforeEvents
    afterEvents actionBefore actionAfter member runPrefix
  have owned := authenticated.correctAuthenticatedBodyIsOwnedAtRun observation
    occurs beforeEvents afterEvents actionBefore actionAfter runPrefix reference
    member authorInRange authorCorrect bodyAuthenticated exactReference accepted
    catalog
  exact action_before_flex_run_own_block_has_past_origin initial runPrefix
    authorInRange authorCorrect owned

/-- A later actual run cannot scan the frontier already installed by one
matching local record action. This theorem is conditional on that later run;
it does not create one. -/
theorem later_actual_run_drops_recorded_frontier
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {earlier later : LocalFlexCommitterRunObservation BlockId CommitId}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (_earlierOccurs : earlier.OccursIn timed)
    (successful :
      (validatorFlexRunStateAt source schedule mapping.cacheAt
        mapping.highestAcceptedRound earlier).output = some output)
    (recordBeforeLater : ValidatorActionBeforeFlexRun timed later
      earlier.validator (.recordCommit output.toCommitHead))
    (sameValidator : later.validator = earlier.validator)
    (validatorInRange : earlier.validator < config.authorityCount)
    (laterOccurs : later.OccursIn timed)
    {beforeEvents afterEvents :
      List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {actionBefore actionAfter :
      ValidatorWorldState BlockId CommitId PacketId}
    (laterPrefix : ValidatorFlexRunActionPrefix timed later beforeEvents
      afterEvents actionBefore actionAfter) :
    (validatorFlexRunStateAt source schedule mapping.cacheAt
        mapping.highestAcceptedRound earlier).output = some output ∧
      ∀ index,
        index < (validatorFlexPreparedInputAt source schedule mapping.cacheAt
          mapping.highestAcceptedRound later).pending.roundCount →
        output.candidate.leaderRound <
          ((validatorFlexPreparedInputAt source schedule mapping.cacheAt
            mapping.highestAcceptedRound later).pending.rounds index).round := by
  have candidateAtOrBeforeLaterHead :
      output.candidate.leaderRound ≤ later.input.commitHead.round := by
    rcases recordBeforeLater with
      ⟨recordTime, recordTimeBeforeLater, recordOccurs⟩ |
        ⟨recordBeforeEvents, recordAfterEvents, recordActionBefore,
          recordActionAfter, recordRunPrefix, recordOccurs⟩
    · rcases validator_world_step_local_action_with_suffix
          (timed.execution.stepsFollowRules recordTime) recordOccurs with
        ⟨recordBefore, recordAfter, suffix, recordStep, suffixStep⟩
      have structural :=
        validator_atomic_local_action_has_structural_effect recordStep
      have installEffect := structural.2.2.2.2.2
      simp only [CommitInstallActionEffect] at installEffect
      have exactHead := installEffect.1
      have throughRecordBatch := validator_world_step_commit_round_monotone
        suffixStep earlier.validator
      rw [exactHead] at throughRecordBatch
      have recordVisibleBeforeLater : recordTime + 1 ≤ later.time := by
        change Nat.succ recordTime ≤ later.time at recordTimeBeforeLater
        simpa [Nat.succ_eq_add_one] using recordTimeBeforeLater
      have throughLaterTrace := (timed.execution.durableStateMonotone
        earlier.validator (recordTime + 1) later.time validatorInRange
        recordVisibleBeforeLater).2.1
      rcases laterPrefix with
        ⟨eventSplit, prefixStep, actionStep, suffixAfter, inputIsActionBefore⟩
      have throughLaterPrefix := validator_world_step_commit_round_monotone
        prefixStep earlier.validator
      have inputIsActionBeforeForValidator : later.input =
          actionBefore.validatorState earlier.validator := by
        simpa [sameValidator] using inputIsActionBefore
      rw [← inputIsActionBeforeForValidator] at throughLaterPrefix
      exact Nat.le_trans (by
        simpa [LocalFlexCommitOutput.toCommitHead] using throughRecordBatch)
        (Nat.le_trans throughLaterTrace throughLaterPrefix)
    · rcases recordRunPrefix with
        ⟨eventSplit, prefixStep, actionStep, suffixAfter,
          inputIsActionBefore⟩
      rcases validator_world_step_local_action_with_suffix prefixStep
          recordOccurs with
        ⟨recordBefore, recordAfter, suffix, recordStep, suffixStep⟩
      have structural :=
        validator_atomic_local_action_has_structural_effect recordStep
      have installEffect := structural.2.2.2.2.2
      simp only [CommitInstallActionEffect] at installEffect
      have exactHead := installEffect.1
      have throughLaterPrefix := validator_world_step_commit_round_monotone
        suffixStep earlier.validator
      rw [exactHead] at throughLaterPrefix
      have inputIsActionBeforeForValidator : later.input =
          recordActionBefore.validatorState earlier.validator := by
        simpa [sameValidator] using inputIsActionBefore
      rw [← inputIsActionBeforeForValidator] at throughLaterPrefix
      simpa [LocalFlexCommitOutput.toCommitHead] using throughLaterPrefix
  constructor
  · exact successful
  · intro index inRange
    let valid := mapping.actualRunPreparedScanIsValid later laterOccurs
    have roundShape := valid.preparedStateValid.roundsConsecutive index inRange
    rw [valid.firstPendingIsCurrentMinimum] at roundShape
    have exactRoundShape :
        ((validatorFlexPreparedInputAt source schedule mapping.cacheAt
          mapping.highestAcceptedRound later).pending.rounds index).round =
          schedule.minimumLeaderRoundForHead later.input.commitHead + index := by
      simpa only [validatorFlexPreparedInputAt] using roundShape
    rw [schedule.minimumIsRoundSuccessor] at exactRoundShape
    omega

/-- Under one fixed accepted-DAG frontier, every already-observed successful
run and matching local record makes an already-observed later run use a
strictly smaller finite prepared rank. The theorem is conditional on both
runs and the record action. It does not create a later action. -/
theorem fixed_frontier_recorded_success_decreases_prepared_rank
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {earlier later : LocalFlexCommitterRunObservation BlockId CommitId}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (earlierOccurs : earlier.OccursIn timed)
    (successful :
      (validatorFlexRunStateAt source schedule mapping.cacheAt
        mapping.highestAcceptedRound earlier).output = some output)
    (recordBeforeLater : ValidatorActionBeforeFlexRun timed later
      earlier.validator (.recordCommit output.toCommitHead))
    (sameValidator : later.validator = earlier.validator)
    (validatorInRange : earlier.validator < config.authorityCount)
    (laterOccurs : later.OccursIn timed)
    {beforeEvents afterEvents :
      List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {actionBefore actionAfter :
      ValidatorWorldState BlockId CommitId PacketId}
    (laterPrefix : ValidatorFlexRunActionPrefix timed later beforeEvents
      afterEvents actionBefore actionAfter)
    (frontierUnchanged :
      mapping.highestAcceptedRound later.validator later.input =
        mapping.highestAcceptedRound earlier.validator earlier.input) :
    mapping.preparedRank later < mapping.preparedRank earlier := by
  have earlierInterval :=
    actual_successful_candidate_is_inside_prepared_frontier mapping
      earlierOccurs successful
  have dropped := later_actual_run_drops_recorded_frontier mapping
    earlierOccurs successful recordBeforeLater sameValidator
    validatorInRange laterOccurs laterPrefix
  have laterCount := actual_run_has_finite_prepared_rank mapping laterOccurs
  by_cases laterRankIsZero : mapping.preparedRank later = 0
  · unfold preparedRank
    unfold preparedRank at laterRankIsZero
    omega
  · have laterRankPositive : 0 < mapping.preparedRank later :=
      Nat.pos_of_ne_zero laterRankIsZero
    have zeroInRange :
        0 < (validatorFlexPreparedInputAt source schedule mapping.cacheAt
          mapping.highestAcceptedRound later).pending.roundCount := by
      rw [laterCount]
      exact laterRankPositive
    have candidateBeforeLaterFirst := dropped.2 0 zeroInRange
    let laterValid := mapping.actualRunPreparedScanIsValid later laterOccurs
    have laterFirstRound :=
      laterValid.preparedStateValid.roundsConsecutive 0 zeroInRange
    rw [laterValid.firstPendingIsCurrentMinimum] at laterFirstRound
    have laterFirstRound' :
        ((validatorFlexPreparedInputAt source schedule mapping.cacheAt
          mapping.highestAcceptedRound later).pending.rounds 0).round =
            schedule.minimumLeaderRoundForHead later.input.commitHead + 0 := by
      simpa only [validatorFlexPreparedInputAt] using laterFirstRound
    have candidateBeforeLaterMinimum :
        output.candidate.leaderRound <
          schedule.minimumLeaderRoundForHead later.input.commitHead := by
      rw [laterFirstRound'] at candidateBeforeLaterFirst
      simpa using candidateBeforeLaterFirst
    unfold preparedRank
    unfold preparedRank at laterRankPositive
    omega

/-- One already-observed successful local commit step under a fixed accepted
DAG frontier. Every witness is a main-trace run or record occurrence. -/
structure ValidatorFlexFixedFrontierRecordedSuccessor
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (later earlier : LocalFlexCommitterRunObservation BlockId CommitId) where
  output : LocalFlexCommitOutput BlockId CommitId
  beforeEvents : List (ValidatorAtomicEvent BlockId CommitId PacketId)
  afterEvents : List (ValidatorAtomicEvent BlockId CommitId PacketId)
  actionBefore : ValidatorWorldState BlockId CommitId PacketId
  actionAfter : ValidatorWorldState BlockId CommitId PacketId
  earlierOccurs : earlier.OccursIn timed
  successful :
    (validatorFlexRunStateAt source schedule mapping.cacheAt
      mapping.highestAcceptedRound earlier).output = some output
  recordBeforeLater : ValidatorActionBeforeFlexRun timed later
    earlier.validator (.recordCommit output.toCommitHead)
  sameValidator : later.validator = earlier.validator
  validatorInRange : earlier.validator < config.authorityCount
  laterOccurs : later.OccursIn timed
  laterPrefix : ValidatorFlexRunActionPrefix timed later beforeEvents
    afterEvents actionBefore actionAfter
  frontierUnchanged :
    mapping.highestAcceptedRound later.validator later.input =
      mapping.highestAcceptedRound earlier.validator earlier.input

theorem ValidatorFlexFixedFrontierRecordedSuccessor.rank_decreases
    {mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule}
    {later earlier : LocalFlexCommitterRunObservation BlockId CommitId}
    (step : ValidatorFlexFixedFrontierRecordedSuccessor mapping later earlier) :
    mapping.preparedRank later < mapping.preparedRank earlier := by
  exact fixed_frontier_recorded_success_decreases_prepared_rank mapping
    step.earlierOccurs step.successful step.recordBeforeLater step.sameValidator
    step.validatorInRange step.laterOccurs step.laterPrefix
    step.frontierUnchanged

/-- A finite list of already-observed local commit-loop successors. This type
does not assert that a next run exists. -/
inductive ValidatorFlexFixedFrontierCommitChain
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule) :
    LocalFlexCommitterRunObservation BlockId CommitId →
      List (LocalFlexCommitterRunObservation BlockId CommitId) → Prop
  | nil (start) : ValidatorFlexFixedFrontierCommitChain mapping start []
  | cons {current next rest}
      (step : ValidatorFlexFixedFrontierRecordedSuccessor mapping next current)
      (tail : ValidatorFlexFixedFrontierCommitChain mapping next rest) :
      ValidatorFlexFixedFrontierCommitChain mapping current (next :: rest)

/-- A fixed-DAG local commit chain contains at most the first prepared rank
many recorded-success transitions. Thus a Rust loop that repeats only this
transition cannot continue forever without accepting new DAG evidence. -/
theorem fixed_frontier_commit_chain_length_le_prepared_rank
    {mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule}
    {start : LocalFlexCommitterRunObservation BlockId CommitId}
    {rest : List (LocalFlexCommitterRunObservation BlockId CommitId)}
    (chain : ValidatorFlexFixedFrontierCommitChain mapping start rest) :
    rest.length ≤ mapping.preparedRank start := by
  induction chain with
  | nil => simp
  | cons step tail inductionHypothesis =>
      have decreases := step.rank_decreases
      simp only [List.length_cons]
      omega

/-- Every successful output makes its candidate round the processed frontier. -/
theorem successful_output_sets_next_minimum
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (_occurs : observation.OccursIn timed)
    (_successful :
      (validatorFlexRunStateAt source schedule mapping.cacheAt
        mapping.highestAcceptedRound observation).output = some output) :
    schedule.minimumLeaderRoundForHead output.toCommitHead =
      output.candidate.leaderRound + 1 := by
  simpa [LocalFlexCommitOutput.toCommitHead] using
    schedule.minimumIsRoundSuccessor output.toCommitHead

end ValidatorFlexPendingRefreshSourceMap

end Mysticeti
