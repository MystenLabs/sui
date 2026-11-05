/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorFixedReferenceFavorablePacing
import Mysticeti.ValidatorPointwiseCommitHeadAlignment

namespace Mysticeti

/-!
Pointwise catch-up through a receiver's own fixed-reference direct range.

The ahead host supplies only one exact installed successor. It does not supply
a certificate, replay manifest, or carrier. The selected receiver either has
already reached that successor, advances while the range is built, or uses its
own `depth + 1` direct range to make a later local Flex commit. Exact-prefix
safety identifies the ahead host's exact successor in every branch.
-/

/-- One exact next-index entry that is already installed at a correct host. -/
structure ValidatorAheadExactNext
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    {genesis : ValidatorCommitHead CommitId}
    (durable : ExactCommitDurablePrefixSourceMap faults trace genesis)
    (time : Time) (prior : ValidatorCommitHead CommitId) where
  holder : Nat
  holderInRange : holder < config.authorityCount
  holderCorrect : faults.correctAvailable holder = true
  next : ValidatorCommitHead CommitId
  nextIndex : next.index = prior.index + 1
  installed : durable.exactInstalledHead time holder next

/-- A correct host whose current index is above `prior` has one exact durable
entry at `prior.index + 1`. -/
theorem correct_ahead_index_gives_exact_next
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
    (durable : ExactCommitDurablePrefixSourceMap faults timed.execution.trace
      genesis)
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {time : Time} {prior : ValidatorCommitHead CommitId}
    (ahead : ∃ validator,
      validator < config.authorityCount ∧
        faults.correctAvailable validator = true ∧
        prior.index + 1 ≤
          (timed.execution.trace time).localCommitIndex validator) :
    Nonempty (ValidatorAheadExactNext durable time prior) := by
  rcases ahead with
    ⟨holder, holderInRange, holderCorrect, nextAtOrBelowHead⟩
  rcases durable.installedAtOrBelowHead time holder (prior.index + 1)
      holderInRange holderCorrect nextAtOrBelowHead with
    ⟨nextId, nextStored⟩
  rcases provenance.exactHeadForStoredId holderInRange holderCorrect nextStored
    with ⟨next, nextInstalled, nextIndex, _nextId⟩
  exact ⟨{
    holder
    holderInRange
    holderCorrect
    next
    nextIndex
    installed := nextInstalled }⟩

/-- One receiver-local progress result creates the first exact next-index
witness.

This theorem is used only to enter the ahead branch when no correct host was
ahead at the selected start. A receiver advance, an intervening install, or
the local Flex result from the direct range all expose the same durable index
`prior.index + 1`. -/
theorem receiver_progress_eventually_gives_exact_next
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    {start receiver : Time}
    {prior : ValidatorCommitHead CommitId}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (headAtStart :
      ((inputs.timedExecution.execution.trace start).validatorState
        receiver).commitHead = prior)
    (progress : ValidatorReceiverCommitAdvance inputs.timedExecution start
        receiver ∨
      Nonempty (ValidatorFixedReferenceFavorableDirectRange
        inputs.timedExecution start receiver
          (inputs.leaderSchedule.indirectDepth + 1) prior)) :
    ∃ later,
      start ≤ later ∧
        Nonempty (ValidatorAheadExactNext inputs.exactCommitPrefix later
          prior) := by
  have extractAt : ∀ {later}, start ≤ later →
      prior.index + 1 ≤
        (inputs.timedExecution.execution.trace later).localCommitIndex
          receiver →
      Nonempty (ValidatorAheadExactNext inputs.exactCommitPrefix later
        prior) := by
    intro later _startBeforeLater aheadAtLater
    exact correct_ahead_index_gives_exact_next inputs.exactCommitPrefix
      inputs.exactCommitInstallProvenance
        ⟨receiver, receiverInRange, receiverCorrect, aheadAtLater⟩
  rcases progress with advanced | rangeRaw
  · rcases advanced with ⟨later, startBeforeLater, indexAdvanced⟩
    have aheadAtLater : prior.index + 1 ≤
        (inputs.timedExecution.execution.trace later).localCommitIndex
          receiver := by
      change prior.index + 1 ≤
        ((inputs.timedExecution.execution.trace later).validatorState
          receiver).commitHead.index
      rw [headAtStart] at indexAdvanced
      omega
    exact ⟨later, startBeforeLater,
      extractAt startBeforeLater aheadAtLater⟩
  · let directRange := Classical.choice rangeRaw
    rcases fixed_reference_direct_range_records_local_commit_or_installed_next
        inputs directRange receiverInRange receiverCorrect with
      installedNext | laterLocalCommit
    · rcases installedNext with
        ⟨later, witnessId, startBeforeLater, installed⟩
      have aheadAtLater : prior.index + 1 ≤
          (inputs.timedExecution.execution.trace later).localCommitIndex
            receiver :=
        (inputs.timedExecution.execution.statesWellFormed later receiver
          receiverInRange).installedIndexIsNotFuture
            (prior.index + 1) witnessId installed
      exact ⟨later, startBeforeLater,
        extractAt startBeforeLater aheadAtLater⟩
    · rcases laterLocalCommit with
        ⟨run, installedAt, startBeforeRun, runReceiver, runPrior,
          runBeforeInstall, installed, _localSource⟩
      subst receiver
      have startBeforeInstall : start ≤ installedAt :=
        Nat.le_trans startBeforeRun (Nat.le_of_lt runBeforeInstall)
      have outputAtOrBelowHead : run.output.toCommitHead.index ≤
          (inputs.timedExecution.execution.trace installedAt).localCommitIndex
            run.observation.validator :=
        (inputs.timedExecution.execution.statesWellFormed installedAt
          run.observation.validator run.validatorInRange
          ).installedIndexIsNotFuture run.output.toCommitHead.index
            run.output.toCommitHead.id installed
      have aheadAtInstall : prior.index + 1 ≤
          (inputs.timedExecution.execution.trace installedAt).localCommitIndex
            run.observation.validator := by
        calc
          prior.index + 1 = run.prior.index + 1 := by rw [runPrior]
          _ = run.output.toCommitHead.index :=
            (ExactFlexSuccessor.correctRunAdvancesOneIndex run).symm
          _ ≤ _ := outputAtOrBelowHead
      exact ⟨installedAt, startBeforeInstall,
        extractAt startBeforeInstall aheadAtInstall⟩

/-- A receiver advance or one receiver-local fixed-reference range completes
the exact next entry which is already installed at another correct host.

`progress` is an internally derived result. It is not an end-to-end input
field. Other validators' installs do not occur in this result type. -/
theorem ahead_exact_next_and_receiver_progress_give_completion
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    {start receiver : Time}
    {prior : ValidatorCommitHead CommitId}
    (ahead : ValidatorAheadExactNext inputs.exactCommitPrefix start prior)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (headAtStart :
      ((inputs.timedExecution.execution.trace start).validatorState
        receiver).commitHead = prior)
    (priorInstalledAtStart :
      ((inputs.timedExecution.execution.trace start).validatorState
        receiver).installedCommitAt prior.index = some prior.id)
    (progress : ValidatorReceiverCommitAdvance inputs.timedExecution start
        receiver ∨
      Nonempty (ValidatorFixedReferenceFavorableDirectRange
        inputs.timedExecution start receiver
          (inputs.leaderSchedule.indirectDepth + 1) prior)) :
    Nonempty (ValidatorExactCommitCompletion
      inputs.timedExecution.execution.trace start receiver ahead.next) := by
  rcases progress with advanced | rangeRaw
  · rcases advanced with ⟨finish, startBeforeFinish, indexAdvanced⟩
    have nextAtOrBelowHead : ahead.next.index ≤
        (inputs.timedExecution.execution.trace finish).localCommitIndex
          receiver := by
      rw [ahead.nextIndex]
      change prior.index + 1 ≤
        ((inputs.timedExecution.execution.trace finish).validatorState
          receiver).commitHead.index
      rw [headAtStart] at indexAdvanced
      omega
    exact exact_prefix_entry_at_or_below_head_gives_completion
      inputs.exactCommitPrefix inputs.authenticatedFlexVotes
        inputs.exactCommitInstallProvenance startBeforeFinish
          ahead.holderInRange ahead.holderCorrect ahead.installed
            receiverInRange receiverCorrect (by
              rw [ahead.nextIndex]
              omega) nextAtOrBelowHead
  · let directRange := Classical.choice rangeRaw
    rcases fixed_reference_direct_range_records_local_commit_or_installed_next
        inputs directRange receiverInRange receiverCorrect with
      installedNext | laterLocalCommit
    · rcases installedNext with
        ⟨finish, witnessId, startBeforeFinish, installed⟩
      have installedExact :
          ((inputs.timedExecution.execution.trace finish).validatorState
            receiver).installedCommitAt ahead.next.index = some witnessId := by
        rw [ahead.nextIndex]
        exact installed
      have nextAtOrBelowHead : ahead.next.index ≤
          (inputs.timedExecution.execution.trace finish).localCommitIndex
            receiver :=
        (inputs.timedExecution.execution.statesWellFormed finish receiver
          receiverInRange).installedIndexIsNotFuture ahead.next.index witnessId
            installedExact
      exact exact_prefix_entry_at_or_below_head_gives_completion
        inputs.exactCommitPrefix inputs.authenticatedFlexVotes
          inputs.exactCommitInstallProvenance startBeforeFinish
            ahead.holderInRange ahead.holderCorrect ahead.installed
              receiverInRange receiverCorrect (by
                rw [ahead.nextIndex]
                omega) nextAtOrBelowHead
    · rcases laterLocalCommit with
        ⟨run, _installedAt, startBeforeRun, runReceiver, _runPrior,
          _runBeforeInstall, _installed, _localSource⟩
      subst receiver
      exact installed_next_precedes_any_later_local_commit
        inputs.flexCommitterRuntime inputs.exactCommitPrefix
          inputs.authenticatedFlexVotes inputs.exactCommitInstallProvenance
            ahead.nextIndex ahead.holderInRange ahead.holderCorrect
              ahead.installed run startBeforeRun priorInstalledAtStart

end Mysticeti
