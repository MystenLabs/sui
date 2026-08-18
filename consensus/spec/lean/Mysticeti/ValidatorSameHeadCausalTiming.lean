/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorHeadRelativeGapWait
import Mysticeti.ValidatorFreshTimerReadyBridge
import Mysticeti.ValidatorPointwiseCommitHeadAlignment
import Mysticeti.ValidatorReceiverRelativeCausalBacklog

namespace Mysticeti

/-! Same-head timing for one commit-liveness step.

The hard branch has one exact prior commit head at all correct, available
validators. If no correct commit advances, that head stays fixed. The local
wait is then `W (round - prior.round)` at every required timer.

This module first proves the exact quadratic rate comparison. It then records
the two past-action facts which the current timer-paced production value does
not retain: the timer's local commit head and the directional start lag between
fresh adjacent timers. Neither fact states that a future timer, proposal,
block, layer, or commit exists.
-/

/-- The complete causal cost has a linear upper bound relative to any fixed
commit-head round. -/
theorem ValidatorCausalCatchupEnvelope.catchup_cost_le_head_gap_linear
    (envelope : ValidatorCausalCatchupEnvelope)
    (costPerWorkUnit headRound : Nat)
    {round : Nat}
    (envelopeBeforeRound : envelope.startRound ≤ round)
    (headBeforeRound : headRound ≤ round) :
    envelope.catchupCost costPerWorkUnit round ≤
      (envelope.catchupCost costPerWorkUnit envelope.startRound +
          (headRound - envelope.startRound) *
            (envelope.newWorkPerRound * costPerWorkUnit)) +
        (round - headRound) *
          (envelope.newWorkPerRound * costPerWorkUnit) := by
  let slope := envelope.newWorkPerRound * costPerWorkUnit
  have costBound := envelope.catchup_cost_linear_bound costPerWorkUnit
    envelopeBeforeRound
  have gapBound : round - envelope.startRound ≤
      (headRound - envelope.startRound) + (round - headRound) := by
    omega
  have scaledGapBound :
      (round - envelope.startRound) * slope ≤
        ((headRound - envelope.startRound) + (round - headRound)) * slope :=
    Nat.mul_le_mul_right slope gapBound
  calc
    envelope.catchupCost costPerWorkUnit round ≤
        envelope.catchupCost costPerWorkUnit envelope.startRound +
          (round - envelope.startRound) * slope := by
      simpa [slope] using costBound
    _ ≤ envelope.catchupCost costPerWorkUnit envelope.startRound +
          ((headRound - envelope.startRound) +
            (round - headRound)) * slope :=
      Nat.add_le_add_left scaledGapBound _
    _ = (envelope.catchupCost costPerWorkUnit envelope.startRound +
          (headRound - envelope.startRound) * slope) +
        (round - headRound) * slope := by
      simp [Nat.add_mul, Nat.add_assoc]

/-- Legacy scaffolding: a quadratic head-relative wait covers one linear
timer-start lag, the complete causal catch-up envelope, and one fixed local
pipeline cost.

The conclusion compares one exact head with itself. It uses no cross-head wait
rule. The final proof uses the actual-block cutoff backlog instead of this
complete-history envelope. -/
theorem head_relative_quadratic_eventually_covers_spread_and_catchup
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
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    (prior : ValidatorCommitHead CommitId)
    (startLagBase startLagSlope fixedPipelineCost : Nat)
    (slopeCondition :
      rate.envelope.newWorkPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules + startLagSlope <
        2 * parameters.quadraticCoefficient) :
    ∃ firstRound,
      max prior.round rate.envelope.startRound ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            parameters.wait prior round +
                ((startLagBase + startLagSlope * (round - prior.round)) +
                  fixedPipelineCost +
                  rate.envelope.catchupCost
                    (validatorBlockSyncAcceptanceBound timed syncRules) round) ≤
              parameters.wait prior (round + 1) := by
  let costPerWorkUnit := validatorBlockSyncAcceptanceBound timed syncRules
  let catchupSlope := rate.envelope.newWorkPerRound * costPerWorkUnit
  let catchupBase :=
    fixedPipelineCost +
      (rate.envelope.catchupCost costPerWorkUnit rate.envelope.startRound +
        (prior.round - rate.envelope.startRound) * catchupSlope)
  have concreteSlope : catchupSlope + startLagSlope <
      2 * parameters.quadraticCoefficient := by
    simpa [catchupSlope, costPerWorkUnit] using slopeCondition
  rcases parameters.wait_adjacent_eventually_dominates_two_linear_costs
      prior catchupBase catchupSlope startLagBase startLagSlope
        concreteSlope with
    ⟨marginStart, headBeforeMargin, margin⟩
  let firstRound := max marginStart rate.envelope.startRound
  refine ⟨firstRound, ?_, ?_⟩
  · exact Nat.max_le.mpr ⟨
      Nat.le_trans headBeforeMargin (Nat.le_max_left _ _),
      Nat.le_max_right _ _⟩
  · intro round firstBeforeRound
    have marginBeforeRound : marginStart ≤ round :=
      Nat.le_trans (Nat.le_max_left _ _) firstBeforeRound
    have envelopeBeforeRound : rate.envelope.startRound ≤ round :=
      Nat.le_trans (Nat.le_max_right _ _) firstBeforeRound
    have headBeforeRound : prior.round ≤ round :=
      Nat.le_trans headBeforeMargin marginBeforeRound
    have costBound := rate.envelope.catchup_cost_le_head_gap_linear
      costPerWorkUnit prior.round envelopeBeforeRound headBeforeRound
    have fullCostBound :
        (startLagBase + startLagSlope * (round - prior.round)) +
              fixedPipelineCost +
              rate.envelope.catchupCost costPerWorkUnit round ≤
          (catchupBase + catchupSlope * (round - prior.round)) +
            (startLagBase + startLagSlope * (round - prior.round)) := by
      dsimp [catchupBase]
      have shifted := Nat.add_le_add_left costBound
        ((startLagBase + startLagSlope * (round - prior.round)) +
          fixedPipelineCost)
      simpa [catchupSlope, Nat.mul_comm, Nat.add_assoc, Nat.add_comm,
        Nat.add_left_comm] using shifted
    calc
      parameters.wait prior round +
            ((startLagBase + startLagSlope * (round - prior.round)) +
              fixedPipelineCost +
              rate.envelope.catchupCost costPerWorkUnit round) ≤
          parameters.wait prior round +
            ((catchupBase + catchupSlope * (round - prior.round)) +
              (startLagBase + startLagSlope * (round - prior.round))) :=
        Nat.add_le_add_left fullCostBound _
      _ ≤ parameters.wait prior (round + 1) :=
        margin round marginBeforeRound

/-- A quadratic head-relative wait covers a linear timer spread, one bounded
round of new causal work, and one fixed pipeline cost.

This result does not use the complete causal-history envelope. The causal cost
is the source-local novelty for one adjacent round. -/
theorem head_relative_quadratic_eventually_covers_spread_and_one_round_novelty
    {CommitId : Type}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    (prior : ValidatorCommitHead CommitId)
    (spreadBase spreadSlope newWorkPerRound costPerWorkUnit
      fixedPipelineCost : Nat)
    (slopeCondition :
      spreadSlope < 2 * parameters.quadraticCoefficient) :
    ∃ firstRound,
      prior.round ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            parameters.wait prior round +
                ((spreadBase + spreadSlope * (round - prior.round)) +
                  (newWorkPerRound * costPerWorkUnit + fixedPipelineCost)) ≤
              parameters.wait prior (round + 1) := by
  have concreteSlope : 0 + spreadSlope <
      2 * parameters.quadraticCoefficient := by
    simpa using slopeCondition
  rcases parameters.wait_adjacent_eventually_dominates_two_linear_costs
      prior (newWorkPerRound * costPerWorkUnit + fixedPipelineCost) 0
        spreadBase spreadSlope concreteSlope with
    ⟨firstRound, headBeforeFirst, margin⟩
  refine ⟨firstRound, headBeforeFirst, ?_⟩
  intro round firstBeforeRound
  have covered := margin round firstBeforeRound
  simpa [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using covered

/-- The quadratic term is a lower bound on the concrete quadratic gap wait. -/
theorem quadraticGapWaitFromGap_quadratic_lower
    (baseWait linearCoefficient quadraticCoefficient gap : Nat) :
    quadraticCoefficient * gap * gap ≤
      quadraticGapWaitFromGap baseWait linearCoefficient
        quadraticCoefficient gap := by
  induction gap with
  | zero => simp [quadraticGapWaitFromGap]
  | succ previous inductionHypothesis =>
      have squareStep :
          quadraticCoefficient * (previous + 1) * (previous + 1) =
            quadraticCoefficient * previous * previous +
              quadraticCoefficient * (2 * previous + 1) := by
        have twice : 2 * previous = previous + previous := by omega
        rw [twice]
        simp [Nat.mul_add, Nat.add_mul, Nat.mul_assoc,
          Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
      rw [squareStep, quadraticGapWaitFromGap_succ]
      exact Nat.le_trans
        (Nat.add_le_add_right inductionHypothesis _)
        (by omega)

/-- A quadratic head-relative wait covers a linear timer spread and a linear
backlog above one fixed receiver cutoff.

The backlog slope can include the maximum admitted references per round and
the block-sync cost for each reference. The result is pure arithmetic. It does
not assume that a future block, timer, layer, or commit exists. -/
theorem head_relative_quadratic_eventually_covers_spread_and_cutoff_backlog
    {CommitId : Type}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    (prior : ValidatorCommitHead CommitId)
    (cutoffRound spreadBase spreadSlope backlogSlope fixedPipelineCost : Nat)
    (slopeCondition :
      backlogSlope + spreadSlope <
        2 * parameters.quadraticCoefficient) :
    ∃ firstRound,
      prior.round ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            parameters.wait prior round +
                ((spreadBase + spreadSlope * (round - prior.round)) +
                  (backlogSlope * (round - cutoffRound) +
                    fixedPipelineCost)) ≤
              parameters.wait prior (round + 1) := by
  let backlogBase :=
    fixedPipelineCost + backlogSlope * (prior.round - cutoffRound)
  rcases parameters.wait_adjacent_eventually_dominates_two_linear_costs
      prior backlogBase backlogSlope spreadBase spreadSlope slopeCondition with
    ⟨firstRound, headBeforeFirst, margin⟩
  refine ⟨firstRound, headBeforeFirst, ?_⟩
  intro round firstBeforeRound
  have headBeforeRound : prior.round ≤ round :=
    Nat.le_trans headBeforeFirst firstBeforeRound
  have gapBound : round - cutoffRound ≤
      (prior.round - cutoffRound) + (round - prior.round) := by
    omega
  have scaledGapBound : backlogSlope * (round - cutoffRound) ≤
      backlogSlope * (prior.round - cutoffRound) +
        backlogSlope * (round - prior.round) := by
    simpa only [Nat.mul_add] using Nat.mul_le_mul_left backlogSlope gapBound
  have backlogCostBound :
      backlogSlope * (round - cutoffRound) + fixedPipelineCost ≤
        backlogBase + backlogSlope * (round - prior.round) := by
    dsimp [backlogBase]
    have shifted := Nat.add_le_add_right scaledGapBound fixedPipelineCost
    simpa only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using shifted
  have covered := margin round firstBeforeRound
  calc
    parameters.wait prior round +
          ((spreadBase + spreadSlope * (round - prior.round)) +
            (backlogSlope * (round - cutoffRound) + fixedPipelineCost)) ≤
        parameters.wait prior round +
          ((spreadBase + spreadSlope * (round - prior.round)) +
            (backlogBase + backlogSlope * (round - prior.round))) :=
      Nat.add_le_add_left
        (Nat.add_le_add_left backlogCostBound
          (spreadBase + spreadSlope * (round - prior.round))) _
    _ ≤ parameters.wait prior (round + 1) := by
      simpa only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using covered

/-- The quadratic wait value eventually covers a quadratic timer spread and
a linear receiver backlog above one fixed cutoff.

The lower-edge timer theorem has already included the prior-round wait.
Therefore, this theorem compares the remaining cost with `W (R + 1)` itself,
not with the adjacent difference `W (R + 1) - W R`. -/
theorem head_relative_quadratic_value_eventually_covers_quadratic_spread_and_cutoff_backlog
    {CommitId : Type}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    (prior : ValidatorCommitHead CommitId)
    (cutoffRound spreadBase spreadLinear spreadQuadratic backlogSlope
      fixedPipelineCost : Nat)
    (quadraticCondition :
      spreadQuadratic < parameters.quadraticCoefficient) :
    ∃ firstRound,
      prior.round ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            (spreadBase + spreadLinear * (round - prior.round) +
                spreadQuadratic * (round - prior.round) *
                  (round - prior.round)) +
                ((round - cutoffRound) * backlogSlope +
                  fixedPipelineCost) ≤
              parameters.wait prior (round + 1) := by
  let fixedCost := spreadBase + fixedPipelineCost +
    (prior.round - cutoffRound) * backlogSlope
  let totalLinear := spreadLinear + backlogSlope
  let firstGap := max fixedCost (totalLinear + 1)
  let firstRound := prior.round + firstGap
  refine ⟨firstRound, by simp [firstRound], ?_⟩
  intro round firstBeforeRound
  have headBeforeRound : prior.round ≤ round :=
    Nat.le_trans (by simp [firstRound]) firstBeforeRound
  let gap := round - prior.round
  have firstGapBeforeGap : firstGap ≤ gap := by
    simp only [firstRound, gap] at firstBeforeRound ⊢
    omega
  have fixedWithinGap : fixedCost ≤ gap :=
    Nat.le_trans (Nat.le_max_left _ _) firstGapBeforeGap
  have linearPlusOneWithinGap : totalLinear + 1 ≤ gap :=
    Nat.le_trans (Nat.le_max_right _ _) firstGapBeforeGap
  have fixedAndLinearWithinSquare :
      fixedCost + totalLinear * gap ≤ gap * gap := by
    calc
      fixedCost + totalLinear * gap ≤ gap + totalLinear * gap :=
        Nat.add_le_add_right fixedWithinGap _
      _ = (totalLinear + 1) * gap := by
        simp [Nat.add_mul, Nat.add_comm]
      _ ≤ gap * gap :=
        Nat.mul_le_mul_right gap linearPlusOneWithinGap
  have coefficientWithUnit : spreadQuadratic + 1 ≤
      parameters.quadraticCoefficient := by
    omega
  have polynomialWithinQuadratic :
      fixedCost + totalLinear * gap + spreadQuadratic * gap * gap ≤
        parameters.quadraticCoefficient * gap * gap := by
    calc
      fixedCost + totalLinear * gap + spreadQuadratic * gap * gap ≤
          gap * gap + spreadQuadratic * gap * gap :=
        Nat.add_le_add_right fixedAndLinearWithinSquare _
      _ = (spreadQuadratic + 1) * (gap * gap) := by
        simp [Nat.add_mul, Nat.mul_assoc, Nat.add_comm]
      _ ≤ parameters.quadraticCoefficient * (gap * gap) :=
        Nat.mul_le_mul_right (gap * gap) coefficientWithUnit
      _ = parameters.quadraticCoefficient * gap * gap := by
        simp [Nat.mul_assoc]
  have cutoffGapBound : round - cutoffRound ≤
      (prior.round - cutoffRound) + gap := by
    simp only [gap]
    omega
  have scaledCutoffGapBound : (round - cutoffRound) * backlogSlope ≤
      (prior.round - cutoffRound) * backlogSlope +
        gap * backlogSlope := by
    simpa only [Nat.add_mul] using
      Nat.mul_le_mul_right backlogSlope cutoffGapBound
  have actualCostWithinPolynomial :
      (spreadBase + spreadLinear * gap +
          spreadQuadratic * gap * gap) +
          ((round - cutoffRound) * backlogSlope + fixedPipelineCost) ≤
        fixedCost + totalLinear * gap + spreadQuadratic * gap * gap := by
    have shifted := Nat.add_le_add_left scaledCutoffGapBound
      ((spreadBase + spreadLinear * gap +
        spreadQuadratic * gap * gap) + fixedPipelineCost)
    simpa [fixedCost, totalLinear, Nat.add_mul, Nat.mul_comm,
      Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using shifted
  have waitAtRoundLower : parameters.quadraticCoefficient * gap * gap ≤
      parameters.wait prior round := by
    simpa only [ValidatorHeadRelativeQuadraticWaitParameters.wait, gap] using
      quadraticGapWaitFromGap_quadratic_lower parameters.baseWait
        parameters.linearCoefficient parameters.quadraticCoefficient gap
  have nextGap : round + 1 - prior.round = gap + 1 := by
    simp only [gap]
    omega
  have waitMonotone : parameters.wait prior round ≤
      parameters.wait prior (round + 1) := by
    simp only [ValidatorHeadRelativeQuadraticWaitParameters.wait]
    rw [nextGap]
    change quadraticGapWaitFromGap parameters.baseWait
        parameters.linearCoefficient parameters.quadraticCoefficient gap ≤
      quadraticGapWaitFromGap parameters.baseWait
        parameters.linearCoefficient parameters.quadraticCoefficient (gap + 1)
    rw [quadraticGapWaitFromGap_succ]
    omega
  exact Nat.le_trans actualCostWithinPolynomial
    (Nat.le_trans polynomialWithinQuadratic
      (Nat.le_trans waitAtRoundLower waitMonotone))


/-- An adjacent wait margin places any concrete receiver-resolution cost
before the next proposal snapshot.

The caller derives `resolutionCost` from current source data. This timing
result does not use a causal-history size envelope. -/
theorem adjacent_wait_margin_covers_timer_paced_resolution_cost
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
    {author receiver round startDifference resolutionCost : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (broadcast : ValidatorTimerPacedPeerBroadcast timed previous.snapshot
      author receiver previous.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (previousStartBound :
      previous.timerStartedAt ≤ next.timerStartedAt + startDifference)
    (visibilityMargin :
      waits.wait previousPrior round +
          (startDifference + 3 * (timed.localActionBound + 1) +
            network.delta + resolutionCost) ≤
        waits.wait receiverPrior (round + 1)) :
    broadcast.packet.deliveredAt + resolutionCost ≤
      next.snapshot.snapshotAt := by
  have deliveryFacts := timer_paced_peer_broadcast_is_delivered previous
    broadcast receiverInRange receiverCorrectAvailable sentAfterGst
  have sentBound :=
    timer_paced_peer_broadcast_sent_within_round_pipeline previous broadcast
  rw [previousHead] at sentBound
  rw [next.snapshotAtDeadline, nextHead]
  calc
    broadcast.packet.deliveredAt + resolutionCost ≤
        broadcast.packet.sentAt + network.delta + resolutionCost :=
      Nat.add_le_add_right deliveryFacts.2.1 resolutionCost
    _ ≤ (previous.timerStartedAt + waits.wait previousPrior round +
          3 * (timed.localActionBound + 1)) + network.delta +
            resolutionCost := by
      exact Nat.add_le_add_right
        (Nat.add_le_add_right sentBound network.delta) resolutionCost
    _ ≤ (next.timerStartedAt + startDifference + waits.wait previousPrior round +
          3 * (timed.localActionBound + 1)) + network.delta +
            resolutionCost := by
      have shifted := Nat.add_le_add_right previousStartBound
        (waits.wait previousPrior round + 3 * (timed.localActionBound + 1) +
          network.delta + resolutionCost)
      simpa only [Nat.add_assoc] using shifted
    _ = next.timerStartedAt +
          (waits.wait previousPrior round +
            (startDifference + 3 * (timed.localActionBound + 1) +
              network.delta + resolutionCost)) := by
      ac_rfl
    _ ≤ next.timerStartedAt + waits.wait receiverPrior (round + 1) := by
      exact Nat.add_le_add_left visibilityMargin next.timerStartedAt

/-- Pairwise spread and the actual lower successor edge retain the complete
prior-round wait in the deadline comparison. -/
theorem fresh_timer_start_spread_and_successor_lower_gives_deadline_bound
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
    {observation author receiver round spread roundWait : Nat}
    (previousSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations
      waits observation round spread)
    (lower : ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
      observation round roundWait)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations waits
      observation receiver (round + 1)) :
    previous.production.timerStartedAt + roundWait ≤
      next.production.timerStartedAt + spread := by
  unfold ValidatorFreshRoundTimerStartSpreadAt at previousSpread
  unfold ValidatorFreshTimerStartSuccessorLowerAt at lower
  rcases lower next with ⟨lowerAuthor, lowerPrevious, lowerBound⟩
  have pairwise := previousSpread previous lowerPrevious
  have pairwiseWithWait := Nat.add_le_add_right pairwise roundWait
  have lowerWithSpread := Nat.add_le_add_right lowerBound spread
  exact Nat.le_trans (by
    simpa only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
      pairwiseWithWait) lowerWithSpread

/-- A lower-edge deadline bound places any concrete receiver-resolution cost
before the next proposal snapshot.

The prior-round wait is already in `previousDeadlineBound`. Therefore, the
remaining visibility margin does not include that wait a second time. -/
theorem adjacent_lower_edge_margin_covers_timer_paced_resolution_cost
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
    {author receiver round startDifference resolutionCost : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (broadcast : ValidatorTimerPacedPeerBroadcast timed previous.snapshot
      author receiver previous.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (previousDeadlineBound :
      previous.timerStartedAt + waits.wait previousPrior round ≤
        next.timerStartedAt + startDifference)
    (visibilityMargin :
      startDifference + 3 * (timed.localActionBound + 1) + network.delta +
          resolutionCost ≤
        waits.wait receiverPrior (round + 1)) :
    broadcast.packet.deliveredAt + resolutionCost ≤
      next.snapshot.snapshotAt := by
  have deliveryFacts := timer_paced_peer_broadcast_is_delivered previous
    broadcast receiverInRange receiverCorrectAvailable sentAfterGst
  have sentBound :=
    timer_paced_peer_broadcast_sent_within_round_pipeline previous broadcast
  rw [previousHead] at sentBound
  rw [next.snapshotAtDeadline, nextHead]
  calc
    broadcast.packet.deliveredAt + resolutionCost ≤
        broadcast.packet.sentAt + network.delta + resolutionCost :=
      Nat.add_le_add_right deliveryFacts.2.1 resolutionCost
    _ ≤ (previous.timerStartedAt + waits.wait previousPrior round +
          3 * (timed.localActionBound + 1)) + network.delta +
            resolutionCost := by
      exact Nat.add_le_add_right
        (Nat.add_le_add_right sentBound network.delta) resolutionCost
    _ ≤ (next.timerStartedAt + startDifference +
          3 * (timed.localActionBound + 1)) + network.delta +
            resolutionCost := by
      exact Nat.add_le_add_right
        (Nat.add_le_add_right
          (Nat.add_le_add_right previousDeadlineBound
            (3 * (timed.localActionBound + 1))) network.delta)
        resolutionCost
    _ = next.timerStartedAt +
          (startDifference + 3 * (timed.localActionBound + 1) +
            network.delta + resolutionCost) := by
      ac_rfl
    _ ≤ next.timerStartedAt + waits.wait receiverPrior (round + 1) :=
      Nat.add_le_add_left visibilityMargin next.timerStartedAt

/-! Actual-block receiver-relative causal resolution. -/

/-- Current and past source facts for one delivered actual block.

The receiver has one cutoff in the actual child capsule. The exact protected
parent-sync source starts when the delivered block is available. The proposed
per-round admission rule supplies the linear unresolved-work bound. No field
states a future acceptance, timer, layer, or commit result. -/
structure ValidatorTimerPacedLinearBacklogSyncSource
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {author receiver round floor : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt) : Type where
  cutoff : ValidatorAcceptedCausalCapsuleRoundCutoffAt timed
    (admission.capsuleFor production.snapshot.block)
      (broadcast.packet.deliveredAt + 1) receiver floor
  targetSyncSource : ValidatorBlockParentSyncSource syncRules
    production.snapshot.block receiver author
      (admission.capsuleFor production.snapshot.block).history
        (broadcast.packet.deliveredAt + 1)

/-- One actual block resolves within its linear receiver-relative backlog.

The only proposed implementation rule is the transitive per-round admission
cap in `admission`. Current Rust does not yet enforce that cap. All other
premises describe the actual packet, persisted block, receiver cutoff, and
protected synchronization source. -/
theorem timer_paced_peer_broadcast_resolves_within_linear_backlog_or_gc
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    {author receiver round floor : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt)
    (source : ValidatorTimerPacedLinearBacklogSyncSource admission production
      broadcast (floor := floor))
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (active : ∀ time, broadcast.packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ acceptedAt,
      broadcast.packet.deliveredAt + 1 ≤ acceptedAt ∧
        acceptedAt ≤ broadcast.packet.deliveredAt + 1 +
            ((round - floor) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1 ∧
        (((timed.execution.trace acceptedAt).validatorState receiver).accepted
            production.snapshot.block.reference = true ∨
          production.snapshot.block.reference.round ≤
            ((timed.execution.trace acceptedAt).validatorState
              receiver).gcRound) := by
  have authorInRange : author < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have authorCorrectAvailable : faults.correctAvailable author = true := by
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  have deliveryBounds := network.postGstDelivery broadcast.packet
    broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using authorInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using authorCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have delivered := timed.execution.protocolPacketsAreDelivered
    broadcast.packetId broadcast.packet broadcast.packetInTrace
      broadcast.packetIsProtocol
      (by simpa [broadcast.packetSender] using authorInRange)
      (by simpa [broadcast.packetReceiver] using receiverInRange)
      (by simpa [broadcast.packetSender] using authorCorrectAvailable)
      (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
      sentAfterGst
  have packetAtDelivery := timed.execution.packetHistoryMonotone
    broadcast.packet.sentAt broadcast.packet.deliveredAt deliveryBounds.1
      broadcast.packetId broadcast.packet broadcast.packetInTrace
  have syncStartsAfterGst : network.gst ≤
      broadcast.packet.deliveredAt + 1 :=
    Nat.le_trans sentAfterGst
      (Nat.le_trans deliveryBounds.1 (Nat.le_add_right _ _))
  rcases admission.history_ready_within_linear_backlog source.targetSyncSource
      authorInRange authorCorrectAvailable production.persistenceOccurs
        source.cutoff receiverInRange receiverCorrectAvailable
          syncStartsAfterGst active with
    ⟨parentsReadyAt, deliveryBeforeParentsReady, parentsReadyBound,
      historyReady⟩
  have parentsReadyBound' : parentsReadyAt ≤
      broadcast.packet.deliveredAt + 1 +
        ((round - floor) * maxAdmittedRefsPerRound) *
          validatorBlockSyncAcceptanceBound timed syncRules := by
    simpa only [production.blockRound] using parentsReadyBound
  have parentsReady : ∀ parent,
      parent ∈ production.snapshot.block.parents →
        ((timed.execution.trace parentsReadyAt).validatorState
            broadcast.packet.receiver).accepted parent = true ∨
          parent.round ≤
            ((timed.execution.trace parentsReadyAt).validatorState
              broadcast.packet.receiver).gcRound := by
    intro parent parentMember
    rw [broadcast.packetReceiver]
    rcases source.targetSyncSource.coversDirectParents parent parentMember with
      acceptedAtStart | ⟨parentBlock, blockMember, blockReference⟩
    · exact Or.inl (timed.execution.accepted_block_persists receiverInRange
        deliveryBeforeParentsReady acceptedAtStart)
    · simpa [ValidatorReferenceAcceptedOrGcRootAt, blockReference] using
        historyReady parentBlock blockMember
  by_cases blockAtRoot : production.snapshot.block.reference.round ≤
      ((timed.execution.trace parentsReadyAt).validatorState receiver).gcRound
  · refine ⟨parentsReadyAt, deliveryBeforeParentsReady, ?_, Or.inr blockAtRoot⟩
    exact Nat.le_trans parentsReadyBound' (by
      simpa only [Nat.add_assoc] using Nat.le_add_right
        (broadcast.packet.deliveredAt + 1 +
          ((round - floor) * maxAdmittedRefsPerRound) *
            validatorBlockSyncAcceptanceBound timed syncRules)
        (timed.localActionBound + 1))
  · have blockAboveGc :
        ((timed.execution.trace parentsReadyAt).validatorState
          broadcast.packet.receiver).gcRound <
            production.snapshot.block.reference.round := by
      rw [broadcast.packetReceiver]
      omega
    rcases delivered_block_with_current_gc_ready_parents_is_accepted timed
        acceptance packetAtDelivery broadcast.packetPayload delivered
          (by simpa [broadcast.packetReceiver] using receiverInRange)
          (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
          (by simpa [production.snapshot.blockIsOwnProposal,
            production.proposer] using authorInRange)
          (Or.inr production.validParents) deliveryBeforeParentsReady
          blockAboveGc parentsReady with
      ⟨acceptedAt, parentsBeforeAccepted, acceptedWithinBound, accepted⟩
    refine ⟨acceptedAt,
      Nat.le_trans deliveryBeforeParentsReady parentsBeforeAccepted, ?_,
        Or.inl (by simpa [broadcast.packetReceiver] using accepted)⟩
    exact Nat.le_trans acceptedWithinBound
      (Nat.add_le_add_right parentsReadyBound'
        (timed.localActionBound + 1))

/-- An actual linear-backlog source gives adjacent parent evidence, unless the
receiver first installs a later commit.

The result uses the actual child capsule. It does not compare that capsule
with one selected parent branch. -/
theorem adjacent_timer_paced_productions_give_parent_evidence_from_linear_backlog_or_commit_advance
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {commonStart author receiver round floor startDifference : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (sourceFor : ∀ broadcast : ValidatorTimerPacedPeerBroadcast timed
      previous.snapshot author receiver previous.proposalActionAt,
        ValidatorTimerPacedLinearBacklogSyncSource admission previous broadcast
          (floor := floor))
    (commonStartBeforePrevious : commonStart ≤ previous.timerStartedAt)
    (commonStartBeforeNext : commonStart ≤ next.timerStartedAt)
    (receiverHeadAtStart :
      ((timed.execution.trace commonStart).validatorState receiver).commitHead =
        receiverPrior)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (referenceAfterPrior : receiverPrior.round < round)
    (previousDeadlineBound :
      previous.timerStartedAt + waits.wait previousPrior round ≤
        next.timerStartedAt + startDifference)
    (visibilityMargin :
      startDifference + 3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((round - floor) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        waits.wait receiverPrior (round + 1))
    (previousStartsAfterGst : network.gst ≤ previous.timerStartedAt)
    (active : ∀ time, commonStart ≤ time →
      (timed.execution.trace time).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed commonStart ∨
      ValidatorAdjacentTimerPacedParentEvidence previous next := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed commonStart
  · exact Or.inl advanced
  · right
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
    have headAt : ∀ time, commonStart ≤ time →
        ((timed.execution.trace time).validatorState receiver).commitHead =
          receiverPrior := by
      intro time ordered
      exact (no_commit_advance_keeps_correct_commit_head receiverInRange
        receiverCorrect ordered advanced).trans receiverHeadAtStart
    have gcBelowPrevious : ∀ time, commonStart ≤ time →
        ((timed.execution.trace time).validatorState receiver).gcRound <
          previous.snapshot.block.reference.round := by
      intro time ordered
      have gcAtMostHead :
          ((timed.execution.trace time).validatorState receiver).gcRound ≤
            ((timed.execution.trace time).validatorState
              receiver).commitHead.round :=
        correct_validator_gc_round_at_most_commit_round
          (time := time) timed.execution receiverInRange receiverCorrect
      rw [headAt time ordered] at gcAtMostHead
      rw [previous.blockRound]
      omega
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
      exact ValidatorCausalCapsuleCatchupRateRules.accepted_retained_timer_paced_block_gives_parent_evidence
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
            previousHead nextHead previousDeadlineBound (by
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
          have belowAtNext := gcBelowPrevious next.snapshot.snapshotAt
            commonStartBeforeNextSnapshot
          omega
      have packetAtDelivery := timed.execution.packetHistoryMonotone
        broadcast.packet.sentAt broadcast.packet.deliveredAt deliveryFacts.1
          broadcast.packetId broadcast.packet broadcast.packetInTrace
      have localBody : ValidatorLocalBlockBodyAt timed
          broadcast.packet.deliveredAt receiver previous.snapshot.block :=
        .delivered broadcast.packetId broadcast.packet packetAtDelivery
          broadcast.packetIsProtocol broadcast.packetReceiver
          broadcast.packetPayload deliveryFacts.2.2
      have bodyPin := sync.local_body_creates_durable_pin receiverInRange
        receiverCorrect
        (active (broadcast.packet.deliveredAt + 1)
          (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1)))
        (gcBelowPrevious (broadcast.packet.deliveredAt + 1)
          (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1)))
        localBody
      have deliveryBeforeNextSnapshot : broadcast.packet.deliveredAt + 1 ≤
          next.snapshot.snapshotAt := by
        have oneBeforeResolution : 1 ≤ resolutionCost := by
          simp only [resolutionCost]
          omega
        exact Nat.le_trans
          (Nat.add_le_add_left oneBeforeResolution broadcast.packet.deliveredAt)
          resolutionBeforeNextSnapshot
      have pinAtNext := sync.body_pin_persists_while_head_is_current
        receiverInRange receiverCorrect deliveryBeforeNextSnapshot bodyPin.1
        (by
          intro time startBeforeTime _timeBeforeFinish
          exact active time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime))
        (by
          intro time startBeforeTime _timeBeforeFinish
          exact gcBelowPrevious time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime))
        (by
          intro time startBeforeTime _timeBeforeFinish
          have laterHead := headAt time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime)
          have initialHead := headAt (broadcast.packet.deliveredAt + 1)
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
          rw [laterHead, initialHead]
          exact Nat.le_refl _)
      have retainedAtNext := sync.acceptedPinnedBodyIsRetained
        next.snapshot.snapshotAt receiver previous.snapshot.block.reference _
          receiverInRange receiverCorrect pinAtNext
          (gcBelowPrevious next.snapshot.snapshotAt
            commonStartBeforeNextSnapshot) acceptedAtNext
      exact ValidatorCausalCapsuleCatchupRateRules.accepted_retained_timer_paced_block_gives_parent_evidence
        representatives previous next acceptedAtNext retainedAtNext

/-- One same-head margin and one actual linear-backlog source give exact
adjacent parent evidence for two actual fresh productions.

The timer-origin rule derives both production heads. The no-advance suffix
removes the receiver-local commit alternative. -/
theorem same_head_fresh_adjacent_productions_give_parent_evidence_from_linear_backlog_of_margin
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {start observation author receiver round floor startDifference : Time}
    {prior : ValidatorCommitHead CommitId}
    (headsAtStart : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior)
    (startBeforeObservation : start ≤ observation)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1))
    (sourceFor : ∀ broadcast : ValidatorTimerPacedPeerBroadcast timed
      previous.production.snapshot author receiver
        previous.production.proposalActionAt,
      ValidatorTimerPacedLinearBacklogSyncSource admission previous.production
        broadcast (floor := floor))
    (referenceAfterPrior : prior.round < round)
    (previousDeadlineBound :
      previous.production.timerStartedAt + waits.wait prior round ≤
        next.production.timerStartedAt + startDifference)
    (visibilityMargin :
      startDifference + 3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((round - floor) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        waits.wait prior (round + 1)) :
    ValidatorAdjacentTimerPacedParentEvidence
      previous.production next.production := by
  have authorInRange : author < config.authorityCount := by
    simpa [previous.production.proposer] using
      previous.production.snapshot.proposerInRange
  have authorCorrect : faults.correctAvailable author = true := by
    simpa [previous.production.proposer] using
      previous.production.snapshot.proposerCorrectAvailable
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerCorrectAvailable
  have startBeforePreviousTimer : start ≤
      previous.production.timerStartedAt :=
    Nat.le_trans startBeforeObservation
      (Nat.le_of_lt previous.timerAfterObservation)
  have startBeforeNextTimer : start ≤ next.production.timerStartedAt :=
    Nat.le_trans startBeforeObservation
      (Nat.le_of_lt next.timerAfterObservation)
  have previousHeadAtTimer :
      ((timed.execution.trace
        previous.production.timerStartedAt).validatorState author).commitHead =
          prior :=
    (no_commit_advance_keeps_correct_commit_head authorInRange authorCorrect
      startBeforePreviousTimer noAdvance).trans
        (headsAtStart author authorInRange authorCorrect)
  have nextHeadAtTimer :
      ((timed.execution.trace
        next.production.timerStartedAt).validatorState receiver).commitHead =
          prior :=
    (no_commit_advance_keeps_correct_commit_head receiverInRange receiverCorrect
      startBeforeNextTimer noAdvance).trans
        (headsAtStart receiver receiverInRange receiverCorrect)
  have previousHead : previous.production.commitHead = prior :=
    (originRules.production_head_at_timer_start previous.production).trans
      previousHeadAtTimer
  have nextHead : next.production.commitHead = prior :=
    (originRules.production_head_at_timer_start next.production).trans
      nextHeadAtTimer
  have receiverHeadAtObservation :
      ((timed.execution.trace observation).validatorState receiver).commitHead =
        prior :=
    (no_commit_advance_keeps_correct_commit_head receiverInRange receiverCorrect
      startBeforeObservation noAdvance).trans
        (headsAtStart receiver receiverInRange receiverCorrect)
  have previousStartsAfterGst : network.gst ≤
      previous.production.timerStartedAt :=
    Nat.le_trans afterGst startBeforePreviousTimer
  have activeFromObservation : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true := by
    intro time observationBeforeTime
    exact active time
      (Nat.le_trans startBeforeObservation observationBeforeTime)
  have noAdvanceAtObservation :
      ¬SomeCorrectAvailableCommitAdvance timed observation :=
    no_commit_advance_persists_to_later_start startBeforeObservation noAdvance
  rcases adjacent_timer_paced_productions_give_parent_evidence_from_linear_backlog_or_commit_advance
      acceptance sync representatives admission previous.production
        next.production sourceFor (Nat.le_of_lt previous.timerAfterObservation)
          (Nat.le_of_lt next.timerAfterObservation)
            receiverHeadAtObservation previousHead nextHead referenceAfterPrior
              previousDeadlineBound visibilityMargin previousStartsAfterGst
                activeFromObservation with advanced | evidence
  · exact False.elim (noAdvanceAtObservation advanced)
  · exact evidence

/-- A finite actual pairwise spread and its exact lower successor edge give
same-head adjacent parent evidence.

This theorem is the window-scoped composition boundary. It does not take a
global timer-gap source map. -/
theorem same_head_fresh_adjacent_productions_give_parent_evidence_from_spread_lower_and_linear_backlog
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {start observation author receiver round floor spread : Time}
    {prior : ValidatorCommitHead CommitId}
    (headsAtStart : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior)
    (startBeforeObservation : start ≤ observation)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1))
    (sourceFor : ∀ broadcast : ValidatorTimerPacedPeerBroadcast timed
      previous.production.snapshot author receiver
        previous.production.proposalActionAt,
      ValidatorTimerPacedLinearBacklogSyncSource admission previous.production
        broadcast (floor := floor))
    (referenceAfterPrior : prior.round < round)
    (previousSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations
      waits observation round spread)
    (lower : ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
      observation round (waits.wait prior round))
    (visibilityMargin :
      spread + 3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((round - floor) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        waits.wait prior (round + 1)) :
    ValidatorAdjacentTimerPacedParentEvidence
      previous.production next.production := by
  have previousDeadlineBound :
      previous.production.timerStartedAt + waits.wait prior round ≤
        next.production.timerStartedAt + spread :=
    fresh_timer_start_spread_and_successor_lower_gives_deadline_bound
      previousSpread lower previous next
  exact same_head_fresh_adjacent_productions_give_parent_evidence_from_linear_backlog_of_margin
    originRules acceptance sync representatives admission headsAtStart
      startBeforeObservation afterGst active noAdvance previous next sourceFor
        referenceAfterPrior previousDeadlineBound visibilityMargin

/-- Every sufficiently late actual adjacent pair has exact parent evidence
when its finite timer spread has the stated quadratic bound.

The theorem uses the exact lower successor edge. It uses the quadratic wait
value once. It does not use a global timer-gap source map, a complete-history
envelope, or a future window or layer result. -/
theorem late_same_head_fresh_adjacent_productions_give_parent_evidence_from_quadratic_spread_and_linear_backlog
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {maxAdmittedRefsPerRound : Nat}
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed parameters.schedule.commonSchedule}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {start observation cutoffRound : Time}
    {prior : ValidatorCommitHead CommitId}
    (headsAtStart : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior)
    (startBeforeObservation : start ≤ observation)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start)
    (spreadBase spreadLinear spreadQuadratic : Nat)
    (quadraticCondition :
      spreadQuadratic < parameters.quadraticCoefficient) :
    ∃ firstRound,
      prior.round < firstRound ∧
        ∀ {author receiver round}
          (previous : ValidatorFreshTimerPacedExactRoundProduction timed
            obligations parameters.schedule.commonSchedule observation author
              round)
          (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
            parameters.schedule.commonSchedule observation receiver (round + 1))
          (sourceFor : ∀ broadcast : ValidatorTimerPacedPeerBroadcast timed
            previous.production.snapshot author receiver
              previous.production.proposalActionAt,
            ValidatorTimerPacedLinearBacklogSyncSource admission
              previous.production broadcast (floor := cutoffRound))
          (previousSpread : ValidatorFreshRoundTimerStartSpreadAt timed
            obligations parameters.schedule.commonSchedule observation round
              (spreadBase + spreadLinear * (round - prior.round) +
                spreadQuadratic * (round - prior.round) *
                  (round - prior.round)))
          (lower : ValidatorFreshTimerStartSuccessorLowerAt timed obligations
            parameters.schedule.commonSchedule observation round
              (parameters.schedule.commonSchedule.wait prior round)),
          firstRound ≤ round →
            ValidatorAdjacentTimerPacedParentEvidence
              previous.production next.production := by
  let syncSlope := maxAdmittedRefsPerRound *
    validatorBlockSyncAcceptanceBound timed syncRules
  let fixedPipelineCost :=
    3 * (timed.localActionBound + 1) + network.delta + 1 +
      timed.localActionBound + 1
  rcases head_relative_quadratic_value_eventually_covers_quadratic_spread_and_cutoff_backlog
      parameters prior cutoffRound spreadBase spreadLinear spreadQuadratic
        syncSlope fixedPipelineCost quadraticCondition with
    ⟨marginStart, headBeforeMargin, margin⟩
  let firstRound := max marginStart (prior.round + 1)
  refine ⟨firstRound, Nat.lt_of_lt_of_le (Nat.lt_succ_self prior.round)
    (Nat.le_max_right _ _), ?_⟩
  intro author receiver round previous next sourceFor previousSpread lower
    firstBeforeRound
  have marginBeforeRound : marginStart ≤ round :=
    Nat.le_trans (Nat.le_max_left _ _) firstBeforeRound
  have headBeforeRound : prior.round < round := by
    have successorBeforeRound : prior.round + 1 ≤ round :=
      Nat.le_trans (Nat.le_max_right _ _) firstBeforeRound
    omega
  have covered := margin round marginBeforeRound
  have visibilityMargin :
      (spreadBase + spreadLinear * (round - prior.round) +
          spreadQuadratic * (round - prior.round) *
            (round - prior.round)) +
          3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((round - cutoffRound) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        parameters.schedule.commonSchedule.wait prior (round + 1) := by
    simp [syncSlope, fixedPipelineCost,
      ValidatorHeadRelativeGapWaitSchedule.commonSchedule,
      ValidatorHeadRelativeQuadraticWaitParameters.schedule_wait_eq,
      Nat.mul_assoc, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
        at covered ⊢
    omega
  exact same_head_fresh_adjacent_productions_give_parent_evidence_from_spread_lower_and_linear_backlog
    originRules acceptance sync representatives admission headsAtStart
      startBeforeObservation afterGst active noAdvance previous next sourceFor
        headBeforeRound previousSpread lower visibilityMargin

/-! Provisional source-local one-round causal resolution.

Do not use this section as a final protocol boundary. Its source map compares
the complete child capsule with one selected parent capsule. A child can merge
history from several parents. Therefore, one parent does not always contain
all old history. A final rule must compare the child with the union of all
parent cutoffs, or it must give a linear backlog bound for the actual block.
-/

/-- Provisional current and past source facts for one actual timer-paced block
delivery.

The selected prior parent is in the actual block. Its exact causal capsule is
already a receiver cutoff when synchronization starts. The target sync source
uses the source-local projected capsule. This record does not state that the
target block is accepted in the future. -/
structure ValidatorTimerPacedOneRoundNoveltySyncSource
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {newWorkPerRound : Nat}
    (delta : ValidatorPersistedCausalCapsuleRoundNoveltySourceMap
      (syncRules := syncRules) newWorkPerRound)
    {author receiver round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt) : Type where
  priorParent : ValidatorBlock BlockId
  priorParentIsParent :
    priorParent.reference ∈ production.snapshot.block.parents
  priorCutoff : ValidatorAcceptedCausalCapsuleCutoffAt timed
    (delta.capsuleFor priorParent) (broadcast.packet.deliveredAt + 1) receiver
  targetSyncSource : ValidatorBlockParentSyncSource syncRules
    production.snapshot.block receiver author
      (delta.capsuleFor production.snapshot.block).history
        (broadcast.packet.deliveredAt + 1)

/-- Provisional: one source-local causal delta resolves an actual timer-paced
block within one configured round of synchronization work.

The proof counts only entries which are new above the accepted prior-parent
capsule. It does not use `capsuleHistoryWithinEnvelope`. -/
theorem timer_paced_peer_broadcast_resolves_within_one_round_novelty
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {newWorkPerRound : Nat}
    (delta : ValidatorPersistedCausalCapsuleRoundNoveltySourceMap
      (syncRules := syncRules) newWorkPerRound)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    {author receiver round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt)
    (source : ValidatorTimerPacedOneRoundNoveltySyncSource delta production
      broadcast)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (active : ∀ time, broadcast.packet.deliveredAt + 1 ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ acceptedAt,
      broadcast.packet.deliveredAt + 1 ≤ acceptedAt ∧
        acceptedAt ≤ broadcast.packet.deliveredAt + 1 +
            newWorkPerRound *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1 ∧
        (((timed.execution.trace acceptedAt).validatorState receiver).accepted
            production.snapshot.block.reference = true ∨
          production.snapshot.block.reference.round ≤
            ((timed.execution.trace acceptedAt).validatorState
              receiver).gcRound) := by
  have authorInRange : author < config.authorityCount := by
    simpa [production.proposer] using production.snapshot.proposerInRange
  have authorCorrectAvailable : faults.correctAvailable author = true := by
    simpa [production.proposer] using
      production.snapshot.proposerCorrectAvailable
  have deliveryBounds := network.postGstDelivery broadcast.packet
    broadcast.packetIsProtocol
    (by simpa [broadcast.packetSender] using authorInRange)
    (by simpa [broadcast.packetReceiver] using receiverInRange)
    (by simpa [broadcast.packetSender] using authorCorrectAvailable)
    (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
    sentAfterGst
  have delivered := timed.execution.protocolPacketsAreDelivered
    broadcast.packetId broadcast.packet broadcast.packetInTrace
      broadcast.packetIsProtocol
      (by simpa [broadcast.packetSender] using authorInRange)
      (by simpa [broadcast.packetReceiver] using receiverInRange)
      (by simpa [broadcast.packetSender] using authorCorrectAvailable)
      (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
      sentAfterGst
  have packetAtDelivery := timed.execution.packetHistoryMonotone
    broadcast.packet.sentAt broadcast.packet.deliveredAt deliveryBounds.1
      broadcast.packetId broadcast.packet broadcast.packetInTrace
  have syncStartsAfterGst : network.gst ≤
      broadcast.packet.deliveredAt + 1 :=
    Nat.le_trans sentAfterGst
      (Nat.le_trans deliveryBounds.1 (Nat.le_add_right _ _))
  have adjacentRound : source.priorParent.reference.round + 1 =
      production.snapshot.block.reference.round :=
    production.validParents.2.1 source.priorParent.reference
      source.priorParentIsParent
  have targetSyncSource : ValidatorBlockParentSyncSource syncRules
      (delta.capsuleFor production.snapshot.block).targetBlock receiver author
        (delta.capsuleFor production.snapshot.block).history
          (broadcast.packet.deliveredAt + 1) := by
    simpa only [delta.capsuleTargetsBlock] using source.targetSyncSource
  rcases delta.adjacent_history_ready_within_delta targetSyncSource
      authorInRange authorCorrectAvailable production.persistenceOccurs
        source.priorParentIsParent adjacentRound source.priorCutoff
          receiverInRange receiverCorrectAvailable syncStartsAfterGst active with
    ⟨parentsReadyAt, deliveryBeforeParentsReady, parentsReadyBound,
      historyReady⟩
  have parentsReady : ∀ parent,
      parent ∈ production.snapshot.block.parents →
        ((timed.execution.trace parentsReadyAt).validatorState
            broadcast.packet.receiver).accepted parent = true ∨
          parent.round ≤
            ((timed.execution.trace parentsReadyAt).validatorState
              broadcast.packet.receiver).gcRound := by
    intro parent parentMember
    rw [broadcast.packetReceiver]
    rcases source.targetSyncSource.coversDirectParents parent parentMember with
      acceptedAtStart | ⟨parentBlock, blockMember, blockReference⟩
    · exact Or.inl (timed.execution.accepted_block_persists receiverInRange
        deliveryBeforeParentsReady acceptedAtStart)
    · simpa [ValidatorReferenceAcceptedOrGcRootAt, blockReference] using
        historyReady parentBlock blockMember
  by_cases blockAtRoot : production.snapshot.block.reference.round ≤
      ((timed.execution.trace parentsReadyAt).validatorState receiver).gcRound
  · refine ⟨parentsReadyAt, deliveryBeforeParentsReady, ?_, Or.inr blockAtRoot⟩
    exact Nat.le_trans parentsReadyBound (by
      simpa only [Nat.add_assoc] using Nat.le_add_right
        (broadcast.packet.deliveredAt + 1 +
          newWorkPerRound * validatorBlockSyncAcceptanceBound timed syncRules)
        (timed.localActionBound + 1))
  · have blockAboveGc :
        ((timed.execution.trace parentsReadyAt).validatorState
          broadcast.packet.receiver).gcRound <
            production.snapshot.block.reference.round := by
      rw [broadcast.packetReceiver]
      omega
    rcases delivered_block_with_current_gc_ready_parents_is_accepted timed
        acceptance packetAtDelivery broadcast.packetPayload delivered
          (by simpa [broadcast.packetReceiver] using receiverInRange)
          (by simpa [broadcast.packetReceiver] using receiverCorrectAvailable)
          (by simpa [production.snapshot.blockIsOwnProposal,
            production.proposer] using authorInRange)
          (Or.inr production.validParents) deliveryBeforeParentsReady
          blockAboveGc parentsReady with
      ⟨acceptedAt, parentsBeforeAccepted, acceptedWithinBound, accepted⟩
    refine ⟨acceptedAt,
      Nat.le_trans deliveryBeforeParentsReady parentsBeforeAccepted, ?_,
        Or.inl (by simpa [broadcast.packetReceiver] using accepted)⟩
    exact Nat.le_trans acceptedWithinBound
      (Nat.add_le_add_right parentsReadyBound (timed.localActionBound + 1))

/-- Provisional: one exact one-round novelty source gives adjacent parent
evidence, unless the receiver first installs a later commit.

This theorem uses the concrete source-local synchronization result above. It
does not use a complete-history size envelope. -/
theorem adjacent_timer_paced_productions_give_parent_evidence_from_one_round_novelty_or_commit_advance
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
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
    {newWorkPerRound : Nat}
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (delta : ValidatorPersistedCausalCapsuleRoundNoveltySourceMap
      (syncRules := syncRules) newWorkPerRound)
    {commonStart author receiver round startDifference : Nat}
    {previousPrior receiverPrior : ValidatorCommitHead CommitId}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    (next : ValidatorTimerPacedRoundProduction timed waits receiver (round + 1))
    (sourceFor : ∀ broadcast : ValidatorTimerPacedPeerBroadcast timed
      previous.snapshot author receiver previous.proposalActionAt,
        ValidatorTimerPacedOneRoundNoveltySyncSource delta previous broadcast)
    (commonStartBeforePrevious : commonStart ≤ previous.timerStartedAt)
    (commonStartBeforeNext : commonStart ≤ next.timerStartedAt)
    (receiverHeadAtStart :
      ((timed.execution.trace commonStart).validatorState receiver).commitHead =
        receiverPrior)
    (previousHead : previous.commitHead = previousPrior)
    (nextHead : next.commitHead = receiverPrior)
    (referenceAfterPrior : receiverPrior.round < round)
    (previousStartBound :
      previous.timerStartedAt ≤ next.timerStartedAt + startDifference)
    (visibilityMargin :
      waits.wait previousPrior round +
          (startDifference + 3 * (timed.localActionBound + 1) +
            network.delta +
            (1 + newWorkPerRound *
                validatorBlockSyncAcceptanceBound timed syncRules +
              timed.localActionBound + 1)) ≤
        waits.wait receiverPrior (round + 1))
    (previousStartsAfterGst : network.gst ≤ previous.timerStartedAt)
    (active : ∀ time, commonStart ≤ time →
      (timed.execution.trace time).epochActive = true) :
    SomeCorrectAvailableCommitAdvance timed commonStart ∨
      ValidatorAdjacentTimerPacedParentEvidence previous next := by
  by_cases advanced : SomeCorrectAvailableCommitAdvance timed commonStart
  · exact Or.inl advanced
  · right
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
    have headAt : ∀ time, commonStart ≤ time →
        ((timed.execution.trace time).validatorState receiver).commitHead =
          receiverPrior := by
      intro time ordered
      exact (no_commit_advance_keeps_correct_commit_head receiverInRange
        receiverCorrect ordered advanced).trans receiverHeadAtStart
    have gcBelowPrevious : ∀ time, commonStart ≤ time →
        ((timed.execution.trace time).validatorState receiver).gcRound <
          previous.snapshot.block.reference.round := by
      intro time ordered
      have gcAtMostHead :
          ((timed.execution.trace time).validatorState receiver).gcRound ≤
            ((timed.execution.trace time).validatorState
              receiver).commitHead.round :=
        correct_validator_gc_round_at_most_commit_round
          (time := time) timed.execution receiverInRange receiverCorrect
      rw [headAt time ordered] at gcAtMostHead
      rw [previous.blockRound]
      omega
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
      exact ValidatorCausalCapsuleCatchupRateRules.accepted_retained_timer_paced_block_gives_parent_evidence
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
        1 + newWorkPerRound *
            validatorBlockSyncAcceptanceBound timed syncRules +
          timed.localActionBound + 1
      have resolutionBeforeNextSnapshot :
          broadcast.packet.deliveredAt + resolutionCost ≤
            next.snapshot.snapshotAt := by
        exact adjacent_wait_margin_covers_timer_paced_resolution_cost
          previous next broadcast receiverInRange receiverCorrect sentAfterGst
            previousHead nextHead previousStartBound (by
              simpa only [resolutionCost] using visibilityMargin)
      rcases timer_paced_peer_broadcast_resolves_within_one_round_novelty
          delta acceptance previous broadcast (sourceFor broadcast)
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
          have belowAtNext := gcBelowPrevious next.snapshot.snapshotAt
            commonStartBeforeNextSnapshot
          omega
      have packetAtDelivery := timed.execution.packetHistoryMonotone
        broadcast.packet.sentAt broadcast.packet.deliveredAt deliveryFacts.1
          broadcast.packetId broadcast.packet broadcast.packetInTrace
      have localBody : ValidatorLocalBlockBodyAt timed
          broadcast.packet.deliveredAt receiver previous.snapshot.block :=
        .delivered broadcast.packetId broadcast.packet packetAtDelivery
          broadcast.packetIsProtocol broadcast.packetReceiver
          broadcast.packetPayload deliveryFacts.2.2
      have bodyPin := sync.local_body_creates_durable_pin receiverInRange
        receiverCorrect
        (active (broadcast.packet.deliveredAt + 1)
          (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1)))
        (gcBelowPrevious (broadcast.packet.deliveredAt + 1)
          (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1)))
        localBody
      have deliveryBeforeNextSnapshot : broadcast.packet.deliveredAt + 1 ≤
          next.snapshot.snapshotAt := by
        have oneBeforeResolution : 1 ≤ resolutionCost := by
          simp only [resolutionCost]
          omega
        exact Nat.le_trans
          (Nat.add_le_add_left oneBeforeResolution broadcast.packet.deliveredAt)
          resolutionBeforeNextSnapshot
      have pinAtNext := sync.body_pin_persists_while_head_is_current
        receiverInRange receiverCorrect deliveryBeforeNextSnapshot bodyPin.1
        (by
          intro time startBeforeTime _timeBeforeFinish
          exact active time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime))
        (by
          intro time startBeforeTime _timeBeforeFinish
          exact gcBelowPrevious time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime))
        (by
          intro time startBeforeTime _timeBeforeFinish
          have laterHead := headAt time (Nat.le_trans
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
            startBeforeTime)
          have initialHead := headAt (broadcast.packet.deliveredAt + 1)
            (Nat.le_trans commonStartBeforeDelivery (Nat.le_add_right _ 1))
          rw [laterHead, initialHead]
          exact Nat.le_refl _)
      have retainedAtNext := sync.acceptedPinnedBodyIsRetained
        next.snapshot.snapshotAt receiver previous.snapshot.block.reference _
          receiverInRange receiverCorrect pinAtNext
          (gcBelowPrevious next.snapshot.snapshotAt
            commonStartBeforeNextSnapshot) acceptedAtNext
      exact ValidatorCausalCapsuleCatchupRateRules.accepted_retained_timer_paced_block_gives_parent_evidence
        representatives previous next acceptedAtNext retainedAtNext

/-- Provisional: one explicit same-head margin and one-round novelty source
give exact adjacent parent evidence for two actual fresh productions.

The past timer-origin rule derives both production commit heads. The no-advance
suffix removes the receiver-local commit alternative. -/
theorem same_head_fresh_adjacent_productions_give_parent_evidence_from_one_round_novelty_of_margin
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
    {newWorkPerRound : Nat}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (delta : ValidatorPersistedCausalCapsuleRoundNoveltySourceMap
      (syncRules := syncRules) newWorkPerRound)
    {start observation author receiver round startDifference : Time}
    {prior : ValidatorCommitHead CommitId}
    (headsAtStart : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior)
    (startBeforeObservation : start ≤ observation)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1))
    (sourceFor : ∀ broadcast : ValidatorTimerPacedPeerBroadcast timed
      previous.production.snapshot author receiver
        previous.production.proposalActionAt,
      ValidatorTimerPacedOneRoundNoveltySyncSource delta previous.production
        broadcast)
    (referenceAfterPrior : prior.round < round)
    (previousStartBound : previous.production.timerStartedAt ≤
      next.production.timerStartedAt + startDifference)
    (visibilityMargin :
      waits.wait prior round +
          (startDifference + 3 * (timed.localActionBound + 1) +
            network.delta +
            (1 + newWorkPerRound *
                validatorBlockSyncAcceptanceBound timed syncRules +
              timed.localActionBound + 1)) ≤
        waits.wait prior (round + 1)) :
    ValidatorAdjacentTimerPacedParentEvidence
      previous.production next.production := by
  have authorInRange : author < config.authorityCount := by
    simpa [previous.production.proposer] using
      previous.production.snapshot.proposerInRange
  have authorCorrect : faults.correctAvailable author = true := by
    simpa [previous.production.proposer] using
      previous.production.snapshot.proposerCorrectAvailable
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerCorrectAvailable
  have startBeforePreviousTimer : start ≤
      previous.production.timerStartedAt :=
    Nat.le_trans startBeforeObservation
      (Nat.le_of_lt previous.timerAfterObservation)
  have startBeforeNextTimer : start ≤ next.production.timerStartedAt :=
    Nat.le_trans startBeforeObservation
      (Nat.le_of_lt next.timerAfterObservation)
  have previousHeadAtTimer :
      ((timed.execution.trace
        previous.production.timerStartedAt).validatorState author).commitHead =
          prior :=
    (no_commit_advance_keeps_correct_commit_head authorInRange authorCorrect
      startBeforePreviousTimer noAdvance).trans
        (headsAtStart author authorInRange authorCorrect)
  have nextHeadAtTimer :
      ((timed.execution.trace
        next.production.timerStartedAt).validatorState receiver).commitHead =
          prior :=
    (no_commit_advance_keeps_correct_commit_head receiverInRange receiverCorrect
      startBeforeNextTimer noAdvance).trans
        (headsAtStart receiver receiverInRange receiverCorrect)
  have previousHead : previous.production.commitHead = prior :=
    (originRules.production_head_at_timer_start previous.production).trans
      previousHeadAtTimer
  have nextHead : next.production.commitHead = prior :=
    (originRules.production_head_at_timer_start next.production).trans
      nextHeadAtTimer
  have receiverHeadAtObservation :
      ((timed.execution.trace observation).validatorState receiver).commitHead =
        prior :=
    (no_commit_advance_keeps_correct_commit_head receiverInRange receiverCorrect
      startBeforeObservation noAdvance).trans
        (headsAtStart receiver receiverInRange receiverCorrect)
  have previousStartsAfterGst : network.gst ≤
      previous.production.timerStartedAt :=
    Nat.le_trans afterGst startBeforePreviousTimer
  have activeFromObservation : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true := by
    intro time observationBeforeTime
    exact active time
      (Nat.le_trans startBeforeObservation observationBeforeTime)
  have noAdvanceAtObservation :
      ¬SomeCorrectAvailableCommitAdvance timed observation :=
    no_commit_advance_persists_to_later_start startBeforeObservation noAdvance
  rcases adjacent_timer_paced_productions_give_parent_evidence_from_one_round_novelty_or_commit_advance
      acceptance sync representatives delta previous.production next.production
        sourceFor (Nat.le_of_lt previous.timerAfterObservation)
          (Nat.le_of_lt next.timerAfterObservation)
            receiverHeadAtObservation previousHead nextHead referenceAfterPrior
              previousStartBound visibilityMargin previousStartsAfterGst
                activeFromObservation with advanced | evidence
  · exact False.elim (noAdvanceAtObservation advanced)
  · exact evidence

/-! Past-action source maps for the erased timer fields. -/

/-- Recover the local commit head which an actual timer-paced production used.

`ValidatorTimerPacedRoundProduction` keeps the head as data, but it does not
keep the equality between that data and the validator state at timer start.
This rule restores only that past equality. -/
structure ValidatorTimerPacedCommitHeadSourceMap
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)) : Prop where
  headAtTimerStart : ∀ {validator round}
    (production : ValidatorTimerPacedRoundProduction timed waits validator round),
    production.commitHead =
      ((timed.execution.trace production.timerStartedAt).validatorState
        validator).commitHead

/-- The exact past timer origin supplies the erased production-head mapping. -/
theorem ValidatorTimerPacedRecoveryOriginRules.toCommitHeadSourceMap
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
    {source : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits}
    (rules : ValidatorTimerPacedRecoveryOriginRules source) :
    ValidatorTimerPacedCommitHeadSourceMap timed waits := by
  exact { headAtTimerStart := rules.production_head_at_timer_start }

/-- Legacy scaffolding for a directional linear bound on adjacent fresh timer
starts.

The rule constrains timers which already occur in the trace. It does not state
that a future timer or proposal exists. Do not use this source map as a final
input. Use the finite actual-timer recurrence instead. -/
structure ValidatorFreshAdjacentTimerStartGapSourceMap
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
    (referenceRound : Nat) : Type where
  base : Nat
  slope : Nat
  adjacentStartBound : ∀ {observation author receiver round}
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1)),
    previous.production.timerStartedAt ≤
      next.production.timerStartedAt +
        (base + slope * (round - referenceRound))

/-- Legacy scaffolding: one explicit same-head timing margin gives exact
adjacent parent evidence for two fresh trace productions.

This theorem uses the complete-history catch-up envelope. The final theorem
uses the actual-block cutoff backlog. -/
theorem same_head_fresh_adjacent_productions_give_parent_evidence_of_margin
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
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    (headSource : ValidatorTimerPacedCommitHeadSourceMap timed waits)
    {start observation author receiver round startDifference : Time}
    {prior : ValidatorCommitHead CommitId}
    (headsAtStart : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior)
    (startBeforeObservation : start ≤ observation)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1))
    (referenceAfterPrior : prior.round < round)
    (envelopeStarted : rate.envelope.startRound ≤ round)
    (previousStartBound : previous.production.timerStartedAt ≤
      next.production.timerStartedAt + startDifference)
    (visibilityMargin :
      waits.wait prior round +
          (startDifference + 3 * (timed.localActionBound + 1) + network.delta +
            1 + rate.envelope.catchupCost
              (validatorBlockSyncAcceptanceBound timed syncRules) round +
            timed.localActionBound + 1) ≤
        waits.wait prior (round + 1)) :
    ValidatorAdjacentTimerPacedParentEvidence
      previous.production next.production := by
  have authorInRange : author < config.authorityCount := by
    simpa [previous.production.proposer] using
      previous.production.snapshot.proposerInRange
  have authorCorrect : faults.correctAvailable author = true := by
    simpa [previous.production.proposer] using
      previous.production.snapshot.proposerCorrectAvailable
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerCorrectAvailable
  have startBeforePreviousTimer : start ≤
      previous.production.timerStartedAt := by
    exact Nat.le_trans startBeforeObservation
      (Nat.le_of_lt previous.timerAfterObservation)
  have startBeforeNextTimer : start ≤ next.production.timerStartedAt := by
    exact Nat.le_trans startBeforeObservation
      (Nat.le_of_lt next.timerAfterObservation)
  have previousHeadAtTimer :
      ((timed.execution.trace
        previous.production.timerStartedAt).validatorState author).commitHead =
          prior :=
    (no_commit_advance_keeps_correct_commit_head authorInRange authorCorrect
      startBeforePreviousTimer noAdvance).trans
        (headsAtStart author authorInRange authorCorrect)
  have nextHeadAtTimer :
      ((timed.execution.trace
        next.production.timerStartedAt).validatorState receiver).commitHead =
          prior :=
    (no_commit_advance_keeps_correct_commit_head receiverInRange receiverCorrect
      startBeforeNextTimer noAdvance).trans
        (headsAtStart receiver receiverInRange receiverCorrect)
  have previousHead : previous.production.commitHead = prior :=
    (headSource.headAtTimerStart previous.production).trans previousHeadAtTimer
  have nextHead : next.production.commitHead = prior :=
    (headSource.headAtTimerStart next.production).trans nextHeadAtTimer
  have receiverHeadAtObservation :
      ((timed.execution.trace observation).validatorState receiver).commitHead =
        prior :=
    (no_commit_advance_keeps_correct_commit_head receiverInRange receiverCorrect
      startBeforeObservation noAdvance).trans
        (headsAtStart receiver receiverInRange receiverCorrect)
  have previousStartsAfterGst : network.gst ≤
      previous.production.timerStartedAt :=
    Nat.le_trans afterGst startBeforePreviousTimer
  have activeFromObservation : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true := by
    intro time observationBeforeTime
    exact active time
      (Nat.le_trans startBeforeObservation observationBeforeTime)
  have noAdvanceAtObservation :
      ¬SomeCorrectAvailableCommitAdvance timed observation :=
    no_commit_advance_persists_to_later_start startBeforeObservation noAdvance
  rcases rate.adjacent_timer_paced_productions_give_parent_evidence_from_catchup_margin_or_commit_advance
      pins frontiers sameGenesis acceptance sync representatives
        previous.production next.production
        (Nat.le_of_lt previous.timerAfterObservation)
        (Nat.le_of_lt next.timerAfterObservation)
        receiverHeadAtObservation previousHead nextHead referenceAfterPrior
        previousStartBound visibilityMargin previousStartsAfterGst
        activeFromObservation envelopeStarted with advanced | evidence
  · exact False.elim (noAdvanceAtObservation advanced)
  · exact evidence

/-- If no correct validator is ahead of the installed prior reference, all
correct heads are the exact prior head and remain so on the no-advance suffix.
-/
theorem installed_prior_without_ahead_gives_stable_exact_heads
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    {start : Time} {prior : ValidatorCommitHead CommitId}
    (priorInstalled : AllCorrectAvailableInstalledExactAt faults
      timed.execution.trace start prior)
    (noAhead : ¬∃ validator,
      validator < config.authorityCount ∧
        faults.correctAvailable validator = true ∧
        prior.index + 1 ≤
          (timed.execution.trace start).localCommitIndex validator)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start) :
    ∀ later validator,
      start ≤ later →
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace later).validatorState validator).commitHead =
        prior := by
  have headsAtStart : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior := by
    rcases all_correct_available_installed_prior_gives_collective_head_split
        prefixMap priorInstalled with ahead | equal
    · exact False.elim (noAhead ahead)
    · exact equal
  intro later validator startBeforeLater validatorInRange validatorCorrect
  exact (no_commit_advance_keeps_correct_commit_head validatorInRange
    validatorCorrect startBeforeLater noAdvance).trans
      (headsAtStart validator validatorInRange validatorCorrect)

/-! The late same-head adjacent edge. -/

/-- Legacy scaffolding: every sufficiently late pair of fresh adjacent
productions has exact leader-parent evidence while the prior head does not
advance.

The fresh productions are trace records. A higher theorem can derive them from
the existing finite-window theorem. Do not use this result as a final theorem.
It still uses the abstract start-gap map and the complete-history envelope. -/
theorem late_same_head_fresh_adjacent_productions_give_parent_evidence
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {canonicalGenesisParents : List (ValidatorBlockRef BlockId)}
    (frontiers : ValidatorOperationalQuorumFrontierSourceMap timed
      canonicalGenesisParents)
    (sameGenesis : pins.canonicalGenesisParents = canonicalGenesisParents)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (rate : ValidatorCausalCapsuleCatchupRateRules syncRules)
    (parameters : ValidatorHeadRelativeQuadraticWaitParameters)
    {start observation : Time} {prior : ValidatorCommitHead CommitId}
    (headSource : ValidatorTimerPacedCommitHeadSourceMap timed
      parameters.schedule.commonSchedule)
    (startLag : ValidatorFreshAdjacentTimerStartGapSourceMap timed obligations
      parameters.schedule.commonSchedule prior.round)
    (headsAtStart : AllCorrectAvailableCommitHeadsEqual faults
      timed.execution.trace start prior)
    (startBeforeObservation : start ≤ observation)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed start)
    (slopeCondition :
      rate.envelope.newWorkPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules + startLag.slope <
        2 * parameters.quadraticCoefficient) :
    ∃ firstRound,
      max prior.round rate.envelope.startRound < firstRound ∧
        ∀ {author receiver round}
          (previous : ValidatorFreshTimerPacedExactRoundProduction timed
            obligations parameters.schedule.commonSchedule observation author
              round)
          (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
            parameters.schedule.commonSchedule observation receiver (round + 1)),
          firstRound ≤ round →
            ValidatorAdjacentTimerPacedParentEvidence
              previous.production next.production := by
  let fixedPipelineCost :=
    3 * (timed.localActionBound + 1) + network.delta + 1 +
      timed.localActionBound + 1
  rcases head_relative_quadratic_eventually_covers_spread_and_catchup rate
      parameters prior startLag.base startLag.slope fixedPipelineCost
        slopeCondition with
    ⟨marginStart, headAndEnvelopeBeforeMargin, margin⟩
  let firstRound := marginStart + 1
  refine ⟨firstRound, Nat.lt_succ_of_le headAndEnvelopeBeforeMargin, ?_⟩
  intro author receiver round previous next firstBeforeRound
  have marginBeforeRound : marginStart ≤ round := by
    exact Nat.le_trans (Nat.le_succ marginStart) firstBeforeRound
  have headBeforeRound : prior.round < round := by
    have priorBeforeMargin : prior.round ≤ marginStart :=
      Nat.le_trans (Nat.le_max_left _ _) headAndEnvelopeBeforeMargin
    omega
  have envelopeBeforeRound : rate.envelope.startRound ≤ round := by
    exact Nat.le_trans
      (Nat.le_trans (Nat.le_max_right _ _) headAndEnvelopeBeforeMargin)
      marginBeforeRound
  let startDifference := startLag.base +
    startLag.slope * (round - prior.round)
  have previousStartBound : previous.production.timerStartedAt ≤
      next.production.timerStartedAt + startDifference := by
    exact startLag.adjacentStartBound previous next
  have visibilityMargin :
      parameters.schedule.commonSchedule.wait prior round +
          (startDifference + 3 * (timed.localActionBound + 1) + network.delta +
            1 + rate.envelope.catchupCost
              (validatorBlockSyncAcceptanceBound timed syncRules) round +
            timed.localActionBound + 1) ≤
        parameters.schedule.commonSchedule.wait prior (round + 1) := by
    have covered := margin round marginBeforeRound
    simpa [startDifference, fixedPipelineCost,
      ValidatorHeadRelativeGapWaitSchedule.commonSchedule,
      ValidatorHeadRelativeQuadraticWaitParameters.schedule_wait_eq,
      Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using covered
  exact same_head_fresh_adjacent_productions_give_parent_evidence_of_margin
    pins frontiers sameGenesis acceptance sync representatives rate headSource
      headsAtStart startBeforeObservation afterGst active noAdvance previous next
        headBeforeRound envelopeBeforeRound previousStartBound visibilityMargin

end Mysticeti
