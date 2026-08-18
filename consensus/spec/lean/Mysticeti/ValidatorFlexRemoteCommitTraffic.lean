/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommitProgressRecovery
import Mysticeti.ValidatorExactNextRecovery
import Mysticeti.ValidatorFlexScanEvidence

namespace Mysticeti

/-!
Receiver-local traffic from one actual successful remote FlexCommitter run.

This module does not state that a future run or commit occurs. It starts with
one actual successful run. Its selected correct vote child is either below a
fixed finite start bound, or its exact author persistence is post-start and the
ordinary proposal broadcast reaches one fixed correct receiver. A commit at a
different validator cannot discharge the receiver-local alternative.

These results are support lemmas for finite commit interruptions. The network
DAG proof must derive ordinary proposal, broadcast, and common acceptance
without using a remote commit or receiver commit advance as its public result.
-/

/-- The fixed per-author and per-receiver round bound at one start time. -/
def ValidatorFlexReceiverEvidenceBound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (initial : ValidatorFlexInitialDagSupport timed)
    (start author receiver : Nat) : Nat :=
  Nat.max (initial.roundBound author)
    (Nat.max
      ((timed.execution.trace start).validatorState
        author).highestSignedRound
      ((timed.execution.trace start).validatorState receiver).gcRound)

/-- One finite start-time evidence bound that covers every in-range author for
one receiver. This value is proof-only. -/
def ValidatorFlexReceiverUniformEvidenceBound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (initial : ValidatorFlexInitialDagSupport timed)
    (start receiver : Nat) : Nat :=
  Nat.max
    (CommitProgressRecoveryView.maxSelectedRound config.authorityCount
      (fun _ => true)
      (fun author => Nat.max (initial.roundBound author)
        ((timed.execution.trace start).validatorState
          author).highestSignedRound))
    ((timed.execution.trace start).validatorState receiver).gcRound

/-- The uniform receiver bound covers each concrete in-range vote author. -/
theorem flex_receiver_evidence_bound_le_uniform
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (initial : ValidatorFlexInitialDagSupport timed)
    {start author receiver : Nat}
    (authorInRange : author < config.authorityCount) :
    ValidatorFlexReceiverEvidenceBound initial start author receiver ≤
      ValidatorFlexReceiverUniformEvidenceBound initial start receiver := by
  have authorPairBound :=
    CommitProgressRecoveryView.selected_round_le_max_selected_round
      (authorityCount := config.authorityCount)
      (selected := fun _ => true)
      (roundOf := fun selectedAuthor => Nat.max
        (initial.roundBound selectedAuthor)
        ((timed.execution.trace start).validatorState
          selectedAuthor).highestSignedRound)
      authorInRange rfl
  unfold ValidatorFlexReceiverEvidenceBound
    ValidatorFlexReceiverUniformEvidenceBound
  rw [Nat.max_le]
  constructor
  · exact Nat.le_trans (Nat.le_max_left _ _)
      (Nat.le_trans authorPairBound (Nat.le_max_left _ _))
  · rw [Nat.max_le]
    constructor
    · exact Nat.le_trans (Nat.le_max_right _ _)
        (Nat.le_trans authorPairBound (Nat.le_max_left _ _))
    · exact Nat.le_max_right _ _

/-- Two actual successful runs on adjacent exact heads use strictly increasing
candidate rounds after the code-faithful pending refresh. -/
theorem adjacent_correct_exact_flex_runs_increase_candidate_round
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
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
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (earlier later : CorrectExactFlexRun runtime)
    (adjacent : later.prior = earlier.output.toCommitHead) :
    earlier.output.candidate.leaderRound <
      later.output.candidate.leaderRound := by
  have earlierPrepared :
      (validatorFlexRunStateAt source schedule mapping.cacheAt
        mapping.highestAcceptedRound earlier.observation).output =
          some earlier.output := by
    rw [← mapping.returned_observation_uses_prepared_scan earlier.returned]
    exact earlier.successful
  have laterPrepared :
      (validatorFlexRunStateAt source schedule mapping.cacheAt
        mapping.highestAcceptedRound later.observation).output =
          some later.output := by
    rw [← mapping.returned_observation_uses_prepared_scan later.returned]
    exact later.successful
  have nextMinimum := mapping.successful_output_sets_next_minimum
    earlier.occurs earlierPrepared
  have laterInside := mapping.actual_successful_candidate_is_inside_prepared_frontier
    later.occurs laterPrepared
  have laterHead : later.observation.input.commitHead =
      earlier.output.toCommitHead := later.priorAtInput.trans adjacent
  rw [laterHead] at laterInside
  omega

/-- A positive exact installed-prefix path ends in one actual run whose
candidate round is at least the predecessor path length. -/
theorem exact_commit_path_has_high_last_flex_run
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
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
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    {genesis : ValidatorCommitHead CommitId}
    {length : Nat} {reference : ValidatorCommitHead CommitId}
    (path : ExactCommitPath (ExactFlexSuccessor runtime) genesis length
      reference)
    (positive : 0 < length) :
    ∃ prior : ValidatorCommitHead CommitId,
      ∃ run : CorrectExactFlexRun runtime,
        run.prior = prior ∧
        run.output.toCommitHead = reference ∧
        length - 1 ≤ run.output.candidate.leaderRound := by
  induction path with
  | genesis => omega
  | @next priorLength prior final priorPath finalStep inductionHypothesis =>
      rcases finalStep with ⟨later, laterPrior, laterOutput⟩
      refine ⟨prior, later, laterPrior, laterOutput, ?_⟩
      by_cases priorPositive : 0 < priorLength
      · rcases inductionHypothesis priorPositive with
          ⟨earlierPrior, earlier, earlierPriorShape, earlierOutput,
            earlierBound⟩
        have adjacent : later.prior = earlier.output.toCommitHead :=
          laterPrior.trans earlierOutput.symm
        have increased := adjacent_correct_exact_flex_runs_increase_candidate_round
          mapping earlier later adjacent
        omega
      · omega

/-- A correct exact vote child above the finite initial and start-time author
floors produces one exact ordinary broadcast after that start. -/
theorem high_correct_flex_vote_child_produces_exact_round_broadcast
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
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
    {obligations : ValidatorProposalObligationExecution timed}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (authenticatedBodies : ValidatorFlexAuthenticatedBodySourceMap mapping)
    {start : Time}
    {run : CorrectExactFlexRun runtime}
    {leader : LeaderBlockRef BlockId} {voter : Nat}
    {reference : ValidatorBlockRef BlockId}
    (voterInRange : voter < config.authorityCount)
    (voterCorrect : faults.correctAvailable voter = true)
    (child : ValidatorFlexExactVoteChild
      (mapping.dagSupport run.observation run.occurs)
      leader voter reference)
    (aboveInitial : initial.roundBound voter < reference.round)
    (aboveAuthorFloor :
      ((timed.execution.trace start).validatorState
        voter).highestSignedRound < reference.round) :
    ∃ exact : ValidatorExactRoundBroadcastProduction timed obligations
        start reference.author reference.round,
      exact.production.proposal.block =
        (mapping.dagSupport run.observation run.occurs).body reference := by
  have exactPersistence :=
    correct_flex_vote_child_above_initial_bound_has_exact_persistence effects
      mapping authenticatedBodies voterInRange voterCorrect child aboveInitial
  have referenceAuthorInRange : reference.author < config.authorityCount := by
    rw [child.author]
    exact voterInRange
  have referenceAuthorCorrect :
      faults.correctAvailable reference.author = true := by
    rw [child.author]
    exact voterCorrect
  have aboveReferenceAuthorFloor :
      ((timed.execution.trace start).validatorState
        reference.author).highestSignedRound < reference.round := by
    simpa only [child.author] using aboveAuthorFloor
  let support := mapping.dagSupport run.observation run.occurs
  have bodyExactReference : (support.body reference).reference = reference :=
    support.bodyHasExactReference reference child.member
  have aboveBodyAuthorFloor :
      ((timed.execution.trace start).validatorState
        reference.author).highestSignedRound <
          (support.body reference).reference.round := by
    simpa only [bodyExactReference] using aboveReferenceAuthorFloor
  rcases flex_persistence_above_time_floor_occurs_after start
      referenceAuthorInRange aboveBodyAuthorFloor
        exactPersistence with
    ⟨persistTime, persistAfterStart, _persistBeforeRun, persistOccurs⟩
  rcases persist_proposal_occurrence_eventually_produces_exact_broadcast
      latchSource effects authorityCountAtLeastTwo referenceAuthorInRange
        referenceAuthorCorrect persistAfterStart (by
          simpa only [bodyExactReference] using aboveReferenceAuthorFloor)
          persistOccurs with
    ⟨⟨production, persistTimeShape, blockShape⟩⟩
  let exact : ValidatorExactRoundBroadcastProduction timed obligations
      start reference.author reference.round :=
    { production
      exactRound := by
        rw [blockShape, bodyExactReference] }
  exact ⟨exact, blockShape⟩

/-- One actual successful remote run supplies bounded old evidence, advances
the fixed receiver, or installs the exact selected vote child as a stable
accepted and retained representative at that receiver. -/
theorem correct_exact_flex_run_vote_traffic_reaches_receiver_or_is_bounded
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
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
    {obligations : ValidatorProposalObligationExecution timed}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (authenticatedVotes : AuthenticatedFlexVoteSourceMap faults functions
      context source)
    (authenticatedBodies : ValidatorFlexAuthenticatedBodySourceMap mapping)
    (scanEvidence : ValidatorFlexScanEvidenceSourceMap mapping
      authenticatedVotes)
    (run : CorrectExactFlexRun runtime)
    {start : Time}
    {receiver : Nat}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ leader voter reference,
      leader ∈ run.output.candidate.orderedCommittedLeaders ∧
        voter < config.authorityCount ∧
        faults.correctAvailable voter = true ∧
        ValidatorFlexExactVoteChild
          (mapping.dagSupport run.observation run.occurs)
          leader voter reference ∧
        (reference.round ≤
            ValidatorFlexReceiverEvidenceBound initial start voter receiver ∨
          ValidatorReceiverCommitAdvance timed start receiver ∨
          ∃ readyAt,
            start ≤ readyAt ∧
              ∀ later, readyAt ≤ later →
                ((timed.execution.trace later).validatorState receiver
                    ).acceptedRepresentative reference.round voter =
                      some reference ∧
                  ((timed.execution.trace later).validatorState receiver
                    ).retained reference = true) := by
  rcases correct_exact_flex_run_has_correct_available_vote_child mapping
      authenticatedVotes scanEvidence run with
    ⟨leader, voter, reference, leaderMember, voterInRange, voterCorrect,
      child⟩
  refine ⟨leader, voter, reference, leaderMember, voterInRange, voterCorrect,
    child, ?_⟩
  by_cases aboveInitial : initial.roundBound voter < reference.round
  · by_cases aboveAuthorFloor :
        ((timed.execution.trace start).validatorState
          voter).highestSignedRound < reference.round
    · by_cases aboveReceiverGc :
          ((timed.execution.trace start).validatorState
            receiver).gcRound < reference.round
      · right
        let exact := Classical.choose
          (high_correct_flex_vote_child_produces_exact_round_broadcast
            latchSource effects authorityCountAtLeastTwo mapping
              authenticatedBodies voterInRange voterCorrect child aboveInitial
                aboveAuthorFloor)
        have exactBlock := Classical.choose_spec
          (high_correct_flex_vote_child_produces_exact_round_broadcast
            latchSource effects authorityCountAtLeastTwo mapping
              authenticatedBodies voterInRange voterCorrect child aboveInitial
                aboveAuthorFloor)
        have referenceAuthorInRange :
            reference.author < config.authorityCount := by
          rw [child.author]
          exact voterInRange
        have referenceAuthorCorrect :
            faults.correctAvailable reference.author = true := by
          rw [child.author]
          exact voterCorrect
        rcases exact_round_broadcast_reaches_receiver_or_receiver_advances
            pins capsuleSync acceptance representatives exact
              referenceAuthorInRange referenceAuthorCorrect receiverInRange
                receiverCorrect aboveReceiverGc afterGst active with
          receiverAdvanced | stable
        · exact Or.inl receiverAdvanced
        · right
          rcases stable with ⟨readyAt, gstBeforeReady, retained⟩
          refine ⟨readyAt, gstBeforeReady, ?_⟩
          intro later readyBeforeLater
          have bodyExact :=
            (mapping.dagSupport run.observation run.occurs
              ).bodyHasExactReference reference child.member
          have exactReference :
              exact.production.proposal.block.reference = reference := by
            exact (congrArg ValidatorBlock.reference exactBlock).trans bodyExact
          have stableAtLater := retained later readyBeforeLater
          simpa only [exactReference, child.author] using stableAtLater
      · left
        simp only [ValidatorFlexReceiverEvidenceBound]
        exact Nat.le_trans (Nat.le_of_not_gt aboveReceiverGc)
          (Nat.le_trans
            (Nat.le_max_right
              ((timed.execution.trace start).validatorState
                voter).highestSignedRound
              ((timed.execution.trace start).validatorState
                receiver).gcRound)
            (Nat.le_max_right (initial.roundBound voter)
              (Nat.max
                ((timed.execution.trace start).validatorState
                  voter).highestSignedRound
                ((timed.execution.trace start).validatorState
                  receiver).gcRound)))
    · left
      simp only [ValidatorFlexReceiverEvidenceBound]
      exact Nat.le_trans (Nat.le_of_not_gt aboveAuthorFloor)
        (Nat.le_trans
          (Nat.le_max_left
            ((timed.execution.trace start).validatorState
              voter).highestSignedRound
            ((timed.execution.trace start).validatorState
              receiver).gcRound)
          (Nat.le_max_right (initial.roundBound voter)
            (Nat.max
              ((timed.execution.trace start).validatorState
                voter).highestSignedRound
              ((timed.execution.trace start).validatorState
                receiver).gcRound)))
  · left
    simp only [ValidatorFlexReceiverEvidenceBound]
    exact Nat.le_trans (Nat.le_of_not_gt aboveInitial) (Nat.le_max_left _ _)

/-- One sufficiently high exact installed head derives fresh ordinary vote
traffic for a fixed receiver, unless that receiver advances first.

The successful run, vote child, author persistence, broadcast, delivery, and
receiver acceptance are all derived. None is an input to this theorem. -/
theorem high_exact_installed_head_gives_receiver_advance_or_fresh_vote_traffic
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
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
    {obligations : ValidatorProposalObligationExecution timed}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {genesis : ValidatorCommitHead CommitId}
    {durable : ExactCommitDurablePrefixSourceMap faults
      timed.execution.trace genesis}
    {validChain : Nat → List (CommonCommitRef CommitId) → Prop}
    {validBlocks : CommitSyncBundle BlockId CommitId → Prop}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (capsuleSync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (authenticatedVotes : AuthenticatedFlexVoteSourceMap faults functions
      context source)
    (authenticatedBodies : ValidatorFlexAuthenticatedBodySourceMap mapping)
    (scanEvidence : ValidatorFlexScanEvidenceSourceMap mapping
      authenticatedVotes)
    (provenance : ExactCommitInstallProvenance runtime durable validChain
      validBlocks)
    {start installTime sourceValidator receiver : Time}
    {installedReference : ValidatorCommitHead CommitId}
    (sourceInRange : sourceValidator < config.authorityCount)
    (sourceCorrect : faults.correctAvailable sourceValidator = true)
    (installed : durable.exactInstalledHead installTime sourceValidator
      installedReference)
    (highIndex :
      ValidatorFlexReceiverUniformEvidenceBound initial start receiver + 2 ≤
        installedReference.index)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorReceiverCommitAdvance timed start receiver ∨
      ∃ run : CorrectExactFlexRun runtime,
        ∃ leader voter reference,
          leader ∈ run.output.candidate.orderedCommittedLeaders ∧
            voter < config.authorityCount ∧
            faults.correctAvailable voter = true ∧
            ValidatorFlexExactVoteChild
              (mapping.dagSupport run.observation run.occurs)
              leader voter reference ∧
            ValidatorFlexReceiverUniformEvidenceBound initial start receiver <
              reference.round ∧
            ∃ readyAt,
              start ≤ readyAt ∧
                ∀ later, readyAt ≤ later →
                  ((timed.execution.trace later).validatorState receiver
                      ).acceptedRepresentative reference.round voter =
                        some reference ∧
                    ((timed.execution.trace later).validatorState receiver
                      ).retained reference = true := by
  have path := provenance.exactInstalledHeadHasPath sourceInRange sourceCorrect
    installed
  have positive : 0 < installedReference.index := by omega
  rcases exact_commit_path_has_high_last_flex_run mapping path positive with
    ⟨prior, run, runPrior, runOutput, runRoundBound⟩
  rcases correct_exact_flex_run_vote_traffic_reaches_receiver_or_is_bounded
      latchSource effects authorityCountAtLeastTwo pins capsuleSync acceptance
        representatives mapping authenticatedVotes authenticatedBodies
          scanEvidence run receiverInRange receiverCorrect afterGst active with
    ⟨leader, voter, reference, leaderMember, voterInRange, voterCorrect,
      child, bounded | receiverAdvanced | stable⟩
  · have leaderRound :=
      correct_exact_flex_run_committed_leaders_match_candidate_round
        authenticatedVotes run leader leaderMember
    have concreteBound := flex_receiver_evidence_bound_le_uniform initial
      (start := start) (receiver := receiver) voterInRange
    have childRound := child.round
    exact False.elim (by omega)
  · exact Or.inl receiverAdvanced
  · right
    rcases stable with ⟨readyAt, startBeforeReady, stableAt⟩
    have leaderRound :=
      correct_exact_flex_run_committed_leaders_match_candidate_round
        authenticatedVotes run leader leaderMember
    have childRound := child.round
    have referenceAbove :
        ValidatorFlexReceiverUniformEvidenceBound initial start receiver <
          reference.round := by
      omega
    exact ⟨run, leader, voter, reference, leaderMember, voterInRange,
      voterCorrect, child, referenceAbove, readyAt, startBeforeReady, stableAt⟩

end Mysticeti
