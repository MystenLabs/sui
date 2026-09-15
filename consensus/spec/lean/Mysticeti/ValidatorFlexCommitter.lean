/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ReferenceFlexCommitterProgress
import Mysticeti.ValidatorTimedExecutionLemmas

namespace Mysticeti

/-!
Local execution bridge for the reference-carrying FlexCommitter.

The bridge has no distributed progress input. One finite local snapshot is
scanned by a pure deterministic function. An actual `runCommitter` action
returns that exact result. A successful result latches the matching
`recordCommit` work item. The bounded one-host rule runs the protected item.
The atomic action effect persists the exact reference and records
`localExecution` as its source.

Leader branch references use `BlockId`. The deterministic builder converts the
ordered leader decisions to a separate `CommitId`.

Another proof must show that protocol execution eventually makes the local
guard true. Exact agreement between two validators is also outside this module.
-/

namespace LocalFlexCommitOutput

/-- Convert the exact commit reference to the validator process commit head. -/
def toCommitHead {BlockId CommitId : Type}
    (output : LocalFlexCommitOutput BlockId CommitId) :
    ValidatorCommitHead CommitId :=
  { index := output.reference.index
    id := output.reference.digest
    round := output.candidate.leaderRound }

end LocalFlexCommitOutput

/-- Remove the exact block reference and keep the status stored in the local
pending-round array. -/
def referenceSlotStatusTag {BlockId : Type} :
    ReferenceSlotStatus BlockId → SelectedLeaderSlotStatus
  | .undecided => .undecided
  | .commit _ => .commit
  | .skip => .skip

/-- Remove exact slot identities and block references from one round view. -/
def referenceRoundPendingProjection {BlockId : Type}
    (round : ReferenceFlexRoundView BlockId) : ValidatorPendingRound :=
  { round := round.round
    selectedLeaderSlotStatuses :=
      round.selectedSlots.map fun slot => referenceSlotStatusTag slot.status }

/-- Use the exact branch ID as the validator execution block ID. -/
def referenceLeaderBlockToValidatorBlockRef {BlockId : Type}
    (block : LeaderBlockRef BlockId) : ValidatorBlockRef BlockId :=
  { round := block.round
    author := block.author
    id := block.digest }

/-- Select the exact FlexCommitter rules for one validator and input state. -/
abbrev ValidatorFlexContextAt (BlockId CommitId History : Type) :=
  Nat → ValidatorLocalState BlockId CommitId →
    ReferenceFlexCommitterContext BlockId History

/-- Local committed-prefix facts for one validator.

The first field says that a current head contains every earlier index. The
second field binds a stored index and ID to the complete local head, including
its round. These are local source mappings. They do not state future progress
or cross-validator agreement. -/
structure ValidatorCommitPrefixSourceMap
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) where
  installedAtOrBelowHead : ∀ time validator index,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    index ≤ (trace time).localCommitIndex validator →
    ∃ commitId,
      ((trace time).validatorState validator).installedCommitAt index =
        some commitId
  sameIndexInstalledHeadIsExact : ∀ time validator reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).installedCommitAt reference.index =
        some reference.id →
    (trace time).localCommitIndex validator = reference.index →
    ValidatorLocalState.commitHead
      ((trace time).validatorState validator) = reference

/-- One-validator source mapping between Rust local state and the pure model.

`snapshot` reads only the named validator's local state. It contains the raw
pending rounds before this call applies its direct and indirect rules. The
mapping fields bind every round, slot order, status, committed block reference,
and commit-body material to that local state.
`priorMatchesHead` binds the prior reference to the durable local commit head.
The final field states the local guard for `runCommitter`. It does not enable a
`recordCommit` action. -/
structure LocalFlexCommitterSourceMap
    {BlockId CommitId History Encoding : Type}
    (config : ValidatorEpochConfig CommitId)
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History)
    (program : ValidatorExecutionProgram BlockId CommitId) where
  snapshot : Nat → ValidatorLocalState BlockId CommitId →
    ReferenceFlexTryCommitInput BlockId CommitId
  materializer : ReferenceCommitMaterializerSourceMap
    (ValidatorLocalState BlockId CommitId) BlockId CommitId
  tryCommitStateValid : ∀ validator state,
    ReferenceFlexTryCommitStateValid (context validator state)
      (snapshot validator state)
  tryCommitMaterialValid : ∀ validator state,
    ReferenceFlexTryCommitMaterialValid (context validator state) materializer state
      (snapshot validator state)
  /-- The configured depth does not change between local calls. -/
  contextDepthStable : ∀ leftValidator leftState rightValidator rightState,
    (context leftValidator leftState).depth =
      (context rightValidator rightState).depth
  /-- The configured indirect rule does not change between local calls. -/
  contextIndirectRuleStable :
    ∀ leftValidator leftState rightValidator rightState,
      (context leftValidator leftState).indirectRule =
        (context rightValidator rightState).indirectRule
  /-- The actual selected leader slot order returned by the local leader
  selection code for one pending round. -/
  selectedLeaderSlots : Nat → ValidatorLocalState BlockId CommitId →
    Nat → List ExactSelectedLeaderSlot
  priorMatchesHead : ∀ validator state,
    let prior := (snapshot validator state).prior
    prior.index = state.commitHead.index ∧
      prior.digest = state.commitHead.id
  flexCommitIndexMatchesHead : ∀ validator state,
    (snapshot validator state).pending.commitIndex = state.commitHead.index
  pendingRoundCountMatches : ∀ validator state,
    (snapshot validator state).pending.roundCount =
      state.committer.pendingRounds.length
  pendingRoundAtMatches : ∀ validator state index,
    index < (snapshot validator state).pending.roundCount →
    state.committer.pendingRounds[index]? = some
      (referenceRoundPendingProjection
        ((snapshot validator state).pending.rounds index))
  selectedSlotsMatch : ∀ validator state index,
    index < (snapshot validator state).pending.roundCount →
    let round := (snapshot validator state).pending.rounds index
    round.selectedSlots.map ReferenceSelectedSlotView.slot =
      selectedLeaderSlots validator state round.round
  /-- The exact local slot order is the configured round leader selection
  order for the current commit head. -/
  selectedSlotsMatchConfig : ∀ validator state index,
    index < (snapshot validator state).pending.roundCount →
    let round := (snapshot validator state).pending.rounds index
    selectedLeaderSlots validator state round.round =
      (config.selectedLeaderOrder state.commitHead.id round.round).map
        (fun selectedValidator =>
          { round := round.round, validator := selectedValidator })
  selectedSlotsAreWellFormed : ∀ validator state index,
    index < (snapshot validator state).pending.roundCount →
    let round := (snapshot validator state).pending.rounds index
    ∀ slot, slot ∈ round.selectedSlots → slot.WellFormed
  committedReferencesMatchLocalState : ∀ validator state index slot block,
    index < (snapshot validator state).pending.roundCount →
    let round := (snapshot validator state).pending.rounds index
    slot ∈ round.selectedSlots →
    slot.status = .commit block →
    state.acceptedRepresentative block.round block.author =
        some (referenceLeaderBlockToValidatorBlockRef block) ∧
      state.accepted (referenceLeaderBlockToValidatorBlockRef block) = true
  pendingRoundsEnableRunCommitter : ∀ validator state,
    state.committer.pendingRounds ≠ [] →
    program.actions.enabled validator .runCommitter state

namespace LocalFlexCommitterSourceMap

variable {BlockId CommitId History Encoding : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- A successful pure local result makes the pending array nonempty. It enables
only the local `runCommitter` action. -/
theorem exact_result_enables_run_committer
    (source : LocalFlexCommitterSourceMap config functions context program)
    {validator : Nat} {state : ValidatorLocalState BlockId CommitId}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (found :
      tryReferenceFlexCommitWithContext functions (context validator state)
        (source.snapshot validator state) = some output) :
    ConcreteValidatorActionEnabled config program.actions validator
      .runCommitter state := by
  have pendingNonempty : state.committer.pendingRounds ≠ [] := by
    intro pendingEmpty
    have countZero :
        (source.snapshot validator state).pending.roundCount = 0 := by
      rw [source.pendingRoundCountMatches validator state, pendingEmpty]
      rfl
    simp [tryReferenceFlexCommitWithContext, tryReferenceFlexCommit,
      findIndexedReferenceFlexCandidate, findIndexedReferenceFlexCandidateFrom,
      runReferenceDirectPass, runFullIndexedReferenceIndirect, countZero] at found
  constructor
  · exact pendingNonempty
  · exact source.pendingRoundsEnableRunCommitter validator state
      pendingNonempty

end LocalFlexCommitterSourceMap

/-- A successful pure scan creates durable local `runCommitter` work. -/
structure ValidatorSuccessfulFlexScanWorkRules
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (source : LocalFlexCommitterSourceMap config functions context program) where
  successfulScanIsProtected : ∀ time validator output,
    tryReferenceFlexCommitWithContext functions
        (context validator
          ((timed.execution.trace time).validatorState validator))
        (source.snapshot validator
          ((timed.execution.trace time).validatorState validator)) =
      some output →
    timed.protectedAction time validator .runCommitter

/-- One exact result observed when one validator runs its local committer. -/
structure LocalFlexCommitterRunObservation
    (BlockId CommitId : Type) where
  time : Time
  validator : Nat
  input : ValidatorLocalState BlockId CommitId
  result : Option (LocalFlexCommitOutput BlockId CommitId)

/-- One observation names an actual `runCommitter` atomic step in the main
validator execution. Its input is the local state immediately before that
step. -/
def LocalFlexCommitterRunObservation.OccursIn
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId) : Prop :=
  ∃ beforeEvents afterEvents actionBefore actionAfter,
    timed.execution.events observation.time =
      beforeEvents ++
        (.localAction observation.validator .runCommitter :: afterEvents) ∧
      ValidatorWorldStep config faults protocolPacket program observation.time
        (timed.execution.trace observation.time) beforeEvents actionBefore ∧
      ValidatorAtomicStep config faults protocolPacket program observation.time
        actionBefore (.localAction observation.validator .runCommitter)
          actionAfter ∧
      ValidatorWorldStep config faults protocolPacket program observation.time
        actionAfter afterEvents
          (timed.execution.trace (observation.time + 1)) ∧
      observation.input =
        actionBefore.validatorState observation.validator

/-- One-validator runtime rules for the committer result and record latch.

The first rule binds every actual `runCommitter` transition to the pure result
for its exact input state. The second rule prevents another result from being
reported. A successful result latches one concrete `recordCommit` action. It
does not enable `recordCommit` from an arbitrary state snapshot.
-/
structure LocalFlexCommitterRuntime
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (source : LocalFlexCommitterSourceMap config functions context program) where
  returned : LocalFlexCommitterRunObservation BlockId CommitId → Prop
  runCommitterOccurrenceReturnsExactResult :
    ∀ (observation : LocalFlexCommitterRunObservation BlockId CommitId),
    observation.OccursIn timed →
    returned
      { time := observation.time
        validator := observation.validator
        input := observation.input
        result := tryReferenceFlexCommitWithContext functions
          (context observation.validator observation.input)
          (source.snapshot observation.validator observation.input) }
  everyReturnedResultIsExact : ∀ observation,
    returned observation →
    observation.result = tryReferenceFlexCommitWithContext functions
      (context observation.validator observation.input)
      (source.snapshot observation.validator observation.input)
  /-- A returned observation cannot be invented outside the main trace. -/
  everyReturnedObservationOccurs : ∀ observation,
    returned observation → observation.OccursIn timed
  successfulResultLatchesRecord : ∀ observation output,
    returned observation →
    observation.result = some output →
    timed.protectedAction (observation.time + 1) observation.validator
      (.recordCommit output.toCommitHead)

/-- An actual `runCommitter` event returns the exact pure result for the local
state read by that event. -/
theorem run_committer_occurrence_returns_exact_flex_result
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source)
    {time : Time}
    {validator : Nat}
    (occurs : ValidatorLocalActionOccurs
      (timed.execution.events time) validator .runCommitter) :
    ∃ input,
      runtime.returned
        { time
          validator
          input
          result := tryReferenceFlexCommitWithContext functions
            (context validator input)
            (source.snapshot validator input) } := by
  rcases occurs with ⟨beforeEvents, afterEvents, eventSplit⟩
  have batchStep := timed.execution.stepsFollowRules time
  rw [eventSplit] at batchStep
  rcases validator_world_step_append_split batchStep with
    ⟨actionBefore, prefixStep, actionAndSuffix⟩
  cases actionAndSuffix with
  | cons actionStep suffixStep =>
      let observation : LocalFlexCommitterRunObservation BlockId CommitId :=
        { time
          validator
          input := actionBefore.validatorState validator
          result := none }
      have origin : observation.OccursIn timed := by
        exact ⟨beforeEvents, afterEvents, actionBefore, _, eventSplit,
          prefixStep, actionStep, suffixStep, rfl⟩
      exact ⟨actionBefore.validatorState validator,
        runtime.runCommitterOccurrenceReturnsExactResult observation origin⟩

/-- The matching atomic `recordCommit` action stores the exact digest and marks
the install source as local execution. -/
theorem local_flex_record_occurrence_has_exact_atomic_effect
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {validator : Nat} {output : LocalFlexCommitOutput BlockId CommitId}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (occurs : ValidatorLocalActionOccurs events validator
      (.recordCommit output.toCommitHead)) :
    ∃ eventAfter : ValidatorWorldState BlockId CommitId PacketId,
      (eventAfter.validatorState validator).commitHead = output.toCommitHead ∧
        (eventAfter.validatorState validator).installedCommitAt
            output.reference.index = some output.reference.digest ∧
        (eventAfter.validatorState validator).commitInstallSourceAt
            output.reference.index = some .localExecution := by
  rcases validator_local_action_occurrence_has_structural_effect step occurs with
    ⟨_eventBefore, eventAfter, structural⟩
  have installEffect := structural.2.2.2.2.2
  simp only [CommitInstallActionEffect] at installEffect
  refine ⟨eventAfter, installEffect.1, ?_, ?_⟩
  · exact installEffect.2.2.1.1
  · exact installEffect.2.2.2.1

private theorem local_flex_installed_commit_persists_through_batch
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
    {validator index : Nat} {commitId : CommitId}
    (installed :
      (before.validatorState validator).installedCommitAt index = some commitId) :
    (after.validatorState validator).installedCommitAt index = some commitId := by
  induction step with
  | nil => exact installed
  | cons firstStep remainingSteps ih =>
      have installedAfterFirst :=
        (validator_atomic_step_durable_monotone firstStep validator)
          |>.installed_commit_persists installed
      exact ih installedAfterFirst

private theorem local_flex_install_source_persists_through_batch
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
    {validator index : Nat} {source : CommitInstallSource}
    (recorded :
      (before.validatorState validator).commitInstallSourceAt index =
        some source) :
    (after.validatorState validator).commitInstallSourceAt index =
      some source := by
  induction step with
  | nil => exact recorded
  | cons firstStep remainingSteps ih =>
      have recordedAfterFirst :=
        (validator_atomic_step_durable_monotone firstStep validator)
          |>.install_source_persists recorded
      exact ih recordedAfterFirst

/-- A matching local record action leaves the exact reference and source in the
state at the end of its event batch. -/
theorem local_flex_record_occurrence_persists_to_batch_end
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {validator : Nat} {output : LocalFlexCommitOutput BlockId CommitId}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (occurs : ValidatorLocalActionOccurs events validator
      (.recordCommit output.toCommitHead)) :
    (after.validatorState validator).installedCommitAt output.reference.index =
        some output.reference.digest ∧
      (after.validatorState validator).commitInstallSourceAt
          output.reference.index = some .localExecution := by
  rcases validator_world_step_local_action_with_suffix step occurs with
    ⟨_actionBefore, actionAfter, suffix, actionStep, suffixStep⟩
  have structural :=
    validator_atomic_local_action_has_structural_effect actionStep
  have installEffect := structural.2.2.2.2.2
  simp only [CommitInstallActionEffect] at installEffect
  have installedAtAction := installEffect.2.2.1.1
  have sourceAtAction := installEffect.2.2.2.1
  exact ⟨
    local_flex_installed_commit_persists_through_batch suffixStep
      installedAtAction,
    local_flex_install_source_persists_through_batch suffixStep
      sourceAtAction⟩

/-- A protected exact record work item completes within the local action bound.
The exact commit ID and its local source remain in the next trace state. -/
theorem protected_local_flex_record_completes_and_persists_exact
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {validator latchedAt : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (validatorInRange : validator < config.authorityCount)
    (validatorLive : faults.correctAvailable validator = true)
    (latched : timed.protectedAction latchedAt validator
      (.recordCommit output.toCommitHead)) :
    ∃ finish,
      latchedAt ≤ finish ∧
        finish ≤ latchedAt + timed.localActionBound ∧
        ((timed.execution.trace (finish + 1)).validatorState validator).installedCommitAt
            output.reference.index =
          some output.reference.digest ∧
        ((timed.execution.trace (finish + 1)).validatorState validator).commitInstallSourceAt
            output.reference.index =
          some CommitInstallSource.localExecution := by
  rcases protected_validator_action_completes_within_bound timed
      validatorInRange validatorLive latched with
    ⟨completion, latchBeforeFinish, finishWithinBound, occurs⟩
  have persisted := local_flex_record_occurrence_persists_to_batch_end
    (timed.execution.stepsFollowRules completion.event.completedAt) occurs
  exact ⟨completion.event.completedAt, latchBeforeFinish, finishWithinBound,
    persisted⟩

/-- The complete local sequence: an exact successful `runCommitter` result
latches its matching record work item. The protected action then persists that
exact reference with source `localExecution`. -/
theorem successful_local_flex_run_completes_and_persists_exact
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    {output : LocalFlexCommitOutput BlockId CommitId}
    (returned : runtime.returned observation)
    (successful : observation.result = some output)
    (validatorInRange : observation.validator < config.authorityCount)
    (validatorLive :
      faults.correctAvailable observation.validator = true) :
    ∃ finish,
      tryReferenceFlexCommitWithContext functions
          (context observation.validator observation.input)
          (source.snapshot observation.validator observation.input) =
          some output ∧
        observation.time + 1 ≤ finish ∧
        finish ≤ observation.time + 1 + timed.localActionBound ∧
        ((timed.execution.trace (finish + 1)).validatorState
            observation.validator).installedCommitAt
            output.reference.index = some output.reference.digest ∧
        ((timed.execution.trace (finish + 1)).validatorState
            observation.validator).commitInstallSourceAt output.reference.index =
          some CommitInstallSource.localExecution := by
  have pureResult :
      tryReferenceFlexCommitWithContext functions
          (context observation.validator observation.input)
          (source.snapshot observation.validator observation.input) =
        some output := by
    rw [← runtime.everyReturnedResultIsExact observation returned]
    exact successful
  have latched := runtime.successfulResultLatchesRecord observation output
    returned successful
  rcases protected_local_flex_record_completes_and_persists_exact timed
      validatorInRange validatorLive latched with
    ⟨finish, latchBeforeFinish, finishWithinBound, installed, installedSource⟩
  exact ⟨finish, pureResult, latchBeforeFinish, finishWithinBound, installed,
    installedSource⟩

end Mysticeti
