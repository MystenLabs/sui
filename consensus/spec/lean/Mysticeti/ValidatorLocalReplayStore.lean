/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorReplayManifest

namespace Mysticeti

/-!
Durable exact replay material created by one successful local committer run.

Creation occurs in the same event batch as the run result. The material is
visible in the next state and then persists. A verified sync action cannot
create replay material through this interface. Exact install provenance must
first find the earlier successful local run.
-/

/-- One material value is the exact retained form of one successful run. -/
structure ValidatorReplayMaterialFromRun
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
    (runtime : LocalFlexCommitterRuntime timed source)
    (material : ValidatorExactReplayMaterial functions context)
    (observation : LocalFlexCommitterRunObservation BlockId CommitId)
    (output : LocalFlexCommitOutput BlockId CommitId) : Prop where
  validatorInRange : observation.validator < config.authorityCount
  validatorCorrect : faults.correctAvailable observation.validator = true
  returned : runtime.returned observation
  successful : observation.result = some output
  sourceValidator : material.sourceValidator = observation.validator
  sourceTime : material.sourceTime = observation.time
  sourceInput : material.sourceInput = observation.input
  outputExact : material.output = output
  snapshotExact : material.snapshot =
    source.snapshot observation.validator observation.input

/-- Durable retained-material bits at one validator. -/
structure ValidatorLocalReplayStoreState
    {BlockId CommitId History Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History) where
  retained : ValidatorExactReplayMaterial functions context → Bool

/-- One batch inserts the exact replay materials created by local runs. -/
structure ValidatorLocalReplayStoreTransition
    {BlockId CommitId History Encoding : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    (before : ValidatorLocalReplayStoreState functions context)
    (created : List (ValidatorExactReplayMaterial functions context))
    (after : ValidatorLocalReplayStoreState functions context) : Prop where
  retainedExact : ∀ material,
    after.retained material = true ↔
      before.retained material = true ∨ material ∈ created

/-- Main-trace mapping for exact local replay retention. -/
structure ValidatorLocalReplayStoreExecution
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
  trace : Time → Nat → ValidatorLocalReplayStoreState functions context
  created : Time → Nat →
    List (ValidatorExactReplayMaterial functions context)
  transitionsFollowRules : ∀ time validator,
    ValidatorLocalReplayStoreTransition (trace time validator)
      (created time validator) (trace (time + 1) validator)
  initialStateEmpty : ∀ validator material,
    (trace 0 validator).retained material = false
  successfulRunCreatesReplay : ∀ observation output,
    observation.validator < config.authorityCount →
    faults.correctAvailable observation.validator = true →
    runtime.returned observation →
    observation.result = some output →
    ∃ material,
      material ∈ created observation.time observation.validator ∧
        ValidatorReplayMaterialFromRun runtime material observation output
  createdReplayHasRunOrigin : ∀ time validator material,
    material ∈ created time validator →
    ∃ observation output,
      observation.time = time ∧
        observation.validator = validator ∧
        ValidatorReplayMaterialFromRun runtime material observation output
  /-- Retained replay material maps to actual pinned local block bodies. -/
  retainedMaterialHasHistory : ∀ time validator material,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (trace time validator).retained material = true →
    RetainedValidatorBlockHistory timed.execution validator material.blocks time

namespace ValidatorLocalReplayStoreExecution

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

/-- A material created in one batch is retained in the next state. -/
theorem created_replay_is_retained
    (store : ValidatorLocalReplayStoreExecution timed source runtime)
    {time validator : Nat}
    {material : ValidatorExactReplayMaterial functions context}
    (created : material ∈ store.created time validator) :
    (store.trace (time + 1) validator).retained material = true := by
  exact (store.transitionsFollowRules time validator).retainedExact material |>.2
    (Or.inr created)

/-- One retained material bit persists. -/
theorem retained_replay_persists
    (store : ValidatorLocalReplayStoreExecution timed source runtime)
    {earlier later validator : Nat}
    {material : ValidatorExactReplayMaterial functions context}
    (ordered : earlier ≤ later)
    (retained : (store.trace earlier validator).retained material = true) :
    (store.trace later validator).retained material = true := by
  induction later with
  | zero =>
      have same : earlier = 0 := by omega
      simpa [same] using retained
  | succ previous ih =>
      by_cases same : earlier = previous + 1
      · simpa [same] using retained
      · have before : earlier ≤ previous := by omega
        have retainedBefore := ih before
        exact (store.transitionsFollowRules previous validator).retainedExact
          material |>.2 (Or.inl retainedBefore)

/-- Every retained material value descends to one actual earlier successful
local run. -/
theorem retained_replay_has_run_origin
    (store : ValidatorLocalReplayStoreExecution timed source runtime)
    {time validator : Nat}
    {material : ValidatorExactReplayMaterial functions context}
    (retained : (store.trace time validator).retained material = true) :
    ∃ observation output,
      observation.time < time ∧
        observation.validator = validator ∧
        ValidatorReplayMaterialFromRun runtime material observation output := by
  induction time with
  | zero =>
      rw [store.initialStateEmpty validator material] at retained
      contradiction
  | succ previous ih =>
      have atNext :
          (store.trace (previous + 1) validator).retained material = true := by
        simpa [Nat.succ_eq_add_one] using retained
      rcases (store.transitionsFollowRules previous validator).retainedExact
          material |>.1 atNext with retainedBefore | created
      · rcases ih retainedBefore with
          ⟨observation, output, before, sameValidator, origin⟩
        exact ⟨observation, output, Nat.lt_succ_of_lt before, sameValidator,
          origin⟩
      · rcases store.createdReplayHasRunOrigin previous validator material
          created with ⟨observation, output, sameTime, sameValidator, origin⟩
        exact ⟨observation, output, by rw [sameTime]; omega, sameValidator,
          origin⟩

/-- One actual successful run creates exact replay material in its next state. -/
theorem successful_run_retains_exact_replay
    (store : ValidatorLocalReplayStoreExecution timed source runtime)
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (validatorInRange : observation.validator < config.authorityCount)
    (validatorCorrect :
      faults.correctAvailable observation.validator = true)
    (returned : runtime.returned observation)
    (successful : observation.result = some output) :
    ∃ material,
      (store.trace (observation.time + 1) observation.validator).retained
          material = true ∧
        ValidatorReplayMaterialFromRun runtime material observation output ∧
        RetainedValidatorBlockHistory timed.execution observation.validator
          material.blocks (observation.time + 1) := by
  rcases store.successfulRunCreatesReplay observation output validatorInRange
      validatorCorrect returned successful
      with ⟨material, created, origin⟩
  have retained := store.created_replay_is_retained created
  exact ⟨material, retained, origin,
    store.retainedMaterialHasHistory _ _ _ validatorInRange validatorCorrect
      retained⟩

end ValidatorLocalReplayStoreExecution

end Mysticeti
