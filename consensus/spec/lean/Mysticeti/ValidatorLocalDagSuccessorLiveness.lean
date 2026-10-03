/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommonCommitStep
import Mysticeti.ValidatorAdjacentRecoveryPropagation
import Mysticeti.ValidatorNormalBlockLiveness
import Mysticeti.ValidatorRecoveryCapsuleSyncExecution

namespace Mysticeti

/-!
Composition of one delivered post-install ordinary carrier.

This module is above local commit safety and recursive block synchronization in
the import graph. It does not add a completed synchronization, a future local
run, or a future commit as an input. The caller must derive the concrete
post-install carrier, its pinned source capsule, and its addressed delivery
from the normal proposal and broadcast rules.
-/

/-- A later correct local commit origin cannot start from a durable prefix that
skips an earlier exact installed head.

The later origin supplies only its actual prior install. Exact installed-prefix
safety identifies the earlier index in that local durable prefix. This is a
safety edge. It does not use commit synchronization, a future proposal, or a
future local run as a progress premise. -/
theorem later_correct_exact_origin_prior_contains_earlier_head
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
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {earlierReference laterReference : ValidatorCommitHead CommitId}
    (earlier : CorrectInstalledExactOrigin runtime durable earlierReference)
    (later : CorrectInstalledExactOrigin runtime durable laterReference)
    (earlierAtOrBelowLaterPrior :
      earlierReference.index ≤ later.run.prior.index) :
    ((timed.execution.trace later.run.observation.time).validatorState
        later.run.observation.validator).installedCommitAt
      earlierReference.index = some earlierReference.id := by
  have laterPriorStored := durable.exactHeadHasStoredId
    later.run.observation.time later.run.observation.validator later.run.prior
      later.priorInstalled
  have laterPriorAtOrBelowHead : later.run.prior.index ≤
      (timed.execution.trace later.run.observation.time).localCommitIndex
        later.run.observation.validator :=
    (timed.execution.statesWellFormed later.run.observation.time
      later.run.observation.validator later.run.validatorInRange)
        |>.installedIndexIsNotFuture later.run.prior.index later.run.prior.id
          laterPriorStored
  exact provenance.exactHeadAtOrBelowLocalHeadIsStored authenticated
    earlier.run.validatorInRange earlier.run.validatorCorrect earlier.installed
      later.run.validatorInRange later.run.validatorCorrect
        (Nat.le_trans earlierAtOrBelowLaterPrior laterPriorAtOrBelowHead)

/-- A strictly later exact local commit contains the earlier exact head in the
durable prefix that its successful run reads.

Each successful full run advances exactly one index. Thus a strict reference
index order puts the earlier reference at or below the later run's prior. -/
theorem later_correct_exact_origin_contains_earlier_head
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
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {earlierReference laterReference : ValidatorCommitHead CommitId}
    (earlier : CorrectInstalledExactOrigin runtime durable earlierReference)
    (later : CorrectInstalledExactOrigin runtime durable laterReference)
    (strictIndexOrder : earlierReference.index < laterReference.index) :
    ((timed.execution.trace later.run.observation.time).validatorState
        later.run.observation.validator).installedCommitAt
      earlierReference.index = some earlierReference.id := by
  have laterIndex := ExactCommitInstallProvenance.originPriorIndex later
  apply later_correct_exact_origin_prior_contains_earlier_head durable
    authenticated provenance earlier later
  omega

/-- One exact direct-quorum range causes a later local commit, unless the
validator installs the next index before the protected committer action
finishes.

The trace theorem reconstructs the pure scan at the state immediately before
the actual `runCommitter` action. The successful branch therefore gives one
real correct run. Protected record work then installs that run's exact output
with a same-host local-execution source. No earlier source run or output is an
input to this theorem. -/
theorem trace_direct_quorum_range_records_later_local_commit_or_installed_next
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
    {start observer baseRound : Nat}
    {prior : ValidatorCommitHead CommitId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState observer).commitHead = prior)
    (baseAfterCommitHead : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (leaderAccepted : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      ((timed.execution.trace start).validatorState observer).accepted
        (leaderAt offset) = true)
    (directQuorum : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (timed.execution.trace start) observer
            (leaderAt offset))) :
    (∃ finish witnessId,
      start ≤ finish ∧
        ((timed.execution.trace finish).validatorState observer).installedCommitAt
          (prior.index + 1) = some witnessId) ∨
      ∃ (run : CorrectExactFlexRun runtime) (installedAt : Time),
        start ≤ run.observation.time ∧
          run.observation.validator = observer ∧
          run.prior = prior ∧
          run.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              observer).installedCommitAt run.output.reference.index =
            some run.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              observer).commitInstallSourceAt run.output.reference.index =
            some .localExecution := by
  rcases trace_direct_quorum_range_runs_exact_committer_or_installed_next
      source runtime prefixMap pending direct work leaderAt observerInRange
      observerCorrect headAtStart baseAfterCommitHead leaderRound firstSelected
      leaderAccepted directQuorum with
    localRun | installedNext
  · rcases localRun with
      ⟨observation, output, startBeforeRun, observationValidator, returned,
        successful, inputPrior, _outputNext⟩
    let run : CorrectExactFlexRun runtime :=
      { observation := observation
        output := output
        prior := prior
        validatorInRange := by
          simpa [observationValidator] using observerInRange
        validatorCorrect := by
          simpa [observationValidator] using observerCorrect
        returned := returned
        successful := successful
        priorAtInput := inputPrior }
    rcases successful_local_flex_run_completes_and_persists_exact runtime
        observation returned successful
        (by simpa [observationValidator] using observerInRange)
        (by simpa [observationValidator] using observerCorrect) with
      ⟨recordAt, _exactResult, runBeforeRecord, _recordBound, installed,
        localSource⟩
    refine Or.inr ⟨run, recordAt + 1, ?_, ?_, rfl, ?_, ?_, ?_⟩
    · simpa [run] using startBeforeRun
    · simpa [run] using observationValidator
    · change observation.time < recordAt + 1
      exact Nat.lt_of_lt_of_le (Nat.lt_succ_self observation.time)
        (Nat.le_trans runBeforeRecord (Nat.le_add_right recordAt 1))
    · simpa [run, observationValidator] using installed
    · simpa [run, observationValidator] using localSource
  · exact Or.inl installedNext

/-- A fresh finite family of accepted adjacent proposal edges gives one later
local Flex commit, unless the observer has already installed the next index.

The higher recovery proof must derive `acceptedVoteEvidence` from its concrete
common-layer family and the favorable first-slot window. This theorem is not an
end-to-end input boundary. For each favorable leader it keeps only one
quorum-weight set of exact accepted next-round children. It does not require a
timer origin, a receiver-owned round-plus-two carrier, or a child from every
correct validator. It aggregates the exact direct votes at one common
`finish`, then invokes protected local Flex execution. -/
theorem fresh_adjacent_direct_range_records_local_commit_or_installed_next
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
    {start finish observer baseRound : Nat}
    {prior : ValidatorCommitHead CommitId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (startBeforeFinish : start ≤ finish)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState observer).commitHead = prior)
    (baseAfterCommitHead : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context observer
        ((timed.execution.trace finish).validatorState observer)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context observer
        ((timed.execution.trace finish).validatorState observer)).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (acceptedVoteEvidence : ∀ offset,
      offset < (context observer
        ((timed.execution.trace finish).validatorState observer)).depth + 1 →
      ((timed.execution.trace finish).validatorState observer).accepted
          (leaderAt offset) = true ∧
        ∃ voters : VoterSet,
          config.thresholds.quorum ≤
              weight config.authorityCount config.stake voters ∧
            ∀ voter,
              voter < config.authorityCount →
              voters voter = true →
              ∃ voteReference voteBlock,
                ((timed.execution.trace finish).validatorState observer
                    ).acceptedRepresentative
                      ((leaderAt offset).round + 1) voter =
                    some voteReference ∧
                  ((timed.execution.trace finish).validatorState
                      observer).accepted voteReference = true ∧
                  (timed.execution.trace finish).blockCatalog
                      voteReference.id = some voteBlock ∧
                  voteBlock.reference = voteReference ∧
                  voteReference.author = voter ∧
                  voteReference.round = (leaderAt offset).round + 1 ∧
                  leaderAt offset ∈ voteBlock.parents) :
    (∃ completedAt witnessId,
      start ≤ completedAt ∧
        ((timed.execution.trace completedAt).validatorState
          observer).installedCommitAt (prior.index + 1) = some witnessId) ∨
      ∃ (run : CorrectExactFlexRun runtime) (installedAt : Time),
        start ≤ run.observation.time ∧
          run.observation.validator = observer ∧
          run.prior = prior ∧
          run.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              observer).installedCommitAt run.output.reference.index =
            some run.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              observer).commitInstallSourceAt run.output.reference.index =
            some .localExecution := by
  have durable := timed.execution.durableStateMonotone observer start finish
    observerInRange startBeforeFinish
  by_cases indexAdvanced : prior.index <
      ((timed.execution.trace finish).validatorState observer).commitHead.index
  · have nextAtOrBelowHead : prior.index + 1 ≤
        (timed.execution.trace finish).localCommitIndex observer := by
      change prior.index + 1 ≤
        ((timed.execution.trace finish).validatorState observer).commitHead.index
      omega
    rcases prefixMap.installedAtOrBelowHead finish observer (prior.index + 1)
        observerInRange observerCorrect nextAtOrBelowHead with
      ⟨witnessId, installed⟩
    exact Or.inl ⟨finish, witnessId, startBeforeFinish, installed⟩
  · have indexMonotone := durable.1
    rw [headAtStart] at indexMonotone
    have sameIndex :
        ((timed.execution.trace start).validatorState observer).commitHead.index =
          ((timed.execution.trace finish).validatorState
            observer).commitHead.index := by
      rw [headAtStart]
      omega
    have headAtFinish :
        ((timed.execution.trace finish).validatorState observer).commitHead =
          prior :=
      (durable.2.2.1 sameIndex).symm.trans headAtStart
    have leaderAccepted : ∀ offset,
        offset < (context observer
          ((timed.execution.trace finish).validatorState observer)).depth + 1 →
        ((timed.execution.trace finish).validatorState observer).accepted
          (leaderAt offset) = true := by
      intro offset offsetInRange
      exact (acceptedVoteEvidence offset offsetInRange).1
    have directQuorum : ∀ offset,
        offset < (context observer
          ((timed.execution.trace finish).validatorState observer)).depth + 1 →
        config.thresholds.quorum ≤
          weight config.authorityCount config.stake
            (traceDirectVoters (timed.execution.trace finish) observer
              (leaderAt offset)) := by
      intro offset offsetInRange
      rcases acceptedVoteEvidence offset offsetInRange with
        ⟨_leaderAccepted, voters, votersQuorum, votes⟩
      have votersIncluded : VoterSet.SubsetAt config.authorityCount voters
          (traceDirectVoters (timed.execution.trace finish) observer
            (leaderAt offset)) := by
        intro voter voterInRange voterSelected
        rcases votes voter voterInRange voterSelected with
          ⟨voteReference, voteBlock, representative, _voteAccepted,
            voteCatalog, voteMatches, voteAuthor, voteRound, leaderIsParent⟩
        exact accepted_child_with_leader_parent_is_direct_voter
          representative voteCatalog voteMatches voteAuthor voteRound
            leaderIsParent
      exact Nat.le_trans votersQuorum
        (weight_mono config.stake votersIncluded)
    rcases trace_direct_quorum_range_records_later_local_commit_or_installed_next
        source runtime prefixMap pending direct work leaderAt observerInRange
        observerCorrect headAtFinish baseAfterCommitHead leaderRound
        firstSelected leaderAccepted directQuorum with
      installedNext | localRun
    · rcases installedNext with
        ⟨completedAt, witnessId, finishBeforeComplete, installed⟩
      exact Or.inl ⟨completedAt, witnessId,
        Nat.le_trans startBeforeFinish finishBeforeComplete, installed⟩
    · rcases localRun with
        ⟨run, installedAt, finishBeforeRun, runValidator, runPrior,
          runBeforeInstall, installed, localSource⟩
      exact Or.inr ⟨run, installedAt,
        Nat.le_trans startBeforeFinish finishBeforeRun, runValidator, runPrior,
          runBeforeInstall, installed, localSource⟩

/-- Exact-prefix safety closes the private receiver-local run-or-race split. -/
private theorem exact_next_and_local_run_or_race_give_receiver_completion
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
    {start commonTime commonValidator observer : Nat}
    {prior next : ValidatorCommitHead CommitId}
    (nextIndex : next.index = prior.index + 1)
    (commonValidatorInRange : commonValidator < config.authorityCount)
    (commonValidatorCorrect : faults.correctAvailable commonValidator = true)
    (commonInstalled : durable.exactInstalledHead commonTime commonValidator next)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (priorInstalledAtStart :
      ((timed.execution.trace start).validatorState observer).installedCommitAt
        prior.index = some prior.id)
    (attempt :
      (∃ completedAt witnessId,
        start ≤ completedAt ∧
          ((timed.execution.trace completedAt).validatorState
            observer).installedCommitAt (prior.index + 1) = some witnessId) ∨
        ∃ (run : CorrectExactFlexRun runtime) (installedAt : Time),
          start ≤ run.observation.time ∧
            run.observation.validator = observer ∧
            run.prior = prior ∧
            run.observation.time < installedAt ∧
            ((timed.execution.trace installedAt).validatorState
                observer).installedCommitAt run.output.reference.index =
              some run.output.reference.digest ∧
            ((timed.execution.trace installedAt).validatorState
                observer).commitInstallSourceAt run.output.reference.index =
              some .localExecution) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace start
      observer next) := by
  rcases attempt with installedNext | laterLocalCommit
  · rcases installedNext with
      ⟨completedAt, witnessId, startBeforeComplete, installed⟩
    have installedAtNext :
        ((timed.execution.trace completedAt).validatorState
          observer).installedCommitAt next.index = some witnessId := by
      rw [nextIndex]
      exact installed
    have nextAtOrBelowHead : next.index ≤
        (timed.execution.trace completedAt).localCommitIndex observer :=
      (timed.execution.statesWellFormed completedAt observer observerInRange)
        |>.installedIndexIsNotFuture next.index witnessId installedAtNext
    exact exact_prefix_entry_at_or_below_head_gives_completion durable
      authenticated provenance startBeforeComplete commonValidatorInRange
      commonValidatorCorrect commonInstalled observerInRange observerCorrect
      (by rw [nextIndex]; omega) nextAtOrBelowHead
  · rcases laterLocalCommit with
      ⟨run, _installedAt, startBeforeRun, runValidator, _runPrior,
        _runBeforeInstall, _installed, _localSource⟩
    subst observer
    exact installed_next_precedes_any_later_local_commit runtime durable
      authenticated provenance nextIndex commonValidatorInRange
      commonValidatorCorrect commonInstalled run startBeforeRun
      priorInstalledAtStart

/-- A fresh favorable adjacent-layer family makes one lagging correct observer
install the exact next reference already installed by a correct peer.

The concrete accepted-layer evidence is an internal result of network DAG
progress and leader sampling. This theorem does not take a future local run,
completed block synchronization, synchronized-commit action, receiver-owned
carrier, or vote from every correct validator as an input. -/
theorem exact_next_and_fresh_adjacent_direct_range_give_receiver_completion
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
    {start finish commonTime commonValidator observer baseRound : Nat}
    {prior next : ValidatorCommitHead CommitId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (nextIndex : next.index = prior.index + 1)
    (commonValidatorInRange : commonValidator < config.authorityCount)
    (commonValidatorCorrect : faults.correctAvailable commonValidator = true)
    (commonInstalled : durable.exactInstalledHead commonTime commonValidator next)
    (startBeforeFinish : start ≤ finish)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState observer).commitHead = prior)
    (priorInstalledAtStart :
      ((timed.execution.trace start).validatorState observer).installedCommitAt
        prior.index = some prior.id)
    (baseAfterCommitHead : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context observer
        ((timed.execution.trace finish).validatorState observer)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context observer
        ((timed.execution.trace finish).validatorState observer)).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (acceptedVoteEvidence : ∀ offset,
      offset < (context observer
        ((timed.execution.trace finish).validatorState observer)).depth + 1 →
      ((timed.execution.trace finish).validatorState observer).accepted
          (leaderAt offset) = true ∧
        ∃ voters : VoterSet,
          config.thresholds.quorum ≤
              weight config.authorityCount config.stake voters ∧
            ∀ voter,
              voter < config.authorityCount →
              voters voter = true →
              ∃ voteReference voteBlock,
                ((timed.execution.trace finish).validatorState observer
                    ).acceptedRepresentative
                      ((leaderAt offset).round + 1) voter =
                    some voteReference ∧
                  ((timed.execution.trace finish).validatorState
                      observer).accepted voteReference = true ∧
                  (timed.execution.trace finish).blockCatalog
                      voteReference.id = some voteBlock ∧
                  voteBlock.reference = voteReference ∧
                  voteReference.author = voter ∧
                  voteReference.round = (leaderAt offset).round + 1 ∧
                  leaderAt offset ∈ voteBlock.parents) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace start
      observer next) := by
  apply exact_next_and_local_run_or_race_give_receiver_completion runtime durable
    authenticated provenance nextIndex commonValidatorInRange
      commonValidatorCorrect commonInstalled observerInRange observerCorrect
        priorInstalledAtStart
  exact fresh_adjacent_direct_range_records_local_commit_or_installed_next
    source runtime prefixMap pending direct work leaderAt
      startBeforeFinish observerInRange observerCorrect headAtStart
        baseAfterCommitHead leaderRound firstSelected acceptedVoteEvidence

/-- One internally derived direct-quorum range makes a lagging validator
install an earlier exact next reference.

If another action installs the next index first, exact durable-prefix safety
identifies that entry. Otherwise the range causes an actual later local Flex
commit. Paper-style prefix agreement then puts the earlier next reference
before that later commit. The later run can use a different candidate and
output. -/
theorem exact_next_and_later_direct_quorum_range_give_receiver_completion
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
    {start commonTime commonValidator observer baseRound : Nat}
    {prior next : ValidatorCommitHead CommitId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (nextIndex : next.index = prior.index + 1)
    (commonValidatorInRange : commonValidator < config.authorityCount)
    (commonValidatorCorrect : faults.correctAvailable commonValidator = true)
    (commonInstalled :
      durable.exactInstalledHead commonTime commonValidator next)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState observer).commitHead = prior)
    (priorInstalledAtStart :
      ((timed.execution.trace start).validatorState observer).installedCommitAt
        prior.index = some prior.id)
    (baseAfterCommitHead : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (leaderAccepted : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      ((timed.execution.trace start).validatorState observer).accepted
        (leaderAt offset) = true)
    (directQuorum : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (timed.execution.trace start) observer
            (leaderAt offset))) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace start
      observer next) := by
  rcases trace_direct_quorum_range_records_later_local_commit_or_installed_next
      source runtime prefixMap pending direct work leaderAt observerInRange
      observerCorrect headAtStart baseAfterCommitHead leaderRound firstSelected
      leaderAccepted directQuorum with
    installedNext | laterLocalCommit
  · rcases installedNext with
      ⟨finish, witnessId, startBeforeFinish, installed⟩
    have installedAtNext :
        ((timed.execution.trace finish).validatorState observer).installedCommitAt
            next.index = some witnessId := by
      rw [nextIndex]
      exact installed
    have nextAtOrBelowHead : next.index ≤
        (timed.execution.trace finish).localCommitIndex observer :=
      (timed.execution.statesWellFormed finish observer observerInRange)
        |>.installedIndexIsNotFuture next.index witnessId installedAtNext
    exact exact_prefix_entry_at_or_below_head_gives_completion durable
      authenticated provenance startBeforeFinish commonValidatorInRange
      commonValidatorCorrect commonInstalled observerInRange observerCorrect
      (by rw [nextIndex]; omega) nextAtOrBelowHead
  · rcases laterLocalCommit with
      ⟨run, _installedAt, startBeforeRun, runValidator, _runPrior,
        _runBeforeInstall, _installed, _localSource⟩
    subst observer
    exact installed_next_precedes_any_later_local_commit runtime durable
      authenticated provenance nextIndex commonValidatorInRange
      commonValidatorCorrect commonInstalled run startBeforeRun
      priorInstalledAtStart

/-! ### A later ordinary carrier gives one receiver-local Flex attempt -/

/-- One exact path from a target block to an ancestor through immediate-round
parent edges only.

This path-local relation avoids a global validity premise about unrelated
catalog entries. It contains only the concrete edges that recursive block
synchronization must transfer to one requester. -/
inductive ValidatorImmediateCausalClosure
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (target : ValidatorBlockRef BlockId) :
    ValidatorBlockRef BlockId → Prop where
  | target : ValidatorImmediateCausalClosure world target target
  | parent {child parent : ValidatorBlockRef BlockId}
      {block : ValidatorBlock BlockId} :
      ValidatorImmediateCausalClosure world target child →
      world.blockCatalog child.id = some block →
      block.reference = child →
      parent ∈ block.parents →
      parent.round + 1 = child.round →
      ValidatorImmediateCausalClosure world target parent

namespace ValidatorImmediateCausalClosure

/-- Forget immediate-round evidence and retain ordinary causal ancestry. -/
theorem to_causal_ancestor
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {target ancestor : ValidatorBlockRef BlockId}
    (path : ValidatorImmediateCausalClosure world target ancestor) :
    ValidatorCausalAncestor world ancestor target := by
  induction path with
  | target => exact .same _
  | parent childPath catalog referenceExact parentIncluded _ inductionHypothesis =>
      exact .trans
        (.parent catalog referenceExact parentIncluded)
        inductionHypothesis

/-- An immediate parent path remains valid when the block catalog gains
entries. -/
theorem of_catalog_monotone
    {BlockId CommitId PacketId : Type}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {target ancestor : ValidatorBlockRef BlockId}
    (catalogMonotone : OptionMapMonotone before.blockCatalog after.blockCatalog)
    (path : ValidatorImmediateCausalClosure before target ancestor) :
    ValidatorImmediateCausalClosure after target ancestor := by
  induction path with
  | target => exact .target
  | parent _ catalog referenceExact parentIncluded immediate
      inductionHypothesis =>
      exact .parent inductionHypothesis
        (catalogMonotone _ _ catalog) referenceExact parentIncluded immediate

/-- Re-anchor one path at a later target that already reaches the old target. -/
theorem reanchor
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {earlierTarget laterTarget ancestor : ValidatorBlockRef BlockId}
    (path : ValidatorImmediateCausalClosure world earlierTarget ancestor)
    (earlierInLater :
      ValidatorImmediateCausalClosure world laterTarget earlierTarget) :
    ValidatorImmediateCausalClosure world laterTarget ancestor := by
  induction path with
  | target => exact earlierInLater
  | parent _ catalog referenceExact parentIncluded immediate
      inductionHypothesis =>
      exact .parent inductionHypothesis catalog referenceExact parentIncluded
        immediate

/-- Lift one path through one exact immediate parent edge of a later block. -/
theorem of_later_parent
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {earlierTarget laterTarget ancestor : ValidatorBlockRef BlockId}
    {laterBlock : ValidatorBlock BlockId}
    (path : ValidatorImmediateCausalClosure world earlierTarget ancestor)
    (laterCatalog : world.blockCatalog laterTarget.id = some laterBlock)
    (laterReference : laterBlock.reference = laterTarget)
    (earlierIsParent : earlierTarget ∈ laterBlock.parents)
    (immediate : earlierTarget.round + 1 = laterTarget.round) :
    ValidatorImmediateCausalClosure world laterTarget ancestor := by
  exact path.reanchor
    (.parent (.target) laterCatalog laterReference earlierIsParent immediate)

/-- One actual adjacent timer-paced proposal edge is an exact immediate-parent
path in every later world after the next proposal was sent. -/
theorem of_adjacent_timer_paced_parent_evidence
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
    {author receiver round finish : Nat}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (evidence : ValidatorAdjacentTimerPacedParentEvidence previous next)
    (nextSentBeforeFinish : next.sentTime ≤ finish) :
    ValidatorImmediateCausalClosure (timed.execution.trace finish)
      next.snapshot.block.reference previous.snapshot.block.reference := by
  have nextStoredBeforeFinish : next.snapshot.storedAt ≤ finish :=
    Nat.le_trans next.storedBeforeSent nextSentBeforeFinish
  have nextCatalog := timed.execution.blockCatalogMonotone
    next.snapshot.storedAt finish nextStoredBeforeFinish
      next.snapshot.block.reference.id next.snapshot.block
        next.snapshot.blockInCatalog
  have immediate : previous.snapshot.block.reference.round + 1 =
      next.snapshot.block.reference.round := by
    rw [previous.blockRound, next.blockRound]
  exact .parent .target nextCatalog rfl evidence.included immediate

/-- If the final ancestor is above a cutoff, every edge needed to reach it is
also above the cutoff. -/
theorem to_above_round
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {target ancestor : ValidatorBlockRef BlockId}
    {cutoff : Nat}
    (path : ValidatorImmediateCausalClosure world target ancestor)
    (above : cutoff < ancestor.round) :
    ValidatorCausalClosureReferenceAboveRound world cutoff target ancestor := by
  induction path with
  | target => exact .anchor above
  | @parent child parent block childPath catalog referenceExact parentIncluded
      immediate inductionHypothesis =>
      have childAbove : cutoff < child.round := by omega
      exact .parent (inductionHypothesis childAbove) catalog referenceExact
        parentIncluded above

end ValidatorImmediateCausalClosure

/-- Concrete leader and direct-vote paths carried by one later ordinary block.

The structure contains no source commit, output material, future delivery, or
future requester result. A higher theorem must derive it from actual valid
proposal blocks. -/
structure ValidatorDirectRangeCarrierCoverage
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (target : ValidatorBlockRef BlockId)
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (count : Nat) : Prop where
  leaderReachesTarget : ∀ offset,
    offset < count →
      ValidatorImmediateCausalClosure world target (leaderAt offset)
  directVotesReachTarget : ∀ offset,
    offset < count →
      ∀ voter,
        voter < config.authorityCount →
        faults.correctAvailable voter = true →
        ∃ voteBlock : ValidatorBlock BlockId,
          world.blockCatalog voteBlock.reference.id = some voteBlock ∧
            voteBlock.reference.author = voter ∧
            voteBlock.reference.round = (leaderAt offset).round + 1 ∧
            leaderAt offset ∈ voteBlock.parents ∧
            ValidatorImmediateCausalClosure world target voteBlock.reference

namespace ValidatorDirectRangeCarrierCoverage

/-- Append one exact leader and its correct-available vote children to an
already carried direct range at the same target. -/
theorem snoc
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {target : ValidatorBlockRef BlockId}
    {leaderAt : Nat → ValidatorBlockRef BlockId}
    {count : Nat}
    (coverage : ValidatorDirectRangeCarrierCoverage config faults world target
      leaderAt count)
    (nextLeaderPath : ValidatorImmediateCausalClosure world target
      (leaderAt count))
    (nextVotePaths : ∀ voter,
      voter < config.authorityCount →
      faults.correctAvailable voter = true →
      ∃ voteBlock : ValidatorBlock BlockId,
        world.blockCatalog voteBlock.reference.id = some voteBlock ∧
          voteBlock.reference.author = voter ∧
          voteBlock.reference.round = (leaderAt count).round + 1 ∧
          leaderAt count ∈ voteBlock.parents ∧
          ValidatorImmediateCausalClosure world target voteBlock.reference) :
    ValidatorDirectRangeCarrierCoverage config faults world target leaderAt
      (count + 1) := by
  refine {
    leaderReachesTarget := ?_
    directVotesReachTarget := ?_ }
  · intro offset offsetInRange
    by_cases before : offset < count
    · exact coverage.leaderReachesTarget offset before
    · have same : offset = count := by omega
      simpa only [same] using nextLeaderPath
  · intro offset offsetInRange voter voterInRange voterCorrect
    by_cases before : offset < count
    · exact coverage.directVotesReachTarget offset before voter voterInRange
        voterCorrect
    · have same : offset = count := by omega
      simpa only [same] using nextVotePaths voter voterInRange voterCorrect

/-- Full direct-range carry remains true in a larger catalog and through one
later immediate target path. -/
theorem of_catalog_monotone_and_target_path
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {earlierTarget laterTarget : ValidatorBlockRef BlockId}
    {leaderAt : Nat → ValidatorBlockRef BlockId}
    {count : Nat}
    (coverage : ValidatorDirectRangeCarrierCoverage config faults before
      earlierTarget leaderAt count)
    (catalogMonotone : OptionMapMonotone before.blockCatalog after.blockCatalog)
    (earlierInLater : ValidatorImmediateCausalClosure after laterTarget
      earlierTarget) :
    ValidatorDirectRangeCarrierCoverage config faults after laterTarget
      leaderAt count := by
  refine {
    leaderReachesTarget := ?_
    directVotesReachTarget := ?_ }
  · intro offset offsetInRange
    exact (coverage.leaderReachesTarget offset offsetInRange
      |>.of_catalog_monotone catalogMonotone).reanchor earlierInLater
  · intro offset offsetInRange voter voterInRange voterCorrect
    rcases coverage.directVotesReachTarget offset offsetInRange voter
        voterInRange voterCorrect with
      ⟨voteBlock, voteCatalog, voteAuthor, voteRound, leaderParent,
        votePath⟩
    exact ⟨voteBlock, catalogMonotone _ _ voteCatalog, voteAuthor, voteRound,
      leaderParent,
      (votePath.of_catalog_monotone catalogMonotone).reanchor earlierInLater⟩

end ValidatorDirectRangeCarrierCoverage

/-- Accepted above-GC closure of one later ordinary carrier transfers its
leader and direct-vote range to one correct requester.

If a required leader is already at or below the requester's GC boundary, the
durable-prefix map instead proves that the requester has installed the next
commit index. -/
theorem accepted_above_gc_direct_range_carrier_gives_range_or_installed_next
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (representatives : ValidatorAcceptedRepresentativeRules execution)
    (prefixMap : ValidatorCommitPrefixSourceMap faults execution.trace)
    {sourceTime start finish requester count baseRound : Time}
    {prior : ValidatorCommitHead CommitId}
    {target : ValidatorBlockRef BlockId}
    {leaderAt : Nat → ValidatorBlockRef BlockId}
    (coverage : ValidatorDirectRangeCarrierCoverage config faults
      (execution.trace sourceTime) target leaderAt count)
    (sourceBeforeFinish : sourceTime ≤ finish)
    (startBeforeFinish : start ≤ finish)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (headAtStart :
      ((execution.trace start).validatorState requester).commitHead = prior)
    (baseAfterPrior : prior.round < baseRound)
    (leaderRound : ∀ offset, offset < count →
      (leaderAt offset).round = baseRound + offset)
    (acceptedClosure : ValidatorAcceptedCausalClosureAboveRound
      (execution.trace finish) requester
        ((execution.trace finish).validatorState requester).gcRound target) :
    (∃ witnessId,
      ((execution.trace finish).validatorState requester).installedCommitAt
        (prior.index + 1) = some witnessId) ∨
      (((execution.trace finish).validatorState requester).commitHead = prior ∧
        ∀ offset, offset < count →
          ((execution.trace finish).validatorState requester).accepted
                (leaderAt offset) = true ∧
            config.thresholds.quorum ≤
              weight config.authorityCount config.stake
                (traceDirectVoters (execution.trace finish) requester
                  (leaderAt offset))) := by
  have durable := execution.durableStateMonotone requester start finish
    requesterInRange startBeforeFinish
  by_cases indexAdvanced : prior.index <
      ((execution.trace finish).validatorState requester).commitHead.index
  · have nextAtOrBelowHead : prior.index + 1 ≤
        (execution.trace finish).localCommitIndex requester := by
      change prior.index + 1 ≤
        ((execution.trace finish).validatorState requester).commitHead.index
      omega
    exact Or.inl (prefixMap.installedAtOrBelowHead finish requester
      (prior.index + 1) requesterInRange requesterCorrect nextAtOrBelowHead)
  · have indexMonotone := durable.1
    rw [headAtStart] at indexMonotone
    have sameIndex :
        ((execution.trace start).validatorState requester).commitHead.index =
          ((execution.trace finish).validatorState requester).commitHead.index := by
      rw [headAtStart]
      omega
    have headAtFinish :
        ((execution.trace finish).validatorState requester).commitHead = prior :=
      (durable.2.2.1 sameIndex).symm.trans headAtStart
    by_cases leaderAtRoot : ∃ offset,
        offset < count ∧
          (leaderAt offset).round ≤
            ((execution.trace finish).validatorState requester).gcRound
    · rcases leaderAtRoot with ⟨offset, offsetInRange, atRoot⟩
      have leaderAfterPrior : prior.round < (leaderAt offset).round := by
        rw [leaderRound offset offsetInRange]
        omega
      rcases required_reference_gc_split execution prefixMap requesterInRange
          requesterCorrect startBeforeFinish headAtStart leaderAfterPrior with
        leaderAbove | installedNext
      · omega
      · exact Or.inl installedNext
    · have everyLeaderAbove : ∀ offset, offset < count →
          ((execution.trace finish).validatorState requester).gcRound <
            (leaderAt offset).round := by
        intro offset offsetInRange
        by_cases above :
            ((execution.trace finish).validatorState requester).gcRound <
              (leaderAt offset).round
        · exact above
        · exfalso
          exact leaderAtRoot ⟨offset, offsetInRange, by omega⟩
      refine Or.inr ⟨headAtFinish, ?_⟩
      intro offset offsetInRange
      have sourceLeaderPath := coverage.leaderReachesTarget offset offsetInRange
      have sourceLeaderAbove := sourceLeaderPath.to_above_round
        (everyLeaderAbove offset offsetInRange)
      have laterLeaderAbove :=
        ValidatorCausalClosureReferenceAboveRound.of_catalog_monotone
          (execution.blockCatalogMonotone sourceTime finish sourceBeforeFinish)
          sourceLeaderAbove
      have leaderAccepted := acceptedClosure (leaderAt offset) laterLeaderAbove
      refine ⟨leaderAccepted, ?_⟩
      apply all_correct_available_children_vote_gives_quorum faults
      intro voter voterInRange voterCorrect
      rcases coverage.directVotesReachTarget offset offsetInRange voter
          voterInRange voterCorrect with
        ⟨voteBlock, voteCatalog, voteAuthor, voteRound, leaderParent,
          sourceVotePath⟩
      have voteAbove :
          ((execution.trace finish).validatorState requester).gcRound <
            voteBlock.reference.round := by
        rw [voteRound]
        have := everyLeaderAbove offset offsetInRange
        omega
      have sourceVoteAbove := sourceVotePath.to_above_round voteAbove
      have laterVoteAbove :=
        ValidatorCausalClosureReferenceAboveRound.of_catalog_monotone
          (execution.blockCatalogMonotone sourceTime finish sourceBeforeFinish)
          sourceVoteAbove
      have voteAccepted := acceptedClosure voteBlock.reference laterVoteAbove
      have voteCatalogAtFinish :
          (execution.trace finish).blockCatalog voteBlock.reference.id =
            some voteBlock :=
        execution.blockCatalogMonotone sourceTime finish sourceBeforeFinish
          voteBlock.reference.id voteBlock voteCatalog
      have voterNotByzantine : faults.byzantine voter = false := by
        have notNonProgress : faults.nonProgress voter = false := by
          simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
            VoterSet.full] using voterCorrect
        have separated : faults.byzantine voter = false ∧
            faults.unavailable voter = false := by
          simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
            notNonProgress
        exact separated.1
      have peerRepresentative :=
        representatives.acceptedCorrectReferenceIsRecorded finish requester
          voteBlock.reference requesterInRange requesterCorrect
          (by simpa [voteAuthor] using voterInRange)
          (by simpa [voteAuthor] using voterNotByzantine) voteAccepted
      have representativeAtVoteRound :
          ((execution.trace finish).validatorState requester).acceptedRepresentative
              ((leaderAt offset).round + 1) voter =
            some voteBlock.reference := by
        simpa [voteAuthor, voteRound] using peerRepresentative
      exact accepted_child_with_leader_parent_is_direct_voter
        representativeAtVoteRound voteCatalogAtFinish rfl voteAuthor voteRound
        leaderParent

/-- One accepted later ordinary carrier causes one receiver-local exact Flex
attempt, unless the receiver installs the next index first.

The accepted carrier closure supplies the concrete leader and voter facts at
`finish`. The trace theorem then runs the protected committer against the
actual atomic action input. Its successful branch records and installs the
later local output. No future receiver run or install is an input. -/
theorem accepted_later_direct_range_carrier_records_local_commit_or_installed_next
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
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {sourceTime start finish requester baseRound : Time}
    {prior : ValidatorCommitHead CommitId}
    {target : ValidatorBlockRef BlockId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (coverage : ValidatorDirectRangeCarrierCoverage config faults
      (timed.execution.trace sourceTime) target leaderAt
        ((context requester
          ((timed.execution.trace finish).validatorState requester)).depth + 1))
    (sourceBeforeFinish : sourceTime ≤ finish)
    (startBeforeFinish : start ≤ finish)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState requester).commitHead = prior)
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
        ((timed.execution.trace finish).validatorState requester).gcRound target) :
    (∃ completedAt witnessId,
      start ≤ completedAt ∧
        ((timed.execution.trace completedAt).validatorState
          requester).installedCommitAt (prior.index + 1) = some witnessId) ∨
      ∃ (run : CorrectExactFlexRun runtime) (installedAt : Time),
        start ≤ run.observation.time ∧
          run.observation.validator = requester ∧
          run.prior = prior ∧
          run.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              requester).installedCommitAt run.output.reference.index =
            some run.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              requester).commitInstallSourceAt run.output.reference.index =
            some .localExecution := by
  rcases accepted_above_gc_direct_range_carrier_gives_range_or_installed_next
      timed.execution representatives prefixMap coverage sourceBeforeFinish
      startBeforeFinish requesterInRange requesterCorrect headAtStart
      baseAfterPrior leaderRound acceptedClosure with
    installedNext | ⟨headAtFinish, rangeAtFinish⟩
  · rcases installedNext with ⟨witnessId, installed⟩
    exact Or.inl ⟨finish, witnessId, startBeforeFinish, installed⟩
  · have leaderAccepted : ∀ offset,
        offset < (context requester
            ((timed.execution.trace finish).validatorState requester)).depth + 1 →
        ((timed.execution.trace finish).validatorState requester).accepted
          (leaderAt offset) = true := by
      intro offset offsetInRange
      exact (rangeAtFinish offset offsetInRange).1
    have directQuorum : ∀ offset,
        offset < (context requester
            ((timed.execution.trace finish).validatorState requester)).depth + 1 →
        config.thresholds.quorum ≤
          weight config.authorityCount config.stake
            (traceDirectVoters (timed.execution.trace finish) requester
              (leaderAt offset)) := by
      intro offset offsetInRange
      exact (rangeAtFinish offset offsetInRange).2
    rcases trace_direct_quorum_range_records_later_local_commit_or_installed_next
        source runtime prefixMap pending direct work leaderAt requesterInRange
        requesterCorrect headAtFinish baseAfterPrior leaderRound firstSelected
        leaderAccepted directQuorum with
      installedNext | localRun
    · rcases installedNext with
        ⟨completedAt, witnessId, finishBeforeComplete, installed⟩
      exact Or.inl ⟨completedAt, witnessId,
        Nat.le_trans startBeforeFinish finishBeforeComplete, installed⟩
    · rcases localRun with
        ⟨run, installedAt, finishBeforeRun, runValidator, runPrior,
          runBeforeInstall, installed, localSource⟩
      exact Or.inr ⟨run, installedAt,
        Nat.le_trans startBeforeFinish finishBeforeRun, runValidator, runPrior,
          runBeforeInstall, installed, localSource⟩

/-- An accepted later ordinary carrier makes one correct requester install an
earlier exact next reference.

The requester can win through an exact next-index race or through any later
successful local Flex commit. The second branch does not reproduce the source
run or its output; durable exact-prefix safety places `next` before that later
local commit. -/
theorem exact_next_and_accepted_later_direct_range_carrier_give_completion
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
    {sourceTime start finish commonTime commonValidator requester baseRound :
      Time}
    {prior next : ValidatorCommitHead CommitId}
    {target : ValidatorBlockRef BlockId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (coverage : ValidatorDirectRangeCarrierCoverage config faults
      (timed.execution.trace sourceTime) target leaderAt
        ((context requester
          ((timed.execution.trace finish).validatorState requester)).depth + 1))
    (nextIndex : next.index = prior.index + 1)
    (commonValidatorInRange : commonValidator < config.authorityCount)
    (commonValidatorCorrect : faults.correctAvailable commonValidator = true)
    (commonInstalled : durable.exactInstalledHead commonTime commonValidator
      next)
    (sourceBeforeFinish : sourceTime ≤ finish)
    (startBeforeFinish : start ≤ finish)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState requester).commitHead = prior)
    (priorInstalledAtStart :
      ((timed.execution.trace start).validatorState requester).installedCommitAt
        prior.index = some prior.id)
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
        ((timed.execution.trace finish).validatorState requester).gcRound target) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace start
      requester next) := by
  rcases accepted_later_direct_range_carrier_records_local_commit_or_installed_next
      source runtime prefixMap pending direct work representatives leaderAt
      coverage sourceBeforeFinish startBeforeFinish requesterInRange
      requesterCorrect headAtStart baseAfterPrior leaderRound firstSelected
      acceptedClosure with
    installedNext | laterLocalCommit
  · rcases installedNext with
      ⟨completedAt, witnessId, startBeforeComplete, installed⟩
    have installedAtNext :
        ((timed.execution.trace completedAt).validatorState
          requester).installedCommitAt next.index = some witnessId := by
      rw [nextIndex]
      exact installed
    have nextAtOrBelowHead : next.index ≤
        (timed.execution.trace completedAt).localCommitIndex requester :=
      (timed.execution.statesWellFormed completedAt requester requesterInRange)
        |>.installedIndexIsNotFuture next.index witnessId installedAtNext
    exact exact_prefix_entry_at_or_below_head_gives_completion durable
      authenticated provenance startBeforeComplete commonValidatorInRange
      commonValidatorCorrect commonInstalled requesterInRange requesterCorrect
      (by rw [nextIndex]; omega) nextAtOrBelowHead
  · rcases laterLocalCommit with
      ⟨run, _installedAt, startBeforeRun, runValidator, _runPrior,
        _runBeforeInstall, _installed, _localSource⟩
    subst requester
    exact installed_next_precedes_any_later_local_commit runtime durable
      authenticated provenance nextIndex commonValidatorInRange
      commonValidatorCorrect commonInstalled run startBeforeRun
      priorInstalledAtStart

/-- An actual normal broadcast built from one ready parent list is one exact
quorum-parent carrier at its finish state.

The proposal action preserves the exact parent list. The persistence action
puts the exact block body in the durable catalog, and local acceptance of every
parent persists to the production finish. -/
theorem normal_broadcast_with_ready_parents_gives_quorum_carrier
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
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator targetRound : Nat}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace start).validatorState validator) targetRound
        production.parents) :
    (timed.execution.trace production.finish).blockCatalog
          production.proposal.block.reference.id =
        some production.proposal.block ∧
      production.proposal.block.HasQuorumImmediateParents config ∧
      ∀ parent, parent ∈ production.proposal.block.parents →
        ((timed.execution.trace production.finish).validatorState
          validator).accepted parent = true := by
  have validatorFacts := validator_local_action_occurrence_is_correct_available
    (timed.execution.stepsFollowRules production.persistedAt)
      production.persistenceOccurs
  have startBeforeFinish : start ≤ production.finish :=
    Nat.le_trans production.startBeforeProposalAction
      (Nat.le_trans (Nat.le_succ production.proposalActionAt)
        (Nat.le_trans production.proposalBeforePersistence
          (Nat.le_trans (Nat.le_succ production.persistedAt)
            production.persistenceBeforeFinish)))
  have catalogAfterPersistence := effects.persistedProposalStoresBlock
    production.persistedAt validator production.proposal.block
      production.persistenceOccurs
  have catalogAtFinish := timed.execution.blockCatalogMonotone
    (production.persistedAt + 1) production.finish
      production.persistenceBeforeFinish _ _ catalogAfterPersistence
  refine ⟨catalogAtFinish, ?_, ?_⟩
  · refine ⟨?_, ?_, ?_⟩
    · change (production.proposal.block.parents.map
          ValidatorBlockRef.author).Nodup
      rw [production.proposalParents]
      exact parentsReady.1.1
    · intro parent parentMember
      have parentMemberExact : parent ∈ production.parents := by
        simpa only [production.proposalParents] using parentMember
      calc
        parent.round + 1 = targetRound :=
          (parentsReady.1.2.1 parent parentMemberExact).1
        _ = production.proposal.block.reference.round :=
          production.proposalRound.symm
    · change config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (validatorParentAuthors production.proposal.block.parents)
      rw [production.proposalParents]
      exact parentsReady.1.2.2
  · intro parent parentMember
    have parentMemberExact : parent ∈ production.parents := by
      simpa only [production.proposalParents] using parentMember
    exact timed.execution.accepted_block_persists validatorFacts.1
      startBeforeFinish (parentsReady.1.2.1 parent parentMemberExact).2

/-- One exact ready parent edge lifts full direct-range coverage at the first
state after proposal persistence.

This is the state in which source-pin execution creates the causal capsule.
Therefore, the coverage cannot depend on catalog entries that appear only
later during broadcast processing. -/
theorem normal_broadcast_ready_parent_lifts_direct_range_at_persistence
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
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {sourceTime start validator targetRound : Nat}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace start).validatorState validator) targetRound
        production.parents)
    {earlierTarget : ValidatorBlockRef BlockId}
    {leaderAt : Nat → ValidatorBlockRef BlockId}
    {count : Nat}
    (coverage : ValidatorDirectRangeCarrierCoverage config faults
      (timed.execution.trace sourceTime) earlierTarget leaderAt count)
    (sourceBeforePersistence : sourceTime ≤ production.persistedAt + 1)
    (earlierTargetIsParent : earlierTarget ∈ production.parents) :
    ValidatorDirectRangeCarrierCoverage config faults
      (timed.execution.trace (production.persistedAt + 1))
        production.proposal.block.reference leaderAt count := by
  have targetCatalog := effects.persistedProposalStoresBlock
    production.persistedAt validator production.proposal.block
      production.persistenceOccurs
  have targetImmediate : earlierTarget.round + 1 =
      production.proposal.block.reference.round := by
    calc
      earlierTarget.round + 1 = targetRound :=
        (parentsReady.1.2.1 earlierTarget earlierTargetIsParent).1
      _ = production.proposal.block.reference.round :=
        production.proposalRound.symm
  have earlierInLater : ValidatorImmediateCausalClosure
      (timed.execution.trace (production.persistedAt + 1))
        production.proposal.block.reference earlierTarget := by
    exact .parent .target targetCatalog rfl
      (by simpa only [production.proposalParents] using earlierTargetIsParent)
      targetImmediate
  exact coverage.of_catalog_monotone_and_target_path
    (timed.execution.blockCatalogMonotone sourceTime
      (production.persistedAt + 1) sourceBeforePersistence) earlierInLater

/-- One exact ready parent edge of a completed normal broadcast lifts a full
direct-range carrier to the newly persisted proposal block.

The ready-parent predicate supplies the immediate-round equation required by
recursive block synchronization. No global validity condition for unrelated
catalog blocks is needed. -/
theorem normal_broadcast_ready_parent_lifts_direct_range_carrier_coverage
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
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {sourceTime start validator targetRound : Nat}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    (parentsReady : ValidatorProposalParentListReady .normal config
      ((timed.execution.trace start).validatorState validator) targetRound
        production.parents)
    {earlierTarget : ValidatorBlockRef BlockId}
    {leaderAt : Nat → ValidatorBlockRef BlockId}
    {count : Nat}
    (coverage : ValidatorDirectRangeCarrierCoverage config faults
      (timed.execution.trace sourceTime) earlierTarget leaderAt count)
    (sourceBeforeFinish : sourceTime ≤ production.finish)
    (earlierTargetIsParent : earlierTarget ∈ production.parents) :
    ValidatorDirectRangeCarrierCoverage config faults
      (timed.execution.trace production.finish)
        production.proposal.block.reference leaderAt count := by
  have carrierFacts := normal_broadcast_with_ready_parents_gives_quorum_carrier
    effects production parentsReady
  have targetImmediate : earlierTarget.round + 1 =
      production.proposal.block.reference.round := by
    calc
      earlierTarget.round + 1 = targetRound :=
        (parentsReady.1.2.1 earlierTarget earlierTargetIsParent).1
      _ = production.proposal.block.reference.round :=
        production.proposalRound.symm
  have earlierInLater : ValidatorImmediateCausalClosure
      (timed.execution.trace production.finish)
        production.proposal.block.reference earlierTarget := by
    exact .parent .target carrierFacts.1 rfl
      (by simpa only [production.proposalParents] using earlierTargetIsParent)
      targetImmediate
  exact coverage.of_catalog_monotone_and_target_path
    (timed.execution.blockCatalogMonotone sourceTime production.finish
      sourceBeforeFinish) earlierInLater

/-- Each exact parent selected by a completed normal broadcast is a direct
causal ancestor of the persisted proposal block at the production finish.

The exact execution-effect map binds the persistence action to the proposal
body in the durable catalog. This theorem does not infer the parent list from a
round or a quorum; the normal producer supplies that exact list. -/
theorem normal_broadcast_parent_is_causal_ancestor
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
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator targetRound : Nat}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    {parent : ValidatorBlockRef BlockId}
    (parentIncluded : parent ∈ production.parents) :
    ValidatorCausalAncestor (timed.execution.trace production.finish) parent
      production.proposal.block.reference := by
  have catalogAfterPersistence := effects.persistedProposalStoresBlock
    production.persistedAt validator production.proposal.block
      production.persistenceOccurs
  have catalogAtFinish := timed.execution.blockCatalogMonotone
    (production.persistedAt + 1) production.finish
      production.persistenceBeforeFinish _ _ catalogAfterPersistence
  apply ValidatorCausalAncestor.of_parent catalogAtFinish
  simpa only [production.proposalParents] using parentIncluded

/-- A later actual normal proposal preserves a complete exact commit-output
carry through one selected parent edge.

The earlier carrier can be the quorum block produced by the local scan carry
theorem. Catalog monotonicity moves its leader and material paths to the normal
proposal finish. The exact normal parent list then extends each path to the
post-install ordinary carrier. -/
theorem normal_broadcast_parent_lifts_exact_commit_output_carry
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
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {sourceTime start validator targetRound : Nat}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    {output : LocalFlexCommitOutput BlockId CommitId}
    {earlierCarrier : ValidatorBlockRef BlockId}
    (sourceBeforeFinish : sourceTime ≤ production.finish)
    (earlierCarrierIsParent : earlierCarrier ∈ production.parents)
    (candidateCarried : ∀ leader,
      leader ∈ output.candidate.orderedCommittedLeaders →
        ValidatorCausalAncestor (timed.execution.trace sourceTime)
          (referenceLeaderBlockToValidatorBlockRef leader) earlierCarrier)
    (materialCarried : ∀ block,
      block ∈ output.builderInput.sortedCommittedBlocks →
        ValidatorCausalAncestor (timed.execution.trace sourceTime)
          (referenceLeaderBlockToValidatorBlockRef block) earlierCarrier)
    (namedLeaderCarried : ValidatorCausalAncestor
      (timed.execution.trace sourceTime)
      (referenceLeaderBlockToValidatorBlockRef output.builderInput.namedLeader)
      earlierCarrier) :
    (∀ leader, leader ∈ output.candidate.orderedCommittedLeaders →
      ValidatorCausalAncestor (timed.execution.trace production.finish)
        (referenceLeaderBlockToValidatorBlockRef leader)
        production.proposal.block.reference) ∧
    (∀ block, block ∈ output.builderInput.sortedCommittedBlocks →
      ValidatorCausalAncestor (timed.execution.trace production.finish)
        (referenceLeaderBlockToValidatorBlockRef block)
        production.proposal.block.reference) ∧
    ValidatorCausalAncestor (timed.execution.trace production.finish)
      (referenceLeaderBlockToValidatorBlockRef output.builderInput.namedLeader)
      production.proposal.block.reference := by
  have catalogMonotone := timed.execution.blockCatalogMonotone sourceTime
    production.finish sourceBeforeFinish
  have earlierReachesOrdinary := normal_broadcast_parent_is_causal_ancestor
    effects production earlierCarrierIsParent
  refine ⟨?_, ?_, ?_⟩
  · intro leader leaderMember
    exact .trans
      (ValidatorCausalAncestor.of_catalog_monotone catalogMonotone
        (candidateCarried leader leaderMember))
      earlierReachesOrdinary
  · intro block blockMember
    exact .trans
      (ValidatorCausalAncestor.of_catalog_monotone catalogMonotone
        (materialCarried block blockMember))
      earlierReachesOrdinary
  · exact .trans
      (ValidatorCausalAncestor.of_catalog_monotone catalogMonotone
        namedLeaderCarried)
      earlierReachesOrdinary

/-- One exact local Flex install followed by one concrete proposal persistence
gives an install-before-carrier witness at the first state after persistence.

This helper uses the exact persistence selected by the caller. It therefore
also applies when a broadcast result contains a different persistence witness
for each addressed peer. -/
def exact_source_record_then_persisted_carrier_gives_ordered_carrier
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
    {recordAt carrierTime : Time} {carrier : ValidatorBlock BlockId}
    (runBeforeRecord : sourceRun.observation.time + 1 ≤ recordAt)
    (installed : ValidatorLocalState.installedCommitAt
      ((timed.execution.trace (recordAt + 1)).validatorState
        sourceRun.observation.validator) sourceRun.output.reference.index =
        some sourceRun.output.reference.digest)
    (localSource : ValidatorLocalState.commitInstallSourceAt
      ((timed.execution.trace (recordAt + 1)).validatorState
        sourceRun.observation.validator) sourceRun.output.reference.index =
        some .localExecution)
    (installBeforeCarrier : recordAt + 1 ≤ carrierTime)
    (carrierAuthor : carrier.reference.author =
      sourceRun.observation.validator)
    (carrierPersists : ValidatorLocalActionOccurs
      (timed.execution.events carrierTime) sourceRun.observation.validator
        (.persistProposal carrier)) :
    LocalSuccessorBeforeOrdinaryCarrier sourceRun (carrierTime + 1)
      carrier.reference := by
  refine {
    installTime := recordAt + 1
    carrierTime := carrierTime
    carrier := carrier
    carrierReference := rfl
    runBeforeInstall := ?_
    installed := installed
    localSource := localSource
    installBeforeCarrier := installBeforeCarrier
    carrierPersists := by simpa only [carrierAuthor] using carrierPersists
    carrierVisibleBySource := Nat.le_refl _ }
  exact Nat.lt_of_lt_of_le
    (Nat.lt_succ_self sourceRun.observation.time)
    (Nat.le_trans runBeforeRecord (Nat.le_succ recordAt))

/-- A concrete normal broadcast that starts after one exact local Flex record
gives the one-host install-before-carrier witness used by the DAG propagation
theorem.

The installed reference and its local source are the direct post-state facts
from the protected `recordCommit` action. The normal production keeps the
actual later persistence action and its durable visibility time. No delivery,
causal coverage, or future commit is an input. -/
def exact_source_record_then_normal_broadcast_gives_ordered_carrier
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
    {obligations : ValidatorProposalObligationExecution timed}
    (sourceRun : CorrectExactFlexRun runtime)
    {recordAt targetRound : Time}
    (runBeforeRecord : sourceRun.observation.time + 1 ≤ recordAt)
    (installed : ValidatorLocalState.installedCommitAt
      ((timed.execution.trace (recordAt + 1)).validatorState
        sourceRun.observation.validator) sourceRun.output.reference.index =
        some sourceRun.output.reference.digest)
    (localSource : ValidatorLocalState.commitInstallSourceAt
      ((timed.execution.trace (recordAt + 1)).validatorState
        sourceRun.observation.validator) sourceRun.output.reference.index =
        some .localExecution)
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      (recordAt + 1) sourceRun.observation.validator targetRound) :
    LocalSuccessorBeforeOrdinaryCarrier sourceRun production.finish
      production.proposal.block.reference := by
  let ordered := exact_source_record_then_persisted_carrier_gives_ordered_carrier
    sourceRun runBeforeRecord installed localSource
      (Nat.le_trans production.startBeforeProposalAction
        (Nat.le_trans (Nat.le_succ production.proposalActionAt)
          production.proposalBeforePersistence))
      production.proposalAuthor
      (by
        simpa only [production.proposalAuthor] using
          production.persistenceOccurs)
  exact {
    installTime := ordered.installTime
    carrierTime := ordered.carrierTime
    carrier := ordered.carrier
    carrierReference := ordered.carrierReference
    runBeforeInstall := ordered.runBeforeInstall
    installed := ordered.installed
    localSource := ordered.localSource
    installBeforeCarrier := ordered.installBeforeCarrier
    carrierPersists := ordered.carrierPersists
    carrierVisibleBySource := Nat.le_trans ordered.carrierVisibleBySource
      production.persistenceBeforeFinish }

/-- One completed normal broadcast makes its exact proposal body available at
each correct, available requester after the proposal persistence batch.

At the proposal author, the durable own-block and catalog records give the
body directly. At every other requester, the theorem follows the actual
addressed packet through partial synchrony and the main delivery event. This
is a delivery theorem, not a completed recursive synchronization premise. -/
theorem normal_broadcast_makes_exact_body_available_after_persistence
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
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator targetRound : Time}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    {requester : Nat}
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start) :
    ∃ persistedAt availableAt,
      start ≤ persistedAt ∧
        ValidatorLocalActionOccurs (timed.execution.events persistedAt)
          validator (.persistProposal production.proposal.block) ∧
      persistedAt + 1 ≤ availableAt ∧
        ValidatorLocalBlockBodyAt timed availableAt requester
          production.proposal.block := by
  by_cases requesterIsAuthor : requester = validator
  · subst requester
    have acceptedAtFinish :
        ((timed.execution.trace production.finish).validatorState
          validator).accepted production.proposal.block.reference = true :=
      ((timed.execution.statesWellFormed production.finish validator
        requesterInRange).ownBlockIsSound
          production.proposal.block.reference.round
          production.proposal.block.reference
          production.ownBlockStoredAtFinish).2.2.1
    have catalogAfterPersistence := effects.persistedProposalStoresBlock
      production.persistedAt validator production.proposal.block
        production.persistenceOccurs
    have catalogAtFinish := timed.execution.blockCatalogMonotone
      (production.persistedAt + 1) production.finish
        production.persistenceBeforeFinish _ _ catalogAfterPersistence
    refine ⟨production.persistedAt, production.finish, ?_,
      production.persistenceOccurs, production.persistenceBeforeFinish,
      .acceptedCatalogued acceptedAtFinish
      catalogAtFinish⟩
    exact Nat.le_trans production.startBeforeProposalAction
      (Nat.le_trans (Nat.le_succ production.proposalActionAt)
        production.proposalBeforePersistence)
  · let broadcast := Classical.choice
        (production.broadcasts requester requesterInRange requesterIsAuthor)
    have authorFacts := validator_local_action_occurrence_is_correct_available
      (timed.execution.stepsFollowRules production.persistedAt)
        production.persistenceOccurs
    have packetPresentAtSend :
        (timed.execution.trace broadcast.packet.sentAt).packets
            broadcast.packetId = some broadcast.packet := by
      rw [broadcast.packetSentAt]
      exact broadcast.packetInTrace
    have senderInRange :
        broadcast.packet.sender < config.authorityCount := by
      simpa only [broadcast.packetSender] using authorFacts.1
    have senderCorrect :
        faults.correctAvailable broadcast.packet.sender = true := by
      simpa only [broadcast.packetSender] using authorFacts.2
    have receiverInRange :
        broadcast.packet.receiver < config.authorityCount := by
      simpa only [broadcast.packetReceiver] using requesterInRange
    have receiverCorrect :
        faults.correctAvailable broadcast.packet.receiver = true := by
      simpa only [broadcast.packetReceiver] using requesterCorrect
    have startBeforeSend : start ≤ broadcast.sendActionAt + 1 := by
      exact Nat.le_trans production.startBeforeProposalAction
        (Nat.le_trans (Nat.le_succ production.proposalActionAt)
          (Nat.le_trans broadcast.readyBeforePersistence
            (Nat.le_trans (Nat.le_succ broadcast.persistedAt)
              (Nat.le_trans broadcast.persistenceBeforeSend
                (Nat.le_succ broadcast.sendActionAt)))))
    have packetAfterGst : network.gst ≤ broadcast.packet.sentAt := by
      rw [broadcast.packetSentAt]
      exact Nat.le_trans afterGst startBeforeSend
    have delivered := validator_protocol_packet_is_delivered timed.execution
      packetPresentAtSend broadcast.packetIsProtocol senderInRange
        receiverInRange senderCorrect receiverCorrect packetAfterGst
    have timing := network.postGstDelivery broadcast.packet
      broadcast.packetIsProtocol senderInRange receiverInRange senderCorrect
        receiverCorrect packetAfterGst
    have packetPresentAtDelivery := timed.execution.packetHistoryMonotone
      broadcast.packet.sentAt broadcast.packet.deliveredAt timing.1
        broadcast.packetId broadcast.packet packetPresentAtSend
    refine ⟨broadcast.persistedAt, broadcast.packet.deliveredAt, ?_,
      broadcast.persistenceOccurs, ?_, ?_⟩
    · exact Nat.le_trans production.startBeforeProposalAction
        (Nat.le_trans (Nat.le_succ production.proposalActionAt)
          broadcast.readyBeforePersistence)
    · have sendBatchBeforeDelivery : broadcast.sendActionAt + 1 ≤
          broadcast.packet.deliveredAt := by
        simpa [broadcast.packetSentAt] using timing.1
      exact Nat.le_trans broadcast.persistenceBeforeSend
        (Nat.le_trans (Nat.le_add_right broadcast.sendActionAt 1)
          sendBatchBeforeDelivery)
    · exact .delivered broadcast.packetId broadcast.packet
        packetPresentAtDelivery broadcast.packetIsProtocol
        broadcast.packetReceiver broadcast.packetPayload delivered

/-- Forget the stronger persistence-to-delivery order and retain the original
start-to-delivery order. -/
theorem normal_broadcast_makes_exact_body_available
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
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator targetRound : Time}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    {requester : Nat}
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start) :
    ∃ availableAt,
      start ≤ availableAt ∧
        ValidatorLocalBlockBodyAt timed availableAt requester
          production.proposal.block := by
  rcases normal_broadcast_makes_exact_body_available_after_persistence effects
      production requesterInRange requesterCorrect afterGst with
    ⟨persistedAt, availableAt, startBeforePersisted, _persisted,
      persistedBeforeAvailable, body⟩
  refine ⟨availableAt, ?_, body⟩
  exact Nat.le_trans startBeforePersisted
    (Nat.le_trans (Nat.le_succ persistedAt) persistedBeforeAvailable)

/-- One actual normal broadcast creates both sides of the recursive carrier
transfer: a source-local pinned capsule and the exact target body at one
correct requester.

The source capsule is created by the proposal persistence batch. The requester
body is obtained only from the durable own block or from the actual addressed
post-GST packet. No completed synchronization or future commit is an input. -/
theorem normal_broadcast_gives_pinned_source_and_requester_body
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
    {obligations : ValidatorProposalObligationExecution timed}
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator targetRound : Time}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    {requester : Nat}
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ persistedAt capsuleId entry availableAt,
      start ≤ persistedAt ∧
        entry.capsule.targetBlock = production.proposal.block ∧
        (pins.trace (persistedAt + 1) validator).capsuleAt
            capsuleId = some entry ∧
        (pins.trace (persistedAt + 1) validator).pinned capsuleId =
          true ∧
        CausalRecoveryCapsuleExecutionSource syncRules entry.capsule validator
          (persistedAt + 1) ∧
        persistedAt + 1 ≤ availableAt ∧
        ValidatorLocalBlockBodyAt timed availableAt requester
          production.proposal.block := by
  rcases normal_broadcast_makes_exact_body_available_after_persistence effects
      production requesterInRange requesterCorrect afterGst with
    ⟨persistedAt, availableAt, startBeforePersisted, persisted,
      persistedBeforeAvailable, body⟩
  have validatorFacts := validator_local_action_occurrence_is_correct_available
    (timed.execution.stepsFollowRules persistedAt) persisted
  have activeAfterPersistence :
      (timed.execution.trace (persistedAt + 1)).epochActive = true :=
    active (persistedAt + 1)
      (Nat.le_trans startBeforePersisted (Nat.le_succ persistedAt))
  rcases pins.persisted_proposal_has_pinned_capsule_source validatorFacts.1
      validatorFacts.2 activeAfterPersistence persisted with
    ⟨capsuleId, entry, target, stored, pinned, source⟩
  exact ⟨persistedAt, capsuleId, entry, availableAt, startBeforePersisted,
    target, stored, pinned, source, persistedBeforeAvailable, body⟩

/-- One actual post-GST normal broadcast installs its complete above-GC causal
closure at a correct requester, unless that requester already installed the
next commit index.

The proof creates the source pin and the exact target delivery from the
broadcast itself. It then applies recursive immediate-parent synchronization.
The only earlier peer fact is its exact durable commit head. -/
theorem normal_broadcast_installs_above_gc_closure_or_next_commit
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
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
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    {obligations : ValidatorProposalObligationExecution timed}
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    {start validator targetRound peerStart requester : Time}
    {prior : ValidatorCommitHead CommitId}
    (production : ValidatorNormalProposalBroadcastProduction timed obligations
      start validator targetRound)
    (peerStartBeforeProduction : peerStart ≤ start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (headAtPeerStart :
      ((timed.execution.trace peerStart).validatorState requester).commitHead =
        prior)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ finish,
      peerStart ≤ finish ∧
        ((∃ commitId,
            ((timed.execution.trace finish).validatorState
              requester).installedCommitAt (prior.index + 1) = some commitId) ∨
          ValidatorAcceptedCausalClosureAboveRound
            (timed.execution.trace finish) requester
            ((timed.execution.trace finish).validatorState requester).gcRound
            production.proposal.block.reference) := by
  rcases normal_broadcast_gives_pinned_source_and_requester_body pins effects
      production requesterInRange requesterCorrect afterGst active with
    ⟨persistedAt, capsuleId, entry, availableAt, startBeforePersisted,
      targetExact, stored, pinned, capsuleSource, sourceBeforeAvailable,
      targetBody⟩
  let sourceAt := persistedAt + 1
  have startBeforeSource : start ≤ sourceAt := by
    dsimp [sourceAt]
    exact Nat.le_trans startBeforePersisted (Nat.le_succ persistedAt)
  have peerStartBeforeSource : peerStart ≤ sourceAt :=
    Nat.le_trans peerStartBeforeProduction startBeforeSource
  have durable := timed.execution.durableStateMonotone requester peerStart
    sourceAt requesterInRange peerStartBeforeSource
  by_cases advanced : prior.index <
      ((timed.execution.trace sourceAt).validatorState
        requester).commitHead.index
  · have installedNext := prefixMap.installedAtOrBelowHead sourceAt requester
        (prior.index + 1) requesterInRange requesterCorrect (by
          change prior.index + 1 ≤
            ((timed.execution.trace sourceAt).validatorState
              requester).commitHead.index
          omega)
    exact ⟨sourceAt, peerStartBeforeSource, Or.inl installedNext⟩
  · have indexMonotone := durable.1
    rw [headAtPeerStart] at indexMonotone
    have sameIndex :
        ((timed.execution.trace peerStart).validatorState
            requester).commitHead.index =
          ((timed.execution.trace sourceAt).validatorState
            requester).commitHead.index := by
      rw [headAtPeerStart]
      omega
    have headAtSource :
        ((timed.execution.trace sourceAt).validatorState requester).commitHead =
          prior :=
      (durable.2.2.1 sameIndex).symm.trans headAtPeerStart
    have sourceAfterGst : network.gst ≤ sourceAt :=
      Nat.le_trans afterGst startBeforeSource
    have activeFromSource : ∀ time, sourceAt ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time sourceBeforeTime
      exact active time (Nat.le_trans startBeforeSource sourceBeforeTime)
    have entryBody : ValidatorLocalBlockBodyAt timed availableAt requester
        entry.capsule.targetBlock := by
      rw [targetExact]
      exact targetBody
    rcases sync.delivered_target_installs_above_gc_closure_or_next_commit pins
        acceptance prefixMap capsuleSource.holderInRange
        capsuleSource.holderCorrectAvailable requesterInRange requesterCorrect
        sourceAfterGst activeFromSource stored pinned headAtSource
        sourceBeforeAvailable entryBody with
      ⟨finish, sourceBeforeFinish, installedNext | acceptedClosure⟩
    · exact ⟨finish, Nat.le_trans peerStartBeforeSource sourceBeforeFinish,
        Or.inl installedNext⟩
    · refine ⟨finish, Nat.le_trans peerStartBeforeSource sourceBeforeFinish,
        Or.inr ?_⟩
      simpa only [targetExact] using acceptedClosure

/-- One delivered post-install ordinary carrier completes the exact source
successor at one correct requester.

Recursive block synchronization first gives one common time at which the
requester either installed the next index or accepted the complete carrier
closure above its final GC round. In the closure branch, the carried exact
leader and direct-vote range runs the requester's actual local committer. Exact
prefix safety resolves either race to the source output. -/
theorem delivered_post_install_carrier_completes_exact_successor
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (sourceRun : CorrectExactFlexRun runtime)
    {peerStart sourceTime targetTime holder requester baseRound : Time}
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    {prior : ValidatorCommitHead CommitId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (coverage : ValidatorExactFlexCarrierCoverage config faults
      (timed.execution.trace sourceTime) entry.capsule.targetBlock.reference
        leaderAt
        ((context sourceRun.observation.validator
          sourceRun.observation.input).depth + 1)
        sourceRun.output)
    (sourcePrior : sourceRun.prior = prior)
    (sourceBeforeCarrier : LocalSuccessorBeforeOrdinaryCarrier sourceRun
      sourceTime entry.capsule.targetBlock.reference)
    (peerStartBeforeSource : peerStart ≤ sourceTime)
    (holderInRange : holder < config.authorityCount)
    (holderCorrect : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ sourceTime)
    (activeEpoch : ∀ time, sourceTime ≤ time →
      (timed.execution.trace time).epochActive = true)
    (stored : (pins.trace sourceTime holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace sourceTime holder).pinned capsuleKey = true)
    (headAtSource :
      ((timed.execution.trace sourceTime).validatorState requester).commitHead =
        prior)
    (headAtPeerStart :
      ((timed.execution.trace peerStart).validatorState requester).commitHead =
        prior)
    (baseAfterPrior : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context sourceRun.observation.validator
        sourceRun.observation.input).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context sourceRun.observation.validator
        sourceRun.observation.input).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (sourceBeforeTarget : sourceTime ≤ targetTime)
    (targetBody : ValidatorLocalBlockBodyAt timed targetTime requester
      entry.capsule.targetBlock) :
    Nonempty (ValidatorExactCommitCompletion timed.execution.trace peerStart
      requester sourceRun.output.toCommitHead) := by
  rcases sync.delivered_target_installs_above_gc_closure_or_next_commit pins
      acceptance prefixMap holderInRange holderCorrect requesterInRange
      requesterCorrect afterGst activeEpoch stored pinned headAtSource
      sourceBeforeTarget targetBody with
    ⟨finish, sourceBeforeFinish, installedNext | acceptedClosure⟩
  · rcases installedNext with ⟨witnessId, installed⟩
    apply local_dag_run_or_installed_next_gives_exact_completion runtime
      durable authenticated provenance sourceRun sourcePrior requesterInRange
        requesterCorrect
    exact Or.inl ⟨finish, witnessId,
      Nat.le_trans peerStartBeforeSource sourceBeforeFinish,
      installed⟩
  · have sameDepth :
        (context sourceRun.observation.validator
            sourceRun.observation.input).depth =
          (context requester
            ((timed.execution.trace finish).validatorState requester)).depth :=
      source.contextDepthStable sourceRun.observation.validator
        sourceRun.observation.input requester
        ((timed.execution.trace finish).validatorState requester)
    have coverageAtFinish : ValidatorExactFlexCarrierCoverage config faults
        (timed.execution.trace sourceTime)
        entry.capsule.targetBlock.reference leaderAt
          ((context requester
            ((timed.execution.trace finish).validatorState requester)).depth + 1)
          sourceRun.output := by
      simpa only [sameDepth] using coverage
    apply accepted_ordinary_dag_carrier_gives_exact_successor_completion source
      runtime prefixMap pending direct work durable authenticated provenance
        representatives sourceRun leaderAt coverageAtFinish sourcePrior
        sourceBeforeCarrier sourceBeforeFinish
        (Nat.le_trans peerStartBeforeSource sourceBeforeFinish) requesterInRange
        requesterCorrect headAtPeerStart
        baseAfterPrior
    · intro offset offsetInRange
      apply leaderRound offset
      simpa only [sameDepth] using offsetInRange
    · intro offset offsetInRange
      apply firstSelected offset
      simpa only [sameDepth] using offsetInRange
    · exact acceptedClosure

end Mysticeti
