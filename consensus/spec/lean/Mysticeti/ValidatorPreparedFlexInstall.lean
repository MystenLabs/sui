/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Lean.Elab.Tactic.Omega
import Mysticeti.ValidatorFlexPendingRefresh
import Mysticeti.ValidatorLocalDagSuccessorLiveness

namespace Mysticeti

/-!
Actual local Flex execution from one prepared receiver state.

The new source map contains only facts about the current validator state. A
successful deterministic prepared scan protects one local `runCommitter`
action. It does not assume a future run, commit install, block layer, or common
commit chain.

A commit install is an in-trace restart point for this proof. A host process
restart starts a new execution trace with the existing empty-cache rule. The
validator execution model does not have a process-restart atomic event.
-/

/-- A current-state observation used only to build the deterministic prepared
scan input. It is not a returned runtime observation. -/
def currentPreparedFlexObservation
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Nat) :
    LocalFlexCommitterRunObservation BlockId CommitId :=
  { time
    validator
    input := (timed.execution.trace time).validatorState validator
    result := none }

/-- Current local preparation and work-latch facts for the corrected Flex scan.

These fields describe deterministic work at one host. The existing pending
refresh source map remains the source for each actual `runCommitter` action. -/
structure ValidatorCurrentPreparedFlexSourceMap
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {faults : FixedFaultInterval config}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (pendingSource : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule) where
  /-- The prepared input is valid in every current local state. -/
  preparedScanIsValid : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    let observation := currentPreparedFlexObservation timed time validator
    ValidatorFlexPreparedScanValid schedule
      (context validator observation.input) observation.input.commitHead
      (pendingSource.highestAcceptedRound validator observation.input)
      (source.selectedLeaderSlots validator observation.input)
      (pendingSource.cacheAt validator observation.input)
      (source.snapshot validator observation.input).materialize
  /-- Each prepared round uses the current selected leader slot order. -/
  preparedSlotsMatchCurrentConfig : ∀ time validator index,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    let observation := currentPreparedFlexObservation timed time validator
    index < (validatorFlexPreparedInputAt source schedule pendingSource.cacheAt
      pendingSource.highestAcceptedRound observation).pending.roundCount →
    let round := (validatorFlexPreparedInputAt source schedule
      pendingSource.cacheAt pendingSource.highestAcceptedRound
      observation).pending.rounds index
    round.selectedSlots.map ReferenceSelectedSlotView.slot =
      (config.selectedLeaderOrder observation.input.commitHead.id
        round.round).map fun selectedValidator =>
          { round := round.round
            validator := selectedValidator }
  /-- Every accepted reference is at or below the prepared DAG frontier. -/
  acceptedRoundAtOrBelowFrontier : ∀ time validator reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    let observation := currentPreparedFlexObservation timed time validator
    observation.input.accepted reference = true →
      reference.round ≤
        pendingSource.highestAcceptedRound validator observation.input
  /-- A successful current prepared scan creates protected local work. -/
  successfulPreparedScanIsProtected : ∀ time validator output,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    let observation := currentPreparedFlexObservation timed time validator
    (validatorFlexRunStateAt source schedule pendingSource.cacheAt
      pendingSource.highestAcceptedRound observation).output = some output →
    timed.protectedAction time validator .runCommitter

private theorem prepared_direct_range_gives_exact_output
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq BlockId]
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (source : LocalFlexCommitterSourceMap config functions context program)
    (schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey)
    (cache : ValidatorFlexPendingCache BlockId CommitId ScheduleKey)
    (highestAcceptedRound : Nat)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    {observer baseRound : Nat}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (valid : ValidatorFlexPreparedScanValid schedule
      (context observer (world.validatorState observer))
      (world.validatorState observer).commitHead highestAcceptedRound
      (source.selectedLeaderSlots observer (world.validatorState observer))
      cache (source.snapshot observer
        (world.validatorState observer)).materialize)
    (slotsMatch : ∀ index,
      index < (prepareValidatorFlexScanInput
        schedule.minimumLeaderRoundForHead schedule.scheduleKeyForHead
        (source.selectedLeaderSlots observer (world.validatorState observer))
        highestAcceptedRound (world.validatorState observer).commitHead cache
        (source.snapshot observer
          (world.validatorState observer)).materialize).pending.roundCount →
      let round := (prepareValidatorFlexScanInput
        schedule.minimumLeaderRoundForHead schedule.scheduleKeyForHead
        (source.selectedLeaderSlots observer (world.validatorState observer))
        highestAcceptedRound (world.validatorState observer).commitHead cache
        (source.snapshot observer
          (world.validatorState observer)).materialize).pending.rounds index
      round.selectedSlots.map ReferenceSelectedSlotView.slot =
        (config.selectedLeaderOrder
          (world.validatorState observer).commitHead.id round.round).map
            fun selectedValidator =>
              { round := round.round
                validator := selectedValidator })
    (frontierCoversAccepted : ∀ reference,
      (world.validatorState observer).accepted reference = true →
        reference.round ≤ highestAcceptedRound)
    (baseAfterHead :
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
      let prepared := prepareValidatorFlexScanInput
        schedule.minimumLeaderRoundForHead schedule.scheduleKeyForHead
        (source.selectedLeaderSlots observer (world.validatorState observer))
        highestAcceptedRound (world.validatorState observer).commitHead cache
        (source.snapshot observer
          (world.validatorState observer)).materialize
      tryReferenceFlexCommitWithContext functions
          (context observer (world.validatorState observer)) prepared = some output ∧
        output.reference.index =
          (world.validatorState observer).commitHead.index + 1 := by
  dsimp only
  let state := world.validatorState observer
  let localContext := context observer state
  let minimum := schedule.minimumLeaderRoundForHead state.commitHead
  let prepared := prepareValidatorFlexScanInput
    schedule.minimumLeaderRoundForHead schedule.scheduleKeyForHead
    (source.selectedLeaderSlots observer state) highestAcceptedRound
    state.commitHead cache (source.snapshot observer state).materialize
  let baseIndex := baseRound - minimum
  have minimumShape : minimum = state.commitHead.round + 1 := by
    exact schedule.minimumIsRoundSuccessor state.commitHead
  have minimumBeforeBase : minimum ≤ baseRound := by
    have baseAfterHead' : state.commitHead.round < baseRound := by
      simpa [state] using baseAfterHead
    rw [minimumShape]
    omega
  have countShape : prepared.pending.roundCount =
      highestAcceptedRound - minimum := by
    simpa only [prepared, state, minimum] using
      prepared_validator_flex_scan_round_count schedule localContext
        state.commitHead highestAcceptedRound
        (source.selectedLeaderSlots observer state) cache
        (source.snapshot observer state).materialize valid
  have indexForOffset : ∀ offset, offset < localContext.depth + 1 →
      baseIndex + offset < prepared.pending.roundCount := by
    intro offset offsetInRange
    rcases higherAccepted offset (by simpa [localContext, state] using offsetInRange)
      with ⟨higherBlock, higherIsAccepted, higherRound⟩
    have higherAtFrontier := frontierCoversAccepted higherBlock higherIsAccepted
    have blockRound := leaderRound offset
      (by simpa [localContext, state] using offsetInRange)
    dsimp only [prepared, baseIndex, minimum] at countShape ⊢
    omega
  have covered : baseIndex + (localContext.depth + 1) ≤
      prepared.pending.roundCount := by
    have lastInRange := indexForOffset localContext.depth (by omega)
    omega
  have anchorWindow : ReferenceAnchorWindow
      (runReferenceDirectPass localContext.directRule prepared.pending)
      baseIndex (localContext.depth + 1) := by
    intro offset offsetInRange
    let index := baseIndex + offset
    have indexInRange : index < prepared.pending.roundCount := by
      exact indexForOffset offset offsetInRange
    let roundView := prepared.pending.rounds index
    have pendingRound : roundView.round = baseRound + offset := by
      have consecutive := valid.preparedStateValid.roundsConsecutive index
        (by simpa [prepared, state] using indexInRange)
      rw [valid.firstPendingIsCurrentMinimum] at consecutive
      have consecutive' : roundView.round = minimum + index := by
        simpa [roundView, prepared, state, minimum] using consecutive
      rw [consecutive']
      dsimp only [index, baseIndex]
      omega
    have slotsMatchAtIndex := slotsMatch index indexInRange
    let selectedSlot : ExactSelectedLeaderSlot :=
      { round := baseRound + offset
        validator := (leaderAt offset).author }
    have selectedHead :
        (roundView.selectedSlots.map ReferenceSelectedSlotView.slot).head? =
          some selectedSlot := by
      have configuredHead := exact_selected_leader_order_head config
        state.commitHead (baseRound + offset) (leaderAt offset).author
        (firstSelected offset
          (by simpa [localContext, state] using offsetInRange))
      change roundView.selectedSlots.map ReferenceSelectedSlotView.slot = _
        at slotsMatchAtIndex
      rw [slotsMatchAtIndex, pendingRound]
      simpa [state, selectedSlot, exactSelectedLeaderOrder] using configuredHead
    change ∃ block, scanReferenceSelectedSlots
      ((runReferenceDirectRound localContext.directRule roundView).selectedSlots) =
        .found block
    cases slotsShape : roundView.selectedSlots with
    | nil => simp [slotsShape] at selectedHead
    | cons first tail =>
        have firstSlot : first.slot = selectedSlot := by
          simpa [slotsShape] using selectedHead
        have blockAtFirst : (leaderAt offset).toLeaderBlockRef.AtSelectedSlot
            first.slot := by
          rw [firstSlot]
          exact ⟨by simp [selectedSlot,
            leaderRound offset (by simpa [localContext, state] using offsetInRange)],
            by simp [selectedSlot]⟩
        have firstCommitted := direct.quorumCommitIsPostDirect world observer
          first (leaderAt offset) blockAtFirst
          (leaderAccepted offset
            (by simpa [localContext, state] using offsetInRange))
          (directQuorum offset
            (by simpa [localContext, state] using offsetInRange))
        refine ⟨(leaderAt offset).toLeaderBlockRef, ?_⟩
        change scanReferenceSelectedSlots
            ((runReferenceDirectRound localContext.directRule
              roundView).selectedSlots) =
          .found (leaderAt offset).toLeaderBlockRef
        simp only [runReferenceDirectRound, slotsShape, List.map_cons,
          scanReferenceSelectedSlots]
        have firstCommittedAtState :
            (applyReferenceDirectDecision localContext.directRule first).status =
              .commit (leaderAt offset).toLeaderBlockRef := by
          simpa [localContext, state] using firstCommitted
        rw [firstCommittedAtState]
  have windowInRange : baseIndex + localContext.depth <
      (runReferenceDirectPass localContext.directRule
        prepared.pending).roundCount := by
    simpa [runReferenceDirectPass] using (show
      baseIndex + localContext.depth < prepared.pending.roundCount by omega)
  rcases reference_anchor_window_gives_exact_try_commit_output functions
      localContext.directRule localContext.indirectRule prepared
      localContext.depthPositive anchorWindow windowInRange with
    ⟨output, found⟩
  refine ⟨output, ?_, ?_⟩
  · simpa [tryReferenceFlexCommitWithContext, localContext] using found
  · unfold tryReferenceFlexCommit at found
    let directState := runReferenceDirectPass localContext.directRule prepared.pending
    cases directFound : findIndexedReferenceFlexCandidate directState with
    | some candidate =>
        simp [directState, directFound] at found
        rw [← found]
        simp [ReferenceFlexTryCommitInput.buildOutput,
          buildLocalFlexCommitOutput, prepared, state,
          prepareValidatorFlexScanInput, constructExactCommitReference,
          ExactCommitBuilderInput.toCommitRecord,
          ReferenceFlexCandidate.toBuilderInput]
    | none =>
        cases indirectFound : findIndexedReferenceFlexCandidate
            (runFullIndexedReferenceIndirect localContext.depth
              localContext.indirectRule directState) with
        | none => simp [directState, directFound, indirectFound] at found
        | some candidate =>
            simp [directState, directFound, indirectFound] at found
            rw [← found]
            simp [ReferenceFlexTryCommitInput.buildOutput,
              buildLocalFlexCommitOutput, prepared, state,
              prepareValidatorFlexScanInput, constructExactCommitReference,
              ExactCommitBuilderInput.toCommitRecord,
              ReferenceFlexCandidate.toBuilderInput]

private theorem prepared_direct_quorum_has_higher_accepted_block
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
  | none => simp [traceDirectVoters, representative] at voted
  | some child =>
      have childSound := (worldWellFormed observer observerInRange)
        |>.acceptedRepresentativeIsSound (leader.round + 1) voter child
          representative
      exact ⟨child, childSound.2.2.1, by omega⟩

private theorem prepared_durable_state_monotone_trans
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
    have firstSame : before.commitHead.index = middle.commitHead.index := by omega
    have secondSame : middle.commitHead.index = after.commitHead.index := by omega
    exact (firstHead firstSame).trans (secondHead secondSame)
  · intro index commitId installed
    exact secondInstalled index commitId (firstInstalled index commitId installed)
  · intro index installSource recorded
    exact secondSource index installSource (firstSource index installSource recorded)
  · intro round reference stored
    exact secondOwn round reference (firstOwn round reference stored)
  · intro round sent
    exact secondSent round (firstSent round sent)
  · intro reference accepted
    exact secondAccepted reference (firstAccepted reference accepted)
  · intro round author reference present
    exact secondRepresentative round author reference
      (firstRepresentative round author reference present)

private theorem prepared_world_step_durable_monotone
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
      exact prepared_durable_state_monotone_trans
        (validator_atomic_step_durable_monotone firstStep validator)
        inductionHypothesis

private theorem prepared_world_step_accepted_representative_persists
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

private theorem prepared_world_step_catalog_persists
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
        ((validator_atomic_step_history_monotone firstStep).1 blockId block present)

private theorem prepared_world_step_direct_voter_persists
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
        prepared_world_step_accepted_representative_persists step representative
      cases catalog : before.blockCatalog childReference.id with
      | none => simp [representative, catalog] at vote
      | some child =>
          have catalogAfter := prepared_world_step_catalog_persists step catalog
          simpa [representativeAfter, catalogAfter] using
            (show decide
                (child.reference = childReference ∧
                  childReference.author = voter ∧
                  childReference.round = leader.round + 1 ∧
                  leader ∈ child.parents) = true by
              simpa [representative, catalog] using vote)

private theorem prepared_world_step_direct_voter_weight_mono
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
  exact prepared_world_step_direct_voter_persists step voted

/-- A prepared exact direct-quorum range runs the local committer and installs
its exact result. An earlier next-index install ends the attempt first. -/
theorem prepared_direct_quorum_range_records_exact_or_installed_next
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq BlockId]
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (pendingSource : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (current : ValidatorCurrentPreparedFlexSourceMap pendingSource)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    {start observer baseRound : Nat}
    {prior : ValidatorCommitHead CommitId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState observer).commitHead = prior)
    (baseAfterPrior : prior.round < baseRound)
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
    (∃ finish witnessId,
      start ≤ finish ∧
        ((timed.execution.trace finish).validatorState observer).installedCommitAt
          (prior.index + 1) = some witnessId) ∨
      ∃ (run : CorrectExactFlexRun runtime) (installedAt : Time),
        start ≤ run.observation.time ∧
          run.observation.validator = observer ∧
          run.prior = prior ∧
          run.output.reference.index = prior.index + 1 ∧
          run.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              observer).installedCommitAt run.output.reference.index =
            some run.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              observer).commitInstallSourceAt run.output.reference.index =
            some .localExecution := by
  let startWorld := timed.execution.trace start
  let startState := startWorld.validatorState observer
  let startObservation := currentPreparedFlexObservation timed start observer
  have higherAcceptedAtStart : ∀ offset,
      offset < (context observer startState).depth + 1 →
      ∃ higherBlock : ValidatorBlockRef BlockId,
        startState.accepted higherBlock = true ∧
          (leaderAt offset).round < higherBlock.round := by
    intro offset offsetInRange
    exact prepared_direct_quorum_has_higher_accepted_block observerInRange
      (timed.execution.statesWellFormed start)
      (directQuorum offset (by simpa [startState] using offsetInRange))
  have initialValid := current.preparedScanIsValid start observer
    observerInRange observerCorrect
  have initialSlots := fun index =>
    current.preparedSlotsMatchCurrentConfig start observer index
      observerInRange observerCorrect
  have initialFrontier := fun reference =>
    current.acceptedRoundAtOrBelowFrontier start observer reference
      observerInRange observerCorrect
  rcases prepared_direct_range_gives_exact_output startWorld source schedule
      (pendingSource.cacheAt observer startState)
      (pendingSource.highestAcceptedRound observer startState) direct leaderAt
      (by simpa [startObservation, startState, startWorld,
        currentPreparedFlexObservation] using initialValid)
      (by simpa [startObservation, startState, startWorld,
        currentPreparedFlexObservation, validatorFlexPreparedInputAt] using
          initialSlots)
      (by simpa [startObservation, startState, startWorld,
        currentPreparedFlexObservation] using initialFrontier)
      (by simpa [startState, startWorld, headAtStart] using baseAfterPrior)
      leaderRound
      (by
        intro offset offsetInRange
        simpa [startState, startWorld, headAtStart] using
          firstSelected offset offsetInRange)
      leaderAccepted higherAcceptedAtStart directQuorum with
    ⟨initialOutput, initialPreparedResult, _initialNext⟩
  have initialRunStateResult :
      (validatorFlexRunStateAt source schedule pendingSource.cacheAt
        pendingSource.highestAcceptedRound startObservation).output =
          some initialOutput := by
    simp only [validatorFlexRunStateAt]
    rw [execute_reference_flex_commit_output]
    simpa [startObservation, startState, startWorld,
      currentPreparedFlexObservation, validatorFlexPreparedInputAt] using
        initialPreparedResult
  have protectedWork := current.successfulPreparedScanIsProtected start observer
    initialOutput observerInRange observerCorrect
      (by simpa [startObservation] using initialRunStateResult)
  let completion := timed.completeProtectedAction observer .runCommitter start
    observerInRange observerCorrect protectedWork
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
            (prior.index + 1) observerInRange observerCorrect nextAtOrBelowHead with
          ⟨witnessId, installed⟩
        exact Or.inl ⟨actionAt + 1, witnessId, startBeforeFinish, installed⟩
      · have startToFinish := timed.execution.durable_fields_persist
          observerInRange startBeforeFinish
        have finalIndex :
            (timed.execution.trace (actionAt + 1)).localCommitIndex observer =
              prior.index := by
          have notDecreased := startToFinish.1
          rw [headAtStart] at notDecreased
          change ¬prior.index <
            ((timed.execution.trace (actionAt + 1)).validatorState
              observer).commitHead.index at advanced
          change ((timed.execution.trace (actionAt + 1)).validatorState
              observer).commitHead.index = prior.index
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
          change ((timed.execution.trace actionAt).validatorState
              observer).commitHead.index ≤
            ((timed.execution.trace (actionAt + 1)).validatorState
              observer).commitHead.index at toFinish
          change ((timed.execution.trace (actionAt + 1)).validatorState
              observer).commitHead.index = prior.index at finalIndex
          change ((timed.execution.trace actionAt).validatorState
              observer).commitHead.index = prior.index
          omega
        have headAtAction :
            ((timed.execution.trace actionAt).validatorState
              observer).commitHead = prior := by
          have sameIndex :
              ((timed.execution.trace start).validatorState
                  observer).commitHead.index =
                ((timed.execution.trace actionAt).validatorState
                  observer).commitHead.index := by
            rw [headAtStart]
            exact actionIndex.symm
          exact (startToAction.2.2.1 sameIndex).symm.trans headAtStart
        have prefixDurable := prepared_world_step_durable_monotone prefixStep observer
        have actionBeforeIndex :
            actionBefore.localCommitIndex observer = prior.index := by
          have prefixIndex := prefixDurable.1
          have actionIndexMono :=
            (validator_atomic_step_durable_monotone actionStep observer).1
          have suffixIndex :=
            (prepared_world_step_durable_monotone suffixStep observer).1
          change ((timed.execution.trace actionAt).validatorState
              observer).commitHead.index ≤
            (actionBefore.validatorState observer).commitHead.index at prefixIndex
          have beforeToFinish := Nat.le_trans actionIndexMono suffixIndex
          change (actionBefore.validatorState observer).commitHead.index ≤
            ((timed.execution.trace (actionAt + 1)).validatorState
              observer).commitHead.index at beforeToFinish
          change ((timed.execution.trace actionAt).validatorState
              observer).commitHead.index = prior.index at actionIndex
          change ((timed.execution.trace (actionAt + 1)).validatorState
              observer).commitHead.index = prior.index at finalIndex
          change (actionBefore.validatorState observer).commitHead.index =
            prior.index
          omega
        have headAtActionBefore :
            (actionBefore.validatorState observer).commitHead = prior := by
          have sameIndex :
              ((timed.execution.trace actionAt).validatorState
                  observer).commitHead.index =
                (actionBefore.validatorState observer).commitHead.index := by
            change ((timed.execution.trace actionAt).validatorState
                observer).commitHead.index = prior.index at actionIndex
            change (actionBefore.validatorState observer).commitHead.index =
              prior.index at actionBeforeIndex
            exact actionIndex.trans actionBeforeIndex.symm
          exact (prefixDurable.2.2.1 sameIndex).symm.trans headAtAction
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
        have depthSame := source.contextDepthStable observer startState observer
          observation.input
        have leaderRoundAtAction : ∀ offset,
            offset < (context observer observation.input).depth + 1 →
            (leaderAt offset).round = baseRound + offset := by
          intro offset offsetInRange
          apply leaderRound offset
          rw [depthSame]
          simpa [startState] using offsetInRange
        have firstSelectedAtAction : ∀ offset,
            offset < (context observer observation.input).depth + 1 →
            (config.selectedLeaderOrder observation.input.commitHead.id
              (baseRound + offset)).head? = some (leaderAt offset).author := by
          intro offset offsetInRange
          rw [show observation.input.commitHead = prior by
            simpa [observation] using headAtActionBefore]
          apply firstSelected offset
          rw [depthSame]
          simpa [startState] using offsetInRange
        have acceptedAtAction : ∀ offset,
            offset < (context observer observation.input).depth + 1 →
            observation.input.accepted (leaderAt offset) = true := by
          intro offset offsetInRange
          have atStart := leaderAccepted offset (by
            rw [depthSame]
            simpa [startState] using offsetInRange)
          have atTrace := timed.execution.accepted_block_persists observerInRange
            startBeforeAction atStart
          have beforeAction := validator_world_step_accepted_block_persists
            prefixStep atTrace
          simpa [observation] using beforeAction
        have higherAtAction : ∀ offset,
            offset < (context observer observation.input).depth + 1 →
            ∃ higherBlock : ValidatorBlockRef BlockId,
              observation.input.accepted higherBlock = true ∧
                (leaderAt offset).round < higherBlock.round := by
          intro offset offsetInRange
          rcases higherAcceptedAtStart offset (by
              rw [depthSame]
              simpa [startState] using offsetInRange) with
            ⟨higherBlock, accepted, higherRound⟩
          have atTrace := timed.execution.accepted_block_persists observerInRange
            startBeforeAction accepted
          have beforeAction := validator_world_step_accepted_block_persists
            prefixStep atTrace
          exact ⟨higherBlock, by simpa [observation] using beforeAction,
            higherRound⟩
        have quorumAtAction : ∀ offset,
            offset < (context observer observation.input).depth + 1 →
            config.thresholds.quorum ≤
              weight config.authorityCount config.stake
                (traceDirectVoters actionBefore observer (leaderAt offset)) := by
          intro offset offsetInRange
          have atStart := directQuorum offset (by
            rw [depthSame]
            simpa [startState] using offsetInRange)
          exact Nat.le_trans atStart (Nat.le_trans
            (trace_direct_voter_weight_mono timed.execution observerInRange
              startBeforeAction)
            (prepared_world_step_direct_voter_weight_mono prefixStep))
        have actualValid := pendingSource.actualRunPreparedScanIsValid
          observation observationOccurs
        have actualSlots := pendingSource.preparedSlotsMatchCurrentConfig
          observation observationOccurs
        have actualFrontier : ∀ reference,
            observation.input.accepted reference = true →
              reference.round ≤ pendingSource.highestAcceptedRound observer
                observation.input := by
          intro reference accepted
          let support := pendingSource.dagSupport observation observationOccurs
          have member : reference ∈ support.references :=
            (support.acceptedMembership reference).2 accepted
          exact support.referencesAtOrBelowFrontier reference member
        rcases prepared_direct_range_gives_exact_output actionBefore source
            schedule (pendingSource.cacheAt observer observation.input)
            (pendingSource.highestAcceptedRound observer observation.input)
            direct leaderAt actualValid actualSlots actualFrontier
            (by simpa [observation, headAtActionBefore] using baseAfterPrior)
            leaderRoundAtAction firstSelectedAtAction acceptedAtAction
            higherAtAction quorumAtAction with
          ⟨output, preparedResult, outputNext⟩
        have returned : runtime.returned observation := by
          simpa [observation] using
            runtime.runCommitterOccurrenceReturnsExactResult observation
              observationOccurs
        have runStateResult :
            (validatorFlexRunStateAt source schedule pendingSource.cacheAt
              pendingSource.highestAcceptedRound observation).output =
                some output := by
          simp only [validatorFlexRunStateAt]
          rw [execute_reference_flex_commit_output]
          simpa [validatorFlexPreparedInputAt, observation] using preparedResult
        /- This equality is the remaining abstract runtime refinement. The
        shared runtime observation does not yet carry Rust's internal prepared
        input. `returned_observation_uses_prepared_scan` reconstructs it. -/
        have successful : observation.result = some output := by
          rw [pendingSource.returned_observation_uses_prepared_scan returned]
          exact runStateResult
        let run : CorrectExactFlexRun runtime :=
          { observation
            output
            prior
            validatorInRange := by simpa [observation] using observerInRange
            validatorCorrect := by simpa [observation] using observerCorrect
            returned
            successful
            priorAtInput := by simpa [observation] using headAtActionBefore }
        rcases successful_local_flex_run_completes_and_persists_exact runtime
            observation returned successful
            (by simpa [observation] using observerInRange)
            (by simpa [observation] using observerCorrect) with
          ⟨recordAt, _pureResult, runBeforeRecord, _recordBound, installed,
            localSource⟩
        refine Or.inr ⟨run, recordAt + 1, ?_, ?_, rfl, ?_, ?_, ?_, ?_⟩
        · simpa [run, observation] using startBeforeAction
        · simp [run, observation]
        · simpa [run, observation, headAtActionBefore] using outputNext
        · change actionAt < recordAt + 1
          exact Nat.lt_of_lt_of_le (Nat.lt_succ_self actionAt)
            (Nat.le_trans runBeforeRecord (Nat.le_add_right recordAt 1))
        · simpa [run, observation] using installed
        · simpa [run, observation] using localSource

/-- One accepted local carrier supplies the prepared direct-vote range. The
receiver then installs an exact local result, unless it installs the next index
first. The first branch restarts the composition from the new commit head. -/
theorem accepted_carrier_runs_prepared_flex_or_installs_next
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq BlockId]
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (pendingSource : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (current : ValidatorCurrentPreparedFlexSourceMap pendingSource)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {sourceTime start finish requester baseRound : Time}
    {prior : ValidatorCommitHead CommitId}
    {target : ValidatorBlockRef BlockId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (coverage : ValidatorDirectRangeCarrierCoverage config faults
      (timed.execution.trace sourceTime) target leaderAt
        ((context requester
          ((timed.execution.trace finish).validatorState requester)).depth + 1))
    (sourceBeforeFinish : sourceTime ≤ finish)
    (startBeforeFinish : start ≤ finish)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState requester).commitHead = prior)
    (baseAfterPrior : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context requester
          ((timed.execution.trace finish).validatorState requester)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context requester
          ((timed.execution.trace finish).validatorState requester)).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (acceptedClosure : ValidatorAcceptedCausalClosureAboveRound
      (timed.execution.trace finish) requester
        ((timed.execution.trace finish).validatorState requester).gcRound target) :
    (∃ completedAt witnessId,
      start ≤ completedAt ∧
        ((timed.execution.trace completedAt).validatorState
          requester).installedCommitAt (prior.index + 1) = some witnessId) ∨
      ∃ (run : CorrectExactFlexRun runtime) (installedAt : Time),
        start ≤ run.observation.time ∧
          run.observation.validator = requester ∧
          run.prior = prior ∧
          run.output.reference.index = prior.index + 1 ∧
          run.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              requester).installedCommitAt run.output.reference.index =
            some run.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              requester).commitInstallSourceAt run.output.reference.index =
            some .localExecution := by
  rcases accepted_above_gc_direct_range_carrier_gives_range_or_installed_next
      timed.execution representatives prefixMap coverage sourceBeforeFinish
      startBeforeFinish requesterInRange requesterCorrect headAtStart
      baseAfterPrior leaderRound acceptedClosure with
    installedNext | ⟨headAtFinish, rangeAtFinish⟩
  · rcases installedNext with ⟨witnessId, installed⟩
    exact Or.inl ⟨finish, witnessId, startBeforeFinish, installed⟩
  · have leaderAcceptedAtFinish : ∀ offset,
        offset < (context requester
            ((timed.execution.trace finish).validatorState requester)).depth + 1 →
        ((timed.execution.trace finish).validatorState requester).accepted
          (leaderAt offset) = true := by
      intro offset offsetInRange
      exact (rangeAtFinish offset offsetInRange).1
    have directQuorumAtFinish : ∀ offset,
        offset < (context requester
            ((timed.execution.trace finish).validatorState requester)).depth + 1 →
        config.thresholds.quorum ≤
          weight config.authorityCount config.stake
            (traceDirectVoters (timed.execution.trace finish) requester
              (leaderAt offset)) := by
      intro offset offsetInRange
      exact (rangeAtFinish offset offsetInRange).2
    rcases prepared_direct_quorum_range_records_exact_or_installed_next
        pendingSource current prefixMap direct leaderAt requesterInRange
        requesterCorrect headAtFinish baseAfterPrior leaderRound firstSelected
        leaderAcceptedAtFinish directQuorumAtFinish with
      installedNext | localRun
    · rcases installedNext with
        ⟨completedAt, witnessId, finishBeforeComplete, installed⟩
      exact Or.inl ⟨completedAt, witnessId,
        Nat.le_trans startBeforeFinish finishBeforeComplete, installed⟩
    · rcases localRun with
        ⟨run, installedAt, finishBeforeRun, runValidator, runPrior, runNext,
          runBeforeInstall, installed, localSource⟩
      exact Or.inr ⟨run, installedAt,
        Nat.le_trans startBeforeFinish finishBeforeRun, runValidator, runPrior,
          runNext, runBeforeInstall, installed, localSource⟩

end Mysticeti
