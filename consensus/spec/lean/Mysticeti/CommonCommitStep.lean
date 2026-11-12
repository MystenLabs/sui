/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommonCommitInduction
import Mysticeti.ExactCommitPrefixSafety
import Mysticeti.ValidatorFlexCommitter
import Mysticeti.ValidatorLocalDagCommitPropagation

namespace Mysticeti

/-!
Composition of one common exact commit step.

The proof has two liveness parts.

* Network commit progress gives one correct, available validator that installs
  the next index.
* Pointwise commit catch-up gives each correct, available validator that is
  still at the prior index a later successful local FlexCommitter run.

The later run does not have to reproduce the first run's causal view. Its local
head cannot move backward. Therefore, its successful record puts the first
missing index in its durable prefix. Exact-prefix safety proves that this entry
is the same next entry that the first validator installed.

Commit-sync provenance remains only in the safety proof for an install that is
already present. Commit votes, certificates, replay, and commit-sync delivery
are not progress premises. The liveness propositions in this file are internal
theorem results. They are not permitted fields of the final liveness input.
-/

/-- A correct, available validator has installed some commit ID at one index. -/
def SomeCorrectAvailableInstalledAtIndex
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (time index : Nat) : Prop :=
  ∃ validator commitId,
    validator < config.authorityCount ∧
      faults.correctAvailable validator = true ∧
      ((trace time).validatorState validator).installedCommitAt index =
        some commitId

/-- Every correct, available validator has the exact same current commit head. -/
def AllCorrectAvailableCommitHeadsEqual
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (time : Nat) (reference : ValidatorCommitHead CommitId) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
      ValidatorLocalState.commitHead
      ((trace time).validatorState validator) = reference

/-- Private no-sync branch for pointwise catch-up.

If a correct, available validator is still at the prior index, the ordinary-DAG
proof can give a later successful local FlexCommitter run. A sync race can
instead install the exact target directly, so this run-valued result is not the
public pointwise catch-up target. -/
private def PointwiseLocalFlexProgress
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (network : AddressedPartialSynchrony config faults protocolPacket)
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source) : Prop :=
  ∀ start prior,
    network.gst ≤ start →
    (∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) →
    AllCorrectAvailableInstalledExactAt faults timed.execution.trace start
      prior →
    ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      (timed.execution.trace start).localCommitIndex validator = prior.index →
      ∃ run : CorrectExactFlexRun runtime,
        start ≤ run.observation.time ∧
          run.observation.validator = validator

/-- One validator's completion evidence for one exact next reference. -/
structure ValidatorExactCommitCompletion
    {BlockId CommitId PacketId : Type}
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (start validator : Nat) (reference : ValidatorCommitHead CommitId) where
  finish : Time
  kind : ValidatorCommitCompletionKind
  finishAfterStart : start ≤ finish
  installedAtFinish :
    ((trace finish).validatorState validator).installedCommitAt reference.index =
      some reference.id
  sourceAtFinish :
    ((trace finish).validatorState validator).commitInstallSourceAt
          reference.index = some .localExecution ∨
      ((trace finish).validatorState validator).commitInstallSourceAt
          reference.index = some .verifiedCommitSync
  kindSound :
    match kind with
    | .alreadyAhead =>
        finish = start ∧
          reference.index ≤ (trace start).localCommitIndex validator ∧
          ((trace start).validatorState validator).installedCommitAt
              reference.index = some reference.id
    | .localCommit =>
        ((trace finish).validatorState validator).commitInstallSourceAt
          reference.index = some .localExecution
    | .verifiedCommitSync =>
        ((trace finish).validatorState validator).commitInstallSourceAt
          reference.index = some .verifiedCommitSync

/-- Per-validator completion facts for one known exact reference. -/
def EveryCorrectAvailableValidatorCompletesReference
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (start : Nat) (reference : ValidatorCommitHead CommitId) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    Nonempty
      (ValidatorExactCommitCompletion trace start validator reference)

/-- Pointwise completion evidence can be packaged as the result used by the
finite common-commit induction. -/
theorem per_validator_completions_give_pointwise_result
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    {start : Nat} {prior next : ValidatorCommitHead CommitId}
    (nextIndex : next.index = prior.index + 1)
    (completes : EveryCorrectAvailableValidatorCompletesReference faults trace
      start next) :
    Nonempty (PointwiseCommonCommitStepResult faults trace start prior) := by
  classical
  let eligible : Nat → Prop := fun validator =>
    validator < config.authorityCount ∧
      faults.correctAvailable validator = true
  let selected : ∀ validator, eligible validator →
      ValidatorExactCommitCompletion trace start validator next :=
    fun validator proof =>
      Classical.choice (completes validator proof.1 proof.2)
  let finishFor : Nat → Time := fun validator =>
    if proof : eligible validator then (selected validator proof).finish
    else start
  let kindFor : Nat → ValidatorCommitCompletionKind := fun validator =>
    if proof : eligible validator then (selected validator proof).kind
    else .alreadyAhead
  refine ⟨{
    next := next
    nextIndex := nextIndex
    finishFor := finishFor
    kindFor := kindFor
    finishAfterStart := ?_
    installedAtFinish := ?_
    sourceAtFinish := ?_
    kindSound := ?_ }⟩
  · intro validator validatorInRange validatorCorrect
    have isEligible : eligible validator := ⟨validatorInRange, validatorCorrect⟩
    simpa [finishFor, isEligible] using
      (selected validator isEligible).finishAfterStart
  · intro validator validatorInRange validatorCorrect
    have isEligible : eligible validator := ⟨validatorInRange, validatorCorrect⟩
    simpa [finishFor, isEligible] using
      (selected validator isEligible).installedAtFinish
  · intro validator validatorInRange validatorCorrect
    have isEligible : eligible validator := ⟨validatorInRange, validatorCorrect⟩
    simpa [finishFor, isEligible] using
      (selected validator isEligible).sourceAtFinish
  · intro validator validatorInRange validatorCorrect
    have isEligible : eligible validator := ⟨validatorInRange, validatorCorrect⟩
    have sound := (selected validator isEligible).kindSound
    cases selectedKind : (selected validator isEligible).kind with
    | alreadyAhead =>
        simp [finishFor, kindFor, isEligible, selectedKind] at sound ⊢
        exact sound
    | localCommit =>
        simp [finishFor, kindFor, isEligible, selectedKind] at sound ⊢
        exact sound
    | verifiedCommitSync =>
        simp [finishFor, kindFor, isEligible, selectedKind] at sound ⊢
        exact sound

/-- Completion facts proved from a later start also complete a step from an
earlier start. The rebased kind is the durable install source at completion. -/
theorem per_validator_completions_rebase_start
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    {earlier later : Time} {reference : ValidatorCommitHead CommitId}
    (earlierBeforeLater : earlier ≤ later)
    (completes : EveryCorrectAvailableValidatorCompletesReference faults trace
      later reference) :
    EveryCorrectAvailableValidatorCompletesReference faults trace earlier
      reference := by
  intro validator validatorInRange validatorCorrect
  let completion := Classical.choice
    (completes validator validatorInRange validatorCorrect)
  rcases completion.sourceAtFinish with localSource | syncSource
  · exact ⟨{
      finish := completion.finish
      kind := .localCommit
      finishAfterStart := Nat.le_trans earlierBeforeLater
        completion.finishAfterStart
      installedAtFinish := completion.installedAtFinish
      sourceAtFinish := Or.inl localSource
      kindSound := localSource }⟩
  · exact ⟨{
      finish := completion.finish
      kind := .verifiedCommitSync
      finishAfterStart := Nat.le_trans earlierBeforeLater
        completion.finishAfterStart
      installedAtFinish := completion.installedAtFinish
      sourceAtFinish := Or.inr syncSource
      kindSound := syncSource }⟩

/-- Commit indexes do not decrease before one local action in an event batch. -/
private theorem common_commit_world_step_index_monotone
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
    (before.validatorState validator).commitHead.index ≤
      (after.validatorState validator).commitHead.index := by
  induction step with
  | nil => exact Nat.le_refl _
  | cons firstStep remainingSteps inductionHypothesis =>
      exact Nat.le_trans
        (validator_atomic_step_durable_monotone firstStep validator).1
        inductionHypothesis

/-- A later correct local FlexCommitter run cannot start below an entry that
was already installed at the same host.

This is a one-host durable-state fact. It does not use a network or commit
progress premise. -/
theorem later_correct_local_run_prior_index_not_behind
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (run : CorrectExactFlexRun runtime)
    {start index : Time} {commitId : CommitId}
    (startBeforeRun : start ≤ run.observation.time)
    (installed : ValidatorLocalState.installedCommitAt
      ((timed.execution.trace start).validatorState
        run.observation.validator) index = some commitId) :
    index ≤ run.prior.index := by
  have installedAtRun := timed.execution.installed_commit_persists
    run.validatorInRange startBeforeRun installed
  have indexAtTrace :=
    (timed.execution.statesWellFormed run.observation.time
      run.observation.validator run.validatorInRange)
      |>.installedIndexIsNotFuture index commitId installedAtRun
  rcases run.occurs with
    ⟨beforeEvents, afterEvents, actionBefore, actionAfter, eventSplit,
      prefixStep, _actionStep, _suffixStep, inputExact⟩
  have prefixMonotone := common_commit_world_step_index_monotone prefixStep
    run.observation.validator
  calc
    index ≤ (timed.execution.trace run.observation.time).localCommitIndex
        run.observation.validator := indexAtTrace
    _ ≤ (actionBefore.validatorState
        run.observation.validator).commitHead.index := prefixMonotone
    _ = run.observation.input.commitHead.index := by rw [inputExact]
    _ = run.prior.index := by rw [run.priorAtInput]

/-- Convert the one-requester local-DAG race split to an exact completion.

The second branch already contains the actual peer run and its local record.
The first branch contains only an installed next-index ID. The source run also
records its output locally, and exact-prefix safety identifies the two stored
IDs. Thus this theorem does not use a future commit-sync action. -/
theorem local_dag_run_or_installed_next_gives_exact_completion
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source)
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (sourceRun : CorrectExactFlexRun runtime)
    {start observer : Nat} {prior : ValidatorCommitHead CommitId}
    (sourcePrior : sourceRun.prior = prior)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (attempt :
      (∃ finish witnessId,
        start ≤ finish ∧
          ValidatorLocalState.installedCommitAt
            ((timed.execution.trace finish).validatorState observer)
              (prior.index + 1) = some witnessId) ∨
      ∃ (peerRun : CorrectExactFlexRun runtime) (installedAt : Time),
        sourceRun.observation.time ≤ peerRun.observation.time ∧
          start ≤ peerRun.observation.time ∧
          peerRun.observation.validator = observer ∧
          peerRun.prior = prior ∧
          peerRun.output = sourceRun.output ∧
          peerRun.observation.time < installedAt ∧
          ValidatorLocalState.installedCommitAt
              ((timed.execution.trace installedAt).validatorState observer)
                sourceRun.output.reference.index =
            some sourceRun.output.reference.digest ∧
          ValidatorLocalState.commitInstallSourceAt
              ((timed.execution.trace installedAt).validatorState observer)
                sourceRun.output.reference.index =
            some .localExecution) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace start
      observer sourceRun.output.toCommitHead) := by
  have sourceNextIndex : sourceRun.output.reference.index = prior.index + 1 := by
    change sourceRun.output.toCommitHead.index = prior.index + 1
    rw [ExactFlexSuccessor.correctRunAdvancesOneIndex sourceRun, sourcePrior]
  rcases attempt with installedNext | localRun
  · rcases installedNext with
      ⟨finish, witnessId, startBeforeFinish, witnessInstalled⟩
    rcases successful_local_flex_run_completes_and_persists_exact runtime
        sourceRun.observation sourceRun.returned sourceRun.successful
        sourceRun.validatorInRange sourceRun.validatorCorrect with
      ⟨sourceRecordAt, _sourceExactResult, _sourceRunBeforeRecord,
        _sourceRecordBound, sourceInstalled, _sourceLocal⟩
    have sourceStoredAtNext :
        ValidatorLocalState.installedCommitAt
          ((timed.execution.trace (sourceRecordAt + 1)).validatorState
            sourceRun.observation.validator) (prior.index + 1) =
          some sourceRun.output.reference.digest := by
      rw [← sourceNextIndex]
      exact sourceInstalled
    have sameId : sourceRun.output.reference.digest = witnessId :=
      provenance.storedIdsAtSameIndexAgree authenticated
        sourceRun.validatorInRange sourceRun.validatorCorrect observerInRange
        observerCorrect sourceStoredAtNext witnessInstalled
    have exactInstalled :
        ValidatorLocalState.installedCommitAt
            ((timed.execution.trace finish).validatorState observer)
              sourceRun.output.toCommitHead.index =
          some sourceRun.output.toCommitHead.id := by
      change ValidatorLocalState.installedCommitAt
          ((timed.execution.trace finish).validatorState observer)
            sourceRun.output.reference.index =
        some sourceRun.output.reference.digest
      rw [sourceNextIndex, sameId]
      exact witnessInstalled
    have permitted :=
      (timed.execution.statesWellFormed finish observer observerInRange)
        |>.installedCommitHasPermittedSource
          sourceRun.output.toCommitHead.index sourceRun.output.toCommitHead.id
          (by
            change 0 < sourceRun.output.reference.index
            rw [sourceNextIndex]
            omega) exactInstalled
    rcases permitted with localSource | syncSource
    · exact ⟨{
        finish := finish
        kind := .localCommit
        finishAfterStart := startBeforeFinish
        installedAtFinish := exactInstalled
        sourceAtFinish := Or.inl localSource
        kindSound := localSource }⟩
    · exact ⟨{
        finish := finish
        kind := .verifiedCommitSync
        finishAfterStart := startBeforeFinish
        installedAtFinish := exactInstalled
        sourceAtFinish := Or.inr syncSource
        kindSound := syncSource }⟩
  · rcases localRun with
      ⟨peerRun, installedAt, _sourceBeforePeer, startBeforePeer,
        peerValidator, _peerPrior, _sameOutput, peerBeforeInstall,
        installed, localSource⟩
    exact ⟨{
      finish := installedAt
      kind := .localCommit
      finishAfterStart := Nat.le_trans startBeforePeer
        (Nat.le_of_lt peerBeforeInstall)
      installedAtFinish := by
        change ValidatorLocalState.installedCommitAt
            ((timed.execution.trace installedAt).validatorState observer)
              sourceRun.output.reference.index =
          some sourceRun.output.reference.digest
        exact installed
      sourceAtFinish := Or.inl localSource
      kindSound := localSource }⟩

/-- A provenance-backed exact prefix entry gives one completion as soon as a
correct host's durable head reaches that index.

The install source can be local execution or an earlier verified sync. This
theorem uses sync only to classify an entry that is already durable. It does
not use sync as a progress step. -/
theorem exact_prefix_entry_at_or_below_head_gives_completion
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {start commonTime commonValidator finish validator : Time}
    {reference : ValidatorCommitHead CommitId}
    (startBeforeFinish : start ≤ finish)
    (commonValidatorInRange : commonValidator < config.authorityCount)
    (commonValidatorCorrect :
      faults.correctAvailable commonValidator = true)
    (commonInstalled :
      durable.exactInstalledHead commonTime commonValidator reference)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true)
    (positive : 0 < reference.index)
    (atOrAbove : reference.index ≤
      (timed.execution.trace finish).localCommitIndex validator) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace start
      validator reference) := by
  have installed := provenance.exactHeadAtOrBelowLocalHeadIsStored
    authenticated commonValidatorInRange commonValidatorCorrect
      commonInstalled validatorInRange validatorCorrect atOrAbove
  have permitted :=
    (timed.execution.statesWellFormed finish validator validatorInRange)
      |>.installedCommitHasPermittedSource reference.index reference.id
        positive installed
  rcases permitted with localSource | syncSource
  · exact ⟨{
      finish := finish
      kind := .localCommit
      finishAfterStart := startBeforeFinish
      installedAtFinish := installed
      sourceAtFinish := Or.inl localSource
      kindSound := localSource }⟩
  · exact ⟨{
      finish := finish
      kind := .verifiedCommitSync
      finishAfterStart := startBeforeFinish
      installedAtFinish := installed
      sourceAtFinish := Or.inr syncSource
      kindSound := syncSource }⟩

/-- If a host with the prior entry later makes any successful local commit,
its durable prefix contains the exact next entry that another correct host
already installed.

The later run can use a different view and can commit at a later index. Local
state cannot move backward, and every local commit advances by one index.
Exact-prefix safety identifies the first missing entry. -/
theorem installed_next_precedes_any_later_local_commit
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source)
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {start commonTime commonValidator : Time}
    {prior next : ValidatorCommitHead CommitId}
    (nextIndex : next.index = prior.index + 1)
    (commonValidatorInRange : commonValidator < config.authorityCount)
    (commonValidatorCorrect :
      faults.correctAvailable commonValidator = true)
    (commonInstalled :
      durable.exactInstalledHead commonTime commonValidator next)
    (run : CorrectExactFlexRun runtime)
    (startBeforeRun : start ≤ run.observation.time)
    (priorInstalledAtStart : ValidatorLocalState.installedCommitAt
      ((timed.execution.trace start).validatorState
        run.observation.validator) prior.index = some prior.id) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace start
      run.observation.validator next) := by
  have runPriorNotBehind := later_correct_local_run_prior_index_not_behind run
    startBeforeRun priorInstalledAtStart
  rcases successful_local_flex_run_completes_and_persists_exact runtime
      run.observation run.returned run.successful run.validatorInRange
      run.validatorCorrect with
    ⟨recordAt, _exactResult, runBeforeRecord, _recordBound,
      outputInstalled, _outputLocalSource⟩
  have outputIndex := ExactFlexSuccessor.correctRunAdvancesOneIndex run
  have outputAtOrBelowHead :=
    (timed.execution.statesWellFormed (recordAt + 1)
      run.observation.validator run.validatorInRange)
      |>.installedIndexIsNotFuture run.output.toCommitHead.index
        run.output.toCommitHead.id (by exact outputInstalled)
  have nextAtOrBelowHead : next.index ≤
      (timed.execution.trace (recordAt + 1)).localCommitIndex
        run.observation.validator := by
    change next.index ≤
      ((timed.execution.trace (recordAt + 1)).validatorState
        run.observation.validator).commitHead.index
    calc
      next.index = prior.index + 1 := nextIndex
      _ ≤ run.prior.index + 1 := Nat.succ_le_succ runPriorNotBehind
      _ = run.output.toCommitHead.index := outputIndex.symm
      _ ≤ ((timed.execution.trace (recordAt + 1)).validatorState
          run.observation.validator).commitHead.index := outputAtOrBelowHead
  have startBeforeInstall : start ≤ recordAt + 1 :=
    Nat.le_trans startBeforeRun
      (Nat.le_trans (Nat.le_succ run.observation.time)
        (Nat.le_trans runBeforeRecord (Nat.le_succ recordAt)))
  exact exact_prefix_entry_at_or_below_head_gives_completion
    (start := start) (commonTime := commonTime)
    (commonValidator := commonValidator) (finish := recordAt + 1)
    (validator := run.observation.validator) (reference := next) durable
      authenticated provenance startBeforeInstall commonValidatorInRange
      commonValidatorCorrect commonInstalled run.validatorInRange
      run.validatorCorrect (by rw [nextIndex]; omega) nextAtOrBelowHead

/-- Network commit progress and pointwise catch-up give one common exact next
entry.

Network progress can first return an entry above the next index. Durable prefix
lookup recovers the first missing index. Pointwise catch-up then installs that
exact entry at each correct, available validator. The finite induction can
choose one common finish time later. -/
theorem network_commit_progress_and_pointwise_catch_up_give_pointwise_step
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (networkProgress : NetworkCommitProgressLiveness config faults network
      timed.execution.trace)
    (catchUp : PointwiseCommitCatchUpLiveness config faults network
      timed.execution.trace) :
    DerivedPointwiseCommonCommitStep faults network
      timed.execution.trace := by
  intro start prior afterGst activeFromStart _priorInstalled
  rcases networkProgress start (prior.index + 1) afterGst activeFromStart with
    ⟨sourceValidator, installedAt, laterReference, sourceInRange,
      sourceCorrect, startBeforeInstalled, laterIndex,
      laterInstalled⟩
  have laterAtOrBelowHead : laterReference.index ≤
      (timed.execution.trace installedAt).localCommitIndex sourceValidator :=
    (timed.execution.statesWellFormed installedAt sourceValidator
      sourceInRange).installedIndexIsNotFuture laterReference.index
        laterReference.id laterInstalled
  have nextAtOrBelowHead : prior.index + 1 ≤
      (timed.execution.trace installedAt).localCommitIndex sourceValidator :=
    Nat.le_trans laterIndex laterAtOrBelowHead
  rcases durable.installedAtOrBelowHead installedAt sourceValidator
      (prior.index + 1) sourceInRange sourceCorrect nextAtOrBelowHead with
    ⟨nextId, nextStored⟩
  rcases provenance.exactHeadForStoredId sourceInRange sourceCorrect nextStored
      with
    ⟨next, nextExact, nextIndex, _nextId⟩
  have installedAfterGst : network.gst ≤ installedAt :=
    Nat.le_trans afterGst startBeforeInstalled
  have activeFromInstalled := active_suffix_of_later_start
    startBeforeInstalled activeFromStart
  have sourceHasNext : ValidatorLocalState.installedCommitAt
      ((timed.execution.trace installedAt).validatorState sourceValidator)
        next.index = some next.id :=
    durable.exactHeadHasStoredId installedAt sourceValidator next nextExact
  have completes : EveryCorrectAvailableValidatorCompletesReference faults
      timed.execution.trace start next := by
    intro receiver receiverInRange receiverCorrect
    rcases catchUp sourceValidator receiver installedAt next sourceInRange
        sourceCorrect receiverInRange receiverCorrect installedAfterGst
        activeFromInstalled sourceHasNext with
      ⟨finish, installedBeforeFinish, receiverHasNext⟩
    have nextPositive : 0 < next.index := by
      rw [nextIndex]
      omega
    have permitted :=
      (timed.execution.statesWellFormed finish receiver receiverInRange)
        |>.installedCommitHasPermittedSource next.index next.id nextPositive
          receiverHasNext
    rcases permitted with localSource | syncSource
    · exact ⟨{
        finish := finish
        kind := .localCommit
        finishAfterStart := Nat.le_trans startBeforeInstalled
          installedBeforeFinish
        installedAtFinish := receiverHasNext
        sourceAtFinish := Or.inl localSource
        kindSound := localSource }⟩
    · exact ⟨{
        finish := finish
        kind := .verifiedCommitSync
        finishAfterStart := Nat.le_trans startBeforeInstalled
          installedBeforeFinish
        installedAtFinish := receiverHasNext
        sourceAtFinish := Or.inr syncSource
        kindSound := syncSource }⟩
  exact per_validator_completions_give_pointwise_result nextIndex completes

/-- One exact local successor is visible before an actual ordinary carrier is
persisted. This is a one-host execution-order witness. The carrier can then be
served by ordinary block synchronization. -/
structure LocalSuccessorBeforeOrdinaryCarrier
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
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (sourceRun : CorrectExactFlexRun runtime)
    (sourceTime : Time) (target : ValidatorBlockRef BlockId) where
  installTime : Time
  carrierTime : Time
  carrier : ValidatorBlock BlockId
  carrierReference : carrier.reference = target
  runBeforeInstall : sourceRun.observation.time < installTime
  installed : ValidatorLocalState.installedCommitAt
    ((timed.execution.trace installTime).validatorState
      sourceRun.observation.validator) sourceRun.output.reference.index =
      some sourceRun.output.reference.digest
  localSource : ValidatorLocalState.commitInstallSourceAt
    ((timed.execution.trace installTime).validatorState
      sourceRun.observation.validator) sourceRun.output.reference.index =
      some .localExecution
  installBeforeCarrier : installTime ≤ carrierTime
  carrierPersists : ValidatorLocalActionOccurs
    (timed.execution.events carrierTime) carrier.reference.author
      (.persistProposal carrier)
  carrierVisibleBySource : carrierTime + 1 ≤ sourceTime

/-- Derived delivery of one post-install ordinary carrier.

The carrier, its full exact evidence coverage, and each accepted requester
closure are lower theorem results. This structure is an internal composition
value. It must not be a field of the end-to-end liveness inputs. -/
structure DerivedPostInstallOrdinaryCarrierDelivery
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
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
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (sourceRun : CorrectExactFlexRun runtime) (start : Time) where
  sourceTime : Time
  target : ValidatorBlockRef BlockId
  baseRound : Nat
  leaderAt : Nat → ValidatorBlockRef BlockId
  sourceBeforeCarrier : LocalSuccessorBeforeOrdinaryCarrier sourceRun
    sourceTime target
  requesterClosure : ∀ requester,
    requester < config.authorityCount →
    faults.correctAvailable requester = true →
    ((timed.execution.trace start).validatorState requester).commitHead =
      sourceRun.prior →
    ∃ finish,
      sourceTime ≤ finish ∧
        start ≤ finish ∧
        ValidatorExactFlexCarrierCoverage config faults
          (timed.execution.trace sourceTime) target leaderAt
            ((context requester
              ((timed.execution.trace finish).validatorState requester)).depth +
              1) sourceRun.output ∧
        sourceRun.prior.round < baseRound ∧
        (∀ offset,
          offset < (context requester
            ((timed.execution.trace finish).validatorState requester)).depth +
              1 →
          (leaderAt offset).round = baseRound + offset) ∧
        (∀ offset,
          offset < (context requester
            ((timed.execution.trace finish).validatorState requester)).depth +
              1 →
          (config.selectedLeaderOrder sourceRun.prior.id
            (baseRound + offset)).head? = some (leaderAt offset).author) ∧
        ValidatorAcceptedCausalClosureAboveRound
          (timed.execution.trace finish) requester
            ((timed.execution.trace finish).validatorState requester).gcRound
            target

/-- One accepted post-install ordinary carrier closure completes the exact
source successor at one correct requester.

The carrier descends from the recovery window's final block. Therefore its
ordinary parent closure retains the original direct-vote frontier, ordered
committed leaders, indirect anchor evidence, and material. The execution-order
witness requires the exact local install before this carrier is persisted.
Exact-prefix safety is used only if the requester races ahead before its local
record. -/
theorem accepted_ordinary_dag_carrier_gives_exact_successor_completion
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
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
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (pending : ValidatorExactPendingIngestionRules source)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    (work : ValidatorSuccessfulFlexScanWorkRules (faults := faults)
      (network := network) (timed := timed) source)
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (sourceRun : CorrectExactFlexRun runtime)
    {sourceTime peerStart finish requester baseRound : Time}
    {prior : ValidatorCommitHead CommitId}
    {target : ValidatorBlockRef BlockId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (coverage : ValidatorExactFlexCarrierCoverage config faults
      (timed.execution.trace sourceTime) target leaderAt
        ((context requester
          ((timed.execution.trace finish).validatorState requester)).depth + 1)
        sourceRun.output)
    (sourcePrior : sourceRun.prior = prior)
    (sourceBeforeCarrier : LocalSuccessorBeforeOrdinaryCarrier sourceRun
      sourceTime target)
    (sourceBeforeFinish : sourceTime ≤ finish)
    (peerStartBeforeFinish : peerStart ≤ finish)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (headAtPeerStart :
      ((timed.execution.trace peerStart).validatorState requester).commitHead =
        prior)
    (baseAfterPrior : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context requester
        ((timed.execution.trace finish).validatorState requester)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context requester
        ((timed.execution.trace finish).validatorState requester)).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (acceptedClosure : ValidatorAcceptedCausalClosureAboveRound
      (timed.execution.trace finish) requester
        ((timed.execution.trace finish).validatorState requester).gcRound
        target) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace peerStart
      requester sourceRun.output.toCommitHead) := by
  have sourceRunBeforeCarrier : sourceRun.observation.time ≤ sourceTime := by
    exact Nat.le_trans (Nat.le_of_lt sourceBeforeCarrier.runBeforeInstall)
      (Nat.le_trans sourceBeforeCarrier.installBeforeCarrier
        (Nat.le_trans (Nat.le_add_right sourceBeforeCarrier.carrierTime 1)
          sourceBeforeCarrier.carrierVisibleBySource))
  apply local_dag_run_or_installed_next_gives_exact_completion runtime durable
    authenticated provenance sourceRun sourcePrior requesterInRange
      requesterCorrect
  exact accepted_carrier_coverage_records_same_successor_or_installed_next
    source runtime prefixMap pending direct work authenticated representatives
      sourceRun leaderAt coverage sourcePrior sourceRunBeforeCarrier
      sourceBeforeFinish peerStartBeforeFinish requesterInRange requesterCorrect
      headAtPeerStart baseAfterPrior leaderRound firstSelected acceptedClosure

/-- A derived post-install carrier delivery completes the exact successor at
every correct requester.

A requester whose head already advanced uses its durable next-index entry. A
requester still at the source prior accepts the carrier closure, runs the local
FlexCommitter, and records the source output. This theorem does not assume a
future successful peer run. -/
theorem derived_post_install_ordinary_carrier_delivery_gives_exact_successor
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
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
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (pending : ValidatorExactPendingIngestionRules source)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    (work : ValidatorSuccessfulFlexScanWorkRules (faults := faults)
      (network := network) (timed := timed) source)
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (sourceRun : CorrectExactFlexRun runtime)
    {start : Time}
    (priorInstalled : AllCorrectAvailableInstalledExactAt faults
      timed.execution.trace start sourceRun.prior)
    (delivery : DerivedPostInstallOrdinaryCarrierDelivery sourceRun start) :
    EveryCorrectAvailableValidatorCompletesReference faults
      timed.execution.trace start sourceRun.output.toCommitHead := by
  intro requester requesterInRange requesterCorrect
  have requesterPrior := priorInstalled requester requesterInRange
    requesterCorrect
  by_cases advanced : sourceRun.prior.index <
      (timed.execution.trace start).localCommitIndex requester
  · have nextAtOrBelowHead : sourceRun.prior.index + 1 ≤
        (timed.execution.trace start).localCommitIndex requester := by
      omega
    rcases prefixMap.installedAtOrBelowHead start requester
        (sourceRun.prior.index + 1) requesterInRange requesterCorrect
        nextAtOrBelowHead with
      ⟨witnessId, installedNext⟩
    apply local_dag_run_or_installed_next_gives_exact_completion runtime durable
      authenticated provenance sourceRun rfl requesterInRange requesterCorrect
    exact Or.inl ⟨start, witnessId, Nat.le_refl start, installedNext⟩
  · have sameIndex :
        (timed.execution.trace start).localCommitIndex requester =
          sourceRun.prior.index := by
      omega
    have headAtPrior :
        ((timed.execution.trace start).validatorState requester).commitHead =
          sourceRun.prior :=
      prefixMap.sameIndexInstalledHeadIsExact start requester sourceRun.prior
        requesterInRange requesterCorrect requesterPrior.1 sameIndex
    rcases delivery.requesterClosure requester requesterInRange requesterCorrect
        headAtPrior with
      ⟨finish, sourceBeforeFinish, startBeforeFinish, coverage,
        baseAfterPrior, leaderRound, firstSelected, acceptedClosure⟩
    exact accepted_ordinary_dag_carrier_gives_exact_successor_completion source
      runtime prefixMap pending direct work durable authenticated provenance
        representatives sourceRun delivery.leaderAt coverage rfl
        delivery.sourceBeforeCarrier sourceBeforeFinish startBeforeFinish
        requesterInRange requesterCorrect headAtPrior baseAfterPrior leaderRound
        firstSelected acceptedClosure

/-- Internal existing-reference branch.

The proof of this proposition must use exact-reference safety, common-chain
lookup, local commit execution, and the verified commit-sync transition. It
must derive the exact reference from the installed witness. -/
def KnownNextReferencePropagation
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) : Prop :=
  ∀ start prior witnessValidator witnessId,
    network.gst ≤ start →
    (∀ time, start ≤ time → (trace time).epochActive = true) →
    AllCorrectAvailableInstalledExactAt faults trace start prior →
    witnessValidator < config.authorityCount →
    faults.correctAvailable witnessValidator = true →
    ((trace start).validatorState witnessValidator).installedCommitAt
        (prior.index + 1) = some witnessId →
    ∃ next,
      next.index = prior.index + 1 ∧
        next.id = witnessId ∧
        EveryCorrectAvailableValidatorCompletesReference faults trace start next

/-- One actual local FlexCommitter output produced by the full recovery scan.

This is the missing recovery-to-committer trace result. Its proof must use the
worst-case Rust scan: for indirect depth `d`, it needs `d + 1` consecutive
usable anchor rounds. The last anchor needs its next-round voting layer, so the
block-production proof needs `d + 2` quorum block layers.

The restricted target-and-anchor result in
`FavorableRecoveryCommitAgreement` is not sufficient here unless the target is
already the first unresolved pending round. This result covers an arbitrary
earlier undecided pending prefix. It ends at one actual successful
`runCommitter` result, or at a next-index commit that another correct, available
validator installed before that run completed. It does not require all
validators to become ready at the same time. The protected local action or the
racing install creates the first witness. Ordinary DAG propagation then
completes the step. -/
def FullRecoveryScanFirstFlexOutput
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (program : ValidatorExecutionProgram BlockId CommitId)
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (context : ValidatorFlexContextAt BlockId CommitId History)
    (source : LocalFlexCommitterSourceMap config functions
      context program)
    (runtime : LocalFlexCommitterRuntime timed source) : Prop :=
  ∀ start prior,
    network.gst ≤ start →
    (∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) →
    AllCorrectAvailableInstalledExactAt faults timed.execution.trace start prior →
    AllCorrectAvailableCommitHeadsEqual faults timed.execution.trace start prior →
    (∃ (observation : LocalFlexCommitterRunObservation BlockId CommitId)
          (output : LocalFlexCommitOutput BlockId CommitId),
        start ≤ observation.time ∧
          observation.validator < config.authorityCount ∧
          faults.correctAvailable observation.validator = true ∧
          runtime.returned observation ∧
          observation.result = some output ∧
          observation.input.commitHead = prior ∧
          output.toCommitHead.index = prior.index + 1) ∨
      (∃ (finish validator : Time) (witnessId : CommitId),
        start ≤ finish ∧
          validator < config.authorityCount ∧
          faults.correctAvailable validator = true ∧
          ValidatorLocalState.installedCommitAt
              ((timed.execution.trace finish).validatorState validator)
              (prior.index + 1) = some witnessId)

/-- The full scan either records its exact output or observes another installed
next-index witness. -/
theorem full_recovery_scan_or_race_gives_witness
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (fullScan : FullRecoveryScanFirstFlexOutput faults network program timed
      functions context source runtime)
    {start : Time} {prior : ValidatorCommitHead CommitId}
    (afterGst : network.gst ≤ start)
    (activeFromStart : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (priorInstalled : AllCorrectAvailableInstalledExactAt faults
      timed.execution.trace start prior)
    (headsEqual : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior) :
    ∃ (finish validator : Nat) (witnessId : CommitId),
      start ≤ finish ∧
        validator < config.authorityCount ∧
        faults.correctAvailable validator = true ∧
        ValidatorLocalState.installedCommitAt
            ((timed.execution.trace finish).validatorState validator)
            (prior.index + 1) = some witnessId := by
  rcases fullScan start prior afterGst activeFromStart
      priorInstalled headsEqual with successfulRun | racingInstall
  · rcases successfulRun with
      ⟨observation, output, startBeforeRun, validatorInRange,
        validatorCorrect, returned, successful, _inputPrior, nextIndex⟩
    rcases successful_local_flex_run_completes_and_persists_exact runtime
        observation returned successful validatorInRange validatorCorrect with
      ⟨completion, _exactResult, runBeforeCompletion, _completionBound,
        installed, _localSource⟩
    have startBeforeFinish : start ≤ completion + 1 :=
      Nat.le_trans startBeforeRun
        (Nat.le_trans (Nat.le_trans (Nat.le_add_right observation.time 1)
          runBeforeCompletion) (Nat.le_add_right completion 1))
    have installedNext :
        ValidatorLocalState.installedCommitAt
            ((timed.execution.trace (completion + 1)).validatorState
              observation.validator)
            (prior.index + 1) = some output.reference.digest := by
      rw [← nextIndex]
      simpa [LocalFlexCommitOutput.toCommitHead] using installed
    exact ⟨completion + 1, observation.validator, output.reference.digest,
      startBeforeFinish, validatorInRange, validatorCorrect, installedNext⟩
  · exact racingInstall

/-- Legacy composition through explicit commit-reference replay or commit
synchronization. The ordinary-DAG composition below is the liveness path. -/
theorem split_common_commit_step_with_reference_replay
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (knownReference : KnownNextReferencePropagation faults network
      timed.execution.trace)
    (fullScanOutput : FullRecoveryScanFirstFlexOutput faults network program
      timed functions context source runtime) :
    DerivedPointwiseCommonCommitStep faults network timed.execution.trace := by
  intro start prior afterGst activeFromStart priorInstalled
  by_cases nextExists : SomeCorrectAvailableInstalledAtIndex faults
      timed.execution.trace start (prior.index + 1)
  · rcases nextExists with
      ⟨witnessValidator, witnessId, witnessInRange, witnessCorrect,
        witnessInstalled⟩
    rcases knownReference start prior witnessValidator witnessId afterGst
        activeFromStart priorInstalled witnessInRange witnessCorrect
        witnessInstalled with
      ⟨next, nextIndex, _sameWitnessId, completes⟩
    exact per_validator_completions_give_pointwise_result nextIndex completes
  · have headsEqual : AllCorrectAvailableCommitHeadsEqual faults
        timed.execution.trace start prior := by
      intro validator validatorInRange validatorCorrect
      have priorAtValidator := priorInstalled validator validatorInRange
        validatorCorrect
      have headAtMostPrior :
          (timed.execution.trace start).localCommitIndex validator ≤
            prior.index := by
        by_cases headAtMostPrior :
            (timed.execution.trace start).localCommitIndex validator ≤
              prior.index
        · exact headAtMostPrior
        · have nextAtOrBelowHead : prior.index + 1 ≤
              (timed.execution.trace start).localCommitIndex validator := by
            omega
          rcases prefixMap.installedAtOrBelowHead start validator
              (prior.index + 1) validatorInRange validatorCorrect
              nextAtOrBelowHead with
            ⟨commitId, installedNext⟩
          exact False.elim (nextExists ⟨validator, commitId, validatorInRange,
            validatorCorrect, installedNext⟩)
      have sameIndex :
          (timed.execution.trace start).localCommitIndex validator =
            prior.index := by
        omega
      exact prefixMap.sameIndexInstalledHeadIsExact start validator prior
        validatorInRange validatorCorrect priorAtValidator.1 sameIndex
    rcases full_recovery_scan_or_race_gives_witness timed source runtime
        fullScanOutput afterGst activeFromStart priorInstalled headsEqual with
      ⟨createdAt, witnessValidator, witnessId, startBeforeCreated,
        witnessInRange, witnessCorrect, witnessInstalled⟩
    have priorInstalledAtCreated :
        AllCorrectAvailableInstalledExactAt faults timed.execution.trace
          createdAt prior := by
      intro validator validatorInRange validatorCorrect
      have priorAtStart := priorInstalled validator validatorInRange
        validatorCorrect
      have installedAtCreated := timed.execution.installed_commit_persists
        validatorInRange startBeforeCreated priorAtStart.1
      have withinHead :=
        (timed.execution.statesWellFormed createdAt validator validatorInRange)
          |>.installedIndexIsNotFuture prior.index prior.id installedAtCreated
      exact ⟨installedAtCreated, withinHead⟩
    have createdAfterGst : network.gst ≤ createdAt :=
      Nat.le_trans afterGst startBeforeCreated
    have activeFromCreated := active_suffix_of_later_start startBeforeCreated
      activeFromStart
    rcases knownReference createdAt prior witnessValidator witnessId
        createdAfterGst activeFromCreated priorInstalledAtCreated witnessInRange
        witnessCorrect witnessInstalled with
      ⟨next, nextIndex, _sameWitnessId, completes⟩
    have completesFromStart := per_validator_completions_rebase_start
      startBeforeCreated completes
    exact per_validator_completions_give_pointwise_result nextIndex
      completesFromStart

/-- The two semantic liveness targets give the pointwise result used by the
finite common-commit induction.

The capstone must derive both liveness targets. This theorem adds only durable
prefix lookup and exact-reference safety. It has no arbitrary-source-run
propagation premise. -/
theorem split_common_commit_step_gives_derived_pointwise_step
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (networkProgress : NetworkCommitProgressLiveness config faults network
      timed.execution.trace)
    (catchUp : PointwiseCommitCatchUpLiveness config faults network
      timed.execution.trace) :
    DerivedPointwiseCommonCommitStep faults network timed.execution.trace :=
  network_commit_progress_and_pointwise_catch_up_give_pointwise_step timed
    durable provenance networkProgress catchUp

/-- Optional common-finish corollary.

The semantic network-progress and pointwise catch-up results first give exact
per-validator installations. The finite validator aggregation and commit-index
induction then give the older common-finish commit-progress property. -/
theorem network_commit_progress_and_pointwise_catch_up_give_commit_progress
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (genesis : ValidatorCommitHead CommitId)
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    (networkProgress : NetworkCommitProgressLiveness config faults network
      timed.execution.trace)
    (catchUp : PointwiseCommitCatchUpLiveness config faults network
      timed.execution.trace)
    (genesisInstalled : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace 0).validatorState validator).installedCommitAt
        genesis.index = some genesis.id) :
    CommitProgressLiveness config faults network timed.execution.trace :=
  derived_common_commit_step_proves_commit_progress timed.execution
    (network_commit_progress_and_pointwise_catch_up_give_pointwise_step timed
      durable provenance networkProgress catchUp)
    genesis genesisInstalled

end Mysticeti
