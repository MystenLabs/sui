/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.ValidatorExactNextRecovery
import Mysticeti.ValidatorFlexPendingRefresh

namespace Mysticeti

/-! Exact first-leader parent inclusion for one unchanged commit head.

This module compares one actual timer-paced proposal with one actual local
FlexCommitter scan. The comparison is local to their exact commit head. A
commit install can change the schedule, so a caller must restart the comparison
after that install.

The parent theorem permits additional proposal parents. It requires only that
the exact first selected leader is the current retained representative at the
proposal snapshot. The existing recovery parent-selection rule then includes
that exact reference.
-/

/-- The head of one configured selected-leader order is a member of the
schedule for the same exact commit head. -/
theorem selected_leader_order_head_is_in_schedule
    {CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {commitId : CommitId} {round leader : Nat}
    (first : (config.selectedLeaderOrder commitId round).head? = some leader) :
    config.leaderSchedule commitId leader = true := by
  cases orderShape : config.selectedLeaderOrder commitId round with
  | nil =>
      simp [orderShape] at first
  | cons firstLeader remaining =>
      have firstLeaderExact : firstLeader = leader := by
        simpa [orderShape] using first
      subst firstLeader
      apply config.selectedLeaderFromSchedule commitId round leader
      simp [orderShape]

/-- The head of one configured selected-leader order is an in-range
validator. -/
theorem selected_leader_order_head_is_in_range
    {CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {commitId : CommitId} {round leader : Nat}
    (first : (config.selectedLeaderOrder commitId round).head? = some leader) :
    leader < config.authorityCount := by
  exact config.scheduleValidatorInRange commitId leader
    (selected_leader_order_head_is_in_schedule first)

/-- One timer-paced proposal includes the exact retained first selected leader
for its commit head.

Additional immediate parents are allowed. The theorem does not compare
schedules across a commit install. -/
theorem timer_paced_proposal_includes_exact_first_selected_leader
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {validator round : Nat}
    {leader : ValidatorBlockRef BlockId}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      (round + 1))
    (first :
      (config.selectedLeaderOrder production.commitHead.id round).head? =
        some leader.author)
    (representative :
      ((timed.execution.trace production.snapshot.snapshotAt).validatorState
        validator).acceptedRepresentative round leader.author = some leader)
    (retained :
      ((timed.execution.trace production.snapshot.snapshotAt).validatorState
        validator).retained leader = true) :
    leader ∈ production.snapshot.block.parents ∧
      config.leaderSchedule production.commitHead.id leader.author = true := by
  have leaderInRange : leader.author < config.authorityCount :=
    selected_leader_order_head_is_in_range first
  constructor
  · apply timer_paced_round_includes_retained_current_parent production
      leaderInRange
    · have roundMinus : round + 1 - 1 = round := by omega
      rw [roundMinus]
      exact representative
    · exact retained
  · exact selected_leader_order_head_is_in_schedule first

/-- An exact same-head Flex scan and timer-paced proposal use the same first
selected leader.

The result contains the exact proposal-parent reference and the exact first
slot in the prepared Flex input. It is conditional on an already-occurring
scan and contains no future proposal, scan, block, or commit. -/
theorem same_head_timer_proposal_and_prepared_flex_scan_share_first_leader
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq BlockId]
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (pendingSource : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {validator round : Nat}
    {leader : ValidatorBlockRef BlockId}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      (round + 1))
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (occurs : observation.OccursIn timed)
    (sameHead : observation.input.commitHead = production.commitHead)
    (index : Nat)
    (indexInRange :
      index < (validatorFlexPreparedInputAt source schedule
        pendingSource.cacheAt pendingSource.highestAcceptedRound
          observation).pending.roundCount)
    (roundAtIndex :
      ((validatorFlexPreparedInputAt source schedule pendingSource.cacheAt
        pendingSource.highestAcceptedRound observation).pending.rounds
          index).round = round)
    (first :
      (config.selectedLeaderOrder production.commitHead.id round).head? =
        some leader.author)
    (representative :
      ((timed.execution.trace production.snapshot.snapshotAt).validatorState
        validator).acceptedRepresentative round leader.author = some leader)
    (retained :
      ((timed.execution.trace production.snapshot.snapshotAt).validatorState
        validator).retained leader = true) :
    leader ∈ production.snapshot.block.parents ∧
      (((validatorFlexPreparedInputAt source schedule pendingSource.cacheAt
          pendingSource.highestAcceptedRound observation).pending.rounds
            index).selectedSlots.map ReferenceSelectedSlotView.slot).head? =
        some (ExactSelectedLeaderSlot.mk round leader.author) := by
  have included := timer_paced_proposal_includes_exact_first_selected_leader
    production first representative retained
  refine ⟨included.1, ?_⟩
  have configured := pendingSource.preparedSlotsMatchCurrentConfig observation
    occurs index indexInRange
  let prepared := validatorFlexPreparedInputAt source schedule
    pendingSource.cacheAt pendingSource.highestAcceptedRound observation
  let preparedRound := prepared.pending.rounds index
  have configuredAtRound :
      preparedRound.selectedSlots.map ReferenceSelectedSlotView.slot =
        (config.selectedLeaderOrder observation.input.commitHead.id
          preparedRound.round).map fun selectedValidator =>
            { round := preparedRound.round, validator := selectedValidator } := by
    exact configured
  have firstAtPreparedRound :
      (config.selectedLeaderOrder observation.input.commitHead.id
        preparedRound.round).head? = some leader.author := by
    simpa [prepared, preparedRound, sameHead, roundAtIndex] using first
  have mappedHead :
      ((config.selectedLeaderOrder observation.input.commitHead.id
        preparedRound.round).map fun selectedValidator =>
          ExactSelectedLeaderSlot.mk preparedRound.round
            selectedValidator).head? =
        some (ExactSelectedLeaderSlot.mk preparedRound.round leader.author) := by
    cases orderShape : config.selectedLeaderOrder
        observation.input.commitHead.id preparedRound.round with
    | nil =>
        simp [orderShape] at firstAtPreparedRound
    | cons firstLeader remaining =>
        have firstLeaderExact : firstLeader = leader.author := by
          simpa [orderShape] using firstAtPreparedRound
        subst firstLeader
        simp
  have preparedHead :
      (preparedRound.selectedSlots.map ReferenceSelectedSlotView.slot).head? =
        some (ExactSelectedLeaderSlot.mk preparedRound.round leader.author) := by
    rw [configuredAtRound]
    exact mappedHead
  change (preparedRound.selectedSlots.map ReferenceSelectedSlotView.slot).head? =
    some (ExactSelectedLeaderSlot.mk round leader.author)
  rw [← roundAtIndex]
  exact preparedHead

end Mysticeti
