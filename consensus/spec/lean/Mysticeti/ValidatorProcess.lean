/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.FlexCommitter
import Mysticeti.Liveness

namespace Mysticeti

/-! Validator-indexed state for the final liveness proofs.

This file contains only static configuration, local validator state, addressed
messages, local actions, and liveness goals. It does not assume that a recovery
quorum, a quorum block layer, a usable anchor, or a later commit exists.
-/

/-- One local commit head. The identifier can contain the Rust commit digest. -/
structure ValidatorCommitHead (CommitId : Type) where
  index : Nat
  id : CommitId
  round : Nat

/-- The only two actions that can install a commit at a correct validator. -/
inductive CommitInstallSource where
  | localExecution
  | verifiedCommitSync
  deriving DecidableEq, Repr

/-- Static epoch data shared by all correct validators.

The leader schedule can change with the committed prefix. The selected leader
order is the ordered list of selected leader slots in one round. -/
structure ValidatorEpochConfig (CommitId : Type) where
  authorityCount : Nat
  stake : Nat → Nat
  thresholds : Thresholds authorityCount stake
  leaderSchedule : CommitId → VoterSet
  selectedLeaderOrder : CommitId → Nat → List Nat
  selectedLeaderOrderNodup :
    ∀ commitId round, (selectedLeaderOrder commitId round).Nodup
  selectedLeaderInRange :
    ∀ commitId round validator,
      validator ∈ selectedLeaderOrder commitId round →
      validator < authorityCount
  selectedLeaderFromSchedule :
    ∀ commitId round validator,
      validator ∈ selectedLeaderOrder commitId round →
      leaderSchedule commitId validator = true
  scheduleValidatorInRange :
    ∀ commitId validator,
      leaderSchedule commitId validator = true →
      validator < authorityCount
  /-- Current v3 selects the complete leader schedule in each pending round. -/
  selectedLeaderCoversSchedule :
    ∀ commitId round validator,
      validator < authorityCount →
      leaderSchedule commitId validator = true →
      validator ∈ selectedLeaderOrder commitId round

/-- Byzantine and unavailable validators for one stable proof interval.

The unavailable set is fixed for the interval. A different interval can use a
different set. The quorum equation is a static threshold condition. -/
structure FixedFaultInterval
    {CommitId : Type} (config : ValidatorEpochConfig CommitId) where
  byzantine : VoterSet
  unavailable : VoterSet
  unavailableStakeBound : Nat
  byzantineStakeBounded :
    weight config.authorityCount config.stake byzantine ≤
      config.thresholds.fault
  unavailableStakeBounded :
    weight config.authorityCount config.stake unavailable ≤
      unavailableStakeBound
  faultBudgetsFit :
    config.thresholds.fault + unavailableStakeBound ≤
      totalWeight config.authorityCount config.stake
  quorumDefinition :
    config.thresholds.quorum =
      totalWeight config.authorityCount config.stake -
        (config.thresholds.fault + unavailableStakeBound)

namespace FixedFaultInterval

variable {CommitId : Type} {config : ValidatorEpochConfig CommitId}

/-- Validators that can prevent progress in one stable proof interval. -/
def nonProgress (faults : FixedFaultInterval config) : VoterSet :=
  VoterSet.union faults.byzantine faults.unavailable

/-- Correct, available validators in one stable proof interval. -/
def correctAvailable (faults : FixedFaultInterval config) : VoterSet :=
  VoterSet.diff VoterSet.full faults.nonProgress

/-- The fixed Byzantine and unavailable sets stay within their combined stake
budget. -/
theorem non_progress_stake_bounded
    (faults : FixedFaultInterval config) :
    weight config.authorityCount config.stake faults.nonProgress ≤
      config.thresholds.fault + faults.unavailableStakeBound := by
  exact Nat.le_trans
    (weight_union_le_add config.authorityCount config.stake
      faults.byzantine faults.unavailable)
    (Nat.add_le_add faults.byzantineStakeBounded
      faults.unavailableStakeBounded)

/-- The fixed correct, available set has quorum stake.

This result is derived from the two fault bounds and the configured quorum
equation. It is not an input to a liveness theorem. -/
theorem correct_available_stake_is_quorum
    (faults : FixedFaultInterval config) :
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake faults.correctAvailable := by
  have nonProgressBound :
      weight config.authorityCount config.stake faults.nonProgress ≤
        config.thresholds.fault + faults.unavailableStakeBound := by
    exact faults.non_progress_stake_bounded
  have nonProgressPlusQuorum :
      weight config.authorityCount config.stake faults.nonProgress +
          config.thresholds.quorum ≤
        totalWeight config.authorityCount config.stake := by
    have budgetsFit := faults.faultBudgetsFit
    rw [faults.quorumDefinition]
    omega
  have fullIntersection :
      VoterSet.inter VoterSet.full faults.nonProgress = faults.nonProgress := by
    funext validator
    simp [VoterSet.inter, VoterSet.full]
  have partition := weight_diff_add_inter config.authorityCount config.stake
    VoterSet.full faults.nonProgress
  rw [fullIntersection] at partition
  change config.thresholds.quorum ≤
    weight config.authorityCount config.stake
      (VoterSet.diff VoterSet.full faults.nonProgress)
  simp [totalWeight] at nonProgressPlusQuorum partition
  omega

end FixedFaultInterval

/-- A block reference contains the fields needed by parent checks. -/
structure ValidatorBlockRef (BlockId : Type) where
  id : BlockId
  author : Nat
  round : Nat

/-- One block and its explicit immediate-parent references. -/
structure ValidatorBlock (BlockId : Type) where
  reference : ValidatorBlockRef BlockId
  parents : List (ValidatorBlockRef BlockId)

/-- The authors in one list of block references. -/
def validatorParentAuthors {BlockId : Type}
    (parents : List (ValidatorBlockRef BlockId)) : VoterSet :=
  fun validator => parents.any (fun parent => parent.author == validator)

/-- An authenticated replay manifest names one exact next commit and the
finite decision-DAG blocks needed to reproduce it. -/
structure ValidatorReplayManifest (BlockId CommitId : Type) where
  prior : ValidatorCommitHead CommitId
  head : ValidatorCommitHead CommitId
  blockReferences : List (ValidatorBlockRef BlockId)

namespace ValidatorBlock

/-- The authors in one block's immediate-parent list. -/
def parentAuthors {BlockId : Type}
    (block : ValidatorBlock BlockId) : VoterSet :=
  fun validator => block.parents.any (fun parent => parent.author == validator)

/-- A parent list has at most one branch for each author. -/
def ParentAuthorsNodup {BlockId : Type}
    (block : ValidatorBlock BlockId) : Prop :=
  (block.parents.map ValidatorBlockRef.author).Nodup

/-- All listed parents are from the immediate preceding round. -/
def ParentsAreImmediate {BlockId : Type}
    (block : ValidatorBlock BlockId) : Prop :=
  ∀ parent, parent ∈ block.parents → parent.round + 1 = block.reference.round

/-- The local parent guard for one non-genesis proposal. -/
def HasQuorumImmediateParents
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (block : ValidatorBlock BlockId) : Prop :=
  block.ParentAuthorsNodup ∧
    block.ParentsAreImmediate ∧
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake block.parentAuthors

end ValidatorBlock

/-- Recovery state owned by one validator.

If `alignmentWitness` is absent, the proposal target is the round after the
highest durable signed round. A present witness is for the separate safe-resume
action. -/
structure ValidatorRecoveryState (BlockId CommitId : Type) where
  baselineCommit : ValidatorCommitHead CommitId
  targetRound : Nat
  parentsReadyAt : Option Time
  deadline : Option Time
  alignmentWitness : Option (ValidatorBlockRef BlockId)

/-- Local decision data for one pending leader round. -/
structure ValidatorPendingRound where
  round : Nat
  selectedLeaderSlotStatuses : List SelectedLeaderSlotStatus

/-- The local state needed by the executable FlexCommitter mapping. -/
structure ValidatorCommitterState where
  pendingRounds : List ValidatorPendingRound

/-- Consensus state owned by one validator. -/
structure ValidatorLocalState (BlockId CommitId : Type) where
  clock : Time
  commitHead : ValidatorCommitHead CommitId
  installedCommitAt : Nat → Option CommitId
  commitInstallSourceAt : Nat → Option CommitInstallSource
  lastCommitTime : Time
  /-- The next round that normal proposal processing would use from the local
  threshold clock. Recovery entry observes this value. It does not replace the
  separate exact recovery proposal target. -/
  thresholdClockRound : Nat
  highestSignedRound : Nat
  ownBlockAt : Nat → Option (ValidatorBlockRef BlockId)
  sentOwnBlockAt : Nat → Bool
  accepted : ValidatorBlockRef BlockId → Bool
  retained : ValidatorBlockRef BlockId → Bool
  requested : ValidatorBlockRef BlockId → Bool
  /-- One accepted representative for stake accounting. -/
  acceptedRepresentative :
    Nat → Nat → Option (ValidatorBlockRef BlockId)
  /-- One parent branch selected for a recovery proposal. -/
  recoveryParentChoice :
    Nat → Nat → Option (ValidatorBlockRef BlockId)
  gcRound : Nat
  recovery : Option (ValidatorRecoveryState BlockId CommitId)
  committer : ValidatorCommitterState
  observedPeerCommit : Nat → Option (ValidatorCommitHead CommitId)

/-- Network data sent between two explicit validators. -/
structure AddressedPacket (Payload : Type) where
  sender : Nat
  receiver : Nat
  payload : Payload
  sentAt : Time
  deliveredAt : Time

/-- Messages needed by block production and commit synchronization. -/
inductive ValidatorMessage (BlockId CommitId : Type) where
  | block (block : ValidatorBlock BlockId)
  | blockRequest (reference : ValidatorBlockRef BlockId)
  | replayManifest (manifest : ValidatorReplayManifest BlockId CommitId)
  | commitAdvertisement (head : ValidatorCommitHead CommitId)
  | commitRequest (index : Nat)
  | commitData (head : ValidatorCommitHead CommitId)

/-- Standard partial synchrony for addressed validator messages. -/
structure AddressedPartialSynchrony
    {CommitId Payload : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (protocolPacket : AddressedPacket Payload → Prop) where
  gst : Time
  delta : Nat
  deltaPositive : 0 < delta
  postGstDelivery : ∀ packet,
    protocolPacket packet →
    packet.sender < config.authorityCount →
    packet.receiver < config.authorityCount →
    faults.correctAvailable packet.sender = true →
    faults.correctAvailable packet.receiver = true →
    gst ≤ packet.sentAt →
    packet.sentAt ≤ packet.deliveredAt ∧
      packet.deliveredAt ≤ packet.sentAt + delta

/-- A protected local consensus action. The execution model supplies its guard,
state transition, and fair scheduling rule. -/
inductive ValidatorLocalAction (BlockId CommitId : Type) where
  | enterRecovery
  | requestBlock (peer : Nat) (reference : ValidatorBlockRef BlockId)
  | serveBlock (peer : Nat) (reference : ValidatorBlockRef BlockId)
  | acceptBlock (block : ValidatorBlock BlockId)
  | persistProposal (block : ValidatorBlock BlockId)
  | sendBlock (receiver : Nat) (reference : ValidatorBlockRef BlockId)
  | sendReplayManifest
      (receiver : Nat) (manifest : ValidatorReplayManifest BlockId CommitId)
  | proposeNormal
      (targetRound : Nat) (parents : List (ValidatorBlockRef BlockId))
  | proposeNext (parents : List (ValidatorBlockRef BlockId))
  | alignProposal
      (witness : ValidatorBlockRef BlockId)
      (parents : List (ValidatorBlockRef BlockId))
  | runCommitter
  | runReplayCommitter (manifest : ValidatorReplayManifest BlockId CommitId)
  | recordCommit (head : ValidatorCommitHead CommitId)
  | applySyncedCommit (head : ValidatorCommitHead CommitId)

/-- One timed local action at one validator. -/
structure ValidatorActionEvent (BlockId CommitId : Type) where
  validator : Nat
  action : ValidatorLocalAction BlockId CommitId
  enabledAt : Time
  completedAt : Time

/-- A proof world contains local states, created block data, and message history.
The catalog does not make a block locally available to every validator. -/
structure ValidatorWorldState
    (BlockId CommitId PacketId : Type) where
  epochActive : Bool
  validatorState : Nat → ValidatorLocalState BlockId CommitId
  blockCatalog : BlockId → Option (ValidatorBlock BlockId)
  packets : PacketId →
    Option (AddressedPacket (ValidatorMessage BlockId CommitId))

/-- Source-to-model invariants for one validator's local state. These facts state
what local fields mean. They do not state that any future action occurs. -/
structure ValidatorLocalStateWellFormed
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (validator : Nat) : Prop where
  commitHeadIsInstalled :
    (world.validatorState validator).installedCommitAt
        (world.validatorState validator).commitHead.index =
      some (world.validatorState validator).commitHead.id
  installedIndexIsNotFuture : ∀ index commitId,
    (world.validatorState validator).installedCommitAt index = some commitId →
    index ≤ (world.validatorState validator).commitHead.index
  installedCommitHasPermittedSource : ∀ index commitId,
    0 < index →
    (world.validatorState validator).installedCommitAt index = some commitId →
    (world.validatorState validator).commitInstallSourceAt index =
        some .localExecution ∨
      (world.validatorState validator).commitInstallSourceAt index =
        some .verifiedCommitSync
  ownBlockIsSound : ∀ round reference,
    (world.validatorState validator).ownBlockAt round = some reference →
    reference.author = validator ∧
      reference.round = round ∧
      (world.validatorState validator).accepted reference = true ∧
      (world.validatorState validator).retained reference = true ∧
      ∃ block,
        world.blockCatalog reference.id = some block ∧
          block.reference = reference
  ownBlockDoesNotExceedSignerFloor : ∀ round reference,
    (world.validatorState validator).ownBlockAt round = some reference →
    round ≤ (world.validatorState validator).highestSignedRound
  sentOwnBlockIsDurable : ∀ round,
    (world.validatorState validator).sentOwnBlockAt round = true →
    ((world.validatorState validator).ownBlockAt round).isSome = true
  acceptedRepresentativeIsSound : ∀ round author reference,
    (world.validatorState validator).acceptedRepresentative round author =
        some reference →
    reference.author = author ∧
      reference.round = round ∧
      (world.validatorState validator).accepted reference = true ∧
      ∃ block,
        world.blockCatalog reference.id = some block ∧
          block.reference = reference
  recoveryParentChoiceIsSound : ∀ round author reference,
    (world.validatorState validator).recoveryParentChoice round author =
        some reference →
    reference.author = author ∧
      reference.round = round ∧
      (world.validatorState validator).accepted reference = true

/-- Every in-range validator has a well-formed local model view. -/
def ValidatorWorldStateWellFormed
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (world : ValidatorWorldState BlockId CommitId PacketId) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    ValidatorLocalStateWellFormed world validator

namespace ValidatorWorldState

variable {BlockId CommitId PacketId : Type}

/-- The local commit index of one validator. -/
def localCommitIndex
  (world : ValidatorWorldState BlockId CommitId PacketId)
    (validator : Nat) : Nat :=
  (world.validatorState validator).commitHead.index

/-- Validators with a durable own block in one round. -/
def producedAuthors
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat) : VoterSet :=
  fun validator => (world.validatorState validator).ownBlockAt round |>.isSome

/-- Authors with one accepted local representative at one validator. -/
def acceptedAuthors
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (observer round : Nat) : VoterSet :=
  fun author =>
    (world.validatorState observer).acceptedRepresentative round author |>.isSome

/-- Authors with one selected immediate-parent branch at one validator. -/
def selectedParentAuthors
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (observer round : Nat) : VoterSet :=
  fun author =>
    (world.validatorState observer).recoveryParentChoice round author |>.isSome

/-- Correct, available validators that have entered local recovery. -/
def recoveryAuthorities
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId) : VoterSet :=
  fun validator =>
    faults.correctAvailable validator &&
      (world.validatorState validator).recovery.isSome

/-- The exact-next target is derived from durable local signer state. -/
def nextRecoveryRound
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (validator : Nat) : Nat :=
  (world.validatorState validator).highestSignedRound + 1

end ValidatorWorldState

/-- Correct, available validators produce quorum stake in one round. -/
def ProducedCorrectQuorumLayer
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat) : Prop :=
  config.thresholds.quorum ≤
    weight config.authorityCount config.stake
      (VoterSet.inter faults.correctAvailable
        (world.producedAuthors round))

/-- Every correct, available validator has accepted correct, available quorum
stake in one round. -/
def CommonAcceptedCorrectQuorumLayer
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat) : Prop :=
  ∀ observer,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (VoterSet.inter faults.correctAvailable
          (world.acceptedAuthors observer round))

/-- One shared exact correct-author quorum is usable at every correct host.

Each receiver is above its local GC boundary and keeps one accepted, retained,
and catalogued representative for every author in the same quorum set. Each
selected author is correct, available, and has produced a durable block in the
round. The set need not contain every correct, available author. This predicate
keeps the exact references needed by later parent and commit proofs without
requiring each correct validator to produce a block. -/
def CommonUsableCorrectQuorumLayer
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat) : Prop :=
  ∃ authors : VoterSet,
    config.thresholds.quorum ≤
        weight config.authorityCount config.stake authors ∧
      VoterSet.SubsetAt config.authorityCount authors
        (VoterSet.inter faults.correctAvailable
          (world.producedAuthors round)) ∧
      ∀ receiver,
        receiver < config.authorityCount →
        faults.correctAvailable receiver = true →
        (world.validatorState receiver).gcRound < round ∧
          ∀ author,
            author < config.authorityCount →
            authors author = true →
            ∃ reference block,
              (world.validatorState receiver).acceptedRepresentative round
                  author = some reference ∧
                (world.validatorState receiver).accepted reference = true ∧
                (world.validatorState receiver).retained reference = true ∧
                world.blockCatalog reference.id = some block ∧
                block.reference = reference ∧
                reference.author = author ∧
                reference.round = round

/-- An exact usable layer implies the Boolean common-acceptance summary. -/
theorem common_usable_correct_quorum_layer_is_common_accepted
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {round : Nat}
    (usable : CommonUsableCorrectQuorumLayer config faults world round) :
    CommonAcceptedCorrectQuorumLayer config faults world round := by
  rcases usable with
    ⟨authors, authorsAreQuorum, authorsProducedCorrect, usable⟩
  intro observer observerInRange observerCorrectAvailable
  have correctSubset : VoterSet.SubsetAt config.authorityCount
      authors
      (VoterSet.inter faults.correctAvailable
        (world.acceptedAuthors observer round)) := by
    intro author authorInRange authorSelected
    have authorProducedCorrect :=
      authorsProducedCorrect author authorInRange authorSelected
    simp only [VoterSet.inter, Bool.and_eq_true] at authorProducedCorrect
    have authorCorrectAvailable := authorProducedCorrect.1
    rcases (usable observer observerInRange observerCorrectAvailable).2 author
        authorInRange authorSelected with
      ⟨reference, _block, recorded, _accepted, _retained, _catalogued,
        _blockReference, _authorExact, _roundExact⟩
    simp [VoterSet.inter, ValidatorWorldState.acceptedAuthors,
      authorCorrectAvailable, recorded]
  exact Nat.le_trans authorsAreQuorum
    (weight_mono config.stake correctSubset)

/-- One correct, available holder has an exact usable total-stake quorum.

The selected blocks can include Byzantine authors. The exact list counts at
most one branch per author and keeps every body accepted, retained, catalogued,
and above the holder's GC boundary. This current-state witness does not assume
that all correct receivers already have the blocks. A construction theorem can
retain stronger proposal, source-pin, broadcast, and receiver-acceptance facts
until it projects to this public stage-one result. Parent-first acceptance also
gives the holder the required accepted causal history. The construction derives
that closure; this public record does not add a separate capsule premise. The
`parents` field is the immediate-parent quorum projection. An internal sync
source must separately retain any older explicit own-author dependency used by
a round-jump block. -/
structure CorrectHeldTotalQuorumLayer
    {BlockId CommitId PacketId : Type}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat) where
  holder : Nat
  holderInRange : holder < config.authorityCount
  holderCorrectAvailable : faults.correctAvailable holder = true
  blocks : List (ValidatorBlock BlockId)
  blockAuthorsNodup :
    (blocks.map (fun block => block.reference.author)).Nodup
  blockAuthorsInRange : ∀ block,
    block ∈ blocks → block.reference.author < config.authorityCount
  blocksAtRound : ∀ block, block ∈ blocks →
    block.reference.round = round
  blockStakeIsQuorum : config.thresholds.quorum ≤
    weight config.authorityCount config.stake
      (validatorParentAuthors (blocks.map (fun block => block.reference)))
  roundPositive : 0 < round
  roundAboveHolderGc : (world.validatorState holder).gcRound < round
  blocksAccepted : ∀ block, block ∈ blocks →
    (world.validatorState holder).accepted block.reference = true
  blocksRetained : ∀ block, block ∈ blocks →
    (world.validatorState holder).retained block.reference = true
  blocksCatalogued : ∀ block, block ∈ blocks →
    world.blockCatalog block.reference.id = some block
  blocksValid : ∀ block, block ∈ blocks →
    block.HasQuorumImmediateParents config

/-- One correct, available validator installs a commit with an index above its
index at the observed start state. -/
def SomeCorrectAvailableCommitAdvanceFrom
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (start : Time) : Prop :=
  ∃ validator finish,
    validator < config.authorityCount ∧
      faults.correctAvailable validator = true ∧
      start ≤ finish ∧
      ((trace start).validatorState validator).commitHead.index <
        ((trace finish).validatorState validator).commitHead.index

/-- Unbounded network DAG progress.

For every requested round, one later state has an exact total-stake quorum at
that round or a later round. One correct, available validator holds every
selected valid block above its local GC boundary. The quorum can contain
Byzantine authors, but the result does not depend on a future Byzantine send. -/
def NetworkDagProgressLiveness
    {BlockId CommitId PacketId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) : Prop :=
  ∀ start minimumRound,
    network.gst ≤ start →
    (∀ time, start ≤ time → (trace time).epochActive = true) →
    ∃ finish round,
      start ≤ finish ∧
      minimumRound ≤ round ∧
      Nonempty (CorrectHeldTotalQuorumLayer config faults (trace finish) round)

/-- Internal split for one fixed-head recovery attempt.

For every requested round, either one correct local commit index interrupts
that attempt, or the network produces one exact usable correct quorum layer.
The commit branch is not stage-one DAG progress. The pure network theorem must
continue proposal work through this branch and erase the split. -/
def NetworkDagOrCommitProgressLiveness
    {BlockId CommitId PacketId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) : Prop :=
  ∀ start minimumRound,
    network.gst ≤ start →
    (∀ time, start ≤ time → (trace time).epochActive = true) →
    SomeCorrectAvailableCommitAdvanceFrom faults trace start ∨
      ∃ finish round,
        start ≤ finish ∧
        minimumRound ≤ round ∧
        ProducedCorrectQuorumLayer config faults (trace finish) round ∧
        CommonUsableCorrectQuorumLayer config faults (trace finish) round

/-- Some correct, available validator installs exact commit references at
unbounded indexes. This result does not require all validators to install each
reference at the same time. -/
def NetworkCommitProgressLiveness
    {BlockId CommitId PacketId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) : Prop :=
  ∀ start minimumIndex,
    network.gst ≤ start →
    (∀ time, start ≤ time → (trace time).epochActive = true) →
    ∃ validator finish, ∃ reference : ValidatorCommitHead CommitId,
      validator < config.authorityCount ∧
      faults.correctAvailable validator = true ∧
      start ≤ finish ∧
      minimumIndex ≤ reference.index ∧
      ((trace finish).validatorState validator).installedCommitAt
          reference.index = some reference.id

/-- Each correct, available validator eventually installs one exact commit
reference that a correct, available peer installed after GST. -/
def PointwiseCommitCatchUpLiveness
    {BlockId CommitId PacketId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) : Prop :=
  ∀ source receiver installedAt,
    ∀ reference : ValidatorCommitHead CommitId,
    source < config.authorityCount →
    faults.correctAvailable source = true →
    receiver < config.authorityCount →
    faults.correctAvailable receiver = true →
    network.gst ≤ installedAt →
    (∀ time, installedAt ≤ time → (trace time).epochActive = true) →
    ((trace installedAt).validatorState source).installedCommitAt
        reference.index = some reference.id →
    ∃ finish,
      installedAt ≤ finish ∧
      ((trace finish).validatorState receiver).installedCommitAt
          reference.index = some reference.id

/-- Every correct, available validator has a durable own block in one round. -/
def EveryCorrectAvailableValidatorProduced
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat) : Prop :=
  ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((world.validatorState validator).ownBlockAt round).isSome = true ∧
      (world.validatorState validator).sentOwnBlockAt round = true

/-- Every correct, available validator has accepted every correct, available
validator's block in one round. -/
def EveryCorrectAvailableValidatorAccepted
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (round : Nat) : Prop :=
  ∀ observer author,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ((world.validatorState observer).acceptedRepresentative round author).isSome =
      true

/-- Final per-validator block-production liveness goal.

In any active post-GST interval, each correct, available validator stores and
sends own blocks at unbounded rounds. Normal progress does not require all
validators to produce in the same round. -/
def BlockProductionLiveness
    {BlockId CommitId PacketId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) : Prop :=
  ∀ validator start minimumRound,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    network.gst ≤ start →
    (∀ time, start ≤ time → (trace time).epochActive = true) →
    ∃ finish round,
      start ≤ finish ∧
      minimumRound ≤ round ∧
      ((trace start).validatorState validator).highestSignedRound < round ∧
      (((trace finish).validatorState validator).ownBlockAt round).isSome =
          true ∧
      ((trace finish).validatorState validator).sentOwnBlockAt round = true

/-- All correct, available validators keep the same local commit head during one
stalled interval. This is an internal condition. The final commit-progress
theorem must derive such an interval from local commit and recovery actions. -/
def CommonStalledCommitHead
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId))
    (start : Time)
    (baseline : ValidatorCommitHead CommitId) : Prop :=
  ∀ time validator,
    start ≤ time →
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ((trace time).validatorState validator).commitHead = baseline

/-- Consecutive common layers needed by the commit-progress proof during one
stalled commit. This is an internal result that the final commit theorem must
derive from local recovery actions. -/
def RecoveryLayerWindowLiveness
    {BlockId CommitId PacketId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) : Prop :=
  ∀ start minimumRound count baseline,
    network.gst ≤ start →
    (∀ time, start ≤ time → (trace time).epochActive = true) →
    CommonStalledCommitHead faults trace start baseline →
    ∃ finish baseRound,
      start ≤ finish ∧
      minimumRound ≤ baseRound ∧
      ∀ offset,
        offset < count →
        EveryCorrectAvailableValidatorProduced faults (trace finish)
            (baseRound + offset) ∧
          EveryCorrectAvailableValidatorAccepted faults (trace finish)
            (baseRound + offset) ∧
          ProducedCorrectQuorumLayer config faults (trace finish)
            (baseRound + offset) ∧
          CommonAcceptedCorrectQuorumLayer config faults (trace finish)
            (baseRound + offset)

/-- Final all-validator commit-progress liveness goal.

In any active post-GST interval, there is one later time at which every correct,
available validator has stored the same commit reference at an index greater than
every such validator's index at the start. The validator can obtain the commit
from locally received blocks or from commit synchronization. -/
def CommitProgressLiveness
    {BlockId CommitId PacketId : Type}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (network : AddressedPartialSynchrony config faults protocolPacket)
    (trace : Trace (ValidatorWorldState BlockId CommitId PacketId)) : Prop :=
  ∀ start,
    network.gst ≤ start →
    (∀ time, start ≤ time → (trace time).epochActive = true) →
    ∃ finish index commitId,
      start ≤ finish ∧
      ∀ validator,
        validator < config.authorityCount →
        faults.correctAvailable validator = true →
        (trace start).localCommitIndex validator < index ∧
          ((trace finish).validatorState validator).installedCommitAt index =
              some commitId ∧
          (((trace finish).validatorState validator).commitInstallSourceAt index
              = some .localExecution ∨
            ((trace finish).validatorState validator).commitInstallSourceAt index
              = some .verifiedCommitSync) ∧
          index ≤ (trace finish).localCommitIndex validator

end Mysticeti
