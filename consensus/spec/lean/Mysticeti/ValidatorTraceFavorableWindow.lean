/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.FavorableRecoveryCommitAgreement
import Mysticeti.ValidatorTracePacing

namespace Mysticeti

/-! Exact-reference recovery windows derived from validator traces.

This module keeps the exact block ID when it moves from the validator execution
model to the reference-carrying FlexCommitter model. It does not take a
favorable window, a direct-vote quorum, or a local FlexCommitter input as an
input.
-/

namespace ValidatorBlockRef

/-- Keep the exact validator block ID as the leader-branch digest. -/
def toLeaderBlockRef {BlockId : Type}
    (reference : ValidatorBlockRef BlockId) : LeaderBlockRef BlockId :=
  { round := reference.round
    author := reference.author
    digest := reference.id }

/-- Converting a validator reference preserves its round. -/
@[simp]
theorem to_leader_block_ref_round
    {BlockId : Type} (reference : ValidatorBlockRef BlockId) :
    reference.toLeaderBlockRef.round = reference.round := by
  rfl

/-- Converting a validator reference preserves its author. -/
@[simp]
theorem to_leader_block_ref_author
    {BlockId : Type} (reference : ValidatorBlockRef BlockId) :
    reference.toLeaderBlockRef.author = reference.author := by
  rfl

/-- Converting a validator reference keeps its exact block ID as the digest. -/
@[simp]
theorem to_leader_block_ref_digest
    {BlockId : Type} (reference : ValidatorBlockRef BlockId) :
    reference.toLeaderBlockRef.digest = reference.id := by
  rfl

end ValidatorBlockRef

/-- Add the absolute round to each validator in the configured selected leader
order. -/
def exactSelectedLeaderOrder
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (commitHead : ValidatorCommitHead CommitId) (round : Nat) :
    List ExactSelectedLeaderSlot :=
  (config.selectedLeaderOrder commitHead.id round).map fun validator =>
    { round, validator }

/-- Every slot made from one configured order names that order's round. -/
theorem exact_selected_leader_order_at_round
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (commitHead : ValidatorCommitHead CommitId) (round : Nat)
    {slot : ExactSelectedLeaderSlot}
    (included : slot ∈ exactSelectedLeaderOrder config commitHead round) :
    slot.round = round := by
  simp only [exactSelectedLeaderOrder, List.mem_map] at included
  rcases included with ⟨validator, _included, rfl⟩
  rfl

/-- A configured first selected validator is the first exact selected slot. -/
theorem exact_selected_leader_order_head
    {CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (commitHead : ValidatorCommitHead CommitId) (round validator : Nat)
    (first :
      (config.selectedLeaderOrder commitHead.id round).head? = some validator) :
    (exactSelectedLeaderOrder config commitHead round).head? =
      some { round, validator } := by
  cases selected : config.selectedLeaderOrder commitHead.id round with
  | nil => simp [selected] at first
  | cons head tail =>
      have headIsValidator : head = validator := by
        simpa [selected] using first
      subst head
      simp [exactSelectedLeaderOrder, selected]

private theorem finite_pointwise_times_have_common_upper_bound
    (count : Nat) (resultAt : Nat → Time → Prop)
    (pointwise : ∀ validator,
      validator < count → ∃ time, resultAt validator time) :
    ∃ finish, ∀ validator,
      validator < count →
      ∃ time, time ≤ finish ∧ resultAt validator time := by
  induction count with
  | zero =>
      exact ⟨0, by intro validator validatorInRange; omega⟩
  | succ previous inductionHypothesis =>
      rcases inductionHypothesis (by
          intro validator validatorInRange
          exact pointwise validator (by omega)) with
        ⟨previousFinish, previousResults⟩
      rcases pointwise previous (by omega) with
        ⟨lastFinish, lastResult⟩
      refine ⟨max previousFinish lastFinish, ?_⟩
      intro validator validatorInRange
      by_cases beforeLast : validator < previous
      · rcases previousResults validator beforeLast with
          ⟨time, timeBeforePrevious, result⟩
        exact ⟨time,
          Nat.le_trans timeBeforePrevious (Nat.le_max_left _ _), result⟩
      · have isLast : validator = previous := by omega
        subst validator
        exact ⟨lastFinish, Nat.le_max_right _ _, lastResult⟩

/-- One exact direct-voter fact persists as the observer receives more blocks.
-/
theorem trace_direct_voter_persists
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {earlier later observer voter : Nat}
    {leader : ValidatorBlockRef BlockId}
    (observerInRange : observer < config.authorityCount)
    (ordered : earlier ≤ later)
    (vote : traceDirectVoters (execution.trace earlier) observer leader voter =
      true) :
    traceDirectVoters (execution.trace later) observer leader voter = true := by
  unfold traceDirectVoters at vote ⊢
  cases representative :
      ((execution.trace earlier).validatorState observer).acceptedRepresentative
        (leader.round + 1) voter with
  | none => simp [representative] at vote
  | some childReference =>
      have representativeLater :=
        (execution.durable_fields_persist observerInRange ordered)
          |>.accepted_representative_persists representative
      cases catalog :
          (execution.trace earlier).blockCatalog childReference.id with
      | none => simp [representative, catalog] at vote
      | some child =>
          have catalogLater := execution.blockCatalogMonotone earlier later
            ordered childReference.id child catalog
          simpa [representativeLater, catalogLater] using
            (show decide
                (child.reference = childReference ∧
                  childReference.author = voter ∧
                  childReference.round = leader.round + 1 ∧
                  leader ∈ child.parents) = true by
              simpa [representative, catalog] using vote)

/-- Finite validator aggregation turns pointwise eventual direct votes into one
actual trace time with direct-vote quorum stake.

The input is an internal pointwise theorem result. It is not a permitted final
liveness assumption. -/
theorem pointwise_direct_votes_eventually_give_trace_quorum
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {start observer : Nat} {leader : ValidatorBlockRef BlockId}
    (observerInRange : observer < config.authorityCount)
    (pointwise : ∀ voter,
      voter < config.authorityCount →
      faults.correctAvailable voter = true →
      ∃ finish,
        start ≤ finish ∧
          traceDirectVoters (execution.trace finish) observer leader voter =
            true) :
    ∃ finish,
      start ≤ finish ∧
        config.thresholds.quorum ≤
          weight config.authorityCount config.stake
            (traceDirectVoters (execution.trace finish) observer leader) := by
  let resultAt := fun voter time =>
    faults.correctAvailable voter = true →
      start ≤ time ∧
        traceDirectVoters (execution.trace time) observer leader voter = true
  have everyValidatorHasTime : ∀ voter,
      voter < config.authorityCount → ∃ time, resultAt voter time := by
    intro voter voterInRange
    by_cases voterCorrect : faults.correctAvailable voter = true
    · rcases pointwise voter voterInRange voterCorrect with
        ⟨finish, startBeforeFinish, vote⟩
      exact ⟨finish, by intro _; exact ⟨startBeforeFinish, vote⟩⟩
    · exact ⟨start, by intro falseCorrect; contradiction⟩
  rcases finite_pointwise_times_have_common_upper_bound
      config.authorityCount resultAt everyValidatorHasTime with
    ⟨finish, common⟩
  let commonFinish := max start finish
  refine ⟨commonFinish, Nat.le_max_left _ _,
    all_correct_available_children_vote_gives_quorum faults ?_⟩
  intro voter voterInRange voterCorrect
  rcases common voter voterInRange with ⟨time, timeBeforeFinish, atTime⟩
  have timeBeforeCommon : time ≤ commonFinish :=
    Nat.le_trans timeBeforeFinish (Nat.le_max_right _ _)
  exact trace_direct_voter_persists execution observerInRange timeBeforeCommon
    (atTime voterCorrect).2

/-- One accepted eligible block round has an entry in the local pending-round
array while the same commit head is active.

This is a one-validator source rule. Rust must establish it when block
acceptance updates `PendingCommitState`. It does not state that a future block,
vote quorum, or anchor exists. -/
structure ValidatorPendingRoundIngestionRules
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (anchorRules : ValidatorAnchorLocalRules config faults trace) where
  acceptedEligibleRoundIsPending : ∀ time observer
      (block : ValidatorBlockRef BlockId),
    ((trace time).validatorState observer).accepted block = true →
    (anchorRules.flexInputAt time observer).firstPendingRound ≤ block.round →
    ∃ index,
      (anchorRules.flexInputAt time observer).firstPendingRound + index =
          block.round ∧
        index < (anchorRules.flexInputAt time observer).roundCount

/-- Direct-voter stake cannot decrease while accepted representatives and the
block catalog are durable. -/
theorem trace_direct_voter_weight_mono
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {earlier later observer : Nat} {leader : ValidatorBlockRef BlockId}
    (observerInRange : observer < config.authorityCount)
    (ordered : earlier ≤ later) :
    weight config.authorityCount config.stake
        (traceDirectVoters (execution.trace earlier) observer leader) ≤
      weight config.authorityCount config.stake
        (traceDirectVoters (execution.trace later) observer leader) := by
  apply weight_mono config.stake
  intro voter voterInRange voted
  exact trace_direct_voter_persists execution observerInRange ordered voted

/-- Finite pointwise direct-vote results give the full adjacent local anchor
range at one trace time.

The result uses actual accepted block references. `pointwise` is an internal
result that the proposal, delivery, parent, and voting trace proves for each
offset. It is not a final liveness input. -/
theorem pointwise_direct_anchor_range_eventually_gives_window
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (anchorRules : ValidatorAnchorLocalRules config faults execution.trace)
    (pendingRules : ValidatorPendingRoundIngestionRules execution.trace
      anchorRules)
    {start observer baseRound count : Nat}
    {commitHead : ValidatorCommitHead CommitId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (countPositive : 0 < count)
    (headAtStart :
      ((execution.trace start).validatorState observer).commitHead = commitHead)
    (baseIsEligible : ∀ time, start ≤ time →
      (execution.trace time).localCommitIndex observer = commitHead.index →
      (anchorRules.flexInputAt time observer).firstPendingRound ≤ baseRound)
    (leaderRound : ∀ offset, offset < count →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset, offset < count →
      (config.selectedLeaderOrder commitHead.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (pointwise : ∀ offset, offset < count →
      ∃ finish,
        start ≤ finish ∧
          ((execution.trace finish).validatorState observer).accepted
              (leaderAt offset) = true ∧
          config.thresholds.quorum ≤
            weight config.authorityCount config.stake
              (traceDirectVoters (execution.trace finish) observer
                (leaderAt offset))) :
    ∃ finish,
      start ≤ finish ∧
        (commitHead.index < (execution.trace finish).localCommitIndex observer ∨
          ∃ baseIndex,
            (anchorRules.flexInputAt finish observer).firstPendingRound +
                baseIndex = baseRound ∧
            baseIndex + count ≤
              (anchorRules.flexInputAt finish observer).roundCount ∧
            FlexAnchorWindow
              (anchorRules.flexInputAt finish observer).toFlexState
              baseIndex count) := by
  let resultAt := fun offset time =>
    start ≤ time ∧
      ((execution.trace time).validatorState observer).accepted
          (leaderAt offset) = true ∧
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (execution.trace time) observer (leaderAt offset))
  rcases finite_pointwise_times_have_common_upper_bound count resultAt
      (by
        intro offset offsetInRange
        exact pointwise offset offsetInRange) with
    ⟨upper, results⟩
  let finish := max start upper
  have startBeforeFinish : start ≤ finish := Nat.le_max_left _ _
  have acceptedAtFinish : ∀ offset, offset < count →
      ((execution.trace finish).validatorState observer).accepted
          (leaderAt offset) = true := by
    intro offset offsetInRange
    rcases results offset offsetInRange with
      ⟨time, timeBeforeUpper, atTime⟩
    have timeBeforeFinish : time ≤ finish :=
      Nat.le_trans timeBeforeUpper (Nat.le_max_right _ _)
    exact execution.accepted_block_persists observerInRange timeBeforeFinish
      atTime.2.1
  have quorumAtFinish : ∀ offset, offset < count →
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (execution.trace finish) observer
            (leaderAt offset)) := by
    intro offset offsetInRange
    rcases results offset offsetInRange with
      ⟨time, timeBeforeUpper, atTime⟩
    have timeBeforeFinish : time ≤ finish :=
      Nat.le_trans timeBeforeUpper (Nat.le_max_right _ _)
    exact Nat.le_trans atTime.2.2
      (trace_direct_voter_weight_mono execution observerInRange
        timeBeforeFinish)
  have durableFromStart := execution.durable_fields_persist observerInRange
    startBeforeFinish
  have priorIndexAtStart :
      (execution.trace start).localCommitIndex observer = commitHead.index := by
    simp [ValidatorWorldState.localCommitIndex, headAtStart]
  have priorIndexNotDecreased :
      commitHead.index ≤ (execution.trace finish).localCommitIndex observer := by
    rw [← priorIndexAtStart]
    exact durableFromStart.1
  by_cases advanced :
      commitHead.index < (execution.trace finish).localCommitIndex observer
  · exact ⟨finish, startBeforeFinish, Or.inl advanced⟩
  · have sameIndex :
        (execution.trace finish).localCommitIndex observer = commitHead.index := by
      omega
    have actualHead :
        ((execution.trace finish).validatorState observer).commitHead =
          commitHead := by
      have equalIndices :
          ((execution.trace start).validatorState observer).commitHead.index =
            ((execution.trace finish).validatorState observer).commitHead.index := by
        rw [headAtStart]
        change ((execution.trace finish).validatorState observer).commitHead.index =
          commitHead.index at sameIndex
        exact sameIndex.symm
      exact (durableFromStart.2.2.1 equalIndices).symm.trans headAtStart
    have baseAccepted := acceptedAtFinish 0 countPositive
    have basePending := pendingRules.acceptedEligibleRoundIsPending finish
      observer (leaderAt 0) baseAccepted (by
        rw [leaderRound 0 countPositive]
        simpa using baseIsEligible finish startBeforeFinish sameIndex)
    rcases basePending with
      ⟨baseIndex, baseIndexRound, baseIndexInRange⟩
    have lastOffsetInRange : count - 1 < count := by omega
    have lastAccepted := acceptedAtFinish (count - 1) lastOffsetInRange
    have lastPending := pendingRules.acceptedEligibleRoundIsPending finish
      observer (leaderAt (count - 1)) lastAccepted (by
        rw [leaderRound (count - 1) lastOffsetInRange]
        exact Nat.le_trans (baseIsEligible finish startBeforeFinish sameIndex)
          (Nat.le_add_right baseRound (count - 1)))
    rcases lastPending with
      ⟨lastIndex, lastIndexRound, lastIndexInRange⟩
    have baseIndexMatches :
        (anchorRules.flexInputAt finish observer).firstPendingRound +
            baseIndex = baseRound := by
      rw [baseIndexRound, leaderRound 0 countPositive]
      simp
    have lastIndexMatches : lastIndex = baseIndex + (count - 1) := by
      rw [leaderRound (count - 1) lastOffsetInRange] at lastIndexRound
      omega
    have windowCovered : baseIndex + count ≤
        (anchorRules.flexInputAt finish observer).roundCount := by
      omega
    have anchorWindow :=
      anchorRules.covered_direct_quorum_range_gives_anchor_window
        observerInRange observerCorrectAvailable windowCovered leaderAt
        (by
          intro offset offsetInRange
          rw [actualHead]
          simpa [baseIndexMatches] using
            firstSelected offset offsetInRange)
        acceptedAtFinish quorumAtFinish
    exact ⟨finish, startBeforeFinish, Or.inr ⟨baseIndex,
      baseIndexMatches, windowCovered, anchorWindow⟩⟩

/-- Exact references in the finite causal closure of one anchor block. -/
inductive ValidatorCausalClosureReference
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (anchor : ValidatorBlockRef BlockId) :
    ValidatorBlockRef BlockId → Prop where
  | anchor : ValidatorCausalClosureReference world anchor anchor
  | parent {child parent : ValidatorBlockRef BlockId}
      {block : ValidatorBlock BlockId} :
      ValidatorCausalClosureReference world anchor child →
      world.blockCatalog child.id = some block →
      block.reference = child →
      parent ∈ block.parents →
      ValidatorCausalClosureReference world anchor parent

/-- One observer has accepted every exact block reference in one anchor's
causal closure. The parent-first block-sync theorem can establish this local
state fact. -/
def ValidatorAcceptedCausalClosure
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (observer : Nat) (anchor : ValidatorBlockRef BlockId) : Prop :=
  ∀ reference,
    ValidatorCausalClosureReference world anchor reference →
    (world.validatorState observer).accepted reference = true

/-- One-validator mapping from an actual accepted causal closure to the exact
anchor view used by the FlexCommitter proof.

Rust must map its block store and causal traversal to `viewAt`. The mapping must
mark a view complete only after every required causal block is accepted. -/
structure ValidatorExactAnchorCausalSourceMap
    {BlockId CommitId PacketId LocalView : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (mapping : ExactAnchorCausalDataMap LocalView BlockId) where
  viewAt : Time → Nat → LocalView
  completeFromAcceptedClosure : ∀ time observer anchor,
    ValidatorAcceptedCausalClosure (trace time) observer anchor →
    mapping.complete (viewAt time observer) anchor.toLeaderBlockRef

/-- One-validator mapping for the exact direct decider.

The first rule says that each cached direct result has its real local vote
evidence. The second rule says that a quorum of exact child votes makes the
decider return the exact commit reference. These are local source-code rules,
not distributed liveness assumptions. -/
structure ValidatorExactDirectDeciderSourceMap
    {BlockId CommitId PacketId LocalView : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (mapping : ExactAnchorCausalDataMap LocalView BlockId) where
  directAt : Time → Nat →
    ExactSelectedLeaderSlot → OptionalExactDirectResult BlockId
  anchorTailStatusAt : Time → Nat →
    ExactSelectedLeaderSlot → ReferenceSlotStatus BlockId
  directResultHasExactEvidence : ∀ time observer anchor
      (commitVotes skipVotes : LeaderBlockRef BlockId → VoterSet)
      (_evidence : @ExactAnchorEvidenceMap BlockId config.authorityCount
        config.stake config.thresholds (mapping.canonicalData anchor)
        commitVotes skipVotes)
      slot decision,
    directAt time observer slot = some decision →
    ExactDirectStatusValid config.thresholds commitVotes skipVotes slot
      (referenceStatusOfExactDecision decision)
  directQuorumReturnsExactCommit : ∀ time observer
      (slot : ExactSelectedLeaderSlot) (block : ValidatorBlockRef BlockId),
    block.toLeaderBlockRef.AtSelectedSlot slot →
    ((trace time).validatorState observer).accepted block = true →
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters (trace time) observer block) →
    directAt time observer slot = some (.commit block.toLeaderBlockRef)

/-- Actual causal closure, exact direct votes, and the one-validator source maps
construct the exact favorable input for one observer.

The returned final equality also records the observer's exact direct commit for
the anchor first slot. Thus, the proof does not invent the anchor's `.commit`
status when it constructs `FavorableRecoveryLocalInput`. -/
theorem trace_evidence_constructs_favorable_recovery_local_input
    {BlockId CommitId PacketId LocalView : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {trace : Trace (ValidatorWorldState BlockId CommitId PacketId)}
    {mapping : ExactAnchorCausalDataMap LocalView BlockId}
    (causalSource : ValidatorExactAnchorCausalSourceMap (config := config)
      trace mapping)
    (deciderSource : ValidatorExactDirectDeciderSourceMap (config := config)
      trace mapping)
    (window : ExactFavorableRecoveryWindow BlockId
      (fun validator => faults.correctAvailable validator = true))
    {time observer : Nat}
    {targetReference anchorReference : ValidatorBlockRef BlockId}
    (targetMatchesWindow :
      targetReference.toLeaderBlockRef = window.targetFirstBlock)
    (anchorMatchesWindow :
      anchorReference.toLeaderBlockRef = window.anchorBlock)
    (anchorClosure :
      ValidatorAcceptedCausalClosure (trace time) observer anchorReference)
    (targetAccepted :
      ((trace time).validatorState observer).accepted targetReference = true)
    (anchorAccepted :
      ((trace time).validatorState observer).accepted anchorReference = true)
    (targetDirectQuorum :
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (trace time) observer targetReference))
    (anchorDirectQuorum :
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (trace time) observer anchorReference))
    {commitVotes skipVotes : LeaderBlockRef BlockId → VoterSet}
    (evidence : @ExactAnchorEvidenceMap BlockId config.authorityCount
      config.stake config.thresholds
      (mapping.canonicalData window.anchorBlock) commitVotes skipVotes) :
    ∃ input : FavorableRecoveryLocalInput mapping window,
      input.view = causalSource.viewAt time observer ∧
        input.targetDirect.directAt = deciderSource.directAt time observer ∧
        deciderSource.directAt time observer window.anchorFirstSlot =
          some (.commit window.anchorBlock) := by
  have completeAnchor : mapping.complete
      (causalSource.viewAt time observer) window.anchorBlock := by
    have complete := causalSource.completeFromAcceptedClosure time observer
      anchorReference anchorClosure
    simpa [anchorMatchesWindow] using complete
  have targetAtFirstSlot :
      targetReference.toLeaderBlockRef.AtSelectedSlot
        window.targetFirstSlot := by
    rw [targetMatchesWindow]
    exact window.targetBlockMatchesFirstSlot
  have targetFirstDirect := deciderSource.directQuorumReturnsExactCommit time
    observer window.targetFirstSlot targetReference targetAtFirstSlot
    targetAccepted targetDirectQuorum
  have anchorAtFirstSlot :
      anchorReference.toLeaderBlockRef.AtSelectedSlot
        window.anchorFirstSlot := by
    rw [anchorMatchesWindow]
    exact window.anchorBlockMatchesFirstSlot
  have anchorFirstDirect := deciderSource.directQuorumReturnsExactCommit time
    observer window.anchorFirstSlot anchorReference anchorAtFirstSlot
    anchorAccepted anchorDirectQuorum
  let input := FavorableRecoveryLocalInput.ofExactEvidence
    (mapping := mapping) (window := window)
    (causalSource.viewAt time observer) completeAnchor
    (deciderSource.anchorTailStatusAt time observer) evidence
    (deciderSource.directAt time observer)
    (by
      intro slot included decision found
      exact deciderSource.directResultHasExactEvidence time observer
        window.anchorBlock commitVotes skipVotes evidence slot decision found)
    (by simpa [targetMatchesWindow] using targetFirstDirect)
  refine ⟨input, rfl, ?_, ?_⟩
  · rfl
  · simpa [anchorMatchesWindow] using anchorFirstDirect

/-- Pointwise recovery timers for two selected leader rounds construct the
exact favorable-window identity used by the FlexCommitter agreement proof.

This theorem records only the target and anchor block identities. Causal parent
inclusion, direct votes, and each observer's complete anchor view are separate
trace results. -/
theorem recovery_timers_produce_exact_favorable_window
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (rules : ValidatorRecoveryProposalPipelineRules faults protocolPacket
      network program timed)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {commitHead : ValidatorCommitHead CommitId}
    {targetRound depth targetLeader targetReceiver anchorLeader anchorReceiver :
      Nat}
    (targetReady : ValidatorRecoveryProposalReady faults timed waits commitHead
      targetRound targetLeader)
    (targetNextReady : ValidatorRecoveryProposalReady faults timed waits
      commitHead (targetRound + 1) targetReceiver)
    (anchorReady : ValidatorRecoveryProposalReady faults timed waits commitHead
      (targetRound + depth) anchorLeader)
    (anchorNextReady : ValidatorRecoveryProposalReady faults timed waits
      commitHead (targetRound + depth + 1) anchorReceiver)
    (targetParentAvailability :
      ValidatorRecoveryBroadcastParentAvailability targetReady targetReceiver)
    (anchorParentAvailability :
      ValidatorRecoveryBroadcastParentAvailability anchorReady anchorReceiver)
    (targetAboveGc :
      ValidatorRecoveryBroadcastAboveGcAfterDelivery targetReady targetReceiver)
    (anchorAboveGc :
      ValidatorRecoveryBroadcastAboveGcAfterDelivery anchorReady anchorReceiver)
    (targetStartsAfterGst : network.gst ≤ targetReady.startedAt)
    (anchorStartsAfterGst : network.gst ≤ anchorReady.startedAt)
    (targetFirst :
      (config.selectedLeaderOrder commitHead.id targetRound).head? =
        some targetLeader)
    (anchorFirst :
      (config.selectedLeaderOrder commitHead.id (targetRound + depth)).head? =
        some anchorLeader) :
    ∃ targetFlow : ValidatorTimedBlockFlow config faults timed.execution.trace
          (timingProtocolPacket :=
            CorrectAvailableAddressedTimingPacket config faults protocolPacket)
          (timingProtocolAction :=
            TraceActionWithin
              (validatorProposalPipelineBound timed.localActionBound))
          waits commitHead targetRound,
      ∃ anchorFlow : ValidatorTimedBlockFlow config faults timed.execution.trace
          (timingProtocolPacket :=
            CorrectAvailableAddressedTimingPacket config faults protocolPacket)
          (timingProtocolAction :=
            TraceActionWithin
              (validatorProposalPipelineBound timed.localActionBound))
          waits commitHead (targetRound + depth),
        ∃ window : ExactFavorableRecoveryWindow BlockId
            (fun validator => faults.correctAvailable validator = true),
          targetFlow.leader = targetLeader ∧
            anchorFlow.leader = anchorLeader ∧
            window.targetOrder =
              exactSelectedLeaderOrder config commitHead targetRound ∧
            window.anchorOrder =
              exactSelectedLeaderOrder config commitHead
                (targetRound + depth) ∧
            window.targetFirstBlock =
              targetFlow.leaderBlock.reference.toLeaderBlockRef ∧
            window.anchorBlock =
              anchorFlow.leaderBlock.reference.toLeaderBlockRef := by
  rcases recovery_timers_produce_timed_block_flow timed rules targetReady
      targetNextReady targetParentAvailability targetAboveGc targetStartsAfterGst with
    ⟨targetFlow, targetLeaderMatches, targetReceiverMatches,
      _targetTimerMatches, _targetNextTimerMatches⟩
  rcases recovery_timers_produce_timed_block_flow timed rules anchorReady
      anchorNextReady anchorParentAvailability anchorAboveGc anchorStartsAfterGst with
    ⟨anchorFlow, anchorLeaderMatches, anchorReceiverMatches,
      _anchorTimerMatches, _anchorNextTimerMatches⟩
  have targetHead := exact_selected_leader_order_head config commitHead
    targetRound targetLeader targetFirst
  have anchorHead := exact_selected_leader_order_head config commitHead
    (targetRound + depth) anchorLeader anchorFirst
  cases targetOrderShape : exactSelectedLeaderOrder config commitHead targetRound with
  | nil => simp [targetOrderShape] at targetHead
  | cons targetSlot targetTail =>
      have targetSlotMatches :
          targetSlot = { round := targetRound, validator := targetLeader } := by
        simpa [targetOrderShape] using targetHead
      subst targetSlot
      cases anchorOrderShape :
          exactSelectedLeaderOrder config commitHead (targetRound + depth) with
      | nil => simp [anchorOrderShape] at anchorHead
      | cons anchorSlot anchorTail =>
          have anchorSlotMatches :
              anchorSlot =
                { round := targetRound + depth, validator := anchorLeader } := by
            simpa [anchorOrderShape] using anchorHead
          subst anchorSlot
          let window : ExactFavorableRecoveryWindow BlockId
              (fun validator => faults.correctAvailable validator = true) :=
            { targetRound := targetRound
              depth := depth
              targetFirstSlot :=
                { round := targetRound, validator := targetLeader }
              targetTailOrder := targetTail
              anchorFirstSlot :=
                { round := targetRound + depth, validator := anchorLeader }
              anchorTailOrder := anchorTail
              targetFirstBlock :=
                targetFlow.leaderBlock.reference.toLeaderBlockRef
              anchorBlock :=
                anchorFlow.leaderBlock.reference.toLeaderBlockRef
              targetOrderAtRound := by
                intro slot included
                have inExactOrder : slot ∈
                    exactSelectedLeaderOrder config commitHead targetRound := by
                  simpa [targetOrderShape] using included
                exact exact_selected_leader_order_at_round config commitHead
                  targetRound inExactOrder
              anchorOrderAtRound := by
                intro slot included
                have inExactOrder : slot ∈
                    exactSelectedLeaderOrder config commitHead
                      (targetRound + depth) := by
                  simpa [anchorOrderShape] using included
                exact exact_selected_leader_order_at_round config commitHead
                  (targetRound + depth) inExactOrder
              targetBlockMatchesFirstSlot := by
                constructor
                · exact targetFlow.leaderBlockRound
                · exact targetFlow.leaderBlockAuthor.trans
                    targetLeaderMatches
              anchorBlockMatchesFirstSlot := by
                constructor
                · exact anchorFlow.leaderBlockRound
                · exact anchorFlow.leaderBlockAuthor.trans
                    anchorLeaderMatches
              anchorAuthorCorrect := by
                simpa [anchorFlow.leaderBlockAuthor, anchorLeaderMatches] using
                  anchorReady.validatorCorrectAvailable }
          refine ⟨targetFlow, anchorFlow, window, targetLeaderMatches,
            anchorLeaderMatches, ?_, ?_, rfl, rfl⟩
          · simp [window, ExactFavorableRecoveryWindow.targetOrder]
          · simp [window, ExactFavorableRecoveryWindow.anchorOrder]

/-- A correct validator has only one durable proposal reference in one round.
Thus, two pointwise timing proofs for different receivers name the same leader
block. -/
theorem timed_block_flows_same_leader_have_same_reference
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {timingProtocolPacket : Packet → Prop}
    {timingProtocolAction : LocalConsensusAction → Prop}
    {CommitPrefix : Type}
    {waits : CommonRoundWaitSchedule CommitPrefix}
    {commitHead : CommitPrefix} {round : Nat}
    (left right : ValidatorTimedBlockFlow config faults execution.trace
      (timingProtocolPacket := timingProtocolPacket)
      (timingProtocolAction := timingProtocolAction)
      waits commitHead round)
    (sameLeader : left.leader = right.leader) :
    left.leaderBlock.reference = right.leaderBlock.reference := by
  let finish := max left.proposal.action.completedAt
    right.proposal.action.completedAt
  have leftBeforeFinish : left.proposal.action.completedAt ≤ finish :=
    Nat.le_max_left _ _
  have rightBeforeFinish : right.proposal.action.completedAt ≤ finish :=
    Nat.le_max_right _ _
  have leftStored :=
    (execution.durable_fields_persist left.leaderInRange leftBeforeFinish)
      |>.own_block_persists left.proposalStoredAtCompletion
  have rightStored :=
    (execution.durable_fields_persist right.leaderInRange rightBeforeFinish)
      |>.own_block_persists right.proposalStoredAtCompletion
  have leftAtRound :
      ((execution.trace finish).validatorState left.leader).ownBlockAt round =
        some left.leaderBlock.reference := by
    simpa [left.leaderBlockRound] using leftStored
  have rightAtRound :
      ((execution.trace finish).validatorState left.leader).ownBlockAt round =
        some right.leaderBlock.reference := by
    simpa [sameLeader, right.leaderBlockRound] using rightStored
  exact Option.some.inj (leftAtRound.symm.trans rightAtRound)

/-- One correct next-round proposer becomes one exact direct voter at an actual
observer.

The theorem derives the leader flow, timely parent inclusion, the child send,
the observer's accepted representative, and the final `traceDirectVoters`
membership. It does not assume a direct vote or a quorum. -/
theorem recovery_timers_produce_observer_direct_vote
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
    (pipelineRules : ValidatorRecoveryProposalPipelineRules faults
      protocolPacket network program timed)
    (anchorRules : ValidatorAnchorLocalRules config faults
      timed.execution.trace)
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {commitHead : ValidatorCommitHead CommitId}
    {round leader voter observer : Nat}
    (leaderReady : ValidatorRecoveryProposalReady faults timed waits commitHead
      round leader)
    (voterReady : ValidatorRecoveryProposalReady faults timed waits commitHead
      (round + 1) voter)
    (leaderParentAvailability :
      ValidatorRecoveryBroadcastParentAvailability leaderReady voter)
    (leaderAboveGc :
      ValidatorRecoveryBroadcastAboveGcAfterDelivery leaderReady voter)
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (earliestRoundStart startSpread : Nat)
    (leaderStartWithinSpread :
      leaderReady.startedAt ≤ earliestRoundStart + startSpread)
    (voterStartsAfterEarliestDeadline :
      earliestRoundStart + waits.wait commitHead round ≤
        voterReady.startedAt)
    (waitDominatesSpread : startSpread ≤ waits.wait commitHead round)
    (leaderStartsAfterGst : network.gst ≤ leaderReady.startedAt)
    (nextWaitCoversVisibility :
      waits.wait commitHead round +
          (validatorProposalPipelineBound timed.localActionBound +
            network.delta +
            validatorProposalPipelineBound timed.localActionBound) ≤
        waits.wait commitHead (round + 1))
    (childParentAvailability : ∀
      (flow : ValidatorTimedBlockFlow config faults timed.execution.trace
        (timingProtocolPacket :=
          CorrectAvailableAddressedTimingPacket config faults protocolPacket)
        (timingProtocolAction :=
          TraceActionWithin
            (validatorProposalPipelineBound timed.localActionBound))
        waits commitHead round),
      flow.leader = leader →
      flow.receiver = voter →
      ValidatorProposalBroadcastParentsAcceptedAtDelivery flow.nextProposal
        observer)
    (childAboveGc : ∀
      (flow : ValidatorTimedBlockFlow config faults timed.execution.trace
        (timingProtocolPacket :=
          CorrectAvailableAddressedTimingPacket config faults protocolPacket)
        (timingProtocolAction :=
          TraceActionWithin
            (validatorProposalPipelineBound timed.localActionBound))
        waits commitHead round),
      flow.leader = leader →
      flow.receiver = voter →
      ValidatorProposalBroadcastAboveGcAfterDelivery flow.nextProposal
        observer) :
    ∃ finish,
      ∃ flow : ValidatorTimedBlockFlow config faults timed.execution.trace
          (timingProtocolPacket :=
            CorrectAvailableAddressedTimingPacket config faults protocolPacket)
          (timingProtocolAction :=
            TraceActionWithin
              (validatorProposalPipelineBound timed.localActionBound))
          waits commitHead round,
        leaderReady.startedAt ≤ finish ∧
          flow.leader = leader ∧
          flow.receiver = voter ∧
          traceDirectVoters (timed.execution.trace finish) observer
              flow.leaderBlock.reference voter = true := by
  rcases recovery_timers_produce_timed_block_flow timed pipelineRules leaderReady
      voterReady leaderParentAvailability leaderAboveGc leaderStartsAfterGst with
    ⟨flow, flowFacts⟩
  let timingNetwork := network.toTimingNetwork
  let pipelineBound := validatorProposalPipelineBound timed.localActionBound
  let processing := traceActionProcessing timingNetwork pipelineBound
  have flowLeaderWithinSpread :
      flow.leaderTimer.startedAt ≤ earliestRoundStart + startSpread := by
    rw [flowFacts.2.2.1]
    exact leaderStartWithinSpread
  have flowVoterStartsAfterDeadline :
      earliestRoundStart + waits.wait commitHead round ≤
        flow.nextTimer.startedAt := by
    rw [flowFacts.2.2.2]
    exact voterStartsAfterEarliestDeadline
  have flowLeaderStartsAfterGst :
      timingNetwork.gst ≤ flow.leaderTimer.startedAt := by
    rw [flowFacts.2.2.1]
    exact leaderStartsAfterGst
  have flowWaitCoversVisibility :
      waits.wait commitHead round +
          (processing.epsilon + timingNetwork.delta + processing.epsilon) ≤
        waits.wait commitHead (round + 1) := by
    exact nextWaitCoversVisibility
  have parentIncluded := anchorRules.timed_flow_includes_leader_from_spread
    timed.execution processing flow earliestRoundStart startSpread
    flowLeaderWithinSpread flowVoterStartsAfterDeadline waitDominatesSpread
    flowLeaderStartsAfterGst flowWaitCoversVisibility
  rcases broadcast_stored_recovery_proposal timed pipelineRules flow.nextProposal
      observerInRange observerCorrectAvailable with
    ⟨childPacketId, childPacket, childBroadcast⟩
  have leaderStartsBeforeVoter :
      leaderReady.startedAt ≤ voterReady.startedAt := by
    exact Nat.le_trans leaderStartWithinSpread
      (Nat.le_trans
        (Nat.add_le_add_left waitDominatesSpread earliestRoundStart)
        voterStartsAfterEarliestDeadline)
  have voterStartsAfterGst : network.gst ≤ voterReady.startedAt :=
    Nat.le_trans leaderStartsAfterGst leaderStartsBeforeVoter
  have voterDeadlineAfterGst :
      network.gst ≤ voterReady.timer.deadline waits := by
    apply Nat.le_trans voterStartsAfterGst
    change voterReady.startedAt ≤
      voterReady.startedAt + waits.wait commitHead (round + 1)
    exact Nat.le_add_right _ _
  have parentSnapshotAfterGst :
      network.gst ≤ flow.nextProposal.snapshotAt := by
    have deadlineBeforeSelection := flow.parentSelection.startsAfterDeadline
    rw [flow.parentSnapshotMatches, flowFacts.2.2.2] at deadlineBeforeSelection
    exact Nat.le_trans voterDeadlineAfterGst deadlineBeforeSelection
  have childSentAfterGst : network.gst ≤ childPacket.sentAt := by
    exact Nat.le_trans parentSnapshotAfterGst
      (Nat.le_trans flow.nextProposal.snapshotBeforeStore
        childBroadcast.storedBeforeSend)
  have childAccepted := completed_broadcast_is_accepted timed pipelineRules
    observerInRange observerCorrectAvailable childBroadcast
    (childParentAvailability flow flowFacts.1 flowFacts.2.1 childPacketId
      childPacket childBroadcast)
    (childAboveGc flow flowFacts.1 flowFacts.2.1 childPacketId childPacket
      childBroadcast)
    flow.nextProposalValidParents childSentAfterGst
  have childDeliveryBounds := network.postGstDelivery childPacket
    childBroadcast.packetIsProtocol
    (by simpa [childBroadcast.packetSender] using
      flow.nextProposal.proposerInRange)
    (by simpa [childBroadcast.packetReceiver] using observerInRange)
    (by simpa [childBroadcast.packetSender] using
      flow.nextProposal.proposerCorrectAvailable)
    (by simpa [childBroadcast.packetReceiver] using
      observerCorrectAvailable)
    childSentAfterGst
  let finish :=
    childPacket.deliveredAt + 1 + timed.localActionBound + 1
  have storedBeforeFinish : flow.nextProposal.storedAt ≤ finish := by
    exact Nat.le_trans childBroadcast.storedBeforeSend
      (Nat.le_trans childDeliveryBounds.1
        (Nat.le_trans (Nat.le_add_right _ 1)
          (Nat.le_trans (Nat.le_add_right _ timed.localActionBound)
            (Nat.le_add_right _ 1))))
  have childInCatalog :
      (timed.execution.trace finish).blockCatalog
          flow.nextProposal.block.reference.id =
        some flow.nextProposal.block := by
    exact timed.execution.blockCatalogMonotone flow.nextProposal.storedAt finish
      storedBeforeFinish flow.nextProposal.block.reference.id
      flow.nextProposal.block flow.nextProposal.blockInCatalog
  have childAuthorInRange :
      flow.nextProposal.block.reference.author < config.authorityCount := by
    simpa [flow.nextProposal.blockIsOwnProposal, flow.nextProposalOwner,
      flowFacts.2.1] using voterReady.validatorInRange
  have childAuthorCorrect :
      faults.correctAvailable flow.nextProposal.block.reference.author = true := by
    simpa [flow.nextProposal.blockIsOwnProposal, flow.nextProposalOwner,
      flowFacts.2.1] using voterReady.validatorCorrectAvailable
  have childRepresentative :=
    anchorRules.acceptedCorrectBlockRecordsRepresentative finish observer
      flow.nextProposal.block observerInRange observerCorrectAvailable
      childAuthorInRange childAuthorCorrect childInCatalog (by
        simpa [finish] using childAccepted)
  have childAuthor : flow.nextProposal.block.reference.author = voter := by
    exact flow.nextProposal.blockIsOwnProposal.trans
      (flow.nextProposalOwner.trans flowFacts.2.1)
  have childRound :
      flow.nextProposal.block.reference.round =
        flow.leaderBlock.reference.round + 1 := by
    rw [flow.nextProposalRound, flow.leaderBlockRound]
  have childRepresentativeAtVoteSlot :
      ValidatorLocalState.acceptedRepresentative
          ((timed.execution.trace finish).validatorState observer)
          (flow.leaderBlock.reference.round + 1) voter =
        some flow.nextProposal.block.reference := by
    simpa [childRound, childAuthor] using childRepresentative
  have directVote :
      traceDirectVoters (timed.execution.trace finish) observer
          flow.leaderBlock.reference voter = true := by
    apply accepted_child_with_leader_parent_is_direct_voter
      childRepresentativeAtVoteSlot childInCatalog rfl childAuthor childRound
      parentIncluded
  have voterStartBeforeSnapshot :
      voterReady.startedAt ≤ flow.nextProposal.snapshotAt := by
    apply Nat.le_trans (Nat.le_add_right voterReady.startedAt
      (waits.wait commitHead (round + 1)))
    rw [← flow.parentSnapshotMatches]
    change voterReady.timer.deadline waits ≤ flow.parentSelection.snapshotAt
    rw [← flowFacts.2.2.2]
    exact flow.parentSelection.startsAfterDeadline
  have leaderStartBeforeFinish : leaderReady.startedAt ≤ finish := by
    exact Nat.le_trans leaderStartsBeforeVoter
      (Nat.le_trans voterStartBeforeSnapshot
        (Nat.le_trans flow.nextProposal.snapshotBeforeStore storedBeforeFinish))
  exact ⟨finish, flow, leaderStartBeforeFinish, flowFacts.1,
    flowFacts.2.1, directVote⟩

end Mysticeti
