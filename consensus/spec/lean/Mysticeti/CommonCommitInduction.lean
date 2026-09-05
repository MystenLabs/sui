/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorExecutionLemmas

namespace Mysticeti

/-!
Finite induction for a common exact commit chain.

This file starts with a pointwise one-step result and proves the finite
aggregation and induction that follow from it. The pointwise result is not a
permitted input to the final liveness theorem. Another module must derive it
from block production, exact commit-rule safety, local FlexCommitter execution,
and commit synchronization.

The one-step result records how each correct, available validator completes the
step. A validator can already have the exact reference, make a local commit, or
install a verified synchronized commit. No case assumes a common future
candidate, certificate, or server.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- All correct, available validators have installed one exact reference. Their
current commit heads can be later descendants. -/
def AllCorrectAvailableInstalledExactAt
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (time : Time) (reference : ValidatorCommitHead CommitId) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).installedCommitAt reference.index =
        some reference.id ∧
      reference.index ≤ (trace time).localCommitIndex validator

/-- Every correct, available validator records a permitted source for one exact
installed reference. Genesis does not need this property. -/
def AllCorrectAvailableInstallSourcesAt
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (time : Time) (reference : ValidatorCommitHead CommitId) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).commitInstallSourceAt
          reference.index = some .localExecution ∨
      ((trace time).validatorState validator).commitInstallSourceAt
          reference.index = some .verifiedCommitSync

/-- The three per-validator cases in one common commit step. -/
inductive ValidatorCommitCompletionKind where
  | alreadyAhead
  | localCommit
  | verifiedCommitSync
  deriving DecidableEq

/-- Pointwise completion data for one derived common commit step.

This structure contains only results that the one-step proof must derive. It is
not part of the final theorem input. `kindSound` keeps the three implementation
paths explicit. -/
structure PointwiseCommonCommitStepResult
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (start : Time) (prior : ValidatorCommitHead CommitId) where
  next : ValidatorCommitHead CommitId
  nextIndex : next.index = prior.index + 1
  finishFor : Nat → Time
  kindFor : Nat → ValidatorCommitCompletionKind
  finishAfterStart : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    start ≤ finishFor validator
  installedAtFinish : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace (finishFor validator)).validatorState validator).installedCommitAt
        next.index = some next.id
  sourceAtFinish : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorLocalState.commitInstallSourceAt
        ((trace (finishFor validator)).validatorState validator) next.index =
        some CommitInstallSource.localExecution ∨
      ValidatorLocalState.commitInstallSourceAt
        ((trace (finishFor validator)).validatorState validator) next.index =
        some CommitInstallSource.verifiedCommitSync
  kindSound : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    match kindFor validator with
    | .alreadyAhead =>
        finishFor validator = start ∧
          next.index ≤ (trace start).localCommitIndex validator ∧
          ((trace start).validatorState validator).installedCommitAt next.index =
            some next.id
    | .localCommit =>
        ValidatorLocalState.commitInstallSourceAt
          ((trace (finishFor validator)).validatorState validator) next.index =
          some CommitInstallSource.localExecution
    | .verifiedCommitSync =>
        ValidatorLocalState.commitInstallSourceAt
          ((trace (finishFor validator)).validatorState validator) next.index =
          some CommitInstallSource.verifiedCommitSync

/-- The internal one-step result that the protocol proof must establish.

Do not add this proposition to `EndToEndLivenessInputs`. Its proof must use the
permitted fundamental inputs and the derived block, anchor, committer, safety,
and commit-sync lemmas. -/
def DerivedPointwiseCommonCommitStep
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) : Prop :=
  ∀ start prior,
    network.gst ≤ start →
    (∀ time, start ≤ time → (trace time).epochActive = true) →
    AllCorrectAvailableInstalledExactAt faults trace start prior →
    Nonempty (PointwiseCommonCommitStepResult faults trace start prior)

/-- The maximum of one start time and the first `count` validator completion
times. -/
def maximumCompletionTime
    (start : Time) (finishFor : Nat → Time) : Nat → Time
  | 0 => start
  | count + 1 =>
      Nat.max (maximumCompletionTime start finishFor count) (finishFor count)

theorem start_le_maximum_completion_time
    (start : Time) (finishFor : Nat → Time) (count : Nat) :
    start ≤ maximumCompletionTime start finishFor count := by
  induction count with
  | zero => exact Nat.le_refl _
  | succ count inductionHypothesis =>
      exact Nat.le_trans inductionHypothesis (Nat.le_max_left _ _)

theorem completion_time_le_maximum_completion_time
    (start : Time) (finishFor : Nat → Time)
    {validator count : Nat}
    (validatorInRange : validator < count) :
    finishFor validator ≤ maximumCompletionTime start finishFor count := by
  induction count with
  | zero => omega
  | succ count inductionHypothesis =>
      by_cases inEarlierRange : validator < count
      · exact Nat.le_trans (inductionHypothesis inEarlierRange)
          (Nat.le_max_left _ _)
      · have isLast : validator = count := by omega
        subst validator
        exact Nat.le_max_right _ _

/-- Pointwise completions have one finite common completion time because the
validator set is finite. Durable state keeps every earlier installation and
source entry at that time. -/
theorem pointwise_common_commit_step_has_common_finish
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {start : Time} {prior : ValidatorCommitHead CommitId}
    (result : PointwiseCommonCommitStepResult faults execution.trace start prior) :
    ∃ finish,
      start ≤ finish ∧
        AllCorrectAvailableInstalledExactAt faults execution.trace finish
          result.next ∧
        AllCorrectAvailableInstallSourcesAt faults execution.trace finish
          result.next := by
  let finish := maximumCompletionTime start result.finishFor
    config.authorityCount
  refine ⟨finish, start_le_maximum_completion_time start result.finishFor
    config.authorityCount, ?_, ?_⟩
  · intro validator validatorInRange validatorCorrect
    have completionBeforeFinish : result.finishFor validator ≤ finish :=
      completion_time_le_maximum_completion_time start result.finishFor
        validatorInRange
    have installedAtFinish := execution.installed_commit_persists
      validatorInRange completionBeforeFinish
      (result.installedAtFinish validator validatorInRange validatorCorrect)
    have withinHead :=
      (execution.statesWellFormed finish validator validatorInRange)
        |>.installedIndexIsNotFuture result.next.index result.next.id
          installedAtFinish
    exact ⟨installedAtFinish, withinHead⟩
  · intro validator validatorInRange validatorCorrect
    have completionBeforeFinish : result.finishFor validator ≤ finish :=
      completion_time_le_maximum_completion_time start result.finishFor
        validatorInRange
    have monotone := execution.durable_fields_persist validatorInRange
      completionBeforeFinish
    rcases result.sourceAtFinish validator validatorInRange validatorCorrect with
      localSource | syncSource
    · exact Or.inl (monotone.install_source_persists localSource)
    · exact Or.inr (monotone.install_source_persists syncSource)

/-- A later time inherits the active-epoch suffix of an earlier time. -/
theorem active_suffix_of_later_start
    {State : Type} {trace : Trace State} {active : State → Bool}
    {start later : Time}
    (ordered : start ≤ later)
    (activeFromStart : ∀ time, start ≤ time → active (trace time) = true) :
    ∀ time, later ≤ time → active (trace time) = true := by
  intro time laterBeforeTime
  exact activeFromStart time (Nat.le_trans ordered laterBeforeTime)

/-- The derived pointwise step gives a common exact installed reference after
any finite number of next-index steps. -/
theorem derived_common_commit_step_iterates
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (step : DerivedPointwiseCommonCommitStep faults network execution.trace)
    {start : Time} {base : ValidatorCommitHead CommitId}
    (afterGst : network.gst ≤ start)
    (activeFromStart :
      ∀ time, start ≤ time → (execution.trace time).epochActive = true)
    (baseInstalled :
      AllCorrectAvailableInstalledExactAt faults execution.trace start base)
    (count : Nat) :
    ∃ finish reference,
      start ≤ finish ∧
        reference.index = base.index + count ∧
        AllCorrectAvailableInstalledExactAt faults execution.trace finish
          reference ∧
        (0 < count →
          AllCorrectAvailableInstallSourcesAt faults execution.trace finish
            reference) := by
  induction count with
  | zero =>
      exact ⟨start, base, Nat.le_refl _, by omega, baseInstalled,
        fun positive => by omega⟩
  | succ count inductionHypothesis =>
      rcases inductionHypothesis with
        ⟨priorFinish, prior, startBeforePriorFinish, priorIndex,
          priorInstalled, _⟩
      have priorAfterGst : network.gst ≤ priorFinish :=
        Nat.le_trans afterGst startBeforePriorFinish
      have activeFromPriorFinish := active_suffix_of_later_start
        startBeforePriorFinish activeFromStart
      rcases step priorFinish prior priorAfterGst activeFromPriorFinish
          priorInstalled with
        ⟨result⟩
      rcases pointwise_common_commit_step_has_common_finish execution result with
        ⟨finish, priorFinishBeforeFinish, installed, sources⟩
      refine ⟨finish, result.next,
        Nat.le_trans startBeforePriorFinish priorFinishBeforeFinish, ?_,
        installed, ?_⟩
      · rw [result.nextIndex, priorIndex]
        omega
      · intro _
        exact sources

/-- The derived exact next-index step gives unbounded exact commit references
at correct, available validators.

The proof starts from durable genesis and uses only finite index induction. It
does not assume a future reference, commit certificate, or synchronization
server. -/
theorem derived_common_commit_step_proves_network_commit_progress
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (step : DerivedPointwiseCommonCommitStep faults network execution.trace)
    (genesis : ValidatorCommitHead CommitId)
    (genesisIndex : genesis.index = 0)
    (genesisInstalled : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((execution.trace 0).validatorState validator).installedCommitAt
        genesis.index = some genesis.id) :
    NetworkCommitProgressLiveness config faults network execution.trace := by
  intro start minimumIndex afterGst activeFromStart
  have genesisInstalledAtStart :
      AllCorrectAvailableInstalledExactAt faults execution.trace start
        genesis := by
    intro validator validatorInRange validatorCorrect
    have installed := execution.installed_commit_persists validatorInRange
      (Nat.zero_le start)
      (genesisInstalled validator validatorInRange validatorCorrect)
    have withinHead :=
      (execution.statesWellFormed start validator validatorInRange)
        |>.installedIndexIsNotFuture genesis.index genesis.id installed
    exact ⟨installed, withinHead⟩
  rcases derived_common_commit_step_iterates execution step afterGst
      activeFromStart genesisInstalledAtStart minimumIndex with
    ⟨finish, reference, startBeforeFinish, referenceIndex, installed, _⟩
  have liveWeightPositive :
      0 < weight config.authorityCount config.stake faults.correctAvailable :=
    Nat.lt_of_lt_of_le config.thresholds.quorum_positive
      faults.correct_available_stake_is_quorum
  rcases positive_weight_has_member liveWeightPositive with
    ⟨validator, validatorInRange, validatorCorrect, _validatorStake⟩
  refine ⟨validator, finish, reference, validatorInRange, validatorCorrect,
    startBeforeFinish, ?_, (installed validator validatorInRange
      validatorCorrect).1⟩
  rw [referenceIndex, genesisIndex]
  simp

/-- The derived exact next-index step makes every correct, available receiver
install each exact reference that a correct, available source installed after
GST.

The induction can construct a reference at the same index. The source has both
entries in one durable map, so their IDs are equal. This proof does not assume
a future synchronized commit or a future receiver run. -/
theorem derived_common_commit_step_proves_pointwise_commit_catch_up
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (step : DerivedPointwiseCommonCommitStep faults network execution.trace)
    (genesis : ValidatorCommitHead CommitId)
    (genesisIndex : genesis.index = 0)
    (genesisInstalled : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((execution.trace 0).validatorState validator).installedCommitAt
        genesis.index = some genesis.id) :
    PointwiseCommitCatchUpLiveness config faults network execution.trace := by
  intro source receiver installedAt reference sourceInRange sourceCorrect
    receiverInRange receiverCorrect afterGst activeFromInstalled sourceInstalled
  have genesisInstalledAtStart :
      AllCorrectAvailableInstalledExactAt faults execution.trace installedAt
        genesis := by
    intro validator validatorInRange validatorCorrect
    have installed := execution.installed_commit_persists validatorInRange
      (Nat.zero_le installedAt)
      (genesisInstalled validator validatorInRange validatorCorrect)
    have withinHead :=
      (execution.statesWellFormed installedAt validator validatorInRange)
        |>.installedIndexIsNotFuture genesis.index genesis.id installed
    exact ⟨installed, withinHead⟩
  rcases derived_common_commit_step_iterates execution step afterGst
      activeFromInstalled genesisInstalledAtStart reference.index with
    ⟨finish, commonReference, installedBeforeFinish, commonIndex,
      commonInstalled, _⟩
  have sameIndex : commonReference.index = reference.index := by
    rw [commonIndex, genesisIndex]
    simp
  have sourceOriginalAtFinish := execution.installed_commit_persists
    sourceInRange installedBeforeFinish sourceInstalled
  have sourceCommonAtFinish :=
    (commonInstalled source sourceInRange sourceCorrect).1
  have sourceCommonAtReference :
      ((execution.trace finish).validatorState source).installedCommitAt
          reference.index = some commonReference.id := by
    simpa only [sameIndex] using sourceCommonAtFinish
  have sameId : commonReference.id = reference.id :=
    Option.some.inj
      (sourceCommonAtReference.symm.trans sourceOriginalAtFinish)
  refine ⟨finish, installedBeforeFinish, ?_⟩
  have receiverCommonAtFinish :=
    (commonInstalled receiver receiverInRange receiverCorrect).1
  simpa only [sameIndex, sameId] using receiverCommonAtFinish

/-- The maximum local commit index among the first `count` validators at one
time. -/
def maximumLocalCommitIndexAt
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (time : Time) : Nat → Nat
  | 0 => 0
  | count + 1 =>
      Nat.max (maximumLocalCommitIndexAt trace time count)
        ((trace time).localCommitIndex count)

theorem local_commit_index_le_maximum_at
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (time : Time) {validator count : Nat}
    (validatorInRange : validator < count) :
    (trace time).localCommitIndex validator ≤
      maximumLocalCommitIndexAt trace time count := by
  induction count with
  | zero => omega
  | succ count inductionHypothesis =>
      by_cases inEarlierRange : validator < count
      · exact Nat.le_trans (inductionHypothesis inEarlierRange)
          (Nat.le_max_left _ _)
      · have isLast : validator = count := by omega
        subst validator
        exact Nat.le_max_right _ _

/-- Pure finite-index completion.

This theorem proves the complete induction after the internal pointwise step is
available. It is not the final end-to-end theorem because `step` is still a
derived protocol result. -/
theorem derived_common_commit_step_proves_commit_progress
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (step : DerivedPointwiseCommonCommitStep faults network execution.trace)
    (genesis : ValidatorCommitHead CommitId)
    (genesisInstalled : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((execution.trace 0).validatorState validator).installedCommitAt
        genesis.index = some genesis.id) :
    CommitProgressLiveness config faults network execution.trace := by
  intro start afterGst activeFromStart
  have genesisInstalledAtStart :
      AllCorrectAvailableInstalledExactAt faults execution.trace start
        genesis := by
    intro validator validatorInRange validatorCorrect
    have installed := execution.installed_commit_persists validatorInRange
      (Nat.zero_le start)
      (genesisInstalled validator validatorInRange validatorCorrect)
    have withinHead :=
      (execution.statesWellFormed start validator validatorInRange)
        |>.installedIndexIsNotFuture genesis.index genesis.id installed
    exact ⟨installed, withinHead⟩
  let maximum := maximumLocalCommitIndexAt execution.trace start
    config.authorityCount
  let count := maximum + 1
  rcases derived_common_commit_step_iterates execution step afterGst
      activeFromStart genesisInstalledAtStart count with
    ⟨finish, reference, startBeforeFinish, referenceIndex, installed,
      sourcesIfAdvanced⟩
  have sources := sourcesIfAdvanced (by simp [count])
  refine ⟨finish, reference.index, reference.id, startBeforeFinish, ?_⟩
  intro validator validatorInRange validatorCorrect
  have startIndexBound := local_commit_index_le_maximum_at execution.trace start
    validatorInRange
  have installedAtFinish :=
    installed validator validatorInRange validatorCorrect
  have sourceAtFinish := sources validator validatorInRange validatorCorrect
  refine ⟨?_, installedAtFinish.1, sourceAtFinish, installedAtFinish.2⟩
  dsimp [count, maximum] at referenceIndex ⊢
  omega

end Mysticeti
