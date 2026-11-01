/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorProcess

namespace Mysticeti

/-! Small-step execution rules for validator-indexed state.

The rules in this file are local. They do not assume block production, parent
availability, common layers, anchors, certificates, synchronization completion,
or commit progress.
-/

/-- A local parent list is ready for one proposal round. -/
def ValidatorParentListReady
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (state : ValidatorLocalState BlockId CommitId)
    (targetRound : Nat)
    (parents : List (ValidatorBlockRef BlockId)) : Prop :=
  (parents.map ValidatorBlockRef.author).Nodup ∧
    (∀ parent, parent ∈ parents →
      parent.round + 1 = targetRound ∧ state.accepted parent = true) ∧
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (validatorParentAuthors parents)

/-- The basic local guard for each concrete validator action.

An implementation can add a stronger local guard. It cannot remove these checks.
-/
def BasicValidatorActionGuard
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (validator : Nat)
    (action : ValidatorLocalAction BlockId CommitId)
    (state : ValidatorLocalState BlockId CommitId) : Prop :=
  match action with
  | .enterRecovery => state.recovery.isNone = true
  | .requestBlock peer reference =>
      peer < config.authorityCount ∧ state.retained reference = false ∧
        state.gcRound < reference.round
  | .serveBlock peer reference =>
      peer < config.authorityCount ∧ state.retained reference = true
  | .acceptBlock block =>
      block.reference.author < config.authorityCount ∧
        state.accepted block.reference = false ∧
        state.gcRound < block.reference.round ∧
        (block.reference.round = 0 ∨
          block.HasQuorumImmediateParents config) ∧
        ∀ parent, parent ∈ block.parents →
          state.accepted parent = true ∨ parent.round ≤ state.gcRound
  | .persistProposal block =>
      block.reference.author = validator ∧
        state.highestSignedRound < block.reference.round ∧
        block.HasQuorumImmediateParents config ∧
        ∀ parent, parent ∈ block.parents → state.accepted parent = true
  | .sendBlock receiver reference =>
      receiver < config.authorityCount ∧
        reference.author = validator ∧
        state.ownBlockAt reference.round = some reference ∧
        state.retained reference = true
  | .sendReplayManifest receiver manifest =>
      receiver < config.authorityCount ∧
        manifest.head.index = manifest.prior.index + 1 ∧
        state.installedCommitAt manifest.head.index = some manifest.head.id
  | .proposeNormal targetRound parents =>
      state.highestSignedRound < targetRound ∧
        ValidatorParentListReady config state targetRound parents ∧
        ∀ parent, parent ∈ parents →
          state.retained parent = true ∧
            (parent.round = 0 ∨ state.gcRound < parent.round)
  | .proposeNext parents =>
      ∃ recovery,
        state.recovery = some recovery ∧
          recovery.alignmentWitness = none ∧
          recovery.targetRound = state.highestSignedRound + 1 ∧
          (∃ deadline,
            recovery.deadline = some deadline ∧ deadline ≤ state.clock) ∧
          ValidatorParentListReady config state recovery.targetRound parents
  | .alignProposal witness parents =>
      ∃ recovery,
        state.recovery = some recovery ∧
          recovery.alignmentWitness = some witness ∧
          state.highestSignedRound < witness.round ∧
          (∃ deadline,
            recovery.deadline = some deadline ∧ deadline ≤ state.clock) ∧
          ValidatorParentListReady config state witness.round parents
  | .runCommitter => state.committer.pendingRounds ≠ []
  | .runReplayCommitter manifest =>
      manifest.head.index = manifest.prior.index + 1 ∧
        state.installedCommitAt manifest.prior.index = some manifest.prior.id ∧
        ∀ reference, reference ∈ manifest.blockReferences →
          state.accepted reference = true
  | .recordCommit head => head.index = state.commitHead.index + 1
  | .applySyncedCommit head => head.index = state.commitHead.index + 1

/-- An option-valued durable map only gains entries. -/
def OptionMapMonotone {Key Value : Type}
    (before after : Key → Option Value) : Prop :=
  ∀ key value, before key = some value → after key = some value

/-- A two-key option-valued durable map only gains entries. -/
def BinaryOptionMapMonotone {Key₁ Key₂ Value : Type}
    (before after : Key₁ → Key₂ → Option Value) : Prop :=
  ∀ key₁ key₂ value,
    before key₁ key₂ = some value → after key₁ key₂ = some value

/-- A Boolean durable map only gains `true` entries. -/
def BoolMapMonotone {Key : Type} (before after : Key → Bool) : Prop :=
  ∀ key, before key = true → after key = true

/-- Durable, installed, and accepted local facts do not go backwards. -/
def ValidatorDurableStateMonotone
    {BlockId CommitId : Type}
    (before after : ValidatorLocalState BlockId CommitId) : Prop :=
  before.commitHead.index ≤ after.commitHead.index ∧
    before.commitHead.round ≤ after.commitHead.round ∧
    (before.commitHead.index = after.commitHead.index →
      before.commitHead = after.commitHead) ∧
    OptionMapMonotone before.installedCommitAt after.installedCommitAt ∧
    OptionMapMonotone before.commitInstallSourceAt
      after.commitInstallSourceAt ∧
    before.lastCommitTime ≤ after.lastCommitTime ∧
    before.highestSignedRound ≤ after.highestSignedRound ∧
    OptionMapMonotone before.ownBlockAt after.ownBlockAt ∧
    BoolMapMonotone before.sentOwnBlockAt after.sentOwnBlockAt ∧
    BoolMapMonotone before.accepted after.accepted ∧
    BinaryOptionMapMonotone before.acceptedRepresentative
      after.acceptedRepresentative ∧
    before.gcRound ≤ after.gcRound

/-- Update one option-valued map entry and preserve all other entries. -/
def OptionMapUpdatesOnly
    {Key Value : Type}
    (before after : Key → Option Value)
    (key : Key) (value : Value) : Prop :=
  after key = some value ∧
    ∀ other, other ≠ key → after other = before other

/-- Update one Boolean map entry and preserve all other entries. -/
def BoolMapSetsOnly
    {Key : Type}
    (before after : Key → Bool) (key : Key) : Prop :=
  after key = true ∧
    ∀ other, other ≠ key → after other = before other

/-- Only proposal persistence can add a durable own block. -/
def OwnBlockActionEffect
    {BlockId CommitId : Type}
    (validator : Nat)
    (action : ValidatorLocalAction BlockId CommitId)
    (before after : ValidatorLocalState BlockId CommitId) : Prop :=
  match action with
  | .persistProposal block =>
      block.reference.author = validator ∧
        OptionMapUpdatesOnly before.ownBlockAt after.ownBlockAt
          block.reference.round block.reference ∧
        after.highestSignedRound = block.reference.round
  | _ =>
      after.ownBlockAt = before.ownBlockAt ∧
        after.highestSignedRound = before.highestSignedRound

/-- Only a block-send action can record that an own block was sent. -/
def SentOwnBlockActionEffect
    {BlockId CommitId : Type}
    (action : ValidatorLocalAction BlockId CommitId)
    (before after : ValidatorLocalState BlockId CommitId) : Prop :=
  match action with
  | .sendBlock _ reference =>
      BoolMapSetsOnly before.sentOwnBlockAt after.sentOwnBlockAt reference.round
  | _ => after.sentOwnBlockAt = before.sentOwnBlockAt

/-- Only block acceptance and proposal persistence can add an accepted block. -/
def AcceptedBlockActionEffect
    {BlockId CommitId : Type}
    (action : ValidatorLocalAction BlockId CommitId)
    (before after : ValidatorLocalState BlockId CommitId) : Prop :=
  match action with
  | .acceptBlock block =>
      BoolMapSetsOnly before.accepted after.accepted block.reference
  | .persistProposal block =>
      BoolMapSetsOnly before.accepted after.accepted block.reference
  | _ => after.accepted = before.accepted

/-- Only local commit execution and verified commit synchronization can install a
commit. The action also records its source. -/
def CommitInstallActionEffect
    {BlockId CommitId : Type}
    (action : ValidatorLocalAction BlockId CommitId)
    (before after : ValidatorLocalState BlockId CommitId) : Prop :=
  match action with
  | .recordCommit head =>
      after.commitHead = head ∧
        after.lastCommitTime = before.clock ∧
        OptionMapUpdatesOnly before.installedCommitAt after.installedCommitAt
          head.index head.id ∧
        OptionMapUpdatesOnly before.commitInstallSourceAt
          after.commitInstallSourceAt head.index .localExecution
  | .applySyncedCommit head =>
      after.commitHead = head ∧
        after.lastCommitTime = before.clock ∧
        OptionMapUpdatesOnly before.installedCommitAt after.installedCommitAt
          head.index head.id ∧
        OptionMapUpdatesOnly before.commitInstallSourceAt
          after.commitInstallSourceAt head.index .verifiedCommitSync
  | _ =>
      after.commitHead = before.commitHead ∧
        after.installedCommitAt = before.installedCommitAt ∧
        after.commitInstallSourceAt = before.commitInstallSourceAt ∧
        after.lastCommitTime = before.lastCommitTime

/-- Source-field restrictions for one local action. -/
def ValidatorActionStructuralEffect
    {BlockId CommitId : Type}
    (validator : Nat)
    (action : ValidatorLocalAction BlockId CommitId)
    (before after : ValidatorLocalState BlockId CommitId) : Prop :=
  after.clock = before.clock ∧
    ValidatorDurableStateMonotone before after ∧
    OwnBlockActionEffect validator action before after ∧
    SentOwnBlockActionEffect action before after ∧
    AcceptedBlockActionEffect action before after ∧
    CommitInstallActionEffect action before after

/-- One implementation-specific local rule for every concrete action. -/
structure ValidatorActionProgram
    (BlockId CommitId : Type) where
  enabled :
    Nat → ValidatorLocalAction BlockId CommitId →
      ValidatorLocalState BlockId CommitId → Prop
  effect :
    Nat → ValidatorLocalAction BlockId CommitId →
      ValidatorLocalState BlockId CommitId →
      ValidatorLocalState BlockId CommitId → Prop

/-- A concrete action is enabled only when its basic and implementation guards
are both true. -/
def ConcreteValidatorActionEnabled
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (program : ValidatorActionProgram BlockId CommitId)
    (validator : Nat)
    (action : ValidatorLocalAction BlockId CommitId)
    (state : ValidatorLocalState BlockId CommitId) : Prop :=
  BasicValidatorActionGuard config validator action state ∧
    program.enabled validator action state

/-- Local processing for one delivered addressed packet. -/
structure ValidatorDeliveryProgram
    (BlockId CommitId : Type) where
  enabled :
    AddressedPacket (ValidatorMessage BlockId CommitId) →
      ValidatorLocalState BlockId CommitId → Prop
  effect :
    AddressedPacket (ValidatorMessage BlockId CommitId) →
      ValidatorLocalState BlockId CommitId →
      ValidatorLocalState BlockId CommitId → Prop

/-- Packet delivery cannot sign a block, record a send, or install a commit. -/
def ValidatorDeliveryStructuralEffect
    {BlockId CommitId : Type}
    (before after : ValidatorLocalState BlockId CommitId) : Prop :=
  after.clock = before.clock ∧
    ValidatorDurableStateMonotone before after ∧
    after.commitHead = before.commitHead ∧
    after.installedCommitAt = before.installedCommitAt ∧
    after.commitInstallSourceAt = before.commitInstallSourceAt ∧
    after.lastCommitTime = before.lastCommitTime ∧
    after.highestSignedRound = before.highestSignedRound ∧
    after.ownBlockAt = before.ownBlockAt ∧
    after.sentOwnBlockAt = before.sentOwnBlockAt

/-- The local program for actions and addressed packet delivery. -/
structure ValidatorExecutionProgram
    (BlockId CommitId : Type) where
  actions : ValidatorActionProgram BlockId CommitId
  delivery : ValidatorDeliveryProgram BlockId CommitId

/-- Existing block and packet history cannot be replaced. -/
def ValidatorWorldHistoryMonotone
    {BlockId CommitId PacketId : Type}
    (before after : ValidatorWorldState BlockId CommitId PacketId) : Prop :=
  OptionMapMonotone before.blockCatalog after.blockCatalog ∧
    OptionMapMonotone before.packets after.packets

/-- Update only one validator's local state. -/
def ValidatorWorldState.updateValidator
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (validator : Nat)
    (state : ValidatorLocalState BlockId CommitId) :
    ValidatorWorldState BlockId CommitId PacketId :=
  { world with
    validatorState := fun current =>
      if current = validator then state else world.validatorState current }

/-- Update only local clocks. -/
def ValidatorWorldState.updateClocks
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (clocks : Nat → Time) : ValidatorWorldState BlockId CommitId PacketId :=
  { world with
    validatorState := fun validator =>
      { world.validatorState validator with clock := clocks validator } }

/-- One atomic event in an execution batch. -/
inductive ValidatorAtomicEvent (BlockId CommitId PacketId : Type) where
  | localAction
      (validator : Nat) (action : ValidatorLocalAction BlockId CommitId)
  | deliverPacket (packetId : PacketId)
  | clockTick

/-- One atomic local, delivery, or clock transition. -/
inductive ValidatorAtomicStep
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop)
    (program : ValidatorExecutionProgram BlockId CommitId)
    (time : Time) :
    ValidatorWorldState BlockId CommitId PacketId →
      ValidatorAtomicEvent BlockId CommitId PacketId →
      ValidatorWorldState BlockId CommitId PacketId → Prop where
  | localAction {before after validator action} :
      validator < config.authorityCount →
      faults.correctAvailable validator = true →
      ConcreteValidatorActionEnabled config program.actions validator action
        (before.validatorState validator) →
      program.actions.effect validator action
        (before.validatorState validator) (after.validatorState validator) →
      ValidatorActionStructuralEffect validator action
        (before.validatorState validator) (after.validatorState validator) →
      (∀ other, other ≠ validator →
        after.validatorState other = before.validatorState other) →
      after.epochActive = before.epochActive →
      ValidatorWorldHistoryMonotone before after →
      ValidatorAtomicStep config faults protocolPacket program time before
        (.localAction validator action) after
  | deliverPacket {before after packetId packet} :
      before.packets packetId = some packet →
      protocolPacket packet →
      packet.deliveredAt = time →
      packet.sender < config.authorityCount →
      packet.receiver < config.authorityCount →
      program.delivery.enabled packet
        (before.validatorState packet.receiver) →
      program.delivery.effect packet
        (before.validatorState packet.receiver)
        (after.validatorState packet.receiver) →
      ValidatorDeliveryStructuralEffect
        (before.validatorState packet.receiver)
        (after.validatorState packet.receiver) →
      (∀ other, other ≠ packet.receiver →
        after.validatorState other = before.validatorState other) →
      after.epochActive = before.epochActive →
      after.blockCatalog = before.blockCatalog →
      after.packets = before.packets →
      ValidatorAtomicStep config faults protocolPacket program time before
        (.deliverPacket packetId) after
  | clockTick {before after clocks} :
      (∀ validator,
        validator < config.authorityCount →
        (before.validatorState validator).clock ≤ clocks validator) →
      after = before.updateClocks clocks →
      ValidatorAtomicStep config faults protocolPacket program time before
        .clockTick after

/-- Apply all atomic events for one logical time. Multiple packets can have the
same delivery time. -/
inductive ValidatorWorldStep
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop)
    (program : ValidatorExecutionProgram BlockId CommitId)
    (time : Time) :
    ValidatorWorldState BlockId CommitId PacketId →
      List (ValidatorAtomicEvent BlockId CommitId PacketId) →
      ValidatorWorldState BlockId CommitId PacketId → Prop where
  | nil {world} :
      ValidatorWorldStep config faults protocolPacket program time world [] world
  | cons {before middle after event events} :
      ValidatorAtomicStep config faults protocolPacket program time before event
        middle →
      ValidatorWorldStep config faults protocolPacket program time middle events
        after →
      ValidatorWorldStep config faults protocolPacket program time before
        (event :: events) after

/-- One concrete local action occurs in one event batch. -/
def ValidatorLocalActionOccurs
    {BlockId CommitId PacketId : Type}
    (events : List (ValidatorAtomicEvent BlockId CommitId PacketId))
    (validator : Nat) (action : ValidatorLocalAction BlockId CommitId) : Prop :=
  ∃ before after,
    events = before ++ (.localAction validator action :: after)

/-- One addressed packet is delivered in one event batch. -/
def ValidatorPacketDeliveryOccurs
    {BlockId CommitId PacketId : Type}
    (events : List (ValidatorAtomicEvent BlockId CommitId PacketId))
    (packetId : PacketId) : Prop :=
  ∃ before after, events = before ++ (.deliverPacket packetId :: after)

/-- One concrete action stays enabled at one correct, available validator. -/
def ValidatorActionContinuouslyEnabled
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (program : ValidatorExecutionProgram BlockId CommitId)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (start : Time) (validator : Nat)
    (action : ValidatorLocalAction BlockId CommitId) : Prop :=
  ∀ time,
    start ≤ time →
    (trace time).epochActive = true ∧
      ConcreteValidatorActionEnabled config program.actions validator action
        ((trace time).validatorState validator)

/-- Foundational executions for the final liveness proofs.

Weak fairness applies only to a fixed concrete local action that stays enabled.
The other fields are local durability, clock, well-formedness, and addressed
delivery conditions. -/
structure ValidatorExecution
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (program : ValidatorExecutionProgram BlockId CommitId) where
  trace : Trace (ValidatorWorldState BlockId CommitId PacketId)
  events : Time → List (ValidatorAtomicEvent BlockId CommitId PacketId)
  stepsFollowRules : ∀ time,
    ValidatorWorldStep config faults protocolPacket program time (trace time)
      (events time) (trace (time + 1))
  statesWellFormed : ∀ time,
    ValidatorWorldStateWellFormed (config := config) (trace time)
  durableStateMonotone : ∀ validator earlier later,
    validator < config.authorityCount →
    earlier ≤ later →
    ValidatorDurableStateMonotone
      ((trace earlier).validatorState validator)
      ((trace later).validatorState validator)
  /-- The installed commit head and static epoch rules determine local GC. -/
  gcRoundForCommitHead : ValidatorCommitHead CommitId → Nat
  /-- GC cannot pass the round of the installed commit head. -/
  gcRoundForCommitHeadAtMostRound : ∀ head,
    gcRoundForCommitHead head ≤ head.round
  correctGcRoundMatchesCommitHead : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).gcRound =
      gcRoundForCommitHead
        ((trace time).validatorState validator).commitHead
  blockCatalogMonotone : ∀ earlier later,
    earlier ≤ later →
    OptionMapMonotone (trace earlier).blockCatalog (trace later).blockCatalog
  packetHistoryMonotone : ∀ earlier later,
    earlier ≤ later →
    OptionMapMonotone (trace earlier).packets (trace later).packets
  clocksMonotone : ∀ validator earlier later,
    validator < config.authorityCount →
    earlier ≤ later →
    ((trace earlier).validatorState validator).clock ≤
      ((trace later).validatorState validator).clock
  clocksProgress : ∀ validator start deadline,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ∃ finish,
      start ≤ finish ∧
      deadline ≤ ((trace finish).validatorState validator).clock
  protocolPacketsAreDelivered : ∀ packetId packet,
    (trace packet.sentAt).packets packetId = some packet →
    protocolPacket packet →
    packet.sender < config.authorityCount →
    packet.receiver < config.authorityCount →
    faults.correctAvailable packet.sender = true →
    faults.correctAvailable packet.receiver = true →
    network.gst ≤ packet.sentAt →
    ValidatorPacketDeliveryOccurs (events packet.deliveredAt) packetId
  fairConcreteActions : ∀ validator action start,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ValidatorActionContinuouslyEnabled config program trace start validator
      action →
    ∃ finish,
      start ≤ finish ∧
      ValidatorLocalActionOccurs (events finish) validator action

end Mysticeti
