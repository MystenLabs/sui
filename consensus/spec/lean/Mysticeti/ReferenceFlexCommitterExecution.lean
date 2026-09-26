/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ReferenceFlexCommitter

namespace Mysticeti

/-!
Pure exact-reference output types for one local FlexCommitter run.

This module does not use validator process state. The validator runtime and the
full direct-and-indirect scan can both import these types without an import
cycle.
-/

/-- The finite input for a pure candidate scan and commit build. -/
structure FiniteLocalFlexDag (BlockId CommitId : Type) where
  prior : ExactCommitReference CommitId
  flexState : ReferenceFlexState BlockId
  materialize : ReferenceFlexCandidate BlockId →
    ExactCommitBuildMaterial (LeaderBlockRef BlockId)

/-- The exact output of one successful local FlexCommitter run. -/
structure LocalFlexCommitOutput (BlockId CommitId : Type) where
  candidate : ReferenceFlexCandidate BlockId
  builderInput : ExactCommitBuilderInput CommitId (LeaderBlockRef BlockId)
  record : ExactCommitRecord CommitId (LeaderBlockRef BlockId)
  reference : ExactCommitReference CommitId

/-- Build the exact output for one candidate. The record, encoding, digest, and
reference are pure functions of the finite local input. -/
def buildLocalFlexCommitOutput
    {BlockId CommitId Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (dag : FiniteLocalFlexDag BlockId CommitId)
    (candidate : ReferenceFlexCandidate BlockId) :
    LocalFlexCommitOutput BlockId CommitId :=
  let builderInput := candidate.toBuilderInput dag.prior
    (dag.materialize candidate)
  let record := builderInput.toCommitRecord
  { candidate
    builderInput
    record
    reference := constructExactCommitReference functions record }

/-- The pure scan and build operation. `none` means that the first pending
prefix does not yet contain a final commit round. -/
def tryBuildLocalFlexCommit
    {BlockId CommitId Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (dag : FiniteLocalFlexDag BlockId CommitId) :
    Option (LocalFlexCommitOutput BlockId CommitId) :=
  match findReferenceFlexCommitCandidate dag.flexState.rounds with
  | none => none
  | some candidate => some (buildLocalFlexCommitOutput functions dag candidate)

@[simp]
theorem build_local_flex_commit_output_reference_index
    {BlockId CommitId Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (dag : FiniteLocalFlexDag BlockId CommitId)
    (candidate : ReferenceFlexCandidate BlockId) :
    (buildLocalFlexCommitOutput functions dag candidate).reference.index =
      dag.prior.index + 1 := by
  rfl

/-- Every successful pure result has the exact next index. -/
theorem try_build_local_flex_commit_reference_index
    {BlockId CommitId Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (dag : FiniteLocalFlexDag BlockId CommitId)
    {output : LocalFlexCommitOutput BlockId CommitId}
    (found : tryBuildLocalFlexCommit functions dag = some output) :
    output.reference.index = dag.prior.index + 1 := by
  unfold tryBuildLocalFlexCommit at found
  cases candidateResult :
      findReferenceFlexCommitCandidate dag.flexState.rounds with
  | none => simp [candidateResult] at found
  | some candidate =>
      simp only [candidateResult] at found
      have outputEqual :
          buildLocalFlexCommitOutput functions dag candidate = output :=
        Option.some.inj found
      rw [← outputEqual]
      exact build_local_flex_commit_output_reference_index functions dag candidate

/-- A successful result contains the exact candidate record and its
deterministic reference. -/
theorem try_build_local_flex_commit_exact_output
    {BlockId CommitId Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (dag : FiniteLocalFlexDag BlockId CommitId)
    {output : LocalFlexCommitOutput BlockId CommitId}
    (found : tryBuildLocalFlexCommit functions dag = some output) :
    output.builderInput = output.candidate.toBuilderInput dag.prior
        (dag.materialize output.candidate) ∧
      output.record = output.builderInput.toCommitRecord ∧
      output.reference =
        constructExactCommitReference functions output.record := by
  unfold tryBuildLocalFlexCommit at found
  cases candidateResult :
      findReferenceFlexCommitCandidate dag.flexState.rounds with
  | none => simp [candidateResult] at found
  | some candidate =>
      simp only [candidateResult] at found
      have outputEqual :
          buildLocalFlexCommitOutput functions dag candidate = output :=
        Option.some.inj found
      rw [← outputEqual]
      exact ⟨rfl, rfl, rfl⟩

end Mysticeti
