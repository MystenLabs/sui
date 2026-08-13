/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommitChain
import Mysticeti.PartialSynchrony

namespace Mysticeti

/-! Conditional liveness under partial synchrony and a safe catch-up rule. -/

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

/-- These predicates are the liveness checkpoints for one target leader opportunity. -/
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

/-- Refinement obligations from the Rust protocol to the liveness checkpoints. -/
structure ConsensusLivenessAssumptions
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (catchupActivation : Time)
    (roundState : State → RoundState)
    (phases : ConsensusPhases State) where
  safeRoundChanges :
    SafeRoundChanges roundState trace (stableStart network catchupActivation)
  /-- The selected leader set eventually has a live correct leader opportunity. -/
  fairLiveLeader :
    SafeRoundChanges roundState trace (stableStart network catchupActivation) →
    LeadsToAfter (stableStart network catchupActivation) trace
      phases.roundOpen phases.goodLeaderWindow
  windowToProposal :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.goodLeaderWindow phases.leaderProposed
  proposalToDelivery :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.leaderProposed phases.leaderDelivered
  deliveryToSupporters :
    WithinAfter (stableStart network catchupActivation) (2 * network.delta) trace
      phases.leaderDelivered phases.supportersProduced
  supporterDelivery :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.supportersProduced phases.supportersDelivered
  deliveryToCertificates :
    WithinAfter (stableStart network catchupActivation) (2 * network.delta) trace
      phases.supportersDelivered phases.certificatesProduced
  certificateDelivery :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.certificatesProduced phases.certificatesDelivered
  deliveryToDecision :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.certificatesDelivered phases.leaderDecided
  decisionToCommit :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.leaderDecided phases.commitProduced

/-- After a good leader window starts, the network part completes within `10 * delta`. -/
theorem good_window_commits_within
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {catchupActivation : Time}
    {roundState : State → RoundState}
    {phases : ConsensusPhases State}
    (assumptions : ConsensusLivenessAssumptions trace network
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

/-- Strong leader liveness after both GST and catch-up activation. -/
theorem consensus_liveness
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {catchupActivation : Time}
    {roundState : State → RoundState}
    {phases : ConsensusPhases State}
    (assumptions : ConsensusLivenessAssumptions trace network
      catchupActivation roundState phases) :
    LeadsToAfter (stableStart network catchupActivation) trace
      phases.roundOpen phases.commitProduced := by
  have leaderOpportunity :=
    assumptions.fairLiveLeader assumptions.safeRoundChanges
  have boundedCommit := (good_window_commits_within assumptions).toLeadsTo
  exact leaderOpportunity.trans boundedCommit

/-- Liveness checkpoints for one transaction that is pending in the v3 finalizer. -/
structure FinalizerPhases (State : Type) where
  pending : State → Prop
  depthTwoTrigger : State → Prop
  decided : State → Prop
  durableOutput : State → Prop

/-- Refinement obligations for finalizer progress on a continuous commit stream. -/
structure FinalizerLivenessAssumptions
    {State : Type} {protocolPacket : Packet → Prop}
    (trace : Trace State)
    (network : PartialSynchrony protocolPacket)
    (catchupActivation : Time)
    (commitStream : CommitStream)
    (phases : FinalizerPhases State) where
  continuousCommitStream : Continuous commitStream
  triggerEventually :
    Continuous commitStream →
    LeadsToAfter (stableStart network catchupActivation) trace
      phases.pending phases.depthTwoTrigger
  triggerToDecision :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.depthTwoTrigger phases.decided
  decisionToDurableOutput :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.decided phases.durableOutput

/-- Every pending transaction eventually gets a durable accept-or-reject output. -/
theorem finalizer_liveness
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {catchupActivation : Time}
    {commitStream : CommitStream}
    {phases : FinalizerPhases State}
    (assumptions : FinalizerLivenessAssumptions trace network
      catchupActivation commitStream phases) :
    LeadsToAfter (stableStart network catchupActivation) trace
      phases.pending phases.durableOutput := by
  have trigger :=
    assumptions.triggerEventually assumptions.continuousCommitStream
  have decide := assumptions.triggerToDecision.toLeadsTo
  have output := assumptions.decisionToDurableOutput.toLeadsTo
  exact (trigger.trans decide).trans output

/-- End-to-end liveness for a target transaction represented by the phase predicates. -/
theorem transaction_liveness
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {catchupActivation : Time}
    {roundState : State → RoundState}
    {consensusPhases : ConsensusPhases State}
    {commitStream : CommitStream}
    {finalizerPhases : FinalizerPhases State}
    (consensus : ConsensusLivenessAssumptions trace network
      catchupActivation roundState consensusPhases)
    (finalizer : FinalizerLivenessAssumptions trace network
      catchupActivation commitStream finalizerPhases)
    (commitEntersFinalizer :
      WithinAfter (stableStart network catchupActivation) network.delta trace
        consensusPhases.commitProduced finalizerPhases.pending) :
    LeadsToAfter (stableStart network catchupActivation) trace
      consensusPhases.roundOpen finalizerPhases.durableOutput := by
  have committed := consensus_liveness consensus
  have pending := commitEntersFinalizer.toLeadsTo
  have output := finalizer_liveness finalizer
  exact (committed.trans pending).trans output

end Mysticeti
