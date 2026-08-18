/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorReplayManifest

namespace Mysticeti

/-!
One-host execution of an authenticated replay manifest.

The replay input is not the validator's whole live DAG. It is a deterministic
view built from the exact ordered references in one accepted manifest. Every
block in that view must be accepted by this validator and present under its
exact identifier in the main block catalog. Extra local blocks and later rounds
cannot enter the pure `try_commit` call through this interface.
-/

/-- Deterministic manifest-scoped input construction at one validator. -/
structure ValidatorManifestReplaySourceMap
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  decisionDag : Nat → ValidatorWorldState BlockId CommitId PacketId →
    ValidatorReplayManifest BlockId CommitId → List (ValidatorBlock BlockId)
  buildSnapshot : ValidatorCommitHead CommitId → List (ValidatorBlock BlockId) →
    ReferenceFlexTryCommitInput BlockId CommitId
  buildContext : ValidatorCommitHead CommitId → List (ValidatorBlock BlockId) →
    ReferenceFlexCommitterContext BlockId History

namespace ValidatorManifestReplaySourceMap

variable {BlockId CommitId History Encoding PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}

/-- The pure input uses only the manifest prior and its exact decision DAG. -/
def snapshot
    (source : ValidatorManifestReplaySourceMap (History := History)
      functions timed)
    (validator : Nat) (world : ValidatorWorldState BlockId CommitId PacketId)
    (manifest : ValidatorReplayManifest BlockId CommitId) :
    ReferenceFlexTryCommitInput BlockId CommitId :=
  source.buildSnapshot manifest.prior
    (source.decisionDag validator world manifest)

/-- The pure context uses only the manifest prior and its exact decision DAG. -/
def replayContext
    (source : ValidatorManifestReplaySourceMap (History := History)
      functions timed)
    (validator : Nat) (world : ValidatorWorldState BlockId CommitId PacketId)
    (manifest : ValidatorReplayManifest BlockId CommitId) :
    ReferenceFlexCommitterContext BlockId History :=
  source.buildContext manifest.prior
    (source.decisionDag validator world manifest)

end ValidatorManifestReplaySourceMap

/-- One exact result from a manifest-scoped replay action. -/
structure ValidatorManifestReplayObservation
    (BlockId CommitId PacketId : Type) where
  time : Time
  validator : Nat
  manifest : ValidatorReplayManifest BlockId CommitId
  input : ValidatorWorldState BlockId CommitId PacketId
  result : Option (LocalFlexCommitOutput BlockId CommitId)

/-- The observation names an actual manifest replay action and its exact
pre-action world state. -/
def ValidatorManifestReplayObservation.OccursIn
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (observation : ValidatorManifestReplayObservation BlockId CommitId PacketId) :
    Prop :=
  ∃ beforeEvents afterEvents actionAfter,
    timed.execution.events observation.time =
      beforeEvents ++
        (.localAction observation.validator
          (.runReplayCommitter observation.manifest) :: afterEvents) ∧
      ValidatorWorldStep config faults protocolPacket program observation.time
        (timed.execution.trace observation.time) beforeEvents observation.input ∧
      ValidatorAtomicStep config faults protocolPacket program observation.time
        observation.input
        (.localAction observation.validator
          (.runReplayCommitter observation.manifest)) actionAfter ∧
      ValidatorWorldStep config faults protocolPacket program observation.time
        actionAfter afterEvents (timed.execution.trace (observation.time + 1))

/-- Exact runtime mapping for the dedicated manifest replay action. -/
structure ValidatorManifestReplayRuntime
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (source : ValidatorManifestReplaySourceMap (History := History)
      functions timed) where
  returned : ValidatorManifestReplayObservation BlockId CommitId PacketId → Prop
  actionOccurrenceReturnsExactResult :
    ∀ observation : ValidatorManifestReplayObservation BlockId CommitId PacketId,
    observation.OccursIn timed →
    returned
      { time := observation.time
        validator := observation.validator
        manifest := observation.manifest
        input := observation.input
        result := tryReferenceFlexCommitWithContext functions
          (source.replayContext observation.validator observation.input
            observation.manifest)
          (source.snapshot observation.validator observation.input
            observation.manifest) }
  everyReturnedResultIsExact :
    ∀ observation : ValidatorManifestReplayObservation BlockId CommitId PacketId,
    returned observation →
    observation.result = tryReferenceFlexCommitWithContext functions
      (source.replayContext observation.validator observation.input
        observation.manifest)
      (source.snapshot observation.validator observation.input
        observation.manifest)
  everyReturnedObservationOccurs :
    ∀ observation : ValidatorManifestReplayObservation BlockId CommitId PacketId,
    returned observation → observation.OccursIn timed
  successfulResultLatchesRecord : ∀ observation output,
    returned observation →
    observation.result = some output →
    timed.protectedAction (observation.time + 1) observation.validator
      (.recordCommit output.toCommitHead)

namespace ValidatorManifestReplayRuntime

variable {BlockId CommitId History Encoding PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {functions : CommitReferenceFunctions
  CommitId (LeaderBlockRef BlockId) Encoding}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {source : ValidatorManifestReplaySourceMap (History := History)
  functions timed}

/-- One actual manifest replay event exposes its exact pre-action input and
returns the exact filtered pure result. -/
theorem action_occurrence_returns_exact_result
    (runtime : ValidatorManifestReplayRuntime source)
    {time validator : Nat}
    {manifest : ValidatorReplayManifest BlockId CommitId}
    (occurs : ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.runReplayCommitter manifest)) :
    ∃ observation : ValidatorManifestReplayObservation BlockId CommitId PacketId,
      observation.time = time ∧
        observation.validator = validator ∧
        observation.manifest = manifest ∧
        runtime.returned observation ∧
        observation.result = tryReferenceFlexCommitWithContext functions
          (source.replayContext observation.validator observation.input
            observation.manifest)
          (source.snapshot observation.validator observation.input
            observation.manifest) := by
  rcases occurs with ⟨beforeEvents, afterEvents, eventsEq⟩
  have batch := timed.execution.stepsFollowRules time
  rw [eventsEq] at batch
  rcases validator_world_step_append_split batch with
    ⟨actionBefore, prefixStep, actionAndSuffix⟩
  cases actionAndSuffix with
  | cons actionStep suffixStep =>
      let observation : ValidatorManifestReplayObservation BlockId CommitId
          PacketId :=
        { time
          validator
          manifest
          input := actionBefore
          result := tryReferenceFlexCommitWithContext functions
            (source.replayContext validator actionBefore manifest)
            (source.snapshot validator actionBefore manifest) }
      have observationOccurs : observation.OccursIn timed := by
        exact ⟨beforeEvents, afterEvents, _, eventsEq, prefixStep, actionStep,
          suffixStep⟩
      have returned := runtime.actionOccurrenceReturnsExactResult observation
        observationOccurs
      exact ⟨observation, rfl, rfl, rfl, returned, rfl⟩

end ValidatorManifestReplayRuntime

end Mysticeti
