/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ReferenceFlexIndexedListBridge
import Mysticeti.ValidatorFlexCommitter
import Mysticeti.ValidatorTraceFavorableWindow

namespace Mysticeti

/-! Exact-reference FlexCommitter progress from one validator trace.

The source records in this module describe one host. They do not assume a
future block layer, vote quorum, anchor window, committer result, or commit.
The trace proof supplies accepted exact blocks and direct-vote stake. This
module maps those facts to the Rust-order direct and indirect scan.
-/

/-- Accepted blocks strictly below a later accepted round occur in the exact
pending array that the next FlexCommitter call reads.

Rust does not put the current highest accepted round in the pending array. The
`higherAccepted` argument is the local evidence that `block` is below that
boundary. -/
structure ValidatorExactPendingIngestionRules
    {BlockId CommitId History Encoding : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (source : LocalFlexCommitterSourceMap config functions context program) where
  acceptedBelowHighestIsPending : ∀ validator
      (state : ValidatorLocalState BlockId CommitId)
      (block : ValidatorBlockRef BlockId),
    state.accepted block = true →
    state.commitHead.round < block.round →
    (∃ higherBlock : ValidatorBlockRef BlockId,
      state.accepted higherBlock = true ∧ block.round < higherBlock.round) →
    ∃ index,
      index < (source.snapshot validator state).pending.roundCount ∧
      ((source.snapshot validator state).pending.rounds index).round = block.round

/-- The exact local direct rule keeps or creates the commit result proved by
one direct-vote quorum. -/
structure ValidatorExactDirectRuleSourceMap
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (source : LocalFlexCommitterSourceMap config functions context program) where
  quorumCommitIsPostDirect : ∀
      (world : ValidatorWorldState BlockId CommitId PacketId) validator
      (view : ReferenceSelectedSlotView BlockId)
      (block : ValidatorBlockRef BlockId),
    block.toLeaderBlockRef.AtSelectedSlot view.slot →
    (world.validatorState validator).accepted block = true →
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters world validator block) →
    (applyReferenceDirectDecision
        (context validator (world.validatorState validator)).directRule
        view).status = .commit block.toLeaderBlockRef

/-- Commit indexes do not decrease inside one event batch. -/
private theorem validator_world_step_commit_index_mono
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (validator : Nat) :
    (before.validatorState validator).commitHead.index ≤
      (after.validatorState validator).commitHead.index := by
  induction step with
  | nil => exact Nat.le_refl _
  | cons firstStep remainingSteps inductionHypothesis =>
      exact Nat.le_trans
        (validator_atomic_step_durable_monotone firstStep validator).1
        inductionHypothesis

/-- Durable local fields compose across two state transitions. -/
private theorem validator_durable_state_monotone_trans
    {BlockId CommitId : Type}
    {before middle after : ValidatorLocalState BlockId CommitId}
    (first : ValidatorDurableStateMonotone before middle)
    (second : ValidatorDurableStateMonotone middle after) :
    ValidatorDurableStateMonotone before after := by
  rcases first with
    ⟨firstIndex, firstRound, firstHead, firstInstalled, firstSource,
      firstCommitTime, firstSigned, firstOwn, firstSent, firstAccepted,
      firstRepresentative, firstGc⟩
  rcases second with
    ⟨secondIndex, secondRound, secondHead, secondInstalled, secondSource,
      secondCommitTime, secondSigned, secondOwn, secondSent, secondAccepted,
      secondRepresentative, secondGc⟩
  refine ⟨Nat.le_trans firstIndex secondIndex,
    Nat.le_trans firstRound secondRound, ?_, ?_, ?_,
    Nat.le_trans firstCommitTime secondCommitTime,
    Nat.le_trans firstSigned secondSigned, ?_, ?_, ?_, ?_,
    Nat.le_trans firstGc secondGc⟩
  · intro sameIndex
    have firstSame : before.commitHead.index = middle.commitHead.index := by
      omega
    have secondSame : middle.commitHead.index = after.commitHead.index := by
      omega
    exact (firstHead firstSame).trans (secondHead secondSame)
  · intro index commitId installed
    exact secondInstalled index commitId
      (firstInstalled index commitId installed)
  · intro index installSource recorded
    exact secondSource index installSource
      (firstSource index installSource recorded)
  · intro round reference stored
    exact secondOwn round reference (firstOwn round reference stored)
  · intro round sent
    exact secondSent round (firstSent round sent)
  · intro reference accepted
    exact secondAccepted reference (firstAccepted reference accepted)
  · intro round author reference present
    exact secondRepresentative round author reference
      (firstRepresentative round author reference present)

/-- One event batch preserves all durable local fields. -/
private theorem validator_world_step_durable_monotone
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (validator : Nat) :
    ValidatorDurableStateMonotone
      (before.validatorState validator) (after.validatorState validator) := by
  induction step with
  | nil =>
      simp [ValidatorDurableStateMonotone, OptionMapMonotone,
        BoolMapMonotone, BinaryOptionMapMonotone]
  | cons firstStep remainingSteps inductionHypothesis =>
      exact validator_durable_state_monotone_trans
        (validator_atomic_step_durable_monotone firstStep validator)
        inductionHypothesis

/-- Equal boundary indexes force the same exact commit head through one event
batch. -/
private theorem validator_world_step_same_commit_head
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (validator : Nat)
    (sameIndex : before.localCommitIndex validator =
      after.localCommitIndex validator) :
    (before.validatorState validator).commitHead =
      (after.validatorState validator).commitHead := by
  change (before.validatorState validator).commitHead.index =
    (after.validatorState validator).commitHead.index at sameIndex
  exact (validator_world_step_durable_monotone step validator).2.2.1 sameIndex

/-- Accepted representatives persist through one event batch. -/
private theorem validator_world_step_accepted_representative_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {validator round author : Nat} {reference : ValidatorBlockRef BlockId}
    (present : (before.validatorState validator).acceptedRepresentative
      round author = some reference) :
    (after.validatorState validator).acceptedRepresentative round author =
      some reference := by
  induction step with
  | nil => exact present
  | cons firstStep remainingSteps inductionHypothesis =>
      exact inductionHypothesis
        ((validator_atomic_step_durable_monotone firstStep validator)
          |>.accepted_representative_persists present)

/-- Block catalog entries persist through one event batch. -/
private theorem validator_world_step_catalog_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {blockId : BlockId} {block : ValidatorBlock BlockId}
    (present : before.blockCatalog blockId = some block) :
    after.blockCatalog blockId = some block := by
  induction step with
  | nil => exact present
  | cons firstStep remainingSteps inductionHypothesis =>
      exact inductionHypothesis
        ((validator_atomic_step_history_monotone firstStep).1 blockId block
          present)

/-- One exact direct-voter fact persists through one event batch. -/
private theorem validator_world_step_direct_voter_persists
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {observer voter : Nat} {leader : ValidatorBlockRef BlockId}
    (vote : traceDirectVoters before observer leader voter = true) :
    traceDirectVoters after observer leader voter = true := by
  unfold traceDirectVoters at vote ⊢
  cases representative :
      (before.validatorState observer).acceptedRepresentative
        (leader.round + 1) voter with
  | none => simp [representative] at vote
  | some childReference =>
      have representativeAfter :=
        validator_world_step_accepted_representative_persists step
          representative
      cases catalog : before.blockCatalog childReference.id with
      | none => simp [representative, catalog] at vote
      | some child =>
          have catalogAfter := validator_world_step_catalog_persists step catalog
          simpa [representativeAfter, catalogAfter] using
            (show decide
                (child.reference = childReference ∧
                  childReference.author = voter ∧
                  childReference.round = leader.round + 1 ∧
                  leader ∈ child.parents) = true by
              simpa [representative, catalog] using vote)

/-- Direct-vote stake does not decrease through one event batch. -/
private theorem validator_world_step_direct_voter_weight_mono
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {observer : Nat} {leader : ValidatorBlockRef BlockId} :
    weight config.authorityCount config.stake
        (traceDirectVoters before observer leader) ≤
      weight config.authorityCount config.stake
        (traceDirectVoters after observer leader) := by
  apply weight_mono config.stake
  intro voter _inRange voted
  exact validator_world_step_direct_voter_persists step voted

/-- A direct-vote quorum contains one locally accepted block from the next
round. This fact supplies the strict upper bound used by Rust when it builds
the pending-round array. -/
private theorem direct_quorum_has_higher_accepted_block
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {observer : Nat} {leader : ValidatorBlockRef BlockId}
    (observerInRange : observer < config.authorityCount)
    (worldWellFormed : ValidatorWorldStateWellFormed (config := config) world)
    (directQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters world observer leader)) :
    ∃ higherBlock : ValidatorBlockRef BlockId,
      (world.validatorState observer).accepted higherBlock = true ∧
        leader.round < higherBlock.round := by
  have positiveWeight : 0 < weight config.authorityCount config.stake
      (traceDirectVoters world observer leader) :=
    Nat.lt_of_lt_of_le config.thresholds.quorum_positive directQuorum
  rcases positive_weight_has_member positiveWeight with
    ⟨voter, _voterInRange, voted, _positiveStake⟩
  cases representative : (world.validatorState observer).acceptedRepresentative
      (leader.round + 1) voter with
  | none =>
      simp [traceDirectVoters, representative] at voted
  | some child =>
      have childSound := (worldWellFormed observer observerInRange)
        |>.acceptedRepresentativeIsSound (leader.round + 1) voter child
          representative
      exact ⟨child, childSound.2.2.1, by omega⟩

/-- A consecutive accepted direct-quorum range becomes an exact post-direct
anchor window in the local Rust-order pending array. -/
theorem trace_direct_quorum_range_gives_exact_reference_anchor_window
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (source : LocalFlexCommitterSourceMap config functions context program)
    (pending : ValidatorExactPendingIngestionRules source)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    {observer baseRound count : Nat}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (countPositive : 0 < count)
    (baseAfterCommitHead :
      (world.validatorState observer).commitHead.round < baseRound)
    (leaderRound : ∀ offset, offset < count →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset, offset < count →
      (config.selectedLeaderOrder
        (world.validatorState observer).commitHead.id
        (baseRound + offset)).head? = some (leaderAt offset).author)
    (leaderAccepted : ∀ offset, offset < count →
      (world.validatorState observer).accepted (leaderAt offset) = true)
    (higherAccepted : ∀ offset, offset < count →
      ∃ higherBlock : ValidatorBlockRef BlockId,
        (world.validatorState observer).accepted higherBlock = true ∧
          (leaderAt offset).round < higherBlock.round)
    (directQuorum : ∀ offset, offset < count →
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters world observer (leaderAt offset))) :
    let state := world.validatorState observer
    let input := source.snapshot observer state
    let postDirect := runReferenceDirectPass
      (context observer state).directRule input.pending
    ∃ baseIndex,
      baseIndex + count ≤ postDirect.roundCount ∧
        ReferenceAnchorWindow postDirect baseIndex count := by
  dsimp only
  let state := world.validatorState observer
  let input := source.snapshot observer state
  let postDirect := runReferenceDirectPass
    (context observer state).directRule input.pending
  have stateValid := source.tryCommitStateValid observer state
  have baseAccepted := leaderAccepted 0 countPositive
  have basePending := pending.acceptedBelowHighestIsPending observer state
    (leaderAt 0) baseAccepted (by
      rw [leaderRound 0 countPositive]
      simpa using baseAfterCommitHead) (higherAccepted 0 countPositive)
  rcases basePending with ⟨baseIndex, baseInRange, baseRoundAtIndex⟩
  have baseIndexRound : (input.pending.rounds baseIndex).round = baseRound := by
    simpa [input, state, leaderRound 0 countPositive] using baseRoundAtIndex
  have firstPendingAtBase :
      stateValid.firstPendingRound + baseIndex = baseRound := by
    have consecutive := stateValid.roundsConsecutive baseIndex baseInRange
    rw [baseIndexRound] at consecutive
    exact consecutive.symm
  have indexForOffset : ∀ offset, offset < count →
      baseIndex + offset < input.pending.roundCount := by
    intro offset offsetInRange
    have accepted := leaderAccepted offset offsetInRange
    have pendingAtOffset := pending.acceptedBelowHighestIsPending observer state
      (leaderAt offset) accepted (by
        rw [leaderRound offset offsetInRange]
        exact Nat.lt_of_lt_of_le baseAfterCommitHead
          (Nat.le_add_right baseRound offset))
      (higherAccepted offset offsetInRange)
    rcases pendingAtOffset with ⟨index, indexInRange, roundAtIndex⟩
    have consecutive := stateValid.roundsConsecutive index indexInRange
    have blockRound := leaderRound offset offsetInRange
    dsimp [input, state] at roundAtIndex consecutive
    rw [roundAtIndex, blockRound] at consecutive
    have indexMatches : index = baseIndex + offset := by omega
    simpa [indexMatches] using indexInRange
  have covered : baseIndex + count ≤ input.pending.roundCount := by
    have lastInRange := indexForOffset (count - 1) (by omega)
    omega
  refine ⟨baseIndex, ?_, ?_⟩
  · simpa [postDirect, runReferenceDirectPass] using covered
  · intro offset offsetInRange
    let index := baseIndex + offset
    have indexInRange : index < input.pending.roundCount := by
      exact indexForOffset offset offsetInRange
    let roundView := input.pending.rounds index
    have pendingRound : roundView.round = baseRound + offset := by
      have consecutive := stateValid.roundsConsecutive index indexInRange
      dsimp [roundView]
      rw [consecutive]
      dsimp [index]
      omega
    have slotsMatch := source.selectedSlotsMatch observer state index
      indexInRange
    have slotsMatchConfig := source.selectedSlotsMatchConfig observer state index
      indexInRange
    let selectedSlot : ExactSelectedLeaderSlot :=
      { round := baseRound + offset
        validator := (leaderAt offset).author }
    have selectedHead :
        (roundView.selectedSlots.map ReferenceSelectedSlotView.slot).head? =
          some selectedSlot := by
      have configuredHead := exact_selected_leader_order_head config
        (world.validatorState observer).commitHead
        (baseRound + offset) (leaderAt offset).author
        (firstSelected offset offsetInRange)
      rw [slotsMatch, slotsMatchConfig]
      rw [pendingRound]
      simpa [state, selectedSlot, exactSelectedLeaderOrder] using configuredHead
    cases slotsShape : roundView.selectedSlots with
    | nil => simp [slotsShape] at selectedHead
    | cons first tail =>
        have firstSlot : first.slot = selectedSlot := by
          simpa [slotsShape] using selectedHead
        have blockAtFirst : (leaderAt offset).toLeaderBlockRef.AtSelectedSlot
            first.slot := by
          rw [firstSlot]
          exact ⟨by simp [selectedSlot, leaderRound offset offsetInRange],
            by simp [selectedSlot]⟩
        have firstCommitted := direct.quorumCommitIsPostDirect world observer
          first (leaderAt offset) blockAtFirst
          (leaderAccepted offset offsetInRange)
          (directQuorum offset offsetInRange)
        refine ⟨(leaderAt offset).toLeaderBlockRef, ?_⟩
        have firstCommittedAtState :
            (applyReferenceDirectDecision
              (context observer state).directRule first).status =
                .commit (leaderAt offset).toLeaderBlockRef := by
          simpa [state] using firstCommitted
        change scanReferenceSelectedSlots
            ((runReferenceDirectRound
              (context observer state).directRule roundView).selectedSlots) =
          .found (leaderAt offset).toLeaderBlockRef
        simp only [runReferenceDirectRound, slotsShape, List.map_cons,
          scanReferenceSelectedSlots]
        rw [firstCommittedAtState]

/-- The same exact anchor range makes the complete Rust-order local scan return
one source-valid next-index output. -/
theorem trace_direct_quorum_range_gives_exact_reference_output
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (source : LocalFlexCommitterSourceMap config functions context program)
    (pending : ValidatorExactPendingIngestionRules source)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    {observer baseRound : Nat}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (baseAfterCommitHead :
      (world.validatorState observer).commitHead.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context observer
        (world.validatorState observer)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context observer
        (world.validatorState observer)).depth + 1 →
      (config.selectedLeaderOrder
        (world.validatorState observer).commitHead.id
        (baseRound + offset)).head? = some (leaderAt offset).author)
    (leaderAccepted : ∀ offset,
      offset < (context observer
        (world.validatorState observer)).depth + 1 →
      (world.validatorState observer).accepted (leaderAt offset) = true)
    (higherAccepted : ∀ offset,
      offset < (context observer
        (world.validatorState observer)).depth + 1 →
      ∃ higherBlock : ValidatorBlockRef BlockId,
        (world.validatorState observer).accepted higherBlock = true ∧
          (leaderAt offset).round < higherBlock.round)
    (directQuorum : ∀ offset,
      offset < (context observer
        (world.validatorState observer)).depth + 1 →
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters world observer (leaderAt offset))) :
    ∃ output,
      tryReferenceFlexCommitWithContext functions
          (context observer (world.validatorState observer))
          (source.snapshot observer (world.validatorState observer)) = some output ∧
        output.reference.index =
          (world.validatorState observer).commitHead.index + 1 := by
  let state := world.validatorState observer
  let localContext := context observer state
  have anchorResult :=
    trace_direct_quorum_range_gives_exact_reference_anchor_window
      (faults := faults) (count := localContext.depth + 1)
      world source pending direct leaderAt (by omega) baseAfterCommitHead leaderRound
      firstSelected leaderAccepted higherAccepted directQuorum
  dsimp only [state, localContext] at anchorResult
  rcases anchorResult with ⟨baseIndex, covered, anchorWindow⟩
  have coveredAtLocalState : baseIndex + (localContext.depth + 1) ≤
      (runReferenceDirectPass localContext.directRule
        (source.snapshot observer state).pending).roundCount := by
    simpa [state, localContext] using covered
  have anchorWindowAtLocalState : ReferenceAnchorWindow
      (runReferenceDirectPass localContext.directRule
        (source.snapshot observer state).pending)
      baseIndex (localContext.depth + 1) := by
    simpa [state, localContext] using anchorWindow
  have windowInRange : baseIndex + localContext.depth <
      (runReferenceDirectPass localContext.directRule
        (source.snapshot observer state).pending).roundCount := by
    omega
  rcases reference_anchor_window_gives_source_valid_exact_output functions
      localContext source.materializer state
      (source.snapshot observer state)
      (source.tryCommitStateValid observer state)
      (source.tryCommitMaterialValid observer state)
      anchorWindowAtLocalState windowInRange with
    ⟨output, found, _returned, _complete, nextIndex, _builder⟩
  have priorMatches := source.priorMatchesHead observer state
  refine ⟨output, found, ?_⟩
  rw [nextIndex, priorMatches.1]

/-- A concrete exact anchor range runs the protected local committer, unless a
concurrent action has already installed the next commit index.

The inputs describe one validator at one trace time. The anchor range is not a
theorem input: accepted exact blocks and their direct-vote stake supply it. -/
theorem trace_direct_quorum_range_runs_exact_committer_or_installed_next
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (pending : ValidatorExactPendingIngestionRules source)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    (work : ValidatorSuccessfulFlexScanWorkRules (faults := faults)
      (network := network) (timed := timed) source)
    {start observer baseRound : Nat}
    {prior : ValidatorCommitHead CommitId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (observerInRange : observer < config.authorityCount)
    (observerCorrectAvailable : faults.correctAvailable observer = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState observer).commitHead = prior)
    (baseAfterCommitHead : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (leaderAccepted : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      ((timed.execution.trace start).validatorState observer).accepted
        (leaderAt offset) = true)
    (directQuorum : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (timed.execution.trace start) observer
            (leaderAt offset))) :
    (∃ (observation : LocalFlexCommitterRunObservation BlockId CommitId)
        (output : LocalFlexCommitOutput BlockId CommitId),
      start ≤ observation.time ∧
        observation.validator = observer ∧
        runtime.returned observation ∧
        observation.result = some output ∧
        observation.input.commitHead = prior ∧
        output.toCommitHead.index = prior.index + 1) ∨
      (∃ finish witnessId,
        start ≤ finish ∧
          ((timed.execution.trace finish).validatorState observer).installedCommitAt
            (prior.index + 1) = some witnessId) := by
  let startWorld := timed.execution.trace start
  let startState := startWorld.validatorState observer
  have baseAfterStartHead : startState.commitHead.round < baseRound := by
    change ((timed.execution.trace start).validatorState observer).commitHead.round <
      baseRound
    rw [headAtStart]
    exact baseAfterCommitHead
  have higherAcceptedAtStart : ∀ offset,
      offset < (context observer startState).depth + 1 →
      ∃ higherBlock : ValidatorBlockRef BlockId,
        startState.accepted higherBlock = true ∧
          (leaderAt offset).round < higherBlock.round := by
    intro offset offsetInRange
    exact direct_quorum_has_higher_accepted_block observerInRange
      (timed.execution.statesWellFormed start)
      (directQuorum offset (by simpa [startState] using offsetInRange))
  rcases trace_direct_quorum_range_gives_exact_reference_output
      (faults := faults) startWorld source pending direct leaderAt
      baseAfterStartHead leaderRound
      (by
        intro offset offsetInRange
        simpa [startWorld, startState, headAtStart] using
          firstSelected offset offsetInRange)
      leaderAccepted higherAcceptedAtStart directQuorum with
    ⟨_initialOutput, initialResult, _initialNextIndex⟩
  have protectedWork := work.successfulScanIsProtected start observer
    _initialOutput (by simpa [startWorld, startState] using initialResult)
  let completion := timed.completeProtectedAction observer .runCommitter start
    observerInRange observerCorrectAvailable protectedWork
  let actionAt := completion.event.completedAt
  have startBeforeAction : start ≤ actionAt := by
    simpa [completion, actionAt, completion.sameEnableTime] using
      completion.enableBeforeCompletion
  have actionOccurs : ValidatorLocalActionOccurs
      (timed.execution.events actionAt) observer .runCommitter := by
    simpa [actionAt, completion.sameValidator, completion.sameAction] using
      completion.occurs
  rcases actionOccurs with ⟨beforeEvents, afterEvents, eventSplit⟩
  have batchStep := timed.execution.stepsFollowRules actionAt
  rw [eventSplit] at batchStep
  rcases validator_world_step_append_split batchStep with
    ⟨actionBefore, prefixStep, actionAndSuffix⟩
  cases actionAndSuffix with
  | cons actionStep suffixStep =>
      have startBeforeFinish : start ≤ actionAt + 1 :=
        Nat.le_trans startBeforeAction (Nat.le_add_right actionAt 1)
      by_cases advanced : prior.index <
          (timed.execution.trace (actionAt + 1)).localCommitIndex observer
      · have nextAtOrBelowHead : prior.index + 1 ≤
            (timed.execution.trace (actionAt + 1)).localCommitIndex observer := by
          omega
        rcases prefixMap.installedAtOrBelowHead (actionAt + 1) observer
            (prior.index + 1) observerInRange observerCorrectAvailable
            nextAtOrBelowHead with ⟨witnessId, installed⟩
        exact Or.inr ⟨actionAt + 1, witnessId, startBeforeFinish, installed⟩
      · have startToFinish := timed.execution.durable_fields_persist
          observerInRange startBeforeFinish
        have startIndex : startWorld.localCommitIndex observer = prior.index := by
          simp [startWorld, ValidatorWorldState.localCommitIndex, headAtStart]
        have finalIndex :
            (timed.execution.trace (actionAt + 1)).localCommitIndex observer =
              prior.index := by
          have notDecreased := startToFinish.1
          rw [headAtStart] at notDecreased
          change
            ((timed.execution.trace (actionAt + 1)).validatorState observer).commitHead.index =
              prior.index
          change ¬prior.index <
            ((timed.execution.trace (actionAt + 1)).validatorState observer).commitHead.index
              at advanced
          omega
        have startToAction := timed.execution.durable_fields_persist
          observerInRange startBeforeAction
        have actionToFinish := timed.execution.durable_fields_persist
          observerInRange (Nat.le_add_right actionAt 1)
        have actionIndex :
            (timed.execution.trace actionAt).localCommitIndex observer =
              prior.index := by
          have fromStart := startToAction.1
          have toFinish := actionToFinish.1
          rw [headAtStart] at fromStart
          change ((timed.execution.trace actionAt).validatorState observer).commitHead.index ≤
            ((timed.execution.trace (actionAt + 1)).validatorState observer).commitHead.index
              at toFinish
          change
            ((timed.execution.trace (actionAt + 1)).validatorState observer).commitHead.index =
              prior.index at finalIndex
          change ((timed.execution.trace actionAt).validatorState observer).commitHead.index =
            prior.index
          omega
        have headAtAction :
            ((timed.execution.trace actionAt).validatorState observer).commitHead =
              prior := by
          have sameBoundaryIndex :
              ((timed.execution.trace start).validatorState observer).commitHead.index =
                ((timed.execution.trace actionAt).validatorState observer).commitHead.index := by
            rw [headAtStart]
            change ((timed.execution.trace actionAt).validatorState observer).commitHead.index =
              prior.index at actionIndex
            exact actionIndex.symm
          exact (startToAction.2.2.1 sameBoundaryIndex).symm.trans headAtStart
        have prefixIndexMonotone :=
          validator_world_step_commit_index_mono prefixStep observer
        have actionIndexMonotone :=
          (validator_atomic_step_durable_monotone actionStep observer).1
        have suffixIndexMonotone :=
          validator_world_step_commit_index_mono suffixStep observer
        have actionBeforeIndex :
            actionBefore.localCommitIndex observer = prior.index := by
          change (actionBefore.validatorState observer).commitHead.index =
            prior.index
          change ((timed.execution.trace actionAt).validatorState observer).commitHead.index ≤
            (actionBefore.validatorState observer).commitHead.index at prefixIndexMonotone
          have beforeToFinish := Nat.le_trans actionIndexMonotone
            suffixIndexMonotone
          change (actionBefore.validatorState observer).commitHead.index ≤
            ((timed.execution.trace (actionAt + 1)).validatorState observer).commitHead.index at beforeToFinish
          change ((timed.execution.trace actionAt).validatorState observer).commitHead.index =
            prior.index at actionIndex
          change
            ((timed.execution.trace (actionAt + 1)).validatorState observer).commitHead.index =
              prior.index at finalIndex
          omega
        have prefixSameIndex :
            (timed.execution.trace actionAt).localCommitIndex observer =
              actionBefore.localCommitIndex observer := by
          rw [actionIndex, actionBeforeIndex]
        have headAtActionBefore :
            (actionBefore.validatorState observer).commitHead = prior := by
          exact (validator_world_step_same_commit_head prefixStep observer
            prefixSameIndex).symm.trans headAtAction
        have depthSame := source.contextDepthStable observer startState observer
          (actionBefore.validatorState observer)
        have depthSameTrace :
            (context observer
              ((timed.execution.trace start).validatorState observer)).depth =
              (context observer
                (actionBefore.validatorState observer)).depth := by
          simpa [startState, startWorld] using depthSame
        have leaderRoundAtAction : ∀ offset,
            offset < (context observer
              (actionBefore.validatorState observer)).depth + 1 →
            (leaderAt offset).round = baseRound + offset := by
          intro offset offsetInRange
          apply leaderRound offset
          rw [depthSameTrace]
          exact offsetInRange
        have firstSelectedAtAction : ∀ offset,
            offset < (context observer
              (actionBefore.validatorState observer)).depth + 1 →
            (config.selectedLeaderOrder
              (actionBefore.validatorState observer).commitHead.id
              (baseRound + offset)).head? = some (leaderAt offset).author := by
          intro offset offsetInRange
          rw [headAtActionBefore]
          apply firstSelected offset
          rw [depthSameTrace]
          exact offsetInRange
        have acceptedAtAction : ∀ offset,
            offset < (context observer
              (actionBefore.validatorState observer)).depth + 1 →
            (actionBefore.validatorState observer).accepted
              (leaderAt offset) = true := by
          intro offset offsetInRange
          have offsetAtStart : offset < (context observer startState).depth + 1 :=
            by
              rw [depthSame]
              exact offsetInRange
          have acceptedAtTraceAction := timed.execution.accepted_block_persists
            observerInRange startBeforeAction
            (leaderAccepted offset (by simpa [startState] using offsetAtStart))
          exact (validator_world_step_durable_monotone prefixStep observer)
            |>.accepted_block_persists acceptedAtTraceAction
        have higherAcceptedAtAction : ∀ offset,
            offset < (context observer
              (actionBefore.validatorState observer)).depth + 1 →
            ∃ higherBlock : ValidatorBlockRef BlockId,
              (actionBefore.validatorState observer).accepted higherBlock = true ∧
                (leaderAt offset).round < higherBlock.round := by
          intro offset offsetInRange
          have offsetAtStart : offset < (context observer startState).depth + 1 := by
            rw [depthSame]
            exact offsetInRange
          rcases higherAcceptedAtStart offset offsetAtStart with
            ⟨higherBlock, acceptedAtStart, higherRound⟩
          have acceptedAtTraceAction := timed.execution.accepted_block_persists
            observerInRange startBeforeAction acceptedAtStart
          have acceptedBeforeAction :=
            (validator_world_step_durable_monotone prefixStep observer)
              |>.accepted_block_persists acceptedAtTraceAction
          exact ⟨higherBlock, acceptedBeforeAction, higherRound⟩
        have quorumAtAction : ∀ offset,
            offset < (context observer
              (actionBefore.validatorState observer)).depth + 1 →
            config.thresholds.quorum ≤
              weight config.authorityCount config.stake
                (traceDirectVoters actionBefore observer (leaderAt offset)) := by
          intro offset offsetInRange
          have offsetAtStart : offset < (context observer startState).depth + 1 :=
            by
              rw [depthSame]
              exact offsetInRange
          exact Nat.le_trans
            (directQuorum offset (by simpa [startState] using offsetAtStart))
            (Nat.le_trans
              (trace_direct_voter_weight_mono timed.execution observerInRange
                startBeforeAction)
              (validator_world_step_direct_voter_weight_mono prefixStep))
        have baseAfterActionHead :
            (actionBefore.validatorState observer).commitHead.round <
              baseRound := by
          rw [headAtActionBefore]
          exact baseAfterCommitHead
        rcases trace_direct_quorum_range_gives_exact_reference_output
            (faults := faults) actionBefore source pending direct leaderAt
            baseAfterActionHead leaderRoundAtAction firstSelectedAtAction
            acceptedAtAction higherAcceptedAtAction quorumAtAction with
          ⟨output, exactResult, nextIndex⟩
        let observation : LocalFlexCommitterRunObservation BlockId CommitId :=
          { time := actionAt
            validator := observer
            input := actionBefore.validatorState observer
            result := tryReferenceFlexCommitWithContext functions
              (context observer (actionBefore.validatorState observer))
              (source.snapshot observer
                (actionBefore.validatorState observer)) }
        have observationOccurs : observation.OccursIn timed := by
          exact ⟨beforeEvents, afterEvents, actionBefore, _, eventSplit,
            prefixStep, actionStep, suffixStep, rfl⟩
        have returned : runtime.returned observation := by
          simpa [observation] using
            runtime.runCommitterOccurrenceReturnsExactResult observation
              observationOccurs
        have successful : observation.result = some output := by
          simpa [observation] using exactResult
        have outputNext : output.toCommitHead.index = prior.index + 1 := by
          simp only [LocalFlexCommitOutput.toCommitHead]
          rw [nextIndex, headAtActionBefore]
        exact Or.inl ⟨observation, output, by simp [observation,
          startBeforeAction], by simp [observation], returned, successful,
          headAtActionBefore, outputNext⟩

end Mysticeti
