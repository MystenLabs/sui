/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.PartialSynchrony

namespace Mysticeti

/-! Local timer rules for commit progress recovery.

The wait duration is a function of the absolute proposal round. The stalled
commit prefix still keys timer identity and reset behavior, but it does not
change the duration. Thus, validators with different installed prefixes use the
same wait for the same round. A local recovery-attempt counter is not part of
the key.

This file proves only a pointwise timing result. It does not assume that a leader
block is included, that a vote exists, or that vote stake reaches a quorum.
-/

/-- A common recovery wait for each absolute target round. Successive wait
margins eventually remain above each finite bound. -/
structure CommonRoundWaitSchedule (CommitPrefix : Type) where
  wait : CommitPrefix → Nat → Time
  permanentSuccessiveMargin : ∀ commitHead bound, ∃ firstRound, ∀ round,
    firstRound ≤ round →
      wait commitHead round + bound ≤ wait commitHead (round + 1)
  /-- The commit head keys timer identity and reset behavior, but the wait
  duration depends only on the absolute proposal round. -/
  headIndependent : ∀ left right round,
    wait left round = wait right round

namespace CommonRoundWaitSchedule

/-- The next-round wait for one local commit head eventually covers the
same-round wait for another head and any fixed delivery cost.

This is the pacing fact used after some validators have installed the next
commit and other validators still have the prior commit. -/
theorem eventually_covers_cross_head_visibility
    {CommitPrefix : Type}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (senderHead receiverHead : CommitPrefix) (cost : Nat) :
    ∃ firstRound, ∀ round,
      firstRound ≤ round →
        schedule.wait senderHead round + cost ≤
          schedule.wait receiverHead (round + 1) := by
  rcases schedule.permanentSuccessiveMargin receiverHead cost with
    ⟨firstRound, margin⟩
  refine ⟨firstRound, ?_⟩
  intro round firstBeforeRound
  rw [schedule.headIndependent senderHead receiverHead round]
  exact margin round firstBeforeRound

/-- The permanent margin rule covers the complete timing cost of one leader
proposal, one message delivery, and one local acceptance action. No comparison
between `epsilon` and `delta` is necessary. -/
theorem eventually_covers_visibility_cost
    {CommitPrefix : Type}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (commitHead : CommitPrefix) (startDifference delta epsilon : Nat) :
    ∃ firstRound, ∀ round,
      firstRound ≤ round →
        schedule.wait commitHead round +
            (startDifference + epsilon + delta + epsilon) ≤
          schedule.wait commitHead (round + 1) := by
  exact schedule.permanentSuccessiveMargin commitHead
    (startDifference + epsilon + delta + epsilon)

/-- A value whose increase per round is bounded has a linear upper bound. -/
theorem value_with_bounded_increase_has_linear_bound
    (value : Nat → Nat) (start increase offset : Nat)
    (boundedIncrease : ∀ round,
      start ≤ round → value (round + 1) ≤ value round + increase) :
    value (start + offset) ≤ value start + offset * increase := by
  induction offset with
  | zero => simp
  | succ previous inductionHypothesis =>
      have nextBound := boundedIncrease (start + previous) (by omega)
      calc
        value (start + (previous + 1)) =
            value ((start + previous) + 1) := by
          simp [Nat.add_assoc]
        _ ≤ value (start + previous) + increase := nextBound
        _ ≤ (value start + previous * increase) + increase :=
          Nat.add_le_add_right inductionHypothesis increase
        _ = value start + (previous + 1) * increase := by
          simp [Nat.succ_mul, Nat.add_assoc]

/-- A permanent successive margin gives a linear lower bound for the wait. -/
theorem wait_with_permanent_margin_has_linear_lower_bound
    {CommitPrefix : Type}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (commitHead : CommitPrefix) (start margin offset : Nat)
    (permanentMargin : ∀ round,
      start ≤ round →
        schedule.wait commitHead round + margin ≤
          schedule.wait commitHead (round + 1)) :
    schedule.wait commitHead start + offset * margin ≤
      schedule.wait commitHead (start + offset) := by
  induction offset with
  | zero => simp
  | succ previous inductionHypothesis =>
      have nextMargin := permanentMargin (start + previous) (by omega)
      calc
        schedule.wait commitHead start + (previous + 1) * margin =
            (schedule.wait commitHead start + previous * margin) + margin := by
          simp [Nat.succ_mul, Nat.add_assoc]
        _ ≤ schedule.wait commitHead (start + previous) + margin :=
          Nat.add_le_add_right inductionHypothesis margin
        _ ≤ schedule.wait commitHead ((start + previous) + 1) := nextMargin
        _ = schedule.wait commitHead (start + (previous + 1)) := by
          simp [Nat.add_assoc]

/-- Successive margins that eventually exceed every fixed value make the wait
larger than every value with a fixed per-round increase.

The `value` can be the spread between the earliest and latest correct timer
starts. The result does not assume a bound on that spread. -/
theorem permanent_margin_eventually_dominates_bounded_increase
    {CommitPrefix : Type}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (commitHead : CommitPrefix)
    (value : Nat → Nat) (start increase extra : Nat)
    (boundedIncrease : ∀ round,
      start ≤ round → value (round + 1) ≤ value round + increase) :
    ∃ firstRound,
      start ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            value round + extra ≤ schedule.wait commitHead round := by
  rcases schedule.permanentSuccessiveMargin commitHead (increase + 1) with
    ⟨marginStart, permanentMargin⟩
  let anchor := max start marginStart
  let catchUp := value anchor + extra
  let firstRound := anchor + catchUp
  have startBeforeAnchor : start ≤ anchor := Nat.le_max_left _ _
  have marginStartBeforeAnchor : marginStart ≤ anchor := Nat.le_max_right _ _
  refine ⟨firstRound, Nat.le_trans startBeforeAnchor (by simp [firstRound]), ?_⟩
  intro round firstBeforeRound
  have anchorBeforeRound : anchor ≤ round := by
    exact Nat.le_trans (by simp [firstRound]) firstBeforeRound
  obtain ⟨offset, roundAtOffset⟩ := Nat.exists_eq_add_of_le anchorBeforeRound
  subst round
  have catchUpBeforeOffset : catchUp ≤ offset := by
    simp only [firstRound] at firstBeforeRound
    omega
  have valueBound :
      value (anchor + offset) ≤ value anchor + offset * increase := by
    exact value_with_bounded_increase_has_linear_bound value anchor increase
      offset (by
        intro current anchorBeforeCurrent
        exact boundedIncrease current
          (Nat.le_trans startBeforeAnchor anchorBeforeCurrent))
  have waitBound :
      schedule.wait commitHead anchor + offset * (increase + 1) ≤
        schedule.wait commitHead (anchor + offset) := by
    exact wait_with_permanent_margin_has_linear_lower_bound schedule commitHead
      anchor (increase + 1) offset (by
        intro current anchorBeforeCurrent
        exact permanentMargin current
          (Nat.le_trans marginStartBeforeAnchor anchorBeforeCurrent))
  have valueWithinLinearWait :
      value anchor + offset * increase + extra ≤
        offset * (increase + 1) := by
    rw [Nat.mul_add]
    simp only [Nat.mul_one]
    omega
  have withoutInitialWait :
      value (anchor + offset) + extra ≤
        offset * (increase + 1) := by
    exact Nat.le_trans (Nat.add_le_add_right valueBound extra)
      valueWithinLinearWait
  exact Nat.le_trans withoutInitialWait
    (Nat.le_trans (Nat.le_add_left _ _) waitBound)

/-- One late-round suffix both dominates a bounded timer-start spread and covers
the pointwise proposal-to-acceptance cost. -/
theorem eventually_dominates_spread_and_covers_visibility
    {CommitPrefix : Type}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (commitHead : CommitPrefix)
    (spread : Nat → Nat) (start increase extra delta epsilon : Nat)
    (boundedIncrease : ∀ round,
      start ≤ round → spread (round + 1) ≤ spread round + increase) :
    ∃ firstRound,
      start ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            spread round + extra ≤ schedule.wait commitHead round ∧
              schedule.wait commitHead round +
                  (epsilon + delta + epsilon) ≤
                schedule.wait commitHead (round + 1) := by
  rcases schedule.permanent_margin_eventually_dominates_bounded_increase
      commitHead spread start increase extra boundedIncrease with
    ⟨spreadStart, startBeforeSpread, spreadDominated⟩
  rcases schedule.eventually_covers_visibility_cost commitHead 0 delta epsilon with
    ⟨visibilityStart, visibilityCovered⟩
  let firstRound := max spreadStart visibilityStart
  refine ⟨firstRound,
    Nat.le_trans startBeforeSpread (Nat.le_max_left _ _), ?_⟩
  intro round firstBeforeRound
  have spreadStartBeforeRound : spreadStart ≤ round :=
    Nat.le_trans (Nat.le_max_left _ _) firstBeforeRound
  have visibilityStartBeforeRound : visibilityStart ≤ round :=
    Nat.le_trans (Nat.le_max_right _ _) firstBeforeRound
  exact ⟨spreadDominated round spreadStartBeforeRound, by
    simpa using visibilityCovered round visibilityStartBeforeRound⟩

end CommonRoundWaitSchedule

/-- The timer for one exact recovery target. For target round `r`, the timer
starts only after the local DAG has quorum stake of valid blocks in round
`r - 1`. -/
structure RecoveryTargetTimer
    {CommitPrefix : Type} (commitHead : CommitPrefix) (targetRound : Nat) where
  parentQuorumReadyAt : Time
  startedAt : Time
  highestObservedRound : Nat
  startsAfterParentQuorum : parentQuorumReadyAt ≤ startedAt

namespace RecoveryTargetTimer

/-- Accept a later observed round without changing the recovery target or its
timer start. -/
def observeRound
    {CommitPrefix : Type} {commitHead : CommitPrefix} {targetRound : Nat}
    (timer : RecoveryTargetTimer commitHead targetRound) (observedRound : Nat) :
    RecoveryTargetTimer commitHead targetRound :=
  { timer with
    highestObservedRound := max timer.highestObservedRound observedRound }

/-- The time at which the target's recovery wait finishes. -/
def deadline
    {CommitPrefix : Type} {commitHead : CommitPrefix} {targetRound : Nat}
    (timer : RecoveryTargetTimer commitHead targetRound)
    (schedule : CommonRoundWaitSchedule CommitPrefix) : Time :=
  timer.startedAt + schedule.wait commitHead targetRound

@[simp]
theorem observe_round_keeps_timer_start
    {CommitPrefix : Type} {commitHead : CommitPrefix} {targetRound : Nat}
    (timer : RecoveryTargetTimer commitHead targetRound) (observedRound : Nat) :
    (timer.observeRound observedRound).startedAt = timer.startedAt := by
  rfl

@[simp]
theorem observe_round_keeps_parent_ready_time
    {CommitPrefix : Type} {commitHead : CommitPrefix} {targetRound : Nat}
    (timer : RecoveryTargetTimer commitHead targetRound) (observedRound : Nat) :
    (timer.observeRound observedRound).parentQuorumReadyAt =
      timer.parentQuorumReadyAt := by
  rfl

@[simp]
theorem observe_round_does_not_reset_deadline
    {CommitPrefix : Type} {commitHead : CommitPrefix} {targetRound : Nat}
    (timer : RecoveryTargetTimer commitHead targetRound)
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (observedRound : Nat) :
    (timer.observeRound observedRound).deadline schedule =
      timer.deadline schedule := by
  rfl

/-- The recorded parent-ready time is before the target deadline. The timer
record does not prove that the local parent set has quorum stake. -/
theorem recorded_parent_ready_time_before_deadline
    {CommitPrefix : Type} {commitHead : CommitPrefix} {targetRound : Nat}
    (timer : RecoveryTargetTimer commitHead targetRound)
    (schedule : CommonRoundWaitSchedule CommitPrefix) :
    timer.parentQuorumReadyAt ≤ timer.deadline schedule := by
  exact Nat.le_trans timer.startsAfterParentQuorum (by
    simp [deadline])

end RecoveryTargetTimer

/-- One correct leader proposal. The proposal is stored before the block packet
is sent. The covered proposal action includes this local storage work. -/
structure TimedLeaderProposal
    {CommitPrefix : Type} {protocolPacket : Packet → Prop}
    {protocolAction : LocalConsensusAction → Prop}
    {commitHead : CommitPrefix} {round : Nat}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (timer : RecoveryTargetTimer commitHead round) (packet : Packet) where
  action : LocalConsensusAction
  actionIsCovered : protocolAction action
  enabledAtDeadline : action.enabledAt = timer.deadline schedule
  packetIsProtocol : protocolPacket packet
  sentAtCompletion : packet.sentAt = action.completedAt

/-- Local verification and acceptance of one correct leader block. -/
structure TimedBlockAcceptance
    {protocolAction : LocalConsensusAction → Prop}
    (packet : Packet) where
  action : LocalConsensusAction
  actionIsCovered : protocolAction action
  enabledAtDelivery : action.enabledAt = packet.deliveredAt

/-- The parent-selection snapshot for one recovery proposal starts only after
the target's recovery deadline. -/
structure TimedParentSelection
    {CommitPrefix : Type} {commitHead : CommitPrefix} {round : Nat}
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    (timer : RecoveryTargetTimer commitHead round) where
  snapshotAt : Time
  startsAfterDeadline : timer.deadline schedule ≤ snapshotAt

/-- If the next-round wait covers the leader timer-start difference, proposal
work, message delivery, and acceptance work, the acceptance action completes
before the next-round proposer selects its parents.

The leader target is round `r`. The next proposal target is round `r + 1`, and
that proposal selects its parents from round `r`.

This theorem is pointwise for one correct leader and one correct next-round
proposer. A later theorem can apply it to validators whose derived stake reaches
quorum. -/
private theorem leader_acceptance_action_completes_before_parent_snapshot
    {CommitPrefix : Type}
    {protocolPacket : Packet → Prop}
    {protocolAction : LocalConsensusAction → Prop}
    {network : PartialSynchrony protocolPacket}
    (processing : BoundedLocalProcessing network protocolAction)
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    {commitHead : CommitPrefix} {round : Nat}
    (leaderTimer : RecoveryTargetTimer commitHead round)
    (nextTimer : RecoveryTargetTimer commitHead (round + 1))
    (packet : Packet)
    (proposal : TimedLeaderProposal
      (protocolPacket := protocolPacket)
      (protocolAction := protocolAction) schedule leaderTimer packet)
    (acceptance : TimedBlockAcceptance
      (protocolAction := protocolAction) packet)
    (parentSelection : TimedParentSelection schedule nextTimer)
    (startDifference : Nat)
    (leaderTimerStartsAfterGst : network.gst ≤ leaderTimer.startedAt)
    (leaderTimerStartBound :
      leaderTimer.startedAt ≤ nextTimer.startedAt + startDifference)
    (nextWaitCoversFlow :
      schedule.wait commitHead round +
          (startDifference + processing.epsilon + network.delta +
            processing.epsilon) ≤
        schedule.wait commitHead (round + 1)) :
    acceptance.action.completedAt ≤ parentSelection.snapshotAt := by
  have leaderStartBeforeDeadline :
      leaderTimer.startedAt ≤ leaderTimer.deadline schedule := by
    simp [RecoveryTargetTimer.deadline]
  have proposalStartsAfterGst : network.gst ≤ proposal.action.enabledAt := by
    rw [proposal.enabledAtDeadline]
    exact Nat.le_trans leaderTimerStartsAfterGst leaderStartBeforeDeadline
  have proposalCompletion := processing.postGstCompletion proposal.action
    proposal.actionIsCovered proposalStartsAfterGst
  have packetStartsAfterGst : network.gst ≤ packet.sentAt := by
    rw [proposal.sentAtCompletion]
    exact Nat.le_trans proposalStartsAfterGst proposalCompletion.1
  have acceptedAfterDelivery :=
    processing.protocol_packet_becomes_locally_visible packet
      proposal.packetIsProtocol acceptance.action acceptance.actionIsCovered
      acceptance.enabledAtDelivery packetStartsAfterGst
  have packetSentByLeaderBound :
      packet.sentAt ≤
        leaderTimer.startedAt + schedule.wait commitHead round +
          processing.epsilon := by
    rw [proposal.sentAtCompletion]
    have completionBound := proposalCompletion.2
    rw [proposal.enabledAtDeadline] at completionBound
    simpa [RecoveryTargetTimer.deadline, Nat.add_assoc] using completionBound
  have acceptedByLeaderCost :
      acceptance.action.completedAt ≤
        leaderTimer.startedAt + schedule.wait commitHead round +
          processing.epsilon + network.delta + processing.epsilon := by
    exact Nat.le_trans acceptedAfterDelivery
      (Nat.add_le_add_right
        (Nat.add_le_add_right packetSentByLeaderBound network.delta)
        processing.epsilon)
  have leaderCostByNextStart :
      leaderTimer.startedAt + schedule.wait commitHead round +
          processing.epsilon + network.delta + processing.epsilon ≤
        nextTimer.startedAt + startDifference +
          schedule.wait commitHead round + processing.epsilon +
          network.delta + processing.epsilon := by
    exact Nat.add_le_add_right
      (Nat.add_le_add_right
        (Nat.add_le_add_right
          (Nat.add_le_add_right leaderTimerStartBound
            (schedule.wait commitHead round))
          processing.epsilon)
        network.delta)
      processing.epsilon
  have nextStartCostWithinWait :
      nextTimer.startedAt + startDifference +
          schedule.wait commitHead round + processing.epsilon +
          network.delta + processing.epsilon ≤
        nextTimer.startedAt + schedule.wait commitHead (round + 1) := by
    have coveredFromNextStart :=
      Nat.add_le_add_left nextWaitCoversFlow nextTimer.startedAt
    simpa only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using
      coveredFromNextStart
  have acceptedBeforeNextDeadline :
      acceptance.action.completedAt ≤ nextTimer.deadline schedule := by
    simpa [RecoveryTargetTimer.deadline] using
      Nat.le_trans acceptedByLeaderCost
        (Nat.le_trans leaderCostByNextStart nextStartCostWithinWait)
  exact Nat.le_trans acceptedBeforeNextDeadline
    parentSelection.startsAfterDeadline

/-- A local timer-start envelope removes the cross-validator start-difference
input from the pointwise visibility theorem.

`earliestRoundStart` is a proof value for the earliest correct timer start in the
leader round. The first two timing facts can be derived from local proposal,
delivery, acceptance, and timer-start actions. This theorem does not assume that
the leader block is a parent, that a vote exists, or that vote stake is a quorum. -/
theorem leader_acceptance_action_completes_before_parent_snapshot_from_spread
    {CommitPrefix : Type}
    {protocolPacket : Packet → Prop}
    {protocolAction : LocalConsensusAction → Prop}
    {network : PartialSynchrony protocolPacket}
    (processing : BoundedLocalProcessing network protocolAction)
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    {commitHead : CommitPrefix} {round : Nat}
    (leaderTimer : RecoveryTargetTimer commitHead round)
    (nextTimer : RecoveryTargetTimer commitHead (round + 1))
    (packet : Packet)
    (proposal : TimedLeaderProposal
      (protocolPacket := protocolPacket)
      (protocolAction := protocolAction) schedule leaderTimer packet)
    (acceptance : TimedBlockAcceptance
      (protocolAction := protocolAction) packet)
    (parentSelection : TimedParentSelection schedule nextTimer)
    (earliestRoundStart startSpread : Nat)
    (leaderStartWithinSpread :
      leaderTimer.startedAt ≤ earliestRoundStart + startSpread)
    (nextTimerStartsAfterEarliestDeadline :
      earliestRoundStart + schedule.wait commitHead round ≤
        nextTimer.startedAt)
    (waitDominatesSpread :
      startSpread ≤ schedule.wait commitHead round)
    (leaderTimerStartsAfterGst : network.gst ≤ leaderTimer.startedAt)
    (nextWaitCoversVisibility :
      schedule.wait commitHead round +
          (processing.epsilon + network.delta + processing.epsilon) ≤
        schedule.wait commitHead (round + 1)) :
    acceptance.action.completedAt ≤ parentSelection.snapshotAt := by
  have leaderStartsBeforeNext :
      leaderTimer.startedAt ≤ nextTimer.startedAt := by
    calc
      leaderTimer.startedAt ≤ earliestRoundStart + startSpread :=
        leaderStartWithinSpread
      _ ≤ earliestRoundStart + schedule.wait commitHead round :=
        Nat.add_le_add_left waitDominatesSpread earliestRoundStart
      _ ≤ nextTimer.startedAt := nextTimerStartsAfterEarliestDeadline
  apply leader_acceptance_action_completes_before_parent_snapshot processing
    schedule
    leaderTimer nextTimer packet proposal acceptance parentSelection 0
    leaderTimerStartsAfterGst
  · simpa using leaderStartsBeforeNext
  · simpa using nextWaitCoversVisibility

/-- Bounded local timer-start spread and the growing wait give a late suffix in
which each leader acceptance action completes before the related next-round
parent snapshot.

All premises are pointwise timing or local-action facts. The theorem does not
take block inclusion, direct votes, a quorum result, or an anchor as an input. -/
theorem eventually_leader_acceptance_completes_before_parent_snapshot
    {CommitPrefix : Type}
    {protocolPacket : Packet → Prop}
    {protocolAction : LocalConsensusAction → Prop}
    {network : PartialSynchrony protocolPacket}
    (processing : BoundedLocalProcessing network protocolAction)
    (schedule : CommonRoundWaitSchedule CommitPrefix)
    {commitHead : CommitPrefix}
    (leaderTimer : (round : Nat) → RecoveryTargetTimer commitHead round)
    (nextTimer : (round : Nat) → RecoveryTargetTimer commitHead (round + 1))
    (packet : Nat → Packet)
    (proposal : ∀ round,
      TimedLeaderProposal
        (protocolPacket := protocolPacket)
        (protocolAction := protocolAction)
        schedule (leaderTimer round) (packet round))
    (acceptance : ∀ round,
      TimedBlockAcceptance
        (protocolAction := protocolAction) (packet round))
    (parentSelection : ∀ round,
      TimedParentSelection schedule (nextTimer round))
    (earliestRoundStart startSpread : Nat → Nat)
    (start spreadIncrease : Nat)
    (boundedSpreadIncrease : ∀ round,
      start ≤ round →
        startSpread (round + 1) ≤ startSpread round + spreadIncrease)
    (leaderStartWithinSpread : ∀ round,
      start ≤ round →
        (leaderTimer round).startedAt ≤
          earliestRoundStart round + startSpread round)
    (nextTimerStartsAfterEarliestDeadline : ∀ round,
      start ≤ round →
        earliestRoundStart round + schedule.wait commitHead round ≤
          (nextTimer round).startedAt)
    (leaderTimerStartsAfterGst : ∀ round,
      start ≤ round → network.gst ≤ (leaderTimer round).startedAt) :
    ∃ firstRound,
      start ≤ firstRound ∧
        ∀ round,
          firstRound ≤ round →
            (acceptance round).action.completedAt ≤
              (parentSelection round).snapshotAt := by
  rcases schedule.eventually_dominates_spread_and_covers_visibility
      commitHead startSpread start spreadIncrease 0 network.delta
      processing.epsilon boundedSpreadIncrease with
    ⟨firstRound, startBeforeFirst, lateBounds⟩
  refine ⟨firstRound, startBeforeFirst, ?_⟩
  intro round firstBeforeRound
  have startBeforeRound := Nat.le_trans startBeforeFirst firstBeforeRound
  have bounds := lateBounds round firstBeforeRound
  apply leader_acceptance_action_completes_before_parent_snapshot_from_spread
    processing schedule (leaderTimer round) (nextTimer round) (packet round)
      (proposal round) (acceptance round) (parentSelection round)
      (earliestRoundStart round) (startSpread round)
  · exact leaderStartWithinSpread round startBeforeRound
  · exact nextTimerStartsAfterEarliestDeadline round startBeforeRound
  · simpa using bounds.1
  · exact leaderTimerStartsAfterGst round startBeforeRound
  · exact bounds.2

end Mysticeti
