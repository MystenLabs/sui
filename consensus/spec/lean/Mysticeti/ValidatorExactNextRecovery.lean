/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCausalExactNext
import Mysticeti.ValidatorNormalBlockLiveness
import Mysticeti.ValidatorOriginAwareTraceBridge
import Mysticeti.ValidatorRecoveryParentNeedExecution
import Mysticeti.ValidatorRecoveryCapsuleSyncExecution
import Mysticeti.ValidatorRecoveryMode
import Mysticeti.ValidatorRecoverySourcePinExecution

namespace Mysticeti

/-! Strict exact-next recovery on the main validator execution.

The local stall clock supplies an active-recovery snapshot. This file selects the
maximum correct-validator signer floor only inside the proof. It then chooses
the first requested round above that maximum. Thus, every block in the result
has a concrete proposal-persistence event in the modeled execution suffix.
-/

/-- A deterministic different receiver for a validator set with at least two
members. It is used only to obtain one concrete send witness. -/
private def recoveryOtherReceiver (validator : Nat) : Nat :=
  if validator = 0 then 1 else 0

private theorem recovery_other_receiver_in_range
    {validator authorityCount : Nat}
    (authorityCountAtLeastTwo : 1 < authorityCount) :
    recoveryOtherReceiver validator < authorityCount := by
  simp only [recoveryOtherReceiver]
  split
  · exact authorityCountAtLeastTwo
  · omega

private theorem recovery_other_receiver_is_different
    {validator : Nat} :
    recoveryOtherReceiver validator ≠ validator := by
  simp only [recoveryOtherReceiver]
  split <;> omega

private theorem recovery_peer_send_bound_arithmetic
    (proposalActionAt localActionBound : Nat) :
    (proposalActionAt + 1 + localActionBound) +
        (1 + localActionBound + 1) =
      proposalActionAt + 2 * (localActionBound + 1) + 1 := by
  rw [Nat.mul_add, Nat.mul_one, Nat.two_mul]
  omega

private theorem recovery_exact_persistence_bound_arithmetic
    (proposalActionAt timerStartedAt wait localActionBound : Nat)
    (proposalWithinBound :
      proposalActionAt ≤ timerStartedAt + wait + localActionBound) :
    (proposalActionAt + 1 + localActionBound) + 1 ≤
      timerStartedAt + wait + 2 * (localActionBound + 1) := by
  have shifted := Nat.add_le_add_right proposalWithinBound
    (1 + localActionBound + 1)
  calc
    (proposalActionAt + 1 + localActionBound) + 1 =
        proposalActionAt + (1 + localActionBound + 1) := by omega
    _ ≤ (timerStartedAt + wait + localActionBound) +
          (1 + localActionBound + 1) := shifted
    _ = timerStartedAt + wait + 2 * (localActionBound + 1) := by
      rw [Nat.mul_add, Nat.mul_one, Nat.two_mul]
      omega

private theorem recovery_exact_send_bound_arithmetic
    (persistTime proposalActionAt localActionBound : Nat)
    (persistenceWithinBound :
      persistTime ≤ proposalActionAt + 1 + localActionBound) :
    (persistTime + 1 + localActionBound) + 1 ≤
      proposalActionAt + 2 * (localActionBound + 1) + 1 := by
  have shifted := Nat.add_le_add_right persistenceWithinBound
    (1 + localActionBound + 1)
  calc
    (persistTime + 1 + localActionBound) + 1 =
        persistTime + (1 + localActionBound + 1) := by omega
    _ ≤ (proposalActionAt + 1 + localActionBound) +
          (1 + localActionBound + 1) := shifted
    _ = proposalActionAt + 2 * (localActionBound + 1) + 1 :=
      recovery_peer_send_bound_arithmetic _ _

private theorem recovery_round_pipeline_arithmetic
    (proposalActionAt timerStartedAt wait localActionBound : Nat)
    (proposalWithinBound :
      proposalActionAt ≤ timerStartedAt + wait + localActionBound) :
    proposalActionAt + 2 * (localActionBound + 1) + 1 ≤
      timerStartedAt + wait + 3 * (localActionBound + 1) := by
  have shifted := Nat.add_le_add_right proposalWithinBound
    (2 * (localActionBound + 1) + 1)
  calc
    proposalActionAt + 2 * (localActionBound + 1) + 1 =
        proposalActionAt + (2 * (localActionBound + 1) + 1) := by omega
    _ ≤ (timerStartedAt + wait + localActionBound) +
          (2 * (localActionBound + 1) + 1) := shifted
    _ = timerStartedAt + wait + 3 * (localActionBound + 1) := by
      simp only [Nat.mul_add, Nat.mul_one, Nat.succ_mul, Nat.zero_mul]
      omega

/-- The derived post-GST state at which every correct, available validator is
in commit progress recovery mode. The per-target timer can be empty or armed. -/
structure ValidatorActiveRecoverySnapshot
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (recoveryWait time : Time) : Prop where
  afterGst : network.gst ≤ time
  recovering : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorCommitProgressRecoveryModeAt timed recoveryWait time validator

/-- A common proof time at which every correct, available host has latched the
combined time-gap or round-gap recovery mode. The time is not selected or
announced by the hosts. -/
structure ValidatorActiveBlockProgressRecoverySnapshot
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
    (time : Time) : Prop where
  afterGst : network.gst ≤ time
  recovering : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorBlockProgressRecoveryModeAt mode time validator

/-- Local parent-work refinement for the new block-progress recovery mode.

When a host enters recovery with an older normal need at or below its GC root,
it refreshes that need to the current canonical safe-resume target and current
local representative pool. This is one host-state invariant. It does not state
that a proposal, packet, layer, or commit will occur. -/
structure ValidatorBlockProgressRecoveryNeedRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait) :
    Prop where
  activeNormalNeedIsCurrentSafeResume : ∀ time validator need,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorBlockProgressRecoveryModeAt mode time validator →
    (needs.trace time validator).active = some need →
    need.proposalOrigin = .normal →
    ((timed.execution.trace time).validatorState
      validator).highestSignedRound ≤
        ((timed.execution.trace time).validatorState validator).gcRound →
    ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator need

/-- One actual commit install does not change the normal proposal clock.

This is the only new commit-to-proposal refinement. Existing installed-parent
rules preserve a legal above-GC DAG frontier. Existing parent-need rules
preserve exact normal work and protect its proposal action. Bounded execution
preserves later persistence and send work. The GC-crossing case uses the
separate safe-resume constructor.

This record contains no future proposal, packet, layer, window, or commit
result. -/
structure ValidatorCommitProposalNonInterferenceRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    : Prop where
  thresholdClockRoundPreserved : ∀ time validator head,
    ValidatorCommitInstallOccurs (timed.execution.events time) validator head →
    ((timed.execution.trace (time + 1)).validatorState
        validator).thresholdClockRound =
      ((timed.execution.trace time).validatorState
        validator).thresholdClockRound

/-- Any protected persistence or send action either runs in the commit batch or
remains protected in the next state.

This fact comes from bounded protected execution. It is not an additional
commit-interference assumption. -/
theorem protected_action_runs_or_survives_one_batch
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time validator : Time}
    {action : ValidatorLocalAction BlockId CommitId}
    (actionProtected : timed.protectedAction time validator action) :
    ValidatorLocalActionOccurs (timed.execution.events time) validator action ∨
      timed.protectedAction (time + 1) validator action := by
  by_cases runs : ValidatorLocalActionOccurs (timed.execution.events time)
      validator action
  · exact Or.inl runs
  · right
    apply timed.protectedActionPersistsUntilRun time (time + 1) validator action
      actionProtected (Nat.le_succ time)
    intro current timeBeforeCurrent currentBeforeNext occurs
    have currentAtMostTime : current ≤ time := by
      exact Nat.lt_succ_iff.mp (by
        simpa [Nat.succ_eq_add_one] using currentBeforeNext)
    have currentIsTime : current = time :=
      Nat.le_antisymm currentAtMostTime timeBeforeCurrent
    subst current
    exact runs occurs

/-- The past local timer and proposal action that produced one persisted
recovery block.

All times in this record are at or before `persistTime`. The record does not
state that a later persistence, send, layer, or commit occurs. -/
structure ValidatorCurrentTimerProposalOriginAt
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
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (persistTime validator : Time) (block : ValidatorBlock BlockId) where
  start : ValidatorRecoveryTimerStart BlockId CommitId
  parents : List (ValidatorBlockRef BlockId)
  proposalActionAt : Time
  timerStarted : timerSource.timerStarted start
  startValidator : start.validator = validator
  proposalTarget : start.targetRound = block.reference.round
  refreshedParents : timerSource.refreshedParents start
    (start.deadline waits) = parents
  headAtDeadline :
    ((timed.execution.trace (start.deadline waits)).validatorState
      validator).commitHead = start.commitHead
  recoveryAtDeadline :
    ((timed.execution.trace (start.deadline waits)).validatorState
      validator).recovery = some (start.recovery waits)
  deadlineBeforeProposal : start.deadline waits ≤ proposalActionAt
  proposalWithinLocalBound : proposalActionAt ≤
    start.deadline waits + timed.localActionBound
  proposalBeforePersistence : proposalActionAt + 1 ≤ persistTime
  proposalOccurs : ValidatorLocalActionOccurs
    (timed.execution.events proposalActionAt) validator
      (.proposeNext parents)
  persistenceEnabled : ValidatorActionEnabledAt timed.execution
    (proposalActionAt + 1) validator (.persistProposal block)
  blockAuthor : block.reference.author = validator
  blockParents : block.parents = parents
  persistenceOccurs : ValidatorLocalActionOccurs
    (timed.execution.events persistTime) validator (.persistProposal block)

/-- Past-time pacing refinement for an actual `proposeNext` action.

The main action uses the current timer at its stored deadline, and bounded
local execution runs the action inside one local-action bound. This rule does
not assert that a proposal action will occur. -/
structure ValidatorRecoveryProposalActionTimingRules
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
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits) : Prop where
  actualProposalUsesTimerWindow : ∀ time validator parents,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.proposeNext parents) →
    ∀ paced : ValidatorPacedRecoveryProposalOccurrence timerSource time
        validator parents,
      paced.start.deadline waits ≤ time ∧
        time ≤ paced.start.deadline waits + timed.localActionBound ∧
        ((timed.execution.trace (paced.start.deadline waits)).validatorState
          validator).commitHead = paced.start.commitHead

/-- Local origin refinement for proposal persistence in block-progress
recovery.

The normal post-GC safe-resume proposal is permitted only while the durable
signer floor is at or below the local GC round. After the signer floor is above
GC, a persisted ready proposal must have recovery origin. The existing latch
source and pacing rules then identify the concrete `proposeNext` action, its
current timer, and its proposal-time refreshed parent list.

This is one local state-and-action invariant. It does not assert that a future
proposal, packet, layer, or commit exists. Rust must cancel or refresh stale
normal ready work before it can persist in this branch. -/
structure ValidatorBlockProgressProposalOriginRules
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
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds) :
    Prop where
  exactNextPersistenceUsesRecoveryOrigin : ∀ time validator proposal,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorBlockProgressRecoveryModeAt mode time validator →
    ((timed.execution.trace time).validatorState validator).gcRound <
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound →
    (obligations.trace time validator).readyProposal = some proposal →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.persistProposal proposal.block) →
    proposal.origin = .commitProgressRecovery

/-- A past timer/action origin and its actual persistence construct the exact
proposal snapshot at the timer deadline. -/
theorem current_timer_proposal_origin_builds_exact_snapshot
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {persistTime validator : Time} {block : ValidatorBlock BlockId}
    (origin : ValidatorCurrentTimerProposalOriginAt timerSource persistTime
      validator block) :
    ∃ snapshot : ValidatorProposalSnapshot config faults
        timed.execution.trace,
      snapshot.proposer = validator ∧
        snapshot.block = block ∧
        snapshot.snapshotAt = origin.start.deadline waits ∧
        snapshot.storedAt = persistTime + 1 ∧
        ValidatorRefreshedRecoveryParentListAt config
          ((timed.execution.trace snapshot.snapshotAt).validatorState
            validator)
          block.reference.round snapshot.block.parents := by
  have refreshedAtDeadline := timerSource.refreshed_parents_at_deadline
    origin.start origin.timerStarted (by
      simpa [origin.startValidator] using origin.headAtDeadline)
  have refreshedAtDeadline' : ValidatorRefreshedRecoveryParentListAt config
      ((timed.execution.trace (origin.start.deadline waits)).validatorState
        validator)
      block.reference.round origin.parents := by
    rw [origin.startValidator] at refreshedAtDeadline
    rw [origin.proposalTarget] at refreshedAtDeadline
    rw [origin.refreshedParents] at refreshedAtDeadline
    exact refreshedAtDeadline
  let snapshot : ValidatorProposalSnapshot config faults
      timed.execution.trace :=
    { proposer := validator
      snapshotAt := origin.start.deadline waits
      storedAt := persistTime + 1
      block
      proposerInRange := by
        simpa [origin.startValidator] using
          timerSource.validatorInRange origin.start origin.timerStarted
      proposerCorrectAvailable := by
        simpa [origin.startValidator] using
          timerSource.validatorCorrectAvailable origin.start origin.timerStarted
      snapshotBeforeStore := Nat.le_trans origin.deadlineBeforeProposal
        (Nat.le_trans (Nat.le_succ origin.proposalActionAt)
          (Nat.le_trans origin.proposalBeforePersistence (Nat.le_succ _)))
      recoveryTargetsProposalRound := ⟨origin.start.recovery waits,
        origin.recoveryAtDeadline, by
          simpa [ValidatorRecoveryTimerStart.recovery] using
            origin.proposalTarget, rfl⟩
      blockIsOwnProposal := origin.blockAuthor
      blockStored := persist_proposal_occurrence_stores_own_block
        timed.execution origin.persistenceOccurs
      blockInCatalog := effects.persistedProposalStoresBlock persistTime
        validator block origin.persistenceOccurs
      parentAuthorsNodup := by
        change (block.parents.map ValidatorBlockRef.author).Nodup
        rw [origin.blockParents]
        exact refreshedAtDeadline'.ready.1.1
      parentsAreImmediate := by
        intro parent parentIncluded
        have includedAtDeadline : parent ∈
            timerSource.refreshedParents origin.start
              (origin.start.deadline waits) := by
          rw [origin.refreshedParents]
          simpa [origin.blockParents] using parentIncluded
        have immediate :=
          (refreshedAtDeadline'.ready.1.2.1 parent (by
            rw [← origin.refreshedParents]
            exact includedAtDeadline)).1
        exact immediate
      parentsAcceptedAtSnapshot := by
        intro parent parentIncluded
        have includedAtDeadline : parent ∈
            timerSource.refreshedParents origin.start
              (origin.start.deadline waits) := by
          rw [origin.refreshedParents]
          simpa [origin.blockParents] using parentIncluded
        exact (refreshedAtDeadline'.ready.1.2.1 parent (by
          rw [← origin.refreshedParents]
          exact includedAtDeadline)).2 }
  refine ⟨snapshot, rfl, rfl, rfl, rfl, ?_⟩
  simpa [snapshot, origin.blockParents] using refreshedAtDeadline'

/-- An actual above-GC persistence in block-progress recovery has a concrete
timer-paced `proposeNext` origin with its refreshed parent list.

The only new input is the local origin invariant above. The latch and pacing
maps derive all remaining history facts from the actual persistence action. -/
theorem block_progress_exact_next_persistence_has_current_timer_origin
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (originRules : ValidatorBlockProgressProposalOriginRules
      (obligations := obligations) mode)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    {time validator : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (recoveryActive :
      ValidatorBlockProgressRecoveryModeAt mode time validator)
    (floorAboveGc :
      ((timed.execution.trace time).validatorState validator).gcRound <
        ((timed.execution.trace time).validatorState
          validator).highestSignedRound)
    (persisted : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.persistProposal block)) :
    ∃ (proposal : ValidatorReadyProposal BlockId) (latchTime : Time)
        (parents : List (ValidatorBlockRef BlockId)),
      proposal.block = block ∧
        (obligations.trace time validator).readyProposal = some proposal ∧
        proposal.origin = .commitProgressRecovery ∧
        latchTime < time ∧
        ValidatorLocalActionOccurs (timed.execution.events latchTime)
          validator (.proposeNext parents) ∧
        proposal.block.parents = parents ∧
        ValidatorActionEnabledAt timed.execution (latchTime + 1) validator
          (.persistProposal proposal.block) ∧
        Nonempty (ValidatorPacedRecoveryProposalOccurrence timerSource
          latchTime validator parents) := by
  have reflected := obligations.persistActionIsReflected time validator block
    persisted
  have transition := obligations.transitionsFollowRules time validator
  rw [reflected] at transition
  have readyExists : ∃ proposal,
      (obligations.trace time validator).readyProposal = some proposal ∧
        proposal.block = block := by
    cases transition with
    | persistProposal proposalReady proposalBlock =>
        exact ⟨_, proposalReady, proposalBlock⟩
  rcases readyExists with ⟨proposal, proposalReady, proposalBlock⟩
  have recoveryOrigin :=
    originRules.exactNextPersistenceUsesRecoveryOrigin time validator proposal
      validatorInRange validatorCorrectAvailable recoveryActive floorAboveGc
        proposalReady (by simpa [proposalBlock] using persisted)
  rcases ready_proposal_has_main_action_origin obligations latchSource
      proposalReady with
    ⟨latchTime, latchBeforePersistence, _latchedAt, _readyAtPersistence,
      concreteOrigin⟩
  rcases concreteOrigin with recoveryAction | normalAction
  · rcases recoveryAction.2 with
      ⟨parents, proposalOccurs, proposalParents, persistenceEnabled⟩
    exact ⟨proposal, latchTime, parents, proposalBlock, proposalReady,
      recoveryOrigin, latchBeforePersistence, proposalOccurs, proposalParents,
      persistenceEnabled,
      actual_propose_next_uses_current_armed_recovery_timer pacing
        proposalOccurs⟩
  · exact False.elim (by
      rw [recoveryOrigin] at normalAction
      exact ValidatorProposalOrigin.noConfusion normalAction.1)

/-- The accepted origin rule and the separate past-action timing refinement
derive the complete current-timer snapshot origin of one actual persistence. -/
theorem block_progress_exact_next_persistence_has_current_timer_snapshot_origin
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (originRules : ValidatorBlockProgressProposalOriginRules
      (obligations := obligations) mode)
    (timingRules : ValidatorRecoveryProposalActionTimingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    {time validator : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (recoveryActive :
      ValidatorBlockProgressRecoveryModeAt mode time validator)
    (floorAboveGc :
      ((timed.execution.trace time).validatorState validator).gcRound <
        ((timed.execution.trace time).validatorState
          validator).highestSignedRound)
    (persisted : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.persistProposal block)) :
    Nonempty (ValidatorCurrentTimerProposalOriginAt timerSource time validator
      block) := by
  rcases block_progress_exact_next_persistence_has_current_timer_origin
      originRules latchSource pacing validatorInRange validatorCorrectAvailable
        recoveryActive floorAboveGc persisted with
    ⟨proposal, proposalActionAt, parents, proposalBlock, proposalReady,
      _recoveryOrigin, proposalBeforePersistence, proposalOccurs,
      proposalParents, persistenceEnabled, ⟨paced⟩⟩
  have timing := timingRules.actualProposalUsesTimerWindow proposalActionAt
    validator parents validatorInRange validatorCorrectAvailable proposalOccurs
      paced
  have headAtDeadline := timing.2.2
  have recoveryAtDeadline :
      ((timed.execution.trace (paced.start.deadline waits)).validatorState
        validator).recovery = some (paced.start.recovery waits) := by
    have stored := timerSource.timerPersistsUntilDeadline paced.start
      (paced.start.deadline waits) paced.timerStarted (by
        simp [ValidatorRecoveryTimerStart.deadline]) (Nat.le_refl _) (by
          simpa [paced.startValidator] using headAtDeadline)
    simpa [paced.startValidator] using stored
  have refreshedAtDeadline := timerSource.refreshed_parents_at_deadline
    paced.start paced.timerStarted (by
      simpa [paced.startValidator] using headAtDeadline)
  have parentsNonempty : timerSource.refreshedParents paced.start
      (paced.start.deadline waits) ≠ [] :=
    validator_parent_list_ready_nonempty refreshedAtDeadline.ready.1
  rcases List.exists_mem_of_ne_nil _ parentsNonempty with
    ⟨parent, parentInRefreshed⟩
  have parentInParents : parent ∈ parents := by
    rw [← paced.refreshedParents]
    exact parentInRefreshed
  have parentInProposal : parent ∈ proposal.block.parents := by
    rw [proposalParents]
    exact parentInParents
  have targetFromTimer :=
    (refreshedAtDeadline.ready.1.2.1 parent parentInRefreshed).1
  have proposalLegal := obligations.readyProposalIsLegal time validator proposal
    proposalReady
  have targetFromProposal :=
    (proposalLegal.2.2.2.1.2.1 parent parentInProposal).1
  have proposalTarget : paced.start.targetRound = block.reference.round := by
    rw [← proposalBlock]
    exact targetFromTimer.symm.trans targetFromProposal
  refine ⟨{
    start := paced.start
    parents
    proposalActionAt
    timerStarted := paced.timerStarted
    startValidator := paced.startValidator
    proposalTarget
    refreshedParents := paced.refreshedParents
    headAtDeadline
    recoveryAtDeadline
    deadlineBeforeProposal := timing.1
    proposalWithinLocalBound := timing.2.1
    proposalBeforePersistence := Nat.succ_le_iff.mpr
      proposalBeforePersistence
    proposalOccurs
    persistenceEnabled := by
      simpa only [proposalBlock] using persistenceEnabled
    blockAuthor := by simpa [proposalBlock] using proposalLegal.1
    blockParents := by rw [← proposalBlock]; exact proposalParents
    persistenceOccurs := persisted }⟩

/-- Timed-execution form of a correct local commit-index advance. -/
abbrev SomeCorrectAvailableCommitAdvance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start : Time) : Prop :=
  SomeCorrectAvailableCommitAdvanceFrom faults timed.execution.trace start

/-- One fixed correct validator installs a commit with a larger local index. -/
def ValidatorReceiverCommitAdvance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start receiver : Time) : Prop :=
  ∃ finish,
    start ≤ finish ∧
      ((timed.execution.trace start).validatorState
          receiver).commitHead.index <
      ((timed.execution.trace finish).validatorState
          receiver).commitHead.index

/-- No correct, available host applies a synchronized commit in the selected
post-GST suffix.

This predicate restricts the analyzed trace. It does not assert that a
synchronized commit will arrive, and it is not a positive liveness input. -/
def NoCorrectAvailableApplySyncedCommitAfter
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start : Time) : Prop :=
  ∀ time validator head,
    start ≤ time →
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ¬ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.applySyncedCommit head)

/-- Without a local commit-index advance, one correct receiver keeps the same
commit head throughout the suffix. -/
private theorem no_receiver_commit_advance_keeps_head
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start later receiver : Time}
    (receiverInRange : receiver < config.authorityCount)
    (ordered : start ≤ later)
    (noAdvance : ¬ValidatorReceiverCommitAdvance timed start receiver) :
    ((timed.execution.trace later).validatorState receiver).commitHead =
      ((timed.execution.trace start).validatorState receiver).commitHead := by
  have durable := timed.execution.durableStateMonotone receiver start later
    receiverInRange ordered
  have notStrict : ¬
      ((timed.execution.trace start).validatorState
          receiver).commitHead.index <
        ((timed.execution.trace later).validatorState
          receiver).commitHead.index := by
    intro advanced
    exact noAdvance ⟨later, ordered, advanced⟩
  have sameIndex :
      ((timed.execution.trace start).validatorState
          receiver).commitHead.index =
        ((timed.execution.trace later).validatorState
          receiver).commitHead.index := by
    have monotone := durable.1
    omega
  exact (durable.2.2.1 sameIndex).symm

/-- Without a local commit-index advance, one correct receiver keeps the same
GC boundary throughout the suffix. -/
private theorem no_receiver_commit_advance_keeps_gc_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start later receiver : Time}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (ordered : start ≤ later)
    (noAdvance : ¬ValidatorReceiverCommitAdvance timed start receiver) :
    ((timed.execution.trace later).validatorState receiver).gcRound =
      ((timed.execution.trace start).validatorState receiver).gcRound := by
  rw [timed.execution.correctGcRoundMatchesCommitHead later receiver
      receiverInRange receiverCorrectAvailable,
    timed.execution.correctGcRoundMatchesCommitHead start receiver
      receiverInRange receiverCorrectAvailable,
    no_receiver_commit_advance_keeps_head receiverInRange ordered noAdvance]

/-- A stable receiver suffix contains no synchronized-commit application at
that receiver.

This is a derived transition fact. It does not assume that synchronized commit
data is available or scheduled. Any actual application strictly increases the
receiver's durable local commit index and contradicts the stable suffix. -/
theorem no_receiver_commit_advance_excludes_apply_synced_commit
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start time receiver : Time}
    (receiverInRange : receiver < config.authorityCount)
    (ordered : start ≤ time)
    (noAdvance : ¬ValidatorReceiverCommitAdvance timed start receiver) :
    ∀ head,
      ¬ValidatorLocalActionOccurs (timed.execution.events time) receiver
        (.applySyncedCommit head) := by
  intro head applied
  have advancedInBatch := commit_install_occurrence_advances_commit_index timed
    (Or.inr applied)
  have startHeadAtMostTimeHead :=
    (timed.execution.durableStateMonotone receiver start time receiverInRange
      ordered).1
  exact noAdvance ⟨time + 1, Nat.le_trans ordered (Nat.le_succ _),
    Nat.lt_of_le_of_lt startHeadAtMostTimeHead advancedInBatch⟩

/-- A suffix without any correct-host advance has no advance at one selected
correct receiver. -/
private theorem no_correct_advance_implies_no_receiver_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start receiver : Time}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ¬ValidatorReceiverCommitAdvance timed start receiver := by
  intro receiverAdvance
  rcases receiverAdvance with
    ⟨finish, startBeforeFinish, headAdvanced⟩
  exact noAdvance ⟨receiver, finish, receiverInRange,
    receiverCorrectAvailable, startBeforeFinish, headAdvanced⟩

/-- Repeated receiver-local commit advances cannot stay below one fixed target
index forever. The receiver either reaches the target, or its local commit
index is constant on one later suffix.

This is the well-founded restart argument for one receiver. It does not use a
commit-sync availability or scheduling premise. -/
theorem receiver_reaches_target_index_or_has_stable_suffix
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start receiver minimumIndex : Time} :
    (∃ finish,
      start ≤ finish ∧
        minimumIndex ≤
          ((timed.execution.trace finish).validatorState
          receiver).commitHead.index) ∨
      ∃ stableAt,
        start ≤ stableAt ∧
          ¬ValidatorReceiverCommitAdvance timed stableAt receiver := by
  let indexAt := fun time =>
    ((timed.execution.trace time).validatorState receiver).commitHead.index
  have go : ∀ fuel current,
      start ≤ current →
      minimumIndex - indexAt current ≤ fuel →
      (∃ finish, start ≤ finish ∧ minimumIndex ≤ indexAt finish) ∨
        ∃ stableAt, start ≤ stableAt ∧
          ¬ValidatorReceiverCommitAdvance timed stableAt receiver := by
    intro fuel
    induction fuel using Nat.strongRecOn with
    | ind fuel inductionHypothesis =>
        intro current startBeforeCurrent remainingAtMostFuel
        by_cases currentAtTarget : minimumIndex ≤ indexAt current
        · exact Or.inl ⟨current, startBeforeCurrent, currentAtTarget⟩
        · by_cases advancesCurrent :
              ValidatorReceiverCommitAdvance timed current receiver
          · rcases advancesCurrent with
              ⟨later, currentBeforeLater, indexIncreases⟩
            have indexIncreases' : indexAt current < indexAt later := by
              simpa [indexAt] using indexIncreases
            have startBeforeLater :=
              Nat.le_trans startBeforeCurrent currentBeforeLater
            by_cases laterAtTarget : minimumIndex ≤ indexAt later
            · exact Or.inl ⟨later, startBeforeLater, laterAtTarget⟩
            · have remainingDecreases :
                  minimumIndex - indexAt later <
                    minimumIndex - indexAt current := by
                have currentBelow := Nat.lt_of_not_ge currentAtTarget
                exact Nat.sub_lt_sub_left currentBelow indexIncreases'
              have remainingBelowFuel :
                  minimumIndex - indexAt later < fuel :=
                Nat.lt_of_lt_of_le remainingDecreases remainingAtMostFuel
              exact inductionHypothesis
                (minimumIndex - indexAt later) remainingBelowFuel later
                  startBeforeLater (Nat.le_refl _)
          · exact Or.inr ⟨current, startBeforeCurrent, advancesCurrent⟩
  simpa [indexAt] using
    (go (minimumIndex - indexAt start) start (Nat.le_refl _) (Nat.le_refl _))

/-- In a suffix with no correct local commit advance, the network progress
split returns a produced and commonly accepted correct quorum layer. -/
theorem network_dag_or_commit_progress_gives_stable_head_layer
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (progress : NetworkDagOrCommitProgressLiveness config faults network
      timed.execution.trace)
    {start minimumRound : Time}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (stableHead : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ∃ finish round,
      start ≤ finish ∧
      minimumRound ≤ round ∧
      ProducedCorrectQuorumLayer config faults
          (timed.execution.trace finish) round ∧
      CommonUsableCorrectQuorumLayer config faults
          (timed.execution.trace finish) round := by
  rcases progress start minimumRound afterGst active with advanced | layer
  · exact False.elim (stableHead advanced)
  · exact layer

/-- A commit advance measured from a later time is also an advance measured
from any earlier time. -/
theorem commit_advance_after_later_implies_advance_after_earlier
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {earlier later : Time}
    (ordered : earlier ≤ later)
    (advanced : SomeCorrectAvailableCommitAdvance timed later) :
    SomeCorrectAvailableCommitAdvance timed earlier := by
  rcases advanced with
    ⟨validator, finish, validatorInRange, validatorCorrect,
      laterBeforeFinish, laterHeadBeforeFinish⟩
  have durable := timed.execution.durableStateMonotone validator earlier later
    validatorInRange ordered
  have earlierHeadAtMostLater := durable.1
  exact ⟨validator, finish, validatorInRange, validatorCorrect,
    Nat.le_trans ordered laterBeforeFinish,
    Nat.lt_of_le_of_lt earlierHeadAtMostLater laterHeadBeforeFinish⟩

/-- If no correct local commit advances from an earlier time, none advances
from a later time. -/
theorem no_commit_advance_persists_to_later_start
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {earlier later : Time}
    (ordered : earlier ≤ later)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed earlier) :
    ¬SomeCorrectAvailableCommitAdvance timed later := by
  intro advanced
  exact noAdvance
    (commit_advance_after_later_implies_advance_after_earlier ordered advanced)

/-- If no correct local commit index advances, one correct host keeps its exact
commit head throughout the suffix. -/
theorem no_commit_advance_keeps_correct_commit_head
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start later validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ later)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ((timed.execution.trace later).validatorState validator).commitHead =
      ((timed.execution.trace start).validatorState validator).commitHead := by
  have durable := timed.execution.durableStateMonotone validator start later
    validatorInRange ordered
  have noStrict : ¬
      ((timed.execution.trace start).validatorState
          validator).commitHead.index <
        ((timed.execution.trace later).validatorState
          validator).commitHead.index := by
    intro advanced
    exact noAdvance ⟨validator, later, validatorInRange,
      validatorCorrectAvailable, ordered, advanced⟩
  have sameIndex :
      ((timed.execution.trace start).validatorState
          validator).commitHead.index =
        ((timed.execution.trace later).validatorState
          validator).commitHead.index := by
    have monotone := durable.1
    omega
  exact (durable.2.2.1 sameIndex).symm

/-- The same stable-head suffix also fixes the correct host's GC boundary. -/
theorem no_commit_advance_keeps_correct_gc_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start later validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ later)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ((timed.execution.trace later).validatorState validator).gcRound =
      ((timed.execution.trace start).validatorState validator).gcRound := by
  rw [timed.execution.correctGcRoundMatchesCommitHead later validator
      validatorInRange validatorCorrectAvailable,
    timed.execution.correctGcRoundMatchesCommitHead start validator
      validatorInRange validatorCorrectAvailable,
    no_commit_advance_keeps_correct_commit_head validatorInRange
      validatorCorrectAvailable ordered noAdvance]

/-- A correct host cannot contain a commit-install event in a suffix where no
correct local commit index advances. -/
theorem no_commit_advance_excludes_correct_commit_install
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {start time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ time)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ∀ head, ¬ValidatorCommitInstallOccurs (timed.execution.events time)
      validator head := by
  intro head installed
  have advancedInBatch := commit_install_occurrence_advances_commit_index timed
    installed
  have startHeadAtMostTimeHead :=
    (timed.execution.durableStateMonotone validator start time
      validatorInRange ordered).1
  exact noAdvance ⟨validator, time + 1, validatorInRange,
    validatorCorrectAvailable, Nat.le_trans ordered (Nat.le_succ _),
    Nat.lt_of_le_of_lt startHeadAtMostTimeHead advancedInBatch⟩

/-- Local storage rules for commit progress recovery mode.

The wait starts at the persisted time of the last local commit. Without a new
commit index, that persisted time does not change. -/
structure ValidatorCommitProgressRecoveryModeRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  recoveryWait : Time
  sameCommitIndexKeepsLastCommitTime : ∀ validator earlier later,
    validator < config.authorityCount →
    earlier ≤ later →
    ((timed.execution.trace earlier).validatorState
        validator).commitHead.index =
      ((timed.execution.trace later).validatorState
        validator).commitHead.index →
    ((timed.execution.trace earlier).validatorState
        validator).lastCommitTime =
      ((timed.execution.trace later).validatorState
        validator).lastCommitTime

/-- If no correct local commit index advances, every correct, available
validator enters recovery by one common finite time. The common time is a proof
result. Validators do not select or announce it. -/
theorem no_commit_advance_gives_active_recovery_snapshot
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    {start : Time}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ∃ snapshot,
      start ≤ snapshot ∧
        ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait snapshot := by
  have sameIndexAt : ∀ validator later,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      start ≤ later →
      ((timed.execution.trace start).validatorState
          validator).commitHead.index =
        ((timed.execution.trace later).validatorState
          validator).commitHead.index := by
    intro validator later validatorInRange validatorCorrect startBeforeLater
    have durable := timed.execution.durableStateMonotone validator start later
      validatorInRange startBeforeLater
    have notStrict : ¬
        ((timed.execution.trace start).validatorState
            validator).commitHead.index <
          ((timed.execution.trace later).validatorState
            validator).commitHead.index := by
      intro advanced
      exact noAdvance ⟨validator, later, validatorInRange, validatorCorrect,
        startBeforeLater, advanced⟩
    have sameIndex :
        ((timed.execution.trace start).validatorState
            validator).commitHead.index =
          ((timed.execution.trace later).validatorState
            validator).commitHead.index := by
      have monotone := durable.1
      omega
    exact sameIndex
  let recovered := fun validator time =>
    start ≤ time ∧
      (validator < config.authorityCount →
        faults.correctAvailable validator = true →
        ValidatorCommitProgressRecoveryModeAt timed modeRules.recoveryWait time
          validator)
  have recoveredPersists : ∀ validator earlier later,
      earlier ≤ later → recovered validator earlier →
      recovered validator later := by
    intro validator earlier later earlierBeforeLater done
    refine ⟨Nat.le_trans done.1 earlierBeforeLater, ?_⟩
    intro validatorInRange validatorCorrect
    have earlierIndex := sameIndexAt validator earlier validatorInRange
      validatorCorrect done.1
    have laterIndex := sameIndexAt validator later validatorInRange
      validatorCorrect (Nat.le_trans done.1 earlierBeforeLater)
    have sameIndex :
        ((timed.execution.trace earlier).validatorState
            validator).commitHead.index =
          ((timed.execution.trace later).validatorState
            validator).commitHead.index := earlierIndex.symm.trans laterIndex
    have sameLastCommit := modeRules.sameCommitIndexKeepsLastCommitTime
      validator earlier later validatorInRange earlierBeforeLater sameIndex
    exact recovery_mode_persists_with_stable_last_commit validatorInRange
      earlierBeforeLater (done.2 validatorInRange validatorCorrect)
      (active later (Nat.le_trans done.1 earlierBeforeLater))
      sameLastCommit.symm
  have eachValidator : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ∃ finish, start ≤ finish ∧ recovered validator finish := by
    intro validator validatorInRange validatorCorrect
    let deadline :=
      ((timed.execution.trace start).validatorState
        validator).lastCommitTime + modeRules.recoveryWait
    rcases timed.execution.clocksProgress validator start deadline
        validatorInRange validatorCorrect with
      ⟨clockTime, startBeforeClock, clockReached⟩
    have sameIndex :
        ((timed.execution.trace start).validatorState
            validator).commitHead.index =
          ((timed.execution.trace clockTime).validatorState
            validator).commitHead.index := by
      exact sameIndexAt validator clockTime validatorInRange validatorCorrect
        startBeforeClock
    have lastCommitTimeAtClock :=
      modeRules.sameCommitIndexKeepsLastCommitTime
      validator start clockTime validatorInRange startBeforeClock sameIndex
    have modeAtClock : ValidatorCommitProgressRecoveryModeAt timed
        modeRules.recoveryWait clockTime validator := by
      refine ⟨active clockTime startBeforeClock, ?_⟩
      rw [← lastCommitTimeAtClock]
      exact clockReached
    exact ⟨clockTime, startBeforeClock, startBeforeClock,
      fun _ _ => modeAtClock⟩
  rcases eventually_every_selected_validator faults.correctAvailable recovered
      start recoveredPersists eachValidator with
    ⟨snapshot, startBeforeSnapshot, allRecovered⟩
  exact ⟨snapshot, startBeforeSnapshot,
    { afterGst := Nat.le_trans afterGst startBeforeSnapshot
      recovering := by
        intro validator validatorInRange validatorCorrect
        exact (allRecovered validator validatorInRange validatorCorrect).2
          validatorInRange validatorCorrect }⟩

/-- A common time-gap snapshot latches the combined recovery mode in the next
local transition. This theorem derives the latch from the one-host hysteresis
rule. It does not assume a future proposal or parent layer. -/
theorem active_stall_snapshot_latches_block_progress_recovery
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
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    {snapshot : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    ValidatorActiveBlockProgressRecoverySnapshot mode (snapshot + 1) := by
  refine
    { afterGst := Nat.le_trans recovery.afterGst (Nat.le_succ snapshot)
      recovering := ?_ }
  intro validator validatorInRange validatorCorrectAvailable
  have sameHead := no_commit_advance_keeps_correct_commit_head
    validatorInRange validatorCorrectAvailable (Nat.le_succ snapshot) noAdvance
  have sameLastCommit := modeRules.sameCommitIndexKeepsLastCommitTime
    validator snapshot (snapshot + 1) validatorInRange (Nat.le_succ snapshot)
      (congrArg ValidatorCommitHead.index sameHead.symm)
  have timedOut : ValidatorCommitProgressRecoveryModeAt timed
      thresholds.recoveryWait (snapshot + 1) validator := by
    rw [sameRecoveryWait]
    exact recovery_mode_persists_with_stable_last_commit validatorInRange
      (Nat.le_succ snapshot)
      (recovery.recovering validator validatorInRange validatorCorrectAvailable)
      (active (snapshot + 1) (Nat.le_succ snapshot)) sameLastCommit.symm
  exact block_progress_recovery_entry_activates_or_keeps_mode mode
    (Or.inl timedOut)

/-- Once the combined recovery mode is active, a stable commit-head suffix
keeps it active. The time-gap signal remains true because the persisted
last-commit time does not change. -/
theorem block_progress_recovery_mode_persists_without_commit_advance
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
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    {start later validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ordered : start ≤ later)
    (timeGapAtStart : ValidatorCommitProgressRecoveryModeAt timed
      modeRules.recoveryWait start validator)
    (modeAtStart : ValidatorBlockProgressRecoveryModeAt mode start validator)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ValidatorBlockProgressRecoveryModeAt mode later validator := by
  let fact := fun time =>
    start ≤ time ∧ ValidatorBlockProgressRecoveryModeAt mode time validator
  have oneStep : ∀ time, fact time → fact (time + 1) := by
    intro time current
    have startBeforeNext : start ≤ time + 1 :=
      Nat.le_trans current.1 (Nat.le_succ time)
    have sameHead := no_commit_advance_keeps_correct_commit_head
      validatorInRange validatorCorrectAvailable startBeforeNext noAdvance
    have sameLastCommit := modeRules.sameCommitIndexKeepsLastCommitTime
      validator start (time + 1) validatorInRange startBeforeNext
        (congrArg ValidatorCommitHead.index sameHead.symm)
    have timeGapNextRaw : ValidatorCommitProgressRecoveryModeAt timed
        modeRules.recoveryWait (time + 1) validator :=
      recovery_mode_persists_with_stable_last_commit validatorInRange
        startBeforeNext timeGapAtStart (active (time + 1) startBeforeNext)
          sameLastCommit.symm
    have timeGapNext : ValidatorCommitProgressRecoveryModeAt timed
        thresholds.recoveryWait (time + 1) validator := by
      rw [sameRecoveryWait]
      exact timeGapNextRaw
    exact ⟨startBeforeNext,
      block_progress_recovery_persists_while_time_gap_remains mode current.2
        timeGapNext⟩
  obtain ⟨offset, laterAtOffset⟩ := Nat.exists_eq_add_of_le ordered
  subst later
  have advance : ∀ offset, fact (start + offset) := by
    intro offset
    induction offset with
    | zero =>
        change start ≤ start ∧
          ValidatorBlockProgressRecoveryModeAt mode start validator
        exact ⟨Nat.le_refl start, modeAtStart⟩
    | succ offset inductionHypothesis =>
        have next := oneStep (start + offset) inductionHypothesis
        simpa [Nat.add_assoc] using next
  exact (advance offset).2

/-- One time-gap recovery snapshot becomes a combined time-gap and block-gap
recovery snapshot after one local transition. Both modes are derived values. -/
theorem active_recovery_snapshot_latches_combined_recovery_snapshot
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
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    {snapshot : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
        (snapshot + 1) ∧
      ValidatorActiveBlockProgressRecoverySnapshot mode (snapshot + 1) := by
  have blockRecovery := active_stall_snapshot_latches_block_progress_recovery
    modeRules mode sameRecoveryWait recovery active noAdvance
  refine ⟨{
    afterGst := Nat.le_trans recovery.afterGst (Nat.le_succ snapshot)
    recovering := ?_ }, blockRecovery⟩
  intro validator validatorInRange validatorCorrectAvailable
  have sameHead := no_commit_advance_keeps_correct_commit_head
    validatorInRange validatorCorrectAvailable (Nat.le_succ snapshot) noAdvance
  have sameLastCommit := modeRules.sameCommitIndexKeepsLastCommitTime validator
    snapshot (snapshot + 1) validatorInRange (Nat.le_succ snapshot)
      (congrArg ValidatorCommitHead.index sameHead.symm)
  exact recovery_mode_persists_with_stable_last_commit validatorInRange
    (Nat.le_succ snapshot)
      (recovery.recovering validator validatorInRange validatorCorrectAvailable)
      (active (snapshot + 1) (Nat.le_succ snapshot)) sameLastCommit.symm

/-- A post-GST active suffix either installs a later correct commit or derives
one common active-recovery snapshot from local clock and entry actions. -/
theorem commit_advance_or_active_recovery_snapshot
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    {start : Time}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed start ∨
      ∃ snapshot,
        start ≤ snapshot ∧
          ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
            snapshot := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed start
  · exact Or.inl advanced
  · exact Or.inr
      (no_commit_advance_gives_active_recovery_snapshot modeRules afterGst
        active advanced)

/-- The ghost maximum of the durable signer floors of the correct, available
validators at one trace time. Validators do not compute or exchange it. -/
def ValidatorCorrectAvailableSignerFloorMaximum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId) : Nat :=
  correctValidatorSignerFloorMaximumUpTo faults world config.authorityCount

/-- One host's first usable recovery round.

If the signer floor is above GC, the durable tip is the base. If GC has passed
the signer floor, the one allowed normal bootstrap uses `gcRound + 2`. -/
def ValidatorLocalRecoveryBaseRound
    {BlockId CommitId : Type}
    (state : ValidatorLocalState BlockId CommitId) : Nat :=
  if state.highestSignedRound ≤ state.gcRound then
    if state.gcRound = 0 then 1 else state.gcRound + 2
  else state.highestSignedRound

/-- One-host proposal-round policy for commit progress recovery.

The rule is stated on an actual proposal persistence batch. Thus, stale normal
proposal work cannot overtake the recovery target. This is a local scheduling
and storage rule. It does not state that a future proposal or layer exists. -/
structure ValidatorCommitProgressProposalRoundRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (recoveryWait : Time) : Prop where
  persistedProposalUsesRecoveryRound : ∀ time validator block,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorCommitProgressRecoveryModeAt timed recoveryWait time validator →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.persistProposal block) →
    block.reference.round = ValidatorCommitProgressProposalRound
      ((timed.execution.trace time).validatorState validator)

/-- The local recovery base has exactly one of the genesis, durable-tip, or
post-GC bootstrap forms. -/
theorem local_recovery_base_cases
    {BlockId CommitId : Type}
    (state : ValidatorLocalState BlockId CommitId) :
    (state.highestSignedRound = 0 ∧ state.gcRound = 0 ∧
        ValidatorLocalRecoveryBaseRound state = 1) ∨
      (state.gcRound < state.highestSignedRound ∧
        ValidatorLocalRecoveryBaseRound state = state.highestSignedRound) ∨
      (0 < state.gcRound ∧
        state.highestSignedRound ≤ state.gcRound ∧
        ValidatorLocalRecoveryBaseRound state = state.gcRound + 2) := by
  by_cases floorAtOrBelowGc : state.highestSignedRound ≤ state.gcRound
  · by_cases gcZero : state.gcRound = 0
    · left
      have floorZero : state.highestSignedRound = 0 := by omega
      exact ⟨floorZero, gcZero, by
        simp [ValidatorLocalRecoveryBaseRound, floorZero, gcZero]⟩
    · right
      right
      exact ⟨Nat.pos_of_ne_zero gcZero, floorAtOrBelowGc, by
        simp [ValidatorLocalRecoveryBaseRound, floorAtOrBelowGc, gcZero]⟩
  · right
    left
    exact ⟨Nat.lt_of_not_ge floorAtOrBelowGc, by
      simp [ValidatorLocalRecoveryBaseRound, floorAtOrBelowGc]⟩

/-- A local signer floor never exceeds its first usable recovery base. -/
theorem signer_floor_le_local_recovery_base
    {BlockId CommitId : Type}
    (state : ValidatorLocalState BlockId CommitId) :
    state.highestSignedRound ≤ ValidatorLocalRecoveryBaseRound state := by
  rcases local_recovery_base_cases state with genesis | durableTip | postGc
  · omega
  · omega
  · omega

/-- The first usable recovery base is always strictly above the local GC
boundary. -/
theorem gc_round_lt_local_recovery_base
    {BlockId CommitId : Type}
    (state : ValidatorLocalState BlockId CommitId) :
    state.gcRound < ValidatorLocalRecoveryBaseRound state := by
  rcases local_recovery_base_cases state with genesis | durableTip | postGc
  · omega
  · omega
  · omega

/-- The common proof-only base round across the correct validators. Validators
do not compute or exchange this value. -/
def correctValidatorRecoveryBaseMaximumUpTo
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId) : Nat → Nat
  | 0 => 0
  | count + 1 =>
      max (correctValidatorRecoveryBaseMaximumUpTo faults world count)
        (if faults.correctAvailable count then
          ValidatorLocalRecoveryBaseRound (world.validatorState count)
        else 0)

/-- Each correct validator's local recovery base is at most the common base. -/
theorem correct_validator_recovery_base_le_maximum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator count : Nat}
    (validatorInRange : validator < count)
    (validatorCorrectAvailable : faults.correctAvailable validator = true) :
    ValidatorLocalRecoveryBaseRound (world.validatorState validator) ≤
      correctValidatorRecoveryBaseMaximumUpTo faults world count := by
  induction count generalizing validator with
  | zero => omega
  | succ previous inductionHypothesis =>
      simp only [correctValidatorRecoveryBaseMaximumUpTo]
      by_cases validatorIsLast : validator = previous
      · subst validator
        simpa [validatorCorrectAvailable] using
          (Nat.le_max_right
            (correctValidatorRecoveryBaseMaximumUpTo faults world previous)
            (ValidatorLocalRecoveryBaseRound
              (world.validatorState previous)))
      · exact Nat.le_trans
          (inductionHypothesis (by omega) validatorCorrectAvailable)
          (Nat.le_max_left _ _)

/-- Static fault bounds imply that the correct-validator common recovery base
is positive. -/
theorem correct_recovery_base_maximum_positive
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId} :
    0 < correctValidatorRecoveryBaseMaximumUpTo faults world
      config.authorityCount := by
  have positiveCorrectWeight :
      0 < weight config.authorityCount config.stake faults.correctAvailable :=
    Nat.lt_of_lt_of_le config.thresholds.quorum_positive
      faults.correct_available_stake_is_quorum
  rcases positive_weight_has_member positiveCorrectWeight with
    ⟨validator, validatorInRange, validatorCorrectAvailable, _positiveStake⟩
  have localPositive :
      0 < ValidatorLocalRecoveryBaseRound (world.validatorState validator) := by
    rcases local_recovery_base_cases (world.validatorState validator) with
      genesis | tip | postGc
    · omega
    · omega
    · omega
  exact Nat.lt_of_lt_of_le localPositive
    (correct_validator_recovery_base_le_maximum validatorInRange
      validatorCorrectAvailable)

/-- A positive common recovery base is the local base of one actual correct,
available validator. This owner is selected only inside the proof. -/
theorem positive_correct_recovery_base_maximum_has_owner
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {count : Nat}
    (positive :
      0 < correctValidatorRecoveryBaseMaximumUpTo faults world count) :
    ∃ validator,
      validator < count ∧
      faults.correctAvailable validator = true ∧
      ValidatorLocalRecoveryBaseRound (world.validatorState validator) =
        correctValidatorRecoveryBaseMaximumUpTo faults world count := by
  induction count with
  | zero =>
      simp [correctValidatorRecoveryBaseMaximumUpTo] at positive
  | succ previous inductionHypothesis =>
      simp only [correctValidatorRecoveryBaseMaximumUpTo] at positive ⊢
      let earlier :=
        correctValidatorRecoveryBaseMaximumUpTo faults world previous
      let last := if faults.correctAvailable previous then
        ValidatorLocalRecoveryBaseRound (world.validatorState previous) else 0
      by_cases lastAtMostEarlier : last ≤ earlier
      · have maximumIsEarlier : max earlier last = earlier :=
          Nat.max_eq_left lastAtMostEarlier
        have earlierPositive : 0 < earlier := by
          rw [maximumIsEarlier] at positive
          exact positive
        rcases inductionHypothesis earlierPositive with
          ⟨validator, validatorInRange, validatorCorrect, validatorMaximum⟩
        exact ⟨validator, by omega, validatorCorrect, by
          rw [maximumIsEarlier]
          exact validatorMaximum⟩
      · have earlierAtMostLast : earlier ≤ last :=
          Nat.le_of_lt (Nat.lt_of_not_ge lastAtMostEarlier)
        have maximumIsLast : max earlier last = last :=
          Nat.max_eq_right earlierAtMostLast
        have lastPositive : 0 < last := by
          rw [maximumIsLast] at positive
          exact positive
        have previousCorrect : faults.correctAvailable previous = true := by
          unfold last at lastPositive
          split at lastPositive
          · assumption
          · omega
        refine ⟨previous, by omega, previousCorrect, ?_⟩
        rw [maximumIsLast]
        simp [last, previousCorrect]

/-- The common recovery base has one actual correct, available owner, and that
owner supplies one of the three local bootstrap cases. The maximum and its
owner are proof values. Validators do not select or announce them. -/
theorem correct_recovery_base_maximum_has_classified_owner
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId} :
    ∃ owner,
      owner < config.authorityCount ∧
      faults.correctAvailable owner = true ∧
      ((world.validatorState owner).highestSignedRound = 0 ∧
          (world.validatorState owner).gcRound = 0 ∧
          correctValidatorRecoveryBaseMaximumUpTo faults world
              config.authorityCount = 1 ∨
        ((world.validatorState owner).gcRound <
              (world.validatorState owner).highestSignedRound ∧
          correctValidatorRecoveryBaseMaximumUpTo faults world
              config.authorityCount =
            (world.validatorState owner).highestSignedRound) ∨
        (0 < (world.validatorState owner).gcRound ∧
          (world.validatorState owner).highestSignedRound ≤
              (world.validatorState owner).gcRound ∧
          correctValidatorRecoveryBaseMaximumUpTo faults world
              config.authorityCount =
            (world.validatorState owner).gcRound + 2)) := by
  have positive := correct_recovery_base_maximum_positive
    (faults := faults) (world := world)
  rcases positive_correct_recovery_base_maximum_has_owner
      (faults := faults) (world := world) positive with
    ⟨owner, ownerInRange, ownerCorrect, ownerMaximum⟩
  refine ⟨owner, ownerInRange, ownerCorrect, ?_⟩
  rcases local_recovery_base_cases (world.validatorState owner) with
    genesis | durableTip | postGc
  · exact Or.inl ⟨genesis.1, genesis.2.1,
      ownerMaximum.symm.trans genesis.2.2⟩
  · exact Or.inr (Or.inl ⟨durableTip.1,
      ownerMaximum.symm.trans durableTip.2⟩)
  · exact Or.inr (Or.inr ⟨postGc.1, postGc.2.1,
      ownerMaximum.symm.trans postGc.2.2⟩)

/-- The common base is at least each current signer floor and strictly above
each current GC boundary. -/
theorem correct_validator_floor_and_gc_below_recovery_base
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator count : Nat}
    (validatorInRange : validator < count)
    (validatorCorrectAvailable : faults.correctAvailable validator = true) :
    (world.validatorState validator).highestSignedRound ≤
        correctValidatorRecoveryBaseMaximumUpTo faults world count ∧
      (world.validatorState validator).gcRound <
        correctValidatorRecoveryBaseMaximumUpTo faults world count := by
  have bound := correct_validator_recovery_base_le_maximum
    (world := world) validatorInRange validatorCorrectAvailable
  unfold ValidatorLocalRecoveryBaseRound at bound
  split at bound
  · split at bound <;> constructor <;> omega
  · constructor <;> omega

/-- A requested round is either the common base or a later round reached by
exact-next induction after the common base exists. -/
def ValidatorRequestedRecoveryRound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (minimumRound : Nat) : Nat :=
  max minimumRound
    (correctValidatorRecoveryBaseMaximumUpTo faults world
      config.authorityCount)

theorem requested_recovery_round_bounds_correct_validator
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {minimumRound validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true) :
    minimumRound ≤
        ValidatorRequestedRecoveryRound faults world minimumRound ∧
      (world.validatorState validator).highestSignedRound ≤
        ValidatorRequestedRecoveryRound faults world minimumRound ∧
      (world.validatorState validator).gcRound <
        ValidatorRequestedRecoveryRound faults world minimumRound := by
  have localBounds := correct_validator_floor_and_gc_below_recovery_base
    (world := world) validatorInRange validatorCorrectAvailable
  exact ⟨Nat.le_max_left _ _,
    Nat.le_trans localBounds.1 (Nat.le_max_right _ _),
    Nat.lt_of_lt_of_le localBounds.2 (Nat.le_max_right _ _)⟩

/-- An idle correct host whose signer floor is behind positive GC starts one
fresh normal bootstrap and broadcasts its legal above-GC block. This is the
only non-exact-next proposal used by commit progress recovery. -/
theorem idle_post_gc_recovery_bootstrap_eventually_produces_broadcast
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time} {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (noCommitInstall : ∀ candidate,
      ¬ValidatorCommitInstallOccurs (timed.execution.events time) validator
        candidate)
    (idle : (needs.trace time validator).active = none)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      (time + 1) validator)
    (positiveGc : 0 < ((timed.execution.trace (time + 1)).validatorState
      validator).gcRound)
    (floorAtOrBelowGc :
      ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound ≤
        ((timed.execution.trace (time + 1)).validatorState validator).gcRound)
    (currentHead :
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead = head) :
    ∃ targetRound,
      Nonempty (ValidatorNormalProposalBroadcastProduction timed obligations
        (time + 1) validator targetRound) := by
  have ready := installed_head_bootstrap_recovery_root_gives_protected_build
    source needs representatives validatorInRange validatorCorrectAvailable
      noCommitInstall idle recoveryMode positiveGc floorAtOrBelowGc
      recoveryMode.1 currentHead
  exact normal_parent_build_ready_eventually_produces_broadcast latchSource
    effects authorityCountAtLeastTwo validatorInRange
      validatorCorrectAvailable ready

/-- An existing parent need cannot block post-GC safe resume after the new
block-progress recovery mode is active.

An active recovery-origin need would be exact-next and is therefore impossible
at or below positive GC. The remaining normal need is the current safe-resume
need by the local refresh rule. Current installed-head storage then protects
the concrete normal proposal build. -/
theorem active_post_gc_block_progress_recovery_gives_protected_build
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (source : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (needRules : ValidatorBlockProgressRecoveryNeedRules mode needs)
    {time validator : Time} {need} {head : ValidatorCommitHead CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (blockProgressRecovery :
      ValidatorBlockProgressRecoveryModeAt mode time validator)
    (needActive : (needs.trace time validator).active = some need)
    (positiveGc :
      0 < ((timed.execution.trace time).validatorState validator).gcRound)
    (floorAtOrBelowGc :
      ((timed.execution.trace time).validatorState
          validator).highestSignedRound ≤
        ((timed.execution.trace time).validatorState validator).gcRound)
    (epochActive : (timed.execution.trace time).epochActive = true)
    (currentHead :
      ((timed.execution.trace time).validatorState validator).commitHead =
        head) :
    ValidatorNormalParentBuildReadyAt timed time validator := by
  have mainFacts := needs.activeNeedMatchesMain time validator need needActive
  have normalOrigin : need.proposalOrigin = .normal := by
    cases origin : need.proposalOrigin with
    | normal => exact rfl
    | commitProgressRecovery =>
        have exactNext := need.recoveryTargetIsExactNext origin
        have targetFence := needs.activeNeedFencesTargetRound time validator need
          needActive
        rcases targetFence with genesis | aboveGc
        · omega
        · omega
  have fresh := needRules.activeNormalNeedIsCurrentSafeResume time validator
    need validatorInRange validatorCorrectAvailable blockProgressRecovery
      needActive normalOrigin floorAtOrBelowGc
  apply installed_head_bootstrap_fresh_need_gives_protected_build source needs
    representatives validatorInRange validatorCorrectAvailable needActive fresh
      epochActive currentHead positiveGc
  omega

/-- Every correct, available signer floor is at most the ghost maximum. -/
theorem correct_available_floor_le_ghost_maximum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true) :
    (world.validatorState validator).highestSignedRound ≤
      ValidatorCorrectAvailableSignerFloorMaximum faults world := by
  exact correct_validator_signer_floor_le_maximum (world := world)
    validatorInRange validatorCorrectAvailable

/-- If the correct, available ghost maximum is zero, every correct, available
validator is still at signer floor zero. -/
theorem zero_correct_available_maximum_gives_zero_floor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    (maximumIsZero :
      ValidatorCorrectAvailableSignerFloorMaximum faults world = 0)
    {validator : Nat}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true) :
    (world.validatorState validator).highestSignedRound = 0 := by
  have bounded := correct_validator_signer_floor_le_maximum (world := world)
    validatorInRange validatorCorrectAvailable
  change (world.validatorState validator).highestSignedRound ≤
    ValidatorCorrectAvailableSignerFloorMaximum faults world at bounded
  rw [maximumIsZero] at bounded
  exact Nat.eq_zero_of_le_zero bounded

/-- A positive ghost maximum has an actual correct, available owner. The
owner's durable tip has one source-local pinned causal capsule at the same
time. This theorem does not assume a useful remote source. -/
theorem positive_correct_available_maximum_has_pinned_source
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {time : Time}
    (active : (timed.execution.trace time).epochActive = true)
    (positive : 0 < ValidatorCorrectAvailableSignerFloorMaximum faults
      (timed.execution.trace time)) :
    ∃ owner block capsuleId entry,
      owner < config.authorityCount ∧
        faults.correctAvailable owner = true ∧
        ((timed.execution.trace time).validatorState owner).ownBlockAt
            (ValidatorCorrectAvailableSignerFloorMaximum faults
              (timed.execution.trace time)) = some block.reference ∧
        block.reference.round =
          ValidatorCorrectAvailableSignerFloorMaximum faults
            (timed.execution.trace time) ∧
        entry.capsule.targetBlock = block ∧
        entry.capsule.targetRound =
          ValidatorCorrectAvailableSignerFloorMaximum faults
            (timed.execution.trace time) ∧
        (pins.trace time owner).capsuleAt capsuleId = some entry ∧
        (pins.trace time owner).pinned capsuleId = true ∧
        CausalRecoveryCapsuleExecutionSource syncRules entry.capsule owner
          time := by
  have positiveRaw : 0 < correctValidatorSignerFloorMaximumUpTo faults
      (timed.execution.trace time) config.authorityCount := by
    exact positive
  rcases positive_correct_signer_floor_maximum_has_owner
      (world := timed.execution.trace time) positiveRaw with
    ⟨owner, ownerInRange, ownerCorrect, ownerMaximum⟩
  have ownerMaximumGhost :
      ((timed.execution.trace time).validatorState
        owner).highestSignedRound =
      ValidatorCorrectAvailableSignerFloorMaximum faults
        (timed.execution.trace time) := by
    exact ownerMaximum
  have ownerPositive : 0 < ((timed.execution.trace time).validatorState
      owner).highestSignedRound := by
    rw [ownerMaximumGhost]
    exact positive
  rcases pins.current_positive_tip_has_pinned_capsule_source ownerInRange
      ownerCorrect active ownerPositive with
    ⟨block, capsuleId, entry, ownTip, targetBlock, stored, pinned, source⟩
  have blockRound :=
    (timed.execution.statesWellFormed time owner ownerInRange).ownBlockIsSound
      ((timed.execution.trace time).validatorState owner).highestSignedRound
      block.reference ownTip
  have ownAtMaximum :
      ((timed.execution.trace time).validatorState owner).ownBlockAt
          (ValidatorCorrectAvailableSignerFloorMaximum faults
            (timed.execution.trace time)) = some block.reference := by
    rw [← ownerMaximumGhost]
    exact ownTip
  have exactRound : block.reference.round =
      ValidatorCorrectAvailableSignerFloorMaximum faults
        (timed.execution.trace time) := by
    exact blockRound.2.1.trans ownerMaximumGhost
  have targetRound : entry.capsule.targetRound =
      ValidatorCorrectAvailableSignerFloorMaximum faults
        (timed.execution.trace time) := by
    unfold CausalRecoveryCapsule.targetRound
    rw [targetBlock]
    exact exactRound
  exact ⟨owner, block, capsuleId, entry, ownerInRange, ownerCorrect,
    ownAtMaximum, exactRound, targetBlock, targetRound, stored, pinned, source⟩

/-- One correct host's current durable tip, its source-local causal pin, and
its active recovery mode. The tip can predate the modeled suffix. -/
structure ValidatorStableRecoveryTipSource
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recoveryWait sourceAt author round : Time) where
  block : ValidatorBlock BlockId
  capsuleKey : ValidatorRecoveryCapsuleKey BlockId
  entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config
  roundPositive : 0 < round
  currentFloor :
    ((timed.execution.trace sourceAt).validatorState
      author).highestSignedRound = round
  ownTip :
    ((timed.execution.trace sourceAt).validatorState author).ownBlockAt round =
      some block.reference
  exactRound : block.reference.round = round
  targetBlock : entry.capsule.targetBlock = block
  stored : (pins.trace sourceAt author).capsuleAt capsuleKey = some entry
  pinned : (pins.trace sourceAt author).pinned capsuleKey = true
  recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
    sourceAt author

/-- A positive current signer floor has one exact durable recovery-tip source.
The source comes from local storage and its active source pin. -/
theorem current_exact_round_gives_stable_recovery_tip_source
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {recoveryWait sourceAt author round : Time}
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (epochActive : (timed.execution.trace sourceAt).epochActive = true)
    (roundPositive : 0 < round)
    (currentFloor :
      ((timed.execution.trace sourceAt).validatorState
        author).highestSignedRound = round)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      sourceAt author) :
    Nonempty (ValidatorStableRecoveryTipSource pins recoveryWait sourceAt author
      round) := by
  have positiveTip : 0 < ((timed.execution.trace sourceAt).validatorState
      author).highestSignedRound := by
    rw [currentFloor]
    exact roundPositive
  rcases pins.current_positive_tip_has_pinned_capsule_source authorInRange
      authorCorrectAvailable epochActive positiveTip with
    ⟨block, capsuleKey, entry, ownAtFloor, targetBlock, stored, pinned,
      _source⟩
  have ownTip :
      ((timed.execution.trace sourceAt).validatorState author).ownBlockAt round =
        some block.reference := by
    rw [← currentFloor]
    exact ownAtFloor
  have exactRound : block.reference.round = round := by
    exact
      ((timed.execution.statesWellFormed sourceAt author authorInRange
        ).ownBlockIsSound round block.reference ownTip).2.1
  exact ⟨{
    block
    capsuleKey
    entry
    roundPositive
    currentFloor
    ownTip
    exactRound
    targetBlock
    stored
    pinned
    recoveryMode }⟩

/-- Each correct author eventually exposes its exact durable tip at one common
round. Source times can differ. They are results of the one-host catch-up
proof, not a common-round input to the protocol. -/
def EveryCorrectAvailableValidatorStableRecoveryTipSource
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recoveryWait observation round : Time) : Prop :=
  ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ∃ sourceAt,
      observation ≤ sourceAt ∧
        Nonempty (ValidatorStableRecoveryTipSource pins recoveryWait sourceAt
          author round)

/-- One durable recovery tip becomes a stable exact representative at one
correct receiver. The source can predate the modeled suffix. -/
theorem stable_recovery_tip_source_reaches_receiver
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation sourceAt author receiver round : Time}
    (source : ValidatorStableRecoveryTipSource pins recoveryWait sourceAt author
      round)
    (observationBeforeSource : observation ≤ sourceAt)
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (roundAboveReceiverGc :
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance :
      ¬ValidatorReceiverCommitAdvance timed observation receiver) :
    ∃ readyAt,
      observation ≤ readyAt ∧
        ∀ later, readyAt ≤ later →
          ((timed.execution.trace later).validatorState receiver
              ).acceptedRepresentative round author =
                some source.block.reference ∧
            ((timed.execution.trace later).validatorState receiver).retained
                source.block.reference = true := by
  have authorNotByzantine : faults.byzantine author = false := by
    have notNonProgress : faults.nonProgress author = false := by
      simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
        VoterSet.full] using authorCorrectAvailable
    have separated : faults.byzantine author = false ∧
        faults.unavailable author = false := by
      simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
        notNonProgress
    exact separated.1
  by_cases receiverIsAuthor : receiver = author
  · subst receiver
    have ownFacts :=
      (timed.execution.statesWellFormed sourceAt author authorInRange
        ).ownBlockIsSound round source.block.reference source.ownTip
    have recorded := representatives.acceptedCorrectReferenceIsRecorded
      sourceAt author source.block.reference authorInRange
        authorCorrectAvailable
        (by simpa [ownFacts.1] using authorInRange)
        (by simpa [ownFacts.1] using authorNotByzantine)
        ownFacts.2.2.1
    refine ⟨sourceAt, observationBeforeSource, ?_⟩
    intro later sourceBeforeLater
    have recordedLater := accepted_representative_persists_in_validator_execution
      timed.execution authorInRange sourceBeforeLater (by
        simpa [source.exactRound, ownFacts.1] using recorded)
    have sourceCurrent := pins.pin_persists_while_epoch_active
      sourceBeforeLater source.stored source.pinned (by
        intro time sourceBeforeTime _timeBeforeLater
        exact active time
          (Nat.le_trans observationBeforeSource sourceBeforeTime))
    have targetMember : source.entry.capsule.targetBlock ∈
        source.entry.capsule.history :=
      source.entry.capsule.target_and_parents_in_history.1
    have localTarget := pins.pinnedHistoryIsLocal later author
      source.capsuleKey source.entry sourceCurrent.1 sourceCurrent.2
        source.entry.capsule.targetBlock targetMember
    refine ⟨recordedLater, ?_⟩
    simpa [source.targetBlock] using localTarget.2.1
  · have activeFromSource : ∀ time, sourceAt ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time sourceBeforeTime
      exact active time (Nat.le_trans observationBeforeSource sourceBeforeTime)
    have currentTip :
        ((timed.execution.trace sourceAt).validatorState author).ownBlockAt
            ((timed.execution.trace sourceAt).validatorState
              author).highestSignedRound =
          some source.entry.capsule.targetBlock.reference := by
      rw [source.currentFloor, source.targetBlock]
      exact source.ownTip
    rcases broadcast.recovery_mode_delivers_pinned_tip_body effects
        authorInRange authorCorrectAvailable receiverInRange
          receiverCorrectAvailable receiverIsAuthor
            (Nat.le_trans afterGst observationBeforeSource)
              source.recoveryMode (by
                rw [source.currentFloor]
                exact source.roundPositive)
              currentTip source.stored source.pinned with
      ⟨packetId, packet, sourceBeforeDelivery, packetPresent,
        packetIsProtocol, _packetSender, packetReceiver, packetPayload,
        deliveryOccurs⟩
    have targetBody : ValidatorLocalBlockBodyAt timed packet.deliveredAt
        receiver source.entry.capsule.targetBlock :=
      .delivered packetId packet packetPresent packetIsProtocol packetReceiver
        packetPayload deliveryOccurs
    rcases
        ValidatorRecoveryCapsuleSyncExecution.delivered_target_installs_retained_above_gc_history_or_commit_advance
          pins capsuleSync acceptance authorInRange authorCorrectAvailable
            receiverInRange receiverCorrectAvailable
              (Nat.le_trans afterGst observationBeforeSource) activeFromSource
                source.stored source.pinned sourceBeforeDelivery targetBody with
      ⟨acceptedAt, sourceBeforeAccepted, headAdvanced | installedHistory⟩
    · have headAtObservationAtMostSource :=
        (timed.execution.durableStateMonotone receiver observation sourceAt
          receiverInRange observationBeforeSource).1
      exact False.elim (noAdvance ⟨acceptedAt,
        Nat.le_trans observationBeforeSource sourceBeforeAccepted,
        Nat.lt_of_le_of_lt headAtObservationAtMostSource headAdvanced⟩)
    have observationBeforeAccepted : observation ≤ acceptedAt :=
      Nat.le_trans observationBeforeSource sourceBeforeAccepted
    have targetMember : source.entry.capsule.targetBlock ∈
        source.entry.capsule.history :=
      source.entry.capsule.target_and_parents_in_history.1
    have targetAvailable := installedHistory.2
      source.entry.capsule.targetBlock targetMember
    have acceptedAndRetained :
        ((timed.execution.trace acceptedAt).validatorState
            receiver).accepted source.block.reference = true ∧
          ((timed.execution.trace acceptedAt).validatorState
            receiver).retained source.block.reference = true := by
      rcases targetAvailable with targetAtRoot | targetReady
      · have sameGc := no_receiver_commit_advance_keeps_gc_round
          receiverInRange receiverCorrectAvailable observationBeforeAccepted
            noAdvance
        rw [source.targetBlock, source.exactRound, sameGc] at targetAtRoot
        exact False.elim
          ((Nat.not_le_of_gt roundAboveReceiverGc) targetAtRoot)
      · simpa [source.targetBlock] using targetReady
    have recorded := representatives.acceptedCorrectReferenceIsRecorded
      acceptedAt receiver source.block.reference receiverInRange
        receiverCorrectAvailable
        (by
          have sound :=
            (timed.execution.statesWellFormed sourceAt author authorInRange
              ).ownBlockIsSound round source.block.reference source.ownTip
          simpa [sound.1] using authorInRange)
        (by
          have sound :=
            (timed.execution.statesWellFormed sourceAt author authorInRange
              ).ownBlockIsSound round source.block.reference source.ownTip
          simpa [sound.1] using authorNotByzantine)
        acceptedAndRetained.1
    have observationBeforePin : observation ≤ packet.deliveredAt + 1 :=
      Nat.le_trans observationBeforeSource
        (Nat.le_trans sourceBeforeDelivery (Nat.le_succ _))
    have gcAtPin := no_receiver_commit_advance_keeps_gc_round receiverInRange
      receiverCorrectAvailable observationBeforePin noAdvance
    have aboveGcAtPin :
        ((timed.execution.trace (packet.deliveredAt + 1)).validatorState
            receiver).gcRound < source.block.reference.round := by
      rw [gcAtPin, source.exactRound]
      exact roundAboveReceiverGc
    have pinAtDelivery := capsuleSync.localBodyLatchesPin packet.deliveredAt
      receiver source.entry.capsule.targetBlock receiverInRange
        receiverCorrectAvailable
        (active (packet.deliveredAt + 1) observationBeforePin)
        (by simpa [source.targetBlock] using aboveGcAtPin) targetBody
    let readyAt := max acceptedAt (packet.deliveredAt + 1)
    have acceptedBeforeReady : acceptedAt ≤ readyAt := Nat.le_max_left _ _
    have pinBeforeReady : packet.deliveredAt + 1 ≤ readyAt := Nat.le_max_right _ _
    refine ⟨readyAt, Nat.le_trans observationBeforeAccepted acceptedBeforeReady,
      ?_⟩
    intro later acceptedBeforeLater
    have recordedLater := accepted_representative_persists_in_validator_execution
      timed.execution receiverInRange
        (Nat.le_trans acceptedBeforeReady acceptedBeforeLater) (by
        simpa [source.exactRound] using recorded)
    have pinBeforeLater := Nat.le_trans pinBeforeReady acceptedBeforeLater
    have pinCurrent := capsuleSync.body_pin_persists_while_head_is_current
      receiverInRange receiverCorrectAvailable pinBeforeLater pinAtDelivery
        (by
          intro time pinBeforeTime _timeBeforeLater
          exact active time (Nat.le_trans observationBeforePin pinBeforeTime))
        (by
          intro time pinBeforeTime _timeBeforeLater
          have sameGc := no_receiver_commit_advance_keeps_gc_round
            receiverInRange receiverCorrectAvailable
              (Nat.le_trans observationBeforePin pinBeforeTime) noAdvance
          rw [sameGc, source.targetBlock, source.exactRound]
          exact roundAboveReceiverGc)
        (by
          intro time pinBeforeTime _timeBeforeLater
          have sameHead := no_receiver_commit_advance_keeps_head
            receiverInRange
              (Nat.le_trans observationBeforePin pinBeforeTime) noAdvance
          have pinHead := no_receiver_commit_advance_keeps_head receiverInRange
            observationBeforePin noAdvance
          rw [sameHead, pinHead]
          exact Nat.le_refl _)
    have sameGcLater := no_receiver_commit_advance_keeps_gc_round
      receiverInRange receiverCorrectAvailable
        (Nat.le_trans observationBeforePin pinBeforeLater) noAdvance
    have aboveGcLater :
        ((timed.execution.trace later).validatorState receiver).gcRound <
          source.block.reference.round := by
      rw [sameGcLater, source.exactRound]
      exact roundAboveReceiverGc
    have acceptedLater :=
      have blockAuthor : source.block.reference.author = author :=
        ((timed.execution.statesWellFormed sourceAt author authorInRange
          ).ownBlockIsSound round source.block.reference source.ownTip).1
      (representatives.representativeIsSound later receiver round author
        source.block.reference receiverInRange receiverCorrectAvailable
          (by simpa [blockAuthor] using recordedLater)).2.2
    have pinCurrentBlock :
        (capsuleSync.bodyPins later receiver).active source.block.reference =
          some ((timed.execution.trace
            (packet.deliveredAt + 1)).validatorState receiver).commitHead := by
      simpa [source.targetBlock] using pinCurrent
    have retainedLater := capsuleSync.acceptedPinnedBodyIsRetained later
      receiver source.block.reference _ receiverInRange
        receiverCorrectAvailable pinCurrentBlock aboveGcLater acceptedLater
    have blockAuthor : source.block.reference.author = author :=
      ((timed.execution.statesWellFormed sourceAt author authorInRange
        ).ownBlockIsSound round source.block.reference source.ownTip).1
    exact ⟨by simpa [blockAuthor] using recordedLater, retainedLater⟩

/-- Every correct, available author has a durable block in one round. This is
the block-production fact needed for a correct quorum layer. It does not
require a separate sent flag because the recovery source carries actual peer
rebroadcast work. -/
def EveryCorrectAvailableValidatorOwnBlockAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((world.validatorState validator).ownBlockAt round).isSome = true

/-- Durable own blocks for every correct author contain correct quorum stake. -/
theorem every_correct_own_block_gives_produced_quorum_layer
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {round : Nat}
    (owned : EveryCorrectAvailableValidatorOwnBlockAt faults world round) :
    ProducedCorrectQuorumLayer config faults world round := by
  have subset : VoterSet.SubsetAt config.authorityCount
      faults.correctAvailable
      (VoterSet.inter faults.correctAvailable (world.producedAuthors round)) := by
    intro author authorInRange authorCorrectAvailable
    simp [VoterSet.inter, ValidatorWorldState.producedAuthors,
      authorCorrectAvailable, owned author authorInRange authorCorrectAvailable]
  exact Nat.le_trans faults.correct_available_stake_is_quorum
    (weight_mono config.stake subset)

/-- One source family gives durable own blocks at one common later time. -/
theorem stable_recovery_tip_source_family_eventually_is_owned
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait observation round : Time}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (sources : EveryCorrectAvailableValidatorStableRecoveryTipSource pins
      recoveryWait observation round) :
    ∃ finish,
      observation ≤ finish ∧
        EveryCorrectAvailableValidatorOwnBlockAt faults
          (timed.execution.trace finish) round := by
  let authorDone := fun author time =>
    author < config.authorityCount ∧
      (((timed.execution.trace time).validatorState author).ownBlockAt
        round).isSome = true
  have authorDonePersists : ∀ author earlier later,
      earlier ≤ later → authorDone author earlier → authorDone author later := by
    intro author earlier later ordered done
    cases ownValue : ((timed.execution.trace earlier).validatorState
        author).ownBlockAt round with
    | none => simp [authorDone, ownValue] at done
    | some reference =>
        have durable := timed.execution.durableStateMonotone author earlier later
          done.1 ordered
        exact ⟨done.1, by
          simp [durable.own_block_persists ownValue]⟩
  have eachAuthor : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ finish, observation ≤ finish ∧ authorDone author finish := by
    intro author authorInRange authorCorrectAvailable
    rcases sources author authorInRange authorCorrectAvailable with
      ⟨sourceAt, observationBeforeSource, source⟩
    let tip := Classical.choice source
    exact ⟨sourceAt, observationBeforeSource, authorInRange, by
      simp [tip.ownTip]⟩
  rcases eventually_every_selected_validator faults.correctAvailable authorDone
      observation authorDonePersists eachAuthor with
    ⟨finish, observationBeforeFinish, allAuthors⟩
  exact ⟨finish, observationBeforeFinish, by
    intro author authorInRange authorCorrectAvailable
    exact (allAuthors author authorInRange authorCorrectAvailable).2⟩

/-- One source family becomes stable accepted and retained state at a fixed
correct receiver. -/
theorem stable_recovery_tip_source_family_reaches_receiver
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation receiver round : Time}
    (sources : EveryCorrectAvailableValidatorStableRecoveryTipSource pins
      recoveryWait observation round)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (roundAboveReceiverGc :
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ readyAt,
      observation ≤ readyAt ∧
        ∀ later, readyAt ≤ later → ∀ author,
          author < config.authorityCount →
          faults.correctAvailable author = true →
          ∃ reference,
            ((timed.execution.trace later).validatorState receiver
                ).acceptedRepresentative round author = some reference ∧
              ((timed.execution.trace later).validatorState receiver).retained
                  reference = true := by
  let authorDone := fun author readyAt =>
    ∀ later, readyAt ≤ later →
      ∃ reference,
        ((timed.execution.trace later).validatorState receiver
            ).acceptedRepresentative round author = some reference ∧
          ((timed.execution.trace later).validatorState receiver).retained
              reference = true
  have authorDonePersists : ∀ author earlier later,
      earlier ≤ later → authorDone author earlier → authorDone author later := by
    intro author earlier later ordered done future laterBeforeFuture
    exact done future (Nat.le_trans ordered laterBeforeFuture)
  have eachAuthor : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ readyAt, observation ≤ readyAt ∧ authorDone author readyAt := by
    intro author authorInRange authorCorrectAvailable
    rcases sources author authorInRange authorCorrectAvailable with
      ⟨sourceAt, observationBeforeSource, source⟩
    let tip := Classical.choice source
    rcases stable_recovery_tip_source_reaches_receiver pins broadcast effects
        capsuleSync acceptance representatives tip observationBeforeSource
          authorInRange authorCorrectAvailable receiverInRange
            receiverCorrectAvailable roundAboveReceiverGc afterGst active
              (no_correct_advance_implies_no_receiver_advance receiverInRange
                receiverCorrectAvailable noAdvance) with
      ⟨readyAt, observationBeforeReady, stable⟩
    exact ⟨readyAt, observationBeforeReady, by
      intro later readyBeforeLater
      exact ⟨tip.block.reference, stable later readyBeforeLater⟩⟩
  rcases eventually_every_selected_validator faults.correctAvailable authorDone
      observation authorDonePersists eachAuthor with
    ⟨readyAt, observationBeforeReady, allAuthors⟩
  exact ⟨readyAt, observationBeforeReady, by
    intro later readyBeforeLater author authorInRange authorCorrectAvailable
    exact allAuthors author authorInRange authorCorrectAvailable later
      readyBeforeLater⟩

/-- Stable accepted and retained representatives from all correct authors.
This early definition is used by the durable-tip bootstrap before the later
timer-paced window definitions. -/
def EveryCorrectAvailableValidatorStableTipAcceptedFrom
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (start round : Nat) : Prop :=
  ∀ later, start ≤ later → ∀ observer author,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ∃ reference,
      ((trace later).validatorState observer).acceptedRepresentative round
          author = some reference ∧
        ((trace later).validatorState observer).retained reference = true

/-- One source family becomes stable accepted and retained state at all
correct receivers. -/
theorem stable_recovery_tip_source_family_eventually_is_stably_common
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation round : Time}
    (sources : EveryCorrectAvailableValidatorStableRecoveryTipSource pins
      recoveryWait observation round)
    (roundAboveGc : ∀ receiver,
      receiver < config.authorityCount →
      faults.correctAvailable receiver = true →
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ readyAt,
      observation ≤ readyAt ∧
        EveryCorrectAvailableValidatorStableTipAcceptedFrom faults
          timed.execution.trace readyAt round := by
  let observerDone := fun observer readyAt =>
    ∀ later, readyAt ≤ later → ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ reference,
        ((timed.execution.trace later).validatorState observer
            ).acceptedRepresentative round author = some reference ∧
          ((timed.execution.trace later).validatorState observer).retained
              reference = true
  have observerDonePersists : ∀ observer earlier later,
      earlier ≤ later → observerDone observer earlier →
        observerDone observer later := by
    intro observer earlier later ordered done future laterBeforeFuture
      author authorInRange authorCorrectAvailable
    exact done future (Nat.le_trans ordered laterBeforeFuture) author
      authorInRange authorCorrectAvailable
  have eachObserver : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ∃ readyAt, observation ≤ readyAt ∧ observerDone observer readyAt := by
    intro observer observerInRange observerCorrectAvailable
    rcases stable_recovery_tip_source_family_reaches_receiver pins broadcast
        effects capsuleSync acceptance representatives sources observerInRange
          observerCorrectAvailable
            (roundAboveGc observer observerInRange observerCorrectAvailable)
              afterGst active noAdvance with
      ⟨readyAt, observationBeforeReady, allAuthors⟩
    exact ⟨readyAt, observationBeforeReady, allAuthors⟩
  rcases eventually_every_selected_validator faults.correctAvailable
      observerDone observation observerDonePersists eachObserver with
    ⟨readyAt, observationBeforeReady, allObservers⟩
  refine ⟨readyAt, observationBeforeReady, ?_⟩
  intro later readyBeforeLater observer author observerInRange
    observerCorrectAvailable authorInRange authorCorrectAvailable
  exact allObservers observer observerInRange observerCorrectAvailable later
    readyBeforeLater author authorInRange authorCorrectAvailable

/-- A common exact durable-tip family yields the first schedule-independent
stable correct quorum layer. -/
theorem stable_recovery_tip_source_family_gives_stable_common_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation round : Time}
    (sources : EveryCorrectAvailableValidatorStableRecoveryTipSource pins
      recoveryWait observation round)
    (roundAboveGc : ∀ receiver,
      receiver < config.authorityCount →
      faults.correctAvailable receiver = true →
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ finish,
      observation ≤ finish ∧
        EveryCorrectAvailableValidatorOwnBlockAt faults
          (timed.execution.trace finish) round ∧
        ProducedCorrectQuorumLayer config faults
          (timed.execution.trace finish) round ∧
        EveryCorrectAvailableValidatorStableTipAcceptedFrom faults
          timed.execution.trace finish round := by
  rcases stable_recovery_tip_source_family_eventually_is_owned sources with
    ⟨ownedAt, observationBeforeOwned, owned⟩
  rcases stable_recovery_tip_source_family_eventually_is_stably_common pins
      broadcast effects capsuleSync acceptance representatives sources
        roundAboveGc afterGst active noAdvance with
    ⟨stableAt, observationBeforeStable, stable⟩
  let finish := max ownedAt stableAt
  have ownedBeforeFinish : ownedAt ≤ finish := Nat.le_max_left _ _
  have stableBeforeFinish : stableAt ≤ finish := Nat.le_max_right _ _
  have ownedAtFinish : EveryCorrectAvailableValidatorOwnBlockAt faults
      (timed.execution.trace finish) round := by
    intro author authorInRange authorCorrectAvailable
    have ownSome := owned author authorInRange authorCorrectAvailable
    cases ownValue : ((timed.execution.trace ownedAt).validatorState
        author).ownBlockAt round with
    | none => simp [ownValue] at ownSome
    | some reference =>
        have durable := timed.execution.durableStateMonotone author ownedAt
          finish authorInRange ownedBeforeFinish
        simp [durable.own_block_persists ownValue]
  exact ⟨finish,
    Nat.le_trans observationBeforeOwned ownedBeforeFinish,
    ownedAtFinish,
    every_correct_own_block_gives_produced_quorum_layer ownedAtFinish,
    by
      intro later finishBeforeLater observer author observerInRange
        observerCorrectAvailable authorInRange authorCorrectAvailable
      exact stable later (Nat.le_trans stableBeforeFinish finishBeforeLater)
        observer author observerInRange observerCorrectAvailable authorInRange
          authorCorrectAvailable⟩

/-- One exact recovery-round block is available either from a proposal which
crossed the round in this suffix or from the author's current durable tip.
This sum prevents a slow receiver from losing a round after another author has
already moved to a later tip. -/
inductive ValidatorExactRecoveryRoundSource
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recoveryWait observation author round : Time) : Type
  | persisted
      (production : ValidatorPersistedProposalBroadcastProduction timed
        obligations observation author)
      (exactRound : production.proposal.block.reference.round = round)
  | durableTip
      (sourceAt : Time)
      (observationBeforeSource : observation ≤ sourceAt)
      (source : ValidatorStableRecoveryTipSource pins recoveryWait sourceAt
        author round)

/-- Every correct, available author has one exact source for the same recovery
round. The common round is a proof value, not a protocol input. -/
def EveryCorrectAvailableValidatorExactRecoveryRoundSource
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recoveryWait observation round : Time) : Prop :=
  ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    Nonempty (ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait observation author round)

/-- One exact recovery-round source leaves a durable own block at a later
time. -/
theorem exact_recovery_round_source_eventually_is_owned
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait observation author round : Time}
    (source : ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait observation author round) :
    ∃ readyAt reference,
      observation ≤ readyAt ∧
        ((timed.execution.trace readyAt).validatorState author).ownBlockAt
          round = some reference := by
  cases source with
  | persisted production exactRound =>
      exact ⟨production.finish,
        production.proposal.block.reference,
        Nat.le_trans production.startBeforePersistence
          (Nat.le_trans (Nat.le_succ _)
            production.persistenceBeforeFinish),
        by simpa [exactRound] using production.ownBlockStoredAtFinish⟩
  | durableTip sourceAt observationBeforeSource source =>
      exact ⟨sourceAt, source.block.reference, observationBeforeSource,
        source.ownTip⟩

/-- A GC-truncated causal history supplies one legal exact-next recovery parent
list when the local signer floor is above GC. No body at or below GC is used. -/
theorem gc_truncated_causal_history_supplies_exact_next_parents
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (state : ValidatorLocalState BlockId CommitId)
    {floor : Nat}
    (floorBelowTarget : floor < capsule.targetRound)
    (gcBelowFloor : state.gcRound < floor)
    (historyAvailable : ∀ block, block ∈ capsule.history →
      block.reference.round ≤ state.gcRound ∨
        (state.accepted block.reference = true ∧
          state.retained block.reference = true)) :
    ∃ parents,
      ValidatorProposalParentListReady .commitProgressRecovery config state
        (floor + 1) parents := by
  rcases causal_history_supplies_exact_next_parent_list capsule floorBelowTarget
      with ⟨child, _childMember, childRound, childValid, parentSources⟩
  have parentReady : ∀ parent, parent ∈ child.parents →
      parent.round + 1 = floor + 1 ∧
        state.accepted parent = true ∧
        state.retained parent = true ∧
        state.gcRound < parent.round := by
    intro parent parentMember
    have source := parentSources parent parentMember
    have parentRound : parent.round = floor := source.1
    have parentBody : ∃ block,
        block ∈ capsule.history ∧ block.reference = parent := by
      rcases source.2 with parentIsZero | ⟨block, member, reference⟩
      · omega
      · exact ⟨block, member, reference⟩
    rcases parentBody with ⟨block, member, reference⟩
    rcases historyAvailable block member with obsolete | available
    · rw [reference, parentRound] at obsolete
      omega
    · rw [reference] at available
      exact ⟨by omega, available.1, available.2, by omega⟩
  refine ⟨child.parents, ⟨⟨childValid.1, ?_, childValid.2.2⟩, ?_⟩⟩
  · intro parent parentMember
    have ready := parentReady parent parentMember
    exact ⟨ready.1, ready.2.1⟩
  · intro parent parentMember
    have ready := parentReady parent parentMember
    exact ⟨ready.2.2.1, Or.inr ready.2.2.2⟩

/-- One requester has discovered a pinned capsule and completed its parent-first
acceptance while the requester's commit head stays unchanged. The discovery
facts keep the requester-local body pins available for later exact-next rounds.
-/
structure ValidatorStablePinnedCausalHistory
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config)
    (rootStart holder requester : Time)
    (capsuleKey : ValidatorRecoveryCapsuleKey BlockId) where
  holderInRange : holder < config.authorityCount
  holderCorrectAvailable : faults.correctAvailable holder = true
  requesterInRange : requester < config.authorityCount
  requesterCorrectAvailable : faults.correctAvailable requester = true
  stored : (pins.trace rootStart holder).capsuleAt capsuleKey = some entry
  pinned : (pins.trace rootStart holder).pinned capsuleKey = true
  activeEpoch : ∀ time, rootStart ≤ time →
    (timed.execution.trace time).epochActive = true
  requesterHeadCurrent : ∀ time, rootStart ≤ time →
    ((timed.execution.trace time).validatorState requester).commitHead.index ≤
      ((timed.execution.trace rootStart).validatorState
        requester).commitHead.index
  discoveryFinish : Time
  acceptedFinish : Time
  rootBeforeDiscovery : rootStart ≤ discoveryFinish
  discoveryBeforeAccepted : discoveryFinish + 1 ≤ acceptedFinish
  discovered : ∀ block, block ∈ entry.capsule.history →
    ValidatorRecoveryCapsuleSyncExecution.BodyObservedOrGcRootAt
      (timed := timed) rootStart discoveryFinish requester block
  acceptedOrRoot : ∀ block, block ∈ entry.capsule.history →
    ((timed.execution.trace acceptedFinish).validatorState requester).accepted
          block.reference = true ∨
      block.reference.round ≤
        ((timed.execution.trace acceptedFinish).validatorState
          requester).gcRound

/-- A finite capsule history stays usable at one validator from one time.
Blocks at or below local GC are committed roots. Every later block is accepted
and retained. -/
def ValidatorCausalHistoryUsableFrom
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (start validator : Time) : Prop :=
  ∀ time, start ≤ time → ∀ block, block ∈ capsule.history →
    block.reference.round ≤
        ((timed.execution.trace time).validatorState validator).gcRound ∨
      (((timed.execution.trace time).validatorState validator).accepted
            block.reference = true ∧
        ((timed.execution.trace time).validatorState validator).retained
            block.reference = true)

namespace ValidatorStablePinnedCausalHistory

/-- The discovered history remains accepted and retained above the requester's
GC boundary at each later time for which the same local commit head remains
current. -/
theorem available_at
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config}
    {rootStart holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId}
    (history : ValidatorStablePinnedCausalHistory pins entry rootStart holder
      requester capsuleKey)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    {time : Time}
    (acceptedBeforeTime : history.acceptedFinish ≤ time) :
    ∀ block, block ∈ entry.capsule.history →
      block.reference.round ≤
          ((timed.execution.trace time).validatorState requester).gcRound ∨
        (((timed.execution.trace time).validatorState requester).accepted
              block.reference = true ∧
          ((timed.execution.trace time).validatorState requester).retained
              block.reference = true) := by
  have readyAtTime : ∀ block, block ∈ entry.capsule.history →
      ((timed.execution.trace time).validatorState requester).accepted
            block.reference = true ∨
        block.reference.round ≤
          ((timed.execution.trace time).validatorState requester).gcRound := by
    intro block blockMember
    rcases history.acceptedOrRoot block blockMember with accepted | atRoot
    · exact Or.inl (timed.execution.accepted_block_persists
        history.requesterInRange acceptedBeforeTime accepted)
    · exact Or.inr (Nat.le_trans atRoot
        (ValidatorRecoveryCapsuleSyncExecution.validator_gc_round_mono
          (timed := timed) history.requesterInRange acceptedBeforeTime))
  exact ValidatorRecoveryCapsuleSyncExecution.accepted_discovered_history_is_retained_above_gc
    sync
    history.requesterInRange history.requesterCorrectAvailable
      history.activeEpoch history.requesterHeadCurrent
      history.rootBeforeDiscovery
      (Nat.le_trans history.discoveryBeforeAccepted acceptedBeforeTime)
      history.discovered readyAtTime

/-- The stable discovered-history result gives the reusable suffix predicate. -/
theorem usable_from_accepted_finish
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config}
    {rootStart holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId}
    (history : ValidatorStablePinnedCausalHistory pins entry rootStart holder
      requester capsuleKey)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules) :
    ValidatorCausalHistoryUsableFrom timed entry.capsule
      history.acceptedFinish requester := by
  intro time acceptedBeforeTime
  exact history.available_at sync acceptedBeforeTime

end ValidatorStablePinnedCausalHistory

/-- A source-local capsule pin keeps its complete history usable while the
epoch remains active. -/
theorem source_pinned_history_is_usable_from
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {start holder : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (stored : (pins.trace start holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace start holder).pinned capsuleKey = true)
    (activeEpoch : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorCausalHistoryUsableFrom timed entry.capsule start holder := by
  intro time startBeforeTime block blockMember
  have currentPin := pins.pin_persists_while_epoch_active startBeforeTime
    stored pinned (by
      intro current startBeforeCurrent _currentBeforeTime
      exact activeEpoch current startBeforeCurrent)
  have localFacts := pins.pinnedHistoryIsLocal time holder capsuleKey entry
    currentPin.1 currentPin.2 block blockMember
  exact Or.inr ⟨localFacts.1, localFacts.2.1⟩

/-- In a suffix with no correct commit advance, one delivered pinned tip gives
the requester a causal history that remains usable for later exact-next work.
The only source is the holder's current durable tip and its local pin. -/
theorem stable_recovery_tip_installs_pinned_causal_history
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    {start holder requester : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (differentValidator : requester ≠ holder)
    (afterGst : network.gst ≤ start)
    (activeEpoch : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      start holder)
    (positiveTip : 0 < ((timed.execution.trace start).validatorState
      holder).highestSignedRound)
    (currentTip :
      ((timed.execution.trace start).validatorState holder).ownBlockAt
          ((timed.execution.trace start).validatorState
              holder).highestSignedRound =
        some entry.capsule.targetBlock.reference)
    (stored : (pins.trace start holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace start holder).pinned capsuleKey = true) :
    Nonempty (ValidatorStablePinnedCausalHistory pins entry start holder
      requester capsuleKey) := by
  rcases broadcast.recovery_mode_delivers_pinned_tip_body effects
      holderInRange holderCorrectAvailable requesterInRange
      requesterCorrectAvailable differentValidator afterGst recoveryMode
      positiveTip currentTip stored pinned with
    ⟨packetId, packet, startBeforeDelivery, packetPresent, packetIsProtocol,
      _packetSender, packetReceiver, packetPayload, deliveryOccurs⟩
  have targetBody : ValidatorLocalBlockBodyAt timed packet.deliveredAt requester
      entry.capsule.targetBlock :=
    .delivered packetId packet packetPresent packetIsProtocol packetReceiver
      packetPayload deliveryOccurs
  have requesterHeadCurrent : ∀ time, start ≤ time →
      ((timed.execution.trace time).validatorState requester).commitHead.index ≤
        ((timed.execution.trace start).validatorState
          requester).commitHead.index := by
    intro time startBeforeTime
    have sameHead := no_commit_advance_keeps_correct_commit_head
      requesterInRange requesterCorrectAvailable startBeforeTime noAdvance
    rw [sameHead]
    exact Nat.le_refl _
  rcases ValidatorRecoveryCapsuleSyncExecution.delivered_target_eventually_discovers_above_gc_history
      pins sync
      holderInRange holderCorrectAvailable requesterInRange
      requesterCorrectAvailable afterGst activeEpoch stored pinned
      requesterHeadCurrent startBeforeDelivery targetBody with
    ⟨discoveryFinish, deliveryBeforeDiscovery, discovered⟩
  rcases ValidatorRecoveryCapsuleSyncExecution.discovered_history_eventually_accepted_or_gc_root
      pins acceptance
      holderInRange holderCorrectAvailable requesterInRange
      requesterCorrectAvailable stored pinned discovered with
    ⟨acceptedFinish, discoveryBeforeAccepted, acceptedOrRoot⟩
  exact ⟨{
    holderInRange := holderInRange
    holderCorrectAvailable := holderCorrectAvailable
    requesterInRange := requesterInRange
    requesterCorrectAvailable := requesterCorrectAvailable
    stored := stored
    pinned := pinned
    activeEpoch := activeEpoch
    requesterHeadCurrent := requesterHeadCurrent
    discoveryFinish := discoveryFinish
    acceptedFinish := acceptedFinish
    rootBeforeDiscovery := Nat.le_trans startBeforeDelivery
      deliveryBeforeDiscovery
    discoveryBeforeAccepted := discoveryBeforeAccepted
    discovered := discovered
    acceptedOrRoot := acceptedOrRoot }⟩

/-- A proposal-persistence occurrence starts its execution batch below the
proposed round. Earlier actions in the same batch can only increase durable
state, so the action guard also bounds the batch-start signer floor. -/
private theorem recovery_world_step_signer_floor_monotone
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (validator : Nat) :
    (before.validatorState validator).highestSignedRound ≤
      (after.validatorState validator).highestSignedRound := by
  induction step with
  | nil => exact Nat.le_refl _
  | cons firstStep remainingSteps inductionHypothesis =>
      exact Nat.le_trans
        ((validator_atomic_step_durable_monotone firstStep validator)
          |>.2.2.2.2.2.2.1)
        inductionHypothesis

theorem persist_proposal_occurrence_starts_below_proposed_round
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {time validator : Time} {block : ValidatorBlock BlockId}
    (occurs : ValidatorLocalActionOccurs (execution.events time) validator
      (.persistProposal block)) :
    ((execution.trace time).validatorState validator).highestSignedRound <
      block.reference.round := by
  rcases occurs with ⟨headEvents, tailEvents, eventsExact⟩
  have completeStep := execution.stepsFollowRules time
  rw [eventsExact] at completeStep
  rcases validator_world_step_append_split completeStep with
    ⟨actionBefore, prefixStep, actionAndSuffix⟩
  cases actionAndSuffix with
  | cons actionStep _suffixStep =>
      have guard := validator_atomic_local_action_has_basic_guard actionStep
      have prefixFloorMonotone :
          ((execution.trace time).validatorState
              validator).highestSignedRound ≤
            (actionBefore.validatorState validator).highestSignedRound := by
        exact recovery_world_step_signer_floor_monotone prefixStep validator
      exact Nat.lt_of_le_of_lt prefixFloorMonotone guard.2.1

/-- One validator cannot persist the same proposal block in two different
batches. The first persistence raises the durable signer floor, so the basic
guard rejects the second persistence. -/
private theorem matching_persist_proposal_times_are_equal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator left right : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (leftOccurs : ValidatorLocalActionOccurs (timed.execution.events left)
      validator (.persistProposal block))
    (rightOccurs : ValidatorLocalActionOccurs (timed.execution.events right)
      validator (.persistProposal block)) :
    left = right := by
  have notLeftBeforeRight : ¬left < right := by
    intro leftBeforeRight
    have stored := persist_proposal_occurrence_stores_own_block
      timed.execution leftOccurs
    have floorAtLeft :=
      (timed.execution.statesWellFormed (left + 1) validator validatorInRange)
        |>.ownBlockDoesNotExceedSignerFloor block.reference.round
          block.reference stored
    have floorMonotone :=
      (timed.execution.durableStateMonotone validator (left + 1) right
        validatorInRange (Nat.succ_le_iff.mpr leftBeforeRight)).2.2.2.2.2.2.1
    have rightBelow := persist_proposal_occurrence_starts_below_proposed_round
      timed.execution rightOccurs
    exact (Nat.not_lt_of_ge (Nat.le_trans floorAtLeft floorMonotone)) rightBelow
  have notRightBeforeLeft : ¬right < left := by
    intro rightBeforeLeft
    have stored := persist_proposal_occurrence_stores_own_block
      timed.execution rightOccurs
    have floorAtRight :=
      (timed.execution.statesWellFormed (right + 1) validator validatorInRange)
        |>.ownBlockDoesNotExceedSignerFloor block.reference.round
          block.reference stored
    have floorMonotone :=
      (timed.execution.durableStateMonotone validator (right + 1) left
        validatorInRange (Nat.succ_le_iff.mpr rightBeforeLeft)).2.2.2.2.2.2.1
    have leftBelow := persist_proposal_occurrence_starts_below_proposed_round
      timed.execution leftOccurs
    exact (Nat.not_lt_of_ge (Nat.le_trans floorAtRight floorMonotone)) leftBelow
  exact Nat.le_antisymm (Nat.le_of_not_gt notRightBeforeLeft)
    (Nat.le_of_not_gt notLeftBeforeRight)

/-- One exact proposal packet sent to one peer from timer-paced work. The
packet result does not expose the internal proposal obligation. -/
structure ValidatorTimerPacedPeerBroadcast
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (snapshot : ValidatorProposalSnapshot config faults timed.execution.trace)
    (validator receiver proposalActionAt : Nat) where
  packetId : PacketId
  packet : AddressedPacket (ValidatorMessage BlockId CommitId)
  packetInTrace :
    (timed.execution.trace packet.sentAt).packets packetId = some packet
  packetIsProtocol : protocolPacket packet
  packetSender : packet.sender = validator
  packetReceiver : packet.receiver = receiver
  packetPayload : packet.payload = .block snapshot.block
  proposalBeforeSend : proposalActionAt + 1 ≤ packet.sentAt
  storedBeforeSend : snapshot.storedAt ≤ packet.sentAt
  sentWithinPipeline :
    packet.sentAt ≤
      proposalActionAt + 2 * (timed.localActionBound + 1) + 1

/-- Erase one per-peer proposal obligation but keep its exact packet and finite
local send bound. -/
theorem strict_recovery_broadcast_gives_peer_packet
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {obligations : ValidatorProposalObligationExecution timed}
    {validator selectedReceiver receiver : Nat}
    (result : ValidatorStrictRecoveryBroadcast timed waits timerSource
      obligations validator selectedReceiver)
    (receiverInRange : receiver < config.authorityCount)
    (differentReceiver : receiver ≠ validator) :
    Nonempty (ValidatorTimerPacedPeerBroadcast timed result.snapshot validator
      receiver result.proposalActionAt) := by
  rcases result.latched.2.2.2.2.2.2 receiver receiverInRange
      differentReceiver with ⟨broadcast⟩
  refine ⟨
    { packetId := broadcast.packetId
      packet := broadcast.packet
      packetInTrace := by
        rw [broadcast.packetSentAt]
        exact broadcast.packetInTrace
      packetIsProtocol := broadcast.packetIsProtocol
      packetSender := broadcast.packetSender
      packetReceiver := broadcast.packetReceiver
      packetPayload := by
        rw [broadcast.packetPayload, result.exactBlock]
      proposalBeforeSend := by
        rw [broadcast.packetSentAt]
        exact Nat.le_trans broadcast.readyBeforePersistence
          (Nat.le_trans (Nat.le_add_right broadcast.persistedAt 1)
            (Nat.le_trans broadcast.persistenceBeforeSend
              (Nat.le_add_right broadcast.sendActionAt 1)))
      storedBeforeSend := by
        have samePersistence : result.persistTime = broadcast.persistedAt :=
          matching_persist_proposal_times_are_equal
            result.ready.validatorInRange result.persistenceOccurs (by
              simpa [result.exactBlock] using broadcast.persistenceOccurs)
        rw [result.storedAfterPersistence, samePersistence,
          broadcast.packetSentAt]
        exact Nat.le_trans broadcast.persistenceBeforeSend
          (Nat.le_add_right broadcast.sendActionAt 1)
      sentWithinPipeline := by
        rw [broadcast.packetSentAt]
        have sendBound := Nat.succ_le_succ broadcast.sendWithinBound
        have persistedBound := Nat.add_le_add_right
          broadcast.persistenceWithinBound
          (1 + timed.localActionBound + 1)
        calc
          broadcast.sendActionAt + 1 ≤
              (broadcast.persistedAt + 1 + timed.localActionBound) + 1 :=
            sendBound
          _ = broadcast.persistedAt +
                (1 + timed.localActionBound + 1) := by
            simp only [Nat.add_assoc]
          _ ≤ (result.proposalActionAt + 1 + timed.localActionBound) +
                (1 + timed.localActionBound + 1) := persistedBound
          _ = result.proposalActionAt +
                2 * (timed.localActionBound + 1) + 1 := by
            exact recovery_peer_send_bound_arithmetic _ _ }⟩

/-- One exact proposal snapshot that is derived from a durable recovery timer.

This result keeps the data needed by the direct-vote proof. It does not expose
the internal timer-ready record as an input or result. -/
structure ValidatorTimerPacedRoundProduction
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
    (validator round : Nat) where
  snapshot : ValidatorProposalSnapshot config faults timed.execution.trace
  commitHead : ValidatorCommitHead CommitId
  parentReadyAt : Time
  timerStartedAt : Time
  timerStartsExactNext :
    round =
      ((timed.execution.trace timerStartedAt).validatorState
        validator).highestSignedRound + 1
  proposalActionAt : Time
  persistTime : Time
  sentTime : Time
  proposer : snapshot.proposer = validator
  blockRound : snapshot.block.reference.round = round
  snapshotAtDeadline :
    snapshot.snapshotAt = timerStartedAt + waits.wait commitHead round
  timerStartsAfterParentReady : parentReadyAt ≤ timerStartedAt
  timerStartWithinLocalBound :
    timerStartedAt ≤ parentReadyAt + timed.localActionBound + 2
  deadlineBeforeProposal :
    timerStartedAt + waits.wait commitHead round ≤ proposalActionAt
  proposalWithinLocalBound :
    proposalActionAt ≤
      timerStartedAt + waits.wait commitHead round + timed.localActionBound
  proposalBeforePersistence : proposalActionAt + 1 ≤ persistTime
  snapshotBeforePersistence : snapshot.snapshotAt ≤ persistTime
  validParents : snapshot.block.HasQuorumImmediateParents config
  refreshedParentList :
    ValidatorRefreshedRecoveryParentListAt config
      ((timed.execution.trace snapshot.snapshotAt).validatorState validator)
      round snapshot.block.parents
  persistenceOccurs : ValidatorLocalActionOccurs
    (timed.execution.events persistTime) validator
      (.persistProposal snapshot.block)
  storedAfterPersistence : snapshot.storedAt = persistTime + 1
  storedWithinPipelinePrefix :
    snapshot.storedAt ≤
      timerStartedAt + waits.wait commitHead round +
        2 * (timed.localActionBound + 1)
  storedBeforeSent : snapshot.storedAt ≤ sentTime
  sentOwnBlock :
    ((timed.execution.trace sentTime).validatorState validator).sentOwnBlockAt
      round = true
  peerBroadcast : ∀ receiver,
    receiver < config.authorityCount →
    receiver ≠ validator →
    Nonempty (ValidatorTimerPacedPeerBroadcast timed snapshot validator
      receiver proposalActionAt)

/-- If round `r` was still above one host's signer floor at an observation,
that host's timer for round `r + 1` starts strictly after the observation.

This is the fresh-target rule used by the favorable-window proof. It prevents
an old round-`r + 1` timer or proposal snapshot from satisfying a newly chosen
round. -/
theorem fresh_next_round_timer_starts_after_observation
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
    {observation validator round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      (round + 1))
    (roundAboveObservedFloor :
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound < round) :
    observation < production.timerStartedAt := by
  apply Nat.lt_of_not_ge
  intro timerBeforeObservation
  have validatorInRange : validator < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have floorMonotone :=
    (timed.execution.durableStateMonotone validator production.timerStartedAt
      observation validatorInRange timerBeforeObservation).2.2.2.2.2.2.1
  have floorAtTimer :
      ((timed.execution.trace production.timerStartedAt).validatorState
        validator).highestSignedRound = round := by
    have exactNext := production.timerStartsExactNext
    omega
  rw [floorAtTimer] at floorMonotone
  omega

/-- The proposal snapshot of a fresh round-`r + 1` timer is also strictly
after the observation at which round `r` was not yet signed. -/
theorem fresh_next_round_snapshot_is_after_observation
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
    {observation validator round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      (round + 1))
    (roundAboveObservedFloor :
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound < round) :
    observation < production.snapshot.snapshotAt := by
  have observationBeforeTimer :=
    fresh_next_round_timer_starts_after_observation production
      roundAboveObservedFloor
  have timerBeforeSnapshot : production.timerStartedAt ≤
      production.snapshot.snapshotAt := by
    rw [production.snapshotAtDeadline]
    exact Nat.le_add_right _ _
  exact Nat.lt_of_lt_of_le observationBeforeTimer timerBeforeSnapshot

/-- An exact per-peer timer-paced packet sent after GST is delivered within
the normal network bound. -/
theorem timer_paced_peer_broadcast_is_delivered
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
    {validator receiver round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      validator receiver production.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt) :
    broadcast.packet.sentAt ≤ broadcast.packet.deliveredAt ∧
      broadcast.packet.deliveredAt ≤
        broadcast.packet.sentAt + network.delta ∧
      ValidatorPacketDeliveryOccurs
        (timed.execution.events broadcast.packet.deliveredAt)
        broadcast.packetId := by
  have senderInRange : broadcast.packet.sender < config.authorityCount := by
    rw [broadcast.packetSender]
    simpa [production.proposer] using production.snapshot.proposerInRange
  have senderCorrectAvailable :
      faults.correctAvailable broadcast.packet.sender = true := by
    rw [broadcast.packetSender]
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  have receiverInRangeAtPacket :
      broadcast.packet.receiver < config.authorityCount := by
    simpa [broadcast.packetReceiver] using receiverInRange
  have receiverCorrectAtPacket :
      faults.correctAvailable broadcast.packet.receiver = true := by
    simpa [broadcast.packetReceiver] using receiverCorrectAvailable
  have bounds := network.postGstDelivery broadcast.packet
    broadcast.packetIsProtocol senderInRange receiverInRangeAtPacket
    senderCorrectAvailable receiverCorrectAtPacket sentAfterGst
  have delivered := timed.execution.protocolPacketsAreDelivered
    broadcast.packetId broadcast.packet broadcast.packetInTrace
    broadcast.packetIsProtocol senderInRange receiverInRangeAtPacket
    senderCorrectAvailable receiverCorrectAtPacket sentAfterGst
  exact ⟨bounds.1, bounds.2, delivered⟩

/-- Each exact per-peer packet is sent within a fixed local pipeline after the
round deadline. -/
theorem timer_paced_peer_broadcast_sent_within_round_pipeline
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
    {validator receiver round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      validator receiver production.proposalActionAt) :
    broadcast.packet.sentAt ≤
      production.timerStartedAt +
        waits.wait production.commitHead round +
          3 * (timed.localActionBound + 1) := by
  apply Nat.le_trans broadcast.sentWithinPipeline
  exact recovery_round_pipeline_arithmetic _ _ _ _
    production.proposalWithinLocalBound

/-- Once the direct parents of one delivered timer-paced proposal are accepted,
the receiver accepts that proposal through its protected local task. -/
theorem timer_paced_peer_broadcast_is_accepted_after_parents
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
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    {validator receiver round parentsReadyAt : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      validator receiver production.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (deliveryBeforeParentsReady : broadcast.packet.deliveredAt + 1 ≤
      parentsReadyAt)
    (blockAboveGcAtParentsReady :
      ((timed.execution.trace parentsReadyAt).validatorState receiver).gcRound <
        production.snapshot.block.reference.round)
    (parentsAccepted : ∀ parent,
      parent ∈ production.snapshot.block.parents →
      ((timed.execution.trace parentsReadyAt).validatorState
        receiver).accepted parent = true) :
    ∃ acceptedAt,
      parentsReadyAt ≤ acceptedAt ∧
        acceptedAt ≤ parentsReadyAt + timed.localActionBound + 1 ∧
        ((timed.execution.trace acceptedAt).validatorState receiver).accepted
          production.snapshot.block.reference = true := by
  have deliveredFacts := timer_paced_peer_broadcast_is_delivered production
    broadcast receiverInRange receiverCorrectAvailable sentAfterGst
  have packetAtDelivery := timed.execution.packetHistoryMonotone
    broadcast.packet.sentAt broadcast.packet.deliveredAt deliveredFacts.1
    broadcast.packetId broadcast.packet broadcast.packetInTrace
  have receiverInRangeAtPacket :
      broadcast.packet.receiver < config.authorityCount := by
    simpa [broadcast.packetReceiver] using receiverInRange
  have receiverCorrectAtPacket :
      faults.correctAvailable broadcast.packet.receiver = true := by
    simpa [broadcast.packetReceiver] using receiverCorrectAvailable
  have authorInRange :
      production.snapshot.block.reference.author < config.authorityCount := by
    rw [production.snapshot.blockIsOwnProposal]
    exact production.snapshot.proposerInRange
  have parentsAtPacket : ∀ parent,
      parent ∈ production.snapshot.block.parents →
      ((timed.execution.trace parentsReadyAt).validatorState
        broadcast.packet.receiver).accepted parent = true := by
    intro parent member
    simpa [broadcast.packetReceiver] using parentsAccepted parent member
  rcases delivered_block_with_ready_parents_is_accepted timed acceptance
      packetAtDelivery broadcast.packetPayload deliveredFacts.2.2
      receiverInRangeAtPacket receiverCorrectAtPacket authorInRange
      (Or.inr production.validParents) deliveryBeforeParentsReady
      (by simpa [broadcast.packetReceiver] using blockAboveGcAtParentsReady)
      parentsAtPacket with
    ⟨acceptedAt, afterParents, withinBound, accepted⟩
  exact ⟨acceptedAt, afterParents, withinBound,
    by simpa [broadcast.packetReceiver] using accepted⟩

/-- A timer for the round after one persisted recovery block cannot predate
that block's persistence. Thus, an observed or restored warm-up round is not
part of the later strict window; the next timer starts fresh. -/
theorem next_recovery_round_timer_starts_after_previous_persistence
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
    (previous : ValidatorTimerPacedRoundProduction timed waits validator
      round)
    (next : ValidatorTimerPacedRoundProduction timed waits validator
      (round + 1)) :
    previous.persistTime + 1 ≤ next.timerStartedAt := by
  have persistStartsBelow :=
    persist_proposal_occurrence_starts_below_proposed_round timed.execution
      previous.persistenceOccurs
  rw [previous.blockRound] at persistStartsBelow
  by_cases startBeforePersist :
      next.timerStartedAt ≤ previous.persistTime
  · have validatorInRange : validator < config.authorityCount := by
      simpa [next.proposer] using next.snapshot.proposerInRange
    have floorMonotone :=
      (timed.execution.durableStateMonotone validator next.timerStartedAt
        previous.persistTime validatorInRange startBeforePersist).2.2.2.2.2.2.1
    have floorAtStart :
        ((timed.execution.trace next.timerStartedAt).validatorState
          validator).highestSignedRound = round := by
      have exactAtStart := next.timerStartsExactNext
      omega
    rw [floorAtStart] at floorMonotone
    omega
  · exact Nat.succ_le_iff.mpr (Nat.lt_of_not_ge startBeforePersist)

/-- Erase one internal ready record but keep its concrete timer, proposal,
persistence, and send evidence. -/
theorem strict_recovery_broadcast_gives_timer_paced_round_production
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {obligations : ValidatorProposalObligationExecution timed}
    {validator receiver round : Nat}
    (result : ValidatorStrictRecoveryBroadcast timed waits timerSource
      obligations validator receiver)
    (targetRound : result.targetRound = round) :
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        round,
      production.parentReadyAt = result.ready.parentReadyAt ∧
        production.timerStartedAt = result.ready.startedAt ∧
        production.persistTime = result.persistTime ∧
        production.sentTime = result.packet.sentAt := by
  let production : ValidatorTimerPacedRoundProduction timed waits validator
      round :=
    { snapshot := result.snapshot
      commitHead := result.commitHead
      parentReadyAt := result.ready.parentReadyAt
      timerStartedAt := result.ready.startedAt
      timerStartsExactNext := by
        have exactAtStart := timerSource.targetIsExactNextAtStart
          result.pacedProposal.start result.pacedProposal.timerStarted
        calc
          round = result.targetRound := targetRound.symm
          _ = result.pacedProposal.start.targetRound :=
            result.pacedTimerTargetRound.symm
          _ = ((timed.execution.trace
                result.pacedProposal.start.startedAt).validatorState
                result.pacedProposal.start.validator).highestSignedRound + 1 :=
            exactAtStart
          _ = ((timed.execution.trace result.ready.startedAt).validatorState
                validator).highestSignedRound + 1 := by
            rw [result.pacedTimerStartedAt,
              result.pacedProposal.startValidator]
      proposalActionAt := result.proposalActionAt
      persistTime := result.persistTime
      sentTime := result.packet.sentAt
      proposer := result.completed.snapshotProposer
      blockRound := by
        rw [result.completed.snapshotRound]
        exact targetRound
      snapshotAtDeadline := result.completed.snapshotAtDeadline.trans
        (congrArg
          (fun target => result.ready.startedAt +
            waits.wait result.commitHead target)
          targetRound)
      timerStartsAfterParentReady := result.ready.startsAfterParentReady
      timerStartWithinLocalBound := result.timerStartWithinLocalBound
      deadlineBeforeProposal := by
        simpa [targetRound] using result.deadlineBeforeProposal
      proposalWithinLocalBound := by
        simpa [targetRound] using result.proposalWithinLocalBound
      proposalBeforePersistence := result.proposalBeforePersistence
      snapshotBeforePersistence := by
        rw [result.completed.snapshotAtDeadline]
        exact Nat.le_trans result.deadlineBeforeProposal
          (Nat.le_trans (Nat.le_add_right result.proposalActionAt 1)
            result.proposalBeforePersistence)
      validParents := result.completed.validParents
      refreshedParentList := by
        rw [result.completed.snapshotAtDeadline,
          result.completed.snapshotParents, ← targetRound]
        exact result.refreshedParentList
      persistenceOccurs := result.persistenceOccurs
      storedAfterPersistence := result.storedAfterPersistence
      storedWithinPipelinePrefix := by
        simpa [targetRound] using result.storedWithinPipelinePrefix
      storedBeforeSent := result.broadcast.storedBeforeSend
      sentOwnBlock := by
        simpa only [result.completed.snapshotProposer,
          result.completed.snapshotRound, targetRound] using
          result.broadcast.sentRecord
      peerBroadcast := by
        intro receiver receiverInRange differentReceiver
        exact strict_recovery_broadcast_gives_peer_packet result
          receiverInRange differentReceiver }
  exact ⟨production, rfl, rfl, rfl, rfl⟩

/-- A past current-timer origin reconstructs the strict protected proposal and
broadcast for that exact target. All future persistence and send facts are
derived from bounded local work. -/
theorem current_timer_snapshot_origin_gives_timer_paced_round_production
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {obligations : ValidatorProposalObligationExecution timed}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {persistTime validator : Time} {block : ValidatorBlock BlockId}
    (origin : ValidatorCurrentTimerProposalOriginAt timerSource persistTime
      validator block)
    (active : ValidatorEpochActiveBetween timed.execution.trace
      origin.start.startedAt (origin.start.deadline waits))
    {receiver : Nat}
    (receiverInRange : receiver < config.authorityCount)
    (differentReceiver : receiver ≠ validator) :
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        block.reference.round,
      production.snapshot.block = block ∧
        production.persistTime = persistTime ∧
        production.timerStartedAt = origin.start.startedAt ∧
        production.snapshot.snapshotAt = origin.start.deadline waits := by
  have validatorInRange : validator < config.authorityCount := by
    simpa [origin.startValidator] using
      timerSource.validatorInRange origin.start origin.timerStarted
  have validatorCorrectAvailable :
      faults.correctAvailable validator = true := by
    simpa [origin.startValidator] using
      timerSource.validatorCorrectAvailable origin.start origin.timerStarted
  have authorityCountAtLeastTwo : 1 < config.authorityCount := by
    by_cases receiverBeforeValidator : receiver < validator
    · have validatorPositive : 0 < validator :=
        Nat.lt_of_le_of_lt (Nat.zero_le receiver) receiverBeforeValidator
      exact Nat.lt_of_le_of_lt validatorPositive validatorInRange
    · have validatorBeforeReceiver : validator < receiver :=
        Nat.lt_of_le_of_ne (Nat.le_of_not_gt receiverBeforeValidator)
          differentReceiver.symm
      have receiverPositive : 0 < receiver :=
        Nat.lt_of_le_of_lt (Nat.zero_le validator) validatorBeforeReceiver
      exact Nat.lt_of_le_of_lt receiverPositive receiverInRange
  have headAtDeadline :
      ((timed.execution.trace (origin.start.deadline waits)).validatorState
        origin.start.validator).commitHead = origin.start.commitHead := by
    simpa [origin.startValidator] using origin.headAtDeadline
  let originReady : ValidatorOriginAwareRecoveryProposalReady faults timed waits
      origin.start.commitHead origin.start.targetRound
        origin.start.validator :=
    timerSource.originAwareReadyOfTimerStart origin.start origin.timerStarted
      headAtDeadline active
  have originReadyParents : originReady.ready.parents = origin.parents := by
    change timerSource.refreshedParents origin.start
      (origin.start.deadline waits) = origin.parents
    exact origin.refreshedParents
  rcases current_timer_proposal_origin_builds_exact_snapshot effects origin with
    ⟨snapshot, snapshotProposer, snapshotBlock, snapshotAt, snapshotStored,
      refreshedParentList⟩
  rcases latchSource.proposeNextResultIsLatched origin.proposalActionAt
      validator origin.parents block origin.proposalOccurs origin.blockAuthor
        origin.blockParents origin.persistenceEnabled with
    ⟨proposal, proposalLatched, _proposalLatchedAt, _proposalOrigin,
      proposalBlock⟩
  have proposalReadyAtLatch :
      (obligations.trace (origin.proposalActionAt + 1) validator).readyProposal =
        some proposal :=
    latch_event_sets_ready_proposal obligations.transitionsFollowRules
      proposalLatched
  rcases latched_proposal_runs_within_bound obligations validatorInRange
      validatorCorrectAvailable proposalReadyAtLatch with
    ⟨completion, _completionAfterLatch, completionWithinBound,
      completionOccurs⟩
  have completionOccursForBlock : ValidatorLocalActionOccurs
      (timed.execution.events completion.event.completedAt) validator
        (.persistProposal block) := by
    simpa only [proposalBlock] using completionOccurs
  have completionIsPersistence : completion.event.completedAt = persistTime :=
    matching_persist_proposal_times_are_equal validatorInRange
      completionOccursForBlock origin.persistenceOccurs
  have persistenceWithinLocalBound :
      persistTime ≤ origin.proposalActionAt + 1 + timed.localActionBound := by
    calc
      persistTime = completion.event.completedAt := completionIsPersistence.symm
      _ ≤ origin.proposalActionAt + 1 + timed.localActionBound :=
        completionWithinBound
  have startBeforePersistence : origin.start.startedAt ≤ persistTime := by
    exact Nat.le_trans (by
        simp [ValidatorRecoveryTimerStart.deadline])
      (Nat.le_trans origin.deadlineBeforeProposal
        (Nat.le_trans (Nat.le_succ origin.proposalActionAt)
          origin.proposalBeforePersistence))
  have targetIsExactNext := timerSource.targetIsExactNextAtStart origin.start
    origin.timerStarted
  have blockAboveStartFloor :
      ((timed.execution.trace origin.start.startedAt).validatorState
          validator).highestSignedRound < block.reference.round := by
    calc
      ((timed.execution.trace origin.start.startedAt).validatorState
          validator).highestSignedRound =
          ((timed.execution.trace origin.start.startedAt).validatorState
            origin.start.validator).highestSignedRound := by
              rw [origin.startValidator]
      _ < origin.start.targetRound := by omega
      _ = block.reference.round := origin.proposalTarget
  rcases persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable startBeforePersistence blockAboveStartFloor
          origin.persistenceOccurs with
    ⟨exactBroadcast⟩
  let broadcastProduction := exactBroadcast.1
  have broadcastPersistedAt : broadcastProduction.persistedAt = persistTime :=
    exactBroadcast.2.1
  have broadcastBlock : broadcastProduction.proposal.block = block :=
    exactBroadcast.2.2
  have validParents : snapshot.block.HasQuorumImmediateParents config := by
    refine ⟨snapshot.parentAuthorsNodup, snapshot.parentsAreImmediate, ?_⟩
    change config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (validatorParentAuthors snapshot.block.parents)
    rw [snapshotBlock, origin.blockParents]
    rw [← originReadyParents]
    exact originReady.refreshedRecoveryParents.ready.1.2.2
  let production : ValidatorTimerPacedRoundProduction timed waits validator
      block.reference.round :=
    { snapshot
      commitHead := origin.start.commitHead
      parentReadyAt := originReady.ready.parentReadyAt
      timerStartedAt := origin.start.startedAt
      timerStartsExactNext := by
        calc
          block.reference.round = origin.start.targetRound :=
            origin.proposalTarget.symm
          _ = ((timed.execution.trace origin.start.startedAt).validatorState
                origin.start.validator).highestSignedRound + 1 :=
            targetIsExactNext
          _ = ((timed.execution.trace origin.start.startedAt).validatorState
                validator).highestSignedRound + 1 := by
            rw [origin.startValidator]
      proposalActionAt := origin.proposalActionAt
      persistTime
      sentTime := broadcastProduction.finish
      proposer := snapshotProposer
      blockRound := by rw [snapshotBlock]
      snapshotAtDeadline := by
        rw [snapshotAt]
        simp [ValidatorRecoveryTimerStart.deadline, origin.proposalTarget]
      timerStartsAfterParentReady := originReady.ready.startsAfterParentReady
      timerStartWithinLocalBound := originReady.startedWithinLocalBound
      deadlineBeforeProposal := by
        simpa [ValidatorRecoveryTimerStart.deadline,
          origin.proposalTarget] using origin.deadlineBeforeProposal
      proposalWithinLocalBound := by
        simpa [ValidatorRecoveryTimerStart.deadline,
          origin.proposalTarget] using origin.proposalWithinLocalBound
      proposalBeforePersistence := origin.proposalBeforePersistence
      snapshotBeforePersistence := by
        rw [snapshotAt]
        exact Nat.le_trans origin.deadlineBeforeProposal
          (Nat.le_trans (Nat.le_succ origin.proposalActionAt)
            origin.proposalBeforePersistence)
      validParents
      refreshedParentList
      persistenceOccurs := by
        simpa only [snapshotBlock] using origin.persistenceOccurs
      storedAfterPersistence := snapshotStored
      storedWithinPipelinePrefix := by
        rw [snapshotStored]
        have proposalWithin : origin.proposalActionAt ≤
            origin.start.startedAt +
                waits.wait origin.start.commitHead block.reference.round +
              timed.localActionBound := by
          simpa [ValidatorRecoveryTimerStart.deadline,
            origin.proposalTarget] using origin.proposalWithinLocalBound
        calc
          persistTime + 1 ≤
              (origin.proposalActionAt + 1 + timed.localActionBound) + 1 :=
            Nat.succ_le_succ persistenceWithinLocalBound
          _ ≤ origin.start.startedAt +
                waits.wait origin.start.commitHead block.reference.round +
              2 * (timed.localActionBound + 1) :=
            recovery_exact_persistence_bound_arithmetic _ _ _ _
              proposalWithin
      storedBeforeSent := by
        rw [snapshotStored]
        simpa only [broadcastPersistedAt] using
          broadcastProduction.persistenceBeforeFinish
      sentOwnBlock := by
        simpa only [broadcastBlock] using
          broadcastProduction.sentOwnBlockAtFinish
      peerBroadcast := by
        intro peer peerInRange differentPeer
        rcases broadcastProduction.broadcasts peer peerInRange differentPeer with
          ⟨broadcast⟩
        have broadcastPersistence : ValidatorLocalActionOccurs
            (timed.execution.events broadcast.persistedAt) validator
              (.persistProposal block) := by
          simpa only [broadcastBlock] using broadcast.persistenceOccurs
        have samePersistence : broadcast.persistedAt = persistTime :=
          matching_persist_proposal_times_are_equal validatorInRange
            broadcastPersistence origin.persistenceOccurs
        refine ⟨
          { packetId := broadcast.packetId
            packet := broadcast.packet
            packetInTrace := by
              rw [broadcast.packetSentAt]
              exact broadcast.packetInTrace
            packetIsProtocol := broadcast.packetIsProtocol
            packetSender := broadcast.packetSender
            packetReceiver := broadcast.packetReceiver
            packetPayload := by
              rw [broadcast.packetPayload, broadcastBlock, snapshotBlock]
            proposalBeforeSend := by
              rw [broadcast.packetSentAt]
              calc
                origin.proposalActionAt + 1 ≤ persistTime :=
                  origin.proposalBeforePersistence
                _ ≤ broadcast.persistedAt + 1 := by
                  rw [samePersistence]
                  exact Nat.le_succ _
                _ ≤ broadcast.sendActionAt :=
                  broadcast.persistenceBeforeSend
                _ ≤ broadcast.sendActionAt + 1 := Nat.le_succ _
            storedBeforeSend := by
              rw [snapshotStored, broadcast.packetSentAt]
              calc
                persistTime + 1 ≤ broadcast.persistedAt + 1 := by
                  rw [samePersistence]
                  exact Nat.le_refl _
                _ ≤ broadcast.sendActionAt :=
                  broadcast.persistenceBeforeSend
                _ ≤ broadcast.sendActionAt + 1 := Nat.le_succ _
            sentWithinPipeline := by
              rw [broadcast.packetSentAt]
              calc
                broadcast.sendActionAt + 1 ≤
                    (broadcast.persistedAt + 1 +
                      timed.localActionBound) + 1 :=
                  Nat.succ_le_succ broadcast.sendWithinBound
                _ = (persistTime + 1 + timed.localActionBound) + 1 := by
                  rw [samePersistence]
                _ ≤ origin.proposalActionAt +
                    2 * (timed.localActionBound + 1) + 1 :=
                  recovery_exact_send_bound_arithmetic _ _ _
                    persistenceWithinLocalBound }⟩ }
  refine ⟨production, ?_, rfl, rfl, ?_⟩
  · simpa [production] using snapshotBlock
  · simpa [production] using snapshotAt

/-- An actual block-progress recovery persistence at a newly selected target
has a fresh timer and proposal snapshot after the selection observation.

The persistence is an actual trace event. The origin and timing rules recover
its local timer history. The target-above-floor premise prevents an old timer
or proposal from satisfying the result. -/
theorem block_progress_fresh_target_persistence_gives_timer_paced_production
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (originRules : ValidatorBlockProgressProposalOriginRules
      (obligations := obligations) mode)
    (timingRules : ValidatorRecoveryProposalActionTimingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {observation persistTime validator targetRound : Time}
    {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (recoveryActive :
      ValidatorBlockProgressRecoveryModeAt mode persistTime validator)
    (floorAboveGc :
      ((timed.execution.trace persistTime).validatorState validator).gcRound <
        ((timed.execution.trace persistTime).validatorState
          validator).highestSignedRound)
    (targetBlockRound : block.reference.round = targetRound + 1)
    (targetAboveObservedFloor :
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound < targetRound)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (persisted : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) validator
        (.persistProposal block)) :
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        (targetRound + 1),
      production.snapshot.block = block ∧
        production.persistTime = persistTime ∧
        observation < production.timerStartedAt ∧
        observation < production.snapshot.snapshotAt := by
  let origin := Classical.choice
    (block_progress_exact_next_persistence_has_current_timer_snapshot_origin
      originRules timingRules latchSource pacing validatorInRange
        validatorCorrectAvailable recoveryActive floorAboveGc persisted)
  have targetIsExactNext := timerSource.targetIsExactNextAtStart origin.start
    origin.timerStarted
  have floorAtTimerStart :
      ((timed.execution.trace origin.start.startedAt).validatorState
        validator).highestSignedRound = targetRound := by
    have exactAtStart : origin.start.targetRound =
        ((timed.execution.trace origin.start.startedAt).validatorState
          validator).highestSignedRound + 1 := by
      simpa only [origin.startValidator] using targetIsExactNext
    rw [origin.proposalTarget, targetBlockRound] at exactAtStart
    omega
  have observationBeforeTimer : observation < origin.start.startedAt := by
    apply Nat.lt_of_not_ge
    intro timerBeforeObservation
    have floorMonotone :=
      (timed.execution.durableStateMonotone validator origin.start.startedAt
        observation validatorInRange timerBeforeObservation).2.2.2.2.2.2.1
    rw [floorAtTimerStart] at floorMonotone
    exact (Nat.not_lt_of_ge floorMonotone) targetAboveObservedFloor
  have activeBetween : ValidatorEpochActiveBetween timed.execution.trace
      origin.start.startedAt (origin.start.deadline waits) := by
    intro time startBeforeTime _timeBeforeDeadline
    exact active time
      (Nat.le_trans (Nat.le_of_lt observationBeforeTimer) startBeforeTime)
  let receiver := recoveryOtherReceiver validator
  have receiverInRange : receiver < config.authorityCount :=
    recovery_other_receiver_in_range authorityCountAtLeastTwo
  have receiverIsOther : receiver ≠ validator :=
    recovery_other_receiver_is_different
  rcases current_timer_snapshot_origin_gives_timer_paced_round_production
      latchSource effects origin activeBetween receiverInRange receiverIsOther
    with ⟨production, exactBlock, exactPersistence, exactTimer,
      exactSnapshot⟩
  have observationBeforeSnapshot : observation <
      production.snapshot.snapshotAt := by
    rw [exactSnapshot]
    exact Nat.lt_of_lt_of_le observationBeforeTimer (by
      simp [ValidatorRecoveryTimerStart.deadline])
  have evidence : ∃ production : ValidatorTimerPacedRoundProduction timed waits
      validator block.reference.round,
      production.snapshot.block = block ∧
        production.persistTime = persistTime ∧
        observation < production.timerStartedAt ∧
        observation < production.snapshot.snapshotAt :=
    ⟨production, exactBlock, exactPersistence, by
      simpa only [exactTimer] using observationBeforeTimer,
      observationBeforeSnapshot⟩
  rw [← targetBlockRound]
  exact evidence

/-- Every correct, available author has one timer-paced proposal snapshot for
the same round after the stated start time. -/
def EveryCorrectAvailableValidatorTimerPacedRound
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
    (start round : Time) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        round,
      start ≤ production.timerStartedAt

/-- One common parent-ready state gives all correct, available validators the
same exact timer-paced round, unless one validator installs a later commit.

The inputs describe current local state and protected workers. They do not
state that a future proposal or layer exists. -/
theorem common_recovery_timer_inputs_give_commit_advance_or_timer_paced_round
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
      EveryCorrectAvailableValidatorTimerPacedRound timed waits readyAt
        round := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed readyAt
  · exact Or.inl advanced
  · right
    intro validator validatorInRange validatorCorrect
    have pointwise := ready_state_builds_strict_recovery_broadcast_or_commit_advance
      timed timerSource pacing arms latchSource effects readyAt validator
      (recoveryOtherReceiver validator)
      (inputs validator validatorInRange validatorCorrect) active
      (recovery_other_receiver_in_range authorityCountAtLeastTwo)
      recovery_other_receiver_is_different
    rcases pointwise with
      ⟨finish, readyBeforeFinish, localAdvance⟩ |
        ⟨result, targetAtInput, readyBeforeTimer, _timerBound⟩
    · exact False.elim (advanced ⟨validator, finish, validatorInRange,
        validatorCorrect, readyBeforeFinish, localAdvance⟩)
    · have targetRound : result.targetRound = round :=
        targetAtInput.trans
          (sameTarget validator validatorInRange validatorCorrect)
      rcases strict_recovery_broadcast_gives_timer_paced_round_production
          result targetRound with
        ⟨production, _parentReady, timerStarted, _persistAt, _sentAt⟩
      exact ⟨production, by simpa [timerStarted] using readyBeforeTimer⟩

/-- Every correct, available author has one exact timer-paced proposal whose
persistence and send are after the observation. The timer itself can have
started before the observation. This is the warm-up form for restored or
already armed timer work. -/
def EveryCorrectAvailableValidatorObservedTimerPacedRound
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
    (observation round : Time) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        round,
      observation ≤ production.persistTime ∧
        observation ≤ production.sentTime

/-- One already-produced timer-paced round makes each correct author's exact
block body available at one fixed correct requester.

The author uses its local durable catalog entry. Every other author uses the
actual addressed proposal packet and post-GST delivery. A later commit install
cannot cancel either fact. This theorem does not require stable commit heads
and does not use commit synchronization. -/
theorem observed_timer_paced_round_makes_each_block_available
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
    {observation round requester : Time}
    (family : EveryCorrectAvailableValidatorObservedTimerPacedRound timed waits
      observation round)
    (afterGst : network.gst ≤ observation)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true) :
    ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ production : ValidatorTimerPacedRoundProduction timed waits author
          round,
        ∃ availableAt,
          observation ≤ production.persistTime ∧
            observation ≤ availableAt ∧
            ValidatorLocalBlockBodyAt timed availableAt requester
              production.snapshot.block := by
  intro author authorInRange authorCorrectAvailable
  rcases family author authorInRange authorCorrectAvailable with
    ⟨production, observationBeforePersistence, _observationBeforeSend⟩
  by_cases requesterIsAuthor : requester = author
  · subst requester
    have blockStoredAtAuthor :
        ((timed.execution.trace production.snapshot.storedAt).validatorState
          author).ownBlockAt production.snapshot.block.reference.round =
            some production.snapshot.block.reference := by
      simpa [production.proposer] using production.snapshot.blockStored
    have ownFacts :=
      (timed.execution.statesWellFormed production.snapshot.storedAt author
        authorInRange).ownBlockIsSound
          production.snapshot.block.reference.round
          production.snapshot.block.reference blockStoredAtAuthor
    have observationBeforeStorage :
        observation ≤ production.snapshot.storedAt := by
      rw [production.storedAfterPersistence]
      exact Nat.le_trans observationBeforePersistence (Nat.le_succ _)
    exact ⟨production, production.snapshot.storedAt,
      observationBeforePersistence, observationBeforeStorage,
      .acceptedCatalogued ownFacts.2.2.1
        production.snapshot.blockInCatalog⟩
  · let peer := Classical.choice
        (production.peerBroadcast requester requesterInRange
          requesterIsAuthor)
    have persistenceBeforeSend : production.persistTime + 1 ≤
        peer.packet.sentAt := by
      rw [← production.storedAfterPersistence]
      exact peer.storedBeforeSend
    have sentAfterGst : network.gst ≤ peer.packet.sentAt := by
      exact Nat.le_trans afterGst
        (Nat.le_trans observationBeforePersistence
          (Nat.le_trans (Nat.le_succ _) persistenceBeforeSend))
    have delivered := timer_paced_peer_broadcast_is_delivered production peer
      requesterInRange requesterCorrectAvailable sentAfterGst
    have packetAtDelivery := timed.execution.packetHistoryMonotone
      peer.packet.sentAt peer.packet.deliveredAt delivered.1 peer.packetId
        peer.packet peer.packetInTrace
    have observationBeforeDelivery : observation ≤ peer.packet.deliveredAt :=
      Nat.le_trans observationBeforePersistence
        (Nat.le_trans (Nat.le_succ _)
          (Nat.le_trans persistenceBeforeSend delivered.1))
    exact ⟨production, peer.packet.deliveredAt,
      observationBeforePersistence, observationBeforeDelivery,
      .delivered peer.packetId peer.packet packetAtDelivery
        peer.packetIsProtocol peer.packetReceiver peer.packetPayload
        delivered.2.2⟩

/-- Current timer state gives the same exact recovery round at every correct,
available validator, unless one validator installs a later local commit.

This theorem covers both a new timer arm and a restored or already armed exact
timer. It does not take a timer-start or ready record as input. -/
theorem common_recovery_current_inputs_give_commit_advance_or_observed_round
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
    {observation round : Time}
    (inputs : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ValidatorRecoveryTimerCurrentInputAt timed observation validator)
    (sameTarget : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound + 1 = round)
    (active : ∀ later, observation ≤ later →
      (timed.execution.trace later).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed observation ∨
      EveryCorrectAvailableValidatorObservedTimerPacedRound timed waits
        observation round := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed observation
  · exact Or.inl advanced
  · right
    intro validator validatorInRange validatorCorrect
    have pointwise := current_recovery_state_builds_strict_broadcast_or_commit_advance
      timed timerSource pacing arms latchSource effects observation validator
      (recoveryOtherReceiver validator)
      (inputs validator validatorInRange validatorCorrect) active
      (recovery_other_receiver_in_range authorityCountAtLeastTwo)
      recovery_other_receiver_is_different
    rcases pointwise with
      ⟨finish, observationBeforeFinish, localAdvance⟩ |
        ⟨result, targetAtInput, observationBeforePersist,
          observationBeforeSend⟩
    · exact False.elim (advanced ⟨validator, finish, validatorInRange,
        validatorCorrect, observationBeforeFinish, localAdvance⟩)
    · have targetRound : result.targetRound = round :=
        targetAtInput.trans
          (sameTarget validator validatorInRange validatorCorrect)
      rcases strict_recovery_broadcast_gives_timer_paced_round_production
          result targetRound with
        ⟨production, _parentReady, _timerStarted, persistAt, sentAt⟩
      exact ⟨production, by simpa [persistAt] using observationBeforePersist,
        by simpa [sentAt] using observationBeforeSend⟩

/-- Every observed timer-paced proposal finishes inside one common finite
trace prefix. The exact proposal snapshot remains available in the result. -/
def EveryCorrectAvailableValidatorObservedTimerPacedRoundWithin
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
    (observation finish round : Time) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        round,
      observation ≤ production.persistTime ∧
        production.sentTime ≤ finish

/-- A finite observed family has one time at which every correct, available
validator has stored and sent its exact recovery block. -/
theorem observed_timer_paced_round_family_eventually_is_produced
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
    {observation round : Time}
    (family : EveryCorrectAvailableValidatorObservedTimerPacedRound timed waits
      observation round) :
    ∃ finish,
      observation ≤ finish ∧
        EveryCorrectAvailableValidatorObservedTimerPacedRoundWithin timed waits
          observation finish round ∧
        EveryCorrectAvailableValidatorProduced faults
          (timed.execution.trace finish) round := by
  let done := fun validator time =>
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        round,
      observation ≤ production.persistTime ∧
        production.sentTime ≤ time
  have donePersists : ∀ validator earlier later,
      earlier ≤ later → done validator earlier → done validator later := by
    intro validator earlier later ordered completed
    rcases completed with
      ⟨production, observationBeforePersist, sentBeforeEarlier⟩
    exact ⟨production, observationBeforePersist,
      Nat.le_trans sentBeforeEarlier ordered⟩
  have eachDone : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ∃ finish, observation ≤ finish ∧ done validator finish := by
    intro validator validatorInRange validatorCorrect
    rcases family validator validatorInRange validatorCorrect with
      ⟨production, observationBeforePersist, observationBeforeSend⟩
    exact ⟨production.sentTime, observationBeforeSend, production,
      observationBeforePersist, Nat.le_refl _⟩
  rcases eventually_every_selected_validator faults.correctAvailable done
      observation donePersists eachDone with
    ⟨finish, observationBeforeFinish, allDone⟩
  have within :
      EveryCorrectAvailableValidatorObservedTimerPacedRoundWithin timed waits
        observation finish round := by
    intro validator validatorInRange validatorCorrect
    exact allDone validator validatorInRange validatorCorrect
  have produced : EveryCorrectAvailableValidatorProduced faults
      (timed.execution.trace finish) round := by
    intro validator validatorInRange validatorCorrect
    rcases within validator validatorInRange validatorCorrect with
      ⟨production, _observationBeforePersist, sentBeforeFinish⟩
    have blockStoredAtValidator :
        ((timed.execution.trace production.snapshot.storedAt).validatorState
          validator).ownBlockAt production.snapshot.block.reference.round =
            some production.snapshot.block.reference := by
      simpa [production.proposer] using production.snapshot.blockStored
    have ownAtSend :=
      (timed.execution.durableStateMonotone validator
        production.snapshot.storedAt production.sentTime validatorInRange
        production.storedBeforeSent)
        |>.own_block_persists blockStoredAtValidator
    cases ownValue : ((timed.execution.trace production.sentTime).validatorState
        validator).ownBlockAt round with
    | none =>
        rw [production.blockRound] at ownAtSend
        simp [ownValue] at ownAtSend
    | some reference =>
        have durable := timed.execution.durableStateMonotone validator
          production.sentTime finish validatorInRange sentBeforeFinish
        exact ⟨by simp [durable.own_block_persists ownValue],
          durable.sent_own_block_persists production.sentOwnBlock⟩
  exact ⟨finish, observationBeforeFinish, within, produced⟩

/-- Current exact timer state at every correct, available validator produces
one common round in a finite prefix, unless a local commit advances first. -/
theorem common_recovery_current_inputs_give_commit_advance_or_produced_round
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
    {observation round : Time}
    (inputs : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ValidatorRecoveryTimerCurrentInputAt timed observation validator)
    (sameTarget : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound + 1 = round)
    (active : ∀ later, observation ≤ later →
      (timed.execution.trace later).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed observation ∨
      ∃ finish,
        observation ≤ finish ∧
          EveryCorrectAvailableValidatorObservedTimerPacedRoundWithin timed waits
            observation finish round ∧
          EveryCorrectAvailableValidatorProduced faults
            (timed.execution.trace finish) round := by
  rcases common_recovery_current_inputs_give_commit_advance_or_observed_round
      timerSource pacing arms latchSource effects authorityCountAtLeastTwo
      inputs sameTarget active with advanced | family
  · exact Or.inl advanced
  · exact Or.inr
      (observed_timer_paced_round_family_eventually_is_produced family)

/-- A current recovery parent quorum starts a new exact timer or reuses the
exact timer already stored at each correct host. Thus, the caller does not
supply timer state. A commit install remains an internal race result. -/
theorem common_recovery_parent_quorums_give_commit_advance_or_produced_round
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
    {observation round : Time}
    (parentsReady : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ValidatorRecoveryParentQuorumReadyAt config
        ((timed.execution.trace observation).validatorState validator)
        (((timed.execution.trace observation).validatorState
          validator).highestSignedRound + 1))
    (sameTarget : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound + 1 = round)
    (active : ∀ later, observation ≤ later →
      (timed.execution.trace later).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed observation ∨
      ∃ finish,
        observation ≤ finish ∧
          EveryCorrectAvailableValidatorObservedTimerPacedRoundWithin timed waits
            observation finish round ∧
          EveryCorrectAvailableValidatorProduced faults
            (timed.execution.trace finish) round := by
  have inputs : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ValidatorRecoveryTimerCurrentInputAt timed observation validator := by
    intro validator validatorInRange validatorCorrectAvailable
    exact timerSource.active_parent_quorum_state_gives_current_timer_input
      observation validator (active observation (Nat.le_refl _))
        validatorInRange validatorCorrectAvailable
          (parentsReady validator validatorInRange validatorCorrectAvailable)
  exact common_recovery_current_inputs_give_commit_advance_or_produced_round
    timerSource pacing arms latchSource effects authorityCountAtLeastTwo inputs
      sameTarget active

/-- One current local recovery parent quorum gives an exact paced broadcast or
a real commit-index advance. This theorem covers an empty timer, a pending arm
worker, and an exact stored timer through the timer source rules. -/
theorem current_recovery_parent_quorum_gives_broadcast_or_commit_advance
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
    {time validator receiver : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (receiverInRange : receiver < config.authorityCount)
    (differentReceiver : receiver ≠ validator)
    (parentsReady : ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace time).validatorState validator)
      (((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1))
    (active : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    (∃ finish,
        time ≤ finish ∧
          ((timed.execution.trace time).validatorState
              validator).commitHead.index <
            ((timed.execution.trace finish).validatorState
              validator).commitHead.index) ∨
      ∃ result : ValidatorStrictRecoveryBroadcast timed waits timerSource
          obligations validator receiver,
        result.targetRound =
            ((timed.execution.trace time).validatorState
              validator).highestSignedRound + 1 ∧
          time ≤ result.persistTime ∧
          time ≤ result.packet.sentAt := by
  have input :=
    timerSource.active_parent_quorum_state_gives_current_timer_input time
      validator (active time (Nat.le_refl _)) validatorInRange
        validatorCorrectAvailable parentsReady
  exact current_recovery_state_builds_strict_broadcast_or_commit_advance timed
    timerSource pacing arms latchSource effects time validator receiver input
      active receiverInRange differentReceiver

/-- Static genesis storage produces the exact round-one source even when an
exact round-one recovery timer is already armed.

The current-timer theorem handles empty, pending, and stored timer states. A
local commit race contradicts the stable-head suffix. -/
theorem canonical_genesis_gives_round_one_source_without_commit_advance
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (genesis : ValidatorCanonicalGenesisParentRules timed)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {recoveryWait time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (floorIsZero :
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound = 0)
    (active : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed time) :
    Nonempty (ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait time validator 1) := by
  have parentsReady : ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace time).validatorState validator)
      (((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1) := by
    refine ⟨genesis.parents, ?_⟩
    refine ⟨⟨genesis.parentAuthorsNodup, ?_,
      genesis.parentStakeIsQuorum⟩, ?_⟩
    · intro parent parentMember
      refine ⟨?_, ?_⟩
      · rw [floorIsZero, genesis.parentsAreRoundZero parent parentMember]
      · exact timed.execution.accepted_block_persists validatorInRange
          (Nat.zero_le time)
          (genesis.initiallyAccepted validator parent validatorInRange
            validatorCorrectAvailable parentMember)
    · intro parent parentMember
      exact ⟨genesis.retainedAtCorrectValidator time validator parent
        validatorInRange validatorCorrectAvailable parentMember,
        Or.inl (genesis.parentsAreRoundZero parent parentMember)⟩
  let receiver := recoveryOtherReceiver validator
  have receiverInRange : receiver < config.authorityCount :=
    recovery_other_receiver_in_range authorityCountAtLeastTwo
  have receiverIsOther : receiver ≠ validator :=
    recovery_other_receiver_is_different
  rcases current_recovery_parent_quorum_gives_broadcast_or_commit_advance
      timerSource pacing arms latchSource effects validatorInRange
        validatorCorrectAvailable receiverInRange receiverIsOther parentsReady
          active with advanced | strict
  · rcases advanced with ⟨finish, timeBeforeFinish, headAdvanced⟩
    exact False.elim (noAdvance ⟨validator, finish, validatorInRange,
      validatorCorrectAvailable, timeBeforeFinish, headAdvanced⟩)
  · rcases strict with
      ⟨result, targetAtTime, timeBeforePersistence, _timeBeforeSend⟩
    have exactRound : result.snapshot.block.reference.round = 1 := by
      rw [result.completed.snapshotRound, targetAtTime, floorIsZero]
    have blockAboveStartFloor :
        ((timed.execution.trace time).validatorState
            validator).highestSignedRound <
          result.snapshot.block.reference.round := by
      rw [floorIsZero, exactRound]
      omega
    let source := Classical.choice
      (persist_proposal_occurrence_eventually_produces_exact_broadcast
        latchSource effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable timeBeforePersistence
            blockAboveStartFloor result.persistenceOccurs)
    refine ⟨.persisted source.1 ?_⟩
    rw [source.2.2]
    exact exactRound

/-- One stable causal history drives one strict exact-next recovery proposal.

The result keeps the timer-paced proposal snapshot. It also gives a later
state whose signer floor is exactly the proposed round. The exact floor is the
induction state for the next recovery round. A local commit race is impossible
because `noAdvance` covers the complete correct-validator suffix. -/
structure ValidatorStableExactNextRound
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
    (start validator floor : Nat) where
  production : ValidatorTimerPacedRoundProduction timed waits validator
    (floor + 1)
  finish : Time
  persistenceAfterStart : start ≤ production.persistTime
  persistenceBeforeFinish : production.persistTime + 1 ≤ finish
  ownBlockStored :
    ((timed.execution.trace finish).validatorState validator).ownBlockAt
        (floor + 1) = some production.snapshot.block.reference
  ownBlockSent :
    ((timed.execution.trace finish).validatorState validator).sentOwnBlockAt
        (floor + 1) = true
  floorAtFinish :
    ((timed.execution.trace finish).validatorState
      validator).highestSignedRound = floor + 1

/-- A reusable above-GC causal history gives one exact-next timer-paced round.
The theorem uses only the current local signer floor, local retained history,
the active epoch suffix, and the absence of a correct-host commit advance. -/
theorem stable_usable_history_gives_one_exact_next_round
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
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {observation historyStart start validator floor : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (observationBeforeHistory : observation ≤ historyStart)
    (historyBeforeStart : historyStart ≤ start)
    (floorAtStart :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound = floor)
    (floorBelowTarget : floor < capsule.targetRound)
    (gcBelowFloor :
      ((timed.execution.trace start).validatorState validator).gcRound < floor)
    (historyUsable : ValidatorCausalHistoryUsableFrom timed capsule historyStart
      validator)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    Nonempty (ValidatorStableExactNextRound timed waits start validator floor) := by
  have historyAtStart := historyUsable start historyBeforeStart
  rcases gc_truncated_causal_history_supplies_exact_next_parents capsule
      ((timed.execution.trace start).validatorState validator) floorBelowTarget
      gcBelowFloor historyAtStart with
    ⟨parents, parentsReady⟩
  have parentQuorum : ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace start).validatorState validator)
      (((timed.execution.trace start).validatorState
        validator).highestSignedRound + 1) := by
    refine ⟨parents, ?_⟩
    simpa only [floorAtStart] using parentsReady
  have activeFromStart : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true := by
    intro later startBeforeLater
    exact active later (Nat.le_trans observationBeforeHistory
      (Nat.le_trans historyBeforeStart startBeforeLater))
  let receiver := recoveryOtherReceiver validator
  have receiverInRange : receiver < config.authorityCount :=
    recovery_other_receiver_in_range authorityCountAtLeastTwo
  have receiverIsOther : receiver ≠ validator :=
    recovery_other_receiver_is_different
  rcases current_recovery_parent_quorum_gives_broadcast_or_commit_advance
      timerSource pacing arms latchSource effects validatorInRange
      validatorCorrectAvailable receiverInRange receiverIsOther parentQuorum
      activeFromStart with advanced | strict
  · rcases advanced with ⟨finish, startBeforeFinish, headAdvanced⟩
    have observationBeforeStart : observation ≤ start :=
      Nat.le_trans observationBeforeHistory historyBeforeStart
    have headAtObservationAtMostStart :=
      (timed.execution.durableStateMonotone validator observation start
        validatorInRange observationBeforeStart).1
    exact False.elim (noAdvance ⟨validator, finish, validatorInRange,
      validatorCorrectAvailable, Nat.le_trans observationBeforeStart
        startBeforeFinish,
      Nat.lt_of_le_of_lt headAtObservationAtMostStart headAdvanced⟩)
  · rcases strict with
      ⟨result, targetRound, startBeforePersistence, _startBeforeSend⟩
    have exactTarget : result.targetRound = floor + 1 := by
      simpa only [floorAtStart] using targetRound
    rcases strict_recovery_broadcast_gives_timer_paced_round_production result
        exactTarget with
      ⟨production, _parentTime, _timerTime, _persistTime, _sentTime⟩
    rcases persist_proposal_occurrence_eventually_sends_block obligations
        authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
        production.persistenceOccurs with
      ⟨finish, persistenceBeforeFinish, stored, sent, floorAtFinish⟩
    refine ⟨{
      production := production
      finish := finish
      persistenceAfterStart := ?_
      persistenceBeforeFinish := persistenceBeforeFinish
      ownBlockStored := ?_
      ownBlockSent := ?_
      floorAtFinish := ?_ }⟩
    · simpa only [_persistTime] using startBeforePersistence
    · simpa only [production.blockRound] using stored
    · simpa only [production.blockRound] using sent
    · simpa only [production.blockRound] using floorAtFinish

/-- A finite strict exact-next suffix keeps every concrete timer-paced block.
`productionAt` is the per-author witness used later by the direct-vote proof. -/
structure ValidatorStableExactNextWindow
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
    (start validator floor count : Nat) where
  finish : Time
  startBeforeFinish : start ≤ finish
  floorAtFinish :
    ((timed.execution.trace finish).validatorState
      validator).highestSignedRound = floor + count
  productionAt : ∀ offset,
    offset < count →
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        (floor + offset + 1),
      start ≤ production.persistTime ∧
        production.persistTime + 1 ≤ finish

/-- One reusable causal history drives any finite exact-next suffix which stays
at or below that history's target round. The induction does not assume a
future block, a common layer, or future parent availability. -/
theorem stable_usable_history_gives_finite_exact_next_window
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
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {observation historyStart start validator floor count : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (observationBeforeHistory : observation ≤ historyStart)
    (historyBeforeStart : historyStart ≤ start)
    (floorAtStart :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound = floor)
    (gcBelowFloor :
      ((timed.execution.trace start).validatorState validator).gcRound < floor)
    (windowBelowTarget : floor + count ≤ capsule.targetRound)
    (historyUsable : ValidatorCausalHistoryUsableFrom timed capsule historyStart
      validator)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    Nonempty (ValidatorStableExactNextWindow timed waits start validator floor
      count) := by
  induction count generalizing start floor with
  | zero =>
      exact ⟨{
        finish := start
        startBeforeFinish := Nat.le_refl _
        floorAtFinish := by simpa using floorAtStart
        productionAt := by
          intro offset offsetInRange
          exact False.elim (Nat.not_lt_zero offset offsetInRange) }⟩
  | succ remaining inductionHypothesis =>
      have floorBelowTarget : floor < capsule.targetRound := by
        have floorBeforeNext : floor < floor + 1 := Nat.lt_succ_self floor
        have nextBeforeWindow : floor + 1 ≤ floor + remaining.succ := by
          exact Nat.add_le_add_left (Nat.succ_le_succ (Nat.zero_le remaining))
            floor
        exact Nat.lt_of_lt_of_le floorBeforeNext
          (Nat.le_trans nextBeforeWindow windowBelowTarget)
      let first := Classical.choice
        (stable_usable_history_gives_one_exact_next_round timerSource pacing
          arms latchSource effects authorityCountAtLeastTwo capsule
          validatorInRange validatorCorrectAvailable observationBeforeHistory
          historyBeforeStart floorAtStart floorBelowTarget gcBelowFloor
          historyUsable active noAdvance)
      have observationBeforeFirstFinish : observation ≤ first.finish := by
        exact Nat.le_trans observationBeforeHistory
          (Nat.le_trans historyBeforeStart
            (Nat.le_trans first.persistenceAfterStart
              (Nat.le_trans (Nat.le_succ _) first.persistenceBeforeFinish)))
      have historyBeforeFirstFinish : historyStart ≤ first.finish := by
        exact Nat.le_trans historyBeforeStart
          (Nat.le_trans first.persistenceAfterStart
            (Nat.le_trans (Nat.le_succ _) first.persistenceBeforeFinish))
      have gcAtFirstFinish :
          ((timed.execution.trace first.finish).validatorState
              validator).gcRound =
            ((timed.execution.trace start).validatorState
              validator).gcRound := by
        calc
          ((timed.execution.trace first.finish).validatorState
              validator).gcRound =
              ((timed.execution.trace observation).validatorState
                validator).gcRound :=
            no_commit_advance_keeps_correct_gc_round validatorInRange
              validatorCorrectAvailable observationBeforeFirstFinish noAdvance
          _ = ((timed.execution.trace start).validatorState
                validator).gcRound :=
            (no_commit_advance_keeps_correct_gc_round validatorInRange
              validatorCorrectAvailable
              (Nat.le_trans observationBeforeHistory historyBeforeStart)
              noAdvance).symm
      have gcBelowNextFloor :
          ((timed.execution.trace first.finish).validatorState
              validator).gcRound < floor + 1 := by
        rw [gcAtFirstFinish]
        omega
      have remainingBelowTarget : (floor + 1) + remaining ≤
          capsule.targetRound := by
        simpa [Nat.succ_eq_add_one, Nat.add_assoc, Nat.add_comm 1 remaining]
          using windowBelowTarget
      let rest := Classical.choice
        (inductionHypothesis historyBeforeFirstFinish
          first.floorAtFinish gcBelowNextFloor remainingBelowTarget)
      refine ⟨{
        finish := rest.finish
        startBeforeFinish := Nat.le_trans
          (Nat.le_trans first.persistenceAfterStart
            (Nat.le_trans (Nat.le_succ _) first.persistenceBeforeFinish))
          rest.startBeforeFinish
        floorAtFinish := ?_
        productionAt := ?_ }⟩
      · rw [rest.floorAtFinish]
        omega
      · intro offset offsetInRange
        cases offset with
        | zero =>
            exact ⟨first.production, first.persistenceAfterStart,
              Nat.le_trans first.persistenceBeforeFinish
                rest.startBeforeFinish⟩
        | succ earlierOffset =>
            have earlierInRange : earlierOffset < remaining := by omega
            rcases rest.productionAt earlierOffset earlierInRange with
              ⟨production, firstFinishBeforePersist, persistBeforeFinish⟩
            have startBeforePersist : start ≤ production.persistTime :=
              Nat.le_trans
                (Nat.le_trans first.persistenceAfterStart
                  (Nat.le_trans (Nat.le_succ _)
                    first.persistenceBeforeFinish))
                firstFinishBeforePersist
            have roundExact : (floor + 1) + earlierOffset + 1 =
                floor + (earlierOffset + 1) + 1 := by
              rw [Nat.add_assoc floor 1 earlierOffset,
                Nat.add_comm 1 earlierOffset]
            rw [← roundExact]
            exact ⟨production, startBeforePersist, persistBeforeFinish⟩

/-- A stable causal history catches one correct host up to the history target
and returns the exact persisted broadcast at that target.

The theorem starts from the host's current signer floor. It derives each
intermediate exact-next action from the retained causal history. It does not
take a future proposal, parent layer, or completed synchronization as input. -/
theorem stable_usable_history_catches_validator_to_exact_round_source
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {recoveryWait observation historyStart start validator floor : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (observationBeforeHistory : observation ≤ historyStart)
    (historyBeforeStart : historyStart ≤ start)
    (floorAtStart :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound = floor)
    (floorBelowTarget : floor < capsule.targetRound)
    (gcBelowFloor :
      ((timed.execution.trace start).validatorState validator).gcRound < floor)
    (historyUsable : ValidatorCausalHistoryUsableFrom timed capsule historyStart
      validator)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    Nonempty (ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait observation validator capsule.targetRound) := by
  let count := capsule.targetRound - floor
  have countPositive : 0 < count := by
    exact Nat.sub_pos_iff_lt.mpr floorBelowTarget
  have reachesTarget : floor + count = capsule.targetRound := by
    exact Nat.add_sub_of_le (Nat.le_of_lt floorBelowTarget)
  let window := Classical.choice
    (stable_usable_history_gives_finite_exact_next_window timerSource pacing
      arms latchSource effects authorityCountAtLeastTwo capsule
        validatorInRange validatorCorrectAvailable observationBeforeHistory
          historyBeforeStart floorAtStart gcBelowFloor (by
            exact Nat.le_of_eq reachesTarget) historyUsable active noAdvance)
  let lastOffset := count - 1
  have lastInRange : lastOffset < count := by
    omega
  rcases window.productionAt lastOffset lastInRange with
    ⟨production, observationBeforePersistence, _persistenceBeforeFinish⟩
  have productionRound : production.snapshot.block.reference.round =
      capsule.targetRound := by
    rw [production.blockRound]
    unfold lastOffset
    omega
  have observationBeforeStart : observation ≤ start :=
    Nat.le_trans observationBeforeHistory historyBeforeStart
  have observationFloorAtMostStartFloor :
      ((timed.execution.trace observation).validatorState
          validator).highestSignedRound ≤
        ((timed.execution.trace start).validatorState
          validator).highestSignedRound :=
    (timed.execution.durableStateMonotone validator observation start
      validatorInRange observationBeforeStart).2.2.2.2.2.2.1
  have blockAboveObservationFloor :
      ((timed.execution.trace observation).validatorState
          validator).highestSignedRound <
        production.snapshot.block.reference.round := by
    rw [productionRound]
    exact Nat.lt_of_le_of_lt observationFloorAtMostStartFloor (by
      rw [floorAtStart]
      exact floorBelowTarget)
  let broadcast := Classical.choice
    (persist_proposal_occurrence_eventually_produces_exact_broadcast
      (start := observation) latchSource effects authorityCountAtLeastTwo
        validatorInRange validatorCorrectAvailable
          (Nat.le_trans observationBeforeStart observationBeforePersistence)
            blockAboveObservationFloor production.persistenceOccurs)
  refine ⟨.persisted broadcast.1 ?_⟩
  rw [broadcast.2.2]
  exact productionRound

/-- One commonly accepted and retained correct round is a recovery parent
quorum for the next round at one observer.

The selected list contains one current representative for each correct,
available author. Its quorum weight follows from the static fault bounds. -/
theorem receiver_retained_correct_round_gives_recovery_parent_quorum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time observer round : Nat}
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (accepted : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      (((timed.execution.trace time).validatorState observer
        ).acceptedRepresentative round author).isSome = true)
    (retained : ∀ author reference,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ((timed.execution.trace time).validatorState observer
        ).acceptedRepresentative round author = some reference →
      ((timed.execution.trace time).validatorState observer).retained
        reference = true)
    (roundAboveGc :
      ((timed.execution.trace time).validatorState observer).gcRound < round) :
    ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace time).validatorState observer) (round + 1) := by
  let state := (timed.execution.trace time).validatorState observer
  let view : ImmediateParentView BlockId :=
    { authorityCount := config.authorityCount
      proposalRound := round + 1
      accepted := state.accepted
      valid := state.accepted
      timely := fun _ => true }
  let selection : ImmediateParentSelection view :=
    { choice := fun author =>
        if author < config.authorityCount ∧
            faults.correctAvailable author = true then
          state.acceptedRepresentative round author
        else none
      selectedMatchesAuthor := by
        intro author parent selected
        by_cases eligible : author < config.authorityCount ∧
            faults.correctAvailable author = true
        · have representative : state.acceptedRepresentative round author =
              some parent := by
            simpa [state, eligible] using selected
          exact (representatives.representativeIsSound time observer round
            author parent observerInRange observerCorrectAvailable
              representative).1
        · simp [eligible] at selected
      selectedIsAcceptedValid := by
        intro author parent selected
        have eligible : author < config.authorityCount ∧
            faults.correctAvailable author = true := by
          by_cases candidate : author < config.authorityCount ∧
              faults.correctAvailable author = true
          · exact candidate
          · simp [candidate] at selected
        have representative : state.acceptedRepresentative round author =
            some parent := by
          simpa [state, eligible] using selected
        have sound := representatives.representativeIsSound time observer round
          author parent observerInRange observerCorrectAvailable representative
        change parent.author < config.authorityCount ∧
          parent.round + 1 = round + 1 ∧
          state.accepted parent = true ∧ state.accepted parent = true
        exact ⟨by simpa [sound.1] using eligible.1,
          by simp [sound.2.1], sound.2.2, sound.2.2⟩ }
  let parents := selection.selectedParentRefs
  have memberHasChoice : ∀ parent, parent ∈ parents →
      ∃ author, author < config.authorityCount ∧
        selection.choice author = some parent := by
    intro parent parentMember
    have fromSelectedRefs : ∀ count,
        parent ∈ ImmediateParentSelection.selectedParentRefsFrom count
            selection.choice →
        ∃ author, author < count ∧
          selection.choice author = some parent := by
      intro count
      induction count with
      | zero =>
          simp [ImmediateParentSelection.selectedParentRefsFrom]
      | succ previous inductionHypothesis =>
          intro member
          cases selected : selection.choice previous with
          | none =>
              have earlier : parent ∈
                  ImmediateParentSelection.selectedParentRefsFrom previous
                    selection.choice := by
                simpa [ImmediateParentSelection.selectedParentRefsFrom,
                  selected] using member
              rcases inductionHypothesis earlier with
                ⟨author, authorInRange, chosen⟩
              exact ⟨author, Nat.lt_succ_of_lt authorInRange, chosen⟩
          | some lastParent =>
              have cases : parent ∈
                    ImmediateParentSelection.selectedParentRefsFrom previous
                      selection.choice ∨
                  parent = lastParent := by
                simpa [ImmediateParentSelection.selectedParentRefsFrom,
                  selected] using member
              rcases cases with earlier | last
              · rcases inductionHypothesis earlier with
                  ⟨author, authorInRange, chosen⟩
                exact ⟨author, Nat.lt_succ_of_lt authorInRange, chosen⟩
              · subst parent
                exact ⟨previous, Nat.lt_succ_self previous, selected⟩
    exact fromSelectedRefs config.authorityCount
      (by simpa [parents, ImmediateParentSelection.selectedParentRefs] using
        parentMember)
  have correctAuthorsSelected : VoterSet.SubsetAt config.authorityCount
      faults.correctAvailable (validatorParentAuthors parents) := by
    intro author authorInRange authorCorrectAvailable
    have acceptedSome := accepted author authorInRange authorCorrectAvailable
    cases representative : state.acceptedRepresentative round author with
    | none =>
        simp [state, representative] at acceptedSome
    | some reference =>
        have selected : selection.choice author = some reference := by
          simp [selection, state, authorInRange, authorCorrectAvailable,
            representative]
        have member := selection.selected_parent_mem authorInRange selected
        have authorExact := selection.selectedMatchesAuthor author reference
          selected
        simp [validatorParentAuthors]
        exact ⟨reference, by simpa [parents] using member, authorExact⟩
  have selectedQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (validatorParentAuthors parents) :=
    Nat.le_trans faults.correct_available_stake_is_quorum
      (weight_mono config.stake correctAuthorsSelected)
  refine ⟨parents, ⟨⟨selection.selected_parent_authors_nodup, ?_,
    selectedQuorum⟩, ?_⟩⟩
  · intro parent parentMember
    rcases memberHasChoice parent parentMember with
      ⟨author, _authorInRange, selected⟩
    have acceptedValid := selection.selectedIsAcceptedValid author parent
      selected
    exact ⟨acceptedValid.2.1, by
      simpa [view, state] using acceptedValid.2.2.1⟩
  · intro parent parentMember
    rcases memberHasChoice parent parentMember with
      ⟨author, authorInRange, selected⟩
    have eligible : author < config.authorityCount ∧
        faults.correctAvailable author = true := by
      by_cases candidate : author < config.authorityCount ∧
          faults.correctAvailable author = true
      · exact candidate
      · simp [selection, candidate] at selected
    have representative : state.acceptedRepresentative round author =
        some parent := by
      simpa [selection, eligible] using selected
    have sound := representatives.representativeIsSound time observer round
      author parent observerInRange observerCorrectAvailable representative
    refine ⟨?_, Or.inr ?_⟩
    · exact retained author parent authorInRange eligible.2
        (by simpa [state] using representative)
    · simpa [state, sound.2.1] using roundAboveGc

/-- The common-layer form of the receiver-local parent-quorum constructor. -/
theorem common_retained_correct_round_gives_recovery_parent_quorum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time observer round : Nat}
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (accepted : EveryCorrectAvailableValidatorAccepted faults
      (timed.execution.trace time) round)
    (retained : ∀ author reference,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ((timed.execution.trace time).validatorState observer
        ).acceptedRepresentative round author = some reference →
      ((timed.execution.trace time).validatorState observer).retained
        reference = true)
    (roundAboveGc :
      ((timed.execution.trace time).validatorState observer).gcRound < round) :
    ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace time).validatorState observer) (round + 1) := by
  apply receiver_retained_correct_round_gives_recovery_parent_quorum
    representatives observerInRange observerCorrectAvailable
  · intro author authorInRange authorCorrectAvailable
    exact accepted observer author observerInRange observerCorrectAvailable
      authorInRange authorCorrectAvailable
  · exact retained
  · exact roundAboveGc

/-- Every exact timer-paced proposal for one round finishes inside one common
finite trace prefix. This keeps each proposal snapshot usable at `finish`. -/
def EveryCorrectAvailableValidatorTimerPacedRoundWithin
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
    (start finish round : Time) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        round,
      start ≤ production.timerStartedAt ∧
        production.sentTime ≤ finish

/-- One timer-paced result gives the durable own block and send record at its
send time. -/
theorem timer_paced_round_production_is_produced_at_send
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
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round) :
    (((timed.execution.trace production.sentTime).validatorState
        validator).ownBlockAt round).isSome = true ∧
      ((timed.execution.trace production.sentTime).validatorState
        validator).sentOwnBlockAt round = true := by
  have validatorInRange : validator < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have blockStoredAtValidator :
      ((timed.execution.trace production.snapshot.storedAt).validatorState
        validator).ownBlockAt production.snapshot.block.reference.round =
          some production.snapshot.block.reference := by
    simpa [production.proposer] using production.snapshot.blockStored
  have ownAtSend :=
    (timed.execution.durableStateMonotone validator production.snapshot.storedAt
      production.sentTime validatorInRange production.storedBeforeSent)
      |>.own_block_persists blockStoredAtValidator
  constructor
  · rw [production.blockRound] at ownAtSend
    simp [ownAtSend]
  · exact production.sentOwnBlock

/-- Proposal-time recovery selection includes each retained current parent.

This is the local no-exclusion rule used by the adjacent-round vote proof. -/
theorem timer_paced_round_includes_retained_current_parent
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
    {validator round author : Nat}
    {parent : ValidatorBlockRef BlockId}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round)
    (authorInRange : author < config.authorityCount)
    (currentRepresentative :
      ((timed.execution.trace production.snapshot.snapshotAt).validatorState
        validator).acceptedRepresentative (round - 1) author = some parent)
    (retained :
      ((timed.execution.trace production.snapshot.snapshotAt).validatorState
        validator).retained parent = true) :
    parent ∈ production.snapshot.block.parents := by
  exact production.refreshedParentList.includesRetainedCurrentRepresentatives
    author parent authorInRange currentRepresentative retained

/-- A correct validator's next consecutive recovery proposal directly extends
its previous durable own block when the next parent snapshot follows storage.
The exact accepted-representative rule identifies the same correct branch. -/
theorem consecutive_timer_paced_own_blocks_are_direct_parents
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
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {validator round : Nat}
    (previous : ValidatorTimerPacedRoundProduction timed waits validator round)
    (next : ValidatorTimerPacedRoundProduction timed waits validator
      (round + 1))
    (storedBeforeNextSnapshot :
      previous.snapshot.storedAt ≤ next.snapshot.snapshotAt) :
    previous.snapshot.block.reference ∈ next.snapshot.block.parents := by
  have validatorInRange : validator < config.authorityCount := by
    simpa [previous.proposer] using previous.snapshot.proposerInRange
  have validatorCorrectAvailable :
      faults.correctAvailable validator = true := by
    simpa [previous.proposer] using
      previous.snapshot.proposerCorrectAvailable
  have ownAtPreviousStore :
      ((timed.execution.trace previous.snapshot.storedAt).validatorState
        validator).ownBlockAt round =
          some previous.snapshot.block.reference := by
    simpa [previous.proposer, previous.blockRound] using
      previous.snapshot.blockStored
  have ownAtNextSnapshot :=
    (timed.execution.durableStateMonotone validator previous.snapshot.storedAt
      next.snapshot.snapshotAt validatorInRange storedBeforeNextSnapshot)
      |>.own_block_persists ownAtPreviousStore
  have ownFacts :=
    (timed.execution.statesWellFormed next.snapshot.snapshotAt validator
      validatorInRange).ownBlockIsSound round
        previous.snapshot.block.reference ownAtNextSnapshot
  have notByzantine : faults.byzantine validator = false := by
    have notNonProgress : faults.nonProgress validator = false := by
      simpa [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full]
        using validatorCorrectAvailable
    have separated : faults.byzantine validator = false ∧
        faults.unavailable validator = false := by
      simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
        notNonProgress
    exact separated.1
  have recorded := representatives.acceptedCorrectReferenceIsRecorded
    next.snapshot.snapshotAt validator previous.snapshot.block.reference
    validatorInRange validatorCorrectAvailable
    (by simpa [ownFacts.1] using validatorInRange)
    (by simpa [ownFacts.1] using notByzantine) ownFacts.2.2.1
  have recordedAtRound :
      ((timed.execution.trace next.snapshot.snapshotAt).validatorState
        validator).acceptedRepresentative round validator =
          some previous.snapshot.block.reference := by
    simpa [ownFacts.1, ownFacts.2.1] using recorded
  apply timer_paced_round_includes_retained_current_parent
    (production := next) (author := validator)
    (parent := previous.snapshot.block.reference) validatorInRange
  · simpa using recordedAtRound
  · exact ownFacts.2.2.2.1

/-- The concrete persistence event in one timer-paced production creates a
source-local pinned causal capsule. The source is on the producing validator;
no remote holder is an input. -/
theorem timer_paced_round_production_has_pinned_capsule_source
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
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {validator round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round)
    (activeAfterPersistence :
      (timed.execution.trace (production.persistTime + 1)).epochActive = true) :
    ∃ capsuleId entry,
      entry.capsule.targetBlock = production.snapshot.block ∧
        (pins.trace (production.persistTime + 1) validator).capsuleAt capsuleId =
          some entry ∧
        (pins.trace (production.persistTime + 1) validator).pinned capsuleId =
          true ∧
        CausalRecoveryCapsuleExecutionSource syncRules entry.capsule validator
          (production.persistTime + 1) := by
  have validatorInRange : validator < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have validatorCorrectAvailable :
      faults.correctAvailable validator = true := by
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  exact pins.persisted_proposal_has_pinned_capsule_source validatorInRange
    validatorCorrectAvailable activeAfterPersistence
    production.persistenceOccurs

/-- In a suffix with no correct commit-index advance, one timer-paced block is
eventually the selected accepted representative at each correct observer.

The author uses its durable own block. A different observer receives the exact
addressed proposal packet and recursively installs the source-local pinned
causal capsule. If the observer's GC boundary reaches the block first, its
commit index advances, which contradicts the stable-head premise. -/
theorem timer_paced_round_production_eventually_recorded_at_correct_observer
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
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation validator observer round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round)
    (observationBeforePersistence : observation ≤ production.persistTime)
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (roundAboveObserverGc :
      ((timed.execution.trace observation).validatorState observer).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ finish,
      observation ≤ finish ∧
      ((timed.execution.trace finish).validatorState observer
        ).acceptedRepresentative round validator =
          some production.snapshot.block.reference := by
  have validatorInRange : validator < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have validatorCorrectAvailable :
      faults.correctAvailable validator = true := by
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  have validatorNotByzantine : faults.byzantine validator = false := by
    have notNonProgress : faults.nonProgress validator = false := by
      simpa [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full]
        using validatorCorrectAvailable
    have separated : faults.byzantine validator = false ∧
        faults.unavailable validator = false := by
      simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
        notNonProgress
    exact separated.1
  by_cases observerIsAuthor : observer = validator
  · subst observer
    have ownAtStorage :
        ((timed.execution.trace production.snapshot.storedAt).validatorState
          validator).ownBlockAt round =
            some production.snapshot.block.reference := by
      simpa [production.blockRound, production.proposer] using
        production.snapshot.blockStored
    have ownFacts :=
      (timed.execution.statesWellFormed production.snapshot.storedAt validator
        validatorInRange).ownBlockIsSound round
          production.snapshot.block.reference ownAtStorage
    have recorded := representatives.acceptedCorrectReferenceIsRecorded
      production.snapshot.storedAt validator
      production.snapshot.block.reference validatorInRange
      validatorCorrectAvailable
      (by
        simpa [production.snapshot.blockIsOwnProposal, production.proposer]
          using validatorInRange)
      (by
        simpa [production.snapshot.blockIsOwnProposal, production.proposer]
          using validatorNotByzantine)
      ownFacts.2.2.1
    refine ⟨production.snapshot.storedAt, ?_, ?_⟩
    · rw [production.storedAfterPersistence]
      exact Nat.le_trans observationBeforePersistence (Nat.le_succ _)
    · simpa [production.blockRound, production.snapshot.blockIsOwnProposal,
        production.proposer] using recorded
  · let peer := Classical.choice
        (production.peerBroadcast observer observerInRange observerIsAuthor)
    have persistenceBeforeSend : production.persistTime + 1 ≤
        peer.packet.sentAt := by
      rw [← production.storedAfterPersistence]
      exact peer.storedBeforeSend
    have sentAfterGst : network.gst ≤ peer.packet.sentAt := by
      exact Nat.le_trans afterGst
        (Nat.le_trans observationBeforePersistence
          (Nat.le_trans (Nat.le_succ _) persistenceBeforeSend))
    have delivered := timer_paced_peer_broadcast_is_delivered production peer
      observerInRange observerCorrectAvailable sentAfterGst
    have packetAtDelivery := timed.execution.packetHistoryMonotone
      peer.packet.sentAt peer.packet.deliveredAt delivered.1 peer.packetId
        peer.packet peer.packetInTrace
    have sourceActive :
        (timed.execution.trace (production.persistTime + 1)).epochActive =
          true :=
      active (production.persistTime + 1)
        (Nat.le_trans observationBeforePersistence (Nat.le_succ _))
    rcases timer_paced_round_production_has_pinned_capsule_source pins
        production sourceActive with
      ⟨capsuleKey, entry, targetBlock, stored, pinned, _source⟩
    have targetBody : ValidatorLocalBlockBodyAt timed peer.packet.deliveredAt
        observer entry.capsule.targetBlock := by
      apply ValidatorLocalBlockBodyAt.delivered peer.packetId peer.packet
        packetAtDelivery peer.packetIsProtocol
      · exact peer.packetReceiver
      · rw [targetBlock, peer.packetPayload]
      · exact delivered.2.2
    have sourceBeforeDelivery : production.persistTime + 1 ≤
        peer.packet.deliveredAt :=
      Nat.le_trans persistenceBeforeSend delivered.1
    have activeFromSource : ∀ time, production.persistTime + 1 ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time ordered
      exact active time (Nat.le_trans
        (Nat.le_trans observationBeforePersistence (Nat.le_succ _)) ordered)
    rcases
        ValidatorRecoveryCapsuleSyncExecution.delivered_target_installs_retained_above_gc_history_or_commit_advance
          pins capsuleSync acceptance validatorInRange
        validatorCorrectAvailable observerInRange observerCorrectAvailable
        (Nat.le_trans afterGst
          (Nat.le_trans observationBeforePersistence (Nat.le_succ _)))
        activeFromSource stored pinned sourceBeforeDelivery targetBody with
      ⟨finish, sourceBeforeFinish, headAdvanced | installedHistory⟩
    · have advancedFromSource :
          SomeCorrectAvailableCommitAdvance timed
            (production.persistTime + 1) :=
        ⟨observer, finish, observerInRange, observerCorrectAvailable,
          sourceBeforeFinish, headAdvanced⟩
      exact False.elim (noAdvance
        (commit_advance_after_later_implies_advance_after_earlier
          (Nat.le_trans observationBeforePersistence (Nat.le_succ _))
          advancedFromSource))
    · have targetMember : entry.capsule.targetBlock ∈
          entry.capsule.history :=
        entry.capsule.target_and_parents_in_history.1
      rcases installedHistory.2 entry.capsule.targetBlock targetMember with
        targetAtRoot | targetReady
      · have observationBeforeFinish : observation ≤ finish :=
          Nat.le_trans
            (Nat.le_trans observationBeforePersistence (Nat.le_succ _))
            sourceBeforeFinish
        have durable := timed.execution.durableStateMonotone observer
          observation finish observerInRange observationBeforeFinish
        have notStrict : ¬
            ((timed.execution.trace observation).validatorState
                observer).commitHead.index <
              ((timed.execution.trace finish).validatorState
                observer).commitHead.index := by
          intro advanced
          exact noAdvance ⟨observer, finish, observerInRange,
            observerCorrectAvailable, observationBeforeFinish, advanced⟩
        have sameIndex :
            ((timed.execution.trace observation).validatorState
                observer).commitHead.index =
              ((timed.execution.trace finish).validatorState
                observer).commitHead.index := by
          have monotone := durable.1
          omega
        have sameHead :
            ((timed.execution.trace observation).validatorState
                observer).commitHead =
              ((timed.execution.trace finish).validatorState
                observer).commitHead :=
          durable.2.2.1 sameIndex
        have sameGc :
            ((timed.execution.trace observation).validatorState
                observer).gcRound =
              ((timed.execution.trace finish).validatorState
                observer).gcRound := by
          rw [timed.execution.correctGcRoundMatchesCommitHead observation
              observer observerInRange observerCorrectAvailable,
            timed.execution.correctGcRoundMatchesCommitHead finish observer
              observerInRange observerCorrectAvailable, sameHead]
        rw [targetBlock, production.blockRound, ← sameGc] at targetAtRoot
        omega
      · have acceptedTarget :
            ((timed.execution.trace finish).validatorState observer).accepted
              production.snapshot.block.reference = true := by
          simpa [targetBlock] using targetReady.1
        have recorded := representatives.acceptedCorrectReferenceIsRecorded
          finish observer production.snapshot.block.reference observerInRange
          observerCorrectAvailable
          (by
            simpa [production.snapshot.blockIsOwnProposal,
              production.proposer] using validatorInRange)
          (by
            simpa [production.snapshot.blockIsOwnProposal,
              production.proposer] using validatorNotByzantine)
          acceptedTarget
        refine ⟨finish, ?_, ?_⟩
        · exact Nat.le_trans
            (Nat.le_trans observationBeforePersistence (Nat.le_succ _))
            sourceBeforeFinish
        · simpa [production.blockRound,
            production.snapshot.blockIsOwnProposal, production.proposer] using
            recorded

/-- One timer-paced block becomes a stable accepted and retained current
representative at one correct observer.

The producing host uses its source-local capsule pin. A peer uses the body pin
created by the delivered proposal packet. A stable commit head keeps that pin
and the observer GC boundary current. -/
theorem timer_paced_round_production_has_stable_recorded_body_at_correct_observer
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
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation validator observer round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round)
    (observationBeforePersistence : observation ≤ production.persistTime)
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (roundAboveObserverGc :
      ((timed.execution.trace observation).validatorState observer).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ readyAt,
      observation ≤ readyAt ∧
        ∀ later, readyAt ≤ later →
          ((timed.execution.trace later).validatorState observer
              ).acceptedRepresentative round validator =
                some production.snapshot.block.reference ∧
            ((timed.execution.trace later).validatorState observer).retained
                production.snapshot.block.reference = true := by
  have validatorInRange : validator < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have validatorCorrectAvailable :
      faults.correctAvailable validator = true := by
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  rcases timer_paced_round_production_eventually_recorded_at_correct_observer
      pins capsuleSync acceptance representatives production
      observationBeforePersistence observerInRange observerCorrectAvailable
      roundAboveObserverGc afterGst active noAdvance with
    ⟨recordedAt, observationBeforeRecorded, recorded⟩
  by_cases observerIsAuthor : observer = validator
  · subst observer
    have sourceActive :
        (timed.execution.trace (production.persistTime + 1)).epochActive =
          true :=
      active (production.persistTime + 1)
        (Nat.le_trans observationBeforePersistence (Nat.le_succ _))
    rcases timer_paced_round_production_has_pinned_capsule_source pins
        production sourceActive with
      ⟨capsuleKey, entry, targetBlock, stored, pinned, _source⟩
    let readyAt := max recordedAt (production.persistTime + 1)
    have recordedBeforeReady : recordedAt ≤ readyAt := Nat.le_max_left _ _
    have sourceBeforeReady : production.persistTime + 1 ≤ readyAt :=
      Nat.le_max_right _ _
    refine ⟨readyAt,
      Nat.le_trans observationBeforeRecorded recordedBeforeReady, ?_⟩
    intro later readyBeforeLater
    have recordedLater :=
      accepted_representative_persists_in_validator_execution timed.execution
        validatorInRange (Nat.le_trans recordedBeforeReady readyBeforeLater)
          recorded
    have sourceCurrent := pins.pin_persists_while_epoch_active
      (Nat.le_trans sourceBeforeReady readyBeforeLater) stored pinned (by
        intro time sourceBeforeTime _timeBeforeLater
        exact active time (Nat.le_trans
          (Nat.le_trans observationBeforePersistence (Nat.le_succ _))
          sourceBeforeTime))
    have targetMember : entry.capsule.targetBlock ∈ entry.capsule.history :=
      entry.capsule.target_and_parents_in_history.1
    have localTarget := pins.pinnedHistoryIsLocal later validator capsuleKey
      entry sourceCurrent.1 sourceCurrent.2 entry.capsule.targetBlock
        targetMember
    refine ⟨recordedLater, ?_⟩
    simpa [targetBlock] using localTarget.2.1
  · let peer := Classical.choice
        (production.peerBroadcast observer observerInRange observerIsAuthor)
    have persistenceBeforeSend : production.persistTime + 1 ≤
        peer.packet.sentAt := by
      rw [← production.storedAfterPersistence]
      exact peer.storedBeforeSend
    have sentAfterGst : network.gst ≤ peer.packet.sentAt := by
      exact Nat.le_trans afterGst
        (Nat.le_trans observationBeforePersistence
          (Nat.le_trans (Nat.le_succ _) persistenceBeforeSend))
    have delivered := timer_paced_peer_broadcast_is_delivered production peer
      observerInRange observerCorrectAvailable sentAfterGst
    have packetAtDelivery := timed.execution.packetHistoryMonotone
      peer.packet.sentAt peer.packet.deliveredAt delivered.1 peer.packetId
        peer.packet peer.packetInTrace
    have body : ValidatorLocalBlockBodyAt timed peer.packet.deliveredAt observer
        production.snapshot.block := by
      exact .delivered peer.packetId peer.packet packetAtDelivery
        peer.packetIsProtocol peer.packetReceiver peer.packetPayload
        delivered.2.2
    have observationBeforePin : observation ≤ peer.packet.deliveredAt + 1 :=
      Nat.le_trans observationBeforePersistence
        (Nat.le_trans (Nat.le_succ _)
          (Nat.le_trans persistenceBeforeSend
            (Nat.le_trans delivered.1 (Nat.le_succ _))))
    have gcAtPin := no_commit_advance_keeps_correct_gc_round observerInRange
      observerCorrectAvailable observationBeforePin noAdvance
    have aboveGcAtPin :
        ((timed.execution.trace (peer.packet.deliveredAt + 1)).validatorState
            observer).gcRound < production.snapshot.block.reference.round := by
      rw [gcAtPin, production.blockRound]
      exact roundAboveObserverGc
    have pinAtDelivery := capsuleSync.localBodyLatchesPin
      peer.packet.deliveredAt observer production.snapshot.block observerInRange
      observerCorrectAvailable
      (active (peer.packet.deliveredAt + 1) observationBeforePin)
      aboveGcAtPin body
    let readyAt := max recordedAt (peer.packet.deliveredAt + 1)
    have recordedBeforeReady : recordedAt ≤ readyAt := Nat.le_max_left _ _
    have pinBeforeReady : peer.packet.deliveredAt + 1 ≤ readyAt :=
      Nat.le_max_right _ _
    refine ⟨readyAt,
      Nat.le_trans observationBeforeRecorded recordedBeforeReady, ?_⟩
    intro later readyBeforeLater
    have recordedLater :=
      accepted_representative_persists_in_validator_execution timed.execution
        observerInRange (Nat.le_trans recordedBeforeReady readyBeforeLater)
          recorded
    have pinBeforeLater := Nat.le_trans pinBeforeReady readyBeforeLater
    have pinCurrent := capsuleSync.body_pin_persists_while_head_is_current
      observerInRange observerCorrectAvailable pinBeforeLater pinAtDelivery
      (by
        intro time pinBeforeTime _timeBeforeLater
        exact active time (Nat.le_trans observationBeforePin pinBeforeTime))
      (by
        intro time pinBeforeTime _timeBeforeLater
        have sameGc := no_commit_advance_keeps_correct_gc_round
          observerInRange observerCorrectAvailable
            (Nat.le_trans observationBeforePin pinBeforeTime) noAdvance
        rw [sameGc, production.blockRound]
        exact roundAboveObserverGc)
      (by
        intro time pinBeforeTime _timeBeforeLater
        have currentHead := no_commit_advance_keeps_correct_commit_head
          observerInRange observerCorrectAvailable
            (Nat.le_trans observationBeforePin pinBeforeTime) noAdvance
        have pinHead := no_commit_advance_keeps_correct_commit_head
          observerInRange observerCorrectAvailable observationBeforePin
            noAdvance
        rw [currentHead, pinHead]
        exact Nat.le_refl _)
    have sameGcLater := no_commit_advance_keeps_correct_gc_round
      observerInRange observerCorrectAvailable
      (Nat.le_trans observationBeforePin pinBeforeLater) noAdvance
    have aboveGcLater :
        ((timed.execution.trace later).validatorState observer).gcRound <
          production.snapshot.block.reference.round := by
      rw [sameGcLater, production.blockRound]
      exact roundAboveObserverGc
    have acceptedLater :
        ((timed.execution.trace later).validatorState observer).accepted
            production.snapshot.block.reference = true :=
      (representatives.representativeIsSound later observer round validator
        production.snapshot.block.reference observerInRange
          observerCorrectAvailable recordedLater).2.2
    exact ⟨recordedLater,
      capsuleSync.acceptedPinnedBodyIsRetained later observer
        production.snapshot.block.reference _ observerInRange
        observerCorrectAvailable pinCurrent aboveGcLater acceptedLater⟩

/-- From one time onward, every correct observer has one accepted and retained
current representative from every correct author in the round. -/
def EveryCorrectAvailableValidatorAcceptedAndRetainedFrom
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (start round : Time) : Prop :=
  EveryCorrectAvailableValidatorStableTipAcceptedFrom faults trace start round

/-- A proposal-time refreshed recovery list includes the exact stable current
representative of each correct author.

The result keeps the reference, block body, catalog entry, and direct parent
edge. A higher proof can select a correct leader after this schedule-independent
construction. -/
theorem stable_round_before_fresh_snapshot_gives_exact_parent
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
    {stableAt validator author round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      (round + 1))
    (stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
      timed.execution.trace stableAt round)
    (stableBeforeSnapshot : stableAt ≤ production.snapshot.snapshotAt)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true) :
    ∃ reference block,
      ((timed.execution.trace production.snapshot.snapshotAt).validatorState
          validator).acceptedRepresentative round author = some reference ∧
        ((timed.execution.trace production.snapshot.snapshotAt).validatorState
          validator).accepted reference = true ∧
        ((timed.execution.trace production.snapshot.snapshotAt).validatorState
          validator).retained reference = true ∧
        (timed.execution.trace production.snapshot.snapshotAt).blockCatalog
          reference.id = some block ∧
        block.reference = reference ∧
        reference.author = author ∧
        reference.round = round ∧
        reference ∈ production.snapshot.block.parents := by
  rcases stable production.snapshot.snapshotAt stableBeforeSnapshot validator
      author validatorInRange validatorCorrectAvailable authorInRange
        authorCorrectAvailable with
    ⟨reference, recorded, retained⟩
  have sound :=
    (timed.execution.statesWellFormed production.snapshot.snapshotAt validator
      validatorInRange).acceptedRepresentativeIsSound round author reference
        recorded
  rcases sound.2.2.2 with ⟨block, catalogued, blockReference⟩
  have included : reference ∈ production.snapshot.block.parents := by
    apply timer_paced_round_includes_retained_current_parent production
      authorInRange
    · simpa using recorded
    · exact retained
  exact ⟨reference, block, recorded, sound.2.2.1, retained, catalogued,
    blockReference, sound.1, sound.2.1, included⟩

/-- A finite timer-paced author family becomes a stable common accepted and
retained round. The common start time is derived by finite aggregation. -/
theorem observed_timer_paced_round_family_eventually_is_stably_common_retained
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
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation round : Nat}
    (family : EveryCorrectAvailableValidatorObservedTimerPacedRound timed waits
      observation round)
    (roundAboveGc : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ((timed.execution.trace observation).validatorState observer).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ readyAt,
      observation ≤ readyAt ∧
        EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
          timed.execution.trace readyAt round := by
  let authorDone := fun observer author readyAt =>
    ∀ later, readyAt ≤ later →
      ∃ reference,
        ((timed.execution.trace later).validatorState observer
            ).acceptedRepresentative round author = some reference ∧
          ((timed.execution.trace later).validatorState observer).retained
              reference = true
  have authorDonePersists : ∀ observer author earlier later,
      earlier ≤ later →
      authorDone observer author earlier →
      authorDone observer author later := by
    intro observer author earlier later ordered done future laterBeforeFuture
    exact done future (Nat.le_trans ordered laterBeforeFuture)
  have eachObserver : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ∃ readyAt,
        observation ≤ readyAt ∧
          ∀ author,
            author < config.authorityCount →
            faults.correctAvailable author = true →
            authorDone observer author readyAt := by
    intro observer observerInRange observerCorrectAvailable
    have eachAuthor : ∀ author,
        author < config.authorityCount →
        faults.correctAvailable author = true →
        ∃ readyAt, observation ≤ readyAt ∧
          authorDone observer author readyAt := by
      intro author authorInRange authorCorrectAvailable
      rcases family author authorInRange authorCorrectAvailable with
        ⟨production, observationBeforePersistence,
          _observationBeforeSend⟩
      rcases
          timer_paced_round_production_has_stable_recorded_body_at_correct_observer
            pins capsuleSync acceptance representatives production
            observationBeforePersistence observerInRange
            observerCorrectAvailable
            (roundAboveGc observer observerInRange observerCorrectAvailable)
            afterGst active noAdvance with
        ⟨readyAt, observationBeforeReady, stable⟩
      exact ⟨readyAt, observationBeforeReady, by
        intro later readyBeforeLater
        exact ⟨production.snapshot.block.reference,
          stable later readyBeforeLater⟩⟩
    rcases eventually_every_selected_validator faults.correctAvailable
        (authorDone observer) observation
        (by
          intro author earlier later ordered done
          exact authorDonePersists observer author earlier later ordered done)
        eachAuthor with
      ⟨readyAt, observationBeforeReady, allAuthors⟩
    exact ⟨readyAt, observationBeforeReady, allAuthors⟩
  let observerDone := fun observer readyAt =>
    ∀ later, readyAt ≤ later → ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ reference,
        ((timed.execution.trace later).validatorState observer
            ).acceptedRepresentative round author = some reference ∧
          ((timed.execution.trace later).validatorState observer).retained
              reference = true
  have observerDonePersists : ∀ observer earlier later,
      earlier ≤ later →
      observerDone observer earlier →
      observerDone observer later := by
    intro observer earlier later ordered done future laterBeforeFuture
    exact done future (Nat.le_trans ordered laterBeforeFuture)
  have eachObserverDone : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ∃ readyAt, observation ≤ readyAt ∧
        observerDone observer readyAt := by
    intro observer observerInRange observerCorrectAvailable
    rcases eachObserver observer observerInRange observerCorrectAvailable with
      ⟨readyAt, observationBeforeReady, allAuthors⟩
    exact ⟨readyAt, observationBeforeReady, by
      intro later readyBeforeLater author authorInRange authorCorrectAvailable
      exact allAuthors author authorInRange authorCorrectAvailable later
        readyBeforeLater⟩
  rcases eventually_every_selected_validator faults.correctAvailable
      observerDone observation observerDonePersists eachObserverDone with
    ⟨readyAt, observationBeforeReady, allObservers⟩
  refine ⟨readyAt, observationBeforeReady, ?_⟩
  intro later readyBeforeLater observer author observerInRange
    observerCorrectAvailable authorInRange authorCorrectAvailable
  exact allObservers observer observerInRange observerCorrectAvailable later
    readyBeforeLater author authorInRange authorCorrectAvailable

/-- A stable common accepted and retained round gives every correct observer a
recovery parent quorum for the next round. -/
theorem stable_common_retained_round_gives_recovery_parent_quorums
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time round : Nat}
    (stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
      timed.execution.trace time round)
    (roundAboveGc : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ((timed.execution.trace time).validatorState observer).gcRound < round) :
    ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ValidatorRecoveryParentQuorumReadyAt config
        ((timed.execution.trace time).validatorState observer) (round + 1) := by
  intro observer observerInRange observerCorrectAvailable
  have accepted : EveryCorrectAvailableValidatorAccepted faults
      (timed.execution.trace time) round := by
    intro currentObserver author currentObserverInRange currentObserverCorrect
      authorInRange authorCorrect
    rcases stable time (Nat.le_refl _) currentObserver author
        currentObserverInRange currentObserverCorrect authorInRange authorCorrect
      with ⟨reference, recorded, _retained⟩
    simp [recorded]
  have retained : ∀ author reference,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ((timed.execution.trace time).validatorState observer
        ).acceptedRepresentative round author = some reference →
      ((timed.execution.trace time).validatorState observer).retained
        reference = true := by
    intro author reference authorInRange authorCorrect recorded
    rcases stable time (Nat.le_refl _) observer author observerInRange
        observerCorrectAvailable authorInRange authorCorrect with
      ⟨selected, selectedRecorded, selectedRetained⟩
    have sameReference : reference = selected := by
      exact Option.some.inj (recorded.symm.trans selectedRecorded)
    simpa [sameReference] using selectedRetained
  exact common_retained_correct_round_gives_recovery_parent_quorum
    representatives observerInRange observerCorrectAvailable accepted retained
      (roundAboveGc observer observerInRange observerCorrectAvailable)

/-- A finite timer-paced author family becomes one common accepted correct
round in a stable-head suffix. The proof aggregates actual per-peer packets and
recursive capsule synchronization. -/
theorem observed_timer_paced_round_family_eventually_is_common_accepted
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
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation round : Nat}
    (family : EveryCorrectAvailableValidatorObservedTimerPacedRound timed waits
      observation round)
    (roundAboveGc : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ((timed.execution.trace observation).validatorState observer).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ finish,
      observation ≤ finish ∧
      EveryCorrectAvailableValidatorAccepted faults
        (timed.execution.trace finish) round := by
  let authorDone := fun observer author time =>
    (((timed.execution.trace time).validatorState observer
      ).acceptedRepresentative round author).isSome = true
  have authorDonePersists : ∀ observer author earlier later,
      observer < config.authorityCount →
      earlier ≤ later →
      authorDone observer author earlier →
      authorDone observer author later := by
    intro observer author earlier later observerInRange ordered done
    cases acceptedAtEarlier :
        ((timed.execution.trace earlier).validatorState observer
          ).acceptedRepresentative round author with
    | none =>
        unfold authorDone at done
        simp [acceptedAtEarlier] at done
    | some reference =>
        have acceptedAtLater :=
          accepted_representative_persists_in_validator_execution
            timed.execution observerInRange ordered acceptedAtEarlier
        simp [authorDone, acceptedAtLater]
  have eachObserver : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ∃ finish,
        observation ≤ finish ∧
        ∀ author,
          author < config.authorityCount →
          faults.correctAvailable author = true →
          authorDone observer author finish := by
    intro observer observerInRange observerCorrect
    have eachAuthor : ∀ author,
        author < config.authorityCount →
        faults.correctAvailable author = true →
        ∃ finish, observation ≤ finish ∧
          authorDone observer author finish := by
      intro author authorInRange authorCorrect
      rcases family author authorInRange authorCorrect with
        ⟨production, observationBeforePersistence,
          _observationBeforeSend⟩
      rcases
          timer_paced_round_production_eventually_recorded_at_correct_observer
            pins capsuleSync acceptance representatives production
            observationBeforePersistence observerInRange observerCorrect
            (roundAboveGc observer observerInRange observerCorrect) afterGst
            active noAdvance with
        ⟨finish, observationBeforeFinish, recorded⟩
      exact ⟨finish, observationBeforeFinish, by
        simp [authorDone, recorded]⟩
    rcases eventually_every_selected_validator faults.correctAvailable
        (authorDone observer) observation
        (by
          intro author earlier later ordered done
          exact authorDonePersists observer author earlier later
            observerInRange ordered done)
        eachAuthor with
      ⟨finish, observationBeforeFinish, allAuthors⟩
    exact ⟨finish, observationBeforeFinish, allAuthors⟩
  let observerDone := fun observer time =>
    observer < config.authorityCount →
    ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      authorDone observer author time
  have observerDonePersists : ∀ observer earlier later,
      earlier ≤ later →
      observerDone observer earlier →
      observerDone observer later := by
    intro observer earlier later ordered done observerInRange author
      authorInRange authorCorrect
    exact authorDonePersists observer author earlier later observerInRange
      ordered (done observerInRange author authorInRange authorCorrect)
  rcases eventually_every_selected_validator faults.correctAvailable
      observerDone observation
      (by
        intro observer earlier later ordered done
        exact observerDonePersists observer earlier later ordered done)
      (by
        intro observer observerInRange observerCorrect
        rcases eachObserver observer observerInRange observerCorrect with
          ⟨finish, observationBeforeFinish, allAuthors⟩
        exact ⟨finish, observationBeforeFinish, fun _ => allAuthors⟩) with
    ⟨finish, observationBeforeFinish, allObservers⟩
  refine ⟨finish, observationBeforeFinish, ?_⟩
  intro observer author observerInRange observerCorrect authorInRange
    authorCorrect
  exact allObservers observer observerInRange observerCorrect observerInRange
    author authorInRange authorCorrect

/-- A finite family of timer-paced results has one time at which all correct,
available authors have stored and sent the round. The exact snapshots remain in
the family input and can be used by the anchor proof. -/
theorem timer_paced_round_family_eventually_is_produced
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
    {start round : Time}
    (family : EveryCorrectAvailableValidatorTimerPacedRound timed waits start
      round) :
    ∃ finish,
      start ≤ finish ∧
        EveryCorrectAvailableValidatorTimerPacedRoundWithin timed waits start
          finish round ∧
        EveryCorrectAvailableValidatorProduced faults
          (timed.execution.trace finish) round := by
  let done := fun validator time =>
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        round,
      start ≤ production.timerStartedAt ∧ production.sentTime ≤ time
  have donePersists : ∀ validator earlier later,
      earlier ≤ later → done validator earlier → done validator later := by
    intro validator earlier later ordered completed
    rcases completed with ⟨production, startBeforeTimer, sentBeforeEarlier⟩
    exact ⟨production, startBeforeTimer,
      Nat.le_trans sentBeforeEarlier ordered⟩
  have eachDone : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ∃ finish, start ≤ finish ∧ done validator finish := by
    intro validator validatorInRange validatorCorrect
    rcases family validator validatorInRange validatorCorrect with
      ⟨production, startBeforeTimer⟩
    have timerBeforeDeadline : production.timerStartedAt ≤
        production.snapshot.snapshotAt := by
      rw [production.snapshotAtDeadline]
      exact Nat.le_add_right _ _
    have sendAfterSnapshot : production.snapshot.snapshotAt ≤
        production.sentTime := Nat.le_trans production.snapshot.snapshotBeforeStore
      production.storedBeforeSent
    exact ⟨production.sentTime,
      Nat.le_trans startBeforeTimer
        (Nat.le_trans timerBeforeDeadline sendAfterSnapshot),
      production, startBeforeTimer, Nat.le_refl _⟩
  rcases eventually_every_selected_validator faults.correctAvailable done start
      donePersists eachDone with
    ⟨finish, startBeforeFinish, allDone⟩
  have within : EveryCorrectAvailableValidatorTimerPacedRoundWithin timed waits
      start finish round := by
    intro validator validatorInRange validatorCorrect
    exact allDone validator validatorInRange validatorCorrect
  have produced : EveryCorrectAvailableValidatorProduced faults
      (timed.execution.trace finish) round := by
    intro validator validatorInRange validatorCorrect
    rcases within validator validatorInRange validatorCorrect with
      ⟨production, _startBeforeTimer, sentBeforeFinish⟩
    have atSend := timer_paced_round_production_is_produced_at_send production
    cases ownValue : ((timed.execution.trace production.sentTime).validatorState
        validator).ownBlockAt round with
    | none => simp [ownValue] at atSend
    | some reference =>
        have durable := timed.execution.durableStateMonotone validator
          production.sentTime finish validatorInRange sentBeforeFinish
        exact ⟨by simp [durable.own_block_persists ownValue],
          durable.sent_own_block_persists atSend.2⟩
  exact ⟨finish, startBeforeFinish, within, produced⟩

/-- Recovering stays true after one active snapshot unless this validator
installs a later commit. Validators do not need the same local commit head. -/
theorem active_recovery_snapshot_persists_without_commit_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    {snapshot current validator : Time}
    (state : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true)
    (ordered : snapshot ≤ current)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    ValidatorCommitProgressRecoveryModeAt timed modeRules.recoveryWait current
      validator := by
  have durable := timed.execution.durableStateMonotone validator snapshot
    current validatorInRange ordered
  have notStrict : ¬
      ((timed.execution.trace snapshot).validatorState
          validator).commitHead.index <
        ((timed.execution.trace current).validatorState
          validator).commitHead.index := by
    intro advanced
    exact noAdvance ⟨validator, current, validatorInRange, validatorCorrect,
      ordered, advanced⟩
  have sameIndex :
      ((timed.execution.trace snapshot).validatorState
          validator).commitHead.index =
        ((timed.execution.trace current).validatorState
          validator).commitHead.index := by
    have monotone := durable.1
    omega
  have sameLastCommit := modeRules.sameCommitIndexKeepsLastCommitTime
    validator snapshot current validatorInRange ordered sameIndex
  exact recovery_mode_persists_with_stable_last_commit validatorInRange ordered
    (state.recovering validator validatorInRange validatorCorrect)
    (active current ordered) sameLastCommit.symm

/-- A complete produced round persists in the main validator execution. -/
theorem every_correct_produced_round_persists_in_execution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {earlier later round : Nat}
    (ordered : earlier ≤ later)
    (produced : EveryCorrectAvailableValidatorProduced faults
      (execution.trace earlier) round) :
    EveryCorrectAvailableValidatorProduced faults (execution.trace later)
      round := by
  intro validator validatorInRange validatorCorrect
  rcases produced validator validatorInRange validatorCorrect with
    ⟨ownSome, sent⟩
  cases ownValue : ((execution.trace earlier).validatorState
      validator).ownBlockAt round with
  | none => simp [ownValue] at ownSome
  | some reference =>
      have durable := execution.durableStateMonotone validator earlier later
        validatorInRange ordered
      have ownLater := durable.own_block_persists ownValue
      have sentLater := durable.sent_own_block_persists sent
      exact ⟨by simp [ownLater], sentLater⟩

/-- A complete accepted round persists in the main validator execution. -/
theorem every_correct_accepted_round_persists_in_execution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {earlier later round : Nat}
    (ordered : earlier ≤ later)
    (accepted : EveryCorrectAvailableValidatorAccepted faults
      (execution.trace earlier) round) :
    EveryCorrectAvailableValidatorAccepted faults (execution.trace later)
      round := by
  intro observer author observerInRange observerCorrect authorInRange
    authorCorrect
  have acceptedSome := accepted observer author observerInRange observerCorrect
    authorInRange authorCorrect
  cases acceptedValue : ((execution.trace earlier).validatorState
      observer).acceptedRepresentative round author with
  | none => simp [acceptedValue] at acceptedSome
  | some reference =>
      have laterValue := accepted_representative_persists_in_validator_execution
        execution observerInRange ordered acceptedValue
      simp [laterValue]

/-- One finite timer-paced recovery family becomes a produced and commonly
accepted correct quorum layer. The common finish time is a proof result. -/
theorem observed_timer_paced_round_family_eventually_is_common_layer
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
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation round : Nat}
    (family : EveryCorrectAvailableValidatorObservedTimerPacedRound timed waits
      observation round)
    (roundAboveGc : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ((timed.execution.trace observation).validatorState observer).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ finish,
      observation ≤ finish ∧
      ProducedCorrectQuorumLayer config faults
          (timed.execution.trace finish) round ∧
      CommonAcceptedCorrectQuorumLayer config faults
          (timed.execution.trace finish) round := by
  rcases observed_timer_paced_round_family_eventually_is_produced family with
    ⟨producedAt, observationBeforeProduced, _within, produced⟩
  rcases observed_timer_paced_round_family_eventually_is_common_accepted
      pins capsuleSync acceptance representatives family roundAboveGc afterGst
      active noAdvance with
    ⟨acceptedAt, observationBeforeAccepted, accepted⟩
  let finish := max producedAt acceptedAt
  have producedBeforeFinish : producedAt ≤ finish := Nat.le_max_left _ _
  have acceptedBeforeFinish : acceptedAt ≤ finish := Nat.le_max_right _ _
  have producedAtFinish := every_correct_produced_round_persists_in_execution
    timed.execution producedBeforeFinish produced
  have acceptedAtFinish := every_correct_accepted_round_persists_in_execution
    timed.execution acceptedBeforeFinish accepted
  refine ⟨finish,
    Nat.le_trans observationBeforeProduced producedBeforeFinish, ?_, ?_⟩
  · exact every_correct_available_validator_produced_gives_quorum_layer
      config faults (timed.execution.trace finish) round producedAtFinish
  · exact every_correct_available_validator_accepted_gives_common_layer
      config faults (timed.execution.trace finish) round acceptedAtFinish

/-- Current exact recovery timer inputs give a real local commit advance or
one produced and commonly accepted correct quorum layer. -/
theorem common_recovery_current_inputs_give_commit_advance_or_common_layer
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {observation round : Time}
    (inputs : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ValidatorRecoveryTimerCurrentInputAt timed observation validator)
    (sameTarget : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound + 1 = round)
    (roundAboveGc : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ((timed.execution.trace observation).validatorState observer).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed observation ∨
      ∃ finish,
        observation ≤ finish ∧
        ProducedCorrectQuorumLayer config faults
            (timed.execution.trace finish) round ∧
        CommonAcceptedCorrectQuorumLayer config faults
            (timed.execution.trace finish) round := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed observation
  · exact Or.inl advanced
  · rcases common_recovery_current_inputs_give_commit_advance_or_observed_round
        timerSource pacing arms latchSource effects authorityCountAtLeastTwo
        inputs sameTarget active with laterAdvance | family
    · exact False.elim (advanced laterAdvance)
    · exact Or.inr
        (observed_timer_paced_round_family_eventually_is_common_layer pins
          capsuleSync acceptance representatives family roundAboveGc afterGst
          active advanced)

/-- If one atomic step first reaches a signer-floor target, that step persists
the proposal block which reaches the target. -/
private theorem recovery_atomic_signer_floor_target_crossing
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time target validator : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
    (step : ValidatorAtomicStep config faults protocolPacket program time before
      event after)
    (beforeTarget :
      (before.validatorState validator).highestSignedRound < target)
    (targetReached :
      target ≤ (after.validatorState validator).highestSignedRound) :
    ∃ block,
      event = .localAction validator (.persistProposal block) ∧
        target ≤ block.reference.round := by
  cases step with
  | localAction actionValidatorInRange actionValidatorCorrect enabled effect
      structural otherUnchanged epochUnchanged historyMonotone =>
      rename_i actionValidator action
      by_cases sameValidator : actionValidator = validator
      · subst actionValidator
        have ownEffect := structural.2.2.1
        cases action with
        | persistProposal block =>
            simp only [OwnBlockActionEffect] at ownEffect
            exact ⟨block, rfl, by simpa [ownEffect.2.2] using targetReached⟩
        | sendReplayManifest _ _ =>
            simp only [OwnBlockActionEffect] at ownEffect
            have impossible : target ≤
                (before.validatorState validator).highestSignedRound := by
              calc
                target ≤ (after.validatorState validator).highestSignedRound :=
                  targetReached
                _ = (before.validatorState validator).highestSignedRound :=
                  ownEffect.2
            exact False.elim ((Nat.not_le_of_gt beforeTarget) impossible)
        | enterRecovery | requestBlock | serveBlock | acceptBlock | sendBlock |
            proposeNormal | proposeNext | alignProposal | runCommitter |
            runReplayCommitter | recordCommit | applySyncedCommit =>
            simp only [OwnBlockActionEffect] at ownEffect
            have impossible : target ≤
                (before.validatorState validator).highestSignedRound := by
              calc
                target ≤ (after.validatorState validator).highestSignedRound :=
                  targetReached
                _ = (before.validatorState validator).highestSignedRound :=
                  ownEffect.2
            exact False.elim ((Nat.not_le_of_gt beforeTarget) impossible)
      · have unchanged := otherUnchanged validator (Ne.symm sameValidator)
        have impossible : target ≤
            (before.validatorState validator).highestSignedRound := by
          calc
            target ≤ (after.validatorState validator).highestSignedRound :=
              targetReached
            _ = (before.validatorState validator).highestSignedRound := by
              rw [unchanged]
        exact False.elim ((Nat.not_le_of_gt beforeTarget) impossible)
  | deliverPacket packetPresent packetProtocol deliveredAt senderInRange
      receiverInRange deliveryEnabled deliveryEffect structural otherUnchanged
      epochUnchanged catalogUnchanged packetsUnchanged =>
      rename_i packetId packet
      by_cases sameValidator : packet.receiver = validator
      · subst validator
        have unchanged := structural.2.2.2.2.2.2.1
        have impossible : target ≤
            (before.validatorState packet.receiver).highestSignedRound := by
          calc
            target ≤ (after.validatorState packet.receiver).highestSignedRound :=
              targetReached
            _ = (before.validatorState packet.receiver).highestSignedRound :=
              unchanged
        exact False.elim ((Nat.not_le_of_gt beforeTarget) impossible)
      · have unchanged := otherUnchanged validator (Ne.symm sameValidator)
        have impossible : target ≤
            (before.validatorState validator).highestSignedRound := by
          calc
            target ≤ (after.validatorState validator).highestSignedRound :=
              targetReached
            _ = (before.validatorState validator).highestSignedRound := by
              rw [unchanged]
        exact False.elim ((Nat.not_le_of_gt beforeTarget) impossible)
  | clockTick clocksMonotone stateUpdate =>
      subst after
      have impossible : target ≤
          (before.validatorState validator).highestSignedRound := by
        simpa [ValidatorWorldState.updateClocks] using targetReached
      exact False.elim ((Nat.not_le_of_gt beforeTarget) impossible)

/-- If one event batch first reaches a signer-floor target, one proposal
persistence in that batch reaches the target. -/
private theorem recovery_world_step_target_crossing
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time target validator : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (beforeTarget :
      (before.validatorState validator).highestSignedRound < target)
    (targetReached :
      target ≤ (after.validatorState validator).highestSignedRound) :
    ∃ block,
      ValidatorLocalActionOccurs events validator (.persistProposal block) ∧
        target ≤ block.reference.round := by
  induction step with
  | nil =>
      exact False.elim ((Nat.not_le_of_gt beforeTarget) targetReached)
  | @cons firstBefore middle final event tail firstStep tailStep
      inductionHypothesis =>
      by_cases reachedInFirst : target ≤
          (middle.validatorState validator).highestSignedRound
      · rcases recovery_atomic_signer_floor_target_crossing firstStep
            beforeTarget reachedInFirst with
          ⟨block, eventExact, blockReaches⟩
        subst event
        exact ⟨block, ⟨[], tail, by simp⟩, blockReaches⟩
      · have middleBeforeTarget :
            (middle.validatorState validator).highestSignedRound < target :=
          Nat.lt_of_not_ge reachedInFirst
        rcases inductionHypothesis middleBeforeTarget targetReached with
          ⟨block, ⟨beforeEvents, afterEvents, tailExact⟩, blockReaches⟩
        exact ⟨block, ⟨event :: beforeEvents, afterEvents,
          by simp [tailExact]⟩,
          blockReaches⟩

/-- When a later state reaches a target above the starting signer floor, one
intervening batch contains the proposal persistence that reaches that target. -/
theorem signer_floor_target_reached_has_target_persist_proposal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {start finish target validator : Time}
    (startBeforeFinish : start ≤ finish)
    (targetAboveStart :
      ((execution.trace start).validatorState validator).highestSignedRound <
        target)
    (targetReached : target ≤
      ((execution.trace finish).validatorState validator).highestSignedRound) :
    ∃ persistTime block,
      start ≤ persistTime ∧
        persistTime < finish ∧
        ValidatorLocalActionOccurs (execution.events persistTime) validator
          (.persistProposal block) ∧
        target ≤ block.reference.round := by
  obtain ⟨offset, finishShape⟩ := Nat.exists_eq_add_of_le startBeforeFinish
  subst finish
  induction offset with
  | zero =>
      have impossible : target ≤
          ((execution.trace start).validatorState
            validator).highestSignedRound := by
        simpa using targetReached
      exact False.elim ((Nat.not_le_of_gt targetAboveStart) impossible)
  | succ previous inductionHypothesis =>
      by_cases reachedEarlier : target ≤
          ((execution.trace (start + previous)).validatorState
            validator).highestSignedRound
      · rcases inductionHypothesis (Nat.le_add_right start previous)
            reachedEarlier with
          ⟨persistTime, block, startBeforePersist, persistBefore,
            persisted, blockReaches⟩
        have persistBeforeNext : persistTime < start + (previous + 1) := by
          calc
            persistTime < start + previous := persistBefore
            _ < start + previous + 1 := Nat.lt_succ_self _
            _ = start + (previous + 1) := by simp [Nat.add_assoc]
        exact ⟨persistTime, block, startBeforePersist, persistBeforeNext,
          persisted, blockReaches⟩
      · have beforeTarget :
            ((execution.trace (start + previous)).validatorState
                validator).highestSignedRound < target := by
          exact Nat.lt_of_not_ge reachedEarlier
        have reachedNext : target ≤
            ((execution.trace (start + previous + 1)).validatorState
                validator).highestSignedRound := by
          simpa [Nat.add_assoc] using targetReached
        rcases recovery_world_step_target_crossing
            (execution.stepsFollowRules (start + previous)) beforeTarget
              reachedNext with
          ⟨block, persisted, blockReaches⟩
        have startBeforePersist : start ≤ start + previous :=
          Nat.le_add_right _ _
        have persistBeforeFinish : start + previous < start + (previous + 1) := by
          calc
            start + previous < start + previous + 1 := Nat.lt_succ_self _
            _ = start + (previous + 1) := by simp [Nat.add_assoc]
        exact ⟨start + previous, block, startBeforePersist,
          persistBeforeFinish, persisted, blockReaches⟩

/-- The first batch which crosses a signer-floor target contains a proposal
persistence which starts below that target. This is the no-hidden-jump form
used by the recovery-round policy. -/
theorem signer_floor_first_target_crossing_has_persist_proposal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {start finish target validator : Time}
    (startBeforeFinish : start ≤ finish)
    (targetAboveStart :
      ((execution.trace start).validatorState validator).highestSignedRound <
        target)
    (targetReached : target ≤
      ((execution.trace finish).validatorState validator).highestSignedRound) :
    ∃ persistTime block,
      start ≤ persistTime ∧
        persistTime < finish ∧
        ((execution.trace persistTime).validatorState
          validator).highestSignedRound < target ∧
        ValidatorLocalActionOccurs (execution.events persistTime) validator
          (.persistProposal block) ∧
        target ≤ block.reference.round := by
  obtain ⟨offset, finishShape⟩ := Nat.exists_eq_add_of_le startBeforeFinish
  subst finish
  induction offset with
  | zero =>
      have impossible : target ≤
          ((execution.trace start).validatorState
            validator).highestSignedRound := by
        simpa using targetReached
      exact False.elim ((Nat.not_le_of_gt targetAboveStart) impossible)
  | succ previous inductionHypothesis =>
      by_cases reachedEarlier : target ≤
          ((execution.trace (start + previous)).validatorState
            validator).highestSignedRound
      · rcases inductionHypothesis (Nat.le_add_right start previous)
            reachedEarlier with
          ⟨persistTime, block, startBeforePersist, persistBefore,
            floorBeforeTarget, persisted, blockReaches⟩
        have persistBeforeNext : persistTime < start + (previous + 1) := by
          calc
            persistTime < start + previous := persistBefore
            _ < start + previous + 1 := Nat.lt_succ_self _
            _ = start + (previous + 1) := by simp [Nat.add_assoc]
        exact ⟨persistTime, block, startBeforePersist, persistBeforeNext,
          floorBeforeTarget, persisted, blockReaches⟩
      · have beforeTarget :
            ((execution.trace (start + previous)).validatorState
                validator).highestSignedRound < target := by
          exact Nat.lt_of_not_ge reachedEarlier
        have reachedNext : target ≤
            ((execution.trace (start + previous + 1)).validatorState
                validator).highestSignedRound := by
          simpa [Nat.add_assoc] using targetReached
        rcases recovery_world_step_target_crossing
            (execution.stepsFollowRules (start + previous)) beforeTarget
              reachedNext with
          ⟨block, persisted, blockReaches⟩
        have startBeforePersist : start ≤ start + previous :=
          Nat.le_add_right _ _
        have persistBeforeFinish : start + previous < start + (previous + 1) := by
          calc
            start + previous < start + previous + 1 := Nat.lt_succ_self _
            _ = start + (previous + 1) := by simp [Nat.add_assoc]
        exact ⟨start + previous, block, startBeforePersist,
          persistBeforeFinish, beforeTarget, persisted, blockReaches⟩

/-- The first crossing of the proof-only common base is an exact proposal at
that base. Genesis and the one post-GC safe-resume target are both at or below
the local recovery base. Exact-next work below the common base also cannot
overtake it. -/
theorem active_recovery_crossing_common_base_is_exact
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    {snapshot finish validator baseRound : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (localBaseAtMostCommon :
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator) ≤
        baseRound)
    (startFloorBelowBase :
      ((timed.execution.trace snapshot).validatorState
        validator).highestSignedRound < baseRound)
    (snapshotBeforeFinish : snapshot ≤ finish)
    (targetReached : baseRound ≤
      ((timed.execution.trace finish).validatorState
        validator).highestSignedRound)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    ∃ persistTime block,
      snapshot ≤ persistTime ∧
        persistTime < finish ∧
        ValidatorLocalActionOccurs (timed.execution.events persistTime)
          validator (.persistProposal block) ∧
        block.reference.round = baseRound := by
  rcases signer_floor_first_target_crossing_has_persist_proposal
      timed.execution snapshotBeforeFinish startFloorBelowBase targetReached with
    ⟨persistTime, block, snapshotBeforePersist, persistBeforeFinish,
      floorBeforeTarget, persisted, blockReaches⟩
  have modeAtPersist := active_recovery_snapshot_persists_without_commit_advance
    modeRules recovery validatorInRange validatorCorrectAvailable
      snapshotBeforePersist active noAdvance
  have selectedRound := roundRules.persistedProposalUsesRecoveryRound
    persistTime validator block validatorInRange validatorCorrectAvailable
      modeAtPersist persisted
  have gcAtPersist := no_commit_advance_keeps_correct_gc_round
    validatorInRange validatorCorrectAvailable snapshotBeforePersist noAdvance
  have floorMonotone :=
    (timed.execution.durableStateMonotone validator snapshot persistTime
      validatorInRange snapshotBeforePersist).2.2.2.2.2.2.1
  have selectedAtMostTarget :
      ValidatorCommitProgressProposalRound
          ((timed.execution.trace persistTime).validatorState validator) ≤
        baseRound := by
    unfold ValidatorCommitProgressProposalRound
    split
    next floorAtOrBelowGc =>
      split
      next gcZero =>
        have localBasePositive : 1 ≤ baseRound := by
          have startFloorAtOrBelowGc :
              ((timed.execution.trace snapshot).validatorState
                  validator).highestSignedRound ≤
                ((timed.execution.trace snapshot).validatorState
                  validator).gcRound := by
            rw [← gcAtPersist]
            exact Nat.le_trans floorMonotone floorAtOrBelowGc
          have startGcZero :
              ((timed.execution.trace snapshot).validatorState
                validator).gcRound = 0 := by
            rw [← gcAtPersist]
            exact gcZero
          have startFloorZero :
              ((timed.execution.trace snapshot).validatorState
                validator).highestSignedRound = 0 := by
            omega
          simpa [ValidatorLocalRecoveryBaseRound, startFloorZero,
            startGcZero] using localBaseAtMostCommon
        exact localBasePositive
      next gcNonzero =>
        have startFloorAtOrBelowGc :
            ((timed.execution.trace snapshot).validatorState
                validator).highestSignedRound ≤
              ((timed.execution.trace snapshot).validatorState
                validator).gcRound := by
          rw [← gcAtPersist]
          exact Nat.le_trans floorMonotone floorAtOrBelowGc
        have startGcNonzero :
            ((timed.execution.trace snapshot).validatorState
              validator).gcRound ≠ 0 := by
          rw [gcAtPersist] at gcNonzero
          exact gcNonzero
        have bootstrapAtMostBase :
            ((timed.execution.trace snapshot).validatorState
                validator).gcRound + 2 ≤ baseRound := by
          simpa [ValidatorLocalRecoveryBaseRound, startFloorAtOrBelowGc,
            startGcNonzero] using localBaseAtMostCommon
        rw [gcAtPersist]
        exact bootstrapAtMostBase
    next floorAboveGc =>
      omega
  have blockAtMostTarget : block.reference.round ≤ baseRound := by
    rw [selectedRound]
    exact selectedAtMostTarget
  exact ⟨persistTime, block, snapshotBeforePersist, persistBeforeFinish,
    persisted, Nat.le_antisymm blockAtMostTarget blockReaches⟩

/-- The exact common-base crossing keeps its durable proposal and every
addressed peer broadcast. -/
theorem active_recovery_crossing_common_base_gives_source
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot finish validator baseRound : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (localBaseAtMostCommon :
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator) ≤
        baseRound)
    (startFloorBelowBase :
      ((timed.execution.trace snapshot).validatorState
        validator).highestSignedRound < baseRound)
    (snapshotBeforeFinish : snapshot ≤ finish)
    (targetReached : baseRound ≤
      ((timed.execution.trace finish).validatorState
        validator).highestSignedRound)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins modeRules.recoveryWait snapshot validator baseRound) := by
  rcases active_recovery_crossing_common_base_is_exact modeRules roundRules
      recovery validatorInRange validatorCorrectAvailable localBaseAtMostCommon
        startFloorBelowBase snapshotBeforeFinish targetReached active noAdvance
    with ⟨persistTime, block, snapshotBeforePersist, _persistBeforeFinish,
      persisted, exactRound⟩
  have blockAboveStartFloor :
      ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound < block.reference.round := by
    rw [exactRound]
    exact startFloorBelowBase
  let broadcast := Classical.choice
    (persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable snapshotBeforePersist blockAboveStartFloor
          persisted)
  refine ⟨.persisted broadcast.1 ?_⟩
  rw [broadcast.2.2]
  exact exactRound

/-- One proof-selected durable tip catches a resumed correct host to the same
common round.

If the host crosses the target while the causal history is synchronized, the
proof keeps that exact persistence event. Otherwise, the accepted causal
history drives the remaining exact-next rounds. -/
theorem stable_recovery_tip_source_catches_resumed_validator_to_common_base
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins
      modeRules.recoveryWait)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot sourceAt resumeAt holder validator baseRound : Time}
    (source : ValidatorStableRecoveryTipSource pins modeRules.recoveryWait
      sourceAt holder baseRound)
    (snapshotBeforeSource : snapshot ≤ sourceAt)
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (localBaseAtMostCommon :
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator) ≤
        baseRound)
    (startFloorBelowBase :
      ((timed.execution.trace snapshot).validatorState
        validator).highestSignedRound < baseRound)
    (snapshotBeforeResume : snapshot ≤ resumeAt)
    (resumeGcBelowFloor :
      ((timed.execution.trace resumeAt).validatorState validator).gcRound <
        ((timed.execution.trace resumeAt).validatorState
          validator).highestSignedRound)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins modeRules.recoveryWait snapshot validator baseRound) := by
  by_cases validatorIsHolder : validator = holder
  · subst validator
    exact ⟨.durableTip sourceAt snapshotBeforeSource source⟩
  · have currentTip :
        ((timed.execution.trace sourceAt).validatorState holder).ownBlockAt
            ((timed.execution.trace sourceAt).validatorState
              holder).highestSignedRound =
          some source.entry.capsule.targetBlock.reference := by
      rw [source.currentFloor, source.targetBlock]
      exact source.ownTip
    have activeFromSource : ∀ time, sourceAt ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time sourceBeforeTime
      exact active time (Nat.le_trans snapshotBeforeSource sourceBeforeTime)
    have noAdvanceFromSource :
        ¬SomeCorrectAvailableCommitAdvance timed sourceAt :=
      no_commit_advance_persists_to_later_start snapshotBeforeSource noAdvance
    have positiveTip : 0 <
        ((timed.execution.trace sourceAt).validatorState
          holder).highestSignedRound := by
      rw [source.currentFloor]
      exact source.roundPositive
    let history := Classical.choice
      (stable_recovery_tip_installs_pinned_causal_history pins broadcast
        effects capsuleSync acceptance holderInRange holderCorrectAvailable
          validatorInRange validatorCorrectAvailable validatorIsHolder
            (Nat.le_trans recovery.afterGst snapshotBeforeSource)
              activeFromSource noAdvanceFromSource source.recoveryMode
                positiveTip currentTip source.stored source.pinned)
    have snapshotBeforeAccepted : snapshot ≤ history.acceptedFinish := by
      exact Nat.le_trans snapshotBeforeSource
        (Nat.le_trans history.rootBeforeDiscovery
          (Nat.le_trans (Nat.le_succ history.discoveryFinish)
            history.discoveryBeforeAccepted))
    let catchStart := max resumeAt history.acceptedFinish
    have snapshotBeforeCatch : snapshot ≤ catchStart :=
      Nat.le_trans snapshotBeforeResume (Nat.le_max_left _ _)
    have acceptedBeforeCatch : history.acceptedFinish ≤ catchStart :=
      Nat.le_max_right _ _
    by_cases targetReached : baseRound ≤
        ((timed.execution.trace catchStart).validatorState
          validator).highestSignedRound
    · exact active_recovery_crossing_common_base_gives_source modeRules
        roundRules latchSource effects pins authorityCountAtLeastTwo recovery
          validatorInRange validatorCorrectAvailable localBaseAtMostCommon
            startFloorBelowBase snapshotBeforeCatch targetReached active
              noAdvance
    · let floor := ((timed.execution.trace catchStart
        ).validatorState validator).highestSignedRound
      have capsuleTargetRound : source.entry.capsule.targetRound = baseRound := by
        unfold CausalRecoveryCapsule.targetRound
        rw [source.targetBlock, source.exactRound]
      have floorBelowTarget : floor < source.entry.capsule.targetRound := by
        rw [capsuleTargetRound]
        exact Nat.lt_of_not_ge targetReached
      have startFloorAtMostFloor :
          ((timed.execution.trace resumeAt).validatorState
              validator).highestSignedRound ≤ floor :=
        (timed.execution.durableStateMonotone validator resumeAt catchStart
          validatorInRange (Nat.le_max_left _ _)
          ).2.2.2.2.2.2.1
      have sameGcAtResume := no_commit_advance_keeps_correct_gc_round
        validatorInRange validatorCorrectAvailable snapshotBeforeResume noAdvance
      have sameGcAtCatch := no_commit_advance_keeps_correct_gc_round
        validatorInRange validatorCorrectAvailable snapshotBeforeCatch noAdvance
      have sameGc :
          ((timed.execution.trace catchStart).validatorState
              validator).gcRound =
            ((timed.execution.trace resumeAt).validatorState
              validator).gcRound :=
        sameGcAtCatch.trans sameGcAtResume.symm
      have gcBelowFloor :
          ((timed.execution.trace catchStart).validatorState
            validator).gcRound < floor := by
        rw [sameGc]
        exact Nat.lt_of_lt_of_le resumeGcBelowFloor startFloorAtMostFloor
      simpa [capsuleTargetRound] using
        (stable_usable_history_catches_validator_to_exact_round_source
          (recoveryWait := modeRules.recoveryWait) timerSource pacing arms
            latchSource effects pins authorityCountAtLeastTwo
              source.entry.capsule validatorInRange validatorCorrectAvailable
                snapshotBeforeAccepted acceptedBeforeCatch rfl
                  floorBelowTarget gcBelowFloor
                    (history.usable_from_accepted_finish capsuleSync) active
                      noAdvance)

/-- One host has completed the local genesis or post-GC resume step. The
signer floor is now strictly above its local GC boundary. -/
structure ValidatorRecoveryResumePoint
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (snapshot validator : Time) where
  resumeAt : Time
  snapshotBeforeResume : snapshot ≤ resumeAt
  gcBelowFloor :
    ((timed.execution.trace resumeAt).validatorState validator).gcRound <
      ((timed.execution.trace resumeAt).validatorState
        validator).highestSignedRound

/-- A host whose signer floor is already above its GC boundary is already at a
valid exact-next recovery resume point. -/
theorem current_floor_above_gc_is_recovery_resume_point
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {snapshot validator : Time}
    (gcBelowFloor :
      ((timed.execution.trace snapshot).validatorState validator).gcRound <
        ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound) :
    Nonempty (ValidatorRecoveryResumePoint timed snapshot validator) := by
  exact ⟨{
    resumeAt := snapshot
    snapshotBeforeResume := Nat.le_refl _
    gcBelowFloor }⟩

/-- A legal normal broadcast whose target is above the starting GC boundary
leaves the signer at a valid exact-next recovery resume point. A stable commit
head keeps the GC boundary fixed while the protected proposal and send work
finish. -/
theorem normal_broadcast_above_gc_gives_recovery_resume_point
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
    {snapshot start validator targetRound : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (snapshotBeforeStart : snapshot ≤ start)
    (targetAboveGc :
      ((timed.execution.trace start).validatorState validator).gcRound <
        targetRound)
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorRecoveryResumePoint timed snapshot validator) := by
  have startBeforeFinish : start ≤ production.finish :=
    Nat.le_trans production.startBeforeProposalAction (by
      have actionBeforePersist : production.proposalActionAt ≤
          production.persistedAt :=
        Nat.le_trans (Nat.le_succ production.proposalActionAt)
          production.proposalBeforePersistence
      have persistBeforeFinish : production.persistedAt ≤
          production.finish :=
        Nat.le_trans (Nat.le_succ production.persistedAt)
          production.persistenceBeforeFinish
      exact Nat.le_trans actionBeforePersist persistBeforeFinish)
  have snapshotBeforeFinish : snapshot ≤ production.finish :=
    Nat.le_trans snapshotBeforeStart startBeforeFinish
  have sameGc := no_commit_advance_keeps_correct_gc_round validatorInRange
    validatorCorrectAvailable snapshotBeforeFinish noAdvance
  have sameGcAtStart := no_commit_advance_keeps_correct_gc_round
    validatorInRange validatorCorrectAvailable snapshotBeforeStart noAdvance
  refine ⟨{
    resumeAt := production.finish
    snapshotBeforeResume := snapshotBeforeFinish
    gcBelowFloor := ?_ }⟩
  rw [sameGc, ← sameGcAtStart, production.signerFloorAtFinish,
    production.proposalRound]
  exact targetAboveGc

/-- Static genesis storage and the paced round-one pipeline produce the first
valid exact-next recovery resume point. No correct-host commit advance is used
as a positive result. -/
theorem canonical_genesis_gives_recovery_resume_point
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (genesis : ValidatorCanonicalGenesisParentRules timed)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {recoveryWait snapshot validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (floorIsZero :
      ((timed.execution.trace snapshot).validatorState
        validator).highestSignedRound = 0)
    (gcIsZero :
      ((timed.execution.trace snapshot).validatorState validator).gcRound = 0)
    (active : ∀ later, snapshot ≤ later →
      (timed.execution.trace later).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorRecoveryResumePoint timed snapshot validator) := by
  let source := Classical.choice
    (canonical_genesis_gives_round_one_source_without_commit_advance
      (recoveryWait := recoveryWait) timerSource pacing arms latchSource
        effects pins genesis
        authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
          floorIsZero active noAdvance)
  rcases exact_recovery_round_source_eventually_is_owned source with
    ⟨readyAt, reference, snapshotBeforeReady, ownAtOne⟩
  have oneAtMostFloor : 1 ≤
      ((timed.execution.trace readyAt).validatorState
        validator).highestSignedRound :=
    (timed.execution.statesWellFormed readyAt validator validatorInRange
      ).ownBlockDoesNotExceedSignerFloor 1 reference ownAtOne
  have sameGc := no_commit_advance_keeps_correct_gc_round validatorInRange
    validatorCorrectAvailable snapshotBeforeReady noAdvance
  refine ⟨{
    resumeAt := readyAt
    snapshotBeforeResume := snapshotBeforeReady
    gcBelowFloor := ?_ }⟩
  rw [sameGc, gcIsZero]
  exact oneAtMostFloor

/-- An exact source above the observation-time GC boundary eventually leaves
the author at a valid local recovery resume point. -/
theorem exact_recovery_round_source_gives_recovery_resume_point
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait snapshot validator round : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (source : ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait snapshot validator round)
    (roundAboveGc :
      ((timed.execution.trace snapshot).validatorState validator).gcRound <
        round)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorRecoveryResumePoint timed snapshot validator) := by
  rcases exact_recovery_round_source_eventually_is_owned source with
    ⟨readyAt, reference, snapshotBeforeReady, ownAtRound⟩
  have roundAtMostFloor : round ≤
      ((timed.execution.trace readyAt).validatorState
        validator).highestSignedRound :=
    (timed.execution.statesWellFormed readyAt validator validatorInRange
      ).ownBlockDoesNotExceedSignerFloor round reference ownAtRound
  have sameGc := no_commit_advance_keeps_correct_gc_round validatorInRange
    validatorCorrectAvailable snapshotBeforeReady noAdvance
  refine ⟨{
    resumeAt := readyAt
    snapshotBeforeResume := snapshotBeforeReady
    gcBelowFloor := ?_ }⟩
  rw [sameGc]
  exact Nat.lt_of_lt_of_le roundAboveGc roundAtMostFloor

/-- An exact common-round source can be normalized to the author's current
restart-safe tip source. The persisted-source case uses the exact signer floor
at the completed proposal; the durable-tip case is already normalized. -/
theorem exact_recovery_round_source_gives_stable_tip_source
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {snapshot author round : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (source : ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins modeRules.recoveryWait snapshot author round)
    (roundPositive : 0 < round)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    ∃ sourceAt,
      snapshot ≤ sourceAt ∧
        Nonempty (ValidatorStableRecoveryTipSource pins modeRules.recoveryWait
          sourceAt author round) := by
  cases source with
  | durableTip sourceAt snapshotBeforeSource tip =>
      exact ⟨sourceAt, snapshotBeforeSource, ⟨tip⟩⟩
  | persisted production exactRound =>
      have snapshotBeforeFinish : snapshot ≤ production.finish :=
        Nat.le_trans production.startBeforePersistence
          (Nat.le_trans (Nat.le_succ production.persistedAt)
            production.persistenceBeforeFinish)
      have currentFloor :
          ((timed.execution.trace production.finish).validatorState
              author).highestSignedRound = round := by
        rw [production.signerFloorAtFinish, exactRound]
      have recoveryAtFinish :=
        active_recovery_snapshot_persists_without_commit_advance modeRules
          recovery authorInRange authorCorrectAvailable snapshotBeforeFinish
            active noAdvance
      exact ⟨production.finish, snapshotBeforeFinish,
        current_exact_round_gives_stable_recovery_tip_source pins
          authorInRange authorCorrectAvailable
            (active production.finish snapshotBeforeFinish) roundPositive
              currentFloor recoveryAtFinish⟩

/-- A concrete normal broadcast can be used as one exact recovery-round
source. This adapter keeps the actual persistence and addressed send evidence;
it does not assume a future layer. -/
theorem normal_broadcast_is_exact_recovery_round_source
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {recoveryWait start validator targetRound : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound) :
    Nonempty (ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait start validator targetRound) := by
  have startBeforePersistence : start ≤ production.persistedAt :=
    Nat.le_trans production.startBeforeProposalAction
      (Nat.le_trans (Nat.le_succ production.proposalActionAt)
        production.proposalBeforePersistence)
  have blockAboveStartFloor :
      ((timed.execution.trace start).validatorState
          validator).highestSignedRound <
        production.proposal.block.reference.round := by
    rw [production.proposalRound]
    exact production.targetAboveStartFloor
  let reflected := Classical.choice
    (persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable startBeforePersistence blockAboveStartFloor
          production.persistenceOccurs)
  refine ⟨.persisted reflected.1 ?_⟩
  rw [reflected.2.2]
  exact production.proposalRound

/-- Rebase one later normal broadcast to an earlier recovery observation. The
caller supplies only the local order and the fact that the exact target was
above the earlier signer floor. -/
theorem later_normal_broadcast_is_exact_recovery_round_source
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {recoveryWait observation start validator targetRound : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (observationBeforeStart : observation ≤ start)
    (targetAboveObservationFloor :
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound < targetRound)
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound) :
    Nonempty (ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait observation validator targetRound) := by
  have startBeforePersistence : start ≤ production.persistedAt :=
    Nat.le_trans production.startBeforeProposalAction
      (Nat.le_trans (Nat.le_succ production.proposalActionAt)
        production.proposalBeforePersistence)
  have observationBeforePersistence : observation ≤ production.persistedAt :=
    Nat.le_trans observationBeforeStart startBeforePersistence
  have blockAboveObservationFloor :
      ((timed.execution.trace observation).validatorState
          validator).highestSignedRound <
        production.proposal.block.reference.round := by
    rw [production.proposalRound]
    exact targetAboveObservationFloor
  let reflected := Classical.choice
    (persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable observationBeforePersistence
          blockAboveObservationFloor production.persistenceOccurs)
  refine ⟨.persisted reflected.1 ?_⟩
  rw [reflected.2.2]
  exact production.proposalRound

/-- In one post-GC recovery transition, the signer either reaches the local
safe-resume base or remains at or below GC. An intermediate signer floor cannot
appear because every persisted recovery proposal uses the single local
recovery target. -/
theorem post_gc_recovery_step_reaches_base_or_stays_at_gc
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    {snapshot validator : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (positiveGc :
      0 < ((timed.execution.trace snapshot).validatorState validator).gcRound)
    (floorAtOrBelowGc :
      ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound ≤
        ((timed.execution.trace snapshot).validatorState validator).gcRound)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator) ≤
        ((timed.execution.trace (snapshot + 1)).validatorState
          validator).highestSignedRound ∨
      ((timed.execution.trace (snapshot + 1)).validatorState
          validator).highestSignedRound ≤
        ((timed.execution.trace (snapshot + 1)).validatorState
          validator).gcRound := by
  let state := (timed.execution.trace snapshot).validatorState validator
  let nextState :=
    (timed.execution.trace (snapshot + 1)).validatorState validator
  have baseRound : ValidatorLocalRecoveryBaseRound state = state.gcRound + 2 := by
    simp [ValidatorLocalRecoveryBaseRound, state, floorAtOrBelowGc,
      Nat.ne_of_gt positiveGc]
  by_cases reached : ValidatorLocalRecoveryBaseRound state ≤
      nextState.highestSignedRound
  · exact Or.inl reached
  · right
    apply Nat.le_of_not_gt
    intro nextAboveGc
    have sameGc := no_commit_advance_keeps_correct_gc_round validatorInRange
      validatorCorrectAvailable (Nat.le_succ snapshot) noAdvance
    have beforeIntermediate : state.highestSignedRound < state.gcRound + 1 := by
      simpa [state] using Nat.lt_succ_of_le floorAtOrBelowGc
    have reachesIntermediate : state.gcRound + 1 ≤
        nextState.highestSignedRound := by
      change ((timed.execution.trace snapshot).validatorState
          validator).gcRound + 1 ≤
        ((timed.execution.trace (snapshot + 1)).validatorState
          validator).highestSignedRound
      rw [← sameGc]
      exact Nat.succ_le_iff.mpr nextAboveGc
    rcases recovery_world_step_target_crossing
        (timed.execution.stepsFollowRules snapshot) beforeIntermediate
          reachesIntermediate with
      ⟨block, persisted, _blockReachesIntermediate⟩
    have recoveryMode := recovery.recovering validator validatorInRange
      validatorCorrectAvailable
    have exactRound := roundRules.persistedProposalUsesRecoveryRound snapshot
      validator block validatorInRange validatorCorrectAvailable recoveryMode
        persisted
    have blockAtBase : block.reference.round =
        ValidatorLocalRecoveryBaseRound state := by
      rw [exactRound, baseRound]
      simp [ValidatorCommitProgressProposalRound, state, floorAtOrBelowGc,
        Nat.ne_of_gt positiveGc]
    have stored := persist_proposal_occurrence_stores_own_block
      timed.execution persisted
    have blockAtMostNextFloor : block.reference.round ≤
        nextState.highestSignedRound :=
      (timed.execution.statesWellFormed (snapshot + 1) validator
        validatorInRange).ownBlockDoesNotExceedSignerFloor
          block.reference.round block.reference stored
    exact reached (by simpa [blockAtBase] using blockAtMostNextFloor)

/-- One correct host produces an exact source at its proof-only local recovery
base from its current state.

The three cases are the current durable tip, paced genesis round one, and one
normal post-GC safe-resume proposal. An existing stale normal need is refreshed
by the one-host recovery-need rule. An idle requester starts the same canonical
need in the next batch. -/
theorem active_block_progress_recovery_gives_local_base_source
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (genesis : ValidatorCanonicalGenesisParentRules timed)
    (installed : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms
      modeRules.recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (needRules : ValidatorBlockProgressRecoveryNeedRules mode needs)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot validator : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (blockRecovery : ValidatorActiveBlockProgressRecoverySnapshot mode snapshot)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins modeRules.recoveryWait snapshot validator
        (ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator))) := by
  let state := (timed.execution.trace snapshot).validatorState validator
  by_cases floorAtOrBelowGc : state.highestSignedRound ≤ state.gcRound
  · by_cases gcZero : state.gcRound = 0
    · have floorZero : state.highestSignedRound = 0 := by omega
      have baseOne : ValidatorLocalRecoveryBaseRound state = 1 := by
        simp [ValidatorLocalRecoveryBaseRound, floorZero, gcZero]
      change Nonempty (ValidatorExactRecoveryRoundSource
        (obligations := obligations) pins modeRules.recoveryWait snapshot
          validator (ValidatorLocalRecoveryBaseRound state))
      rw [baseOne]
      exact canonical_genesis_gives_round_one_source_without_commit_advance
          (recoveryWait := modeRules.recoveryWait) timerSource pacing arms
            latchSource effects pins genesis authorityCountAtLeastTwo
              validatorInRange validatorCorrectAvailable (by
                simpa [state] using floorZero) active noAdvance
    · have positiveGc : 0 < state.gcRound := Nat.pos_of_ne_zero gcZero
      have floorAtOrBelowGcTrace :
          ((timed.execution.trace snapshot).validatorState
              validator).highestSignedRound ≤
            ((timed.execution.trace snapshot).validatorState
              validator).gcRound := by
        simpa [state] using floorAtOrBelowGc
      have positiveGcTrace : 0 <
          ((timed.execution.trace snapshot).validatorState
            validator).gcRound := by
        simpa [state] using positiveGc
      have baseRound : ValidatorLocalRecoveryBaseRound state =
          state.gcRound + 2 := by
        simp [ValidatorLocalRecoveryBaseRound, floorAtOrBelowGc, gcZero]
      cases needAtSnapshot : (needs.trace snapshot validator).active with
      | some need =>
          have mainFacts := needs.activeNeedMatchesMain snapshot validator need
            needAtSnapshot
          have normalOrigin : need.proposalOrigin = .normal := by
            cases origin : need.proposalOrigin with
            | normal => exact rfl
            | commitProgressRecovery =>
                have exactNext := need.recoveryTargetIsExactNext origin
                have targetFence := needs.activeNeedFencesTargetRound snapshot
                  validator need needAtSnapshot
                rcases targetFence with genesisTarget | aboveGcTarget
                · omega
                · omega
          have fresh := needRules.activeNormalNeedIsCurrentSafeResume snapshot
            validator need validatorInRange validatorCorrectAvailable
              (blockRecovery.recovering validator validatorInRange
                validatorCorrectAvailable) needAtSnapshot normalOrigin
                  (by simpa [state] using floorAtOrBelowGc)
          have needTargetBase : need.targetRound =
              ValidatorLocalRecoveryBaseRound state := by
            calc
              need.targetRound = Nat.max (need.signerFloor + 1)
                  (state.gcRound + 2) := by simpa [state] using fresh.2.2
              _ = Nat.max (state.highestSignedRound + 1)
                  (state.gcRound + 2) := by rw [fresh.1.2.2.2.1]
              _ = state.gcRound + 2 := Nat.max_eq_right (by omega)
              _ = ValidatorLocalRecoveryBaseRound state := baseRound.symm
          let production := Classical.choice
            (installed_head_bootstrap_fresh_need_eventually_produces_exact_broadcast
              installed needs representatives latchSource effects
                authorityCountAtLeastTwo validatorInRange
                  validatorCorrectAvailable needAtSnapshot fresh
                    (active snapshot (Nat.le_refl _)) rfl (by
                      simpa [state] using positiveGc) (by omega))
          simpa [needTargetBase] using
            (normal_broadcast_is_exact_recovery_round_source
              (pins := pins) (recoveryWait := modeRules.recoveryWait)
                latchSource effects authorityCountAtLeastTwo validatorInRange
                  validatorCorrectAvailable production)
      | none =>
          rcases post_gc_recovery_step_reaches_base_or_stays_at_gc modeRules
              roundRules recovery validatorInRange validatorCorrectAvailable
                (by simpa [state] using positiveGc)
                  (by simpa [state] using floorAtOrBelowGc) noAdvance with
            reachedBase | nextFloorAtOrBelowGc
          · exact active_recovery_crossing_common_base_gives_source modeRules
              roundRules latchSource effects pins authorityCountAtLeastTwo
                recovery validatorInRange validatorCorrectAvailable
                  (Nat.le_refl _) (by
                    change state.highestSignedRound <
                      ValidatorLocalRecoveryBaseRound state
                    rw [baseRound]
                    omega)
                    (Nat.le_succ snapshot) reachedBase active noAdvance
          · have noCommitInstall :=
              no_commit_advance_excludes_correct_commit_install
                validatorInRange validatorCorrectAvailable (Nat.le_refl _)
                  noAdvance
            have recoveryAtNext :=
              active_recovery_snapshot_persists_without_commit_advance
                modeRules recovery validatorInRange validatorCorrectAvailable
                  (Nat.le_succ snapshot) active noAdvance
            have sameGc := no_commit_advance_keeps_correct_gc_round
              validatorInRange validatorCorrectAvailable (Nat.le_succ snapshot)
                noAdvance
            have positiveGcAtNext : 0 <
                ((timed.execution.trace (snapshot + 1)).validatorState
                  validator).gcRound := by
              rw [sameGc]
              simpa [state] using positiveGc
            let production := Classical.choice
              (installed_head_bootstrap_idle_recovery_root_eventually_produces_canonical_broadcast
                installed needs representatives latchSource effects
                  authorityCountAtLeastTwo validatorInRange
                    validatorCorrectAvailable noCommitInstall needAtSnapshot
                      recoveryAtNext positiveGcAtNext nextFloorAtOrBelowGc
                        (active (snapshot + 1) (Nat.le_succ snapshot)) rfl)
            have targetIsBase :
                Nat.max
                    (((timed.execution.trace (snapshot + 1)).validatorState
                      validator).highestSignedRound + 1)
                    (((timed.execution.trace (snapshot + 1)).validatorState
                      validator).gcRound + 2) =
                  ValidatorLocalRecoveryBaseRound state := by
              calc
                Nat.max
                    (((timed.execution.trace (snapshot + 1)).validatorState
                      validator).highestSignedRound + 1)
                    (((timed.execution.trace (snapshot + 1)).validatorState
                      validator).gcRound + 2) =
                    ((timed.execution.trace (snapshot + 1)).validatorState
                      validator).gcRound + 2 := Nat.max_eq_right (by omega)
                _ = state.gcRound + 2 := by
                  change
                    ((timed.execution.trace (snapshot + 1)).validatorState
                        validator).gcRound + 2 =
                      ((timed.execution.trace snapshot).validatorState
                        validator).gcRound + 2
                  exact congrArg (fun round => round + 2) sameGc
                _ = ValidatorLocalRecoveryBaseRound state := baseRound.symm
            have baseAboveStartFloor : state.highestSignedRound <
                ValidatorLocalRecoveryBaseRound state := by
              rw [baseRound]
              omega
            simpa [targetIsBase] using
              (later_normal_broadcast_is_exact_recovery_round_source
                (pins := pins) (recoveryWait := modeRules.recoveryWait)
                  latchSource effects authorityCountAtLeastTwo validatorInRange
                    validatorCorrectAvailable (Nat.le_succ snapshot)
                      (by simpa [targetIsBase, state] using baseAboveStartFloor)
                        production)
  · have gcBelowFloor : state.gcRound < state.highestSignedRound :=
      Nat.lt_of_not_ge floorAtOrBelowGc
    have baseAtFloor : ValidatorLocalRecoveryBaseRound state =
        state.highestSignedRound := by
      simp [ValidatorLocalRecoveryBaseRound, floorAtOrBelowGc]
    let tip := Classical.choice
      (current_exact_round_gives_stable_recovery_tip_source pins
        (round := state.highestSignedRound)
        validatorInRange validatorCorrectAvailable
          (active snapshot (Nat.le_refl _))
            (Nat.lt_of_le_of_lt (Nat.zero_le state.gcRound) gcBelowFloor) (by
            rfl)
              (recovery.recovering validator validatorInRange
                validatorCorrectAvailable))
    exact ⟨.durableTip snapshot (Nat.le_refl _) (by
      simpa [baseAtFloor, state] using tip)⟩

/-- Every correct host has one exact source at its own proof-only local
recovery base. The rounds can differ before the common-base catch-up. -/
def EveryCorrectAvailableValidatorLocalRecoveryBaseSource
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recoveryWait snapshot : Time) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    Nonempty (ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait snapshot validator
        (ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator)))

/-- The one-host local-base theorem aggregates pointwise over the fixed correct,
available validator set. It still does not assume or return a common layer. -/
theorem active_block_progress_recovery_gives_local_base_source_family
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (genesis : ValidatorCanonicalGenesisParentRules timed)
    (installed : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms
      modeRules.recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (needRules : ValidatorBlockProgressRecoveryNeedRules mode needs)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (blockRecovery : ValidatorActiveBlockProgressRecoverySnapshot mode snapshot)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    EveryCorrectAvailableValidatorLocalRecoveryBaseSource
      (obligations := obligations) pins modeRules.recoveryWait snapshot := by
  intro validator validatorInRange validatorCorrectAvailable
  exact active_block_progress_recovery_gives_local_base_source modeRules
    roundRules timerSource pacing arms latchSource effects pins genesis
      installed needs representatives needRules authorityCountAtLeastTwo
        recovery blockRecovery validatorInRange validatorCorrectAvailable
          active noAdvance

/-- Each correct host below the proof-only common base eventually completes
one local resume step. This is an internal aggregation predicate. The final
network theorem must derive it from genesis storage, installed-head storage,
and the one-host recovery scheduling rules. -/
def EveryCorrectAvailableValidatorRecoveryResume
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (snapshot baseRound : Time) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((timed.execution.trace snapshot).validatorState
      validator).highestSignedRound < baseRound →
    Nonempty (ValidatorRecoveryResumePoint timed snapshot validator)

/-- Exact local-base sources derive all local resume points. The proof uses
only the local-base GC inequality and durable source ownership. -/
theorem local_base_source_family_gives_recovery_resumes
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait snapshot baseRound : Time}
    (sources : EveryCorrectAvailableValidatorLocalRecoveryBaseSource
      (obligations := obligations) pins recoveryWait snapshot)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    EveryCorrectAvailableValidatorRecoveryResume timed snapshot baseRound := by
  intro validator validatorInRange validatorCorrectAvailable _floorBelowBase
  let source := Classical.choice
    (sources validator validatorInRange validatorCorrectAvailable)
  exact exact_recovery_round_source_gives_recovery_resume_point
    validatorInRange validatorCorrectAvailable source
      (gc_round_lt_local_recovery_base
        ((timed.execution.trace snapshot).validatorState validator)) noAdvance

/-- One caller-selected common tip and the derived local resume points give an
exact restart-safe source at that round for every correct author.

Unlike the recovery-base-maximum specialization below, this theorem does not
choose the common round. A higher proof can select a finite maximum from actual
local broadcasts, prove that every local recovery base is below it, and supply
one exact owner tip at the selected round. The stable-suffix premise remains
explicit: this lemma does not treat a later GC-moving install as progress. -/
theorem stable_arbitrary_common_tip_and_local_resumes_give_source_family
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins
      modeRules.recoveryWait)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot sourceAt holder baseRound : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (source : ValidatorStableRecoveryTipSource pins modeRules.recoveryWait
      sourceAt holder baseRound)
    (snapshotBeforeSource : snapshot ≤ sourceAt)
    (baseRoundPositive : 0 < baseRound)
    (localBasesAtMostCommon : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator) ≤
        baseRound)
    (resumes : EveryCorrectAvailableValidatorRecoveryResume timed snapshot
      baseRound)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    EveryCorrectAvailableValidatorExactRecoveryRoundSource
      (obligations := obligations) pins modeRules.recoveryWait snapshot
        baseRound := by
  intro validator validatorInRange validatorCorrectAvailable
  have localBaseAtMostCommon := localBasesAtMostCommon validator validatorInRange
    validatorCorrectAvailable
  have floorAtMostBase :
      ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound ≤ baseRound :=
    Nat.le_trans
      (signer_floor_le_local_recovery_base
        ((timed.execution.trace snapshot).validatorState validator))
      localBaseAtMostCommon
  by_cases floorAtBase :
      ((timed.execution.trace snapshot).validatorState
        validator).highestSignedRound = baseRound
  · let tip := Classical.choice
      (current_exact_round_gives_stable_recovery_tip_source pins
        validatorInRange validatorCorrectAvailable
          (active snapshot (Nat.le_refl _)) baseRoundPositive floorAtBase
            (recovery.recovering validator validatorInRange
              validatorCorrectAvailable))
    exact ⟨.durableTip snapshot (Nat.le_refl _) tip⟩
  · have floorBelowBase :
        ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound < baseRound :=
      Nat.lt_of_le_of_ne floorAtMostBase floorAtBase
    let resume := Classical.choice
      (resumes validator validatorInRange validatorCorrectAvailable
        floorBelowBase)
    exact stable_recovery_tip_source_catches_resumed_validator_to_common_base
      modeRules roundRules timerSource pacing arms latchSource effects pins
        broadcast capsuleSync acceptance authorityCountAtLeastTwo source
          snapshotBeforeSource recovery holderInRange holderCorrectAvailable
            validatorInRange validatorCorrectAvailable localBaseAtMostCommon
              floorBelowBase resume.snapshotBeforeResume resume.gcBelowFloor
                active noAdvance

/-- A proof-selected common-base tip and the derived local resume points give
one exact restart-safe source at that base for every correct author.

The common base and its owner are proof values. A validator that already has a
tip at the base uses its own source pin. Every lower validator installs the
owner's causal history and reaches the base by exact-next actions. -/
theorem stable_common_base_tip_and_local_resumes_give_source_family
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins
      modeRules.recoveryWait)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot sourceAt holder : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (source : ValidatorStableRecoveryTipSource pins modeRules.recoveryWait
      sourceAt holder
        (correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount))
    (snapshotBeforeSource : snapshot ≤ sourceAt)
    (resumes : EveryCorrectAvailableValidatorRecoveryResume timed snapshot
      (correctValidatorRecoveryBaseMaximumUpTo faults
        (timed.execution.trace snapshot) config.authorityCount))
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    EveryCorrectAvailableValidatorExactRecoveryRoundSource
      (obligations := obligations) pins modeRules.recoveryWait snapshot
        (correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount) := by
  let baseRound := correctValidatorRecoveryBaseMaximumUpTo faults
    (timed.execution.trace snapshot) config.authorityCount
  have basePositive : 0 < baseRound :=
    correct_recovery_base_maximum_positive
      (faults := faults) (world := timed.execution.trace snapshot)
  intro validator validatorInRange validatorCorrectAvailable
  have localBaseAtMostCommon :
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator) ≤
        baseRound :=
    correct_validator_recovery_base_le_maximum
      (world := timed.execution.trace snapshot) validatorInRange
        validatorCorrectAvailable
  have floorAtMostBase :
      ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound ≤ baseRound :=
    Nat.le_trans
      (signer_floor_le_local_recovery_base
        ((timed.execution.trace snapshot).validatorState validator))
      localBaseAtMostCommon
  by_cases floorAtBase :
      ((timed.execution.trace snapshot).validatorState
        validator).highestSignedRound = baseRound
  · let tip := Classical.choice
      (current_exact_round_gives_stable_recovery_tip_source pins
        validatorInRange validatorCorrectAvailable
          (active snapshot (Nat.le_refl _)) basePositive floorAtBase
            (recovery.recovering validator validatorInRange
              validatorCorrectAvailable))
    exact ⟨.durableTip snapshot (Nat.le_refl _) tip⟩
  · have floorBelowBase :
        ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound < baseRound :=
      Nat.lt_of_le_of_ne floorAtMostBase floorAtBase
    let resume := Classical.choice
      (resumes validator validatorInRange validatorCorrectAvailable
        floorBelowBase)
    exact stable_recovery_tip_source_catches_resumed_validator_to_common_base
      modeRules roundRules timerSource pacing arms latchSource effects pins
        broadcast capsuleSync acceptance authorityCountAtLeastTwo source
          snapshotBeforeSource recovery holderInRange holderCorrectAvailable
            validatorInRange validatorCorrectAvailable localBaseAtMostCommon
              floorBelowBase resume.snapshotBeforeResume resume.gcBelowFloor
                active noAdvance

/-- Exact local-base sources select their common-base owner inside the proof,
normalize that owner's source to a durable current tip, and catch every lower
correct host to the same exact round. -/
theorem local_base_source_family_gives_common_base_source_family
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins
      modeRules.recoveryWait)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (localSources : EveryCorrectAvailableValidatorLocalRecoveryBaseSource
      (obligations := obligations) pins modeRules.recoveryWait snapshot)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    EveryCorrectAvailableValidatorExactRecoveryRoundSource
      (obligations := obligations) pins modeRules.recoveryWait snapshot
        (correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount) := by
  let baseRound := correctValidatorRecoveryBaseMaximumUpTo faults
    (timed.execution.trace snapshot) config.authorityCount
  have basePositive : 0 < baseRound :=
    correct_recovery_base_maximum_positive
      (faults := faults) (world := timed.execution.trace snapshot)
  rcases positive_correct_recovery_base_maximum_has_owner
      (faults := faults) (world := timed.execution.trace snapshot)
        basePositive with
    ⟨owner, ownerInRange, ownerCorrectAvailable, ownerBase⟩
  let ownerLocalSource := Classical.choice
    (localSources owner ownerInRange ownerCorrectAvailable)
  have ownerBaseSource : ValidatorExactRecoveryRoundSource
      (obligations := obligations) pins modeRules.recoveryWait snapshot owner
        baseRound := by
    simpa [baseRound, ownerBase] using ownerLocalSource
  rcases exact_recovery_round_source_gives_stable_tip_source modeRules pins
      recovery ownerInRange ownerCorrectAvailable ownerBaseSource basePositive
        active noAdvance with
    ⟨sourceAt, snapshotBeforeSource, stableSource⟩
  let source := Classical.choice stableSource
  have resumes : EveryCorrectAvailableValidatorRecoveryResume timed snapshot
      baseRound :=
    local_base_source_family_gives_recovery_resumes localSources noAdvance
  simpa [baseRound] using
    (stable_common_base_tip_and_local_resumes_give_source_family modeRules
      roundRules timerSource pacing arms latchSource effects pins broadcast
        capsuleSync acceptance authorityCountAtLeastTwo recovery ownerInRange
          ownerCorrectAvailable source snapshotBeforeSource resumes active
            noAdvance)

/-- Crossing the round after the proof-only common base cannot skip that
round. Genesis and post-GC safe-resume targets are at or below the common base;
all later recovery proposals are exact-next. -/
theorem active_recovery_crossing_above_common_base_is_exact
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    {snapshot finish validator baseRound : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (localBaseAtMostCommon :
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator) ≤
        baseRound)
    (snapshotBeforeFinish : snapshot ≤ finish)
    (targetReached : baseRound + 1 ≤
      ((timed.execution.trace finish).validatorState
        validator).highestSignedRound)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    ∃ persistTime block,
      snapshot ≤ persistTime ∧
        persistTime < finish ∧
        ValidatorLocalActionOccurs (timed.execution.events persistTime)
          validator (.persistProposal block) ∧
        block.reference.round = baseRound + 1 := by
  have startFloorAtMostBase :
      ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound ≤ baseRound :=
    Nat.le_trans
      (signer_floor_le_local_recovery_base
        ((timed.execution.trace snapshot).validatorState validator))
      localBaseAtMostCommon
  have targetAboveStart :
      ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound < baseRound + 1 := by
    omega
  rcases signer_floor_first_target_crossing_has_persist_proposal
      timed.execution snapshotBeforeFinish targetAboveStart targetReached with
    ⟨persistTime, block, snapshotBeforePersist, persistBeforeFinish,
      floorBeforeTarget, persisted, blockReaches⟩
  have modeAtPersist := active_recovery_snapshot_persists_without_commit_advance
    modeRules recovery validatorInRange validatorCorrectAvailable
      snapshotBeforePersist active noAdvance
  have selectedRound := roundRules.persistedProposalUsesRecoveryRound
    persistTime validator block validatorInRange validatorCorrectAvailable
      modeAtPersist persisted
  have gcAtPersist := no_commit_advance_keeps_correct_gc_round
    validatorInRange validatorCorrectAvailable snapshotBeforePersist noAdvance
  have floorMonotone :=
    (timed.execution.durableStateMonotone validator snapshot persistTime
      validatorInRange snapshotBeforePersist).2.2.2.2.2.2.1
  have selectedAtMostTarget :
      ValidatorCommitProgressProposalRound
          ((timed.execution.trace persistTime).validatorState validator) ≤
        baseRound + 1 := by
    unfold ValidatorCommitProgressProposalRound
    split
    next floorAtOrBelowGc =>
      split
      next gcZero => omega
      next gcNonzero =>
        have startFloorAtOrBelowGc :
            ((timed.execution.trace snapshot).validatorState
                validator).highestSignedRound ≤
              ((timed.execution.trace snapshot).validatorState
                validator).gcRound := by
          rw [← gcAtPersist]
          exact Nat.le_trans floorMonotone floorAtOrBelowGc
        have startGcNonzero :
            ((timed.execution.trace snapshot).validatorState
                validator).gcRound ≠ 0 := by
          rw [gcAtPersist] at gcNonzero
          exact gcNonzero
        have gcBootstrapAtMostBase :
            ((timed.execution.trace snapshot).validatorState
                validator).gcRound + 2 ≤ baseRound := by
          simpa [ValidatorLocalRecoveryBaseRound, startFloorAtOrBelowGc,
            startGcNonzero] using localBaseAtMostCommon
        rw [gcAtPersist]
        omega
    next floorAboveGc =>
      omega
  have blockAtMostTarget : block.reference.round ≤ baseRound + 1 := by
    rw [selectedRound]
    exact selectedAtMostTarget
  exact ⟨persistTime, block, snapshotBeforePersist, persistBeforeFinish,
    persisted, Nat.le_antisymm blockAtMostTarget blockReaches⟩

/-- One exact proposal at a selected round keeps its durable block and every
addressed peer broadcast. The start time is before its persistence. -/
structure ValidatorExactRoundBroadcastProduction
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (start validator round : Nat) where
  production : ValidatorPersistedProposalBroadcastProduction timed obligations
    start validator
  exactRound : production.proposal.block.reference.round = round

/-- An exact broadcast strictly beyond the initial recovery base comes from a
fresh timer-paced recovery proposal.

Offset zero is intentionally excluded. That first round can be a warm-up whose
timer started before `observation`. A positive offset places the timer target
strictly above the observed signer floor, so the exact timer start and refreshed
proposal snapshot are both after `observation`. -/
theorem late_exact_round_broadcast_gives_fresh_timer_paced_production
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (originRules : ValidatorBlockProgressProposalOriginRules
      (obligations := obligations) mode)
    (timingRules : ValidatorRecoveryProposalActionTimingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {observation validator baseRound offset : Time}
    (exact : ValidatorExactRoundBroadcastProduction timed obligations
      observation validator (baseRound + offset + 1))
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (timeGapAtObservation : ValidatorCommitProgressRecoveryModeAt timed
      modeRules.recoveryWait observation validator)
    (blockRecoveryAtObservation :
      ValidatorBlockProgressRecoveryModeAt mode observation validator)
    (localBaseAtMostBase :
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace observation).validatorState validator) ≤
        baseRound)
    (offsetPositive : 0 < offset)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ production : ValidatorTimerPacedRoundProduction timed waits validator
        (baseRound + offset + 1),
      production.snapshot.block = exact.production.proposal.block ∧
        production.persistTime = exact.production.persistedAt ∧
        observation < production.timerStartedAt ∧
        observation < production.snapshot.snapshotAt := by
  have observationBeforePersist : observation ≤ exact.production.persistedAt :=
    exact.production.startBeforePersistence
  have blockRecoveryAtPersist :=
    block_progress_recovery_mode_persists_without_commit_advance modeRules mode
      sameRecoveryWait validatorInRange validatorCorrectAvailable
        observationBeforePersist timeGapAtObservation blockRecoveryAtObservation
          active noAdvance
  have sameHead := no_commit_advance_keeps_correct_commit_head
    validatorInRange validatorCorrectAvailable observationBeforePersist noAdvance
  have sameLastCommit := modeRules.sameCommitIndexKeepsLastCommitTime validator
    observation exact.production.persistedAt validatorInRange
      observationBeforePersist (congrArg ValidatorCommitHead.index sameHead.symm)
  have timeGapAtPersist : ValidatorCommitProgressRecoveryModeAt timed
      modeRules.recoveryWait exact.production.persistedAt validator :=
    recovery_mode_persists_with_stable_last_commit validatorInRange
      observationBeforePersist timeGapAtObservation
        (active exact.production.persistedAt observationBeforePersist)
          sameLastCommit.symm
  have selectedRound := roundRules.persistedProposalUsesRecoveryRound
    exact.production.persistedAt validator exact.production.proposal.block
      validatorInRange validatorCorrectAvailable timeGapAtPersist
        exact.production.persistenceOccurs
  have sameGc := no_commit_advance_keeps_correct_gc_round validatorInRange
    validatorCorrectAvailable observationBeforePersist noAdvance
  have floorMonotone :=
    (timed.execution.durableStateMonotone validator observation
      exact.production.persistedAt validatorInRange observationBeforePersist
      ).2.2.2.2.2.2.1
  have floorAboveGc :
      ((timed.execution.trace exact.production.persistedAt).validatorState
          validator).gcRound <
        ((timed.execution.trace exact.production.persistedAt).validatorState
          validator).highestSignedRound := by
    apply Nat.lt_of_not_ge
    intro floorAtOrBelowGc
    have startFloorAtOrBelowGc :
        ((timed.execution.trace observation).validatorState
            validator).highestSignedRound ≤
          ((timed.execution.trace observation).validatorState
            validator).gcRound := by
      rw [← sameGc]
      exact Nat.le_trans floorMonotone floorAtOrBelowGc
    by_cases gcZero :
        ((timed.execution.trace exact.production.persistedAt).validatorState
          validator).gcRound = 0
    · have initialBasePositive : 0 < ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace observation).validatorState validator) := by
        rcases local_recovery_base_cases
            ((timed.execution.trace observation).validatorState validator) with
          genesis | durableTip | postGc <;> omega
      have floorAtPersistZero :
          ((timed.execution.trace exact.production.persistedAt).validatorState
            validator).highestSignedRound = 0 := by
        omega
      have blockIsOne :
          exact.production.proposal.block.reference.round = 1 := by
        rw [selectedRound]
        simp [ValidatorCommitProgressProposalRound, floorAtPersistZero, gcZero]
      rw [exact.exactRound] at blockIsOne
      omega
    · have startGcNonzero :
          ((timed.execution.trace observation).validatorState
            validator).gcRound ≠ 0 := by
        intro startGcZero
        exact gcZero (sameGc.trans startGcZero)
      have gcBootstrapAtMostBase :
          ((timed.execution.trace observation).validatorState
              validator).gcRound + 2 ≤ baseRound := by
        simpa [ValidatorLocalRecoveryBaseRound, startFloorAtOrBelowGc,
          startGcNonzero] using localBaseAtMostBase
      have blockAtGcBootstrap :
          exact.production.proposal.block.reference.round =
            ((timed.execution.trace observation).validatorState
              validator).gcRound + 2 := by
        have floorAtOrBelowStartGc :
            ((timed.execution.trace exact.production.persistedAt).validatorState
                validator).highestSignedRound ≤
              ((timed.execution.trace observation).validatorState
                validator).gcRound := by
          rw [← sameGc]
          exact floorAtOrBelowGc
        rw [selectedRound]
        simp [ValidatorCommitProgressProposalRound, sameGc,
          floorAtOrBelowStartGc, startGcNonzero]
      rw [exact.exactRound] at blockAtGcBootstrap
      omega
  have startFloorAtMostBase :
      ((timed.execution.trace observation).validatorState
          validator).highestSignedRound ≤ baseRound :=
    Nat.le_trans
      (signer_floor_le_local_recovery_base
        ((timed.execution.trace observation).validatorState validator))
      localBaseAtMostBase
  have targetAboveObservedFloor :
      ((timed.execution.trace observation).validatorState
          validator).highestSignedRound < baseRound + offset := by
    exact Nat.lt_of_le_of_lt startFloorAtMostBase
      (Nat.lt_add_of_pos_right offsetPositive)
  have targetBlockRound : exact.production.proposal.block.reference.round =
      (baseRound + offset) + 1 := by
    simpa [Nat.add_assoc] using exact.exactRound
  exact block_progress_fresh_target_persistence_gives_timer_paced_production
    originRules timingRules latchSource pacing effects authorityCountAtLeastTwo
      validatorInRange validatorCorrectAvailable blockRecoveryAtPersist
        floorAboveGc targetBlockRound targetAboveObservedFloor active
          exact.production.persistenceOccurs

/-- Every correct, available author has one concrete proposal and addressed
peer broadcast at the same exact round. -/
def EveryCorrectAvailableValidatorExactRoundBroadcast
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (start round : Nat) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    Nonempty (ValidatorExactRoundBroadcastProduction timed obligations start
      validator round)

/-- One author's exact broadcast and the exact timer-paced snapshot which
created the same block. -/
structure ValidatorFreshTimerPacedExactRoundProduction
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (observation validator round : Time) where
  broadcast : ValidatorExactRoundBroadcastProduction timed obligations
    observation validator round
  production : ValidatorTimerPacedRoundProduction timed waits validator round
  exactBlock : production.snapshot.block = broadcast.production.proposal.block
  exactPersistence : production.persistTime = broadcast.production.persistedAt
  timerAfterObservation : observation < production.timerStartedAt
  snapshotAfterObservation : observation < production.snapshot.snapshotAt

/-- Every correct, available author has one exact late-round broadcast and its
fresh timer-paced proposal snapshot. The concrete block identity is retained. -/
def EveryCorrectAvailableValidatorFreshTimerPacedExactRound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (observation round : Time) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    Nonempty (ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation validator round)

/-- Lift a common exact-broadcast family at one positive offset into the rich
fresh timer-paced family without changing any block reference. -/
theorem late_exact_broadcast_family_gives_fresh_timer_paced_family
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (originRules : ValidatorBlockProgressProposalOriginRules
      (obligations := obligations) mode)
    (timingRules : ValidatorRecoveryProposalActionTimingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {observation baseRound offset : Time}
    (family : EveryCorrectAvailableValidatorExactRoundBroadcast timed
      obligations observation (baseRound + offset + 1))
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      observation)
    (blockRecovery : ValidatorActiveBlockProgressRecoverySnapshot mode
      observation)
    (commonBaseAtMostBase :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace observation) config.authorityCount ≤ baseRound)
    (offsetPositive : 0 < offset)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed obligations
      waits observation (baseRound + offset + 1) := by
  intro validator validatorInRange validatorCorrectAvailable
  let exact := Classical.choice
    (family validator validatorInRange validatorCorrectAvailable)
  have localBaseAtMostBase :
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace observation).validatorState validator) ≤
        baseRound :=
    Nat.le_trans
      (correct_validator_recovery_base_le_maximum validatorInRange
        validatorCorrectAvailable)
      commonBaseAtMostBase
  rcases late_exact_round_broadcast_gives_fresh_timer_paced_production
      modeRules sameRecoveryWait roundRules originRules timingRules latchSource
        pacing effects authorityCountAtLeastTwo exact validatorInRange
          validatorCorrectAvailable
            (recovery.recovering validator validatorInRange
              validatorCorrectAvailable)
            (blockRecovery.recovering validator validatorInRange
              validatorCorrectAvailable)
            localBaseAtMostBase offsetPositive active noAdvance with
    ⟨production, exactBlock, exactPersistence, timerAfterObservation,
      snapshotAfterObservation⟩
  exact ⟨{
    broadcast := exact
    production
    exactBlock
    exactPersistence
    timerAfterObservation
    snapshotAfterObservation }⟩

/-- One addressed proposal packet from an exact persisted proposal is
delivered to a correct receiver after GST. -/
private theorem persisted_proposal_peer_broadcast_is_delivered
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
    {start author receiver : Time}
    (production : ValidatorPersistedProposalBroadcastProduction timed
      obligations start author)
    (broadcast : ValidatorLatchedProposalBroadcast timed obligations
      production.readyAt author receiver production.proposal)
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt) :
    broadcast.packet.sentAt ≤ broadcast.packet.deliveredAt ∧
      broadcast.packet.deliveredAt ≤
        broadcast.packet.sentAt + network.delta ∧
      ValidatorPacketDeliveryOccurs
        (timed.execution.events broadcast.packet.deliveredAt)
        broadcast.packetId := by
  have senderInRange : broadcast.packet.sender < config.authorityCount := by
    rw [broadcast.packetSender]
    exact authorInRange
  have senderCorrect :
      faults.correctAvailable broadcast.packet.sender = true := by
    rw [broadcast.packetSender]
    exact authorCorrectAvailable
  have packetReceiverInRange :
      broadcast.packet.receiver < config.authorityCount := by
    rw [broadcast.packetReceiver]
    exact receiverInRange
  have packetReceiverCorrect :
      faults.correctAvailable broadcast.packet.receiver = true := by
    rw [broadcast.packetReceiver]
    exact receiverCorrectAvailable
  have bounds := network.postGstDelivery broadcast.packet
    broadcast.packetIsProtocol senderInRange packetReceiverInRange
      senderCorrect packetReceiverCorrect sentAfterGst
  have packetAtSent :
      (timed.execution.trace broadcast.packet.sentAt).packets
          broadcast.packetId = some broadcast.packet := by
    rw [broadcast.packetSentAt]
    exact broadcast.packetInTrace
  have delivered := timed.execution.protocolPacketsAreDelivered
    broadcast.packetId broadcast.packet packetAtSent broadcast.packetIsProtocol
      senderInRange packetReceiverInRange senderCorrect packetReceiverCorrect
        sentAfterGst
  exact ⟨bounds.1, bounds.2, delivered⟩

/-- One exact broadcast either advances the queried receiver's commit index or
becomes its stable accepted and retained representative.

The alternative is local to `receiver`. A commit advance at a different host
does not discharge this result. -/
theorem exact_round_broadcast_reaches_receiver_or_receiver_advances
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation author receiver round : Time}
    (exact : ValidatorExactRoundBroadcastProduction timed obligations
      observation author round)
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (roundAboveReceiverGc :
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorReceiverCommitAdvance timed observation receiver ∨
      ∃ readyAt,
        observation ≤ readyAt ∧
          ∀ later, readyAt ≤ later →
            ((timed.execution.trace later).validatorState receiver
                ).acceptedRepresentative round author =
                  some exact.production.proposal.block.reference ∧
              ((timed.execution.trace later).validatorState receiver).retained
                  exact.production.proposal.block.reference = true := by
  classical
  by_cases receiverAdvanced :
      ValidatorReceiverCommitAdvance timed observation receiver
  · exact Or.inl receiverAdvanced
  · right
    have authorNotByzantine : faults.byzantine author = false := by
      have notNonProgress : faults.nonProgress author = false := by
        simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
          VoterSet.full] using authorCorrectAvailable
      have separated : faults.byzantine author = false ∧
          faults.unavailable author = false := by
        simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
          notNonProgress
      exact separated.1
    have sourceActive :
        (timed.execution.trace
          (exact.production.persistedAt + 1)).epochActive = true :=
      active (exact.production.persistedAt + 1)
        (Nat.le_trans exact.production.startBeforePersistence (Nat.le_succ _))
    rcases pins.persisted_proposal_has_pinned_capsule_source authorInRange
        authorCorrectAvailable sourceActive exact.production.persistenceOccurs
      with ⟨capsuleKey, entry, targetBlock, stored, pinned, _source⟩
    by_cases receiverIsAuthor : receiver = author
    · subst receiver
      have ownAtFinish :
          ((timed.execution.trace exact.production.finish).validatorState
              author).ownBlockAt round =
            some exact.production.proposal.block.reference := by
        simpa [exact.exactRound] using
          exact.production.ownBlockStoredAtFinish
      have ownFacts :=
        (timed.execution.statesWellFormed exact.production.finish author
          authorInRange).ownBlockIsSound round
            exact.production.proposal.block.reference ownAtFinish
      have recorded := representatives.acceptedCorrectReferenceIsRecorded
        exact.production.finish author
        exact.production.proposal.block.reference authorInRange
        authorCorrectAvailable
        (by simpa [exact.production.proposalAuthor] using authorInRange)
        (by simpa [exact.production.proposalAuthor] using authorNotByzantine)
        ownFacts.2.2.1
      let readyAt := max exact.production.finish
        (exact.production.persistedAt + 1)
      have finishBeforeReady : exact.production.finish ≤ readyAt :=
        Nat.le_max_left _ _
      have sourceBeforeReady : exact.production.persistedAt + 1 ≤ readyAt :=
        Nat.le_max_right _ _
      refine ⟨readyAt,
        Nat.le_trans exact.production.startBeforePersistence
          (Nat.le_trans (Nat.le_succ _)
            (Nat.le_trans exact.production.persistenceBeforeFinish
              finishBeforeReady)), ?_⟩
      intro later readyBeforeLater
      have recordedLater :=
        accepted_representative_persists_in_validator_execution
          timed.execution authorInRange
            (Nat.le_trans finishBeforeReady readyBeforeLater) (by
              simpa [exact.exactRound, exact.production.proposalAuthor] using
                recorded)
      have sourceCurrent := pins.pin_persists_while_epoch_active
        (Nat.le_trans sourceBeforeReady readyBeforeLater) stored pinned (by
          intro time sourceBeforeTime _timeBeforeLater
          exact active time (Nat.le_trans
            (Nat.le_trans exact.production.startBeforePersistence
              (Nat.le_succ _)) sourceBeforeTime))
      have targetMember : entry.capsule.targetBlock ∈ entry.capsule.history :=
        entry.capsule.target_and_parents_in_history.1
      have localTarget := pins.pinnedHistoryIsLocal later author capsuleKey
        entry sourceCurrent.1 sourceCurrent.2 entry.capsule.targetBlock
          targetMember
      refine ⟨recordedLater, ?_⟩
      simpa [targetBlock] using localTarget.2.1
    · let peer := Classical.choice
          (exact.production.broadcasts receiver receiverInRange
            receiverIsAuthor)
      have samePersistence :
          exact.production.persistedAt = peer.persistedAt :=
        matching_persist_proposal_times_are_equal authorInRange
          exact.production.persistenceOccurs peer.persistenceOccurs
      have persistenceBeforeSend : exact.production.persistedAt + 1 ≤
          peer.packet.sentAt := by
        rw [samePersistence, peer.packetSentAt]
        exact Nat.le_trans peer.persistenceBeforeSend (Nat.le_succ _)
      have sentAfterGst : network.gst ≤ peer.packet.sentAt :=
        Nat.le_trans afterGst
          (Nat.le_trans exact.production.startBeforePersistence
            (Nat.le_trans (Nat.le_succ _) persistenceBeforeSend))
      have delivered := persisted_proposal_peer_broadcast_is_delivered
        exact.production peer authorInRange authorCorrectAvailable
          receiverInRange receiverCorrectAvailable sentAfterGst
      have packetAtSent :
          (timed.execution.trace peer.packet.sentAt).packets peer.packetId =
            some peer.packet := by
        rw [peer.packetSentAt]
        exact peer.packetInTrace
      have packetAtDelivery := timed.execution.packetHistoryMonotone
        peer.packet.sentAt peer.packet.deliveredAt delivered.1 peer.packetId
          peer.packet packetAtSent
      have proposalBody : ValidatorLocalBlockBodyAt timed
          peer.packet.deliveredAt receiver exact.production.proposal.block := by
        exact .delivered peer.packetId peer.packet packetAtDelivery
          peer.packetIsProtocol peer.packetReceiver peer.packetPayload
          delivered.2.2
      have targetBody : ValidatorLocalBlockBodyAt timed peer.packet.deliveredAt
          receiver entry.capsule.targetBlock := by
        simpa [targetBlock] using proposalBody
      have sourceBeforeDelivery : exact.production.persistedAt + 1 ≤
          peer.packet.deliveredAt :=
        Nat.le_trans persistenceBeforeSend delivered.1
      have observationBeforeSource : observation ≤
          exact.production.persistedAt + 1 :=
        Nat.le_trans exact.production.startBeforePersistence (Nat.le_succ _)
      have activeFromSource : ∀ time,
          exact.production.persistedAt + 1 ≤ time →
          (timed.execution.trace time).epochActive = true := by
        intro time sourceBeforeTime
        exact active time
          (Nat.le_trans observationBeforeSource sourceBeforeTime)
      rcases
          ValidatorRecoveryCapsuleSyncExecution.delivered_target_installs_retained_above_gc_history_or_commit_advance
            pins capsuleSync acceptance authorInRange authorCorrectAvailable
              receiverInRange receiverCorrectAvailable
                (Nat.le_trans afterGst observationBeforeSource)
                activeFromSource stored pinned sourceBeforeDelivery targetBody
        with ⟨recordedAt, sourceBeforeRecorded,
          headAdvanced | installedHistory⟩
      · have headAtObservationAtMostSource :=
          (timed.execution.durableStateMonotone receiver observation
            (exact.production.persistedAt + 1) receiverInRange
              observationBeforeSource).1
        exact False.elim (receiverAdvanced ⟨recordedAt,
          Nat.le_trans observationBeforeSource sourceBeforeRecorded,
          Nat.lt_of_le_of_lt headAtObservationAtMostSource headAdvanced⟩)
      · have targetMember : entry.capsule.targetBlock ∈
            entry.capsule.history :=
          entry.capsule.target_and_parents_in_history.1
        rcases installedHistory.2 entry.capsule.targetBlock targetMember with
          targetAtRoot | targetReady
        · have observationBeforeRecorded : observation ≤ recordedAt :=
            Nat.le_trans observationBeforeSource sourceBeforeRecorded
          have sameGc := no_receiver_commit_advance_keeps_gc_round
            receiverInRange receiverCorrectAvailable observationBeforeRecorded
              receiverAdvanced
          rw [targetBlock, exact.exactRound, sameGc] at targetAtRoot
          exact False.elim ((Nat.not_le_of_gt roundAboveReceiverGc)
            targetAtRoot)
        · have acceptedTarget :
              ((timed.execution.trace recordedAt).validatorState
                  receiver).accepted
                exact.production.proposal.block.reference = true := by
            simpa [targetBlock] using targetReady.1
          have recorded := representatives.acceptedCorrectReferenceIsRecorded
            recordedAt receiver exact.production.proposal.block.reference
              receiverInRange receiverCorrectAvailable
              (by simpa [exact.production.proposalAuthor] using authorInRange)
              (by
                simpa [exact.production.proposalAuthor] using
                  authorNotByzantine)
              acceptedTarget
          have observationBeforePin : observation ≤
              peer.packet.deliveredAt + 1 :=
            Nat.le_trans observationBeforeSource
              (Nat.le_trans sourceBeforeDelivery (Nat.le_succ _))
          have gcAtPin := no_receiver_commit_advance_keeps_gc_round
            receiverInRange receiverCorrectAvailable observationBeforePin
              receiverAdvanced
          have aboveGcAtPin :
              ((timed.execution.trace
                (peer.packet.deliveredAt + 1)).validatorState
                  receiver).gcRound <
                exact.production.proposal.block.reference.round := by
            rw [gcAtPin, exact.exactRound]
            exact roundAboveReceiverGc
          have pinAtDelivery := capsuleSync.localBodyLatchesPin
            peer.packet.deliveredAt receiver exact.production.proposal.block
              receiverInRange receiverCorrectAvailable
              (active (peer.packet.deliveredAt + 1) observationBeforePin)
              aboveGcAtPin proposalBody
          let readyAt := max recordedAt (peer.packet.deliveredAt + 1)
          have recordedBeforeReady : recordedAt ≤ readyAt :=
            Nat.le_max_left _ _
          have pinBeforeReady : peer.packet.deliveredAt + 1 ≤ readyAt :=
            Nat.le_max_right _ _
          refine ⟨readyAt,
            Nat.le_trans observationBeforeSource
              (Nat.le_trans sourceBeforeRecorded recordedBeforeReady), ?_⟩
          intro later readyBeforeLater
          have recordedLater :=
            accepted_representative_persists_in_validator_execution
              timed.execution receiverInRange
                (Nat.le_trans recordedBeforeReady readyBeforeLater) (by
                  simpa [exact.exactRound, exact.production.proposalAuthor]
                    using recorded)
          have pinBeforeLater := Nat.le_trans pinBeforeReady readyBeforeLater
          have pinCurrent := capsuleSync.body_pin_persists_while_head_is_current
            receiverInRange receiverCorrectAvailable pinBeforeLater
              pinAtDelivery
              (by
                intro time pinBeforeTime _timeBeforeLater
                exact active time
                  (Nat.le_trans observationBeforePin pinBeforeTime))
              (by
                intro time pinBeforeTime _timeBeforeLater
                have sameGcAtTime :=
                  no_receiver_commit_advance_keeps_gc_round receiverInRange
                    receiverCorrectAvailable
                      (Nat.le_trans observationBeforePin pinBeforeTime)
                      receiverAdvanced
                rw [sameGcAtTime, exact.exactRound]
                exact roundAboveReceiverGc)
              (by
                intro time pinBeforeTime _timeBeforeLater
                have currentHead := no_receiver_commit_advance_keeps_head
                  receiverInRange
                    (Nat.le_trans observationBeforePin pinBeforeTime)
                    receiverAdvanced
                have pinHead := no_receiver_commit_advance_keeps_head
                  receiverInRange observationBeforePin receiverAdvanced
                rw [currentHead, pinHead]
                exact Nat.le_refl _)
          have sameGcLater := no_receiver_commit_advance_keeps_gc_round
            receiverInRange receiverCorrectAvailable
              (Nat.le_trans observationBeforePin pinBeforeLater)
              receiverAdvanced
          have aboveGcLater :
              ((timed.execution.trace later).validatorState receiver).gcRound <
                exact.production.proposal.block.reference.round := by
            rw [sameGcLater, exact.exactRound]
            exact roundAboveReceiverGc
          have acceptedLater :
              ((timed.execution.trace later).validatorState receiver).accepted
                  exact.production.proposal.block.reference = true :=
            (representatives.representativeIsSound later receiver round author
              exact.production.proposal.block.reference receiverInRange
                receiverCorrectAvailable recordedLater).2.2
          exact ⟨recordedLater,
            capsuleSync.acceptedPinnedBodyIsRetained later receiver
              exact.production.proposal.block.reference _ receiverInRange
                receiverCorrectAvailable pinCurrent aboveGcLater
                  acceptedLater⟩

/-- One exact recovery-round source either reaches one fixed correct receiver
or that receiver advances its own commit index. An advance at another host is
not an alternative. -/
theorem exact_recovery_round_source_reaches_receiver_or_receiver_advances
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation author receiver round : Time}
    (source : ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait observation author round)
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (roundAboveReceiverGc :
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorReceiverCommitAdvance timed observation receiver ∨
      ∃ readyAt reference,
        observation ≤ readyAt ∧
          ∀ later, readyAt ≤ later →
            ((timed.execution.trace later).validatorState receiver
                ).acceptedRepresentative round author = some reference ∧
              ((timed.execution.trace later).validatorState receiver).retained
                  reference = true := by
  cases source with
  | persisted production exactRound =>
      let exact : ValidatorExactRoundBroadcastProduction timed obligations
          observation author round :=
        { production
          exactRound }
      rcases exact_round_broadcast_reaches_receiver_or_receiver_advances pins
          capsuleSync acceptance representatives exact authorInRange
            authorCorrectAvailable receiverInRange receiverCorrectAvailable
              roundAboveReceiverGc afterGst active with advanced | stable
      · exact Or.inl advanced
      · rcases stable with ⟨readyAt, observationBeforeReady, stable⟩
        exact Or.inr ⟨readyAt, production.proposal.block.reference,
          observationBeforeReady, stable⟩
  | durableTip sourceAt observationBeforeSource source =>
      by_cases receiverAdvanced :
          ValidatorReceiverCommitAdvance timed observation receiver
      · exact Or.inl receiverAdvanced
      · right
        rcases stable_recovery_tip_source_reaches_receiver pins broadcast
            effects capsuleSync acceptance representatives source
              observationBeforeSource authorInRange authorCorrectAvailable
                receiverInRange receiverCorrectAvailable roundAboveReceiverGc
                  afterGst active receiverAdvanced with
          ⟨readyAt, observationBeforeReady, stable⟩
        exact ⟨readyAt, source.block.reference, observationBeforeReady, stable⟩

/-- One exact recovery-round source becomes a stable exact representative at
one fixed receiver in a suffix without a correct-host commit advance. -/
theorem exact_recovery_round_source_reaches_receiver
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation author receiver round : Time}
    (source : ValidatorExactRecoveryRoundSource (obligations := obligations)
      pins recoveryWait observation author round)
    (authorInRange : author < config.authorityCount)
    (authorCorrectAvailable : faults.correctAvailable author = true)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (roundAboveReceiverGc :
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ readyAt reference,
      observation ≤ readyAt ∧
        ∀ later, readyAt ≤ later →
          ((timed.execution.trace later).validatorState receiver
              ).acceptedRepresentative round author = some reference ∧
            ((timed.execution.trace later).validatorState receiver).retained
                reference = true := by
  rcases exact_recovery_round_source_reaches_receiver_or_receiver_advances
      pins broadcast effects capsuleSync acceptance representatives source
        authorInRange authorCorrectAvailable receiverInRange
          receiverCorrectAvailable roundAboveReceiverGc afterGst active with
    receiverAdvance | stable
  · exact False.elim
      ((no_correct_advance_implies_no_receiver_advance receiverInRange
        receiverCorrectAvailable noAdvance) receiverAdvance)
  · exact stable

/-- An exact recovery-round source family leaves durable own blocks at one
common later time. -/
theorem exact_recovery_round_source_family_eventually_is_owned
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait observation round : Time}
    (sources : EveryCorrectAvailableValidatorExactRecoveryRoundSource
      (obligations := obligations) pins recoveryWait observation round) :
    ∃ finish,
      observation ≤ finish ∧
        EveryCorrectAvailableValidatorOwnBlockAt faults
          (timed.execution.trace finish) round := by
  let authorDone := fun author time =>
    author < config.authorityCount ∧
      (((timed.execution.trace time).validatorState author).ownBlockAt
        round).isSome = true
  have authorDonePersists : ∀ author earlier later,
      earlier ≤ later → authorDone author earlier → authorDone author later := by
    intro author earlier later ordered done
    cases ownValue : ((timed.execution.trace earlier).validatorState
        author).ownBlockAt round with
    | none => simp [authorDone, ownValue] at done
    | some reference =>
        have durable := timed.execution.durableStateMonotone author earlier later
          done.1 ordered
        exact ⟨done.1, by simp [durable.own_block_persists ownValue]⟩
  have eachAuthor : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ finish, observation ≤ finish ∧ authorDone author finish := by
    intro author authorInRange authorCorrectAvailable
    let source := Classical.choice
      (sources author authorInRange authorCorrectAvailable)
    rcases exact_recovery_round_source_eventually_is_owned source with
      ⟨readyAt, reference, observationBeforeReady, owned⟩
    exact ⟨readyAt, observationBeforeReady, authorInRange, by simp [owned]⟩
  rcases eventually_every_selected_validator faults.correctAvailable authorDone
      observation authorDonePersists eachAuthor with
    ⟨finish, observationBeforeFinish, allAuthors⟩
  exact ⟨finish, observationBeforeFinish, by
    intro author authorInRange authorCorrectAvailable
    exact (allAuthors author authorInRange authorCorrectAvailable).2⟩

/-- An exact recovery-round source family either advances one fixed correct
receiver or becomes stable at that receiver. Other hosts' commit advances do
not discharge this result. -/
theorem exact_recovery_round_source_family_reaches_receiver_or_receiver_advances
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation receiver round : Time}
    (sources : EveryCorrectAvailableValidatorExactRecoveryRoundSource
      (obligations := obligations) pins recoveryWait observation round)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (roundAboveReceiverGc :
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorReceiverCommitAdvance timed observation receiver ∨
      ∃ readyAt,
        observation ≤ readyAt ∧
          ∀ later, readyAt ≤ later → ∀ author,
            author < config.authorityCount →
            faults.correctAvailable author = true →
            ∃ reference,
              ((timed.execution.trace later).validatorState receiver
                  ).acceptedRepresentative round author = some reference ∧
                ((timed.execution.trace later).validatorState receiver).retained
                    reference = true := by
  by_cases receiverAdvanced :
      ValidatorReceiverCommitAdvance timed observation receiver
  · exact Or.inl receiverAdvanced
  · right
    let authorDone := fun author readyAt =>
      ∀ later, readyAt ≤ later →
        ∃ reference,
          ((timed.execution.trace later).validatorState receiver
              ).acceptedRepresentative round author = some reference ∧
            ((timed.execution.trace later).validatorState receiver).retained
                reference = true
    have authorDonePersists : ∀ author earlier later,
        earlier ≤ later → authorDone author earlier →
          authorDone author later := by
      intro author earlier later ordered done future laterBeforeFuture
      exact done future (Nat.le_trans ordered laterBeforeFuture)
    have eachAuthor : ∀ author,
        author < config.authorityCount →
        faults.correctAvailable author = true →
        ∃ readyAt, observation ≤ readyAt ∧ authorDone author readyAt := by
      intro author authorInRange authorCorrectAvailable
      let source := Classical.choice
        (sources author authorInRange authorCorrectAvailable)
      rcases
          exact_recovery_round_source_reaches_receiver_or_receiver_advances
            pins broadcast effects capsuleSync acceptance representatives
              source authorInRange authorCorrectAvailable receiverInRange
                receiverCorrectAvailable roundAboveReceiverGc afterGst active
        with advanced | stable
      · exact False.elim (receiverAdvanced advanced)
      · rcases stable with
          ⟨readyAt, reference, observationBeforeReady, stable⟩
        exact ⟨readyAt, observationBeforeReady, by
          intro later readyBeforeLater
          exact ⟨reference, stable later readyBeforeLater⟩⟩
    rcases eventually_every_selected_validator faults.correctAvailable
        authorDone observation authorDonePersists eachAuthor with
      ⟨readyAt, observationBeforeReady, allAuthors⟩
    exact ⟨readyAt, observationBeforeReady, by
      intro later readyBeforeLater author authorInRange authorCorrectAvailable
      exact allAuthors author authorInRange authorCorrectAvailable later
        readyBeforeLater⟩

/-- An exact recovery-round source family becomes stable at one correct
receiver in a suffix without a correct-host commit advance. -/
theorem exact_recovery_round_source_family_reaches_receiver
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation receiver round : Time}
    (sources : EveryCorrectAvailableValidatorExactRecoveryRoundSource
      (obligations := obligations) pins recoveryWait observation round)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (roundAboveReceiverGc :
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ readyAt,
      observation ≤ readyAt ∧
        ∀ later, readyAt ≤ later → ∀ author,
          author < config.authorityCount →
          faults.correctAvailable author = true →
          ∃ reference,
            ((timed.execution.trace later).validatorState receiver
                ).acceptedRepresentative round author = some reference ∧
              ((timed.execution.trace later).validatorState receiver).retained
                  reference = true := by
  rcases
      exact_recovery_round_source_family_reaches_receiver_or_receiver_advances
        pins broadcast effects capsuleSync acceptance representatives sources
          receiverInRange receiverCorrectAvailable roundAboveReceiverGc
            afterGst active with
    receiverAdvance | stable
  · exact False.elim
      ((no_correct_advance_implies_no_receiver_advance receiverInRange
        receiverCorrectAvailable noAdvance) receiverAdvance)
  · exact stable

/-- An exact recovery-round source family gives the first stable correct quorum
layer. This theorem keeps all concrete source blocks until the layer is
assembled. -/
theorem exact_recovery_round_source_family_gives_stable_common_round
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation round : Time}
    (sources : EveryCorrectAvailableValidatorExactRecoveryRoundSource
      (obligations := obligations) pins recoveryWait observation round)
    (roundAboveGc : ∀ receiver,
      receiver < config.authorityCount →
      faults.correctAvailable receiver = true →
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ∃ finish,
      observation ≤ finish ∧
        EveryCorrectAvailableValidatorOwnBlockAt faults
          (timed.execution.trace finish) round ∧
        ProducedCorrectQuorumLayer config faults
          (timed.execution.trace finish) round ∧
        EveryCorrectAvailableValidatorStableTipAcceptedFrom faults
          timed.execution.trace finish round := by
  rcases exact_recovery_round_source_family_eventually_is_owned sources with
    ⟨ownedAt, observationBeforeOwned, owned⟩
  let observerDone := fun observer readyAt =>
    ∀ later, readyAt ≤ later → ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ reference,
        ((timed.execution.trace later).validatorState observer
            ).acceptedRepresentative round author = some reference ∧
          ((timed.execution.trace later).validatorState observer).retained
              reference = true
  have observerDonePersists : ∀ observer earlier later,
      earlier ≤ later → observerDone observer earlier →
        observerDone observer later := by
    intro observer earlier later ordered done future laterBeforeFuture
      author authorInRange authorCorrectAvailable
    exact done future (Nat.le_trans ordered laterBeforeFuture) author
      authorInRange authorCorrectAvailable
  have eachObserver : ∀ observer,
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
      ∃ readyAt, observation ≤ readyAt ∧ observerDone observer readyAt := by
    intro observer observerInRange observerCorrectAvailable
    rcases exact_recovery_round_source_family_reaches_receiver pins broadcast
        effects capsuleSync acceptance representatives sources observerInRange
          observerCorrectAvailable
            (roundAboveGc observer observerInRange observerCorrectAvailable)
              afterGst active noAdvance with
      ⟨readyAt, observationBeforeReady, stable⟩
    exact ⟨readyAt, observationBeforeReady, stable⟩
  rcases eventually_every_selected_validator faults.correctAvailable
      observerDone observation observerDonePersists eachObserver with
    ⟨stableAt, observationBeforeStable, allObservers⟩
  let finish := max ownedAt stableAt
  have ownedBeforeFinish : ownedAt ≤ finish := Nat.le_max_left _ _
  have stableBeforeFinish : stableAt ≤ finish := Nat.le_max_right _ _
  have ownedAtFinish : EveryCorrectAvailableValidatorOwnBlockAt faults
      (timed.execution.trace finish) round := by
    intro author authorInRange authorCorrectAvailable
    have ownSome := owned author authorInRange authorCorrectAvailable
    cases ownValue : ((timed.execution.trace ownedAt).validatorState
        author).ownBlockAt round with
    | none => simp [ownValue] at ownSome
    | some reference =>
        have durable := timed.execution.durableStateMonotone author ownedAt
          finish authorInRange ownedBeforeFinish
        simp [durable.own_block_persists ownValue]
  refine ⟨finish, Nat.le_trans observationBeforeOwned ownedBeforeFinish,
    ownedAtFinish,
    every_correct_own_block_gives_produced_quorum_layer ownedAtFinish, ?_⟩
  intro later finishBeforeLater observer author observerInRange
    observerCorrectAvailable authorInRange authorCorrectAvailable
  exact allObservers observer observerInRange observerCorrectAvailable later
    (Nat.le_trans stableBeforeFinish finishBeforeLater) author authorInRange
      authorCorrectAvailable

/-- One fixed correct receiver advances its commit index or eventually keeps
one exact accepted and retained block from every correct author in the round. -/
theorem exact_round_broadcast_family_reaches_receiver_or_receiver_advances
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation receiver round : Time}
    (family : EveryCorrectAvailableValidatorExactRoundBroadcast timed
      obligations observation round)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (roundAboveReceiverGc :
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorReceiverCommitAdvance timed observation receiver ∨
      ∃ readyAt,
        observation ≤ readyAt ∧
          ∀ later, readyAt ≤ later → ∀ author,
            author < config.authorityCount →
            faults.correctAvailable author = true →
            ∃ reference,
              ((timed.execution.trace later).validatorState receiver
                  ).acceptedRepresentative round author = some reference ∧
                ((timed.execution.trace later).validatorState receiver).retained
                    reference = true := by
  classical
  by_cases receiverAdvanced :
      ValidatorReceiverCommitAdvance timed observation receiver
  · exact Or.inl receiverAdvanced
  · right
    let authorDone := fun author readyAt =>
      ∀ later, readyAt ≤ later →
        ∃ reference,
          ((timed.execution.trace later).validatorState receiver
              ).acceptedRepresentative round author = some reference ∧
            ((timed.execution.trace later).validatorState receiver).retained
                reference = true
    have authorDonePersists : ∀ author earlier later,
        earlier ≤ later →
        authorDone author earlier →
        authorDone author later := by
      intro author earlier later ordered done future laterBeforeFuture
      exact done future (Nat.le_trans ordered laterBeforeFuture)
    have eachAuthor : ∀ author,
        author < config.authorityCount →
        faults.correctAvailable author = true →
        ∃ readyAt, observation ≤ readyAt ∧ authorDone author readyAt := by
      intro author authorInRange authorCorrectAvailable
      let exact := Classical.choice
        (family author authorInRange authorCorrectAvailable)
      rcases exact_round_broadcast_reaches_receiver_or_receiver_advances pins
          capsuleSync acceptance representatives exact authorInRange
            authorCorrectAvailable receiverInRange receiverCorrectAvailable
              roundAboveReceiverGc afterGst active with advanced | stable
      · exact False.elim (receiverAdvanced advanced)
      · rcases stable with ⟨readyAt, observationBeforeReady, stable⟩
        exact ⟨readyAt, observationBeforeReady, by
          intro later readyBeforeLater
          exact ⟨exact.production.proposal.block.reference,
            stable later readyBeforeLater⟩⟩
    rcases eventually_every_selected_validator faults.correctAvailable
        authorDone observation authorDonePersists eachAuthor with
      ⟨readyAt, observationBeforeReady, allAuthors⟩
    exact ⟨readyAt, observationBeforeReady, by
      intro later readyBeforeLater author authorInRange authorCorrectAvailable
      exact allAuthors author authorInRange authorCorrectAvailable later
        readyBeforeLater⟩

/-- A finite exact-broadcast family becomes one produced correct layer at a
common trace time. -/
theorem exact_round_broadcast_family_eventually_is_produced
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
    {observation round : Time}
    (family : EveryCorrectAvailableValidatorExactRoundBroadcast timed
      obligations observation round) :
    ∃ finish,
      observation ≤ finish ∧
        EveryCorrectAvailableValidatorProduced faults
          (timed.execution.trace finish) round := by
  classical
  let done := fun validator time =>
    validator < config.authorityCount ∧
      (((timed.execution.trace time).validatorState validator).ownBlockAt
          round).isSome = true ∧
        ((timed.execution.trace time).validatorState validator).sentOwnBlockAt
            round = true
  have donePersists : ∀ validator earlier later,
      earlier ≤ later → done validator earlier → done validator later := by
    intro validator earlier later ordered completed
    rcases completed with ⟨validatorInRange, ownSome, sent⟩
    cases ownValue : ((timed.execution.trace earlier).validatorState
        validator).ownBlockAt round with
    | none => simp [ownValue] at ownSome
    | some reference =>
        have durable := timed.execution.durableStateMonotone validator earlier
          later validatorInRange ordered
        exact ⟨validatorInRange, by simp [durable.own_block_persists ownValue],
          durable.sent_own_block_persists sent⟩
  have eachDone : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ∃ finish, observation ≤ finish ∧ done validator finish := by
    intro validator validatorInRange validatorCorrectAvailable
    let exact := Classical.choice
      (family validator validatorInRange validatorCorrectAvailable)
    refine ⟨exact.production.finish, ?_, ?_⟩
    · exact Nat.le_trans exact.production.startBeforePersistence
        (Nat.le_trans (Nat.le_succ _)
          exact.production.persistenceBeforeFinish)
    · refine ⟨validatorInRange, ?_, ?_⟩
      · have ownAtRound :
            ((timed.execution.trace exact.production.finish).validatorState
                validator).ownBlockAt round =
              some exact.production.proposal.block.reference := by
          simpa only [exact.exactRound] using
            exact.production.ownBlockStoredAtFinish
        simp [ownAtRound]
      · simpa [done, exact.exactRound] using
          exact.production.sentOwnBlockAtFinish
  rcases eventually_every_selected_validator faults.correctAvailable done
      observation donePersists eachDone with
    ⟨finish, observationBeforeFinish, allDone⟩
  exact ⟨finish, observationBeforeFinish, by
    intro validator validatorInRange validatorCorrectAvailable
    exact (allDone validator validatorInRange validatorCorrectAvailable).2⟩

/-- One exact-broadcast family becomes stable at every correct receiver. If
this does not happen, one actual correct receiver advances its commit index. -/
theorem exact_round_broadcast_family_eventually_is_stably_common_or_commit_advance
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation round : Time}
    (family : EveryCorrectAvailableValidatorExactRoundBroadcast timed
      obligations observation round)
    (roundAboveGc : ∀ receiver,
      receiver < config.authorityCount →
      faults.correctAvailable receiver = true →
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed observation ∨
      ∃ readyAt,
        observation ≤ readyAt ∧
          EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
            timed.execution.trace readyAt round := by
  classical
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed observation
  · exact Or.inl advanced
  · right
    let receiverDone := fun receiver readyAt =>
      ∀ later, readyAt ≤ later → ∀ author,
        author < config.authorityCount →
        faults.correctAvailable author = true →
        ∃ reference,
          ((timed.execution.trace later).validatorState receiver
              ).acceptedRepresentative round author = some reference ∧
            ((timed.execution.trace later).validatorState receiver).retained
                reference = true
    have receiverDonePersists : ∀ receiver earlier later,
        earlier ≤ later →
        receiverDone receiver earlier →
        receiverDone receiver later := by
      intro receiver earlier later ordered done future laterBeforeFuture
      exact done future (Nat.le_trans ordered laterBeforeFuture)
    have eachReceiver : ∀ receiver,
        receiver < config.authorityCount →
        faults.correctAvailable receiver = true →
        ∃ readyAt,
          observation ≤ readyAt ∧ receiverDone receiver readyAt := by
      intro receiver receiverInRange receiverCorrectAvailable
      rcases
          exact_round_broadcast_family_reaches_receiver_or_receiver_advances
            pins capsuleSync acceptance representatives family receiverInRange
              receiverCorrectAvailable
              (roundAboveGc receiver receiverInRange receiverCorrectAvailable)
              afterGst active with receiverAdvance | stable
      · rcases receiverAdvance with ⟨finish, observationBeforeFinish,
          headAdvanced⟩
        exact False.elim (advanced ⟨receiver, finish, receiverInRange,
          receiverCorrectAvailable, observationBeforeFinish, headAdvanced⟩)
      · exact stable
    rcases eventually_every_selected_validator faults.correctAvailable
        receiverDone observation receiverDonePersists eachReceiver with
      ⟨readyAt, observationBeforeReady, allReceivers⟩
    refine ⟨readyAt, observationBeforeReady, ?_⟩
    intro later readyBeforeLater receiver author receiverInRange
      receiverCorrectAvailable authorInRange authorCorrectAvailable
    exact allReceivers receiver receiverInRange receiverCorrectAvailable later
      readyBeforeLater author authorInRange authorCorrectAvailable

/-- One exact-broadcast family gives the stable produced and retained state
used by the next-round induction. A real correct-host commit advance is the
only alternative. -/
theorem exact_round_broadcast_family_gives_stable_common_round_or_commit_advance
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {observation round : Time}
    (family : EveryCorrectAvailableValidatorExactRoundBroadcast timed
      obligations observation round)
    (roundAboveGc : ∀ receiver,
      receiver < config.authorityCount →
      faults.correctAvailable receiver = true →
      ((timed.execution.trace observation).validatorState receiver).gcRound <
        round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed observation ∨
      ∃ readyAt,
        observation ≤ readyAt ∧
          EveryCorrectAvailableValidatorProduced faults
            (timed.execution.trace readyAt) round ∧
          EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
            timed.execution.trace readyAt round := by
  rcases exact_round_broadcast_family_eventually_is_stably_common_or_commit_advance
      pins capsuleSync acceptance representatives family roundAboveGc afterGst
        active with advanced | stable
  · exact Or.inl advanced
  · rcases stable with ⟨stableAt, observationBeforeStable, stable⟩
    rcases exact_round_broadcast_family_eventually_is_produced family with
      ⟨producedAt, observationBeforeProduced, produced⟩
    let readyAt := max stableAt producedAt
    have stableBeforeReady : stableAt ≤ readyAt := Nat.le_max_left _ _
    have producedBeforeReady : producedAt ≤ readyAt := Nat.le_max_right _ _
    have producedAtReady := every_correct_produced_round_persists_in_execution
      timed.execution producedBeforeReady produced
    exact Or.inr ⟨readyAt,
      Nat.le_trans observationBeforeStable stableBeforeReady,
      producedAtReady, by
        intro later readyBeforeLater receiver author receiverInRange
          receiverCorrectAvailable authorInRange authorCorrectAvailable
        exact stable later (Nat.le_trans stableBeforeReady readyBeforeLater)
          receiver author receiverInRange receiverCorrectAvailable
            authorInRange authorCorrectAvailable⟩

/-- One schedule-independent stable-round induction step. The result keeps the
exact block and peer packet for every correct author. -/
structure ValidatorStableCommonRoundStep
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (snapshot round : Time) where
  family : EveryCorrectAvailableValidatorExactRoundBroadcast timed obligations
    snapshot (round + 1)
  finish : Time
  snapshotBeforeFinish : snapshot ≤ finish
  produced : EveryCorrectAvailableValidatorProduced faults
    (timed.execution.trace finish) (round + 1)
  stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
    timed.execution.trace finish (round + 1)

/-- If one correct validator crosses the round after the proof-only common
base, its exact persisted block is sent to every other validator. -/
theorem active_recovery_crossing_above_common_base_gives_exact_broadcast
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
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot finish validator baseRound : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (localBaseAtMostCommon :
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator) ≤
        baseRound)
    (snapshotBeforeFinish : snapshot ≤ finish)
    (targetReached : baseRound + 1 ≤
      ((timed.execution.trace finish).validatorState
        validator).highestSignedRound)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorExactRoundBroadcastProduction timed obligations snapshot
      validator (baseRound + 1)) := by
  rcases active_recovery_crossing_above_common_base_is_exact modeRules
      roundRules recovery validatorInRange validatorCorrectAvailable
      localBaseAtMostCommon snapshotBeforeFinish targetReached active noAdvance
    with ⟨persistTime, block, snapshotBeforePersist, _persistBeforeFinish,
      persisted, blockRound⟩
  have startFloorAtMostBase :
      ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound ≤ baseRound :=
    Nat.le_trans
      (signer_floor_le_local_recovery_base
        ((timed.execution.trace snapshot).validatorState validator))
      localBaseAtMostCommon
  have blockAboveStartFloor :
      ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound < block.reference.round := by
    rw [blockRound]
    omega
  let result := Classical.choice
    (persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable snapshotBeforePersist blockAboveStartFloor
          persisted)
  exact ⟨{
    production := result.1
    exactRound := by simpa [result.2.2] using blockRound }⟩

/-- A stable common round produces one exact broadcast from every correct
validator in the next round.

The proof handles a validator that crossed the next round while the other
validators finished block synchronization. The local recovery round rule
identifies that earlier crossing as the same exact round. -/
theorem stable_common_round_gives_exact_next_broadcast_family
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
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot readyAt round : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (snapshotBeforeReady : snapshot ≤ readyAt)
    (baseAtMostRound :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount ≤ round)
    (produced : EveryCorrectAvailableValidatorOwnBlockAt faults
      (timed.execution.trace readyAt) round)
    (stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
      timed.execution.trace readyAt round)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    EveryCorrectAvailableValidatorExactRoundBroadcast timed obligations
      snapshot (round + 1) := by
  have roundAboveGc : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace readyAt).validatorState validator).gcRound <
        round := by
    intro validator validatorInRange validatorCorrectAvailable
    have initialBounds := correct_validator_floor_and_gc_below_recovery_base
      (world := timed.execution.trace snapshot) validatorInRange
        validatorCorrectAvailable
    have sameGc := no_commit_advance_keeps_correct_gc_round validatorInRange
      validatorCorrectAvailable snapshotBeforeReady noAdvance
    rw [sameGc]
    exact Nat.lt_of_lt_of_le initialBounds.2 baseAtMostRound
  have parentQuorums :=
    stable_common_retained_round_gives_recovery_parent_quorums representatives
      stable roundAboveGc
  intro validator validatorInRange validatorCorrectAvailable
  have localBaseAtMostRound :
      ValidatorLocalRecoveryBaseRound
          ((timed.execution.trace snapshot).validatorState validator) ≤
        round :=
    Nat.le_trans
      (correct_validator_recovery_base_le_maximum
        (world := timed.execution.trace snapshot) validatorInRange
          validatorCorrectAvailable)
      baseAtMostRound
  have startFloorAtMostRound :
      ((timed.execution.trace snapshot).validatorState
          validator).highestSignedRound ≤ round :=
    Nat.le_trans
      (signer_floor_le_local_recovery_base
        ((timed.execution.trace snapshot).validatorState validator))
      localBaseAtMostRound
  have roundAtMostReadyFloor : round ≤
      ((timed.execution.trace readyAt).validatorState
        validator).highestSignedRound := by
    have ownSome := produced validator validatorInRange
      validatorCorrectAvailable
    cases ownValue : ((timed.execution.trace readyAt).validatorState
        validator).ownBlockAt round with
    | none => simp [ownValue] at ownSome
    | some reference =>
        exact
          (timed.execution.statesWellFormed readyAt validator
            validatorInRange).ownBlockDoesNotExceedSignerFloor round reference
              ownValue
  by_cases targetReached : round + 1 ≤
      ((timed.execution.trace readyAt).validatorState
        validator).highestSignedRound
  · exact active_recovery_crossing_above_common_base_gives_exact_broadcast
      modeRules roundRules latchSource effects authorityCountAtLeastTwo
        recovery validatorInRange validatorCorrectAvailable
          localBaseAtMostRound snapshotBeforeReady targetReached active noAdvance
  · have floorAtReady :
        ((timed.execution.trace readyAt).validatorState
            validator).highestSignedRound = round := by
      exact Nat.le_antisymm
        (Nat.le_of_lt_succ (Nat.lt_of_not_ge targetReached))
        roundAtMostReadyFloor
    have activeFromReady : ∀ later, readyAt ≤ later →
        (timed.execution.trace later).epochActive = true := by
      intro later readyBeforeLater
      exact active later (Nat.le_trans snapshotBeforeReady readyBeforeLater)
    let receiver := recoveryOtherReceiver validator
    have receiverInRange : receiver < config.authorityCount :=
      recovery_other_receiver_in_range authorityCountAtLeastTwo
    have receiverIsOther : receiver ≠ validator :=
      recovery_other_receiver_is_different
    have parentsAtRound := parentQuorums validator validatorInRange
      validatorCorrectAvailable
    have parentsAtCurrentTarget : ValidatorRecoveryParentQuorumReadyAt config
        ((timed.execution.trace readyAt).validatorState validator)
        (((timed.execution.trace readyAt).validatorState
          validator).highestSignedRound + 1) := by
      rw [floorAtReady]
      exact parentsAtRound
    rcases current_recovery_parent_quorum_gives_broadcast_or_commit_advance
        timerSource pacing arms latchSource effects validatorInRange
          validatorCorrectAvailable receiverInRange receiverIsOther
            parentsAtCurrentTarget activeFromReady with localAdvance | strict
    · rcases localAdvance with ⟨finish, readyBeforeFinish, advanced⟩
      have headAtSnapshotAtMostReady :=
        (timed.execution.durableStateMonotone validator snapshot readyAt
          validatorInRange snapshotBeforeReady).1
      exact False.elim (noAdvance ⟨validator, finish, validatorInRange,
        validatorCorrectAvailable,
        Nat.le_trans snapshotBeforeReady readyBeforeFinish,
        Nat.lt_of_le_of_lt headAtSnapshotAtMostReady advanced⟩)
    · rcases strict with
        ⟨result, targetAtReady, readyBeforePersistence, _readyBeforeSend⟩
      have exactRound : result.snapshot.block.reference.round = round + 1 := by
        rw [result.completed.snapshotRound, targetAtReady, floorAtReady]
      have snapshotBeforePersistence : snapshot ≤ result.persistTime :=
        Nat.le_trans snapshotBeforeReady readyBeforePersistence
      have blockAboveStartFloor :
          ((timed.execution.trace snapshot).validatorState
              validator).highestSignedRound <
            result.snapshot.block.reference.round := by
        rw [exactRound]
        omega
      let broadcast := Classical.choice
        (persist_proposal_occurrence_eventually_produces_exact_broadcast
          latchSource effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable snapshotBeforePersistence
              blockAboveStartFloor result.persistenceOccurs)
      exact ⟨{
        production := broadcast.1
        exactRound := by simpa [broadcast.2.2] using exactRound }⟩

/-- One produced and stable common round gives the same state at the next
round, unless an actual correct host advances its commit index. -/
theorem stable_common_round_advances_or_gives_next_stable_common_round
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot readyAt round : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (snapshotBeforeReady : snapshot ≤ readyAt)
    (baseAtMostRound :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount ≤ round)
    (produced : EveryCorrectAvailableValidatorOwnBlockAt faults
      (timed.execution.trace readyAt) round)
    (stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
      timed.execution.trace readyAt round)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed snapshot ∨
      Nonempty (ValidatorStableCommonRoundStep timed obligations snapshot
        round) := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed snapshot
  · exact Or.inl advanced
  · have family := stable_common_round_gives_exact_next_broadcast_family
      modeRules roundRules timerSource pacing arms latchSource effects
        representatives authorityCountAtLeastTwo recovery snapshotBeforeReady
          baseAtMostRound produced stable active advanced
    have roundAboveGc : ∀ receiver,
        receiver < config.authorityCount →
        faults.correctAvailable receiver = true →
        ((timed.execution.trace snapshot).validatorState receiver).gcRound <
          round + 1 := by
      intro receiver receiverInRange receiverCorrectAvailable
      have initialBounds := correct_validator_floor_and_gc_below_recovery_base
        (world := timed.execution.trace snapshot) receiverInRange
          receiverCorrectAvailable
      exact Nat.lt_of_lt_of_le initialBounds.2
        (Nat.le_trans baseAtMostRound (Nat.le_succ round))
    rcases exact_round_broadcast_family_gives_stable_common_round_or_commit_advance
        pins capsuleSync acceptance representatives family roundAboveGc
          recovery.afterGst active with laterAdvance | next
    · exact False.elim (advanced laterAdvance)
    · rcases next with ⟨finish, snapshotBeforeFinish, nextProduced,
        nextStable⟩
      exact Or.inr ⟨{
        family
        finish
        snapshotBeforeFinish
        produced := nextProduced
        stable := nextStable }⟩

/-- A finite schedule-independent suffix above one stable common round. Each
offset keeps the concrete exact block and peer packets for every correct
author. -/
structure ValidatorStableCommonRoundWindow
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (snapshot baseRound count : Time) where
  finish : Time
  snapshotBeforeFinish : snapshot ≤ finish
  produced : EveryCorrectAvailableValidatorProduced faults
    (timed.execution.trace finish) (baseRound + count)
  stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
    timed.execution.trace finish (baseRound + count)
  familyAt : ∀ offset,
    offset < count →
    EveryCorrectAvailableValidatorExactRoundBroadcast timed obligations
      snapshot (baseRound + offset + 1)

/-- In a suffix without a correct-host commit advance, one stable common round
extends through any finite number of exact rounds. -/
theorem stable_common_round_gives_finite_exact_broadcast_window
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot readyAt baseRound count : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (snapshotBeforeReady : snapshot ≤ readyAt)
    (commonBaseAtMostBaseRound :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount ≤ baseRound)
    (produced : EveryCorrectAvailableValidatorProduced faults
      (timed.execution.trace readyAt) baseRound)
    (stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
      timed.execution.trace readyAt baseRound)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorStableCommonRoundWindow timed obligations snapshot
      baseRound count) := by
  induction count generalizing readyAt with
  | zero =>
      exact ⟨{
        finish := readyAt
        snapshotBeforeFinish := snapshotBeforeReady
        produced := by simpa using produced
        stable := by simpa using stable
        familyAt := by
          intro offset offsetInRange
          exact False.elim (Nat.not_lt_zero offset offsetInRange) }⟩
  | succ previous inductionHypothesis =>
      let windowPrefix := Classical.choice
        (inductionHypothesis snapshotBeforeReady produced stable)
      have baseAtMostCurrent :
          correctValidatorRecoveryBaseMaximumUpTo faults
              (timed.execution.trace snapshot) config.authorityCount ≤
            baseRound + previous :=
        Nat.le_trans commonBaseAtMostBaseRound
          (Nat.le_add_right baseRound previous)
      have ownedCurrent : EveryCorrectAvailableValidatorOwnBlockAt faults
          (timed.execution.trace windowPrefix.finish)
            (baseRound + previous) := by
        intro validator validatorInRange validatorCorrectAvailable
        exact (windowPrefix.produced validator validatorInRange
          validatorCorrectAvailable).1
      rcases stable_common_round_advances_or_gives_next_stable_common_round
          modeRules roundRules timerSource pacing arms latchSource effects pins
            capsuleSync acceptance representatives authorityCountAtLeastTwo
              recovery windowPrefix.snapshotBeforeFinish baseAtMostCurrent
                ownedCurrent windowPrefix.stable active with
                  advanced | next
      · exact False.elim (noAdvance advanced)
      · let step := Classical.choice next
        refine ⟨{
          finish := step.finish
          snapshotBeforeFinish := step.snapshotBeforeFinish
          produced := by
            simpa [Nat.add_assoc] using step.produced
          stable := by
            simpa [Nat.add_assoc] using step.stable
          familyAt := ?_ }⟩
        intro offset offsetInRange
        by_cases earlier : offset < previous
        · exact windowPrefix.familyAt offset earlier
        · have last : offset = previous :=
            Nat.le_antisymm (Nat.le_of_lt_succ offsetInRange)
              (Nat.le_of_not_gt earlier)
          subst offset
          simpa [Nat.add_assoc] using step.family

/-- The late part of one stable common-round window, after its warm-up round.
Each family member keeps the exact broadcast block and the exact timer-paced
proposal snapshot which created it. -/
structure ValidatorFreshTimerPacedExactRoundWindow
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (observation baseRound count : Time) where
  source : ValidatorStableCommonRoundWindow timed obligations observation
    baseRound (count + 1)
  freshAt : ∀ offset,
    offset < count →
    EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed obligations
      waits observation (baseRound + offset + 2)

/-- Remove the warm-up family from an already-derived stable window and recover
the exact fresh timer origin for every later offset. -/
theorem stable_common_round_window_gives_fresh_timer_paced_suffix
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (originRules : ValidatorBlockProgressProposalOriginRules
      (obligations := obligations) mode)
    (timingRules : ValidatorRecoveryProposalActionTimingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {observation baseRound count : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      observation)
    (blockRecovery : ValidatorActiveBlockProgressRecoverySnapshot mode
      observation)
    (commonBaseAtMostBase :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace observation) config.authorityCount ≤ baseRound)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation)
    (window : ValidatorStableCommonRoundWindow timed obligations observation
      baseRound (count + 1)) :
    Nonempty (ValidatorFreshTimerPacedExactRoundWindow timed obligations waits
      observation baseRound count) := by
  refine ⟨{
    source := window
    freshAt := ?_ }⟩
  intro offset offsetInRange
  have sourceOffsetInRange : offset + 1 < count + 1 := by
    exact Nat.add_lt_add_right offsetInRange 1
  have sourceFamily := window.familyAt (offset + 1) sourceOffsetInRange
  have fresh := late_exact_broadcast_family_gives_fresh_timer_paced_family
    modeRules sameRecoveryWait roundRules originRules timingRules latchSource
      pacing effects authorityCountAtLeastTwo sourceFamily recovery blockRecovery
        commonBaseAtMostBase (Nat.zero_lt_succ offset) active noAdvance
  simpa [Nat.add_assoc] using fresh

/-- A current stable common round derives a requested finite family of fresh
timer-paced rounds. The first exact-next round is used only as a warm-up and is
not returned. No future layer, proposal, or timer is an input. -/
theorem stable_common_round_gives_finite_fresh_timer_paced_window
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (originRules : ValidatorBlockProgressProposalOriginRules
      (obligations := obligations) mode)
    (timingRules : ValidatorRecoveryProposalActionTimingRules timerSource)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {observation readyAt baseRound count : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      observation)
    (blockRecovery : ValidatorActiveBlockProgressRecoverySnapshot mode
      observation)
    (observationBeforeReady : observation ≤ readyAt)
    (commonBaseAtMostBase :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace observation) config.authorityCount ≤ baseRound)
    (produced : EveryCorrectAvailableValidatorProduced faults
      (timed.execution.trace readyAt) baseRound)
    (stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
      timed.execution.trace readyAt baseRound)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    Nonempty (ValidatorFreshTimerPacedExactRoundWindow timed obligations waits
      observation baseRound count) := by
  let window := Classical.choice
    (stable_common_round_gives_finite_exact_broadcast_window modeRules
      roundRules timerSource pacing arms latchSource effects pins capsuleSync
        acceptance representatives authorityCountAtLeastTwo recovery
          observationBeforeReady commonBaseAtMostBase produced stable active
            noAdvance (count := count + 1))
  exact stable_common_round_window_gives_fresh_timer_paced_suffix modeRules
    sameRecoveryWait roundRules originRules timingRules latchSource pacing effects
      authorityCountAtLeastTwo recovery blockRecovery commonBaseAtMostBase active
        noAdvance window

/-- A finite schedule-independent window which also keeps the first round's
restart-safe source. Offset zero can be a durable pre-suffix tip. Every later
offset is a concrete persisted exact-round broadcast. -/
structure ValidatorExactRecoveryRoundSourceWindow
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recoveryWait snapshot baseRound count : Time) where
  finish : Time
  snapshotBeforeFinish : snapshot ≤ finish
  owned : EveryCorrectAvailableValidatorOwnBlockAt faults
    (timed.execution.trace finish) (baseRound + count)
  stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
    timed.execution.trace finish (baseRound + count)
  sourceAt : ∀ offset,
    offset ≤ count →
    EveryCorrectAvailableValidatorExactRecoveryRoundSource
      (obligations := obligations) pins recoveryWait snapshot
        (baseRound + offset)

/-- A concrete persisted exact-round family is also a restart-safe source
family for that round. -/
theorem exact_round_broadcast_family_is_recovery_source_family
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait snapshot round : Time}
    (family : EveryCorrectAvailableValidatorExactRoundBroadcast timed
      obligations snapshot round) :
    EveryCorrectAvailableValidatorExactRecoveryRoundSource
      (obligations := obligations) pins recoveryWait snapshot round := by
  intro author authorInRange authorCorrectAvailable
  let exact := Classical.choice
    (family author authorInRange authorCorrectAvailable)
  exact ⟨.persisted exact.production exact.exactRound⟩

/-- One arbitrary restart-safe common source family extends through any finite
number of schedule-independent exact rounds. No future layer is an input. -/
theorem exact_recovery_round_source_family_gives_finite_window
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins
      modeRules.recoveryWait)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot baseRound count : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (baseAtLeastRecoveryMaximum :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount ≤ baseRound)
    (baseSources :
      EveryCorrectAvailableValidatorExactRecoveryRoundSource
        (obligations := obligations) pins modeRules.recoveryWait snapshot
          baseRound)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorExactRecoveryRoundSourceWindow timed obligations pins
      modeRules.recoveryWait snapshot baseRound count) := by
  have baseRoundAboveGc : ∀ receiver,
      receiver < config.authorityCount →
      faults.correctAvailable receiver = true →
      ((timed.execution.trace snapshot).validatorState receiver).gcRound <
        baseRound := by
    intro receiver receiverInRange receiverCorrectAvailable
    have bounds := correct_validator_floor_and_gc_below_recovery_base
      (world := timed.execution.trace snapshot) receiverInRange
        receiverCorrectAvailable
    exact Nat.lt_of_lt_of_le bounds.2 baseAtLeastRecoveryMaximum
  rcases exact_recovery_round_source_family_gives_stable_common_round pins
      broadcast effects capsuleSync acceptance representatives baseSources
        baseRoundAboveGc recovery.afterGst active noAdvance with
    ⟨baseFinish, snapshotBeforeBaseFinish, baseOwned, _baseProduced,
      baseStable⟩
  have baseStable' : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom
      faults timed.execution.trace baseFinish baseRound := by
    exact baseStable
  induction count with
  | zero =>
      exact ⟨{
        finish := baseFinish
        snapshotBeforeFinish := snapshotBeforeBaseFinish
        owned := by simpa using baseOwned
        stable := by simpa using baseStable'
        sourceAt := by
          intro offset offsetAtMostZero
          have offsetZero : offset = 0 := Nat.eq_zero_of_le_zero offsetAtMostZero
          subst offset
          simpa using baseSources }⟩
  | succ previous inductionHypothesis =>
      let windowPrefix := Classical.choice inductionHypothesis
      have baseAtMostCurrent :
          correctValidatorRecoveryBaseMaximumUpTo faults
              (timed.execution.trace snapshot) config.authorityCount ≤
            baseRound + previous :=
        Nat.le_trans baseAtLeastRecoveryMaximum
          (Nat.le_add_right baseRound previous)
      rcases stable_common_round_advances_or_gives_next_stable_common_round
          modeRules roundRules timerSource pacing arms latchSource effects pins
            capsuleSync acceptance representatives authorityCountAtLeastTwo
              recovery windowPrefix.snapshotBeforeFinish baseAtMostCurrent
                windowPrefix.owned windowPrefix.stable active with advanced | next
      · exact False.elim (noAdvance advanced)
      · let step := Classical.choice next
        let nextSources := exact_round_broadcast_family_is_recovery_source_family
          (pins := pins) (recoveryWait := modeRules.recoveryWait) step.family
        refine ⟨{
          finish := step.finish
          snapshotBeforeFinish := step.snapshotBeforeFinish
          owned := ?_
          stable := by simpa [Nat.add_assoc] using step.stable
          sourceAt := ?_ }⟩
        · intro validator validatorInRange validatorCorrectAvailable
          simpa [Nat.add_assoc] using
            (step.produced validator validatorInRange
              validatorCorrectAvailable).1
        · intro offset offsetAtMostNext
          by_cases inPrefix : offset ≤ previous
          · exact windowPrefix.sourceAt offset inPrefix
          · have previousBeforeOffset : previous < offset :=
              Nat.lt_of_not_ge inPrefix
            have nextAtMostOffset : previous + 1 ≤ offset :=
              Nat.succ_le_iff.mpr previousBeforeOffset
            have offsetIsNext : offset = previous + 1 :=
              Nat.le_antisymm (by simpa [Nat.succ_eq_add_one] using offsetAtMostNext)
                nextAtMostOffset
            subst offset
            simpa [Nat.add_assoc] using nextSources

/-- A concrete recovery-source prefix gives a later exact fresh timer-paced
window while the observed correct commit heads stay fixed.

One additional exact-next round converts a restart-safe source into a sent
proposal family. The returned window then discards its own warm-up family and
keeps only proposal snapshots whose timer starts are after the observed common
state. No proposal, timer, or future layer is an input. -/
theorem exact_recovery_source_window_gives_fresh_timer_paced_suffix
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (originRules : ValidatorBlockProgressProposalOriginRules
      (obligations := obligations) mode)
    (timingRules : ValidatorRecoveryProposalActionTimingRules timerSource)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot baseRound prefixCount freshCount : Time}
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (blockRecovery : ValidatorActiveBlockProgressRecoverySnapshot mode snapshot)
    (baseAtLeastRecoveryMaximum :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount ≤ baseRound)
    (window : ValidatorExactRecoveryRoundSourceWindow timed obligations pins
      modeRules.recoveryWait snapshot baseRound prefixCount)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    Nonempty (ValidatorFreshTimerPacedExactRoundWindow timed obligations waits
      snapshot (baseRound + prefixCount + 1) freshCount) := by
  have baseAtMostCurrent :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount ≤
        baseRound + prefixCount :=
    Nat.le_trans baseAtLeastRecoveryMaximum
      (Nat.le_add_right baseRound prefixCount)
  rcases stable_common_round_advances_or_gives_next_stable_common_round
      modeRules roundRules timerSource pacing arms latchSource effects pins
        capsuleSync acceptance representatives authorityCountAtLeastTwo
          recovery window.snapshotBeforeFinish baseAtMostCurrent window.owned
            window.stable active with advanced | next
  · exact False.elim (noAdvance advanced)
  · let first := Classical.choice next
    have baseAtMostFirst :
        correctValidatorRecoveryBaseMaximumUpTo faults
            (timed.execution.trace snapshot) config.authorityCount ≤
          baseRound + prefixCount + 1 :=
      Nat.le_trans baseAtMostCurrent (Nat.le_succ _)
    exact stable_common_round_gives_finite_fresh_timer_paced_window modeRules
      sameRecoveryWait roundRules originRules timingRules pacing arms latchSource
        effects pins capsuleSync acceptance representatives
          authorityCountAtLeastTwo recovery blockRecovery
            first.snapshotBeforeFinish baseAtMostFirst first.produced
              first.stable active noAdvance

/-- One fixed receiver has an exact above-GC quorum layer which is ready for
the next proposal. Every correct-author representative is accepted, retained,
and present in the local block catalog. -/
structure ValidatorReceiverUsableCorrectQuorumLayer
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (receiver round : Nat) : Prop where
  roundAboveGc : (world.validatorState receiver).gcRound < round
  correctStakeIsQuorum : config.thresholds.quorum ≤
    weight config.authorityCount config.stake faults.correctAvailable
  exactCorrectRepresentatives : ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ∃ reference block,
      (world.validatorState receiver).acceptedRepresentative round author =
          some reference ∧
        (world.validatorState receiver).accepted reference = true ∧
        (world.validatorState receiver).retained reference = true ∧
        world.blockCatalog reference.id = some block ∧
        block.reference = reference ∧
        reference.author = author ∧
        reference.round = round
  nextParentsReady : ValidatorRecoveryParentQuorumReadyAt config
    (world.validatorState receiver) (round + 1)

/-- A finite exact recovery source window together with its exact usable layer
at one fixed correct receiver. The source window keeps every concrete source
block. All offsets after zero keep the actual proposal and peer broadcasts. -/
structure ValidatorReceiverExactRecoverySourceWindow
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recoveryWait snapshot baseRound count receiver : Time) where
  sourceWindow : ValidatorExactRecoveryRoundSourceWindow timed obligations pins
    recoveryWait snapshot baseRound count
  finish : Time
  sourceFinishBeforeFinish : sourceWindow.finish ≤ finish
  usable : ValidatorReceiverUsableCorrectQuorumLayer config faults
    (timed.execution.trace finish) receiver (baseRound + count)

/-- A concrete exact source window either advances the queried receiver or
gives that receiver the exact accepted, retained, and catalogued quorum layer.

The alternative is local to `receiver`. Commit advances at other hosts do not
discharge the result. -/
theorem exact_recovery_source_window_reaches_receiver_or_receiver_advances
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {recoveryWait snapshot baseRound count receiver : Time}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins recoveryWait)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (window : ValidatorExactRecoveryRoundSourceWindow timed obligations pins
      recoveryWait snapshot baseRound count)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (roundAboveReceiverGc :
      ((timed.execution.trace snapshot).validatorState receiver).gcRound <
        baseRound + count)
    (afterGst : network.gst ≤ snapshot)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorReceiverCommitAdvance timed snapshot receiver ∨
      Nonempty (ValidatorReceiverExactRecoverySourceWindow timed obligations
        pins recoveryWait snapshot baseRound count receiver) := by
  by_cases receiverAdvanced :
      ValidatorReceiverCommitAdvance timed snapshot receiver
  · exact Or.inl receiverAdvanced
  · let sources := window.sourceAt count (Nat.le_refl count)
    rcases
        exact_recovery_round_source_family_reaches_receiver_or_receiver_advances
          pins broadcast effects capsuleSync acceptance representatives sources
            receiverInRange receiverCorrectAvailable roundAboveReceiverGc
              afterGst active with
      laterAdvance | stable
    · exact False.elim (receiverAdvanced laterAdvance)
    · right
      rcases stable with ⟨readyAt, snapshotBeforeReady, stable⟩
      let finish := max window.finish readyAt
      have sourceFinishBeforeFinish : window.finish ≤ finish :=
        Nat.le_max_left _ _
      have readyBeforeFinish : readyAt ≤ finish := Nat.le_max_right _ _
      have finalRoundAboveGc :
          ((timed.execution.trace finish).validatorState receiver).gcRound <
            baseRound + count := by
        rw [no_receiver_commit_advance_keeps_gc_round receiverInRange
          receiverCorrectAvailable
            (Nat.le_trans snapshotBeforeReady readyBeforeFinish)
              receiverAdvanced]
        exact roundAboveReceiverGc
      have accepted : ∀ author,
          author < config.authorityCount →
          faults.correctAvailable author = true →
          (((timed.execution.trace finish).validatorState receiver
            ).acceptedRepresentative (baseRound + count) author).isSome =
              true := by
        intro author authorInRange authorCorrectAvailable
        rcases stable finish readyBeforeFinish author authorInRange
            authorCorrectAvailable with
          ⟨reference, recorded, _retained⟩
        simp [recorded]
      have retained : ∀ author reference,
          author < config.authorityCount →
          faults.correctAvailable author = true →
          ((timed.execution.trace finish).validatorState receiver
            ).acceptedRepresentative (baseRound + count) author =
              some reference →
          ((timed.execution.trace finish).validatorState receiver).retained
              reference = true := by
        intro author reference authorInRange authorCorrectAvailable recorded
        rcases stable finish readyBeforeFinish author authorInRange
            authorCorrectAvailable with
          ⟨selected, selectedRecorded, selectedRetained⟩
        have sameReference : reference = selected :=
          Option.some.inj (recorded.symm.trans selectedRecorded)
        simpa [sameReference] using selectedRetained
      have nextParents :=
        receiver_retained_correct_round_gives_recovery_parent_quorum
          representatives receiverInRange receiverCorrectAvailable accepted
            retained finalRoundAboveGc
      refine ⟨{
        sourceWindow := window
        finish
        sourceFinishBeforeFinish
        usable := {
          roundAboveGc := finalRoundAboveGc
          correctStakeIsQuorum :=
            faults.correct_available_stake_is_quorum
          exactCorrectRepresentatives := ?_
          nextParentsReady := nextParents } }⟩
      intro author authorInRange authorCorrectAvailable
      rcases stable finish readyBeforeFinish author authorInRange
          authorCorrectAvailable with
        ⟨reference, recorded, retainedReference⟩
      have sound :=
        (timed.execution.statesWellFormed finish receiver receiverInRange
          ).acceptedRepresentativeIsSound (baseRound + count) author reference
            recorded
      rcases sound.2.2.2 with ⟨block, catalogued, blockReference⟩
      exact ⟨reference, block, recorded, sound.2.2.1, retainedReference,
        catalogued, blockReference, sound.1, sound.2.1⟩

/-- A stable common retained round gives one exact usable quorum layer at a
fixed correct receiver. -/
theorem stable_common_round_gives_receiver_usable_quorum_layer
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time receiver round : Nat}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
      timed.execution.trace time round)
    (roundAboveGc :
      ((timed.execution.trace time).validatorState receiver).gcRound < round) :
    ValidatorReceiverUsableCorrectQuorumLayer config faults
      (timed.execution.trace time) receiver round := by
  have accepted : EveryCorrectAvailableValidatorAccepted faults
      (timed.execution.trace time) round := by
    intro observer author observerInRange observerCorrect
      authorInRange authorCorrect
    rcases stable time (Nat.le_refl _) observer author observerInRange
        observerCorrect authorInRange authorCorrect with
      ⟨reference, recorded, _retained⟩
    simp [recorded]
  have retained : ∀ author reference,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ((timed.execution.trace time).validatorState receiver
        ).acceptedRepresentative round author = some reference →
      ((timed.execution.trace time).validatorState receiver).retained
        reference = true := by
    intro author reference authorInRange authorCorrect recorded
    rcases stable time (Nat.le_refl _) receiver author receiverInRange
        receiverCorrectAvailable authorInRange authorCorrect with
      ⟨selected, selectedRecorded, selectedRetained⟩
    have sameReference : reference = selected :=
      Option.some.inj (recorded.symm.trans selectedRecorded)
    simpa [sameReference] using selectedRetained
  have nextParents := common_retained_correct_round_gives_recovery_parent_quorum
    representatives receiverInRange receiverCorrectAvailable accepted retained
      roundAboveGc
  refine {
    roundAboveGc
    correctStakeIsQuorum := faults.correct_available_stake_is_quorum
    exactCorrectRepresentatives := ?_
    nextParentsReady := nextParents }
  intro author authorInRange authorCorrectAvailable
  rcases stable time (Nat.le_refl _) receiver author receiverInRange
      receiverCorrectAvailable authorInRange authorCorrectAvailable with
    ⟨reference, recorded, retained⟩
  have sound :=
    (timed.execution.statesWellFormed time receiver receiverInRange
      ).acceptedRepresentativeIsSound round author reference recorded
  rcases sound.2.2.2 with ⟨block, catalogued, blockReference⟩
  exact ⟨reference, block, recorded, sound.2.2.1, retained, catalogued,
    blockReference, sound.1, sound.2.1⟩

/-- A stable exact round above every correct receiver's GC boundary is one
common usable correct quorum layer.

This is a stronger private recovery result. It keeps the accepted
representative, retained body, and catalog entry at each correct receiver for
the later favorable-window proof. The public network-DAG result needs only one
correct-held total quorum layer. -/
theorem stable_common_round_gives_common_usable_quorum_layer
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time round : Nat}
    (produced : ProducedCorrectQuorumLayer config faults
      (timed.execution.trace time) round)
    (stable : EveryCorrectAvailableValidatorAcceptedAndRetainedFrom faults
      timed.execution.trace time round)
    (roundAboveGc : ∀ receiver,
      receiver < config.authorityCount →
      faults.correctAvailable receiver = true →
      ((timed.execution.trace time).validatorState receiver).gcRound < round) :
    CommonUsableCorrectQuorumLayer config faults
      (timed.execution.trace time) round := by
  refine ⟨VoterSet.inter faults.correctAvailable
      ((timed.execution.trace time).producedAuthors round),
    produced, ?_, ?_⟩
  · intro author _authorInRange authorSelected
    exact authorSelected
  intro receiver receiverInRange receiverCorrectAvailable
  refine ⟨roundAboveGc receiver receiverInRange receiverCorrectAvailable, ?_⟩
  intro author authorInRange authorSelected
  simp only [VoterSet.inter, ValidatorWorldState.producedAuthors,
    Bool.and_eq_true] at authorSelected
  rcases stable time (Nat.le_refl _) receiver author receiverInRange
      receiverCorrectAvailable authorInRange authorSelected.1 with
    ⟨reference, recorded, retained⟩
  have sound :=
    (timed.execution.statesWellFormed time receiver receiverInRange
      ).acceptedRepresentativeIsSound round author reference recorded
  rcases sound.2.2.2 with ⟨block, catalogued, blockReference⟩
  exact ⟨reference, block, recorded, sound.2.2.1, retained, catalogued,
    blockReference, sound.1, sound.2.1⟩

/-- A restart-safe recovery source family gives one exact usable quorum layer
at a fixed correct receiver after any requested finite number of rounds.

The result keeps the concrete source family for every offset. The public DAG
progress theorem can erase this evidence only after the commit proof has used
the exact block references and proposal parents. -/
theorem exact_recovery_round_source_family_gives_receiver_usable_round
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins
      modeRules.recoveryWait)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {snapshot baseRound count receiver : Time}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (recovery : ValidatorActiveRecoverySnapshot timed modeRules.recoveryWait
      snapshot)
    (baseAtLeastRecoveryMaximum :
      correctValidatorRecoveryBaseMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount ≤ baseRound)
    (baseSources :
      EveryCorrectAvailableValidatorExactRecoveryRoundSource
        (obligations := obligations) pins modeRules.recoveryWait snapshot
          baseRound)
    (active : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed snapshot) :
    ∃ window : ValidatorExactRecoveryRoundSourceWindow timed obligations pins
        modeRules.recoveryWait snapshot baseRound count,
      ValidatorReceiverUsableCorrectQuorumLayer config faults
        (timed.execution.trace window.finish) receiver (baseRound + count) := by
  let window : ValidatorExactRecoveryRoundSourceWindow timed obligations pins
      modeRules.recoveryWait snapshot baseRound count := Classical.choice
    (exact_recovery_round_source_family_gives_finite_window modeRules
      roundRules timerSource pacing arms latchSource effects pins broadcast
        capsuleSync acceptance representatives authorityCountAtLeastTwo
          (count := count) recovery baseAtLeastRecoveryMaximum baseSources
            active noAdvance)
  have initialGcBelowBase :
      ((timed.execution.trace snapshot).validatorState receiver).gcRound <
        baseRound := by
    have localBounds := correct_validator_floor_and_gc_below_recovery_base
      (world := timed.execution.trace snapshot) receiverInRange
        receiverCorrectAvailable
    exact Nat.lt_of_lt_of_le localBounds.2 baseAtLeastRecoveryMaximum
  have sameGc := no_commit_advance_keeps_correct_gc_round receiverInRange
    receiverCorrectAvailable window.snapshotBeforeFinish noAdvance
  have finalGcBelowRound :
      ((timed.execution.trace window.finish).validatorState receiver).gcRound <
        baseRound + count := by
    rw [sameGc]
    exact Nat.lt_of_lt_of_le initialGcBelowBase
      (Nat.le_add_right baseRound count)
  exact ⟨window,
    stable_common_round_gives_receiver_usable_quorum_layer representatives
      receiverInRange receiverCorrectAvailable window.stable
        finalGcBelowRound⟩

/-- Fundamental one-host recovery rules give network DAG progress or an
actual correct-host commit-index advance.

The proof selects the recovery time and common base only internally. In the
stable-head branch, every correct host first creates a source at its local
genesis, durable-tip, or post-GC safe-resume base. A proof-selected correct
source then brings all correct hosts to one common base. Exact-next recovery
extends that base far enough to meet the requested minimum round. -/
theorem recovery_inputs_give_network_dag_or_commit_progress
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins
      modeRules.recoveryWait)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (genesis : ValidatorCanonicalGenesisParentRules timed)
    (installed : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms
      modeRules.recoveryWait)
    (needRules : ValidatorBlockProgressRecoveryNeedRules mode needs)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    (authorityCountAtLeastTwo : 1 < config.authorityCount) :
    NetworkDagOrCommitProgressLiveness config faults network
      timed.execution.trace := by
  intro start minimumRound afterGst active
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed start
  · exact Or.inl advanced
  · right
    rcases no_commit_advance_gives_active_recovery_snapshot modeRules afterGst
        active advanced with
      ⟨snapshot, startBeforeSnapshot, recovery⟩
    have activeFromSnapshot : ∀ time, snapshot ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time snapshotBeforeTime
      exact active time (Nat.le_trans startBeforeSnapshot snapshotBeforeTime)
    have noAdvanceAtSnapshot :
        ¬SomeCorrectAvailableCommitAdvance timed snapshot :=
      no_commit_advance_persists_to_later_start startBeforeSnapshot advanced
    have blockRecovery : ValidatorActiveBlockProgressRecoverySnapshot mode
        (snapshot + 1) :=
      active_stall_snapshot_latches_block_progress_recovery modeRules mode
        sameRecoveryWait recovery activeFromSnapshot noAdvanceAtSnapshot
    have recoveryAtNext : ValidatorActiveRecoverySnapshot timed
        modeRules.recoveryWait (snapshot + 1) :=
      { afterGst := Nat.le_trans recovery.afterGst (Nat.le_succ snapshot)
        recovering := by
          intro validator validatorInRange validatorCorrectAvailable
          exact active_recovery_snapshot_persists_without_commit_advance
            modeRules recovery validatorInRange validatorCorrectAvailable
              (Nat.le_succ snapshot) activeFromSnapshot noAdvanceAtSnapshot }
    have activeFromNext : ∀ time, snapshot + 1 ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time nextBeforeTime
      exact activeFromSnapshot time
        (Nat.le_trans (Nat.le_succ snapshot) nextBeforeTime)
    have noAdvanceAtNext :
        ¬SomeCorrectAvailableCommitAdvance timed (snapshot + 1) :=
      no_commit_advance_persists_to_later_start (Nat.le_succ snapshot)
        noAdvanceAtSnapshot
    let localSources :=
      active_block_progress_recovery_gives_local_base_source_family modeRules
        roundRules timerSource pacing arms latchSource effects pins genesis
          installed needs representatives needRules authorityCountAtLeastTwo
            recoveryAtNext blockRecovery activeFromNext noAdvanceAtNext
    let baseRound := correctValidatorRecoveryBaseMaximumUpTo faults
      (timed.execution.trace (snapshot + 1)) config.authorityCount
    have commonSources :
        EveryCorrectAvailableValidatorExactRecoveryRoundSource
          (obligations := obligations) pins modeRules.recoveryWait
            (snapshot + 1) baseRound := by
      simpa [baseRound] using
        (local_base_source_family_gives_common_base_source_family modeRules
          roundRules timerSource pacing arms latchSource effects pins broadcast
            capsuleSync acceptance authorityCountAtLeastTwo recoveryAtNext
              localSources activeFromNext noAdvanceAtNext)
    let window := Classical.choice
      (exact_recovery_round_source_family_gives_finite_window modeRules
        roundRules timerSource pacing arms latchSource effects pins broadcast
          capsuleSync acceptance representatives authorityCountAtLeastTwo
            (count := minimumRound) recoveryAtNext (Nat.le_refl baseRound)
              commonSources activeFromNext noAdvanceAtNext)
    refine ⟨window.finish, baseRound + minimumRound, ?_, ?_, ?_, ?_⟩
    · exact Nat.le_trans startBeforeSnapshot
        (Nat.le_trans (Nat.le_succ snapshot) window.snapshotBeforeFinish)
    · exact Nat.le_add_left minimumRound baseRound
    · exact every_correct_own_block_gives_produced_quorum_layer window.owned
    · apply stable_common_round_gives_common_usable_quorum_layer
        (every_correct_own_block_gives_produced_quorum_layer window.owned)
          window.stable
      intro receiver receiverInRange receiverCorrectAvailable
      have sameGc := no_commit_advance_keeps_correct_gc_round
        receiverInRange receiverCorrectAvailable window.snapshotBeforeFinish
          noAdvanceAtNext
      rw [sameGc]
      have localBound := correct_validator_floor_and_gc_below_recovery_base
        (world := timed.execution.trace (snapshot + 1)) receiverInRange
          receiverCorrectAvailable
      exact Nat.lt_of_lt_of_le localBound.2
        (Nat.le_add_right baseRound minimumRound)

/-- Current one-host recovery and bootstrap rules derive a requested rich
fresh proposal window on a suffix with fixed correct commit heads.

The theorem starts from an arbitrary post-GST state. It derives the recovery
snapshot, each local genesis/tip/post-GC source, the proof-only common base,
and a finite source prefix. One additional produced round turns the restart
source into a sent proposal family. The returned late window keeps every exact
block, timer-paced proposal snapshot, and refreshed parent list.

The `noAdvance` premise is the precise remaining boundary. The final pure DAG
theorem must remove it with finite commit-handler return and proposal
non-interference. It is not an end-to-end input. -/
theorem recovery_inputs_give_fresh_timer_paced_window_on_stable_head
    {BlockId CommitId PacketId Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (modeRules : ValidatorCommitProgressRecoveryModeRules timed)
    (sameRecoveryWait : thresholds.recoveryWait = modeRules.recoveryWait)
    (roundRules : ValidatorCommitProgressProposalRoundRules timed
      modeRules.recoveryWait)
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (originRules : ValidatorBlockProgressProposalOriginRules
      (obligations := obligations) mode)
    (timingRules : ValidatorRecoveryProposalActionTimingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (broadcast : ValidatorRecoveryTipRebroadcastExecution pins
      modeRules.recoveryWait)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (genesis : ValidatorCanonicalGenesisParentRules timed)
    (installed : ValidatorInstalledHeadBootstrapSourceMap functions timed)
    (needs : ValidatorRecoveryParentNeedExecution pins arms
      modeRules.recoveryWait)
    (needRules : ValidatorBlockProgressRecoveryNeedRules mode needs)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start minimumRound count : Time}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ∃ observation baseRound,
      start ≤ observation ∧
        minimumRound ≤ baseRound ∧
        Nonempty (ValidatorFreshTimerPacedExactRoundWindow timed obligations
          waits observation baseRound count) := by
  rcases no_commit_advance_gives_active_recovery_snapshot modeRules afterGst
      active noAdvance with
    ⟨snapshot, startBeforeSnapshot, recovery⟩
  have activeFromSnapshot : ∀ time, snapshot ≤ time →
      (timed.execution.trace time).epochActive = true := by
    intro time snapshotBeforeTime
    exact active time (Nat.le_trans startBeforeSnapshot snapshotBeforeTime)
  have noAdvanceAtSnapshot :
      ¬SomeCorrectAvailableCommitAdvance timed snapshot :=
    no_commit_advance_persists_to_later_start startBeforeSnapshot noAdvance
  have blockRecovery : ValidatorActiveBlockProgressRecoverySnapshot mode
      (snapshot + 1) :=
    active_stall_snapshot_latches_block_progress_recovery modeRules mode
      sameRecoveryWait recovery activeFromSnapshot noAdvanceAtSnapshot
  have recoveryAtNext : ValidatorActiveRecoverySnapshot timed
      modeRules.recoveryWait (snapshot + 1) :=
    { afterGst := Nat.le_trans recovery.afterGst (Nat.le_succ snapshot)
      recovering := by
        intro validator validatorInRange validatorCorrectAvailable
        exact active_recovery_snapshot_persists_without_commit_advance
          modeRules recovery validatorInRange validatorCorrectAvailable
            (Nat.le_succ snapshot) activeFromSnapshot noAdvanceAtSnapshot }
  have activeFromNext : ∀ time, snapshot + 1 ≤ time →
      (timed.execution.trace time).epochActive = true := by
    intro time nextBeforeTime
    exact activeFromSnapshot time
      (Nat.le_trans (Nat.le_succ snapshot) nextBeforeTime)
  have noAdvanceAtNext :
      ¬SomeCorrectAvailableCommitAdvance timed (snapshot + 1) :=
    no_commit_advance_persists_to_later_start (Nat.le_succ snapshot)
      noAdvanceAtSnapshot
  let localSources :=
    active_block_progress_recovery_gives_local_base_source_family modeRules
      roundRules timerSource pacing arms latchSource effects pins genesis
        installed needs representatives needRules authorityCountAtLeastTwo
          recoveryAtNext blockRecovery activeFromNext noAdvanceAtNext
  let initialBase := correctValidatorRecoveryBaseMaximumUpTo faults
    (timed.execution.trace (snapshot + 1)) config.authorityCount
  have commonSources :
      EveryCorrectAvailableValidatorExactRecoveryRoundSource
        (obligations := obligations) pins modeRules.recoveryWait
          (snapshot + 1) initialBase := by
    simpa [initialBase] using
      (local_base_source_family_gives_common_base_source_family modeRules
        roundRules timerSource pacing arms latchSource effects pins broadcast
          capsuleSync acceptance authorityCountAtLeastTwo recoveryAtNext
            localSources activeFromNext noAdvanceAtNext)
  let sourceWindow := Classical.choice
    (exact_recovery_round_source_family_gives_finite_window modeRules
      roundRules timerSource pacing arms latchSource effects pins broadcast
        capsuleSync acceptance representatives authorityCountAtLeastTwo
          (count := minimumRound) recoveryAtNext (Nat.le_refl initialBase)
            commonSources activeFromNext noAdvanceAtNext)
  have rich := exact_recovery_source_window_gives_fresh_timer_paced_suffix
    modeRules sameRecoveryWait roundRules originRules timingRules pacing arms
      latchSource effects pins capsuleSync acceptance representatives
        authorityCountAtLeastTwo recoveryAtNext blockRecovery
          (Nat.le_refl initialBase) sourceWindow activeFromNext noAdvanceAtNext
          (freshCount := count)
  refine ⟨snapshot + 1, initialBase + minimumRound + 1,
    Nat.le_trans startBeforeSnapshot (Nat.le_succ snapshot), ?_, ?_⟩
  · exact Nat.le_trans (Nat.le_add_left minimumRound initialBase)
      (Nat.le_succ _)
  · exact rich

/-- A reached signer-floor target gives the exact durable own block and send
record which cross that target. -/
theorem signer_floor_target_reached_gives_sent_block
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (obligations : ValidatorProposalObligationExecution timed)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start reachedAt target validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (startBeforeReached : start ≤ reachedAt)
    (targetAboveStart :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < target)
    (targetReached : target ≤
      ((timed.execution.trace reachedAt).validatorState
        validator).highestSignedRound) :
    ∃ finish round,
      start ≤ finish ∧
        target ≤ round ∧
        ((timed.execution.trace start).validatorState
            validator).highestSignedRound < round ∧
        (((timed.execution.trace finish).validatorState
            validator).ownBlockAt round).isSome = true ∧
        ((timed.execution.trace finish).validatorState
            validator).sentOwnBlockAt round = true := by
  rcases signer_floor_target_reached_has_target_persist_proposal
      timed.execution startBeforeReached targetAboveStart targetReached with
    ⟨persistTime, block, startBeforePersist, _persistBeforeReached,
      persisted, blockReaches⟩
  rcases persist_proposal_occurrence_eventually_sends_block obligations
      authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
      persisted with
    ⟨finish, persistBeforeFinish, stored, sent, _floor⟩
  refine ⟨finish, block.reference.round,
    Nat.le_trans startBeforePersist (Nat.le_trans (Nat.le_succ _)
      persistBeforeFinish), blockReaches, ?_, ?_, sent⟩
  · exact Nat.lt_of_lt_of_le targetAboveStart blockReaches
  · simp [stored]

/-- The strict timer-paced window used by the direct-vote and commit proofs.
It keeps one exact proposal snapshot for every correct author in every round. -/
def ValidatorStrictRecoveryWindowResult
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
    (snapshot finish baseRound count : Nat) : Prop :=
  snapshot ≤ finish ∧
  ∀ offset,
    offset < count →
    EveryCorrectAvailableValidatorTimerPacedRoundWithin timed waits snapshot
        finish (baseRound + offset) ∧
    EveryCorrectAvailableValidatorProduced faults (timed.execution.trace finish)
        (baseRound + offset) ∧
    EveryCorrectAvailableValidatorAccepted faults (timed.execution.trace finish)
        (baseRound + offset) ∧
    ProducedCorrectQuorumLayer config faults (timed.execution.trace finish)
        (baseRound + offset) ∧
    CommonAcceptedCorrectQuorumLayer config faults (timed.execution.trace finish)
        (baseRound + offset)

/-- Erase the timer snapshots from a strict window while keeping the public
produced, accepted, and quorum-layer facts. -/
theorem strict_recovery_window_gives_layer_window
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
    {snapshot finish baseRound count : Nat}
    (strict : ValidatorStrictRecoveryWindowResult timed waits snapshot finish
      baseRound count) :
    snapshot ≤ finish ∧
      ∀ offset,
        offset < count →
        EveryCorrectAvailableValidatorProduced faults
            (timed.execution.trace finish) (baseRound + offset) ∧
          EveryCorrectAvailableValidatorAccepted faults
            (timed.execution.trace finish) (baseRound + offset) ∧
          ProducedCorrectQuorumLayer config faults
            (timed.execution.trace finish) (baseRound + offset) ∧
          CommonAcceptedCorrectQuorumLayer config faults
            (timed.execution.trace finish) (baseRound + offset) := by
  refine ⟨strict.1, ?_⟩
  intro offset offsetInRange
  exact (strict.2 offset offsetInRange).2

end Mysticeti
