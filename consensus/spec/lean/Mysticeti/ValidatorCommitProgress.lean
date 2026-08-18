/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommitProgressRecovery
import Mysticeti.LeaderCoverage
import Mysticeti.ValidatorProcess

namespace Mysticeti

/-! A finite validator-level construction for commit progress.

The public theorem uses a fixed fault interval, a leader schedule, and an
executable finite block schedule. Quorum block layers and usable anchors occur
only as derived results. The selected validator set stays equal to the leader
schedule. Only the first selected leader slot changes.
-/

/-- An author-level view of produced blocks and immediate-parent choices. -/
structure ValidatorBlockExecution where
  produced : Nat → Nat → Bool
  includedParent : Nat → Nat → Nat → Bool

namespace ValidatorBlockExecution

/-- Validators whose next-round blocks include the selected leader block. -/
def directVoters
    (execution : ValidatorBlockExecution) (round leader : Nat) : VoterSet :=
  fun voter =>
    execution.produced (round + directVoteRoundOffset) voter &&
      execution.includedParent round leader voter

end ValidatorBlockExecution

/-- The finite scheduled execution used by the public construction. -/
def scheduledCommitExecution
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (height : Nat) : ValidatorBlockExecution where
  produced := fun round validator => decide
    (validator < config.authorityCount ∧
      faults.correctAvailable validator = true ∧ round ≤ height)
  includedParent := fun round leader voter => decide
    (leader < config.authorityCount ∧
      faults.correctAvailable leader = true ∧
      voter < config.authorityCount ∧
      faults.correctAvailable voter = true ∧
      round + directVoteRoundOffset ≤ height)

/-- The status list for one full leader schedule.

The first slot is Commit after a direct quorum. Every other selected slot can
remain Undecided. The status count stays equal to the schedule member count. -/
def scheduleDirectDecisionSlotStatuses
    {authorityCount : Nat} {stake : Nat → Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (thresholds : Thresholds authorityCount stake)
    (execution : ValidatorBlockExecution)
    (depth round : Nat) : List SelectedLeaderSlotStatus :=
  let leader := repeatedFirstLeader schedule (depth + 1) round
  let tail := List.replicate (schedule.memberCount - 1)
    SelectedLeaderSlotStatus.undecided
  if thresholds.quorum ≤
      weight authorityCount stake (execution.directVoters round leader) then
    .commit :: tail
  else
    .undecided :: tail

theorem schedule_direct_status_count
    {authorityCount : Nat} {stake : Nat → Nat}
    (schedule : DeterministicLeaderSchedule authorityCount)
    (thresholds : Thresholds authorityCount stake)
    (execution : ValidatorBlockExecution)
    (depth round : Nat) :
    (scheduleDirectDecisionSlotStatuses schedule thresholds execution depth round).length =
      schedule.memberCount := by
  unfold scheduleDirectDecisionSlotStatuses
  dsimp only
  have countPositive := schedule.memberCountPositive
  have countEq : schedule.memberCount - 1 + 1 = schedule.memberCount := by
    omega
  split <;> simp [countEq]

theorem direct_quorum_gives_usable_schedule_order
    {authorityCount : Nat} {stake : Nat → Nat}
    {schedule : DeterministicLeaderSchedule authorityCount}
    {thresholds : Thresholds authorityCount stake}
    {execution : ValidatorBlockExecution}
    {depth round : Nat}
    (directQuorum : thresholds.quorum ≤
      weight authorityCount stake
        (execution.directVoters round
          (repeatedFirstLeader schedule (depth + 1) round))) :
    UsableAnchorOrder
      (scheduleDirectDecisionSlotStatuses schedule thresholds execution depth
        round) := by
  simp [scheduleDirectDecisionSlotStatuses, directQuorum, UsableAnchorOrder]

/-- A reference returned by the modeled commit operation. -/
structure ModeledCommitReference where
  index : Nat
  leaderRound : Nat
  deriving DecidableEq, Repr

def commonCommitReference
    (previousCommitIndex firstPendingRound candidateIndex : Nat) :
    ModeledCommitReference :=
  { index := previousCommitIndex + 1
    leaderRound := firstPendingRound + candidateIndex }

/-- One validator records the common reference after the commit action runs. -/
def recordLocalCommit
    (nonProgress : VoterSet) (validator : Nat)
    (reference : ModeledCommitReference) : Option ModeledCommitReference :=
  if nonProgress validator then none else some reference

@[simp]
theorem correct_available_validator_records_common_commit
    {CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {validator : Nat} {reference : ModeledCommitReference}
    (correctAvailable : faults.correctAvailable validator = true) :
    recordLocalCommit faults.nonProgress validator reference = some reference := by
  have canProgress : faults.nonProgress validator = false := by
    simpa [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full] using
      correctAvailable
  simp [recordLocalCommit, canProgress]

/-! ### Internal pointwise bridge -/

/-- Pointwise production and parent inclusion give collective layer and direct
vote facts for one finite favorable window. These high-level facts are internal.
The public theorem below constructs them from `scheduledCommitExecution`. -/
private theorem validator_window_gives_layers_and_direct_votes
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {correctAvailable : VoterSet}
    {schedule : DeterministicLeaderSchedule authorityCount}
    {execution : ValidatorBlockExecution}
    {depth baseRound : Nat}
    (correctAvailableReachesQuorum :
      thresholds.quorum ≤
        weight authorityCount stake correctAvailable)
    (favorable : FavorableFirstLeaderWindow schedule correctAvailable
      (depth + 1) baseRound)
    (producesWindow :
      ∀ validator offset,
        validator < authorityCount →
        correctAvailable validator = true →
        offset < requiredRecoveryLayerCount
            (requiredRecoveryAnchorCount depth) →
        execution.produced (baseRound + offset) validator = true)
    (includesProgressingParent :
      ∀ round parentAuthor validator,
        parentAuthor < authorityCount →
        validator < authorityCount →
        correctAvailable parentAuthor = true →
        correctAvailable validator = true →
        execution.produced round parentAuthor = true →
        execution.produced (round + directVoteRoundOffset) validator = true →
        execution.includedParent round parentAuthor validator = true) :
    (∀ offset,
      offset < requiredRecoveryLayerCount
          (requiredRecoveryAnchorCount depth) →
      thresholds.quorum ≤
        weight authorityCount stake (execution.produced (baseRound + offset))) ∧
    (∀ offset, offset < requiredRecoveryAnchorCount depth →
      thresholds.quorum ≤
        weight authorityCount stake
          (execution.directVoters (baseRound + offset)
            (repeatedFirstLeader schedule (depth + 1)
              (baseRound + offset)))) := by
  constructor
  · intro offset offsetInWindow
    apply Nat.le_trans correctAvailableReachesQuorum
    apply weight_mono stake
    intro validator validatorInRange validatorLive
    exact producesWindow validator offset validatorInRange validatorLive
      offsetInWindow
  · intro offset offsetInWindow
    apply Nat.le_trans correctAvailableReachesQuorum
    apply weight_mono stake
    intro validator validatorInRange validatorLive
    have anchorOffsetInLayers :
        offset < requiredRecoveryLayerCount
          (requiredRecoveryAnchorCount depth) := by
      simp [requiredRecoveryLayerCount, requiredRecoveryAnchorCount,
        directVoteRoundOffset] at offsetInWindow ⊢
      omega
    have voteOffsetInLayers :
        offset + directVoteRoundOffset < requiredRecoveryLayerCount
          (requiredRecoveryAnchorCount depth) := by
      simp [requiredRecoveryLayerCount, requiredRecoveryAnchorCount,
        directVoteRoundOffset] at offsetInWindow ⊢
      omega
    have firstLeaderLive :
        correctAvailable
          (repeatedFirstLeader schedule (depth + 1) (baseRound + offset)) = true :=
      favorable offset (by
        simpa [requiredRecoveryAnchorCount] using offsetInWindow)
    have firstLeaderInRange := schedule.memberInRange
      (repeatedFirstIndex schedule (depth + 1) (baseRound + offset))
    have leaderBlock := producesWindow
      (repeatedFirstLeader schedule (depth + 1) (baseRound + offset)) offset
      firstLeaderInRange firstLeaderLive anchorOffsetInLayers
    have voterBlockAtOffset := producesWindow validator
      (offset + directVoteRoundOffset) validatorInRange validatorLive
      voteOffsetInLayers
    have voterBlock :
        execution.produced
          (baseRound + offset + directVoteRoundOffset) validator = true := by
      simpa [Nat.add_assoc] using voterBlockAtOffset
    have included := includesProgressingParent (baseRound + offset)
      (repeatedFirstLeader schedule (depth + 1) (baseRound + offset)) validator
      firstLeaderInRange validatorInRange firstLeaderLive validatorLive
      leaderBlock voterBlock
    simp [ValidatorBlockExecution.directVoters, voterBlock, included]

private theorem favorable_window_gives_anchor_window
    {authorityCount : Nat} {stake : Nat → Nat}
    {schedule : DeterministicLeaderSchedule authorityCount}
    {thresholds : Thresholds authorityCount stake}
    {execution : ValidatorBlockExecution}
    {depth baseRound baseIndex firstPendingRound previousCommitIndex : Nat}
    (baseRoundAtIndex : firstPendingRound + baseIndex = baseRound)
    (directQuorums : ∀ offset,
      offset < requiredRecoveryAnchorCount depth →
      thresholds.quorum ≤
        weight authorityCount stake
          (execution.directVoters (baseRound + offset)
            (repeatedFirstLeader schedule (depth + 1)
              (baseRound + offset)))) :
    let state := flexCommitStateFromSlotStatuses previousCommitIndex
      (baseIndex + depth + 1)
      (fun index => scheduleDirectDecisionSlotStatuses schedule thresholds
        execution depth (firstPendingRound + index))
    FlexAnchorWindow state baseIndex (depth + 1) := by
  intro state
  apply usable_orders_give_flex_anchor_window
  intro offset offsetInWindow
  have roundAtIndex :
      firstPendingRound + (baseIndex + offset) = baseRound + offset := by
    omega
  apply direct_quorum_gives_usable_schedule_order
  rw [roundAtIndex]
  exact directQuorums offset (by
    simpa [requiredRecoveryAnchorCount] using offsetInWindow)

private theorem scheduled_execution_produces_window
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    {height baseRound count : Nat}
    (windowBelowHeight : baseRound + count ≤ height) :
    ∀ validator offset,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      offset < count →
      (scheduledCommitExecution config faults height).produced
        (baseRound + offset) validator = true := by
  intro validator offset validatorInRange validatorLive offsetInWindow
  have roundBelowHeight : baseRound + offset ≤ height := by omega
  simp [scheduledCommitExecution, validatorInRange, validatorLive,
    roundBelowHeight]

private theorem scheduled_execution_includes_correct_parent
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (height round parentAuthor validator : Nat)
    (parentAuthorInRange : parentAuthor < config.authorityCount)
    (validatorInRange : validator < config.authorityCount)
    (parentAuthorLive : faults.correctAvailable parentAuthor = true)
    (validatorLive : faults.correctAvailable validator = true)
    (_parentProduced :
      (scheduledCommitExecution config faults height).produced round parentAuthor =
        true)
    (_validatorProduced :
      (scheduledCommitExecution config faults height).produced
      (round + directVoteRoundOffset) validator = true) :
    (scheduledCommitExecution config faults height).includedParent
      round parentAuthor validator = true := by
  have roundBelowHeight : round + directVoteRoundOffset ≤ height := by
    have producedFacts :
        validator < config.authorityCount ∧
          faults.correctAvailable validator = true ∧
          round + directVoteRoundOffset ≤ height := by
      simpa [scheduledCommitExecution] using _validatorProduced
    exact producedFacts.2.2
  simp [scheduledCommitExecution, parentAuthorInRange, parentAuthorLive,
    validatorInRange, validatorLive, roundBelowHeight]

/-! ### End-to-end finite construction -/

/-- The finite scheduled validator execution produces one common commit.

The schedule stake bound supplies a correct, available schedule member. The
repeated-first rule gives that member `depth + 1` consecutive first slots. The
selected set remains the complete leader schedule in every round. -/
theorem scheduled_validator_execution_produces_common_commit
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (schedule : DeterministicLeaderSchedule config.authorityCount)
    (scheduleCommitId : CommitId)
    (scheduleMatchesConfig :
      schedule.selected = config.leaderSchedule scheduleCommitId)
    (depth productionStartRound firstPendingRound previousCommitIndex : Nat)
    (outcome : Nat → IndirectRoundOutcome)
    (depthPositive : 0 < depth)
    (scheduleViable :
      config.thresholds.fault + faults.unavailableStakeBound <
        weight config.authorityCount config.stake schedule.selected) :
    ∃ baseRound baseIndex height candidate reference,
      productionStartRound ≤ baseRound ∧
      firstPendingRound ≤ baseRound ∧
      firstPendingRound + baseIndex = baseRound ∧
      FavorableFirstLeaderWindow schedule faults.correctAvailable
        (depth + 1) baseRound ∧
      (∀ round,
        repeatedFirstSelection schedule round =
          config.leaderSchedule scheduleCommitId) ∧
      let execution := scheduledCommitExecution config faults height
      let slotStatuses := fun index =>
        scheduleDirectDecisionSlotStatuses schedule config.thresholds execution
          depth (firstPendingRound + index)
      let state := flexCommitStateFromSlotStatuses previousCommitIndex
        (baseIndex + depth + 1) slotStatuses
      let result := runFlexIndirectDescending depth outcome baseIndex
        (baseIndex + 1) state
      let expectedReference := commonCommitReference previousCommitIndex
        firstPendingRound candidate
      reference = expectedReference ∧
        (∀ offset,
          offset < requiredRecoveryLayerCount
              (requiredRecoveryAnchorCount depth) →
          config.thresholds.quorum ≤
            weight config.authorityCount config.stake
              (execution.produced (baseRound + offset))) ∧
        (∀ offset, offset < requiredRecoveryAnchorCount depth →
          config.thresholds.quorum ≤
            weight config.authorityCount config.stake
              (execution.directVoters (baseRound + offset)
                (repeatedFirstLeader schedule (depth + 1)
                  (baseRound + offset)))) ∧
        FlexAnchorWindow state baseIndex (depth + 1) ∧
        findFlexCommitRound result = some candidate ∧
        (recordFlexCommitResult result).commitIndex = previousCommitIndex + 1 ∧
        (∀ validator, validator < config.authorityCount →
          faults.correctAvailable validator = true →
          recordLocalCommit faults.nonProgress validator reference =
            some reference) := by
  have containsCorrectAvailable :=
    viable_schedule_contains_correct_available_member faults schedule
      scheduleViable
  let scheduleStart := max productionStartRound firstPendingRound
  rcases repeated_first_has_depth_window_after schedule faults.correctAvailable
      depth containsCorrectAvailable scheduleStart with
    ⟨baseRound, baseAfterScheduleStart, favorable⟩
  let layerCount := requiredRecoveryLayerCount
    (requiredRecoveryAnchorCount depth)
  let height := baseRound + layerCount
  let baseIndex := baseRound - firstPendingRound
  have baseAfterProductionStart : productionStartRound ≤ baseRound :=
    Nat.le_trans (Nat.le_max_left _ _) baseAfterScheduleStart
  have baseAfterPendingStart : firstPendingRound ≤ baseRound :=
    Nat.le_trans (Nat.le_max_right _ _) baseAfterScheduleStart
  have baseRoundAtIndex : firstPendingRound + baseIndex = baseRound := by
    simp only [baseIndex]
    omega
  have selectedSetUnchanged : ∀ round,
      repeatedFirstSelection schedule round =
        config.leaderSchedule scheduleCommitId := by
    intro round
    rw [repeated_first_keeps_selected_set, scheduleMatchesConfig]
  let execution := scheduledCommitExecution config faults height
  have producesWindow :
      ∀ validator offset,
        validator < config.authorityCount →
        faults.correctAvailable validator = true →
        offset < layerCount →
        execution.produced (baseRound + offset) validator = true := by
    exact scheduled_execution_produces_window config faults
      (baseRound := baseRound) (count := layerCount) (height := height)
      (by simp [height])
  have layerAndVotes := validator_window_gives_layers_and_direct_votes
    (thresholds := config.thresholds) (schedule := schedule)
    (execution := execution) faults.correct_available_stake_is_quorum favorable
    (by simpa [layerCount] using producesWindow)
    (by
      intro round parentAuthor validator parentAuthorInRange validatorInRange
        parentAuthorLive validatorLive parentProduced validatorProduced
      exact scheduled_execution_includes_correct_parent config faults height round
        parentAuthor validator parentAuthorInRange validatorInRange parentAuthorLive
        validatorLive parentProduced validatorProduced)
  have layerQuorums := layerAndVotes.1
  have directQuorums := layerAndVotes.2
  let slotStatuses := fun index =>
    scheduleDirectDecisionSlotStatuses schedule config.thresholds execution depth
      (firstPendingRound + index)
  let state := flexCommitStateFromSlotStatuses previousCommitIndex
    (baseIndex + depth + 1) slotStatuses
  have anchorWindow : FlexAnchorWindow state baseIndex (depth + 1) := by
    exact favorable_window_gives_anchor_window baseRoundAtIndex directQuorums
  have windowInRange : baseIndex + depth < state.roundCount := by
    simp [state, flexCommitStateFromSlotStatuses]
  let result := runFlexIndirectDescending depth outcome baseIndex
    (baseIndex + 1) state
  rcases complete_flex_indirect_prefix_finds_commit depthPositive anchorWindow
      windowInRange with ⟨candidate, foundCandidate⟩
  let reference := commonCommitReference previousCommitIndex firstPendingRound
    candidate
  refine ⟨baseRound, baseIndex, height, candidate, reference,
    baseAfterProductionStart, baseAfterPendingStart, baseRoundAtIndex, favorable,
    selectedSetUnchanged, ?_⟩
  dsimp only
  constructor
  · rfl
  constructor
  · exact layerQuorums
  constructor
  · exact directQuorums
  constructor
  · exact anchorWindow
  constructor
  · simpa [result] using foundCandidate
  constructor
  · have advanced := flex_anchor_window_advances_commit_index
      (outcome := outcome) depthPositive anchorWindow windowInRange
    simpa [result, state, flexCommitStateFromSlotStatuses] using advanced
  · intro validator _validatorInRange validatorLive
    exact correct_available_validator_records_common_commit validatorLive

end Mysticeti
