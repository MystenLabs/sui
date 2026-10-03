/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommitChain
import Mysticeti.PartialSynchrony

namespace Mysticeti

/-! Temporal operators, and stage composition for v3 transaction liveness.

`LeadsToAfter` and `WithinAfter` are the shared temporal operators. Commit
liveness uses them through `CommitProgressRecovery` and `ValidatorProcess`.

The finalizer part composes the supplied stages of one pending transaction into
a durable output. It is the only transaction-liveness result. Its obligations
are `ASM-SAFE-COMMIT-STORE`,
`ASM-LIVE-FINALIZER-TRIGGER`, `ASM-LIVE-TASK-FAIRNESS`, `ASM-LIVE-DURABILITY`,
and `ASM-LIVE-LOCAL-RESPONSE`.

Commit liveness no longer composes supplied consensus stages. It is derived in
`ValidatorFixedReferenceCurrentPacing`, so the earlier consensus stage ladder
was removed.
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

/-- GST and catch-up activation must both have passed. -/
def stableStart {protocolPacket : Packet → Prop}
    (network : PartialSynchrony protocolPacket) (catchupActivation : Time) : Time :=
  max network.gst catchupActivation

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
  /-- `ASM-SAFE-COMMIT-STORE`. -/
  continuousCommitStream : Continuous commitStream
  /-- `ASM-LIVE-FINALIZER-TRIGGER` and `ASM-LIVE-TASK-FAIRNESS`. -/
  triggerEventually :
    Continuous commitStream →
    LeadsToAfter (stableStart network catchupActivation) trace
      phases.pending phases.depthTwoTrigger
  /-- `ASM-LIVE-DURABILITY` and `ASM-LIVE-LOCAL-RESPONSE`. -/
  triggerToDecision :
    WithinAfter (stableStart network catchupActivation) network.delta trace
      phases.depthTwoTrigger phases.decided
  /-- `ASM-LIVE-DURABILITY` and `ASM-LIVE-LOCAL-RESPONSE`. -/
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

/-- Compose commit liveness and the supplied finalizer stages for one target
transaction.

`commitLiveness` is the commit-production input. `ValidatorFixedReferenceCurrentPacing`
derives commit liveness from fundamental inputs, but that result is stated over
the end-to-end execution model. A trace refinement must still connect the two
models, so this composition is not an end-to-end theorem. -/
theorem transaction_liveness_stage_composition
    {State : Type} {protocolPacket : Packet → Prop}
    {trace : Trace State}
    {network : PartialSynchrony protocolPacket}
    {catchupActivation : Time}
    {roundOpen commitProduced : State → Prop}
    {commitStream : CommitStream}
    {finalizerPhases : FinalizerPhases State}
    (commitLiveness :
      LeadsToAfter (stableStart network catchupActivation) trace
        roundOpen commitProduced)
    (finalizer : FinalizerLivenessStageObligations trace network
      catchupActivation commitStream finalizerPhases)
    (commitEntersFinalizer :
      WithinAfter (stableStart network catchupActivation) network.delta trace
        commitProduced finalizerPhases.pending) :
    LeadsToAfter (stableStart network catchupActivation) trace
      roundOpen finalizerPhases.durableOutput := by
  have pending := commitEntersFinalizer.toLeadsTo
  have output := finalizer_liveness_stage_composition finalizer
  exact (commitLiveness.trans pending).trans output

end Mysticeti
