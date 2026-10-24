/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorDagProgressComposition

namespace Mysticeti

/-! Operational-frontier successor projection.

This module starts after one correct author has an actual addressed proposal
broadcast at or above the successor of an observed operational maximum. If the
block is in the exact successor round, the result keeps that exact block and
its own immediate-parent quorum. If the block skipped past the successor, its
own parents prove that the author's accepted-quorum frontier has already
advanced far enough. The current operational source then projects that attained
frontier to the public total-quorum layer.

The theorem does not assume that the produced child used the earlier maximum
owner's representative list. It uses the child's exact parent list. A separate
normal max-timeout scheduler must derive the first actual broadcast from the
current maximum source.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- The exact parent quorum of one persisted proposal remains accepted at the
production finish. -/
theorem persisted_broadcast_parents_are_accepted_at_finish
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {start validator : Time}
    (production : ValidatorPersistedProposalBroadcastProduction timed
      obligations start validator) :
    ValidatorParentListReady config
      ((timed.execution.trace production.finish).validatorState validator)
      production.proposal.block.reference.round
      production.proposal.block.parents := by
  have validatorFacts := validator_local_action_occurrence_is_correct_available
    (timed.execution.stepsFollowRules production.persistedAt)
      production.persistenceOccurs
  rcases validatorFacts with ⟨validatorInRange, _validatorCorrect⟩
  rcases validator_world_step_local_action_with_suffix
      (timed.execution.stepsFollowRules production.persistedAt)
        production.persistenceOccurs with
    ⟨actionBefore, actionAfter, _suffix, actionStep, suffixStep⟩
  have guard := validator_atomic_local_action_has_basic_guard actionStep
  have readyBefore := persist_proposal_guard_gives_ready_parent_list guard
  refine ⟨readyBefore.1, ?_, readyBefore.2.2⟩
  intro parent parentMember
  have parentBefore := readyBefore.2.1 parent parentMember
  have parentAfterAction :=
    (validator_atomic_step_durable_monotone actionStep validator)
      |>.accepted_block_persists parentBefore.2
  have parentAfterBatch := validator_world_step_accepted_block_persists
    suffixStep parentAfterAction
  have parentAtFinish := timed.execution.accepted_block_persists
    validatorInRange production.persistenceBeforeFinish parentAfterBatch
  exact ⟨parentBefore.1, parentAtFinish⟩

/-- A current operational maximum has either produced its exact correct
successor child, or the child's own parents already force a later public
total-quorum layer.

This is an internal rich result. The exact-child branch retains the proposal
body, broadcasts, and its exact accepted parent quorum. -/
inductive ValidatorOperationalMaximumSuccessorResult
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (start author maximum : Time) : Prop where
  | exactChild
      (production : ValidatorPersistedProposalBroadcastProduction timed
        obligations start author)
      (childAtSuccessor :
        production.proposal.block.reference.round = maximum + 1)
      (ownParentsAccepted : ValidatorAcceptedQuorumAt config
        ((timed.execution.trace production.finish).validatorState author)
          maximum) :
      ValidatorOperationalMaximumSuccessorResult timed obligations start author
        maximum
  | laterLayer
      (finish round : Time)
      (startBeforeFinish : start ≤ finish)
      (successorAtMostRound : maximum + 1 ≤ round)
      (layer : Nonempty (CorrectHeldTotalQuorumLayer config faults
        (timed.execution.trace finish) round)) :
      ValidatorOperationalMaximumSuccessorResult timed obligations start author
        maximum

/-- Normalize one actual at-or-above broadcast against any supplied
operational base round.

If the exact child is later than `base + 1`, its own immediate parents form an
accepted quorum at `childRound - 1` at the correct author. The author's current
operational frontier at the production finish is at least that round, so one
correct host holds a later public total-quorum layer. -/
theorem at_or_above_broadcast_gives_operational_successor
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {start author base : Time}
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (broadcast : ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations
      start author (base + 1)) :
    ValidatorOperationalMaximumSuccessorResult timed obligations start author
      base := by
  rcases broadcast with ⟨childRound, successorAtMostChild, productionNonempty⟩
  rcases productionNonempty with ⟨⟨production, childRoundExact⟩⟩
  by_cases exactSuccessor : childRound = base + 1
  · apply ValidatorOperationalMaximumSuccessorResult.exactChild production
      (childRoundExact.trans exactSuccessor)
    have parentsReady := persisted_broadcast_parents_are_accepted_at_finish
      production
    refine ⟨production.proposal.block.parents, ?_⟩
    simpa [childRoundExact, exactSuccessor] using parentsReady
  · have childPastSuccessor : base + 1 < childRound := by
      exact Nat.lt_of_le_of_ne successorAtMostChild (Ne.symm exactSuccessor)
    have startBeforeFinish : start ≤ production.finish :=
      Nat.le_trans production.startBeforePersistence
        (Nat.le_trans (Nat.le_succ production.persistedAt)
          production.persistenceBeforeFinish)
    have activeAtFinish := active production.finish startBeforeFinish
    have parentsReady := persisted_broadcast_parents_are_accepted_at_finish
      production
    have childPositive : 0 < childRound :=
      Nat.lt_of_lt_of_le (Nat.zero_lt_succ base) successorAtMostChild
    have authorAcceptedPrevious : ValidatorAcceptedQuorumAt config
        ((timed.execution.trace production.finish).validatorState author)
          (childRound - 1) := by
      refine ⟨production.proposal.block.parents, ?_⟩
      have childIsPreviousSuccessor : childRound - 1 + 1 = childRound :=
        Nat.sub_add_cancel (Nat.succ_le_iff.mpr childPositive)
      simpa [childRoundExact, childIsPreviousSuccessor] using parentsReady
    rcases frontiers.currentSource production.finish author authorInRange
        authorCorrectAvailable activeAtFinish with ⟨authorFrontier⟩
    have previousAtMostAuthorFrontier : childRound - 1 ≤
        frontiers.frontier production.finish author :=
      authorFrontier.upperBound (childRound - 1) authorAcceptedPrevious
    have authorAtMostMaximum : frontiers.frontier production.finish author ≤
        correctOperationalQuorumFrontierMaximumUpTo frontiers
          production.finish config.authorityCount :=
      correct_operational_quorum_frontier_le_maximum frontiers
        (time := production.finish) authorInRange authorCorrectAvailable
    have successorAtMostPrevious : base + 1 ≤ childRound - 1 :=
      Nat.le_sub_one_of_lt childPastSuccessor
    have laterMaximumAtLeastSuccessor : base + 1 ≤
        correctOperationalQuorumFrontierMaximumUpTo frontiers
          production.finish config.authorityCount :=
      Nat.le_trans successorAtMostPrevious
        (Nat.le_trans previousAtMostAuthorFrontier authorAtMostMaximum)
    rcases correct_operational_quorum_frontier_maximum_has_source frontiers
        activeAtFinish with
      ⟨holder, holderInRange, holderCorrect, holderMaximum,
        holderSourceNonempty⟩
    rcases holderSourceNonempty with ⟨holderSource⟩
    have successorAtMostHolderFrontier : base + 1 ≤
        frontiers.frontier production.finish holder := by
      calc
        base + 1 ≤
            correctOperationalQuorumFrontierMaximumUpTo frontiers
              production.finish config.authorityCount :=
          laterMaximumAtLeastSuccessor
        _ = frontiers.frontier production.finish holder := holderMaximum.symm
    have laterMaximumPositive : 0 < frontiers.frontier production.finish holder :=
      Nat.lt_of_lt_of_le (Nat.zero_lt_succ base)
        successorAtMostHolderFrontier
    have publicLayer :=
      positive_operational_frontier_gives_correct_held_total_quorum_layer
        holderInRange holderCorrect laterMaximumPositive holderSource
    exact ValidatorOperationalMaximumSuccessorResult.laterLayer
      production.finish (frontiers.frontier production.finish holder)
        startBeforeFinish successorAtMostHolderFrontier publicLayer

/-- Normalize one actual at-or-above broadcast against the operational maximum
at its observation time.

If the exact child is later than `maximum + 1`, its own immediate parents form
an accepted quorum at `childRound - 1` at the correct author. The author's
current operational frontier at the production finish is at least that round.
The finite correct-host maximum therefore exposes a positive public layer no
earlier than `maximum + 1`. -/
theorem at_or_above_broadcast_gives_operational_maximum_successor
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    {start author : Time}
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (broadcast : ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations
      start author
        (correctOperationalQuorumFrontierMaximumUpTo frontiers start
          config.authorityCount + 1)) :
    ValidatorOperationalMaximumSuccessorResult timed obligations start author
      (correctOperationalQuorumFrontierMaximumUpTo frontiers start
        config.authorityCount) := by
  let maximum := correctOperationalQuorumFrontierMaximumUpTo frontiers start
    config.authorityCount
  rcases broadcast with ⟨childRound, successorAtMostChild, productionNonempty⟩
  rcases productionNonempty with ⟨⟨production, childRoundExact⟩⟩
  change maximum + 1 ≤ childRound at successorAtMostChild
  by_cases exactSuccessor : childRound = maximum + 1
  · apply ValidatorOperationalMaximumSuccessorResult.exactChild production
      (by simpa only [maximum] using childRoundExact.trans exactSuccessor)
    have parentsReady := persisted_broadcast_parents_are_accepted_at_finish
      production
    refine ⟨production.proposal.block.parents, ?_⟩
    simpa [childRoundExact, exactSuccessor] using parentsReady
  · have childPastSuccessor : maximum + 1 < childRound := by
      change childRound ≠ maximum + 1 at exactSuccessor
      exact Nat.lt_of_le_of_ne successorAtMostChild exactSuccessor.symm
    have startBeforeFinish : start ≤ production.finish :=
      Nat.le_trans production.startBeforePersistence
        (Nat.le_trans (Nat.le_succ production.persistedAt)
          production.persistenceBeforeFinish)
    have activeAtFinish := active production.finish startBeforeFinish
    have parentsReady := persisted_broadcast_parents_are_accepted_at_finish
      production
    have childPositive : 0 < childRound :=
      Nat.lt_of_lt_of_le (Nat.zero_lt_succ maximum) successorAtMostChild
    have authorAcceptedPrevious : ValidatorAcceptedQuorumAt config
        ((timed.execution.trace production.finish).validatorState author)
          (childRound - 1) := by
      refine ⟨production.proposal.block.parents, ?_⟩
      have childIsPreviousSuccessor : childRound - 1 + 1 = childRound := by
        exact Nat.sub_add_cancel (Nat.succ_le_iff.mpr childPositive)
      simpa [childRoundExact, childIsPreviousSuccessor] using parentsReady
    rcases frontiers.currentSource production.finish author authorInRange
        authorCorrectAvailable activeAtFinish with ⟨authorFrontier⟩
    have previousAtMostAuthorFrontier : childRound - 1 ≤
        frontiers.frontier production.finish author :=
      authorFrontier.upperBound (childRound - 1) authorAcceptedPrevious
    have authorAtMostMaximum : frontiers.frontier production.finish author ≤
        correctOperationalQuorumFrontierMaximumUpTo frontiers
          production.finish config.authorityCount :=
      correct_operational_quorum_frontier_le_maximum frontiers
        (time := production.finish) authorInRange authorCorrectAvailable
    have successorAtMostPrevious : maximum + 1 ≤ childRound - 1 :=
      Nat.le_sub_one_of_lt childPastSuccessor
    have laterMaximumAtLeastSuccessor : maximum + 1 ≤
        correctOperationalQuorumFrontierMaximumUpTo frontiers
          production.finish config.authorityCount := by
      exact Nat.le_trans successorAtMostPrevious
        (Nat.le_trans previousAtMostAuthorFrontier authorAtMostMaximum)
    rcases correct_operational_quorum_frontier_maximum_has_source frontiers
        activeAtFinish with
      ⟨holder, holderInRange, holderCorrect, holderMaximum,
        holderSourceNonempty⟩
    rcases holderSourceNonempty with ⟨holderSource⟩
    have successorAtMostHolderFrontier : maximum + 1 ≤
        frontiers.frontier production.finish holder := by
      calc
        maximum + 1 ≤
            correctOperationalQuorumFrontierMaximumUpTo frontiers
              production.finish config.authorityCount :=
          laterMaximumAtLeastSuccessor
        _ = frontiers.frontier production.finish holder := holderMaximum.symm
    have laterMaximumPositive : 0 < frontiers.frontier production.finish holder :=
      Nat.lt_of_lt_of_le (Nat.zero_lt_succ maximum)
        successorAtMostHolderFrontier
    have publicLayer :=
      positive_operational_frontier_gives_correct_held_total_quorum_layer
        holderInRange holderCorrect laterMaximumPositive holderSource
    exact ValidatorOperationalMaximumSuccessorResult.laterLayer
      production.finish (frontiers.frontier production.finish holder)
        startBeforeFinish successorAtMostHolderFrontier publicLayer

end Mysticeti
