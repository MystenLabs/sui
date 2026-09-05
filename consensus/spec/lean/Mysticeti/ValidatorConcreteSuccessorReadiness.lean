/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorFixedReferenceFavorablePacing
import Mysticeti.ValidatorFreshRoundPinnedSyncSource
import Mysticeti.ValidatorFreshWindowReceiverSync

namespace Mysticeti

/-! Concrete receiver readiness and an exact successor-timer upper bound.

This module uses one already-actual fresh round. It selects the latest actual
proposal deadline in that round, delivers each actual block to one receiver,
and resolves the exact pinned causal capsule with the finite per-round
admission bound.

No premise fixes a common commit head, a common round baseline, a future
family, a future window, or global commit silence. A later local commit is
harmless: the actual next proposal supplies its own GC fence. The only proposed
timer rule is action-local. It says that an already-actual exact-next timer is
started promptly when its exact current parent quorum is ready.
-/

/-- A finite maximum is either its base or one of its indexed values. -/
theorem validator_concrete_maximum_up_to_base_or_attained
    (base : Nat) (value : Nat → Nat) (count : Nat) :
    validatorTimerStartMaximumUpTo base value count = base ∨
      ∃ index, index < count ∧
        value index = validatorTimerStartMaximumUpTo base value count := by
  induction count with
  | zero =>
      exact Or.inl rfl
  | succ previous inductionHypothesis =>
      by_cases previousLeLast :
          validatorTimerStartMaximumUpTo base value previous ≤ value previous
      · right
        refine ⟨previous, by omega, ?_⟩
        simp only [validatorTimerStartMaximumUpTo,
          Nat.max_eq_right previousLeLast]
      · have lastLePrevious : value previous ≤
            validatorTimerStartMaximumUpTo base value previous := by
          omega
        rw [validatorTimerStartMaximumUpTo,
          Nat.max_eq_left lastLePrevious]
        rcases inductionHypothesis with baseValue | attained
        · exact Or.inl baseValue
        · rcases attained with ⟨index, indexInRange, indexAtMaximum⟩
          exact Or.inr ⟨index, by omega, indexAtMaximum⟩

/-- Selected actual round productions and their latest actual proposal
deadline. The selected productions can have different commit heads. -/
structure ValidatorSelectedFreshRoundDeadlineEnvelope
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (observation round : Time) where
  family : ValidatorSelectedFreshTimerPacedRoundFamily
    (timed := timed) (obligations := obligations) (waits := waits)
      observation round
  latestDeadline : Time
  latestAuthor : Nat
  latestAuthorInRange : latestAuthor < config.authorityCount
  latestAuthorCorrect : faults.correctAvailable latestAuthor = true
  latestDeadlineIsSelected :
    let selected := family.selectedAt latestAuthor latestAuthorInRange
      latestAuthorCorrect
    selected.production.timerStartedAt +
        waits.wait selected.production.commitHead round = latestDeadline
  observationBeforeLatest : observation ≤ latestDeadline
  deadlineAtMostLatest : ∀ author
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true),
    let selected := family.selectedAt author authorInRange authorCorrect
    selected.production.timerStartedAt +
        waits.wait selected.production.commitHead round ≤ latestDeadline

/-- One already-selected actual fresh family has an attained latest proposal
deadline. This theorem does not require equal local commit heads. -/
theorem selected_fresh_round_selects_deadline_envelope
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation round : Time}
    (family : ValidatorSelectedFreshTimerPacedRoundFamily
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round) :
    Nonempty (ValidatorSelectedFreshRoundDeadlineEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round) := by
  classical
  let deadlineFor := fun author =>
    if authorInRange : author < config.authorityCount then
      if authorCorrect : faults.correctAvailable author = true then
        let selected := family.selectedAt author authorInRange authorCorrect
        selected.production.timerStartedAt +
          waits.wait selected.production.commitHead round
      else
        observation
    else
      observation
  let latestDeadline := validatorTimerStartMaximumUpTo observation deadlineFor
    config.authorityCount
  have deadlineAtMostLatest : ∀ author
      (authorInRange : author < config.authorityCount)
      (authorCorrect : faults.correctAvailable author = true),
      let selected := family.selectedAt author authorInRange authorCorrect
      selected.production.timerStartedAt +
          waits.wait selected.production.commitHead round ≤ latestDeadline := by
    intro author authorInRange authorCorrect
    have bounded := validator_timer_start_le_maximum_up_to observation
      deadlineFor authorInRange
    simpa [latestDeadline, deadlineFor, authorInRange, authorCorrect] using
      bounded
  have positiveCorrectWeight : 0 <
      weight config.authorityCount config.stake faults.correctAvailable :=
    Nat.lt_of_lt_of_le config.thresholds.quorum_positive
      faults.correct_available_stake_is_quorum
  rcases positive_weight_has_member positiveCorrectWeight with
    ⟨someAuthor, someAuthorInRange, someAuthorCorrect, _positiveStake⟩
  let someSelected := family.selectedAt someAuthor someAuthorInRange
    someAuthorCorrect
  have observationBeforeLatest : observation < latestDeadline := by
    have timerBeforeDeadline : someSelected.production.timerStartedAt ≤
        someSelected.production.timerStartedAt +
          waits.wait someSelected.production.commitHead round :=
      Nat.le_add_right _ _
    exact Nat.lt_of_lt_of_le someSelected.timerAfterObservation
      (Nat.le_trans timerBeforeDeadline
        (deadlineAtMostLatest someAuthor someAuthorInRange someAuthorCorrect))
  rcases validator_concrete_maximum_up_to_base_or_attained observation
      deadlineFor config.authorityCount with baseValue | attained
  · have latestIsObservation : latestDeadline = observation := baseValue
    have impossible : observation < observation := by
      calc
        observation < latestDeadline := observationBeforeLatest
        _ = observation := latestIsObservation
    exact (Nat.lt_irrefl observation impossible).elim
  · rcases attained with
      ⟨latestAuthor, latestAuthorInRange, latestAuthorAtMaximum⟩
    have latestAuthorCorrect :
        faults.correctAvailable latestAuthor = true := by
      by_cases isCorrect : faults.correctAvailable latestAuthor = true
      · exact isCorrect
      · have atObservation : deadlineFor latestAuthor = observation := by
          simp [deadlineFor, latestAuthorInRange, isCorrect]
        have latestIsObservation : latestDeadline = observation := by
          rw [atObservation] at latestAuthorAtMaximum
          exact latestAuthorAtMaximum.symm
        have impossible : observation < observation := by
          calc
            observation < latestDeadline := observationBeforeLatest
            _ = observation := latestIsObservation
        exact (Nat.lt_irrefl observation impossible).elim
    have latestDeadlineIsSelected :
        let selected := family.selectedAt latestAuthor latestAuthorInRange
          latestAuthorCorrect
        selected.production.timerStartedAt +
            waits.wait selected.production.commitHead round =
          latestDeadline := by
      simpa [deadlineFor, latestAuthorInRange, latestAuthorCorrect] using
        latestAuthorAtMaximum
    exact ⟨{
      family
      latestDeadline
      latestAuthor
      latestAuthorInRange
      latestAuthorCorrect
      latestDeadlineIsSelected
      observationBeforeLatest := Nat.le_of_lt observationBeforeLatest
      deadlineAtMostLatest }⟩

/-- Fixed delivery and local pipeline cost after the latest actual proposal
deadline. -/
def validatorConcreteFreshRoundDeliveryCost
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) : Time :=
  3 * (timed.localActionBound + 1) + network.delta + 1

/-- Linear zero-cutoff history resolution and target-acceptance cost. -/
def validatorConcreteFreshRoundResolutionCost
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (round maxAdmittedRefsPerRound : Nat) : Time :=
  (round * maxAdmittedRefsPerRound) *
      validatorBlockSyncAcceptanceBound timed syncRules +
    timed.localActionBound + 1

/-- One common receiver deadline for all selected round blocks. -/
def validatorConcreteFreshRoundReceiverReadyDeadline
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {observation round : Time}
    (envelope : ValidatorSelectedFreshRoundDeadlineEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round)
    (maxAdmittedRefsPerRound : Nat) : Time :=
  envelope.latestDeadline + validatorConcreteFreshRoundDeliveryCost timed +
    validatorConcreteFreshRoundResolutionCost (syncRules := syncRules) timed
      round maxAdmittedRefsPerRound

/-- Total upper-edge cost, including the local exact-next timer arm. -/
def validatorConcreteFreshRoundSuccessorCost
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (round maxAdmittedRefsPerRound : Nat) : Time :=
  validatorConcreteFreshRoundDeliveryCost timed +
    validatorConcreteFreshRoundResolutionCost (syncRules := syncRules) timed
      round maxAdmittedRefsPerRound +
    timed.localActionBound + 2

/-- A head-local upper edge from one selected actual round to each
already-actual exact-next timer. The witness pays its own actual round wait;
no schedule equality between validators is required. -/
def ValidatorFreshTimerStartHeadLocalSuccessorUpperAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    {observation round : Time}
    (family : ValidatorSelectedFreshTimerPacedRoundFamily
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round)
    (stepCost : Time) : Prop :=
  ∀ {receiver}
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1)),
    ∃ author,
      ∃ authorInRange : author < config.authorityCount,
      ∃ authorCorrect : faults.correctAvailable author = true,
      let previous := family.selectedAt author authorInRange authorCorrect
      next.production.timerStartedAt ≤
        previous.production.timerStartedAt +
          waits.wait previous.production.commitHead round + stepCost

/-- Every selected author's own block is stored before the common concrete
receiver deadline. -/
theorem selected_fresh_round_block_stored_before_concrete_deadline
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound observation round : Nat}
    (envelope : ValidatorSelectedFreshRoundDeadlineEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round)
    {author : Nat}
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true) :
    (envelope.family.selectedAt author authorInRange
      authorCorrect).production.snapshot.storedAt ≤
      validatorConcreteFreshRoundReceiverReadyDeadline
        (syncRules := syncRules) timed envelope maxAdmittedRefsPerRound := by
  let production := (envelope.family.selectedAt author authorInRange
    authorCorrect).production
  have selectedDeadlineBeforeLatest :
      production.timerStartedAt + waits.wait production.commitHead round ≤
        envelope.latestDeadline :=
    envelope.deadlineAtMostLatest author authorInRange authorCorrect
  have storedBound := production.storedWithinPipelinePrefix
  have toLatest : production.snapshot.storedAt ≤
      envelope.latestDeadline + 2 * (timed.localActionBound + 1) := by
    exact Nat.le_trans storedBound
      (Nat.add_le_add_right selectedDeadlineBeforeLatest
        (2 * (timed.localActionBound + 1)))
  exact Nat.le_trans toLatest (by
    dsimp [validatorConcreteFreshRoundReceiverReadyDeadline,
      validatorConcreteFreshRoundDeliveryCost,
      validatorConcreteFreshRoundResolutionCost]
    omega)

/-- Proposed action-local promptness for an already-actual exact-next timer.

The premise is one current active local state whose signer floor is exactly
the prior round and whose exact next-round parent quorum is ready. The rule
does not assert that a timer or proposal will exist. It only bounds the timer
start already named by `next`. -/
structure ValidatorConcreteExactNextTimerPromptnessRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (obligations : ValidatorProposalObligationExecution timed)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)) : Type where
  actualExactNextStartsPromptly : ∀
    {observation readyAt receiver round : Time},
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1)) →
    (timed.execution.trace readyAt).epochActive = true →
    ((timed.execution.trace readyAt).validatorState
      receiver).highestSignedRound = round →
    ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace readyAt).validatorState receiver) (round + 1) →
    readyAt < next.production.timerStartedAt →
    next.production.timerStartedAt ≤
      readyAt + timed.localActionBound + 2

/-- A correct, available validator is not Byzantine in the fixed interval. -/
theorem validator_concrete_correct_available_not_byzantine
    {CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {validator : Nat}
    (correct : faults.correctAvailable validator = true) :
    faults.byzantine validator = false := by
  have notNonProgress : faults.nonProgress validator = false := by
    simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
      VoterSet.full] using correct
  have separated : faults.byzantine validator = false ∧
      faults.unavailable validator = false := by
    simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
      notNonProgress
  exact separated.1

/-- If the common receiver deadline is before the already-actual next timer,
all selected prior-round blocks form an exact usable quorum at that deadline.

The hard timing branch supplies the needed GC fact from the next proposal's
own legal parent snapshot. Thus, a commit install can occur during delivery or
sync. It can only help by making an item a GC root; it cannot invalidate a
still-required round block. -/
theorem selected_fresh_round_gives_concrete_receiver_readiness_before_next_start
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    {observation round : Time}
    (envelope : ValidatorSelectedFreshRoundDeadlineEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round)
    (roundPositive : 0 < round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    {receiver : Nat}
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1))
    (deadlineBeforeNextStart :
      validatorConcreteFreshRoundReceiverReadyDeadline
          (syncRules := syncRules) timed envelope maxAdmittedRefsPerRound <
        next.production.timerStartedAt) :
    ValidatorReceiverUsableCorrectQuorumLayer config faults
      (timed.execution.trace
        (validatorConcreteFreshRoundReceiverReadyDeadline
          (syncRules := syncRules) timed envelope maxAdmittedRefsPerRound))
      receiver round := by
  classical
  let readyAt := validatorConcreteFreshRoundReceiverReadyDeadline
    (syncRules := syncRules) timed envelope maxAdmittedRefsPerRound
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerCorrectAvailable
  have observationBeforeReady : observation ≤ readyAt := by
    exact Nat.le_trans envelope.observationBeforeLatest
      (by
        dsimp [readyAt, validatorConcreteFreshRoundReceiverReadyDeadline]
        exact Nat.le_trans
          (Nat.le_add_right envelope.latestDeadline
            (validatorConcreteFreshRoundDeliveryCost timed))
          (Nat.le_add_right
            (envelope.latestDeadline +
              validatorConcreteFreshRoundDeliveryCost timed)
            (validatorConcreteFreshRoundResolutionCost
              (syncRules := syncRules) timed round maxAdmittedRefsPerRound)))
  have nextStartBeforeSnapshot : next.production.timerStartedAt ≤
      next.production.snapshot.snapshotAt := by
    rw [next.production.snapshotAtDeadline]
    exact Nat.le_add_right _ _
  have readyBeforeSnapshot : readyAt ≤ next.production.snapshot.snapshotAt :=
    Nat.le_trans (Nat.le_of_lt deadlineBeforeNextStart)
      nextStartBeforeSnapshot
  have gcBelowAtSnapshot :
      ((timed.execution.trace next.production.snapshot.snapshotAt
        ).validatorState receiver).gcRound < round :=
    timer_paced_successor_snapshot_gc_below_previous_round next.production
      roundPositive
  have gcBeforeSnapshot :=
    ValidatorRecoveryCapsuleSyncExecution.validator_gc_round_mono
      (timed := timed) receiverInRange readyBeforeSnapshot
  have gcBelowAtReady :
      ((timed.execution.trace readyAt).validatorState receiver).gcRound <
        round := by
    omega
  have selectedReady : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ reference,
        ((timed.execution.trace readyAt).validatorState receiver
          ).acceptedRepresentative round author = some reference ∧
        ((timed.execution.trace readyAt).validatorState receiver).accepted
          reference = true ∧
        ((timed.execution.trace readyAt).validatorState receiver).retained
          reference = true := by
    intro author authorInRange authorCorrect
    let fresh := envelope.family.selectedAt author authorInRange authorCorrect
    let production := fresh.production
    have selectedDeadlineBeforeLatest :
        production.timerStartedAt +
            waits.wait production.commitHead round ≤ envelope.latestDeadline :=
      envelope.deadlineAtMostLatest author authorInRange authorCorrect
    have storedBeforeReady : production.snapshot.storedAt ≤ readyAt := by
      have storedBound := production.storedWithinPipelinePrefix
      have toLatest : production.snapshot.storedAt ≤
          envelope.latestDeadline + 2 * (timed.localActionBound + 1) := by
        exact Nat.le_trans storedBound
          (Nat.add_le_add_right selectedDeadlineBeforeLatest
            (2 * (timed.localActionBound + 1)))
      exact Nat.le_trans toLatest (by
        dsimp [readyAt, validatorConcreteFreshRoundReceiverReadyDeadline,
          validatorConcreteFreshRoundDeliveryCost,
          validatorConcreteFreshRoundResolutionCost]
        omega)
    have authorNotByzantine : faults.byzantine author = false :=
      validator_concrete_correct_available_not_byzantine authorCorrect
    by_cases receiverIsAuthor : receiver = author
    · subst receiver
      have ownAtReady :
          ((timed.execution.trace readyAt).validatorState author).ownBlockAt
              round = some production.snapshot.block.reference := by
        apply (timed.execution.durable_fields_persist authorInRange
          storedBeforeReady).own_block_persists
        simpa [production.proposer, production.blockRound] using
          production.snapshot.blockStored
      have ownFacts :=
        (timed.execution.statesWellFormed readyAt author authorInRange
          ).ownBlockIsSound round production.snapshot.block.reference ownAtReady
      have recorded := representatives.acceptedCorrectReferenceIsRecorded
        readyAt author production.snapshot.block.reference authorInRange
          authorCorrect
          (by simpa [production.snapshot.blockIsOwnProposal,
            production.proposer] using authorInRange)
          (by simpa [production.snapshot.blockIsOwnProposal,
            production.proposer] using authorNotByzantine)
          ownFacts.2.2.1
      refine ⟨production.snapshot.block.reference, ?_, ownFacts.2.2.1,
        ownFacts.2.2.2.1⟩
      simpa [production.blockRound, production.snapshot.blockIsOwnProposal,
        production.proposer] using recorded
    · let broadcast := Classical.choice
        (production.peerBroadcast receiver receiverInRange (by
          intro receiverEqualsAuthor
          exact receiverIsAuthor receiverEqualsAuthor))
      have timerAfterObservation : observation < production.timerStartedAt :=
        fresh.timerAfterObservation
      have sentAfterGst : network.gst ≤ broadcast.packet.sentAt := by
        exact Nat.le_trans afterGst
          (Nat.le_trans (Nat.le_of_lt timerAfterObservation)
            (Nat.le_trans (Nat.le_add_right _ _)
              (Nat.le_trans production.deadlineBeforeProposal
                (Nat.le_trans (Nat.le_add_right _ 1)
                  broadcast.proposalBeforeSend))))
      have observationBeforePersistence : observation ≤
          production.persistTime + 1 := by
        exact Nat.le_trans (Nat.le_of_lt timerAfterObservation)
          (Nat.le_trans (Nat.le_add_right _ _)
            (Nat.le_trans production.deadlineBeforeProposal
              (Nat.le_trans (Nat.le_add_right _ 1)
                (Nat.le_trans production.proposalBeforePersistence
                  (Nat.le_add_right _ 1)))))
      have activeFromPersistence : ∀ time,
          production.persistTime + 1 ≤ time →
          (timed.execution.trace time).epochActive = true := by
        intro time persistenceBeforeTime
        exact active time
          (Nat.le_trans observationBeforePersistence persistenceBeforeTime)
      have syncSource := sourceRules.sourceFor production broadcast
        receiverInRange receiverCorrect sentAfterGst activeFromPersistence
      have cutoff : ValidatorAcceptedCausalCapsuleRoundCutoffAt timed
          (admission.capsuleFor production.snapshot.block)
            (broadcast.packet.deliveredAt + 1) receiver 0 := by
        intro block _member blockAtZero
        exact Or.inr (Nat.le_trans blockAtZero (Nat.zero_le _))
      let source : ValidatorTimerPacedLinearBacklogSyncSource admission
          production broadcast (floor := 0) := {
        cutoff
        targetSyncSource := syncSource }
      have deliveryFacts := timer_paced_peer_broadcast_is_delivered production
        broadcast receiverInRange receiverCorrect sentAfterGst
      have observationBeforeDelivery : observation ≤
          broadcast.packet.deliveredAt + 1 := by
        exact Nat.le_trans (Nat.le_of_lt timerAfterObservation)
          (Nat.le_trans (Nat.le_add_right _ _)
            (Nat.le_trans production.deadlineBeforeProposal
              (Nat.le_trans (Nat.le_add_right _ 1)
                (Nat.le_trans broadcast.proposalBeforeSend
                  (Nat.le_trans deliveryFacts.1 (Nat.le_add_right _ 1))))))
      have activeFromDelivery : ∀ time,
          broadcast.packet.deliveredAt + 1 ≤ time →
          (timed.execution.trace time).epochActive = true := by
        intro time deliveryBeforeTime
        exact active time
          (Nat.le_trans observationBeforeDelivery deliveryBeforeTime)
      rcases timer_paced_peer_broadcast_resolves_within_linear_backlog_or_gc
          admission acceptance production broadcast source receiverInRange
            receiverCorrect sentAfterGst activeFromDelivery with
        ⟨acceptedAt, _deliveryBeforeAccepted, acceptedBound,
          acceptedOrRoot⟩
      have sentBound := timer_paced_peer_broadcast_sent_within_round_pipeline
        production broadcast
      have acceptedBeforeReady : acceptedAt ≤ readyAt := by
        have deliveredBound := deliveryFacts.2.1
        let backlog := (round * maxAdmittedRefsPerRound) *
          validatorBlockSyncAcceptanceBound timed syncRules
        calc
          acceptedAt ≤ broadcast.packet.deliveredAt + 1 + backlog +
                timed.localActionBound + 1 := by
            simpa [backlog, Nat.add_assoc] using acceptedBound
          _ ≤ broadcast.packet.sentAt + network.delta + 1 + backlog +
                timed.localActionBound + 1 := by
            have shifted := Nat.add_le_add_right deliveredBound
              (1 + backlog + timed.localActionBound + 1)
            simpa only [Nat.add_assoc] using shifted
          _ ≤ (production.timerStartedAt +
                  waits.wait production.commitHead round +
                    3 * (timed.localActionBound + 1)) +
                network.delta + 1 + backlog + timed.localActionBound + 1 := by
            have shifted := Nat.add_le_add_right sentBound
              (network.delta + 1 + backlog + timed.localActionBound + 1)
            simpa only [Nat.add_assoc] using shifted
          _ ≤ (envelope.latestDeadline +
                  3 * (timed.localActionBound + 1)) +
                network.delta + 1 + backlog + timed.localActionBound + 1 := by
            have shifted := Nat.add_le_add_right selectedDeadlineBeforeLatest
              (3 * (timed.localActionBound + 1) + network.delta + 1 +
                backlog + timed.localActionBound + 1)
            simpa only [Nat.add_assoc] using shifted
          _ = readyAt := by
            dsimp [readyAt,
              validatorConcreteFreshRoundReceiverReadyDeadline,
              validatorConcreteFreshRoundDeliveryCost,
              validatorConcreteFreshRoundResolutionCost]
            dsimp [backlog]
            simp only [Nat.mul_assoc, Nat.mul_comm,
              Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
      have acceptedAtSource :
          ((timed.execution.trace acceptedAt).validatorState receiver).accepted
            production.snapshot.block.reference = true := by
        rcases acceptedOrRoot with accepted | atRoot
        · exact accepted
        · have gcAtAcceptedBeforeReady :=
            ValidatorRecoveryCapsuleSyncExecution.validator_gc_round_mono
              (timed := timed) receiverInRange acceptedBeforeReady
          rw [production.blockRound] at atRoot
          omega
      have acceptedAtReady :
          ((timed.execution.trace readyAt).validatorState receiver).accepted
            production.snapshot.block.reference = true :=
        timed.execution.accepted_block_persists receiverInRange
          acceptedBeforeReady acceptedAtSource
      have retainedAtReady :
          ((timed.execution.trace readyAt).validatorState receiver).retained
            production.snapshot.block.reference = true :=
        retention.acceptedAboveGcIsRetained readyAt receiver
          production.snapshot.block.reference receiverInRange receiverCorrect
            acceptedAtReady (by simpa [production.blockRound] using
              gcBelowAtReady)
      have recorded := representatives.acceptedCorrectReferenceIsRecorded
        readyAt receiver production.snapshot.block.reference receiverInRange
          receiverCorrect
          (by simpa [production.snapshot.blockIsOwnProposal,
            production.proposer] using authorInRange)
          (by simpa [production.snapshot.blockIsOwnProposal,
            production.proposer] using authorNotByzantine)
          acceptedAtReady
      refine ⟨production.snapshot.block.reference, ?_, acceptedAtReady,
        retainedAtReady⟩
      simpa [production.blockRound, production.snapshot.blockIsOwnProposal,
        production.proposer] using recorded
  have acceptedSome : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      (((timed.execution.trace readyAt).validatorState receiver
        ).acceptedRepresentative round author).isSome = true := by
    intro author authorInRange authorCorrect
    rcases selectedReady author authorInRange authorCorrect with
      ⟨reference, recorded, _accepted, _retained⟩
    simp [recorded]
  have retainedRepresentative : ∀ author reference,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ((timed.execution.trace readyAt).validatorState receiver
        ).acceptedRepresentative round author = some reference →
      ((timed.execution.trace readyAt).validatorState receiver).retained
        reference = true := by
    intro author reference authorInRange authorCorrect recorded
    rcases selectedReady author authorInRange authorCorrect with
      ⟨selected, selectedRecorded, _accepted, selectedRetained⟩
    have sameReference : reference = selected :=
      Option.some.inj (recorded.symm.trans selectedRecorded)
    simpa [sameReference] using selectedRetained
  have nextParents :=
    receiver_retained_correct_round_gives_recovery_parent_quorum
      representatives receiverInRange receiverCorrect acceptedSome
        retainedRepresentative gcBelowAtReady
  have usable : ValidatorReceiverUsableCorrectQuorumLayer config faults
      (timed.execution.trace readyAt) receiver round := by
    refine {
      roundAboveGc := gcBelowAtReady
      correctStakeIsQuorum := faults.correct_available_stake_is_quorum
      exactCorrectRepresentatives := ?_
      nextParentsReady := nextParents }
    intro author authorInRange authorCorrect
    rcases selectedReady author authorInRange authorCorrect with
      ⟨reference, recorded, accepted, retained⟩
    have sound :=
      (timed.execution.statesWellFormed readyAt receiver receiverInRange
        ).acceptedRepresentativeIsSound round author reference recorded
    rcases sound.2.2.2 with ⟨block, catalogued, blockReference⟩
    exact ⟨reference, block, recorded, accepted, retained, catalogued,
      blockReference, sound.1, sound.2.1⟩
  simpa only [readyAt] using usable

/-- One actual prior-round family gives a quantitative upper edge for every
already-actual exact-next timer.

The proof first checks whether the timer already started by the concrete
receiver deadline. In the remaining case, the theorem above constructs the
exact usable quorum at that deadline. The action-local promptness rule then
bounds the same actual next timer. -/
theorem selected_fresh_round_gives_head_local_successor_timer_upper
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    (promptness : ValidatorConcreteExactNextTimerPromptnessRules timed
      obligations waits)
    {observation round : Time}
    (envelope : ValidatorSelectedFreshRoundDeadlineEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round)
    (roundPositive : 0 < round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorFreshTimerStartHeadLocalSuccessorUpperAt timed obligations waits
      envelope.family
        (validatorConcreteFreshRoundSuccessorCost (syncRules := syncRules)
          timed round maxAdmittedRefsPerRound) := by
  intro receiver next
  let readyAt := validatorConcreteFreshRoundReceiverReadyDeadline
    (syncRules := syncRules) timed envelope maxAdmittedRefsPerRound
  let stepCost := validatorConcreteFreshRoundSuccessorCost
    (syncRules := syncRules) timed round maxAdmittedRefsPerRound
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerCorrectAvailable
  let latest := envelope.family.selectedAt envelope.latestAuthor
    envelope.latestAuthorInRange envelope.latestAuthorCorrect
  have readyWithArm : readyAt + timed.localActionBound + 2 =
      envelope.latestDeadline + stepCost := by
    dsimp [readyAt, stepCost,
      validatorConcreteFreshRoundReceiverReadyDeadline,
      validatorConcreteFreshRoundSuccessorCost,
      validatorConcreteFreshRoundDeliveryCost,
      validatorConcreteFreshRoundResolutionCost]
    simp only [Nat.add_assoc]
  have latestForm : envelope.latestDeadline + stepCost =
      latest.production.timerStartedAt +
        waits.wait latest.production.commitHead round + stepCost := by
    rw [envelope.latestDeadlineIsSelected]
  by_cases timerByReady : next.production.timerStartedAt ≤ readyAt
  · refine ⟨envelope.latestAuthor, envelope.latestAuthorInRange,
      envelope.latestAuthorCorrect, ?_⟩
    have readyArmToLatest : readyAt + (timed.localActionBound + 2) =
        latest.production.timerStartedAt +
          waits.wait latest.production.commitHead round + stepCost := by
      calc
        readyAt + (timed.localActionBound + 2) =
            readyAt + timed.localActionBound + 2 := by
          simp only [Nat.add_assoc]
        _ = envelope.latestDeadline + stepCost := readyWithArm
        _ = latest.production.timerStartedAt +
              waits.wait latest.production.commitHead round + stepCost :=
          latestForm
    exact Nat.le_trans timerByReady
      (Nat.le_trans (Nat.le_add_right readyAt
        (timed.localActionBound + 2)) (by
          simpa only [latest, stepCost] using Nat.le_of_eq readyArmToLatest))
  · have readyBeforeTimer : readyAt < next.production.timerStartedAt := by
      exact Nat.lt_of_not_ge timerByReady
    have usable :=
      selected_fresh_round_gives_concrete_receiver_readiness_before_next_start
        admission sourceRules acceptance representatives retention envelope
          roundPositive afterGst active next (by simpa only [readyAt] using
            readyBeforeTimer)
    let previousAtReceiver := envelope.family.selectedAt receiver
      receiverInRange receiverCorrect
    have previousStoredBeforeReady :
        previousAtReceiver.production.snapshot.storedAt ≤ readyAt := by
      simpa only [readyAt, previousAtReceiver] using
        selected_fresh_round_block_stored_before_concrete_deadline
          (syncRules := syncRules) (maxAdmittedRefsPerRound :=
            maxAdmittedRefsPerRound) envelope receiverInRange receiverCorrect
    have ownAtReady :
        ((timed.execution.trace readyAt).validatorState receiver).ownBlockAt
            round =
          some previousAtReceiver.production.snapshot.block.reference := by
      apply (timed.execution.durable_fields_persist receiverInRange
        previousStoredBeforeReady).own_block_persists
      simpa [previousAtReceiver.production.proposer,
        previousAtReceiver.production.blockRound] using
          previousAtReceiver.production.snapshot.blockStored
    have roundAtMostReadyFloor : round ≤
        ((timed.execution.trace readyAt).validatorState
          receiver).highestSignedRound :=
      (timed.execution.statesWellFormed readyAt receiver receiverInRange
        ).ownBlockDoesNotExceedSignerFloor round
          previousAtReceiver.production.snapshot.block.reference ownAtReady
    have readyBeforeTimerWeak : readyAt ≤ next.production.timerStartedAt :=
      Nat.le_of_lt readyBeforeTimer
    have readyFloorAtMostTimerFloor :=
      (timed.execution.durableStateMonotone receiver readyAt
        next.production.timerStartedAt receiverInRange readyBeforeTimerWeak
        ).2.2.2.2.2.2.1
    have timerFloor :
        ((timed.execution.trace next.production.timerStartedAt).validatorState
          receiver).highestSignedRound = round := by
      have exactNext := next.production.timerStartsExactNext
      omega
    have readyFloor :
        ((timed.execution.trace readyAt).validatorState
          receiver).highestSignedRound = round := by
      apply Nat.le_antisymm
      · simpa only [timerFloor] using readyFloorAtMostTimerFloor
      · exact roundAtMostReadyFloor
    have observationBeforeReady : observation ≤ readyAt := by
      exact Nat.le_trans envelope.observationBeforeLatest (by
        dsimp [readyAt, validatorConcreteFreshRoundReceiverReadyDeadline]
        exact Nat.le_trans
          (Nat.le_add_right envelope.latestDeadline
            (validatorConcreteFreshRoundDeliveryCost timed))
          (Nat.le_add_right
            (envelope.latestDeadline +
              validatorConcreteFreshRoundDeliveryCost timed)
            (validatorConcreteFreshRoundResolutionCost
              (syncRules := syncRules) timed round maxAdmittedRefsPerRound)))
    have activeAtReady :
        (timed.execution.trace readyAt).epochActive = true :=
      active readyAt observationBeforeReady
    have prompt := promptness.actualExactNextStartsPromptly next activeAtReady
      readyFloor (by simpa only [readyAt] using usable.nextParentsReady)
        readyBeforeTimer
    refine ⟨envelope.latestAuthor, envelope.latestAuthorInRange,
      envelope.latestAuthorCorrect, ?_⟩
    exact Nat.le_trans prompt (by
      rw [readyWithArm, latestForm]
      exact Nat.le_refl _)

/-- A head-local upper edge becomes the existing recurrence edge when all
local heads use the same wait value at this exact round. -/
theorem ValidatorFreshTimerStartHeadLocalSuccessorUpperAt.toCommonWait
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {observation round roundWait stepCost : Time}
    {family : ValidatorSelectedFreshTimerPacedRoundFamily
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round}
    (upper : ValidatorFreshTimerStartHeadLocalSuccessorUpperAt timed
      obligations waits family stepCost)
    (waitAtRound : ∀ head, waits.wait head round = roundWait) :
    ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
      observation round roundWait stepCost := by
  intro receiver next
  rcases upper next with
    ⟨author, authorInRange, authorCorrect, bounded⟩
  let previous := family.selectedAt author authorInRange authorCorrect
  refine ⟨author, previous, ?_⟩
  change next.production.timerStartedAt ≤
    previous.production.timerStartedAt +
      waits.wait previous.production.commitHead round + stepCost at bounded
  rw [waitAtRound previous.production.commitHead] at bounded
  exact bounded

/-- The concrete receiver theorem exposed through the timer-spread recurrence
API. `waitAtRound` is the fixed-reference, head-independent wait rule for this
one round. -/
theorem selected_fresh_round_gives_concrete_timer_start_successor_upper
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    (promptness : ValidatorConcreteExactNextTimerPromptnessRules timed
      obligations waits)
    {observation round roundWait : Time}
    (envelope : ValidatorSelectedFreshRoundDeadlineEnvelope
      (timed := timed) (obligations := obligations) (waits := waits)
        observation round)
    (waitAtRound : ∀ head, waits.wait head round = roundWait)
    (roundPositive : 0 < round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
      observation round roundWait
        (validatorConcreteFreshRoundSuccessorCost (syncRules := syncRules)
          timed round maxAdmittedRefsPerRound) := by
  intro receiver next
  rcases (selected_fresh_round_gives_head_local_successor_timer_upper
      admission sourceRules acceptance representatives retention promptness
        envelope roundPositive afterGst active) next with
    ⟨author, authorInRange, authorCorrect, bounded⟩
  let previous := envelope.family.selectedAt author authorInRange authorCorrect
  refine ⟨author, previous, ?_⟩
  change next.production.timerStartedAt ≤
    previous.production.timerStartedAt +
      waits.wait previous.production.commitHead round +
        validatorConcreteFreshRoundSuccessorCost (syncRules := syncRules)
          timed round maxAdmittedRefsPerRound at bounded
  rw [waitAtRound previous.production.commitHead] at bounded
  exact bounded

/-- An already-derived exact fresh round selects its current/past deadline
envelope and gives the concrete recurrence upper edge. No next-round family or
window is an input. -/
theorem fresh_timer_paced_exact_round_gives_concrete_timer_start_successor_upper
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    (promptness : ValidatorConcreteExactNextTimerPromptnessRules timed
      obligations waits)
    {observation round roundWait : Time}
    (fresh : EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed
      obligations waits observation round)
    (waitAtRound : ∀ head, waits.wait head round = roundWait)
    (roundPositive : 0 < round)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
      observation round roundWait
        (validatorConcreteFreshRoundSuccessorCost (syncRules := syncRules)
          timed round maxAdmittedRefsPerRound) := by
  let family := Classical.choice
    (fresh_timer_paced_exact_round_selects_family fresh)
  let envelope := Classical.choice
    (selected_fresh_round_selects_deadline_envelope family)
  intro receiver next
  exact (selected_fresh_round_gives_concrete_timer_start_successor_upper
    admission sourceRules acceptance representatives retention promptness
      envelope waitAtRound roundPositive afterGst active) next

end Mysticeti
