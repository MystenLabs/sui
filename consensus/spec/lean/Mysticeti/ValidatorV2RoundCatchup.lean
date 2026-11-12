/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCollectiveRecoveryCarrier
import Mysticeti.ValidatorGcAwareBlockProductionV2

namespace Mysticeti

/-! Actual-round backfill for V2 block-production liveness.

`BlockProductionLiveness` gives an actual high own block. It does not state
that the author produced every lower round. This module isolates the proposed
`ASM-LIVE-ROUND-CATCHUP` no-skip refinement which is needed to recover those
lower proposals.

Both source fields classify actions which already occur in the trace. They do
not state that a future block, window, delivery, or commit exists. The derived
theorems use the existing bounded proposal and send workers to construct the
later broadcast facts. The adopted path uses recovery `proposeNext` work for
each backfilled round.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- Past-only no-skip and fixed-gate provenance for actual correct proposal
persistence.

The first field is the proposed round-catchup behavior: an active correct
author persists only the exact round after its current signer floor. The second
field recovers the timer, action, and refreshed-parent snapshot for an actual
fresh persistence. The timer source then gives full retained-representative
selection at that snapshot.
-/
structure ValidatorV2RoundCatchupSourceMap
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits) : Prop where
  correctPersistIsExactNext : ∀ time validator block,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace time).epochActive = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.persistProposal block) →
    block.reference.round =
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1
  freshExactNextPersistHasTimerOrigin : ∀
      observation persistTime validator block,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    observation ≤ persistTime →
    ValidatorEpochActiveBetween timed.execution.trace observation persistTime →
    ((timed.execution.trace observation).validatorState
        validator).highestSignedRound + 1 < block.reference.round →
    block.reference.round =
      ((timed.execution.trace persistTime).validatorState
        validator).highestSignedRound + 1 →
    ValidatorLocalActionOccurs (timed.execution.events persistTime) validator
        (.persistProposal block) →
    Nonempty (ValidatorCurrentTimerProposalOriginAt timerSource persistTime
      validator block)

/-- A deterministic peer gives the timer-production adapter one concrete send
witness. -/
private def v2RoundCatchupOtherReceiver (validator : Nat) : Nat :=
  if validator = 0 then 1 else 0

private theorem v2_round_catchup_other_receiver_in_range
    {validator authorityCount : Nat}
    (authorityCountAtLeastTwo : 1 < authorityCount) :
    v2RoundCatchupOtherReceiver validator < authorityCount := by
  simp only [v2RoundCatchupOtherReceiver]
  split
  · exact authorityCountAtLeastTwo
  · omega

private theorem v2_round_catchup_other_receiver_is_different
    {validator : Nat} :
    v2RoundCatchupOtherReceiver validator ≠ validator := by
  simp only [v2RoundCatchupOtherReceiver]
  split <;> omega

/-- One actual high own block recovers one exact recovery-timer proposal.

The adopted fixed-reference path uses the proposed no-skip recovery worker.
Its origin provides the exact recovery snapshot, deadline, selected parents,
and addressed broadcasts required by the adjacent-round proof. -/
theorem actual_high_own_block_gives_fresh_timer_paced_intermediate
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorV2RoundCatchupSourceMap timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {observation finish validator targetRound highRound : Time}
    {highReference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true)
    (observationBeforeFinish : observation ≤ finish)
    (targetAtMostHigh : targetRound ≤ highRound)
    (targetFresh :
      ((timed.execution.trace observation).validatorState
          validator).highestSignedRound + 1 < targetRound)
    (highOwned :
      ((timed.execution.trace finish).validatorState validator).ownBlockAt
        highRound = some highReference)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    Nonempty (ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation validator targetRound) := by
  have highAtMostFinishFloor : highRound ≤
      ((timed.execution.trace finish).validatorState
        validator).highestSignedRound :=
    (timed.execution.statesWellFormed finish validator validatorInRange)
      |>.ownBlockDoesNotExceedSignerFloor highRound highReference highOwned
  have targetReached : targetRound ≤
      ((timed.execution.trace finish).validatorState
        validator).highestSignedRound :=
    Nat.le_trans targetAtMostHigh highAtMostFinishFloor
  have targetAboveStart :
      ((timed.execution.trace observation).validatorState
          validator).highestSignedRound < targetRound := by
    omega
  rcases signer_floor_first_target_crossing_has_persist_proposal
      timed.execution observationBeforeFinish targetAboveStart targetReached with
    ⟨persistTime, block, observationBeforePersist, _persistBeforeFinish,
      floorBeforeTarget, persisted, targetAtMostBlock⟩
  have activeAtPersist :
      (timed.execution.trace persistTime).epochActive = true :=
    active persistTime observationBeforePersist
  have exactNext := source.correctPersistIsExactNext persistTime validator block
    validatorInRange validatorCorrect activeAtPersist persisted
  have blockAtMostTarget : block.reference.round ≤ targetRound := by
    omega
  have blockRound : block.reference.round = targetRound :=
    Nat.le_antisymm blockAtMostTarget targetAtMostBlock
  have activeToPersist : ValidatorEpochActiveBetween timed.execution.trace
      observation persistTime := by
    intro time observationBeforeTime _timeBeforePersist
    exact active time observationBeforeTime
  have blockFresh :
      ((timed.execution.trace observation).validatorState
          validator).highestSignedRound + 1 < block.reference.round := by
    rw [blockRound]
    exact targetFresh
  rcases source.freshExactNextPersistHasTimerOrigin observation persistTime
      validator block validatorInRange validatorCorrect observationBeforePersist
        activeToPersist blockFresh exactNext persisted with ⟨origin⟩
  have targetAtTimerStart : targetRound =
      ((timed.execution.trace origin.start.startedAt).validatorState
          validator).highestSignedRound + 1 := by
    calc
      targetRound = block.reference.round := blockRound.symm
      _ = origin.start.targetRound := origin.proposalTarget.symm
      _ = ((timed.execution.trace origin.start.startedAt).validatorState
            origin.start.validator).highestSignedRound + 1 :=
        timerSource.targetIsExactNextAtStart origin.start origin.timerStarted
      _ = ((timed.execution.trace origin.start.startedAt).validatorState
            validator).highestSignedRound + 1 := by
        rw [origin.startValidator]
  have observationBeforeTimer : observation < origin.start.startedAt := by
    apply Nat.lt_of_not_ge
    intro timerAtOrBeforeObservation
    have floorMonotone :=
      (timed.execution.durableStateMonotone validator origin.start.startedAt
        observation validatorInRange timerAtOrBeforeObservation).2.2.2.2.2.2.1
    have targetAtMostObservedFloor : targetRound ≤
        ((timed.execution.trace observation).validatorState
          validator).highestSignedRound + 1 := by
      rw [targetAtTimerStart]
      exact Nat.add_le_add_right floorMonotone 1
    have blockAtMostObservedFloor : block.reference.round ≤
        ((timed.execution.trace observation).validatorState
          validator).highestSignedRound + 1 := by
      rw [blockRound]
      exact targetAtMostObservedFloor
    exact (Nat.not_le_of_gt blockFresh) blockAtMostObservedFloor
  have activeTimer : ValidatorEpochActiveBetween timed.execution.trace
      origin.start.startedAt (origin.start.deadline waits) := by
    intro time timerBeforeTime _timeBeforeDeadline
    exact active time (Nat.le_trans (Nat.le_of_lt observationBeforeTimer)
      timerBeforeTime)
  let receiver := v2RoundCatchupOtherReceiver validator
  have receiverInRange : receiver < config.authorityCount :=
    v2_round_catchup_other_receiver_in_range authorityCountAtLeastTwo
  have receiverDifferent : receiver ≠ validator :=
    v2_round_catchup_other_receiver_is_different
  rcases current_timer_snapshot_origin_gives_timer_paced_round_production
      latchSource effects origin activeTimer receiverInRange receiverDifferent
    with ⟨production, productionBlock, productionPersistence,
      productionTimer, productionSnapshot⟩
  have observationBeforeSnapshot : observation <
      production.snapshot.snapshotAt := by
    rw [productionSnapshot]
    exact Nat.lt_of_lt_of_le observationBeforeTimer (by
      simp [ValidatorRecoveryTimerStart.deadline])
  have blockAboveObservedFloor :
      ((timed.execution.trace observation).validatorState
          validator).highestSignedRound < block.reference.round := by
    omega
  rcases persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrect observationBeforePersist blockAboveObservedFloor
          persisted with ⟨broadcast⟩
  let exactBroadcast : ValidatorExactRoundBroadcastProduction timed obligations
      observation validator block.reference.round :=
    { production := broadcast.1
      exactRound := by rw [broadcast.2.2] }
  rw [← blockRound]
  exact ⟨
    { broadcast := exactBroadcast
      production
      exactBlock := productionBlock.trans broadcast.2.2.symm
      exactPersistence := productionPersistence.trans broadcast.2.1.symm
      timerAfterObservation := by
        simpa only [productionTimer] using observationBeforeTimer
      snapshotAfterObservation := observationBeforeSnapshot }⟩

/-- A finite family of actual fresh timer-paced productions.

The round at `offset` is `baseRound + offset + 2`. This definition matches the
family used by the fixed-reference direct-vote consumer. It does not include
common retention, receiver acceptance, a future carrier, or a commit result.
-/
structure ValidatorV2BackfilledTimerPacedWindow
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (observation baseRound count : Time) where
  freshAt : ValidatorFreshTimerPacedExactRoundFamily timed obligations waits
    observation baseRound count

namespace ValidatorV2BackfilledTimerPacedWindow

/-- Erase the V2 source label and keep the minimal production family. -/
theorem toFreshFamily
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation baseRound count : Time}
    (window : ValidatorV2BackfilledTimerPacedWindow timed obligations waits
      observation baseRound count) :
    ValidatorFreshTimerPacedExactRoundFamily timed obligations waits observation
      baseRound count :=
  window.freshAt

end ValidatorV2BackfilledTimerPacedWindow

/-- V2 unbounded own-block production and past-only no-skip provenance give a
finite exact-round production family.

The premise `baseAboveObservedFloors` gives one unused warm-up round. Thus, all
returned timer starts and proposal snapshots are strictly after `observation`.
-/
theorem block_production_liveness_gives_backfilled_timer_paced_window
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorV2RoundCatchupSourceMap timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    (productionLiveness : BlockProductionLiveness config faults network
      timed.execution.trace)
    {observation baseRound count : Time}
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (baseAboveObservedFloors : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound ≤ baseRound) :
    Nonempty (ValidatorV2BackfilledTimerPacedWindow timed obligations waits
      observation baseRound count) := by
  refine ⟨{ freshAt := ?_ }⟩
  intro offset offsetInRange validator validatorInRange validatorCorrect
  rcases productionLiveness validator observation (baseRound + count + 1)
      validatorInRange validatorCorrect afterGst active with
    ⟨finish, highRound, observationBeforeFinish, windowEndAtMostHigh,
      _highFresh, highOwned, _highSent⟩
  cases owned :
      ((timed.execution.trace finish).validatorState validator).ownBlockAt
        highRound with
  | none => simp [owned] at highOwned
  | some highReference =>
      apply actual_high_own_block_gives_fresh_timer_paced_intermediate source
        latchSource effects authorityCountAtLeastTwo validatorInRange
          validatorCorrect observationBeforeFinish (by
            exact Nat.le_trans (by omega) windowEndAtMostHigh) (by
            have baseFloor := baseAboveObservedFloors validator validatorInRange
              validatorCorrect
            omega) owned active

end Mysticeti
