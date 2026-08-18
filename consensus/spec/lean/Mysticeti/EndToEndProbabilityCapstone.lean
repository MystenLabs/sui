/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.EndToEndLiveness

namespace Mysticeti

/-!
The final probability transfer for a ranking-indexed execution family.

The independent-uniform law proves favorable windows for every receiver's
leader schedule. Each schedule uses that receiver's commit head from the
earlier ranking prefix. Thus, the schedule cannot inspect the current or a
future ranking.

This module does not take a favorable trace or a favorable path as an input.
Its last theorem is an internal transfer lemma. A final public theorem must call
the deterministic composition theorem directly. It must not take that theorem
or a successful-execution property as an input.

`IndependentUniformRoundRankingLaw.probabilityOne` is still an abstract ideal
law. A real measure-one result must construct this law from the countable
product of uniform finite rankings, or give an interpretation theorem for that
measure. This module does not hide that construction in a protocol premise.
-/

namespace UniformRankingEndToEndExecutionFamily

/-- Extend one finite ranking history with the identity ranking. The extension
is only a canonical representative of the finite prefix. Prefix causality makes
the protocol state at the boundary independent of this chosen suffix. -/
def rankingTraceFromHistory
    {authorityCount : Nat}
    (history : List (ValidatorRanking authorityCount)) :
    UniformRoundRankingTrace authorityCount :=
  fun round =>
    if inHistory : round < history.length then
      history[round]
    else
      identityValidatorRanking authorityCount

/-- The canonical extension preserves all entries in the supplied history. -/
theorem ranking_trace_prefix_from_history
    {authorityCount : Nat}
    (history : List (ValidatorRanking authorityCount)) :
    rankingTracePrefix (rankingTraceFromHistory history) history.length =
      history := by
  unfold rankingTracePrefix rankingTraceFromHistory
  simp

end UniformRankingEndToEndExecutionFamily

namespace UniformRankingExecutionSourceMap

open UniformRankingEndToEndExecutionFamily

/-- Select one local commit head from a finite ranking history. The canonical
suffix cannot affect the selected head because the state boundary is prefix
causal. -/
def causalCommitHeadChoice
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family)
    (validator : Nat) : PastDependentCommitHeadChoice family where
  headAfter := fun history =>
    ((((family.execution (rankingTraceFromHistory history)).inputs.timedExecution.execution.trace
        (source.stateBeforeRound history.length)).validatorState validator).commitHead.id)

/-- The past-dependent choice is the actual local commit head at the state
boundary before the sampled round. -/
theorem causal_commit_head_choice_matches
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family)
    (validator : Nat)
    (sampled : UniformRoundRankingTrace authorityCount)
    (round : Nat) :
    (source.causalCommitHeadChoice validator).headAfter
        (rankingTracePrefix sampled round) =
      ((((family.execution sampled).inputs.timedExecution.execution.trace
        (source.stateBeforeRound round)).validatorState validator).commitHead.id) := by
  have historyLength :
      (rankingTracePrefix sampled round).length = round := by
    simp [rankingTracePrefix]
  have canonicalAtHistoryLength :=
    ranking_trace_prefix_from_history (rankingTracePrefix sampled round)
  have samePrefix :
      rankingTracePrefix
          (rankingTraceFromHistory (rankingTracePrefix sampled round)) round =
        rankingTracePrefix sampled round := by
    simpa only [historyLength] using canonicalAtHistoryLength
  have sameState := source.tracePrefixCausal
    (rankingTraceFromHistory (rankingTracePrefix sampled round)) sampled round
    samePrefix (source.stateBeforeRound round) (Nat.le_refl _)
  unfold causalCommitHeadChoice
  change
    ((((family.execution
        (rankingTraceFromHistory (rankingTracePrefix sampled round))).inputs.timedExecution.execution.trace
            (source.stateBeforeRound
              (rankingTracePrefix sampled round).length)).validatorState
          validator).commitHead.id) =
      ((((family.execution sampled).inputs.timedExecution.execution.trace
        (source.stateBeforeRound round)).validatorState validator).commitHead.id)
  rw [historyLength]
  exact congrArg
    (fun world =>
      ((world.validatorState validator).commitHead.id))
    sameState

/-- The adaptive schedule follows one validator's actual commit head at each
pre-round state boundary. The validator is an argument because the local
catch-up proof follows one receiver at a time. -/
noncomputable def causalCommitHeadScheduleFor
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family) (validator : Nat) :
    AdaptiveViableLeaderSchedule authorityCount family.correctAvailable :=
  (source.causalCommitHeadChoice validator).adaptiveSchedule

/-- At each round, the receiver-specific causal schedule is the leader schedule
of that receiver's actual commit head at the pre-round state boundary. -/
theorem causal_commit_head_schedule_for_matches_execution
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family)
    (sampled : UniformRoundRankingTrace authorityCount)
    (validator round : Nat) :
    (source.causalCommitHeadScheduleFor validator).selectedAfter
        (rankingTracePrefix sampled round) =
      family.leaderSchedule
        (((family.execution sampled).inputs.timedExecution.execution.trace
          (source.stateBeforeRound round)).validatorState validator).commitHead.id := by
  change
    family.leaderSchedule
        ((source.causalCommitHeadChoice validator).headAfter
          (rankingTracePrefix sampled round)) = _
  rw [source.causal_commit_head_choice_matches]

end UniformRankingExecutionSourceMap

namespace UniformRankingEndToEndExecutionFamily

/-- The sampled ranking gives favorable future windows for every validator's
causally selected local commit head. This is an internal probability event. It
is not an end-to-end theorem input. -/
noncomputable def AllValidatorCausalHeadFavorableWindows
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family) :
    RoundRankingEvent authorityCount :=
  fun sampled => ∀ validator,
    validator < authorityCount →
    AdaptiveFavorableWindowsAfterEveryRound
      (source.causalCommitHeadScheduleFor validator) (family.depth + 1) sampled

/-- Independent uniform rankings give the causal-head favorable-window event
for every validator. The countable intersection covers the receiver selected
inside each local catch-up proof. -/
theorem all_validator_causal_head_favorable_windows_probability_one
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    (law : IndependentUniformRoundRankingLaw authorityCount)
    (family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount)
    (source : UniformRankingExecutionSourceMap family) :
    law.probabilityOne (AllValidatorCausalHeadFavorableWindows source) := by
  apply law.probabilityOneCountableInter
  intro validator
  apply law.probabilityOneMono
    (adaptive_viable_schedule_has_favorable_windows_probability_one law
      family.correctAvailable (source.causalCommitHeadScheduleFor validator)
        (family.depth + 1))
  intro sampled favorable _validatorInRange
  exact favorable

/-- If one receiver's local head stays unchanged, the all-validator event gives
the exact head-specific leader path for that receiver. -/
theorem causal_favorable_windows_give_stable_validator_head_path
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family)
    (sampled : UniformRoundRankingTrace authorityCount)
    (validator : Nat)
    (validatorInRange : validator < authorityCount)
    (commitId : CommitId)
    (firstFutureRound : Nat)
    (favorable : AllValidatorCausalHeadFavorableWindows source sampled)
    (headStays : ∀ round,
      firstFutureRound ≤ round →
      (((family.execution sampled).inputs.timedExecution.execution.trace
        (source.stateBeforeRound round)).validatorState validator).commitHead.id =
          commitId) :
    CommitHeadFirstSlotLeaderPathCoverageAfter
      (family.execution sampled).config (family.execution sampled).faults
      family.depth commitId firstFutureRound := by
  let choice := source.causalCommitHeadChoice validator
  have favorableFrom :
      AdaptiveFavorableWindowsFrom choice.adaptiveSchedule
        (family.depth + 1) firstFutureRound sampled := by
    intro start _startInFuture
    exact favorable validator validatorInRange start
  have choiceHeadStays : ∀ round,
      firstFutureRound ≤ round →
      choice.headAfter (rankingTracePrefix sampled round) = commitId := by
    intro round roundInFuture
    rw [source.causal_commit_head_choice_matches]
    exact headStays round roundInFuture
  exact favorable_future_gives_stable_commit_head_path choice sampled commitId
    family.depth firstFutureRound favorableFrom choiceHeadStays

/-- A stable receiver suffix and one later ranking boundary give the favorable
leader path for that receiver's unchanged commit head. The no-advance fact is
an internal branch result. It is not a commit-sync or liveness input. -/
theorem stable_receiver_suffix_gives_causal_head_path
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family)
    (sampled : UniformRoundRankingTrace authorityCount)
    (receiver : Nat)
    (receiverInRange : receiver < authorityCount)
    (stableAt firstFutureRound : Time)
    (prior : ValidatorCommitHead CommitId)
    (favorable : AllValidatorCausalHeadFavorableWindows source sampled)
    (headAtStable :
      (((family.execution sampled).inputs.timedExecution.execution.trace
        stableAt).validatorState receiver).commitHead = prior)
    (stableBeforeFirstBoundary :
      stableAt ≤ source.stateBeforeRound firstFutureRound)
    (noAdvance : ¬ValidatorReceiverCommitAdvance
      (family.execution sampled).inputs.timedExecution stableAt receiver) :
    CommitHeadFirstSlotLeaderPathCoverageAfter
      (family.execution sampled).config (family.execution sampled).faults
      family.depth prior.id firstFutureRound := by
  apply causal_favorable_windows_give_stable_validator_head_path source sampled
    receiver receiverInRange prior.id firstFutureRound favorable
  intro round firstBeforeRound
  have firstBoundaryBeforeRound :
      source.stateBeforeRound firstFutureRound ≤ source.stateBeforeRound round :=
    source.stateBeforeRoundMonotone firstFutureRound round firstBeforeRound
  have stableBeforeRound : stableAt ≤ source.stateBeforeRound round :=
    Nat.le_trans stableBeforeFirstBoundary firstBoundaryBeforeRound
  let timed := (family.execution sampled).inputs.timedExecution
  have receiverInExecutionRange :
      receiver < (family.execution sampled).config.authorityCount := by
    rw [family.authorityCountMatches sampled]
    exact receiverInRange
  have durable := timed.execution.durableStateMonotone receiver stableAt
    (source.stateBeforeRound round) receiverInExecutionRange stableBeforeRound
  have notStrict : ¬
      ((timed.execution.trace stableAt).validatorState
          receiver).commitHead.index <
        ((timed.execution.trace (source.stateBeforeRound round)).validatorState
          receiver).commitHead.index := by
    intro advanced
    exact noAdvance ⟨source.stateBeforeRound round, stableBeforeRound, advanced⟩
  have sameIndex :
      ((timed.execution.trace stableAt).validatorState
          receiver).commitHead.index =
        ((timed.execution.trace (source.stateBeforeRound round)).validatorState
          receiver).commitHead.index := by
    have monotone := durable.1
    omega
  have sameHead :
      ((timed.execution.trace (source.stateBeforeRound round)).validatorState
          receiver).commitHead =
        ((timed.execution.trace stableAt).validatorState receiver).commitHead :=
    (durable.2.2.1 sameIndex).symm
  change
    ((timed.execution.trace (source.stateBeforeRound round)).validatorState
      receiver).commitHead.id = prior.id
  rw [sameHead, headAtStable]

/-- Finite-prefix causality selects the first unobserved ranking boundary for
one stable receiver suffix. The probability event then supplies the receiver's
favorable path after that boundary. -/
theorem stable_receiver_suffix_has_future_causal_head_path
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family)
    (sampled : UniformRoundRankingTrace authorityCount)
    (receiver : Nat)
    (receiverInRange : receiver < authorityCount)
    (stableAt : Time)
    (prior : ValidatorCommitHead CommitId)
    (favorable : AllValidatorCausalHeadFavorableWindows source sampled)
    (headAtStable :
      (((family.execution sampled).inputs.timedExecution.execution.trace
        stableAt).validatorState receiver).commitHead = prior)
    (noAdvance : ¬ValidatorReceiverCommitAdvance
      (family.execution sampled).inputs.timedExecution stableAt receiver) :
    ∃ firstFutureRound,
      stableAt ≤ source.stateBeforeRound firstFutureRound ∧
        CommitHeadFirstSlotLeaderPathCoverageAfter
          (family.execution sampled).config (family.execution sampled).faults
          family.depth prior.id firstFutureRound := by
  rcases source.finiteTimeHasFutureRankingBoundary stableAt with
    ⟨firstFutureRound, stableBeforeBoundary⟩
  exact ⟨firstFutureRound, stableBeforeBoundary,
    stable_receiver_suffix_gives_causal_head_path source sampled receiver
      receiverInRange stableAt firstFutureRound prior favorable headAtStable
        stableBeforeBoundary noAdvance⟩

/-- Convert the family-level receiver and depth indexes to the exact indexes of
one sampled execution.

This result is the receiver-local adapter for the deterministic commit-step
proof. It still derives the favorable path from the probability event. -/
theorem stable_execution_receiver_suffix_has_future_causal_head_path
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (source : UniformRankingExecutionSourceMap family)
    (sampled : UniformRoundRankingTrace authorityCount)
    (receiver : Nat)
    (receiverInRange :
      receiver < (family.execution sampled).config.authorityCount)
    (stableAt : Time)
    (prior : ValidatorCommitHead CommitId)
    (favorable : AllValidatorCausalHeadFavorableWindows source sampled)
    (headAtStable :
      (((family.execution sampled).inputs.timedExecution.execution.trace
        stableAt).validatorState receiver).commitHead = prior)
    (noAdvance : ¬ValidatorReceiverCommitAdvance
      (family.execution sampled).inputs.timedExecution stableAt receiver) :
    ∃ firstFutureRound,
      stableAt ≤ source.stateBeforeRound firstFutureRound ∧
        CommitHeadFirstSlotLeaderPathCoverageAfter
          (family.execution sampled).config (family.execution sampled).faults
          (family.execution sampled).inputs.leaderSchedule.indirectDepth
          prior.id firstFutureRound := by
  have receiverInFamilyRange : receiver < authorityCount := by
    rw [← family.authorityCountMatches sampled]
    exact receiverInRange
  rcases stable_receiver_suffix_has_future_causal_head_path source sampled
      receiver receiverInFamilyRange stableAt prior favorable headAtStable
        noAdvance with
    ⟨firstFutureRound, stableBeforeBoundary, path⟩
  refine ⟨firstFutureRound, stableBeforeBoundary, ?_⟩
  simpa only [family.indirectDepthMatches sampled] using path

end UniformRankingEndToEndExecutionFamily

end Mysticeti
