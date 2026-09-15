/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorProposalLatchBridge
import Mysticeti.ValidatorRecoveryBroadcastParentSync
import Mysticeti.ValidatorRecoveryTimerDerivation

namespace Mysticeti

/-! Strict recovery proposal results for the main validator trace.

The canonical path in this module uses the recovery-origin parent rule and the
durable proposal latch. It does not call the older direct proposal pipeline.
The result keeps the exact block, parent list, persistence event, and addressed
broadcast from the main execution.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- One-host source mapping for commit-progress proposal pacing.

Every actual `proposeNext` step must use the current timer record that was made
by the recovery timer worker. Thus, another local proposal path cannot use the
same recovery action without first using that timer. This rule is about the
source of one local action. It does not state that an action, block, layer, or
commit occurs in the future. -/
structure ValidatorCommitProgressProposalPacingRules
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits) : Prop where
  proposeNextUsesCurrentTimer : ∀ time
      (before after : ValidatorWorldState BlockId CommitId PacketId)
      validator parents,
    ValidatorAtomicStep config faults protocolPacket program time before
        (.localAction validator (.proposeNext parents)) after →
    ∃ start : ValidatorRecoveryTimerStart BlockId CommitId,
      timerSource.timerStarted start ∧
        start.validator = validator ∧
        timerSource.refreshedParents start (start.deadline waits) = parents ∧
        start.commitHead = (before.validatorState validator).commitHead ∧
        (before.validatorState validator).recovery =
          some (start.recovery waits)
  /-- The action that completes protected proposal work uses the timer that
  protected that work. A restored replacement timer creates new work. -/
  protectedProposalUsesReadyTimer : ∀
      {commitHead : ValidatorCommitHead CommitId}
      {targetRound validator time : Nat}
      (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
        targetRound validator)
      (start : ValidatorRecoveryTimerStart BlockId CommitId),
    timerSource.timerStarted start →
    start.validator = validator →
    timerSource.refreshedParents start (start.deadline waits) = ready.parents →
    ready.startedAt + waits.wait commitHead targetRound ≤ time →
    time ≤ ready.startedAt + waits.wait commitHead targetRound +
      timed.localActionBound →
    start.startedAt = ready.startedAt ∧
      start.commitHead = commitHead ∧
      start.targetRound = targetRound

/-- Exact local facts at one actual paced recovery proposal. -/
structure ValidatorPacedRecoveryProposalOccurrence
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (time validator : Nat) (parents : List (ValidatorBlockRef BlockId)) where
  actionState : ValidatorWorldState BlockId CommitId PacketId
  start : ValidatorRecoveryTimerStart BlockId CommitId
  timerStarted : timerSource.timerStarted start
  startValidator : start.validator = validator
  refreshedParents :
    timerSource.refreshedParents start (start.deadline waits) = parents
  currentCommitHead :
    start.commitHead = (actionState.validatorState validator).commitHead
  currentTimer :
    (actionState.validatorState validator).recovery = some (start.recovery waits)
  exactNext :
    start.targetRound =
      (actionState.validatorState validator).highestSignedRound + 1
  deadlineExpired :
    start.deadline waits ≤ (actionState.validatorState validator).clock
  parentsReady : ValidatorParentListReady config
    (actionState.validatorState validator) start.targetRound parents

/-- An actual recovery proposal cannot bypass the armed pacing timer.

The action's basic guard supplies the exact-next, deadline, and parent checks.
The pacing source rule supplies the timer that created the current recovery
record. -/
theorem actual_propose_next_uses_current_armed_recovery_timer
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    {time validator : Nat} {parents : List (ValidatorBlockRef BlockId)}
    (occurs : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.proposeNext parents)) :
    Nonempty (ValidatorPacedRecoveryProposalOccurrence timerSource time validator
      parents) := by
  rcases validator_world_step_local_action_with_suffix
      (timed.execution.stepsFollowRules time) occurs with
    ⟨actionState, _actionAfter, _suffix, actionStep, _suffixStep⟩
  rcases pacing.proposeNextUsesCurrentTimer time actionState _ validator parents
      actionStep with
    ⟨start, started, startValidator, refreshedParents, currentHead,
      currentTimer⟩
  have guard := validator_atomic_local_action_has_basic_guard actionStep
  change ∃ recovery,
      (actionState.validatorState validator).recovery = some recovery ∧
        recovery.alignmentWitness = none ∧
        recovery.targetRound =
          (actionState.validatorState validator).highestSignedRound + 1 ∧
        (∃ deadline,
          recovery.deadline = some deadline ∧
            deadline ≤ (actionState.validatorState validator).clock) ∧
        ValidatorParentListReady config
          (actionState.validatorState validator) recovery.targetRound parents at guard
  rcases guard with
    ⟨recovery, recoveryAtAction, _noAlignment, exactNext,
      ⟨deadline, deadlineStored, deadlineExpired⟩, parentsReady⟩
  have sameRecovery : recovery = start.recovery waits := by
    exact Option.some.inj (recoveryAtAction.symm.trans currentTimer)
  subst recovery
  have exactDeadline : deadline = start.deadline waits := by
    simpa [ValidatorRecoveryTimerStart.recovery] using
      (Option.some.inj deadlineStored).symm
  subst deadline
  exact ⟨
    { actionState
      start
      timerStarted := started
      startValidator
      refreshedParents
      currentCommitHead := currentHead
      currentTimer
      exactNext
      deadlineExpired
      parentsReady }⟩

/-- The exact main-trace result of one strict recovery proposal and one
addressed broadcast. This is a theorem result, not a liveness input. -/
structure ValidatorStrictRecoveryBroadcast
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (obligations : ValidatorProposalObligationExecution timed)
    (validator receiver : Nat) where
  commitHead : ValidatorCommitHead CommitId
  targetRound : Nat
  ready : ValidatorRecoveryProposalReady faults timed waits commitHead
    targetRound validator
  refreshedParentList :
    ValidatorRefreshedRecoveryParentListAt config
      ((timed.execution.trace
        (ready.startedAt + waits.wait commitHead targetRound)).validatorState
          validator)
      targetRound ready.parents
  timerStartWithinLocalBound :
    ready.startedAt ≤ ready.parentReadyAt + timed.localActionBound + 2
  proposalActionAt : Time
  deadlineBeforeProposal :
    ready.startedAt + waits.wait commitHead targetRound ≤ proposalActionAt
  proposalWithinLocalBound :
    proposalActionAt ≤
      ready.startedAt + waits.wait commitHead targetRound +
        timed.localActionBound
  proposalOccurs : ValidatorLocalActionOccurs
    (timed.execution.events proposalActionAt) validator
    (.proposeNext ready.parents)
  pacedProposal : ValidatorPacedRecoveryProposalOccurrence timerSource
    proposalActionAt validator ready.parents
  pacedTimerStartedAt : pacedProposal.start.startedAt = ready.startedAt
  pacedTimerCommitHead : pacedProposal.start.commitHead = commitHead
  pacedTimerTargetRound : pacedProposal.start.targetRound = targetRound
  proposal : ValidatorReadyProposal BlockId
  snapshot : ValidatorProposalSnapshot config faults timed.execution.trace
  packetId : PacketId
  packet : AddressedPacket (ValidatorMessage BlockId CommitId)
  latched : ValidatorLatchedProposalResult timed obligations
    (proposalActionAt + 1) validator proposal
  completed : ValidatorCompletedRecoveryProposal ready snapshot
  broadcast : ValidatorCompletedProposalBroadcast snapshot receiver packetId
    packet
  recoveryOrigin : proposal.origin = .commitProgressRecovery
  exactBlock : proposal.block = snapshot.block
  persistTime : Time
  proposalBeforePersistence : proposalActionAt + 1 ≤ persistTime
  storedAfterPersistence : snapshot.storedAt = persistTime + 1
  persistenceOccurs : ValidatorLocalActionOccurs
    (timed.execution.events persistTime) validator
      (.persistProposal snapshot.block)
  storedWithinPipelinePrefix :
    snapshot.storedAt ≤
      ready.startedAt + waits.wait commitHead targetRound +
        2 * (timed.localActionBound + 1)

/-- One origin-aware protected proposal becomes one exact stored recovery
proposal through the durable latch.

The extra receiver is used only to obtain the already proved persistence and
broadcast result from the latch. It is not an anchor or quorum input. -/
theorem origin_aware_proposal_latch_builds_recovery_snapshot
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound validator receiver : Nat}
    {obligations : ValidatorProposalObligationExecution timed}
    (source : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound validator)
    (refreshedParentList :
      ValidatorRefreshedRecoveryParentListAt config
        ((timed.execution.trace
          (ready.startedAt + waits.wait commitHead targetRound)).validatorState
            validator)
        targetRound ready.parents)
    (receiverInRange : receiver < config.authorityCount)
    (differentReceiver : receiver ≠ validator) :
    ∃ proposalActionAt persistTime proposal snapshot packetId packet,
      ready.startedAt + waits.wait commitHead targetRound ≤ proposalActionAt ∧
        proposalActionAt ≤
          ready.startedAt + waits.wait commitHead targetRound +
            timed.localActionBound ∧
        ValidatorLocalActionOccurs
          (timed.execution.events proposalActionAt) validator
          (.proposeNext ready.parents) ∧
        Nonempty (ValidatorPacedRecoveryProposalOccurrence timerSource
          proposalActionAt validator ready.parents) ∧
        (∀ paced : ValidatorPacedRecoveryProposalOccurrence timerSource
            proposalActionAt validator ready.parents,
          paced.start.startedAt = ready.startedAt ∧
            paced.start.commitHead = commitHead ∧
            paced.start.targetRound = targetRound) ∧
        ValidatorLatchedProposalResult timed obligations
          (proposalActionAt + 1) validator proposal ∧
        ValidatorCompletedRecoveryProposal ready snapshot ∧
        ValidatorCompletedProposalBroadcast snapshot receiver packetId packet ∧
        proposal.origin = .commitProgressRecovery ∧
        proposal.block = snapshot.block ∧
        proposalActionAt + 1 ≤ persistTime ∧
        snapshot.storedAt = persistTime + 1 ∧
        ValidatorLocalActionOccurs (timed.execution.events persistTime)
          validator (.persistProposal snapshot.block) := by
  let deadline := ready.startedAt + waits.wait commitHead targetRound
  have originParentsReady := refreshedParentList.ready
  rcases enabled_propose_next_completes_exact_latched_pipeline source
      effects ready.validatorInRange
      ready.validatorCorrectAvailable ready.proposalProtected with
    ⟨proposalActionAt, proposal, deadlineBeforeProposal,
      proposalWithinBound, proposalOccurs, latchedAt, proposalOrigin,
      proposalAuthor, proposalParents, proposalRoundAtDeadline, latched⟩
  have broadcastExists := latched.2.2.2.2.2.2 receiver receiverInRange
    differentReceiver
  rcases broadcastExists with ⟨broadcast⟩
  have pacedProposal :=
    actual_propose_next_uses_current_armed_recovery_timer pacing proposalOccurs
  have pacedTimerMatches : ∀ paced :
      ValidatorPacedRecoveryProposalOccurrence timerSource proposalActionAt
        validator ready.parents,
      paced.start.startedAt = ready.startedAt ∧
        paced.start.commitHead = commitHead ∧
        paced.start.targetRound = targetRound := by
    intro paced
    exact pacing.protectedProposalUsesReadyTimer ready paced.start
      paced.timerStarted paced.startValidator paced.refreshedParents
      deadlineBeforeProposal proposalWithinBound
  have targetRoundAtDeadline : targetRound =
      ((timed.execution.trace deadline).validatorState
        validator).highestSignedRound + 1 := by
    have enabled := timed.protectedActionIsEnabled deadline validator
      (.proposeNext ready.parents) ready.proposalProtected
    rcases enabled.2.1 with
      ⟨recovery, recoveryAtDeadline, _noAlignment, recoveryTarget,
        _expired, _parents⟩
    have sameRecovery : recovery = ready.recovery := by
      have readyRecoveryAtDeadline :
          ((timed.execution.trace deadline).validatorState validator).recovery =
            some ready.recovery := by
        simpa [deadline] using ready.recoveryAtDeadline
      rw [readyRecoveryAtDeadline] at recoveryAtDeadline
      exact Option.some.inj recoveryAtDeadline.symm
    subst recovery
    rw [ready.recoveryTarget] at recoveryTarget
    exact recoveryTarget
  let snapshot : ValidatorProposalSnapshot config faults timed.execution.trace :=
    { proposer := validator
      snapshotAt := deadline
      storedAt := broadcast.persistedAt + 1
      block := proposal.block
      proposerInRange := ready.validatorInRange
      proposerCorrectAvailable := ready.validatorCorrectAvailable
      snapshotBeforeStore := by
        exact Nat.le_trans deadlineBeforeProposal
          (Nat.le_trans (Nat.le_add_right proposalActionAt 1)
            (Nat.le_trans broadcast.readyBeforePersistence
              (Nat.le_add_right broadcast.persistedAt 1)))
      recoveryTargetsProposalRound := by
        refine ⟨ready.recovery, ?_, ?_, ready.recoveryHasNoAlignment⟩
        · simpa [deadline] using ready.recoveryAtDeadline
        · have parentsAtDeadline := originParentsReady.1
          have parentsNonempty :=
            validator_parent_list_ready_nonempty parentsAtDeadline
          rcases List.exists_mem_of_ne_nil ready.parents parentsNonempty with
            ⟨parent, parentIncluded⟩
          have parentTargetsReady :=
            (parentsAtDeadline.2.1 parent parentIncluded).1
          have parentInProposal : parent ∈ proposal.block.parents := by
            rw [proposalParents]
            exact parentIncluded
          have parentTargetsProposal :=
            (latched.2.1.1.2.1 parent parentInProposal).1
          rw [ready.recoveryTarget]
          omega
      blockIsOwnProposal := proposalAuthor
      blockStored := broadcast.ownBlockStored
      blockInCatalog := broadcast.proposalCataloged
      parentAuthorsNodup := by
        change (proposal.block.parents.map ValidatorBlockRef.author).Nodup
        rw [proposalParents]
        exact originParentsReady.1.1
      parentsAreImmediate := by
        intro parent parentIncluded
        have parentInReady : parent ∈ ready.parents := by
          rw [← proposalParents]
          exact parentIncluded
        have targetsReady :=
          (originParentsReady.1.2.1 parent parentInReady).1
        rw [proposalRoundAtDeadline]
        rw [targetRoundAtDeadline] at targetsReady
        simpa [deadline] using targetsReady
      parentsAcceptedAtSnapshot := by
        intro parent parentIncluded
        have parentInReady : parent ∈ ready.parents := by
          rw [← proposalParents]
          exact parentIncluded
        simpa [deadline] using
          (originParentsReady.1.2.1 parent parentInReady).2 }
  have snapshotRound : snapshot.block.reference.round = targetRound := by
    dsimp [snapshot]
    rw [proposalRoundAtDeadline]
    simpa [deadline] using targetRoundAtDeadline.symm
  have validParents : snapshot.block.HasQuorumImmediateParents config := by
    refine ⟨snapshot.parentAuthorsNodup, snapshot.parentsAreImmediate, ?_⟩
    change config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (validatorParentAuthors proposal.block.parents)
    have quorum := originParentsReady.1.2.2
    rw [proposalParents]
    exact quorum
  have storedWithinPrefix : snapshot.storedAt ≤
      ready.startedAt + waits.wait commitHead targetRound +
        2 * (timed.localActionBound + 1) := by
    dsimp [snapshot, deadline]
    calc
      broadcast.persistedAt + 1 ≤
          (proposalActionAt + 1 + timed.localActionBound) + 1 :=
        Nat.add_le_add_right broadcast.persistenceWithinBound 1
      _ ≤
          (deadline + timed.localActionBound + 1 +
              timed.localActionBound) + 1 := by
        exact Nat.add_le_add_right
          (Nat.add_le_add_right
            (Nat.add_le_add_right proposalWithinBound 1)
            timed.localActionBound) 1
      _ ≤
          ready.startedAt + waits.wait commitHead targetRound +
            2 * (timed.localActionBound + 1) := by
        simp [deadline, Nat.mul_add, Nat.succ_mul, Nat.add_assoc,
          Nat.add_comm, Nat.add_left_comm]
        omega
  let completed : ValidatorCompletedRecoveryProposal ready snapshot :=
    { snapshotProposer := rfl
      snapshotAtDeadline := rfl
      snapshotRound := snapshotRound
      snapshotParents := by simpa [snapshot] using proposalParents
      validParents := validParents
      storedWithinPipelinePrefix := storedWithinPrefix }
  let completedBroadcast : ValidatorCompletedProposalBroadcast snapshot receiver
      broadcast.packetId broadcast.packet :=
    { packetInTrace := by
        simpa [broadcast.packetSentAt] using broadcast.packetInTrace
      packetIsProtocol := broadcast.packetIsProtocol
      packetSender := broadcast.packetSender
      packetReceiver := broadcast.packetReceiver
      packetPayload := by simpa [snapshot] using broadcast.packetPayload
      storedBeforeSend := by
        dsimp [snapshot]
        rw [broadcast.packetSentAt]
        exact Nat.le_trans broadcast.persistenceBeforeSend
          (Nat.le_add_right _ 1)
      sentWithinBound := by
        dsimp [snapshot]
        rw [broadcast.packetSentAt]
        exact Nat.add_le_add_right broadcast.sendWithinBound 1
      sentRecord := by
        simpa [snapshot, broadcast.packetSentAt] using
          broadcast.sentOwnBlockRecorded }
  exact ⟨proposalActionAt, broadcast.persistedAt, proposal, snapshot,
    broadcast.packetId,
    broadcast.packet, deadlineBeforeProposal, proposalWithinBound,
    proposalOccurs, pacedProposal,
    pacedTimerMatches, latched,
    completed, completedBroadcast, proposalOrigin, rfl,
    broadcast.readyBeforePersistence, rfl,
    broadcast.persistenceOccurs⟩

/-- Current one-validator recovery state produces one strict recovery proposal
or loses to a local commit-index advance.

This theorem does not take a timer, ready proposal, completed action, or block
as an input. The timer worker selects one retained exact-next parent list. The
bounded worker arms the timer. The recovery-origin latch then persists and
sends the exact proposal. -/
theorem ready_state_latches_recovery_snapshot_or_commit_race
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerSourceMap.ValidatorRecoveryTimerArmExecution
      timerSource)
    {obligations : ValidatorProposalObligationExecution timed}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (time validator receiver : Time)
    (input : ValidatorRecoveryTimerArmInputAt timed time validator)
    (active : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true)
    (receiverInRange : receiver < config.authorityCount)
    (differentReceiver : receiver ≠ validator) :
    ((∃ finish,
        time ≤ finish ∧
          ((timed.execution.trace time).validatorState
              validator).commitHead.index <
            ((timed.execution.trace finish).validatorState
              validator).commitHead.index) ∨
      ∃ (commitHead : ValidatorCommitHead CommitId) (targetRound : Nat)
          (ready : ValidatorRecoveryProposalReady faults timed waits commitHead
            targetRound validator)
          (proposalActionAt persistTime : Time)
          (proposal : ValidatorReadyProposal BlockId)
          (snapshot : ValidatorProposalSnapshot config faults
            timed.execution.trace)
          (packetId : PacketId)
          (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
        ready.startedAt + waits.wait commitHead targetRound ≤ proposalActionAt ∧
        proposalActionAt ≤
          ready.startedAt + waits.wait commitHead targetRound +
            timed.localActionBound ∧
        commitHead =
          ((timed.execution.trace time).validatorState validator).commitHead ∧
        targetRound =
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound + 1 ∧
        time ≤ ready.startedAt ∧
        ready.startedAt ≤ time + timed.localActionBound + 2 ∧
        ready.startedAt ≤
          ready.parentReadyAt + timed.localActionBound + 2 ∧
        ValidatorRefreshedRecoveryParentListAt config
          ((timed.execution.trace
            (ready.startedAt + waits.wait commitHead targetRound)).validatorState
              validator)
          targetRound ready.parents ∧
        ValidatorLocalActionOccurs
            (timed.execution.events proposalActionAt) validator
            (.proposeNext ready.parents) ∧
        Nonempty (ValidatorPacedRecoveryProposalOccurrence timerSource
            proposalActionAt validator ready.parents) ∧
        (∀ paced : ValidatorPacedRecoveryProposalOccurrence timerSource
            proposalActionAt validator ready.parents,
          paced.start.startedAt = ready.startedAt ∧
            paced.start.commitHead = commitHead ∧
            paced.start.targetRound = targetRound) ∧
        ValidatorLatchedProposalResult timed obligations
            (proposalActionAt + 1) validator proposal ∧
          ValidatorCompletedRecoveryProposal ready snapshot ∧
          ValidatorCompletedProposalBroadcast snapshot receiver packetId
            packet ∧
          proposal.origin = .commitProgressRecovery ∧
          proposal.block = snapshot.block ∧
          proposalActionAt + 1 ≤ persistTime ∧
          snapshot.storedAt = persistTime + 1 ∧
          ValidatorLocalActionOccurs (timed.execution.events persistTime)
            validator (.persistProposal snapshot.block)) := by
  rcases timerSource.recovery_state_and_active_suffix_derives_exact_ready_or_commit_advance
      arms time validator input active with readyResult | commitRace
  · right
    rcases readyResult with ⟨originReady, timeBeforeStart, startBound⟩
    rcases origin_aware_proposal_latch_builds_recovery_snapshot timed
        timerSource pacing latchSource effects originReady.ready
        originReady.refreshedRecoveryParents receiverInRange differentReceiver with
      ⟨proposalActionAt, persistTime, proposal, snapshot, packetId, packet,
        deadlineBeforeProposal, proposalWithinBound, proposalOccurs,
        pacedProposal,
        pacedTimerMatches, latched, completed, broadcast,
        proposalOrigin, proposalBlock, proposalBeforePersistence,
        storedAfterPersistence,
        persistenceOccurs⟩
    exact ⟨((timed.execution.trace time).validatorState validator).commitHead,
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1,
      originReady.ready, proposalActionAt,
      persistTime, proposal, snapshot,
      packetId, packet, deadlineBeforeProposal,
      proposalWithinBound,
      rfl, rfl,
      timeBeforeStart, startBound, originReady.startedWithinLocalBound,
      originReady.refreshedRecoveryParents,
      proposalOccurs, pacedProposal, pacedTimerMatches,
      latched, completed, broadcast, proposalOrigin, proposalBlock,
      proposalBeforePersistence, storedAfterPersistence, persistenceOccurs⟩
  · left
    exact commitRace

/-- Pack the canonical pointwise result for finite range composition. -/
theorem ready_state_builds_strict_recovery_broadcast_or_commit_advance
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    {obligations : ValidatorProposalObligationExecution timed}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (time validator receiver : Time)
    (input : ValidatorRecoveryTimerArmInputAt timed time validator)
    (active : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true)
    (receiverInRange : receiver < config.authorityCount)
    (differentReceiver : receiver ≠ validator) :
    ((∃ finish,
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
          time ≤ result.ready.startedAt ∧
          result.ready.startedAt ≤ time + timed.localActionBound + 2) := by
  rcases ready_state_latches_recovery_snapshot_or_commit_race timed timerSource
      pacing arms latchSource effects time validator receiver
      input active receiverInRange differentReceiver with
    commitRace | ⟨commitHead, targetRound, ready, proposalActionAt,
      persistTime, proposal, snapshot, packetId, packet, deadlineBeforeProposal,
      proposalWithinBound, headAtInput, targetAtInput, timeBeforeStart, startBound,
      timerStartWithinLocalBound, refreshedParentList, proposalOccurs,
      pacedProposal, pacedTimerMatches, latched, completed, broadcast, origin,
      exactBlock,
      proposalBeforePersistence, storedAfterPersistence, persistenceOccurs⟩
  · exact Or.inl commitRace
  · refine Or.inr ⟨
      { commitHead := commitHead
        targetRound := targetRound
        ready := ready
        refreshedParentList := refreshedParentList
        timerStartWithinLocalBound := timerStartWithinLocalBound
        proposalActionAt := proposalActionAt
        deadlineBeforeProposal := by
          exact deadlineBeforeProposal
        proposalWithinLocalBound := proposalWithinBound
        proposalOccurs := proposalOccurs
        pacedProposal := Classical.choice pacedProposal
        pacedTimerStartedAt :=
          (pacedTimerMatches (Classical.choice pacedProposal)).1
        pacedTimerCommitHead :=
          (pacedTimerMatches (Classical.choice pacedProposal)).2.1
        pacedTimerTargetRound :=
          (pacedTimerMatches (Classical.choice pacedProposal)).2.2
        proposal := proposal
        snapshot := snapshot
        packetId := packetId
        packet := packet
        latched := latched
        completed := completed
        broadcast := broadcast
        recoveryOrigin := origin
        exactBlock := exactBlock
        persistTime := persistTime
        proposalBeforePersistence := proposalBeforePersistence
        storedAfterPersistence := storedAfterPersistence
        persistenceOccurs := persistenceOccurs
        storedWithinPipelinePrefix := completed.storedWithinPipelinePrefix },
      targetAtInput, timeBeforeStart, startBound⟩

/-- An exact-next observation cannot follow persistence of the same proposed
round. Persistence stores the own block and raises the durable signer floor to
at least that round. -/
theorem strict_recovery_persistence_is_not_before_exact_next_observation
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {obligations : ValidatorProposalObligationExecution timed}
    {validator receiver observation : Nat}
    (result : ValidatorStrictRecoveryBroadcast timed waits timerSource
      obligations validator receiver)
    (targetAtObservation : result.targetRound =
      ((timed.execution.trace observation).validatorState
        validator).highestSignedRound + 1) :
    observation ≤ result.persistTime := by
  by_cases ordered : observation ≤ result.persistTime
  · exact ordered
  · have persistedBeforeObservation : result.persistTime + 1 ≤ observation := by
      exact Nat.succ_le_iff.mpr (Nat.lt_of_not_ge ordered)
    have storedAfterPersistence :=
      persist_proposal_occurrence_stores_own_block timed.execution
        result.persistenceOccurs
    have storedAtObservation :=
      (timed.execution.durable_fields_persist result.ready.validatorInRange
        persistedBeforeObservation).own_block_persists storedAfterPersistence
    have signerFloorCoversBlock :=
      (timed.execution.statesWellFormed observation validator
        result.ready.validatorInRange).ownBlockDoesNotExceedSignerFloor
          result.snapshot.block.reference.round result.snapshot.block.reference
          storedAtObservation
    rw [result.completed.snapshotRound, targetAtObservation] at signerFloorCoversBlock
    omega

/-- Current recovery state gives one future persistence and send, or loses to a
local commit-index advance.

An already armed timer can have completed its proposal action before the
observation. Therefore, the result does not claim that the proposal action is
later than the observation. Exact-next persistence must still be later, and the
durable send follows that persistence. -/
theorem current_recovery_state_builds_strict_broadcast_or_commit_advance
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    {obligations : ValidatorProposalObligationExecution timed}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (time validator receiver : Time)
    (input : ValidatorRecoveryTimerCurrentInputAt timed time validator)
    (active : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true)
    (receiverInRange : receiver < config.authorityCount)
    (differentReceiver : receiver ≠ validator) :
    ((∃ finish,
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
          time ≤ result.packet.sentAt) := by
  rcases timerSource.current_timer_input_and_active_suffix_derives_ready_or_commit_advance
      arms time validator input active with readyResult | commitRace
  · right
    rcases readyResult with ⟨originReady⟩
    rcases origin_aware_proposal_latch_builds_recovery_snapshot timed
        timerSource pacing latchSource effects originReady.ready
        originReady.refreshedRecoveryParents receiverInRange differentReceiver with
      ⟨proposalActionAt, persistTime, proposal, snapshot, packetId, packet,
        deadlineBeforeProposal, proposalWithinBound, proposalOccurs,
        pacedProposal,
        pacedTimerMatches, latched, completed, broadcast, proposalOrigin,
        proposalBlock, proposalBeforePersistence,
        storedAfterPersistence, persistenceOccurs⟩
    let result : ValidatorStrictRecoveryBroadcast timed waits timerSource
        obligations validator receiver :=
      { commitHead :=
          ((timed.execution.trace time).validatorState validator).commitHead
        targetRound :=
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound + 1
        ready := originReady.ready
        refreshedParentList := originReady.refreshedRecoveryParents
        timerStartWithinLocalBound := originReady.startedWithinLocalBound
        proposalActionAt := proposalActionAt
        deadlineBeforeProposal := deadlineBeforeProposal
        proposalWithinLocalBound := proposalWithinBound
        proposalOccurs := proposalOccurs
        pacedProposal := Classical.choice pacedProposal
        pacedTimerStartedAt :=
          (pacedTimerMatches (Classical.choice pacedProposal)).1
        pacedTimerCommitHead :=
          (pacedTimerMatches (Classical.choice pacedProposal)).2.1
        pacedTimerTargetRound :=
          (pacedTimerMatches (Classical.choice pacedProposal)).2.2
        proposal := proposal
        snapshot := snapshot
        packetId := packetId
        packet := packet
        latched := latched
        completed := completed
        broadcast := broadcast
        recoveryOrigin := proposalOrigin
        exactBlock := proposalBlock
        persistTime := persistTime
        proposalBeforePersistence := proposalBeforePersistence
        storedAfterPersistence := storedAfterPersistence
        persistenceOccurs := persistenceOccurs
        storedWithinPipelinePrefix := completed.storedWithinPipelinePrefix }
    have persistenceAfter :=
      strict_recovery_persistence_is_not_before_exact_next_observation result rfl
    have sendAfterPersistence : result.persistTime ≤ result.packet.sentAt := by
      have storedBeforeSend := result.broadcast.storedBeforeSend
      rw [result.storedAfterPersistence] at storedBeforeSend
      exact Nat.le_trans (Nat.le_add_right result.persistTime 1)
        storedBeforeSend
    exact ⟨result, rfl, persistenceAfter,
      Nat.le_trans persistenceAfter sendAfterPersistence⟩
  · exact Or.inl commitRace

/-- Parent-first block sync accepts the exact strict recovery broadcast.

The source is one retained finite causal history for this already produced
block. A higher theorem must derive it from the block-sync source rules. -/
theorem strict_recovery_broadcast_eventually_accepted_or_gc_root_via_parent_sync
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {obligations : ValidatorProposalObligationExecution timed}
    {validator receiver holder : Nat}
    (result : ValidatorStrictRecoveryBroadcast timed waits timerSource
      obligations validator receiver)
    (acceptanceRules : ValidatorParentReadyAcceptanceRules timed)
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    {blocks : List (ValidatorBlock BlockId)}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ result.packet.sentAt)
    (active : ∀ time, result.packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (syncSource : ValidatorBroadcastParentSyncSource syncRules result.snapshot
      receiver holder blocks (result.packet.deliveredAt + 1)) :
    ∃ acceptedAt,
      result.packet.deliveredAt + 1 ≤ acceptedAt ∧
        (((timed.execution.trace acceptedAt).validatorState receiver).accepted
            result.snapshot.block.reference = true ∨
          result.snapshot.block.reference.round ≤
            ((timed.execution.trace acceptedAt).validatorState
              receiver).gcRound) := by
  exact completed_recovery_broadcast_eventually_accepted_or_gc_root_via_parent_sync timed
    acceptanceRules syncRules result.completed receiverInRange
    receiverCorrectAvailable result.broadcast sentAfterGst active syncSource

/-- Two strict pointwise broadcasts for the same proposer and target round use
the same durable block reference. -/
theorem strict_recovery_broadcasts_same_round_have_same_reference
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {obligations : ValidatorProposalObligationExecution timed}
    {validator leftReceiver rightReceiver : Nat}
    (left : ValidatorStrictRecoveryBroadcast timed waits timerSource
      obligations validator leftReceiver)
    (right : ValidatorStrictRecoveryBroadcast timed waits timerSource
      obligations validator rightReceiver)
    (sameTarget : left.targetRound = right.targetRound) :
    left.snapshot.block.reference = right.snapshot.block.reference := by
  let finish := max left.snapshot.storedAt right.snapshot.storedAt
  have leftBeforeFinish : left.snapshot.storedAt ≤ finish :=
    Nat.le_max_left _ _
  have rightBeforeFinish : right.snapshot.storedAt ≤ finish :=
    Nat.le_max_right _ _
  have leftStoredAt :
      ((timed.execution.trace left.snapshot.storedAt).validatorState
        validator).ownBlockAt left.snapshot.block.reference.round =
          some left.snapshot.block.reference := by
    simpa [left.completed.snapshotProposer] using left.snapshot.blockStored
  have rightStoredAt :
      ((timed.execution.trace right.snapshot.storedAt).validatorState
        validator).ownBlockAt right.snapshot.block.reference.round =
          some right.snapshot.block.reference := by
    simpa [right.completed.snapshotProposer] using right.snapshot.blockStored
  have leftStored :=
    (timed.execution.durable_fields_persist left.ready.validatorInRange
      leftBeforeFinish).own_block_persists leftStoredAt
  have rightStored :=
    (timed.execution.durable_fields_persist right.ready.validatorInRange
      rightBeforeFinish).own_block_persists rightStoredAt
  have leftAtTarget :
      ((timed.execution.trace finish).validatorState validator).ownBlockAt
          left.targetRound = some left.snapshot.block.reference := by
    simpa [left.completed.snapshotRound] using leftStored
  have rightAtTarget :
      ((timed.execution.trace finish).validatorState validator).ownBlockAt
          left.targetRound = some right.snapshot.block.reference := by
    simpa [sameTarget, right.completed.snapshotRound] using rightStored
  exact Option.some.inj (leftAtTarget.symm.trans rightAtTarget)

/-- Two adjacent exact recovery proposals give one direct vote after the child
is accepted by an observer.

The leader acceptance at the child snapshot is a timing result. The local
parent rule then includes that exact leader reference. The rest follows from
the durable block catalog and accepted-representative update. -/
theorem adjacent_recovery_snapshots_give_exact_direct_vote
    [DecidableEq BlockId]
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (anchorRules : ValidatorAnchorLocalRules config faults execution.trace)
    {leaderSnapshot childSnapshot :
      ValidatorProposalSnapshot config faults execution.trace}
    {observer voter round finish : Time}
    (childOwner : childSnapshot.proposer = voter)
    (leaderRound : leaderSnapshot.block.reference.round = round)
    (childRound : childSnapshot.block.reference.round = round + 1)
    (leaderAcceptedAtChildSnapshot :
      ((execution.trace childSnapshot.snapshotAt).validatorState
        childSnapshot.proposer).accepted leaderSnapshot.block.reference = true)
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (childStoredBeforeFinish : childSnapshot.storedAt ≤ finish)
    (childAccepted :
      ((execution.trace finish).validatorState observer).accepted
        childSnapshot.block.reference = true) :
    traceDirectVoters (execution.trace finish) observer
        leaderSnapshot.block.reference voter = true := by
  have leaderAuthorInRange :
      leaderSnapshot.block.reference.author < config.authorityCount := by
    simpa [leaderSnapshot.blockIsOwnProposal] using
      leaderSnapshot.proposerInRange
  have leaderAuthorCorrect :
      faults.correctAvailable leaderSnapshot.block.reference.author = true := by
    simpa [leaderSnapshot.blockIsOwnProposal] using
      leaderSnapshot.proposerCorrectAvailable
  have leaderImmediatelyPrecedesChild :
      leaderSnapshot.block.reference.round + 1 =
        childSnapshot.block.reference.round := by
    omega
  have leaderIsParent : leaderSnapshot.block.reference ∈
      childSnapshot.block.parents := by
    exact anchorRules.includesAcceptedCorrectImmediateParent childSnapshot
      leaderSnapshot.block.reference leaderAuthorInRange leaderAuthorCorrect
      leaderImmediatelyPrecedesChild leaderAcceptedAtChildSnapshot
  have childCatalog := execution.blockCatalogMonotone childSnapshot.storedAt
    finish childStoredBeforeFinish childSnapshot.block.reference.id
    childSnapshot.block childSnapshot.blockInCatalog
  have childAuthorInRange :
      childSnapshot.block.reference.author < config.authorityCount := by
    simpa [childSnapshot.blockIsOwnProposal] using childSnapshot.proposerInRange
  have childAuthorCorrect :
      faults.correctAvailable childSnapshot.block.reference.author = true := by
    simpa [childSnapshot.blockIsOwnProposal] using
      childSnapshot.proposerCorrectAvailable
  have representative :=
    anchorRules.acceptedCorrectBlockRecordsRepresentative finish observer
      childSnapshot.block observerInRange observerCorrectAvailable
      childAuthorInRange childAuthorCorrect childCatalog childAccepted
  have childAuthor : childSnapshot.block.reference.author = voter := by
    exact childSnapshot.blockIsOwnProposal.trans childOwner
  have representativeAtVoteSlot :
      ((execution.trace finish).validatorState observer).acceptedRepresentative
          (leaderSnapshot.block.reference.round + 1) voter =
        some childSnapshot.block.reference := by
    simpa [childRound, leaderRound, childAuthor] using representative
  exact accepted_child_with_leader_parent_is_direct_voter
    representativeAtVoteSlot childCatalog rfl childAuthor
    leaderImmediatelyPrecedesChild.symm leaderIsParent

end Mysticeti
