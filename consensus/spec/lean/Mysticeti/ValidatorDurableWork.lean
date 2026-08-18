/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorFlexCommitter

namespace Mysticeti

/-! Durable one-validator work for the final liveness path.

The primary path keeps exact local replay data and two full-output latches. An
optional extension keeps commit request and service goals. Neither structure
states that work completes or that a remote validator has data.
-/

/-- Convert one stored validator block to the reference used in a commit body. -/
def validatorReplayBlockRef {BlockId : Type}
    (block : ValidatorBlock BlockId) : LeaderBlockRef BlockId :=
  { round := block.reference.round
    author := block.reference.author
    digest := block.reference.id }

/-- A replay DAG is parent-first relative to explicit roots. This definition is
kept below the block-sync layer so replay material does not depend on a future
network result. -/
def ValidatorReplayParentFirst
    {BlockId : Type}
    (roots : ValidatorBlockRef BlockId → Prop) :
    List (ValidatorBlock BlockId) → Prop
  | [] => True
  | block :: remaining =>
      (∀ parent, parent ∈ block.parents → roots parent) ∧
        ValidatorReplayParentFirst
          (fun reference => roots reference ∨ reference = block.reference)
          remaining

/-- The finite local data that reproduces one exact `try_commit` result. -/
structure ValidatorExactReplayMaterial
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History) where
  sourceValidator : Nat
  sourceTime : Time
  sourceInput : ValidatorLocalState BlockId CommitId
  /-- The complete finite decision DAG, including vote and anchor blocks. -/
  blocks : List (ValidatorBlock BlockId)
  blocksNonempty : blocks ≠ []
  /-- Canonical genesis parents outside the replay DAG. -/
  genesisRoots : List (ValidatorBlockRef BlockId)
  genesisRootsAtRoundZero : ∀ reference, reference ∈ genesisRoots →
    reference.round = 0
  /-- Other parents outside the replay DAG that the exact prior supplies. -/
  externalRoots : List (ValidatorBlockRef BlockId)
  /-- The replay DAG is parent-first. A missing in-list parent must be either a
  genesis reference or an explicit root supplied by the exact prior. -/
  parentFirstFromRoots : ValidatorReplayParentFirst
    (fun reference =>
      reference ∈ genesisRoots ∨ reference ∈ externalRoots)
    blocks
  /-- The subset serialized in the exact commit body. -/
  commitBlocks : List (ValidatorBlock BlockId)
  commitBlocksSubset : ∀ block, block ∈ commitBlocks → block ∈ blocks
  snapshot : ReferenceFlexTryCommitInput BlockId CommitId
  output : LocalFlexCommitOutput BlockId CommitId
  outputFromSnapshot :
    tryReferenceFlexCommitWithContext functions
      (context sourceValidator sourceInput) snapshot = some output
  exactCommitBlocks :
    output.record.blocks = commitBlocks.map validatorReplayBlockRef

/-- One exact replay has an earlier local run in the main validator trace.

A verified sync install is not enough to authenticate arbitrary replay data.
Its install provenance must first descend to an earlier exact local run. -/
def ValidatorExactReplayMaterial.HasMainTraceOrigin
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (material : ValidatorExactReplayMaterial functions context)
    (beforeTime : Time) : Prop :=
  material.sourceTime < beforeTime ∧
    source.snapshot material.sourceValidator material.sourceInput =
      material.snapshot ∧
    runtime.returned
      { time := material.sourceTime
        validator := material.sourceValidator
        input := material.sourceInput
        result := some material.output }

/-- An established replay origin remains earlier than every later time. -/
theorem ValidatorExactReplayMaterial.main_trace_origin_mono
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {material : ValidatorExactReplayMaterial functions context}
    {earlier later : Time}
    (origin : material.HasMainTraceOrigin (timed := timed) (source := source)
      (runtime := runtime) earlier)
    (ordered : earlier ≤ later) :
    material.HasMainTraceOrigin (timed := timed) (source := source)
      (runtime := runtime) later := by
  rcases origin with ⟨originBefore, snapshotMatches, actualOrigin⟩
  change material.sourceTime < later ∧ _
  exact ⟨Nat.lt_of_lt_of_le originBefore ordered, snapshotMatches,
    actualOrigin⟩

/-- Primary durable work for exact local replay and `try_commit`. -/
structure ValidatorTryCommitWorkState
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History) where
  retainedReplay : ValidatorExactReplayMaterial functions context → Bool
  /-- The exact output that the queued `runCommitter` action must return. -/
  pendingTryCommitRun : Option (LocalFlexCommitOutput BlockId CommitId)
  /-- The exact completed output that the queued record action must install. -/
  pendingTryCommitResult : Option (LocalFlexCommitOutput BlockId CommitId)

/-- One local change to primary `try_commit` work. -/
inductive ValidatorTryCommitWorkEvent
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History) where
  | retainReplay (material : ValidatorExactReplayMaterial functions context)
  | latchTryCommitRun (output : LocalFlexCommitOutput BlockId CommitId)
  | completeTryCommit (output : LocalFlexCommitOutput BlockId CommitId)
  | completeRecord (output : LocalFlexCommitOutput BlockId CommitId)
  | idle

/-- Explicit local transitions for primary `try_commit` work. -/
inductive ValidatorTryCommitWorkTransition
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History} :
    ValidatorTryCommitWorkState functions context →
      ValidatorTryCommitWorkEvent functions context →
      ValidatorTryCommitWorkState functions context → Prop where
  | retainReplay before after material
      (pendingRunSame :
        after.pendingTryCommitRun = before.pendingTryCommitRun)
      (pendingResultSame :
        after.pendingTryCommitResult = before.pendingTryCommitResult)
      (retainedUpdate : BoolMapSetsOnly before.retainedReplay
        after.retainedReplay material) :
      ValidatorTryCommitWorkTransition before (.retainReplay material) after
  | latchTryCommitRun state output
      (runEmpty : state.pendingTryCommitRun = none)
      (resultEmpty : state.pendingTryCommitResult = none) :
      ValidatorTryCommitWorkTransition state (.latchTryCommitRun output)
        { state with pendingTryCommitRun := some output }
  | completeTryCommit state output
      (runLatched : state.pendingTryCommitRun = some output)
      (resultEmpty : state.pendingTryCommitResult = none) :
      ValidatorTryCommitWorkTransition state (.completeTryCommit output)
        { state with
          pendingTryCommitRun := none
          pendingTryCommitResult := some output }
  | completeRecord state output
      (latched : state.pendingTryCommitResult = some output) :
      ValidatorTryCommitWorkTransition state (.completeRecord output)
        { state with pendingTryCommitResult := none }
  | idle state :
      ValidatorTryCommitWorkTransition state .idle state

/-- The two work phases do not hold values at the same time. -/
def ValidatorTryCommitWorkState.LatchesExclusive
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    (state : ValidatorTryCommitWorkState functions context) : Prop :=
  state.pendingTryCommitRun = none ∨
    state.pendingTryCommitResult = none

/-- One legal work transition preserves the exclusive work phases. -/
theorem try_commit_work_transition_preserves_latch_exclusivity
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {event : ValidatorTryCommitWorkEvent functions context}
    (step : ValidatorTryCommitWorkTransition before event after)
    (exclusive : before.LatchesExclusive) :
    after.LatchesExclusive := by
  cases step <;> simp_all [ValidatorTryCommitWorkState.LatchesExclusive]

/-- A retained entry after one transition was either inserted by that event or
was already retained before the transition. -/
theorem retained_replay_after_transition_has_source
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {event : ValidatorTryCommitWorkEvent functions context}
    {material : ValidatorExactReplayMaterial functions context}
    (step : ValidatorTryCommitWorkTransition before event after)
    (retained : after.retainedReplay material = true) :
    event = .retainReplay material ∨
      before.retainedReplay material = true := by
  cases step with
  | retainReplay _ stored _ _ updated =>
      by_cases same : material = stored
      · subst stored
        exact Or.inl rfl
      · exact Or.inr (by
          rw [← updated.2 material same]
          exact retained)
  | latchTryCommitRun => exact Or.inr retained
  | completeTryCommit => exact Or.inr retained
  | completeRecord => exact Or.inr retained
  | idle => exact Or.inr retained

/-- A queued run after one transition was either inserted by that event or was
already queued before the transition. -/
theorem pending_run_after_transition_has_source
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {event : ValidatorTryCommitWorkEvent functions context}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (step : ValidatorTryCommitWorkTransition before event after)
    (pending : after.pendingTryCommitRun = some output) :
    event = .latchTryCommitRun output ∨
      before.pendingTryCommitRun = some output := by
  cases step <;> simp_all

/-- A completed result after one transition was either inserted by that event
or was already complete before the transition. -/
theorem pending_result_after_transition_has_source
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {event : ValidatorTryCommitWorkEvent functions context}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (step : ValidatorTryCommitWorkTransition before event after)
    (pending : after.pendingTryCommitResult = some output) :
    event = .completeTryCommit output ∨
      before.pendingTryCommitResult = some output := by
  cases step <;> simp_all

/-- Retained exact replay data remains after one primary-work transition. -/
theorem retained_replay_persists_one_step
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {event : ValidatorTryCommitWorkEvent functions context}
    {material : ValidatorExactReplayMaterial functions context}
    (step : ValidatorTryCommitWorkTransition before event after)
    (retained : before.retainedReplay material = true) :
    after.retainedReplay material = true := by
  cases step with
  | retainReplay _ stored _ _ updated =>
      by_cases same : material = stored
      · subst stored
        exact updated.1
      · rw [updated.2 material same]
        exact retained
  | latchTryCommitRun => exact retained
  | completeTryCommit => exact retained
  | completeRecord => exact retained
  | idle => exact retained

/-- A queued run remains until its matching completion event. -/
theorem try_commit_run_persists_unless_completed
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {event : ValidatorTryCommitWorkEvent functions context}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (step : ValidatorTryCommitWorkTransition before event after)
    (latched : before.pendingTryCommitRun = some output)
    (notCompleted : event ≠ .completeTryCommit output) :
    after.pendingTryCommitRun = some output := by
  cases step <;> simp_all

/-- A completed result remains until its matching record event. -/
theorem try_commit_result_persists_unless_recorded
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {event : ValidatorTryCommitWorkEvent functions context}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (step : ValidatorTryCommitWorkTransition before event after)
    (latched : before.pendingTryCommitResult = some output)
    (notRecorded : event ≠ .completeRecord output) :
    after.pendingTryCommitResult = some output := by
  cases step <;> simp_all

/-- Latching one run stores its exact expected output. -/
theorem latch_try_commit_run_sets_pending
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (step : ValidatorTryCommitWorkTransition before (.latchTryCommitRun output)
      after) :
    after.pendingTryCommitRun = some output := by
  cases step
  rfl

/-- Completing one run moves its exact output to the record latch. -/
theorem complete_try_commit_sets_result
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (step : ValidatorTryCommitWorkTransition before (.completeTryCommit output)
      after) :
    after.pendingTryCommitRun = none ∧
      after.pendingTryCommitResult = some output := by
  cases step
  exact ⟨rfl, rfl⟩

/-- Recording one completed result clears its durable result latch. -/
theorem complete_record_clears_result
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (step : ValidatorTryCommitWorkTransition before (.completeRecord output)
      after) :
    after.pendingTryCommitResult = none := by
  cases step
  rfl

/-- Retaining one replay stores that exact material. -/
theorem retain_replay_sets_entry
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorTryCommitWorkState functions context}
    {material : ValidatorExactReplayMaterial functions context}
    (step : ValidatorTryCommitWorkTransition before (.retainReplay material)
      after) :
    after.retainedReplay material = true := by
  cases step with
  | retainReplay _ _ _ _ updated => exact updated.1

/-- The primary durable-work trace for each validator. -/
abbrev ValidatorTryCommitWorkTrace
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History) :=
  Time → Nat → ValidatorTryCommitWorkState functions context

/-- Adapter from primary durable work to the main validator execution.

Each field describes one local state change or exact action effect. No field
states that replay, `try_commit`, or record work will finish.
-/
structure ValidatorTryCommitWorkExecution
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source) where
  trace : ValidatorTryCommitWorkTrace functions context
  event : Time → Nat → ValidatorTryCommitWorkEvent functions context
  transitionsFollowRules : ∀ time validator,
    ValidatorTryCommitWorkTransition (trace time validator)
      (event time validator) (trace (time + 1) validator)
  /-- The genesis-rooted work trace starts with no queued or retained work. -/
  initialWorkEmpty : ∀ validator,
    (trace 0 validator).pendingTryCommitRun = none ∧
      (trace 0 validator).pendingTryCommitResult = none ∧
      ∀ material, (trace 0 validator).retainedReplay material = false
  reconstructedResultIsReflected : ∀ time validator output,
    tryReferenceFlexCommitWithContext functions
        (context validator
          ((timed.execution.trace time).validatorState validator))
        (source.snapshot validator
          ((timed.execution.trace time).validatorState validator)) =
      some output →
    (trace time validator).pendingTryCommitRun = none →
    (trace time validator).pendingTryCommitResult = none →
    event time validator = .latchTryCommitRun output
  /-- A run-latch event must name the exact full result for the main state at
  that time. -/
  latchTryCommitRunEventHasOrigin : ∀ time validator output,
    event time validator = .latchTryCommitRun output →
    tryReferenceFlexCommitWithContext functions
        (context validator
          ((timed.execution.trace time).validatorState validator))
        (source.snapshot validator
          ((timed.execution.trace time).validatorState validator)) =
      some output
  pendingRunIsProtected : ∀ time validator output,
    (trace time validator).pendingTryCommitRun = some output →
    timed.protectedAction time validator .runCommitter
  /-- The completion indexed by one latch still has that full output. -/
  latchedRunSurvivesToCompletion : ∀ enabledAt validator output
      (completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator .runCommitter enabledAt),
    (trace enabledAt validator).pendingTryCommitRun = some output →
    (trace completion.event.completedAt validator).pendingTryCommitRun =
      some output
  protectedRunReturnsLatchedResult : ∀ time validator output,
    (trace time validator).pendingTryCommitRun = some output →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      .runCommitter →
    ∃ input,
      runtime.returned
        { time
          validator
          input
          result := some output }
  successfulResultIsReflected : ∀ observation output,
    runtime.returned observation →
    observation.result = some output →
    event observation.time observation.validator = .completeTryCommit output
  /-- A completed-run event must come from a real returned observation at the
  same validator and time. -/
  completeTryCommitEventHasOrigin : ∀ time validator output,
    event time validator = .completeTryCommit output →
    ∃ input,
      runtime.returned
        { time
          validator
          input
          result := some output }
  pendingResultIsProtected : ∀ time validator output,
    (trace time validator).pendingTryCommitResult = some output →
    timed.protectedAction time validator
      (.recordCommit output.toCommitHead)
  /-- The completion indexed by a result latch still has that full result. -/
  latchedResultSurvivesToCompletion : ∀ enabledAt validator output
      (completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator (.recordCommit output.toCommitHead)
          enabledAt),
    (trace enabledAt validator).pendingTryCommitResult = some output →
    (trace completion.event.completedAt validator).pendingTryCommitResult =
      some output
  recordActionIsReflected : ∀ time validator output,
    (trace time validator).pendingTryCommitResult = some output →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit output.toCommitHead) →
    event time validator = .completeRecord output
  /-- A completed-record event must come from the matching real main-trace
  action. -/
  completeRecordEventHasOrigin : ∀ time validator output,
    event time validator = .completeRecord output →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.recordCommit output.toCommitHead)
  /-- A replay-retention event must name exact material with an earlier local
  run in the main trace. -/
  retainReplayEventHasOrigin : ∀ time validator material,
    event time validator = .retainReplay material →
    material.HasMainTraceOrigin (timed := timed) (source := source)
      (runtime := runtime) (time + 1)
  retainedReplayPinsBlocks : ∀ time validator material,
    (trace time validator).retainedReplay material = true →
    ∀ block, block ∈ material.blocks →
      ((timed.execution.trace time).validatorState validator).retained
          block.reference = true ∧
        (timed.execution.trace time).blockCatalog block.reference.id = some block

namespace ValidatorTryCommitWorkExecution

variable {BlockId CommitId History Encoding PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}

/-- Initial emptiness and the local transition rules imply exclusive work
phases at every time. -/
theorem work_latches_are_exclusive
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    (time : Time) (validator : Nat) :
    (work.trace time validator).LatchesExclusive := by
  induction time with
  | zero =>
      exact Or.inl (work.initialWorkEmpty validator).1
  | succ previous ih =>
      have preserved := try_commit_work_transition_preserves_latch_exclusivity
        (work.transitionsFollowRules previous validator) ih
      simpa [Nat.succ_eq_add_one] using preserved

/-- Every retained replay entry has an earlier local-run origin in the main
trace. This result follows from initial emptiness, the
retention transition, and the reverse origin rule for retention events. -/
theorem retained_replay_has_main_trace_origin
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    {time : Time} {validator : Nat}
    {material : ValidatorExactReplayMaterial functions context}
    (retained : (work.trace time validator).retainedReplay material = true) :
    material.HasMainTraceOrigin (timed := timed) (source := source)
      (runtime := runtime) time := by
  induction time with
  | zero =>
      rw [(work.initialWorkEmpty validator).2.2 material] at retained
      contradiction
  | succ previous ih =>
      have step := work.transitionsFollowRules previous validator
      have retainedAtSuccessor :
          (work.trace (previous + 1) validator).retainedReplay material =
            true := by
        simpa [Nat.succ_eq_add_one] using retained
      rcases retained_replay_after_transition_has_source step
          retainedAtSuccessor with inserted | retainedBefore
      · exact work.retainReplayEventHasOrigin previous validator material
          inserted
      · exact ValidatorExactReplayMaterial.main_trace_origin_mono
          (ih retainedBefore) (Nat.le_succ previous)

/-- Every queued run was latched by an earlier exact reconstruction event. -/
theorem pending_run_has_reconstruction_origin
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    {time : Time} {validator : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (pending : (work.trace time validator).pendingTryCommitRun = some output) :
    ∃ latchTime,
      latchTime < time ∧
        work.event latchTime validator = .latchTryCommitRun output ∧
        tryReferenceFlexCommitWithContext functions
            (context validator
              ((timed.execution.trace latchTime).validatorState validator))
            (source.snapshot validator
              ((timed.execution.trace latchTime).validatorState validator)) =
          some output := by
  induction time with
  | zero =>
      rw [(work.initialWorkEmpty validator).1] at pending
      contradiction
  | succ previous ih =>
      have step := work.transitionsFollowRules previous validator
      have pendingAtSuccessor :
          (work.trace (previous + 1) validator).pendingTryCommitRun =
            some output := by
        simpa [Nat.succ_eq_add_one] using pending
      rcases pending_run_after_transition_has_source step pendingAtSuccessor with
        inserted | pendingBefore
      · exact ⟨previous, Nat.lt_succ_self previous, inserted,
          work.latchTryCommitRunEventHasOrigin previous validator output
            inserted⟩
      · rcases ih pendingBefore with
          ⟨latchTime, latchBefore, latchEvent, exactResult⟩
        exact ⟨latchTime, Nat.lt_succ_of_lt latchBefore, latchEvent,
          exactResult⟩

/-- Every queued result was produced by an earlier real local committer run. -/
theorem pending_result_has_run_origin
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    {time : Time} {validator : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (pending :
      (work.trace time validator).pendingTryCommitResult = some output) :
    ∃ runTime,
      runTime < time ∧
        work.event runTime validator = .completeTryCommit output ∧
        ∃ input,
          runtime.returned
            { time := runTime
              validator
              input
              result := some output } := by
  induction time with
  | zero =>
      rw [(work.initialWorkEmpty validator).2.1] at pending
      contradiction
  | succ previous ih =>
      have step := work.transitionsFollowRules previous validator
      have pendingAtSuccessor :
          (work.trace (previous + 1) validator).pendingTryCommitResult =
            some output := by
        simpa [Nat.succ_eq_add_one] using pending
      rcases pending_result_after_transition_has_source step pendingAtSuccessor with
        inserted | pendingBefore
      · rcases work.completeTryCommitEventHasOrigin previous validator output
            inserted with ⟨input, returned⟩
        exact ⟨previous, Nat.lt_succ_self previous, inserted, input, returned⟩
      · rcases ih pendingBefore with
          ⟨runTime, runBefore, runEvent, input, returned⟩
        exact ⟨runTime, Nat.lt_succ_of_lt runBefore, runEvent, input,
          returned⟩

/-- A reconstructed exact result becomes one protected local run. -/
theorem reconstructed_try_commit_run_is_latched
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    {time : Time} {validator : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (reconstructed :
      tryReferenceFlexCommitWithContext functions
          (context validator
            ((timed.execution.trace time).validatorState validator))
          (source.snapshot validator
            ((timed.execution.trace time).validatorState validator)) =
        some output)
    (runEmpty : (work.trace time validator).pendingTryCommitRun = none)
    (resultEmpty :
      (work.trace time validator).pendingTryCommitResult = none) :
    (work.trace (time + 1) validator).pendingTryCommitRun = some output ∧
      timed.protectedAction (time + 1) validator .runCommitter := by
  have reflected := work.reconstructedResultIsReflected time validator output
    reconstructed runEmpty resultEmpty
  have step := work.transitionsFollowRules time validator
  rw [reflected] at step
  have latched := latch_try_commit_run_sets_pending step
  exact ⟨latched, work.pendingRunIsProtected _ _ _ latched⟩

/-- A latched run uses the bounded protected-action rule. -/
theorem latched_try_commit_run_runs_within_bound
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    {time : Time} {validator : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (latched : (work.trace time validator).pendingTryCommitRun = some output) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator .runCommitter time,
      time ≤ completion.event.completedAt ∧
        completion.event.completedAt ≤ time + timed.localActionBound ∧
        ValidatorLocalActionOccurs
          (timed.execution.events completion.event.completedAt) validator
          .runCommitter := by
  exact protected_validator_action_completes_within_bound timed
    validatorInRange correctAvailable
    (work.pendingRunIsProtected time validator output latched)

/-- A completed latched run stores its full result before record work runs. -/
theorem completed_try_commit_result_is_latched
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    {time : Time} {validator : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (latched : (work.trace time validator).pendingTryCommitRun = some output)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events time)
      validator .runCommitter) :
    (work.trace (time + 1) validator).pendingTryCommitResult = some output ∧
      timed.protectedAction (time + 1) validator
        (.recordCommit output.toCommitHead) := by
  rcases work.protectedRunReturnsLatchedResult time validator output latched occurs
      with ⟨input, returned⟩
  let observation : LocalFlexCommitterRunObservation BlockId CommitId :=
    { time
      validator
      input
      result := some output }
  have reflected := work.successfulResultIsReflected observation output
    (by simpa [observation] using returned) (by simp [observation])
  have reflectedAt :
      work.event time validator = .completeTryCommit output := by
    simpa [observation] using reflected
  have step := work.transitionsFollowRules time validator
  rw [reflectedAt] at step
  have result := complete_try_commit_sets_result step
  exact ⟨result.2, work.pendingResultIsProtected _ _ _ result.2⟩

/-- One indexed protected completion moves the same full output to record work. -/
theorem latched_try_commit_completion_moves_exact_result
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    {enabledAt : Time} {validator : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (latched :
      (work.trace enabledAt validator).pendingTryCommitRun = some output)
    (completion : ValidatorActionCompletion timed.execution
      timed.localActionBound validator .runCommitter enabledAt) :
    ∃ input,
      runtime.returned
          { time := completion.event.completedAt
            validator
            input
            result := some output } ∧
        (work.trace (completion.event.completedAt + 1) validator).pendingTryCommitResult =
          some output ∧
        timed.protectedAction (completion.event.completedAt + 1) validator
          (.recordCommit output.toCommitHead) := by
  have atCompletion := work.latchedRunSurvivesToCompletion enabledAt validator
    output completion latched
  rcases work.protectedRunReturnsLatchedResult completion.event.completedAt
      validator output atCompletion completion.occurs with
    ⟨input, returned⟩
  have moved := completed_try_commit_result_is_latched work atCompletion
    completion.occurs
  exact ⟨input, returned, moved.1, moved.2⟩

/-- A latched full result gives one bounded protected record action. -/
theorem latched_try_commit_result_runs_within_bound
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    {time : Time} {validator : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (validatorInRange : validator < config.authorityCount)
    (correctAvailable : faults.correctAvailable validator = true)
    (latched :
      (work.trace time validator).pendingTryCommitResult = some output) :
    ∃ completion : ValidatorActionCompletion timed.execution
        timed.localActionBound validator
          (.recordCommit output.toCommitHead) time,
      time ≤ completion.event.completedAt ∧
        completion.event.completedAt ≤ time + timed.localActionBound ∧
        ValidatorLocalActionOccurs
          (timed.execution.events completion.event.completedAt) validator
          (.recordCommit output.toCommitHead) := by
  exact protected_validator_action_completes_within_bound timed
    validatorInRange correctAvailable
    (work.pendingResultIsProtected time validator output latched)

/-- One indexed record completion clears its result latch and persists the
same exact local commit reference. -/
theorem latched_try_commit_result_completion_clears_and_persists
    (work : ValidatorTryCommitWorkExecution timed source runtime)
    {enabledAt : Time} {validator : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (latched :
      (work.trace enabledAt validator).pendingTryCommitResult = some output)
    (completion : ValidatorActionCompletion timed.execution
      timed.localActionBound validator (.recordCommit output.toCommitHead)
        enabledAt) :
    (work.trace (completion.event.completedAt + 1) validator).pendingTryCommitResult =
        none ∧
      ((timed.execution.trace (completion.event.completedAt + 1)).validatorState
          validator).installedCommitAt output.reference.index =
        some output.reference.digest ∧
      ((timed.execution.trace (completion.event.completedAt + 1)).validatorState
          validator).commitInstallSourceAt output.reference.index =
        some .localExecution := by
  have atCompletion := work.latchedResultSurvivesToCompletion enabledAt
    validator output completion latched
  have reflected := work.recordActionIsReflected completion.event.completedAt
    validator output atCompletion completion.occurs
  have workStep := work.transitionsFollowRules completion.event.completedAt
    validator
  rw [reflected] at workStep
  have cleared := complete_record_clears_result workStep
  have persisted := local_flex_record_occurrence_persists_to_batch_end
    (timed.execution.stepsFollowRules completion.event.completedAt)
    completion.occurs
  exact ⟨cleared, persisted⟩

end ValidatorTryCommitWorkExecution

/-! ## Optional commit transfer work -/

/-- One optional request for an exact commit after a local index. -/
structure ValidatorCommitRequestGoal (CommitId : Type) where
  peer : Nat
  afterIndex : Nat
  reference : ValidatorCommitHead CommitId

/-- One optional service task for retained exact replay data. -/
structure ValidatorCommitServiceGoal
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History) where
  peer : Nat
  afterIndex : Nat
  material : ValidatorExactReplayMaterial functions context

/-- Optional local request and service goals. -/
structure ValidatorCommitTransferWorkState
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History) where
  requestGoal : Option (ValidatorCommitRequestGoal CommitId)
  serviceGoal : Option (ValidatorCommitServiceGoal functions context)

/-- One local change to optional commit transfer work. -/
inductive ValidatorCommitTransferWorkEvent
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History) where
  | latchRequest (goal : ValidatorCommitRequestGoal CommitId)
  | completeRequest (goal : ValidatorCommitRequestGoal CommitId)
  | latchService (goal : ValidatorCommitServiceGoal functions context)
  | completeService (goal : ValidatorCommitServiceGoal functions context)
  | idle

/-- Explicit local transitions for optional commit transfer work. -/
inductive ValidatorCommitTransferWorkTransition
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History} :
    ValidatorCommitTransferWorkState functions context →
      ValidatorCommitTransferWorkEvent functions context →
      ValidatorCommitTransferWorkState functions context → Prop where
  | latchRequest state goal (empty : state.requestGoal = none) :
      ValidatorCommitTransferWorkTransition state (.latchRequest goal)
        { state with requestGoal := some goal }
  | completeRequest state goal (latched : state.requestGoal = some goal) :
      ValidatorCommitTransferWorkTransition state (.completeRequest goal)
        { state with requestGoal := none }
  | latchService state goal (empty : state.serviceGoal = none) :
      ValidatorCommitTransferWorkTransition state (.latchService goal)
        { state with serviceGoal := some goal }
  | completeService state goal (latched : state.serviceGoal = some goal) :
      ValidatorCommitTransferWorkTransition state (.completeService goal)
        { state with serviceGoal := none }
  | idle state :
      ValidatorCommitTransferWorkTransition state .idle state

/-- A request remains until its matching completion event. -/
theorem commit_request_persists_unless_completed
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorCommitTransferWorkState functions context}
    {event : ValidatorCommitTransferWorkEvent functions context}
    {goal : ValidatorCommitRequestGoal CommitId}
    (step : ValidatorCommitTransferWorkTransition before event after)
    (latched : before.requestGoal = some goal)
    (notCompleted : event ≠ .completeRequest goal) :
    after.requestGoal = some goal := by
  cases step <;> simp_all

/-- A service remains until its matching completion event. -/
theorem commit_service_persists_unless_completed
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {before after : ValidatorCommitTransferWorkState functions context}
    {event : ValidatorCommitTransferWorkEvent functions context}
    {goal : ValidatorCommitServiceGoal functions context}
    (step : ValidatorCommitTransferWorkTransition before event after)
    (latched : before.serviceGoal = some goal)
    (notCompleted : event ≠ .completeService goal) :
    after.serviceGoal = some goal := by
  cases step <;> simp_all

/-- Optional commit transfer adapter for the main execution. -/
structure ValidatorCommitTransferWorkExecution
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (core : ValidatorTryCommitWorkExecution timed source runtime) where
  trace : Time → Nat → ValidatorCommitTransferWorkState functions context
  event : Time → Nat → ValidatorCommitTransferWorkEvent functions context
  transitionsFollowRules : ∀ time validator,
    ValidatorCommitTransferWorkTransition (trace time validator)
      (event time validator) (trace (time + 1) validator)
  serviceLatchUsesRetainedReplay : ∀ time validator goal,
    event time validator = .latchService goal →
    (core.trace time validator).retainedReplay goal.material = true
  packetCarriesReplay : PacketId →
    ValidatorExactReplayMaterial functions context → Prop
  packetReplayUnique : ∀ packetId left right,
    packetCarriesReplay packetId left →
    packetCarriesReplay packetId right →
    left = right
  requestCompletionCreatesPacket : ∀ time validator goal,
    event time validator = .completeRequest goal →
    ∃ packetId packet,
      (timed.execution.trace (time + 1)).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.sender = validator ∧
        packet.receiver = goal.peer ∧
        packet.payload = .commitRequest goal.afterIndex ∧
        packet.sentAt = time + 1
  serviceCompletionCreatesPacket : ∀ time validator goal,
    event time validator = .completeService goal →
    ∃ packetId packet,
      (timed.execution.trace (time + 1)).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.sender = validator ∧
        packet.receiver = goal.peer ∧
        packet.payload = .commitData goal.material.output.toCommitHead ∧
        packet.sentAt = time + 1 ∧
        packetCarriesReplay packetId goal.material
  deliveredReplayIsReflected : ∀ time packetId packet material,
    (timed.execution.trace time).packets packetId = some packet →
    packetCarriesReplay packetId material →
    ValidatorPacketDeliveryOccurs (timed.execution.events time) packetId →
    core.event time packet.receiver = .retainReplay material

namespace ValidatorCommitTransferWorkExecution

variable {BlockId CommitId History Encoding PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {context : ValidatorFlexContextAt BlockId CommitId History}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : LocalFlexCommitterSourceMap config functions context program}
variable {runtime : LocalFlexCommitterRuntime timed source}

/-- Delivery of one exact replay packet stores that replay material locally. -/
theorem delivered_replay_is_retained_next
    {core : ValidatorTryCommitWorkExecution timed source runtime}
    (transfer : ValidatorCommitTransferWorkExecution core)
    {time : Time} {packetId : PacketId}
    {packet : AddressedPacket (ValidatorMessage BlockId CommitId)}
    {material : ValidatorExactReplayMaterial functions context}
    (stored : (timed.execution.trace time).packets packetId = some packet)
    (carries : transfer.packetCarriesReplay packetId material)
    (delivered :
      ValidatorPacketDeliveryOccurs (timed.execution.events time) packetId) :
    (core.trace (time + 1) packet.receiver).retainedReplay material = true := by
  have reflected := transfer.deliveredReplayIsReflected time packetId packet
    material stored carries delivered
  have step := core.transitionsFollowRules time packet.receiver
  rw [reflected] at step
  exact retain_replay_sets_entry step

end ValidatorCommitTransferWorkExecution

end Mysticeti
