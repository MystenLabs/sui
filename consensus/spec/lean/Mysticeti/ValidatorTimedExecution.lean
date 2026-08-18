/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorExecutionLemmas

namespace Mysticeti

/-! Bounded local processing and exact action effects.

This file adds only rules for one validator action or one addressed packet. It
does not assume block production, quorum layers, anchor creation, commit
progress, or synchronization progress.
-/

/-- One concrete action is enabled at one execution time. -/
def ValidatorActionEnabledAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time : Time) (validator : Nat)
    (action : ValidatorLocalAction BlockId CommitId) : Prop :=
  (execution.trace time).epochActive = true ∧
    ConcreteValidatorActionEnabled config program.actions validator action
      ((execution.trace time).validatorState validator)

/-- A concrete local action with its enable and completion times. -/
structure ValidatorActionCompletion
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (localActionBound validator : Nat)
    (action : ValidatorLocalAction BlockId CommitId)
    (enabledAt : Time) where
  event : ValidatorActionEvent BlockId CommitId
  sameValidator : event.validator = validator
  sameAction : event.action = action
  sameEnableTime : event.enabledAt = enabledAt
  enabled : ValidatorActionEnabledAt execution event.enabledAt validator action
  enableBeforeCompletion : event.enabledAt ≤ event.completedAt
  completesWithinBound : event.completedAt ≤ event.enabledAt + localActionBound
  occurs : ValidatorLocalActionOccurs (execution.events event.completedAt)
    validator action

/-- One action has not run in a half-open local time interval. -/
def ValidatorActionAbsentBefore
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start finish : Time) (validator : Nat)
    (action : ValidatorLocalAction BlockId CommitId) : Prop :=
  ∀ time, start ≤ time → time < finish →
    ¬ValidatorLocalActionOccurs (execution.events time) validator action

/-- Bounded local execution applies only to protected, latched work.

A protected action stays protected until it runs. A transient enabled action is
not protected by this structure. Such an action uses the weak continuous
fairness rule in `ValidatorExecution`.
-/
structure ValidatorBoundedExecution
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (program : ValidatorExecutionProgram BlockId CommitId) where
  execution : ValidatorExecution (PacketId := PacketId) config faults
    protocolPacket network program
  localActionBound : Nat
  localActionBoundPositive : 0 < localActionBound
  protectedAction :
    Time → Nat → ValidatorLocalAction BlockId CommitId → Prop
  protectedActionIsEnabled : ∀ time validator action,
    protectedAction time validator action →
    ValidatorActionEnabledAt execution time validator action
  protectedActionPersistsUntilRun : ∀ start finish validator action,
    protectedAction start validator action →
    start ≤ finish →
    ValidatorActionAbsentBefore execution start finish validator action →
    protectedAction finish validator action
  completeProtectedAction : ∀ validator action latchedAt,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    protectedAction latchedAt validator action →
    ValidatorActionCompletion execution localActionBound validator action
      latchedAt

/-- The status list stored at one zero-based pending-round index. -/
def validatorPendingSlotStatusesAt :
    List ValidatorPendingRound → Nat → List SelectedLeaderSlotStatus
  | [], _ => []
  | pending :: _, 0 => pending.selectedLeaderSlotStatuses
  | _ :: remaining, index + 1 =>
      validatorPendingSlotStatusesAt remaining index

/-- The protocol round stored at one zero-based pending-round index. -/
def validatorPendingRoundAt :
    List ValidatorPendingRound → Nat → Option Nat
  | [], _ => none
  | pending :: _, 0 => some pending.round
  | _ :: remaining, index + 1 => validatorPendingRoundAt remaining index

/-- The FlexCommitter input made from one validator's pending-round data. -/
def validatorPendingFlexState
    {BlockId CommitId : Type}
    (state : ValidatorLocalState BlockId CommitId) : FlexCommitState :=
  flexCommitStateFromSlotStatuses state.commitHead.index
    state.committer.pendingRounds.length
    (validatorPendingSlotStatusesAt state.committer.pendingRounds)

/-- The exact local result of the status scan over pending rounds. -/
structure ValidatorPendingCommitterResult where
  candidateIndex : Option Nat
  candidateRound : Option Nat
  deriving DecidableEq, Repr

/-- Compute the exact pending-round result for one local state. -/
def validatorPendingCommitterResult
    {BlockId CommitId : Type}
    (state : ValidatorLocalState BlockId CommitId) :
    ValidatorPendingCommitterResult :=
  let candidateIndex := findFlexCommitRound (validatorPendingFlexState state)
  { candidateIndex
    candidateRound := candidateIndex.bind
      (validatorPendingRoundAt state.committer.pendingRounds) }

/-- One observed return value from a local committer action. -/
structure ValidatorCommitterObservation
    (BlockId CommitId : Type) where
  time : Time
  validator : Nat
  input : ValidatorLocalState BlockId CommitId
  result : ValidatorPendingCommitterResult

/-- Exact effects that are not in the base structural transition.

The committer observation is ghost data. It records an action return value. It
does not add a result to the validator's durable state.
-/
structure ValidatorExactExecutionEffects
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (program : ValidatorExecutionProgram BlockId CommitId)
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program) where
  committerReturned :
    ValidatorCommitterObservation BlockId CommitId → Prop
  sendBlockCreatesPacket : ∀ time
      (before after : ValidatorWorldState BlockId CommitId PacketId)
      validator receiver reference,
    ValidatorAtomicStep config faults protocolPacket program time before
      (.localAction validator (.sendBlock receiver reference)) after →
    ∃ (packetId : PacketId) (block : ValidatorBlock BlockId)
      (packet : AddressedPacket (ValidatorMessage BlockId CommitId)),
      before.blockCatalog reference.id = some block ∧
      block.reference = reference ∧
      before.packets packetId = none ∧
      after.packets packetId = some packet ∧
      protocolPacket packet ∧
      packet.sender = validator ∧
      packet.receiver = receiver ∧
      packet.payload = .block block ∧
      packet.sentAt = time + 1
  proposalEnablesPersistence : ∀ time validator parents,
    ValidatorLocalActionOccurs (execution.events time) validator
      (.proposeNext parents) →
    ∃ block,
      block.reference.author = validator ∧
      block.parents = parents ∧
      ValidatorActionEnabledAt execution (time + 1) validator
        (.persistProposal block)
  normalProposalEnablesPersistence : ∀ time validator targetRound parents,
    ValidatorLocalActionOccurs (execution.events time) validator
      (.proposeNormal targetRound parents) →
    ∃ block,
      block.reference.author = validator ∧
      block.reference.round = targetRound ∧
      block.parents = parents ∧
      ValidatorActionEnabledAt execution (time + 1) validator
        (.persistProposal block)
  persistedProposalStoresBlock : ∀ time validator block,
    ValidatorLocalActionOccurs (execution.events time) validator
      (.persistProposal block) →
    (execution.trace (time + 1)).blockCatalog block.reference.id = some block
  runCommitterReturnsExactResult : ∀ time
      (before after : ValidatorWorldState BlockId CommitId PacketId) validator,
    ValidatorAtomicStep config faults protocolPacket program time before
      (.localAction validator .runCommitter) after →
    committerReturned
      { time
        validator
        input := before.validatorState validator
        result := validatorPendingCommitterResult
          (before.validatorState validator) }
  everyCommitterResultIsExact : ∀ observation,
    committerReturned observation →
    observation.result = validatorPendingCommitterResult observation.input

end Mysticeti
