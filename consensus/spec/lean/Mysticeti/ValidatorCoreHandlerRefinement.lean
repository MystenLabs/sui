/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorTimedExecutionLemmas

namespace Mysticeti

/-! Refinement boundary for one finite Core input handler.

The main action model represents successful committer and proposal actions. It
does not represent the terminal `try_commit_v3` call which returns `None`, or
the later `try_propose(false)` call which can also return without a proposal.
This file keeps those two calls as handler-local observations.

One episode contains a finite exact chain of successful scan, record, and
post-commit steps. Post-commit GC unsuspension can use only the finite store at
handler entry. The episode ends with one terminal commit scan and one later
normal proposal attempt before the handler returns. It contains no proposal
result, protected proposal action, block, quorum, or future commit.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- The terminal handler-local commit scan. It is not a `.runCommitter` action,
because that main action requires a nonempty pending-round guard. -/
structure ValidatorTerminalCommitScanObservation
    (BlockId CommitId : Type) where
  time : Time
  validator : Nat
  input : ValidatorLocalState BlockId CommitId

/-- The handler-local normal proposal call. The `force` field distinguishes
the ordinary `try_propose(false)` call from recovery or timeout selection. -/
structure ValidatorNormalProposalAttemptObservation
    (BlockId CommitId : Type) where
  time : Time
  validator : Nat
  input : ValidatorLocalState BlockId CommitId
  force : Bool

/-- One actual finite accepted-block input batch for `Core::add_blocks`.

The positive DAG proof uses only this ordinary block path. Certified-commit
processing needs a separate per-head post-commit model and is not part of this
liveness refinement. -/
structure ValidatorCoreHandlerInputObservation
    (BlockId CommitId : Type) where
  time : Time
  validator : Nat
  acceptedBlocks : List (ValidatorBlock BlockId)
  acceptedInputNonempty : acceptedBlocks ≠ []

/-- The exact state-changing main events in one Core input batch. -/
def ValidatorCoreHandlerInputObservation.events
    (input : ValidatorCoreHandlerInputObservation BlockId CommitId) :
    List (ValidatorAtomicEvent BlockId CommitId PacketId) :=
  input.acceptedBlocks.map fun block =>
    .localAction input.validator (.acceptBlock block)

/-- One ordinary block packet was delivered and the receiver later ran its
actual block-accept action. All fields describe past main-trace events. -/
structure ValidatorPacketDrivenBlockAcceptanceAt
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (acceptTime : Time) (validator : Nat)
    (block : ValidatorBlock BlockId) where
  deliveredAt : Time
  packetId : PacketId
  packet : AddressedPacket (ValidatorMessage BlockId CommitId)
  packetPresent :
    (timed.execution.trace deliveredAt).packets packetId = some packet
  packetIsProtocol : protocolPacket packet
  packetDeliveredAt : packet.deliveredAt = deliveredAt
  packetReceiver : packet.receiver = validator
  packetPayload : packet.payload = .block block
  deliveryOccurs :
    ValidatorPacketDeliveryOccurs (timed.execution.events deliveredAt) packetId
  deliveryBeforeAccept : deliveredAt ≤ acceptTime
  acceptOccurs : ValidatorLocalActionOccurs
    (timed.execution.events acceptTime) validator (.acceptBlock block)

/-- The finite successful part of one Core commit-processing loop.

Each constructor records one actual successful `.runCommitter` action, its
actual `.recordCommit` action, and the finite post-commit event slice. The
recursive tail is the next scan. Thus the indexed event list contains every
successful iteration and no unlisted gap between iterations. -/
inductive ValidatorFiniteCoreCommitProcessing
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (entryStore : List (ValidatorBlock BlockId))
    (time : Time) (validator : Nat) :
    ValidatorWorldState BlockId CommitId PacketId →
      List (ValidatorCommitHead CommitId) →
      List (ValidatorAtomicEvent BlockId CommitId PacketId) →
      ValidatorWorldState BlockId CommitId PacketId → Type
  | done (state) :
      ValidatorFiniteCoreCommitProcessing effects entryStore time validator
        state [] [] state
  | commit
      {before afterScan afterRecord afterPost final :
        ValidatorWorldState BlockId CommitId PacketId}
      {remainingHeads : List (ValidatorCommitHead CommitId)}
      {remainingEvents postCommitEvents :
        List (ValidatorAtomicEvent BlockId CommitId PacketId)}
      (scan : ValidatorCommitterObservation BlockId CommitId)
      (head : ValidatorCommitHead CommitId)
      (unsuspended : List (ValidatorBlock BlockId))
      (scanTime : scan.time = time)
      (scanValidator : scan.validator = validator)
      (scanInput : scan.input = before.validatorState validator)
      (scanSuccessful : ∃ candidateIndex candidateRound,
        scan.result =
          { candidateIndex := some candidateIndex
            candidateRound := some candidateRound })
      (scanReturned : effects.committerReturned scan)
      (scanStep : ValidatorAtomicStep config faults protocolPacket program time
        before (.localAction validator .runCommitter) afterScan)
      (recordStep : ValidatorAtomicStep config faults protocolPacket program
        time afterScan (.localAction validator (.recordCommit head)) afterRecord)
      (postCommitStep : ValidatorWorldStep config faults protocolPacket program
        time afterRecord postCommitEvents afterPost)
      (unsuspendedBound : unsuspended.length ≤ entryStore.length)
      (unsuspendedFromEntry : ∀ block, block ∈ unsuspended →
        block ∈ entryStore)
      (postCommitAcceptsExactlyUnsuspended : ∀ block,
        (.localAction validator (.acceptBlock block)) ∈ postCommitEvents ↔
          block ∈ unsuspended)
      (postCommitEventsOnlyUnsuspend : ∀ event,
        event ∈ postCommitEvents →
        ∃ block, block ∈ unsuspended ∧
          event = .localAction validator (.acceptBlock block))
      (rest : ValidatorFiniteCoreCommitProcessing effects entryStore time
        validator afterPost remainingHeads remainingEvents final) :
      ValidatorFiniteCoreCommitProcessing effects entryStore time validator
        before (head :: remainingHeads)
          (.localAction validator .runCommitter ::
            .localAction validator (.recordCommit head) ::
              postCommitEvents ++ remainingEvents)
        final

/-- One actual finite Core handler episode.

The main-event split gives the concrete input and the complete successful
commit-processing slice. The terminal commit scan and proposal attempt are
handler-local observations because the present main action type cannot
represent calls that return `None`. Local ordinals state their order before
the handler returns. -/
structure ValidatorFiniteCoreHandlerEpisode
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (input : ValidatorCoreHandlerInputObservation BlockId CommitId) where
  beforeInputEvents : List (ValidatorAtomicEvent BlockId CommitId PacketId)
  handlerEvents : List (ValidatorAtomicEvent BlockId CommitId PacketId)
  afterHandlerEvents : List (ValidatorAtomicEvent BlockId CommitId PacketId)
  beforeInput : ValidatorWorldState BlockId CommitId PacketId
  handlerEntry : ValidatorWorldState BlockId CommitId PacketId
  handlerExit : ValidatorWorldState BlockId CommitId PacketId
  eventSplit : timed.execution.events input.time =
    beforeInputEvents ++ input.events ++ handlerEvents ++ afterHandlerEvents
  beforeInputStep : ValidatorWorldStep config faults protocolPacket program
    input.time (timed.execution.trace input.time) beforeInputEvents beforeInput
  inputStep : ValidatorWorldStep config faults protocolPacket program input.time
    beforeInput input.events handlerEntry
  entryStore : List (ValidatorBlock BlockId)
  entryStoreIsLocal : ∀ block, block ∈ entryStore →
    handlerEntry.blockCatalog block.reference.id = some block
  recordedHeads : List (ValidatorCommitHead CommitId)
  processing : ValidatorFiniteCoreCommitProcessing effects entryStore input.time
    input.validator handlerEntry recordedHeads handlerEvents handlerExit
  terminal : ValidatorTerminalCommitScanObservation BlockId CommitId
  terminalTime : terminal.time = input.time
  terminalValidator : terminal.validator = input.validator
  terminalInput : terminal.input = handlerExit.validatorState input.validator
  terminalReturnsNone : validatorPendingCommitterResult terminal.input =
    { candidateIndex := none, candidateRound := none }
  proposalAttempt : ValidatorNormalProposalAttemptObservation BlockId CommitId
  proposalAttemptTime : proposalAttempt.time = input.time
  proposalAttemptValidator : proposalAttempt.validator = input.validator
  proposalAttemptInput : proposalAttempt.input = terminal.input
  proposalAttemptIsNormal : proposalAttempt.force = false
  terminalOrdinal : Nat
  proposalAttemptOrdinal : Nat
  returnOrdinal : Nat
  processingBeforeTerminal : handlerEvents.length ≤ terminalOrdinal
  terminalBeforeProposalAttempt : terminalOrdinal < proposalAttemptOrdinal
  proposalAttemptBeforeReturn : proposalAttemptOrdinal < returnOrdinal
  afterHandlerStep : ValidatorWorldStep config faults protocolPacket program
    input.time handlerExit afterHandlerEvents
      (timed.execution.trace (input.time + 1))

/-- The proposal attempt reads the exact state after finite commit processing.
The remaining same-batch slice, including any successful proposal work, reaches
the next main-trace state. -/
theorem ValidatorFiniteCoreHandlerEpisode.proposal_attempt_input_and_suffix
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution}
    {input : ValidatorCoreHandlerInputObservation BlockId CommitId}
    (episode : ValidatorFiniteCoreHandlerEpisode effects input) :
    episode.proposalAttempt.input =
        episode.handlerExit.validatorState input.validator ∧
      ValidatorWorldStep config faults protocolPacket program input.time
        episode.handlerExit episode.afterHandlerEvents
          (timed.execution.trace (input.time + 1)) := by
  exact ⟨episode.proposalAttemptInput.trans episode.terminalInput,
    episode.afterHandlerStep⟩

/-- The implementation mapping for finite commit work in one actual Core input
handler.

`handlerInputOccurs` is the implementation-level origin marker for one past
`add_blocks` call with a nonempty accepted batch. It is intentionally narrower
than the generic `.acceptBlock` action. That action also occurs in GC
unsuspension paths which do not start a new normal-attempt handler.

The packet-driven source field maps one already delivered and already accepted
ordinary block to its exact past handler input. The last field maps only that
qualifying input to its finite episode. No field states that another handler or
proposal occurs in the future. -/
structure ValidatorCoreHandlerRefinementRules
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution) where
  handlerInputOccurs :
    ValidatorCoreHandlerInputObservation BlockId CommitId → Prop
  packetDrivenAcceptanceHasInputOrigin : ∀ acceptTime validator block,
    ValidatorPacketDrivenBlockAcceptanceAt timed acceptTime validator block →
    ∃ input,
      handlerInputOccurs input ∧
        input.time = acceptTime ∧
        input.validator = validator ∧
        block ∈ input.acceptedBlocks
  qualifyingInputHasFiniteHandler : ∀ input,
    handlerInputOccurs input →
    Nonempty (ValidatorFiniteCoreHandlerEpisode effects input)

/-- Every qualifying actual Core input finishes its finite commit-processing
loop, observes a terminal no-more-commits scan, and invokes one later ordinary
proposal attempt before the same handler returns.

This theorem exposes only observations inside the already occurring handler.
It does not expose a proposal result or future execution. -/
theorem qualifying_core_handler_input_has_terminal_scan_and_normal_proposal_attempt
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution}
    (rules : ValidatorCoreHandlerRefinementRules effects)
    {input : ValidatorCoreHandlerInputObservation BlockId CommitId}
    (occurs : rules.handlerInputOccurs input) :
    ∃ episode : ValidatorFiniteCoreHandlerEpisode effects input,
      validatorPendingCommitterResult episode.terminal.input =
          { candidateIndex := none, candidateRound := none } ∧
        episode.terminalOrdinal < episode.proposalAttemptOrdinal ∧
        episode.proposalAttemptOrdinal < episode.returnOrdinal ∧
        episode.proposalAttempt.force = false := by
  rcases rules.qualifyingInputHasFiniteHandler input occurs with ⟨episode⟩
  exact ⟨episode, episode.terminalReturnsNone,
    episode.terminalBeforeProposalAttempt,
    episode.proposalAttemptBeforeReturn, episode.proposalAttemptIsNormal⟩

/-- One actual packet-driven acceptance has a concrete finite `add_blocks`
episode at the same receiver and logical time. -/
theorem packet_driven_acceptance_has_finite_core_handler
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution}
    (rules : ValidatorCoreHandlerRefinementRules effects)
    {acceptTime : Time} {validator : Nat}
    {block : ValidatorBlock BlockId}
    (accepted : ValidatorPacketDrivenBlockAcceptanceAt timed acceptTime
      validator block) :
    ∃ input,
      input.time = acceptTime ∧
        input.validator = validator ∧
        block ∈ input.acceptedBlocks ∧
        Nonempty (ValidatorFiniteCoreHandlerEpisode effects input) := by
  rcases rules.packetDrivenAcceptanceHasInputOrigin acceptTime validator block
      accepted with
    ⟨input, occurs, inputTime, inputValidator, blockMember⟩
  exact ⟨input, inputTime, inputValidator, blockMember,
    rules.qualifyingInputHasFiniteHandler input occurs⟩

end Mysticeti
