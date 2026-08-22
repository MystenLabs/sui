/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorRecoverySourcePinExecution
import Mysticeti.ValidatorRecoveryTimerDerivation
import Mysticeti.ValidatorRecoveryMode

namespace Mysticeti

/-! Durable requester-local parent work for recovery and normal proposals.

One need contains a finite candidate list. The requester can stop work on all
other candidates as soon as one local quorum parent list is ready. Recovery
work starts the protected timer-arm worker. Normal work starts the protected
normal proposal builder.
-/

/-- The same host discovers a parent need from one delivered child or from its
current pinned tip and accepted representatives. -/
inductive ValidatorRecoveryParentNeedOrigin where
  | deliveredChild
  | pinnedTip
  | localAccumulator
  deriving DecidableEq, Repr

/-- One requester-local parent candidate pool for one proposal target. -/
structure ValidatorRecoveryParentNeed
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  proposalOrigin : ValidatorProposalOrigin
  discoveryOrigin : ValidatorRecoveryParentNeedOrigin
  capsuleKey : Option (ValidatorRecoveryCapsuleKey BlockId)
  baselineCommit : ValidatorCommitHead CommitId
  signerFloor : Nat
  targetRound : Nat
  targetAboveSignerFloor : signerFloor < targetRound
  recoveryTargetIsExactNext :
    proposalOrigin = .commitProgressRecovery →
      targetRound = signerFloor + 1
  sourceBlock : Option (ValidatorBlock BlockId)
  candidateRefs : List (ValidatorBlockRef BlockId)
  candidateRefsNodup : candidateRefs.Nodup
  deliveredChildSourceShape :
    discoveryOrigin = .deliveredChild →
      ∃ block,
        sourceBlock = some block ∧
          block.reference.round = targetRound ∧
          block.HasQuorumImmediateParents config ∧
          candidateRefs = block.parents
  deliveredChildHasNoCapsuleKey :
    discoveryOrigin = .deliveredChild → capsuleKey = none
  pinnedTipHasNoChild :
    discoveryOrigin = .pinnedTip → sourceBlock = none
  pinnedTipHasCapsuleKey :
    discoveryOrigin = .pinnedTip → capsuleKey.isSome = true
  localAccumulatorHasNoChild :
    discoveryOrigin = .localAccumulator → sourceBlock = none
  localAccumulatorHasNoCapsuleKey :
    discoveryOrigin = .localAccumulator → capsuleKey = none
  candidatesAreImmediate : ∀ reference,
    reference ∈ candidateRefs → reference.round + 1 = targetRound

/-- Replace stale recovery work with fresh normal parent acquisition after one
local commit install. The replacement does not retain the old target. -/
def ValidatorRecoveryParentNeedIsRebase
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (_before after :
      ValidatorRecoveryParentNeed BlockId CommitId config)
    (newHead : ValidatorCommitHead CommitId) : Prop :=
  after.proposalOrigin = .normal ∧
    after.discoveryOrigin = .localAccumulator ∧
    after.baselineCommit = newHead

/-- Add every newly discovered exact reference. The final ready parent list,
not this candidate pool, selects at most one reference per author. -/
def ValidatorRecoveryParentNeedIsExactExtension
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (before : ValidatorRecoveryParentNeed BlockId CommitId config)
    (discovered : ValidatorBlockRef BlockId → Bool)
    (after : ValidatorRecoveryParentNeed BlockId CommitId config) :
    Prop :=
  after.proposalOrigin = before.proposalOrigin ∧
    after.discoveryOrigin = before.discoveryOrigin ∧
    after.capsuleKey = before.capsuleKey ∧
    after.baselineCommit = before.baselineCommit ∧
    after.signerFloor = before.signerFloor ∧
    after.targetRound = before.targetRound ∧
    after.sourceBlock = before.sourceBlock ∧
    ∀ reference,
      reference ∈ after.candidateRefs ↔
        reference ∈ before.candidateRefs ∨
          discovered reference = true

/-- One accumulator update preserves every earlier candidate and its target. -/
def ValidatorRecoveryParentNeedAccumulatorExtends
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (before after :
      ValidatorRecoveryParentNeed BlockId CommitId config) : Prop :=
  after.proposalOrigin = before.proposalOrigin ∧
    after.baselineCommit = before.baselineCommit ∧
    after.signerFloor = before.signerFloor ∧
    after.targetRound = before.targetRound ∧
    ∀ reference, reference ∈ before.candidateRefs →
      reference ∈ after.candidateRefs

/-- Isolated durable state for one requester's current recovery need. -/
structure ValidatorRecoveryParentNeedState
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  active : Option
    (ValidatorRecoveryParentNeed BlockId CommitId config)

/-- One batch of local recovery-need inputs. -/
structure ValidatorRecoveryParentNeedEvent
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  startNeed : Option
    (ValidatorRecoveryParentNeed BlockId CommitId config)
  extendNeed : Option
    (ValidatorRecoveryParentNeed BlockId CommitId config)
  rebaseNeed : Option
    (ValidatorRecoveryParentNeed BlockId CommitId config)
  timerArmLatched : Bool

/-- One exact change to durable requester-local work. -/
inductive ValidatorRecoveryParentNeedTransition
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId} :
    ValidatorRecoveryParentNeedState BlockId CommitId config →
      ValidatorRecoveryParentNeedEvent BlockId CommitId config →
      ValidatorLocalState BlockId CommitId → Bool →
      ValidatorRecoveryParentNeedState BlockId CommitId config →
      Prop where
  | idle {before event mainAfter after} :
      before.active = none →
      event.startNeed = none →
      event.extendNeed = none →
      event.rebaseNeed = none →
      event.timerArmLatched = false →
      after.active = none →
      ValidatorRecoveryParentNeedTransition before event mainAfter true after
  | start {before event mainAfter after need} :
      before.active = none →
      event.startNeed = some need →
      event.extendNeed = none →
      event.rebaseNeed = none →
      event.timerArmLatched = false →
      after.active = some need →
      ValidatorRecoveryParentNeedTransition before event mainAfter true after
  | keep {before event mainAfter after need} :
      before.active = some need →
      event.startNeed = none →
      event.extendNeed = none →
      event.rebaseNeed = none →
      event.timerArmLatched = false →
      mainAfter.highestSignedRound < need.targetRound →
      after.active = some need →
      ValidatorRecoveryParentNeedTransition before event mainAfter true after
  | extend {before event mainAfter after need extended} :
      before.active = some need →
      event.startNeed = none →
      event.extendNeed = some extended →
      event.rebaseNeed = none →
      event.timerArmLatched = false →
      ValidatorRecoveryParentNeedAccumulatorExtends need extended →
      mainAfter.highestSignedRound < extended.targetRound →
      after.active = some extended →
      ValidatorRecoveryParentNeedTransition before event mainAfter true after
  | rebase {before event mainAfter after need rebased} :
      before.active = some need →
      event.startNeed = none →
      event.extendNeed = none →
      event.rebaseNeed = some rebased →
      event.timerArmLatched = false →
      ValidatorRecoveryParentNeedIsRebase need rebased
        mainAfter.commitHead →
      mainAfter.highestSignedRound < rebased.targetRound →
      after.active = some rebased →
      ValidatorRecoveryParentNeedTransition before event mainAfter true after
  | timerArmed {before event mainAfter after need} :
      before.active = some need →
      event.startNeed = none →
      event.extendNeed = none →
      event.rebaseNeed = none →
      event.timerArmLatched = true →
      after.active = some need →
      ValidatorRecoveryParentNeedTransition before event mainAfter true after
  | timerArmedAndRebase {before event mainAfter after need rebased} :
      before.active = some need →
      event.startNeed = none →
      event.extendNeed = none →
      event.rebaseNeed = some rebased →
      event.timerArmLatched = true →
      ValidatorRecoveryParentNeedIsRebase need rebased
        mainAfter.commitHead →
      mainAfter.highestSignedRound < rebased.targetRound →
      after.active = some rebased →
      ValidatorRecoveryParentNeedTransition before event mainAfter true after
  | targetReached {before event mainAfter after need} :
      before.active = some need →
      event.startNeed = none →
      event.extendNeed = none →
      event.rebaseNeed = none →
      event.timerArmLatched = false →
      need.targetRound ≤ mainAfter.highestSignedRound →
      after.active = none →
      ValidatorRecoveryParentNeedTransition before event mainAfter true after
  | epochEnded {before event mainAfter after} :
      event.startNeed = none →
      event.extendNeed = none →
      event.rebaseNeed = none →
      event.timerArmLatched = false →
      after.active = none →
      ValidatorRecoveryParentNeedTransition before event mainAfter false after

/-- One need has a local quorum parent list. Unresolved extra candidates do not
matter after this predicate becomes true. -/
def ValidatorRecoveryParentNeedReadyAt
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (state : ValidatorLocalState BlockId CommitId)
    (need : ValidatorRecoveryParentNeed BlockId CommitId config) :
    Prop :=
  ∃ parents,
    (∀ parent, parent ∈ parents → parent ∈ need.candidateRefs) ∧
      ValidatorProposalParentListReady need.proposalOrigin config state
        need.targetRound parents

/-- One unresolved candidate is a durable local block-sync goal. -/
def ValidatorRecoveryParentSyncGoalAt
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (needState :
      ValidatorRecoveryParentNeedState BlockId CommitId config)
    (mainState : ValidatorLocalState BlockId CommitId)
    (reference : ValidatorBlockRef BlockId) : Prop :=
  ∃ need,
    needState.active = some need ∧
      reference ∈ need.candidateRefs ∧
      ¬ValidatorRecoveryParentNeedReadyAt mainState need ∧
      mainState.gcRound < reference.round ∧
      mainState.accepted reference = false

/-- Canonical retained genesis parents for a validator whose signer floor is
zero. This case does not require an existing round-one child block. -/
def ValidatorCanonicalGenesisParentReadyAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time) : Prop :=
    ((timed.execution.trace time).validatorState
      validator).highestSignedRound = 0 ∧
      ((timed.execution.trace time).validatorState
        validator).recovery.isNone = true ∧
      (timed.execution.trace time).epochActive = true ∧
      ∃ parents,
        ValidatorProposalParentListReady .commitProgressRecovery config
          ((timed.execution.trace time).validatorState validator) 1 parents

/-- One legal normal parent list and its protected local builder action. -/
def ValidatorNormalParentBuildReadyAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time) : Prop :=
  ∃ targetRound parents,
    ((timed.execution.trace time).validatorState
        validator).highestSignedRound < targetRound ∧
      ValidatorProposalParentListReady .normal config
        ((timed.execution.trace time).validatorState validator) targetRound
          parents ∧
      timed.protectedAction time validator
        (.proposeNormal targetRound parents)

/-- One need came from one exact authenticated block packet delivered to the
same host. The child can wait for missing parents before local acceptance. -/
def ValidatorDeliveredChildNeedSourceAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time)
    (need : ValidatorRecoveryParentNeed BlockId CommitId config) :
    Prop :=
  need.discoveryOrigin = .deliveredChild ∧
    need.baselineCommit =
      ((timed.execution.trace time).validatorState validator).commitHead ∧
    need.signerFloor =
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound ∧
    ∃ (packetId : PacketId)
        (packet : AddressedPacket (ValidatorMessage BlockId CommitId))
        (block : ValidatorBlock BlockId),
      (timed.execution.trace time).packets packetId = some packet ∧
        protocolPacket packet ∧
        packet.receiver = validator ∧
        packet.payload = .block block ∧
        ValidatorPacketDeliveryOccurs (timed.execution.events time) packetId ∧
      need.sourceBlock = some block ∧
        block.reference.round = need.targetRound

/-- One need came from an exact same-host pinned capsule. -/
def ValidatorPinnedTipNeedSourceAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (time validator : Time)
    (need : ValidatorRecoveryParentNeed BlockId CommitId config) :
    Prop :=
  need.discoveryOrigin = .pinnedTip ∧
    need.sourceBlock = none ∧
    need.baselineCommit =
      ((timed.execution.trace time).validatorState validator).commitHead ∧
    need.signerFloor =
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound ∧
    ∃ capsuleKey entry tipBlock,
      need.capsuleKey = some capsuleKey ∧
        (pins.trace time validator).capsuleAt capsuleKey = some entry ∧
        (pins.trace time validator).pinned capsuleKey = true ∧
        entry.capsule.targetBlock = tipBlock ∧
        tipBlock.reference.round = need.signerFloor ∧
        ((timed.execution.trace time).validatorState validator).ownBlockAt
          need.signerFloor = some tipBlock.reference ∧
        (∀ reference, reference ∈ need.candidateRefs →
          ((timed.execution.trace time).validatorState
            validator).acceptedRepresentative need.signerFloor
              reference.author = some reference) ∧
        (∀ author reference,
          author < config.authorityCount →
          ((timed.execution.trace time).validatorState
            validator).acceptedRepresentative need.signerFloor author =
              some reference →
          reference ∈ need.candidateRefs)

/-- One parent source uses the host's current accepted representatives for one
target. It can extend normal or recovery work with the same floor and target. -/
def ValidatorLocalAccumulatorNeedSourceAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time)
    (need : ValidatorRecoveryParentNeed BlockId CommitId config) :
    Prop :=
  need.discoveryOrigin = .localAccumulator ∧
    need.sourceBlock = none ∧
    need.baselineCommit =
      ((timed.execution.trace time).validatorState validator).commitHead ∧
    need.signerFloor =
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound ∧
    (∀ reference, reference ∈ need.candidateRefs →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative reference.round reference.author =
          some reference) ∧
    (∀ author reference,
      author < config.authorityCount →
      ((timed.execution.trace time).validatorState
        validator).acceptedRepresentative (need.targetRound - 1) author =
          some reference →
      reference ∈ need.candidateRefs)

/-- Fresh normal work uses one canonical target above the signer floor and the
current GC boundary. Later commits cannot replace this target. -/
def ValidatorFreshNormalAccumulatorNeedSourceAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time validator : Time)
    (need : ValidatorRecoveryParentNeed BlockId CommitId config) :
    Prop :=
  ValidatorLocalAccumulatorNeedSourceAt timed time validator need ∧
    need.proposalOrigin = .normal ∧
    need.targetRound = Nat.max (need.signerFloor + 1)
      (((timed.execution.trace time).validatorState validator).gcRound + 2)

/-- One need has an exact local discovery source. -/
def ValidatorRecoveryParentNeedLocalSourceAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (time validator : Time)
    (need : ValidatorRecoveryParentNeed BlockId CommitId config) :
    Prop :=
  ValidatorDeliveredChildNeedSourceAt timed time validator need ∨
    ValidatorPinnedTipNeedSourceAt pins time validator need ∨
    ValidatorLocalAccumulatorNeedSourceAt timed time validator need

/-- One durable requester execution connected to source pins, block sync, and
the protected timer-arm worker. -/
structure ValidatorRecoveryParentNeedExecution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits}
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (recoveryWait : Time) where
  trace : Time → Nat →
    ValidatorRecoveryParentNeedState BlockId CommitId config
  event : Time → Nat →
    ValidatorRecoveryParentNeedEvent BlockId CommitId config
  /-- Exact new references discovered by the host in one batch. -/
  discoveredCandidate : Time → Nat → ValidatorBlockRef BlockId → Bool
  transitionsFollowRules : ∀ time validator,
    ValidatorRecoveryParentNeedTransition (trace time validator)
      (event time validator)
      ((timed.execution.trace (time + 1)).validatorState validator)
      (timed.execution.trace (time + 1)).epochActive
      (trace (time + 1) validator)
  /-- Every active need matches the signer state. Recovery uses the current
  commit head. Protected normal work can keep an older commit baseline. -/
  activeNeedMatchesMain : ∀ time validator need,
    (trace time validator).active = some need →
    need.baselineCommit.index ≤
        ((timed.execution.trace time).validatorState
          validator).commitHead.index ∧
      need.signerFloor =
        ((timed.execution.trace time).validatorState
          validator).highestSignedRound ∧
      (timed.execution.trace time).epochActive = true ∧
      (need.proposalOrigin = .commitProgressRecovery →
        need.baselineCommit =
            ((timed.execution.trace time).validatorState validator).commitHead ∧
          ((timed.execution.trace time).validatorState
              validator).recovery.isNone = true ∧
            ValidatorCommitProgressRecoveryModeAt timed recoveryWait time
              validator)
  /-- The source block of every active need is exact durable block data. -/
  activeNeedSourceIsCatalogued : ∀ time validator need block,
    (trace time validator).active = some need →
    need.sourceBlock = some block →
    (timed.execution.trace time).blockCatalog block.reference.id = some block
  /-- An accepted candidate remains retained while its need is active. -/
  acceptedCandidateIsPinned : ∀ time validator need reference,
    (trace time validator).active = some need →
    reference ∈ need.candidateRefs →
    (reference.round = 0 ∨
      ((timed.execution.trace time).validatorState validator).gcRound <
        reference.round) →
    ((timed.execution.trace time).validatorState validator).accepted
        reference = true →
    ((timed.execution.trace time).validatorState validator).retained
        reference = true
  /-- All active work keeps its immediate-parent round above local GC. Round
  one uses only static genesis parents. -/
  activeNeedFencesTargetRound : ∀ time validator need,
    (trace time validator).active = some need →
    (need.targetRound = 1 ∧
      ((timed.execution.trace time).validatorState validator).gcRound = 0) ∨
      ((timed.execution.trace time).validatorState validator).gcRound + 1 <
        need.targetRound
  /-- Requester work starts empty. Restored pins and later local discovery
  create exact work through the normal transition rules. -/
  initialStateEmpty : ∀ validator,
    (trace 0 validator).active = none
  /-- A started need has an exact local source. A commit can create fresh
  normal work from the post-commit local state in the same batch. -/
  startedNeedHasLocalSource : ∀ time validator need,
    (event time validator).startNeed = some need →
    ValidatorRecoveryParentNeedLocalSourceAt pins time validator need ∨
      (ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
          validator need ∧
        ((∃ head,
          ValidatorCommitInstallOccurs (timed.execution.events time) validator
            head) ∨
          (ValidatorCommitProgressRecoveryModeAt timed recoveryWait (time + 1)
              validator ∧
            0 < ((timed.execution.trace (time + 1)).validatorState
              validator).highestSignedRound ∧
            ((timed.execution.trace (time + 1)).validatorState
                validator).highestSignedRound ≤
              ((timed.execution.trace (time + 1)).validatorState
                validator).gcRound)))
  /-- Every normal start is the exact fresh canonical accumulator selected by
  this host. A normal start can use the batch input state or its result state. -/
  startedNormalNeedHasFreshSource : ∀ time validator need,
    (event time validator).startNeed = some need →
    need.proposalOrigin = .normal →
    ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator need ∨
      ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
        validator need
  /-- Every accumulator extension is exactly the old pool plus all discovered
  references. -/
  extendedNeedIsExact : ∀ time validator extended,
    (event time validator).extendNeed = some extended →
    ∃ before,
      (trace time validator).active = some before ∧
        ValidatorRecoveryParentNeedIsExactExtension before
          (discoveredCandidate time validator) extended
  /-- Each discovered candidate has one exact local source in the same batch. -/
  discoveredCandidateHasLocalSource : ∀ time validator before reference,
    (trace time validator).active = some before →
    discoveredCandidate time validator reference = true →
    ∃ source,
      ValidatorRecoveryParentNeedLocalSourceAt pins time validator source ∧
        source.signerFloor = before.signerFloor ∧
        source.targetRound = before.targetRound ∧
        reference ∈ source.candidateRefs
  /-- Every exact reference from a compatible local source is discovered. A
  Byzantine equivocation can add extra candidates, but cannot replace or block
  a later correct reference. -/
  compatibleSourceCandidateIsDiscovered : ∀ time validator before source
      reference,
    (trace time validator).active = some before →
    ValidatorRecoveryParentNeedLocalSourceAt pins time validator source →
    source.signerFloor = before.signerFloor →
    source.targetRound = before.targetRound →
    reference ∈ source.candidateRefs →
    discoveredCandidate time validator reference = true
  /-- Each current retained representative has a same-host accumulator source
  for the active recovery target. This is a current-state construction, not a
  future discovery result. -/
  retainedRepresentativeHasAccumulatorSource : ∀ time validator before author
      reference,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (trace time validator).active = some before →
    before.proposalOrigin = .commitProgressRecovery →
    ((timed.execution.trace time).validatorState validator).retained reference =
      true →
    ((timed.execution.trace time).validatorState
      validator).acceptedRepresentative (before.targetRound - 1) author =
        some reference →
    ∃ source,
      ValidatorLocalAccumulatorNeedSourceAt timed time validator source ∧
        source.signerFloor = before.signerFloor ∧
        source.targetRound = before.targetRound ∧
        reference ∈ source.candidateRefs
  /-- A timer latch has priority in its batch. In other batches, every new
  reference extends the durable accumulator, including while a timer runs. -/
  discoveredCandidatesExtendActiveNeed : ∀ time validator before,
    (trace time validator).active = some before →
    (event time validator).rebaseNeed = none →
    (event time validator).timerArmLatched = false →
    ((timed.execution.trace (time + 1)).validatorState
      validator).highestSignedRound < before.targetRound →
    (∃ reference, discoveredCandidate time validator reference = true) →
    ∃ extended,
      (event time validator).extendNeed = some extended ∧
        ValidatorRecoveryParentNeedIsExactExtension before
          (discoveredCandidate time validator) extended
  /-- Every active candidate has a past exact source at the same host. -/
  activeCandidateHasLocalSource : ∀ time validator need reference,
    (trace time validator).active = some need →
    reference ∈ need.candidateRefs →
    ∃ sourceTime source,
      sourceTime ≤ time ∧
        ValidatorRecoveryParentNeedLocalSourceAt pins sourceTime validator
          source ∧
        source.signerFloor = need.signerFloor ∧
        source.targetRound = need.targetRound ∧
        reference ∈ source.candidateRefs
  /-- An idle recovery host selects a recovery source while recovery mode is
  active. -/
  availableRecoverySourceStartsWhenIdle : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (∀ head, ¬ValidatorCommitInstallOccurs (timed.execution.events time)
      validator head) →
    (trace time validator).active = none →
    ValidatorCommitProgressRecoveryModeAt timed recoveryWait (time + 1)
      validator →
    (∃ need,
      ValidatorRecoveryParentNeedLocalSourceAt pins time validator need ∧
        need.proposalOrigin = .commitProgressRecovery ∧
        ((need.targetRound = 1 ∧
            ((timed.execution.trace (time + 1)).validatorState
              validator).gcRound = 0) ∨
          ((timed.execution.trace (time + 1)).validatorState
              validator).gcRound + 1 < need.targetRound)) →
    ∃ need,
      (event time validator).startNeed = some need ∧
        ValidatorRecoveryParentNeedLocalSourceAt pins time validator need ∧
        need.proposalOrigin = .commitProgressRecovery ∧
        ((need.targetRound = 1 ∧
            ((timed.execution.trace (time + 1)).validatorState
              validator).gcRound = 0) ∨
          ((timed.execution.trace (time + 1)).validatorState
              validator).gcRound + 1 < need.targetRound)
  /-- If the signer floor is already a committed root, recovery mode starts
  fresh normal work above local GC. -/
  recoveryRootStartsNormalNeedWhenIdle : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (∀ head, ¬ValidatorCommitInstallOccurs (timed.execution.events time)
      validator head) →
    (trace time validator).active = none →
    ValidatorCommitProgressRecoveryModeAt timed recoveryWait (time + 1)
      validator →
    0 < ((timed.execution.trace (time + 1)).validatorState
      validator).gcRound →
    ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound ≤
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound →
    (timed.execution.trace (time + 1)).epochActive = true →
    ∃ need,
      (event time validator).startNeed = some need ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
          validator need
  /-- Outside recovery mode, an idle host selects fresh normal parent work. -/
  availableNormalSourceStartsWhenIdle : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (∀ head, ¬ValidatorCommitInstallOccurs (timed.execution.events time)
      validator head) →
    (trace time validator).active = none →
    (timed.execution.trace (time + 1)).epochActive = true →
    ¬ValidatorCommitProgressRecoveryModeAt timed recoveryWait (time + 1)
      validator →
    (∃ need,
      ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator need) →
    ∃ need,
      (event time validator).startNeed = some need ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator need
  /-- A need rebase has an actual commit install in the same batch. -/
  rebasedNeedHasCommitSource : ∀ time validator rebased,
    (event time validator).rebaseNeed = some rebased →
    ∃ need head,
      (trace time validator).active = some need ∧
        need.proposalOrigin = .commitProgressRecovery ∧
        ValidatorCommitInstallOccurs (timed.execution.events time) validator
          head ∧
        ValidatorRecoveryParentNeedIsRebase need rebased
          ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1) validator
          rebased
  /-- A commit advance rebases unresolved work as normal work in the same local
  transition. -/
  commitAdvanceRebasesNeed : ∀ time validator need,
    (trace time validator).active = some need →
    need.proposalOrigin = .commitProgressRecovery →
    need.baselineCommit.index <
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead.index →
    ((timed.execution.trace (time + 1)).validatorState
      validator).highestSignedRound < need.targetRound →
    ∃ rebased,
      (event time validator).rebaseNeed = some rebased ∧
        ValidatorRecoveryParentNeedIsRebase need rebased
          ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1) validator
          rebased
  /-- A commit install starts fresh normal work when the requester is idle. -/
  commitInstallStartsNormalNeedWhenIdle : ∀ time validator head,
    (trace time validator).active = none →
    ValidatorCommitInstallOccurs (timed.execution.events time) validator head →
    (timed.execution.trace (time + 1)).epochActive = true →
    ∃ need,
      (event time validator).startNeed = some need ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
          validator need
  /-- Later commits do not replace open normal work. Its target, candidate pool,
  and GC fence remain until the target is signed. This is an explicit host
  scheduling rule. Partial synchrony does not imply this rule. -/
  normalNeedSurvivesCommit : ∀ time validator need head,
    (trace time validator).active = some need →
    need.proposalOrigin = .normal →
    ValidatorCommitInstallOccurs (timed.execution.events time) validator head →
    (timed.execution.trace (time + 1)).epochActive = true →
    ((timed.execution.trace (time + 1)).validatorState
      validator).highestSignedRound < need.targetRound →
    ∃ afterNeed,
      (trace (time + 1) validator).active = some afterNeed ∧
        ValidatorRecoveryParentNeedAccumulatorExtends need afterNeed
  /-- The sync goal is live exactly while the local need is unresolved. -/
  syncGoalIsNotObsolete : ∀ time validator reference,
    ValidatorRecoveryParentSyncGoalAt (trace time validator)
      ((timed.execution.trace time).validatorState validator) reference →
    ¬syncRules.goalObsolete validator reference time
  /-- A ready need selects a timer goal for the exact recovery target. The
  proposal selects fresh retained parents when the timer expires. -/
  readyNeedSelectsTimerGoal : ∀ time validator need,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (trace time validator).active = some need →
    need.proposalOrigin = .commitProgressRecovery →
    ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need →
    (arms.trace time validator).pending = none →
    ∃ goal,
      arms.selectedGoal time validator = some goal ∧
        goal.validator = validator ∧
        goal.targetRound = need.targetRound
  /-- A ready normal need protects its exact local proposal-builder action. -/
  readyNormalNeedProtectsBuild : ∀ time validator need parents,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (trace time validator).active = some need →
    need.proposalOrigin = .normal →
    (∀ parent, parent ∈ parents → parent ∈ need.candidateRefs) →
    ValidatorProposalParentListReady .normal config
      ((timed.execution.trace time).validatorState validator)
      need.targetRound parents →
    timed.protectedAction time validator
      (.proposeNormal need.targetRound parents)
  /-- The requester records exactly the timer-arm latches for its active need. -/
  timerArmLatchIff : ∀ time validator,
    (event time validator).timerArmLatched = true ↔
      ∃ need goal,
        (trace time validator).active = some need ∧
          arms.selectedGoal time validator = some goal ∧
          arms.events time validator = .latch goal ∧
          need.proposalOrigin = .commitProgressRecovery ∧
          goal.targetRound = need.targetRound

namespace ValidatorRecoveryParentNeedExecution

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {syncRules : ValidatorBlockSyncExecutionRules timed}
variable {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
variable {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
  network program timed waits}
variable {pins : ValidatorRecoverySourcePinExecution syncRules}
variable {arms : ValidatorRecoveryTimerArmExecution timerSource}
variable {recoveryWait : Time}
variable {time : Time}
variable {validator : Nat}

/-- An accumulator need extends itself. -/
theorem accumulator_extends_refl
    (need : ValidatorRecoveryParentNeed BlockId CommitId config) :
    ValidatorRecoveryParentNeedAccumulatorExtends need need := by
  exact ⟨rfl, rfl, rfl, rfl, fun _ member => member⟩

/-- Two accumulator extensions compose. -/
theorem accumulator_extends_trans
    {first middle last :
      ValidatorRecoveryParentNeed BlockId CommitId config}
    (firstToMiddle :
      ValidatorRecoveryParentNeedAccumulatorExtends first middle)
    (middleToLast :
      ValidatorRecoveryParentNeedAccumulatorExtends middle last) :
    ValidatorRecoveryParentNeedAccumulatorExtends first last := by
  rcases firstToMiddle with
    ⟨middleOrigin, middleBaseline, middleFloor, middleTarget,
      middleContains⟩
  rcases middleToLast with
    ⟨lastOrigin, lastBaseline, lastFloor, lastTarget, lastContains⟩
  exact ⟨lastOrigin.trans middleOrigin, lastBaseline.trans middleBaseline,
    lastFloor.trans middleFloor, lastTarget.trans middleTarget,
    fun reference member =>
      lastContains reference (middleContains reference member)⟩

/-- Every active normal need comes from one earlier fresh normal accumulator.
Later local batches can only add candidates to this fixed work item. -/
theorem active_normal_need_has_fresh_accumulator_source
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    (active : (needs.trace time validator).active = some need)
    (normalOrigin : need.proposalOrigin = .normal) :
    ∃ sourceTime sourceNeed,
      sourceTime ≤ time ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed sourceTime validator
          sourceNeed ∧
        ValidatorRecoveryParentNeedAccumulatorExtends sourceNeed need := by
  induction time generalizing need with
  | zero =>
      rw [needs.initialStateEmpty validator] at active
      contradiction
  | succ time inductionHypothesis =>
      have activeAfter :
          (needs.trace (time + 1) validator).active = some need := by
        simpa [Nat.succ_eq_add_one] using active
      have classify : ∀ (epochActiveAfter : Bool)
          (after : ValidatorRecoveryParentNeedState BlockId CommitId config),
          ValidatorRecoveryParentNeedTransition
            (needs.trace time validator) (needs.event time validator)
            ((timed.execution.trace (time + 1)).validatorState validator)
            epochActiveAfter after →
          after.active = some need →
          (ValidatorFreshNormalAccumulatorNeedSourceAt timed time validator
                need ∨
              ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
                validator need) ∨
            ∃ beforeNeed,
              (needs.trace time validator).active = some beforeNeed ∧
                beforeNeed.proposalOrigin = .normal ∧
                ValidatorRecoveryParentNeedAccumulatorExtends beforeNeed
                  need := by
        intro epochActiveAfter after step afterActive
        cases step with
        | idle beforeIdle startNone extendNone rebaseNone timerNotLatched
            afterIdle =>
            simp_all
        | start beforeIdle started extendNone rebaseNone timerNotLatched
            afterStarted =>
            rename_i startedNeed
            have startedIsNeed : startedNeed = need := by
              rw [afterActive] at afterStarted
              exact (Option.some.inj afterStarted).symm
            subst startedNeed
            exact Or.inl (needs.startedNormalNeedHasFreshSource time validator need
              started normalOrigin)
        | keep beforeActive startNone extendNone rebaseNone timerNotLatched
            belowTarget afterKept =>
            rename_i beforeNeed
            have beforeIsNeed : beforeNeed = need := by
              rw [afterActive] at afterKept
              exact (Option.some.inj afterKept).symm
            subst beforeNeed
            exact Or.inr ⟨need, beforeActive, normalOrigin,
              accumulator_extends_refl _⟩
        | extend beforeActive startNone extendedEvent rebaseNone
            timerNotLatched extension belowTarget afterExtended =>
            rename_i beforeNeed extendedNeed
            have extendedIsNeed : extendedNeed = need := by
              rw [afterActive] at afterExtended
              exact (Option.some.inj afterExtended).symm
            subst extendedNeed
            have beforeNormal : beforeNeed.proposalOrigin = .normal := by
              exact extension.1.symm.trans normalOrigin
            exact Or.inr ⟨beforeNeed, beforeActive, beforeNormal, extension⟩
        | rebase beforeActive startNone extendNone rebasedEvent timerNotLatched
            isRebase belowTarget afterRebased =>
            rename_i beforeNeed rebasedNeed
            have rebasedIsNeed : rebasedNeed = need := by
              rw [afterActive] at afterRebased
              exact (Option.some.inj afterRebased).symm
            subst rebasedNeed
            rcases needs.rebasedNeedHasCommitSource time validator need
                rebasedEvent with
              ⟨_oldNeed, _head, _oldActive, _oldRecovery, _installed,
                _exactRebase, fresh⟩
            exact Or.inl (Or.inr fresh)
        | timerArmed beforeActive startNone extendNone rebaseNone timerLatched
            afterKept =>
            rename_i beforeNeed
            have beforeIsNeed : beforeNeed = need := by
              rw [afterActive] at afterKept
              exact (Option.some.inj afterKept).symm
            subst beforeNeed
            exact Or.inr ⟨need, beforeActive, normalOrigin,
              accumulator_extends_refl _⟩
        | timerArmedAndRebase beforeActive startNone extendNone rebasedEvent
            timerLatched isRebase belowTarget afterRebased =>
            rename_i beforeNeed rebasedNeed
            have rebasedIsNeed : rebasedNeed = need := by
              rw [afterActive] at afterRebased
              exact (Option.some.inj afterRebased).symm
            subst rebasedNeed
            rcases needs.rebasedNeedHasCommitSource time validator need
                rebasedEvent with
              ⟨_oldNeed, _head, _oldActive, _oldRecovery, _installed,
                _exactRebase, fresh⟩
            exact Or.inl (Or.inr fresh)
        | targetReached beforeActive startNone extendNone rebaseNone
            timerNotLatched targetReached afterIdle =>
            simp_all
        | epochEnded startNone extendNone rebaseNone timerNotLatched
            afterIdle =>
            simp_all
      rcases classify _ _ (needs.transitionsFollowRules time validator)
          activeAfter with fresh | predecessor
      · rcases fresh with freshBefore | freshAfter
        · exact ⟨time, need, Nat.le_succ time, freshBefore,
            accumulator_extends_refl _⟩
        · exact ⟨time + 1, need, by simp, freshAfter,
            accumulator_extends_refl _⟩
      · rcases predecessor with
          ⟨beforeNeed, beforeActive, beforeNormal, beforeExtends⟩
        rcases inductionHypothesis beforeActive beforeNormal with
          ⟨sourceTime, sourceNeed, sourceBefore, fresh, sourceExtends⟩
        exact ⟨sourceTime, sourceNeed,
          Nat.le_trans sourceBefore (Nat.le_succ time), fresh,
          accumulator_extends_trans sourceExtends beforeExtends⟩

/-- A local action in the tail of a batch also occurs in the complete batch. -/
private theorem parent_need_action_occurs_in_cons
    {BlockId CommitId PacketId : Type}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {validator : Nat} {action : ValidatorLocalAction BlockId CommitId}
    (occurs : ValidatorLocalActionOccurs events validator action) :
    ValidatorLocalActionOccurs (event :: events) validator action := by
  rcases occurs with ⟨before, after, rfl⟩
  exact ⟨event :: before, after, by simp⟩

/-- One atomic step without commit installation or proposal persistence keeps
the local commit head and signer floor. -/
private theorem parent_need_atomic_step_keeps_baseline
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {event : ValidatorAtomicEvent BlockId CommitId PacketId}
    {validator : Nat}
    (step : ValidatorAtomicStep config faults protocolPacket program time before
      event after)
    (noRecord : ∀ head,
      event ≠ .localAction validator (.recordCommit head))
    (noApply : ∀ head,
      event ≠ .localAction validator (.applySyncedCommit head))
    (noPersist : ∀ block,
      event ≠ .localAction validator (.persistProposal block)) :
    (before.validatorState validator).commitHead =
        (after.validatorState validator).commitHead ∧
      (before.validatorState validator).highestSignedRound =
        (after.validatorState validator).highestSignedRound := by
  cases step with
  | localAction actionValidatorInRange actionValidatorCorrect enabled effect
      structural otherUnchanged epochUnchanged historyMonotone =>
      rename_i actionValidator action
      by_cases sameValidator : actionValidator = validator
      · subst actionValidator
        rcases structural with
          ⟨_clock, _durable, ownEffect, _sent, _accepted, installEffect⟩
        cases action <;>
          simp_all [OwnBlockActionEffect, CommitInstallActionEffect]
      · have unchanged := otherUnchanged validator (Ne.symm sameValidator)
        exact ⟨congrArg ValidatorLocalState.commitHead unchanged.symm,
          congrArg ValidatorLocalState.highestSignedRound unchanged.symm⟩
  | deliverPacket packetPresent packetProtocol deliveredAt senderInRange
      receiverInRange deliveryEnabled deliveryEffect structural otherUnchanged
      epochUnchanged catalogUnchanged packetsUnchanged =>
      rename_i packetId packet
      by_cases sameReceiver : packet.receiver = validator
      · subst validator
        exact ⟨structural.2.2.1.symm, structural.2.2.2.2.2.2.1.symm⟩
      · have unchanged := otherUnchanged validator (Ne.symm sameReceiver)
        exact ⟨congrArg ValidatorLocalState.commitHead unchanged.symm,
          congrArg ValidatorLocalState.highestSignedRound unchanged.symm⟩
  | clockTick clocksMonotone stateUpdate =>
      subst after
      simp [ValidatorWorldState.updateClocks]

/-- One event batch without commit installation or proposal persistence keeps
the local commit head and signer floor. -/
private theorem parent_need_world_step_keeps_baseline
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    {validator : Nat}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    (noRecord : ∀ head,
      ¬ValidatorLocalActionOccurs events validator (.recordCommit head))
    (noApply : ∀ head,
      ¬ValidatorLocalActionOccurs events validator (.applySyncedCommit head))
    (noPersist : ∀ block,
      ¬ValidatorLocalActionOccurs events validator (.persistProposal block)) :
    (before.validatorState validator).commitHead =
        (after.validatorState validator).commitHead ∧
      (before.validatorState validator).highestSignedRound =
        (after.validatorState validator).highestSignedRound := by
  induction step with
  | nil => exact ⟨rfl, rfl⟩
  | cons firstStep remainingSteps inductionHypothesis =>
      rename_i firstBefore firstMiddle firstAfter event tail
      have noRecordFirst : ∀ head,
          event ≠ .localAction validator (.recordCommit head) := by
        intro head eventExact
        apply noRecord head
        subst event
        exact ⟨[], tail, rfl⟩
      have noApplyFirst : ∀ head,
          event ≠ .localAction validator (.applySyncedCommit head) := by
        intro head eventExact
        apply noApply head
        subst event
        exact ⟨[], tail, rfl⟩
      have noPersistFirst : ∀ block,
          event ≠ .localAction validator (.persistProposal block) := by
        intro block eventExact
        apply noPersist block
        subst event
        exact ⟨[], tail, rfl⟩
      have firstKeeps := parent_need_atomic_step_keeps_baseline firstStep
        noRecordFirst noApplyFirst noPersistFirst
      have noRecordTail : ∀ head,
          ¬ValidatorLocalActionOccurs tail validator (.recordCommit head) := by
        intro head occurs
        exact noRecord head (parent_need_action_occurs_in_cons occurs)
      have noApplyTail : ∀ head,
          ¬ValidatorLocalActionOccurs tail validator
            (.applySyncedCommit head) := by
        intro head occurs
        exact noApply head (parent_need_action_occurs_in_cons occurs)
      have noPersistTail : ∀ block,
          ¬ValidatorLocalActionOccurs tail validator
            (.persistProposal block) := by
        intro block occurs
        exact noPersist block (parent_need_action_occurs_in_cons occurs)
      have tailKeeps := inductionHypothesis noRecordTail noApplyTail
        noPersistTail
      exact ⟨firstKeeps.1.trans tailKeeps.1,
        firstKeeps.2.trans tailKeeps.2⟩

/-- The finite accepted-representative map gives one exact-next recovery
accumulator. The accumulator can grow when the host accepts more branches. -/
theorem accepted_representatives_give_local_accumulator_need_source
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (floorAboveGc : ((timed.execution.trace time).validatorState
        validator).gcRound <
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound) :
    ∃ need,
      ValidatorLocalAccumulatorNeedSourceAt timed time validator need ∧
        need.proposalOrigin = .commitProgressRecovery ∧
        need.targetRound =
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound + 1 ∧
        ((timed.execution.trace time).validatorState validator).gcRound + 1 <
          need.targetRound := by
  let state := (timed.execution.trace time).validatorState validator
  have floorAboveGcState : state.gcRound < state.highestSignedRound := by
    simpa [state] using floorAboveGc
  let candidateRefs :=
    (List.range config.authorityCount).filterMap
      (fun author =>
        state.acceptedRepresentative state.highestSignedRound author)
  have candidateRefsNodup : candidateRefs.Nodup := by
    rw [List.nodup_iff_pairwise_ne]
    apply List.Pairwise.filterMap
        (fun author =>
          state.acceptedRepresentative state.highestSignedRound author)
        ?_ (List.nodup_iff_pairwise_ne.mp List.nodup_range)
    intro leftAuthor rightAuthor authorsDiffer leftReference leftSelected
      rightReference rightSelected referencesEqual
    have leftSound := representatives.representativeIsSound time validator
      state.highestSignedRound leftAuthor leftReference validatorInRange
      validatorCorrectAvailable leftSelected
    have rightSound := representatives.representativeIsSound time validator
      state.highestSignedRound rightAuthor rightReference validatorInRange
      validatorCorrectAvailable rightSelected
    apply authorsDiffer
    calc
      leftAuthor = leftReference.author := leftSound.1.symm
      _ = rightReference.author := by rw [referencesEqual]
      _ = rightAuthor := rightSound.1
  have candidateSound : ∀ reference, reference ∈ candidateRefs →
      state.acceptedRepresentative reference.round reference.author =
        some reference := by
    intro reference member
    rcases List.mem_filterMap.mp member with
      ⟨author, authorMember, selected⟩
    have authorInRange := List.mem_range.mp authorMember
    have sound := representatives.representativeIsSound time validator
      state.highestSignedRound author reference validatorInRange
      validatorCorrectAvailable selected
    simpa [sound.1, sound.2.1] using selected
  have candidateComplete : ∀ author reference,
      author < config.authorityCount →
      state.acceptedRepresentative state.highestSignedRound author =
        some reference →
      reference ∈ candidateRefs := by
    intro author reference authorInRange selected
    exact List.mem_filterMap.mpr
      ⟨author, List.mem_range.mpr authorInRange, selected⟩
  let need : ValidatorRecoveryParentNeed BlockId CommitId config :=
    {
      proposalOrigin := .commitProgressRecovery
      discoveryOrigin := .localAccumulator
      capsuleKey := none
      baselineCommit := state.commitHead
      signerFloor := state.highestSignedRound
      targetRound := state.highestSignedRound + 1
      targetAboveSignerFloor := Nat.lt_succ_self _
      recoveryTargetIsExactNext := by intro; rfl
      sourceBlock := none
      candidateRefs := candidateRefs
      candidateRefsNodup := candidateRefsNodup
      deliveredChildSourceShape := by simp
      deliveredChildHasNoCapsuleKey := by simp
      pinnedTipHasNoChild := by simp
      pinnedTipHasCapsuleKey := by simp
      localAccumulatorHasNoChild := by simp
      localAccumulatorHasNoCapsuleKey := by simp
      candidatesAreImmediate := by
        intro reference member
        rcases List.mem_filterMap.mp member with
          ⟨author, authorMember, selected⟩
        have sound := representatives.representativeIsSound time validator
          state.highestSignedRound author reference validatorInRange
          validatorCorrectAvailable selected
        change reference.round + 1 = state.highestSignedRound + 1
        omega
    }
  refine ⟨need, ?_, rfl, rfl, ?_⟩
  refine ⟨rfl, rfl, rfl, rfl, ?_, ?_⟩
  · intro reference member
    exact candidateSound reference member
  · intro author reference authorInRange selected
    have targetMinusOne : need.targetRound - 1 = state.highestSignedRound := by
      simp [need]
    rw [targetMinusOne] at selected
    exact candidateComplete author reference authorInRange selected
  · change state.gcRound + 1 < state.highestSignedRound + 1
    omega

/-- A retained current representative is discovered for the active recovery
target in the same local batch. -/
theorem retained_representative_is_discovered
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator author : Time} {before}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some before)
    (recoveryOrigin : before.proposalOrigin = .commitProgressRecovery)
    (retained : ((timed.execution.trace time).validatorState
      validator).retained reference = true)
    (representative : ((timed.execution.trace time).validatorState
      validator).acceptedRepresentative (before.targetRound - 1) author =
        some reference) :
    needs.discoveredCandidate time validator reference = true := by
  rcases needs.retainedRepresentativeHasAccumulatorSource time validator before
      author reference validatorInRange validatorCorrectAvailable active
      recoveryOrigin retained representative with
    ⟨source, sourceAtTime, sameFloor, sameTarget, member⟩
  exact needs.compatibleSourceCandidateIsDiscovered time validator before source
    reference active (Or.inr (Or.inr sourceAtTime)) sameFloor sameTarget member

/-- Canonical genesis readiness gives the timer-arm input for round one. -/
theorem canonical_genesis_ready_gives_timer_arm_input
    (ready : ValidatorCanonicalGenesisParentReadyAt timed time validator)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true) :
    ValidatorRecoveryTimerArmInputAt timed time validator := by
  rcases ready with
    ⟨signerFloorIsZero, timerIsEmpty, epochActive, parents, parentsReady⟩
  refine ⟨epochActive, validatorInRange, validatorCorrectAvailable,
    timerIsEmpty, ?_⟩
  refine ⟨parents, ?_⟩
  simpa [signerFloorIsZero] using parentsReady

/-- Canonical genesis readiness atomically latches protected round-one timer
work. It does not assume an existing round-one block. -/
theorem canonical_genesis_ready_latches_timer_arm
    (ready : ValidatorCanonicalGenesisParentReadyAt timed time validator)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (armEmpty : (arms.trace time validator).pending = none) :
    ∃ goal,
      arms.selectedGoal time validator = some goal ∧
        goal.targetRound = 1 ∧
        arms.events time validator = .latch goal ∧
        (arms.trace (time + 1) validator).pending = some goal ∧
        arms.protectedArmWork (time + 1) goal := by
  have input := canonical_genesis_ready_gives_timer_arm_input ready
    validatorInRange validatorCorrectAvailable
  obtain ⟨signerFloorIsZero, _, _, _, _⟩ := ready
  rcases arms.readyStateSelectsGoal time validator input armEmpty with
    ⟨goal, selected, goalValidator, _head, target, _readyAt, _eligible⟩
  have selectedAtGoal : arms.selectedGoal time goal.validator = some goal := by
    simpa [goalValidator] using selected
  have latchedAtGoal := arms.selectedGoalLatches time goal selectedAtGoal
  have latched : arms.events time validator = .latch goal := by
    simpa [goalValidator] using latchedAtGoal
  have protectedAtGoal := arms.latchedGoalIsProtected time goal latchedAtGoal
  have protectedWork : arms.protectedArmWork (time + 1) goal :=
    protectedAtGoal
  have armStep := arms.transitionsFollowRules time validator
  rw [latched] at armStep
  have pending : (arms.trace (time + 1) validator).pending = some goal := by
    cases armStep
    assumption
  have targetOne : goal.targetRound = 1 := by
    rw [target, signerFloorIsZero]
  exact ⟨goal, selected, targetOne, latched, pending, protectedWork⟩

/-- Canonical genesis parents derive a paced round-one proposal or a local
commit advance. The caller does not supply an empty timer worker. -/
theorem canonical_genesis_ready_derives_round_one_ready_or_commit_advance
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket network
      program timed waits)
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (ready : ValidatorCanonicalGenesisParentReadyAt timed time validator)
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (activeSuffix : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) :
    (∃ proposal : ValidatorOriginAwareRecoveryProposalReady faults timed waits
        ((timed.execution.trace time).validatorState validator).commitHead 1
        validator,
      time ≤ proposal.ready.startedAt ∧
        proposal.ready.startedAt ≤ time + timed.localActionBound + 2) ∨
      (∃ changedAt,
        time ≤ changedAt ∧
          ((timed.execution.trace time).validatorState
              validator).commitHead.index <
            ((timed.execution.trace changedAt).validatorState
              validator).commitHead.index) := by
  have input := canonical_genesis_ready_gives_timer_arm_input ready
    validatorInRange validatorCorrectAvailable
  obtain ⟨signerFloorIsZero, _, _, _, _⟩ := ready
  have result :=
    timerSource.recovery_state_and_active_suffix_derives_exact_ready_or_commit_advance
      arms time validator input activeSuffix
  rw [signerFloorIsZero] at result
  simpa using result

/-- A ready normal need gives one protected exact-next proposal-builder action. -/
theorem ready_normal_need_gives_protected_build
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (normalOrigin : need.proposalOrigin = .normal)
    (ready : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need) :
    ValidatorNormalParentBuildReadyAt timed time validator := by
  rcases ready with ⟨parents, withinCandidates, parentsReady⟩
  have mainFacts := needs.activeNeedMatchesMain time validator need active
  have protectedBuild := needs.readyNormalNeedProtectsBuild time validator need
    parents validatorInRange validatorCorrectAvailable active normalOrigin
      withinCandidates (by
      simpa only [normalOrigin] using parentsReady)
  refine ⟨need.targetRound, parents, ?_, ?_, protectedBuild⟩
  · simpa only [mainFacts.2.1] using need.targetAboveSignerFloor
  · simpa only [normalOrigin] using parentsReady

/-- A local commit starts fresh canonical normal work when no requester work is
active. -/
theorem commit_install_while_idle_starts_fresh_normal_need
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {head : ValidatorCommitHead CommitId}
    (idle : (needs.trace time validator).active = none)
    (installed : ValidatorCommitInstallOccurs (timed.execution.events time)
      validator head)
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true) :
    ∃ need,
      (needs.trace (time + 1) validator).active = some need ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
          validator need := by
  rcases needs.commitInstallStartsNormalNeedWhenIdle time validator head idle
      installed activeAfter with
    ⟨need, started, fresh⟩
  have step := needs.transitionsFollowRules time validator
  have activeNext : (needs.trace (time + 1) validator).active = some need := by
    have startFromStep : ∀ (epochActiveAfter : Bool)
        (after : ValidatorRecoveryParentNeedState BlockId CommitId config),
        ValidatorRecoveryParentNeedTransition
          (needs.trace time validator) (needs.event time validator)
          ((timed.execution.trace (time + 1)).validatorState validator)
          epochActiveAfter after →
        (needs.event time validator).startNeed = some need →
        after.active = some need := by
      intro epochActiveAfter after transition startEvent
      cases transition <;> simp_all
    exact startFromStep _ _ step started
  exact ⟨need, activeNext, fresh⟩

/-- An idle recovery host starts durable recovery parent work from its current
accepted representatives. If a commit wins the batch, the same host starts
fresh normal work from the installed state. -/
theorem accepted_representatives_start_recovery_need_or_commit_replacement
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (idle : (needs.trace time validator).active = none)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      (time + 1) validator)
    (noProposalPersistence : ∀ block,
      ¬ValidatorLocalActionOccurs (timed.execution.events time) validator
        (.persistProposal block))
    (floorAboveGc : ((timed.execution.trace time).validatorState
        validator).gcRound <
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound) :
    (∃ need,
      (needs.trace (time + 1) validator).active = some need ∧
        need.proposalOrigin = .commitProgressRecovery ∧
        ValidatorRecoveryParentNeedLocalSourceAt pins time validator need ∧
        ((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 1 < need.targetRound) ∨
      (∃ head need,
        ValidatorCommitInstallOccurs (timed.execution.events time) validator
            head ∧
          (needs.trace (time + 1) validator).active = some need ∧
          ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
            validator need) := by
  by_cases commitInstalled : ∃ head,
      ValidatorCommitInstallOccurs (timed.execution.events time) validator head
  · rcases commitInstalled with ⟨head, installed⟩
    rcases needs.commit_install_while_idle_starts_fresh_normal_need idle
        installed recoveryMode.1 with ⟨need, active, fresh⟩
    exact Or.inr ⟨head, need, installed, active, fresh⟩
  · have noCommitInstall : ∀ head,
        ¬ValidatorCommitInstallOccurs (timed.execution.events time)
          validator head := by
      intro head installed
      exact commitInstalled ⟨head, installed⟩
    have noRecord : ∀ head,
        ¬ValidatorLocalActionOccurs (timed.execution.events time) validator
          (.recordCommit head) := by
      intro head occurs
      exact noCommitInstall head (Or.inl occurs)
    have noApply : ∀ head,
        ¬ValidatorLocalActionOccurs (timed.execution.events time) validator
          (.applySyncedCommit head) := by
      intro head occurs
      exact noCommitInstall head (Or.inr occurs)
    have stable := parent_need_world_step_keeps_baseline
      (timed.execution.stepsFollowRules time) noRecord noApply
        noProposalPersistence
    have gcStable :
        ((timed.execution.trace (time + 1)).validatorState validator).gcRound =
          ((timed.execution.trace time).validatorState validator).gcRound := by
      calc
        ((timed.execution.trace (time + 1)).validatorState
            validator).gcRound =
            timed.execution.gcRoundForCommitHead
              ((timed.execution.trace (time + 1)).validatorState
                validator).commitHead :=
          timed.execution.correctGcRoundMatchesCommitHead (time + 1) validator
            validatorInRange validatorCorrectAvailable
        _ = timed.execution.gcRoundForCommitHead
              ((timed.execution.trace time).validatorState
                validator).commitHead := by rw [← stable.1]
        _ = ((timed.execution.trace time).validatorState
              validator).gcRound :=
          (timed.execution.correctGcRoundMatchesCommitHead time validator
            validatorInRange validatorCorrectAvailable).symm
    rcases accepted_representatives_give_local_accumulator_need_source
        representatives validatorInRange validatorCorrectAvailable floorAboveGc
      with ⟨source, sourceLocal, sourceOrigin, _sourceTarget, sourceFence⟩
    have sourceFenceAfter :
        ((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 1 < source.targetRound := by
      rw [gcStable]
      exact sourceFence
    have sourceExists : ∃ need,
        ValidatorRecoveryParentNeedLocalSourceAt pins time validator need ∧
          need.proposalOrigin = .commitProgressRecovery ∧
          ((need.targetRound = 1 ∧
              ((timed.execution.trace (time + 1)).validatorState
                validator).gcRound = 0) ∨
            ((timed.execution.trace (time + 1)).validatorState
                validator).gcRound + 1 < need.targetRound) :=
      ⟨source, Or.inr (Or.inr sourceLocal), sourceOrigin,
        Or.inr sourceFenceAfter⟩
    rcases needs.availableRecoverySourceStartsWhenIdle time validator
        validatorInRange validatorCorrectAvailable noCommitInstall idle
        recoveryMode sourceExists with
      ⟨need, started, needSource, needOrigin, needFence⟩
    have activeNext :
        (needs.trace (time + 1) validator).active = some need := by
      have step := needs.transitionsFollowRules time validator
      have startFromStep : ∀ (epochActiveAfter : Bool)
          (after : ValidatorRecoveryParentNeedState BlockId CommitId config),
          ValidatorRecoveryParentNeedTransition
            (needs.trace time validator) (needs.event time validator)
            ((timed.execution.trace (time + 1)).validatorState validator)
            epochActiveAfter after →
          (needs.event time validator).startNeed = some need →
          after.active = some need := by
        intro epochActiveAfter after transition startEvent
        cases transition <;> simp_all
      exact startFromStep _ _ step started
    have aboveGc :
        ((timed.execution.trace (time + 1)).validatorState
            validator).gcRound + 1 < need.targetRound := by
      rcases needFence with ⟨targetOne, gcZero⟩ | targetAboveGc
      · have positiveFloor : 0 <
            ((timed.execution.trace time).validatorState
              validator).highestSignedRound := by
          omega
        have needFloor := (needs.activeNeedMatchesMain (time + 1) validator
          need activeNext).2.1
        have recoveryExact := need.recoveryTargetIsExactNext needOrigin
        omega
      · exact targetAboveGc
    exact Or.inl ⟨need, activeNext, needOrigin, needSource, aboveGc⟩

/-- An idle recovery host starts fresh normal work when GC has moved and its
signer floor is at or below the committed root. This includes signer floor
zero after verified commit sync. -/
theorem recovery_root_while_idle_starts_fresh_normal_need
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (noCommitInstall : ∀ head,
      ¬ValidatorCommitInstallOccurs (timed.execution.events time) validator head)
    (idle : (needs.trace time validator).active = none)
    (recoveryMode : ValidatorCommitProgressRecoveryModeAt timed recoveryWait
      (time + 1) validator)
    (positiveGc : 0 < ((timed.execution.trace (time + 1)).validatorState
      validator).gcRound)
    (floorIsCommittedRoot :
      ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound ≤
        ((timed.execution.trace (time + 1)).validatorState validator).gcRound)
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true) :
    ∃ need,
      (needs.trace (time + 1) validator).active = some need ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
          validator need := by
  rcases needs.recoveryRootStartsNormalNeedWhenIdle time validator
      validatorInRange validatorCorrectAvailable noCommitInstall idle recoveryMode
      positiveGc floorIsCommittedRoot activeAfter with
    ⟨need, started, fresh⟩
  have step := needs.transitionsFollowRules time validator
  have activeNext : (needs.trace (time + 1) validator).active = some need := by
    have startFromStep : ∀ (epochActiveAfter : Bool)
        (after : ValidatorRecoveryParentNeedState BlockId CommitId config),
        ValidatorRecoveryParentNeedTransition
          (needs.trace time validator) (needs.event time validator)
          ((timed.execution.trace (time + 1)).validatorState validator)
          epochActiveAfter after →
        (needs.event time validator).startNeed = some need →
        after.active = some need := by
      intro epochActiveAfter after transition startEvent
      cases transition <;> simp_all
    exact startFromStep _ _ step started
  exact ⟨need, activeNext, fresh⟩

/-- A local commit converts active recovery work into fresh canonical normal
work in the same batch. This also covers a timer-arm race. -/
theorem commit_advance_rebases_recovery_need
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    (active : (needs.trace time validator).active = some need)
    (recoveryOrigin : need.proposalOrigin = .commitProgressRecovery)
    (commitAdvanced : need.baselineCommit.index <
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead.index)
    (targetNotReached :
      ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound < need.targetRound) :
    ∃ rebased,
      (needs.trace (time + 1) validator).active = some rebased ∧
        ValidatorRecoveryParentNeedIsRebase need rebased
          ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
          validator rebased := by
  rcases needs.commitAdvanceRebasesNeed time validator need active recoveryOrigin
      commitAdvanced targetNotReached with
    ⟨rebased, rebaseEvent, isRebase, fresh⟩
  have step := needs.transitionsFollowRules time validator
  have activeNext : (needs.trace (time + 1) validator).active = some rebased := by
    have rebaseFromStep : ∀ (epochActiveAfter : Bool)
        (after : ValidatorRecoveryParentNeedState BlockId CommitId config),
        ValidatorRecoveryParentNeedTransition
          (needs.trace time validator) (needs.event time validator)
          ((timed.execution.trace (time + 1)).validatorState validator)
          epochActiveAfter after →
        (needs.event time validator).rebaseNeed = some rebased →
        after.active = some rebased := by
      intro epochActiveAfter after transition rebaseRecorded
      cases transition <;> simp_all
    exact rebaseFromStep _ _ step rebaseEvent
  exact ⟨rebased, activeNext, isRebase, fresh⟩

/-- If GC reaches a positive recovery parent round, the same local commit
batch replaces recovery work with fresh normal work above GC. -/
theorem gc_crossing_rebases_recovery_need
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (recoveryOrigin : need.proposalOrigin = .commitProgressRecovery)
    (positiveFloor : 0 < need.signerFloor)
    (floorIsCommittedRoot : need.signerFloor ≤
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound)
    (targetNotReached :
      ((timed.execution.trace (time + 1)).validatorState
        validator).highestSignedRound < need.targetRound) :
    ∃ rebased,
      (needs.trace (time + 1) validator).active = some rebased ∧
        ValidatorRecoveryParentNeedIsRebase need rebased
          ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead ∧
        ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
          validator rebased := by
  have mainFacts := needs.activeNeedMatchesMain time validator need active
  have recoveryFacts := mainFacts.2.2.2 recoveryOrigin
  have targetExact := need.recoveryTargetIsExactNext recoveryOrigin
  have fence := needs.activeNeedFencesTargetRound time validator need active
  have oldGcBelowFloor :
      ((timed.execution.trace time).validatorState validator).gcRound <
        need.signerFloor := by
    rcases fence with ⟨targetOne, _gcZero⟩ | targetAboveGc
    · omega
    · omega
  have gcAdvanced :
      ((timed.execution.trace time).validatorState validator).gcRound <
        ((timed.execution.trace (time + 1)).validatorState validator).gcRound :=
    Nat.lt_of_lt_of_le oldGcBelowFloor floorIsCommittedRoot
  have headAdvanced := gc_round_advance_implies_commit_index_advance
    timed.execution validatorInRange validatorCorrectAvailable gcAdvanced
  have baselineAdvanced : need.baselineCommit.index <
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead.index := by
    rw [recoveryFacts.1]
    exact headAdvanced
  exact needs.commit_advance_rebases_recovery_need active recoveryOrigin
    baselineAdvanced targetNotReached

/-- An active ready need supplies the fundamental timer-arm input. -/
theorem ready_need_gives_timer_arm_input
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (recoveryOrigin : need.proposalOrigin = .commitProgressRecovery)
    (ready : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need) :
    ValidatorRecoveryTimerArmInputAt timed time validator := by
  have mainFacts := needs.activeNeedMatchesMain time validator need active
  rcases ready with ⟨parents, _withinCandidates, parentsReady⟩
  have recoveryState := mainFacts.2.2.2 recoveryOrigin
  refine ⟨mainFacts.2.2.1, validatorInRange, validatorCorrectAvailable,
    recoveryState.2.1, ?_⟩
  refine ⟨parents, ?_⟩
  have target : need.targetRound =
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1 := by
    rw [need.recoveryTargetIsExactNext recoveryOrigin, mainFacts.2.1]
  simpa only [recoveryOrigin, target] using parentsReady

/-- A ready need atomically latches protected timer-arm work and keeps its
parent pool active. It does not call `proposeNext`. -/
theorem ready_parent_need_latches_timer_arm
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (needs.trace time validator).active = some need)
    (recoveryOrigin : need.proposalOrigin = .commitProgressRecovery)
    (ready : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need)
    (armEmpty : (arms.trace time validator).pending = none) :
    ∃ goal,
      arms.selectedGoal time validator = some goal ∧
        arms.events time validator = .latch goal ∧
        (arms.trace (time + 1) validator).pending = some goal ∧
        arms.protectedArmWork (time + 1) goal ∧
        ∃ nextNeed,
          (needs.trace (time + 1) validator).active = some nextNeed ∧
            (nextNeed = need ∨
              ValidatorRecoveryParentNeedIsRebase need nextNeed
                ((timed.execution.trace (time + 1)).validatorState
                  validator).commitHead) := by
  rcases needs.readyNeedSelectsTimerGoal time validator need validatorInRange
      validatorCorrectAvailable active recoveryOrigin ready armEmpty with
    ⟨goal, selected, goalValidator, target⟩
  have selectedAtGoal : arms.selectedGoal time goal.validator = some goal := by
    simpa [goalValidator] using selected
  have latchedAtGoal := arms.selectedGoalLatches time goal selectedAtGoal
  have latched : arms.events time validator = .latch goal := by
    simpa [goalValidator] using latchedAtGoal
  have protectedWork := arms.latchedGoalIsProtected time goal latchedAtGoal
  have timerRecorded := (needs.timerArmLatchIff time validator).2
    ⟨need, goal, active, selected, latched, recoveryOrigin, target⟩
  have armStep := arms.transitionsFollowRules time validator
  rw [latched] at armStep
  have armPending : (arms.trace (time + 1) validator).pending = some goal := by
    cases armStep
    assumption
  have needStep := needs.transitionsFollowRules time validator
  have needContinues : ∃ nextNeed,
      (needs.trace (time + 1) validator).active = some nextNeed ∧
        (nextNeed = need ∨
          ValidatorRecoveryParentNeedIsRebase need nextNeed
            ((timed.execution.trace (time + 1)).validatorState
              validator).commitHead) := by
    have continueFromStep : ∀ (epochActiveAfter : Bool)
        (after : ValidatorRecoveryParentNeedState BlockId CommitId config),
        ValidatorRecoveryParentNeedTransition
          (needs.trace time validator) (needs.event time validator)
          ((timed.execution.trace (time + 1)).validatorState validator)
          epochActiveAfter after →
        (needs.event time validator).timerArmLatched = true →
        ∃ nextNeed,
          after.active = some nextNeed ∧
            (nextNeed = need ∨
              ValidatorRecoveryParentNeedIsRebase need nextNeed
                ((timed.execution.trace (time + 1)).validatorState
                  validator).commitHead) := by
      intro epochActiveAfter after step timerLatched
      cases step <;> simp_all
    exact continueFromStep _ _ needStep timerRecorded
  exact ⟨goal, selected, latched, armPending, protectedWork, needContinues⟩

/-- An unresolved missing candidate is exactly a live requester sync goal. -/
theorem missing_candidate_is_live_sync_goal
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    {reference : ValidatorBlockRef BlockId}
    (active : (needs.trace time validator).active = some need)
    (candidate : reference ∈ need.candidateRefs)
    (notReady : ¬ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need)
    (aboveGc : ((timed.execution.trace time).validatorState
      validator).gcRound < reference.round)
    (missing : ((timed.execution.trace time).validatorState
      validator).accepted reference = false) :
    ValidatorRecoveryParentSyncGoalAt (needs.trace time validator)
        ((timed.execution.trace time).validatorState validator) reference ∧
      ¬syncRules.goalObsolete validator reference time := by
  have goal : ValidatorRecoveryParentSyncGoalAt
      (needs.trace time validator)
      ((timed.execution.trace time).validatorState validator) reference :=
    ⟨need, active, candidate, notReady, aboveGc, missing⟩
  exact ⟨goal, needs.syncGoalIsNotObsolete time validator reference goal⟩

/-- Once a local quorum list is ready, an unresolved extra candidate is not a
sync goal. -/
theorem ready_need_ignores_extra_candidates
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    (active : (needs.trace time validator).active = some need)
    (ready : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need)
    (reference : ValidatorBlockRef BlockId) :
    ¬ValidatorRecoveryParentSyncGoalAt (needs.trace time validator)
      ((timed.execution.trace time).validatorState validator) reference := by
  intro goal
  rcases goal with ⟨other, sameNeed, _, notReady, _⟩
  have same : other = need := by
    rw [active] at sameNeed
    cases sameNeed
    rfl
  subst other
  exact notReady ready

end ValidatorRecoveryParentNeedExecution

end Mysticeti
