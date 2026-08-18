/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.EndToEndProbabilityCapstone
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

/-- Current and past trace rules for fixed-reference pacing.

`spreadAt` and `lowerAt` concern only productions which already occur in the
trace. `sourceFor` maps one actual delivered proposal to its protected,
GC-aware parent-sync work. A local implementation can establish these fields
from timer-arm accounting and the post-GST causal-service rule. -/
structure ValidatorFixedReferencePacingTraceSourceMap
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (parameters : ValidatorQuadraticGapWaitParameters) : Type where
  sourceStartRound : Nat
  spreadBase : Nat
  spreadLinear : Nat
  spreadQuadratic : Nat
  spreadQuadraticBelowWait :
    spreadQuadratic < parameters.quadraticCoefficient
  waitValue : ∀ head round,
    waits.wait head round = parameters.wait round
  spreadAt : ∀ {observation round},
    sourceStartRound ≤ round →
      ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
        round
          (spreadBase + spreadLinear * parameters.gap round +
            spreadQuadratic * parameters.gap round * parameters.gap round)
  lowerAt : ∀ {observation round},
    sourceStartRound ≤ round →
      ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
        observation round (parameters.wait round)
  sourceFor : ∀ {observation author receiver round}
    (_roundAfterSource : sourceStartRound ≤ round)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed
      previous.production.snapshot author receiver
        previous.production.proposalActionAt),
    ValidatorTimerPacedLinearBacklogSyncSource admission previous.production
      broadcast (floor := 0)

namespace ValidatorFixedReferencePacingTraceSourceMap

/-- The source map states the required head-independent wait rule. -/
theorem headIndependent
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
    {admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound}
    {parameters : ValidatorQuadraticGapWaitParameters}
    (source : ValidatorFixedReferencePacingTraceSourceMap timed obligations
      waits admission parameters) :
    ValidatorHeadIndependentRoundWait waits := by
  intro left right round
  rw [source.waitValue, source.waitValue]

/-- Every actual fresh recovery proposal uses the fixed round deadline. Thus,
head-specific allowed-leader readiness cannot move its parent snapshot before
the common not-before gate. -/
theorem snapshotAtFixedDeadline
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
    {admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound}
    {parameters : ValidatorQuadraticGapWaitParameters}
    (source : ValidatorFixedReferencePacingTraceSourceMap timed obligations
      waits admission parameters)
    {observation validator round : Nat}
    (production : ValidatorFreshTimerPacedExactRoundProduction timed
      obligations waits observation validator round) :
    production.production.snapshot.snapshotAt =
      production.production.timerStartedAt + parameters.wait round := by
  rw [production.production.snapshotAtDeadline, source.waitValue]

end ValidatorFixedReferencePacingTraceSourceMap

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

/-! ### Current-head bounds for one selected late window -/

/-- The proof-only maximum current commit-head round of the correct, available
validators. Validators do not compute or exchange this value. -/
def correctAvailableCommitHeadRoundMaximumUpTo
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId) : Nat → Nat
  | 0 => 0
  | count + 1 =>
      max (correctAvailableCommitHeadRoundMaximumUpTo faults world count)
        (if faults.correctAvailable count then
          (world.validatorState count).commitHead.round
        else
          0)

/-- Each current correct, available commit-head round is at most the finite
proof-only maximum. -/
theorem correct_available_commit_head_round_le_maximum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator count : Nat}
    (validatorInRange : validator < count)
    (validatorCorrect : faults.correctAvailable validator = true) :
    (world.validatorState validator).commitHead.round ≤
      correctAvailableCommitHeadRoundMaximumUpTo faults world count := by
  induction count generalizing validator with
  | zero => omega
  | succ previous inductionHypothesis =>
      simp only [correctAvailableCommitHeadRoundMaximumUpTo]
      by_cases validatorIsLast : validator = previous
      · subst validator
        simpa [validatorCorrect] using
          (Nat.le_max_right
            (correctAvailableCommitHeadRoundMaximumUpTo faults world previous)
            (world.validatorState previous).commitHead.round)
      · exact Nat.le_trans
          (inductionHypothesis (by omega) validatorCorrect)
          (Nat.le_max_left _ _)

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

/-- Fixed-reference pacing gives one exact adjacent parent edge even when the
producer and receiver have different or changing commit heads. -/
theorem fixed_reference_fresh_adjacent_productions_give_parent_evidence
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
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (source : ValidatorFixedReferencePacingTraceSourceMap timed obligations
      waits admission parameters)
    {observation author receiver round : Nat}
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1))
    (roundAfterSource : source.sourceStartRound ≤ round)
    (roundPositive : 0 < round)
    (visibilityMargin :
      (source.spreadBase + source.spreadLinear * parameters.gap round +
          source.spreadQuadratic * parameters.gap round *
            parameters.gap round) +
          3 * (timed.localActionBound + 1) + network.delta +
          (1 + (round * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        parameters.wait (round + 1)) :
    ValidatorAdjacentTimerPacedParentEvidence previous.production
      next.production := by
  have fixedDeadlineBound :
      previous.production.timerStartedAt + parameters.wait round ≤
        next.production.timerStartedAt +
          (source.spreadBase + source.spreadLinear * parameters.gap round +
            source.spreadQuadratic * parameters.gap round *
              parameters.gap round) :=
    fresh_timer_start_spread_and_successor_lower_gives_deadline_bound
      (source.spreadAt roundAfterSource) (source.lowerAt roundAfterSource)
        previous next
  have previousDeadlineBound :
      previous.production.timerStartedAt +
          waits.wait previous.production.commitHead round ≤
        next.production.timerStartedAt +
          (source.spreadBase + source.spreadLinear * parameters.gap round +
            source.spreadQuadratic * parameters.gap round *
              parameters.gap round) := by
    rw [source.waitValue]
    exact fixedDeadlineBound
  have actualVisibilityMargin :
      (source.spreadBase + source.spreadLinear * parameters.gap round +
          source.spreadQuadratic * parameters.gap round *
            parameters.gap round) +
          3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((round - 0) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        waits.wait next.production.commitHead (round + 1) := by
    rw [source.waitValue]
    simpa using visibilityMargin
  have previousStartsAfterGst : network.gst ≤
      previous.production.timerStartedAt :=
    Nat.le_trans afterGst (Nat.le_of_lt previous.timerAfterObservation)
  exact
    adjacent_timer_paced_productions_give_parent_evidence_commit_orthogonal
      acceptance representatives admission retention previous.production
        next.production (fun broadcast =>
          source.sourceFor roundAfterSource previous broadcast)
        (Nat.le_of_lt previous.timerAfterObservation)
        (Nat.le_of_lt next.timerAfterObservation) roundPositive
        previousDeadlineBound actualVisibilityMargin previousStartsAfterGst
          active

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

/-- Forget only the old stable-common-round source record. -/
def ValidatorAlignedFavorableFreshWindow.toFixedReferenceFamily
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
      prior observation windowBase leaderCount) :
    ValidatorFixedReferenceAlignedFavorableFamily timed obligations waits prior
      observation windowBase leaderCount where
  family := aligned.window.freshAt
  leaderAuthorAt := aligned.leaderAuthorAt
  leaderInRange := aligned.leaderInRange
  leaderCorrect := aligned.leaderCorrect
  firstSelected := aligned.firstSelected

/-- One late aligned window gets both exact parent edges at every favorable
offset. Commit heads can differ across validators. -/
theorem fixed_reference_pacing_gives_aligned_family_parent_evidence
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
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (source : ValidatorFixedReferencePacingTraceSourceMap timed obligations
      waits admission parameters)
    (threshold : ValidatorFixedReferenceVisibilityThreshold parameters
      source.spreadBase source.spreadLinear source.spreadQuadratic
        (maxAdmittedRefsPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules)
        (3 * (timed.localActionBound + 1) + network.delta + 1 +
          timed.localActionBound + 1))
    {prior : ValidatorCommitHead CommitId}
    {observation windowBase leaderCount receiver : Nat}
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (sourceBeforeWindow : source.sourceStartRound ≤ windowBase + 2)
    (thresholdBeforeWindow : threshold.firstRound ≤ windowBase + 2)
    (aligned : ValidatorFixedReferenceAlignedFavorableFamily timed obligations waits
      prior observation windowBase leaderCount)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true) :
    ValidatorFixedReferenceAlignedAdjacentParentEvidence aligned receiver := by
  intro offset offsetInRange leader carrier voter voterInRange voterCorrect vote
  have leaderRoundAfterSource :
      source.sourceStartRound ≤ windowBase + offset + 2 := by
    omega
  have voteRoundAfterSource :
      source.sourceStartRound ≤ (windowBase + offset + 2) + 1 := by
    omega
  have leaderRoundAfterThreshold :
      threshold.firstRound ≤ windowBase + offset + 2 := by
    omega
  have voteRoundAfterThreshold :
      threshold.firstRound ≤ (windowBase + offset + 2) + 1 := by
    omega
  have leaderCovered := threshold.covers (windowBase + offset + 2)
    leaderRoundAfterThreshold
  have voteCovered := threshold.covers ((windowBase + offset + 2) + 1)
    voteRoundAfterThreshold
  have leaderVisibility :
      (source.spreadBase + source.spreadLinear *
          parameters.gap (windowBase + offset + 2) +
        source.spreadQuadratic * parameters.gap (windowBase + offset + 2) *
          parameters.gap (windowBase + offset + 2)) +
          3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((windowBase + offset + 2) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        parameters.wait ((windowBase + offset + 2) + 1) := by
    simpa [Nat.mul_assoc, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
      leaderCovered
  have voteVisibility :
      (source.spreadBase + source.spreadLinear *
          parameters.gap ((windowBase + offset + 2) + 1) +
        source.spreadQuadratic *
            parameters.gap ((windowBase + offset + 2) + 1) *
          parameters.gap ((windowBase + offset + 2) + 1)) +
          3 * (timed.localActionBound + 1) + network.delta +
          (1 + (((windowBase + offset + 2) + 1) *
                maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        parameters.wait (((windowBase + offset + 2) + 1) + 1) := by
    simpa [Nat.mul_assoc, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
      voteCovered
  constructor
  · exact fixed_reference_fresh_adjacent_productions_give_parent_evidence
      acceptance representatives admission retention parameters source afterGst
        active leader vote leaderRoundAfterSource (by omega) leaderVisibility
  · exact fixed_reference_fresh_adjacent_productions_give_parent_evidence
      acceptance representatives admission retention parameters source afterGst
        active vote carrier voteRoundAfterSource (by omega) voteVisibility

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

/-- Fixed-reference pacing turns one already-actual aligned family into a
usable direct range without any commit-silence premise. All commit-head and GC
facts used by the two edges come from the concrete proposal actions. -/
theorem fixed_reference_pacing_and_aligned_family_give_receiver_direct_range
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
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (source : ValidatorFixedReferencePacingTraceSourceMap timed obligations
      waits admission parameters)
    (threshold : ValidatorFixedReferenceVisibilityThreshold parameters
      source.spreadBase source.spreadLinear source.spreadQuadratic
        (maxAdmittedRefsPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules)
        (3 * (timed.localActionBound + 1) + network.delta + 1 +
          timed.localActionBound + 1))
    {start observation windowBase leaderCount receiver : Nat}
    {prior : ValidatorCommitHead CommitId}
    (startBeforeObservation : start ≤ observation)
    (headAtStart :
      ((timed.execution.trace start).validatorState receiver).commitHead = prior)
    (baseAfterPrior : prior.round < windowBase + 2)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (sourceBeforeWindow : source.sourceStartRound ≤ windowBase + 2)
    (thresholdBeforeWindow : threshold.firstRound ≤ windowBase + 2)
    (aligned : ValidatorFixedReferenceAlignedFavorableFamily timed obligations waits
      prior observation windowBase leaderCount)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (leaderCountPositive : 0 < leaderCount) :
    Nonempty (ValidatorFixedReferenceFavorableDirectRange timed start receiver
      leaderCount prior) := by
  have adjacentEvidence := fixed_reference_pacing_gives_aligned_family_parent_evidence
    acceptance representatives admission retention parameters source threshold
      afterGst active sourceBeforeWindow thresholdBeforeWindow aligned
        receiverInRange receiverCorrect
  exact aligned_family_parent_evidence_gives_fixed_reference_direct_range
    startBeforeObservation headAtStart baseAfterPrior aligned receiverInRange
      receiverCorrect leaderCountPositive adjacentEvidence

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

/-- A fixed-reference pacing source and one law-derived favorable path produce
an actual `depth + 1` direct range at a receiver with an arbitrary current
commit head.

The favorable path is a tail property of the ideal ranking law. It is queried
only after the numeric threshold and the current-head maximum are fixed. -/
theorem fixed_reference_pacing_and_favorable_path_give_receiver_direct_range
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := inputs.blockSync) maxAdmittedRefsPerRound)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules
      inputs.timedExecution)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (pacing : ValidatorFixedReferencePacingTraceSourceMap
      inputs.timedExecution inputs.proposalObligations inputs.recoveryWait
        admission parameters)
    {start firstFutureRound receiver : Nat}
    {prior : ValidatorCommitHead CommitId}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (inputs.timedExecution.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance
      inputs.timedExecution start)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (headAtStart :
      ((inputs.timedExecution.execution.trace start).validatorState
        receiver).commitHead = prior)
    (path : CommitHeadFirstSlotLeaderPathCoverageAfter config faults
      inputs.leaderSchedule.indirectDepth prior.id firstFutureRound) :
    Nonempty (ValidatorFixedReferenceFavorableDirectRange
      inputs.timedExecution start receiver
        (inputs.leaderSchedule.indirectDepth + 1) prior) := by
  classical
  let timed := inputs.timedExecution
  let syncCost :=
    validatorBlockSyncAcceptanceBound timed inputs.blockSync
  let backlogSlope := maxAdmittedRefsPerRound * syncCost
  let fixedPipelineCost :=
    3 * (timed.localActionBound + 1) + network.delta + 1 +
      timed.localActionBound + 1
  let threshold := Classical.choice
    (fixed_reference_visibility_threshold parameters prior pacing.spreadBase
      pacing.spreadLinear pacing.spreadQuadratic backlogSlope
        fixedPipelineCost pacing.spreadQuadraticBelowWait)
  let headMaximum := correctAvailableCommitHeadRoundMaximumUpTo faults
    (timed.execution.trace start) config.authorityCount
  let lateFirstRound := max pacing.sourceStartRound
    (max threshold.firstRound (headMaximum + 1))
  rcases recovery_inputs_and_favorable_path_give_aligned_fresh_window
      (lateFirstRound := lateFirstRound) inputs afterGst active noAdvance path
    with ⟨observation, windowBase, startBeforeObservation,
      lateBeforeWindow, alignedRaw⟩
  let aligned := Classical.choice alignedRaw
  have afterGstAtObservation : network.gst ≤ observation :=
    Nat.le_trans afterGst startBeforeObservation
  have activeAtObservation : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true := by
    intro time observationBeforeTime
    exact active time (Nat.le_trans startBeforeObservation
      observationBeforeTime)
  have sourceBeforeWindow : pacing.sourceStartRound ≤ windowBase + 2 :=
    Nat.le_trans
      (Nat.le_trans (Nat.le_max_left _ _) lateBeforeWindow)
      (Nat.le_refl _)
  have thresholdBeforeWindow : threshold.firstRound ≤ windowBase + 2 :=
    Nat.le_trans
      (Nat.le_trans (Nat.le_max_left _ _)
        (Nat.le_max_right pacing.sourceStartRound _))
      lateBeforeWindow
  have headMaximumBeforeWindow : headMaximum < windowBase + 2 := by
    have headSuccessorBeforeLate : headMaximum + 1 ≤ lateFirstRound := by
      exact Nat.le_trans (Nat.le_max_right _ _)
        (Nat.le_max_right pacing.sourceStartRound _)
    omega
  have baseAfterPrior : prior.round < windowBase + 2 := by
    rw [← headAtStart]
    exact Nat.lt_of_le_of_lt
      (correct_available_commit_head_round_le_maximum receiverInRange
        receiverCorrect)
      headMaximumBeforeWindow
  exact fixed_reference_pacing_and_aligned_family_give_receiver_direct_range
    inputs.recoveryParentAcceptance.toValidatorParentReadyAcceptanceRules
      inputs.acceptedRepresentatives admission retention parameters pacing
        threshold startBeforeObservation headAtStart baseAfterPrior
          afterGstAtObservation activeAtObservation sourceBeforeWindow
            thresholdBeforeWindow aligned.toFixedReferenceFamily receiverInRange receiverCorrect
              (by omega)

/-- The ideal ranking event supplies the favorable path internally. The
caller does not supply a future window, proposal, block, or path. -/
theorem favorable_sample_and_fixed_reference_pacing_give_receiver_direct_range
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    {family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount}
    (rankingSource : UniformRankingExecutionSourceMap family)
    (sampled : UniformRoundRankingTrace authorityCount)
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := (family.execution sampled).inputs.blockSync)
        maxAdmittedRefsPerRound)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules
      (family.execution sampled).inputs.timedExecution)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (pacing : ValidatorFixedReferencePacingTraceSourceMap
      (family.execution sampled).inputs.timedExecution
      (family.execution sampled).inputs.proposalObligations
      (family.execution sampled).inputs.recoveryWait admission parameters)
    (favorable :
      UniformRankingEndToEndExecutionFamily.AllValidatorCausalHeadFavorableWindows
        rankingSource sampled)
    {start receiver : Nat}
    {prior : ValidatorCommitHead CommitId}
    (afterGst : (family.execution sampled).network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      ((family.execution sampled).inputs.timedExecution.execution.trace time
        ).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance
      (family.execution sampled).inputs.timedExecution start)
    (receiverInRange :
      receiver < (family.execution sampled).config.authorityCount)
    (receiverCorrect :
      (family.execution sampled).faults.correctAvailable receiver = true)
    (headAtStart :
      (((family.execution sampled).inputs.timedExecution.execution.trace start
        ).validatorState receiver).commitHead = prior) :
    Nonempty (ValidatorFixedReferenceFavorableDirectRange
      (family.execution sampled).inputs.timedExecution start receiver
        ((family.execution sampled).inputs.leaderSchedule.indirectDepth + 1)
          prior) := by
  have noReceiverAdvance : ¬ValidatorReceiverCommitAdvance
      (family.execution sampled).inputs.timedExecution start receiver := by
    rintro ⟨finish, startBeforeFinish, advanced⟩
    exact noAdvance ⟨receiver, finish, receiverInRange, receiverCorrect,
      startBeforeFinish, advanced⟩
  rcases
      UniformRankingEndToEndExecutionFamily.stable_execution_receiver_suffix_has_future_causal_head_path
      rankingSource sampled receiver receiverInRange start prior favorable
        headAtStart noReceiverAdvance with
    ⟨firstFutureRound, _startBeforeBoundary, path⟩
  exact fixed_reference_pacing_and_favorable_path_give_receiver_direct_range
    (family.execution sampled).inputs admission retention parameters pacing
      afterGst active noAdvance receiverInRange receiverCorrect headAtStart path

/-- Under the ideal independent-uniform ranking law, every stable correct
receiver gets a fixed-reference direct range with probability one. The event
does not take a concrete future window, proposal, FlexCommitter run, install,
or commit as an input. -/
theorem fixed_reference_receiver_direct_range_probability_one
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {authorityCount : Nat}
    (law : IndependentUniformRoundRankingLaw authorityCount)
    (family : UniformRankingEndToEndExecutionFamily
      BlockId CommitId PacketId Encoding authorityCount)
    (rankingSource : UniformRankingExecutionSourceMap family)
    {maxAdmittedRefsPerRound : Nat}
    (admission : ∀ sampled,
      ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
        (syncRules := (family.execution sampled).inputs.blockSync)
          maxAdmittedRefsPerRound)
    (retention : ∀ sampled, ValidatorCommitOrthogonalAcceptedRetentionRules
      (family.execution sampled).inputs.timedExecution)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (pacing : ∀ sampled,
      ValidatorFixedReferencePacingTraceSourceMap
        (family.execution sampled).inputs.timedExecution
        (family.execution sampled).inputs.proposalObligations
        (family.execution sampled).inputs.recoveryWait (admission sampled)
          parameters)
    :
    law.probabilityOne (fun sampled =>
      ∀ {start receiver : Nat} {prior : ValidatorCommitHead CommitId},
        (family.execution sampled).network.gst ≤ start →
        (∀ time, start ≤ time →
          ((family.execution sampled).inputs.timedExecution.execution.trace
            time).epochActive = true) →
        ¬SomeCorrectAvailableCommitAdvance
          (family.execution sampled).inputs.timedExecution start →
        receiver < (family.execution sampled).config.authorityCount →
        (family.execution sampled).faults.correctAvailable receiver = true →
        (((family.execution sampled).inputs.timedExecution.execution.trace
          start).validatorState receiver).commitHead = prior →
        Nonempty (ValidatorFixedReferenceFavorableDirectRange
          (family.execution sampled).inputs.timedExecution start receiver
            ((family.execution sampled).inputs.leaderSchedule.indirectDepth + 1)
              prior)) := by
  apply law.probabilityOneMono
    (UniformRankingEndToEndExecutionFamily.all_validator_causal_head_favorable_windows_probability_one
      law family rankingSource)
  intro sampled favorable start receiver prior afterGst active noAdvance
    receiverInRange receiverCorrect headAtStart
  exact favorable_sample_and_fixed_reference_pacing_give_receiver_direct_range
    rankingSource sampled (admission sampled) (retention sampled) parameters
      (pacing sampled) favorable afterGst active noAdvance receiverInRange
        receiverCorrect headAtStart

end Mysticeti
