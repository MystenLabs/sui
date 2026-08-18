/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorExecutionLemmas
import Mysticeti.ValidatorLowestPendingRecovery

namespace Mysticeti

/-! One-validator durable recovery queue execution.

The event at time `t` contains local work observed at time `t`. For positive
time, that event moves the queue state from `t - 1` to `t`. This convention
lets accepted block processing queue its round before the new accepted state is
visible to the proposer.
-/

/-- Local inputs processed in one recovery queue batch. -/
structure ValidatorRecoveryQueueEvent
    (BlockId CommitId : Type) where
  queuedRound : Nat → Bool
  recordOwn : Nat → Bool
  markSent : Nat → Bool
  lockLowest : Option Nat
  observedCommit : ValidatorCommitHead CommitId
  observedSignerFloor : Nat
  observedGcRound : Nat
  observedRecoveryTarget : Option Nat
  acceptedBlocks : ValidatorBlock BlockId → Bool

/-- The deterministic lowest pending round above the signer floor. -/
def ValidatorLowestEligiblePending
    (state : LowestPendingRecoveryLocalState) : Option Nat → Prop
  | none => ∀ round,
      state.pending round = true → state.signerFloor < round → False
  | some round =>
      state.pending round = true ∧
        state.signerFloor < round ∧
        ∀ other,
          state.pending other = true →
          state.signerFloor < other →
          round ≤ other

/-- Local queue invariants after one batch. -/
structure ValidatorRecoveryQueueStateWellFormed
    (state : LowestPendingRecoveryLocalState) : Prop where
  sentRequiresOwn : ∀ round,
    state.sentAt round = true → state.ownAt round = true
  lockedRoundIsPending : ∀ round,
    state.lockedRound = some round → state.pending round = true
  lockedRoundIsAboveFloor : ∀ round,
    state.lockedRound = some round → state.signerFloor < round
  lockedRoundIsLowest : ∀ round,
    state.lockedRound = some round →
    ∀ other,
      state.pending other = true →
      state.signerFloor < other →
      round ≤ other
  pendingAtOrBelowFloorHasOwn : ∀ round,
    state.pending round = true →
    round ≤ state.signerFloor →
    state.ownAt round = true

/-- One explicit local recovery queue transition. -/
structure ValidatorRecoveryQueueTransition
    {BlockId CommitId : Type}
    (before : LowestPendingRecoveryLocalState)
    (event : ValidatorRecoveryQueueEvent BlockId CommitId)
    (after : LowestPendingRecoveryLocalState) : Prop where
  signerFloorObserved : after.signerFloor = event.observedSignerFloor
  gcRoundObserved : after.gcRound = event.observedGcRound
  signerFloorMonotone : before.signerFloor ≤ after.signerFloor
  gcRoundMonotone : before.gcRound ≤ after.gcRound
  ownMonotone : BoolMapMonotone before.ownAt after.ownAt
  sentMonotone : BoolMapMonotone before.sentAt after.sentAt
  pendingMonotone : BoolMapMonotone before.pending after.pending
  ownUpdateExact : ∀ round,
    after.ownAt round = true ↔
      before.ownAt round = true ∨ event.recordOwn round = true
  sentUpdateExact : ∀ round,
    after.sentAt round = true ↔
      before.sentAt round = true ∨ event.markSent round = true
  pendingUpdateExact : ∀ round,
    after.pending round = true ↔
      before.pending round = true ∨ event.queuedRound round = true
  queuedRoundIsPending : ∀ round,
    event.queuedRound round = true → after.pending round = true
  recordedOwnIsStored : ∀ round,
    event.recordOwn round = true → after.ownAt round = true
  markedSentIsStored : ∀ round,
    event.markSent round = true → after.sentAt round = true
  lockUpdateExact : after.lockedRound = event.lockLowest
  lockIsDeterministicLowest :
    ValidatorLowestEligiblePending after event.lockLowest
  lockIsStickyUntilOwn : ∀ round,
    before.lockedRound = some round →
    after.ownAt round = false →
    after.lockedRound = some round
  crossedRoundIsQueued : ∀ round,
    before.signerFloor < round →
    round ≤ after.signerFloor →
    after.pending round = true
  afterWellFormed : ValidatorRecoveryQueueStateWellFormed after

/-- A local recovery queue mapped to the main validator execution. -/
structure ValidatorRecoveryQueueExecution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (syncTrace : BlockSyncTrace (ValidatorBlock BlockId)) where
  trace : LowestPendingRecoveryTrace
  event : Time → Nat → ValidatorRecoveryQueueEvent BlockId CommitId
  mapping : LowestPendingRecoveryExecutionMapping execution.trace syncTrace trace
  initialWellFormed : ∀ validator,
    ValidatorRecoveryQueueStateWellFormed (trace 0 validator)
  initialPendingExact : ∀ validator round,
    (trace 0 validator).pending round = true ↔
      ∃ block,
        (event 0 validator).acceptedBlocks block = true ∧
          block.reference.round = round ∧
          (trace 0 validator).signerFloor < round
  initialRecordOwnIsEmpty : ∀ validator round,
    (event 0 validator).recordOwn round = false
  initialMarkSentIsEmpty : ∀ validator round,
    (event 0 validator).markSent round = false
  transitionsFollowRules : ∀ time validator,
    ValidatorRecoveryQueueTransition (trace time validator)
      (event (time + 1) validator) (trace (time + 1) validator)
  usableHistoryBlockIsCatalogued : ∀ validator
      (capsule : CausalRecoveryCapsule (BlockId := BlockId) config) block time,
    CausalHistoryBlockUsableAt syncTrace trace validator capsule block time →
    (execution.trace time).blockCatalog block.reference.id = some block
  acceptedBlockInputIffLocal : ∀ time validator block,
    (event time validator).acceptedBlocks block = true ↔
      ((execution.trace time).validatorState validator).accepted
          block.reference = true ∧
        (execution.trace time).blockCatalog block.reference.id = some block
  queuedRoundIffAcceptedBlock : ∀ time validator round,
    (event (time + 1) validator).queuedRound round = true ↔
      ∃ block,
        (event (time + 1) validator).acceptedBlocks block = true ∧
          block.reference.round = round ∧
          (trace time validator).signerFloor < round
  acceptedBlockActionIsObserved : ∀ time validator block,
    ValidatorLocalActionOccurs (execution.events time) validator
      (.acceptBlock block) →
    (event (time + 1) validator).acceptedBlocks block = true
  recordOwnIffPersistAction : ∀ time validator round,
    (event (time + 1) validator).recordOwn round = true ↔
      ∃ block,
        ValidatorLocalActionOccurs (execution.events time) validator
            (.persistProposal block) ∧
          block.reference.round = round
  markSentIffSendAction : ∀ time validator round,
    (event (time + 1) validator).markSent round = true ↔
      ∃ receiver reference,
        ValidatorLocalActionOccurs (execution.events time) validator
            (.sendBlock receiver reference) ∧
          reference.round = round
  lockObservationMatches : ∀ time validator,
    (event time validator).lockLowest = (trace time validator).lockedRound
  lockObservationIsLowest : ∀ time validator,
    ValidatorLowestEligiblePending (trace time validator)
      (event time validator).lockLowest
  commitObservationMatches : ∀ time validator,
    (event time validator).observedCommit =
      ((execution.trace time).validatorState validator).commitHead
  signerFloorObservationMatches : ∀ time validator,
    (event time validator).observedSignerFloor =
      ((execution.trace time).validatorState validator).highestSignedRound
  gcObservationMatches : ∀ time validator,
    (event time validator).observedGcRound =
      ((execution.trace time).validatorState validator).gcRound
  recoveryTargetObservationMatches : ∀ time validator,
    (event time validator).observedRecoveryTarget =
      ((execution.trace time).validatorState validator).recovery.map
        ValidatorRecoveryState.targetRound

/-- A pointwise fact that survives each natural-number trace step survives any
later time. -/
theorem nat_trace_fact_persists
    (fact : Nat → Prop)
    (oneStep : ∀ time, fact time → fact (time + 1))
    {earlier later : Nat}
    (ordered : earlier ≤ later)
    (atEarlier : fact earlier) :
    fact later := by
  obtain ⟨offset, laterAtOffset⟩ := Nat.exists_eq_add_of_le ordered
  subst later
  have advance : ∀ offset, fact (earlier + offset) := by
    intro offset
    induction offset with
    | zero => simpa using atEarlier
    | succ offset inductionHypothesis =>
        have next := oneStep (earlier + offset) inductionHypothesis
        simpa [Nat.add_assoc] using next
  exact advance offset

namespace ValidatorRecoveryQueueExecution

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {execution : ValidatorExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {syncTrace : BlockSyncTrace (ValidatorBlock BlockId)}

/-- Every queue state in the execution is well formed. -/
theorem state_well_formed
    (queue : ValidatorRecoveryQueueExecution execution syncTrace)
    (time validator : Nat) :
    ValidatorRecoveryQueueStateWellFormed (queue.trace time validator) := by
  cases time with
  | zero => exact queue.initialWellFormed validator
  | succ time =>
      simpa [Nat.add_comm] using
        (queue.transitionsFollowRules time validator).afterWellFormed

/-- A durable Boolean queue field persists between any two ordered times. -/
theorem bool_field_persists
    (queue : ValidatorRecoveryQueueExecution execution syncTrace)
    (field : LowestPendingRecoveryLocalState → Nat → Bool)
    (fieldMonotone : ∀ time validator,
      BoolMapMonotone (field (queue.trace time validator))
        (field (queue.trace (time + 1) validator)))
    {validator round earlier later : Nat}
    (ordered : earlier ≤ later)
    (present : field (queue.trace earlier validator) round = true) :
    field (queue.trace later validator) round = true := by
  exact nat_trace_fact_persists
    (fun time => field (queue.trace time validator) round = true)
    (fun time fact => fieldMonotone time validator round fact) ordered present

/-- Own-block state persists. -/
theorem own_persists
    (queue : ValidatorRecoveryQueueExecution execution syncTrace)
    {validator round earlier later : Nat}
    (ordered : earlier ≤ later)
    (own : RecoveryOwnAt queue.trace validator round earlier) :
    RecoveryOwnAt queue.trace validator round later := by
  exact bool_field_persists queue LowestPendingRecoveryLocalState.ownAt
    (fun time validator =>
      (queue.transitionsFollowRules time validator).ownMonotone)
    ordered own

/-- Sent-block state persists. -/
theorem sent_persists
    (queue : ValidatorRecoveryQueueExecution execution syncTrace)
    {validator round earlier later : Nat}
    (ordered : earlier ≤ later)
    (sent : RecoverySentAt queue.trace validator round earlier) :
    RecoverySentAt queue.trace validator round later := by
  exact bool_field_persists queue LowestPendingRecoveryLocalState.sentAt
    (fun time validator =>
      (queue.transitionsFollowRules time validator).sentMonotone)
    ordered sent

/-- Pending-round state persists. -/
theorem pending_persists
    (queue : ValidatorRecoveryQueueExecution execution syncTrace)
    {validator round earlier later : Nat}
    (ordered : earlier ≤ later)
    (pending : RecoveryPendingAt queue.trace validator round earlier) :
    RecoveryPendingAt queue.trace validator round later := by
  exact bool_field_persists queue LowestPendingRecoveryLocalState.pending
    (fun time validator =>
      (queue.transitionsFollowRules time validator).pendingMonotone)
    ordered pending

/-- One sticky lock remains until the locked own block is durable. -/
theorem lock_persists_until_own
    (queue : ValidatorRecoveryQueueExecution execution syncTrace)
    {validator round earlier later : Nat}
    (ordered : earlier ≤ later)
    (locked : (queue.trace earlier validator).lockedRound = some round)
    (notOwn : ¬RecoveryOwnAt queue.trace validator round later) :
    (queue.trace later validator).lockedRound = some round := by
  obtain ⟨offset, laterAtOffset⟩ := Nat.exists_eq_add_of_le ordered
  subst later
  have advance : ∀ offset,
      ¬RecoveryOwnAt queue.trace validator round (earlier + offset) →
      (queue.trace (earlier + offset) validator).lockedRound = some round := by
    intro offset
    induction offset with
    | zero =>
        intro _
        simpa using locked
    | succ offset inductionHypothesis =>
        intro noOwnNext
        have noOwnCurrent :
            ¬RecoveryOwnAt queue.trace validator round (earlier + offset) := by
          intro ownCurrent
          apply noOwnNext
          exact own_persists queue (by omega) ownCurrent
        have lockedCurrent := inductionHypothesis noOwnCurrent
        have ownNextFalse :
            (queue.trace ((earlier + offset) + 1) validator).ownAt round =
              false := by
          exact Bool.eq_false_iff.mpr noOwnNext
        have step := queue.transitionsFollowRules (earlier + offset) validator
        have sticky := step.lockIsStickyUntilOwn round lockedCurrent ownNextFalse
        simpa [Nat.add_assoc] using sticky
  exact advance offset notOwn

/-- The explicit local queue execution derives all durable queue rules. -/
theorem durable_lowest_pending_rules
    (queue : ValidatorRecoveryQueueExecution execution syncTrace) :
    DurableLowestPendingRules queue.trace where
  ownPersists := fun _ _ _ _ => own_persists queue
  sentPersists := fun _ _ _ _ => sent_persists queue
  sentRequiresOwn := by
    intro validator round time sent
    exact (queue.state_well_formed time validator).sentRequiresOwn round sent
  pendingPersistsUntilOwn := by
    intro validator round earlier later ordered pending _
    exact pending_persists queue ordered pending
  lockedRoundIsPending := by
    intro validator round time locked
    exact (queue.state_well_formed time validator).lockedRoundIsPending round locked
  lockedRoundIsAboveFloor := by
    intro validator round time locked
    exact (queue.state_well_formed time validator).lockedRoundIsAboveFloor round
      locked
  lockedRoundIsLowest := by
    intro validator round time locked
    exact (queue.state_well_formed time validator).lockedRoundIsLowest round locked
  lockIsStickyUntilOwn := by
    intro validator round earlier later ordered locked notOwn
    exact lock_persists_until_own queue ordered locked notOwn
  pendingRoundCannotBeCrossed := by
    intro validator round earlier later ordered pending crossed
    have pendingLater := pending_persists queue ordered pending
    exact (queue.state_well_formed later validator).pendingAtOrBelowFloorHasOwn
      round pendingLater crossed

/-- Accepted usable history rounds are in the durable queue at the same time. -/
theorem causal_acceptance_barrier_rules
    (queue : ValidatorRecoveryQueueExecution execution syncTrace) :
    CausalAcceptanceBarrierRules (config := config) syncTrace queue.trace where
  historyRoundAboveFloorIsQueued := by
    intro validator capsule block time usable aboveFloor
    have acceptedLocal :
        ((execution.trace time).validatorState validator).accepted
            block.reference = true := by
      rw [← queue.mapping.acceptedBodyMatches validator block time]
      exact usable.2.2.1 block usable.1
    have catalogued := queue.usableHistoryBlockIsCatalogued validator capsule
      block time usable
    have observed :=
      (queue.acceptedBlockInputIffLocal time validator block).2
        ⟨acceptedLocal, catalogued⟩
    cases time with
    | zero =>
        exact (queue.initialPendingExact validator block.reference.round).2
          ⟨block, observed, rfl, aboveFloor⟩
    | succ time =>
        have step := queue.transitionsFollowRules time validator
        have previousAbove :
            (queue.trace time validator).signerFloor < block.reference.round :=
          Nat.lt_of_le_of_lt step.signerFloorMonotone aboveFloor
        have observedAtStep :
            (queue.event (time + 1) validator).acceptedBlocks block = true := by
          simpa [Nat.add_comm] using observed
        have queuedAtStep :
            (queue.event (time + 1) validator).queuedRound
                block.reference.round = true := by
          exact (queue.queuedRoundIffAcceptedBlock time validator
            block.reference.round).2
              ⟨block, observedAtStep, rfl, previousAbove⟩
        exact step.queuedRoundIsPending block.reference.round queuedAtStep

/-- Crossing a round after one baseline makes that exact round pending. -/
theorem crossed_round_is_pending
    (queue : ValidatorRecoveryQueueExecution execution syncTrace)
    {validator round start finish : Nat}
    (ordered : start ≤ finish)
    (aboveStart : (queue.trace start validator).signerFloor < round)
    (crossed : round ≤ (queue.trace finish validator).signerFloor) :
    RecoveryPendingAt queue.trace validator round finish := by
  obtain ⟨offset, finishAtOffset⟩ := Nat.exists_eq_add_of_le ordered
  subst finish
  have advance : ∀ offset,
      round ≤ (queue.trace (start + offset) validator).signerFloor →
      RecoveryPendingAt queue.trace validator round (start + offset) := by
    intro offset
    induction offset with
    | zero =>
        intro crossedAtStart
        simp only [Nat.add_zero] at crossedAtStart
        omega
    | succ offset inductionHypothesis =>
        intro crossedAfter
        have step := queue.transitionsFollowRules (start + offset) validator
        by_cases crossedBefore :
            round ≤ (queue.trace (start + offset) validator).signerFloor
        · have pendingBefore := inductionHypothesis crossedBefore
          have pendingAfter := step.pendingMonotone round pendingBefore
          simpa [RecoveryPendingAt, Nat.add_assoc] using pendingAfter
        · have beforeBelow :
              (queue.trace (start + offset) validator).signerFloor < round := by
            omega
          have crossedAtNext :
              round ≤
                (queue.trace ((start + offset) + 1) validator).signerFloor := by
            simpa [Nat.add_assoc] using crossedAfter
          have pendingAfter :=
            step.crossedRoundIsQueued round beforeBelow crossedAtNext
          simpa [RecoveryPendingAt, Nat.add_assoc] using pendingAfter
  exact advance offset crossed

/-- The queue execution derives the proposal-history rule from round crossing. -/
theorem recovery_proposal_history_rules
    (queue : ValidatorRecoveryQueueExecution execution syncTrace)
    (sources : FixedCausalRecoverySources BlockId CommitId config)
    (start : Time)
    (initial : InitialRecoveryExecutionBase config faults syncTrace queue.trace
      sources start) :
    RecoveryProposalHistoryRules queue.trace sources start where
  crossedRoundWasPending := by
    intro validator member round time startBeforeTime aboveSource crossed
    have aboveStart : (queue.trace start validator).signerFloor < round := by
      rw [initial.signerFloorMatches validator member]
      exact aboveSource
    have pending := crossed_round_is_pending queue startBeforeTime aboveStart crossed
    exact ⟨time, Nat.le_refl _, pending⟩

/-- The final recovery convergence theorem can use one queue execution instead
of three raw queue, barrier, and signer-history rule inputs. -/
theorem maximum_initial_recovery_round_eventually_owned_from_queue_execution
    (queue : ValidatorRecoveryQueueExecution execution syncTrace)
    {syncProtocolPacket :
      AddressedPacket (BlockSyncMessage (ValidatorBlock BlockId)) → Prop}
    {syncNetwork : AddressedPartialSynchrony config faults syncProtocolPacket}
    {sources : FixedCausalRecoverySources BlockId CommitId config}
    {start : Time}
    (afterGst : syncNetwork.gst ≤ start)
    (storage : RecoveryBodyStorageRules syncTrace)
    (requests : FairProtectedRequestActions config faults syncProtocolPacket
      syncTrace)
    (serving : FairProtectedServeActions config faults syncProtocolPacket
      syncTrace)
    (accepting : FairProtectedAcceptActions config faults syncProtocolPacket
      syncTrace)
    (base : InitialRecoveryExecutionBase config faults syncTrace queue.trace
      sources start)
    (gcRules : RecoveryGcProtectionRules queue.trace sources start)
    (proposalFair : FairLowestPendingProposalActions faults queue.trace)
    (genesisFair : FairCanonicalGenesisActions faults queue.trace)
    (resendFair : FairDurableOwnBlockActions faults queue.trace) :
    ∃ finish,
      start ≤ finish ∧
      ∀ validator,
        validator < config.authorityCount →
        faults.correctAvailable validator = true →
        ValidatorExecutionOwnAndSentAt execution.trace validator
          sources.maximumRound finish := by
  exact maximum_initial_recovery_round_eventually_owned_in_validator_execution
    afterGst storage requests serving accepting base gcRules
    queue.causal_acceptance_barrier_rules queue.durable_lowest_pending_rules
    (queue.recovery_proposal_history_rules sources start base) proposalFair
    genesisFair resendFair queue.mapping

end ValidatorRecoveryQueueExecution

end Mysticeti
