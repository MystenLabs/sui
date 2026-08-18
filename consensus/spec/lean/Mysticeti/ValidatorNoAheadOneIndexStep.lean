/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCollectiveRecoveryCarrier
import Mysticeti.ValidatorFavorableFreshWindowAlignment
import Mysticeti.ValidatorPointwiseCommitHeadAlignment
import Mysticeti.ValidatorPreparedFlexInstall
import Mysticeti.ValidatorSameHeadCausalTiming

namespace Mysticeti

/-!
The deterministic local step for the branch in which no correct, available
validator is already past the exact installed prior commit.

The lower theorem starts from one already-derived finite fresh window and its
past adjacent-parent evidence. The receiver's round-plus-two proposals give a
direct-vote range, and the current prepared Flex scan records the exact next
local result. A higher theorem must derive the window and its edges from the
favorable path and finite timer recurrence. No carrier, Flex run, or install is
an input.
-/

/-- An accepted direct-vote witness is a quorum in the exact voter set read by
the prepared Flex scan. -/
theorem receiver_accepted_direct_vote_quorum_gives_trace_quorum
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {receiver : Nat} {leader : ValidatorBlockRef BlockId}
    (accepted : ValidatorReceiverAcceptedDirectVoteQuorum config world
      receiver leader) :
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters world receiver leader) := by
  rcases accepted with ⟨voters, quorum, evidence⟩
  exact Nat.le_trans quorum (by
    apply weight_mono config.stake
    intro voter voterInRange voterSelected
    rcases evidence voter voterInRange voterSelected with
      ⟨voteReference, voteBlock, representative, _accepted, catalog,
        blockReference, voteAuthor, voteRound, leaderParent⟩
    exact accepted_child_with_leader_parent_is_direct_voter representative
      catalog blockReference voteAuthor voteRound leaderParent)

/-- The two finite adjacent edges required for each leader offset in one
already-derived aligned fresh window. This proposition contains only past
relations between the concrete productions in that window. -/
def ValidatorAlignedFavorableAdjacentParentEvidence
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
    {prior : ValidatorCommitHead CommitId}
    {observation windowBase leaderCount : Nat}
    (aligned : ValidatorAlignedFavorableFreshWindow timed obligations waits
      prior observation windowBase leaderCount)
    (receiver : Nat) : Prop :=
  ∀ offset,
    offset < leaderCount →
    ∀
      (leader : ValidatorFreshTimerPacedExactRoundProduction timed obligations
        waits observation (aligned.leaderAuthorAt offset)
          (windowBase + offset + 2))
      (carrier : ValidatorFreshTimerPacedExactRoundProduction timed obligations
        waits observation receiver ((windowBase + offset + 2) + 2))
      voter,
      voter < config.authorityCount →
      faults.correctAvailable voter = true →
      ∀ vote : ValidatorFreshTimerPacedExactRoundProduction timed obligations
        waits observation voter ((windowBase + offset + 2) + 1),
        ValidatorAdjacentTimerPacedParentEvidence leader.production
            vote.production ∧
          ValidatorAdjacentTimerPacedParentEvidence vote.production
            carrier.production

/-- In the exact no-ahead branch, one finite aligned fresh window and its
adjacent-parent evidence produce an actual local next-index install at the
selected receiver. An intervening next-index install can end the prepared
attempt first.

Only this receiver's head must stay fixed while the already-derived range is
consumed. Commit installs at other validators do not affect this theorem. -/
theorem no_ahead_aligned_fresh_window_records_exact_local_next_or_intervening_install
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq BlockId]
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (pendingSource : ValidatorFlexPendingRefreshSourceMap
      source runtime initial schedule)
    (current : ValidatorCurrentPreparedFlexSourceMap pendingSource)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    (leaderDepth : Nat)
    {start observation windowBase : Time}
    {prior : ValidatorCommitHead CommitId}
    (priorInstalled : AllCorrectAvailableInstalledExactAt faults
      timed.execution.trace start prior)
    (noAhead : ¬∃ validator,
      validator < config.authorityCount ∧
        faults.correctAvailable validator = true ∧
        prior.index + 1 ≤
          (timed.execution.trace start).localCommitIndex validator)
    {receiver : Nat}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (noReceiverAdvance : ¬ValidatorReceiverCommitAdvance timed start receiver)
    (depthAtStart :
      (context receiver
        ((timed.execution.trace start).validatorState receiver)).depth =
          leaderDepth)
    (startBeforeObservation : start ≤ observation)
    (baseAfterPrior : prior.round < windowBase + 2)
    (aligned : ValidatorAlignedFavorableFreshWindow timed obligations waits
      prior observation windowBase (leaderDepth + 1))
    (adjacentEvidence :
      ValidatorAlignedFavorableAdjacentParentEvidence aligned receiver) :
    (∃ finish witnessId,
      start ≤ finish ∧
        ((timed.execution.trace finish).validatorState
          receiver).installedCommitAt (prior.index + 1) = some witnessId) ∨
      ∃ (run : CorrectExactFlexRun runtime) (installedAt : Time),
        start ≤ run.observation.time ∧
          run.observation.validator = receiver ∧
          run.prior = prior ∧
          run.output.reference.index = prior.index + 1 ∧
          run.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              receiver).installedCommitAt run.output.reference.index =
            some run.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              receiver).commitInstallSourceAt run.output.reference.index =
            some .localExecution := by
  rcases fresh_window_and_adjacent_evidence_give_receiver_direct_vote_range
      aligned.window (by omega) (by omega) aligned.leaderAuthorAt
        aligned.leaderInRange aligned.leaderCorrect receiverInRange
          receiverCorrect adjacentEvidence with
    ⟨rangeAt, observationBeforeRange, rangeRaw⟩
  let range := Classical.choice rangeRaw
  have startBeforeRange : start ≤ rangeAt :=
    Nat.le_trans startBeforeObservation observationBeforeRange
  have headsAtStart : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior := by
    rcases all_correct_available_installed_prior_gives_collective_head_split
        prefixMap priorInstalled with ahead | equal
    · exact False.elim (noAhead ahead)
    · exact equal
  have headAtRange :
      ((timed.execution.trace rangeAt).validatorState
        receiver).commitHead = prior :=
    (receiver_head_stays_fixed_without_local_commit_advance receiverInRange
      startBeforeRange noReceiverAdvance).trans
        (headsAtStart receiver receiverInRange receiverCorrect)
  have countMatchesDepth :
      leaderDepth + 1 =
        (context receiver
          ((timed.execution.trace rangeAt).validatorState receiver)).depth + 1 := by
    have depthAtRange :
        (context receiver
          ((timed.execution.trace rangeAt).validatorState receiver)).depth =
            leaderDepth :=
      (source.contextDepthStable receiver
        ((timed.execution.trace rangeAt).validatorState receiver) receiver
          ((timed.execution.trace start).validatorState receiver)).trans
            depthAtStart
    rw [depthAtRange]
  have leaderRound : ∀ offset,
      offset < (context receiver
        ((timed.execution.trace rangeAt).validatorState receiver)).depth + 1 →
      (range.leaderAt offset).round = windowBase + 2 + offset := by
    intro offset offsetInRange
    exact range.leaderRound offset (by
      rw [countMatchesDepth]
      exact offsetInRange)
  have firstSelected : ∀ offset,
      offset < (context receiver
        ((timed.execution.trace rangeAt).validatorState receiver)).depth + 1 →
      (config.selectedLeaderOrder prior.id (windowBase + 2 + offset)).head? =
        some (range.leaderAt offset).author := by
    intro offset offsetInRange
    have offsetInCount : offset < leaderDepth + 1 := by
      rw [countMatchesDepth]
      exact offsetInRange
    rw [range.leaderAuthor offset offsetInCount]
    exact aligned.firstSelected offset offsetInCount
  have leaderAccepted : ∀ offset,
      offset < (context receiver
        ((timed.execution.trace rangeAt).validatorState receiver)).depth + 1 →
      ((timed.execution.trace rangeAt).validatorState
        receiver).accepted (range.leaderAt offset) = true := by
    intro offset offsetInRange
    exact (range.acceptedVoteEvidence offset (by
      rw [countMatchesDepth]
      exact offsetInRange)).1
  have directQuorum : ∀ offset,
      offset < (context receiver
        ((timed.execution.trace rangeAt).validatorState receiver)).depth + 1 →
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters
            (timed.execution.trace rangeAt) receiver
              (range.leaderAt offset)) := by
    intro offset offsetInRange
    exact receiver_accepted_direct_vote_quorum_gives_trace_quorum
      (range.acceptedVoteEvidence offset (by
        rw [countMatchesDepth]
        exact offsetInRange)).2
  rcases prepared_direct_quorum_range_records_exact_or_installed_next
      pendingSource current prefixMap direct
        range.leaderAt receiverInRange receiverCorrect headAtRange
          baseAfterPrior leaderRound firstSelected leaderAccepted directQuorum with
    installed | localResult
  · rcases installed with ⟨finish, witnessId, rangeBeforeFinish, witness⟩
    exact Or.inl ⟨finish, witnessId,
      Nat.le_trans startBeforeRange rangeBeforeFinish, witness⟩
  · rcases localResult with
      ⟨run, installedAt, rangeBeforeRun, runReceiver, runPrior, runNext,
        runBeforeInstall, installed, localSource⟩
    exact Or.inr ⟨run, installedAt,
      Nat.le_trans startBeforeRange rangeBeforeRun, runReceiver, runPrior,
        runNext, runBeforeInstall, installed, localSource⟩

/-- Schedule-independent timing data for one actual adjacent fresh-production
edge.

The prior-round wait is retained through the actual successor lower edge. The
remaining margin pays only for the pairwise timer spread, the fixed proposal
pipeline, and this receiver's actual causal backlog. The record does not name a
quadratic or cubic wait law. -/
structure ValidatorFreshAdjacentLinearBacklogTimingEvidence
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
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {prior : ValidatorCommitHead CommitId}
    {observation author receiver round : Nat}
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations waits
      observation receiver (round + 1)) : Type where
  spread : Nat
  floor : Nat
  previousSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations waits
    observation round spread
  successorLower : ValidatorFreshTimerStartSuccessorLowerAt timed obligations
    waits observation round (waits.wait prior round)
  sourceFor : ∀ broadcast : ValidatorTimerPacedPeerBroadcast timed
    previous.production.snapshot author receiver
      previous.production.proposalActionAt,
    ValidatorTimerPacedLinearBacklogSyncSource admission previous.production
      broadcast (floor := floor)
  visibilityMargin :
    spread + 3 * (timed.localActionBound + 1) + network.delta +
        (1 + ((round - floor) * maxAdmittedRefsPerRound) *
            validatorBlockSyncAcceptanceBound timed syncRules +
          timed.localActionBound + 1) ≤
      waits.wait prior (round + 1)

/-- Finite timing and receiver-sync evidence for the two adjacent edges at
each offset of one already-derived aligned favorable window.

This record is tied to concrete productions which already occur in that
window. It is an internal composition boundary, not an end-to-end input. -/
structure ValidatorAlignedFavorableAdjacentTimingEvidence
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
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {prior : ValidatorCommitHead CommitId}
    {observation windowBase leaderCount : Nat}
    (aligned : ValidatorAlignedFavorableFreshWindow timed obligations waits
      prior observation windowBase leaderCount)
    (receiver : Nat) : Type where
  edgeEvidence : ∀ offset,
    offset < leaderCount →
    ∀
      (leader : ValidatorFreshTimerPacedExactRoundProduction timed obligations
        waits observation (aligned.leaderAuthorAt offset)
          (windowBase + offset + 2))
      (carrier : ValidatorFreshTimerPacedExactRoundProduction timed obligations
        waits observation receiver ((windowBase + offset + 2) + 2))
      voter,
      voter < config.authorityCount →
      faults.correctAvailable voter = true →
      ∀ vote : ValidatorFreshTimerPacedExactRoundProduction timed obligations
        waits observation voter ((windowBase + offset + 2) + 1),
      ValidatorFreshAdjacentLinearBacklogTimingEvidence admission
          (prior := prior) leader vote ×
        ValidatorFreshAdjacentLinearBacklogTimingEvidence admission
          (prior := prior) vote carrier

/-- A finite aligned window and its derived lower-edge timing record produce
the exact next local commit result, or an intervening install ends the attempt.

The proof converts each concrete timing edge to exact parent evidence. It then
calls the deterministic no-ahead composer above. -/
theorem no_ahead_aligned_fresh_window_timing_records_exact_local_next_or_intervening_install
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq BlockId]
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (pendingSource : ValidatorFlexPendingRefreshSourceMap
      source runtime initial schedule)
    (current : ValidatorCurrentPreparedFlexSourceMap pendingSource)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    (leaderDepth : Nat)
    {start observation windowBase : Time}
    {prior : ValidatorCommitHead CommitId}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (priorInstalled : AllCorrectAvailableInstalledExactAt faults
      timed.execution.trace start prior)
    (noAhead : ¬∃ validator,
      validator < config.authorityCount ∧
        faults.correctAvailable validator = true ∧
        prior.index + 1 ≤
          (timed.execution.trace start).localCommitIndex validator)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start)
    {receiver : Nat}
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (depthAtStart :
      (context receiver
        ((timed.execution.trace start).validatorState receiver)).depth =
          leaderDepth)
    (startBeforeObservation : start ≤ observation)
    (baseAfterPrior : prior.round < windowBase + 2)
    (aligned : ValidatorAlignedFavorableFreshWindow timed obligations waits
      prior observation windowBase (leaderDepth + 1))
    (timing : ValidatorAlignedFavorableAdjacentTimingEvidence admission aligned
      receiver) :
    (∃ finish witnessId,
      start ≤ finish ∧
        ((timed.execution.trace finish).validatorState
          receiver).installedCommitAt (prior.index + 1) = some witnessId) ∨
      ∃ (run : CorrectExactFlexRun runtime) (installedAt : Time),
        start ≤ run.observation.time ∧
          run.observation.validator = receiver ∧
          run.prior = prior ∧
          run.output.reference.index = prior.index + 1 ∧
          run.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              receiver).installedCommitAt run.output.reference.index =
            some run.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              receiver).commitInstallSourceAt run.output.reference.index =
            some .localExecution := by
  have headsAtStart : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior := by
    rcases all_correct_available_installed_prior_gives_collective_head_split
        prefixMap priorInstalled with ahead | equal
    · exact False.elim (noAhead ahead)
    · exact equal
  have adjacentEvidence :
      ValidatorAlignedFavorableAdjacentParentEvidence aligned receiver := by
    intro offset offsetInRange leader carrier voter voterInRange voterCorrect
      vote
    rcases timing.edgeEvidence offset offsetInRange leader carrier voter
        voterInRange voterCorrect vote with
      ⟨leaderTiming, carrierTiming⟩
    constructor
    · exact
        same_head_fresh_adjacent_productions_give_parent_evidence_from_spread_lower_and_linear_backlog
          originRules acceptance sync representatives admission headsAtStart
            startBeforeObservation afterGst active noAdvance leader vote
              leaderTiming.sourceFor (by omega) leaderTiming.previousSpread
                leaderTiming.successorLower
                leaderTiming.visibilityMargin
    · exact
        same_head_fresh_adjacent_productions_give_parent_evidence_from_spread_lower_and_linear_backlog
          originRules acceptance sync representatives admission headsAtStart
            startBeforeObservation afterGst active noAdvance vote carrier
              carrierTiming.sourceFor (by omega) carrierTiming.previousSpread
                carrierTiming.successorLower
                carrierTiming.visibilityMargin
  exact
    no_ahead_aligned_fresh_window_records_exact_local_next_or_intervening_install
      pendingSource current prefixMap direct leaderDepth priorInstalled noAhead
        receiverInRange receiverCorrect (by
          intro receiverAdvance
          rcases receiverAdvance with
            ⟨finish, startBeforeFinish, indexAdvance⟩
          exact noAdvance ⟨receiver, finish, receiverInRange,
            receiverCorrect, startBeforeFinish, indexAdvance⟩) depthAtStart
          startBeforeObservation baseAfterPrior aligned adjacentEvidence

end Mysticeti
