/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommitChain
import Mysticeti.PartialSynchrony

namespace Mysticeti

/-! Stage composition for strong leader liveness under partial synchrony and a safe
catch-up rule.

Applying these theorems to Rust requires proofs for the stage ledger. Key
obligations are
`ASM-LIVE-PARTIAL-SYNCHRONY`, `ASM-LIVE-ROUND-CATCHUP`,
`ASM-LIVE-BLOCK-SYNC`, and `ASM-LIVE-COMMIT-SYNC`.

These theorems do not yet derive block production from fundamental network and
single-validator rules. The safe intermediate-proposal rule is intended to give
liveness for old leader blocks. `CommitProgressRecovery.lean` contains separate
composition results for the weaker commit-index progress property.
-/

abbrev Trace (State : Type) := Time → State

def EventuallyFrom {State : Type} (trace : Trace State)
    (predicate : State → Prop) (start : Time) : Prop :=
  ∃ finish, start ≤ finish ∧ predicate (trace finish)

def LeadsToAfter {State : Type} (stableStart : Time) (trace : Trace State)
    (before after : State → Prop) : Prop :=
  ∀ start,
    stableStart ≤ start →
    before (trace start) →
    EventuallyFrom trace after start

def WithinAfter {State : Type} (stableStart bound : Time) (trace : Trace State)
    (before after : State → Prop) : Prop :=
  ∀ start,
    stableStart ≤ start →
    before (trace start) →
    ∃ finish,
      start ≤ finish ∧
      finish ≤ start + bound ∧
      after (trace finish)

namespace LeadsToAfter

theorem trans {State : Type} {stableStart : Time} {trace : Trace State}
    {first second third : State → Prop}
    (left : LeadsToAfter stableStart trace first second)
    (right : LeadsToAfter stableStart trace second third) :
    LeadsToAfter stableStart trace first third := by
  intro start afterStable firstNow
  rcases left start afterStable firstNow with ⟨middle, startMiddle, secondMiddle⟩
  have middleStable : stableStart ≤ middle :=
    Nat.le_trans afterStable startMiddle
  rcases right middle middleStable secondMiddle with ⟨finish, middleFinish, thirdFinish⟩
  exact ⟨finish, Nat.le_trans startMiddle middleFinish, thirdFinish⟩

end LeadsToAfter

namespace WithinAfter

theorem toLeadsTo {State : Type} {stableStart bound : Time} {trace : Trace State}
    {before after : State → Prop}
    (bounded : WithinAfter stableStart bound trace before after) :
    LeadsToAfter stableStart trace before after := by
  intro start afterStable beforeNow
  rcases bounded start afterStable beforeNow with ⟨finish, startFinish, _, afterFinish⟩
  exact ⟨finish, startFinish, afterFinish⟩

theorem trans {State : Type} {stableStart leftBound rightBound : Time}
    {trace : Trace State} {first second third : State → Prop}
    (left : WithinAfter stableStart leftBound trace first second)
    (right : WithinAfter stableStart rightBound trace second third) :
    WithinAfter stableStart (leftBound + rightBound) trace first third := by
  intro start afterStable firstNow
  rcases left start afterStable firstNow with
    ⟨middle, startMiddle, middleBound, secondMiddle⟩
  have middleStable : stableStart ≤ middle :=
    Nat.le_trans afterStable startMiddle
  rcases right middle middleStable secondMiddle with
    ⟨finish, middleFinish, finishBound, thirdFinish⟩
  have startFinish : start ≤ finish := Nat.le_trans startMiddle middleFinish
  have finishCombined : finish ≤ start + (leftBound + rightBound) := by
    calc
      finish ≤ middle + rightBound := finishBound
      _ ≤ (start + leftBound) + rightBound :=
        Nat.add_le_add_right middleBound rightBound
      _ = start + (leftBound + rightBound) := by simp [Nat.add_assoc]
  exact ⟨finish, startFinish, finishCombined, thirdFinish⟩

end WithinAfter

/-- These predicates are the liveness checkpoints for one selected leader slot. -/
structure ConsensusPhases (State : Type) where
  roundOpen : State → Prop
  goodLeaderWindow : State → Prop
  leaderProposed : State → Prop
  leaderDelivered : State → Prop
  supportersProduced : State → Prop
  supportersDelivered : State → Prop
  certificatesProduced : State → Prop
  certificatesDelivered : State → Prop
  leaderDecided : State → Prop
  commitProduced : State → Prop

/-- GST and catch-up activation must both have passed. -/
def stableStart {protocolPacket : Packet → Prop}
    (network : PartialSynchrony protocolPacket) (catchupActivation : Time) : Time :=
  max network.gst catchupActivation

/-- Every post-activation round transition creates each required intermediate block. -/
def SafeRoundChanges {State : Type} (roundState : State → RoundState)
    (trace : Trace State) (after : Time) : Prop :=
  ∀ time,
    after ≤ time →
    SafeIntermediateProposals
      (roundState (trace time))
      (roundState (trace (time + 1)))
      (roundState (trace (time + 1))).currentRound

/-- Temporary stage obligations from the Rust protocol to the liveness checkpoints.
These fields are derived theorem goals, not primitive environment assumptions.
`ASM-LIVE-PIPELINE-BOUNDS` records the common timing abstraction. -/
structure ConsensusLivenessStageObligations
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (catchupActivation : Time)
    (roundState : State → RoundState)
    (phases : ConsensusPhases State) where
  /-- `ASM-LIVE-ROUND-CATCHUP`. -/
  safeRoundChanges :
    SafeRoundChanges roundState trace (stableStart network catchupActivation)
  /-- Abstract good-window stage. Static leader viability and a probability or
  deterministic coverage rule must derive this result. -/
  fairRoundLeaderSelection :
    SafeRoundChanges roundState trace (stableStart network catchupActivation) →
    LeadsToAfter (stableStart network catchupActivation) trace
      phases.roundOpen phases.goodLeaderWindow
  /-- Local proposal progress uses `ASM-LIVE-TASK-FAIRNESS` and
  `ASM-LIVE-PIPELINE-BOUNDS`. -/
  windowToProposal :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.goodLeaderWindow phases.leaderProposed
  /-- Proposal delivery uses `ASM-LIVE-PARTIAL-SYNCHRONY` and
  `ASM-LIVE-PIPELINE-BOUNDS`. -/
  proposalToDelivery :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.leaderProposed phases.leaderDelivered
  /-- Supporter production can require missing ancestors. The hidden obligations are
  `ASM-LIVE-BLOCK-SYNC`, `ASM-LIVE-PEER-FAIRNESS`,
  `ASM-LIVE-TASK-FAIRNESS`, and `ASM-LIVE-PIPELINE-BOUNDS`. -/
  deliveryToSupporters :
    WithinAfter (stableStart network catchupActivation) (2 * network.delta) trace
      phases.leaderDelivered phases.supportersProduced
  /-- Supporter delivery uses `ASM-LIVE-PARTIAL-SYNCHRONY` and
  `ASM-LIVE-PIPELINE-BOUNDS`. -/
  supporterDelivery :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.supportersProduced phases.supportersDelivered
  /-- Certificate production can require missing ancestors. The hidden obligations
  are `ASM-LIVE-BLOCK-SYNC`, `ASM-LIVE-TASK-FAIRNESS`, and
  `ASM-LIVE-PIPELINE-BOUNDS`. -/
  deliveryToCertificates :
    WithinAfter (stableStart network catchupActivation) (2 * network.delta) trace
      phases.supportersDelivered phases.certificatesProduced
  /-- Certificate delivery uses `ASM-LIVE-PARTIAL-SYNCHRONY` and
  `ASM-LIVE-PIPELINE-BOUNDS`. -/
  certificateDelivery :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.certificatesProduced phases.certificatesDelivered
  /-- Local decision progress uses `ASM-LIVE-TASK-FAIRNESS` and
  `ASM-LIVE-PIPELINE-BOUNDS`. -/
  deliveryToDecision :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.certificatesDelivered phases.leaderDecided
  /-- Commit production uses `ASM-LIVE-TASK-FAIRNESS` and
  `ASM-LIVE-PIPELINE-BOUNDS`. -/
  decisionToCommit :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.leaderDecided phases.commitProduced

/-- Compose the supplied timed stages after a good leader window starts. -/
theorem good_window_stage_composition_within
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {catchupActivation : Time}
    {roundState : State → RoundState}
    {phases : ConsensusPhases State}
    (assumptions : ConsensusLivenessStageObligations trace network
      catchupActivation roundState phases) :
    WithinAfter (stableStart network catchupActivation) (10 * network.delta) trace
      phases.goodLeaderWindow phases.commitProduced := by
  have allSteps := assumptions.windowToProposal
    |>.trans assumptions.proposalToDelivery
    |>.trans assumptions.deliveryToSupporters
    |>.trans assumptions.supporterDelivery
    |>.trans assumptions.deliveryToCertificates
    |>.trans assumptions.certificateDelivery
    |>.trans assumptions.deliveryToDecision
    |>.trans assumptions.decisionToCommit
  have boundEquality :
      (((((((network.delta + network.delta) + 2 * network.delta) + network.delta) +
          2 * network.delta) + network.delta) + network.delta) + network.delta) =
        10 * network.delta := by
    omega
  rw [← boundEquality]
  exact allSteps

/-- Compose the supplied consensus stages after GST and catch-up activation.

This is not the final block-production theorem boundary. The caller still supplies
distributed proposal, certificate, decision, and commit stages. -/
theorem consensus_liveness_stage_composition
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {catchupActivation : Time}
    {roundState : State → RoundState}
    {phases : ConsensusPhases State}
    (assumptions : ConsensusLivenessStageObligations trace network
      catchupActivation roundState phases) :
    LeadsToAfter (stableStart network catchupActivation) trace
      phases.roundOpen phases.commitProduced := by
  have leaderOpportunity :=
    assumptions.fairRoundLeaderSelection assumptions.safeRoundChanges
  have boundedCommit :=
    (good_window_stage_composition_within assumptions).toLeadsTo
  exact leaderOpportunity.trans boundedCommit

/-- Liveness checkpoints for one transaction that is pending in the v3 finalizer. -/
structure FinalizerPhases (State : Type) where
  pending : State → Prop
  depthTwoTrigger : State → Prop
  decided : State → Prop
  durableOutput : State → Prop

/-- Temporary stage obligations for finalizer progress on a continuous commit
stream. These fields are derived theorem goals, not primitive environment
assumptions. -/
structure FinalizerLivenessStageObligations
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (catchupActivation : Time)
    (commitStream : CommitStream)
    (phases : FinalizerPhases State) where
  /-- `ASM-SAFE-COMMIT-CHAIN` and `ASM-LIVE-COMMIT-SYNC`. -/
  continuousCommitStream : Continuous commitStream
  /-- `ASM-LIVE-COMMIT-SYNC`, `ASM-LIVE-FINALIZER-TRIGGER`, and
  `ASM-LIVE-TASK-FAIRNESS`. -/
  triggerEventually :
    Continuous commitStream →
    LeadsToAfter (stableStart network catchupActivation) trace
      phases.pending phases.depthTwoTrigger
  /-- `ASM-LIVE-DURABILITY` and `ASM-LIVE-PIPELINE-BOUNDS`. -/
  triggerToDecision :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.depthTwoTrigger phases.decided
  /-- `ASM-LIVE-DURABILITY` and `ASM-LIVE-PIPELINE-BOUNDS`. -/
  decisionToDurableOutput :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.decided phases.durableOutput

/-- Compose the supplied finalizer stages into a durable-output result. -/
theorem finalizer_liveness_stage_composition
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {catchupActivation : Time}
    {commitStream : CommitStream}
    {phases : FinalizerPhases State}
    (assumptions : FinalizerLivenessStageObligations trace network
      catchupActivation commitStream phases) :
    LeadsToAfter (stableStart network catchupActivation) trace
      phases.pending phases.durableOutput := by
  have trigger :=
    assumptions.triggerEventually assumptions.continuousCommitStream
  have decide := assumptions.triggerToDecision.toLeadsTo
  have output := assumptions.decisionToDurableOutput.toLeadsTo
  exact (trigger.trans decide).trans output

/-- Compose the supplied consensus and finalizer stages for one target transaction.
This is not an end-to-end theorem from fundamental inputs. -/
theorem transaction_liveness_stage_composition
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {catchupActivation : Time}
    {roundState : State → RoundState}
    {consensusPhases : ConsensusPhases State}
    {commitStream : CommitStream}
    {finalizerPhases : FinalizerPhases State}
    (consensus : ConsensusLivenessStageObligations trace network
      catchupActivation roundState consensusPhases)
    (finalizer : FinalizerLivenessStageObligations trace network
      catchupActivation commitStream finalizerPhases)
    (commitEntersFinalizer :
      WithinAfter (stableStart network catchupActivation) network.delta trace
        consensusPhases.commitProduced finalizerPhases.pending) :
    LeadsToAfter (stableStart network catchupActivation) trace
      consensusPhases.roundOpen finalizerPhases.durableOutput := by
  have committed := consensus_liveness_stage_composition consensus
  have pending := commitEntersFinalizer.toLeadsTo
  have output := finalizer_liveness_stage_composition finalizer
  exact (committed.trans pending).trans output

end Mysticeti
