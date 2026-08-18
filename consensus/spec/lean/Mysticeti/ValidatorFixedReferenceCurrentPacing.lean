/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.ValidatorConcreteSuccessorReadiness
import Mysticeti.ValidatorFixedReferenceNetworkCommitCapstone
import Mysticeti.ValidatorFreshWindowTimerSpread
import Mysticeti.ValidatorFreshRoundPinnedSyncSource
import Mysticeti.ValidatorOrderedHeadTimerLower
import Mysticeti.ValidatorV2RoundCatchup

namespace Mysticeti

/-!
Current-source fixed-reference pacing.

The public source rule below does not contain a future proposal, production
family, favorable window, direct range, Flex run, or commit. It fixes the wait
for each round independently of the local commit head. The proof derives a
finite initial timer spread from actual persisted blocks. It derives each next
timer bound from actual broadcasts, pinned synchronization, bounded local
work, and one receiver-local exact-next timer rule. There is no shared
parent-ready baseline or envelope in the public source rule.

This is the proposed `ASM-LIVE-ROUND-CATCHUP` pacing refinement. It is not
current Rust behavior. Current Rust also does not protect the exact first
selected leader from score-based parent exclusion. The timer-origin rule used
by the V2 adapter includes that proposed action-local snapshot behavior.
-/

/-- Static fixed-reference pacing and its action-local proposal gate.

The wait value is independent of the local commit head. An actual timer-paced
production already carries the local not-before gate through
`snapshotAtDeadline` and `deadlineBeforeProposal`. -/
structure ValidatorFixedReferenceStrictLocalPacingRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId))
    (parameters : ValidatorQuadraticGapWaitParameters) : Type where
  waitValue : ∀ head round,
    waits.wait head round = parameters.wait round

namespace ValidatorFixedReferenceStrictLocalPacingRules

/-- The local snapshot of one actual proposal is at its fixed-reference
deadline. This is the per-host not-before gate used below. -/
theorem snapshotAtFixedDeadline
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
    {parameters : ValidatorQuadraticGapWaitParameters}
    (rules : ValidatorFixedReferenceStrictLocalPacingRules timed waits
      parameters)
    {validator round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits validator
      round) :
    production.snapshot.snapshotAt =
      production.timerStartedAt + parameters.wait round := by
  rw [production.snapshotAtDeadline, rules.waitValue]

end ValidatorFixedReferenceStrictLocalPacingRules

/-- One actual fresh family has a finite initial timer-start spread without
commit-head alignment.

Any two proof records for one author and round have the same persistence time.
Each timer start is before that persistence. A finite maximum of the selected
persistence times therefore bounds every timer record in the family. -/
theorem fresh_family_has_finite_timer_start_spread_without_head_alignment
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
    {observation round : Nat}
    (family : EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed
      obligations waits observation round) :
    ∃ spread,
      ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
        round spread := by
  classical
  let persistFor := fun author =>
    if authorInRange : author < config.authorityCount then
      if authorCorrect : faults.correctAvailable author = true then
        (Classical.choice
          (family author authorInRange authorCorrect)).production.persistTime
      else
        0
    else
      0
  let spread := validatorTimerStartMaximumUpTo 0 persistFor
    config.authorityCount
  refine ⟨spread, ?_⟩
  intro left right leftProduction _rightProduction
  have leftInRange : left < config.authorityCount := by
    simpa [leftProduction.production.proposer] using
      leftProduction.production.snapshot.proposerInRange
  have leftCorrect : faults.correctAvailable left = true := by
    simpa [leftProduction.production.proposer] using
      leftProduction.production.snapshot.proposerCorrectAvailable
  let selected := Classical.choice (family left leftInRange leftCorrect)
  have sameRound :
      leftProduction.production.snapshot.block.reference.round =
        selected.production.snapshot.block.reference.round := by
    rw [leftProduction.production.blockRound, selected.production.blockRound]
  have samePersistence := persist_proposal_same_round_blocks_are_equal
    leftInRange leftProduction.production.persistenceOccurs
      selected.production.persistenceOccurs sameRound
  have timerBeforePersistence :
      leftProduction.production.timerStartedAt ≤
        leftProduction.production.persistTime := by
    exact Nat.le_trans
      (Nat.le_trans (Nat.le_add_right _ _)
        leftProduction.production.deadlineBeforeProposal)
      (Nat.le_trans (Nat.le_add_right _ 1)
        leftProduction.production.proposalBeforePersistence)
  have selectedPersistenceBound : selected.production.persistTime ≤ spread := by
    have bounded := validator_timer_start_le_maximum_up_to 0 persistFor
      leftInRange
    simpa [spread, persistFor, leftInRange, leftCorrect, selected] using bounded
  have leftBound : leftProduction.production.timerStartedAt ≤ spread := by
    rw [samePersistence.1] at timerBeforePersistence
    exact Nat.le_trans timerBeforePersistence selectedPersistenceBound
  exact Nat.le_trans leftBound (Nat.le_add_left spread _)

/-- The actual initial-parent lower edge becomes head independent when every
local head uses the same fixed-reference wait value. -/
theorem fresh_family_gives_fixed_reference_timer_start_successor_lower
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (ownership : ValidatorAuthenticatedAcceptedBodyOwnershipRules
      (timed := timed))
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {parameters : ValidatorQuadraticGapWaitParameters}
    (pacing : ValidatorFixedReferenceStrictLocalPacingRules timed waits
      parameters)
    {observation round : Nat}
    (previousFamily :
      EveryCorrectAvailableValidatorFreshTimerPacedExactRound timed obligations
        waits observation round) :
    ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits observation
      round (parameters.wait round) := by
  unfold ValidatorFreshTimerStartSuccessorLowerAt
  intro receiver next
  rcases
      fresh_family_gives_timer_start_successor_head_relative_lower timerSource
        ownership originRules previousFamily next with
    ⟨author, previous, lower⟩
  refine ⟨author, previous, ?_⟩
  rw [pacing.waitValue] at lower
  exact lower

/-- The per-round causal synchronization slope used by the strict
fixed-reference recurrence. -/
def validatorFixedReferenceTimerSyncSlope
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

/-- The fixed delivery, acceptance, and timer-arm cost in one strict
successor step. -/
def validatorFixedReferenceTimerFixedStepCost
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

/-- A zero-cutoff upper bound for one receiver's next exact timer. -/
def validatorFixedReferenceTimerStepCost
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
    (round maxAdmittedRefsPerRound : Nat) : Nat :=
  validatorFixedReferenceTimerFixedStepCost timed +
    round * validatorFixedReferenceTimerSyncSlope
      (syncRules := syncRules) timed maxAdmittedRefsPerRound

/-- Linear coefficient after the absolute round is split at the fixed
reference round. -/
def validatorFixedReferenceTimerSpreadLinearCoefficient
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
    (parameters : ValidatorQuadraticGapWaitParameters)
    (maxAdmittedRefsPerRound : Nat) : Nat :=
  validatorFixedReferenceTimerFixedStepCost timed +
    parameters.referenceRound *
      validatorFixedReferenceTimerSyncSlope
        (syncRules := syncRules) timed maxAdmittedRefsPerRound

/-- The zero-cutoff step cost is linear in distance from the fixed reference
round. -/
theorem validator_fixed_reference_timer_step_cost_le_gap_linear
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
    (parameters : ValidatorQuadraticGapWaitParameters)
    (round maxAdmittedRefsPerRound : Nat)
    (afterReference : parameters.referenceRound ≤ round) :
    validatorFixedReferenceTimerStepCost (syncRules := syncRules) timed round
        maxAdmittedRefsPerRound ≤
      validatorFixedReferenceTimerSpreadLinearCoefficient
          (syncRules := syncRules) timed parameters maxAdmittedRefsPerRound +
        validatorFixedReferenceTimerSyncSlope (syncRules := syncRules) timed
          maxAdmittedRefsPerRound * parameters.gap round := by
  have split : parameters.referenceRound + parameters.gap round = round := by
    simp only [ValidatorQuadraticGapWaitParameters.gap]
    omega
  unfold validatorFixedReferenceTimerStepCost
  unfold validatorFixedReferenceTimerSpreadLinearCoefficient
  let slope := validatorFixedReferenceTimerSyncSlope
    (syncRules := syncRules) timed maxAdmittedRefsPerRound
  calc
    validatorFixedReferenceTimerFixedStepCost timed + round * slope =
        validatorFixedReferenceTimerFixedStepCost timed +
          (parameters.referenceRound + parameters.gap round) * slope := by
      rw [split]
    _ = validatorFixedReferenceTimerFixedStepCost timed +
          parameters.referenceRound * slope +
            slope * parameters.gap round := by
      simp only [Nat.add_mul]
      ac_rfl
    _ ≤ validatorFixedReferenceTimerFixedStepCost timed +
          parameters.referenceRound *
              validatorFixedReferenceTimerSyncSlope
                (syncRules := syncRules) timed maxAdmittedRefsPerRound +
            validatorFixedReferenceTimerSyncSlope
                (syncRules := syncRules) timed maxAdmittedRefsPerRound *
              parameters.gap round := by
      dsimp [slope]
      exact Nat.le_refl _

/-- Quadratic timer spread derived over one already-actual finite family.

The record is a theorem result. Its fields are not public execution inputs. -/
structure ValidatorFixedReferenceWindowQuadraticTimerSpread
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
    (parameters : ValidatorQuadraticGapWaitParameters)
    (maxAdmittedRefsPerRound observation baseRound count : Nat) : Type where
  baseSpread : Nat
  spreadLinear : Nat
  spreadQuadratic : Nat
  spreadLinearEq : spreadLinear =
    validatorFixedReferenceTimerSpreadLinearCoefficient
      (syncRules := syncRules) timed parameters maxAdmittedRefsPerRound
  spreadQuadraticEq : spreadQuadratic =
    validatorFixedReferenceTimerSyncSlope
      (syncRules := syncRules) timed maxAdmittedRefsPerRound
  spreadAt : ∀ offset,
    offset < count →
    ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
      (baseRound + offset + 2)
        (baseSpread + spreadLinear *
            parameters.gap (baseRound + offset + 2) +
          spreadQuadratic * parameters.gap (baseRound + offset + 2) *
            parameters.gap (baseRound + offset + 2))
  lowerAt : ∀ offset,
    offset + 1 < count →
    ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits observation
      (baseRound + offset + 2)
        (parameters.wait (baseRound + offset + 2))

/-- Pointwise successor-upper facts from actual synchronization, plus actual
initial-parent lower facts, derive the finite quadratic spread. -/
def fresh_timer_paced_family_gives_fixed_reference_quadratic_timer_spread
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
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    (ownership : ValidatorAuthenticatedAcceptedBodyOwnershipRules
      (timed := timed))
    (originRules : ValidatorTimerPacedRecoveryOriginRules timerSource)
    {parameters : ValidatorQuadraticGapWaitParameters}
    (pacing : ValidatorFixedReferenceStrictLocalPacingRules timed waits
      parameters)
    {maxAdmittedRefsPerRound observation baseRound count : Nat}
    (freshFamily : ValidatorFreshTimerPacedExactRoundFamily timed obligations
      waits observation baseRound count)
    (baseAfterReference : parameters.referenceRound ≤ baseRound + 2)
    (baseSpread : Nat)
    (firstSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations waits
      observation (baseRound + 2) baseSpread)
    (upperAt : ∀ offset,
      offset + 1 < count →
      ValidatorFreshTimerStartSuccessorUpperAt timed obligations waits
        observation (baseRound + offset + 2)
          (parameters.wait (baseRound + offset + 2))
          (validatorFixedReferenceTimerStepCost (syncRules := syncRules) timed
            (baseRound + offset + 2) maxAdmittedRefsPerRound)) :
    ValidatorFixedReferenceWindowQuadraticTimerSpread
      (syncRules := syncRules) timed obligations waits parameters
        maxAdmittedRefsPerRound observation baseRound count := by
  let linear := validatorFixedReferenceTimerSpreadLinearCoefficient
    (syncRules := syncRules) timed parameters maxAdmittedRefsPerRound
  let quadratic := validatorFixedReferenceTimerSyncSlope
    (syncRules := syncRules) timed maxAdmittedRefsPerRound
  have lowerAt : ∀ offset,
      offset + 1 < count →
      ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
        observation (baseRound + offset + 2)
          (parameters.wait (baseRound + offset + 2)) := by
    intro offset edgeInRange
    exact fresh_family_gives_fixed_reference_timer_start_successor_lower
      ownership originRules pacing (freshFamily offset (by omega))
  let spreadBound := fun offset =>
    baseSpread + linear * parameters.gap (baseRound + offset + 2) +
      quadratic * parameters.gap (baseRound + offset + 2) *
        parameters.gap (baseRound + offset + 2)
  have spreadAt : ∀ offset,
      offset < count →
      ValidatorFreshRoundTimerStartSpreadAt timed obligations waits observation
        (baseRound + offset + 2) (spreadBound offset) := by
    intro offset
    induction offset with
    | zero =>
        intro _zeroInRange left right leftProduction rightProduction
        have firstBound := firstSpread leftProduction rightProduction
        have baseWithin : baseSpread ≤ spreadBound 0 := by
          dsimp [spreadBound]
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
        have rawSuccessor :
            ValidatorFreshRoundTimerStartSpreadAt timed obligations waits
              observation ((baseRound + previous + 2) + 1)
                (spreadBound previous +
                  validatorFixedReferenceTimerStepCost
                    (syncRules := syncRules) timed
                      (baseRound + previous + 2) maxAdmittedRefsPerRound) :=
          fresh_timer_start_pairwise_spread_successor previousSpread
            (upperAt previous successorInRange)
              (lowerAt previous successorInRange)
        let previousRound := baseRound + previous + 2
        let stepCost := validatorFixedReferenceTimerStepCost
          (syncRules := syncRules) timed previousRound maxAdmittedRefsPerRound
        have afterReference : parameters.referenceRound ≤ previousRound := by
          dsimp [previousRound]
          omega
        have stepBound : stepCost ≤ linear +
            quadratic * parameters.gap previousRound := by
          have bounded :=
            validator_fixed_reference_timer_step_cost_le_gap_linear
              (syncRules := syncRules) parameters previousRound
                maxAdmittedRefsPerRound afterReference
          simpa [stepCost, linear, quadratic] using bounded
        have nextGap : parameters.gap (previousRound + 1) =
            parameters.gap previousRound + 1 := by
          simp only [ValidatorQuadraticGapWaitParameters.gap]
          omega
        have nextRound : baseRound + (previous + 1) + 2 =
            previousRound + 1 := by
          dsimp [previousRound]
          omega
        have polynomialStep : spreadBound previous + stepCost ≤
            spreadBound (previous + 1) := by
          have bounded := validator_fresh_window_quadratic_spread_step
            baseSpread linear quadratic (parameters.gap previousRound)
              stepCost stepBound
          dsimp [spreadBound]
          rw [nextRound, nextGap]
          simpa [previousRound] using bounded
        intro left right leftProduction rightProduction
        have rawBound := rawSuccessor leftProduction rightProduction
        have shifted := Nat.add_le_add_left polynomialStep
          rightProduction.production.timerStartedAt
        exact Nat.le_trans (by
          simpa [previousRound, stepCost, Nat.add_assoc] using rawBound) shifted
  exact {
    baseSpread
    spreadLinear := linear
    spreadQuadratic := quadratic
    spreadLinearEq := rfl
    spreadQuadraticEq := rfl
    spreadAt := by
      intro offset offsetInRange left right leftProduction rightProduction
      exact spreadAt offset offsetInRange leftProduction rightProduction
    lowerAt }

/-- The current GC root supplies the round-zero cutoff, while the pinned-source
rules supply the exact protected synchronization source for one actual packet.
-/
def timer_paced_peer_broadcast_has_linear_backlog_source_at_zero
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
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (sourceRules : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    {author receiver round : Nat}
    (production : ValidatorTimerPacedRoundProduction timed waits author round)
    (broadcast : ValidatorTimerPacedPeerBroadcast timed production.snapshot
      author receiver production.proposalActionAt)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (sentAfterGst : network.gst ≤ broadcast.packet.sentAt)
    (active : ∀ time, production.persistTime + 1 ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ValidatorTimerPacedLinearBacklogSyncSource admission production broadcast
      (floor := 0) := by
  refine {
    cutoff := ?_
    targetSyncSource := sourceRules.sourceFor production broadcast
      receiverInRange receiverCorrect sentAfterGst active }
  intro block _member blockAtZero
  exact Or.inr (Nat.le_trans blockAtZero (Nat.zero_le _))

/-- A derived same-round spread and actual initial-parent lower edge give one
strict fixed-reference adjacent parent edge. -/
theorem fixed_reference_strict_fresh_adjacent_productions_give_parent_evidence
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
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (syncSources : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (pacing : ValidatorFixedReferenceStrictLocalPacingRules timed waits
      parameters)
    {observation author receiver round spread : Nat}
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (previous : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation author round)
    (next : ValidatorFreshTimerPacedExactRoundProduction timed obligations
      waits observation receiver (round + 1))
    (previousSpread : ValidatorFreshRoundTimerStartSpreadAt timed obligations
      waits observation round spread)
    (lower : ValidatorFreshTimerStartSuccessorLowerAt timed obligations waits
      observation round (parameters.wait round))
    (roundPositive : 0 < round)
    (visibilityMargin :
      spread + 3 * (timed.localActionBound + 1) + network.delta +
          (1 + (round * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        parameters.wait (round + 1)) :
    ValidatorAdjacentTimerPacedParentEvidence previous.production
      next.production := by
  have receiverInRange : receiver < config.authorityCount := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerInRange
  have receiverCorrect : faults.correctAvailable receiver = true := by
    simpa [next.production.proposer] using
      next.production.snapshot.proposerCorrectAvailable
  have previousStartsAfterGst : network.gst ≤
      previous.production.timerStartedAt :=
    Nat.le_trans afterGst (Nat.le_of_lt previous.timerAfterObservation)
  have fixedDeadlineBound :
      previous.production.timerStartedAt + parameters.wait round ≤
        next.production.timerStartedAt + spread :=
    fresh_timer_start_spread_and_successor_lower_gives_deadline_bound
      previousSpread lower previous next
  have previousDeadlineBound :
      previous.production.timerStartedAt +
          waits.wait previous.production.commitHead round ≤
        next.production.timerStartedAt + spread := by
    rw [pacing.waitValue]
    exact fixedDeadlineBound
  have actualVisibilityMargin :
      spread + 3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((round - 0) * maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        waits.wait next.production.commitHead (round + 1) := by
    rw [pacing.waitValue]
    simpa using visibilityMargin
  apply adjacent_timer_paced_productions_give_parent_evidence_commit_orthogonal
    acceptance representatives admission retention previous.production
      next.production ?_ (Nat.le_of_lt previous.timerAfterObservation)
        (Nat.le_of_lt next.timerAfterObservation) roundPositive
          previousDeadlineBound actualVisibilityMargin previousStartsAfterGst
            active
  intro broadcast
  have sentAfterGst : network.gst ≤ broadcast.packet.sentAt := by
    exact Nat.le_trans previousStartsAfterGst
      (Nat.le_trans (Nat.le_add_right _ _)
        (Nat.le_trans previous.production.deadlineBeforeProposal
          (Nat.le_trans (Nat.le_add_right _ 1)
            broadcast.proposalBeforeSend)))
  have observationBeforePersistence : observation ≤
      previous.production.persistTime + 1 := by
    exact Nat.le_trans (Nat.le_of_lt previous.timerAfterObservation)
      (Nat.le_trans (Nat.le_add_right _ _)
        (Nat.le_trans previous.production.deadlineBeforeProposal
          (Nat.le_trans (Nat.le_add_right _ 1)
            (Nat.le_trans previous.production.proposalBeforePersistence
              (Nat.le_add_right _ 1)))))
  exact timer_paced_peer_broadcast_has_linear_backlog_source_at_zero admission
    syncSources previous.production broadcast receiverInRange receiverCorrect
      sentAfterGst (by
        intro time persistenceBeforeTime
        exact active time
          (Nat.le_trans observationBeforePersistence persistenceBeforeTime))

/-- A derived strict quadratic spread gives both parent edges in one later
favorable slice of the actual long family. -/
theorem fixed_reference_strict_pacing_gives_aligned_family_parent_evidence
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
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (syncSources : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (pacing : ValidatorFixedReferenceStrictLocalPacingRules timed waits
      parameters)
    {observation baseRound count : Nat}
    (timerSpread : ValidatorFixedReferenceWindowQuadraticTimerSpread
      (syncRules := syncRules) timed obligations waits parameters
        maxAdmittedRefsPerRound observation baseRound count)
    (threshold : ValidatorFixedReferenceVisibilityThreshold parameters
      timerSpread.baseSpread timerSpread.spreadLinear
        timerSpread.spreadQuadratic
        (maxAdmittedRefsPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules)
        (3 * (timed.localActionBound + 1) + network.delta + 1 +
          timed.localActionBound + 1))
    {prior : ValidatorCommitHead CommitId}
    {shift leaderCount receiver : Nat}
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (withinWindow : shift + leaderCount + 1 < count)
    (thresholdBeforeWindow :
      threshold.firstRound ≤ baseRound + shift + 2)
    (aligned : ValidatorFixedReferenceAlignedFavorableFamily timed obligations
      waits prior observation (baseRound + shift) leaderCount) :
    ValidatorFixedReferenceAlignedAdjacentParentEvidence aligned receiver := by
  intro offset offsetInRange leader carrier voter _voterInRange _voterCorrect
    vote
  have leaderLongOffset : shift + offset < count := by omega
  have leaderEdgeInRange : shift + offset + 1 < count := by omega
  have voteLongOffset : shift + offset + 1 < count := by omega
  have voteEdgeInRange : shift + offset + 1 + 1 < count := by omega
  have leaderRoundAfterThreshold :
      threshold.firstRound ≤ baseRound + (shift + offset) + 2 := by
    omega
  have voteRoundAfterThreshold :
      threshold.firstRound ≤ baseRound + (shift + offset + 1) + 2 := by
    omega
  have leaderCovered := threshold.covers
    (baseRound + (shift + offset) + 2) leaderRoundAfterThreshold
  have voteCovered := threshold.covers
    (baseRound + (shift + offset + 1) + 2) voteRoundAfterThreshold
  have leaderVisibility :
      (timerSpread.baseSpread + timerSpread.spreadLinear *
            parameters.gap (baseRound + (shift + offset) + 2) +
          timerSpread.spreadQuadratic *
              parameters.gap (baseRound + (shift + offset) + 2) *
            parameters.gap (baseRound + (shift + offset) + 2)) +
          3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((baseRound + (shift + offset) + 2) *
                maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        parameters.wait ((baseRound + (shift + offset) + 2) + 1) := by
    simpa [Nat.mul_assoc, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
      leaderCovered
  have voteVisibility :
      (timerSpread.baseSpread + timerSpread.spreadLinear *
            parameters.gap (baseRound + (shift + offset + 1) + 2) +
          timerSpread.spreadQuadratic *
              parameters.gap (baseRound + (shift + offset + 1) + 2) *
            parameters.gap (baseRound + (shift + offset + 1) + 2)) +
          3 * (timed.localActionBound + 1) + network.delta +
          (1 + ((baseRound + (shift + offset + 1) + 2) *
                maxAdmittedRefsPerRound) *
              validatorBlockSyncAcceptanceBound timed syncRules +
            timed.localActionBound + 1) ≤
        parameters.wait ((baseRound + (shift + offset + 1) + 2) + 1) := by
    simpa [Nat.mul_assoc, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
      voteCovered
  constructor
  · apply fixed_reference_strict_fresh_adjacent_productions_give_parent_evidence
      (round := baseRound + shift + offset + 2)
      (spread := timerSpread.baseSpread + timerSpread.spreadLinear *
        parameters.gap (baseRound + shift + offset + 2) +
        timerSpread.spreadQuadratic *
          parameters.gap (baseRound + shift + offset + 2) *
            parameters.gap (baseRound + shift + offset + 2))
      acceptance representatives admission syncSources retention parameters
        pacing afterGst active leader vote
    · have roundEq : baseRound + (shift + offset) + 2 =
          baseRound + shift + offset + 2 := by omega
      rw [← roundEq]
      intro left right leftProduction rightProduction
      exact timerSpread.spreadAt (shift + offset) leaderLongOffset
        leftProduction rightProduction
    · have roundEq : baseRound + (shift + offset) + 2 =
          baseRound + shift + offset + 2 := by omega
      rw [← roundEq]
      intro receiver' next
      exact timerSpread.lowerAt (shift + offset) leaderEdgeInRange next
    · omega
    · have roundEq : baseRound + (shift + offset) + 2 =
          baseRound + shift + offset + 2 := by omega
      rw [← roundEq]
      exact leaderVisibility
  · apply fixed_reference_strict_fresh_adjacent_productions_give_parent_evidence
      (round := baseRound + shift + offset + 2 + 1)
      (spread := timerSpread.baseSpread + timerSpread.spreadLinear *
        parameters.gap (baseRound + shift + offset + 2 + 1) +
        timerSpread.spreadQuadratic *
          parameters.gap (baseRound + shift + offset + 2 + 1) *
            parameters.gap (baseRound + shift + offset + 2 + 1))
      acceptance representatives admission syncSources retention parameters
        pacing afterGst active vote carrier
    · have roundEq : baseRound + (shift + offset + 1) + 2 =
          baseRound + shift + offset + 2 + 1 := by omega
      rw [← roundEq]
      intro left right leftProduction rightProduction
      exact timerSpread.spreadAt (shift + offset + 1) voteLongOffset
        leftProduction rightProduction
    · have roundEq : baseRound + (shift + offset + 1) + 2 =
          baseRound + shift + offset + 2 + 1 := by omega
      rw [← roundEq]
      intro receiver' next
      exact timerSpread.lowerAt (shift + offset + 1) voteEdgeInRange next
    · omega
    · have roundEq : baseRound + (shift + offset + 1) + 2 =
          baseRound + shift + offset + 2 + 1 := by omega
      rw [← roundEq]
      exact voteVisibility

/-- One strict derived timer spread and one already-actual favorable family
give the receiver's direct range. -/
theorem fixed_reference_strict_pacing_and_aligned_family_give_receiver_direct_range
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
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    (acceptance : ValidatorParentReadyAcceptanceRules timed)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    (syncSources : ValidatorFreshRoundPinnedSyncSourceRules pins admission)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules timed)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (pacing : ValidatorFixedReferenceStrictLocalPacingRules timed waits
      parameters)
    {observation baseRound count : Nat}
    (timerSpread : ValidatorFixedReferenceWindowQuadraticTimerSpread
      (syncRules := syncRules) timed obligations waits parameters
        maxAdmittedRefsPerRound observation baseRound count)
    (threshold : ValidatorFixedReferenceVisibilityThreshold parameters
      timerSpread.baseSpread timerSpread.spreadLinear
        timerSpread.spreadQuadratic
        (maxAdmittedRefsPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules)
        (3 * (timed.localActionBound + 1) + network.delta + 1 +
          timed.localActionBound + 1))
    {start shift leaderCount receiver : Nat}
    {prior : ValidatorCommitHead CommitId}
    (startBeforeObservation : start ≤ observation)
    (headAtStart :
      ((timed.execution.trace start).validatorState receiver).commitHead = prior)
    (baseAfterPrior : prior.round < baseRound + shift + 2)
    (afterGst : network.gst ≤ observation)
    (active : ∀ time, observation ≤ time →
      (timed.execution.trace time).epochActive = true)
    (withinWindow : shift + leaderCount + 1 < count)
    (thresholdBeforeWindow :
      threshold.firstRound ≤ baseRound + shift + 2)
    (aligned : ValidatorFixedReferenceAlignedFavorableFamily timed obligations
      waits prior observation (baseRound + shift) leaderCount)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrect : faults.correctAvailable receiver = true)
    (leaderCountPositive : 0 < leaderCount) :
    Nonempty (ValidatorFixedReferenceFavorableDirectRange timed start receiver
      leaderCount prior) := by
  have adjacent :=
    fixed_reference_strict_pacing_gives_aligned_family_parent_evidence
      (receiver := receiver) acceptance representatives admission syncSources
        retention parameters pacing timerSpread threshold afterGst active
          withinWindow thresholdBeforeWindow aligned
  exact aligned_family_parent_evidence_gives_fixed_reference_direct_range
    startBeforeObservation headAtStart baseAfterPrior aligned receiverInRange
      receiverCorrect leaderCountPositive adjacent

/-- Strict fixed-reference sources which are local, static, current, or past.

The package has no shared round baseline, parent-ready envelope, future
production family, favorable window, commit, replay, or commit-silence fact.
`promptness` is the proposed one-host exact-next recovery rule. -/
structure ValidatorFixedReferenceStrictCurrentSourceMaps
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (maxAdmittedRefsPerRound : Nat) : Type where
  waitValue : ∀ head round,
    inputs.recoveryWait.wait head round = parameters.wait round
  promptness : ValidatorConcreteExactNextTimerPromptnessRules
    inputs.timedExecution inputs.proposalObligations inputs.recoveryWait
  ownership : ValidatorAuthenticatedAcceptedBodyOwnershipRules
    (timed := inputs.timedExecution)
  originRules : ValidatorTimerPacedRecoveryOriginRules
    inputs.recoveryTimerSource
  syncSlopeBelowWait :
    validatorFixedReferenceTimerSyncSlope
        (syncRules := inputs.blockSync) inputs.timedExecution
          maxAdmittedRefsPerRound <
      parameters.quadraticCoefficient

/-- The concrete successor cost equals the strict polynomial step cost. -/
theorem validator_concrete_successor_cost_eq_fixed_reference_step_cost
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
    (round maxAdmittedRefsPerRound : Nat) :
    validatorConcreteFreshRoundSuccessorCost
        (syncRules := syncRules) timed round maxAdmittedRefsPerRound =
      validatorFixedReferenceTimerStepCost
        (syncRules := syncRules) timed round maxAdmittedRefsPerRound := by
  unfold validatorConcreteFreshRoundSuccessorCost
  unfold validatorConcreteFreshRoundDeliveryCost
  unfold validatorConcreteFreshRoundResolutionCost
  unfold validatorFixedReferenceTimerStepCost
  unfold validatorFixedReferenceTimerFixedStepCost
  unfold validatorFixedReferenceTimerSyncSlope
  simp only [Nat.mul_assoc, Nat.mul_comm,
    Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]

/-- A one-round warm-up fixes the timer spread before the favorable suffix is
selected. A later V2 backfilled window then derives all successor edges from
actual broadcasts and pinned synchronization. -/
theorem strict_v2_backfill_and_favorable_path_give_fixed_reference_direct_range
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
    (syncSources : ValidatorFreshRoundPinnedSyncSourceRules
      inputs.recoverySourcePins admission)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules
      inputs.timedExecution)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (strict : ValidatorFixedReferenceStrictCurrentSourceMaps inputs parameters
      maxAdmittedRefsPerRound)
    (catchup : ValidatorV2RoundCatchupSourceMap
      inputs.recoveryTimerSource)
    (productionLiveness : BlockProductionLiveness config faults network
      inputs.timedExecution.execution.trace)
    {start firstFutureRound receiver : Nat}
    {prior : ValidatorCommitHead CommitId}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (inputs.timedExecution.execution.trace time).epochActive = true)
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
  let signerMaximum := ValidatorCorrectAvailableSignerFloorMaximum faults
    (timed.execution.trace start)
  let baseRound := max signerMaximum parameters.referenceRound
  have floorsBeforeBase : ∀ validator,
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound ≤ baseRound := by
    intro validator validatorInRange validatorCorrect
    have floorAtMostMaximum := correct_available_floor_le_ghost_maximum
      (world := timed.execution.trace start) validatorInRange validatorCorrect
    exact Nat.le_trans floorAtMostMaximum (Nat.le_max_left _ _)
  let warmup := Classical.choice
    (block_production_liveness_gives_backfilled_timer_paced_window catchup
      inputs.proposalLatch inputs.executionEffects.effects
        inputs.authorityCountAtLeastTwo productionLiveness afterGst active
          floorsBeforeBase (baseRound := baseRound) (count := 1))
  rcases fresh_family_has_finite_timer_start_spread_without_head_alignment
      (warmup.freshAt 0 (by omega)) with
    ⟨baseSpread, firstSpread⟩
  let spreadLinear := validatorFixedReferenceTimerSpreadLinearCoefficient
    (syncRules := inputs.blockSync) timed parameters maxAdmittedRefsPerRound
  let spreadQuadratic := validatorFixedReferenceTimerSyncSlope
    (syncRules := inputs.blockSync) timed maxAdmittedRefsPerRound
  let backlogSlope := maxAdmittedRefsPerRound *
    validatorBlockSyncAcceptanceBound timed inputs.blockSync
  let fixedPipelineCost :=
    3 * (timed.localActionBound + 1) + network.delta + 1 +
      timed.localActionBound + 1
  have spreadQuadraticBelowWait :
      spreadQuadratic < parameters.quadraticCoefficient := by
    simpa [spreadQuadratic] using strict.syncSlopeBelowWait
  let threshold := Classical.choice
    (fixed_reference_visibility_threshold parameters prior baseSpread
      spreadLinear spreadQuadratic backlogSlope fixedPipelineCost
        spreadQuadraticBelowWait)
  let requestedRound := max firstFutureRound
    (max threshold.firstRound (max (baseRound + 2) (prior.round + 1)))
  have firstFutureBeforeRequested : firstFutureRound ≤ requestedRound :=
    Nat.le_max_left _ _
  rcases path requestedRound firstFutureBeforeRequested with
    ⟨favorableBase, requestedBeforeFavorable, favorable⟩
  have thresholdBeforeFavorable : threshold.firstRound ≤ favorableBase := by
    exact Nat.le_trans
      (Nat.le_trans (Nat.le_max_left _ _)
        (Nat.le_max_right firstFutureRound _))
      requestedBeforeFavorable
  have baseBeforeFavorable : baseRound + 2 ≤ favorableBase := by
    have inner : baseRound + 2 ≤
        max (baseRound + 2) (prior.round + 1) := Nat.le_max_left _ _
    have beforeRequested : baseRound + 2 ≤ requestedRound := by
      exact Nat.le_trans
        (Nat.le_trans inner (Nat.le_max_right threshold.firstRound _))
        (Nat.le_max_right firstFutureRound _)
    exact Nat.le_trans beforeRequested requestedBeforeFavorable
  have priorBeforeFavorable : prior.round < favorableBase := by
    have inner : prior.round + 1 ≤
        max (baseRound + 2) (prior.round + 1) := Nat.le_max_right _ _
    have beforeRequested : prior.round + 1 ≤ requestedRound := by
      exact Nat.le_trans
        (Nat.le_trans inner (Nat.le_max_right threshold.firstRound _))
        (Nat.le_max_right firstFutureRound _)
    omega
  let shift := favorableBase - (baseRound + 2)
  have alignedBase : baseRound + shift + 2 = favorableBase := by
    dsimp [shift]
    omega
  let leaderCount := inputs.leaderSchedule.indirectDepth + 1
  let count := shift + leaderCount + 2
  let rawWindow := Classical.choice
    (block_production_liveness_gives_backfilled_timer_paced_window catchup
      inputs.proposalLatch inputs.executionEffects.effects
        inputs.authorityCountAtLeastTwo productionLiveness afterGst active
          floorsBeforeBase (baseRound := baseRound) (count := count))
  let pacing : ValidatorFixedReferenceStrictLocalPacingRules timed
      inputs.recoveryWait parameters := {
    waitValue := strict.waitValue }
  have upperAt : ∀ offset,
      offset + 1 < count →
      ValidatorFreshTimerStartSuccessorUpperAt timed
        inputs.proposalObligations inputs.recoveryWait start
          (baseRound + offset + 2)
          (parameters.wait (baseRound + offset + 2))
          (validatorFixedReferenceTimerStepCost
            (syncRules := inputs.blockSync) timed
              (baseRound + offset + 2) maxAdmittedRefsPerRound) := by
    intro offset edgeInRange
    unfold ValidatorFreshTimerStartSuccessorUpperAt
    intro receiver' next
    have concrete :=
      (fresh_timer_paced_exact_round_gives_concrete_timer_start_successor_upper
          (receiver := receiver') admission syncSources
            inputs.recoveryParentAcceptance.toValidatorParentReadyAcceptanceRules
              inputs.acceptedRepresentatives retention strict.promptness
                (rawWindow.freshAt offset (by omega))
                  (fun head => strict.waitValue head
                    (baseRound + offset + 2))
                  (by exact Nat.zero_lt_succ (baseRound + offset + 1))
                    afterGst active) next
    rw [validator_concrete_successor_cost_eq_fixed_reference_step_cost] at concrete
    exact concrete
  have baseAfterReference : parameters.referenceRound ≤ baseRound + 2 := by
    have referenceBeforeBase : parameters.referenceRound ≤ baseRound :=
      Nat.le_max_right _ _
    omega
  let timerSpread :=
    fresh_timer_paced_family_gives_fixed_reference_quadratic_timer_spread
      strict.ownership strict.originRules pacing rawWindow.toFreshFamily
        baseAfterReference baseSpread firstSpread upperAt
  have timerSpreadBase : timerSpread.baseSpread = baseSpread := by
    rfl
  let thresholdForSpread : ValidatorFixedReferenceVisibilityThreshold
      parameters timerSpread.baseSpread timerSpread.spreadLinear
        timerSpread.spreadQuadratic
        (maxAdmittedRefsPerRound *
          validatorBlockSyncAcceptanceBound timed inputs.blockSync)
        (3 * (timed.localActionBound + 1) + network.delta + 1 +
          timed.localActionBound + 1) := {
    firstRound := threshold.firstRound
    afterReference := threshold.afterReference
    covers := by
      intro round firstBefore
      rw [timerSpreadBase, timerSpread.spreadLinearEq,
        timerSpread.spreadQuadraticEq]
      simpa [backlogSlope, fixedPipelineCost, spreadLinear, spreadQuadratic]
        using threshold.covers round firstBefore }
  let leaderAuthorAt := fun offset =>
    if offsetInRange : offset < leaderCount then
      Classical.choose (favorable offset (by simpa [leaderCount] using
        offsetInRange))
    else
      0
  have leaderEvidence : ∀ offset,
      offset < leaderCount →
        leaderAuthorAt offset < config.authorityCount ∧
          (config.selectedLeaderOrder prior.id
            (baseRound + shift + 2 + offset)).head? =
              some (leaderAuthorAt offset) ∧
          faults.correctAvailable (leaderAuthorAt offset) = true := by
    intro offset offsetInRange
    have evidence := Classical.choose_spec
      (favorable offset (by simpa [leaderCount] using offsetInRange))
    simpa only [leaderAuthorAt, dif_pos offsetInRange, alignedBase] using
      evidence
  let aligned : ValidatorFixedReferenceAlignedFavorableFamily timed
      inputs.proposalObligations inputs.recoveryWait prior start
        (baseRound + shift) leaderCount := {
    family := by
      intro offset offsetInRange validator validatorInRange validatorCorrect
      have production := rawWindow.freshAt (shift + offset) (by
        dsimp [count]
        omega) validator validatorInRange validatorCorrect
      simpa [Nat.add_assoc] using production
    leaderAuthorAt
    leaderInRange := fun offset offsetInRange =>
      (leaderEvidence offset offsetInRange).1
    leaderCorrect := fun offset offsetInRange =>
      (leaderEvidence offset offsetInRange).2.2
    firstSelected := fun offset offsetInRange =>
      (leaderEvidence offset offsetInRange).2.1 }
  have thresholdBeforeWindow : thresholdForSpread.firstRound ≤
      baseRound + shift + 2 := by
    change threshold.firstRound ≤ baseRound + shift + 2
    rw [alignedBase]
    exact thresholdBeforeFavorable
  have baseAfterPrior : prior.round < baseRound + shift + 2 := by
    rwa [alignedBase]
  have withinWindow : shift + leaderCount + 1 < count := by
    dsimp [count]
    omega
  have result :=
    fixed_reference_strict_pacing_and_aligned_family_give_receiver_direct_range
      inputs.recoveryParentAcceptance.toValidatorParentReadyAcceptanceRules
        inputs.acceptedRepresentatives admission syncSources retention parameters
          pacing timerSpread thresholdForSpread (Nat.le_refl start) headAtStart
            baseAfterPrior afterGst active withinWindow thresholdBeforeWindow
              aligned receiverInRange receiverCorrect (by simp [leaderCount])
  simpa [leaderCount] using result

/-- The ideal favorable event gives the strict receiver-local disjunction.
The event is internal to the probability proof and is not an execution input.
-/
theorem favorable_event_and_strict_v2_backfill_give_derived_receiver_progress
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
    (syncSources : ValidatorFreshRoundPinnedSyncSourceRules
      (family.execution sampled).inputs.recoverySourcePins admission)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules
      (family.execution sampled).inputs.timedExecution)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (strict : ValidatorFixedReferenceStrictCurrentSourceMaps
      (family.execution sampled).inputs parameters maxAdmittedRefsPerRound)
    (catchup : ValidatorV2RoundCatchupSourceMap
      (family.execution sampled).inputs.recoveryTimerSource)
    (productionLiveness : BlockProductionLiveness
      (family.execution sampled).config (family.execution sampled).faults
        (family.execution sampled).network
          (family.execution sampled).inputs.timedExecution.execution.trace)
    (favorable :
      UniformRankingEndToEndExecutionFamily.AllValidatorCausalHeadFavorableWindows
        rankingSource sampled) :
    DerivedReceiverFixedReferenceProgress (family.execution sampled).inputs := by
  intro start receiver prior afterGst active _priorInstalled receiverInRange
    receiverCorrect headAtStart
  -- This split is source-independent. A verified synchronized install is
  -- progress in the left branch. In the right branch, no local commit install
  -- can repeatedly reset this receiver on the analyzed suffix.
  by_cases receiverAdvanced : ValidatorReceiverCommitAdvance
      (family.execution sampled).inputs.timedExecution start receiver
  · exact Or.inl receiverAdvanced
  · rcases
        UniformRankingEndToEndExecutionFamily.stable_execution_receiver_suffix_has_future_causal_head_path
          rankingSource sampled receiver receiverInRange start prior favorable
            headAtStart receiverAdvanced with
      ⟨firstFutureRound, _startBeforeBoundary, path⟩
    exact Or.inr
      (strict_v2_backfill_and_favorable_path_give_fixed_reference_direct_range
        (family.execution sampled).inputs admission syncSources retention
          parameters strict catchup productionLiveness afterGst active
            receiverInRange receiverCorrect headAtStart path)

/-- Current V2 source maps needed to derive semantic block-production
liveness. The package contains no future proposal, block, family, or commit.
-/
structure ValidatorV2BlockProductionCurrentSourceMaps
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    (inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network) where
  supports : ValidatorRecoverySelectedSupportExecution
    inputs.recoveryParentNeeds
  recursive : ValidatorRecoveryRecursiveParentNeedExecution inputs.blockSync
  queueSource : ValidatorSelectedSupportQueueSourceMap supports recursive
  noIdle : ValidatorGcAwareNoIdleSourceMap inputs.recoveryParentNeeds
    inputs.proposalObligations

namespace ValidatorV2BlockProductionCurrentSourceMaps

/-- The current V2 package derives the semantic liveness theorem internally.
-/
theorem blockProductionLiveness
    {BlockId CommitId PacketId Encoding : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {inputs : EndToEndLivenessInputs (PacketId := PacketId)
      (Encoding := Encoding) config faults protocolPacket network}
    (source : ValidatorV2BlockProductionCurrentSourceMaps inputs) :
    BlockProductionLiveness config faults network
      inputs.timedExecution.execution.trace := by
  exact ValidatorGcAwareNoIdleSourceMap.block_production_liveness
    source.queueSource source.noIdle inputs.authorLocalCommitContinuation
      inputs.recoveryProposalPacing inputs.proposalLatch
        inputs.executionEffects.effects inputs.authorityCountAtLeastTwo

end ValidatorV2BlockProductionCurrentSourceMaps

/-- Boundary-clean strict receiver progress. The source package contains only
static configuration, one-host action rules, and current or past provenance.
-/
theorem favorable_event_and_current_sources_give_derived_receiver_progress
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
    (syncSources : ValidatorFreshRoundPinnedSyncSourceRules
      (family.execution sampled).inputs.recoverySourcePins admission)
    (retention : ValidatorCommitOrthogonalAcceptedRetentionRules
      (family.execution sampled).inputs.timedExecution)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (strict : ValidatorFixedReferenceStrictCurrentSourceMaps
      (family.execution sampled).inputs parameters maxAdmittedRefsPerRound)
    (catchup : ValidatorV2RoundCatchupSourceMap
      (family.execution sampled).inputs.recoveryTimerSource)
    (blockSources : ValidatorV2BlockProductionCurrentSourceMaps
      (family.execution sampled).inputs)
    (favorable :
      UniformRankingEndToEndExecutionFamily.AllValidatorCausalHeadFavorableWindows
        rankingSource sampled) :
    DerivedReceiverFixedReferenceProgress (family.execution sampled).inputs := by
  exact favorable_event_and_strict_v2_backfill_give_derived_receiver_progress
    rankingSource sampled admission syncSources retention parameters strict
      catchup blockSources.blockProductionLiveness favorable

/-- The ideal ranking law derives strict receiver progress with probability
one. No future favorable window is a source-map field. -/
theorem current_sources_give_derived_receiver_progress_probability_one
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
    (syncSources : ∀ sampled, ValidatorFreshRoundPinnedSyncSourceRules
      (family.execution sampled).inputs.recoverySourcePins
        (admission sampled))
    (retention : ∀ sampled, ValidatorCommitOrthogonalAcceptedRetentionRules
      (family.execution sampled).inputs.timedExecution)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (strict : ∀ sampled, ValidatorFixedReferenceStrictCurrentSourceMaps
      (family.execution sampled).inputs parameters maxAdmittedRefsPerRound)
    (catchup : ∀ sampled, ValidatorV2RoundCatchupSourceMap
      (family.execution sampled).inputs.recoveryTimerSource)
    (blockSources : ∀ sampled, ValidatorV2BlockProductionCurrentSourceMaps
      (family.execution sampled).inputs) :
    law.probabilityOne (fun sampled =>
      DerivedReceiverFixedReferenceProgress
        (family.execution sampled).inputs) := by
  apply law.probabilityOneMono
    (UniformRankingEndToEndExecutionFamily.all_validator_causal_head_favorable_windows_probability_one
      law family rankingSource)
  intro sampled favorable
  exact favorable_event_and_current_sources_give_derived_receiver_progress
    rankingSource sampled (admission sampled) (syncSources sampled)
      (retention sampled) parameters (strict sampled) (catchup sampled)
        (blockSources sampled) favorable

/-- Strict receiver progress composes with the existing exact-prefix and
network DAG theorems to prove the end-to-end liveness goal. -/
theorem current_sources_give_end_to_end_liveness_probability_one
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
    (syncSources : ∀ sampled, ValidatorFreshRoundPinnedSyncSourceRules
      (family.execution sampled).inputs.recoverySourcePins
        (admission sampled))
    (retention : ∀ sampled, ValidatorCommitOrthogonalAcceptedRetentionRules
      (family.execution sampled).inputs.timedExecution)
    (parameters : ValidatorQuadraticGapWaitParameters)
    (strict : ∀ sampled, ValidatorFixedReferenceStrictCurrentSourceMaps
      (family.execution sampled).inputs parameters maxAdmittedRefsPerRound)
    (catchup : ∀ sampled, ValidatorV2RoundCatchupSourceMap
      (family.execution sampled).inputs.recoveryTimerSource)
    (blockSources : ∀ sampled, ValidatorV2BlockProductionCurrentSourceMaps
      (family.execution sampled).inputs) :
    law.probabilityOne (fun sampled =>
      EndToEndLivenessGoal (family.execution sampled).inputs) := by
  apply law.probabilityOneMono
    (current_sources_give_derived_receiver_progress_probability_one
      law family rankingSource admission syncSources retention parameters
        strict catchup blockSources)
  intro sampled progress
  exact derived_receiver_fixed_reference_progress_proves_end_to_end_goal
    (family.execution sampled).inputs progress

end Mysticeti
