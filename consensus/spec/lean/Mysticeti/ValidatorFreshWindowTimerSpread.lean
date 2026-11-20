/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorFreshWindowReceiverSync

namespace Mysticeti

/-! Finite-window timer spread from quantitative receiver synchronization.

Each fresh round has an actual latest timer. Receiver synchronization gives an
upper edge from that timer to every next-round timer. The initial-parent quorum
gives the matching lower edge. The receiver-relative backlog is linear in the
round gap, so the accumulated pairwise timer spread has a quadratic bound.

All window productions already occur in the trace. This module does not take a
future timer, layer, proposal, or commit result as an input.
-/

/-- The per-round block-sync slope in the receiver timer cost. -/
def validatorFreshWindowTimerSyncSlope
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
    (maxAdmittedRefsPerRound : Nat) : Nat :=
  maxAdmittedRefsPerRound *
    validatorBlockSyncAcceptanceBound timed syncRules

/-- The fixed part of one receiver synchronization and timer-arm step. -/
def validatorFreshWindowTimerFixedStepCost
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program) : Nat :=
  3 * (timed.localActionBound + 1) + network.delta + 1 +
    timed.localActionBound + 1 + timed.localActionBound + 2

/-- Linear coefficient for the quadratic spread bound. The constant GC offset
is charged here, while the changing head gap is charged by the quadratic
coefficient. -/
def validatorFreshWindowTimerSpreadLinearCoefficient
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
    (prior : ValidatorCommitHead CommitId)
    (maxAdmittedRefsPerRound : Nat) : Nat :=
  validatorFreshWindowTimerFixedStepCost timed +
    (prior.round - timed.execution.gcRoundForCommitHead prior) *
      validatorFreshWindowTimerSyncSlope
        (syncRules := syncRules) timed maxAdmittedRefsPerRound

/-- The concrete receiver step is linear in the target round's distance from
the fixed commit head. -/
theorem validator_fresh_round_receiver_timer_step_cost_le_head_gap_linear
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
    (prior : ValidatorCommitHead CommitId)
    (round maxAdmittedRefsPerRound : Nat)
    (priorBeforeRound : prior.round ≤ round) :
    validatorFreshRoundReceiverTimerStepCost (syncRules := syncRules)
        timed prior round maxAdmittedRefsPerRound ≤
      validatorFreshWindowTimerSpreadLinearCoefficient
          (syncRules := syncRules) timed prior maxAdmittedRefsPerRound +
        validatorFreshWindowTimerSyncSlope (syncRules := syncRules) timed
          maxAdmittedRefsPerRound * (round - prior.round) := by
  have gcBeforePrior := timed.execution.gcRoundForCommitHeadAtMostRound prior
  have gapSplit :
      round - timed.execution.gcRoundForCommitHead prior =
        (prior.round - timed.execution.gcRoundForCommitHead prior) +
          (round - prior.round) := by
    omega
  have backlogSplit :
      ((((prior.round - timed.execution.gcRoundForCommitHead prior) +
            (round - prior.round)) * maxAdmittedRefsPerRound) *
          validatorBlockSyncAcceptanceBound timed syncRules) =
        ((prior.round - timed.execution.gcRoundForCommitHead prior) *
            maxAdmittedRefsPerRound) *
            validatorBlockSyncAcceptanceBound timed syncRules +
          (maxAdmittedRefsPerRound *
            validatorBlockSyncAcceptanceBound timed syncRules) *
              (round - prior.round) := by
    simp [Nat.mul_add, Nat.mul_assoc, Nat.mul_comm, Nat.mul_left_comm]
  rw [validatorFreshRoundReceiverTimerStepCost, gapSplit, backlogSplit]
  unfold validatorFreshWindowTimerSpreadLinearCoefficient
  unfold validatorFreshWindowTimerFixedStepCost
  unfold validatorFreshWindowTimerSyncSlope
  simp only [Nat.mul_assoc, Nat.mul_comm, Nat.mul_left_comm,
    Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
  exact Nat.le_refl _

/-- Adding one linear receiver step stays inside the next value of the loose
quadratic spread envelope. -/
theorem validator_fresh_window_quadratic_spread_step
    (base linear quadratic gap stepCost : Nat)
    (stepBound : stepCost ≤ linear + quadratic * gap) :
    base + linear * gap + quadratic * gap * gap + stepCost ≤
      base + linear * (gap + 1) +
        quadratic * (gap + 1) * (gap + 1) := by
  have shifted := Nat.add_le_add_left stepBound
    (base + linear * gap + quadratic * gap * gap)
  apply Nat.le_trans shifted
  simp only [Nat.mul_add, Nat.add_mul]
  omega

/-- The quantitative facts for every offset in one actual finite fresh window.

The spread uses the same head-gap polynomial consumed by the same-head timing
theorem. `upperAt` and `lowerAt` expose the two current/past recurrence edges
which derive it. -/
structure ValidatorFreshWindowQuadraticTimerSpread
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (maxAdmittedRefsPerRound : Nat)
    (prior : ValidatorCommitHead CommitId)
    (observation baseRound count : Nat) : Type where
  baseSpread : Nat
  spreadLinear : Nat
  spreadQuadratic : Nat
  spreadLinearEq : spreadLinear =
    validatorFreshWindowTimerSpreadLinearCoefficient
      (syncRules := syncRules) timed prior maxAdmittedRefsPerRound
  spreadQuadraticEq : spreadQuadratic =
    validatorFreshWindowTimerSyncSlope
      (syncRules := syncRules) timed maxAdmittedRefsPerRound
  spreadAt : ∀ offset,
    offset < count →
    ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
      (baseRound + offset + 2)
        (baseSpread + spreadLinear *
            (baseRound + offset + 2 - prior.round) +
          spreadQuadratic * (baseRound + offset + 2 - prior.round) *
            (baseRound + offset + 2 - prior.round))
  upperAt : ∀ offset,
    offset + 1 < count →
    ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits observation
      (baseRound + offset + 2) (waits.wait prior (baseRound + offset + 2))
        (validatorFreshRoundReceiverTimerStepCost (syncRules := syncRules)
          timed prior (baseRound + offset + 2) maxAdmittedRefsPerRound)
  lowerAt : ∀ offset,
    offset + 1 < count →
    ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits observation
      (baseRound + offset + 2) (waits.wait prior (baseRound + offset + 2))

/-- The two adjacent timer facts needed at every index of one later finite
suffix of the long window. The long-window base spread and coefficients stay
unchanged. -/
structure ValidatorFreshWindowQuadraticTimerSpreadSlice
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (maxAdmittedRefsPerRound : Nat)
    (prior : ValidatorCommitHead CommitId)
    (observation baseRound count : Nat)
    (spread : ValidatorFreshWindowQuadraticTimerSpread
      (syncRules := syncRules) timed obligations waits
        maxAdmittedRefsPerRound prior observation baseRound count)
    (shift sliceCount : Nat) : Prop where
  spreadAt : ∀ offset,
    offset < sliceCount →
    ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
      (baseRound + (shift + offset) + 2)
        (spread.baseSpread + spread.spreadLinear *
            (baseRound + (shift + offset) + 2 - prior.round) +
          spread.spreadQuadratic *
              (baseRound + (shift + offset) + 2 - prior.round) *
            (baseRound + (shift + offset) + 2 - prior.round))
  lowerAt : ∀ offset,
    offset < sliceCount →
    ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits observation
      (baseRound + (shift + offset) + 2)
        (waits.wait prior (baseRound + (shift + offset) + 2))
  nextSpreadAt : ∀ offset,
    offset < sliceCount →
    ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
      (baseRound + (shift + offset + 1) + 2)
        (spread.baseSpread + spread.spreadLinear *
            (baseRound + (shift + offset + 1) + 2 - prior.round) +
          spread.spreadQuadratic *
              (baseRound + (shift + offset + 1) + 2 - prior.round) *
            (baseRound + (shift + offset + 1) + 2 - prior.round))
  nextLowerAt : ∀ offset,
    offset < sliceCount →
    ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits observation
      (baseRound + (shift + offset + 1) + 2)
        (waits.wait prior (baseRound + (shift + offset + 1) + 2))

/-- Restrict a long-window timer spread to a later finite suffix. The extra
two source rounds cover both adjacent edges at the last requested index. -/
theorem ValidatorFreshWindowQuadraticTimerSpread.slice
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
    {maxAdmittedRefsPerRound : Nat}
    {prior : ValidatorCommitHead CommitId}
    {observation baseRound count : Nat}
    (spread : ValidatorFreshWindowQuadraticTimerSpread
      (syncRules := syncRules) timed obligations waits
        maxAdmittedRefsPerRound prior observation baseRound count)
    (shift sliceCount : Nat)
    (withinWindow : shift + sliceCount + 1 < count) :
    ValidatorFreshWindowQuadraticTimerSpreadSlice
      (syncRules := syncRules) timed obligations waits
        maxAdmittedRefsPerRound prior observation baseRound count spread
          shift sliceCount := by
  constructor
  · intro offset offsetInRange
    have longOffsetInRange : shift + offset < count := by omega
    intro left right leftProduction rightProduction
    exact spread.spreadAt (shift + offset) longOffsetInRange
      leftProduction rightProduction
  · intro offset offsetInRange
    have longEdgeInRange : shift + offset + 1 < count := by omega
    unfold ValidatorFreshTimerStartSuccessorLowerAt
    intro receiver next
    exact spread.lowerAt (shift + offset) longEdgeInRange next
  · intro offset offsetInRange
    have longOffsetInRange : shift + offset + 1 < count := by omega
    intro left right leftProduction rightProduction
    exact spread.spreadAt (shift + offset + 1) longOffsetInRange
      leftProduction rightProduction
  · intro offset offsetInRange
    have longEdgeInRange : shift + offset + 1 + 1 < count := by omega
    unfold ValidatorFreshTimerStartSuccessorLowerAt
    intro receiver next
    exact spread.lowerAt (shift + offset + 1) longEdgeInRange next

/-- A fixed initial spread and an actual fresh window give the complete finite
quadratic timer-spread record.

A higher theorem must fix `baseSpread` for the first family in a window which
starts at the recovery base before it selects a favorable late suffix. This
order avoids a circular late threshold. -/
def fresh_timer_paced_window_gives_quadratic_timer_spread_from_fixed_base
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {maxAdmittedRefsPerRound recoveryWait : Nat}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (sync : ValidatorRecoveryCapsuleSyncExecution syncRules)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (ownership : ValidatorAuthenticatedAcceptedBodyOwnershipRules
      (timed := timed))
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {observation baseRound count : Nat}
    (freshWindow : ValidatorFreshTimerPacedExactRoundWindow timed obligations
      waits observation baseRound count)
    (baseSpread : Nat)
    (firstSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations waits
      observation (baseRound + 2) baseSpread)
    {prior : ValidatorCommitHead CommitId}
    (headsAtObservation : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace observation).validatorState
        validator).commitHead = prior)
    (priorBeforeFirstRound : prior.round < baseRound + 2)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (noAdvance : ¬SomeCorrectAvailableCommitAdvance timed observation) :
    ValidatorFreshWindowQuadraticTimerSpread
      (syncRules := syncRules) timed obligations waits
        maxAdmittedRefsPerRound prior observation baseRound count := by
  let linear := validatorFreshWindowTimerSpreadLinearCoefficient
    (syncRules := syncRules) timed prior maxAdmittedRefsPerRound
  let quadratic := validatorFreshWindowTimerSyncSlope
    (syncRules := syncRules) timed maxAdmittedRefsPerRound
  have productionHead : ∀ {author round}
      (production : ValidatorFreshTimerPacedExactRoundProduction timed
        obligations waits observation author round),
      production.production.commitHead = prior := by
    intro author round production
    exact originRules.fresh_production_head_on_suffix headsAtObservation
      (Nat.le_refl observation) noAdvance production
  have upperAt : ∀ offset,
      offset + 1 < count →
      ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
        observation (baseRound + offset + 2)
          (waits.wait prior (baseRound + offset + 2))
          (validatorFreshRoundReceiverTimerStepCost (syncRules := syncRules)
            timed prior (baseRound + offset + 2)
              maxAdmittedRefsPerRound) := by
    intro offset nextInRange
    unfold ValidatorFreshTimerStartSuccessorUpperAt
    intro receiver next
    have offsetInRange : offset < count := by omega
    have priorBeforeRound : prior.round < baseRound + offset + 2 := by omega
    exact (fresh_timer_paced_window_offset_gives_timer_start_successor_upper
      (recoveryWait := recoveryWait) admission pins sourceRules acceptance sync
        representatives arms originRules freshWindow offsetInRange
          headsAtObservation priorBeforeRound afterGst active noAdvance) next
  have lowerAt : ∀ offset,
      offset + 1 < count →
      ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
        observation (baseRound + offset + 2)
          (waits.wait prior (baseRound + offset + 2)) := by
    intro offset nextInRange
    unfold ValidatorFreshTimerStartSuccessorLowerAt
    intro receiver next
    have offsetInRange : offset < count := by omega
    exact (fresh_family_gives_timer_start_successor_lower timerSource ownership
      originRules (freshWindow.freshAt offset offsetInRange) (by
        intro author previous
        exact productionHead previous)) next
  let gapAt := fun offset => baseRound + offset + 2 - prior.round
  let spreadBound := fun offset =>
    baseSpread + linear * gapAt offset +
      quadratic * gapAt offset * gapAt offset
  have spreadAt : ∀ offset,
      offset < count →
      ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
        (baseRound + offset + 2) (spreadBound offset) := by
    intro offset
    induction offset with
    | zero =>
        intro _zeroInRange
        intro left right leftProduction rightProduction
        have firstBound := firstSpread leftProduction rightProduction
        have baseWithin : baseSpread ≤ spreadBound 0 := by
          dsimp [spreadBound, gapAt]
          omega
        exact Nat.le_trans firstBound
          (Nat.add_le_add_left baseWithin
            rightProduction.production.timerStartedAt)
    | succ previous inductionHypothesis =>
        intro successorInRange
        have previousInRange : previous < count := by omega
        have previousSpread :
            ValidatorFreshRoundTimerStartSpreadAt timed obligations waits
              observation (baseRound + previous + 2)
                (spreadBound previous) :=
          inductionHypothesis previousInRange
        have upper : ValidatorFreshTimerStartSuccessorUpperAt timed obligations
            waits observation (baseRound + previous + 2)
              (waits.wait prior (baseRound + previous + 2))
              (validatorFreshRoundReceiverTimerStepCost
                (syncRules := syncRules) timed prior
                  (baseRound + previous + 2) maxAdmittedRefsPerRound) :=
          upperAt previous successorInRange
        have lower : ValidatorFreshTimerStartSuccessorLowerAt timed obligations
            waits observation (baseRound + previous + 2)
              (waits.wait prior (baseRound + previous + 2)) :=
          lowerAt previous successorInRange
        have rawSuccessor :
            ValidatorFreshRoundTimerStartSpreadAt timed obligations waits
              observation ((baseRound + previous + 2) + 1)
                (spreadBound previous +
                  validatorFreshRoundReceiverTimerStepCost
                    (syncRules := syncRules) timed prior
                      (baseRound + previous + 2) maxAdmittedRefsPerRound) :=
          fresh_timer_start_pairwise_spread_successor
            previousSpread upper lower
        let previousRound := baseRound + previous + 2
        let stepCost := validatorFreshRoundReceiverTimerStepCost
          (syncRules := syncRules) timed prior previousRound
            maxAdmittedRefsPerRound
        have priorBeforePrevious : prior.round ≤ previousRound := by
          dsimp [previousRound]
          omega
        have stepBound : stepCost ≤ linear + quadratic * gapAt previous := by
          have bounded :=
            validator_fresh_round_receiver_timer_step_cost_le_head_gap_linear
              (syncRules := syncRules) prior previousRound
                maxAdmittedRefsPerRound priorBeforePrevious
          simpa [stepCost, previousRound, linear, quadratic, gapAt] using bounded
        have polynomialStep : spreadBound previous + stepCost ≤
            spreadBound (previous + 1) := by
          have bounded := validator_fresh_window_quadratic_spread_step
            baseSpread linear quadratic (gapAt previous) stepCost stepBound
          have nextGap : gapAt (previous + 1) = gapAt previous + 1 := by
            dsimp [gapAt]
            omega
          simpa [spreadBound, nextGap] using bounded
        intro left right leftProduction rightProduction
        have rawBound := rawSuccessor leftProduction rightProduction
        have shifted := Nat.add_le_add_left polynomialStep
          rightProduction.production.timerStartedAt
        exact Nat.le_trans (by
          simpa [previousRound, stepCost, Nat.add_assoc] using rawBound)
            shifted
  exact {
    baseSpread
    spreadLinear := linear
    spreadQuadratic := quadratic
    spreadLinearEq := rfl
    spreadQuadraticEq := rfl
    spreadAt := by
      intro offset offsetInRange
      intro left right leftProduction rightProduction
      exact spreadAt offset offsetInRange leftProduction rightProduction
    upperAt
    lowerAt }

end Mysticeti
