/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorTimedExecution

namespace Mysticeti

/-! Local commit progress recovery mode.

The last commit-install time and the normal threshold-clock proposal round
control this mode. The optional `recovery` field in `ValidatorLocalState` stores
one armed target timer. It does not store the mode latch.
-/

/-- The next legal proposal round while commit progress recovery is active.

Genesis starts at round one. A signer floor at or below positive GC uses one
normal safe-resume proposal at `gcRound + 2`. Later recovery proposals use the
round after the durable signer floor. -/
def ValidatorCommitProgressProposalRound
    {BlockId CommitId : Type}
    (state : ValidatorLocalState BlockId CommitId) : Nat :=
  if state.highestSignedRound ≤ state.gcRound then
    if state.gcRound = 0 then 1 else state.gcRound + 2
  else state.highestSignedRound + 1

/-- One validator is in commit progress recovery after its local commit-stall
deadline expires. -/
def ValidatorCommitProgressRecoveryModeAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (recoveryWait time validator : Time) : Prop :=
  (timed.execution.trace time).epochActive = true ∧
    ((timed.execution.trace time).validatorState validator).lastCommitTime +
        recoveryWait ≤
      ((timed.execution.trace time).validatorState validator).clock

/-- The two local round-gap thresholds and the persisted commit-stall wait.

The smaller exit threshold gives hysteresis. A host does not leave recovery
when only one of the round-gap or time-gap signals recovers. -/
structure ValidatorBlockProgressRecoveryThresholds where
  recoveryWait : Time
  proposalGapEnter : Nat
  proposalGapExit : Nat
  proposalGapExitLtEnter : proposalGapExit < proposalGapEnter

/-- The current normal next-proposal round is at least one configured distance
ahead of the installed commit leader round.

This predicate uses only the current state of one host. The addition form does
not truncate when the commit round is greater than the proposal target. The
normal threshold-clock round is only the recovery signal. It is not the exact
proposal target used after recovery starts. -/
def ValidatorProposalRoundGapRecoveryAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (proposalGap time validator : Time) : Prop :=
  (timed.execution.trace time).epochActive = true ∧
    let state := (timed.execution.trace time).validatorState validator
    state.commitHead.round + proposalGap ≤
      state.thresholdClockRound

/-- An inactive host enters recovery when either the large round gap or the
persisted commit-stall deadline is present. -/
def ValidatorBlockProgressRecoveryEntryAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (thresholds : ValidatorBlockProgressRecoveryThresholds)
    (time validator : Time) : Prop :=
  ValidatorCommitProgressRecoveryModeAt timed thresholds.recoveryWait time
      validator ∨
    ValidatorProposalRoundGapRecoveryAt timed thresholds.proposalGapEnter time
      validator

/-- An active host stays in recovery while either the smaller round gap or the
persisted commit-stall deadline remains. -/
def ValidatorBlockProgressRecoveryStayAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (thresholds : ValidatorBlockProgressRecoveryThresholds)
    (time validator : Time) : Prop :=
  ValidatorCommitProgressRecoveryModeAt timed thresholds.recoveryWait time
      validator ∨
    ValidatorProposalRoundGapRecoveryAt timed thresholds.proposalGapExit time
      validator

/-- Both local recovery signals are below their exit thresholds. -/
def ValidatorBlockProgressRecoveryExitReadyAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (thresholds : ValidatorBlockProgressRecoveryThresholds)
    (time validator : Time) : Prop :=
  (timed.execution.trace time).epochActive = true ∧
    let state := (timed.execution.trace time).validatorState validator
    state.thresholdClockRound <
        state.commitHead.round + thresholds.proposalGapExit ∧
      state.clock < state.lastCommitTime + thresholds.recoveryWait

/-- One host's latched recovery mode follows the entry and exit thresholds.

The Boolean trace maps the implementation mode bit. The transition law is
local. It does not state that a future proposal, block, quorum, or commit
exists. -/
structure ValidatorBlockProgressRecoveryModeExecution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (thresholds : ValidatorBlockProgressRecoveryThresholds) where
  active : Time → Nat → Bool
  transitionsFollowHysteresis : ∀ time validator,
    active (time + 1) validator = true ↔
      (active time validator = true ∧
        ValidatorBlockProgressRecoveryStayAt timed thresholds (time + 1)
          validator) ∨
      (active time validator = false ∧
        ValidatorBlockProgressRecoveryEntryAt timed thresholds (time + 1)
          validator)

/-- The implementation's latched block-progress recovery mode is active at one
host and time. -/
def ValidatorBlockProgressRecoveryModeAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    (time validator : Time) : Prop :=
  mode.active time validator = true

/-- The entry threshold is stronger than the stay threshold. -/
theorem block_progress_recovery_entry_implies_stay
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {time validator : Time}
    (entry : ValidatorBlockProgressRecoveryEntryAt timed thresholds time
      validator) :
    ValidatorBlockProgressRecoveryStayAt timed thresholds time validator := by
  rcases entry with timedOut | roundGap
  · exact Or.inl timedOut
  · refine Or.inr ⟨roundGap.1, ?_⟩
    have exitLeEnter : thresholds.proposalGapExit ≤
        thresholds.proposalGapEnter :=
      Nat.le_of_lt thresholds.proposalGapExitLtEnter
    exact Nat.le_trans
      (Nat.add_le_add_left exitLeEnter
        ((timed.execution.trace time).validatorState
          validator).commitHead.round)
      roundGap.2

/-- A current entry signal activates an inactive host or keeps an active host
in recovery on the next local transition. -/
theorem block_progress_recovery_entry_activates_or_keeps_mode
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    {time validator : Time}
    (entry : ValidatorBlockProgressRecoveryEntryAt timed thresholds (time + 1)
      validator) :
    ValidatorBlockProgressRecoveryModeAt mode (time + 1) validator := by
  rw [ValidatorBlockProgressRecoveryModeAt,
    mode.transitionsFollowHysteresis]
  cases activeNow : mode.active time validator with
  | false => exact Or.inr ⟨rfl, entry⟩
  | true => exact Or.inl ⟨rfl,
      block_progress_recovery_entry_implies_stay entry⟩

/-- Recovery stays active across one local transition while either exit signal
remains. This rule also applies to a commit-install transition. -/
theorem block_progress_recovery_persists_while_either_signal_remains
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    {time validator : Time}
    (active : ValidatorBlockProgressRecoveryModeAt mode time validator)
    (stay : ValidatorBlockProgressRecoveryStayAt timed thresholds (time + 1)
      validator) :
    ValidatorBlockProgressRecoveryModeAt mode (time + 1) validator := by
  rw [ValidatorBlockProgressRecoveryModeAt,
    mode.transitionsFollowHysteresis]
  exact Or.inl ⟨active, stay⟩

/-- A commit update cannot clear recovery while the post-update round gap is
at least the smaller exit threshold. The commit index can advance by any
amount; only the post-state round inequality matters. -/
theorem block_progress_recovery_persists_while_round_gap_remains
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    {time validator : Time}
    (active : ValidatorBlockProgressRecoveryModeAt mode time validator)
    (gapRemains : ValidatorProposalRoundGapRecoveryAt timed
      thresholds.proposalGapExit (time + 1) validator) :
    ValidatorBlockProgressRecoveryModeAt mode (time + 1) validator :=
  block_progress_recovery_persists_while_either_signal_remains mode active
    (Or.inr gapRemains)

/-- Recovery cannot clear while the post-update commit-stall deadline remains,
even if the round gap is below its exit threshold. -/
theorem block_progress_recovery_persists_while_time_gap_remains
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    {time validator : Time}
    (active : ValidatorBlockProgressRecoveryModeAt mode time validator)
    (timeGapRemains : ValidatorCommitProgressRecoveryModeAt timed
      thresholds.recoveryWait (time + 1) validator) :
    ValidatorBlockProgressRecoveryModeAt mode (time + 1) validator :=
  block_progress_recovery_persists_while_either_signal_remains mode active
    (Or.inl timeGapRemains)

/-- Recovery can deactivate only when both the round gap and the time gap are
below their exit thresholds. -/
theorem block_progress_recovery_deactivation_requires_both_recovered
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    {time validator : Time}
    (active : ValidatorBlockProgressRecoveryModeAt mode time validator)
    (inactiveAfter : mode.active (time + 1) validator = false)
    (epochActiveAfter :
      (timed.execution.trace (time + 1)).epochActive = true) :
    ValidatorBlockProgressRecoveryExitReadyAt timed thresholds (time + 1)
      validator := by
  have notStay : ¬ValidatorBlockProgressRecoveryStayAt timed thresholds
      (time + 1) validator := by
    intro stay
    have activeAfter : mode.active (time + 1) validator = true := by
      rw [mode.transitionsFollowHysteresis]
      exact Or.inl ⟨active, stay⟩
    simp_all
  have noTimeGap : ¬ValidatorCommitProgressRecoveryModeAt timed
      thresholds.recoveryWait (time + 1) validator := by
    intro timedOut
    exact notStay (Or.inl timedOut)
  have noRoundGap : ¬ValidatorProposalRoundGapRecoveryAt timed
      thresholds.proposalGapExit (time + 1) validator := by
    intro roundGap
    exact notStay (Or.inr roundGap)
  refine ⟨epochActiveAfter, ?_, ?_⟩
  · apply Nat.lt_of_not_ge
    intro notBelow
    apply noRoundGap
    exact ⟨epochActiveAfter, notBelow⟩
  · apply Nat.lt_of_not_ge
    intro notBelow
    apply noTimeGap
    exact ⟨epochActiveAfter, notBelow⟩

/-- Recovery exits on the next local transition when both signals are below
their exit thresholds. -/
theorem block_progress_recovery_exits_when_both_recovered
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    {time validator : Time}
    (_active : ValidatorBlockProgressRecoveryModeAt mode time validator)
    (exitReady : ValidatorBlockProgressRecoveryExitReadyAt timed thresholds
      (time + 1) validator) :
    mode.active (time + 1) validator = false := by
  rcases exitReady with ⟨_epochActiveAfter, roundBelow, timeBelow⟩
  have noStay : ¬ValidatorBlockProgressRecoveryStayAt timed thresholds
      (time + 1) validator := by
    intro stay
    rcases stay with timedOut | roundGap
    · exact (Nat.not_le_of_lt timeBelow) timedOut.2
    · exact (Nat.not_le_of_lt roundBelow) roundGap.2
  cases activeAfter : mode.active (time + 1) validator with
  | false => rfl
  | true =>
      have transition := (mode.transitionsFollowHysteresis time validator).mp
        activeAfter
      rcases transition with current | entered
      · exact False.elim (noStay current.2)
      · exact False.elim (noStay
          (block_progress_recovery_entry_implies_stay entered.2))

/-- Any proposal persisted while the combined recovery mode is active uses the
single recovery proposal round derived from the same current host state. -/
structure ValidatorBlockProgressProposalRoundRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds) :
    Prop where
  persistedProposalUsesRecoveryRound : ∀ time validator block,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorBlockProgressRecoveryModeAt mode time validator →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.persistProposal block) →
    block.reference.round = ValidatorCommitProgressProposalRound
      ((timed.execution.trace time).validatorState validator)

/-- Recovery mode stays active while the epoch and the persisted last-commit
time stay unchanged. -/
theorem recovery_mode_persists_with_stable_last_commit
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {recoveryWait validator earlier later : Time}
    (validatorInRange : validator < config.authorityCount)
    (ordered : earlier ≤ later)
    (activeAtEarlier :
      ValidatorCommitProgressRecoveryModeAt timed recoveryWait earlier
        validator)
    (epochActiveAtLater :
      (timed.execution.trace later).epochActive = true)
    (sameLastCommitTime :
      ((timed.execution.trace later).validatorState validator).lastCommitTime =
        ((timed.execution.trace earlier).validatorState
          validator).lastCommitTime) :
    ValidatorCommitProgressRecoveryModeAt timed recoveryWait later validator := by
  refine ⟨epochActiveAtLater, ?_⟩
  rw [sameLastCommitTime]
  exact Nat.le_trans activeAtEarlier.2
    (timed.execution.clocksMonotone validator earlier later validatorInRange
      ordered)

end Mysticeti
