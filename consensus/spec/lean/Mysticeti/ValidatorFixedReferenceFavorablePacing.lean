/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.EndToEndProbabilityCapstone
import Mysticeti.ValidatorHeadRelativeGapWait
import Mysticeti.ValidatorLinearBacklogTiming
import Mysticeti.ValidatorNoAheadOneIndexStep

namespace Mysticeti

/-!
Fixed-reference recovery pacing for validators with different commit heads.

The local commit head still keys the recovery timer. The wait value does not
use that head. It uses one epoch-local reference round which does not change
when a validator installs a commit. Thus, two validators at different commit
heads use the same wait value for one proposal round.

This file proves the smallest timing result needed by the ahead branch. A late
correct first leader reaches every correct next-round proposal snapshot. The
next edge reaches one fixed receiver's round-plus-two proposal. Repeating this
for `depth + 1` favorable rounds gives one usable local direct range.

The trace source map below contains only rules about actual timer starts,
actual packets, and current parent-sync work. It does not contain a future
favorable window, block, FlexCommitter run, install, or commit.

The local implementation rule is a recovery-mode not-before gate. For proposal
round `R`, an allowed-leader notification cannot take the final parent snapshot
before `timerStartedAt + W(R)`. At that deadline, the existing forced proposal
path can run. `W(R)` uses an epoch-fixed reference round and does not read the
local commit head. A max forced timeout without this not-before gate is not
sufficient, because the normal allowed-leader path can otherwise snapshot
early under a different local schedule.
-/

/-- A wait schedule is head independent when one absolute proposal round has
one wait value for every local commit head. -/
def ValidatorHeadIndependentRoundWait
    {CommitId : Type}
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)) : Prop :=
  ∀ left right round, waits.wait left round = waits.wait right round

/-- The fixed-reference quadratic schedule is head independent. -/
theorem fixed_reference_quadratic_wait_is_head_independent
    {CommitId : Type}
    (parameters : ValidatorQuadraticGapWaitParameters) :
    ValidatorHeadIndependentRoundWait
      (parameters.commonSchedule (ValidatorCommitHead CommitId)) := by
  intro left right round
  rfl

/-- Express a fixed-reference wait through the existing head-relative
arithmetic interface. The head value is only an arithmetic witness. -/
private def ValidatorQuadraticGapWaitParameters.asHeadRelative
    (parameters : ValidatorQuadraticGapWaitParameters) :
    ValidatorHeadRelativeQuadraticWaitParameters where
  baseWait := parameters.baseWait
  linearCoefficient := parameters.linearCoefficient
  quadraticCoefficient := parameters.quadraticCoefficient
  quadraticPositive := parameters.quadraticPositive

/-- Replace only the round of one head to obtain an arithmetic witness for the
fixed reference. -/
private def ValidatorQuadraticGapWaitParameters.referenceHead
    {CommitId : Type}
    (parameters : ValidatorQuadraticGapWaitParameters)
    (witness : ValidatorCommitHead CommitId) : ValidatorCommitHead CommitId :=
  { witness with round := parameters.referenceRound }

@[simp]
private theorem ValidatorQuadraticGapWaitParameters.reference_head_wait_eq
    {CommitId : Type}
    (parameters : ValidatorQuadraticGapWaitParameters)
    (witness : ValidatorCommitHead CommitId)
    (round : Nat) :
    parameters.asHeadRelative.wait (parameters.referenceHead witness) round =
      parameters.wait round := by
  rfl

/-- One fixed-reference quadratic wait eventually covers a quadratic timer
spread, a linear causal backlog above round zero, and fixed pipeline work. -/
theorem fixed_reference_quadratic_value_eventually_covers_visibility
    {CommitId : Type}
    (parameters : ValidatorQuadraticGapWaitParameters)
    (witness : ValidatorCommitHead CommitId)
    (spreadBase spreadLinear spreadQuadratic backlogSlope fixedPipelineCost :
      Nat)
    (quadraticCondition :
      spreadQuadratic < parameters.quadraticCoefficient) :
    ∃ firstRound,
      parameters.referenceRound ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            (spreadBase + spreadLinear * parameters.gap round +
                spreadQuadratic * parameters.gap round *
                  parameters.gap round) +
                (round * backlogSlope + fixedPipelineCost) ≤
              parameters.wait (round + 1) := by
  let headParameters := parameters.asHeadRelative
  let referenceHead := parameters.referenceHead witness
  rcases
      head_relative_quadratic_value_eventually_covers_quadratic_spread_and_cutoff_backlog
        headParameters referenceHead 0 spreadBase spreadLinear
          spreadQuadratic backlogSlope fixedPipelineCost quadraticCondition with
    ⟨firstRound, referenceBeforeFirst, covered⟩
  refine ⟨firstRound, ?_, ?_⟩
  · simpa [referenceHead, ValidatorQuadraticGapWaitParameters.referenceHead]
      using referenceBeforeFirst
  · intro round firstBeforeRound
    have result := covered round firstBeforeRound
    simpa [headParameters, referenceHead,
      ValidatorQuadraticGapWaitParameters.asHeadRelative,
      ValidatorQuadraticGapWaitParameters.referenceHead,
      ValidatorQuadraticGapWaitParameters.gap,
      ValidatorQuadraticGapWaitParameters.wait,
      ValidatorHeadRelativeQuadraticWaitParameters.wait] using result

/-- A numeric threshold selected before any favorable window is selected. -/
structure ValidatorFixedReferenceVisibilityThreshold
    (parameters : ValidatorQuadraticGapWaitParameters)
    (spreadBase spreadLinear spreadQuadratic backlogSlope fixedPipelineCost :
      Nat) : Type where
  firstRound : Nat
  afterReference : parameters.referenceRound ≤ firstRound
  covers : ∀ round,
    firstRound ≤ round →
      (spreadBase + spreadLinear * parameters.gap round +
          spreadQuadratic * parameters.gap round * parameters.gap round) +
          (round * backlogSlope + fixedPipelineCost) ≤
        parameters.wait (round + 1)

/-- Build the fixed-reference visibility threshold from static coefficients. -/
theorem fixed_reference_visibility_threshold
    {CommitId : Type}
    (parameters : ValidatorQuadraticGapWaitParameters)
    (witness : ValidatorCommitHead CommitId)
    (spreadBase spreadLinear spreadQuadratic backlogSlope fixedPipelineCost :
      Nat)
    (quadraticCondition :
      spreadQuadratic < parameters.quadraticCoefficient) :
    Nonempty (ValidatorFixedReferenceVisibilityThreshold parameters spreadBase
      spreadLinear spreadQuadratic backlogSlope fixedPipelineCost) := by
  rcases fixed_reference_quadratic_value_eventually_covers_visibility
      parameters witness spreadBase spreadLinear spreadQuadratic backlogSlope
        fixedPipelineCost quadraticCondition with
    ⟨firstRound, afterReference, covers⟩
  exact ⟨{ firstRound, afterReference, covers }⟩


/-! ### Commit-orthogonal accepted-block retention -/

/-- Installing a commit does not remove one accepted block which remains above
the new local GC round. This is the narrow storage rule needed by the
head-independent proposal gate. A block at or below GC is allowed to disappear.

The rule does not preserve a fetch, pin, timer, or proposal across a commit. It
only describes the current accepted-block store. -/
structure ValidatorCommitOrthogonalAcceptedRetentionRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) : Type where
  acceptedAboveGcIsRetained : ∀ time validator reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((timed.execution.trace time).validatorState validator).accepted reference =
      true →
    ((timed.execution.trace time).validatorState validator).gcRound <
      reference.round →
    ((timed.execution.trace time).validatorState validator).retained reference =
      true

/-- An actual positive-round successor proposal proves its own action-local GC
fence. Any parent in its nonempty legal parent list is at the previous round
and above the snapshot GC boundary. -/
theorem timer_paced_successor_snapshot_gc_below_previous_round
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
    {receiver round : Nat}
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (roundPositive : 0 < round) :
    ((timed.execution.trace next.snapshot.snapshotAt).validatorState
      receiver).gcRound < round := by
  have parentsNonempty : next.snapshot.block.parents ≠ [] :=
    validator_parent_list_ready_nonempty next.refreshedParentList.ready.1
  rcases List.exists_mem_of_ne_nil _ parentsNonempty with ⟨parent, parentMember⟩
  have parentRound :=
    next.refreshedParentList.ready.1.2.1 parent parentMember |>.1
  have parentGc := next.refreshedParentList.ready.2 parent parentMember |>.2
  rcases parentGc with parentGenesis | parentAboveGc
  · omega
  · omega


/-! ### One heterogeneous-head adjacent edge -/

/-- One adjacent edge from action-local facts only.

The two productions can use different commit heads. Commits at the author,
the receiver, or another validator can occur between the two actions. The
successor snapshot proves its own GC fence. The commit-orthogonal retention
rule keeps the accepted previous block when that fence still needs it. -/
theorem adjacent_timer_paced_productions_give_parent_evidence_commit_orthogonal
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
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    {commonStart author receiver round floor startDifference : Nat}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (sourceFor : ∀ broadcast : ValidatorTimerPacedPeerBroadcast timed
      previous.snapshot author receiver previous.proposalActionAt,
        ValidatorTimerPacedLinearBacklogSyncSource admission previous broadcast
          (floor := floor))
    (commonStartBeforePrevious : commonStart ≤ previous.timerStartedAt)
    (commonStartBeforeNext : commonStart ≤ next.timerStartedAt)
    (roundPositive : 0 < round)
    (previousDeadlineBound :
      previous.timerStartedAt + waits.wait previous.commitHead round ≤
        next.timerStartedAt + startDifference)
    (visibilityMargin :
      startDifference + 3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((round - floor) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        waits.wait next.commitHead (round + 1))
    (previousStartsAfterGst : network.gst ≤ previous.timerStartedAt)
    (active : ∀ time, commonStart ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorAdjacentTimerPacedParentEvidence previous next := by
  have authorInRange : author < config.authorityCount := by
    simpa [previous.proposer] using previous.snapshot.proposerInRange
  have authorCorrect : faults.correctAvailable author = true := by
    simpa [previous.proposer] using
      previous.snapshot.proposerCorrectAvailable
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.proposer] using next.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.proposer] using next.snapshot.proposerCorrectAvailable
  have nextStartBeforeSnapshot : next.timerStartedAt ≤
      next.snapshot.snapshotAt := by
    rw [next.snapshotAtDeadline]
    exact Nat.le_add_right _ _
  have commonStartBeforeNextSnapshot : commonStart ≤
      next.snapshot.snapshotAt :=
    Nat.le_trans commonStartBeforeNext nextStartBeforeSnapshot
  have aboveGcAtNext :
      ((timed.execution.trace next.snapshot.snapshotAt).validatorState
        receiver).gcRound < previous.snapshot.block.reference.round := by
    rw [previous.blockRound]
    exact timer_paced_successor_snapshot_gc_below_previous_round next
      roundPositive
  by_cases sameAuthor : author = receiver
  · subst author
    have persistenceBeforeNextStart :=
      next_recovery_round_timer_starts_after_previous_persistence previous next
    have storedBeforeNextSnapshot : previous.snapshot.storedAt ≤
        next.snapshot.snapshotAt := by
      rw [previous.storedAfterPersistence]
      exact Nat.le_trans persistenceBeforeNextStart nextStartBeforeSnapshot
    have ownAtNext :=
      (timed.execution.durableStateMonotone receiver
        previous.snapshot.storedAt next.snapshot.snapshotAt receiverInRange
        storedBeforeNextSnapshot).own_block_persists
          (by simpa [previous.proposer] using previous.snapshot.blockStored)
    have ownFacts :=
      (timed.execution.statesWellFormed next.snapshot.snapshotAt receiver
        receiverInRange).ownBlockIsSound round
          previous.snapshot.block.reference (by
            simpa [previous.blockRound] using ownAtNext)
    exact
      ValidatorCausalCapsuleCatchupRateRules.accepted_retained_timer_paced_block_gives_parent_evidence
        representatives previous next ownFacts.2.2.1 ownFacts.2.2.2.1
  · let broadcast := Classical.choice
        (previous.peerBroadcast receiver receiverInRange (by
          intro receiverIsAuthor
          exact sameAuthor receiverIsAuthor.symm))
    have sentAfterGst : network.gst ≤ broadcast.packet.sentAt := by
      exact Nat.le_trans previousStartsAfterGst
        (Nat.le_trans (Nat.le_add_right _ _)
          (Nat.le_trans previous.deadlineBeforeProposal
            (Nat.le_trans (Nat.le_add_right _ 1)
              broadcast.proposalBeforeSend)))
    have deliveryFacts := timer_paced_peer_broadcast_is_delivered previous
      broadcast receiverInRange receiverCorrect sentAfterGst
    have commonStartBeforeDelivery : commonStart ≤
        broadcast.packet.deliveredAt := by
      exact Nat.le_trans commonStartBeforePrevious
        (Nat.le_trans (Nat.le_add_right _ _)
          (Nat.le_trans previous.deadlineBeforeProposal
            (Nat.le_trans (Nat.le_add_right _ 1)
              (Nat.le_trans broadcast.proposalBeforeSend deliveryFacts.1))))
    have activeFromDelivery : ∀ time,
        broadcast.packet.deliveredAt + 1 ≤ time →
          (timed.execution.trace time).epochActive = true := by
      intro time deliveryBeforeTime
      exact active time (Nat.le_trans commonStartBeforeDelivery
        (Nat.le_trans (Nat.le_add_right _ 1) deliveryBeforeTime))
    let resolutionCost :=
      1 + ((round - floor) * maxAdmittedRefsPerRound) *
          validatorBlockSyncAcceptanceBound timed syncRules +
        timed.localActionBound + 1
    have resolutionBeforeNextSnapshot :
        broadcast.packet.deliveredAt + resolutionCost ≤
          next.snapshot.snapshotAt := by
      exact adjacent_lower_edge_margin_covers_timer_paced_resolution_cost
        previous next broadcast receiverInRange receiverCorrect sentAfterGst
          rfl rfl previousDeadlineBound (by
            simpa only [resolutionCost] using visibilityMargin)
    rcases timer_paced_peer_broadcast_resolves_within_linear_backlog_or_gc
        admission acceptance previous broadcast (sourceFor broadcast)
          receiverInRange receiverCorrect sentAfterGst activeFromDelivery with
      ⟨acceptedAt, _deliveryBeforeAccepted, acceptedBound, acceptedOrRoot⟩
    have acceptedBeforeNextSnapshot : acceptedAt ≤
        next.snapshot.snapshotAt := by
      exact Nat.le_trans acceptedBound (by
        simpa only [resolutionCost, Nat.add_assoc] using
          resolutionBeforeNextSnapshot)
    have acceptedAtNext :
        ((timed.execution.trace next.snapshot.snapshotAt).validatorState
          receiver).accepted previous.snapshot.block.reference = true := by
      rcases acceptedOrRoot with accepted | atRoot
      · exact timed.execution.accepted_block_persists receiverInRange
          acceptedBeforeNextSnapshot accepted
      · have gcMonotone :=
          ValidatorRecoveryCapsuleSyncExecution.validator_gc_round_mono
            (timed := timed) receiverInRange acceptedBeforeNextSnapshot
        omega
    have retainedAtNext :
        ((timed.execution.trace next.snapshot.snapshotAt).validatorState
          receiver).retained previous.snapshot.block.reference = true :=
      retention.acceptedAboveGcIsRetained next.snapshot.snapshotAt receiver
        previous.snapshot.block.reference receiverInRange receiverCorrect
          acceptedAtNext aboveGcAtNext
    exact
      ValidatorCausalCapsuleCatchupRateRules.accepted_retained_timer_paced_block_gives_parent_evidence
        representatives previous next acceptedAtNext retainedAtNext

/-! ### A complete favorable direct range -/

/-- A favorable finite family with only the past proposal values used by the
commit-orthogonal direct-range proof. It does not contain the old globally
stable common-round source. -/
structure ValidatorFixedReferenceAlignedFavorableFamily
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
    (prior : ValidatorCommitHead CommitId)
    (observation windowBase leaderCount : Nat) where
  family : ValidatorFreshTimerPacedExactRoundFamily timed obligations waits
    observation windowBase (leaderCount + 2)
  leaderAuthorAt : Nat → Nat
  leaderInRange : ∀ offset,
    offset < leaderCount →
      leaderAuthorAt offset < config.authorityCount
  leaderCorrect : ∀ offset,
    offset < leaderCount →
      faults.correctAvailable (leaderAuthorAt offset) = true
  firstSelected : ∀ offset,
    offset < leaderCount →
      (config.selectedLeaderOrder prior.id
        (windowBase + 2 + offset)).head? = some (leaderAuthorAt offset)

/-- The two action-local parent edges for one minimal favorable family. -/
def ValidatorFixedReferenceAlignedAdjacentParentEvidence
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
    (aligned : ValidatorFixedReferenceAlignedFavorableFamily timed obligations
      waits prior observation windowBase leaderCount)
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

/-- The usable direct range produced by fixed-reference pacing. All fields are
actual trace results. -/
structure ValidatorFixedReferenceFavorableDirectRange
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start receiver leaderCount : Nat)
    (prior : ValidatorCommitHead CommitId) where
  observation : Time
  windowBase : Nat
  finish : Time
  leaderAuthorAt : Nat → Nat
  startBeforeObservation : start ≤ observation
  observationBeforeFinish : observation ≤ finish
  headAtStart :
    ((timed.execution.trace start).validatorState receiver).commitHead = prior
  baseAfterPrior : prior.round < windowBase + 2
  firstSelected : ∀ offset,
    offset < leaderCount →
      (config.selectedLeaderOrder prior.id
        (windowBase + 2 + offset)).head? = some (leaderAuthorAt offset)
  range : ValidatorReceiverAcceptedDirectVoteRange config
    (timed.execution.trace finish) receiver (windowBase + 2) leaderCount
      leaderAuthorAt

/-- An actual aligned window and its action-local adjacent edges give the
receiver range. A commit at any validator can occur during the window. If the
receiver installs first, the later Flex adapter reports that install as the
winning branch. -/
theorem aligned_family_parent_evidence_gives_fixed_reference_direct_range
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
    {start observation windowBase leaderCount receiver : Nat}
    {prior : ValidatorCommitHead CommitId}
    (startBeforeObservation : start ≤ observation)
    (headAtStart :
      ((timed.execution.trace start).validatorState receiver).commitHead = prior)
    (baseAfterPrior : prior.round < windowBase + 2)
    (aligned : ValidatorFixedReferenceAlignedFavorableFamily timed obligations waits
      prior observation windowBase leaderCount)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (leaderCountPositive : 0 < leaderCount)
    (adjacentEvidence :
      ValidatorFixedReferenceAlignedAdjacentParentEvidence aligned receiver) :
    Nonempty (ValidatorFixedReferenceFavorableDirectRange timed start receiver
      leaderCount prior) := by
  rcases fresh_family_and_adjacent_evidence_give_receiver_direct_vote_range
      aligned.family leaderCountPositive (by omega) aligned.leaderAuthorAt
        aligned.leaderInRange aligned.leaderCorrect receiverInRange
          receiverCorrect adjacentEvidence with
    ⟨finish, observationBeforeFinish, rangeRaw⟩
  exact ⟨{
    observation
    windowBase
    finish
    leaderAuthorAt := aligned.leaderAuthorAt
    startBeforeObservation
    observationBeforeFinish
    headAtStart
    baseAfterPrior
    firstSelected := aligned.firstSelected
    range := Classical.choice rangeRaw }⟩

/-- The fixed-reference range has the exact shape consumed by the protected
local FlexCommitter path. It records either an intervening next-index install
or a successful local run and install. -/
theorem fixed_reference_direct_range_records_local_commit_or_installed_next
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    {start receiver : Nat}
    {prior : ValidatorCommitHead CommitId}
    (directRange : ValidatorFixedReferenceFavorableDirectRange
      inputs.timedExecution start receiver
        (inputs.leaderSchedule.indirectDepth + 1) prior)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true) :
    (∃ completedAt witnessId,
      start ≤ completedAt ∧
        ((inputs.timedExecution.execution.trace completedAt).validatorState
          receiver).installedCommitAt (prior.index + 1) = some witnessId) ∨
      ∃ (run : CorrectExactFlexRun inputs.flexCommitterRuntime)
          (installedAt : Time),
        start ≤ run.observation.time ∧
          run.observation.validator = receiver ∧
          run.prior = prior ∧
          run.observation.time < installedAt ∧
          ((inputs.timedExecution.execution.trace installedAt).validatorState
              receiver).installedCommitAt run.output.reference.index =
            some run.output.reference.digest ∧
          ((inputs.timedExecution.execution.trace installedAt).validatorState
              receiver).commitInstallSourceAt run.output.reference.index =
            some .localExecution := by
  apply receiver_direct_vote_range_records_local_commit_or_installed_next
    inputs.flexCommitterSource inputs.flexCommitterRuntime inputs.commitPrefix
      inputs.exactPendingIngestion inputs.exactDirectRule
        inputs.successfulFlexScanWork directRange.range
  · simpa only [inputs.flex_committer_depth_matches_leader_schedule]
  · exact Nat.le_trans directRange.startBeforeObservation
      directRange.observationBeforeFinish
  · exact receiverInRange
  · exact receiverCorrect
  · exact directRange.headAtStart
  · exact directRange.baseAfterPrior
  · exact directRange.firstSelected


end Mysticeti
