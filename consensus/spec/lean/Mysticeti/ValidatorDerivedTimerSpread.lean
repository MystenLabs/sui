/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorAdjacentRecoveryPropagation
import Mysticeti.ValidatorRoundFrontierBridge

namespace Mysticeti

/-! Derived timer-start envelopes for adjacent recovery rounds.

This module keeps timer spread as an internal theorem result. One current common
arm state gives each correct, available validator an actual timer-paced proposal
whose timer starts in the same finite interval. The interval gives the pairwise
start bound used by adjacent parent propagation.

`ValidatorRecoveryTimerArmInputAt` is intentionally required below. A common
parent-ready state gives only `ValidatorRecoveryTimerCurrentInputAt`: some hosts
can already have a stored timer whose origin predates the observation. Current
timer-source rules do not bound that earlier origin from a new common state. A
concrete catch-up rule must either give that origin a common lower bound or
classify the host's earlier exact production. No future spread, block layer, or
receiver-completion result is introduced here.
-/

/-- A deterministic peer used only to obtain one concrete broadcast witness. -/
private def derivedTimerOtherReceiver (validator : Nat) : Nat :=
  if validator = 0 then 1 else 0

private theorem derived_timer_other_receiver_in_range
    {validator authorityCount : Nat}
    (authorityCountAtLeastTwo : 1 < authorityCount) :
    derivedTimerOtherReceiver validator < authorityCount := by
  simp only [derivedTimerOtherReceiver]
  split
  · exact authorityCountAtLeastTwo
  · omega

private theorem derived_timer_other_receiver_is_different
    {validator : Nat} :
    derivedTimerOtherReceiver validator ≠ validator := by
  simp only [derivedTimerOtherReceiver]
  split <;> omega

/-- The local timer-arm cost is also the spread bound when all timer arms start
from one common current state. -/
def validatorRecoveryTimerStartSpreadBound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) : Time :=
  timed.localActionBound + 2

/-- Every correct, available validator has one actual timer-paced production in
the same timer-start interval. This predicate is a theorem result, not a
liveness input. -/
def EveryCorrectAvailableValidatorTimerStartEnvelope
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (readyAt round spread : Time) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator round,
      readyAt ≤ production.timerStartedAt ∧
        production.timerStartedAt ≤ readyAt + spread

/-- The pairwise form of one timer-start envelope. The productions remain
explicit so a higher theorem can use their exact proposal snapshots. -/
def EveryCorrectAvailableValidatorTimerStartSpread
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (round spread : Time) : Prop :=
  ∀ left right,
    left < config.authorityCount →
    faults.correctAvailable left = true →
    right < config.authorityCount →
    faults.correctAvailable right = true →
    ∃ leftProduction : ValidatorTimerPacedRoundProduction timed waits left round,
      ∃ rightProduction : ValidatorTimerPacedRoundProduction timed waits right round,
        leftProduction.timerStartedAt ≤
          rightProduction.timerStartedAt + spread

/-- One common fresh arm state produces the same exact timer-paced round at
every correct, available validator and preserves the actual timer-start bound.

This strengthens `common_recovery_timer_inputs_give_commit_advance_or_timer_paced_round`,
whose current result erases the upper timer-start bound. -/
theorem common_recovery_timer_inputs_give_commit_advance_or_timer_start_envelope
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {readyAt round : Time}
    (inputs : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ValidatorRecoveryTimerArmInputAt timed readyAt validator)
    (sameTarget : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace readyAt).validatorState
        validator).highestSignedRound + 1 = round)
    (active : ∀ later, readyAt ≤ later →
      (timed.execution.trace later).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed readyAt ∨
      EveryCorrectAvailableValidatorTimerStartEnvelope timed waits readyAt round
        (validatorRecoveryTimerStartSpreadBound timed) := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed readyAt
  · exact Or.inl advanced
  · right
    intro validator validatorInRange validatorCorrect
    have pointwise := ready_state_builds_strict_recovery_broadcast_or_commit_advance
      timed timerSource pacing arms latchSource effects readyAt validator
      (derivedTimerOtherReceiver validator)
      (inputs validator validatorInRange validatorCorrect) active
      (derived_timer_other_receiver_in_range authorityCountAtLeastTwo)
      derived_timer_other_receiver_is_different
    rcases pointwise with
      ⟨finish, readyBeforeFinish, localAdvance⟩ |
        ⟨result, targetAtInput, readyBeforeTimer, timerBound⟩
    · exact False.elim (advanced ⟨validator, finish, validatorInRange,
        validatorCorrect, readyBeforeFinish, localAdvance⟩)
    · have targetRound : result.targetRound = round :=
        targetAtInput.trans
          (sameTarget validator validatorInRange validatorCorrect)
      rcases strict_recovery_broadcast_gives_timer_paced_round_production
          result targetRound with
        ⟨production, _parentReady, timerStarted, _persistAt, _sentAt⟩
      refine ⟨production, ?_, ?_⟩
      · simpa only [timerStarted] using readyBeforeTimer
      · simpa [validatorRecoveryTimerStartSpreadBound, timerStarted,
          Nat.add_assoc] using timerBound

/-- A common timer-start envelope derives pairwise correct-validator spread.
The spread is not a caller-provided execution fact. -/
theorem timer_start_envelope_gives_pairwise_spread
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
    {readyAt round spread : Time}
    (envelope : EveryCorrectAvailableValidatorTimerStartEnvelope timed waits
      readyAt round spread) :
    EveryCorrectAvailableValidatorTimerStartSpread timed waits round spread := by
  intro left right leftInRange leftCorrect rightInRange rightCorrect
  rcases envelope left leftInRange leftCorrect with
    ⟨leftProduction, _readyBeforeLeft, leftBound⟩
  rcases envelope right rightInRange rightCorrect with
    ⟨rightProduction, readyBeforeRight, _rightBound⟩
  refine ⟨leftProduction, rightProduction, ?_⟩
  exact Nat.le_trans leftBound (by
    simpa [Nat.add_comm] using Nat.add_le_add_right readyBeforeRight spread)

/-- Consecutive exact recovery targets at one receiver order their current
ready observations. The result follows from the durable signer-floor
chronology; no time order is an input. -/
theorem consecutive_receiver_recovery_targets_give_ready_order
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {receiver previousReady nextReady round : Time}
    (receiverInRange : receiver < config.authorityCount)
    (previousTarget :
      ((timed.execution.trace previousReady).validatorState
        receiver).highestSignedRound + 1 = round)
    (nextTarget :
      ((timed.execution.trace nextReady).validatorState
        receiver).highestSignedRound + 1 = round + 1) :
    previousReady ≤ nextReady := by
  rcases Nat.le_total previousReady nextReady with ordered | reverse
  · exact ordered
  · have floorMonotone :=
      (timed.execution.durableStateMonotone receiver nextReady previousReady
        receiverInRange reverse).2.2.2.2.2.2.1
    omega

/-- Current local cutoff provenance for the exact parent list of one fresh
timer generation.

`cutoffParents` is an accepted and retained list at `receiver` when the timer
generation starts. A higher common-state proof supplies this local fact at
each correct receiver. The subset field is the smallest fact which the current
causal-closure/frontier source does not derive. In particular,
`initial_quorum_parent_is_in_refreshed_deadline` proves the other inclusion:
the initial quorum remains in the refreshed list. Current refresh rules can add
a newly accepted non-common branch, so they do not prove that the refreshed
list stays inside this common cutoff. This record has no future delivery,
layer, or receiver-completion field. -/
structure ValidatorAdjacentFreshParentCutoffAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time receiver round : Time)
    (parents : List (ValidatorBlockRef BlockId)) : Type where
  cutoffParents : List (ValidatorBlockRef BlockId)
  cutoffReady : ValidatorProposalParentListReady
    .commitProgressRecovery config
      ((timed.execution.trace time).validatorState receiver) round cutoffParents
  refreshedParentsFromCutoff : ∀ parent,
    parent ∈ parents → parent ∈ cutoffParents

/-- An exact operational-frontier quorum and a refresh-cutoff relation form the
local fresh-parent source used by adjacent propagation. -/
def operational_frontier_quorum_gives_adjacent_fresh_parent_cutoff
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time receiver round : Time}
    {parents : List (ValidatorBlockRef BlockId)}
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontier : ValidatorOperationalQuorumFrontierAt config
      (timed.execution.trace time) receiver (round - 1)
        canonicalGenesisParents)
    (roundPositive : 0 < round)
    (parentsFromFrontier : ∀ parent,
      parent ∈ parents → parent ∈ frontier.quorum.references) :
    ValidatorAdjacentFreshParentCutoffAt timed time receiver round parents := by
  refine {
    cutoffParents := frontier.quorum.references
    cutoffReady := ?_
    refreshedParentsFromCutoff := parentsFromFrontier }
  simpa only [ValidatorProposalParentListReady,
    Nat.sub_add_cancel (Nat.succ_le_iff.mpr roundPositive)] using
      frontier.successorParentListReady

/-- The local refresh cutoff supplies exact parent acceptance at its current
trace state. -/
theorem adjacent_fresh_parent_cutoff_gives_accepted
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time receiver round : Time}
    {parents : List (ValidatorBlockRef BlockId)}
    (source : ValidatorAdjacentFreshParentCutoffAt timed time receiver
      round parents) :
    ∀ parent, parent ∈ parents →
      ((timed.execution.trace time).validatorState receiver).accepted parent =
        true := by
  intro parent parentMember
  exact (source.cutoffReady.1.2.1 parent
    (source.refreshedParentsFromCutoff parent parentMember)).2

/-- Ordered common ready states turn the previous timer's envelope into the
start-difference bound consumed by adjacent propagation. -/
theorem consecutive_ready_envelopes_give_previous_start_bound
    {previousReady nextReady previousStart nextStart spread : Time}
    (readyOrder : previousReady ≤ nextReady)
    (previousBound : previousStart ≤ previousReady + spread)
    (nextStartsAfterReady : nextReady ≤ nextStart) :
    previousStart ≤ nextStart + spread := by
  exact Nat.le_trans previousBound
    (Nat.le_trans (Nat.add_le_add_right readyOrder spread)
      (Nat.add_le_add_right nextStartsAfterReady spread))

/-- The adjacent parent-evidence theorem with both ready-state chronology and
timer-start difference derived from current state.

The fresh-parent cutoff is local receiver state. Its exact-list provenance is
the remaining implementation rule: the proposal's refreshed parents must stay
inside an already common accepted and retained arm-time list. It is not a
future receiver-completion input. -/
theorem adjacent_timer_paced_productions_give_parent_evidence_from_ready_envelopes_or_commit_advance
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {previousReady nextReady author receiver round : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (previousTargetAtReceiver :
      ((timed.execution.trace previousReady).validatorState
        receiver).highestSignedRound + 1 = round)
    (nextTargetAtReceiver :
      ((timed.execution.trace nextReady).validatorState
        receiver).highestSignedRound + 1 = round + 1)
    (previousStartsAfterReady :
      previousReady ≤ previous.timerStartedAt)
    (previousStartsWithinEnvelope :
      previous.timerStartedAt ≤
        previousReady + validatorRecoveryTimerStartSpreadBound timed)
    (nextStartsAfterReady : nextReady ≤ next.timerStartedAt)
    (receiverHeadAtStart :
      ((timed.execution.trace previousReady).validatorState receiver).commitHead =
        receiverPrior)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (referenceAfterPrior : receiverPrior.round < round)
    (visibilityMargin :
      waits.wait previousPrior round +
          (validatorRecoveryTimerStartSpreadBound timed +
            3 * (timed.localActionBound + 1) + network.delta +
            3 * (timed.localActionBound + 1)) ≤
        waits.wait receiverPrior (round + 1))
    (previousReadyAfterGst : network.gst ≤ previousReady)
    (active : ∀ time, previousReady ≤ time →
      (timed.execution.trace time).epochActive = true)
    (previousParentCutoff : ValidatorAdjacentFreshParentCutoffAt timed
      previousReady receiver round previous.snapshot.block.parents) :
    SomeCorrectAvailableCommitAdvance timed previousReady ∨
      ValidatorAdjacentTimerPacedParentEvidence previous next := by
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.proposer] using next.snapshot.proposerInRange
  have readyOrder : previousReady ≤ nextReady :=
    consecutive_receiver_recovery_targets_give_ready_order receiverInRange
      previousTargetAtReceiver nextTargetAtReceiver
  have commonStartBeforeNext : previousReady ≤ next.timerStartedAt :=
    Nat.le_trans readyOrder nextStartsAfterReady
  have previousStartBound :
      previous.timerStartedAt ≤
        next.timerStartedAt + validatorRecoveryTimerStartSpreadBound timed :=
    consecutive_ready_envelopes_give_previous_start_bound readyOrder
      previousStartsWithinEnvelope nextStartsAfterReady
  have previousStartsAfterGst : network.gst ≤ previous.timerStartedAt :=
    Nat.le_trans previousReadyAfterGst previousStartsAfterReady
  have previousParentsAcceptedAtStart :=
    adjacent_fresh_parent_cutoff_gives_accepted previousParentCutoff
  exact adjacent_timer_paced_productions_give_parent_evidence_or_commit_advance
    acceptance sync representatives previous next previousStartsAfterReady
    commonStartBeforeNext receiverHeadAtStart previousHead nextHead
    referenceAfterPrior previousStartBound visibilityMargin
    previousStartsAfterGst active previousParentsAcceptedAtStart

end Mysticeti
